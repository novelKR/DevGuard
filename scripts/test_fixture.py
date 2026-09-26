"""The foreground fixture through headless Chrome: samples arrive, and a headless fixture is never a
valid observation. Native macOS only; where Chrome is absent the case is recorded as not run.
The raw receipt goes to DEVGUARD_EVIDENCE_DIR when the qualification harness sets it."""
import argparse
import json
import os
from pathlib import Path
import platform
import tempfile
import unittest

import measure


def record(value):
    directory = os.environ.get("DEVGUARD_EVIDENCE_DIR")
    if directory:
        (Path(directory) / "fixture-check.json").write_text(json.dumps(value, indent=2) + "\n")


class Fixture(unittest.TestCase):
    def test_a_headless_fixture_is_observed_but_never_valid(self):
        if platform.system() != "Darwin" or not measure.CHROME.exists():
            record({"status": "not_run", "reason": "the fixture needs Google Chrome on macOS"})
            return
        with tempfile.TemporaryDirectory(prefix="dg-f-", dir="/private/tmp") as directory:
            out = Path(directory) / "check"
            self.assertEqual(measure.fixture_check(argparse.Namespace(out=out, seconds=15, headless=True)), 0)
            report = json.loads((out / "fixture-check.json").read_text())
            raw = (out / "fixture.jsonl").read_text().splitlines()
        self.assertEqual(report["verdict"], "inconclusive")
        self.assertIn("headless fixture", report["invalid"])
        self.assertGreater(report["metrics"]["input"]["painted"], 10)
        self.assertGreater(report["metrics"]["frames"]["frames"], 100)
        self.assertEqual(report["fixture_sha256"], measure.sha256(measure.FIXTURE))
        self.assertTrue(raw)
        record(report)


if __name__ == "__main__":
    unittest.main()
