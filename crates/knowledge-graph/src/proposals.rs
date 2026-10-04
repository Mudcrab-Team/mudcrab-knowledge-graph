//! Explicit notes/proposal import. Reasoning remains hypothesis/proposed until operator review.
use crate::{
    error::{Error, Result},
    extract::ExtractionBuilder,
    ids,
    model::*,
    query::{EvidenceInput, NodeInput, Query},
    store::{Context, Store},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Producer {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub proposal_key: String,
    pub subject: String,
    pub predicate: Predicate,
    pub object: String,
    pub explanation: String,
    pub supporting_evidence_ids: Vec<String>,
    pub missing_evidence: Vec<String>,
    pub producer: Producer,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalInput {
    pub schema_version: u32,
    pub context_id: String,
    pub proposals: Vec<Proposal>,
}
/// Resolve only full pinned IDs. Input never launches suggested commands or resolves names.
pub fn index(store: &Store, raw: &[u8]) -> Result<ExtractionBuilder> {
    if raw.len() > 8 * 1024 * 1024 {
        return Err(Error::new("INVALID_INPUT", "proposal file too large"));
    }
    let value: Value = serde_json::from_slice(raw)?;
    ids::canonical(&value)?;
    let mut input: ProposalInput = serde_json::from_value(value)?;
    crate::query::validate_id(&input.context_id, "ctx")?;
    if input.schema_version != 1 || input.proposals.is_empty() || input.proposals.len() > 256 {
        return Err(Error::new("INVALID_INPUT", "select 1–256 proposals"));
    }
    let ctx = store.context(&input.context_id, None)?;
    let q = Query::new(store, ctx.clone());
    let mut floor = Visibility::Public;
    let mut keys = BTreeSet::new();
    input
        .proposals
        .sort_by(|a, b| a.proposal_key.cmp(&b.proposal_key));
    for p in &mut input.proposals {
        if p.proposal_key.trim().is_empty()
            || !keys.insert(p.proposal_key.clone())
            || p.explanation.trim().is_empty()
            || p.explanation.len() > 65536
            || p.producer.name.trim().is_empty()
            || p.producer.kind.trim().is_empty()
            || p.supporting_evidence_ids.is_empty()
            || p.supporting_evidence_ids.len() > 32
            || p.missing_evidence.len() > 32
        {
            return Err(Error::new("INVALID_INPUT", "invalid bounded proposal"));
        }
        p.supporting_evidence_ids.sort();
        if p.supporting_evidence_ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(Error::new("INVALID_INPUT", "duplicate proposal evidence"));
        }
        for node in [&p.subject, &p.object] {
            q.node(NodeInput {
                node_id: node.clone(),
            })?;
            let n: Node = store
                .record(node)?
                .ok_or_else(|| Error::new("UNAVAILABLE", "proposal endpoint unavailable"))?;
            floor = floor.max(n.visibility);
        }
        for id in &p.supporting_evidence_ids {
            q.evidence(EvidenceInput {
                evidence_id: id.clone(),
                max_bytes: 65536,
            })?;
            let ev: Evidence = store
                .record(id)?
                .ok_or_else(|| Error::new("UNAVAILABLE", "proposal evidence unavailable"))?;
            floor = floor.max(ev.visibility);
        }
    }
    let normalized = serde_json::to_value(&input)?;
    let hash = ids::digest(&normalized)?;
    let source_id = format!("proposals-{}", floor.as_str());
    let source = Source {
        id: source_id.clone(),
        kind: "notes".into(),
        origin_uri: "operator:proposals".into(),
        visibility: floor,
        license_expression: None,
        lineage_roots: vec![source_id],
        redistribution: if floor == Visibility::Restricted {
            "review_required"
        } else {
            "with_notice"
        }
        .into(),
    };
    let mut b = ExtractionBuilder::new(
        source,
        &hash,
        &hash,
        "proposal-import",
        json!({"adapter":"proposals-v1","context_id":input.context_id}),
        json!({"proposal_count":input.proposals.len()}),
        ctx.snapshots,
    )?;
    let art = b.artifact(
        "proposals.json",
        "application/json",
        ids::canonical(&normalized)?,
    )?;
    for (index, p) in input.proposals.iter().enumerate() {
        let note = b.evidence(
            &art,
            json!({"kind":"json_pointer","pointer":format!("/proposals/{index}")}),
            "producer proposal note; not original behavior proof",
            Provenance::Hypothesis,
        )?;
        let mut ev = p.supporting_evidence_ids.clone();
        ev.push(note);
        b.edge(&p.subject,p.predicate,&p.object,json!({"proposal_key":p.proposal_key,"explanation":p.explanation,"missing_evidence":p.missing_evidence,"producer":p.producer}),&ev,Provenance::Hypothesis)?;
    }
    b.capability(
        "proposals",
        "complete",
        "Validated pinned endpoints; all new assertions hypothesis/proposed; no commands executed",
        input.proposals.len() as u64,
        0,
        0,
    );
    Ok(b)
}
/// Explicit operator conflicts between version/context-scoped claims.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictInput {
    pub assertion_a: String,
    pub assertion_b: String,
    pub evidence_id: String,
    pub reason: String,
}
/// Record a conflict after enforcing the same policy/evidence closure as a query.
pub fn conflict(store: &mut Store, context: Context, input: ConflictInput) -> Result<Value> {
    if input.reason.trim().is_empty()
        || input.reason.len() > 65536
        || input.assertion_a == input.assertion_b
    {
        return Err(Error::new(
            "INVALID_INPUT",
            "conflict needs two different claims and a reason",
        ));
    }
    let q = Query::new(store, context.clone());
    for id in [&input.assertion_a, &input.assertion_b] {
        crate::query::validate_id(id, "as")?;
        let a: Assertion = store
            .record(id)?
            .ok_or_else(|| Error::new("UNAVAILABLE", "conflict claim unavailable"))?;
        let owner: String = store.db.query_row(
            "SELECT snapshot_id FROM assertions WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if !context.snapshots.contains(&owner) || a.visibility > context.ceiling {
            return Err(Error::new("UNAVAILABLE", "conflict claim unavailable"));
        }
        q.node(NodeInput { node_id: a.subject })?;
        q.node(NodeInput { node_id: a.object })?;
        for e in a.evidence {
            q.evidence(EvidenceInput {
                evidence_id: e.id,
                max_bytes: 65536,
            })?;
        }
    }
    q.evidence(EvidenceInput {
        evidence_id: input.evidence_id.clone(),
        max_bytes: 65536,
    })?;
    let mut pair = [input.assertion_a, input.assertion_b];
    pair.sort();
    let id = ids::id(
        "conflict",
        &[
            &context.id,
            &pair[0],
            &pair[1],
            &input.evidence_id,
            &input.reason,
        ],
    );
    let tx = store
        .db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT OR IGNORE INTO conflicts VALUES(?1,?2,?3,?4,?5,?6)",
        rusqlite::params![
            id,
            pair[0],
            pair[1],
            input.evidence_id,
            json!({"context_id":context.id}).to_string(),
            input.reason
        ],
    )?;
    if tx.changes() > 0 {
        tx.execute("UPDATE generation SET value=value+1", [])?;
    }
    tx.commit()?;
    Ok(json!({"conflict_id":id}))
}
