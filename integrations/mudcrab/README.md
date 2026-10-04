# Companion Mudcrab material integration

The graph imports converter inspection and actual published GLB fields. It does not contain a NIF parser, link the converter, or convert assets itself. This directory supplies the read-only integration and original procedural fixture as [material-export.patch](material-export.patch).

The patch is based on Mudcrab **`b2758e522dfbf273f641e9c787e6cbbcd019495c`**. It adds `kg-export`, a bounded raw-inspection DTO using the vendored parser and existing `inspect_nif_materials`, preservation of original texture spelling, extraction tests and an original two-shape fixture. The Cargo.lock change adds only the converter's dependency on its already-locked Clap version. It excludes the graph package and engine changes.

## Apply in an isolated companion worktree

Use an existing authorized Mudcrab checkout containing the reviewed commit. Do not apply the patch to a dirty main checkout. These commands do not acquire game files.

```sh
KG_CHECKOUT="$PWD"  # run from the standalone KG checkout
MUDCRAB_CHECKOUT=/absolute/path/to/existing/mudcrab
MUDCRAB_KG_WORKTREE=/absolute/path/to/new/mudcrab-kg-integration
git -C "$MUDCRAB_CHECKOUT" worktree add "$MUDCRAB_KG_WORKTREE" \
  -b feature/kg-material-export b2758e522dfbf273f641e9c787e6cbbcd019495c
git -C "$MUDCRAB_KG_WORKTREE" apply --check "$KG_CHECKOUT/integrations/mudcrab/material-export.patch"
git -C "$MUDCRAB_KG_WORKTREE" apply "$KG_CHECKOUT/integrations/mudcrab/material-export.patch"
```

In that companion's supported development shell (`devenv shell` on NixOS), run:

```sh
cargo test --locked -p converter --test kg_materials
cargo build --locked -p converter --bin converter --bin kg-export
cargo build --locked -p dummy-content --example kg_material_fixture
```

The companion retains Mudcrab's native dependency requirements. Optional `RUSTC_WRAPPER=kache` can share compiler-cache entries; each checkout must keep its own target. The graph build does not require these converter dependencies.

From the KG checkout after building its binary:

```sh
cargo build --locked --features mcp --bin mudcrab-kg
python3 scripts/knowledge-graph/smoke_materials.py \
  --mudcrab-repo "$MUDCRAB_KG_WORKTREE" --out /new/external/material-smoke
```

Use `--companion-target` only if the companion intentionally has a different target location. This smoke generates original NIF/DDS/ESM inputs, executes the real converter pipeline, exports actual input/output/manifest bytes, and imports them with reviewed source. Source indexing uses the reviewed material implementation revision; inspection output retains that **operator-declared** revision attribution. The patch adds inspection and fixture code, not a different material-conversion implementation.

Checks cover raw/interpreted emission bits, original shape identities, published `doubleSided`, texture sRGB/linear aliases, pruning, proposed source-concept mappings, and shape-filtered traces. Reordered and missing/empty-extension copies test joins and unmatched-manifest lineage; they are separate transformed test outputs. Duplicate published shape identity is rejected. No native or capture mapping is inferred.

Outputs include `result.json`, `pipeline-report.json`, `q01.json`, `q02.json`, `q03.json`, shape-specific traces and material-variant results. They stay outside Git. Supplied synthetic bundle examples are contract fixtures and are not a replacement for this pipeline.

## Inspect your own selected local files

```sh
/path/to/kg-export --input-root /external/input --converted-root /external/converted \
  --selection /external/selection.json --out /new/external/inspection \
  --converter-revision FULL_MATERIAL_IMPLEMENTATION_REVISION
/path/to/mudcrab-kg --root /external/store ingest materials \
  --dir /new/external/inspection --source local-materials \
  --source-snapshot FULL_SOURCE_SNAPSHOT_ID
```

Selection format:

```json
{"schema_version":1,"inputs":[{"nif":"meshes/selected.nif","glb":"meshes/selected.glb"}],"manifest":"conversion-manifest.json"}
```

Paths are relative allowlisted inputs; traversal and symlink components are rejected. Output must be new. Real inputs default restricted. `--synthetic` declares original redistributable test inputs and must not be used for game assets. The exporter is read-only and does not reconvert or change source/output files.

Raw decoded fields, resilient fallback diagnostics, interpreted contracts and observed GLB fields stay separate. Manifest hash matches support input/output lineage; unverified operator pairings remain manual/proposed. An absent `OPEN_SKYRIM_material` extension does not establish original-engine behavior or a runtime consumer. Source links need matching pinned revision; unresolved consumption stays unresolved.

## Licensing

Original converter and fixture changes retain Mudcrab's MIT OR Apache-2.0 licensing. The patch section for `vendor/project-wormhole-nif/src/bs/shader_texture_set.rs` retains **GPL-3.0-or-later**, as declared by [project-wormhole-nif 0.1.0](https://crates.io/crates/project-wormhole-nif/0.1.0). [The GPL text](LICENSE-GPL-3.0) accompanies it. Applying/building that component retains its license obligations. The independent graph neither links nor depends on it; see [NOTICE](../../NOTICE).
