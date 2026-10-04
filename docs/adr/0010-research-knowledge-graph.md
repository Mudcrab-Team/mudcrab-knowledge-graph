# 0010: Local research knowledge graph

Accepted for this implementation. Baseline: b2758e522dfbf273f641e9c787e6cbbcd019495c.

Use an independent Rust package with SQLite indexed adjacency and FTS5. Queries share
one service across CLI and optional rmcp stdio. Graph core depends on serialization,
source parsers and database libraries; converter emits a serialization-only DTO and
never depends on the graph. Inputs and contexts are immutable and content-addressed.

SCIP occurrences establish references. Tree-sitter provides lexical declarations when
semantic indexes are absent. Neither mode invents call resolution. Source indexing
reads pinned Git bytes and never runs the indexed repository's build scripts/macros.

Materials retain decoded input, interpreted contract and observed output separately.
Converter inspection reuses the vendored parser and existing material interpretation.
External adapters read prepared Ghidra programs or selected existing captures.

Public export builds a fresh database with permitted evidence closure. Hypotheses and
append-only review retain their origin. No game schema, Bevy version, wire identifier,
conversion behavior or renderer behavior changes.

Dependencies are Cargo.lock-pinned. Required dependency compilation is part of the
implementation build; importing an untrusted repository never invokes its build system.
Optional external-tool smoke failures are recorded separately from contract tests.
