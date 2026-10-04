//! Transactional publication, pinned contexts, immutable records and local evidence bytes.
use crate::{
    bundle::{self, Closure},
    error::{Error, Result},
    ids,
    model::*,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
/// A pinned context plus explicit retrieval policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextManifest {
    pub schema_version: u32,
    pub name: String,
    pub snapshot_ids: Vec<String>,
    pub policy: String,
    #[serde(default)]
    pub notes: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Context {
    pub id: String,
    pub snapshots: Vec<String>,
    pub ceiling: Visibility,
    pub config: Value,
}
/// One connection; writes serialize with SQLite immediate transactions.
pub struct Store {
    pub(crate) db: Connection,
    pub root: PathBuf,
}
impl Store {
    /// Initialize an independent graph database, atomically applying migrations.
    pub fn init(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let path = root.join("graph.sqlite");
        reject_symlink(&path)?;
        let mut db = Connection::open(&path)?;
        configure(&db)?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 3 {
            return Err(Error::new("INVALID_SCHEMA", "newer database schema"));
        }
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if version == 0 {
            tx.execute_batch(include_str!("../migrations/0001.sql"))?;
        }
        if version < 2 {
            tx.execute_batch(include_str!("../migrations/0002.sql"))?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("../migrations/0003.sql"))?;
        }
        let ontology: Value = serde_json::from_str(include_str!(
            "../../../research/knowledge-graph/contracts/ontology.json"
        ))?;
        for p in ontology["relations"].as_array().expect("ontology") {
            tx.execute(
                "INSERT OR IGNORE INTO predicates VALUES(?1,?2)",
                params![p["name"].as_str(), serde_json::to_string(p)?],
            )?;
        }
        tx.commit()?;
        let mode: String = db.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
        if mode != "wal" {
            return Err(Error::new("STORE", "local filesystem did not enable WAL"));
        }
        Ok(Self {
            db,
            root: root.to_owned(),
        })
    }
    /// Open existing storage. Read-only queries do not initialize or migrate data.
    pub fn open(root: &Path, write: bool) -> Result<Self> {
        let path = root.join("graph.sqlite");
        reject_symlink(&path)?;
        if !path.is_file() {
            return Err(Error::new("MISSING_INPUT", "initialize graph first"));
        }
        let flags = if write {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let db =
            Connection::open_with_flags(path, flags | rusqlite::OpenFlags::SQLITE_OPEN_FULL_MUTEX)?;
        configure(&db)?;
        let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if v != 3 {
            return Err(Error::new(
                "INVALID_SCHEMA",
                "initialize migrations before querying",
            ));
        }
        Ok(Self {
            db,
            root: root.to_owned(),
        })
    }
    /// Pin all reads in one operation to the same SQLite generation. Nested query
    /// helpers reuse the outer transaction; dropping the guard releases the snapshot.
    pub(crate) fn read_snapshot(&self) -> Result<Option<rusqlite::Transaction<'_>>> {
        if self.db.is_autocommit() {
            Ok(Some(self.db.unchecked_transaction()?))
        } else {
            Ok(None)
        }
    }
    /// Return a published canonical wire record.
    pub fn record<T: serde::de::DeserializeOwned>(&self, id: &str) -> Result<Option<T>> {
        let data:Option<String>=self.db.query_row("SELECT r.json FROM records r JOIN snapshots s ON s.id=r.snapshot_id WHERE r.id=?1 AND s.state='published'",[id],|r|r.get(0)).optional()?;
        data.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    /// Resolve transitive dependencies without selecting mutable latest revisions.
    pub fn dependencies(&self, ids_in: &[String]) -> Result<Vec<String>> {
        let mut all = BTreeSet::new();
        let mut pending = ids_in.to_vec();
        while let Some(id) = pending.pop() {
            if all.len() > 10000 {
                return Err(Error::new(
                    "INVALID_INPUT",
                    "dependency closure exceeds limit",
                ));
            }
            if !all.insert(id.clone()) {
                continue;
            }
            let snapshot: Snapshot = self
                .record(&id)?
                .ok_or_else(|| Error::new("MISSING_INPUT", "unavailable snapshot dependency"))?;
            pending.extend(snapshot.dependencies);
        }
        Ok(all.into_iter().collect())
    }
    fn closure(&self, deps: &[String]) -> Result<Closure> {
        let mut closure = Closure::default();
        for snapshot in self.dependencies(deps)? {
            let mut stmt = self
                .db
                .prepare("SELECT category,json FROM records WHERE snapshot_id=?1")?;
            for row in stmt.query_map([snapshot], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (c, v) = row?;
                match c.as_str() {
                    "node" => {
                        let n: Node = serde_json::from_str(&v)?;
                        closure.nodes.insert(n.id.clone(), n);
                    }
                    "evidence" => {
                        let e: Evidence = serde_json::from_str(&v)?;
                        closure.evidence.insert(e.id.clone(), e);
                    }
                    "artifact" => {
                        let a: Artifact = serde_json::from_str(&v)?;
                        closure.artifacts.insert(a.id.clone(), a);
                    }
                    _ => {}
                }
            }
        }
        Ok(closure)
    }
    /// Load allowlisted artifact files and publish a validated bundle in one transaction.
    pub fn ingest(&mut self, b: &Bundle, artifact_root: Option<&Path>) -> Result<Value> {
        let mut bytes = BTreeMap::new();
        if let Some(root) = artifact_root {
            for a in &b.artifacts {
                let path = root.join(&a.path);
                if fs::symlink_metadata(&path).is_ok() {
                    let file = ids::resolve_file(root, &a.path)?;
                    if fs::metadata(&file)?.len() > bundle::IMPORT_LIMIT {
                        return Err(Error::new("INVALID_INPUT", "artifact too large"));
                    }
                    bytes.insert(a.id.clone(), fs::read(file)?);
                }
            }
        }
        self.ingest_bytes(b, &bytes)
    }
    /// Publish already verified extraction bytes without filesystem path selection.
    pub fn ingest_bytes(&mut self, b: &Bundle, bytes: &BTreeMap<String, Vec<u8>>) -> Result<Value> {
        let result = self.publish(b, bytes);
        if let Err(e) = &result {
            let report = json!({"code":e.code,"stage":"validation_or_publication"});
            self.db.execute(
                "INSERT INTO runs(snapshot_id,status,report_json) VALUES(NULL,'failed',?1)",
                [serde_json::to_string(&report)?],
            )?;
        }
        result
    }
    fn publish(&mut self, b: &Bundle, bytes: &BTreeMap<String, Vec<u8>>) -> Result<Value> {
        let closure = self.closure(&b.snapshot.dependencies)?;
        bundle::validate(b, &closure, bytes)?;
        let fingerprint = bundle::fingerprint(b)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior: Option<String> = tx
            .query_row(
                "SELECT bundle_digest FROM imports WHERE snapshot_id=?1",
                [&b.snapshot.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(prior) = prior {
            if prior != fingerprint {
                return Err(Error::new(
                    "IDENTITY_COLLISION",
                    "immutable snapshot content changed",
                ));
            }
            return Ok(json!({"snapshot_id":b.snapshot.id,"idempotent":true}));
        }
        let source_json = serde_json::to_string(&b.source)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT metadata_json FROM sources WHERE id=?1",
                [&b.source.id],
                |r| r.get(0),
            )
            .optional()?;
        if old.as_ref().is_some_and(|old| old != &source_json) {
            return Err(Error::new(
                "IDENTITY_COLLISION",
                "immutable source metadata changed",
            ));
        }
        tx.execute(
            "INSERT OR IGNORE INTO sources VALUES(?1,?2,?3,?4)",
            params![b.source.id, b.source.kind, b.source.origin_uri, source_json],
        )?;
        tx.execute(
            "INSERT INTO snapshots VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'staging')",
            params![
                b.snapshot.id,
                b.source.id,
                b.snapshot.revision,
                b.snapshot.content_sha256,
                serde_json::to_string(&b.snapshot.extraction)?,
                serde_json::to_string(&b.snapshot.scope)?,
                serde_json::to_string(&b.snapshot.capabilities)?,
                b.snapshot.visibility.as_str()
            ],
        )?;
        put(&tx, &b.snapshot.id, &b.snapshot.id, "snapshot", &b.snapshot)?;
        for dep in &b.snapshot.dependencies {
            tx.execute(
                "INSERT INTO snapshot_dependencies VALUES(?1,?2)",
                params![b.snapshot.id, dep],
            )?;
        }
        for a in &b.artifacts {
            tx.execute(
                "INSERT INTO artifacts VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    a.id,
                    b.snapshot.id,
                    a.path,
                    a.sha256,
                    a.media_type,
                    a.byte_size,
                    a.visibility.as_str(),
                    a.blob_sha256
                ],
            )?;
            put(&tx, &a.id, &b.snapshot.id, "artifact", a)?;
            if let Some(raw) = bytes.get(&a.id) {
                tx.execute(
                    "INSERT INTO artifact_bytes VALUES(?1,?2)",
                    params![a.id, raw],
                )?;
            }
        }
        for n in &b.nodes {
            tx.execute(
                "INSERT INTO nodes VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    n.id,
                    b.snapshot.id,
                    n.kind.as_str(),
                    n.logical_key,
                    n.display_name,
                    serde_json::to_string(&n.attributes)?,
                    n.visibility.as_str()
                ],
            )?;
            put(&tx, &n.id, &b.snapshot.id, "node", n)?;
            tx.execute(
                "INSERT INTO node_fts VALUES(?1,?2,?3)",
                params![n.id, n.display_name, search_body(n)],
            )?;
        }
        for e in &b.evidence {
            tx.execute(
                "INSERT INTO evidence VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    e.id,
                    b.snapshot.id,
                    e.artifact_id,
                    serde_json::to_string(&e.locator)?,
                    e.method,
                    e.provenance_class.as_str(),
                    e.visibility.as_str()
                ],
            )?;
            put(&tx, &e.id, &b.snapshot.id, "evidence", e)?;
        }
        for n in &b.nodes {
            for ev in &n.evidence_ids {
                tx.execute("INSERT INTO node_evidence VALUES(?1,?2)", params![n.id, ev])?;
            }
        }
        for a in &b.assertions {
            tx.execute(
                "INSERT INTO assertions VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    a.id,
                    b.snapshot.id,
                    a.subject,
                    a.predicate.as_str(),
                    a.object,
                    serde_json::to_string(&a.attributes)?,
                    a.provenance_class.as_str(),
                    a.initial_state,
                    a.visibility.as_str()
                ],
            )?;
            put(&tx, &a.id, &b.snapshot.id, "assertion", a)?;
            for e in &a.evidence {
                tx.execute(
                    "INSERT INTO assertion_evidence VALUES(?1,?2,?3)",
                    params![a.id, e.id, e.role],
                )?;
            }
        }
        tx.execute(
            "UPDATE snapshots SET state='published' WHERE id=?1",
            [&b.snapshot.id],
        )?;
        tx.execute(
            "INSERT INTO imports VALUES(?1,?2)",
            params![b.snapshot.id, fingerprint],
        )?;
        let report = json!({"snapshot_id":b.snapshot.id,"idempotent":false,"nodes":b.nodes.len(),"assertions":b.assertions.len()});
        tx.execute(
            "INSERT INTO runs(snapshot_id,status,report_json) VALUES(?1,'published',?2)",
            params![b.snapshot.id, serde_json::to_string(&report)?],
        )?;
        tx.execute("UPDATE generation SET value=value+1", [])?;
        tx.commit()?;
        Ok(report)
    }
    /// Store immutable dependency-complete context and update its explicit alias.
    pub fn create_context(&mut self, m: &ContextManifest) -> Result<Context> {
        if m.schema_version != 1
            || m.name.trim().is_empty()
            || m.snapshot_ids.iter().collect::<BTreeSet<_>>().len() != m.snapshot_ids.len()
        {
            return Err(Error::new("INVALID_INPUT", "invalid context manifest"));
        }
        let snapshots = self.dependencies(&m.snapshot_ids)?;
        let ceiling = profile(&m.policy)?;
        let config = json!({"profile":m.policy});
        let id = ids::id(
            "ctx",
            &[
                &ids::digest(&json!(snapshots))?,
                ceiling.as_str(),
                &ids::digest(&config)?,
            ],
        );
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO contexts VALUES(?1,?2,?3)",
            params![id, ceiling.as_str(), serde_json::to_string(&config)?],
        )?;
        for s in &snapshots {
            tx.execute(
                "INSERT OR IGNORE INTO context_snapshots VALUES(?1,?2)",
                params![id, s],
            )?;
        }
        tx.execute("INSERT INTO context_aliases VALUES(?1,?2) ON CONFLICT(alias) DO UPDATE SET context_id=excluded.context_id",params![m.name,id])?;
        tx.commit()?;
        Ok(Context {
            id,
            snapshots,
            ceiling,
            config,
        })
    }
    /// Resolve aliases once and optionally lower the policy; never raise it.
    pub fn context(&self, name: &str, lower_profile: Option<&str>) -> Result<Context> {
        let id:String=self.db.query_row("SELECT id FROM contexts WHERE id=?1 UNION ALL SELECT context_id FROM context_aliases WHERE alias=?1 LIMIT 1",[name],|r|r.get(0)).optional()?.ok_or_else(||Error::new("UNAVAILABLE","context unavailable"))?;
        let (policy, config): (String, String) = self.db.query_row(
            "SELECT policy,config_json FROM contexts WHERE id=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut config: Value = serde_json::from_str(&config)?;
        let mut ceiling = visibility(&policy)?;
        let mut stmt = self.db.prepare(
            "SELECT snapshot_id FROM context_snapshots WHERE context_id=?1 ORDER BY snapshot_id",
        )?;
        let snapshots = stmt
            .query_map([&id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        if let Some(p) = lower_profile {
            let lower = profile(p)?;
            if lower > ceiling {
                return Err(Error::new("POLICY", "server cannot raise context policy"));
            }
            ceiling = lower;
            config = json!({"profile":p});
        }
        let id = ids::id(
            "ctx",
            &[
                &ids::digest(&json!(snapshots))?,
                ceiling.as_str(),
                &ids::digest(&config)?,
            ],
        );
        Ok(Context {
            id,
            snapshots,
            ceiling,
            config,
        })
    }
    /// Append a review decision; provenance and immutable records do not change.
    pub fn review(
        &mut self,
        assertion: &str,
        actor: &str,
        decision: &str,
        reason: &str,
    ) -> Result<Value> {
        if actor.trim().is_empty()
            || reason.trim().is_empty()
            || !matches!(decision, "admitted" | "rejected" | "withdrawn" | "proposed")
        {
            return Err(Error::new(
                "INVALID_INPUT",
                "review requires actor, reason and valid decision",
            ));
        }
        let _: Assertion = self
            .record(assertion)?
            .ok_or_else(|| Error::new("UNAVAILABLE", "assertion unavailable"))?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO review_events(assertion_id,actor,reason,decision,reviewed_at,evidence_json) VALUES(?1,?2,?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now'),'[]')",params![assertion,actor,reason,decision])?;
        let seq = tx.last_insert_rowid();
        tx.execute("UPDATE generation SET value=value+1", [])?;
        tx.commit()?;
        Ok(json!({"assertion_id":assertion,"review_sequence":seq,"decision":decision}))
    }
    /// Verify stored bytes on every evidence read.
    pub fn bytes(&self, artifact: &Artifact) -> Result<Option<Vec<u8>>> {
        let raw: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT bytes FROM artifact_bytes WHERE artifact_id=?1",
                [&artifact.id],
                |r| r.get(0),
            )
            .optional()?;
        if raw.as_ref().is_some_and(|v| {
            v.len() as u64 != artifact.byte_size || ids::sha256(v) != artifact.sha256
        }) {
            return Err(Error::new("INTEGRITY", "stored evidence hash mismatch"));
        }
        Ok(raw)
    }
    /// Generation changes whenever publication or review can invalidate pagination.
    pub fn generation(&self) -> Result<i64> {
        Ok(self
            .db
            .query_row("SELECT value FROM generation", [], |r| r.get(0))?)
    }
}
pub(crate) fn put<T: Serialize>(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    snapshot: &str,
    category: &str,
    record: &T,
) -> Result<()> {
    tx.execute(
        "INSERT INTO records VALUES(?1,?2,?3,?4)",
        params![id, snapshot, category, serde_json::to_string(record)?],
    )?;
    Ok(())
}
fn configure(db: &Connection) -> Result<()> {
    db.pragma_update(None, "foreign_keys", true)?;
    db.busy_timeout(Duration::from_secs(5))?;
    Ok(())
}
fn reject_symlink(p: &Path) -> Result<()> {
    if fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(Error::new("POLICY", "symlink database forbidden"));
    }
    Ok(())
}
/// Map an operator-facing profile to its visibility ceiling.
pub fn profile(p: &str) -> Result<Visibility> {
    match p {
        "public" => Ok(Visibility::Public),
        "implementation" => Ok(Visibility::Reviewed),
        "research-local" => Ok(Visibility::Restricted),
        _ => Err(Error::new("INVALID_INPUT", "unknown policy profile")),
    }
}
pub(crate) fn visibility(p: &str) -> Result<Visibility> {
    match p {
        "public" => Ok(Visibility::Public),
        "reviewed" => Ok(Visibility::Reviewed),
        "restricted" => Ok(Visibility::Restricted),
        _ => Err(Error::new("INTEGRITY", "invalid stored visibility")),
    }
}
fn search_body(n: &Node) -> String {
    let mut s = n.logical_key.clone();
    s.push(' ');
    s.push_str(&n.attributes.to_string());
    s.push(' ');
    let mut prev_lower = false;
    for c in n.display_name.chars() {
        if c == '_' || c == ':' {
            s.push(' ');
        } else {
            if c.is_uppercase() && prev_lower {
                s.push(' ');
            }
            s.push(c);
        }
        prev_lower = c.is_lowercase();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        extract::ExtractionBuilder,
        query::{NeighborsInput, Query},
    };

    #[test]
    fn read_snapshot_keeps_review_and_generation_consistent() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut writer = Store::init(root.path())?;
        let source = Source {
            id: "consistent-read".into(),
            kind: "synthetic".into(),
            origin_uri: "original:consistent-read".into(),
            visibility: Visibility::Public,
            license_expression: Some("MIT".into()),
            lineage_roots: vec!["consistent-read".into()],
            redistribution: "with_notice".into(),
        };
        let mut b = ExtractionBuilder::new(
            source,
            "v1",
            &ids::sha256(b"{}"),
            "test",
            json!({}),
            json!({}),
            vec![],
        )?;
        let art = b.artifact("recipe.json", "application/json", b"{}".to_vec())?;
        let ev = b.evidence(
            &art,
            json!({"kind":"json_pointer","pointer":""}),
            "original test recipe",
            Provenance::Extracted,
        )?;
        let one = b.node(
            NodeKind::CodeSymbol,
            "one",
            "one",
            json!({}),
            std::slice::from_ref(&ev),
        );
        let two = b.node(
            NodeKind::CodeSymbol,
            "two",
            "two",
            json!({}),
            std::slice::from_ref(&ev),
        );
        b.edge(
            &one,
            Predicate::References,
            &two,
            json!({}),
            &[ev],
            Provenance::Extracted,
        )?;
        writer.ingest_bytes(&b.bundle, &b.bytes)?;
        let ctx = writer.create_context(&ContextManifest {
            schema_version: 1,
            name: "test".into(),
            snapshot_ids: vec![b.bundle.snapshot.id.clone()],
            policy: "public".into(),
            notes: None,
        })?;
        let reader = Store::open(root.path(), false)?;
        let query = Query::new(&reader, ctx);
        let input = NeighborsInput {
            node_id: one,
            direction: "out".into(),
            predicates: vec![],
            limit: 20,
            cursor: None,
        };
        let read = reader.read_snapshot()?;
        let generation = reader.generation()?; // Establish the WAL read snapshot.
        writer.review(
            &b.bundle.assertions[0].id,
            "operator",
            "rejected",
            "changed evidence",
        )?;
        let pinned = query.neighbors(input.clone())?;
        assert_eq!(pinned.result["assertions"].as_array().unwrap().len(), 1);
        assert_eq!(pinned.result["store_generation"], generation);
        drop(read);
        let current = query.neighbors(input)?;
        assert!(current.result["assertions"].as_array().unwrap().is_empty());
        assert!(current.result["store_generation"].as_i64().unwrap() > generation);
        Ok(())
    }
}
