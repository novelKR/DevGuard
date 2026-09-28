#!/usr/bin/env python3
"""Run one CI job's share of the plan and bind its evidence to this commit, event, policy, run and attempt.

  ci_run.py repository              whitespace and the plan's other non-Rust validator stages
  ci_run.py bind contracts --os OS  bind a contracts job before its validator runs
  ci_run.py contracts --os OS       the functional suites the plan selects for OS, in policy order
  ci_run.py summary --os OS         the contracts job summary, written even after a failure

The plan comes from the CI_PLAN environment variable. A job refuses a plan made for another
commit, event, policy, run or attempt. Hosted-runner exceptions come only from the policy: a suite
the policy allows to be incomplete on OS runs with --allow-incomplete, and each case its report
records as not run must then match one of the suite's allowances exactly.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import ci_plan
import qualify
import validate

ROOT = ci_plan.ROOT
BINDING_SCHEMA = "devguard-ci-binding/v1"
LEG_SCHEMA = "devguard-ci-leg/v1"
REPOSITORY_OUT = Path("target/ci/repository")
CONTRACTS_OUT = Path("target/qualification")
# The summary keeps the old workflow's statement of what the functional checks do not cover.
SCOPE = ("Contract regression, service/UDS, native host evidence, native scope, launch helper, reconciliation, "
         "command-line owner, Cargo adapter, installation, parent lease, upgrade and repair, and SLO harness "
         "functional checks only, as planned; native suites run on macOS. Real self-use under an installed "
         "parent and the SLO protocol itself are not run.")


class RunError(Exception):
    pass


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def digest(value):
    return hashlib.sha256(canonical(value).encode()).hexdigest()


def head(root):
    return ci_plan.git(root, "rev-parse", "HEAD").decode().strip()


def load_plan(env, root=ROOT):
    """The plan handed to this job, refused unless it was made for this very run."""
    try:
        plan = json.loads(env.get("CI_PLAN") or "")
    except ValueError:
        raise RunError("CI_PLAN is not a plan") from None
    _, policy_digest = ci_plan.read_policy(root)
    expected = {"schema": ci_plan.SCHEMA, "source_sha": env.get("GITHUB_SHA"), "event": env.get("GITHUB_EVENT_NAME"),
                "policy_sha256": policy_digest, "run_id": env.get("GITHUB_RUN_ID"),
                "run_attempt": env.get("GITHUB_RUN_ATTEMPT")}
    if not isinstance(plan, dict):
        raise RunError("CI_PLAN is not a plan")
    wrong = sorted(key for key, value in expected.items() if not value or plan.get(key) != value)
    if wrong:
        raise RunError("the plan is not bound to this run: " + ", ".join(wrong))
    if head(root) != plan["source_sha"]:
        raise RunError("the checkout is not the planned source")
    return plan


def binding(plan, job, platform=None):
    return {"schema": BINDING_SCHEMA, "job": job, "platform": platform,
            **{key: plan[key] for key in ("source_sha", "event", "policy_sha256", "run_id", "run_attempt")},
            "plan_sha256": digest(plan)}


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def planned_suites(policy, plan, platform):
    if platform not in policy["contracts"]["platforms"]:
        raise RunError("no contracts job is defined for " + str(platform))
    suites = plan["suites"][platform]
    if (not isinstance(suites, list) or len(set(suites)) != len(suites)
            or any(suite not in policy["suites"] or platform not in policy["suites"][suite]["platforms"]
                   for suite in suites)):
        raise RunError("the plan names a suite the policy does not run on " + platform)
    return [suite for suite in policy["suites"] if suite in suites]


def allowed(policy, platform, suite, case):
    """The allowance that covers one not-run case exactly, or None."""
    for entry in policy["allowances"].get(platform, {}).get(suite, []):
        for name in entry["cases"]:
            if case == {"file": f"raw/{entry['stage']}/{name}.json", "path": entry["path"], "reason": entry["reason"]}:
                return entry
    return None


def evaluate_suite(policy, platform, suite, report, source):
    """passed, incomplete-allowed or failed, and why a report is not accepted."""
    if not isinstance(report, dict):
        return "failed", ["no report"]
    problems = []
    if report.get("suite") != suite or report.get("schema") != "devguard-functional-qualification/v1":
        problems.append("the report is not for this suite")
    if (report.get("source") or {}).get("head") != source:
        problems.append("the report is not for the planned source")
    stages = report.get("stages") or []
    if [stage.get("name") for stage in stages] != [name for name, *_ in qualify.SUITES[suite]]:
        problems.append("the report does not show every stage of the suite")
    cases = [case for stage in stages for case in stage.get("not_run_cases") or []]
    status = report.get("status")
    if status == "passed":
        if cases or any(stage.get("status") != "passed" for stage in stages):
            problems.append("a passed report records a case as not run")
    elif status == "incomplete":
        if not cases:
            problems.append("an incomplete report names no case")
        for case in cases:
            if allowed(policy, platform, suite, case) is None:
                problems.append(f"not allowed to be not run on {platform}: {canonical(case)}")
        if any(stage.get("status") not in ("passed", "incomplete") for stage in stages):
            problems.append("a stage did not pass")
    else:
        problems.append(f"status {status}")
    if problems:
        return "failed", problems
    return ("passed" if status == "passed" else "incomplete-allowed"), []


def run_repository(env, root=ROOT, runner=subprocess.run):
    plan = load_plan(env, root)
    stages = plan["repository_stages"]
    if (not isinstance(stages, list) or "whitespace" not in stages
            or not set(stages) <= set(validate.STAGES) - validate.RUST_STAGES):
        raise RunError("the plan's repository checks are not non-Rust validator stages")
    out = root / REPOSITORY_OUT
    out.mkdir(parents=True, exist_ok=False)
    write_json(out / "binding.json", binding(plan, "repository"))
    command = [sys.executable, "-B", "scripts/validate.py", "--stages", ",".join(stages),
               "--output", str(REPOSITORY_OUT / "validate")]
    if plan["whitespace"]["mode"] == "diff":
        command += ["--diff-base", plan["whitespace"]["base"]]
    completed = runner(command, cwd=root)
    write_json(out / "leg.json", {"schema": LEG_SCHEMA, "job": "repository", "command": command,
                                  "exit_code": completed.returncode,
                                  "status": "passed" if completed.returncode == 0 else "failed"})
    return 0 if completed.returncode == 0 else 1


def bind_contracts(env, platform, root=ROOT):
    plan = load_plan(env, root)
    policy, _ = ci_plan.read_policy(root)
    if plan.get("rust") is not True:
        raise RunError("the plan does not select the contracts jobs")
    planned_suites(policy, plan, platform)
    out = root / CONTRACTS_OUT
    out.mkdir(parents=True, exist_ok=True)
    if (out / "binding.json").exists():
        raise RunError("this job is already bound")
    write_json(out / "binding.json", binding(plan, "contracts", platform))
    return 0


def run_contracts(env, platform, root=ROOT, runner=subprocess.run):
    plan = load_plan(env, root)
    policy, _ = ci_plan.read_policy(root)
    out = root / CONTRACTS_OUT
    try:
        bound = json.loads((out / "binding.json").read_text())
    except (OSError, ValueError):
        bound = None
    if bound != binding(plan, "contracts", platform):
        raise RunError("this job was not bound to the plan before its validator ran")
    results = []
    for suite in planned_suites(policy, plan, platform):
        command = [sys.executable, "scripts/qualify.py", suite]
        if policy["allowances"].get(platform, {}).get(suite):
            command.append("--allow-incomplete")
        command += ["--output", str(CONTRACTS_OUT / suite)]
        print(f"::group::{suite}", flush=True)
        completed = runner(command, cwd=root)
        print("::endgroup::", flush=True)
        try:
            report = json.loads((out / suite / "report.json").read_text())
        except (OSError, ValueError):
            report = None
        status, problems = evaluate_suite(policy, platform, suite, report, plan["source_sha"])
        if completed.returncode != 0:
            status, problems = "failed", problems + [f"exit code {completed.returncode}"]
        results.append({"suite": suite, "command": command, "exit_code": completed.returncode,
                        "status": status, "problems": problems})
        print(f"{suite}: {status}" + "".join("\n  - " + problem for problem in problems), flush=True)
    failed = any(result["status"] == "failed" for result in results)
    write_json(out / "leg.json", {"schema": LEG_SCHEMA, "job": "contracts", "platform": platform,
                                  "suites": results, "status": "failed" if failed else "passed"})
    return 1 if failed else 0


def status_of(path):
    try:
        return json.loads(path.read_text()).get("status") or "unknown"
    except (OSError, ValueError, AttributeError):
        return None


def summarize(env, platform, root=ROOT):
    """The contracts job summary; it describes what exists and never fails the job."""
    policy, _ = ci_plan.read_policy(root)
    try:
        planned = json.loads(env.get("CI_PLAN") or "{}").get("suites", {}).get(platform, [])
    except (ValueError, AttributeError):
        planned = []
    out = root / CONTRACTS_OUT
    lines = [f"### Contracts on {platform}", "", "| Check | Result |", "| --- | --- |",
             f"| validator (ci) | {status_of(out / 'ci' / 'report.json') or 'not_run'} |"]
    for suite, spec in policy["suites"].items():
        found = status_of(out / suite / "report.json")
        if found:
            result = found
        elif suite in planned:
            result = "not_run (not reached)"
        elif platform in spec["platforms"]:
            result = "not_selected_by_plan"
        else:
            result = "not_run (native macOS suite)"
        lines.append(f"| {suite} | {result} |")
    text = "\n".join(lines + ["", SCOPE, ""])
    if env.get("GITHUB_STEP_SUMMARY"):
        with open(env["GITHUB_STEP_SUMMARY"], "a") as handle:
            handle.write(text)
    else:
        print(text)
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=["repository", "bind", "contracts", "summary"])
    parser.add_argument("job", nargs="?", choices=["contracts"])
    parser.add_argument("--os", dest="platform")
    args = parser.parse_args(argv)
    if args.command in ("bind", "contracts", "summary") and not args.platform:
        parser.error(args.command + " needs --os")
    if args.command == "bind" and args.job != "contracts":
        parser.error("bind needs the job: contracts")
    try:
        if args.command == "repository":
            return run_repository(os.environ)
        if args.command == "bind":
            return bind_contracts(os.environ, args.platform)
        if args.command == "contracts":
            return run_contracts(os.environ, args.platform)
        return summarize(os.environ, args.platform)
    except (RunError, OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        if args.command == "summary":
            print(f"no summary: {error}")
            return 0
        print(f"CI job refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
