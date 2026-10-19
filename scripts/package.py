#!/usr/bin/env python3
"""Build a DevGuard release package for installation as the current user's LaunchAgent.

The package holds `devguardd`, `devguard` and `devguard-launch` from one release build of a
clean tree, and a manifest of their hashes, the build's compiled compatibility and its source
provenance. Install it with `<package>/bin/devguardd install --package <package>`; the installer
verifies every hash and refuses to run from any other build. A package is a functional artifact:
it is not SLO-qualified until DG1-C12 measures it.

The binaries embed no build path. The checkout, the Cargo home and the toolchain are remapped,
a macOS link leaves out the linker's debug map, the macOS deployment target is pinned, and these
flags replace any inherited Rust flags. Two checkouts of one commit built with the same toolchain
and SDK therefore make the same binaries and the same release id. A build that still contains a
build path or the builder's home is refused before anything is written.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import devguard_color as color  # noqa: E402
from validate import PINNED_RUST, source_fingerprint  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
BINARIES = ["devguardd", "devguard", "devguard-launch"]
PACKAGES = ["devguard-daemon", "devguard-cli", "devguard-launch"]
# The oldest macOS the binaries run on: the oldest version whose CI runner (macos-14) runs the
# native suites, so a release names no version that nothing checks. Pinned, it no longer follows
# the default of whichever toolchain or SDK builds it.
MACOS_DEPLOYMENT_TARGET = "14.0"
# What each build path becomes inside the binaries. Unmapped, they embed absolute paths under the
# builder's home, the user name included, and each checkout path builds different binaries.
REMAPPED = {"checkout": "/devguard", "cargo_home": "/cargo", "toolchain": "/toolchain"}


def capture(args, environment=None):
    return subprocess.check_output(args, cwd=ROOT, text=True, env=environment).strip()


def tool(*args, lines=1):
    """The first LINES lines a host tool reports, or None when it is missing or fails."""
    try:
        completed = subprocess.run(args, cwd=ROOT, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return None
    text = (completed.stdout.strip() or completed.stderr.strip()) if completed.returncode == 0 else ""
    return "; ".join(text.splitlines()[:lines]) or None


def host_tools():
    """The C compiler, which builds the bundled SQLite, and on macOS the Xcode, SDK and linker."""
    tools = {"cc": tool("cc", "--version")}
    if platform.system() == "Darwin":
        tools.update(xcode=tool("xcodebuild", "-version", lines=2),
                     sdk=tool("xcrun", "--sdk", "macosx", "--show-sdk-version"),
                     sdk_build=tool("xcrun", "--sdk", "macosx", "--show-sdk-build-version"),
                     ld=tool("ld", "-v"))
    return tools


def build_paths(environment, sysroot):
    """The paths a build can embed, as Cargo and rustc see them, keyed as in REMAPPED."""
    home = environment.get("HOME") or str(Path.home())
    cargo_home = environment.get("CARGO_HOME") or os.path.join(home, ".cargo")
    # Cargo resolves a relative CARGO_HOME against its working directory, the checkout.
    return {"checkout": str(ROOT), "cargo_home": os.path.join(str(ROOT), cargo_home), "toolchain": sysroot}


def forms(path):
    """A path as given and resolved, without a trailing separator; never the root itself."""
    found = {path.rstrip(os.sep), os.path.realpath(path).rstrip(os.sep)} if path else set()
    return found - {""}


def remap_flags(paths):
    """A --remap-path-prefix flag for each form of each build path, the longest last.

    rustc applies the last matching flag, so a path inside another build path keeps its own name.
    """
    pairs = {(form, REMAPPED[key]) for key, path in paths.items() for form in forms(path)}
    return ["--remap-path-prefix=" + source + "=" + target
            for source, target in sorted(pairs, key=lambda pair: (len(pair[0]), pair))]


def link_flags():
    """Linker flags of a release build on this platform.

    macOS's linker writes a debug map that names, by path, each object with debug information,
    the toolchain's standard library among them. The release build strips the map afterwards;
    `-S` keeps it out of the link, so that no such path reaches the linked output at all.
    """
    return ["-Clink-arg=-Wl,-S"] if platform.system() == "Darwin" else []


def leaks(data, paths, environment):
    """Which build paths, or the builder's home, DATA still contains at the start of a path."""
    candidates = dict(paths, home=environment.get("HOME") or "")
    return sorted(name for name, path in candidates.items()
                  if any((form + os.sep).encode() in data for form in forms(path)))


