# External analysis and observations

The graph accepts exact normalized streams. The Python exporters implement selected API extraction, with explicit byte/record/time limits, separate child-process deadlines and no automatic input acquisition. Real native/capture inputs default restricted. `synthetic: true` is an operator declaration for original redistributable fixtures, not a declassification tool.

| Adapter | Target API | Needs | Verification boundary |
|---|---|---|---|
| Ghidra | Ghidra 11.4 / PyGhidra 2.2 | Compatible Python/JVM, existing analyzed project and matched executable | API tests pass; installed-application smoke not run |
| RenderDoc | RenderDoc 1.43 | Application Python, original existing capture, supported local replay environment | API tests pass; installed replay smoke not run |
| Runtime observations | Contract schema version 1 | Inert records from an authorized producer | Rust module/RVA/session/hash validation tests |
| Materials | Companion patch on reviewed Mudcrab base | Locally built converter/exporter and selected inputs | Actual original NIF→GLB smoke; [integration guide](../integrations/mudcrab/README.md) |

`mudcrab-kg --root /external/store doctor --adapter ghidra` (or `renderdoc`) reports visible prerequisites. It does not prove project/capture compatibility or replay success. Use the same Python environment for the exporter and its application modules.

## Ghidra

Compile the original [C fixture](../scripts/knowledge-graph/fixtures/native_probe.c), then prepare and analyze a disposable project as a separate authorized operator action:

```sh
cc -g -O0 scripts/knowledge-graph/fixtures/native_probe.c -o /external/native-probe
# Prepare an analyzed Ghidra project and obtain exact function RVAs and type paths.
python3 scripts/knowledge-graph/ghidra/export_kg.py \
  --project-location /external/project-parent --project-name toy \
  --program /native-probe --module /external/native-probe \
  --selection /external/native-selection.json --out /new/external/native-export
```

Schematic selection; replace the RVA/type path with actual analyzed selections:

```json
{"schema_version":1,"source":"toy-native","synthetic":true,"functions":["0x1234"],"types":["/Material"],"level":2,"max_functions":200,"neighbor_hops":2,"timeout_seconds":30,"max_relationships":50000,"max_pcode_ops":10000,"max_seconds":300}
```

The exporter opens an existing DomainFile read-only, verifies its executable SHA-256, performs no import/analysis/save and checks the modification state. Level 0 exports metadata, selected types/signatures/body ranges, direct call sites and unresolved indirect sites. Level 1 adds bounded decompilation. Level 2 retains selected high P-code and reports field access only with recovered struct-pointer arithmetic, a constant offset and immediate LOAD/STORE def-use within the bounded slice. Other interpretations remain unresolved.

External-space functions stay external. Module/address logical keys survive label renames; analysis snapshots change when names/types/settings or versions change. Optional `imports` record exact origin/revision/kind/selector and declared dependency snapshots. Operator source/native mappings remain manual/proposed and non-independent corroboration; community names do not become recovered original names by coincidence.

## RenderDoc

Prepare an original capture separately, then run with RenderDoc's compatible Python:

```sh
python3 scripts/knowledge-graph/renderdoc/export_kg.py \
  --capture /external/original-toy.rdc --selection /external/capture-selection.json \
  --out /new/external/capture-export
```

```json
{"schema_version":1,"source":"toy-capture","synthetic":true,"events":[7],"stages":["Vertex","Pixel"],"constants":[{"stage":"Pixel","index":0,"array_element":0}],"max_events":100,"max_bindings":1000,"max_constants":256,"max_seconds":120}
```

Only selected events, bindings, reflection and selected constant blocks are inspected. Constant union bits are preserved as hex strings. Unsupported reflection/replay failures are diagnostics. Resource identity includes capture hash, so equal resource IDs in different captures do not join. The exporter does not dump textures/buffers or infer asset/native/GPU correspondences. Binary captures are not copied into bundles.

## Import and inspect

Both exporters write `manifest.json`, exact-hashed artifact bytes and five normalized JSONL files: artifacts, evidence, nodes, assertions and diagnostics. Import after a successful export:

```sh
mudcrab-kg --root /external/store ingest bundle --bundle /new/external/native-export
```

Create a context selecting the returned snapshot and intended policy as in [the research guide](research-workflow.md). Extraction failure and graph-import failure are separate; the CLI rejects producer-reported extraction errors instead of publishing a successful snapshot.

## Runtime observations

`ingest observations --file FILE --source ID` validates inert records; it does not attach a debugger. Records contain a session ID, exact producer/version, time domain, modules and observations with canonical payload hashes. See [the importer](../crates/knowledge-graph/src/observations.rs) and [its tests](../crates/knowledge-graph/tests/observations.rs) for complete executable examples.

Code identity needs a full module SHA-256, matched load base/image size, and range-checked VA→RVA normalization. Heap identity is session scoped and has no invented module/RVA. Addresses/large values use strings. A call observation yields `OBSERVED_CALL`; it does not establish a complete static control-flow graph.

## What tests prove

Eight Python tests exercise actual exporter extraction functions against explicit API doubles, real normalized-stream import, timeout/mutation handling, label rename, typed P-code def-use and selected capture descriptors/reflection. The original C fixture is actually compiled and executed. These tests do not substitute for running Ghidra or replaying RenderDoc. Record installed versions, selected original inputs, commands and application outcomes when those smokes become available.
