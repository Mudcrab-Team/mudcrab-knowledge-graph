#!/usr/bin/env bash
# Run from a committed checkout. Activate the development venv first.
set -euo pipefail
cd "$(dirname "$0")/.."
verification_dir=$(mktemp -d)
trap 'rm -rf "$verification_dir"' EXIT

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --no-default-features
cargo test --locked --workspace --all-targets --all-features
cargo test --locked --workspace --doc --all-features
cargo build --locked --all-features --bin mudcrab-kg
python3 research/knowledge-graph/tests/validate_spec.py
MUDCRAB_KG_BIN="$PWD/target/debug/mudcrab-kg" \
  python3 -m unittest discover -s scripts/knowledge-graph/tests -v
python3 scripts/knowledge-graph/smoke_handoff.py \
  --out "$verification_dir/source" > "$verification_dir/source-result.json"
python3 scripts/knowledge-graph/smoke_mcp.py \
  --binary "$PWD/target/debug/mudcrab-kg" --root "$verification_dir/source/graph" \
  --context source --out "$verification_dir/mcp.json"
printf 'Standalone verification passed: Rust, contracts, adapters, source/history/privacy, and MCP.\n'
