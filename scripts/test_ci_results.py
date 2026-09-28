"""The required gate: plan re-derivation, job results, current-attempt evidence and policy-listed exceptions."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import check_ci_results as gate
import ci_plan
import ci_run
import qualify
import validate

POLICY, DIGEST = ci_plan.read_policy()
MACOS, UBUNTU = POLICY["contracts"]["platforms"]
ALLOWED_CARGO = POLICY["allowances"][MACOS]["dg1-cargo"][0]


def git(root, *args):
    env = dict(os.environ, GIT_AUTHOR_NAME="CI", GIT_AUTHOR_EMAIL="ci@example.invalid",
               GIT_COMMITTER_NAME="CI", GIT_COMMITTER_EMAIL="ci@example.invalid")
    command = ["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args]
    return subprocess.run(command, cwd=root, env=env, check=True, capture_output=True, text=True).stdout.strip()


def validator(source, selected, whitespace=None):
    """A validator report shaped like scripts/validate.py's."""
    stages = []
    for name in validate.STAGES:
        stage = {"name": name, "status": "passed" if name in selected else "not_selected_by_plan"}
        if name == "whitespace" and name in selected:
            stage.update(whitespace)
        stages.append(stage)
    complete = set(validate.DEFAULT_STAGES) <= set(selected)
    return {"schema": "devguard-qualification/v1", "status": "passed", "milestone": "DG-0" if complete else None,
            "toolchain_matches": True if complete else None, "source": {"head": source}, "stages": stages}


def suite_report(suite, source, status="passed", cases=()):
    stages = []
    for name, _, *expected in qualify.SUITES[suite]:
        stage = {"name": name, "status": "passed"}
        if expected:
            stage["not_run_cases"] = [case for case in cases if case["file"].startswith(f"raw/{name}/")]
            stage["status"] = "incomplete" if stage["not_run_cases"] else "passed"
        stages.append(stage)
    return {"schema": "devguard-functional-qualification/v1", "suite": suite, "status": status,
            "source": {"head": source}, "stages": stages}


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value))


