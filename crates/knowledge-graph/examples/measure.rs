//! Original synthetic degree-skew benchmark. Run build and query in separate fresh processes.
use mudcrab_knowledge_graph::{
    error::Result,
    extract::ExtractionBuilder,
    ids,
    model::*,
    query::*,
    store::{ContextManifest, Store},
};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};
fn build(root: &Path) -> Result<()> {
    if root.exists() {
        return Err(mudcrab_knowledge_graph::error::Error::new(
            "POLICY",
            "benchmark root must be new",
        ));
    }
    let mut s = Store::init(root)?;
    let source = Source {
        id: "performance-original".into(),
        kind: "synthetic".into(),
        origin_uri: "original:degree-skew".into(),
        visibility: Visibility::Public,
        license_expression: Some("MIT OR Apache-2.0".into()),
        lineage_roots: vec!["performance-original".into()],
        redistribution: "with_notice".into(),
    };
    let mut b = ExtractionBuilder::new(
        source,
        "100k-500k-v1",
        &ids::sha256(b"deterministic-degree-skew-v1"),
        "synthetic-performance",
        json!({"adapter":"degree-skew-v1"}),
        json!({"nodes":100000,"assertions":500000,"degree":"hub + five lexical edges per node"}),
        vec![],
    )?;
    let art = b.artifact(
        "workload.json",
        "application/json",
        b"{\"purpose\":\"original synthetic performance graph\"}".to_vec(),
    )?;
    let ev = b.evidence(
        &art,
        json!({"kind":"json_pointer","pointer":""}),
        "deterministic synthetic graph recipe",
        Provenance::Extracted,
    )?;
    for i in 0..100000 {
        let key = format!("probe::{i}");
        b.bundle.nodes.push(Node{id:ids::id("node",&[&b.bundle.snapshot.id,"CodeSymbol",&key]),kind:NodeKind::CodeSymbol,logical_key:key,display_name:format!("MaterialProbe{i}"),attributes:json!({"path":format!("synthetic/module{}.rs",i/100),"body":"material publication glossiness texture_slot","synthetic":true}),evidence_ids:vec![ev.clone()],visibility:Visibility::Public});
    }
    for i in 0..500000 {
        let from = i / 5;
        let to = if i % 5 == 0 {
            0
        } else {
            (from * 31 + (i % 5) * 101 + 1) % 100000
        };
        let mut a = Assertion {
            id: String::new(),
            subject: b.bundle.nodes[from].id.clone(),
            predicate: Predicate::References,
            object: b.bundle.nodes[to].id.clone(),
            attributes: json!({"synthetic_site":i}),
            evidence: vec![EvidenceLink {
                id: ev.clone(),
                role: "supports".into(),
            }],
            provenance_class: Provenance::Extracted,
            initial_state: "admitted".into(),
            visibility: Visibility::Public,
        };
        a.id = a.expected_id(&b.bundle.snapshot.id)?;
        b.bundle.assertions.push(a);
    }
    b.capability(
        "synthetic_performance",
        "complete",
        "100k nodes / 500k edges; degree-skew hub; no source or engine-behavior claims",
        100000,
        0,
        0,
    );
    let clock = Instant::now();
    s.ingest_bytes(&b.bundle, &b.bytes)?;
    s.create_context(&ContextManifest {
        schema_version: 1,
        name: "benchmark".into(),
        snapshot_ids: vec![b.bundle.snapshot.id.clone()],
        policy: "public".into(),
        notes: None,
    })?;
    std::fs::write(
        root.join("probe.json"),
        serde_json::to_vec(
            &json!({"node":b.bundle.nodes[75319].id,"hub":b.bundle.nodes[0].id,"import_ms":clock.elapsed().as_millis().to_string()}),
        )?,
    )?;
    println!(
        "{}",
        json!({"status":"built","nodes":100000,"assertions":500000,"import_ms":clock.elapsed().as_millis().to_string()})
    );
    Ok(())
}
fn p95(times: &mut [u128]) -> u128 {
    times.sort_unstable();
    times[(times.len() * 95).div_ceil(100) - 1]
}
fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let root = Path::new(args.get(2).expect("measure build|query ROOT"));
    if args[1] == "build" {
        return build(root);
    }
    let s = Store::open(root, false)?;
    let ctx = s.context("benchmark", None)?;
    let q = Query::new(&s, ctx);
    let probe: Value = serde_json::from_slice(&std::fs::read(root.join("probe.json"))?)?;
    let node = probe["node"].as_str().unwrap().to_owned();
    let mut report = json!({"build_profile":if cfg!(debug_assertions){"dev (workspace optimized dependencies)"}else{"release"},"conditions":"fresh query process; first access cold within process; repeated accesses warm; OS cache not flushed","iterations":20});
    for kind in ["lookup", "search", "trace", "packet"] {
        let mut times = vec![];
        let mut cold = 0;
        let mut bytes = 0;
        let mut truncated = false;
        for n in 0..21 {
            let clock = Instant::now();
            let e = match kind {
                "lookup" => q.node(NodeInput {
                    node_id: node.clone(),
                })?,
                "search" => q.search(SearchInput {
                    query: "material".into(),
                    kinds: vec![],
                    sources: vec![],
                    limit: 20,
                    cursor: None,
                })?,
                "trace" => q.trace(TraceInput {
                    mode: "node".into(),
                    seed: node.clone(),
                    include_hypotheses: false,
                    limit: 20,
                })?,
                _ => q.pack(ContextInput {
                    seeds: vec![Seed {
                        kind: "node".into(),
                        value: node.clone(),
                    }],
                    max_bytes: 65536,
                    include_hypotheses: true,
                })?,
            };
            let elapsed = clock.elapsed().as_micros();
            if n == 0 {
                cold = elapsed;
            } else {
                times.push(elapsed);
            }
            bytes = serde_json::to_vec(&e)?.len();
            truncated = e.truncated;
        }
        report[kind] = json!({"cold_us":cold.to_string(),"warm_p95_us":p95(&mut times).to_string(),"response_bytes":bytes,"truncated":truncated});
    }
    let raw = serde_json::to_vec_pretty(&report)?;
    std::fs::write(root.join("measurements.json"), &raw)?;
    println!("{}", String::from_utf8(raw).unwrap());
    Ok(())
}
