"""The CI plan: path classes, suite selection, event binding and the policy's agreement with the repository."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock

import ci_plan
import qualify
import validate

ROOT = ci_plan.ROOT
POLICY, DIGEST = ci_plan.read_policy()
MACOS, UBUNTU = POLICY["contracts"]["platforms"]
ALL = list(POLICY["suites"])
PORTABLE = [suite for suite in ALL if UBUNTU in POLICY["suites"][suite]["platforms"]]
REPOSITORY = POLICY["repository"]["full"]
# The one-file diff of PR #17 (base 9e21cc8f, head 70c814c9).
PR17 = "docs/handoff/2026-09-28-w3-decision-packet.md"


def plan(*paths, base="e" * 40, reasons=()):
    return ci_plan.build_plan(POLICY, DIGEST, "local", "f" * 40, base, list(paths), set(reasons))


def suites(*names):
    return [suite for suite in ALL if suite in names]


def workspace():
    members = re.search(r"members = \[([^\]]*)\]", (ROOT / "Cargo.toml").read_text()).group(1)
    return sorted(Path(member).name for member in re.findall(r'"([^"]+)"', members))


def tracked():
    out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, check=True, capture_output=True).stdout
    return [os.fsdecode(path) for path in out.split(b"\0") if path]


class Selection(unittest.TestCase):
    def test_pr17_documentation_record_selects_no_rust_and_no_suite(self):
        record = plan(PR17)
        self.assertEqual((record["profile"], record["reasons"], record["repository_stages"], record["rust"]),
                         ("affected", [], ["whitespace", "documentation"], False))
        self.assertEqual(record["suites"], {MACOS: [], UBUNTU: []})
        self.assertFalse(any("dg1-upgrade" in names for names in record["suites"].values()))
        self.assertEqual(record["whitespace"], {"mode": "diff", "base": "e" * 40})

    def test_historical_records_and_normative_documents_compile_nothing(self):
        for paths in ([PR17, "docs/handoff/2026-09-27-session-close.md"],
                      ["docs/contracts.md", "docs/ko/contracts.md", "docs/translations.json"],
                      ["docs/planning/verification.md", "docs/ko/planning/verification.md"],
                      ["AGENTS.md"], ["README.md"], ["LICENSE", "NOTICE"], ["docs/design-revision-1.md"]):
            with self.subTest(paths=paths):
                result = plan(*paths)
                self.assertEqual((result["profile"], result["rust"], result["repository_stages"]),
                                 ("affected", False, ["whitespace", "documentation"]))

    def test_source_contract_inputs_add_only_the_source_contract_check(self):
        for path in POLICY["repository"]["source_contract_inputs"]:
            with self.subTest(path=path):
                result = plan(path)
                self.assertEqual((result["profile"], result["rust"], result["repository_stages"]),
                                 ("affected", False, ["whitespace", "source-contract", "documentation"]))

    def test_a_component_runs_the_complete_validator_and_its_suites(self):
        rows = {
            "crates/launch/src/main.rs": suites("dg1-launch", "dg1-reconcile", "dg1-cli", "dg1-cargo",
                                                "dg1-self-use", "dg1-macos"),
            "crates/cargo/src/lib.rs": suites("dg1-cli", "dg1-cargo", "dg1-self-use", "dg1-upgrade", "dg1-macos"),
            "crates/cli/tests/exec.rs": suites("dg1-cli", "dg1-cargo", "dg1-self-use", "dg1-upgrade", "dg1-macos"),
            "crates/qualify/fixture/foreground.html": ["dg1-macos"],
            "crates/macos/src/scope.rs": ALL,
            "crates/core/src/journal.rs": ALL,
            "crates/contract/src/lib.rs": ALL,
            "crates/daemon/tests/upgrade.rs": [suite for suite in ALL if suite != "dg1-scopes"],
            "crates/client/src/protocol.rs": [suite for suite in ALL if suite != "dg1-scopes"],
        }
        for path, expected in rows.items():
            with self.subTest(path=path):
                result = plan(path)
                self.assertEqual((result["profile"], result["rust"], result["repository_stages"]),
                                 ("affected", True, ["whitespace"]))
                self.assertEqual(result["suites"][MACOS], expected)
                self.assertEqual(result["suites"][UBUNTU], [suite for suite in expected if suite in PORTABLE])

    def test_documents_and_a_component_together_take_both(self):
        result = plan("docs/contracts.md", "crates/cargo/src/lib.rs")
        self.assertEqual((result["profile"], result["repository_stages"], result["components"]),
                         ("affected", ["whitespace", "documentation"], ["cargo"]))
        self.assertEqual(result["suites"][MACOS], plan("crates/cargo/src/lib.rs")["suites"][MACOS])

    def test_full_inputs_and_unknown_paths_run_everything(self):
        for path in (".github/workflows/ci.yml", "scripts/ci-policy.json", "scripts/qualify.py",
                     "scripts/check_docs.py", "Cargo.toml", "Cargo.lock", "crates/cli/Cargo.toml",
                     "rust-toolchain.toml", ".cargo/config.toml", "crates/core/.cargo/config.toml", ".gitignore",
                     ".devguard.toml", "Makefile", "docs/new-guide.md", "crates/README.md", ".kiro/settings.json"):
            with self.subTest(path=path):
                result = plan(PR17, path)
                self.assertEqual((result["profile"], result["rust"], result["repository_stages"]),
                                 ("full", True, REPOSITORY))
                self.assertEqual(result["suites"], {MACOS: ALL, UBUNTU: PORTABLE})
        self.assertEqual(plan("Makefile")["reasons"], ["unclassified:Makefile"])
        self.assertEqual(plan("Cargo.lock")["reasons"], ["full-path:**/Cargo.lock"])

    def test_an_empty_diff_and_given_reasons_are_full(self):
        self.assertEqual(plan()["reasons"], ["empty-diff"])
        self.assertEqual(plan(PR17, reasons={"planning-changed"})["profile"], "full")

    def test_without_a_base_whitespace_covers_the_whole_tree(self):
        result = plan(base=None, reasons={"event:schedule"})
        self.assertEqual(result["whitespace"], {"mode": "tree", "base": None})
        self.assertEqual(result["profile"], "full")

    def test_unclassified_reasons_are_capped_but_counted(self):
        result = plan(*[f"unknown/{index:02}" for index in range(12)])
        self.assertEqual(len(result["reasons"]), ci_plan.SHOWN + 1)
        self.assertIn("unclassified:2 more", result["reasons"])
        self.assertEqual(result["paths"]["unclassified"], 12)

    def test_globs_respect_segments(self):
        self.assertTrue(ci_plan.matches("**/Cargo.toml", "Cargo.toml"))
        self.assertTrue(ci_plan.matches("**/Cargo.toml", "crates/cli/Cargo.toml"))
        self.assertTrue(ci_plan.matches("docs/ko/**", "docs/ko/planning/README.md"))
        self.assertTrue(ci_plan.matches("docs/handoff/**", "docs/handoff/a\nb.md"))
        self.assertFalse(ci_plan.matches("docs/handoff/**", "docs/handoff"))
        self.assertFalse(ci_plan.matches("README.md", "crates/cli/README.md"))
        self.assertFalse(ci_plan.matches("crates/*/src", "crates/a/b/src"))
        self.assertFalse(ci_plan.matches("docs/design.md", "docs/designXmd"))

    def test_the_path_digest_covers_every_changed_path(self):
        self.assertEqual(plan("b", "a")["paths"]["sha256"], hashlib.sha256(b"a\0b").hexdigest())
        self.assertNotEqual(plan("a")["paths"]["sha256"], plan("a", "b")["paths"]["sha256"])


class Repo:
    """A throwaway repository holding the real planning inputs."""

    def __init__(self, root):
        self.root = Path(root)
        self.git("init", "-q", "-b", "main")
        for path in ci_plan.PLANNING:
            if (ROOT / path).is_file():
                self.write(path, (ROOT / path).read_text())
        self.write("docs/handoff/record.md", "record\n")
        self.base = self.commit("base")

    def git(self, *args):
        env = dict(os.environ, GIT_AUTHOR_NAME="CI", GIT_AUTHOR_EMAIL="ci@example.invalid",
                   GIT_COMMITTER_NAME="CI", GIT_COMMITTER_EMAIL="ci@example.invalid")
        command = ["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args]
        return subprocess.run(command, cwd=self.root, env=env, check=True, capture_output=True).stdout.decode().strip()

    def write(self, path, text):
        (self.root / path).parent.mkdir(parents=True, exist_ok=True)
        (self.root / path).write_text(text)

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", message)
        return self.git("rev-parse", "HEAD")

    def pull_request(self, path=None, text="changed\n"):
        self.git("checkout", "-q", "-b", "topic", self.base)
        if path:
            self.write(path, text)
        head = self.commit("head")
        self.git("checkout", "-q", "main")
        self.git("merge", "-q", "--no-ff", "-m", "merge", "topic")
        return head, self.git("rev-parse", "HEAD")

    def prepare(self, event, name, sha=None, **env):
        values = {"GITHUB_EVENT_NAME": name, "GITHUB_SHA": sha or self.git("rev-parse", "HEAD"),
                  "GITHUB_RUN_ID": "4242", "GITHUB_RUN_ATTEMPT": "1", "GITHUB_REF": "refs/heads/main", **env}
        return ci_plan.prepare(self.root, {key: value for key, value in values.items() if value is not None}, event)


class Events(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repo = Repo(self.temporary.name)

    def test_a_pull_request_plans_its_merge_against_the_base(self):
        head, merge = self.repo.pull_request("crates/cargo/src/lib.rs")
        result, paths = self.repo.prepare({"pull_request": {"head": {"sha": head}}}, "pull_request")
        self.assertEqual((result["source_sha"], result["base_sha"], result["event"]),
                         (merge, self.repo.base, "pull_request"))
        self.assertEqual((result["profile"], result["components"], paths), ("affected", ["cargo"],
                                                                            ["crates/cargo/src/lib.rs"]))
        self.assertEqual((result["run_id"], result["run_attempt"], result["policy_sha256"]), ("4242", "1", DIGEST))

    def test_a_documentation_record_pull_request_selects_no_rust(self):
        head, _ = self.repo.pull_request("docs/handoff/2026-09-28-record.md")
        result, _ = self.repo.prepare({"pull_request": {"head": {"sha": head}}}, "pull_request")
        self.assertEqual((result["profile"], result["rust"], result["repository_stages"]),
                         ("affected", False, ["whitespace", "documentation"]))

    def test_a_changed_planning_input_is_full(self):
        head, _ = self.repo.pull_request(ci_plan.POLICY, (ROOT / ci_plan.POLICY).read_text() + "\n")
        result, _ = self.repo.prepare({"pull_request": {"head": {"sha": head}}}, "pull_request")
        self.assertIn("planning-changed", result["reasons"])
        self.assertEqual((result["profile"], result["suites"]), ("full", {MACOS: ALL, UBUNTU: PORTABLE}))

    def test_an_empty_merge_is_full(self):
        head, _ = self.repo.pull_request()
        result, _ = self.repo.prepare({"pull_request": {"head": {"sha": head}}}, "pull_request")
        self.assertEqual(result["reasons"], ["empty-diff"])

    def test_the_checkout_must_be_the_event_commit(self):
        head, _ = self.repo.pull_request("docs/handoff/new.md")
        with self.assertRaisesRegex(ci_plan.PlanError, "GITHUB_SHA"):
            self.repo.prepare({"pull_request": {"head": {"sha": head}}}, "pull_request", sha=head)

    def test_a_merge_of_another_head_is_full_without_a_base(self):
        self.repo.pull_request("docs/handoff/new.md")
        result, paths = self.repo.prepare({"pull_request": {"head": {"sha": self.repo.base}}}, "pull_request")
        self.assertEqual((result["reasons"], result["base_sha"], paths), (["pull-request-base-untrusted"], None, []))
        self.assertEqual(result["whitespace"], {"mode": "tree", "base": None})

    def test_a_push_to_main_plans_before_to_after(self):
        self.repo.write("docs/handoff/record.md", "more\n")
        after = self.repo.commit("docs")
        result, _ = self.repo.prepare({"before": self.repo.base}, "push")
        self.assertEqual((result["source_sha"], result["base_sha"], result["profile"], result["rust"]),
                         (after, self.repo.base, "affected", False))

    def test_a_push_without_a_trusted_base_is_full(self):
        self.repo.write("docs/handoff/record.md", "more\n")
        self.repo.commit("docs")
        self.repo.git("checkout", "-q", "--orphan", "other")
        unrelated = self.repo.commit("unrelated")
        self.repo.git("checkout", "-q", "main")
        for event in ({"before": "0" * 40}, {"before": "1" * 40}, {"before": unrelated}, {}, {"before": "HEAD~1"},
                      {"before": self.repo.base, "forced": True}):
            with self.subTest(event=event):
                result, _ = self.repo.prepare(event, "push")
                self.assertEqual((result["reasons"], result["base_sha"], result["profile"]),
                                 (["push-base-untrusted"], None, "full"))

    def test_a_push_outside_main_has_no_plan(self):
        with self.assertRaisesRegex(ci_plan.PlanError, "refs/heads/main"):
            self.repo.prepare({"before": self.repo.base}, "push", GITHUB_REF="refs/heads/topic")

    def test_scheduled_and_manual_runs_are_full(self):
        for name in ("schedule", "workflow_dispatch"):
            with self.subTest(event=name):
                result, paths = self.repo.prepare({}, name)
                self.assertEqual((result["profile"], result["reasons"], result["base_sha"], paths),
                                 ("full", ["event:" + name], None, []))
                self.assertEqual((result["repository_stages"], result["whitespace"]["mode"]), (REPOSITORY, "tree"))

    def test_other_events_have_no_plan(self):
        for name in ("merge_group", "issue_comment", "pull_request_target", ""):
            with self.subTest(event=name), self.assertRaisesRegex(ci_plan.PlanError, "no plan is defined"):
                self.repo.prepare({}, name)

    def test_the_run_identity_is_required(self):
        for values in ({"GITHUB_RUN_ID": None}, {"GITHUB_RUN_ATTEMPT": None}, {"GITHUB_RUN_ATTEMPT": "0"}):
            with self.subTest(values=values), self.assertRaisesRegex(ci_plan.PlanError, "run id or attempt"):
                self.repo.prepare({}, "schedule", **values)


class Outputs(unittest.TestCase):
    def test_outputs_are_single_ascii_lines_that_round_trip(self):
        result = plan(PR17, "docs/handoff/caf\u00e9\nnote.md", "unknown/\u2028x")
        lines = ci_plan.outputs(result)
        self.assertEqual([line.split("=", 1)[0] for line in lines], ["plan", "rust", "profile"])
        for line in lines:
            self.assertNotIn("\n", line)
            self.assertTrue(line.isascii())
        self.assertEqual(json.loads(lines[0].split("=", 1)[1]), result)
        self.assertEqual(lines[1:], ["rust=true", "profile=full"])
        self.assertEqual(ci_plan.outputs(plan(PR17))[1:], ["rust=false", "profile=affected"])

    def test_the_summary_names_what_was_left_out(self):
        text = ci_plan.summary(plan(PR17))
        self.assertIn("Rust validation: not selected by plan", text)
        self.assertIn(f"Functional suites on {MACOS}: none", text)


def closure(package):
    seen, todo = set(), [package]
    while todo:
        name = todo.pop()
        if name not in seen:
            seen.add(name)
            todo += validate.WORKSPACE_GRAPH[name] - {name}
    return seen


def suite_packages(suite):
    """Workspace packages a suite tests, and those whose binaries it builds first."""
    packages = set()
    for _, selectors, *_ in qualify.SUITES[suite]:
        if selectors[0] != "unittest":
            packages.add(selectors[selectors.index("-p") + 1])
    for selectors in qualify.PREBUILD.get(suite, []):
        packages.add(selectors[selectors.index("-p") + 1])
    return packages


class PolicyAgreesWithTheRepository(unittest.TestCase):
    def test_components_are_the_workspace_members(self):
        self.assertEqual(sorted(POLICY["components"]), workspace())
        self.assertEqual(set(validate.WORKSPACE_GRAPH), {"devguard-" + name for name in workspace()})
        for name, patterns in POLICY["components"].items():
            self.assertEqual(patterns, [f"crates/{name}/**"])

    def test_the_suite_map_is_the_dependency_closure_of_each_suite(self):
        self.assertEqual(set(ALL), set(qualify.SUITES))
        for suite in ALL:
            with self.subTest(suite=suite):
                packages = set().union(*(closure(package) for package in suite_packages(suite)))
                expected = sorted(package.removeprefix("devguard-") for package in packages)
                self.assertEqual(POLICY["suites"][suite]["components"], expected)

    def test_native_suites_run_only_on_macos(self):
        for suite in ALL:
            with self.subTest(suite=suite):
                expected = [MACOS] if suite in qualify.NATIVE else [MACOS, UBUNTU]
                self.assertEqual(POLICY["suites"][suite]["platforms"], expected)

    def test_source_contract_inputs_are_the_files_it_reads(self):
        read = set()
        originals = {name: getattr(Path, name) for name in ("read_text", "read_bytes")}

        def recording(name):
            def method(self, *args, **kwargs):
                read.add(self.resolve().relative_to(ROOT.resolve()).as_posix())
                return originals[name](self, *args, **kwargs)
            return method

        with mock.patch.object(Path, "read_text", recording("read_text")), \
                mock.patch.object(Path, "read_bytes", recording("read_bytes")):
            validate.source_contract()
        self.assertEqual(read, set(POLICY["repository"]["source_contract_inputs"]))

    def test_every_tracked_path_is_classified(self):
        unclassified = [path for path in tracked() if ci_plan.classify(POLICY, path) == (None, None)]
        self.assertEqual(unclassified, [])

    def test_every_document_pattern_matches_a_tracked_file(self):
        files = tracked()
        for pattern in POLICY["classes"]["historical"] + POLICY["classes"]["normative"]:
            with self.subTest(pattern=pattern):
                self.assertTrue(any(ci_plan.matches(pattern, path) for path in files))

    def test_validation_and_planning_inputs_are_full(self):
        paths = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".gitignore", ".devguard.toml",
                 *ci_plan.PLANNING, *(f"crates/{name}/Cargo.toml" for name in workspace())]
        paths += [path for path in tracked() if path.startswith(("scripts/", ".github/"))]
        for path in paths:
            with self.subTest(path=path):
                self.assertEqual(ci_plan.classify(POLICY, path)[0], "full")

    def test_repository_stages_are_non_rust_validator_stages_in_order(self):
        self.assertEqual(REPOSITORY, [stage for stage in validate.STAGES if stage in REPOSITORY])
        self.assertFalse(set(REPOSITORY) & validate.RUST_STAGES)
        for key in ("always", "documents"):
            self.assertLessEqual(set(POLICY["repository"][key]), set(REPOSITORY))
        self.assertIn("whitespace", POLICY["repository"]["always"])

    def test_suite_platforms_are_the_contract_platforms(self):
        used = {platform for spec in POLICY["suites"].values() for platform in spec["platforms"]}
        self.assertEqual(used, set(POLICY["contracts"]["platforms"]))

    def test_allowances_name_real_cases_and_quote_their_sources(self):
        for platform, allowed in POLICY["allowances"].items():
            for suite, entries in allowed.items():
                self.assertIn(platform, POLICY["suites"][suite]["platforms"])
                stages = {name: expected[0] for name, _, *expected in qualify.SUITES[suite] if expected}
                for entry in entries:
                    with self.subTest(platform=platform, suite=suite, stage=entry["stage"]):
                        self.assertEqual(set(entry), {"stage", "cases", "path", "reason", "source"})
                        self.assertLessEqual(set(entry["cases"]), set(stages[entry["stage"]]))
                        self.assertIn(entry["reason"], (ROOT / entry["source"]).read_text())

    def test_whitespace_exemptions_are_pinned_to_the_current_bytes(self):
        exemptions = POLICY["whitespace"]["tree_exemptions"]
        for path, digest in exemptions.items():
            with self.subTest(path=path):
                self.assertEqual(hashlib.sha256((ROOT / path).read_bytes()).hexdigest(), digest)
        design = json.loads((ROOT / "docs/design-source.json").read_text())
        self.assertEqual(exemptions[design["document"]], design["sha256"])


if __name__ == "__main__":
    unittest.main()
