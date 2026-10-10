# Mudcrab Knowledge Graph

A local research graph for [Mudcrab](https://github.com/Mudcrab-Team/mudcrab). Index source, inspect material conversion, and connect selected native analysis, runtime observations, and GPU captures through versioned evidence.

Every relationship has provenance, citations, and a review state. Unresolved connections stay unresolved. The graph does not establish original-game behavior from matching names or conversion success.

The Rust core uses SQLite/FTS5 and runs without Mudcrab, Bevy, game files, Python, a GPU, analyzer installations, network credentials, or a model API. Python is used by the optional exporters and verification scripts. MCP is an optional, read-only stdio interface.

## Start with actual source

Install [Rust](https://rustup.rs/), Git, Python 3.11+, and a C compiler. The checkout pins Rust 1.98.1. On Debian/Ubuntu, `build-essential` supplies the native compiler needed for bundled SQLite.

```sh
git clone https://github.com/Mudcrab-Team/mudcrab-knowledge-graph.git
cd mudcrab-knowledge-graph
cargo build --locked --features mcp --bin mudcrab-kg

# Choose a new directory outside Git. This indexes this repository's committed source.
KG_DEMO="$(mktemp -d)/source-research"
python3 scripts/knowledge-graph/smoke_handoff.py --out "$KG_DEMO"
target/debug/mudcrab-kg --root "$KG_DEMO/graph" --context source \
  search --query Store --kind CodeSymbol --limit 2
```

The smoke reads real Git objects, reimports unchanged source, retrieves cited source bytes, checks deletion history across two real commits, creates bounded packets, and scans a fresh public export for a restricted test marker. Its generated history/privacy fixtures are original test inputs, separate from the indexed repository. It does not substitute hand-authored graph bundles for extraction.

To install the binary on your PATH:

```sh
cargo install --locked --path crates/knowledge-graph --features mcp
mudcrab-kg --help
```

The CLI emits JSON. Queries require a named or immutable context; importing a snapshot does not silently advance a context. [The research guide](docs/research-workflow.md) shows the manual import, context, citation, and review workflow.

## Guides

| Reader or task | Start here |
|---|---|
| Human researcher | [Research workflow](docs/research-workflow.md) |
| Agent using the graph | [Agent and MCP guide](docs/agents.md) |
| Agent changing this repository | [AGENTS.md](AGENTS.md) |
| Materials and converter integration | [Companion Mudcrab integration](integrations/mudcrab/README.md) |
| Ghidra, RenderDoc, runtime records | [Adapter guide](docs/adapters.md) |
| Lighting-corpus claims import | [Corpus import](docs/corpus-import.md) |
| Data classification and sharing | [Privacy and public export](docs/privacy.md) |
| Developer | [CONTRIBUTING.md](CONTRIBUTING.md), [architecture](docs/architecture.md) |
| Contracts and implementation scope | [SPEC.md](SPEC.md), [contract index](research/knowledge-graph/README.md) |
| Evidence of verification and limits | [Verification](docs/verification.md), [provenance](docs/provenance.md) |

## Supported research

- Source: pinned Git bytes; optional pre-generated SCIP references; Rust, C/C++, Markdown, XML, and WGSL structural indexing.
- Materials: separate decoded input, converter interpretation, and actual published GLB fields; original shape identities, texture roles, color spaces, and pruning.
- Native/capture/runtime: bounded, selected observations with module/address, capture, or session identity. External application prerequisites are explicit.
- Retrieval: search, nodes, neighbors, traces, verified evidence, coverage, bounded context packets, and seven read-only MCP tools.
- Review and sharing: proposed mappings, append-only review, scoped conflicts, immutable history, and fresh policy-filtered public exports.

Structural source indexing does not resolve dispatch, ABI layouts, or all build configurations. Generic SCIP references do not imply direct calls. Installed Ghidra/RenderDoc application smokes remain unverified; API tests use labeled test doubles. Broad search on the measured 100k-node workload exceeded its 300 ms target. See [verification](docs/verification.md) for the measured scope.

No proprietary game assets, captures, decompilation, or working research databases are distributed. Keep those inputs outside Git. Original graph code is **MIT OR Apache-2.0**; the companion parser patch retains its **GPL-3.0-or-later** component. [NOTICE](NOTICE) and [LEGAL.md](LEGAL.md) describe attribution and boundaries.
