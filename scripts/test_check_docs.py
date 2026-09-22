"""Regression tests for stale translations and broken planning, using isolated copies."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import check_docs


class DocumentationChecks(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        shutil.copytree(check_docs.ROOT / "docs", self.root / "docs")
        for name in ("README.md", "AGENTS.md", "milestones.json", "LICENSE", "NOTICE"):
            shutil.copy2(check_docs.ROOT / name, self.root / name)

    def test_current_pairs_and_complete_plan(self):
        self.assertEqual(check_docs.check(self.root)["work_units"], 46)

    def test_edit_requires_review_of_changed_pair(self):
        for name in ("docs/planning/README.md", "docs/ko/planning/README.md"):
            with self.subTest(name=name):
                path = self.root / name
                original = path.read_bytes()
                path.write_bytes(original + b"\nChanged.\n")
                with self.assertRaisesRegex(ValueError, "unreviewed"):
                    check_docs.check_translations(self.root)
                path.write_bytes(original)

    def test_missing_translation_fails(self):
        (self.root / "docs/ko/design.md").unlink()
        with self.assertRaisesRegex(ValueError, "missing"):
            check_docs.check_translations(self.root)

    def test_duplicate_pair_fails(self):
        path = self.root / "docs/translations.json"
        data = json.loads(path.read_text())
        data["pairs"].append(data["pairs"][0])
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            check_docs.check_translations(self.root)

    def test_runtime_guide_pair_cannot_be_dropped_to_bypass_review(self):
        path = self.root / "docs/translations.json"
        data = json.loads(path.read_text())
        data["pairs"] = [p for p in data["pairs"] if p["id"] != "operations"]
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, "unpaired"):
            check_docs.check_translations(self.root)

    def test_broken_local_link_fails(self):
        with (self.root / "README.md").open("a") as stream:
            stream.write("\n[missing](docs/missing.md)\n")
        with self.assertRaisesRegex(ValueError, "broken local link"):
            check_docs.check_links(self.root)

    def test_prerequisite_cycle_fails(self):
        path = self.root / "docs/planning/milestones/DG-1.md"
        text = path.read_text().replace("- Prerequisites: DG-0.", "- Prerequisites: DG1-C12.", 1)
        path.write_text(text)
        with self.assertRaisesRegex(ValueError, "cycle"):
            check_docs.check_planning(self.root)

    def test_missing_test_field_fails(self):
        path = self.root / "docs/planning/milestones/DG-1.md"
        path.write_text(path.read_text().replace("- Tests (normal / failure / race):", "- Omitted:", 1))
        with self.assertRaisesRegex(ValueError, "missing field"):
            check_docs.check_planning(self.root)


if __name__ == "__main__":
    unittest.main()
