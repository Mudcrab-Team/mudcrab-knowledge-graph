> Historical record from the Mudcrab integration worktree. Current standalone checks are documented in [verification](../../docs/verification.md).

# Implementation and verification

Implementation branch: `feature/research-knowledge-graph`, isolated worktree
the isolated Mudcrab implementation worktree. Reviewed main and implementation base
both equal `b2758e522dfbf273f641e9c787e6cbbcd019495c`; main was pulled with
`--ff-only`, with its untracked `.pr-reviews/` preserved. See [BASELINE.md](BASELINE.md)
and [ADR 0010](../../docs/adr/0010-research-knowledge-graph.md).
All work is local. No push, remote comment, issue or PR was created.
Assisted by Codex; local commits include `Assisted-by: Codex`.

## Delivered behavior

KG-00–15 / M0–M6 implement independent graph storage and queries, actual pinned
source indexing, actual materials extraction and publication lineage, bounded
external-analysis adapters, runtime observation import, proposals/reviews/conflicts,
read-only stdio MCP, historical contexts and fresh public export.
The [quick start](README.md) includes exact commands and input formats. Local commits
cover foundation (`98f5e9d`), materials (`c75166d`), adapters (`de85a43`) and agent
handoff (`b594f1a`), followed by the final integration/documentation commit. A complete
unified diff is saved outside Git as
`<local-artifact-root>/implementation.patch`.

The graph package has no converter, Bevy, engine, GPU or original-game dependency.
SQLite owns transactions, FTS and adjacency; source structure uses Tree-sitter,
SCIP uses official generated protobuf bindings, and MCP uses rmcp. Optional analyzers
remain separate Python processes. Existing extraction and conversion are reused.
No rendering behavior, root SPEC.md, Bevy version, world/converter data schema or
OpenSkyrim wire name changed. The engine config test's existing non-engine flag
allowlist now includes research/converter/analyzer command options; no runtime
parser arm or acceptance script was rewritten. Existing Cargo.lock package versions were retained;
59 package/version entries were added for the new tools.

Contract and wire schema remain v1. Local SQLite migration 0003 adds ordered
outgoing/incoming adjacency indexes after migrations 0001/0002. Query and export
read transactions pin one SQLite generation throughout each operation. Generation
cursors reject subsequent publication/review changes. Source adapter v4 removes
execution-specific cache-hit counts from immutable metadata; structural parser
cache v3 still keys content and grammar. Material adapter v2 records the final
extraction semantics. Earlier immutable snapshots remain readable.

Schema validation operates record by record; normalized-bundle hashing streams
exactly the previous sorted canonical representation. Neither optimization weakens
identity, schema, ontology, evidence, dependency or visibility validation.

## Actual source and material evidence

Artifacts are outside Git under `<local-artifact-root>/`.
The reproducible source-only smoke is `scripts/knowledge-graph/smoke_handoff.py`.
It indexes the reviewed Git objects without running the indexed project's build,
reimports unchanged source, creates real Git deletion history, retrieves pinned
source evidence, builds bounded packets, and scans every fresh public-export file
and its FTS/cache for a restricted original privacy fixture. The full reviewed-source
index yielded 13,558 nodes / 13,353 assertions and unchanged reimport was idempotent.
Its packet measured 40,648 bytes; the fresh public packet measured 48,924 bytes.
Actual outputs are in `source-handoff-final/`, including Q07–Q10 and verified source
evidence. No source, game or GPU build is needed to run this source-only smoke.

Final material smoke: `material-final-v2/`. The procedural original two-shape NIF,
DDS and ESM were converted with the real converter pipeline, then exported with
`kg-export` and imported. Selected source indexing contributed 678 nodes / 673
assertions; material extraction contributed 61 nodes / 99 assertions.

- Source snapshot: `snap_22f5d004c79a913b7325a3a964336be6ddcc1df79aae8d486d1aa9fac4bcedbf`.
- Material snapshot: `snap_8349afa9dd4f98bf4c0f0513f10c25e2e0a650cc6dee38b6fd95fed7d2103758`.
- Context: `ctx_8f425d963df05f32b43763e07d333602d5105cd90dc6b2517b148b7dabe60dcd`.
- Q01 citation: `crates/converter/src/material.rs:124–143`, UTF-8 bytes
  `[3304,3950)`, reviewed revision; evidence
  `ev_9d12ab33aaf485a10a78a6f3173c2cdfe249f1234b7086ea0236e3bb15e769b0`.
- Q02 keeps raw flags, interpreted double-sided material, and actual published
  fields separate. Raw negative emission bits `0xbf800000` and interpreted clamp
  `0x00000000` are distinct. Source concept mappings remain manual/proposed.
