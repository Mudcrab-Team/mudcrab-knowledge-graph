//! Import converter inspection DTOs and actual published GLB fields, with verified lineage.
use crate::{
    bundle,
    error::{Error, Result},
    extract::{ExtractionBuilder, tagged_numbers},
    ids,
    model::*,
    store::Store,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
/// Import only explicitly exported NIF/GLB files. No parser or conversion dependency.
pub fn index(
    store: &Store,
    dir: &Path,
    source_id: &str,
    source_snapshot: Option<&str>,
) -> Result<ExtractionBuilder> {
    let raw = std::fs::read(ids::resolve_file(dir, "inspection.json")?)?;
    if raw.len() > crate::bundle::RECORD_LIMIT {
        return Err(Error::new(
            "INVALID_INPUT",
            "inspection DTO exceeds record limit",
        ));
    }
    let report: Value = serde_json::from_slice(&raw)?;
    ids::canonical(&report)?;
    if report["format"] != "mudcrab-material-inspection" || report["schema_version"] != 1 {
        return Err(Error::new(
            "INVALID_SCHEMA",
            "unsupported material inspection DTO",
        ));
    }
    let inputs = report["inputs"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 256)
        .ok_or_else(|| Error::new("INVALID_INPUT", "select 1–256 material inputs"))?;
    let synthetic = report["synthetic"].as_bool().unwrap_or(false);
    let source = Source {
        id: source_id.into(),
        kind: if synthetic {
            "synthetic"
        } else {
            "asset_collection"
        }
        .into(),
        origin_uri: format!("selected-materials:{source_id}"),
        visibility: if synthetic {
            Visibility::Public
        } else {
            Visibility::Restricted
        },
        license_expression: if synthetic {
            Some("MIT OR Apache-2.0".into())
        } else {
            None
        },
        lineage_roots: vec![source_id.into()],
        redistribution: if synthetic {
            "with_notice"
        } else {
            "review_required"
        }
        .into(),
    };
    let deps = source_snapshot
        .map(|s| vec![s.to_owned()])
        .unwrap_or_default();
    let config = json!({"adapter_revision":"material-v2","converter_revision":report["converter_revision"],"source_snapshot":source_snapshot});
    let scope =
        json!({"mode":"selected_materials","synthetic":synthetic,"selected_inputs":inputs.len()});
    let mut b = ExtractionBuilder::new(
        source,
        report["converter_revision"].as_str().unwrap_or("unknown"),
        &ids::sha256(&raw),
        "material-index",
        config,
        scope,
        deps,
    )?;
    let report_art = b.artifact("inspection.json", "application/json", raw)?;
    let root = dir.join("artifacts");
    let mut handlers = BTreeMap::new();
    if let Some(s) = source_snapshot {
        let snapshot: Snapshot = store
            .record(s)?
            .ok_or_else(|| Error::new("MISSING_INPUT", "source snapshot unavailable"))?;
        if report["converter_revision"].as_str() != Some(&snapshot.revision) {
            return Err(Error::new(
                "INTEGRITY",
                "material/source revisions do not match",
            ));
        }
        let mut stmt = store
            .db
            .prepare("SELECT json FROM records WHERE category='node' AND snapshot_id=?1")?;
        for row in stmt.query_map([s], |r| r.get::<_, String>(0))? {
            let n: Node = serde_json::from_str(&row?)?;
            if n.kind == NodeKind::CodeSymbol
                && n.attributes["path"] == "crates/converter/src/material.rs"
            {
                handlers
                    .entry(n.display_name.clone())
                    .or_insert_with(Vec::new)
                    .push(n);
            }
        }
    }
    let manifest = &report["manifest"]["data"];
    let manifest_art = if let Some(path) = report["manifest"]["path"].as_str() {
        let bytes = std::fs::read(ids::resolve_file(&root, path)?)?;
        if ids::sha256(&bytes) != report["manifest"]["sha256"].as_str().unwrap_or("") {
            return Err(Error::new("INTEGRITY", "manifest hash mismatch"));
        }
        Some(b.artifact(path, "application/json", bytes)?)
    } else {
        None
    };
    for (input_index, input) in inputs.iter().enumerate() {
        let path = input["nif"]["path"]
            .as_str()
            .ok_or_else(|| Error::new("INVALID_INPUT", "NIF artifact path missing"))?;
        let bytes = read_artifact(&root, &input["nif"])?;
        let hash = ids::sha256(&bytes);
        if input["inspection"]["asset_sha256"] != hash {
            return Err(Error::new("INTEGRITY", "inspection/NIF hash mismatch"));
        }
        let art = b.artifact(path, "model/vnd.nif", bytes)?;
        let ev=b.evidence(&report_art,json!({"kind":"json_pointer","pointer":format!("/inputs/{input_index}/inspection/header")}),"decoded NIF header",Provenance::Extracted)?;
        let asset = b.node(
            NodeKind::Asset,
            &format!("{hash}:{path}"),
            path,
            json!({"sha256":hash,"stage":"original","synthetic":synthetic}),
            std::slice::from_ref(&ev),
        );
        let mut fields = BTreeMap::new();
        for raw_block in input["inspection"]["raw_blocks"]
            .as_array()
            .ok_or_else(|| Error::new("INVALID_INPUT", "raw blocks missing"))?
        {
            let block = raw_block["block"]
                .as_u64()
                .ok_or_else(|| Error::new("INVALID_INPUT", "raw block identity missing"))?;
            let block_ev = b.evidence(
                &art,
                json!({"kind":"asset_field","asset_sha256":hash,"block":block,"field":"block"}),
                "vendored NIF parser",
                Provenance::Extracted,
            )?;
            let owner=b.node(NodeKind::AssetBlock,&format!("{hash}:block:{block}"),&format!("{} block {block}",raw_block["block_type"].as_str().unwrap_or("unknown")),json!({"block":block,"block_type":raw_block["block_type"],"stage":"raw","decode_status":raw_block["status"]}),std::slice::from_ref(&block_ev));
            b.edge(
                &asset,
                Predicate::Contains,
                &owner,
                json!({}),
                std::slice::from_ref(&block_ev),
                Provenance::Extracted,
            )?;
            if raw_block["status"] != "decoded" {
                continue;
            }
            for (field, value) in raw_block["fields"]
                .as_object()
                .ok_or_else(|| Error::new("INVALID_INPUT", "raw block fields malformed"))?
            {
                let e = b.evidence(
                    &art,
                    json!({"kind":"asset_field","asset_sha256":hash,"block":block,"field":field}),
                    "decoded field, no inferred byte offset",
                    Provenance::Extracted,
                )?;
                let n = b.node(
                    NodeKind::AssetField,
                    &format!("{hash}:block:{block}:{field}"),
                    field,
                    json!({"stage":"raw","block":block,"field":field,"value":value}),
                    std::slice::from_ref(&e),
                );
                b.edge(
                    &owner,
                    Predicate::HasField,
                    &n,
                    json!({}),
                    std::slice::from_ref(&e),
                    Provenance::Extracted,
                )?;
                fields.insert((block, field.clone()), n);
            }
        }
        let mut materials = BTreeMap::new();
        let mut textures = BTreeMap::new();
        for (shape_index, shape) in input["inspection"]["interpreted"]
            .as_array()
            .ok_or_else(|| Error::new("INVALID_INPUT", "interpreted materials missing"))?
            .iter()
            .enumerate()
        {
            let block = shape["shape_block"]
                .as_u64()
                .ok_or_else(|| Error::new("INVALID_INPUT", "shape block missing"))?;
            let e=b.evidence(&report_art,json!({"kind":"json_pointer","pointer":format!("/inputs/{input_index}/inspection/interpreted/{shape_index}")}),"MeshConverter::inspect_nif_materials",Provenance::Extracted)?;
            let n = b.node(
                NodeKind::Material,
                &format!("{hash}:shape:{block}:interpreted"),
                &format!("shape {block} interpreted material"),
                json!({"stage":"interpreted","shape_block":block,"contract":shape}),
                std::slice::from_ref(&e),
            );
            b.edge(
                &asset,
                Predicate::HasMaterial,
                &n,
                json!({"shape_block":block}),
                std::slice::from_ref(&e),
                Provenance::Extracted,
            )?;
            materials.insert(block, n.clone());
            let material = &shape["disposition"]["material"];
            if let Some(shader) = material["shader_block"].as_u64() {
                for ((raw_block, field), raw_field) in &fields {
                    if *raw_block == shader {
                        b.edge(&n,Predicate::DerivedFrom,raw_field,json!({"stage":"interpretation","field":field,"not_original_engine_behavior":true}),std::slice::from_ref(&e),Provenance::Extracted)?;
                    }
                }
            }
            for slot in material["textures"].as_array().into_iter().flatten() {
                let index = slot["slot"]
                    .as_u64()
                    .ok_or_else(|| Error::new("INVALID_INPUT", "texture slot missing"))?;
                let semantic = slot["semantic"].as_str().unwrap_or("unclassified");
                let binding=b.node(NodeKind::TextureBinding,&format!("{hash}:shape:{block}:interpreted:{index}:{semantic}"),&format!("shape {block} slot {index} {semantic}"),json!({"stage":"interpreted","shape_block":block,"slot":index,"semantic":semantic,"path":slot["path"],"required":slot["required"]}),std::slice::from_ref(&e));
                b.edge(
                    &n,
                    Predicate::HasBinding,
                    &binding,
                    json!({}),
                    std::slice::from_ref(&e),
                    Provenance::Extracted,
                )?;
                if let Some(set) = material["texture_set_block"].as_u64()
                    && let Some(raw_fields) = fields.get(&(set, "textures".into()))
                {
                    b.edge(
                        &binding,
                        Predicate::DerivedFrom,
                        raw_fields,
                        json!({"slot":index}),
                        std::slice::from_ref(&e),
                        Provenance::Extracted,
                    )?;
                }
                textures.insert((block, index), binding);
            }
            let handler = match material["shader_family"].as_str() {
                Some("lighting") => Some("build_lighting_material"),
                Some("effect") => Some("build_effect_material"),
                _ => None,
            };
            if let Some(handler) = handler {
                link_handler(&mut b, &n, handler, &handlers, &e)?;
            }
        }
        if !input["glb"].is_null() {
            let output = &input["glb"];
            let output_path = output["path"]
                .as_str()
                .ok_or_else(|| Error::new("INVALID_INPUT", "GLB path missing"))?;
            let raw = read_artifact(&root, output)?;
            let glb = bundle::artifact_json(&raw)?;
            let output_hash = ids::sha256(&raw);
            let ga = b.artifact(output_path, "model/gltf-binary", raw)?;
            let ge = b.evidence(
                &ga,
                json!({"kind":"json_pointer","pointer":""}),
                "actual published GLB JSON",
                Provenance::Extracted,
            )?;
            let gn = b.node(
                NodeKind::Asset,
                &format!("{output_hash}:{output_path}"),
                output_path,
                json!({"sha256":output_hash,"stage":"published"}),
                std::slice::from_ref(&ge),
            );
            let matched = manifest["entries"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.values())
                .any(|entry| {
                    entry["source_hash"] == hash
                        && entry["output_hash"] == output_hash
                        && entry["output"] == output["output_path"]
                });
            let class = if matched {
                Provenance::Extracted
            } else {
                Provenance::Manual
            };
            let step_ev = if let Some(ma) = &manifest_art {
                b.evidence(
                    ma,
                    json!({"kind":"json_pointer","pointer":""}),
                    "published conversion manifest",
                    Provenance::Extracted,
                )?
            } else {
                ge.clone()
            };
            let step=b.node(NodeKind::ConversionStep,&format!("{hash}:{output_hash}:conversion"),"selected material publication",json!({"input_sha256":hash,"output_sha256":output_hash,"configuration_hash":manifest["configuration_hash"],"converter_schema":manifest["schema_version"],"converter_revision":report["converter_revision"],"revision_origin":report["converter_revision_origin"],"lineage_status":if matched{"verified_manifest_hashes"}else{"operator_pair_unverified"}}),std::slice::from_ref(&step_ev));
            b.edge(
                &asset,
                Predicate::InputTo,
                &step,
                json!({"lineage_status":if matched{"verified"}else{"proposed"}}),
                std::slice::from_ref(&step_ev),
                class,
            )?;
            b.edge(
                &step,
                Predicate::Produces,
                &gn,
                json!({}),
                std::slice::from_ref(&step_ev),
                class,
            )?;
            let mut used_shapes = std::collections::BTreeSet::new();
            for (index, m) in glb["materials"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let shape = m
                    .pointer("/extras/openSkyrim/shapeBlock")
                    .and_then(Value::as_u64);
                let pointer = format!("/materials/{index}");
                let e = b.evidence(
                    &ga,
                    json!({"kind":"json_pointer","pointer":pointer}),
                    "actual final GLB material",
                    Provenance::Extracted,
                )?;
                let output_material=b.node(NodeKind::Material,&format!("{output_hash}:{pointer}"),&format!("GLB material {index}"),json!({"stage":"published","material_index":index,"shape_block":shape,"fields":tagged_numbers(m.clone()),"OPEN_SKYRIM_material":if m.pointer("/extensions/OPEN_SKYRIM_material").is_some(){"present"}else{"absent; flags remain unknown"}}),std::slice::from_ref(&e));
                b.edge(
                    &gn,
                    Predicate::HasMaterial,
                    &output_material,
                    json!({}),
                    std::slice::from_ref(&e),
                    Provenance::Extracted,
                )?;
                b.edge(
                    &step,
                    Predicate::Produces,
                    &output_material,
                    json!({}),
                    std::slice::from_ref(&e),
                    class,
                )?;
                if let Some(shape) = shape {
                    if !used_shapes.insert(shape) {
                        return Err(Error::new("INTEGRITY", "duplicate output shape identity"));
                    }
                    if let Some(interpreted) = materials.get(&shape) {
                        b.edge(&output_material,Predicate::DerivedFrom,interpreted,json!({"join":"source hash + original shape block; names/array order unused"}),std::slice::from_ref(&e),class)?;
                    }
                }
                for (field, value) in m.as_object().into_iter().flat_map(|o| o.iter()) {
                    if matches!(
                        field.as_str(),
                        "doubleSided"
                            | "pbrMetallicRoughness"
                            | "alphaMode"
                            | "alphaCutoff"
                            | "extensions"
                            | "normalTexture"
                            | "emissiveTexture"
                    ) {
                        let p = format!("{pointer}/{field}");
                        let fe = b.evidence(
                            &ga,
                            json!({"kind":"json_pointer","pointer":p}),
                            "observed GLB field",
                            Provenance::Extracted,
                        )?;
                        let f=b.node(NodeKind::AssetField,&format!("{output_hash}:{p}"),field,json!({"stage":"published","pointer":p,"value":tagged_numbers(value.clone()),"shape_block":shape}),std::slice::from_ref(&fe));
                        b.edge(
                            &output_material,
                            Predicate::HasField,
                            &f,
                            json!({}),
                            std::slice::from_ref(&fe),
                            Provenance::Extracted,
                        )?;
                    }
                }
                link_handler(&mut b, &output_material, "publish_material", &handlers, &e)?;
                texture_outputs(
                    &mut b,
                    PublishedMaterial {
                        glb: &glb,
                        m,
                        art: &ga,
                        hash: &output_hash,
                        pointer: &pointer,
                        shape,
                    },
                    &output_material,
                    &textures,
                    class,
                )?;
            }
            for (output_name, paths) in manifest["pruned_texture_references"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.iter())
            {
                if Some(output_name.as_str()) != output["output_path"].as_str() {
                    continue;
                }
                for path in paths
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    let binding=b.node(NodeKind::TextureBinding,&format!("{hash}:{output_hash}:pruned:{path}"),path,json!({"stage":"pruned","path":path,"reason":"published manifest decision"}),std::slice::from_ref(&step_ev));
                    b.edge(
                        &step,
                        Predicate::PrunedReference,
                        &binding,
                        json!({}),
                        std::slice::from_ref(&step_ev),
                        class,
                    )?;
                }
            }
        }
    }
    let raw_partial = inputs.iter().any(|input| {
        input["inspection"]["raw_blocks"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|block| block["status"] != "decoded")
    });
    b.capability("materials", if raw_partial{"partial"}else{"complete"},"decoded fields, interpreted contract and actual GLB observations kept distinct; lineage admitted only for matching published hashes; runtime extension handling unresolved",inputs.len()as u64,0,0);
    Ok(b)
}
fn read_artifact(root: &Path, record: &Value) -> Result<Vec<u8>> {
    let path = record["path"]
        .as_str()
        .ok_or_else(|| Error::new("INVALID_INPUT", "artifact path missing"))?;
    let path = ids::resolve_file(root, path)?;
    if std::fs::metadata(&path)?.len() > 128 * 1024 * 1024 {
        return Err(Error::new(
            "INVALID_INPUT",
            "material artifact exceeds limit",
        ));
    }
    let bytes = std::fs::read(path)?;
    if ids::sha256(&bytes) != record["sha256"].as_str().unwrap_or("") {
        return Err(Error::new("INTEGRITY", "material artifact hash mismatch"));
    }
    Ok(bytes)
}
fn link_handler(
    b: &mut ExtractionBuilder,
    node: &str,
    name: &str,
    handlers: &BTreeMap<String, Vec<Node>>,
    evidence: &str,
) -> Result<()> {
    if let Some(matches) = handlers.get(name)
        && matches.len() == 1
    {
        let target = &matches[0];
        let mut ev = vec![evidence.into()];
        ev.extend(target.evidence_ids.clone());
        b.edge(node,Predicate::DerivedFrom,&target.id,json!({"selector":{"path":"crates/converter/src/material.rs","symbol":name},"relation":"converter implementation at declared revision","runtime_consumption":"unresolved"}),&ev,Provenance::Extracted)?;
    }
    Ok(())
}
struct PublishedMaterial<'a> {
    glb: &'a Value,
    m: &'a Value,
    art: &'a str,
    hash: &'a str,
    pointer: &'a str,
    shape: Option<u64>,
}
fn texture_outputs(
    b: &mut ExtractionBuilder,
    published: PublishedMaterial<'_>,
    material: &str,
    interpreted: &BTreeMap<(u64, u64), String>,
    class: Provenance,
) -> Result<()> {
    let PublishedMaterial {
        glb,
        m,
        art,
        hash,
        pointer,
        shape,
    } = published;
    let slots = [
        (
            "/pbrMetallicRoughness/baseColorTexture",
            0,
            "diffuse",
            "srgb",
        ),
        ("/normalTexture", 1, "normal", "linear"),
        ("/emissiveTexture", 2, "glow", "srgb"),
        (
            "/extensions/KHR_materials_specular/specularColorTexture",
            7,
            "specular",
            "srgb",
        ),
    ];
    let mut selected = vec![];
    for (p, slot, semantic, color) in slots {
        if let Some(index) = m.pointer(p).and_then(|v| v["index"].as_u64()) {
            selected.push((
                format!("{pointer}{p}"),
                slot,
                semantic.to_owned(),
                color.to_owned(),
                index,
            ));
        }
    }
    for (i, slot) in m
        .pointer("/extensions/OPEN_SKYRIM_material/textureSlots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let (Some(index), Some(original_slot)) =
            (slot["texture"].as_u64(), slot["slot"].as_u64())
        {
            selected.push((
                format!("{pointer}/extensions/OPEN_SKYRIM_material/textureSlots/{i}"),
                original_slot,
                slot["semantic"].as_str().unwrap_or("unclassified").into(),
                slot["colorSpace"].as_str().unwrap_or("unknown").into(),
                index,
            ));
        }
    }
    for (p, slot, semantic, color, index) in selected {
        let texture = &glb["textures"][index as usize];
        let image = texture
            .pointer("/extensions/KHR_texture_basisu/source")
            .or_else(|| texture.get("source"))
            .and_then(Value::as_u64);
        let uri = image.and_then(|i| glb["images"][i as usize]["uri"].as_str());
        let ev = b.evidence(
            art,
            json!({"kind":"json_pointer","pointer":p}),
            "actual material texture index",
            Provenance::Extracted,
        )?;
        let n=b.node(NodeKind::TextureBinding,&format!("{hash}:{p}:{slot}:{color}"),&format!("published slot {slot} {semantic} {color}"),json!({"stage":"published","shape_block":shape,"slot":slot,"semantic":semantic,"color_space":color,"texture_index":index,"image_index":image,"uri":uri}),std::slice::from_ref(&ev));
        b.edge(
            material,
            Predicate::HasBinding,
            &n,
            json!({}),
            std::slice::from_ref(&ev),
            Provenance::Extracted,
        )?;
        if let Some(shape) = shape
            && let Some(input) = interpreted.get(&(shape, slot))
        {
            b.edge(
                &n,
                Predicate::DerivedFrom,
                input,
                json!({"slot":slot,"color_space":color}),
                std::slice::from_ref(&ev),
                class,
            )?;
        }
    }
    Ok(())
}
