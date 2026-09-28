#!/usr/bin/env python3
"""The required gate: pass only when this run and attempt did exactly what its plan requires.

The plan is derived again here from the same event and must equal the plan job's, so it is bound
to this commit, event, policy, run and attempt. Every job must have the planned result: the
repository job succeeds, and the contracts jobs succeed when the plan selects Rust and are skipped
otherwise. Only evidence uploaded by this run in this attempt counts. Each artifact name ends in
the current attempt and carries a binding to the run; an artifact of an earlier attempt is
ignored and never satisfies the current one. Re-running only the failed jobs therefore cannot
pass; re-run every job. A suite may be incomplete only for cases the policy allows on its platform.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess

import ci_plan
import ci_run
import validate

ROOT = ci_plan.ROOT
REQUIRED_JOBS = {"plan", "repository", "contracts"}


def read(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def artifact_names(artifacts):
    return sorted(path.name for path in artifacts.iterdir() if path.is_dir()) if artifacts.is_dir() else []


def check_repository(plan, directory):
    """Problems with the repository job's evidence, and the status found for each stage."""
    problems = []
    if read(directory / "binding.json") != ci_run.binding(plan, "repository"):
        problems.append("repository: its evidence is not bound to this run and attempt")
    report = read(directory / "validate" / "report.json")
    if not isinstance(report, dict):
        return problems + ["repository: no validator report"], {}
    if report.get("status") != "passed" or (report.get("source") or {}).get("head") != plan["source_sha"]:
        problems.append("repository: the validator report does not show a pass at the planned source")
    stages = {stage.get("name"): stage for stage in report.get("stages") or []}
    statuses = {name: stage.get("status") for name, stage in stages.items()}
    if list(stages) != list(validate.STAGES):
        return problems + ["repository: the validator report does not list every stage"], statuses
    for name, status in statuses.items():
        expected = "passed" if name in plan["repository_stages"] else "not_selected_by_plan"
        if status != expected:
            problems.append(f"repository: stage {name} is {status}, expected {expected}")
    whitespace = stages["whitespace"]
    if (whitespace.get("mode"), whitespace.get("base")) != (plan["whitespace"]["mode"], plan["whitespace"]["base"]):
        problems.append("repository: the whitespace check did not cover the planned range")
    return problems, statuses


def check_contracts(policy, plan, platform, directory):
    """Problems with one contracts job's evidence, the validator's status and each planned suite's."""
    label = "contracts " + platform
    problems = []
    if read(directory / "binding.json") != ci_run.binding(plan, "contracts", platform):
        problems.append(f"{label}: its evidence is not bound to this run and attempt")
    report = read(directory / "ci" / "report.json")
    report = report if isinstance(report, dict) else {}
    stages = {stage.get("name"): stage.get("status") for stage in report.get("stages") or []}
    expected = {"whitespace": "not_selected_by_plan", **{name: "passed" for name in validate.DEFAULT_STAGES}}
    if (report.get("status") != "passed" or report.get("milestone") != "DG-0"
            or report.get("toolchain_matches") is not True or stages != expected
            or (report.get("source") or {}).get("head") != plan["source_sha"]):
        problems.append(f"{label}: the complete validator did not pass at the planned source")
    planned = ci_run.planned_suites(policy, plan, platform)
    leg = read(directory / "leg.json")
    if not isinstance(leg, dict) or [entry.get("suite") for entry in leg.get("suites") or []] != planned:
        problems.append(f"{label}: the runner did not run exactly the planned suites")
    results = {}
    for suite in planned:
        results[suite], found = ci_run.evaluate_suite(policy, platform, suite, read(directory / suite / "report.json"),
                                                      plan["source_sha"])
        problems += [f"{label}: {suite}: {problem}" for problem in found]
    extra = sorted(path.parent.name for path in directory.glob("dg1-*/report.json") if path.parent.name not in planned)
    problems += [f"{label}: {suite} ran although the plan did not select it" for suite in extra]
    return problems, report.get("status") or "no report", results