class Run(unittest.TestCase):
    """One workflow run: a repository, its event, the plan, job results and uploaded artifacts."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "repo"
        self.root.mkdir()
        git(self.root, "init", "-q", "-b", "main")
        for path in ci_plan.PLANNING:
            if (ci_plan.ROOT / path).is_file():
                (self.root / path).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ci_plan.ROOT / path, self.root / path)
        (self.root / "docs/handoff").mkdir(parents=True)
        (self.root / "docs/handoff/record.md").write_text("record\n")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "base")
        self.base = git(self.root, "rev-parse", "HEAD")
        self.artifacts = Path(self.temporary.name) / "artifacts"

    def pull_request(self, path, attempt="2"):
        git(self.root, "checkout", "-q", "-b", "topic")
        (self.root / path).parent.mkdir(parents=True, exist_ok=True)
        (self.root / path).write_text("changed\n")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "head")
        head = git(self.root, "rev-parse", "HEAD")
        git(self.root, "checkout", "-q", "main")
        git(self.root, "merge", "-q", "--no-ff", "-m", "merge", "topic")
        self.use("pull_request", {"pull_request": {"head": {"sha": head}}}, attempt)

    def use(self, event_name, event, attempt="2"):
        self.source = git(self.root, "rev-parse", "HEAD")
        self.event = event
        self.env = {"GITHUB_EVENT_NAME": event_name, "GITHUB_SHA": self.source, "GITHUB_RUN_ID": "77",
                    "GITHUB_RUN_ATTEMPT": attempt, "GITHUB_REF": "refs/heads/main"}
        self.plan, _ = ci_plan.prepare(self.root, self.env, event)
        self.results = {"plan": {"result": "success"}, "repository": {"result": "success"},
                        "contracts": {"result": "success" if self.plan["rust"] else "skipped"}}
        self.upload(attempt)

    def upload(self, attempt, plan=None):
        plan = plan or self.plan
        write(self.artifacts / f"ci-plan-{attempt}" / "plan.json", plan)
        repository = self.artifacts / f"ci-repository-{attempt}"
        write(repository / "binding.json", ci_run.binding(plan, "repository"))
        write(repository / "validate" / "report.json",
              validator(self.source, plan["repository_stages"], plan["whitespace"]))
        if not plan["rust"]:
            return
        for platform in POLICY["contracts"]["platforms"]:
            directory = self.artifacts / f"dg0-{platform}-{attempt}"
            write(directory / "binding.json", ci_run.binding(plan, "contracts", platform))
            write(directory / "ci" / "report.json", validator(self.source, validate.DEFAULT_STAGES))
            suites = ci_run.planned_suites(POLICY, plan, platform)
            write(directory / "leg.json", {"suites": [{"suite": suite} for suite in suites]})
            for suite in suites:
                cases = []
                if suite == "dg1-cargo":
                    cases = [{"file": f"raw/native-cargo/{name}.json", "path": "", "reason": ALLOWED_CARGO["reason"]}
                             for name in ALLOWED_CARGO["cases"]]
                write(directory / suite / "report.json",
                      suite_report(suite, self.source, "incomplete" if cases else "passed", cases))

    def problems(self, plan=None, results=None, env=None, event=None):
        plan_text = json.dumps(plan if plan is not None else self.plan)
        return gate.problems(results or self.results, plan_text, env or self.env, event or self.event,
                             self.artifacts, self.root)[0]


class Passing(Run):
    def test_a_documentation_record_run_passes_without_contracts(self):
        self.pull_request("docs/handoff/2026-09-28-w3-decision-packet.md")
        self.assertEqual((self.plan["rust"], self.results["contracts"]["result"]), (False, "skipped"))
        found, table = gate.problems(self.results, json.dumps(self.plan), self.env, self.event, self.artifacts, self.root)
        self.assertEqual(found, [])
        self.assertIn(("repository contracts", "not_selected_by_plan"), table)
        self.assertIn((f"contracts {MACOS}", "not_selected_by_plan"), table)

    def test_a_component_run_passes_with_only_allowed_incomplete_suites(self):
        self.pull_request("crates/cargo/src/lib.rs")
        found, table = gate.problems(self.results, json.dumps(self.plan), self.env, self.event, self.artifacts, self.root)
        self.assertEqual(found, [])
        self.assertIn((f"contracts {MACOS} dg1-cargo", "incomplete-allowed"), table)
        self.assertIn((f"contracts {MACOS} dg1-scopes", "not_selected_by_plan"), table)

    def test_a_scheduled_run_is_full_and_passes(self):
        self.use("schedule", {})
        self.assertEqual((self.plan["profile"], self.plan["suites"][MACOS]), ("full", list(POLICY["suites"])))
        self.assertEqual(self.problems(), [])


class Jobs(Run):
    def test_every_job_must_have_its_planned_result(self):
        self.pull_request("crates/cargo/src/lib.rs")
        for job, result in (("repository", "failure"), ("contracts", "skipped"), ("contracts", "cancelled"),
                            ("contracts", "failure"), ("repository", "skipped")):
            with self.subTest(job=job, result=result):
                self.assertIn(f"{job}: {result}, expected success",
                              self.problems(results=dict(self.results, **{job: {"result": result}})))

    def test_contracts_must_be_skipped_when_rust_is_not_planned(self):
        self.pull_request("docs/handoff/new.md")
        self.assertEqual(self.problems(results=dict(self.results, contracts={"result": "success"})),
                         ["contracts: success, expected skipped"])

    def test_a_failed_plan_or_a_changed_job_set_fails(self):
        self.pull_request("docs/handoff/new.md")
        self.assertIn("without a plan", self.problems(results=dict(self.results, plan={"result": "failure"}))[0])
        for results in ({"plan": {"result": "success"}}, dict(self.results, extra={"result": "success"})):
            self.assertEqual(self.problems(results=results), ["the gate does not see exactly the required jobs"])


class PlanBinding(Run):
    def test_the_plan_must_be_the_one_derived_for_this_run(self):
        self.pull_request("crates/cargo/src/lib.rs")
        narrowed = json.loads(json.dumps(self.plan))
        narrowed["suites"][MACOS] = narrowed["suites"][MACOS][:1]
        for plan in (narrowed, dict(self.plan, profile="affected", rust=False), dict(self.plan, policy_sha256="0" * 64),
                     dict(self.plan, source_sha="c" * 40), dict(self.plan, run_id="78")):
            with self.subTest(plan={key: plan[key] for key in ("profile", "policy_sha256", "run_id")}):
                self.assertEqual(self.problems(plan=plan),
                                 ["the plan job's plan differs from the plan derived for this run"])

    def test_a_plan_from_an_earlier_attempt_never_satisfies_this_one(self):
        self.pull_request("docs/handoff/new.md")
        earlier = dict(self.plan, run_attempt="1")
        self.assertEqual(self.problems(plan=earlier), ["the plan is not from this attempt 2; re-run all jobs"])

    def test_an_event_without_a_plan_fails(self):
        self.pull_request("docs/handoff/new.md")
        found = self.problems(env=dict(self.env, GITHUB_EVENT_NAME="merge_group"))
        self.assertEqual(len(found), 1)
        self.assertIn("cannot be derived again: no plan is defined for the event merge_group", found[0])


class Evidence(Run):
    def test_evidence_from_an_earlier_attempt_is_ignored(self):
        self.pull_request("crates/cargo/src/lib.rs")
        for name in list(gate.artifact_names(self.artifacts)):
            (self.artifacts / name).rename(self.artifacts / (name.rsplit("-", 1)[0] + "-1"))
        found = self.problems()
        self.assertIn("ci-repository-2: no artifact from this attempt", found)
        self.assertIn(f"dg0-{MACOS}-2: no artifact from this attempt", found)
        self.upload("2")
        found, table = gate.problems(self.results, json.dumps(self.plan), self.env, self.event, self.artifacts, self.root)
        self.assertEqual(found, [])
        self.assertEqual(table[-1][0], "ignored: other attempts")

    def test_a_current_name_with_an_earlier_binding_fails(self):
        self.pull_request("crates/cargo/src/lib.rs")
        write(self.artifacts / f"dg0-{MACOS}-2" / "binding.json",
              ci_run.binding(dict(self.plan, run_attempt="1"), "contracts", MACOS))
        write(self.artifacts / "ci-repository-2" / "binding.json", ci_run.binding(self.plan, "contracts", MACOS))
        found = self.problems()
        self.assertIn(f"contracts {MACOS}: its evidence is not bound to this run and attempt", found)
        self.assertIn("repository: its evidence is not bound to this run and attempt", found)

    def test_evidence_the_plan_does_not_produce_fails(self):
        self.pull_request("docs/handoff/new.md")
        write(self.artifacts / f"dg0-{MACOS}-2" / "binding.json", {})
        self.assertEqual(self.problems(), [f"dg0-{MACOS}-2: an artifact the plan does not produce"])

    def test_the_uploaded_plan_must_be_the_planned_one(self):
        self.pull_request("docs/handoff/new.md")
        write(self.artifacts / "ci-plan-2" / "plan.json", dict(self.plan, profile="full"))
        self.assertEqual(self.problems(), ["ci-plan: the uploaded plan is not the planned one"])

    def test_repository_stages_must_be_exactly_the_planned_ones(self):
        self.pull_request("docs/handoff/new.md")
        path = self.artifacts / "ci-repository-2" / "validate" / "report.json"
        cases = {
            "an unplanned stage shown as passed": validator(self.source, ["whitespace", "documentation", "contracts"],
                                                            self.plan["whitespace"]),
            "a planned stage left out": validator(self.source, ["whitespace"], self.plan["whitespace"]),
            "another range": validator(self.source, self.plan["repository_stages"], {"mode": "tree", "base": None}),
            "another source": validator("c" * 40, self.plan["repository_stages"], self.plan["whitespace"]),
        }
        for label, report in cases.items():
            with self.subTest(case=label):
                write(path, report)
                self.assertTrue(self.problems())
        path.unlink()
        self.assertIn("repository: no validator report", self.problems())

    def test_the_contracts_jobs_need_the_complete_validator_and_exactly_the_planned_suites(self):
        self.pull_request("crates/cargo/src/lib.rs")
        directory = self.artifacts / f"dg0-{MACOS}-2"
        partial = validator(self.source, validate.DEFAULT_STAGES)
        partial["stages"][-1]["status"] = "failed"
        for label, change in (
                ("a failed stage", lambda: write(directory / "ci" / "report.json", partial)),
                ("an unplanned suite", lambda: write(directory / "dg1-scopes" / "report.json",
                                                     suite_report("dg1-scopes", self.source))),
                ("a missing suite", lambda: shutil.rmtree(directory / "dg1-upgrade")),
                ("a disallowed case", lambda: write(directory / "dg1-cli" / "report.json", suite_report(
                    "dg1-cli", self.source, "incomplete",
                    [{"file": "raw/native-exec/doctor.json", "path": "", "reason": ALLOWED_CARGO["reason"]}]))),
                ("another runner record", lambda: write(directory / "leg.json", {"suites": []}))):
            with self.subTest(case=label):
                shutil.rmtree(self.artifacts)
                self.upload("2")
                change()
                self.assertTrue(any(line.startswith(f"contracts {MACOS}") for line in self.problems()), label)


if __name__ == "__main__":
    unittest.main()
