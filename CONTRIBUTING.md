# Contributing

Changes should make research reproducible and preserve the distinction between evidence and hypotheses. Read [AGENTS.md](AGENTS.md), the [architecture](docs/architecture.md), and the contracts for the layer you change.

## Local development

Install Rust 1.98.1 through rustup, Git, Python 3.11+, and a C compiler. SQLite is built from the Cargo dependency; no running database, game, graphics stack or analyzer is needed. Use a feature worktree and keep its `target/` separate. On NixOS, enter a compiler-capable development shell before running Cargo. `RUSTC_WRAPPER=kache` is optional.

```sh
git worktree add ../kg-my-change -b feature/my-change
cd ../kg-my-change
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-dev.txt
bash scripts/verify.sh
```

For a focused Rust change, use `cargo test --locked -p mudcrab-knowledge-graph --all-features TEST_NAME`, then run the verifier before publication. CI runs the same verifier on Ubuntu. Python API integration tests require the built binary via `MUDCRAB_KG_BIN`; the verifier sets it.

`cargo doc --locked --no-deps --all-features` builds library documentation. `cargo run --locked --example measure -- build /new/external/workload` and then `query /that/workload` reproduce the deterministic 100k-node/500k-assertion benchmark. This is a large, optional local workload; do not run it for every small change or publish research data as CI artifacts.

## Changing contracts

Keep schema/wire identifiers compatible unless the change deliberately versions the interface. Update identity vectors, validators, readers and named acceptance tests together. Add schema migrations instead of rewriting published migrations. Preserve old contexts and snapshots. Changes to truncation, review or visibility must include meaningful evidence-closure and policy tests.

Companion conversion changes belong in Mudcrab. Update and verify [the integration patch](integrations/mudcrab/README.md) against its declared base; do not add converter dependencies to this workspace. An indexed project's source build is a separate operator action.

## Submitting work

Search existing issues and PRs for overlap. Describe the concrete change, relevant tests and any unresolved application prerequisites. Keep claims within the selection actually tested. A human owns the submitted contribution.

Assisted commits carry `Assisted-by: <tool>`. An assisted PR identifies the tool in one line. Agents need explicit authorization for each remote publication or message; a local implementation request alone does not authorize posting. Never upload game assets, captures, decompilation, local graph stores, secrets or local diagnostic logs. Use original fixture source and generate disposable outputs outside Git.

Contributions to the original graph code are licensed under MIT OR Apache-2.0. Preserve third-party notices and component licenses; see [NOTICE](NOTICE) and [LEGAL.md](LEGAL.md).
