#!/usr/bin/env python3
"""Build a DevGuard release package for installation as the current user's LaunchAgent.

The package holds `devguardd`, `devguard` and `devguard-launch` from one release build of a
clean tree, and a manifest of their hashes, the build's compiled compatibility and its source
provenance. Install it with `<package>/bin/devguardd install --package <package>`; the installer
verifies every hash and refuses to run from any other build. A package is a functional artifact:
it is not SLO-qualified until DG1-C12 measures it.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from validate import PINNED_RUST, source_fingerprint  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
BINARIES = ["devguardd", "devguard", "devguard-launch"]
PACKAGES = ["devguard-daemon", "devguard-cli", "devguard-launch"]


def capture(args, environment=None):
    return subprocess.check_output(args, cwd=ROOT, text=True, env=environment).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--output", help="the package directory (default target/package/<release-id>)")
    args = parser.parse_args()
    if capture(["git", "status", "--porcelain"]):
        raise SystemExit("package requires a clean tree")
    environment = dict(os.environ)
    environment.setdefault("CARGO_BUILD_JOBS", "1")
    rustc = capture(["rustc", "-vV"], environment)
    if f"release: {PINNED_RUST}" not in rustc:
        raise SystemExit(f"package requires Rust {PINNED_RUST}")
    source = source_fingerprint()
    command = ["cargo", "build", "--release", "--locked"]
    for package in PACKAGES:
        command += ["-p", package]
    if args.offline:
        command.append("--offline")
    subprocess.run(command, cwd=ROOT, env=environment, check=True)
    built = ROOT / "target" / "release"
    compatibility = json.loads(capture([str(built / "devguardd"), "version", "--json"]))
    artifacts = {}
    for name in BINARIES:
        data = (built / name).read_bytes()
        artifacts[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
    digest = hashlib.sha256(json.dumps(artifacts, sort_keys=True).encode()).hexdigest()
    commit = source["head"]
    release_id = f"{compatibility['package_version']}-{commit[:7]}-{digest[:8]}"
    manifest = {
        "schema": "devguard-release-manifest/v1",
        "release_id": release_id,
        "version": compatibility["package_version"],
        "source": {"commit": commit, "tree": capture(["git", "rev-parse", "HEAD^{tree}"]),
                   "tree_digest": source["tree_digest"], "clean": True},
        "build": {"profile": "release", "command": command,
                  "environment": {"CARGO_BUILD_JOBS": environment["CARGO_BUILD_JOBS"]},
                  "bootstrap": True, "rustc": rustc, "cargo": capture(["cargo", "-V"], environment)},
        "artifacts": artifacts,
        "compatibility": compatibility,
        "scope": "functional",
        "slo_qualified": False,
        "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    }
    output = Path(args.output) if args.output else ROOT / "target" / "package" / release_id
    if output.exists():
        raise SystemExit(f"refusing to overwrite {output}")
    (output / "bin").mkdir(parents=True)
    for name in BINARIES:
        shutil.copy2(built / name, output / "bin" / name)
        os.chmod(output / "bin" / name, 0o755)
        if hashlib.sha256((output / "bin" / name).read_bytes()).hexdigest() != artifacts[name]["sha256"]:
            raise SystemExit(f"the packaged {name} does not match its build")
    (output / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"package": str(output), "release_id": release_id}))


if __name__ == "__main__":
    main()
