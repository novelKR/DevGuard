"""The CI job runner: run binding, planned stages and suites only, and policy-listed hosted exceptions."""
import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import ci_plan
import ci_run
import qualify

POLICY, DIGEST = ci_plan.read_policy()
MACOS, UBUNTU = POLICY["contracts"]["platforms"]
CARGO = ["dg1-cli", "dg1-cargo", "dg1-self-use", "dg1-upgrade", "dg1-macos"]
CAPACITY = "this host's work capacity cannot fit the Cargo jobs the case needs"


def git(root, *args):
    env = dict(os.environ, GIT_AUTHOR_NAME="CI", GIT_AUTHOR_EMAIL="ci@example.invalid",
               GIT_COMMITTER_NAME="CI", GIT_COMMITTER_EMAIL="ci@example.invalid")
    command = ["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args]
    return subprocess.run(command, cwd=root, env=env, check=True, capture_output=True, text=True).stdout.strip()


def cargo_case(case, reason=CAPACITY):
    return {"file": f"raw/native-cargo/{case}.json", "path": "", "reason": reason}


def report(suite, source, status="passed", cases=()):
    """A functional report shaped like scripts/qualify.py's."""
    stages = []
    for name, _, *expected in qualify.SUITES[suite]:
        stage = {"name": name, "status": "passed"}
        if expected:
            stage["not_run_cases"] = [case for case in cases if case["file"].startswith(f"raw/{name}/")]
            if stage["not_run_cases"]:
                stage["status"] = "incomplete"
        stages.append(stage)
    return {"schema": "devguard-functional-qualification/v1", "suite": suite, "status": status,
            "source": {"head": source}, "stages": stages}


class Runner:
    """Stands in for subprocess.run: records commands and writes the report each suite would."""

    def __init__(self, root, source, reports=None, codes=None):
        self.root, self.source, self.commands = root, source, []
        self.reports, self.codes = reports or {}, codes or {}

    def __call__(self, command, cwd):
        self.commands.append(command)
        if command[1] == "scripts/qualify.py":
            suite = command[2]
            value = self.reports.get(suite, report(suite, self.source))
            if value is not None:
                path = Path(cwd) / command[command.index("--output") + 1] / "report.json"
                path.parent.mkdir(parents=True)
                path.write_text(json.dumps(value))
        return subprocess.CompletedProcess(command, self.codes.get(command[2] if len(command) > 2 else None, 0))


class Job(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        git(self.root, "init", "-q", "-b", "main")
        (self.root / "scripts").mkdir()
        shutil.copy2(ci_plan.ROOT / ci_plan.POLICY, self.root / ci_plan.POLICY)
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "source")
        self.source = git(self.root, "rev-parse", "HEAD")
        self.base = "b" * 40

    def plan(self, *paths, base=True, reasons=()):
        return ci_plan.build_plan(POLICY, DIGEST, "pull_request", self.source, self.base if base else None,
                                  list(paths), set(reasons), ("77", "2"))

    def env(self, plan, **changes):
        values = {"CI_PLAN": json.dumps(plan), "GITHUB_SHA": self.source, "GITHUB_EVENT_NAME": "pull_request",
                  "GITHUB_RUN_ID": "77", "GITHUB_RUN_ATTEMPT": "2", **changes}
        return {key: value for key, value in values.items() if value is not None}


class Binding(Job):
    def test_only_a_plan_for_this_commit_event_policy_run_and_attempt_is_accepted(self):
        plan = self.plan("docs/handoff/x.md")
        self.assertEqual(ci_run.load_plan(self.env(plan), self.root), plan)
        for changes in ({"GITHUB_SHA": "c" * 40}, {"GITHUB_EVENT_NAME": "push"}, {"GITHUB_RUN_ID": "78"},
                        {"GITHUB_RUN_ATTEMPT": "1"}, {"GITHUB_RUN_ATTEMPT": None}, {"CI_PLAN": ""},
                        {"CI_PLAN": "[]"}, {"CI_PLAN": json.dumps(dict(plan, policy_sha256="0" * 64))},
                        {"CI_PLAN": json.dumps(dict(plan, schema="other"))}):
            with self.subTest(changes=changes), self.assertRaises(ci_run.RunError):
                ci_run.load_plan(self.env(plan, **changes), self.root)

    def test_the_checkout_must_be_the_planned_source(self):
        plan = dict(self.plan("docs/handoff/x.md"), source_sha="c" * 40)
        with self.assertRaisesRegex(ci_run.RunError, "checkout"):
            ci_run.load_plan(self.env(plan, GITHUB_SHA="c" * 40), self.root)

    def test_the_binding_names_the_run_and_the_plan(self):
        plan = self.plan("crates/cargo/src/lib.rs")
        self.assertEqual(ci_run.binding(plan, "contracts", MACOS), {
            "schema": ci_run.BINDING_SCHEMA, "job": "contracts", "platform": MACOS, "source_sha": self.source,
            "event": "pull_request", "policy_sha256": DIGEST, "run_id": "77", "run_attempt": "2",
            "plan_sha256": ci_run.digest(plan)})


class Repository(Job):
    def test_the_planned_stages_run_against_the_planned_base(self):
        runner = Runner(self.root, self.source)
        self.assertEqual(ci_run.run_repository(self.env(self.plan("docs/handoff/x.md")), self.root, runner), 0)
        self.assertEqual(runner.commands, [[sys.executable, "-B", "scripts/validate.py", "--stages",
                                            "whitespace,documentation", "--output", "target/ci/repository/validate",
                                            "--diff-base", self.base]])
        out = self.root / ci_run.REPOSITORY_OUT
        self.assertEqual(json.loads((out / "binding.json").read_text())["job"], "repository")
        self.assertEqual(json.loads((out / "leg.json").read_text())["status"], "passed")

    def test_without_a_base_the_whole_tree_is_checked(self):
        runner = Runner(self.root, self.source)
        plan = self.plan(base=False, reasons={"event:schedule"})
        ci_run.run_repository(self.env(plan), self.root, runner)
        self.assertNotIn("--diff-base", runner.commands[0])
        self.assertIn("whitespace,source-contract,documentation,protocol-analysis", runner.commands[0])

    def test_a_failed_check_fails_the_job(self):
        runner = Runner(self.root, self.source, codes={"scripts/validate.py": 1})
        self.assertEqual(ci_run.run_repository(self.env(self.plan("docs/handoff/x.md")), self.root, runner), 1)
        leg = json.loads((self.root / ci_run.REPOSITORY_OUT / "leg.json").read_text())
        self.assertEqual((leg["status"], leg["exit_code"]), ("failed", 1))

    def test_a_rust_stage_is_never_a_repository_check(self):
        plan = self.plan("docs/handoff/x.md")
        plan["repository_stages"] = ["whitespace", "contracts"]
        with self.assertRaises(ci_run.RunError):
            ci_run.run_repository(self.env(plan), self.root, Runner(self.root, self.source))


class Contracts(Job):
    def run_suites(self, plan, platform=MACOS, **runner):
        env = self.env(plan)
        ci_run.bind_contracts(env, platform, self.root)
        stub = Runner(self.root, self.source, **runner)
        with contextlib.redirect_stdout(io.StringIO()):
            code = ci_run.run_contracts(env, platform, self.root, stub)
        leg = json.loads((self.root / ci_run.CONTRACTS_OUT / "leg.json").read_text())
        return code, stub, leg

    def test_planned_suites_run_in_order_with_allow_incomplete_only_where_the_policy_allows(self):
        cases = [cargo_case(name) for name in POLICY["allowances"][MACOS]["dg1-cargo"][0]["cases"]]
        code, stub, leg = self.run_suites(self.plan("crates/cargo/src/lib.rs"), reports={
            "dg1-cargo": report("dg1-cargo", self.source, "incomplete", cases)})
        self.assertEqual(code, 0)
        self.assertEqual([command[2] for command in stub.commands], CARGO)
        flagged = {command[2] for command in stub.commands if "--allow-incomplete" in command}
        self.assertEqual(flagged, {"dg1-cargo", "dg1-macos"})
        self.assertEqual({result["suite"]: result["status"] for result in leg["suites"]},
                         {**{suite: "passed" for suite in CARGO}, "dg1-cargo": "incomplete-allowed"})
        self.assertEqual(leg["status"], "passed")

    def test_a_case_not_run_for_an_unlisted_reason_fails_but_the_rest_still_run(self):
        code, stub, leg = self.run_suites(self.plan("crates/cargo/src/lib.rs"), reports={
            "dg1-cargo": report("dg1-cargo", self.source, "incomplete", [cargo_case("direct-build", "no reason")])})
        self.assertEqual(code, 1)
        self.assertEqual([command[2] for command in stub.commands], CARGO)
        failed = [result for result in leg["suites"] if result["status"] == "failed"]
        self.assertEqual([result["suite"] for result in failed], ["dg1-cargo"])
        self.assertIn("not allowed to be not run", failed[0]["problems"][0])

    def test_an_allowance_covers_only_its_own_suite_and_case(self):
        stray = {"file": "raw/native-upgrade/drain-timeout.json", "path": "", "reason": CAPACITY}
        code, _, leg = self.run_suites(self.plan("crates/cargo/src/lib.rs"), reports={
            "dg1-upgrade": report("dg1-upgrade", self.source, "incomplete", [stray])}, codes={"dg1-upgrade": 2})
        self.assertEqual(code, 1)
        self.assertEqual({result["suite"] for result in leg["suites"] if result["status"] == "failed"},
                         {"dg1-upgrade"})

    def test_a_failed_or_missing_report_fails_and_later_suites_still_run(self):
        code, stub, leg = self.run_suites(self.plan("crates/cargo/src/lib.rs"),
                                          reports={"dg1-cli": None}, codes={"dg1-cli": 1})
        self.assertEqual(code, 1)
        self.assertEqual([command[2] for command in stub.commands], CARGO)
        self.assertEqual([result["suite"] for result in leg["suites"] if result["status"] == "failed"], ["dg1-cli"])

    def test_only_suites_planned_for_this_platform_start(self):
        code, stub, leg = self.run_suites(self.plan("crates/cargo/src/lib.rs"), platform=UBUNTU)
        self.assertEqual((code, stub.commands, leg["suites"]), (0, [], []))
        _, stub, _ = self.run_suites_fresh(self.plan("crates/core/src/lib.rs"), UBUNTU)
        self.assertEqual([command[2] for command in stub.commands], ["dg1-authority", "dg1-auth"])

    def run_suites_fresh(self, plan, platform):
        shutil.rmtree(self.root / ci_run.CONTRACTS_OUT)
        return self.run_suites(plan, platform)

    def test_suites_need_the_binding_written_before_the_validator(self):
        plan = self.plan("crates/cargo/src/lib.rs")
        with self.assertRaisesRegex(ci_run.RunError, "not bound"):
            ci_run.run_contracts(self.env(plan), MACOS, self.root, Runner(self.root, self.source))
        ci_run.bind_contracts(self.env(plan), UBUNTU, self.root)
        with self.assertRaisesRegex(ci_run.RunError, "not bound"):
            ci_run.run_contracts(self.env(plan), MACOS, self.root, Runner(self.root, self.source))

    def test_a_plan_without_rust_or_an_unknown_platform_binds_nothing(self):
        with self.assertRaisesRegex(ci_run.RunError, "does not select"):
            ci_run.bind_contracts(self.env(self.plan("docs/handoff/x.md")), MACOS, self.root)
        with self.assertRaises(ci_run.RunError):
            ci_run.bind_contracts(self.env(self.plan("crates/cargo/src/lib.rs")), "windows-2022", self.root)
        self.assertFalse((self.root / ci_run.CONTRACTS_OUT / "binding.json").exists())

    def test_a_plan_that_names_an_unknown_or_misplaced_suite_is_refused(self):
        for suites in (["dg1-other"], ["dg1-scopes"], ["dg1-auth", "dg1-auth"]):
            plan = self.plan("crates/core/src/lib.rs")
            plan["suites"][UBUNTU] = suites
            with self.subTest(suites=suites), self.assertRaises(ci_run.RunError):
                ci_run.planned_suites(POLICY, plan, UBUNTU)


class Reports(Job):
    def check(self, value, suite="dg1-upgrade", platform=MACOS):
        return ci_run.evaluate_suite(POLICY, platform, suite, value, self.source)

    def test_only_a_complete_matching_report_passes(self):
        self.assertEqual(self.check(report("dg1-upgrade", self.source)), ("passed", []))
        partial = report("dg1-upgrade", self.source)
        partial["stages"] = partial["stages"][:-1]
        stray = report("dg1-upgrade", self.source)
        stray["stages"][-1]["not_run_cases"] = [cargo_case("direct-build")]
        for value in (None, report("dg1-cargo", self.source), report("dg1-upgrade", "c" * 40), partial, stray,
                      report("dg1-upgrade", self.source, "failed"), report("dg1-upgrade", self.source, "not_run"),
                      report("dg1-upgrade", self.source, "incomplete")):
            with self.subTest(value=value):
                self.assertEqual(self.check(value)[0], "failed")

    def test_allowances_are_per_platform(self):
        value = report("dg1-scopes", self.source, "incomplete", [{
            "file": "raw/native-scopes/scope-refusals.json", "path": "unclamped_root",
            "reason": POLICY["allowances"][MACOS]["dg1-scopes"][0]["reason"]}])
        self.assertEqual(self.check(value, "dg1-scopes"), ("incomplete-allowed", []))
        self.assertEqual(self.check(value, "dg1-scopes", UBUNTU)[0], "failed")
        value["stages"][-1]["not_run_cases"][0]["path"] = ""
        self.assertEqual(self.check(value, "dg1-scopes")[0], "failed")

    def test_an_allowed_case_must_be_in_its_own_incomplete_stage(self):
        case = cargo_case("direct-build")
        value = report("dg1-cargo", self.source, "incomplete", [case])
        native = next(stage for stage in value["stages"] if stage["name"] == "native-cargo")
        other = next(stage for stage in value["stages"] if stage["name"] == "cargo-plan")
        native["not_run_cases"] = []
        native["status"] = "passed"
        other["not_run_cases"] = [case]
        other["status"] = "incomplete"
        status, problems = self.check(value, "dg1-cargo")
        self.assertEqual(status, "failed")
        self.assertTrue(any("not allowed to be not run in cargo-plan" in problem for problem in problems))

    def test_each_incomplete_stage_needs_a_case_and_cases_cannot_repeat(self):
        unexplained = report("dg1-cargo", self.source, "incomplete", [])
        unexplained["stages"][0]["status"] = "incomplete"
        status, problems = self.check(unexplained, "dg1-cargo")
        self.assertEqual(status, "failed")
        self.assertTrue(any("expected passed" in problem for problem in problems))
        duplicated = report("dg1-cargo", self.source, "incomplete", [cargo_case("direct-build")])
        native = next(stage for stage in duplicated["stages"] if stage["name"] == "native-cargo")
        native["not_run_cases"].append(dict(native["not_run_cases"][0]))
        status, problems = self.check(duplicated, "dg1-cargo")
        self.assertEqual(status, "failed")
        self.assertTrue(any("repeats a not-run case" in problem for problem in problems))


class Summary(Job):
    def test_the_summary_says_what_ran_and_what_the_plan_left_out(self):
        plan = self.plan("crates/cargo/src/lib.rs")
        out = self.root / ci_run.CONTRACTS_OUT
        ci_run.write_json(out / "ci" / "report.json", {"status": "passed"})
        ci_run.write_json(out / "dg1-cli" / "report.json", {"status": "passed"})
        summary = self.root / "summary.md"
        ci_run.summarize(dict(self.env(plan), GITHUB_STEP_SUMMARY=str(summary)), MACOS, self.root)
        text = summary.read_text()
        self.assertIn("| validator (ci) | passed |", text)
        self.assertIn("| dg1-cli | passed |", text)
        self.assertIn("| dg1-cargo | not_run (not reached) |", text)
        self.assertIn("| dg1-scopes | not_selected_by_plan |", text)
        self.assertIn(ci_run.SCOPE, text)
        ci_run.summarize(dict(self.env(plan), GITHUB_STEP_SUMMARY=str(summary)), UBUNTU, self.root)
        self.assertIn("| dg1-scopes | not_run (native macOS suite) |", summary.read_text())


if __name__ == "__main__":
    unittest.main()
