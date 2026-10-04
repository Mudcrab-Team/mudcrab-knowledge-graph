> Historical record from the Mudcrab integration worktree. Current standalone checks are documented in [verification](../../docs/verification.md).

# Baseline reconciliation

Original main fast-forwarded with `git pull --ff-only origin main` from 1ad25cd to
b2758e522dfbf273f641e9c787e6cbbcd019495c. Reviewed and implementation base commits
are identical; `git diff` is empty. Original untracked `.pr-reviews/` preserved.
Implementation branch: feature/research-knowledge-graph, isolated worktree.

Read CONTRIBUTING.md, LEGAL.md, docs/AI_POLICY.md, contribution workflow and spec
index. No in-repository AGENTS.md found; the operator workspace AGENTS.md applies. Read-only
issue/PR searches for knowledge graph found no overlap. No remote publication.

Anchors verified: inspect_nif_materials, build_nif_material_contract,
ValidatedNifMaterial, lighting_texture_slots, publish_material and conditional
OPEN_SKYRIM_material publication. Converter schema 16, world schema 5, cell cache 3,
minimum converter schema 15; Bevy 0.19.0, rusqlite 0.32, Rust 1.98.1.
Streaming loads GltfAssetLabel::Scene; terrain/water extended material references
are source structure, not evidence of a static custom-extension consumer.

## Verification contract

KG-01: identity_vectors, rejects_noncanonical_values, validates_bundle_integrity.
KG-02: atomic_import_and_idempotency, rejects_immutable_collision.
KG-03: pinned_context_and_unicode_evidence, privacy_closure, append_only_review.
KG-04: bounded_search_and_cursor, bounded_adjacency_and_trace.
KG-05/06: actual_checkout_index, scip_reference_is_not_call, structural_languages.
KG-07/08/09: real_nif_material_roundtrip, fallback_is_not_raw,
parser_retains_original_texture_spelling; smoke_materials.py and material_variants.
KG-10/11: adapter tests exercise API-facing extraction functions and bounded errors;
actual tool smokes run only with available installations and original toy inputs.
KG-12/13/14: bounded_packet, proposal_review_conflict_and_idempotency,
mcp_same_service_and_rejected_escalation + smoke_mcp.py,
fresh_public_export_has_only_permitted_closure,
incremental_source_history_and_configuration_invalidation,
packet_truncation_keeps_complete_evidence_and_endpoint_closure,
read_snapshot_keeps_review_and_generation_consistent,
streaming_fingerprint_matches_canonical_bundle + smoke_handoff.py.
KG-15: package/workspace fmt, Clippy, nextest, dependency tree and measured workload.

Invariants and public interfaces are SPEC §§4–13 and IDENTITY/WIRE_PROTOCOL.
The supplied task graph owns progress; root project SPEC.md is unchanged.
