use mudcrab_knowledge_graph::{bundle, ids, model::*, observations, store::Store};
use serde_json::json;
#[test]
fn loaded_module_code_and_session_scoped_heap() {
    let payload = json!({"flag":16});
    let m = "a".repeat(64);
    let mut input = json!({"schema_version":1,"session_id":"original-synthetic","synthetic":true,"producer":{"name":"test-debugger","version":"1"},"time_domain":"sequence","modules":[{"sha256":m,"load_base":"0x1000","image_size":"0x200"}],"observations":[{"record_id":"1","kind":"call","time":"1","payload":payload,"payload_sha256":ids::digest(&payload).unwrap(),"identity":{"kind":"code","va":"0x1010","module_sha256":m},"callees":[{"kind":"code","va":"0x1030","module_sha256":m}]},{"record_id":"2","kind":"object","time":"2","payload":payload,"payload_sha256":ids::digest(&payload).unwrap(),"identity":{"kind":"heap","va":"0x1040"}}]});
    let b = observations::index(&serde_json::to_vec(&input).unwrap(), "runtime-toy").unwrap();
    bundle::validate(&b.bundle, &Default::default(), &b.bytes).unwrap();
    assert!(
        b.bundle
            .assertions
            .iter()
            .any(|a| a.predicate == Predicate::ObservedCall)
    );
    assert!(
        !b.bundle
            .assertions
            .iter()
            .any(|a| a.predicate == Predicate::CallsDirect)
    );
    assert!(b.bundle.nodes.iter().any(|n| n.attributes["rva"] == "0x10"));
    let heap = b
        .bundle
        .nodes
        .iter()
        .find(|n| n.display_name == "object")
        .unwrap();
    assert!(heap.attributes["identity"].get("rva").is_none());
    let d = tempfile::tempdir().unwrap();
    Store::init(d.path())
        .unwrap()
        .ingest_bytes(&b.bundle, &b.bytes)
        .unwrap();
    input["observations"][0]["identity"]["va"] = json!("0x1300");
    assert!(observations::index(&serde_json::to_vec(&input).unwrap(), "runtime-toy").is_err());
    input["observations"][0]["identity"]["va"] = json!("0x1010");
    input["observations"][1]["identity"]["module_sha256"] = json!(m);
    assert!(observations::index(&serde_json::to_vec(&input).unwrap(), "runtime-toy").is_err());
}
