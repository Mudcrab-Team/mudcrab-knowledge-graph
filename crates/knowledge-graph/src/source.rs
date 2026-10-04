//! Pin Git bytes; index Rust/C++ structure, docs, XML and Bevy shader declarations.
use crate::{
    error::{Error, Result},
    extract::ExtractionBuilder,
    ids,
    model::*,
    store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command};
#[derive(Clone, Serialize, Deserialize)]
struct Decl {
    kind: String,
    name: String,
    qualified: String,
    start: usize,
    end: usize,
    parent: Option<usize>,
    attributes: Value,
}
/// Explicit opt-in local source configuration; no acquisition or build execution.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub schema_version: u32,
    pub network_access: bool,
    pub execute_build_scripts: bool,
    pub execute_proc_macros: bool,
    pub sources: Vec<RegisteredSource>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredSource {
    pub id: String,
    pub kind: String,
    pub origin: String,
    pub root_alias: String,
    pub revision: String,
    pub visibility: Visibility,
    pub mode: String,
    pub lineage_roots: Vec<String>,
    pub redistribution: String,
    #[serde(default)]
    pub license_expression: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(Error::new(
            "MISSING_INPUT",
            "Git input/revision unavailable",
        ));
    }
    Ok(output.stdout)
}
/// Extract from an exact Git commit, including source that differs from the worktree.
pub struct SourceRequest<'a> {
    pub repo: &'a Path,
    pub source: &'a Source,
    pub revision: &'a str,
    pub selected: &'a [String],
    pub scip: Option<&'a Path>,
    pub encoding_override: Option<&'a str>,
    pub configuration: Value,
}
/// Extract from pinned source input and explicit index/build configuration.
pub fn index(store: &Store, request: SourceRequest<'_>) -> Result<ExtractionBuilder> {
    let SourceRequest {
        repo,
        source,
        revision,
        selected,
        scip,
        encoding_override,
        configuration: config,
    } = request;
    let commit = String::from_utf8(git(
        repo,
        &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
    )?)
    .map_err(|_| Error::new("EXTRACTION_FAILED", "non-UTF8 Git revision"))?
    .trim()
    .to_owned();
    if !revision.bytes().all(|c| c.is_ascii_hexdigit())
        || revision.len() != 40
        || commit != revision
    {
        return Err(Error::new(
            "INVALID_INPUT",
            "source revision must be the exact full Git commit",
        ));
    }
    for p in selected {
        ids::portable_path(p)?;
    }
    let tree = git(repo, &["ls-tree", "-rz", &commit])?;
    let mut files = BTreeMap::new();
    let mut skipped = 0u64;
    let mut total = 0usize;
    for entry in tree.split(|c| *c == 0).filter(|e| !e.is_empty()) {
        let entry = std::str::from_utf8(entry)
            .map_err(|_| Error::new("INVALID_INPUT", "non-UTF8 repository path"))?;
        let (meta, path) = entry
            .split_once('\t')
            .ok_or_else(|| Error::new("INTEGRITY", "invalid Git tree"))?;
        ids::portable_path(path)?;
        if !selected.is_empty()
            && !selected
                .iter()
                .any(|p| path == p || path.starts_with(&format!("{p}/")))
        {
            continue;
        }
        let ext = path.rsplit('.').next().unwrap_or("");
        if !matches!(
            ext,
            "rs" | "cpp" | "c" | "h" | "hpp" | "cc" | "md" | "wgsl" | "xml" | "toml"
        ) && path != "Cargo.lock"
        {
            continue;
        }
        if !meta.starts_with("100") {
            skipped += 1;
            continue;
        }
        let blob = meta
            .split_whitespace()
            .nth(2)
            .ok_or_else(|| Error::new("INTEGRITY", "invalid Git blob"))?;
        let bytes = git(repo, &["cat-file", "blob", blob])?;
        total += bytes.len();
        if bytes.len() > 8 * 1024 * 1024 || total > 512 * 1024 * 1024 {
            return Err(Error::new(
                "INVALID_INPUT",
                "selected source exceeds file/total extraction limits",
            ));
        }
        files.insert(path.to_owned(), bytes);
    }
    if files.is_empty() {
        return Err(Error::new(
            "EXTRACTION_FAILED",
            "no supported files in selected source scope",
        ));
    }
    let input = json!(
        files
            .iter()
            .map(|(p, b)| json!([p, ids::sha256(b)]))
            .collect::<Vec<_>>()
    );
    let mut conf = json!({"operator_configuration":config,"adapter_revision":"source-index-v4","parsers":{"rust":"tree-sitter-rust-0.24.2","cpp":"tree-sitter-cpp-0.23.4","xml":"roxmltree-0.20.0"},"seed_sha256":ids::sha256(include_bytes!("../../../research/knowledge-graph/seeds/materials.json")),"mode":if scip.is_some(){"scip"}else{"source_structure"}});
    let index_bytes = if let Some(path) = scip {
        let raw = std::fs::read(path)?;
        if raw.len() > crate::bundle::IMPORT_LIMIT as usize {
            return Err(Error::new("INVALID_INPUT", "SCIP exceeds byte limit"));
        }
        conf["scip_sha256"] = json!(ids::sha256(&raw));
        conf["encoding_override"] = json!(encoding_override);
        Some(raw)
    } else {
        None
    };
    let scope = json!({"mode":if scip.is_some(){"scip_and_source_structure"}else{"source_structure"},"paths":files.keys().collect::<Vec<_>>(),"revision":commit});
    let mut builder = ExtractionBuilder::new(
        source.clone(),
        revision,
        &ids::digest(&input)?,
        "source-index",
        conf,
        scope,
        vec![],
    )?;
    builder.bundle.snapshot.capabilities.clear();
    let mut file_nodes = BTreeMap::new();
    let mut artifacts = BTreeMap::new();
    let mut failed = 0u64;
    for (path, raw) in &files {
        let text = match std::str::from_utf8(raw) {
            Ok(t) => t,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let art = builder.artifact(path, "text/plain", raw.clone())?;
        artifacts.insert(path.clone(), art.clone());
        let ev = span(&mut builder, &art, path, text, 0, text.len(), None)?;
        let file = builder.node(
            if path.ends_with(".wgsl") {
                NodeKind::ShaderModule
            } else {
                NodeKind::SourceFile
            },
            path,
            path,
            json!({"file_sha256":ids::sha256(raw),"resolution":"syntactic"}),
            std::slice::from_ref(&ev),
        );
        file_nodes.insert(path.clone(), file.clone());
        let ext = path.rsplit('.').next().unwrap_or("");
        let cache_key = ids::digest(
            &json!({"content":ids::sha256(raw),"language":ext,"parser":"source-index-v3"}),
        )?;
        use rusqlite::OptionalExtension;
        let prior: Option<String> = store
            .db
            .query_row(
                "SELECT declarations_json FROM source_cache WHERE fingerprint=?1",
                [&cache_key],
                |r| r.get(0),
            )
            .optional()?;
        let declarations = if let Some(data) = prior {
            serde_json::from_str(&data)?
        } else {
            let parsed = declarations(text, ext)?;
            store.db.execute(
                "INSERT OR IGNORE INTO source_cache VALUES(?1,?2)",
                rusqlite::params![cache_key, serde_json::to_string(&parsed)?],
            )?;
            parsed
        };
        let mut declared = BTreeMap::new();
        for (idx, d) in declarations.iter().enumerate() {
            let evidence = span(&mut builder, &art, path, text, d.start, d.end, None)?;
            let kind = match d.kind.as_str() {
                "field_declaration" | "field_identifier" => NodeKind::TypeField,
                "markdown" => NodeKind::DocumentSection,
                "resource" => NodeKind::ShaderBinding,
                _ => NodeKind::CodeSymbol,
            };
            let key = format!("{path}::{}::{}::{}-{}", d.kind, d.qualified, d.start, d.end);
            let mut attrs = d.attributes.clone();
            attrs["declaration_kind"] = json!(d.kind);
            attrs["qualified_name"] = json!(d.qualified);
            attrs["resolution"] = json!("syntactic");
            attrs["path"] = json!(path);
            let id = builder.node(kind, &key, &d.name, attrs, std::slice::from_ref(&evidence));
            let parent = d
                .parent
                .and_then(|p| declared.get(&p))
                .cloned()
                .unwrap_or_else(|| file.clone());
            let predicate = if kind == NodeKind::TypeField && parent != file {
                Predicate::HasField
            } else if parent == file && kind != NodeKind::DocumentSection {
                Predicate::Declares
            } else {
                Predicate::Contains
            };
            builder.edge(
                &parent,
                predicate,
                &id,
                json!({"resolution":"syntactic"}),
                std::slice::from_ref(&evidence),
                Provenance::Extracted,
            )?;
            declared.insert(idx, id);
        }
        if ext == "wgsl" {
            let mut offset = 0;
            for line in text.split_inclusive('\n') {
                if line.trim_start().starts_with("#import ") {
                    let ev = span(
                        &mut builder,
                        &art,
                        path,
                        text,
                        offset,
                        offset + line.len(),
                        None,
                    )?;
                    let external = builder.node(
                        NodeKind::ExternalSymbol,
                        &format!("{path}::wgsl_import::{offset}"),
                        line.trim(),
                        json!({"resolution":"unresolved_shader_import","preprocessed":false}),
                        std::slice::from_ref(&ev),
                    );
                    builder.edge(
                        &file,
                        Predicate::Imports,
                        &external,
                        json!({"source_import":true}),
                        &[ev],
                        Provenance::Extracted,
                    )?;
                }
                offset += line.len();
            }
        }
        if ext == "rs" || matches!(ext, "h" | "hpp" | "c" | "cpp" | "cc") {
            imports(&mut builder, &file, &art, path, text, ext)?;
        }
    }
    builder.capability("source_structure",if failed==0{"complete"}else{"partial"},"literal declarations and lexical containment; ABI/calls unresolved; structural declarations cached by content and grammar",files.len()as u64,failed,skipped);
    builder.capability(
        "semantic_references",
        if scip.is_some() { "partial" } else { "not_run" },
        "pre-generated index required; no indexed repository build scripts or proc macros executed",
        0,
        0,
        0,
    );
    if let Some(raw) = index_bytes {
        import_scip(
            &mut builder,
            &raw,
            &files,
            &file_nodes,
            &artifacts,
            encoding_override,
        )?;
    }
    if source.id == "mudcrab" {
        material_seeds(&mut builder)?;
    }
    Ok(builder)
}
fn material_seeds(b: &mut ExtractionBuilder) -> Result<()> {
    let data = include_str!("../../../research/knowledge-graph/seeds/materials.json");
    let seeds: Value = serde_json::from_str(data)?;
    let art = b.artifact(
        "_research/material-seeds.json",
        "application/json",
        data.as_bytes().to_vec(),
    )?;
    let mut concepts = BTreeMap::new();
    let mut resolved = 0;
    let mut failed = 0;
    for (i, selector) in seeds["mappings"]
        .as_array()
        .expect("built-in seeds")
        .iter()
        .enumerate()
    {
        let key = selector["concept"].as_str().expect("seed key");
        let ev = b.evidence(
            &art,
            json!({"kind":"json_pointer","pointer":format!("/mappings/{i}")}),
            "authored research concept selector",
            Provenance::Manual,
        )?;
        let concept=concepts.entry(key.to_owned()).or_insert_with(||b.node(NodeKind::Concept,key,key,json!({"meaning":"research selector; no original-engine or runtime support claim"}),std::slice::from_ref(&ev))).clone();
        let candidates = b
            .bundle
            .nodes
            .iter()
            .filter(|n| {
                n.kind == NodeKind::CodeSymbol
                    && n.attributes["path"] == selector["path"]
                    && n.display_name == selector["symbol"]
            })
            .cloned()
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            failed += 1;
            continue;
        }
        let target = &candidates[0];
        let mut evidence = vec![ev];
        evidence.extend(target.evidence_ids.clone());
        b.edge(&target.id,Predicate::HandlesConcept,&concept,json!({"selector":selector,"source_revision":b.bundle.snapshot.revision,"claim":"authored Mudcrab source mapping; review required"}),&evidence,Provenance::Manual)?;
        resolved += 1;
    }
    b.capability("material_source_seeds",if failed==0{"complete"}else{"partial"},"zero/multiple selector matches are unresolved; manual source mappings remain proposed until explicit review",resolved+failed,failed,0);
    Ok(())
}
fn span(
    b: &mut ExtractionBuilder,
    art: &str,
    path: &str,
    text: &str,
    start: usize,
    end: usize,
    original: Option<Value>,
) -> Result<String> {
    let mut l = json!({"kind":"source_span","path":path,"start_byte":start,"end_byte":end,"start_line":1+text[..start].bytes().filter(|c|*c==b'\n').count(),"end_line":1+text.as_bytes()[..if end > start {end-1} else {end}].iter().filter(|c|**c==b'\n').count(),"encoding":"utf8_bytes"});
    if let Some(o) = original {
        l["original_range"] = o;
    }
    b.evidence(art, l, "source_structure", Provenance::Extracted)
}
fn declarations(text: &str, ext: &str) -> Result<Vec<Decl>> {
    if ext == "md" {
        return Ok(markdown(text));
    }
    if ext == "wgsl" {
        return Ok(wgsl(text));
    }
    if ext == "xml" {
        return xml(text);
    }
    if !matches!(ext, "rs" | "h" | "hpp" | "cpp" | "cc" | "c") {
        return Ok(vec![]);
    }
    let rust = ext == "rs";
    let mut parser = tree_sitter::Parser::new();
    let lang = if rust {
        tree_sitter_rust::LANGUAGE.into()
    } else {
        tree_sitter_cpp::LANGUAGE.into()
    };
    parser
        .set_language(&lang)
        .map_err(|_| Error::new("EXTRACTION_FAILED", "source grammar initialization failed"))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| Error::new("EXTRACTION_FAILED", "source parsing failed"))?;
    let mut out = vec![];
    walk(tree.root_node(), text, "", None, &mut out);
    Ok(out)
}
fn walk(
    n: tree_sitter::Node<'_>,
    text: &str,
    scope: &str,
    parent: Option<usize>,
    out: &mut Vec<Decl>,
) {
    let kind = n.kind();
    let is_decl = matches!(
        kind,
        "function_item"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "type_item"
            | "const_item"
            | "static_item"
            | "mod_item"
            | "impl_item"
            | "field_declaration"
            | "enum_variant"
            | "macro_definition"
            | "function_definition"
            | "struct_specifier"
            | "class_specifier"
            | "enum_specifier"
            | "namespace_definition"
            | "type_definition"
    );
    let mut current = parent;
    let mut qualified = scope.to_owned();
    if is_decl {
        let name_node = n.child_by_field_name("name").or_else(|| {
            if kind == "impl_item" {
                n.child_by_field_name("type")
            } else {
                n.child_by_field_name("declarator")
            }
        });
        let name = name_node
            .map(|x| text[x.byte_range()].to_owned())
            .unwrap_or_else(|| format!("{kind}@{}", n.start_byte()));
        qualified = if scope.is_empty() {
            name.clone()
        } else {
            format!("{scope}::{name}")
        };
        let mut attrs = json!({"language":if kind.ends_with("_item"){"rust"}else{"source"},"syntax_has_errors":n.has_error(),"unexpanded_conditions":conditions(n,text),"abi_layout":"unresolved"});
        if matches!(kind, "function_item" | "function_definition") {
            let mut calls = vec![];
            collect_calls(n, text, &mut calls);
            attrs["unresolved_calls"] = json!(calls);
        }
        current = Some(out.len());
        out.push(Decl {
            kind: kind.into(),
            name,
            qualified: qualified.clone(),
            start: n.start_byte(),
            end: n.end_byte(),
            parent,
            attributes: attrs,
        });
    }
    let mut cursor = n.walk();
    for child in n.named_children(&mut cursor) {
        walk(child, text, &qualified, current, out);
    }
}
fn conditions(mut n: tree_sitter::Node<'_>, text: &str) -> Vec<String> {
    let mut out = vec![];
    while let Some(parent) = n.parent() {
        if parent.kind().starts_with("preproc_") {
            let condition = parent
                .child_by_field_name("condition")
                .map(|x| text[x.byte_range()].to_owned())
                .unwrap_or_else(|| parent.kind().into());
            out.push(condition);
        }
        n = parent;
    }
    out
}
fn collect_calls(n: tree_sitter::Node<'_>, text: &str, out: &mut Vec<Value>) {
    if n.kind() == "call_expression" {
        let target = n
            .child_by_field_name("function")
            .map(|x| text[x.byte_range()].to_owned())
            .unwrap_or_default();
        out.push(json!({"target_text":target,"start_byte":n.start_byte(),"end_byte":n.end_byte(),"status":"unresolved"}));
    }
    let mut cursor = n.walk();
    for c in n.named_children(&mut cursor) {
        collect_calls(c, text, out);
    }
}
fn imports(
    b: &mut ExtractionBuilder,
    file: &str,
    art: &str,
    path: &str,
    text: &str,
    ext: &str,
) -> Result<()> {
    let mut parser = tree_sitter::Parser::new();
    let lang = if ext == "rs" {
        tree_sitter_rust::LANGUAGE.into()
    } else {
        tree_sitter_cpp::LANGUAGE.into()
    };
    parser
        .set_language(&lang)
        .map_err(|_| Error::new("EXTRACTION_FAILED", "grammar unavailable"))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| Error::new("EXTRACTION_FAILED", "parse unavailable"))?;
    fn visit(n: tree_sitter::Node<'_>, ranges: &mut Vec<(usize, usize)>) {
        if matches!(
            n.kind(),
            "use_declaration" | "extern_crate_declaration" | "preproc_include" | "macro_invocation"
        ) {
            ranges.push((n.start_byte(), n.end_byte()));
        }
        let mut c = n.walk();
        for x in n.named_children(&mut c) {
            visit(x, ranges);
        }
    }
    let mut ranges = vec![];
    visit(tree.root_node(), &mut ranges);
    for (s, e) in ranges {
        let ev = span(b, art, path, text, s, e, None)?;
        let target = &text[s..e];
        let id=b.node(NodeKind::ExternalSymbol,&format!("{path}::unresolved_import::{s}"),target,json!({"status":"unresolved","resolution":"syntactic","dependency_origin":"not_resolved","macro_expansion":"not_run"}),std::slice::from_ref(&ev));
        b.edge(
            file,
            Predicate::Imports,
            &id,
            json!({"resolution":"syntactic"}),
            std::slice::from_ref(&ev),
            Provenance::Extracted,
        )?;
    }
    Ok(())
}
fn markdown(text: &str) -> Vec<Decl> {
    let mut out = vec![];
    let mut pos = 0;
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        if !fence {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let count = trimmed.chars().take_while(|c| *c == '#').count();
                if (1..=6).contains(&count) && trimmed.as_bytes().get(count) == Some(&b' ') {
                    let name = trimmed[count..].trim().to_owned();
                    out.push(Decl {
                        kind: "markdown".into(),
                        name: name.clone(),
                        qualified: name,
                        start: pos,
                        end: pos + line.len(),
                        parent: None,
                        attributes: json!({"heading_level":count}),
                    });
                }
            }
        }
        pos += line.len();
    }
    for i in 0..out.len() {
        out[i].end = out.get(i + 1).map_or(text.len(), |d| d.start);
    }
    out
}
fn xml(text: &str) -> Result<Vec<Decl>> {
    let doc = roxmltree::Document::parse(text)
        .map_err(|_| Error::new("EXTRACTION_FAILED", "invalid XML source"))?;
    let mut out = vec![];
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(name) = n.attribute("name") {
            let r = n.range();
            let attrs = n
                .attributes()
                .map(|a| (a.name().to_owned(), json!(a.value())))
                .collect::<serde_json::Map<_, _>>();
            out.push(Decl{kind:n.tag_name().name().into(),name:name.into(),qualified:name.into(),start:r.start,end:r.end,parent:None,attributes:json!({"format_definition":attrs,"version_conditions":"preserved_source"})});
        }
    }
    Ok(out)
}
// Token scanning skips comments/strings; bindings remain symbolic until preprocessing.
fn wgsl(text: &str) -> Vec<Decl> {
    let ts = tokens(text);
    let mut out = vec![];
    let mut pending = vec![];
    let mut i = 0;
    while i < ts.len() {
        let (start, _, token) = &ts[i];
        if token == "@" {
            let mut j = i + 1;
            let mut depth = 0;
            while j < ts.len() {
                if ts[j].2 == "(" {
                    depth += 1;
                }
                if ts[j].2 == ")" {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                j += 1;
            }
            if j < ts.len() {
                pending.push(text[*start..ts[j].1].to_owned());
                i = j + 1;
                continue;
            }
        }
        if !matches!(token.as_str(), "fn" | "struct" | "var") {
            i += 1;
            continue;
        }
        let mut name_pos = i + 1;
        if ts.get(name_pos).is_some_and(|x| x.2 == "<") {
            let mut depth = 0;
            while name_pos < ts.len() {
                if ts[name_pos].2 == "<" {
                    depth += 1;
                }
                if ts[name_pos].2 == ">" {
                    depth -= 1;
                    if depth == 0 {
                        name_pos += 1;
                        break;
                    }
                }
                name_pos += 1;
            }
        }
        let Some((_, _, name)) = ts.get(name_pos) else {
            break;
        };
        let mut j = name_pos + 1;
        let mut body = None;
        let mut depth = 0;
        while j < ts.len() {
            if ts[j].2 == "{" {
                if depth == 0 {
                    body = Some(j);
                }
                depth += 1;
            } else if ts[j].2 == "}" {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if ts[j].2 == ";" && depth == 0 {
                break;
            }
            j += 1;
        }
        let end = ts.get(j).map_or(text.len(), |x| x.1);
        let parent = out.len();
        out.push(Decl{kind:if token=="var"{"resource"}else{token}.into(),name:name.clone(),qualified:name.clone(),start:*start,end,parent:None,attributes:json!({"language":"bevy_wgsl","annotations":pending,"bindings":"source_symbolic","preprocessed":false})});
        pending = vec![];
        if token == "struct"
            && let Some(body) = body
        {
            let mut k = body + 1;
            while k + 1 < j {
                if ts[k + 1].2 == ":" {
                    let field = ts[k].2.clone();
                    let mut field_end = k + 2;
                    let mut generic = 0;
                    while field_end < j {
                        if ts[field_end].2 == "<" {
                            generic += 1;
                        }
                        if ts[field_end].2 == ">" {
                            generic -= 1;
                        }
                        if ts[field_end].2 == "," && generic == 0 {
                            break;
                        }
                        field_end += 1;
                    }
                    out.push(Decl{kind:"field_declaration".into(),name:field.clone(),qualified:format!("{name}::{field}"),start:ts[k].0,end:ts.get(field_end).map_or(end,|x|x.1),parent:Some(parent),attributes:json!({"language":"bevy_wgsl","type_source":text[ts[k+1].1..ts.get(field_end).map_or(end,|x|x.0)],"layout":"unresolved"})});
                    k = field_end;
                }
                k += 1;
            }
        }
        i = j + 1;
    }
    out
}

fn tokens(text: &str) -> Vec<(usize, usize, String)> {
    let b = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b.get(i..i + 2) == Some(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b.get(i..i + 2) == Some(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b.get(i..i + 2) == Some(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b.get(i..i + 2) == Some(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        let start = i;
        if b[i] == b'"' {
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        if b[i].is_ascii_alphanumeric() || b[i] == b'_' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
        } else {
            let ch = text[i..].chars().next().expect("char");
            i += ch.len_utf8();
        }
        out.push((start, i, text[start..i].into()));
    }
    out
}
fn import_scip(
    b: &mut ExtractionBuilder,
    raw: &[u8],
    files: &BTreeMap<String, Vec<u8>>,
    file_nodes: &BTreeMap<String, String>,
    artifacts: &BTreeMap<String, String>,
    override_encoding: Option<&str>,
) -> Result<()> {
    use protobuf::Message;
    let index = scip::types::Index::parse_from_bytes(raw)
        .map_err(|_| Error::new("INVALID_INPUT", "invalid SCIP protobuf"))?;
    let key = |path: &str, symbol: &str| {
        if symbol.starts_with("local ") {
            format!("{path}::{symbol}")
        } else {
            symbol.into()
        }
    };
    let mut defined = std::collections::BTreeSet::new();
    let mut information = BTreeMap::new();
    for doc in &index.documents {
        ids::portable_path(&doc.relative_path)?;
        for occurrence in &doc.occurrences {
            if occurrence.symbol_roles & 1 != 0 {
                defined.insert(key(&doc.relative_path, &occurrence.symbol));
            }
        }
        for s in &doc.symbols {
            information.insert(key(&doc.relative_path,&s.symbol),json!({"symbol":s.symbol,"display_name":s.display_name,"documentation":s.documentation,"kind":s.kind.value(),"enclosing_symbol":s.enclosing_symbol,"relationships":s.relationships.iter().map(|r|json!({"symbol":r.symbol,"is_reference":r.is_reference,"is_implementation":r.is_implementation,"is_type_definition":r.is_type_definition,"is_definition":r.is_definition})).collect::<Vec<_>>() }));
        }
    }
    for s in &index.external_symbols {
        information.insert(s.symbol.clone(),json!({"symbol":s.symbol,"display_name":s.display_name,"documentation":s.documentation,"external":true,"relationships":s.relationships.iter().map(|r|json!({"symbol":r.symbol,"is_reference":r.is_reference,"is_implementation":r.is_implementation,"is_type_definition":r.is_type_definition,"is_definition":r.is_definition})).collect::<Vec<_>>() }));
    }
    let metadata = json!({"input_sha256":ids::sha256(raw),"producer":index.metadata.as_ref().map(|m|json!({"version":m.version.value(),"text_document_encoding":m.text_document_encoding.value()})),"symbols":information});
    let meta_art = b.artifact(
        "_indexes/scip.metadata.json",
        "application/json",
        serde_json::to_vec(&metadata)?,
    )?;
    let mut occurrences = vec![];
    let mut node_evidence: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut skipped = 0;
    for doc in &index.documents {
        let Some(bytes) = files.get(&doc.relative_path) else {
            skipped += 1;
            continue;
        };
        let text = std::str::from_utf8(bytes)
            .map_err(|_| Error::new("INTEGRITY", "SCIP document bytes invalid UTF-8"))?;
        if !doc.text.is_empty() && doc.text != text {
            return Err(Error::new(
                "INTEGRITY",
                "SCIP text differs from pinned revision",
            ));
        }
        let encoding = match doc.position_encoding.value() {
            1 => "utf8",
            2 => "utf16",
            3 => "utf32",
            0 => override_encoding.ok_or_else(|| {
                Error::new(
                    "INVALID_INPUT",
                    "unspecified SCIP position encoding needs an explicit override",
                )
            })?,
            _ => {
                return Err(Error::new(
                    "INVALID_INPUT",
                    "unknown SCIP position encoding",
                ));
            }
        };
        for occurrence in &doc.occurrences {
            if occurrence.symbol.is_empty() {
                continue;
            }
            if occurrence.range.iter().any(|x| *x < 0) {
                return Err(Error::new("INTEGRITY", "negative SCIP coordinate"));
            }
            let (sl, sc, el, ec) = match occurrence.range.as_slice() {
                [l, s, e] => (*l, *s, *l, *e),
                [l, s, el, e] => (*l, *s, *el, *e),
                _ => return Err(Error::new("INTEGRITY", "invalid SCIP range")),
            };
            let start = position_to_byte(text, sl as usize, sc as usize, encoding)?;
            let end = position_to_byte(text, el as usize, ec as usize, encoding)?;
            if start > end {
                return Err(Error::new("INTEGRITY", "reversed SCIP range"));
            }
            let ev = span(
                b,
                &artifacts[&doc.relative_path],
                &doc.relative_path,
                text,
                start,
                end,
                Some(json!({"encoding":encoding,"range":occurrence.range})),
            )?;
            let logical = key(&doc.relative_path, &occurrence.symbol);
            node_evidence
                .entry(logical.clone())
                .or_default()
                .push(ev.clone());
            occurrences.push((
                doc.relative_path.clone(),
                logical,
                occurrence.symbol_roles,
                ev,
            ));
        }
    }
    for logical in information.keys() {
        if !node_evidence.contains_key(logical) {
            let pointer = format!("/symbols/{}", logical.replace('~', "~0").replace('/', "~1"));
            let ev = b.evidence(
                &meta_art,
                json!({"kind":"json_pointer","pointer":pointer}),
                "SCIP symbol metadata",
                Provenance::Extracted,
            )?;
            node_evidence.insert(logical.clone(), vec![ev]);
        }
    }
    let mut symbols = BTreeMap::new();
    for (logical, mut evs) in node_evidence {
        evs.sort();
        evs.dedup();
        let info = information
            .get(&logical)
            .cloned()
            .unwrap_or_else(|| json!({"symbol":logical,"definition":"unresolved"}));
        let display = info["display_name"]
            .as_str()
            .filter(|x| !x.is_empty())
            .unwrap_or(&logical);
        let id=b.node(if defined.contains(&logical){NodeKind::CodeSymbol}else{NodeKind::ExternalSymbol},&logical,display,json!({"resolution":"SCIP","symbol_information":info,"dependency_origin":"exact SCIP symbol/package string; dependency locks indexed separately"}),&evs);
        symbols.insert(logical, id);
    }
    for (path, logical, roles, ev) in occurrences {
        b.edge(&file_nodes[&path],if roles&1!=0{Predicate::Defines}else{Predicate::References},&symbols[&logical],json!({"SCIP_symbol_roles":roles,"resolution":"semantic_navigation","call_resolution":"not_established"}),&[ev],Provenance::Extracted)?;
    }
    for (doc_path, infos) in index
        .documents
        .iter()
        .map(|d| (&d.relative_path, &d.symbols))
    {
        for info in infos {
            let from = key(doc_path, &info.symbol);
            let Some(subject) = symbols.get(&from) else {
                continue;
            };
            let ev = b
                .bundle
                .nodes
                .iter()
                .find(|n| &n.id == subject)
                .expect("symbol node")
                .evidence_ids
                .clone();
            for r in &info.relationships {
                let target = key(doc_path, &r.symbol);
                if let Some(object) = symbols.get(&target) {
                    let p = if r.is_definition {
                        Predicate::Defines
                    } else if r.is_type_definition {
                        Predicate::HasType
                    } else {
                        Predicate::References
                    };
                    b.edge(subject,p,object,json!({"scip_relationship":{"is_reference":r.is_reference,"is_implementation":r.is_implementation,"is_type_definition":r.is_type_definition,"is_definition":r.is_definition}}),&ev,Provenance::Extracted)?;
                }
            }
        }
    }
    b.capability("SCIP",if skipped==0{"complete"}else{"partial"},"navigation roles preserved; generic references never become calls; index revision is operator-pinned and document text verified when supplied",(index.documents.len()-skipped)as u64,0,skipped as u64);
    Ok(())
}
/// Convert indexer coordinates without splitting Unicode codepoints or surrogate pairs.
pub fn position_to_byte(text: &str, line: usize, column: usize, encoding: &str) -> Result<usize> {
    let mut start = 0usize;
    for _ in 0..line {
        let Some(newline) = text[start..].find('\n') else {
            return Err(Error::new("INTEGRITY", "SCIP line exceeds source"));
        };
        start += newline + 1;
    }
    let line_text = text[start..].split('\n').next().unwrap_or("");
    if encoding == "utf8" {
        if column > line_text.len() || !line_text.is_char_boundary(column) {
            return Err(Error::new("INTEGRITY", "invalid UTF-8 column"));
        }
        return Ok(start + column);
    }
    if !matches!(encoding, "utf16" | "utf32") {
        return Err(Error::new(
            "INVALID_INPUT",
            "encoding override must be utf8/utf16/utf32",
        ));
    }
    let mut units = 0;
    for (offset, c) in line_text.char_indices() {
        if units == column {
            return Ok(start + offset);
        }
        units += if encoding == "utf16" {
            c.len_utf16()
        } else {
            1
        };
        if units > column {
            return Err(Error::new(
                "INTEGRITY",
                "SCIP column splits Unicode character",
            ));
        }
    }
    if units == column {
        Ok(start + line_text.len())
    } else {
        Err(Error::new("INTEGRITY", "SCIP column exceeds source"))
    }
}
