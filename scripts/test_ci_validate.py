"""Selectable validation stages: fixed order, refusals, runs without Rust and recorded omissions."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import uuid

import validate


def git(root, *args):
    environment = dict(os.environ, GIT_AUTHOR_NAME="CI", GIT_AUTHOR_EMAIL="ci@example.invalid",
                       GIT_COMMITTER_NAME="CI", GIT_COMMITTER_EMAIL="ci@example.invalid")
    command = ["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args]
    return subprocess.run(command, cwd=root, env=environment, check=True, capture_output=True,
                          text=True).stdout.strip()


class Repository:
    """A throwaway repository for the whitespace stage."""

    def __init__(self, root):
        self.root = Path(root)
        self.root.mkdir()
        git(self.root, "init", "-q", "-b", "main")

    def write(self, path, data):
        (self.root / path).parent.mkdir(parents=True, exist_ok=True)
        (self.root / path).write_bytes(data)

    def commit(self, message):
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "--allow-empty", "-m", message)
        return git(self.root, "rev-parse", "HEAD")

    def policy(self, exemptions):
        self.write(validate.POLICY, json.dumps({"whitespace": {"tree_exemptions": exemptions}}).encode())


class StageSelection(unittest.TestCase):
    def test_the_default_is_every_stage_but_whitespace_in_the_legacy_order(self):
        self.assertEqual(validate.STAGES, ("whitespace",) + validate.DEFAULT_STAGES)
        self.assertEqual(validate.DEFAULT_STAGES, ("source-contract", "documentation", "protocol-analysis",
                                                   "dependency-boundary", "format", "clippy", "contracts"))

    def test_a_selection_runs_in_the_fixed_order(self):
        self.assertEqual(validate.parse_stages("documentation, whitespace"), ["whitespace", "documentation"])

    def test_unknown_repeated_or_empty_selections_are_refused(self):
        for text in ("docs", "documentation,documentation", "", " , "):
            with self.subTest(text=text), self.assertRaises(ValueError):
                validate.parse_stages(text)

    def test_only_the_cargo_stages_need_rust(self):
        self.assertEqual(validate.RUST_STAGES, {"dependency-boundary", "format", "clippy", "contracts"})


class RunWithoutRust(unittest.TestCase):
    """Non-Rust stages run with no rustc or cargo reachable, and the report says what was left out."""

    def setUp(self):
        self.bin = tempfile.TemporaryDirectory()
        self.addCleanup(self.bin.cleanup)
        # Only git is reachable: the validator runs Python checks with its own interpreter.
        os.symlink(shutil.which("git"), Path(self.bin.name) / "git")
        self.output = validate.ROOT / "target" / "qualification" / ("test-ci-validate-" + uuid.uuid4().hex)
        self.addCleanup(shutil.rmtree, self.output, ignore_errors=True)

    def validate(self, *args):
        return subprocess.run([sys.executable, "-B", str(validate.ROOT / "scripts/validate.py"), *args,
                               "--output", str(self.output)],
                              cwd=validate.ROOT, env=dict(os.environ, PATH=self.bin.name),
                              capture_output=True, text=True)

    def test_selected_non_rust_stages_pass_without_a_toolchain(self):
        completed = self.validate("--stages", "documentation,source-contract,whitespace", "--diff-base", "HEAD")
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        report = json.loads((self.output / "report.json").read_text())
        self.assertEqual((report["status"], report["milestone"], report["execution_mode"]),
                         ("passed", None, "selected-stages"))
        self.assertIsNone(report["toolchain_matches"])
        self.assertNotIn("rustc", report)
        statuses = {stage["name"]: stage["status"] for stage in report["stages"]}
        self.assertEqual([stage["name"] for stage in report["stages"]], list(validate.STAGES))
        self.assertEqual(statuses, {
            "whitespace": "passed", "source-contract": "passed", "documentation": "passed",
            "protocol-analysis": "not_selected_by_plan", "dependency-boundary": "not_selected_by_plan",
            "format": "not_selected_by_plan", "clippy": "not_selected_by_plan", "contracts": "not_selected_by_plan"})
        whitespace = report["stages"][0]
        self.assertEqual((whitespace["mode"], whitespace["base"]), ("diff", whitespace["head"]))
        self.assertEqual(report["source"]["head"], whitespace["head"])

    def test_a_rust_stage_without_a_toolchain_fails_before_any_stage_runs(self):
        completed = self.validate("--stages", "source-contract,format")
        self.assertEqual(completed.returncode, 1)
        report = json.loads((self.output / "report.json").read_text())
        self.assertEqual(report["status"], "failed")
        self.assertEqual({stage["name"]: stage["status"] for stage in report["stages"]
                          if stage["name"] in ("source-contract", "format")},
                         {"source-contract": "not_run", "format": "not_run"})

    def test_a_diff_base_needs_the_whitespace_stage(self):
        completed = self.validate("--stages", "documentation", "--diff-base", "HEAD")
        self.assertEqual(completed.returncode, 2)
        self.assertIn("--diff-base applies only to the whitespace stage", completed.stderr)
        self.assertFalse(self.output.exists())


class Whitespace(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repo = Repository(Path(self.temporary.name) / "repo")
        self.output = Path(self.temporary.name) / "out"
        self.output.mkdir()

    def check(self, base):
        return validate.whitespace(self.output, base, root=self.repo.root)

    def test_a_diff_is_checked_for_its_own_lines_only(self):
        self.repo.write("old.md", b"trailing \n")
        base = self.repo.commit("base")
        self.repo.write("clean.md", b"clean\n")
        self.repo.commit("clean")
        self.assertEqual(self.check(base)["status"], "passed")
        self.repo.write("new.md", b"trailing \n")
        entry = self.check(base)
        self.assertEqual(entry["status"], "passed", "an uncommitted file is not part of HEAD")
        self.repo.commit("dirty")
        entry = self.check(base)
        self.assertEqual((entry["status"], entry["mode"], entry["base"]), ("failed", "diff", base))
        log = (self.output / entry["log"]).read_text()
        self.assertIn("new.md:1: trailing whitespace", log)
        self.assertNotIn("old.md", log)

    def test_the_whole_tree_exempts_only_pinned_bytes(self):
        license_text = b"License.\n\n"
        self.repo.write("LICENSE", license_text)
        self.repo.policy({"LICENSE": hashlib.sha256(license_text).hexdigest()})
        self.repo.commit("pinned")
        entry = self.check(None)
        self.assertEqual((entry["status"], entry["mode"], entry["base"]), ("passed", "tree", None))
        self.assertEqual((list(entry["exempt"]), entry["exemption_lost"]), (["LICENSE"], []))
        self.repo.write("LICENSE", b"Changed license.\n\n")
        self.repo.commit("changed")
        entry = self.check(None)
        self.assertEqual((entry["status"], entry["exempt"], entry["exemption_lost"]), ("failed", {}, ["LICENSE"]))
        self.assertIn("LICENSE:2: new blank line at EOF", (self.output / entry["log"]).read_text())

    def test_the_whole_tree_still_checks_every_other_file(self):
        self.repo.policy({})
        self.repo.write("guide.md", b"a\ttab and trailing  \n")
        self.repo.commit("dirty")
        entry = self.check(None)
        self.assertEqual(entry["status"], "failed")
        self.assertIn("guide.md:1: trailing whitespace", (self.output / entry["log"]).read_text())

    def test_an_unknown_base_is_an_error_not_a_pass(self):
        self.repo.policy({})
        self.repo.commit("base")
        with self.assertRaises(subprocess.CalledProcessError):
            self.check("f" * 40)


if __name__ == "__main__":
    unittest.main()