def release_identity(version, commit, artifacts):
    """`<version>-<commit7>-<digest8>`, the digest over the binaries' hashes and sizes."""
    digest = hashlib.sha256(json.dumps(artifacts, sort_keys=True).encode()).hexdigest()
    return f"{version}-{commit[:7]}-{digest[:8]}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--output", help="the package directory (default target/package/<release-id>)")
    color.add_option(parser)
    args = parser.parse_args()
    color.choose(args.color)
    if capture(["git", "status", "--porcelain"]):
        color.refuse("package requires a clean tree")
    environment = dict(os.environ)
    environment.setdefault("CARGO_BUILD_JOBS", "1")
    rustc = capture(["rustc", "-vV"], environment)
    if f"release: {PINNED_RUST}" not in rustc:
        color.refuse(f"package requires Rust {PINNED_RUST}")
    paths = build_paths(environment, capture(["rustc", "--print", "sysroot"], environment))
    flags = remap_flags(paths) + link_flags()
    # CARGO_ENCODED_RUSTFLAGS takes precedence over RUSTFLAGS and configured flags, so rustc gets
    # exactly these flags. Cargo hashes no flags that remap paths into its file names.
    environment.update(CARGO_ENCODED_RUSTFLAGS="\x1f".join(flags),
                       CARGO_TARGET_DIR=str(ROOT / "target"),
                       MACOSX_DEPLOYMENT_TARGET=MACOS_DEPLOYMENT_TARGET)
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
        found = leaks(data, paths, environment)
        if found:
            color.refuse(f"the built {name} contains a build path ({', '.join(found)}); nothing was packaged")
        artifacts[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
    commit = source["head"]
    release_id = release_identity(compatibility["package_version"], commit, artifacts)
    manifest = {
        "schema": "devguard-release-manifest/v1",
        "release_id": release_id,
        "version": compatibility["package_version"],
        "source": {"commit": commit, "tree": capture(["git", "rev-parse", "HEAD^{tree}"]),
                   "tree_digest": source["tree_digest"], "clean": True},
        "build": {"profile": "release", "command": command,
                  "environment": {"CARGO_BUILD_JOBS": environment["CARGO_BUILD_JOBS"],
                                  "MACOSX_DEPLOYMENT_TARGET": MACOS_DEPLOYMENT_TARGET},
                  "remap_path_prefix": dict(REMAPPED), "link_flags": link_flags(),
                  "bootstrap": True, "rustc": rustc, "cargo": capture(["cargo", "-V"], environment),
                  "host_tools": host_tools()},
        "artifacts": artifacts,
        "compatibility": compatibility,
        "scope": "functional",
        "slo_qualified": False,
        "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    }
    text = json.dumps(manifest, indent=2) + "\n"
    found = leaks(text.encode(), paths, environment)
    if found:
        color.refuse(f"the manifest would name a build path ({', '.join(found)}); nothing was packaged")
    output = Path(args.output) if args.output else ROOT / "target" / "package" / release_id
    if output.exists():
        color.refuse(f"refusing to overwrite {output}")
    (output / "bin").mkdir(parents=True)
    for name in BINARIES:
        shutil.copy2(built / name, output / "bin" / name)
        os.chmod(output / "bin" / name, 0o755)
        if hashlib.sha256((output / "bin" / name).read_bytes()).hexdigest() != artifacts[name]["sha256"]:
            color.refuse(f"the packaged {name} does not match its build")
    (output / "MANIFEST.json").write_text(text)
    print(json.dumps({"package": str(output), "release_id": release_id}))


if __name__ == "__main__":
    main()
