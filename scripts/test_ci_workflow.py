"""The workflow consumes the plan: job wiring, the unskippable gate, triggers and no inline policy."""
import re
import unittest

import check_ci_results
import ci_plan

WORKFLOW = (ci_plan.ROOT / ".github/workflows/ci.yml").read_text()
POLICY, _ = ci_plan.read_policy()


def jobs(text):
    body = text.split("\njobs:\n", 1)[1]
    parts = re.split(r"^  ([a-z0-9-]+):\n", body, flags=re.M)
    return dict(zip(parts[1::2], parts[2::2]))


def steps(job):
    return re.split(r"^      - ", job, flags=re.M)[1:]


class Workflow(unittest.TestCase):
    def setUp(self):
        self.jobs = jobs(WORKFLOW)

    def test_the_jobs_are_the_gate_requirements_and_the_gate(self):
        self.assertEqual(set(self.jobs), check_ci_results.REQUIRED_JOBS | {"required"})

    def test_the_gate_always_runs_needs_every_job_and_has_a_stable_name(self):
        gate = self.jobs["required"]
        self.assertIn("    name: Required checks\n", gate)
        self.assertIn("    if: ${{ always() }}\n", gate)
        needs = re.search(r"^    needs: \[([^\]]*)\]$", gate, re.M).group(1)
        self.assertEqual({name.strip() for name in needs.split(",")}, check_ci_results.REQUIRED_JOBS)
        self.assertIn("CI_RESULTS: ${{ toJSON(needs) }}", gate)
        self.assertIn("CI_PLAN: ${{ needs.plan.outputs.plan }}", gate)
        self.assertIn("run: python3 -B scripts/check_ci_results.py --artifacts target/ci-artifacts", gate)
        download = next(step for step in steps(gate) if "download-artifact" in step)
        self.assertIn("uses: actions/download-artifact@v7", download)
        self.assertIn("path: target/ci-artifacts", download)
        # Every artifact of the run, each in its own directory; the gate picks this attempt's.
        for key in ("name:", "pattern:", "merge-multiple:", "run-id:", "github-token:"):
            self.assertNotIn(key, download.split("with:", 1)[1])
        self.assertIn("fetch-depth: 0", gate)

    def test_the_plan_job_verifies_the_policy_and_exports_the_plan(self):
        plan = self.jobs["plan"]
        self.assertIn("fetch-depth: 0", plan)
        self.assertIn("run: python3 -B -m unittest discover -s scripts -p 'test_ci_*.py'", plan)
        self.assertIn("run: python3 -B scripts/ci_plan.py", plan)
        self.assertLess(plan.index("test_ci_*.py"), plan.index("scripts/ci_plan.py"))
        for output in ("plan", "rust"):
            self.assertIn(f"      {output}: ${{{{ steps.plan.outputs.{output} }}}}\n", plan)
        self.assertIn("name: ci-plan-${{ github.run_attempt }}", plan)

    def test_repository_checks_always_follow_the_plan(self):
        repository = self.jobs["repository"]
        self.assertIn("    needs: plan\n", repository)
        self.assertNotIn("\n    if:", repository)
        self.assertIn("fetch-depth: 0", repository)
        self.assertIn("CI_PLAN: ${{ needs.plan.outputs.plan }}", repository)
        self.assertIn("run: python3 -B scripts/ci_run.py repository", repository)
        self.assertIn("name: ci-repository-${{ github.run_attempt }}", repository)

    def test_contracts_keep_the_legacy_validation_and_run_only_planned_suites(self):
        contracts = self.jobs["contracts"]
        self.assertIn("    needs: plan\n", contracts)
        self.assertIn("    if: ${{ needs.plan.outputs.rust == 'true' }}\n", contracts)
        platforms = re.search(r"^        os: \[([^\]]*)\]$", contracts, re.M).group(1).split(", ")
        self.assertEqual(platforms, POLICY["contracts"]["platforms"])
        for line in ('CARGO_BUILD_JOBS: "1"', 'RUST_TEST_THREADS: "1"', "CI_OS: ${{ matrix.os }}",
                     "CI_PLAN: ${{ needs.plan.outputs.plan }}",
                     "run: rustup toolchain install 1.95.0 --profile minimal --component clippy --component rustfmt",
                     "run: python3 scripts/validate.py --output target/qualification/ci",
                     "name: dg0-${{ matrix.os }}-${{ github.run_attempt }}"):
            self.assertIn(line, contracts)
        order = [contracts.index(text) for text in (
            'run: python3 -B scripts/ci_run.py bind contracts --os "$CI_OS"', "rustup toolchain install",
            "scripts/validate.py --output", 'run: python3 -B scripts/ci_run.py contracts --os "$CI_OS"',
            "actions/upload-artifact@v6", 'run: python3 -B scripts/ci_run.py summary --os "$CI_OS"')]
        self.assertEqual(order, sorted(order))
        self.assertNotIn("\n    name:", contracts, "the check names stay contracts (<os>)")

    def test_evidence_is_always_preserved_and_required(self):
        for name in ("repository", "contracts"):
            upload = next(step for step in steps(self.jobs[name]) if "upload-artifact" in step)
            self.assertIn("if: always()", upload)
            self.assertIn("if-no-files-found: error", upload)
        summary = next(step for step in steps(self.jobs["contracts"]) if "ci_run.py summary" in step)
        self.assertIn("if: always()", summary)

    def test_no_policy_lives_in_the_workflow(self):
        self.assertNotIn("--allow-incomplete", WORKFLOW)
        self.assertIsNone(re.search(r"dg1-[a-z]", WORKFLOW))
        self.assertNotIn("runner.os", WORKFLOW)
        self.assertNotIn("paths:", WORKFLOW)
        self.assertNotIn("paths-ignore:", WORKFLOW)

    def test_no_expression_is_expanded_inside_a_script(self):
        self.assertNotRegex(WORKFLOW, r"run: *[|>]")
        for line in WORKFLOW.splitlines():
            if line.strip().startswith("run:"):
                self.assertNotIn("${{", line)

    def test_every_checkout_drops_its_credentials(self):
        checkouts = [step for job in self.jobs.values() for step in steps(job) if "actions/checkout@" in step]
        self.assertEqual(len(checkouts), len(self.jobs))
        for step in checkouts:
            self.assertIn("uses: actions/checkout@v5", step)
            self.assertIn("persist-credentials: false", step)

    def test_triggers_permissions_and_concurrency(self):
        header = WORKFLOW.split("\njobs:\n", 1)[0]
        self.assertIn("on:\n  pull_request:\n  push:\n    branches: [main]\n  schedule:\n    - cron: '17 18 * * *'\n"
                      "  workflow_dispatch:\n", header)
        self.assertNotIn("merge_group", header)
        self.assertIn("compensating control", header)
        self.assertIn("\npermissions:\n  contents: read\n", header)
        self.assertIn("format('pr-{0}', github.event.pull_request.number) || format('run-{0}', github.run_id)", header)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", header)
        self.assertEqual(set(ci_plan.EVENTS), {"pull_request", "push", "schedule", "workflow_dispatch"})


if __name__ == "__main__":
    unittest.main()