def problems(results, plan_text, env, event, artifacts, root=ROOT):
    """Every way this attempt differs from its plan, and a table of what was found."""
    if not isinstance(results, dict) or set(results) != REQUIRED_JOBS:
        return ["the gate does not see exactly the required jobs"], []
    outcome = {name: (results[name] or {}).get("result") for name in REQUIRED_JOBS}
    if outcome["plan"] != "success":
        return [f"plan: {outcome['plan']}; without a plan no check is satisfied"], []
    try:
        plan = json.loads(plan_text or "")
    except ValueError:
        return ["the plan job's output is not a plan"], []
    attempt = env.get("GITHUB_RUN_ATTEMPT")
    if not isinstance(plan, dict) or plan.get("run_attempt") != attempt:
        return [f"the plan is not from this attempt {attempt}; re-run all jobs"], []
    try:
        derived, _ = ci_plan.prepare(root, env, event)
    except (ci_plan.PlanError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        return [f"the plan cannot be derived again: {error}"], []
    if plan != derived:
        return ["the plan job's plan differs from the plan derived for this run"], []
    if plan["event"] in ci_plan.FULL_EVENTS and plan["profile"] != "full":
        return ["a scheduled or manual run must be full"], []
    policy, _ = ci_plan.read_policy(root)
    found, table = [], [("plan", f"{plan['profile']}; job {outcome['plan']}")]
    for name, expected in (("repository", "success"), ("contracts", "success" if plan["rust"] else "skipped")):
        table.append((f"job {name}", outcome[name]))
        if outcome[name] != expected:
            found.append(f"{name}: {outcome[name]}, expected {expected}")
    names = artifact_names(artifacts)
    current = {name for name in names if re.fullmatch(r".+-[1-9][0-9]*", name) and name.rsplit("-", 1)[1] == attempt}
    wanted = {f"ci-plan-{attempt}", f"ci-repository-{attempt}"}
    if plan["rust"]:
        wanted |= {f"dg0-{platform}-{attempt}" for platform in policy["contracts"]["platforms"]}
    found += [f"{name}: no artifact from this attempt" for name in sorted(wanted - current)]
    found += [f"{name}: an artifact the plan does not produce" for name in sorted(current - wanted)]
    if f"ci-plan-{attempt}" in current and read(artifacts / f"ci-plan-{attempt}" / "plan.json") != plan:
        found.append("ci-plan: the uploaded plan is not the planned one")
    if f"ci-repository-{attempt}" in current:
        repository, statuses = check_repository(plan, artifacts / f"ci-repository-{attempt}")
        found += repository
        table += [(f"repository {stage}", statuses.get(stage, "no report")) for stage in validate.STAGES]
    for platform in policy["contracts"]["platforms"]:
        name = f"dg0-{platform}-{attempt}"
        if not plan["rust"]:
            table.append((f"contracts {platform}", "not_selected_by_plan"))
        elif name in current:
            contracts, validator, suites = check_contracts(policy, plan, platform, artifacts / name)
            found += contracts
            table.append((f"contracts {platform} validator", validator))
            table += [(f"contracts {platform} {suite}", suites.get(suite, "not_selected_by_plan"))
                      for suite, spec in policy["suites"].items() if platform in spec["platforms"]]
    ignored = sorted(set(names) - current)
    if ignored:
        table.append(("ignored: other attempts", ", ".join(ignored)))
    return found, table


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--artifacts", type=Path, required=True, help="the directory of this run's downloaded artifacts")
    args = parser.parse_args(argv)
    env = os.environ
    try:
        event = json.loads(Path(env["GITHUB_EVENT_PATH"]).read_text())
        found, table = problems(json.loads(env["CI_RESULTS"]), env.get("CI_PLAN"), env, event, args.artifacts)
    except (OSError, ValueError, KeyError, TypeError, AttributeError, ci_run.RunError) as error:
        found, table = [f"cannot evaluate the CI results: {error}"], []
    verdict = "Required checks: " + ("failed" if found else "passed")
    print("\n".join([verdict] + ["- " + line for line in found]))
    if env.get("GITHUB_STEP_SUMMARY"):
        rows = ["| Check | Result |", "| --- | --- |"] + [f"| {check} | {result} |" for check, result in table]
        with open(env["GITHUB_STEP_SUMMARY"], "a") as handle:
            handle.write("\n".join([f"### {verdict}", ""] + ["- " + line for line in found] + [""] + rows) + "\n")
    raise SystemExit(1 if found else 0)


if __name__ == "__main__":
    main()
