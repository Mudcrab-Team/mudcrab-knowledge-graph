//! Bounded portable bundle IO and reference-closure validation.
use crate::{
    error::{Error, Result},
    ids,
    model::*,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader},
    path::Path,
    sync::OnceLock,
};
pub const RECORD_LIMIT: usize = 8 * 1024 * 1024;
pub const IMPORT_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
pub const ENTITY_LIMIT: usize = 5_000_000;
const SCHEMA: &str = include_str!("../../../research/knowledge-graph/contracts/bundle.schema.json");
const STREAM_SCHEMA: &str =
    include_str!("../../../research/knowledge-graph/contracts/stream.schema.json");
const ONTOLOGY: &str = include_str!("../../../research/knowledge-graph/contracts/ontology.json");
/// Dependency records belong only to the explicit transitive snapshot closure.
#[derive(Default)]
pub struct Closure {
    pub nodes: BTreeMap<String, Node>,
    pub evidence: BTreeMap<String, Evidence>,
    pub artifacts: BTreeMap<String, Artifact>,
}
/// Loaded normalized records plus producer diagnostics (stream format).
pub struct Loaded {
    pub bundle: Bundle,
    pub diagnostics: Vec<Value>,
}
fn schema_check(v: &Value, stream: bool) -> Result<()> {
    static FULL: OnceLock<jsonschema::Validator> = OnceLock::new();
    static STREAM: OnceLock<jsonschema::Validator> = OnceLock::new();
    let validator = if stream {
        STREAM.get_or_init(|| {
            jsonschema::validator_for(
                &serde_json::from_str(STREAM_SCHEMA).expect("built-in schema"),
            )
            .expect("built-in schema")
        })
    } else {
        FULL.get_or_init(|| {
            jsonschema::validator_for(&serde_json::from_str(SCHEMA).expect("built-in schema"))
                .expect("built-in schema")
        })
    };
    if !validator.is_valid(v) {
        return Err(Error::new(
            "INVALID_SCHEMA",
            "bundle does not match the v1 contract",
        ));
    }
    Ok(())
}
/// Read JSON or five declared NDJSON streams with exact hashes and counts.
pub fn load(path: &Path) -> Result<Loaded> {
    if !path.is_dir() {
        let bytes = fs::read(path)?;
        if bytes.len() > IMPORT_LIMIT as usize {
            return Err(Error::new("INVALID_INPUT", "bundle exceeds byte limit"));
        }
        let v: Value = serde_json::from_slice(&bytes)?;
        schema_check(&v, false)?;
        ids::canonical(&v)?;
        let b: Bundle = serde_json::from_value(v)?;
        record_limits(&b)?;
        return Ok(Loaded {
            bundle: b,
            diagnostics: vec![],
        });
    }
    let manifest = fs::read(ids::resolve_file(path, "manifest.json")?)?;
    if manifest.len() > RECORD_LIMIT {
        return Err(Error::new("INVALID_INPUT", "manifest exceeds record limit"));
    }
    let m: Value = serde_json::from_slice(&manifest)?;
    schema_check(&m, true)?;
    ids::canonical(&m)?;
    let mut full = json!({"format":"mudcrab-kg-bundle","schema_version":1,"source":m["source"],"snapshot":m["snapshot"]});
    let mut kinds = BTreeSet::new();
    let mut total = 0u64;
    let mut entities = 0usize;
    let mut diagnostics = vec![];
    for f in m["files"].as_array().expect("validated schema") {
        let kind = f["kind"].as_str().expect("validated schema");
        let name = f["path"].as_str().expect("validated schema");
        if !kinds.insert(kind) || name != format!("{kind}.jsonl") {
            return Err(Error::new(
                "INVALID_INPUT",
                "duplicate or nonstandard stream file",
            ));
        }
        let p = ids::resolve_file(path, name)?;
        let size = fs::metadata(&p)?.len();
        total = total
            .checked_add(size)
            .ok_or_else(|| Error::new("INVALID_INPUT", "byte count overflow"))?;
        if total > IMPORT_LIMIT || size != f["byte_size"].as_u64().unwrap_or(u64::MAX) {
            return Err(Error::new("INTEGRITY", "stream size mismatch or limit"));
        }
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        let mut records = vec![];
        let mut reader = BufReader::new(fs::File::open(p)?);
        loop {
            let mut line = vec![];
            let n = std::io::Read::take(&mut reader, (RECORD_LIMIT + 1) as u64)
                .read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            if n > RECORD_LIMIT || !line.ends_with(b"\n") {
                return Err(Error::new(
                    "INVALID_INPUT",
                    "stream record exceeds limit or lacks terminal LF",
                ));
            }
            hash.update(&line);
            let v: Value = serde_json::from_slice(&line)?;
            ids::canonical(&v)?;
            if kind == "diagnostics" {
                validate_diagnostic(&v)?;
            }
            records.push(v);
            entities += 1;
            if entities > ENTITY_LIMIT {
                return Err(Error::new("INVALID_INPUT", "entity limit exceeded"));
            }
        }
        if format!("{:x}", hash.finalize()) != f["sha256"].as_str().unwrap_or("")
            || records.len() as u64 != f["record_count"].as_u64().unwrap_or(u64::MAX)
        {
            return Err(Error::new("INTEGRITY", "stream hash/count mismatch"));
        }
        if kind == "diagnostics" {
            diagnostics = records;
        } else {
            full[kind] = json!(records);
        }
    }
    if kinds.len() != 5 {
        return Err(Error::new("INVALID_INPUT", "missing stream file"));
    }
    schema_check(&full, false)?;
    let bundle = serde_json::from_value(full)?;
    record_limits(&bundle)?;
    Ok(Loaded {
        bundle,
        diagnostics,
    })
}
fn validate_diagnostic(v: &Value) -> Result<()> {
    let Some(o) = v.as_object() else {
        return Err(Error::new("INVALID_INPUT", "invalid producer diagnostic"));
    };
    if o.len() != 5
        || ![
            "code",
            "severity",
            "message",
            "source_selector",
            "capability",
        ]
        .iter()
        .all(|k| o.contains_key(*k))
        || !matches!(v["severity"].as_str(), Some("info" | "warning" | "error"))
    {
        return Err(Error::new("INVALID_INPUT", "invalid producer diagnostic"));
    }
    Ok(())
}
fn record_limits(b: &Bundle) -> Result<()> {
    if b.artifacts.len() + b.evidence.len() + b.nodes.len() + b.assertions.len() > ENTITY_LIMIT {
        return Err(Error::new("INVALID_INPUT", "entity limit exceeded"));
    }
    fn records<T: serde::Serialize>(values: &[T]) -> Result<()> {
        for record in values {
            if ids::canonical(&serde_json::to_value(record)?)?.len() > RECORD_LIMIT {
                return Err(Error::new("INVALID_INPUT", "record exceeds limit"));
            }
        }
        Ok(())
    }
    records(&b.artifacts)?;
    records(&b.evidence)?;
    records(&b.nodes)?;
    records(&b.assertions)
}
/// Validate the unchanged wire schema one record at a time. This avoids holding
/// several complete JSON/typed bundle copies for large normalized imports.
fn validate_shape(b: &Bundle) -> Result<()> {
    static VALIDATORS: OnceLock<BTreeMap<String, jsonschema::Validator>> = OnceLock::new();
    let validators = VALIDATORS.get_or_init(|| {
        let schema: Value = serde_json::from_str(SCHEMA).expect("built-in schema");
        let mut validators = BTreeMap::new();
        let mut header = schema.clone();
        for key in ["artifacts", "evidence", "nodes", "assertions"] {
            header["properties"][key]["minItems"] = json!(0);
            let mut item = schema["properties"][key]["items"].clone();
            item["$defs"] = schema["$defs"].clone();
            item["$schema"] = schema["$schema"].clone();
            validators.insert(
                key.to_owned(),
                jsonschema::validator_for(&item).expect("record schema"),
            );
        }
        validators.insert(
            "header".into(),
            jsonschema::validator_for(&header).expect("header schema"),
        );
        validators
    });
    let header = json!({"format":b.format,"schema_version":b.schema_version,"source":b.source,
        "snapshot":b.snapshot,"artifacts":[],"evidence":[],"nodes":[],"assertions":[]});
    if !validators["header"].is_valid(&header) || b.artifacts.is_empty() || b.evidence.is_empty() {
        return Err(Error::new(
            "INVALID_SCHEMA",
            "bundle does not match the v1 contract",
        ));
    }
    ids::canonical(&header)?;
    fn records<T: serde::Serialize>(
        records: &[T],
        validator: &jsonschema::Validator,
    ) -> Result<()> {
        for record in records {
            let v = serde_json::to_value(record)?;
            if !validator.is_valid(&v) {
                return Err(Error::new(
                    "INVALID_SCHEMA",
                    "record does not match the v1 contract",
                ));
            }
            if ids::canonical(&v)?.len() > RECORD_LIMIT {
                return Err(Error::new("INVALID_INPUT", "record exceeds limit"));
            }
        }
        Ok(())
    }
    records(&b.artifacts, &validators["artifacts"])?;
    records(&b.evidence, &validators["evidence"])?;
    records(&b.nodes, &validators["nodes"])?;
    records(&b.assertions, &validators["assertions"])?;
    require(
        b.artifacts.len() + b.evidence.len() + b.nodes.len() + b.assertions.len() <= ENTITY_LIMIT,
        "entity limit exceeded",
    )
}
/// Hash exactly the canonical bundle with record arrays sorted by immutable ID,
/// without allocating a second bundle or a complete canonical JSON buffer.
pub fn fingerprint(b: &Bundle) -> Result<String> {
    use sha2::{Digest, Sha256};
    fn array<T: serde::Serialize>(h: &mut Sha256, mut records: Vec<(&str, &T)>) -> Result<()> {
        records.sort_by_key(|r| r.0);
        h.update(b"[");
        for (index, (_, record)) in records.into_iter().enumerate() {
            if index != 0 {
                h.update(b",");
            }
            h.update(ids::canonical(&serde_json::to_value(record)?)?);
        }
        h.update(b"]");
        Ok(())
    }
    let mut h = Sha256::new();
    h.update(b"{\"artifacts\":");
    array(
        &mut h,
        b.artifacts.iter().map(|r| (r.id.as_str(), r)).collect(),
    )?;
    h.update(b",\"assertions\":");
    array(
        &mut h,
        b.assertions.iter().map(|r| (r.id.as_str(), r)).collect(),
    )?;
    h.update(b",\"evidence\":");
    array(
        &mut h,
        b.evidence.iter().map(|r| (r.id.as_str(), r)).collect(),
    )?;
    h.update(b",\"format\":");
    h.update(ids::canonical(&json!(b.format))?);
    h.update(b",\"nodes\":");
    array(&mut h, b.nodes.iter().map(|r| (r.id.as_str(), r)).collect())?;
    h.update(b",\"schema_version\":");
    h.update(ids::canonical(&json!(b.schema_version))?);
    h.update(b",\"snapshot\":");
    h.update(ids::canonical(&serde_json::to_value(&b.snapshot)?)?);
    h.update(b",\"source\":");
    h.update(ids::canonical(&serde_json::to_value(&b.source)?)?);
    h.update(b"}");
    Ok(format!("{:x}", h.finalize()))
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::new("INTEGRITY", message))
    }
}
/// Validate identities, ontology, visibility and exact bytes before publication.
pub fn validate(b: &Bundle, closure: &Closure, bytes: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    validate_shape(b)?;
    require(
        b.snapshot.source_id == b.source.id,
        "snapshot source mismatch",
    )?;
    require(
        b.snapshot.id == b.snapshot.expected_id()?,
        "snapshot identity mismatch",
    )?;
    require(
        b.snapshot.extraction.scope_sha256 == ids::digest(&b.snapshot.scope)?,
        "scope digest mismatch",
    )?;
    require(
        b.snapshot.visibility >= b.source.visibility,
        "snapshot visibility weakens source",
    )?;
    require(
        b.snapshot
            .dependencies
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            == b.snapshot.dependencies.len(),
        "duplicate dependencies",
    )?;
    let mut arts = closure
        .artifacts
        .iter()
        .map(|(id, a)| (id.clone(), a))
        .collect::<BTreeMap<_, _>>();
    let mut evs = closure
        .evidence
        .iter()
        .map(|(id, e)| (id.clone(), e))
        .collect::<BTreeMap<_, _>>();
    let mut nodes = closure
        .nodes
        .iter()
        .map(|(id, n)| (id.clone(), n))
        .collect::<BTreeMap<_, _>>();
    let mut unique = BTreeSet::new();
    for a in &b.artifacts {
        require(unique.insert(&a.id), "duplicate immutable record")?;
        ids::portable_path(&a.path)?;
        require(
            a.id == ids::id("art", &[&b.snapshot.id, &a.path, &a.sha256]),
            "artifact identity mismatch",
        )?;
        require(
            a.visibility >= b.snapshot.visibility,
            "artifact visibility weakens snapshot",
        )?;
        if let Some(raw) = bytes.get(&a.id) {
            require(
                raw.len() as u64 == a.byte_size && ids::sha256(raw) == a.sha256,
                "artifact bytes mismatch",
            )?;
        }
        if let Some(hash) = &a.blob_sha256 {
            require(
                hash == &a.sha256 && bytes.contains_key(&a.id),
                "missing or mismatched blob",
            )?;
        }
        arts.insert(a.id.clone(), a);
    }
    for e in &b.evidence {
        require(unique.insert(&e.id), "duplicate immutable record")?;
        let a = arts
            .get(&e.artifact_id)
            .ok_or_else(|| Error::new("INTEGRITY", "dangling evidence artifact"))?;
        require(
            b.artifacts.iter().any(|x| x.id == a.id),
            "evidence artifact belongs to foreign snapshot",
        )?;
        require(
            e.id == ids::id(
                "ev",
                &[
                    &b.snapshot.id,
                    &e.artifact_id,
                    &ids::digest(&e.locator)?,
                    &e.method,
                    e.provenance_class.as_str(),
                ],
            ),
            "evidence identity mismatch",
        )?;
        require(
            e.visibility >= a.visibility,
            "evidence visibility weakens artifact",
        )?;
        validate_locator(e, a, bytes.get(&a.id).map(Vec::as_slice))?;
        evs.insert(e.id.clone(), e);
    }
    for n in &b.nodes {
        require(unique.insert(&n.id), "duplicate immutable record")?;
        require(
            n.id == ids::id("node", &[&b.snapshot.id, n.kind.as_str(), &n.logical_key]),
            "node identity mismatch",
        )?;
        require(
            n.visibility >= b.snapshot.visibility,
            "node visibility weakens snapshot",
        )?;
        require(
            n.evidence_ids.iter().collect::<BTreeSet<_>>().len() == n.evidence_ids.len(),
            "duplicate node evidence",
        )?;
        for id in &n.evidence_ids {
            let e = evs
                .get(id)
                .ok_or_else(|| Error::new("INTEGRITY", "foreign or missing node evidence"))?;
            require(
                n.visibility >= e.visibility,
                "node visibility weakens evidence",
            )?;
        }
        nodes.insert(n.id.clone(), n);
    }
    let ontology: Value = serde_json::from_str(ONTOLOGY)?;
    for a in &b.assertions {
        require(unique.insert(&a.id), "duplicate immutable record")?;
        require(
            a.id == a.expected_id(&b.snapshot.id)?,
            "assertion identity mismatch",
        )?;
        let s = nodes
            .get(&a.subject)
            .ok_or_else(|| Error::new("INTEGRITY", "foreign or missing subject"))?;
        let o = nodes
            .get(&a.object)
            .ok_or_else(|| Error::new("INTEGRITY", "foreign or missing object"))?;
        let pred = ontology["relations"]
            .as_array()
            .expect("ontology")
            .iter()
            .find(|p| p["name"] == a.predicate.as_str())
            .expect("typed predicate");
        require(
            pred["subject_kinds"]
                .as_array()
                .expect("ontology")
                .iter()
                .any(|k| k == s.kind.as_str())
                && pred["object_kinds"]
                    .as_array()
                    .expect("ontology")
                    .iter()
                    .any(|k| k == o.kind.as_str()),
            "ontology domain/range violation",
        )?;
        require(
            a.visibility >= s.visibility.max(o.visibility).max(b.snapshot.visibility),
            "assertion visibility weakens endpoints",
        )?;
        require(
            a.evidence.iter().any(|e| e.role == "supports"),
            "assertion lacks supporting evidence",
        )?;
        let mut seen = BTreeSet::new();
        for link in &a.evidence {
            require(
                seen.insert((&link.id, &link.role)),
                "duplicate assertion evidence",
            )?;
            let e = evs
                .get(&link.id)
                .ok_or_else(|| Error::new("INTEGRITY", "foreign or missing assertion evidence"))?;
            require(
                a.visibility >= e.visibility,
                "assertion visibility weakens evidence",
            )?;
        }
        if matches!(
            a.provenance_class,
            Provenance::Manual | Provenance::Hypothesis
        ) {
            require(
                a.initial_state == "proposed",
                "manual/hypothesis requires explicit review",
            )?;
        }
    }
    Ok(())
}
fn validate_locator(e: &Evidence, a: &Artifact, raw: Option<&[u8]>) -> Result<()> {
    let l = &e.locator;
    match l["kind"].as_str().unwrap_or("") {
        "source_span" => {
            require(l["path"] == a.path, "span path differs from artifact")?;
            let data =
                raw.ok_or_else(|| Error::new("INTEGRITY", "source span requires verified bytes"))?;
            let s = l["start_byte"].as_u64().unwrap_or(u64::MAX) as usize;
            let end = l["end_byte"].as_u64().unwrap_or(u64::MAX) as usize;
            let text = std::str::from_utf8(data)
                .map_err(|_| Error::new("INTEGRITY", "source artifact is not UTF-8"))?;
            require(
                s <= end
                    && end <= data.len()
                    && text.is_char_boundary(s)
                    && text.is_char_boundary(end),
                "invalid UTF-8 span",
            )?;
            let line = |pos: usize| 1 + data[..pos].iter().filter(|x| **x == b'\n').count() as u64;
            require(
                l["start_line"].as_u64() == Some(line(s))
                    && l["end_line"].as_u64() == Some(line(if end > s { end - 1 } else { end })),
                "source lines do not match byte span",
            )?;
        }
        "asset_field" => {
            require(l["asset_sha256"] == a.sha256, "asset locator hash mismatch")?;
            if l.get("byte_start").is_some() || l.get("byte_end").is_some() {
                let s = l["byte_start"].as_u64().unwrap_or(u64::MAX);
                let end = l["byte_end"].as_u64().unwrap_or(0);
                require(s <= end && end <= a.byte_size, "invalid asset byte range")?;
            }
        }
        "json_pointer" => {
            if let Some(raw) = raw {
                let data = artifact_json(raw)?;
                require(
                    data.pointer(l["pointer"].as_str().unwrap_or("bad"))
                        .is_some(),
                    "JSON pointer does not resolve",
                )?;
            }
        }
        "note" => {
            if let (Some(s), Some(end)) = (l.get("start_byte"), l.get("end_byte")) {
                require(
                    s.as_u64().unwrap_or(u64::MAX) <= end.as_u64().unwrap_or(0)
                        && end.as_u64().unwrap_or(u64::MAX) <= a.byte_size,
                    "invalid note span",
                )?;
            }
        }
        "binary_address" | "pcode" => {
            for key in ["rva", "function_rva", "instruction_rva"] {
                if let Some(v) = l.get(key) {
                    let h = v.as_str().unwrap_or("");
                    require(
                        h.starts_with("0x")
                            && h.len() > 2
                            && h[2..]
                                .bytes()
                                .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x)),
                        "invalid canonical address",
                    )?;
                }
            }
        }
        "capture_event" | "observation" => {}
        _ => return Err(Error::new("INTEGRITY", "unsupported locator")),
    }
    Ok(())
}
/// Parse the actual JSON chunk from a validated GLB or ordinary JSON artifact.
pub fn artifact_json(raw: &[u8]) -> Result<Value> {
    if raw.starts_with(b"glTF") {
        require(raw.len() >= 20, "truncated GLB")?;
        let word = |i| u32::from_le_bytes(raw[i..i + 4].try_into().expect("bounds")) as usize;
        require(
            word(4) == 2 && word(8) == raw.len() && word(16) == 0x4e4f534a,
            "invalid GLB header",
        )?;
        let end = 20usize
            .checked_add(word(12))
            .ok_or_else(|| Error::new("INTEGRITY", "GLB overflow"))?;
        require(end <= raw.len(), "truncated GLB JSON")?;
        Ok(serde_json::from_slice(&raw[20..end])?)
    } else {
        Ok(serde_json::from_slice(raw)?)
    }
}
