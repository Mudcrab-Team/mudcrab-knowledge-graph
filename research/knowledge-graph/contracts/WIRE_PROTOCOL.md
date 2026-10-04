# Wire and command details, v1

This companion completes SPEC.md. These interfaces are to be implemented, not existing commands.

## Normalized bundles and streaming

A full bundle is one UTF-8 JSON document validated by `bundle.schema.json`. A streamed bundle is a directory with `manifest.json`, validated by `stream.schema.json`, and exactly five UTF-8 NDJSON files: `artifacts.jsonl`, `evidence.jsonl`, `nodes.jsonl`, `assertions.jsonl`, and `diagnostics.jsonl`.

The manifest carries the same source and snapshot objects as the full bundle, `format="mudcrab-kg-stream"`, `schema_version=1`, and one descriptor per file. Each descriptor has `kind`, portable relative `path`, lowercase SHA-256 over the exact file bytes, `byte_size`, and `record_count`. Empty files are legal and have count zero plus SHA-256 of the empty string. No compressed streams in v1. The files must be ordinary files below the bundle root, not symlinks.

Each nonempty line is exactly one JSON record with the corresponding full-bundle item schema. Do not skip blank/malformed lines silently. A terminal LF is required for each record. Diagnostics use `{code,severity,message,source_selector,capability}` where severity is `info|warning|error`, source_selector is a canonical tagged value, and strings are bounded by record limits. A producer error cannot be erased by dropping its diagnostic. Publication rules in SPEC.md still decide whether the selection is usable/partial or failed.

Read manifests first, reject undeclared files when they are offered as records, verify hashes/counts, stream into staging, validate dependency and evidence closure, then publish atomically. Never construct the snapshot ID from its own normalized export. Use the upstream input manifest and extraction configuration defined in IDENTITY.md. JSON and streaming representations of the same extraction must produce the same identities and stored records.

Capability counts are `inputs_enumerated`, `inputs_attempted`, `records_emitted`, `inputs_failed`, and `inputs_skipped`. Record counts refer to normalized artifact/evidence/node/assertion records, not the input denominator. Unknown counts are null, never fabricated zero. The detail field defines the input unit and skipped reasons. Complete means the declared selection, not complete original-engine knowledge.

## Policy profiles and context manifests

A context manifest is `{schema_version:1,name,snapshot_ids,policy,notes?}`. Resolve and store the full pinned transitive dependency closure before creating its identity. Reject duplicate IDs and unavailable dependencies. Alias `name` is not identity.

`public` maps to visibility ceiling `public`; `implementation` maps to ceiling `reviewed`; `research-local` maps to ceiling `restricted`. Visibility is not redistribution permission. The implementation profile requires the reviewed-note/source policy described in SPEC.md. Store the profile, resolved ceiling and complete query-policy configuration separately; the context ID hashes the ceiling and policy configuration. Source-only is an input capability mode, not a fourth visibility value.

The server can lower its context's policy but never raise it. Effective context identity in responses includes this lower policy configuration. `serve --policy research-local` requires `--acknowledge-client-export` as an explicit operator acknowledgement; never accept this acknowledgement from a tool-call argument. Default serve profile is public. An implementation deployment with stronger isolation uses a physically filtered database.

## Query inputs and envelopes

Each MCP method uses an object with additional properties rejected. IDs use full IDENTITY.md forms. Unknown predicate/kind enums fail, rather than being evaluated as query text. Optional `limit` defaults to 20 and is 1–100; evidence max_bytes defaults to 8192 and is 1–65536; context max_bytes defaults to 65536 and is 1024–262144. Search query is 1–4096 UTF-8 bytes. Lists of sources, kinds, predicates, or seeds are bounded to 32 entries; lists must not contain duplicates. The immutable server context is not caller-selectable.

- `kg.search`: `{query,kinds?,sources?,limit?,cursor?}`.
- `kg.node`: `{node_id}`.
- `kg.neighbors`: `{node_id,direction?,predicates?,limit?,cursor?}`; direction defaults out, enum in/out/both.
- `kg.trace`: `{mode,seed,include_hypotheses?,limit?}`; mode material/concept, seed is an asset node ID or exact concept key respectively. Default hypotheses=false.
- `kg.evidence`: `{evidence_id,max_bytes?}`. The client cannot provide a path or URI.
- `kg.context`: `{seeds,max_bytes?,include_hypotheses?}`; each seed is tagged `{kind:"node"|"concept",value}`. Default hypotheses=true but hypotheses remain in their own section, never admitted paths.
- `kg.coverage`: `{sources?,capabilities?}`.

Use the shared result envelope in SPEC.md §12.2. Byte limits include the complete serialized UTF-8 response, not only excerpts. If fixed envelope/evidence metadata cannot fit the requested limit, return BUDGET_TOO_SMALL. All dropped results/excerpts are reflected in truncation metadata. Resolve duplicate IDs, allowed-source filters, admission state, and profile restrictions before computing visible counts.

The selected initial-state and review cursor must be reported in query warnings/metadata when review decisions changed after a cursor was issued. Pin cursor behavior to store generation. A cursor is opaque validated data, not SQL. A missing or policy-filtered ID returns a generic unavailable response to low-visibility clients; do not confirm hidden object existence.

## Proposal and review inputs

Proposal input is `{schema_version:1,context_id,proposals:[...]}`. Each proposal includes a local proposal_key, subject and object node IDs, an ontology predicate, explanation, supporting evidence IDs, missing_evidence descriptions, and producer `{kind,name,version?}`. Require at least one supporting evidence record; an original analyst note may support a hypothesis as a note, not as original-game proof.

The importer creates an immutable notes/proposal snapshot depending on the pinned context snapshots; assertions are `hypothesis/proposed`. It resolves no unqualified function names and executes no suggested commands. Unresolved selectors belong in the proposal explanation and diagnostics, not dangling admitted edges. Reimporting identical normalized proposals is idempotent.

Review takes a valid assertion ID, actor, decision and reason. Allowed decisions are admitted/rejected/withdrawn/proposed. Nonempty actor/reason are mandatory. Store an append-only sequence and timestamp; output the review sequence. Actor text is attribution, not cryptographic authentication. Only an operator-approved CLI path can perform review; MCP has no write tool.
