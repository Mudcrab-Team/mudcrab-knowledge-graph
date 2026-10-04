-- Reference first migration for the proposed local graph; not a running Mudcrab implementation.
-- The application enforces ID derivation, typed locators, immutable publication, evidence closure,
-- context/policy filtering and ontology domain/range checks in addition to these constraints.
PRAGMA foreign_keys = ON;
PRAGMA user_version = 1;
CREATE TABLE sources (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL, origin_uri TEXT NOT NULL,
 metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json))
);
CREATE TABLE snapshots (
 id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES sources(id), revision TEXT NOT NULL,
 content_sha256 TEXT NOT NULL CHECK(length(content_sha256)=64),
 extraction_json TEXT NOT NULL CHECK(json_valid(extraction_json)),
 scope_json TEXT NOT NULL CHECK(json_valid(scope_json)),
 capabilities_json TEXT NOT NULL CHECK(json_valid(capabilities_json)),
 visibility TEXT NOT NULL CHECK(visibility IN ('public','reviewed','restricted')),
 state TEXT NOT NULL CHECK(state IN ('staging','published'))
);
CREATE TABLE snapshot_dependencies (
 snapshot_id TEXT NOT NULL REFERENCES snapshots(id), dependency_id TEXT NOT NULL REFERENCES snapshots(id),
 PRIMARY KEY(snapshot_id,dependency_id), CHECK(snapshot_id<>dependency_id)
);
CREATE TABLE runs (
 id INTEGER PRIMARY KEY, snapshot_id TEXT REFERENCES snapshots(id),
 status TEXT NOT NULL CHECK(status IN ('staging','published','failed','cancelled')),
 report_json TEXT NOT NULL CHECK(json_valid(report_json))
);
CREATE TABLE artifacts (
 id TEXT PRIMARY KEY, snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
 path TEXT NOT NULL, sha256 TEXT NOT NULL CHECK(length(sha256)=64),
 media_type TEXT NOT NULL, byte_size INTEGER NOT NULL CHECK(byte_size>=0),
 visibility TEXT NOT NULL CHECK(visibility IN ('public','reviewed','restricted')),
 blob_sha256 TEXT, UNIQUE(snapshot_id,path,sha256)
);
CREATE TABLE nodes (
 id TEXT PRIMARY KEY, snapshot_id TEXT NOT NULL REFERENCES snapshots(id), kind TEXT NOT NULL,
 logical_key TEXT NOT NULL, display_name TEXT NOT NULL,
 attributes_json TEXT NOT NULL CHECK(json_valid(attributes_json)),
 visibility TEXT NOT NULL CHECK(visibility IN ('public','reviewed','restricted')),
 UNIQUE(snapshot_id,kind,logical_key)
);
CREATE INDEX nodes_kind_snapshot ON nodes(kind,snapshot_id);
CREATE INDEX nodes_logical_key ON nodes(logical_key);
CREATE TABLE evidence (
 id TEXT PRIMARY KEY, snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
 artifact_id TEXT NOT NULL REFERENCES artifacts(id),
 locator_json TEXT NOT NULL CHECK(json_valid(locator_json)), method TEXT NOT NULL,
 provenance_class TEXT NOT NULL CHECK(provenance_class IN ('extracted','community','observed','manual','hypothesis')),
 visibility TEXT NOT NULL CHECK(visibility IN ('public','reviewed','restricted'))
);
CREATE TABLE node_evidence (
 node_id TEXT NOT NULL REFERENCES nodes(id), evidence_id TEXT NOT NULL REFERENCES evidence(id),
 PRIMARY KEY(node_id,evidence_id)
);
CREATE TABLE predicates (name TEXT PRIMARY KEY, definition_json TEXT NOT NULL CHECK(json_valid(definition_json)));
CREATE TABLE assertions (
 id TEXT PRIMARY KEY, snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
 subject_id TEXT NOT NULL REFERENCES nodes(id), predicate TEXT NOT NULL REFERENCES predicates(name),
 object_id TEXT NOT NULL REFERENCES nodes(id),
 attributes_json TEXT NOT NULL CHECK(json_valid(attributes_json)),
 provenance_class TEXT NOT NULL CHECK(provenance_class IN ('extracted','community','observed','manual','hypothesis')),
 initial_state TEXT NOT NULL CHECK(initial_state IN ('admitted','proposed')),
 visibility TEXT NOT NULL CHECK(visibility IN ('public','reviewed','restricted')),
 CHECK(provenance_class<>'hypothesis' OR initial_state='proposed')
);
CREATE INDEX assertion_outgoing ON assertions(subject_id,predicate,object_id,snapshot_id);
CREATE INDEX assertion_incoming ON assertions(object_id,predicate,subject_id,snapshot_id);
CREATE INDEX assertion_snapshot ON assertions(snapshot_id);
CREATE TABLE assertion_evidence (
 assertion_id TEXT NOT NULL REFERENCES assertions(id), evidence_id TEXT NOT NULL REFERENCES evidence(id),
 role TEXT NOT NULL CHECK(role IN ('supports','contradicts','context')),
 PRIMARY KEY(assertion_id,evidence_id,role)
);
CREATE TABLE review_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 assertion_id TEXT NOT NULL REFERENCES assertions(id), actor TEXT NOT NULL, reason TEXT NOT NULL,
 decision TEXT NOT NULL CHECK(decision IN ('admitted','rejected','withdrawn','proposed')),
 reviewed_at TEXT NOT NULL, evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json))
);
CREATE INDEX reviews_by_assertion ON review_events(assertion_id,sequence DESC);
CREATE VIEW assertion_states AS SELECT a.*,
 COALESCE((SELECT r.decision FROM review_events r WHERE r.assertion_id=a.id ORDER BY r.sequence DESC LIMIT 1),a.initial_state) AS current_state
 FROM assertions a;
CREATE TABLE conflicts (
 id TEXT PRIMARY KEY, assertion_a TEXT NOT NULL REFERENCES assertions(id),
 assertion_b TEXT NOT NULL REFERENCES assertions(id), evidence_id TEXT NOT NULL REFERENCES evidence(id),
 context_json TEXT NOT NULL CHECK(json_valid(context_json)), reason TEXT NOT NULL,
 CHECK(assertion_a<>assertion_b)
);
CREATE TABLE contexts (
 id TEXT PRIMARY KEY, policy TEXT NOT NULL CHECK(policy IN ('public','reviewed','restricted')),
 config_json TEXT NOT NULL CHECK(json_valid(config_json))
);
CREATE TABLE context_snapshots (
 context_id TEXT NOT NULL REFERENCES contexts(id), snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
 PRIMARY KEY(context_id,snapshot_id)
);
CREATE TABLE context_aliases (alias TEXT PRIMARY KEY, context_id TEXT NOT NULL REFERENCES contexts(id));
CREATE VIRTUAL TABLE node_fts USING fts5(node_id UNINDEXED, display_name, body, tokenize='unicode61');
-- FTS writes occur in the same publication transaction as the corresponding node changes.
-- All query paths join FTS hits back to allowed context snapshots/nodes before returning data.
-- Do not accept arbitrary SQL from MCP. Public export rebuilds a new database/index.
