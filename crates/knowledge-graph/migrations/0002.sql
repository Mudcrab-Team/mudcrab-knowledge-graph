-- Canonical immutable wire records and verified local bytes.
CREATE TABLE records (id TEXT PRIMARY KEY, snapshot_id TEXT NOT NULL REFERENCES snapshots(id), category TEXT NOT NULL, json TEXT NOT NULL CHECK(json_valid(json)));
CREATE INDEX records_snapshot ON records(snapshot_id,category);
CREATE TABLE artifact_bytes (artifact_id TEXT PRIMARY KEY REFERENCES artifacts(id), bytes BLOB NOT NULL);
CREATE TABLE imports (snapshot_id TEXT PRIMARY KEY REFERENCES snapshots(id), bundle_digest TEXT NOT NULL);
CREATE TABLE generation (singleton INTEGER PRIMARY KEY CHECK(singleton=1), value INTEGER NOT NULL);
INSERT INTO generation VALUES(1,0);
CREATE TABLE source_cache (fingerprint TEXT PRIMARY KEY, declarations_json TEXT NOT NULL CHECK(json_valid(declarations_json)));
PRAGMA user_version = 2;
