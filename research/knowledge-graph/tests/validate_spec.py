#!/usr/bin/env python3
"""Validate this SPEC package, not the future Mudcrab KG implementation.

Run from any directory with Python 3.11+ and jsonschema 4.x installed.
This script does not invoke network, Cargo, Ghidra, RenderDoc, or a game.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import re
import sqlite3
import sys
import time
import unittest
from pathlib import Path
from typing import Any

try:
    from jsonschema import Draft202012Validator
except ImportError as exc:
    raise SystemExit("Install jsonschema 4.x before running this package validator.") from exc

ROOT = Path(__file__).resolve().parents[1]
VIS = {"public": 0, "reviewed": 1, "restricted": 2}
BUNDLE_SCHEMA = json.loads((ROOT / "contracts/bundle.schema.json").read_text())
ONTOLOGY = json.loads((ROOT / "contracts/ontology.json").read_text())
PREDICATES = {r["name"]: r for r in ONTOLOGY["relations"]}
BUNDLES = [json.loads(p.read_text()) for p in sorted((ROOT / "examples").glob("*.bundle.json"))]


def canonical(value: Any) -> bytes:
    """Return the restricted canonical payload encoding from IDENTITY.md."""
    def check(x: Any) -> None:
        if x is None or type(x) is bool:
            return
        if type(x) is int:
            if not -(2**53 - 1) <= x <= 2**53 - 1:
                raise ValueError("integer outside exact wire range")
            return
        if isinstance(x, str):
            x.encode("utf-8", "strict")
            return
        if isinstance(x, list):
            for y in x:
                check(y)
            return
        if isinstance(x, dict):
            for k, v in x.items():
                if not isinstance(k, str):
                    raise ValueError("non-string object key")
                check(k)
                check(v)
            return
        raise ValueError(f"unsupported canonical value: {type(x).__name__}")
    check(value)
    return json.dumps(value, sort_keys=True, ensure_ascii=False,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def hid(kind: str, *parts: str) -> str:
    """Length-prefixed, domain-separated identity reference implementation."""
    result = bytearray(b"MUDCRAB-KG-ID-v1\x00")
    for item in (kind, *parts):
        encoded = item.encode("utf-8", "strict")
        result.extend(len(encoded).to_bytes(8, "big"))
        result.extend(encoded)
    return kind + "_" + hashlib.sha256(result).hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def portable(path: str) -> None:
    require(bool(path) and not path.startswith("/"), "absolute or empty path")
    require("\\" not in path and "\x00" not in path and not re.match(r"^[A-Za-z]:", path),
            "nonportable path")
    require(all(p not in ("", ".", "..") for p in path.split("/")), "noncanonical path")


def pointer_value(raw: bytes, pointer: str) -> Any:
    result = json.loads(raw)
    if pointer == "":
        return result
    require(pointer.startswith("/"), "invalid pointer")
    for piece in pointer[1:].split("/"):
        require(not re.search(r"~(?![01])", piece), "invalid pointer escape")
        piece = piece.replace("~1", "/").replace("~0", "~")
        result = result[int(piece)] if isinstance(result, list) else result[piece]
    return result


def verify(bundles: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    """Validate contract examples with explicit transitive dependency closure."""
    entities: dict[str, dict[str, Any]] = {}
    owners: dict[str, str] = {}
    snapshots: dict[str, dict[str, Any]] = {}
    source_records: dict[str, dict[str, Any]] = {}
    validator = Draft202012Validator(BUNDLE_SCHEMA)
    for b in bundles:
        canonical(b)
        validator.validate(b)
        s = b["snapshot"]
        source = b["source"]
        x = s["extraction"]
        sid = s["id"]
        require(sid not in snapshots, "duplicate snapshot in import set")
        require(s["source_id"] == source["id"], "source mismatch")
        if source["id"] in source_records:
            require(source_records[source["id"]] == source, "conflicting source declaration")
        source_records[source["id"]] = source
        require(len(s["dependencies"]) == len(set(s["dependencies"])), "duplicate dependency")
        require(all(d in snapshots for d in s["dependencies"]), "missing or cyclic dependency")
        require(x["scope_sha256"] == digest(s["scope"]), "scope hash mismatch")
        expected = hid("snap", source["id"], s["revision"], s["content_sha256"],
                       x["tool"], x["version"], x["configuration_sha256"],
                       x["scope_sha256"], digest(sorted(s["dependencies"])))
        require(sid == expected, "snapshot identity mismatch")
        require(VIS[s["visibility"]] >= VIS[source["visibility"]], "snapshot policy downgrade")
        allowed = {sid}
        queue = list(s["dependencies"])
        while queue:
            dep = queue.pop()
            if dep in allowed:
                continue
            allowed.add(dep)
            queue.extend(snapshots[dep]["dependencies"])
        snapshots[sid] = s
        raw_by_artifact: dict[str, bytes] = {}
        for group in ("artifacts", "evidence", "nodes", "assertions"):
            for item in b[group]:
                require(item["id"] not in entities, "duplicate immutable ID")
                entities[item["id"]] = item
                owners[item["id"]] = sid
                require(VIS[item["visibility"]] >= VIS[s["visibility"]], "snapshot visibility floor")
        def ref(ident: str) -> dict[str, Any]:
            require(ident in entities and owners[ident] in allowed, "undeclared or missing reference")
            return entities[ident]
        for a in b["artifacts"]:
            portable(a["path"])
            base = ROOT / "examples/artifacts"
            file = base / a["path"]
            require(not file.is_symlink() and file.resolve().is_relative_to(base.resolve()),
                    "artifact path escape")
            raw = file.read_bytes()
            require(len(raw) == a["byte_size"], "artifact size mismatch")
            require(hashlib.sha256(raw).hexdigest() == a["sha256"], "artifact digest mismatch")
            require(a["id"] == hid("art", sid, a["path"], a["sha256"]), "artifact identity mismatch")
            raw_by_artifact[a["id"]] = raw
        for e in b["evidence"]:
            a = ref(e["artifact_id"])
            require(owners[a["id"]] == sid, "artifact outside evidence snapshot")
            require(VIS[e["visibility"]] >= VIS[a["visibility"]], "evidence visibility downgrade")
            require(e["id"] == hid("ev", sid, a["id"], digest(e["locator"]),
                                    e["method"], e["provenance_class"]), "evidence identity mismatch")
            loc = e["locator"]
            raw = raw_by_artifact[a["id"]]
            if loc["kind"] == "source_span":
                portable(loc["path"])
                require(loc["path"] == a["path"], "source locator path mismatch")
                start, end = loc["start_byte"], loc["end_byte"]
                require(0 <= start <= end <= len(raw), "source span outside artifact")
                raw[:start].decode("utf-8")
                raw[start:end].decode("utf-8")
                require(loc["start_line"] == raw[:start].count(b"\n") + 1, "start line mismatch")
                line_end = raw[:max(start, end - 1)].count(b"\n") + 1
                require(loc["end_line"] == line_end, "end line mismatch")
            if loc["kind"] == "json_pointer":
                pointer_value(raw, loc["pointer"])
            # The binary/capture examples identify SYNTHETIC observations. This validator
            # checks their schema, not nonexistent original module/capture contents.
        for n in b["nodes"]:
            require(n["id"] == hid("node", sid, n["kind"], n["logical_key"]), "node identity mismatch")
            for eid in n["evidence_ids"]:
                require(VIS[n["visibility"]] >= VIS[ref(eid)["visibility"]], "node visibility downgrade")
        for a in b["assertions"]:
            subject, obj = ref(a["subject"]), ref(a["object"])
            p = PREDICATES[a["predicate"]]
            require(subject["kind"] in p["subject_kinds"] and obj["kind"] in p["object_kinds"],
                    "ontology domain/range mismatch")
            links = sorted(a["evidence"], key=lambda z: (z["id"], z["role"]))
            require(len({(z["id"], z["role"]) for z in links}) == len(links), "duplicate evidence link")
            require(any(z["role"] == "supports" for z in links), "missing supporting evidence")
            expected = hid("as", sid, a["subject"], a["predicate"], a["object"],
                           digest(a["attributes"]), digest(links), a["provenance_class"])
            require(a["id"] == expected, "assertion identity mismatch")
            floor = max([VIS[subject["visibility"]], VIS[obj["visibility"]]] +
                        [VIS[ref(z["id"])["visibility"]] for z in links])
            require(VIS[a["visibility"]] >= floor, "assertion visibility downgrade")
            if a["provenance_class"] in ("hypothesis", "manual"):
                require(a["initial_state"] == "proposed", "unreviewed interpretation admitted")
    return entities


def database() -> sqlite3.Connection:
    """Instantiate the reference schema and insert only synthetic fixture records."""
    db = sqlite3.connect(":memory:")
    db.executescript((ROOT / "contracts/storage.sql").read_text())
    for p in ONTOLOGY["relations"]:
        db.execute("INSERT INTO predicates VALUES (?,?)", (p["name"], json.dumps(p)))
    for b in BUNDLES:
        s = b["snapshot"]
        src = b["source"]
        db.execute("INSERT INTO sources VALUES (?,?,?,?)", (src["id"],src["kind"],src["origin_uri"],json.dumps(src)))
        db.execute("INSERT INTO snapshots VALUES (?,?,?,?,?,?,?,?,?)", (
            s["id"],s["source_id"],s["revision"],s["content_sha256"],json.dumps(s["extraction"]),
            json.dumps(s["scope"]),json.dumps(s["capabilities"]),s["visibility"],"published"))
        db.executemany("INSERT INTO snapshot_dependencies VALUES (?,?)", [(s["id"], d) for d in s["dependencies"]])
        for a in b["artifacts"]:
            db.execute("INSERT INTO artifacts VALUES (?,?,?,?,?,?,?,?)", (
                a["id"],s["id"],a["path"],a["sha256"],a["media_type"],a["byte_size"],a["visibility"],a["blob_sha256"]))
        for e in b["evidence"]:
            db.execute("INSERT INTO evidence VALUES (?,?,?,?,?,?,?)", (
                e["id"],s["id"],e["artifact_id"],json.dumps(e["locator"]),e["method"],e["provenance_class"],e["visibility"]))
        for n in b["nodes"]:
            db.execute("INSERT INTO nodes VALUES (?,?,?,?,?,?,?)", (
                n["id"],s["id"],n["kind"],n["logical_key"],n["display_name"],json.dumps(n["attributes"]),n["visibility"]))
            db.executemany("INSERT INTO node_evidence VALUES (?,?)", [(n["id"], e) for e in n["evidence_ids"]])
            db.execute("INSERT INTO node_fts VALUES (?,?,?)", (n["id"],n["display_name"],n["logical_key"]))
        for a in b["assertions"]:
            db.execute("INSERT INTO assertions VALUES (?,?,?,?,?,?,?,?,?)", (
                a["id"],s["id"],a["subject"],a["predicate"],a["object"],json.dumps(a["attributes"]),
                a["provenance_class"],a["initial_state"],a["visibility"]))
            db.executemany("INSERT INTO assertion_evidence VALUES (?,?,?)",
                           [(a["id"], e["id"], e["role"]) for e in a["evidence"]])
    db.commit()
    return db


class SpecContractTests(unittest.TestCase):
    def test_01_schemas_are_valid(self):
        for path in (ROOT / "contracts").glob("*.schema.json"):
            Draft202012Validator.check_schema(json.loads(path.read_text()))

    def test_02_all_bundles_and_cross_references(self):
        entities = verify(BUNDLES)
        self.assertGreater(len(entities), 30)

    def test_03_identity_vectors(self):
        data = json.loads((ROOT / "examples/identity-vectors.json").read_text())
        for v in data["vectors"]:
            self.assertEqual(hid(v["kind"], *v["parts"]), v["expected"])
        self.assertNotEqual(hid("node", "a", "bc"), hid("node", "ab", "c"))

    def test_04_canonical_payload_constraints(self):
        self.assertEqual(canonical({"é": 2, "a": "\n"}), b'{"a":"\\n","\xc3\xa9":2}')
        for bad in (1.0, float("nan"), float("inf"), 2**53, {1: "bad"}):
            with self.assertRaises(ValueError):
                canonical(bad)
        with self.assertRaises(UnicodeError):
            canonical("\ud800")

    def test_05_reject_unsafe_paths(self):
        for bad in ("../x", "/x", "x/../y", "C:/x", "a\\b", "a\x00b", "a//b", "./x"):
            with self.assertRaises(ValueError):
                portable(bad)

    def test_06_reject_modified_artifact_hash(self):
        bad = copy.deepcopy(BUNDLES)
        bad[0]["artifacts"][0]["sha256"] = "0" * 64
        with self.assertRaises(ValueError):
            verify(bad)

    def test_07_reject_missing_dependency(self):
        with self.assertRaises(ValueError):
            verify(BUNDLES[1:])

    def test_08_reject_policy_downgrade(self):
        bad = copy.deepcopy(BUNDLES)
        bad[2]["nodes"][0]["visibility"] = "public"
        with self.assertRaises(ValueError):
            verify(bad)

    def test_09_reject_hypothesis_admission(self):
        bad = copy.deepcopy(BUNDLES)
        bad[2]["assertions"][-1]["initial_state"] = "admitted"
        with self.assertRaises(ValueError):
            verify(bad)

    def test_10_reject_corrupt_identity(self):
        bad = copy.deepcopy(BUNDLES)
        bad[0]["nodes"][0]["id"] = "node_" + "0" * 64
        with self.assertRaises(ValueError):
            verify(bad)

    def test_11_reference_sql_and_foreign_keys(self):
        with database() as db:
            self.assertEqual(db.execute("PRAGMA foreign_key_check").fetchall(), [])
            self.assertEqual(db.execute("SELECT COUNT(*) FROM nodes").fetchone()[0],sum(len(b["nodes"]) for b in BUNDLES))
            with self.assertRaises(sqlite3.IntegrityError):
                db.execute("INSERT INTO node_evidence VALUES ('not-a-node','not-evidence')")

    def test_12_reference_fts_search(self):
        with database() as db:
            matches = db.execute("SELECT node_id FROM node_fts WHERE node_fts MATCH ?", ('"toy"',)).fetchall()
            self.assertTrue(matches)

    def test_13_review_view_preserves_origin(self):
        a = BUNDLES[2]["assertions"][-1]
        with database() as db:
            self.assertEqual(db.execute("SELECT current_state FROM assertion_states WHERE id=?",(a["id"],)).fetchone()[0],"proposed")
            db.execute("INSERT INTO review_events(assertion_id,actor,reason,decision,reviewed_at,evidence_json) VALUES(?,?,?,?,?,?)",(a["id"],"synthetic reviewer","test only","rejected","synthetic-test-time","[]"))
            row = db.execute("SELECT current_state,provenance_class FROM assertion_states WHERE id=?",(a["id"],)).fetchone()
            self.assertEqual(row,("rejected","hypothesis"))

    def test_14_context_dependency_closure(self):
        c = json.loads((ROOT / "examples/context.example.json").read_text())
        ids = set(c["snapshot_ids"])
        self.assertEqual(ids,{b["snapshot"]["id"] for b in BUNDLES})
        for b in BUNDLES:
            self.assertTrue(set(b["snapshot"]["dependencies"]) <= ids)
        self.assertEqual(c["policy"],"research-local")

    def test_15_tasks_are_dependency_ordered(self):
        data = json.loads((ROOT / "IMPLEMENTATION_TASKS.json").read_text())
        done = set()
        for t in data["tasks"]:
            self.assertNotIn(t["id"],done)
            self.assertTrue(set(t["depends_on"]) <= done)
            self.assertTrue(t["deliverables"] and t["acceptance"])
            done.add(t["id"])
        self.assertEqual(len(done),16)

    def test_16_package_local_markdown_links(self):
        for path in ROOT.parents[1].rglob("*.md"):
            if any(part in {"target", ".venv", ".git"} for part in path.parts):
                continue
            for link in re.findall(r"\]\(([^)]+)\)",path.read_text()):
                if "://" in link or link.startswith("#"):
                    continue
                target = link.split("#",1)[0]
                if target == "VALIDATION_REPORT.json":
                    continue  # This script emits that report after the suite.
                self.assertTrue((path.parent / target).exists(),(str(path),target))

    def test_17_stream_manifest_contract(self):
        schema = json.loads((ROOT / "contracts/stream.schema.json").read_text())
        b = BUNDLES[0]
        files = []
        for name in ("artifacts","evidence","nodes","assertions","diagnostics"):
            values = b.get(name,[])
            raw = b"".join(canonical(v)+b"\n" for v in values)
            files.append({"kind":name,"path":name+".jsonl","sha256":hashlib.sha256(raw).hexdigest(),"byte_size":len(raw),"record_count":len(values)})
        manifest = {"format":"mudcrab-kg-stream","schema_version":1,"source":b["source"],"snapshot":b["snapshot"],"files":files}
        Draft202012Validator(schema).validate(manifest)
        self.assertEqual(len({f["kind"] for f in files}),5)

    def test_18_repo_review_and_main_spec_agree(self):
        review = json.loads((ROOT / "REPO_REVIEW.json").read_text())
        self.assertIn(review["reviewed_commit"],(ROOT.parents[1] / "SPEC.md").read_text())
        self.assertEqual(review["observed_versions"]["converter_schema"],16)
        self.assertEqual(review["observed_versions"]["world_database_schema"],5)
        for b in BUNDLES:
            self.assertTrue(b["snapshot"]["scope"]["synthetic"])
            self.assertIn("Synthetic",b["snapshot"]["capabilities"][0]["detail"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, help="Optional external JSON report path")
    args = parser.parse_args()
    started = time.perf_counter()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SpecContractTests)
    stream = io.StringIO()
    result = unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    output = stream.getvalue()
    print(output,end="")
    report = {
        "scope":"Specification-package validation only; not a Mudcrab build or KG implementation test.",
        "tests_run":result.testsRun,"failures":len(result.failures),"errors":len(result.errors),
        "success":result.wasSuccessful(),"elapsed_seconds":round(time.perf_counter()-started,3),
        "bundle_count":len(BUNDLES),"nodes":sum(len(b["nodes"]) for b in BUNDLES),
        "assertions":sum(len(b["assertions"]) for b in BUNDLES),
        "not_tested":["Mudcrab Rust build","Original Skyrim data","Ghidra exporter execution","RenderDoc replay","MCP transport (verified by a separate smoke)"],
        "output":output
    }
    if args.report:
        args.report.write_text(json.dumps(report,indent=2)+"\n")
    sys.exit(0 if result.wasSuccessful() else 1)
