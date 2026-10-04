use mudcrab_knowledge_graph::{
    error::Result,
    ids,
    model::*,
    query::*,
    source,
    store::{ContextManifest, Store},
};
use serde_json::json;
use std::process::Command;
fn origin() -> Source {
    Source {
        id: "mudcrab".into(),
        kind: "repository".into(),
        origin_uri: "https://github.com/Mudcrab-Team/mudcrab".into(),
        visibility: Visibility::Public,
        license_expression: Some("MIT OR Apache-2.0".into()),
        lineage_roots: vec!["mudcrab".into()],
        redistribution: "with_notice".into(),
    }
}
#[test]
fn pinned_git_source_index() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut store = Store::init(dir.path())?;
    // Index real, committed source bytes in an original temporary Git repository.
    // External Mudcrab coverage is provided separately by smoke_handoff.py --repo.
    let (repo, revision) = local_repo(&[(
        "material.rs",
        "pub struct ValidatedNifMaterial { pub double_sided: bool }\n",
    )]);
    let b = source::index(
        &store,
        source::SourceRequest {
            repo: repo.path(),
            source: &origin(),
            revision: &revision,
            selected: &["material.rs".into()],
            scip: None,
            encoding_override: None,
            configuration: json!({"target":"syntactic","features":"unexpanded"}),
        },
    )?;
    assert!(
        b.bundle
            .nodes
            .iter()
            .any(|n| n.display_name == "ValidatedNifMaterial")
    );
    assert!(
        b.bundle
            .nodes
            .iter()
            .any(|n| n.display_name == "double_sided" && n.kind == NodeKind::TypeField)
    );
    assert!(
        !b.bundle
            .assertions
            .iter()
            .any(|a| a.predicate == Predicate::CallsDirect)
    );
    store.ingest_bytes(&b.bundle, &b.bytes)?;
    let ctx = store.create_context(&ContextManifest {
        schema_version: 1,
        name: "source".into(),
        snapshot_ids: vec![b.bundle.snapshot.id.clone()],
        policy: "public".into(),
        notes: None,
    })?;
    let q = Query::new(&store, ctx);
    let e = q.search(SearchInput {
        query: "ValidatedNifMaterial".into(),
        kinds: vec![],
        sources: vec![],
        limit: 20,
        cursor: None,
    })?;
    assert!(!e.result["nodes"].as_array().unwrap().is_empty());
    assert!(e.evidence.iter().any(|e| e["revision"] == revision));
    Ok(())
}
fn local_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    for (p, t) in files {
        std::fs::write(dir.path().join(p), t).unwrap();
    }
    for args in [
        vec!["init"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=KG Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "original fixture",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let sha = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .into();
    (dir, sha)
}
#[test]
fn scip_reference_is_not_call() -> Result<()> {
    use protobuf::Message;
    let text = "pub fn probe() {}\n";
    let (repo, rev) = local_repo(&[("toy.rs", text)]);
    let mut doc = scip::types::Document::new();
    doc.relative_path = "toy.rs".into();
    doc.language = "rust".into();
    doc.text = text.into();
    doc.position_encoding = protobuf::EnumOrUnknown::from_i32(1);
    let mut occurrence = scip::types::Occurrence::new();
    occurrence.range = vec![0, 7, 12];
    occurrence.symbol = "local 0".into();
    occurrence.symbol_roles = 0;
    doc.occurrences.push(occurrence);
    let mut index = scip::types::Index::new();
    index.documents.push(doc);
    let path = repo.path().join("index.scip");
    std::fs::write(&path, index.write_to_bytes().unwrap())?;
    let d = tempfile::tempdir()?;
    let store = Store::init(d.path())?;
    let b = source::index(
        &store,
        source::SourceRequest {
            repo: repo.path(),
            source: &origin(),
            revision: &rev,
            selected: &[],
            scip: Some(&path),
            encoding_override: None,
            configuration: json!({"target":"toy","features":[]}),
        },
    )?;
    assert!(
        b.bundle
            .nodes
            .iter()
            .any(|n| n.logical_key == "toy.rs::local 0")
    );
    assert!(
        b.bundle
            .assertions
            .iter()
            .any(|a| a.predicate == Predicate::References)
    );
    assert!(
        !b.bundle
            .assertions
            .iter()
            .any(|a| a.predicate == Predicate::CallsDirect)
    );
    Ok(())
}
#[test]
fn structural_languages() -> Result<()> {
    let (repo, rev) = local_repo(&[
        (
            "toy.hpp",
            "#if SOME_RUNTIME\nstruct Material { bool double_sided; };\n#endif\n",
        ),
        (
            "toy.wgsl",
            "// @binding(999) ignored\n@group(#{MATERIAL_BIND_GROUP})\n@binding(100) var texture_map: texture_2d<f32>;\nfn shade() -> f32 { return 1.0; }\n",
        ),
        ("toy.md", "# Material\nOriginal notes.\n"),
        (
            "nif.xml",
            "<nif><bitflags name=\"ShaderFlags2\" versions=\"20.2.0.7\"><option name=\"DoubleSided\" value=\"4\"/></bitflags></nif>",
        ),
    ]);
    let d = tempfile::tempdir()?;
    let s = Store::init(d.path())?;
    let b = source::index(
        &s,
        source::SourceRequest {
            repo: repo.path(),
            source: &origin(),
            revision: &rev,
            selected: &[],
            scip: None,
            encoding_override: None,
            configuration: json!({}),
        },
    )?;
    assert!(b.bundle.nodes.iter().any(|n| n.display_name == "Material"));
    assert!(b.bundle.nodes.iter().any(|n| {
        n.kind == NodeKind::ShaderBinding
            && n.attributes["annotations"]
                .to_string()
                .contains("MATERIAL_BIND_GROUP")
    }));
    assert!(
        b.bundle
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::DocumentSection)
    );
    assert!(
        b.bundle
            .nodes
            .iter()
            .any(|n| n.display_name == "DoubleSided")
    );
    Ok(())
}
#[test]
fn unicode_scip_positions() {
    let text = "é🦀A\n";
    assert_eq!(source::position_to_byte(text, 0, 3, "utf16").unwrap(), 6);
    assert_eq!(source::position_to_byte(text, 0, 2, "utf32").unwrap(), 6);
    assert_eq!(source::position_to_byte(text, 0, 6, "utf8").unwrap(), 6);
    assert!(source::position_to_byte(text, 0, 2, "utf16").is_err());
    assert!(source::position_to_byte(text, 0, 1, "utf8").is_err());
    assert_ne!(ids::id("node", &["A.rs"]), ids::id("node", &["a.rs"]));
}
#[test]
fn incremental_source_history_and_configuration_invalidation() -> Result<()> {
    let (repo, rev1) = local_repo(&[("toy.rs", "pub fn kept() {}\npub fn removed() {}\n")]);
    let d = tempfile::tempdir()?;
    let mut s = Store::init(d.path())?;
    let index = |s: &Store, rev: &str, config| {
        source::index(
            s,
            source::SourceRequest {
                repo: repo.path(),
                source: &origin(),
                revision: rev,
                selected: &[],
                scip: None,
                encoding_override: None,
                configuration: config,
            },
        )
    };
    let first = index(&s, &rev1, json!({"features":[]}))?;
    s.ingest_bytes(&first.bundle, &first.bytes)?;
    let old = s.create_context(&ContextManifest {
        schema_version: 1,
        name: "history".into(),
        snapshot_ids: vec![first.bundle.snapshot.id.clone()],
        policy: "public".into(),
        notes: None,
    })?;
    let repeat = index(&s, &rev1, json!({"features":[]}))?;
    assert_eq!(first.bundle, repeat.bundle);
    assert_eq!(
        s.ingest_bytes(&repeat.bundle, &repeat.bytes)?["idempotent"],
        true
    );
    std::fs::write(repo.path().join("toy.rs"), "pub fn kept() {}\n")?;
    for args in [
        vec!["add", "toy.rs"],
        vec![
            "-c",
            "user.name=KG Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "remove declaration",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .output()?
                .status
                .success()
        );
    }
    let rev2 = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["rev-parse", "HEAD"])
            .output()?
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let second = index(&s, &rev2, json!({"features":[]}))?;
    s.ingest_bytes(&second.bundle, &second.bytes)?;
    let new = s.create_context(&ContextManifest {
        schema_version: 1,
        name: "history".into(),
        snapshot_ids: vec![second.bundle.snapshot.id.clone()],
        policy: "public".into(),
        notes: None,
    })?;
    assert_ne!(old.id, new.id);
    let search = || SearchInput {
        query: "removed".into(),
        kinds: vec![],
        sources: vec![],
        limit: 20,
        cursor: None,
    };
    assert!(
        !Query::new(&s, old.clone()).search(search())?.result["nodes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        Query::new(&s, new).search(search())?.result["nodes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(s.context(&old.id, None)?.id, old.id);
    let config = index(&s, &rev2, json!({"features":["different-target"]}))?;
    assert_ne!(config.bundle.snapshot.id, second.bundle.snapshot.id);
    Ok(())
}
