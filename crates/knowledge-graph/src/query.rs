//! Shared bounded, policy-filtered query service for CLI and MCP.
use crate::{
    error::{Error, Result},
    ids,
    model::*,
    store::{Context, Store},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::{Duration, Instant},
};
fn twenty() -> u32 {
    20
}
fn evidence_budget() -> usize {
    8192
}
fn packet_budget() -> usize {
    65536
}
fn out() -> String {
    "out".into()
}
fn yes() -> bool {
    true
}
macro_rules! input {($name:ident {$($field:tt)*})=>{#[derive(Debug,Clone,Serialize,Deserialize)] #[serde(deny_unknown_fields)] #[cfg_attr(feature="mcp",derive(schemars::JsonSchema))] pub struct $name {$($field)*}};}
input!(SearchInput {pub query:String,#[serde(default)]pub kinds:Vec<NodeKind>,#[serde(default)]pub sources:Vec<String>,#[serde(default="twenty")]pub limit:u32,#[serde(default)]pub cursor:Option<String>});
input!(NodeInput {pub node_id:String});
input!(NeighborsInput {pub node_id:String,#[serde(default="out")]pub direction:String,#[serde(default)]pub predicates:Vec<Predicate>,#[serde(default="twenty")]pub limit:u32,#[serde(default)]pub cursor:Option<String>});
input!(TraceInput {pub mode:String,pub seed:String,#[serde(default)]pub include_hypotheses:bool,#[serde(default="twenty")]pub limit:u32});
input!(EvidenceInput {pub evidence_id:String,#[serde(default="evidence_budget")]pub max_bytes:usize});
input!(Seed {pub kind:String,pub value:String});
input!(ContextInput {pub seeds:Vec<Seed>,#[serde(default="packet_budget")]pub max_bytes:usize,#[serde(default="yes")]pub include_hypotheses:bool});
input!(CoverageInput {#[serde(default)]pub sources:Vec<String>,#[serde(default)]pub capabilities:Vec<String>});
/// Exact wire response, including complete-response truncation information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct Envelope {
    pub schema_version: u32,
    pub context_id: String,
    pub policy: String,
    pub result: Value,
    pub evidence: Vec<Value>,
    pub coverage: Vec<Value>,
    pub warnings: Vec<Value>,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
    pub next_cursor: Option<String>,
}
/// Fixed immutable context and maximum policy for the lifetime of a query service.
pub struct Query<'a> {
    pub store: &'a Store,
    pub context: Context,
}
const VISIBLE: &str = "WITH allowed_snapshots AS (SELECT s.id FROM snapshots s JOIN sources src ON src.id=s.source_id WHERE s.state='published' AND s.id IN (SELECT value FROM json_each(?1)) AND s.visibility IN (SELECT value FROM json_each(?2)) AND (json_extract(src.metadata_json,'$.redistribution')='with_notice' OR ?3='research-local')), visible_nodes AS (SELECT n.* FROM nodes n WHERE n.snapshot_id IN (SELECT id FROM allowed_snapshots) AND n.visibility IN (SELECT value FROM json_each(?2))), visible_evidence AS (SELECT e.* FROM evidence e JOIN allowed_snapshots s ON s.id=e.snapshot_id JOIN artifacts a ON a.id=e.artifact_id WHERE e.visibility IN (SELECT value FROM json_each(?2)) AND a.visibility IN (SELECT value FROM json_each(?2))) ";
impl<'a> Query<'a> {
    /// Bind a context once; aliases cannot switch underneath a running server.
    pub fn new(store: &'a Store, context: Context) -> Self {
        Self { store, context }
    }
    fn args(&self) -> Result<(String, String, String)> {
        let floors = match self.context.ceiling {
            Visibility::Public => vec!["public"],
            Visibility::Reviewed => vec!["public", "reviewed"],
            Visibility::Restricted => vec!["public", "reviewed", "restricted"],
        };
        Ok((
            serde_json::to_string(&self.context.snapshots)?,
            serde_json::to_string(&floors)?,
            self.context.config["profile"]
                .as_str()
                .unwrap_or("public")
                .into(),
        ))
    }
    fn envelope(&self, result: Value) -> Envelope {
        Envelope {
            schema_version: 1,
            context_id: self.context.id.clone(),
            policy: self.context.config["profile"]
                .as_str()
                .unwrap_or("public")
                .into(),
            result,
            evidence: vec![],
            coverage: vec![],
            warnings: vec![],
            truncated: false,
            truncation_reason: None,
            next_cursor: None,
        }
    }
    /// All entry points reject unknown fields, invalid enums and caller-selected context.
    pub fn call(&self, tool: &str, args: Value) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        match tool {
            "kg.search" => self.search(serde_json::from_value(args)?),
            "kg.node" => self.node(serde_json::from_value(args)?),
            "kg.neighbors" => self.neighbors(serde_json::from_value(args)?),
            "kg.trace" => {
                let input: TraceInput = serde_json::from_value(args)?;
                if !matches!(input.mode.as_str(), "material" | "concept") {
                    return Err(Error::new(
                        "INVALID_INPUT",
                        "trace mode must be material or concept",
                    ));
                }
                self.trace(input)
            }
            "kg.evidence" => self.evidence(serde_json::from_value(args)?),
            "kg.context" => self.pack(serde_json::from_value(args)?),
            "kg.coverage" => self.coverage(serde_json::from_value(args)?),
            _ => Err(Error::new("UNKNOWN_TOOL", "unknown read-only graph tool")),
        }
    }
    fn visible_node(&self, id: &str) -> Result<Option<Node>> {
        let (a, b, c) = self.args()?;
        let sql = format!(
            "{VISIBLE} SELECT r.json FROM visible_nodes n JOIN records r ON r.id=n.id WHERE n.id=?4"
        );
        let s: Option<String> = self
            .store
            .db
            .query_row(&sql, params![a, b, c, id], |r| r.get(0))
            .optional()?;
        s.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    fn visible_evidence(&self, id: &str) -> Result<Option<Evidence>> {
        let (a, b, c) = self.args()?;
        let sql = format!(
            "{VISIBLE} SELECT r.json FROM visible_evidence e JOIN records r ON r.id=e.id WHERE e.id=?4"
        );
        let s: Option<String> = self
            .store
            .db
            .query_row(&sql, params![a, b, c, id], |r| r.get(0))
            .optional()?;
        s.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    fn unavailable() -> Error {
        Error::new(
            "UNAVAILABLE",
            "object unavailable in this context and policy",
        )
    }
    fn citations(&self, ids: &[String]) -> Result<Vec<Value>> {
        let mut out = vec![];
        for id in ids.iter().collect::<BTreeSet<_>>() {
            if let Some(e) = self.visible_evidence(id)? {
                let snapshot: String = self.store.db.query_row(
                    "SELECT snapshot_id FROM evidence WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )?;
                let s: Snapshot = self
                    .store
                    .record(&snapshot)?
                    .ok_or_else(Self::unavailable)?;
                out.push(json!({"id":e.id,"artifact_id":e.artifact_id,"locator":e.locator,"method":e.method,"provenance_class":e.provenance_class,"snapshot_id":snapshot,"source_id":s.source_id,"revision":s.revision}));
            }
        }
        Ok(out)
    }
    /// Literal FTS search with deterministic rank/ID order and validated generation cursors.
    pub fn search(&self, i: SearchInput) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        limit(i.limit)?;
        list(&i.kinds)?;
        list(&i.sources)?;
        if i.query.is_empty() || i.query.len() > 4096 {
            return Err(Error::new(
                "INVALID_INPUT",
                "search query must be 1–4096 bytes",
            ));
        }
        let fingerprint = ids::digest(
            &json!({"tool":"search","query":i.query,"kinds":i.kinds,"sources":i.sources,"limit":i.limit}),
        )?;
        let offset = self.offset(i.cursor.as_deref(), &fingerprint)?;
        let quoted = i
            .query
            .split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        if quoted.is_empty() {
            return Err(Error::new("INVALID_INPUT", "empty search terms"));
        }
        let (a, b, c) = self.args()?;
        let kinds = serde_json::to_string(&i.kinds)?;
        let sources = serde_json::to_string(&i.sources)?;
        let sql = format!(
            "{VISIBLE} SELECT n.id,bm25(node_fts) FROM node_fts JOIN visible_nodes n ON n.id=node_fts.node_id JOIN snapshots s ON s.id=n.snapshot_id WHERE node_fts MATCH ?4 AND (?5='[]' OR n.kind IN (SELECT value FROM json_each(?5))) AND (?6='[]' OR s.source_id IN (SELECT value FROM json_each(?6))) ORDER BY CASE WHEN n.display_name=?7 OR n.logical_key=?7 THEN 0 ELSE 1 END,bm25(node_fts),n.id LIMIT ?8 OFFSET ?9"
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut nodes = vec![];
        let mut evidence = vec![];
        for row in stmt.query_map(
            params![
                a,
                b,
                c,
                quoted,
                kinds,
                sources,
                i.query,
                i.limit + 1,
                offset
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)),
        )? {
            let (s, rank) = row?;
            let n: Node = self.store.record(&s)?.ok_or_else(Self::unavailable)?;
            evidence.extend(n.evidence_ids.clone());
            nodes
                .push(json!({"node":n,"lexical_score":rank,"score_kind":"FTS5 candidate ranking"}));
        }
        let more = nodes.len() > i.limit as usize;
        nodes.truncate(i.limit as usize);
        // Citation closure is computed only after bounded visible results have been selected.
        evidence = nodes
            .iter()
            .flat_map(|n| {
                n["node"]["evidence_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x.as_str().map(String::from))
            })
            .collect();
        let mut e=self.envelope(json!({"nodes":nodes,"status":if nodes.is_empty(){"no_match_in_indexed_scope"}else{"matched"}}));
        e.evidence = self.citations(&evidence)?;
        if more {
            e.truncated = true;
            e.truncation_reason = Some("limit".into());
            e.next_cursor = Some(self.cursor(&fingerprint, offset + i.limit as u64)?);
        }
        Ok(e)
    }
    /// Exact lookup reveals neither hidden IDs nor hidden-object existence.
    pub fn node(&self, i: NodeInput) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        validate_id(&i.node_id, "node")?;
        let n = self
            .visible_node(&i.node_id)?
            .ok_or_else(Self::unavailable)?;
        let snapshot: String =
            self.store
                .db
                .query_row("SELECT snapshot_id FROM nodes WHERE id=?1", [&n.id], |r| {
                    r.get(0)
                })?;
        let s: Snapshot = self
            .store
            .record(&snapshot)?
            .ok_or_else(Self::unavailable)?;
        let mut e = self.envelope(json!({"node":n,"snapshot":s}));
        e.evidence = self.citations(&n.evidence_ids)?;
        Ok(e)
    }
    fn edges(
        &self,
        node: &str,
        direction: &str,
        predicates: &[Predicate],
        hypotheses: bool,
        count: u32,
        offset: u64,
    ) -> Result<Vec<(Assertion, String)>> {
        let (a, b, c) = self.args()?;
        let p = serde_json::to_string(predicates)?;
        // Each direction has a (endpoint, claim ID) index. Select at most the
        // requested page in each branch, then merge; a self-edge appears once.
        let mut selected = BTreeMap::new();
        let directions: &[&str] = match direction {
            "out" => &["subject_id"],
            "in" => &["object_id"],
            _ => &["subject_id", "object_id"],
        };
        for endpoint in directions {
            let sql = format!(
                "{VISIBLE} SELECT r.json,a.current_state FROM assertion_states a JOIN allowed_snapshots snap ON snap.id=a.snapshot_id JOIN visible_nodes s ON s.id=a.subject_id JOIN visible_nodes o ON o.id=a.object_id JOIN records r ON r.id=a.id WHERE a.visibility IN (SELECT value FROM json_each(?2)) AND a.{endpoint}=?4 AND (?5='[]' OR a.predicate IN (SELECT value FROM json_each(?5))) AND (a.current_state='admitted' OR (?6 AND a.current_state='proposed')) AND (?6 OR a.provenance_class<>'hypothesis') AND NOT EXISTS(SELECT 1 FROM assertion_evidence ae WHERE ae.assertion_id=a.id AND ae.evidence_id NOT IN(SELECT id FROM visible_evidence)) ORDER BY a.id LIMIT ?7"
            );
            let mut stmt = self.store.db.prepare(&sql)?;
            for row in stmt.query_map(
                params![a, b, c, node, p, hypotheses, u64::from(count) + offset],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )? {
                let (v, state) = row?;
                let assertion: Assertion = serde_json::from_str(&v)?;
                selected.insert(assertion.id.clone(), (assertion, state));
            }
        }
        let out = selected
            .into_values()
            .skip(offset as usize)
            .take(count as usize)
            .collect();
        Ok(out)
    }
    /// One-hop indexed adjacency with complete permitted endpoints.
    pub fn neighbors(&self, i: NeighborsInput) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        validate_id(&i.node_id, "node")?;
        limit(i.limit)?;
        list(&i.predicates)?;
        if !matches!(i.direction.as_str(), "in" | "out" | "both") {
            return Err(Error::new("INVALID_INPUT", "unknown traversal direction"));
        }
        self.visible_node(&i.node_id)?
            .ok_or_else(Self::unavailable)?;
        let fp = ids::digest(
            &json!({"tool":"neighbors","node":i.node_id,"direction":i.direction,"predicates":i.predicates,"limit":i.limit}),
        )?;
        let offset = self.offset(i.cursor.as_deref(), &fp)?;
        let mut edges = self.edges(
            &i.node_id,
            &i.direction,
            &i.predicates,
            false,
            i.limit + 1,
            offset,
        )?;
        let more = edges.len() > i.limit as usize;
        edges.truncate(i.limit as usize);
        let mut nodes = BTreeMap::new();
        let mut ids = vec![];
        let mut values = vec![];
        for (a, state) in edges {
            for id in [&a.subject, &a.object] {
                if let Some(n) = self.visible_node(id)? {
                    ids.extend(n.evidence_ids.clone());
                    nodes.insert(n.id.clone(), n);
                }
            }
            ids.extend(a.evidence.iter().map(|e| e.id.clone()));
            values.push(json!({"assertion":a,"current_state":state}));
        }
        let mut e=self.envelope(json!({"nodes":nodes.into_values().collect::<Vec<_>>(),"assertions":values,"store_generation":self.store.generation()?}));
        e.evidence = self.citations(&ids)?;
        if more {
            e.truncated = true;
            e.truncation_reason = Some("limit".into());
            e.next_cursor = Some(self.cursor(&fp, offset + i.limit as u64)?);
        }
        Ok(e)
    }
    fn resolve_seed(&self, mode: &str, seed: &str) -> Result<Node> {
        if mode == "material" || mode == "node" {
            validate_id(seed, "node")?;
            let n = self.visible_node(seed)?.ok_or_else(Self::unavailable)?;
            if mode == "material" && n.kind != NodeKind::Asset {
                return Err(Error::new(
                    "INVALID_INPUT",
                    "material trace seed must be an Asset",
                ));
            }
            return Ok(n);
        }
        if mode != "concept" {
            return Err(Error::new(
                "INVALID_INPUT",
                "trace mode must be material or concept",
            ));
        }
        let (a, b, c) = self.args()?;
        let sql = format!(
            "{VISIBLE} SELECT r.json FROM visible_nodes n JOIN records r ON r.id=n.id WHERE n.kind='Concept' AND n.logical_key=?4 ORDER BY n.id LIMIT 2"
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let rows = stmt
            .query_map(params![a, b, c, seed], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if rows.len() != 1 {
            return Err(Error::new(
                "UNAVAILABLE",
                "concept unavailable or ambiguous in selected context",
            ));
        }
        Ok(serde_json::from_str(&rows[0])?)
    }
    /// Three-hop breadth-first research trace with fan-out, edge, node and time limits.
    pub fn trace(&self, i: TraceInput) -> Result<Envelope> {
        self.trace_selected(i, None)
    }
    /// Restrict a material trace to an original shape block, never an array position or name.
    pub fn trace_material_shape(&self, i: TraceInput, shape: u32) -> Result<Envelope> {
        if i.mode != "material" {
            return Err(Error::new(
                "INVALID_INPUT",
                "shape selection requires a material asset",
            ));
        }
        self.trace_selected(i, Some(shape))
    }
    fn trace_selected(&self, i: TraceInput, shape: Option<u32>) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        limit(i.limit)?;
        let seed = self.resolve_seed(&i.mode, &i.seed)?;
        let start = Instant::now();
        let mut queue = VecDeque::from([(seed.id.clone(), 0u32)]);
        let mut nodes = BTreeMap::from([(seed.id.clone(), seed.clone())]);
        let mut edges = BTreeMap::new();
        let mut evidence = vec![];
        let mut reason = None;
        while let Some((id, depth)) = queue.pop_front() {
            if start.elapsed() > Duration::from_secs(2) {
                reason = Some("time_limit");
                break;
            }
            if depth >= 3 {
                continue;
            }
            let neighbors = self.edges(&id, "both", &[], i.include_hypotheses, 51, 0)?;
            if neighbors.len() > 50 {
                reason.get_or_insert("per_node_limit");
            }
            for (a, state) in neighbors.into_iter().take(50) {
                if let Some(shape) = shape {
                    if id == seed.id
                        && a.predicate != Predicate::HasMaterial
                        && a.predicate != Predicate::InputTo
                    {
                        continue;
                    }
                    let mut outside = false;
                    for endpoint in [&a.subject, &a.object] {
                        if let Some(n) = self.visible_node(endpoint)? {
                            if n.kind == NodeKind::Asset && n.id != seed.id {
                                outside = true;
                            }
                            if n.attributes["shape_block"]
                                .as_u64()
                                .is_some_and(|block| block != u64::from(shape))
                            {
                                outside = true;
                            }
                        }
                    }
                    if outside {
                        continue;
                    }
                }
                if edges.len() >= 2000 {
                    reason.get_or_insert("edge_limit");
                    break;
                }
                for nid in [&a.subject, &a.object] {
                    if !nodes.contains_key(nid) {
                        if nodes.len() >= 500 {
                            reason.get_or_insert("node_limit");
                            break;
                        }
                        if let Some(n) = self.visible_node(nid)? {
                            evidence.extend(n.evidence_ids.clone());
                            nodes.insert(n.id.clone(), n);
                            queue.push_back((nid.clone(), depth + 1));
                        }
                    }
                }
                if nodes.contains_key(&a.subject) && nodes.contains_key(&a.object) {
                    evidence.extend(a.evidence.iter().map(|e| e.id.clone()));
                    edges.insert(a.id.clone(), json!({"assertion":a,"current_state":state}));
                }
            }
        }
        evidence.extend(seed.evidence_ids.clone());
        let kinds: BTreeSet<_> = nodes.values().map(|n| n.kind).collect();
        let stages = [
            ("source", NodeKind::CodeSymbol),
            ("raw_asset", NodeKind::AssetField),
            ("interpretation", NodeKind::Material),
            ("output", NodeKind::ConversionStep),
            ("native", NodeKind::BinaryFunction),
            ("capture", NodeKind::DrawEvent),
        ];
        let visible_coverage = self
            .coverage(CoverageInput {
                sources: vec![],
                capabilities: vec![],
            })?
            .coverage;
        let capability_names = [
            ("source", "source_structure"),
            ("raw_asset", "materials"),
            ("interpretation", "materials"),
            ("output", "materials"),
            ("native", "native_metadata"),
            ("capture", "capture_events"),
        ];
        let missing=stages.into_iter().filter(|(_,k)|!kinds.contains(k)).map(|(stage,_)| {
            let name=capability_names.iter().find(|(s,_)|*s==stage).map(|(_,n)|*n).unwrap_or("");
            let caps=visible_coverage.iter().flat_map(|s|s["capabilities"].as_array().into_iter().flatten()).filter(|c|c["name"]==name).collect::<Vec<_>>();
            let status=if caps.is_empty()||caps.iter().all(|c|c["status"]=="not_run"){"not_indexed"}else if caps.iter().all(|c|c["status"]=="unsupported"){"unsupported"}else if caps.iter().all(|c|c["counts"]["inputs_failed"].as_u64().unwrap_or(0)>0){"extraction_failed"}else{"unresolved"};
            json!({"stage":stage,"status":status,"detail":"no supported connection in visible indexed selection; no implementation-absence claim"})
        }).collect::<Vec<_>>();
        let mut paths = vec![];
        for v in edges.values().take(i.limit as usize) {
            paths.push(v.clone());
        }
        if edges.len() > i.limit as usize {
            reason.get_or_insert("path_limit");
        }
        let mut e=self.envelope(json!({"seed":seed,"nodes":nodes.into_values().collect::<Vec<_>>(),"assertions":edges.into_values().collect::<Vec<_>>(),"paths":paths,"missing_stages":missing,"provenance_note":"every assertion retains its own origin; a path does not establish engine-wide behavior"}));
        e.evidence = self.citations(&evidence)?;
        e.coverage = visible_coverage;
        if let Some(shape) = shape {
            e.result["selected_shape_block"] = json!(shape);
        }
        e.truncated = reason.is_some();
        e.truncation_reason = reason.map(String::from);
        Ok(e)
    }
    /// Retrieve only verified local bytes, never a mutable current-main file.
    pub fn evidence(&self, i: EvidenceInput) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        validate_id(&i.evidence_id, "ev")?;
        if !(1..=65536).contains(&i.max_bytes) {
            return Err(Error::new(
                "INVALID_INPUT",
                "evidence budget must be 1–65536",
            ));
        }
        let ev = self
            .visible_evidence(&i.evidence_id)?
            .ok_or_else(Self::unavailable)?;
        let art: Artifact = self
            .store
            .record(&ev.artifact_id)?
            .ok_or_else(Self::unavailable)?;
        let mut e = self.envelope(
            json!({"evidence":ev,"artifact":art,"status":"unavailable_bytes","excerpt":null}),
        );
        e.evidence = self.citations(std::slice::from_ref(&i.evidence_id))?;
        if let Some(raw) = self.store.bytes(&art)? {
            let l = &ev.locator;
            let text = match l["kind"].as_str() {
                Some("source_span") => {
                    let s = l["start_byte"].as_u64().unwrap_or(0) as usize;
                    let end = l["end_byte"].as_u64().unwrap_or(0) as usize;
                    String::from_utf8(
                        raw.get(s..end)
                            .ok_or_else(|| Error::new("INTEGRITY", "stored span out of range"))?
                            .to_vec(),
                    )
                    .map_err(|_| Error::new("INTEGRITY", "stored span invalid UTF-8"))?
                }
                Some("json_pointer") => crate::bundle::artifact_json(&raw)?
                    .pointer(l["pointer"].as_str().unwrap_or(""))
                    .ok_or_else(|| Error::new("INTEGRITY", "stored pointer unavailable"))?
                    .to_string(),
                Some("binary_address" | "pcode" | "capture_event" | "observation")
                    if art.media_type == "application/json" =>
                {
                    let data = crate::bundle::artifact_json(&raw)?;
                    let record = match l["kind"].as_str() {
                        Some("binary_address") => {
                            data["functions"].get(l["rva"].as_str().unwrap_or(""))
                        }
                        Some("pcode") => data["functions"]
                            [l["function_rva"].as_str().unwrap_or("")]["pcode"]
                            .as_array()
                            .and_then(|ops| {
                                ops.iter().find(|op| {
                                    op["instruction_rva"] == l["instruction_rva"]
                                        && op["sequence"] == l["sequence"]
                                })
                            }),
                        Some("capture_event") => data["events"].get(l["event_id"].to_string()),
                        Some("observation") => {
                            data["observations"].as_array().and_then(|records| {
                                records.iter().find(|r| {
                                    r["record_id"] == l["record_id"]
                                        && data["session_id"] == l["session_id"]
                                })
                            })
                        }
                        _ => None,
                    };
                    record.map(Value::to_string).unwrap_or_default()
                }
                Some("note" | "observation") => String::from_utf8(raw)
                    .map_err(|_| Error::new("INTEGRITY", "text evidence is not UTF-8"))?,
                _ => String::new(),
            };
            e.result["status"] = json!(if text.is_empty() {
                "locator_only"
            } else {
                "verified"
            });
            e.result["excerpt"] = json!(text);
        }
        fit_excerpt(&mut e, i.max_bytes)?;
        Ok(e)
    }
    /// Coverage describes only the allowed declared selection, never hidden counts.
    pub fn coverage(&self, i: CoverageInput) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        list(&i.sources)?;
        list(&i.capabilities)?;
        let (a, b, c) = self.args()?;
        let sql = format!(
            "{VISIBLE} SELECT r.json FROM allowed_snapshots a JOIN records r ON r.id=a.id JOIN snapshots s ON s.id=a.id WHERE ?4='[]' OR s.source_id IN(SELECT value FROM json_each(?4)) ORDER BY a.id"
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut coverage = vec![];
        for row in stmt.query_map(params![a, b, c, serde_json::to_string(&i.sources)?], |r| {
            r.get::<_, String>(0)
        })? {
            let s: Snapshot = serde_json::from_str(&row?)?;
            let caps = s
                .capabilities
                .into_iter()
                .filter(|x| i.capabilities.is_empty() || i.capabilities.contains(&x.name))
                .collect::<Vec<_>>();
            coverage.push(json!({"snapshot_id":s.id,"source_id":s.source_id,"revision":s.revision,"scope":s.scope,"capabilities":caps}));
        }
        let mut e=self.envelope(json!({"scope_note":"complete refers only to declared selection and adapter capability"}));
        e.coverage = coverage;
        Ok(e)
    }
    /// Build bounded source/research packets; hypotheses occupy a separate section.
    pub fn pack(&self, i: ContextInput) -> Result<Envelope> {
        let _read = self.store.read_snapshot()?;
        if !(1024..=262144).contains(&i.max_bytes) || i.seeds.is_empty() {
            return Err(Error::new(
                "INVALID_INPUT",
                "context requires seeds and a 1024–262144 byte budget",
            ));
        }
        list(
            &i.seeds
                .iter()
                .map(|s| format!("{}:{}", s.kind, s.value))
                .collect::<Vec<_>>(),
        )?;
        let mut admitted = vec![];
        let mut hypotheses = vec![];
        let mut nodes = BTreeMap::new();
        let mut evs = BTreeMap::new();
        let mut unresolved = vec![];
        let mut truncated = false;
        for s in &i.seeds {
            if !matches!(s.kind.as_str(), "node" | "concept") {
                return Err(Error::new("INVALID_INPUT", "invalid seed kind"));
            }
            let seed = self.resolve_seed(&s.kind, &s.value)?;
            let mode = if s.kind == "concept" {
                "concept"
            } else {
                "node"
            };
            let trace = if mode == "node" {
                self.trace_from_node(seed.clone(), i.include_hypotheses)?
            } else {
                self.trace(TraceInput {
                    mode: "concept".into(),
                    seed: s.value.clone(),
                    include_hypotheses: i.include_hypotheses,
                    limit: 100,
                })?
            };
            truncated |= trace.truncated;
            for n in trace.result["nodes"].as_array().into_iter().flatten() {
                if let Some(id) = n["id"].as_str() {
                    nodes.insert(id.to_owned(), n.clone());
                }
            }
            for edge in trace.result["assertions"].as_array().into_iter().flatten() {
                if edge["assertion"]["provenance_class"] == "hypothesis"
                    || edge["current_state"] == "proposed"
                {
                    hypotheses.push(edge.clone());
                } else {
                    admitted.push(edge.clone());
                }
            }
            for ev in trace.evidence {
                if let Some(id) = ev["id"].as_str() {
                    evs.insert(id.to_owned(), ev);
                }
            }
            unresolved.extend(
                trace.result["missing_stages"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        admitted.sort_by_key(|a| a["assertion"]["id"].as_str().unwrap_or("").to_owned());
        admitted.dedup();
        hypotheses.sort_by_key(|a| a["assertion"]["id"].as_str().unwrap_or("").to_owned());
        hypotheses.dedup();
        let coverage = self
            .coverage(CoverageInput {
                sources: vec![],
                capabilities: vec![],
            })?
            .coverage;
        let mut e=self.envelope(json!({"seeds":i.seeds,"nodes":nodes.into_values().collect::<Vec<_>>(),"admitted_interpretations":admitted,"hypotheses":hypotheses,"conflicts":self.conflicts()?,"unresolved_links":unresolved,"suggested_operations":["Request exact evidence by evidence ID; operations are inert text"],"byte_count":0,"estimated_tokens":0}));
        e.evidence = evs.into_values().collect();
        let conflict_evidence = e.result["conflicts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["evidence_id"].as_str().map(String::from))
            .collect::<Vec<_>>();
        for citation in self.citations(&conflict_evidence)? {
            if !e.evidence.iter().any(|e| e["id"] == citation["id"]) {
                e.evidence.push(citation);
            }
        }
        e.coverage = coverage;
        e.truncated = truncated;
        if truncated {
            e.truncation_reason = Some("traversal_limit".into());
        }
        fit_packet(&mut e, i.max_bytes)?;
        Ok(e)
    }
    fn trace_from_node(&self, n: Node, hypotheses: bool) -> Result<Envelope> {
        self.trace(TraceInput {
            mode: "node".into(),
            seed: n.id,
            include_hypotheses: hypotheses,
            limit: 100,
        })
    }
    fn conflicts(&self) -> Result<Vec<Value>> {
        let mut stmt=self.store.db.prepare("SELECT id,assertion_a,assertion_b,evidence_id,reason,context_json FROM conflicts ORDER BY id")?;
        let mut out = vec![];
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })? {
            let (id, a, b, ev, reason, context) = row?;
            let ctx: Value = serde_json::from_str(&context)?;
            if ctx["context_id"] != self.context.id {
                continue;
            }
            let aa: Option<Assertion> = self.store.record(&a)?;
            let bb: Option<Assertion> = self.store.record(&b)?;
            if let (Some(aa), Some(bb)) = (aa, bb)
                && self.visible_node(&aa.subject)?.is_some()
                && self.visible_node(&aa.object)?.is_some()
                && self.visible_node(&bb.subject)?.is_some()
                && self.visible_node(&bb.object)?.is_some()
                && self.visible_evidence(&ev)?.is_some()
                && aa.visibility <= self.context.ceiling
                && bb.visibility <= self.context.ceiling
            {
                out.push(json!({"id":id,"assertions":[a,b],"evidence_id":ev,"reason":reason}));
            }
        }
        Ok(out)
    }
    fn cursor(&self, query: &str, offset: u64) -> Result<String> {
        let v = json!({"context":self.context.id,"query":query,"generation":self.store.generation()?,"offset":offset});
        let bytes = ids::canonical(&v)?;
        Ok(format!("{}.{}", hex(&bytes), ids::sha256(&bytes)))
    }
    fn offset(&self, cursor: Option<&str>, query: &str) -> Result<u64> {
        let Some(c) = cursor else { return Ok(0) };
        if c.len() > 4096 {
            return Err(Error::new("INVALID_CURSOR", "cursor too large"));
        }
        let (a, b) = c
            .split_once('.')
            .ok_or_else(|| Error::new("INVALID_CURSOR", "invalid cursor"))?;
        let bytes = unhex(a)?;
        if ids::sha256(&bytes) != b {
            return Err(Error::new("INVALID_CURSOR", "invalid cursor checksum"));
        }
        let v: Value = serde_json::from_slice(&bytes)?;
        if v["context"] != self.context.id || v["query"] != query {
            return Err(Error::new(
                "INVALID_CURSOR",
                "cursor does not match query context",
            ));
        }
        if v["generation"].as_i64() != Some(self.store.generation()?) {
            return Err(Error::new(
                "STALE_CURSOR",
                "store generation changed; restart pagination and inspect review state",
            ));
        }
        v["offset"]
            .as_u64()
            .filter(|x| *x <= 5_000_000)
            .ok_or_else(|| Error::new("INVALID_CURSOR", "invalid cursor offset"))
    }
}
fn limit(n: u32) -> Result<()> {
    if (1..=100).contains(&n) {
        Ok(())
    } else {
        Err(Error::new("INVALID_INPUT", "limit must be 1–100"))
    }
}
fn list<T: Ord>(v: &[T]) -> Result<()> {
    if v.len() > 32 || v.iter().collect::<BTreeSet<_>>().len() != v.len() {
        Err(Error::new(
            "INVALID_INPUT",
            "lists allow at most 32 unique entries",
        ))
    } else {
        Ok(())
    }
}
/// Full IDs are mandatory at query boundaries.
pub fn validate_id(s: &str, kind: &str) -> Result<()> {
    let prefix = format!("{kind}_");
    if !s.starts_with(&prefix)
        || s.len() != prefix.len() + 64
        || !s[prefix.len()..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        Err(Error::new("INVALID_INPUT", "invalid full object ID"))
    } else {
        Ok(())
    }
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2)
        || !s
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(Error::new("INVALID_CURSOR", "invalid cursor encoding"));
    }
    (0..s.len())
        .step_by(2)
        .map(|n| {
            u8::from_str_radix(&s[n..n + 2], 16)
                .map_err(|_| Error::new("INVALID_CURSOR", "invalid cursor encoding"))
        })
        .collect()
}
fn fit_excerpt(e: &mut Envelope, max: usize) -> Result<()> {
    loop {
        if serde_json::to_vec(&e)?.len() <= max {
            return Ok(());
        }
        let text = e.result["excerpt"].as_str().unwrap_or("");
        if text.is_empty() {
            return Err(Error::new(
                "BUDGET_TOO_SMALL",
                "metadata cannot fit response budget",
            ));
        }
        let mut end = text.len() / 2;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        e.result["excerpt"] = json!(&text[..end]);
        e.truncated = true;
        e.truncation_reason = Some("byte_budget".into());
    }
}
fn fit_packet(e: &mut Envelope, max: usize) -> Result<()> {
    loop {
        packet_closure(e);
        let size = serde_json::to_vec(&e)?.len();
        e.result["byte_count"] = json!(size);
        e.result["estimated_tokens"] = json!(size.div_ceil(4));
        let corrected = serde_json::to_vec(&e)?.len();
        if corrected <= max {
            if size == corrected {
                return Ok(());
            }
            continue;
        }
        e.truncated = true;
        e.truncation_reason = Some("byte_budget".into());
        let excess = corrected - max;
        let mut removed_bytes = 0usize;
        let mut dropped = false;
        for k in [
            "hypotheses",
            "admitted_interpretations",
            "nodes",
            "conflicts",
            "unresolved_links",
        ] {
            if let Some(a) = e.result[k].as_array_mut() {
                while removed_bytes < excess {
                    let Some(item) = a.pop() else {
                        break;
                    };
                    removed_bytes += serde_json::to_vec(&item)?.len() + 1;
                    dropped = true;
                }
            }
            if removed_bytes >= excess {
                break;
            }
        }
        while removed_bytes < excess {
            if let Some(item) = e.evidence.pop().or_else(|| e.coverage.pop()) {
                removed_bytes += serde_json::to_vec(&item)?.len() + 1;
                dropped = true;
            } else {
                break;
            }
        }
        if !dropped {
            return Err(Error::new(
                "BUDGET_TOO_SMALL",
                "fixed packet metadata exceeds budget",
            ));
        }
        // Drop claims whose endpoints or complete supporting evidence no longer fit.
        let node_ids = e.result["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["id"].as_str().map(String::from))
            .collect::<BTreeSet<_>>();
        for k in ["admitted_interpretations", "hypotheses"] {
            if let Some(a) = e.result[k].as_array_mut() {
                a.retain(|v| {
                    node_ids.contains(v["assertion"]["subject"].as_str().unwrap_or(""))
                        && node_ids.contains(v["assertion"]["object"].as_str().unwrap_or(""))
                });
            }
        }
    }
}

// Keep every retained claim's endpoints and all evidence; dropping metadata drops the claim.
fn packet_closure(e: &mut Envelope) {
    let ev_ids = e
        .evidence
        .iter()
        .filter_map(|v| v["id"].as_str().map(String::from))
        .collect::<BTreeSet<_>>();
    if let Some(nodes) = e.result["nodes"].as_array_mut() {
        nodes.retain(|n| {
            n["evidence_ids"].as_array().is_some_and(|ids| {
                ids.iter()
                    .all(|id| ev_ids.contains(id.as_str().unwrap_or("")))
            })
        });
    }
    let nodes = e.result["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["id"].as_str().map(String::from))
        .collect::<BTreeSet<_>>();
    for key in ["admitted_interpretations", "hypotheses"] {
        if let Some(edges) = e.result[key].as_array_mut() {
            edges.retain(|v| {
                nodes.contains(v["assertion"]["subject"].as_str().unwrap_or(""))
                    && nodes.contains(v["assertion"]["object"].as_str().unwrap_or(""))
                    && v["assertion"]["evidence"].as_array().is_some_and(|ids| {
                        ids.iter()
                            .all(|id| ev_ids.contains(id["id"].as_str().unwrap_or("")))
                    })
            });
        }
    }
    let claims = ["admitted_interpretations", "hypotheses"]
        .into_iter()
        .flat_map(|key| e.result[key].as_array().into_iter().flatten())
        .filter_map(|v| v["assertion"]["id"].as_str().map(String::from))
        .collect::<BTreeSet<_>>();
    if let Some(conflicts) = e.result["conflicts"].as_array_mut() {
        conflicts.retain(|c| {
            c["assertions"].as_array().is_some_and(|ids| {
                ids.iter()
                    .all(|id| claims.contains(id.as_str().unwrap_or("")))
            }) && ev_ids.contains(c["evidence_id"].as_str().unwrap_or(""))
        });
    }
    let mut needed = e.result["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|n| {
            n["evidence_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|id| id.as_str().map(String::from))
        })
        .collect::<BTreeSet<_>>();
    for key in ["admitted_interpretations", "hypotheses"] {
        for edge in e.result[key].as_array().into_iter().flatten() {
            for id in edge["assertion"]["evidence"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(id) = id["id"].as_str() {
                    needed.insert(id.into());
                }
            }
        }
    }
    if let Some(conflicts) = e.result["conflicts"].as_array() {
        for c in conflicts {
            if let Some(id) = c["evidence_id"].as_str() {
                needed.insert(id.into());
            }
        }
    }
    e.evidence
        .retain(|v| v["id"].as_str().is_some_and(|id| needed.contains(id)));
}
