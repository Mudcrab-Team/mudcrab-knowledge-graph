# Research with pinned evidence

Run commands from the KG checkout after `cargo build --locked --features mcp --bin mudcrab-kg`. Choose a research directory outside Git. This example indexes this repository's actual committed Rust source.

```sh
KG_ROOT="$(mktemp -d)/graph"
mkdir -p "$KG_ROOT"
KG_REV="$(git rev-parse HEAD)"
KG_BIN="$PWD/target/debug/mudcrab-kg"
"$KG_BIN" --root "$KG_ROOT" init
"$KG_BIN" --root "$KG_ROOT" ingest source \
  --repo . --source kg --revision "$KG_REV" \
  --origin https://github.com/Mudcrab-Team/mudcrab-knowledge-graph \
  --path crates/knowledge-graph/src/store.rs > "$KG_ROOT/import.json"
python3 - "$KG_ROOT" <<'PY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
snapshot = json.loads((root / 'import.json').read_text())['snapshot_id']
(root / 'context.json').write_text(json.dumps({
    'schema_version': 1, 'name': 'source', 'snapshot_ids': [snapshot], 'policy': 'public'
}))
PY
"$KG_BIN" --root "$KG_ROOT" context create --name source --manifest "$KG_ROOT/context.json"
"$KG_BIN" --root "$KG_ROOT" --context source search --query Store --kind CodeSymbol --limit 2
"$KG_BIN" --root "$KG_ROOT" --context source coverage
```

`ingest source` declares source visibility **public**, with MIT OR Apache-2.0 and `with_notice` defaults. Use this command only for source you can classify that way; pass its actual `--license-expression` and `--redistribution`. For differently classified repositories, use an explicit source registry with the correct rights and visibility, rather than trusting these defaults. [Privacy](privacy.md) explains the boundary.

## Read the response

Queries return the schema-version-1 envelope defined in [WIRE_PROTOCOL.md](../research/knowledge-graph/contracts/WIRE_PROTOCOL.md): pinned context, result, evidence, coverage, ranking information and truncation. Use full IDs returned by the graph. `node --id`, `neighbors --id`, and `evidence --id` accept those IDs; `evidence --max-bytes` bounds verified artifact excerpts.

For a factual statement, retain the context ID, revision/hash, locator and evidence ID. A source span reports UTF-8 byte bounds and line numbers at that revision. Retrieved bytes are verified against the stored artifact hash. Line numbers in a current checkout may differ.

Source structure represents lexical declarations and containment. Unexpanded conditions, symbolic WGSL bindings, same-name matches, and generic SCIP references do not establish runtime dispatch, ABI facts or compiled shader reflection. `no_match_in_indexed_scope` only describes the indexed selection.

## Index another checkout

Supply an explicit local checkout and immutable commit. No build or network acquisition occurs during import.

```sh
"$KG_BIN" --root "$KG_ROOT" ingest source \
  --repo /absolute/path/to/mudcrab --source mudcrab \
  --revision b2758e522dfbf273f641e9c787e6cbbcd019495c \
  --origin https://github.com/Mudcrab-Team/mudcrab \
  --path crates/converter/src/material.rs --path crates/engine/src/render.rs \
  --path crates/engine/src/shaders
```

Omitting `--path` indexes all supported files in the pinned tree. It reads committed Git objects, not uncommitted working files. Material concept seeds apply to source ID `mudcrab`; authored selector mappings remain manual/proposed.

For a pre-generated SCIP index, add `--mode scip --scip /absolute/index.scip --configuration /absolute/build-config.json`. Record exact producer/protocol, target, feature and analysis settings. Unspecified position encoding requires `--encoding-override utf8|utf16|utf32`. Generate SCIP only as a separate authorized step; the KG never builds an indexed repository or runs its macros.

`ingest registry --file FILE` reads the [registry format](../research/knowledge-graph/examples/sources.example.toml). Local paths live in the store's `local-roots.json` under declared aliases. Resolve origin, immutable revision, license, visibility, redistribution and lineage for each entry first. The supplied registry contains unresolved examples; it is not ready to import wholesale. Native/capture streams are imported separately.

## Compare and review

A new import produces a snapshot. Create an explicit new context containing the desired snapshots; dependencies must be present and contexts reject multiple revisions of the same source. A context name can be reassigned explicitly, while its old `ctx_…` ID stays readable. Reimporting unchanged inputs is idempotent; deletion in a later revision does not delete old evidence.

`trace concept --concept skyrim.material.double_sided` and `trace material --asset FULL_NODE_ID --shape ORIGINAL_BLOCK_INDEX` follow typed relationships. `--include-hypotheses` explicitly includes proposed mappings. Native/capture stages can be not indexed even when material conversion is verified. [The material guide](../integrations/mudcrab/README.md) runs actual generated NIF→GLB extraction.

Import a proposal using `proposals import --file FILE` with the exact [proposal envelope](../research/knowledge-graph/contracts/WIRE_PROTOCOL.md): immutable context ID, endpoints, ontology predicate, explanation, supporting evidence, missing evidence and producer. Proposals start hypothesis/proposed; suggested commands are never executed.

Only an explicit local `review --assertion FULL_ASSERTION_ID --actor NAME --decision admitted|rejected|withdrawn|proposed --reason TEXT` appends a decision. Actor text is attribution, not authentication. Admission does not change hypothesis/manual origin. `conflict --context CONTEXT --file FILE` records an explicit pair of assertions, supporting evidence and reason within that context.

Use `context pack --context CONTEXT --seed FULL_NODE_ID --max-bytes 65536 --out /external/packet.json` for a bounded research packet. Claims that lose endpoint or evidence closure are removed when the budget is exhausted. Share a [fresh public export](privacy.md), after checking classification and rights.
