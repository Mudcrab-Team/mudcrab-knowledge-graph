use mudcrab_knowledge_graph::{
    bundle,
    error::Result,
    extract::ExtractionBuilder,
    ids,
    model::*,
    query::*,
    store::{ContextManifest, Store},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/knowledge-graph")
}
fn fixture() -> (Bundle, BTreeMap<String, Vec<u8>>) {
    let source = Source {
        id: "test-source".into(),
        kind: "synthetic".into(),
        origin_uri: "synthetic:original".into(),
        visibility: Visibility::Public,
        license_expression: Some("MIT".into()),
        lineage_roots: vec!["test-source".into()],
        redistribution: "with_notice".into(),
    };
    let text = "// é 🦀\npub struct Material { pub double_sided: bool }\n";
    let mut b = ExtractionBuilder::new(
        source,
        "fixture-v1",
        &ids::sha256(text.as_bytes()),
        "test-extraction",
        json!({}),
        json!({"paths":["toy.rs"]}),
        vec![],
    )
    .unwrap();
    let a = b
        .artifact("toy.rs", "text/plain", text.as_bytes().to_vec())
        .unwrap();
    let e=b.evidence(&a,json!({"kind":"source_span","path":"toy.rs","start_byte":0,"end_byte":text.len(),"start_line":1,"end_line":2,"encoding":"utf8_bytes"}),"literal",Provenance::Extracted).unwrap();
    let f = b.node(
        NodeKind::SourceFile,
        "toy.rs",
        "toy.rs",
        json!({}),
        std::slice::from_ref(&e),
    );
    let n = b.node(
        NodeKind::CodeSymbol,
        "toy.rs::Material",
        "Material",
        json!({"meaning":"literal declaration"}),
        std::slice::from_ref(&e),
    );
    b.edge(
        &f,
        Predicate::Declares,
        &n,
        json!({}),
        &[e],
        Provenance::Extracted,
    )
    .unwrap();
    (b.bundle, b.bytes)
}
fn context(store: &mut Store, b: &Bundle, policy: &str) -> mudcrab_knowledge_graph::store::Context {
    store
        .create_context(&ContextManifest {
            schema_version: 1,
            name: "test".into(),
            snapshot_ids: vec![b.snapshot.id.clone()],
            policy: policy.into(),
            notes: None,
        })
        .unwrap()
}
#[test]
fn identity_vectors() {
    let v: Value = serde_json::from_slice(
        &std::fs::read(package().join("examples/identity-vectors.json")).unwrap(),
    )
    .unwrap();
    for x in v["vectors"].as_array().unwrap() {
        let p = x["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids::id(x["kind"].as_str().unwrap(), &p), x["expected"]);
    }
}
#[test]
fn rejects_noncanonical_values() {
    for v in [
        json!(1.0),
        json!(9007199254740992u64),
        serde_json::from_str::<Value>("-0").unwrap(),
    ] {
        assert!(ids::canonical(&v).is_err());
    }
    assert_eq!(
        String::from_utf8(ids::canonical(&json!({"z":"é/🦀","a":"\n"})).unwrap()).unwrap(),
        "{\"a\":\"\\n\",\"z\":\"é/🦀\"}"
    );
    for p in ["/foo", "a/../b", "C:/a", "a\\b", "a\0b", "a//b", "a/./b"] {
        assert!(ids::portable_path(p).is_err());
    }
}
#[test]
fn validates_bundle_integrity() {
    let (b, bytes) = fixture();
    bundle::validate(&b, &Default::default(), &bytes).unwrap();
    let mut bad = b.clone();
    bad.nodes[0].evidence_ids.push(ids::id("ev", &["foreign"]));
    assert!(bundle::validate(&bad, &Default::default(), &bytes).is_err());
    let mut bad = b.clone();
    bad.assertions[0].predicate = Predicate::CallsDirect;
    bad.assertions[0].id = bad.assertions[0].expected_id(&bad.snapshot.id).unwrap();
    assert!(bundle::validate(&bad, &Default::default(), &bytes).is_err());
    let mut badbytes = bytes.clone();
    badbytes.values_mut().next().unwrap()[0] = b'!';
    assert!(bundle::validate(&b, &Default::default(), &badbytes).is_err());
}
#[test]
fn imports_contract_examples() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Store::init(dir.path())?;
    for name in [
        "01-synthetic-source",
        "02-synthetic-material",
        "03-synthetic-native",
        "04-synthetic-capture",
    ] {
        let loaded = bundle::load(&package().join(format!("examples/{name}.bundle.json")))?;
        s.ingest(&loaded.bundle, Some(&package().join("examples/artifacts")))?;
    }
    Ok(())
}
#[test]
fn initialized_database_enables_wal() {
    let dir = tempfile::tempdir().unwrap();
    Store::init(dir.path()).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("graph.sqlite")).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}
