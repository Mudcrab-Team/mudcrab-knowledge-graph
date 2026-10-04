# Contracts, fixtures and implementation records

Start with the root [README](../../README.md) for installation and [research workflow](../../docs/research-workflow.md) for real source indexing.

| Contract | Responsibility |
|---|---|
| [IDENTITY.md](contracts/IDENTITY.md) | Canonical JSON, framed IDs, revision/address/resource identity |
| [bundle.schema.json](contracts/bundle.schema.json) | Single-file bundle records and locators |
| [stream.schema.json](contracts/stream.schema.json) | Exact-hashed five-file normalized stream |
| [ontology.json](contracts/ontology.json) | Node/predicate kinds and endpoint restrictions |
| [storage.sql](contracts/storage.sql) | Reviewed baseline storage schema |
| [WIRE_PROTOCOL.md](contracts/WIRE_PROTOCOL.md) | Context, query, MCP, proposal and review interfaces |

[SPEC.md](../../SPEC.md) is the reviewed normative KG specification. [IMPLEMENTATION_TASKS.json](IMPLEMENTATION_TASKS.json) records KG-00–15/M0–M6; [AGENT_HANDOFF.md](AGENT_HANDOFF.md) preserves the original implementation brief. Current agent editing instructions are in [AGENTS.md](../../AGENTS.md).

The examples are **hand-authored synthetic contract fixtures**, clearly labeled in their scope. They test identity, locators, ontology and privacy validation. They are not evidence that a native/capture exporter ran. Actual extraction uses [source](../../scripts/knowledge-graph/smoke_handoff.py), [material](../../scripts/knowledge-graph/smoke_materials.py) and [MCP](../../scripts/knowledge-graph/smoke_mcp.py) smokes; installed-application limits are described in [verification](../../docs/verification.md).

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-dev.txt
python research/knowledge-graph/tests/validate_spec.py
```

The validator runs 18 contract checks and local documentation-link validation. An optional `--report /external/report.json` writes its results outside the tracked tree. It does not run Cargo or either analyzer.

[BASELINE.md](BASELINE.md), [REPO_REVIEW.json](REPO_REVIEW.json), [IMPLEMENTATION_REPORT.md](IMPLEMENTATION_REPORT.md), [DEPENDENCY_REPORT.json](DEPENDENCY_REPORT.json) and [TEST_REPORT.json](TEST_REPORT.json) are **historical Mudcrab integration records**, not a claim about this standalone checkout's CI. See [provenance](../../docs/provenance.md) for extraction and [verification](../../docs/verification.md) for current portable checks.
