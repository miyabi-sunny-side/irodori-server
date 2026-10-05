"""Tests the pure request handling of irodori_engine.py. Run: python3 -m unittest tests/engine_test.py"""
import base64
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from irodori_engine import decode_references  # noqa: E402


def item(fmt, data=b"AUDIO"):
    return {"format": fmt, "data": base64.b64encode(data).decode()}


class DecodeReferences(unittest.TestCase):
    def test_keeps_order_and_lowercases_the_format(self):
        self.assertEqual(decode_references([item("M4A", b"a"), item("wav", b"b")]), [("m4a", b"a"), ("wav", b"b")])

    def test_missing_references_mean_none(self):
        self.assertEqual(decode_references(None), [])

    def test_formats_that_could_form_a_path_are_rejected(self):
        for fmt in ["", "../x", "wav/x", "a.b", "toolongext"]:
            with self.assertRaises(ValueError, msg=fmt):
                decode_references([item(fmt)])

    def test_invalid_base64_is_rejected(self):
        with self.assertRaises(ValueError):
            decode_references([{"format": "wav", "data": "not base64!"}])


if __name__ == "__main__":
    unittest.main()
