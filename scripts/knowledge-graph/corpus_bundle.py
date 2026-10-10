#!/usr/bin/env python3
"""Export an accepted Mudcrab lighting-corpus checkpoint to a KG bundle file.

Reads every artifact byte through ``git show REV:path`` in a Mudcrab checkout,
so the export is deterministic for a pinned corpus revision. Accepted notes and
ledgers come from a corpus receipt; staged notes without ledgers are excluded.

Example:
  python3 corpus_bundle.py --repo /path/to/mudcrab \\
      --corpus-rev 3725a2e970d244d481675819015277ea57a149b8 \\
      --receipt 20261010-02 \\
      --source-snapshot snap_... --source-scope indexed-paths.txt \\
      --out /external/corpus-bundle
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from kg_bundle import canonical, digest, identity  # noqa: E402

TOOL = "corpus-bundle"
VERSION = "1.1.0"
SOURCE_ID = "mudcrab-lighting-corpus"
NOTES_ROOT = "docs/research/skyrim-render-map/research-notes"

# Accepted checkpoint 20261010-02: N001-N003, 7 ledgers, 173 claims.
LEDGERS = [
    "note-0001/claims-ck-data.json",
    "note-0002/claims-native-lighting.json",
    "note-0002/claims-imagespace.json",
    "note-0002/claims-mod-interventions.json",
    "note-0003/claims-native-lighting.json",
    "note-0003/claims-imagespace.json",
    "note-0003/claims-mod-interventions.json",
]
SUPPLIED = {
    "N001": ["note-0001/supplied-notes.md"],
    "N002": ["note-0002/supplied-notes.txt", "note-0002/supplied-message.txt"],
    "N003": ["note-0003/supplied-notes.md"],
}
AUDITS = [
    "note-0002/native-lighting-audit.md",
    "note-0002/imagespace-audit.md",
    "note-0002/mod-interventions-audit.md",
    "note-0002/intake-summary.md",
    "note-0002/review-notes.md",
    "note-0003/native-lighting-audit.md",
    "note-0003/imagespace-audit.md",
    "note-0003/mod-interventions-audit.md",
    "note-0003/intake-summary.md",
    "note-0003/review-notes.md",
]
ROOT_FILES = [
    "input-receipt.json",
    "note-0003/input-receipt.json",
    "corpus-receipt-20261010-01.json",
    "corpus-receipt-20261010-02.json",
    "claims-index.json",
    "world-model.json",
    "world-model.md",
    "model-associations.json",
    "README.md",
    "note-0002/source-sections.json",
    "note-0003/source-sections.json",
]
COMPARISON_DOC = "docs/research/skyrim-lighting-notes-comparison-20261010.md"
RENDER_MAP_PREFIX = "docs/research/skyrim-render-map/"
RELATION_PREDICATE = {
    "strengthens": "SUPPORTS",
    "challenges": "CHALLENGES",
    "duplicates": "DUPLICATES",
    "related": "RELATED_TO",
}
RANGE_RE = re.compile(r"^\s*(\d+)\s*(?:-\s*(\d+)\s*)?$")
CLAIM_ID_RE = re.compile(rb'"id"\s*:\s*"(N\d+-[A-Z]+\d+)"')
ASSOC_ID_RE = re.compile(rb'"claim_id"\s*:\s*"(N\d+-[A-Z]+\d+)"')
GENERIC_ID_RE = re.compile(rb'"id"\s*:\s*"([^"]+)"')


class ExportError(Exception):
    pass


def git_show(repo, rev, path):
    proc = subprocess.run(
        ["git", "-C", repo, "show", "%s:%s" % (rev, path)],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if proc.returncode != 0:
        raise ExportError("git show %s:%s failed: %s" % (rev, path, proc.stderr.decode()[:200]))
    return proc.stdout


def git_is_blob(repo, rev, path):
    proc = subprocess.run(
        ["git", "-C", repo, "cat-file", "-t", "%s:%s" % (rev, path)],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False)
    return proc.stdout.strip() == b"blob"


def git_is_tree(repo, rev, path):
    proc = subprocess.run(
        ["git", "-C", repo, "cat-file", "-t", "%s:%s" % (rev, path)],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False)
    return proc.stdout.strip() == b"tree"


def sha256_hex(raw):
    import hashlib
    return hashlib.sha256(raw).hexdigest()


def parse_ranges(value):
    """Parse '3, 13, 32-44' or a single int into [(start, end)] 1-based inclusive."""
    if isinstance(value, int):
        if value < 1:
            raise ExportError("line number must be >= 1: %r" % (value,))
        return [(value, value)]
    if not isinstance(value, str) or not value.strip():
        raise ExportError("empty line range: %r" % (value,))
    out = []
    for part in value.split(","):
        m = RANGE_RE.match(part)
        if not m:
            raise ExportError("unparseable line range %r in %r" % (part, value))
        start = int(m.group(1))
        end = int(m.group(2)) if m.group(2) else start
        if start < 1 or end < start:
            raise ExportError("invalid line range %r in %r" % (part, value))
        out.append((start, end))
    return out


def line_starts(raw):
    starts = [0]
    pos = 0
    while True:
        nxt = raw.find(b"\n", pos)
        if nxt < 0:
            return starts
        starts.append(nxt + 1)
        pos = nxt + 1


def line_span_bytes(raw, start_line, end_line):
    """Byte interval covering 1-based inclusive lines; validates UTF-8 boundaries."""
    starts = line_starts(raw)
    line_count = len(starts) if raw.endswith(b"\n") else len(starts)
    if raw.endswith(b"\n"):
        line_count = len(starts) - 1
    if start_line > line_count or end_line > line_count:
        raise ExportError("line range %d-%d exceeds %d lines" % (start_line, end_line, line_count))
    start = starts[start_line - 1]
    end = starts[end_line] if end_line < len(starts) else len(raw)
    raw[start:end].decode("utf-8", "strict")
    return start, end


def slice_records(raw, pattern, expected_ids):
    """Split a JSON array region into per-record spans by id match order."""
    matches = [(m.group(1).decode(), m.start()) for m in pattern.finditer(raw)]
    found = [mid for mid, _ in matches]
    if found != expected_ids:
        raise ExportError("record id sequence mismatch: found %d expected %d" % (len(found), len(expected_ids)))
    starts = line_starts(raw)
    spans = {}
    for i, (mid, pos) in enumerate(matches):
        line_no = next(n for n in range(len(starts)) if starts[n] > pos) - 1
        start = starts[line_no]
        if i + 1 < len(matches):
            npos = matches[i + 1][1]
            nline = next(n for n in range(len(starts)) if starts[n] > npos) - 1
            end = starts[nline]
        else:
            tail = re.search(rb'\n\s{0,8}\]', raw[pos:])
            end = pos + tail.start() if tail else len(raw)
        spans[mid] = (start, end)
    return spans


def find_excerpt(raw, excerpt):
    needle = excerpt.encode("utf-8")
    pos = raw.find(needle)
    if pos < 0:
        return None
    return (pos, pos + len(needle))


def span_locator(artifact_path, start, end, raw):
    """source_span locator with validator-consistent line numbers."""
    raw[start:end].decode("utf-8", "strict")
    def line_of(pos):
        return 1 + raw[:pos].count(b"\n")
    start_line = line_of(start)
    end_line = line_of(end - 1) if end > start else line_of(end)
    return {"kind": "source_span", "path": artifact_path, "start_byte": start, "end_byte": end,
            "start_line": start_line, "end_line": end_line, "encoding": "utf8_bytes"}


def split_path_lines(value):
    """Split 'path:1-3,5' anchor forms; bare paths pass through unchanged."""
    if ":" not in value:
        return value, None
    head, tail = value.rsplit(":", 1)
    try:
        parse_ranges(tail)
    except ExportError:
        return value, None
    return head, tail


def media_type(path):
    if path.endswith(".json"):
        return "application/json"
    if path.endswith(".md"):
        return "text/markdown"
    if path.endswith((".txt", ".pas", ".h", ".cpp", ".hlsl", ".hlsli", ".xml", ".bib", ".csv")):
        return "text/plain"
    if path.endswith(".html"):
        return "text/html"
    return "application/octet-stream"


class Export:
    def __init__(self):
        self.artifacts = {}
        self.artifact_bytes = {}
        self.evidence = {}
        self.nodes = {}
        self.assertions = {}
        self.warnings = []
        self.skipped = []

    def warn(self, message):
        self.warnings.append(message)

    def skip(self, reason):
        self.skipped.append(reason)


def build_snapshot_id(source_id, revision, content, config, scope, dependencies):
    return identity("snap", source_id, revision, content, TOOL, VERSION,
                     digest(config), digest(scope), digest(sorted(dependencies)))


def main(argv=None):
    args = parse_args(argv)
    repo = args.repo
    rev = args.corpus_rev
    exp = Export()

    def read_repo(path):
        if not git_is_blob(repo, rev, path):
            raise ExportError("not a file at %s:%s" % (rev, path))
        return git_show(repo, rev, path)

    # ---- Tier-1 integrity: receipt-listed bytes must match receipts exactly.
    receipts = {}
    for rel in ["input-receipt.json", "note-0003/input-receipt.json"]:
        raw = read_repo(NOTES_ROOT + "/" + rel)
        receipts[rel] = (raw, json.loads(raw.decode("utf-8")))
    for rel, (raw, receipt) in receipts.items():
        for note in receipt.get("notes", []):
            for art in note.get("artifacts", []):
                got = read_repo(NOTES_ROOT + "/" + art["path"])
                if sha256_hex(got) != art["sha256"] or len(got) != art["bytes"]:
                    raise ExportError("receipt mismatch for %s in %s" % (art["path"], rel))

    # ---- Ledgers, supplied inputs, world files.
    ledgers = {}
    for rel in LEDGERS:
        raw = read_repo(NOTES_ROOT + "/" + rel)
        ledgers[rel] = (raw, json.loads(raw.decode("utf-8")))
    claim_ids = [c["id"] for _, (_, doc) in ledgers.items() for c in doc["claims"]]
    if len(claim_ids) != 173 or len(set(claim_ids)) != 173:
        raise ExportError("expected 173 unique claims, found %d" % len(claim_ids))
    supplied = {}
    for note_id, rels in SUPPLIED.items():
        for rel in rels:
            supplied[rel] = read_repo(NOTES_ROOT + "/" + rel)
    world_raw = read_repo(NOTES_ROOT + "/world-model.json")
    world = json.loads(world_raw.decode("utf-8"))
    assoc_raw = read_repo(NOTES_ROOT + "/model-associations.json")
    assoc = json.loads(assoc_raw.decode("utf-8"))
    index_raw = read_repo(NOTES_ROOT + "/claims-index.json")
    index = json.loads(index_raw.decode("utf-8"))
    quotation = {c["id"]: c.get("source", {}).get("quotation_status") for c in index["claims"]}
    sections = {}
    for rel in ["note-0002/source-sections.json", "note-0003/source-sections.json"]:
        sections[rel] = json.loads(read_repo(NOTES_ROOT + "/" + rel).decode("utf-8"))

    # ---- Anchor normalization across ledger schema variants.
    file_anchors = []  # (claim_id, kind, path, ranges|None, contract_id|None, sha|None, symbol|None, role)
    for rel, (_, doc) in ledgers.items():
        for claim in doc["claims"]:
            cid = claim["id"]
            for row in claim.get("evidence_anchors", []) + claim.get("evidence", []):
                if not isinstance(row, dict):
                    continue
                path = row.get("path") or row.get("file")
                if not path:
                    continue
                path, extra_ranges = split_path_lines(path)
                if extra_ranges and not any(row.get(k) for k in ("lines", "anchors", "line")):
                    row = dict(row, lines=extra_ranges)
                if not path.startswith(("docs/", "crates/", "scripts/", "assets/", "tests/", "data/", "config/")):
                    exp.skip("%s anchor %s: non-workspace path" % (cid, path))
                    continue
                ranges = None
                for key in ("lines", "anchors", "line"):
                    if row.get(key) not in (None, ""):
                        try:
                            ranges = parse_ranges(row[key])
                        except ExportError as exc:
                            exp.warn("%s anchor %s: %s" % (cid, path, exc))
                        break
                file_anchors.append((cid, "evidence", path, ranges, row.get("contract_id"), row.get("sha256"), None))
            impl_rows = list(claim.get("current_implementation", []) or [])
            impl_rows += list(claim.get("implementation", []) or [])
            impl_rows += list(claim.get("implementation_anchors", []) or [])
            for row in impl_rows:
                if not isinstance(row, dict) or not row.get("path"):
                    continue
                ipath, extra_ranges = split_path_lines(row["path"])
                if extra_ranges and not any(row.get(k) for k in ("lines", "line")):
                    row = dict(row, path=ipath, lines=extra_ranges)
                else:
                    row = dict(row, path=ipath)
                ranges = None
                for key in ("lines", "line"):
                    if row.get(key) not in (None, ""):
                        try:
                            ranges = parse_ranges(row[key])
                        except ExportError as exc:
                            exp.warn("%s impl %s: %s" % (cid, row["path"], exc))
                        break
                file_anchors.append((cid, "implementation", row["path"], ranges, None, row.get("sha256"), row.get("symbol")))
            extra = claim.get("current_mudcrab_implementation")
            if isinstance(extra, dict):
                for anchor in extra.get("anchors", []) or []:
                    if not isinstance(anchor, str):
                        continue
                    base, extra_ranges = split_path_lines(anchor)
                    if not re.match(r"^(docs|crates|scripts|assets|tests|data|config)/\S+$", base):
                        continue
                    ranges = parse_ranges(extra_ranges) if extra_ranges else None
                    file_anchors.append((cid, "implementation", base, ranges, None, None, None))

    # ---- Artifact bytes: core corpus files plus every anchored workspace file.
    # Corpus bytes come from git at corpus_rev. Anchored files missing there fall
    # back to --work bytes only when an anchor hash verifies; provenance is
    # recorded in scope so the snapshot fingerprint describes the extraction.
    wanted_core = set()
    for rel in LEDGERS:
        wanted_core.add(NOTES_ROOT + "/" + rel)
    for rels in SUPPLIED.values():
        for rel in rels:
            wanted_core.add(NOTES_ROOT + "/" + rel)
    for rel in AUDITS + ROOT_FILES:
        wanted_core.add(NOTES_ROOT + "/" + rel)
    wanted_core.add(COMPARISON_DOC)
    artifact_bytes = {}
    for path in sorted(wanted_core):
        try:
            artifact_bytes[path] = read_repo(path)
        except ExportError as exc:
            raise ExportError("missing core artifact %s: %s" % (path, exc))
    work_sourced = {}
    anchor_wants = {}
    for cid, role, path, ranges, contract_id, sha, symbol in file_anchors:
        anchor_wants.setdefault(path, []).append(sha)
    for path in sorted(anchor_wants):
        if path in artifact_bytes:
            continue
        raw, origin = None, None
        try:
            raw = read_repo(path)
            origin = "corpus_rev"
        except ExportError:
            pass
        if raw is None and args.work:
            candidate = Path(args.work) / path
            if candidate.is_file():
                raw = candidate.read_bytes()
                origin = "work"
        if raw is None:
            continue
        anchor_shas = {s for s in anchor_wants[path] if s}
        if anchor_shas and sha256_hex(raw) not in anchor_shas:
            if origin == "corpus_rev" and args.work:
                candidate = Path(args.work) / path
                if candidate.is_file() and sha256_hex(candidate.read_bytes()) in anchor_shas:
                    raw = candidate.read_bytes()
                    origin = "work"
                else:
                    continue
            else:
                continue
        artifact_bytes[path] = raw
        if origin == "work":
            work_sourced[path] = sha256_hex(raw)

    # ---- Snapshot identity.
    manifest = sorted([[p, sha256_hex(b)] for p, b in artifact_bytes.items()])
    content = digest(manifest)
    work_head = None
    if args.work:
        proc = subprocess.run(["git", "-C", args.work, "rev-parse", "HEAD"],
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False)
        if proc.returncode == 0:
            work_head = proc.stdout.decode().strip()
    scope = {
        "corpus_receipt": args.receipt,
        "corpus_revision": rev,
        "accepted_notes": ["N001", "N002", "N003"],
        "ledger_claims": len(claim_ids),
        "excluded_derived": ["model.html"],
        "excluded_staged": ["note-0004"],
        "worktree_sourced": work_sourced,
        "worktree_head": work_head,
    }
    config = {
        "exporter": TOOL,
        "exporter_version": VERSION,
        "excerpt_rule": "first byte match in note order; supplied_statement fallback",
        "line_range_grammar": "comma-separated 1-based lines and ranges",
        "source_snapshot": args.source_snapshot,
    }
    dependencies = [args.source_snapshot] if args.source_snapshot else []
    snapshot_id = build_snapshot_id(SOURCE_ID, rev, content, config, scope, dependencies)
    scope_sha = digest(scope)
    config_sha = digest(config)

    source = {
        "id": SOURCE_ID, "kind": "notes",
        "origin_uri": "https://github.com/Mudcrab-Team/mudcrab",
        "visibility": "reviewed", "license_expression": None,
        "lineage_roots": [SOURCE_ID], "redistribution": "review_required",
    }

    def add_artifact(path):
        raw = artifact_bytes[path]
        key = identity("art", snapshot_id, path, sha256_hex(raw))
        exp.artifacts[key] = {"id": key, "path": path, "sha256": sha256_hex(raw),
                              "media_type": media_type(path), "byte_size": len(raw),
                              "visibility": "reviewed", "blob_sha256": None}
        exp.artifact_bytes[path] = raw
        return key

    def add_evidence(artifact_id, locator, method, provenance="extracted"):
        key = identity("ev", snapshot_id, artifact_id, digest(locator), method, provenance)
        exp.evidence[key] = {"id": key, "artifact_id": artifact_id, "locator": locator,
                             "method": method, "provenance_class": provenance, "visibility": "reviewed"}
        return key

    def add_node(kind, logical, name, attrs, evidence):
        key = identity("node", snapshot_id, kind, logical)
        value = {"id": key, "kind": kind, "logical_key": logical, "display_name": name,
                 "attributes": attrs, "evidence_ids": sorted(set(evidence)), "visibility": "reviewed"}
        if key in exp.nodes and exp.nodes[key] != value:
            raise ExportError("conflicting node extraction for %s %s" % (kind, logical))
        exp.nodes[key] = value
        return key

    def add_edge(subject, predicate, obj, attrs, evidence, provenance):
        links = sorted({(x, "supports") for x in evidence})
        link_rows = [{"id": x, "role": r} for x, r in links]
        if not link_rows:
            raise ExportError("supporting evidence required for %s edge" % predicate)
        key = identity("as", snapshot_id, subject, predicate, obj, digest(attrs), digest(link_rows), provenance)
        state = "proposed" if provenance in ("manual", "hypothesis") else "admitted"
        exp.assertions[key] = {"id": key, "subject": subject, "predicate": predicate, "object": obj,
                               "attributes": attrs, "evidence": link_rows,
                               "provenance_class": provenance, "initial_state": state, "visibility": "reviewed"}
        return key

    artifact_ids = {p: add_artifact(p) for p in artifact_bytes}

    # ---- File-level DocumentSections for receipt-verified core files.
    section_nodes = {}
    for path, raw in artifact_bytes.items():
        if path not in wanted_core:
            continue
        if not path.endswith((".md", ".txt", ".json")):
            continue
        try:
            raw.decode("utf-8", "strict")
        except UnicodeDecodeError:
            continue
        ev = add_evidence(artifact_ids[path], span_locator(path, 0, len(raw), raw), "corpus-file-span")
        section_nodes[path] = add_node("DocumentSection", path, path.rsplit("/", 1)[-1],
                                       {"repo_path": path}, [ev])

    # ---- Indexed supplied-note sections with exact spans.
    for rel, doc in sections.items():
        src_rel = doc["source"]["path"]
        src_path = NOTES_ROOT + "/" + src_rel if not src_rel.startswith("docs/") else src_rel
        raw = artifact_bytes.get(src_path)
        if raw is None:
            exp.warn("section map %s source %s not in artifacts" % (rel, src_rel))
            continue
        for row in doc.get("sections", []):
            span = row.get("source_span_bytes", {})
            start, end = span.get("start"), span.get("end_exclusive")
            heading = row.get("heading_verbatim", "")
            if not isinstance(start, int) or not isinstance(end, int) or start < 0 or end > len(raw):
                exp.warn("section %s span outside %s" % (row.get("id"), src_rel))
                continue
            if heading.encode("utf-8") not in raw[start:end]:
                exp.warn("section %s heading not verbatim in span" % row.get("id"))
                continue
            try:
                locator = span_locator(src_path, start, end, raw)
            except (UnicodeDecodeError, ValueError):
                exp.warn("section %s span is not a UTF-8 boundary" % row.get("id"))
                continue
            ev = add_evidence(artifact_ids[src_path], locator, "corpus-section-span")
            key = "%s#%s" % (src_path, row.get("id"))
            section_nodes[key] = (add_node("DocumentSection", key, heading[:120], {"repo_path": src_path}, [ev]), row.get("claim_ids", []), ev)

    # ---- Claims.
    claim_node = {}
    claim_span_ev = {}
    for rel, (raw, doc) in ledgers.items():
        ledger_path = NOTES_ROOT + "/" + rel
        note_id = doc.get("note_id", "")
        domain = doc.get("domain", "")
        expected = [c["id"] for c in doc["claims"]]
        try:
            spans = slice_records(raw, CLAIM_ID_RE, expected)
        except ExportError as exc:
            raise ExportError("%s: %s" % (rel, exc))
        by_id = {c["id"]: c for c in doc["claims"]}
        for cid in expected:
            claim = by_id[cid]
            start, end = spans[cid]
            ledger_ev = add_evidence(artifact_ids[ledger_path], span_locator(ledger_path, start, end, raw), "corpus-ledger-span")
            claim_span_ev[cid] = ledger_ev
            evidence_ids = [ledger_ev]
            excerpt = claim.get("supplied_excerpt_verbatim") or claim.get("supplied_statement", "")
            found = None
            if excerpt:
                for sup_rel in SUPPLIED.get(note_id, []):
                    sup_path = NOTES_ROOT + "/" + sup_rel
                    hit = find_excerpt(artifact_bytes[sup_path], excerpt)
                    if hit is not None:
                        sraw = artifact_bytes[sup_path]
                        try:
                            locator = span_locator(sup_path, hit[0], hit[1], sraw)
                        except (UnicodeDecodeError, ValueError):
                            continue
                        evidence_ids.append(add_evidence(artifact_ids[sup_path], locator, "corpus-excerpt-locate"))
                        found = sup_rel
                        break
            quote_status = "verbatim" if found else "paraphrase"
            indexed = quotation.get(cid)
            if indexed == "audit_paraphrase":
                indexed = "paraphrase"
            statement = (claim.get("supplied_statement", "") or "")
            stmt_found = False
            if statement:
                for sup_rel in SUPPLIED.get(note_id, []):
                    if find_excerpt(artifact_bytes[NOTES_ROOT + "/" + sup_rel], statement) is not None:
                        stmt_found = True
                        break
            if indexed is not None and ("verbatim" if stmt_found else "paraphrase") != indexed:
                exp.warn("%s statement quotation mismatch vs index %s" % (cid, indexed))
            flat_statement = statement.replace("\n", " ").strip()
            claim_node[cid] = add_node("ResearchClaim", cid, "%s: %s" % (cid, flat_statement[:110]),
                                       {"note_id": note_id, "domain": domain,
                                        "assessment": claim.get("assessment", ""),
                                        "scope": claim.get("subject_scope", ""),
                                        "quotation": quote_status}, evidence_ids)
            add_edge(section_nodes[ledger_path], "DOCUMENTS", claim_node[cid], {}, [ledger_ev], "extracted")

    # ---- Supplied-section DOCUMENTS from section claim maps.
    for key, value in list(section_nodes.items()):
        if not isinstance(value, tuple):
            continue
        node_id, claim_ids_map, ev = value
        section_nodes[key] = node_id
        for cid in claim_ids_map or []:
            if cid in claim_node:
                add_edge(node_id, "DOCUMENTS", claim_node[cid], {}, [ev], "extracted")
            else:
                exp.warn("section %s references unknown claim %s" % (key, cid))

    # ---- Inter-claim relations.
    for _, (_, doc) in ledgers.items():
        for claim in doc["claims"]:
            for link in claim.get("relates_to", []) or []:
                target = link.get("claim_id")
                predicate = RELATION_PREDICATE.get(link.get("relation"))
                if target not in claim_node:
                    exp.warn("%s relates to unknown claim %s" % (claim["id"], target))
                    continue
                if predicate is None:
                    exp.warn("%s has unknown relation %r" % (claim["id"], link.get("relation")))
                    continue
                add_edge(claim_node[claim["id"]], predicate, claim_node[target], {},
                         [claim_span_ev[claim["id"]]], "manual")

    # ---- World-model concepts and dependency edges.
    world_path = NOTES_ROOT + "/world-model.json"
    # Slice nodes and edges separately by region to avoid id collisions.
    nodes_at = world_raw.find(b'"nodes"')
    edges_at = world_raw.find(b'"edges"')
    concept_node = {}
    for node in world["nodes"]:
        needle = ('"id": "%s"' % node["id"]).encode()
        pos = world_raw.find(needle, nodes_at, edges_at)
        if pos < 0:
            raise ExportError("world-model node %s bytes not found" % node["id"])
        nxt = [world_raw.find(('"id": "%s"' % n["id"]).encode(), pos + 1) for n in world["nodes"]]
        nxt = [p for p in nxt if p > pos]
        end = min(nxt) if nxt else edges_at
        line_no = world_raw[:pos].count(b"\n")
        start = world_raw.rfind(b"\n", 0, pos) + 1
        end_line_start = world_raw.rfind(b"\n", 0, end) + 1
        ev = add_evidence(artifact_ids[world_path], span_locator(world_path, start, end_line_start, world_raw), "corpus-world-span")
        concept_node[node["id"]] = add_node("Concept", node["id"], node.get("label", node["id"]),
                                            {"domain_layer": node.get("domain_layer", "")}, [ev])
    for edge in world["edges"]:
        if edge.get("from") not in concept_node or edge.get("to") not in concept_node:
            exp.warn("world edge %s has unknown endpoint" % edge.get("id"))
            continue
        needle = ('"id": "%s"' % edge["id"]).encode()
        pos = world_raw.find(needle, edges_at)
        if pos < 0:
            raise ExportError("world-model edge %s bytes not found" % edge["id"])
        start = world_raw.rfind(b"\n", 0, pos) + 1
        following = [world_raw.find(('"id": "%s"' % e["id"]).encode(), pos + 1) for e in world["edges"]]
        following = [p for p in following if p > pos]
        end = min(following) if following else len(world_raw)
        end_line_start = world_raw.rfind(b"\n", 0, end) + 1
        ev = add_evidence(artifact_ids[world_path], span_locator(world_path, start, end_line_start, world_raw), "corpus-world-span")
        add_edge(concept_node[edge["from"]], "DEPENDS_ON", concept_node[edge["to"]],
                 {"label": edge.get("relation", ""), "kind": edge.get("kind", "")}, [ev], "manual")

    # ---- Curated claim-to-concept associations.
    assoc_path = NOTES_ROOT + "/model-associations.json"
    assoc_raw = artifact_bytes[assoc_path]
    assoc_ids = [a["claim_id"] for a in assoc["associations"]]
    assoc_spans = slice_records(assoc_raw, ASSOC_ID_RE, assoc_ids)
    for entry in assoc["associations"]:
        cid = entry["claim_id"]
        if cid not in claim_node:
            exp.warn("association references unknown claim %s" % cid)
            continue
        start, end = assoc_spans[cid]
        ev = add_evidence(artifact_ids[assoc_path], span_locator(assoc_path, start, end, assoc_raw), "corpus-association-span")
        for node_id in entry.get("node_ids", []) or []:
            if node_id not in concept_node:
                exp.warn("association %s references unknown node %s" % (cid, node_id))
                continue
            add_edge(claim_node[cid], "DESCRIBES_CONCEPT", concept_node[node_id],
                     {"basis": entry.get("basis", "")}, [ev], "manual")

    # ---- Anchors: exact-byte spans plus cross-snapshot code traces.
    source_scope = set()
    if args.source_scope:
        source_scope = {ln.strip() for ln in Path(args.source_scope).read_text().splitlines() if ln.strip()}
    trace_count = 0
    span_count = 0
    for cid, role, path, ranges, contract_id, sha, symbol in file_anchors:
        raw = artifact_bytes.get(path)
        if raw is not None and sha and sha256_hex(raw) != sha:
            exp.warn("%s anchor %s hash mismatch; span skipped" % (cid, path))
            raw = None
        if raw is not None and not sha and path in work_sourced:
            raw = None
        if raw is not None and (sha and sha256_hex(raw) == sha):
            try:
                raw.decode("utf-8", "strict")
            except UnicodeDecodeError:
                exp.warn("%s anchor %s not UTF-8; span skipped" % (cid, path))
                raw = None
        elif raw is not None:
            exp.warn("%s anchor %s hash mismatch; span skipped" % (cid, path))
            raw = None
        if raw is not None:
            method = "corpus-worktree-span" if path in work_sourced else "corpus-anchor-lines"
            if contract_id:
                pos = raw.find(contract_id.encode())
                if pos < 0:
                    exp.warn("%s contract %s not found in %s" % (cid, contract_id, path))
                else:
                    start = raw.rfind(b"\n", 0, pos) + 1
                    stop = raw.find(b"\n", pos)
                    stop = stop + 1 if stop >= 0 else len(raw)
                    ev = add_evidence(artifact_ids[path], span_locator(path, start, stop, raw), method)
                    node = exp.nodes[claim_node[cid]]
                    node["evidence_ids"] = sorted(set(node["evidence_ids"]) | {ev})
                    span_count += 1
            else:
                use_ranges = ranges or [(1, raw.count(b"\n") + (0 if raw.endswith(b"\n") else 1))]
                for lo, hi in use_ranges:
                    try:
                        start, stop = line_span_bytes(raw, lo, hi)
                    except ExportError as exc:
                        exp.warn("%s anchor %s: %s" % (cid, path, exc))
                        continue
                    ev = add_evidence(artifact_ids[path], span_locator(path, start, stop, raw), method)
                    node = exp.nodes[claim_node[cid]]
                    node["evidence_ids"] = sorted(set(node["evidence_ids"]) | {ev})
                    span_count += 1
        if args.source_snapshot and path in source_scope:
            target = identity("node", args.source_snapshot, "SourceFile", path)
            attrs = {"traced_path": path, "anchor_role": role}
            if symbol:
                attrs["symbol"] = symbol
            if ranges:
                attrs["lines"] = ",".join("%d-%d" % r if r[0] != r[1] else "%d" % r[0] for r in ranges)
            if sha:
                attrs["audited_sha256"] = sha
            add_edge(claim_node[cid], "CORRESPONDS_TO", target, attrs, [claim_span_ev[cid]], "manual")
            trace_count += 1
        elif raw is None:
            if git_is_tree(repo, rev, path):
                exp.skip("%s %s %s is a directory anchor; no file target" % (cid, role, path))
            else:
                exp.skip("%s %s %s has no verified bytes and no in-scope trace" % (cid, role, path))

    # ---- Drop artifacts no evidence references.
    referenced = {e["artifact_id"] for e in exp.evidence.values()}
    pruned = [k for k, a in exp.artifacts.items() if k not in referenced]
    for key in pruned:
        del exp.artifacts[key]
    for path in [p for p in exp.artifact_bytes if p not in {a["path"] for a in exp.artifacts.values()}]:
        del exp.artifact_bytes[path]

    # ---- Bundle assembly.
    snapshot = {"id": snapshot_id, "source_id": SOURCE_ID, "revision": rev, "content_sha256": content,
                "extraction": {"tool": TOOL, "version": VERSION, "configuration_sha256": config_sha, "scope_sha256": scope_sha},
                "scope": scope, "dependencies": sorted(dependencies), "visibility": "reviewed",
                "capabilities": [
                    {"name": "corpus_claims", "status": "complete" if not exp.warnings else "partial",
                     "detail": "accepted ledgers through receipt %s" % args.receipt,
                     "counts": {"inputs_enumerated": len(claim_ids), "inputs_attempted": len(claim_ids),
                                "records_emitted": len(exp.nodes) + len(exp.assertions),
                                "inputs_failed": 0, "inputs_skipped": len(exp.skipped)}},
                    {"name": "anchor_spans", "status": "complete" if not exp.warnings else "partial",
                     "detail": "hash-verified line spans into corpus artifacts",
                     "counts": {"inputs_enumerated": len(file_anchors), "inputs_attempted": len(file_anchors),
                                "records_emitted": span_count, "inputs_failed": 0, "inputs_skipped": 0}},
                    {"name": "code_traces", "status": "complete" if args.source_snapshot else "not_run",
                     "detail": "CORRESPONDS_TO links into source snapshot" if args.source_snapshot else "no source snapshot supplied",
                     "counts": {"inputs_enumerated": None, "inputs_attempted": trace_count,
                                "records_emitted": trace_count, "inputs_failed": 0, "inputs_skipped": len(exp.skipped)}},
                ]}
    bundle = {"format": "mudcrab-kg-bundle", "schema_version": 1, "source": source, "snapshot": snapshot,
              "artifacts": [exp.artifacts[k] for k in sorted(exp.artifacts)],
              "evidence": [exp.evidence[k] for k in sorted(exp.evidence)],
              "nodes": [exp.nodes[k] for k in sorted(exp.nodes)],
              "assertions": [exp.assertions[k] for k in sorted(exp.assertions)]}
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=False)
    try:
        raw_bundle = canonical(bundle) + b"\n"
        if len(raw_bundle) > 2 * 1024 * 1024 * 1024:
            raise ExportError("bundle exceeds 2 GiB")
        for record in bundle["artifacts"] + bundle["evidence"] + bundle["nodes"] + bundle["assertions"]:
            if len(canonical(record)) > 8 * 1024 * 1024:
                raise ExportError("record exceeds 8 MiB: %s" % record.get("id"))
        (out / "bundle.json").write_bytes(raw_bundle)
        for path, raw in exp.artifact_bytes.items():
            target = out / "artifacts" / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
        report = {"snapshot_id": snapshot_id, "content_sha256": content,
                  "counts": {"artifacts": len(exp.artifacts), "evidence": len(exp.evidence),
                             "nodes": len(exp.nodes), "assertions": len(exp.assertions),
                             "pruned_artifacts": len(pruned)},
                  "warnings": exp.warnings, "skipped": exp.skipped}
        (out / "report.json").write_bytes(canonical(report) + b"\n")
    except BaseException:
        import shutil
        shutil.rmtree(out, ignore_errors=True)
        raise
    print(json.dumps({"snapshot_id": snapshot_id, "out": str(out),
                      "artifacts": len(exp.artifacts), "evidence": len(exp.evidence),
                      "nodes": len(exp.nodes), "assertions": len(exp.assertions),
                      "warnings": len(exp.warnings), "skipped": len(exp.skipped)}, indent=1))
    for warning in exp.warnings[:20]:
        print("warning: " + warning, file=sys.stderr)
    return 0


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--corpus-rev", required=True)
    parser.add_argument("--receipt", default="20261010-02")
    parser.add_argument("--source-snapshot", default=None)
    parser.add_argument("--source-scope", default=None)
    parser.add_argument("--work", default=None,
                        help="working tree supplying anchored bytes missing at corpus_rev (hash-verified)")
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    if args.receipt != "20261010-02":
        raise ExportError("only receipt 20261010-02 is supported, got %s" % args.receipt)
    if bool(args.source_snapshot) != bool(args.source_scope):
        raise ExportError("--source-snapshot and --source-scope are required together")
    return args


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ExportError as exc:
        print("corpus_bundle: %s" % exc, file=sys.stderr)
        sys.exit(2)
