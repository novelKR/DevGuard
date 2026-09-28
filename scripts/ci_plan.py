#!/usr/bin/env python3
"""Plan the CI checks a change needs and bind the plan to the checked-out source and run.

Each changed path is classified by scripts/ci-policy.json, first match in this order:
- full: CI, planning, validation and qualification scripts, Cargo, toolchain and Git inputs;
- historical: dated records, which run the whitespace and documentation checks only;
- normative: policy, contract and design inputs, which run the non-Rust checks they feed;
- a component crate, which runs the complete validator on both platforms and the functional
  suites whose packages, or the binaries they start, depend on it.
A path in no class makes the plan full. So do a missing or untrusted base, an empty diff and any
change to a planning input, so a pull request cannot narrow its own checks. Scheduled and manual
runs are full. Any other event, merge_group included, has no plan: planning fails, and with it
the required gate.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
POLICY = "scripts/ci-policy.json"
SCHEMA = "devguard-ci-plan/v1"
# The inputs that decide what runs. A difference in any of them between base and head makes the
# plan full, whatever the path classes say.
PLANNING = (".github/workflows/ci.yml", POLICY, "scripts/ci_plan.py", "scripts/ci_run.py",
            "scripts/check_ci_results.py", "scripts/validate.py", "scripts/qualify.py")
FULL_EVENTS = ("schedule", "workflow_dispatch")
EVENTS = ("pull_request", "push") + FULL_EVENTS
SHA = re.compile("[0-9a-f]{40}")
RUN = re.compile("[1-9][0-9]*")
GLOB = {"**/": "(?:.*/)?", "**": ".*", "*": "[^/]*", "?": "[^/]"}
# Reasons name at most this many unclassified paths; the plan artifact lists every changed path.
SHOWN = 10


class PlanError(Exception):
    pass


def git(root, *args):
    return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True).stdout


def read_policy(root=ROOT):
    raw = (root / POLICY).read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def matches(pattern, path):
    """`*` and `?` stay inside one path segment; `**` spans segments."""
    parts = re.split(r"(\*\*/|\*\*|\*|\?)", pattern)
    regex = "".join(GLOB.get(part, re.escape(part)) for part in parts)
    return re.fullmatch(regex, path, flags=re.DOTALL) is not None


def classify(policy, path):
    """The class of one path and what matched it (a pattern or a component), or (None, None)."""
    for name in ("full", "historical", "normative"):
        for pattern in policy["classes"][name]:
            if matches(pattern, path):
                return name, pattern
    for component, patterns in policy["components"].items():
        if any(matches(pattern, path) for pattern in patterns):
            return "component", component
    return None, None


def paths_digest(paths):
    data = "\0".join(sorted(paths)).encode("utf-8", "surrogateescape")
    return hashlib.sha256(data).hexdigest()


def build_plan(policy, digest, event, source, base, paths, reasons, run=(None, None)):
    """The plan for PATHS changed from BASE to SOURCE. Any reason makes it full."""
    reasons = set(reasons)
    counts = {"full": 0, "historical": 0, "normative": 0, "component": 0, "unclassified": 0}
    components, unclassified, source_inputs = set(), [], False
    repository = policy["repository"]
    for path in paths:
        kind, detail = classify(policy, path)
        counts[kind or "unclassified"] += 1
        if kind is None:
            unclassified.append(path)
        elif kind == "full":
            reasons.add("full-path:" + detail)
        elif kind == "component":
            components.add(detail)
        source_inputs |= path in repository["source_contract_inputs"]
    reasons |= {"unclassified:" + path for path in unclassified[:SHOWN]}
    if len(unclassified) > SHOWN:
        reasons.add(f"unclassified:{len(unclassified) - SHOWN} more")
    if base is not None and not paths:
        reasons.add("empty-diff")
    full = bool(reasons)
    if full:
        chosen = set(repository["full"])
    else:
        chosen = set(repository["always"])
        if counts["historical"] or counts["normative"]:
            chosen |= set(repository["documents"])
        if source_inputs:
            chosen.add("source-contract")
    suites = policy["suites"]
    rust = full or bool(components)
    selected = [name for name, spec in suites.items() if full or components & set(spec["components"])]
    return {
        "schema": SCHEMA, "event": event, "source_sha": source, "base_sha": base, "policy_sha256": digest,
        "run_id": run[0], "run_attempt": run[1],
        "profile": "full" if full else "affected", "reasons": sorted(reasons),
        "paths": {"count": len(paths), "sha256": paths_digest(paths), **counts},
        "components": sorted(components),
        "repository_stages": [stage for stage in repository["full"] if stage in chosen],
        "whitespace": {"mode": "diff", "base": base} if base is not None else {"mode": "tree", "base": None},
        "rust": rust,
        "suites": {platform: [name for name in selected if rust and platform in suites[name]["platforms"]]
                   for platform in policy["contracts"]["platforms"]},
    }


def blob(root, commit, path):
    found = subprocess.run(["git", "rev-parse", "--verify", "-q", f"{commit}:{path}"], cwd=root,
                           capture_output=True, text=True)
    return found.stdout.strip() if found.returncode == 0 else None


def planning_changed(root, base, head):
    """Whether any planning input differs; one absent from both sides is unchanged."""
    return any(blob(root, base, path) != blob(root, head, path) for path in PLANNING)


def changed_paths(root, base, head):
    out = git(root, "diff", "--name-only", "--no-renames", "-z", base, head)
    return sorted(os.fsdecode(path) for path in out.split(b"\0") if path)


def plan_diff(root, policy, digest, event, base, source, run):
    paths = changed_paths(root, base, source)
    reasons = {"planning-changed"} if planning_changed(root, base, source) else set()
    return build_plan(policy, digest, event, source, base, paths, reasons, run), paths


def prepare(root, env, event):
    """The plan of this run and the paths it changed, from the GitHub environment and event payload."""
    name = env.get("GITHUB_EVENT_NAME") or ""
    if name not in EVENTS:
        raise PlanError(f"no plan is defined for the event {name or '(none)'}")
    source = git(root, "rev-parse", "HEAD").decode().strip()
    if not SHA.fullmatch(source) or source != env.get("GITHUB_SHA"):
        raise PlanError("the checkout is not GITHUB_SHA")
    run = (env.get("GITHUB_RUN_ID") or "", env.get("GITHUB_RUN_ATTEMPT") or "")
    if not all(RUN.fullmatch(value) for value in run):
        raise PlanError("the run id or attempt is missing")
    policy, digest = read_policy(root)
    if name in FULL_EVENTS:
        return build_plan(policy, digest, name, source, None, [], {"event:" + name}, run), []
    if name == "pull_request":
        parents = git(root, "rev-list", "--parents", "-n", "1", source).decode().split()
        head = ((event.get("pull_request") or {}).get("head") or {}).get("sha")
        if len(parents) != 3 or parents[2] != head:
            return build_plan(policy, digest, name, source, None, [], {"pull-request-base-untrusted"}, run), []
        return plan_diff(root, policy, digest, name, parents[1], source, run)
    if env.get("GITHUB_REF") != "refs/heads/main":
        raise PlanError("push runs are planned for refs/heads/main only")
    base = event.get("before") or ""
    trusted = (SHA.fullmatch(base) and base != "0" * 40 and not event.get("forced")
               and subprocess.run(["git", "merge-base", "--is-ancestor", base, source], cwd=root,
                                  capture_output=True).returncode == 0)
    if not trusted:
        return build_plan(policy, digest, name, source, None, [], {"push-base-untrusted"}, run), []
    return plan_diff(root, policy, digest, name, base, source, run)


def outputs(plan):
    """GITHUB_OUTPUT lines. The plan is one line of ASCII JSON."""
    return ["plan=" + json.dumps(plan, separators=(",", ":"), sort_keys=True),
            "rust=" + str(plan["rust"]).lower(), "profile=" + plan["profile"]]


def summary(plan):
    lines = ["### CI plan", "", f"Profile **{plan['profile']}** for `{plan['source_sha']}` ({plan['event']}"
             + (f", base `{plan['base_sha']}`" if plan["base_sha"] else ", no base") + ").", ""]
    if plan["reasons"]:
        lines += ["Full because: " + ", ".join(f"`{reason}`" for reason in plan["reasons"]), ""]
    lines += [f"- Changed paths: {plan['paths']['count']}",
              "- Repository checks: " + ", ".join(plan["repository_stages"]),
              "- Rust validation: " + ("complete validator on " + ", ".join(plan["suites"]) if plan["rust"]
                                       else "not selected by plan")]
    for platform, suites in plan["suites"].items():
        lines.append(f"- Functional suites on {platform}: " + (", ".join(suites) or "none"))
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", help="print the plan for the committed diff BASE..HEAD instead")
    parser.add_argument("--head", default="HEAD")
    args = parser.parse_args(argv)
    try:
        if args.base:
            policy, digest = read_policy()
            base, head = (git(ROOT, "rev-parse", "--verify", rev + "^{commit}").decode().strip()
                          for rev in (args.base, args.head))
            plan, _ = plan_diff(ROOT, policy, digest, "local", base, head, (None, None))
            print(json.dumps(plan, indent=2))
            return
        plan, paths = prepare(ROOT, os.environ, json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()))
        out = ROOT / "target" / "ci" / "plan"
        out.mkdir(parents=True, exist_ok=True)
        (out / "plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
        (out / "changed-paths.json").write_text(json.dumps(paths, indent=2) + "\n")
        with open(os.environ["GITHUB_OUTPUT"], "a") as handle:
            handle.write("\n".join(outputs(plan)) + "\n")
        if os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as handle:
                handle.write(summary(plan))
        print(f"CI plan: {plan['profile']}; repository checks {', '.join(plan['repository_stages'])}; "
              f"Rust {'selected' if plan['rust'] else 'not selected'}")
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError, PlanError) as error:
        raise SystemExit(f"CI planning failed ({error}); no reduced coverage is authorized")


if __name__ == "__main__":
    main()
