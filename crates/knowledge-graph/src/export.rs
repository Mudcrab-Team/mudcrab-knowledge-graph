//! Construct a fresh policy-filtered store. Never copy the working SQLite file or its WAL.
use crate::{
    error::{Error, Result},
    extract::ExtractionBuilder,
    ids,
    model::*,
    store::{Context, ContextManifest, Store},
};
use rusqlite::params;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
fn records<T: serde::de::DeserializeOwned>(
    store: &Store,
    snapshot: &str,
    category: &str,
) -> Result<Vec<T>> {
    let mut statement = store
        .db
        .prepare("SELECT json FROM records WHERE snapshot_id=?1 AND category=?2 ORDER BY id")?;
    let mut records = vec![];
    for row in statement.query_map(params![snapshot, category], |r| r.get::<_, String>(0))? {
        records.push(serde_json::from_str(&row?)?);
    }
    Ok(records)
}
fn order(
    store: &Store,
    id: &str,
    seen: &mut BTreeSet<String>,
    out: &mut Vec<String>,
) -> Result<()> {
    if !seen.insert(id.into()) {
        return Ok(());
    }
    let s: Snapshot = store
        .record(id)?
        .ok_or_else(|| Error::new("INTEGRITY", "export dependency unavailable"))?;
    for dep in s.dependencies {
        order(store, &dep, seen, out)?;
    }
    out.push(id.into());
    Ok(())
}
/// New destination, new filtered snapshots/IDs, permitted evidence closure and rebuilt FTS.
/// Original revisions and locator bytes stay pinned. No hidden counts or identifiers are copied.
pub fn public(store: &Store, context: &Context, out: &Path) -> Result<Value> {
    let _read = store.read_snapshot()?;
    if out.exists() || std::fs::symlink_metadata(out).is_ok() {
        return Err(Error::new(
            "POLICY",
            "public export requires a new destination",
        ));
    }
    let mut ordered = vec![];
    let mut seen = BTreeSet::new();
    for id in &context.snapshots {
        order(store, id, &mut seen, &mut ordered)?;
    }
    let mut builds = vec![];
    let mut mapped_snapshots = BTreeMap::new();
    let mut mapped_nodes = BTreeMap::new();
    let mut mapped_evidence = BTreeMap::new();
    let mut reviews = vec![];
    let mut notices = BTreeMap::new();
    for id in ordered {
        let s: Snapshot = store
            .record(&id)?
            .ok_or_else(|| Error::new("INTEGRITY", "export snapshot unavailable"))?;
        let source_json: String = store.db.query_row(
            "SELECT metadata_json FROM sources WHERE id=?1",
            [&s.source_id],
            |r| r.get(0),
        )?;
        let source: Source = serde_json::from_str(&source_json)?;
        if s.visibility != Visibility::Public
            || source.visibility != Visibility::Public
            || source.redistribution != "with_notice"
        {
            continue;
        }
        let artifacts = records::<Artifact>(store, &id, "artifact")?
            .into_iter()
            .filter(|a| a.visibility == Visibility::Public)
            .map(|a| (a.id.clone(), a))
            .collect::<BTreeMap<_, _>>();
        let evidence = records::<Evidence>(store, &id, "evidence")?
            .into_iter()
            .filter(|e| {
                e.visibility == Visibility::Public && artifacts.contains_key(&e.artifact_id)
            })
            .collect::<Vec<_>>();
        let ev_ids = evidence
            .iter()
            .map(|e| e.id.clone())
            .chain(mapped_evidence.keys().cloned())
            .collect::<BTreeSet<_>>();
        let nodes = records::<Node>(store, &id, "node")?
            .into_iter()
            .filter(|n| {
                n.visibility == Visibility::Public
                    && n.evidence_ids.iter().all(|id| ev_ids.contains(id))
            })
            .collect::<Vec<_>>();
        let node_ids = nodes
            .iter()
            .map(|n| n.id.clone())
            .chain(mapped_nodes.keys().cloned())
            .collect::<BTreeSet<_>>();
        let assertions = records::<Assertion>(store, &id, "assertion")?
            .into_iter()
            .filter(|a| {
                a.visibility == Visibility::Public
                    && node_ids.contains(&a.subject)
                    && node_ids.contains(&a.object)
                    && a.evidence.iter().all(|e| ev_ids.contains(&e.id))
            })
            .collect::<Vec<_>>();
        if nodes.is_empty() && assertions.is_empty() {
            continue;
        }
        // Copy only evidence/artifacts required by the visible node/assertion closure.
        let used_evs = nodes
            .iter()
            .flat_map(|n| n.evidence_ids.clone())
            .chain(
                assertions
                    .iter()
                    .flat_map(|a| a.evidence.iter().map(|e| e.id.clone())),
            )
            .collect::<BTreeSet<_>>();
        let evidence = evidence
            .into_iter()
            .filter(|e| used_evs.contains(&e.id))
            .collect::<Vec<_>>();
        let used_artifacts = evidence
            .iter()
            .map(|e| e.artifact_id.clone())
            .collect::<BTreeSet<_>>();
        let permitted_manifest = json!({"revision":s.revision,"nodes":nodes,"evidence":evidence,"assertions":assertions});
        let deps = s
            .dependencies
            .iter()
            .filter_map(|d| mapped_snapshots.get(d).cloned())
            .collect();
        let export_source = Source {
            id: format!("{}.public-export", source.id),
            kind: source.kind.clone(),
            origin_uri: source.origin_uri.clone(),
            visibility: Visibility::Public,
            license_expression: source.license_expression.clone(),
            lineage_roots: source.lineage_roots.clone(),
            redistribution: source.redistribution.clone(),
        };
        notices.insert(export_source.id.clone(),json!({"origin_uri":source.origin_uri,"license_expression":source.license_expression,"lineage_roots":source.lineage_roots,"redistribution":"with_notice"}));
        let mut b = ExtractionBuilder::new(
            export_source,
            &s.revision,
            &ids::digest(&permitted_manifest)?,
            "public-export",
            json!({"adapter":"public-export-v1","policy":"public","input_revision":s.revision}),
            json!({"mode":"filtered_evidence_closure","nodes":nodes.len(),"assertions":assertions.len()}),
            deps,
        )?;
        let mut art_map = BTreeMap::new();
        for (aid, a) in artifacts
            .iter()
            .filter(|(id, _)| used_artifacts.contains(*id))
        {
            let raw = store.bytes(a)?.ok_or_else(|| {
                Error::new("INTEGRITY", "permitted export evidence bytes unavailable")
            })?;
            let new = b.artifact(&a.path, &a.media_type, raw)?;
            art_map.insert(aid.clone(), new);
        }
        for e in evidence {
            let artifact = art_map
                .get(&e.artifact_id)
                .ok_or_else(|| Error::new("INTEGRITY", "export artifact closure incomplete"))?;
            let new = b.evidence(artifact, e.locator, &e.method, e.provenance_class)?;
            mapped_evidence.insert(e.id, new);
        }
        for n in nodes {
            let ev = n
                .evidence_ids
                .iter()
                .map(|id| {
                    mapped_evidence.get(id).cloned().ok_or_else(|| {
                        Error::new("INTEGRITY", "export node evidence closure incomplete")
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let new = b.node(n.kind, &n.logical_key, &n.display_name, n.attributes, &ev);
            mapped_nodes.insert(n.id, new);
        }
        for a in assertions {
            let ev = a
                .evidence
                .iter()
                .map(|e| {
                    Ok(EvidenceLink {
                        id: mapped_evidence.get(&e.id).cloned().ok_or_else(|| {
                            Error::new("INTEGRITY", "export assertion evidence closure incomplete")
                        })?,
                        role: e.role.clone(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let mut new = Assertion {
                id: String::new(),
                subject: mapped_nodes[&a.subject].clone(),
                predicate: a.predicate,
                object: mapped_nodes[&a.object].clone(),
                attributes: a.attributes,
                evidence: ev,
                provenance_class: a.provenance_class,
                initial_state: a.initial_state,
                visibility: Visibility::Public,
            };
            new.id = new.expected_id(&b.bundle.snapshot.id)?;
            let state: String = store.db.query_row(
                "SELECT current_state FROM assertion_states WHERE id=?1",
                [&a.id],
                |r| r.get(0),
            )?;
            if state != new.initial_state {
                reviews.push((new.id.clone(), state));
            }
            b.bundle.assertions.push(new);
        }
        b.capability("public_export","complete","Permitted closure only. Original extraction coverage is not exposed by this filtered export",b.bundle.nodes.len()as u64,0,0);
        mapped_snapshots.insert(s.id, b.bundle.snapshot.id.clone());
        builds.push(b);
    }
    // Stage under an adjacent operator-owned directory, then atomically rename to the new destination.
    let parent = out.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staging = parent.join(format!(
        ".kg-export-{}-{}",
        std::process::id(),
        ids::digest(&json!(mapped_snapshots))?
    ));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        let mut destination = Store::init(&staging)?;
        for b in &builds {
            destination.ingest_bytes(&b.bundle, &b.bytes)?;
        }
        for (id, state) in &reviews {
            destination.review(id,"public-export-state",state,"Current operator decision retained; original review attribution remains in local store")?;
        }
        let ctx = destination.create_context(&ContextManifest {
            schema_version: 1,
            name: "public".into(),
            snapshot_ids: mapped_snapshots.values().cloned().collect(),
            policy: "public".into(),
            notes: None,
        })?;
        std::fs::write(
            staging.join("NOTICE.json"),
            serde_json::to_vec_pretty(&notices)?,
        )?;
        let report = json!({"schema_version":1,"policy":"public","context_id":ctx.id,"snapshot_ids":ctx.snapshots,"nodes":mapped_nodes.len(),"evidence":mapped_evidence.len(),"format":"fresh_filtered_store","original_restricted_counts":"not_reported"});
        std::fs::write(
            staging.join("export.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        let checkpoint: (i64, i64, i64) =
            destination
                .db
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        if checkpoint.0 != 0 {
            return Err(Error::new("STORE", "fresh export checkpoint busy"));
        }
        let mode: String =
            destination
                .db
                .pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))?;
        if mode != "delete" {
            return Err(Error::new("STORE", "fresh export journal did not close"));
        }
        drop(destination);
        for name in ["graph.sqlite-wal", "graph.sqlite-shm"] {
            let path = staging.join(name);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        std::fs::rename(&staging, out)?;
        Ok(report)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}
