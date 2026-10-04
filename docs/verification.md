# Verification and limits

## Portable standalone checks

[The CI workflow](../.github/workflows/verify.yml) runs [scripts/verify.sh](../scripts/verify.sh) with Rust 1.98.1, Python 3.12 and the pinned contract-validator dependency. It uses no game files, external analyzer installations, GPU, model API or private data. Dependencies are Cargo.lock-pinned.

The script checks formatting, strict Clippy, default and all-feature graph tests, doctests, 18 companion contract tests (including local documentation links), eight Python adapter tests with actual normalized-stream import, and real committed-source/history/privacy/MCP smokes. Source tests use original temporary Git repositories. API tests use explicit doubles; the original C fixture is actually compiled and executed. MCP performs a real stdio handshake and checks seven read-only tools, exact CLI equivalence and four escalation rejections.

Current standalone results will be recorded after the first complete local run. The repository is published only after that verifier passes.

## Companion extraction evidence

Before standalone extraction, the verified Mudcrab integration tree passed 862 workspace tests (18 ignored), strict workspace Clippy, 26 all-feature graph tests, 18 contract tests, eight Python API tests and four release dummy-content performance checks. These historical results apply to that tree, not to this smaller workspace. `cargo-nextest` and `cargo-audit` were unavailable; no audit result is claimed.

Actual reviewed Mudcrab source indexing produced 13,558 nodes and 13,353 assertions; unchanged reimport was idempotent. The original two-shape NIF/DDS/ESM fixture ran through the real converter, then `kg-export`, then graph import. Selected source yielded 678 nodes/673 assertions; material import yielded 61 nodes/99 assertions. Shape joins, raw/interpreted clamps, aliases, pruning, reordered output, absent/empty extensions and duplicate-shape rejection were checked. These are extraction results, not the hand-authored contract bundles.

Reproduce actual Mudcrab indexing with an explicit authorized checkout:

```sh
python3 scripts/knowledge-graph/smoke_handoff.py \
  --repo /absolute/path/to/mudcrab \
  --revision b2758e522dfbf273f641e9c787e6cbbcd019495c \
  --source mudcrab --origin https://github.com/Mudcrab-Team/mudcrab \
  --query ValidatedNifMaterial --out /new/external/mudcrab-source-smoke
```

[The material integration guide](../integrations/mudcrab/README.md) reproduces actual generated assets/conversion without distributing them. Historical local artifacts are not checked in.

## External applications

Ghidra 11.4/PyGhidra 2.2 and RenderDoc 1.43 are the targeted APIs. Their Python modules, an analyzed original toy project and a supported original capture/replay environment were unavailable during implementation. Installed-application native export and capture replay smokes were **not run**. API doubles and stream ingestion do not prove installed-application compatibility. [The adapter guide](adapters.md) gives exact prerequisites and commands.

## Measured workload

Historical deterministic 100,000-node/500,000-assertion graph, including a 100,000-edge hub, on a 12-vCPU Xeon X5675 Linux/KVM environment. Development build, optimized dependencies; fresh query process followed by 20 warm repeats; OS cache not flushed. Other local work could overlap.

| Operation | Warm p95 | Target | Outcome |
|---|---:|---:|---|
| Exact lookup | 0.633 ms | <100 ms | met |
| Broad top-20 lexical search | 951.901 ms | <300 ms | missed |
| Three-hop trace | 344.228 ms | <1 s | met |
| 64 KiB packet | 368.445 ms | <2 s | met |

Import took about 367.7 seconds and peaked at 915,356 KiB (~0.87 GiB), meeting the <1 GiB memory target. Query peak RSS was 15,480 KiB. Packet/traversal truncation was reported. These are historical machine-specific measurements, not a current CI benchmark or game performance claim.

## Scope boundaries

Source syntax does not establish dispatch, ABI layout or compiled shader reflection. Generic SCIP references do not establish direct calls. Captured/runtime observations do not prove complete original control flow or CPU/GPU identity. Source/native mappings retain origin and stay proposed until explicit review. Review actors are plain attribution, not authenticated identities. SQLite policy filters are not OS isolation or legal clearance. Coverage describes declared selections. No complete original-game graph or game-level parity is claimed.
