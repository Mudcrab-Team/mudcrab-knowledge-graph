# Implementation-agent handoff: Mudcrab Research Knowledge Graph

## Assignment

Implement the system specified in `SPEC.md` in `Mudcrab-Team/mudcrab`. It is a local reverse-engineering knowledge tool connecting **original-game analysis, community knowledge, Mudcrab source, and asset/runtime/GPU observations**. Do not turn the assignment into a parity tracker, generic RAG chatbot, link collection, or engine rewrite.

The reviewed baseline is `b2758e522dfbf273f641e9c787e6cbbcd019495c`. Inspect the actual checkout and reconcile changes before coding. Read current repository instructions, `CONTRIBUTING.md`, `docs/AI_POLICY.md`, relevant contribution workflow, and any `AGENTS.md` files before touching code. Do not assume the earlier `506852...` snapshot or old schema numbers.

## Read in this order

1. `SPEC.md` §§0–3 and §1 repository integration map.
2. `contracts/IDENTITY.md`, `contracts/bundle.schema.json`, `contracts/ontology.json`, `contracts/storage.sql`, `contracts/WIRE_PROTOCOL.md`.
3. The remaining SPEC sections and `IMPLEMENTATION_TASKS.json`.
4. Actual source locations listed in `REPO_REVIEW.json`, especially `material.rs`, `mesh.rs`, `cache.rs`, `shared/src/lib.rs`, `streaming.rs`, and `render.rs`.
5. Synthetic examples and `tests/validate_spec.py`; run this validator to understand the wire identities. These are contract examples, not substitutes for real adapter integration tests.

## Fixed architectural decisions

Create `crates/knowledge-graph` with package `mudcrab-knowledge-graph` and binary `mudcrab-kg`. Use a separate local SQLite/FTS5 database and shared query service. Do not depend on Bevy, the converter, or the engine from the graph core. Keep the MCP interface read-only and fixed to an explicit context/policy at launch.

Index Mudcrab directly from source. Use SCIP where supplied and a labeled structural fallback otherwise. Ghidra is for the original binary; it exports from an existing analyzed program. Community types and names retain their source and revision. Index selected captures and observations, not a guessed CPU-to-GPU correspondence.

Reuse `MeshConverter::inspect_nif_materials` and existing material contracts. Add a bounded raw-inspection DTO and read-only `kg-export` binary in the converter. Keep **raw parsed fields**, **converter interpretations**, and **actual published GLB fields** distinct. Do not duplicate the NIF parser or modify material conversion behavior.

Every relation needs evidence, exact versions, a provenance class, and a review state. Matching names or agent reasoning produce hypotheses, not extracted facts. Query output must identify missing links without declaring unsupported features absent from the engine.

## Execute in order

Follow tasks KG-00 through KG-15. Finish each milestone with a runnable demonstration before adding the next layer. Source-only use must work without Skyrim, Ghidra, RenderDoc, a GPU, network credentials, or a model API.

The first useful slice is double-sided material and texture-slot research: parsed NIF → interpreted contract → published GLB → Mudcrab source handlers, with community/native/capture connections added only where supported. Unresolved runtime extension handling must remain unresolved rather than invented.

## Constraints

- Do not replace the root project SPEC, rename existing OpenSkyrim wire identifiers, upgrade Bevy, change world/converter schemas, or rewrite existing acceptance tools.
- Do not run third-party build scripts, proc macros, aggregate Ghidra tooling, or network acquisition without explicit operator permission.
- Do not redistribute real assets, decompilation, shaders, memory dumps, or captures. Keep private research outside Git and out of public MCP/exports.
- Do not admit your own semantic mapping as established behavior. Store proposals and separate review events.
- Do not report hand-authored JSON import as proof that Ghidra or RenderDoc extraction works. Implement the adapters, add tests, and separately disclose unavailable external-tool smoke tests.
- Do not claim a complete original-game graph. Report exact indexed selection and limitations.

## Required final handoff from you

Return the implementation diff, dependency decisions and baseline reconciliation; passing automated-test commands/results; a source-only quick start; an actual synthetic NIF→GLB research trace; bounded CLI and MCP example output with citations; the Ghidra/RenderDoc toy-input smoke results or specific unavailable prerequisites; and the outstanding scope limitations.

All M0–M6 functionality must be implemented. A collection of interfaces, TODOs, schemas, or mocked adapters is not completion. Missing authorized original-game inputs is not a reason to leave core functionality unfinished. No game-level parity claim is expected.
