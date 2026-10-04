//! Bounded producer observations; only matched module code addresses become RVAs.
use crate::{
    error::{Error, Result},
    extract::ExtractionBuilder,
    ids,
    model::*,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Producer {
    pub name: String,
    pub version: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub sha256: String,
    pub load_base: String,
    pub image_size: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedIdentity {
    pub kind: String,
    pub va: String,
    #[serde(default)]
    pub module_sha256: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub record_id: String,
    pub kind: String,
    pub payload_sha256: String,
    pub payload: Value,
    pub identity: ObservedIdentity,
    pub time: String,
    #[serde(default)]
    pub callees: Vec<ObservedIdentity>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationInput {
    pub schema_version: u32,
    pub session_id: String,
    pub producer: Producer,
    pub time_domain: String,
    pub modules: Vec<Module>,
    pub observations: Vec<Observation>,
    #[serde(default)]
    pub synthetic: bool,
}
fn invalid(message: &str) -> Error {
    Error::new("INVALID_INPUT", message)
}
fn hex(s: &str) -> Result<u64> {
    if !s.starts_with("0x") {
        return Err(invalid("canonical hex address required"));
    }
    let n = u64::from_str_radix(&s[2..], 16).map_err(|_| invalid("invalid address"))?;
    if format!("{n:#x}") != s {
        return Err(invalid("canonical hex address required"));
    }
    Ok(n)
}
fn hash(s: &str) -> Result<()> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        Err(invalid("SHA-256 required"))
    } else {
        Ok(())
    }
}
/// Validate runtime address normalization without treating heap pointers as code.
pub fn normalize_identity(i: &ObservedIdentity, modules: &[Module]) -> Result<Value> {
    let va = hex(&i.va)?;
    if i.kind == "heap" {
        if i.module_sha256.is_some() {
            return Err(invalid("heap pointers have no module RVA"));
        }
        return Ok(json!({"kind":"heap","va":i.va,"normalization":"session_scoped_only"}));
    }
    if i.kind != "code" {
        return Err(invalid("identity kind must be code or heap"));
    }
    let h = i
        .module_sha256
        .as_deref()
        .ok_or_else(|| invalid("code address requires module hash"))?;
    hash(h)?;
    let found = modules.iter().filter(|m| m.sha256 == h).collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(invalid("code address module unavailable or ambiguous"));
    }
    let base = hex(&found[0].load_base)?;
    let size = hex(&found[0].image_size)?;
    if size == 0 || va < base || va - base >= size {
        return Err(invalid("code address outside matched loaded module"));
    }
    Ok(
        json!({"kind":"code","va":i.va,"module_sha256":h,"load_base":found[0].load_base,"rva":format!("{:#x}",va-base),"space":"ram"}),
    )
}
/// Import inert, bounded producer payloads. No debugger attachment or tool execution.
pub fn index(raw: &[u8], source_id: &str) -> Result<ExtractionBuilder> {
    if raw.len() > 8 * 1024 * 1024 {
        return Err(invalid("observation input exceeds record limit"));
    }
    let value: Value = serde_json::from_slice(raw)?;
    ids::canonical(&value)?;
    let input: ObservationInput = serde_json::from_value(value.clone())?;
    if input.schema_version != 1
        || input.session_id.trim().is_empty()
        || input.producer.name.trim().is_empty()
        || input.producer.version.trim().is_empty()
        || input.time_domain.trim().is_empty()
        || input.modules.len() > 32
        || input.observations.is_empty()
        || input.observations.len() > 10000
    {
        return Err(invalid("invalid bounded observation envelope"));
    }
    let mut keys = BTreeSet::new();
    for m in &input.modules {
        hash(&m.sha256)?;
        hex(&m.load_base)?;
        hex(&m.image_size)?;
        if !keys.insert(m.sha256.clone()) {
            return Err(invalid("duplicate module identity"));
        }
    }
    keys.clear();
    let visibility = if input.synthetic {
        Visibility::Public
    } else {
        Visibility::Restricted
    };
    let source = Source {
        id: source_id.into(),
        kind: if input.synthetic {
            "synthetic"
        } else {
            "notes"
        }
        .into(),
        origin_uri: format!("observations:{source_id}"),
        visibility,
        license_expression: input.synthetic.then(|| "MIT OR Apache-2.0".into()),
        lineage_roots: vec![source_id.into()],
        redistribution: if input.synthetic {
            "with_notice"
        } else {
            "review_required"
        }
        .into(),
    };
    let mut b = ExtractionBuilder::new(
        source,
        &input.session_id,
        &ids::digest(&value)?,
        "runtime-observations",
        json!({"adapter":"runtime-v1","producer":input.producer}),
        json!({"session_id":input.session_id,"time_domain":input.time_domain,"records":input.observations.len()}),
        vec![],
    )?;
    let art = b.artifact(
        "observations.json",
        "application/json",
        ids::canonical(&value)?,
    )?;
    for o in &input.observations {
        if o.record_id.is_empty()
            || !keys.insert(o.record_id.clone())
            || o.kind.is_empty()
            || o.time.is_empty()
            || o.callees.len() > 32
        {
            return Err(invalid("invalid observation record"));
        }
        hash(&o.payload_sha256)?;
        if ids::digest(&o.payload)? != o.payload_sha256 {
            return Err(Error::new("INTEGRITY", "observation payload hash mismatch"));
        }
        let e = b.evidence(
            &art,
            json!({"kind":"observation","session_id":input.session_id,"record_id":o.record_id}),
            "producer-declared runtime observation",
            Provenance::Observed,
        )?;
        let identity = normalize_identity(&o.identity, &input.modules)?;
        let observation=b.node(NodeKind::RuntimeObservation,&format!("{}:{}:{}",input.session_id,input.producer.name,o.record_id),&o.kind,json!({"session_id":input.session_id,"producer":input.producer,"time_domain":input.time_domain,"time":o.time,"identity":identity,"payload_sha256":o.payload_sha256,"payload":o.payload,"stage":"runtime"}),std::slice::from_ref(&e));
        if o.identity.kind == "code" {
            let code = code_node(&mut b, &identity, &e);
            b.edge(
                &code,
                Predicate::ObservedAt,
                &observation,
                json!({}),
                std::slice::from_ref(&e),
                Provenance::Observed,
            )?;
            for callee in &o.callees {
                let target = normalize_identity(callee, &input.modules)?;
                if callee.kind != "code" {
                    return Err(invalid("observed call target must be module code"));
                }
                let target = code_node(&mut b, &target, &e);
                b.edge(&code,Predicate::ObservedCall,&target,json!({"session_id":input.session_id,"record_id":o.record_id,"time":o.time,"not_complete_control_flow":true}),std::slice::from_ref(&e),Provenance::Observed)?;
            }
        } else if !o.callees.is_empty() {
            return Err(invalid("heap observation cannot assert code calls"));
        }
    }
    b.capability("runtime_observations","complete","Declared producer records only; heap identities remain session-scoped; matched loaded-module code normalized with range checks",input.observations.len()as u64,0,0);
    Ok(b)
}
fn code_node(b: &mut ExtractionBuilder, identity: &Value, e: &str) -> String {
    let key = format!(
        "{}:ram:{}",
        identity["module_sha256"].as_str().unwrap_or(""),
        identity["rva"].as_str().unwrap_or("")
    );
    b.node(NodeKind::BinaryFunction,&key,&format!("observed code {}",identity["rva"].as_str().unwrap_or("")),json!({"module_sha256":identity["module_sha256"],"rva":identity["rva"],"identity_basis":"observed code address; function boundary unresolved"}),&[e.into()])
}
