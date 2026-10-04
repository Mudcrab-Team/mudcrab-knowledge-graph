# Identity and normalization contract, v1

This is the exact wire identity algorithm for SPEC.md. IDs are not mutable database row numbers.

## Hash function

`H(kind, part_1, ..., part_n)` starts with the byte sequence `MUDCRAB-KG-ID-v1` followed by one NUL byte. For `kind` and then each part in order, append its UTF-8 byte length as an unsigned 64-bit **big-endian** integer, followed by those bytes. SHA-256 the result. Return `kind + "_" + lowercase_hex_digest`.

Parts are strings, not implicit language stringifications of objects. Reject unpaired Unicode surrogates; preserve Unicode spelling rather than normalizing it silently. Embedded NUL in an abstract hash component is permitted and collision-safe; file paths containing NUL are forbidden. An exact reference implementation and test vectors are in `tests/validate_spec.py` and `examples/identity-vectors.json`.

## Canonical payload fingerprints

`C(x)` is UTF-8 JSON without insignificant whitespace, with object keys ordered by Unicode scalar value, array order preserved, and only mandatory string escapes (quote/backslash/control characters). Use the short escapes for backspace, tab, LF, FF and CR; other control characters use lowercase `\u00xx`. Do not ASCII-escape non-ASCII characters or escape `/`.

Identity payloads permit null, bool, valid Unicode strings, arrays, objects and integers in `[-9007199254740991,9007199254740991]`. They do **not** permit binary floats, NaN, Infinity, negative-zero numbers, or larger numeric literals. Use tagged bit patterns / decimal strings for those values. `D(x) = lowercase SHA-256(C(x))`. This restricted canonical form intentionally avoids cross-language floating-point formatting differences.

## Components

- Snapshot: `H("snap", source_id, revision, content_sha256, extractor_tool, extractor_version, configuration_sha256, scope_sha256, D(sorted_dependency_snapshot_ids))`.
- Artifact: `H("art", snapshot_id, portable_relative_path, artifact_sha256)`.
- Node: `H("node", snapshot_id, node_kind, logical_key)`.
- Evidence: `H("ev", snapshot_id, artifact_id, D(locator), method, provenance_class)`.
- Assertion: `H("as", owning_snapshot_id, subject_node_id, predicate, object_node_id, D(attributes), D(sorted_evidence_links), provenance_class)`.
- Context: `H("ctx", D(sorted_snapshot_ids), visibility_ceiling, D(query_policy_config))`. Human-readable context aliases are not identity.

An evidence link is `{ "id": "ev_...", "role": "supports|contradicts|context" }`; sort by `(id, role)` and deduplicate exact pairs before hashing. Dependency and context lists are sorted lexicographically and deduplicated. Duplicate input lists should produce an input diagnostic, not different semantics.

`scope_sha256` must equal `D(scope)`. The configuration digest covers actual extraction configuration including build target/features, external index/protocol/grammar versions and dependency locks. A source snapshot's content fingerprint is its exact input file hash or a hash of a sorted manifest of `(path, content hash)` entries. A Git blob/tree SHA-1 is not a SHA-256; record it separately. Mutable run timestamps, absolute paths, elapsed times and display aliases must not affect identity.

Visibility, display labels and initial review state do not appear in the ID formula. They are immutable properties of the identified snapshot record: a conflicting re-import is rejected. Changing interpretation/labels requires a new snapshot fingerprint; changing a review decision adds a review event, not an altered assertion. A policy restriction can be strengthened by a separate local policy record, never silently weakened.

The referenced input fingerprint and extractor/config/scope/dependency digests must uniquely describe extraction. Do not compute a snapshot fingerprint from an export containing its own snapshot ID: that creates a circular definition.

## Validation beyond JSON Schema

The importer must verify IDs, source/snapshot consistency, all declared dependencies, artifact content hashes when accessible, evidence locators, end >= start, source spans against bytes, all references, ontology domains/ranges, and visibility propagation. Each node has evidence; every assertion has at least one **supporting** evidence link. Contextual or contradictory links alone are insufficient to admit an assertion.

A bundle may reference previously imported node/evidence IDs only from declared dependency snapshots (transitively resolved and pinned). References to arbitrary existing snapshots are rejected. Evidence artifacts belong to the evidence's owning snapshot. Cross-source assertion ownership belongs to the extractor/notes snapshot that introduced the relation, not whichever endpoint happens to be older.

Within one import, validate the entire reference closure before publication. Never use JSON Schema validation as a substitute for these checks.