- Q03 observes sRGB and linear output aliases and the published manifest's one
  pruned missing-texture reference. Slot, semantic, original shape and output URI
  remain attached to their own evidence.

`q01.json`, `q02.json`, `q03.json`, `shape-1.json`, `shape-4.json`,
`pipeline-report.json` and `material-variant-results.json` contain the actual results.
Copies of the actual GLB were reordered, stripped of an extension, given an empty
extension, and assigned duplicate shape identities. Shape joins survive reordering;
changed bytes invalidate manifest lineage and leave it manual/proposed. Removing an
already absent extension leaves bytes and verified lineage unchanged. Duplicate
output shape identities are rejected. Shape-filtered traces exclude other shapes.
These transformed test copies are explicitly separate from converter output.

## CLI and MCP demonstrations

Final stdio smoke: `mcp-final.json`. It performs a real initialize/discover/call
exchange with the built rmcp server, discovers exactly seven tools, checks generated
schemas/read-only annotations, compares the response exactly with CLI Q01, and
rejects unknown SQL tools, context/policy escalation and filesystem-path arguments.
No HTTP server or model API is involved.

Equivalent tool input:

```json
{"name":"kg.search","arguments":{"query":"ValidatedNifMaterial","kinds":["CodeSymbol"],"limit":2}}
```

Its result uses the immutable context above and the Q01 source citation. Native and
capture stages in materials traces are explicitly `not_indexed`; unsupported or
failed extraction and indexed-but-unresolved connections have distinct statuses.
Q07's source deletion demonstration uses real separate Git commits and immutable
contexts. Q08 is a source-only packet. Q09 is a packet from a newly constructed public
store, with no restricted marker, source cache, local roots or working DB sidecars.
Q10 reports `no_match_in_indexed_scope`, never implementation absence.

API-extraction examples Q04–Q06 are in `adapter-api-integration/`. They exercise the
actual extraction functions against API test doubles, then import the resulting
normalized streams with the Rust CLI. Native label rename changes the analysis
snapshot but preserves the module/address logical key; direct and unresolved
indirect sites remain distinct. Capture resource identity includes the capture
hash. These examples do not prove installed analyzer/replay compatibility.

## External-tool prerequisites and limitations

The native adapter targets Ghidra 11.4 / PyGhidra 2.2. It opens an existing DomainFile
read-only, verifies executable SHA-256, performs no analysis/save, checks modification
state, and exports selected metadata, signatures/types, direct/indirect call sites,
decompilation and high P-code. Field-access assertions require recovered typed
struct-pointer arithmetic, a constant offset and immediate LOAD/STORE def-use.
Both pointer and LOAD/STORE operations must belong to the retained bounded P-code
slice. Other field/control-flow interpretations remain unresolved. Operator import lineage
and candidate source mappings remain proposed and non-independent corroboration.

The capture adapter targets RenderDoc 1.43 descriptor/replay APIs. It reads selected
existing-capture events, descriptors, reflection and explicitly selected constant
blocks; resource IDs are capture scoped, constant union bits are preserved, and
CPU/GPU correspondences are never inferred. Binary captures are not copied.
Both application sessions have child-process deadlines and record/byte limits.

`external-prerequisites.json` records Python 3.12.8, absent `pyghidra` and `renderdoc`
Python modules, and no configured `GHIDRA_INSTALL_DIR`. No analyzed original toy
Ghidra project or supported original RenderDoc capture/replay environment was present.
Installed-application native export and capture replay smokes were therefore **not
run**. Eight API-facing tests passed, including real compilation/execution of
`fixtures/native_probe.c`, rename, timeout/mutation detection, typed P-code def-use,
capture selection/reflection and actual stream ingestion. No hand-authored bundle is
reported as application extraction proof. Commands for the missing application
smokes are documented in README.md.

## Verification

Rust commands run in `devenv shell` with Rust/Cargo 1.98.1, opt-in kache 0.26.3,
`CARGO_INCREMENTAL=0`, six build jobs and this worktree's own target directory.
The system Cargo 1.82 is unsuitable for this workspace.

```sh
export TMPDIR=/dev/shm/mudcrab-kg-tests  # after creating this task-owned directory
cargo fmt --all -- --check
cargo test --locked -p mudcrab-knowledge-graph --all-features
cargo clippy -p mudcrab-knowledge-graph --all-targets --all-features -- -D warnings
cargo build --locked -p mudcrab-knowledge-graph --all-features --bin mudcrab-kg --example measure
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --no-fail-fast
cargo test --locked --workspace --doc
cargo test --release -p dummy-content -- --ignored performance --test-threads=1
cargo tree --locked -p mudcrab-knowledge-graph --all-features
```