#[test]
fn atomic_import_and_idempotency() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (b, raw) = fixture();
    let db = rusqlite::Connection::open(dir.path().join("graph.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_publication BEFORE INSERT ON nodes BEGIN SELECT RAISE(ABORT,'injected interruption'); END;").unwrap();
    assert!(s.ingest_bytes(&b, &raw).is_err());
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM snapshots WHERE state='published'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    db.execute_batch("DROP TRIGGER fail_publication;").unwrap();
    s.ingest_bytes(&b, &raw).unwrap();
    assert_eq!(s.ingest_bytes(&b, &raw).unwrap()["idempotent"], true);
}
#[test]
fn rejects_immutable_collision() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (mut b, raw) = fixture();
    s.ingest_bytes(&b, &raw).unwrap();
    assert_eq!(s.ingest_bytes(&b, &raw).unwrap()["idempotent"], true);
    b.nodes[0].display_name = "changed".into();
    assert_eq!(
        s.ingest_bytes(&b, &raw).unwrap_err().code,
        "IDENTITY_COLLISION"
    );
}
#[test]
fn pinned_context_and_unicode_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (b, raw) = fixture();
    s.ingest_bytes(&b, &raw).unwrap();
    let ctx = context(&mut s, &b, "public");
    let q = Query::new(&s, ctx);
    let e = q
        .evidence(EvidenceInput {
            evidence_id: b.evidence[0].id.clone(),
            max_bytes: 4096,
        })
        .unwrap();
    assert!(e.result["excerpt"].as_str().unwrap().contains("é 🦀"));
    let small = q
        .evidence(EvidenceInput {
            evidence_id: b.evidence[0].id.clone(),
            max_bytes: 1,
        })
        .unwrap_err();
    assert_eq!(small.code, "BUDGET_TOO_SMALL");
}
#[test]
fn privacy_closure() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (mut b, raw) = fixture();
    b.nodes[1].visibility = Visibility::Restricted;
    b.nodes[1].display_name = "PRIVATE_MARKER_937".into();
    b.assertions[0].visibility = Visibility::Restricted;
    s.ingest_bytes(&b, &raw).unwrap();
    let q = Query::new(&s, context_dummy(&b));
    let result = q
        .search(SearchInput {
            query: "PRIVATE_MARKER_937".into(),
            kinds: vec![],
            sources: vec![],
            limit: 20,
            cursor: None,
        })
        .unwrap();
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("PRIVATE_MARKER_937")
    );
    assert!(
        q.node(NodeInput {
            node_id: b.nodes[1].id.clone()
        })
        .is_err()
    );
    let edges = q
        .neighbors(NeighborsInput {
            node_id: b.nodes[0].id.clone(),
            direction: "out".into(),
            predicates: vec![],
            limit: 20,
            cursor: None,
        })
        .unwrap();
    assert_eq!(edges.result["assertions"].as_array().unwrap().len(), 0);
}
fn context_dummy(b: &Bundle) -> mudcrab_knowledge_graph::store::Context {
    mudcrab_knowledge_graph::store::Context {
        id: ids::id("ctx", &["public test"]),
        snapshots: vec![b.snapshot.id.clone()],
        ceiling: Visibility::Public,
        config: json!({"profile":"public"}),
    }
}
#[test]
fn append_only_review() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (mut b, raw) = fixture();
    b.assertions[0].provenance_class = Provenance::Hypothesis;
    b.assertions[0].initial_state = "proposed".into();
    b.assertions[0].id = b.assertions[0].expected_id(&b.snapshot.id).unwrap();
    s.ingest_bytes(&b, &raw).unwrap();
    let a = &b.assertions[0];
    let r1 = s.review(&a.id, "analyst", "admitted", "tested").unwrap();
    let r2 = s
        .review(&a.id, "analyst", "withdrawn", "new evidence")
        .unwrap();
    assert!(r2["review_sequence"].as_i64() > r1["review_sequence"].as_i64());
    assert_eq!(
        s.record::<Assertion>(&a.id)
            .unwrap()
            .unwrap()
            .provenance_class,
        Provenance::Hypothesis
    );
}
#[test]
fn bounded_search_and_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (mut b, raw) = fixture();
    for i in 0..3 {
        let mut n = b.nodes[1].clone();
        n.logical_key = format!("Material{i}");
        n.display_name = format!("Material{i}");
        n.id = ids::id("node", &[&b.snapshot.id, n.kind.as_str(), &n.logical_key]);
        b.nodes.push(n);
    }
    s.ingest_bytes(&b, &raw).unwrap();
    let ctx = context(&mut s, &b, "public");
    let mut input = SearchInput {
        query: "literal".into(),
        kinds: vec![],
        sources: vec![],
        limit: 1,
        cursor: None,
    };
    let q = Query::new(&s, ctx.clone());
    let first = q.search(input.clone()).unwrap();
    assert!(first.truncated);
    input.cursor = first.next_cursor;
    let next = q.search(input.clone()).unwrap();
    assert_ne!(
        first.result["nodes"][0]["node"]["id"],
        next.result["nodes"][0]["node"]["id"]
    );
    drop(q);
    s.review(
        &b.assertions[0].id,
        "analyst",
        "rejected",
        "cursor invalidation",
    )
    .unwrap();
    assert_eq!(
        Query::new(&s, ctx).search(input).unwrap_err().code,
        "STALE_CURSOR"
    );
}
#[test]
fn bounded_adjacency_and_trace() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (b, raw) = fixture();
    s.ingest_bytes(&b, &raw).unwrap();
    let q = Query::new(&s, context_dummy(&b));
    let n = q
        .neighbors(NeighborsInput {
            node_id: b.nodes[0].id.clone(),
            direction: "out".into(),
            predicates: vec![],
            limit: 20,
            cursor: None,
        })
        .unwrap();
    assert_eq!(n.result["assertions"].as_array().unwrap().len(), 1);
    let t = q
        .trace(TraceInput {
            mode: "node".into(),
            seed: b.nodes[0].id.clone(),
            include_hypotheses: false,
            limit: 20,
        })
        .unwrap();
    assert!(
        t.result["missing_stages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["stage"] == "native")
    );
}
#[test]
fn bounded_packet() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    let (b, raw) = fixture();
    s.ingest_bytes(&b, &raw).unwrap();
    let q = Query::new(&s, context_dummy(&b));
    let e = q
        .pack(ContextInput {
            seeds: vec![Seed {
                kind: "node".into(),
                value: b.nodes[0].id.clone(),
            }],
            max_bytes: 4096,
            include_hypotheses: true,
        })
        .unwrap();
    let bytes = serde_json::to_vec(&e).unwrap();
    assert!(bytes.len() <= 4096);
    assert_eq!(e.result["byte_count"].as_u64(), Some(bytes.len() as u64));
}
#[test]
fn path_symlink_escape() {
    use std::os::unix::fs::symlink;
    let d = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("secret"), "private").unwrap();
    symlink(other.path(), d.path().join("escape")).unwrap();
    assert_eq!(
        ids::resolve_file(d.path(), "escape/secret")
            .unwrap_err()
            .code,
        "POLICY"
    );
}
#[test]
fn proposal_review_conflict_and_idempotency() {
    use mudcrab_knowledge_graph::proposals;
    let d = tempfile::tempdir().unwrap();
    let mut s = Store::init(d.path()).unwrap();
    let (mut b, raw) = fixture();
    let mut conflict_evidence = b.evidence[0].clone();
    conflict_evidence.method = "operator conflict note".into();
    conflict_evidence.provenance_class = Provenance::Manual;
    conflict_evidence.id = ids::id(
        "ev",
        &[
            &b.snapshot.id,
            &conflict_evidence.artifact_id,
            &ids::digest(&conflict_evidence.locator).unwrap(),
            &conflict_evidence.method,
            "manual",
        ],
    );
    b.evidence.push(conflict_evidence.clone());
    s.ingest_bytes(&b, &raw).unwrap();
    let ctx = context(&mut s, &b, "public");
    let proposal = json!({"schema_version":1,"context_id":ctx.id,"proposals":[{"proposal_key":"candidate","subject":b.nodes[0].id,"predicate":"CORRESPONDS_TO","object":b.nodes[1].id,"explanation":"Synthetic candidate only; no equivalence claim","supporting_evidence_ids":[b.evidence[0].id],"missing_evidence":["native analysis"],"producer":{"kind":"test","name":"original-toy"}}]});
    let p = proposals::index(&s, &serde_json::to_vec(&proposal).unwrap()).unwrap();
    assert_eq!(
        p.bundle.assertions[0].provenance_class,
        Provenance::Hypothesis
    );
    assert_eq!(p.bundle.assertions[0].initial_state, "proposed");
    s.ingest_bytes(&p.bundle, &p.bytes).unwrap();
    assert_eq!(
        s.ingest_bytes(&p.bundle, &p.bytes).unwrap()["idempotent"],
        true
    );
    let ctx = context(&mut s, &p.bundle, "public");
    let aid = p.bundle.assertions[0].id.clone();
    let c = proposals::conflict(
        &mut s,
        ctx.clone(),
        proposals::ConflictInput {
            assertion_a: b.assertions[0].id.clone(),
            assertion_b: aid.clone(),
            evidence_id: conflict_evidence.id.clone(),
            reason: "Synthetic research conflict".into(),
        },
    )
    .unwrap();
    assert!(c["conflict_id"].as_str().is_some());
    let packet = Query::new(&s, ctx.clone())
        .pack(ContextInput {
            seeds: vec![Seed {
                kind: "node".into(),
                value: b.nodes[0].id.clone(),
            }],
            max_bytes: 65536,
            include_hypotheses: true,
        })
        .unwrap();
    assert_eq!(packet.result["hypotheses"].as_array().unwrap().len(), 1);
    assert_eq!(packet.result["conflicts"].as_array().unwrap().len(), 1);
    assert!(
        packet
            .evidence
            .iter()
            .any(|e| e["id"] == conflict_evidence.id)
    );
    s.review(
        &aid,
        "test operator",
        "rejected",
        "No supported correspondence",
    )
    .unwrap();
    let packet = Query::new(&s, ctx)
        .pack(ContextInput {
            seeds: vec![Seed {
                kind: "node".into(),
                value: b.nodes[0].id.clone(),
            }],
            max_bytes: 65536,
            include_hypotheses: true,
        })
        .unwrap();
    assert!(packet.result["hypotheses"].as_array().unwrap().is_empty());
    assert!(packet.result["conflicts"].as_array().unwrap().is_empty());
}
#[test]
fn fresh_public_export_has_only_permitted_closure() {
    use mudcrab_knowledge_graph::export;
    let d = tempfile::tempdir().unwrap();
    let out = d.path().join("new-public");
    let mut s = Store::init(&d.path().join("local")).unwrap();
    let (mut b, mut raw) = fixture();
    let marker = "PRIVATE_MARKER_EXPORT_8317";
    let path = "restricted.txt";
    let hash = ids::sha256(marker.as_bytes());
    let art = ids::id("art", &[&b.snapshot.id, path, &hash]);
    b.artifacts.push(Artifact {
        id: art.clone(),
        path: path.into(),
        sha256: hash.clone(),
        media_type: "text/plain".into(),
        byte_size: marker.len() as u64,
        visibility: Visibility::Restricted,
        blob_sha256: Some(hash),
    });
    raw.insert(art.clone(), marker.as_bytes().to_vec());
    let loc = json!({"kind":"note","path":path,"section":"private"});
    let ev = ids::id(
        "ev",
        &[
            &b.snapshot.id,
            &art,
            &ids::digest(&loc).unwrap(),
            "private note",
            "manual",
        ],
    );
    b.evidence.push(Evidence {
        id: ev.clone(),
        artifact_id: art,
        locator: loc,
        method: "private note".into(),
        provenance_class: Provenance::Manual,
        visibility: Visibility::Restricted,
    });
    let n = Node {
        id: ids::id("node", &[&b.snapshot.id, "DocumentSection", marker]),
        kind: NodeKind::DocumentSection,
        logical_key: marker.into(),
        display_name: marker.into(),
        attributes: json!({"secret":marker}),
        evidence_ids: vec![ev],
        visibility: Visibility::Restricted,
    };
    let hidden = n.id.clone();
    b.nodes.push(n);
    s.ingest_bytes(&b, &raw).unwrap();
    let ctx = context(&mut s, &b, "research-local");
    let report = export::public(&s, &ctx, &out).unwrap();
    assert_eq!(report["nodes"], 2);
    assert!(!out.join("graph.sqlite-wal").exists());
    assert!(!out.join("graph.sqlite-shm").exists());
    for entry in std::fs::read_dir(&out).unwrap() {
        let e = entry.unwrap();
        if e.path().is_file() {
            let bytes = std::fs::read(e.path()).unwrap();
            assert!(!bytes.windows(marker.len()).any(|s| s == marker.as_bytes()));
            assert!(!bytes.windows(hidden.len()).any(|s| s == hidden.as_bytes()));
        }
    }
    let dst = Store::open(&out, false).unwrap();
    let q = Query::new(&dst, dst.context("public", None).unwrap());
    let hit = q
        .search(SearchInput {
            query: "Material".into(),
            kinds: vec![],
            sources: vec![],
            limit: 20,
            cursor: None,
        })
        .unwrap();
    assert!(!hit.result["nodes"].as_array().unwrap().is_empty());
    let id = hit.result["nodes"][0]["node"]["evidence_ids"][0]
        .as_str()
        .unwrap();
    assert_eq!(
        q.evidence(EvidenceInput {
            evidence_id: id.into(),
            max_bytes: 8192
        })
        .unwrap()
        .result["status"],
        "verified"
    );
    assert!(export::public(&s, &ctx, &out).is_err());
}
#[test]
fn packet_truncation_keeps_complete_evidence_and_endpoint_closure() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Store::init(d.path()).unwrap();
    let (mut b, raw) = fixture();
    for i in 0..40 {
        let mut n = b.nodes[1].clone();
        n.logical_key = format!("Large{i}");
        n.id = ids::id("node", &[&b.snapshot.id, n.kind.as_str(), &n.logical_key]);
        n.attributes = json!({"large":"x".repeat(1024)});
        let mut a = b.assertions[0].clone();
        a.object = n.id.clone();
        a.id = a.expected_id(&b.snapshot.id).unwrap();
        b.nodes.push(n);
        b.assertions.push(a);
    }
    s.ingest_bytes(&b, &raw).unwrap();
    let ctx = context(&mut s, &b, "public");
    let q = Query::new(&s, ctx);
    for budget in [4096, 8192, 16384] {
        let p = q
            .pack(ContextInput {
                seeds: vec![Seed {
                    kind: "node".into(),
                    value: b.nodes[0].id.clone(),
                }],
                max_bytes: budget,
                include_hypotheses: true,
            })
            .unwrap();
        assert!(serde_json::to_vec(&p).unwrap().len() <= budget);
        let ids = p
            .evidence
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        let nodes = p.result["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        for node in p.result["nodes"].as_array().unwrap() {
            for ev in node["evidence_ids"].as_array().unwrap() {
                assert!(ids.contains(ev.as_str().unwrap()));
            }
        }
        for edge in p.result["admitted_interpretations"].as_array().unwrap() {
            let a = &edge["assertion"];
            assert!(nodes.contains(a["subject"].as_str().unwrap()));
            assert!(nodes.contains(a["object"].as_str().unwrap()));
            for ev in a["evidence"].as_array().unwrap() {
                assert!(ids.contains(ev["id"].as_str().unwrap()));
            }
        }
    }
}
#[cfg(feature = "mcp")]
#[test]
fn mcp_same_service_and_rejected_escalation() {
    use mudcrab_knowledge_graph::mcp::GraphServer;
    let d = tempfile::tempdir().unwrap();
    let mut s = Store::init(d.path()).unwrap();
    let (b, raw) = fixture();
    s.ingest_bytes(&b, &raw).unwrap();
    let ctx = context(&mut s, &b, "public");
    let args = json!({"query":"Material"});
    let expected = Query::new(&s, ctx.clone())
        .call("kg.search", args.clone())
        .unwrap();
    let server = GraphServer::new(Store::open(d.path(), false).unwrap(), ctx);
    assert_eq!(
        serde_json::to_value(server.call("kg.search", args).unwrap()).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(GraphServer::tools().len(), 7);
    for (name, args) in [
        ("kg.sql", json!({"sql":"DELETE FROM nodes"})),
        (
            "kg.node",
            json!({"node_id":b.nodes[0].id,"context":"other"}),
        ),
        (
            "kg.evidence",
            json!({"evidence_id":b.evidence[0].id,"path":"/etc/passwd"}),
        ),
        ("kg.trace", json!({"mode":"node","seed":b.nodes[0].id})),
        (
            "kg.search",
            json!({"query":"Material","policy":"research-local"}),
        ),
    ] {
        assert!(server.call(name, args).is_err());
    }
    assert!(
        mudcrab_knowledge_graph::mcp::serve(d.path(), "test", "research-local", false).is_err()
    );
}

#[test]
fn streaming_fingerprint_matches_canonical_bundle() {
    let (mut b, _) = fixture();
    let streamed = bundle::fingerprint(&b).unwrap();
    b.artifacts.sort_by(|a, b| a.id.cmp(&b.id));
    b.evidence.sort_by(|a, b| a.id.cmp(&b.id));
    b.nodes.sort_by(|a, b| a.id.cmp(&b.id));
    b.assertions.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(
        streamed,
        ids::digest(&serde_json::to_value(&b).unwrap()).unwrap()
    );
    b.nodes.reverse();
    assert_eq!(streamed, bundle::fingerprint(&b).unwrap());
}

#[test]
fn research_claim_predicates_validate_and_query() {
    let source = Source {
        id: "test-corpus".into(),
        kind: "notes".into(),
        origin_uri: "notes:test-corpus".into(),
        visibility: Visibility::Public,
        license_expression: None,
        lineage_roots: vec!["test-corpus".into()],
        redistribution: "with_notice".into(),
    };
    let text = "{\"claims\": [{\"id\": \"T01\"},{\"id\": \"T02\"}]}";
    let mut b = ExtractionBuilder::new(
        source,
        "corpus-v1",
        &ids::sha256(text.as_bytes()),
        "test-corpus-export",
        json!({}),
        json!({"ledgers":["ledger.json"]}),
        vec![],
    )
    .unwrap();
    let a = b
        .artifact("ledger.json", "application/json", text.as_bytes().to_vec())
        .unwrap();
    let e = b.evidence(&a, json!({"kind":"source_span","path":"ledger.json","start_byte":0,"end_byte":text.len(),"start_line":1,"end_line":1,"encoding":"utf8_bytes"}), "literal", Provenance::Extracted).unwrap();
    let section = b.node(
        NodeKind::DocumentSection,
        "ledger.json",
        "ledger.json",
        json!({}),
        std::slice::from_ref(&e),
    );
    let c1 = b.node(NodeKind::ResearchClaim, "T01", "T01", json!({"note_id":"TN","domain":"native-lighting","assessment":"partially_supported","scope":"test"}), std::slice::from_ref(&e));
    let c2 = b.node(NodeKind::ResearchClaim, "T02", "T02", json!({"note_id":"TN","domain":"native-lighting","assessment":"supported_static","scope":"test"}), std::slice::from_ref(&e));
    let k1 = b.node(
        NodeKind::Concept,
        "topic-a",
        "topic a",
        json!({"domain_layer":"test"}),
        std::slice::from_ref(&e),
    );
    let k2 = b.node(
        NodeKind::Concept,
        "topic-b",
        "topic b",
        json!({"domain_layer":"test"}),
        std::slice::from_ref(&e),
    );
    b.edge(
        &section,
        Predicate::Documents,
        &c1,
        json!({}),
        std::slice::from_ref(&e),
        Provenance::Extracted,
    )
    .unwrap();
    b.edge(
        &c1,
        Predicate::Supports,
        &c2,
        json!({}),
        std::slice::from_ref(&e),
        Provenance::Manual,
    )
    .unwrap();
    b.edge(
        &c1,
        Predicate::DescribesConcept,
        &k1,
        json!({"basis":"test association"}),
        std::slice::from_ref(&e),
        Provenance::Manual,
    )
    .unwrap();
    b.edge(
        &c1,
        Predicate::CorrespondsTo,
        &k1,
        json!({"traced_path":"test.rs"}),
        std::slice::from_ref(&e),
        Provenance::Manual,
    )
    .unwrap();
    b.edge(
        &k1,
        Predicate::DependsOn,
        &k2,
        json!({"label":"test dependency"}),
        std::slice::from_ref(&e),
        Provenance::Manual,
    )
    .unwrap();
    let (bundle, bytes) = (b.bundle, b.bytes);
    bundle::validate(&bundle, &Default::default(), &bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::init(dir.path()).unwrap();
    s.ingest_bytes(&bundle, &bytes).unwrap();
    for a in bundle.assertions.iter().skip(1) {
        s.review(
            &a.id,
            "test-receipt",
            "admitted",
            "curated fixture judgment",
        )
        .unwrap();
    }
    let ctx = context(&mut s, &bundle, "public");
    let q = Query::new(&s, ctx);
    let n = q
        .neighbors(NeighborsInput {
            node_id: c1.clone(),
            direction: "out".into(),
            predicates: vec![],
            limit: 20,
            cursor: None,
        })
        .unwrap();
    let preds: Vec<String> = n.result["assertions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["assertion"]["predicate"].as_str().unwrap().to_owned())
        .collect();
    assert!(preds.contains(&"SUPPORTS".to_owned()));
    assert!(preds.contains(&"DESCRIBES_CONCEPT".to_owned()));
    assert!(preds.contains(&"CORRESPONDS_TO".to_owned()));
    // Domain violations must fail validation with corrected identities.
    let mut bad = bundle.clone();
    bad.assertions[1].subject = k1.clone();
    bad.assertions[1].id = bad.assertions[1].expected_id(&bad.snapshot.id).unwrap();
    assert!(bundle::validate(&bad, &Default::default(), &bytes).is_err());
    let mut bad = bundle.clone();
    bad.assertions[4].subject = c1.clone();
    bad.assertions[4].id = bad.assertions[4].expected_id(&bad.snapshot.id).unwrap();
    assert!(bundle::validate(&bad, &Default::default(), &bytes).is_err());
}
