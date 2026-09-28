#!/usr/bin/env python3
"""DG-0 qualification only. Never infer runtime or OS qualification from unit tests.

Without --stages every default stage runs, as before. --stages selects a subset, run in the
fixed stage order, so a CI plan can run its non-Rust checks without the Rust toolchain. A stage
left out is recorded as not_selected_by_plan, never as passed, and a run of only some stages
does not claim the DG-0 milestone.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
PINNED_RUST = "1.95.0"
POLICY = "scripts/ci-policy.json"
# Every stage, in the order it runs. The whitespace check runs only when it is selected: a CI
# plan checks its diff, or the whole tree when it has no trusted base.
STAGES = ("whitespace", "source-contract", "documentation", "protocol-analysis",
          "dependency-boundary", "format", "clippy", "contracts")
DEFAULT_STAGES = STAGES[1:]
# Only these stages need the pinned Rust toolchain, so only they probe it. The whitespace check and
# the three Cargo commands record a failure and let the next stage run; any other failed stage ends
# the run, and the selected stages after it stay not_run.
RUST_STAGES = frozenset({"dependency-boundary", "format", "clippy", "contracts"})
# The explicit workspace graph: the workspace packages each package may depend on, in any
# dependency kind. The dependency-boundary stage enforces it; the CI planner derives which
# functional suites a changed crate can affect from it.
WORKSPACE_GRAPH = {
    "devguard-contract": set(),
    "devguard-core": {"devguard-contract"},
    "devguard-macos": {"devguard-contract", "devguard-core"},
    # The self edge only enables the daemon's fixtures in its own tests.
    "devguard-daemon": {"devguard-contract", "devguard-core", "devguard-client", "devguard-macos",
                        "devguard-daemon"},
    "devguard-client": {"devguard-contract"},
    # The daemon edge is a test-only dependency for isolated fixture authorities.
    "devguard-launch": {"devguard-contract", "devguard-client", "devguard-macos", "devguard-daemon"},
    # The CLI shares the daemon's canonical paths and configuration; it never
    # depends on the native backend or core directly.
    "devguard-cli": {"devguard-contract", "devguard-client", "devguard-daemon", "devguard-cargo"},
    # The Cargo adapter is a pure transformation over the contract types.
    "devguard-cargo": {"devguard-contract"},
    # The qualification harness measures a service as an ordinary CLI owner;
    # it is never packaged in a release.
    "devguard-qualify": {"devguard-contract", "devguard-client", "devguard-daemon", "devguard-cli"},
}


def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def git(root, *args, stdin=None):
    return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True,
                          input=stdin).stdout.strip()


def parse_stages(text):
    """The named stages in the fixed stage order; unknown, repeated or no names are refused."""
    names = [name.strip() for name in text.split(",") if name.strip()]
    if not names:
        raise ValueError("no stage selected")
    unknown = sorted(set(names) - set(STAGES))
    if unknown:
        raise ValueError("unknown stage: " + ", ".join(unknown))
    if len(set(names)) != len(names):
        raise ValueError("a stage is selected twice")
    return [stage for stage in STAGES if stage in names]


def source_fingerprint():
    names = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT
    ).split(b"\0")
    files = {}
    for raw in sorted(set(names)):
        if not raw:
            continue
        path = ROOT / os.fsdecode(raw)
        if path.is_file():
            files[os.fsdecode(raw)] = hashlib.sha256(path.read_bytes()).hexdigest()
    head = subprocess.run(["git", "rev-parse", "--verify", "HEAD"], cwd=ROOT, text=True, capture_output=True)
    return {"head": head.stdout.strip() if head.returncode == 0 else None,
            "files": files,
            "tree_digest": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()}


def source_contract():
    reference = json.loads((ROOT / "docs/design-source.json").read_text())
    actual = hashlib.sha256((ROOT / reference["document"]).read_bytes()).hexdigest()
    if actual != reference["sha256"]:
        raise RuntimeError("approved design changed; keep implementation clarifications separate")
    milestones = json.loads((ROOT / "milestones.json").read_text())
    if milestones["critical_path"] != ["DG-0", "DG-1", "CS-RG", "P1-RECOVERY"]:
        raise RuntimeError("approved critical path changed")
    items = {item["id"]: item for item in milestones["milestones"]}
    for current, previous in [("DG-1", "DG-0"), ("CS-RG", "DG-1"), ("P1-RECOVERY", "CS-RG")]:
        if items[current]["requires"] != [previous]:
            raise RuntimeError("unexpected P1 prerequisite")
    if not items["DG-LINUX"]["required_for_product_completion"]:
        raise RuntimeError("Linux qualification must remain required")


def tree_exemptions(root, head):
    """Files whose whitespace findings the policy accepts, only while their bytes are the pinned ones."""
    pinned = json.loads((root / POLICY).read_text())["whitespace"]["tree_exemptions"]
    exempt, lost = {}, []
    for path, digest in sorted(pinned.items()):
        blob = subprocess.run(["git", "cat-file", "blob", f"{head}:{path}"], cwd=root, capture_output=True)
        if blob.returncode == 0 and hashlib.sha256(blob.stdout).hexdigest() == digest:
            exempt[path] = digest
        else:
            lost.append(path)
    return exempt, lost


def whitespace(output, base, root=ROOT):
    """`git diff --check` of BASE..HEAD or, without a base, of the whole tree at HEAD."""
    head = git(root, "rev-parse", "--verify", "HEAD^{commit}")
    entry = {"name": "whitespace", "status": "failed", "head": head, "log": "whitespace.log"}
    if base is not None:
        entry.update(mode="diff", base=git(root, "rev-parse", "--verify", base + "^{commit}"))
        command = ["git", "diff", "--check", "--no-color", entry["base"], head]
    else:
        exempt, lost = tree_exemptions(root, head)
        entry.update(mode="tree", base=None, exempt=exempt, exemption_lost=lost)
        empty = git(root, "hash-object", "-t", "tree", "--stdin", stdin="")
        command = ["git", "diff", "--check", "--no-color", empty, head, "--", "."]
        command += [":(exclude,literal)" + path for path in exempt]
    entry["command"] = command
    with (output / entry["log"]).open("w") as log:
        completed = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    entry["status"] = "passed" if completed.returncode == 0 else "failed"
    return entry


def unittest(pattern):
    subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", "scripts", "-p", pattern],
                   cwd=ROOT, check=True)


def dependency_boundary(offline, environment, output):
    command = ["cargo", "metadata", "--locked", "--format-version", "1"]
    if offline:
        command.append("--offline")
    metadata = json.loads(subprocess.check_output(command, cwd=ROOT, env=environment, text=True))
    packages = metadata["packages"]
    forbidden = [p["name"] for p in packages if p["name"].startswith(("codespace-", "codex-"))]
    if forbidden:
        raise RuntimeError("independent graph includes product dependencies: " + ", ".join(forbidden))
    roots = {p["name"] for p in packages if p["id"] in metadata["workspace_members"]}
    allowed = WORKSPACE_GRAPH
    if roots != set(allowed):
        raise RuntimeError("unexpected workspace graph; update explicit boundaries with new crates")
    for package in packages:
        if package["name"] in allowed:
            edges = {d["name"] for d in package["dependencies"]} & roots
            if edges != allowed[package["name"]]:
                raise RuntimeError("workspace dependency boundary changed: " + package["name"])
    daemon = next(p for p in packages if p["name"] == "devguard-daemon")
    if any(d["name"] == "devguard-daemon" and (d.get("kind") != "dev" or d.get("features") != ["test-fixtures"])
           for d in daemon["dependencies"]):
        raise RuntimeError("devguard-daemon may depend on itself only to enable its fixtures in tests")
    # The launch crate uses the daemon only for isolated test authorities.
    launch = next(p for p in packages if p["name"] == "devguard-launch")
    if any(d["name"] == "devguard-daemon" and d.get("kind") != "dev" for d in launch["dependencies"]):
        raise RuntimeError("devguard-launch may depend on devguard-daemon only for tests")
    # The CLI's fixture authorities are compiled for its tests only.
    cli = next(p for p in packages if p["name"] == "devguard-cli")
    if any(d["name"] == "devguard-daemon" and "test-fixtures" in d.get("features", []) and d.get("kind") != "dev"
           for d in cli["dependencies"]):
        raise RuntimeError("devguard-cli may enable daemon test fixtures only for tests")
    # Like the CLI, the harness compiles fixture authorities for its tests only.
    qualify = next(p for p in packages if p["name"] == "devguard-qualify")
    if any(d["name"] == "devguard-daemon" and "test-fixtures" in d.get("features", []) and d.get("kind") != "dev"
           for d in qualify["dependencies"]):
        raise RuntimeError("devguard-qualify may enable daemon test fixtures only for tests")
    contract = next(p for p in packages if p["name"] == "devguard-contract")
    if {d["name"] for d in contract["dependencies"]} != {"serde", "serde_json", "sha2"}:
        raise RuntimeError("contract must remain independent of persistence and host adapters")
    (output / "dependencies.json").write_text(json.dumps(
        sorted([[p["name"], p["version"], p["source"]] for p in packages]), indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--allow-toolchain-mismatch", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--stages", metavar="LIST",
                        help="comma-separated stages to run, always in the order " + ", ".join(STAGES)
                        + "; default: every stage except whitespace")
    parser.add_argument("--diff-base", metavar="COMMIT",
                        help="check whitespace in COMMIT..HEAD; without it the whole tree at HEAD is checked")
    args = parser.parse_args()
    try:
        selected = parse_stages(args.stages) if args.stages is not None else list(DEFAULT_STAGES)
    except ValueError as error:
        parser.error(str(error))
    if args.diff_base is not None and "whitespace" not in selected:
        parser.error("--diff-base applies only to the whitespace stage")
    run_id = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:8]
    output = args.output or ROOT / "target/qualification" / run_id
    output = output.resolve()
    if subprocess.run(["git", "check-ignore", "-q", str(output / "report.json")], cwd=ROOT).returncode:
        parser.error("qualification output must be Git-ignored inside this checkout")
    output.mkdir(parents=True, exist_ok=False)
    # Only a run of every default stage is DG-0 qualification.
    complete = set(DEFAULT_STAGES) <= set(selected)
    omitted = "not selected by --stages" if args.stages is not None else "not a default stage"
    stages = [{"name": name, "status": "not_run", "reason": "not reached"} if name in selected
              else {"name": name, "status": "not_selected_by_plan", "reason": omitted} for name in STAGES]
    report = {"schema": "devguard-qualification/v1", "milestone": "DG-0" if complete else None, "run_id": run_id,
              "platform": platform.platform(), "status": "failed", "stages": stages, "selected_stages": selected,
              "execution_mode": "bootstrap-contract-validation" if complete else "selected-stages",
              "self_governed": False,
              "runtime_qualification": {"macos_launch": "not_run", "linux_cgroups": "not_run",
                                        "browser_slo": "not_run", "candidate_self_use": "not_run",
                                        "codespace_integration": "not_run"}}
    by_name = {entry["name"]: entry for entry in stages}
    environment = os.environ.copy()
    environment["CARGO_BUILD_JOBS"] = "1"
    environment["RUST_TEST_THREADS"] = "1"
    failed = False
    current = None
    try:
        before = source_fingerprint()
        report["source"] = before
        report["pinned_toolchain"] = PINNED_RUST
        if RUST_STAGES & set(selected):
            report["rustc"] = capture("rustc", "-vV")
            report["cargo"] = capture("cargo", "--version")
            actual = report["rustc"].splitlines()[0].split()[1]
            report["toolchain_matches"] = actual == PINNED_RUST
            if not report["toolchain_matches"] and not args.allow_toolchain_mismatch:
                raise RuntimeError(f"expected Rust {PINNED_RUST}, observed {actual}; put the pinned toolchain first in PATH")
        else:
            report["toolchain_matches"] = None
            report["toolchain"] = "not probed: no selected stage needs Rust"
        cargo_flags = ["--locked"] + (["--offline"] if args.offline else [])
        commands = {
            "format": ["cargo", "fmt", "--all", "--", "--check"],
            "clippy": ["cargo", "clippy", *cargo_flags, "--workspace", "--all-targets", "--", "-D", "warnings"],
            "contracts": ["cargo", "test", *cargo_flags, "--workspace"],
        }
        for name in selected:
            current = by_name[name]
            current.pop("reason")
            current["status"] = "failed"
            if name in commands:
                started = time.monotonic()
                current.update(command=commands[name], log=name + ".log")
                with (output / current["log"]).open("w") as log:
                    completed = subprocess.run(commands[name], cwd=ROOT, env=environment, stdout=log,
                                               stderr=subprocess.STDOUT)
                current["seconds"] = round(time.monotonic() - started, 3)
                current["status"] = "passed" if completed.returncode == 0 else "failed"
                if name == "contracts" and completed.returncode == 0:
                    text = (output / current["log"]).read_text(errors="replace")
                    current["tests_passed"] = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", text)))
            elif name == "whitespace":
                current.update(whitespace(output, args.diff_base))
            elif name == "source-contract":
                source_contract()
                current["status"] = "passed"
            elif name == "documentation":
                subprocess.run([sys.executable, "scripts/check_docs.py"], cwd=ROOT, check=True)
                unittest("test_check_docs.py")
                current["status"] = "passed"
            elif name == "protocol-analysis":
                unittest("test_measure.py")
                current["status"] = "passed"
            elif name == "dependency-boundary":
                dependency_boundary(args.offline, environment, output)
                current["status"] = "passed"
            failed |= current["status"] != "passed"
            print(name + ": " + current["status"], flush=True)
        current = None
        if source_fingerprint() != before:
            raise RuntimeError("source inputs changed during qualification")
        matches = report["toolchain_matches"] is not False
        report["status"] = "failed" if failed else ("passed" if matches else "incomplete")
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.CalledProcessError) as error:
        failed = True
        report["error"] = str(error)
        if current is not None:
            current["error"] = str(error)
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps({"status": report["status"], "report": str(output / "report.json")}, ensure_ascii=False))
    return 1 if failed else (0 if report["status"] == "passed" else 2)


if __name__ == "__main__":
    raise SystemExit(main())
