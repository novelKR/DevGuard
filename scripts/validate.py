#!/usr/bin/env python3
"""DG-0 qualification only. Never infer runtime or OS qualification from unit tests."""
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


def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


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
    allowed = {
        "devguard-contract": set(),
        "devguard-core": {"devguard-contract"},
        "devguard-daemon": {"devguard-contract", "devguard-core"},
    }
    if roots != set(allowed):
        raise RuntimeError("unexpected workspace graph; update explicit boundaries with new crates")
    for package in packages:
        if package["name"] in allowed:
            edges = {d["name"] for d in package["dependencies"]} & roots
            if edges != allowed[package["name"]]:
                raise RuntimeError("workspace dependency boundary changed: " + package["name"])
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
    args = parser.parse_args()
    run_id = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:8]
    output = args.output or ROOT / "target/qualification" / run_id
    output = output.resolve()
    if subprocess.run(["git", "check-ignore", "-q", str(output / "report.json")], cwd=ROOT).returncode:
        parser.error("qualification output must be Git-ignored inside this checkout")
    output.mkdir(parents=True, exist_ok=False)
    report = {"schema": "devguard-qualification/v1", "milestone": "DG-0", "run_id": run_id,
              "platform": platform.platform(), "status": "failed", "stages": [],
              "execution_mode": "bootstrap-contract-validation", "self_governed": False,
              "runtime_qualification": {"macos_launch": "not_run", "linux_cgroups": "not_run",
                                        "browser_slo": "not_run", "candidate_self_use": "not_run",
                                        "codespace_integration": "not_run"}}
    environment = os.environ.copy()
    environment["CARGO_BUILD_JOBS"] = "1"
    environment["RUST_TEST_THREADS"] = "1"
    failed = False
    try:
        before = source_fingerprint()
        report["source"] = before
        report["rustc"] = capture("rustc", "-vV")
        report["cargo"] = capture("cargo", "--version")
        actual = report["rustc"].splitlines()[0].split()[1]
        report["pinned_toolchain"] = PINNED_RUST
        report["toolchain_matches"] = actual == PINNED_RUST
        if not report["toolchain_matches"] and not args.allow_toolchain_mismatch:
            raise RuntimeError(f"expected Rust {PINNED_RUST}, observed {actual}; put the pinned toolchain first in PATH")
        source_contract()
        report["stages"].append({"name": "source-contract", "status": "passed"})
        subprocess.run([sys.executable, "scripts/check_docs.py"], cwd=ROOT, check=True)
        subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", "scripts",
                        "-p", "test_check_docs.py"], cwd=ROOT, check=True)
        report["stages"].append({"name": "documentation", "status": "passed"})
        dependency_boundary(args.offline, environment, output)
        report["stages"].append({"name": "dependency-boundary", "status": "passed"})
        cargo_flags = ["--locked"] + (["--offline"] if args.offline else [])
        commands = [
            ("format", ["cargo", "fmt", "--all", "--", "--check"]),
            ("clippy", ["cargo", "clippy", *cargo_flags, "--workspace", "--all-targets", "--", "-D", "warnings"]),
            ("contracts", ["cargo", "test", *cargo_flags, "--workspace"]),
        ]
        for name, command in commands:
            started = time.monotonic()
            entry = {"name": name, "status": "failed", "command": command, "log": name + ".log"}
            report["stages"].append(entry)
            with (output / entry["log"]).open("w") as log:
                completed = subprocess.run(command, cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT)
            entry["seconds"] = round(time.monotonic() - started, 3)
            entry["status"] = "passed" if completed.returncode == 0 else "failed"
            if name == "contracts" and completed.returncode == 0:
                text = (output / entry["log"]).read_text(errors="replace")
                entry["tests_passed"] = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", text)))
            failed |= completed.returncode != 0
            print(name + ": " + entry["status"], flush=True)
        if source_fingerprint() != before:
            raise RuntimeError("source inputs changed during qualification")
        report["status"] = "failed" if failed else ("passed" if report["toolchain_matches"] else "incomplete")
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        failed = True
        report["error"] = str(error)
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps({"status": report["status"], "report": str(output / "report.json")}, ensure_ascii=False))
    return 1 if failed else (0 if report["status"] == "passed" else 2)


if __name__ == "__main__":
    raise SystemExit(main())
