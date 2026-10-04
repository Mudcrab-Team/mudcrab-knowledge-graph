# Source provenance

The independent package, contracts, original fixture source, Python exporters and acceptance tests were extracted from a locally verified Mudcrab implementation tree. The standalone repository starts a new history; Mudcrab's history, engine, renderer, assets and research databases are not copied.

- Reviewed/base Mudcrab commit: `b2758e522dfbf273f641e9c787e6cbbcd019495c`.
- Local completed implementation tree: `65a77c3e11bbb5db5cc09c9cb35bb9357f8c2c7e` (not published to Mudcrab by this extraction).
- Source URL: [Mudcrab-Team/mudcrab](https://github.com/Mudcrab-Team/mudcrab).
- Original supplied handoff: `Mudcrab_KG_Implementation_Spec.zip`; contracts and KG-00–15 task records are retained under `research/knowledge-graph/`.
- Companion material integration SHA-256: `840c7b0d7406507588a2a2708ecb27ddb89961a08e72b27eeabc8a718ecf6a3b`.

The standalone workspace retains the package layout and its schema/wire names. Cargo.lock is pruned to the graph's dependency closure; inherited version/edition/database/serialization dependencies and existing locked versions are retained. Rust remains 1.98.1. No engine/converter/analyzer dependency is added.

Portability changes replace the source test's hardcoded enclosing Mudcrab checkout with an actual original temporary Git repository, make source smokes accept explicit checkout/revision/query arguments, and make material smokes require an explicit companion checkout. The ordinary smoke indexes this repository's actual committed source. External Mudcrab extraction remains a separate real check. Python validation can write optional reports outside Git and checks the standalone documentation tree.

Original licenses and attribution are retained. [NOTICE](../NOTICE) separates dual-licensed graph code from the GPL parser component in the [companion patch](../integrations/mudcrab/README.md). Contract fixture bundles are labeled synthetic. No generated assets, captures, native decompilation, binaries, local databases, credentials, caches, sidecars, or machine-local logs are copied into the publication tree.

[Historical implementation results](../research/knowledge-graph/IMPLEMENTATION_REPORT.md) preserve the reviewed baseline, actual material/source results and measured limitations. [Current verification](verification.md) describes the standalone proof. These are separate scopes.
