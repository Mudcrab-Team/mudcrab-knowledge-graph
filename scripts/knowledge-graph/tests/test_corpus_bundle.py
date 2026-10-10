"""Unit tests for corpus_bundle pure helpers. No git checkout required."""
import importlib.util
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))


def load():
    spec = importlib.util.spec_from_file_location("corpus_bundle", ROOT / "corpus_bundle.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CB = load()


class RangeTests(unittest.TestCase):
    def test_mixed_singles_and_ranges(self):
        self.assertEqual(CB.parse_ranges("3, 13, 32-44, 69, 101-114"),
                         [(3, 3), (13, 13), (32, 44), (69, 69), (101, 114)])

    def test_single_int(self):
        self.assertEqual(CB.parse_ranges(20), [(20, 20)])

    def test_rejects_bad_ranges(self):
        for bad in ["", "0", "5-2", "a-b", "3-", None]:
            with self.assertRaises(CB.ExportError, msg=repr(bad)):
                CB.parse_ranges(bad)


class LineSpanTests(unittest.TestCase):
    RAW = b"alpha\nbeta\ngamma\n"

    def test_middle_lines(self):
        self.assertEqual(CB.line_span_bytes(self.RAW, 2, 3), (6, 17))

    def test_last_line_with_trailing_newline(self):
        self.assertEqual(CB.line_span_bytes(self.RAW, 3, 3), (11, 17))

    def test_rejects_out_of_range(self):
        with self.assertRaises(CB.ExportError):
            CB.line_span_bytes(self.RAW, 2, 4)

    def test_span_locator_lines_match_validator_math(self):
        raw = b"one\ntwo\nthree\n"
        loc = CB.span_locator("f.md", 4, 11, raw)
        self.assertEqual((loc["start_line"], loc["end_line"]), (2, 3))
        single = CB.span_locator("f.md", 0, 3, raw)
        self.assertEqual((single["start_line"], single["end_line"]), (1, 1))


class SliceTests(unittest.TestCase):
    RAW = b'{"claims": [\n  {"id": "N003-L01", "x": 1},\n  {"id": "N003-L02", "x": 2}\n ]}\n'

    def test_record_spans_follow_id_order(self):
        spans = CB.slice_records(self.RAW, CB.CLAIM_ID_RE, ["N003-L01", "N003-L02"])
        self.assertIn(b"N003-L01", self.RAW[spans["N003-L01"][0]:spans["N003-L01"][1]])
        self.assertIn(b"N003-L02", self.RAW[spans["N003-L02"][0]:spans["N003-L02"][1]])
        self.assertLess(spans["N003-L01"][1], spans["N003-L02"][1])

    def test_rejects_id_mismatch(self):
        with self.assertRaises(CB.ExportError):
            CB.slice_records(self.RAW, CB.CLAIM_ID_RE, ["N003-L01"])

    def test_excerpt_find(self):
        raw = "héllo world".encode("utf-8")
        hit = CB.find_excerpt(raw, "llo wo")
        self.assertEqual(raw[hit[0]:hit[1]], "llo wo".encode("utf-8"))
        self.assertIsNone(CB.find_excerpt(raw, "missing"))


if __name__ == "__main__":
    unittest.main()