Graph all-feature verification: 26 tests passed (one store unit, 19 core, one
observation, five source). Workspace Clippy with warnings denied passed. All four
repository release performance tests passed. The companion validator passed all 18 tests. Workspace all-targets: 862 passed,
18 ignored, zero failures. Workspace doctests:
0 passed, 0 ignored, zero failures.
See [TEST_REPORT.json](TEST_REPORT.json) and [DEPENDENCY_REPORT.json](DEPENDENCY_REPORT.json). The initial disk-backed streaming fixture was
stopped after slow durable writes; the exact unchanged test passed in 4.12 seconds
with temporary files on `/dev/shm`. The workspace suite uses that isolated temporary
location. Its first complete rerun exposed the script flag allowlist omission above,
which was corrected using the existing test-only mechanism.
`cargo-nextest` is unavailable; the repository's documented workspace `cargo test`
fallback was used. `cargo-audit` is unavailable; no audit result is claimed.

Named acceptance proofs include `identity_vectors`, `validates_bundle_integrity`,
`atomic_import_and_idempotency`, `rejects_immutable_collision`,
`pinned_context_and_unicode_evidence`, `privacy_closure`, `append_only_review`,
`bounded_search_and_cursor`, `bounded_adjacency_and_trace`, `bounded_packet`,
`packet_truncation_keeps_complete_evidence_and_endpoint_closure`,
`proposal_review_conflict_and_idempotency`,
`fresh_public_export_has_only_permitted_closure`,
`streaming_fingerprint_matches_canonical_bundle`,
`read_snapshot_keeps_review_and_generation_consistent`,
`actual_checkout_index`, `scip_reference_is_not_call`, `structural_languages`,
`unicode_scip_positions`, `incremental_source_history_and_configuration_invalidation`,
`loaded_module_code_and_session_scoped_heap`, and
`mcp_same_service_and_rejected_escalation`.
Converter integration proofs: `real_nif_material_roundtrip`, `fallback_is_not_raw`,
`parser_retains_original_texture_spelling`, plus the real pipeline/material variants.
The companion validator's contract examples remain contract tests, separate from
actual source/material/adapter integration.

## Measured workload

Deterministic original 100,000-node / 500,000-assertion degree-skew graph, including
a 100,000-edge hub. Machine: 12 vCPUs reporting Intel Xeon X5675 @ 3.07 GHz,
Linux 6.6.94 x86_64 in KVM. Development graph build, workspace dependencies optimized.
Each query run starts a fresh process, records its first access and 20 warm repeats;
OS file cache is not flushed, and other local/workspace work can overlap.

| Operation | First access | Warm p95 | Target | Result |
|---|---:|---:|---:|---|
| Exact lookup | 1.097 ms | 0.633 ms | <100 ms | pass |
| Broad top-20 lexical search | 959.247 ms | 951.901 ms | <300 ms | missed |
| Three-hop trace | 359.825 ms | 344.228 ms | <1 s | pass |
| 64 KiB packet | 364.097 ms | 368.445 ms | <2 s | pass |

Query peak RSS: 15,480 KiB. Trace/packet results honestly report traversal/byte
truncation. Packet size was 65,362 bytes, including its full envelope.
Compared with the first run, trace fell from 4,617 ms to 344 ms and packet from
4,319 ms to 368 ms. Ordered endpoint/claim indexes avoid high-degree sorts;
packet trimming removes batches rather than repeatedly serializing after each item.
Search fell from 2,119 ms to 952 ms by fetching full records after result selection.
Its remaining cost is enumerating/ranking 100,000 matching rows with deterministic
exact-match and ID ordering. Next tuning should measure FTS ranked candidate scans
and tie handling with context/policy filters retained. No remote graph server is
needed to establish that diagnosis. No sub-300 ms search claim is made.

The original import took 361.07 s and peaked at 3,489,608 KiB; that memory target
was missed because validation/publication duplicated whole bundles. The final
record-validation/streamed-fingerprint import took 367.709 s inside ingestion
(375.211 s including workload construction and context setup) and peaked at
915,356 KiB, about 0.87 GiB, meeting the <1 GiB target. Time did not improve; memory
use did. `performance-build-v2.json`, `query-resources-final.json` and
`performance-100k-500k-v2/measurements.json` retain raw measurements.

## Scope limits

Source fallback reports syntax and lexical containment, not resolved Rust dispatch,
complete calls, ABI layouts or compiled WGSL reflection. No SCIP producer, community
checkout, proprietary executable/asset, debugger session or real capture was acquired
or run automatically. Runtime imports validate producer evidence; they do not attach
a debugger. Reviewed-note authorization is local operator attribution, not user
identity authentication. SQLite row filters are not OS isolation or legal permission.
Real inputs default restricted and remain outside Git/public export. Candidate
relationships need independent evidence and explicit review. Complete capability
status covers a declared selection, never the entire original engine.
