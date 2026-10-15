"""Tests the pure request handling of irodori_engine.py. Run: python3 -m unittest tests/engine_test.py"""
import base64
import gc
from pathlib import Path
import sys
import unittest
import weakref

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from irodori_engine import decode_references, run_captured  # noqa: E402


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


class OutOfMemoryError(RuntimeError):
    """Stands in for torch.OutOfMemoryError, whose message need not say "out of memory"."""


class ReleaseAfterFailure(unittest.TestCase):
    def run_failing(self, exc):
        released = []

        def call():
            raise exc

        reply = run_captured(call, lambda: released.append(True))
        return reply, released

    def test_out_of_memory_releases_and_keeps_the_error(self):
        for exc in [RuntimeError("CUDA out of memory. Tried to allocate 20.00 MiB"), OutOfMemoryError("CUDA error")]:
            reply, released = self.run_failing(exc)
            self.assertEqual(released, [True], msg=exc)
            self.assertEqual((reply["ok"], reply["error_type"], reply["error"]), (False, type(exc).__name__, str(exc)))

    def test_release_runs_after_the_failed_call_frees_its_locals(self):
        class Model:
            pass

        refs, alive = [], []

        def call():
            model = Model()  # a half-loaded model, referenced only by the failing frame
            refs.append(weakref.ref(model))
            raise RuntimeError("CUDA out of memory")

        def release():
            gc.collect()
            alive.append(refs[0]() is not None)

        run_captured(call, release)
        self.assertEqual(alive, [False])

    def test_other_errors_do_not_release(self):
        for exc in [ValueError("bad input"), RuntimeError("connection reset")]:
            self.assertEqual(self.run_failing(exc)[1], [], msg=exc)

    def test_success_does_not_release(self):
        released = []
        self.assertTrue(run_captured(lambda: {}, lambda: released.append(True))["ok"])
        self.assertEqual(released, [])

    def test_a_failing_release_keeps_the_error_reply(self):
        def release():
            raise RuntimeError("release failed")

        reply = run_captured(lambda: (_ for _ in ()).throw(RuntimeError("out of memory")), release)
        self.assertEqual((reply["ok"], reply["error"]), (False, "out of memory"))
        self.assertIn("release failed", reply["engine_log"])


if __name__ == "__main__":
    unittest.main()
