# Instructions for agents working on Mudcrab Knowledge Graph

Read [README.md](README.md), [CONTRIBUTING.md](CONTRIBUTING.md), [docs/architecture.md](docs/architecture.md), and the relevant contracts before editing. [SPEC.md](SPEC.md) is the reviewed KG specification; its Mudcrab-specific integration requirements apply to the companion checkout, not to adding engine dependencies here. The original implementation handoff in `research/knowledge-graph/AGENT_HANDOFF.md` is historical context.

## Repository map

- `crates/knowledge-graph/src/`: independent Rust library, CLI, optional MCP.
- `crates/knowledge-graph/migrations/`: append-only SQLite schema migrations.
- `crates/knowledge-graph/tests/`: graph, source, runtime, policy, review, query tests.
- `research/knowledge-graph/contracts/`: normative identity, schema, ontology, and wire contracts.
- `research/knowledge-graph/examples/`: labeled synthetic contract fixtures; never extraction proof.
- `scripts/knowledge-graph/`: API exporters, actual source/material/MCP smokes, original C fixture.
- `integrations/mudcrab/`: separately applied converter/raw-inspection integration patch.
- `docs/`: human and agent usage, architecture, provenance, verification, privacy.

## Invariants

1. Preserve contract names, schema versions, identity framing and canonical JSON restrictions. Do not replace content hashes with mutable labels or abbreviated IDs.
2. Imports are atomic and idempotent. Snapshots and contexts are immutable; a context name changes only through explicit context creation. Keep old contexts readable.
3. Each assertion needs evidence, provenance and review state. Name matches, manual source selectors and agent reasoning start proposed. Review events do not rewrite origin.
4. Report `not_indexed`, failed/unsupported extraction, indexed-but-unresolved links, and no match within scope distinctly. None establishes implementation absence.
5. Preserve raw parsed values, interpreted material contracts, and published output separately. Reuse companion extraction; do not implement another NIF parser or alter rendering/conversion behavior.
6. Never infer complete call graphs, ABI facts, CPU-to-GPU identity or original-game parity from structural syntax, generic SCIP references, observation records or matching names.
7. Verify artifact hashes, locator byte boundaries, endpoint/evidence closure, policy, budgets and context before returning claims. Packet truncation must remove incomplete claims.
8. MCP stays read-only, with context/policy fixed at launch. Reject SQL, filesystem paths, unknown arguments and client policy/context escalation.
9. Public export constructs a fresh filtered database with permitted evidence closure. Never copy the working SQLite file, WAL, free pages, parse cache or local root aliases.
10. Do not add Bevy, engine, converter or analyzer installations to graph-core dependencies. Source indexing must not run the indexed repository's build scripts or proc macros.

## Working and verification

Use an isolated feature worktree. Preserve unrelated local files. Keep each worktree's own `target/`; optional `RUSTC_WRAPPER=kache` belongs in the caller environment, not committed Cargo settings. Do not use parallel implementation agents unless the operator requests them.

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-dev.txt
bash scripts/verify.sh
```

The verifier runs formatting, strict Clippy, default/all-feature tests, doctests, contracts, Python API tests with actual stream import, committed-source/history/privacy smoke, and a real rmcp stdio smoke. It removes its temporary artifacts. A test failure requires identifying whether code, contract, or an unspecified edge case caused it before retrying. Add a regression invariant only when the contract needs one. Use [verification](docs/verification.md) to distinguish CI from external application checks.

When changing adapters, preserve deadlines, limits, read-only application access, exact versions, label/type lineage and unresolved states. Run installed-application smokes only against authorized original inputs; report unavailable prerequisites explicitly. API doubles are useful tests, not application compatibility evidence.

## Publication and communication

Keep assets, captures, executables, native decompilation, memory dumps, databases, local aliases, credentials and machine-local logs outside Git. Inspect the staged tree before committing. Original/procedural fixture source belongs in Git; generated binaries and assets do not.

Do not acquire third-party inputs, push, create PRs/issues, post comments, merge or deploy without item-specific operator authorization. Local reversible work and tests do not need repeated permission. Every assisted commit has `Assisted-by: <tool>`; disclose assistance in an authorized PR and state tests actually run.

Write concise, direct prose. Cite the pinned context, source revision, locator and evidence ID when presenting a factual relationship. Separate admitted evidence, hypotheses, missing relationships and external-tool limitations. Do not present a hand-authored bundle as a successful exporter run.
