# Lighting-corpus import

Import an accepted Mudcrab lighting-corpus checkpoint as `ResearchClaim` nodes
with claim relations, concept associations, and code traces. The exporter reads
corpus bytes through pinned Git objects; anchored implementation files missing
from the corpus revision fall back to hash-verified working-tree bytes, and
that provenance is recorded in the snapshot scope.

## Workflow

Build the binary, ingest the Mudcrab source snapshot that claim traces target,
then export and import the corpus bundle into the same store:

```sh
cargo build --locked --features mcp --bin mudcrab-kg
KG_BIN="$PWD/target/debug/mudcrab-kg"
KG_ROOT=/external/kg-stores/lighting-corpus-01  # outside Git
"$KG_BIN" --root "$KG_ROOT" init
"$KG_BIN" --root "$KG_ROOT" ingest source --repo /path/to/mudcrab \
  --source mudcrab --revision <main-commit> \
  --origin https://github.com/Mudcrab-Team/mudcrab \
  --path crates --path docs > "$KG_ROOT/import-mudcrab.json"
```

List the indexed paths that claim traces may target, one per line, then export.
`--corpus-rev` must be the commit carrying the accepted corpus files;
`--work` supplies hash-verified bytes only for anchored files missing there:

```sh
python3 scripts/knowledge-graph/corpus_bundle.py --repo /path/to/mudcrab \
  --corpus-rev <corpus-commit> --receipt 20261010-02 \
  --source-snapshot <snap-id-from-import> --source-scope indexed-paths.txt \
  --work /path/to/mudcrab-worktree --out /external/corpus-bundle
"$KG_BIN" --root "$KG_ROOT" ingest bundle --bundle /external/corpus-bundle/bundle.json
```

Create one context over both snapshots. The corpus source declares
`review_required` redistribution, so the context profile is `research-local`:

```sh
cat > /tmp/context.json <<'JSON'
{"schema_version": 1, "name": "lighting-corpus-01",
 "snapshot_ids": ["<source-snap>", "<corpus-snap>"], "policy": "research-local"}
JSON
"$KG_BIN" --root "$KG_ROOT" context create --name lighting-corpus-01 \
  --manifest /tmp/context.json
```

## Review and queries

Curated relations import as manual proposed assertions. Admit them with the
checkpoint as the recorded reason; origin stays manual:

```sh
"$KG_BIN" --root "$KG_ROOT" review --assertion <as-id> \
  --actor corpus-receipt-20261010-02 --decision admitted \
  --reason "accepted in lighting corpus checkpoint 20261010-02"
```

Useful queries against the context:

```sh
CTX=<ctx-id>
"$KG_BIN" --root "$KG_ROOT" --context "$CTX" search --query attenuation --kind ResearchClaim
"$KG_BIN" --root "$KG_ROOT" --context "$CTX" neighbors --id <claim-node> --predicate SUPPORTS --predicate CHALLENGES
"$KG_BIN" --root "$KG_ROOT" --context "$CTX" trace concept --concept ambient-environment
"$KG_BIN" --root "$KG_ROOT" --context "$CTX" evidence --id <ev-id>
```

The recorded claim assessment is informational node provenance; admission is
governed by review state. `DEPENDS_ON` edges carry the source model's
conceptual dependencies only, never frame order or execution. Re-exporting
unchanged inputs reproduces the snapshot ID; exporter logic changes bump the
exporter version and mint a new snapshot instead.

## Limits

Line anchors that fail hash verification are skipped with warnings, and the
run report lists every skip. External references, directory anchors, and
targets outside the declared source scope never become assertions. Staged
notes without accepted ledgers are excluded from the snapshot scope.
