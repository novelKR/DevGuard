"""The release path: only a manual run on main publishes, after "Required checks" passed on its
commit; a pull request proves that two builds make one release; a package embeds no build path;
the archive depends only on the package; and an existing release or tag is never replaced."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

import ci_plan
import devguard_color as color
import package
import release
import validate
from test_ci_workflow import jobs, steps

ROOT = ci_plan.ROOT
WORKFLOW = (ROOT / ".github/workflows/release.yml").read_text()
COMMIT = "c0ffee" + "0" * 34
REPOSITORY = "novelKR/DevGuard"
RUSTC = ("rustc 1.95.0 (0000000 2026-01-01)\nbinary: rustc\ncommit-hash: 0000000\ncommit-date: 2026-01-01\n"
         "host: aarch64-apple-darwin\nrelease: 1.95.0\nLLVM version: 20.1.8")
DISPATCH = {"GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_REF": "refs/heads/main",
            "GITHUB_REPOSITORY": REPOSITORY, "GITHUB_SHA": COMMIT}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def binaries(tag=b""):
    return {name: b"\x7fELF " + name.encode() + tag for name in package.BINARIES}


def make_package(directory, contents=None, commit=COMMIT, created="2026-10-19T00:00:00+00:00"):
    """A package as package.py writes one, with stand-in binaries."""
    directory = Path(directory)
    (directory / "bin").mkdir(parents=True)
    artifacts = {}
    for name, data in (contents or binaries()).items():
        (directory / "bin" / name).write_bytes(data)
        os.chmod(directory / "bin" / name, 0o755)
        artifacts[name] = {"sha256": sha256(data), "bytes": len(data)}
    manifest = {"schema": release.MANIFEST_SCHEMA,
                "release_id": package.release_identity("0.1.0", commit, artifacts), "version": "0.1.0",
                "source": {"commit": commit, "tree": "e" * 40, "tree_digest": "d" * 64, "clean": True},
                "build": {"profile": "release", "rustc": RUSTC}, "artifacts": artifacts,
                "scope": "functional", "slo_qualified": False, "created_at": created}
    (directory / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def rewrite_sums(dist):
    files = {path.name: path for path in Path(dist).iterdir() if path.name != release.SUMS}
    (Path(dist) / release.SUMS).write_text(release.sums(files))


@contextlib.contextmanager
def color_choice():
    """Restore the process's color choice after a test that runs a script's main."""
    chosen = color._choice
    try:
        yield
    finally:
        color.choose(chosen)


def permissions(job):
    block = re.search(r"^    permissions:\n((?:      [a-z-]+: (?:read|write)\n)+)", job, re.M)
    return dict(line.strip().split(": ") for line in block.group(1).splitlines()) if block else None


def path_filters():
    block = WORKFLOW.split("  pull_request:\n    paths:\n", 1)[1].split("\n  workflow_dispatch:", 1)[0]
    return re.findall(r"^      - '([^']+)'$", block, re.M)


def starts_dry_run(path):
    """Whether a pull request changing only PATH starts the dry run: GitHub's last matching filter decides."""
    selected = False
    for pattern in path_filters():
        if ci_plan.matches(pattern.lstrip("!"), path):
            selected = not pattern.startswith("!")
    return selected


def package_crates():
    """The workspace crates the packaged binaries are built from: their path dependencies, transitively."""
    names, dependencies = {}, {}
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        text = manifest.read_text()
        names[re.search(r'^name = "([^"]+)"', text, re.M).group(1)] = manifest.parent.name
        normal = re.search(r"^\[dependencies\]\n(.*?)(?=^\[|\Z)", text, re.M | re.S)
        dependencies[manifest.parent.name] = re.findall(r'path = "\.\./([^"]+)"', normal.group(1) if normal else "")
    found, pending = set(), [names[name] for name in package.PACKAGES]
    while pending:
        crate = pending.pop()
        if crate not in found:
            found.add(crate)
            pending.extend(dependencies[crate])
    return found


def script_modules(name, found=None):
    """SCRIPT and the modules of scripts/ it imports, transitively."""
    found = set() if found is None else found
    if name not in found:
        found.add(name)
        for module in re.findall(r"^(?:import|from) ([a-z_]+)", (ROOT / "scripts" / (name + ".py")).read_text(), re.M):
            if (ROOT / "scripts" / (module + ".py")).exists():
                script_modules(module, found)
    return found


class Workflow(unittest.TestCase):
    def setUp(self):
        self.jobs = jobs(WORKFLOW)

    def test_only_a_pull_request_or_a_manual_run_starts_it(self):
        header = WORKFLOW.split("\njobs:\n", 1)[0]
        on = header.split("\non:\n", 1)[1].split("\npermissions:\n", 1)[0]
        self.assertEqual(re.findall(r"^  ([a-z_]+):", on, re.M), ["pull_request", "workflow_dispatch"])
        self.assertIn("\npermissions:\n  contents: read\n", header)
        self.assertIn("format('pr-{0}', github.event.pull_request.number) || 'publish'", header)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", header)

    def test_a_pull_request_that_changes_what_a_package_is_built_from_starts_the_dry_run(self):
        inputs = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
                  ".github/workflows/release.yml"]
        inputs += [f"crates/{crate}/src/lib.rs" for crate in sorted(package_crates())]
        inputs += [f"crates/{crate}/Cargo.toml" for crate in sorted(package_crates())]
        inputs += [f"scripts/{module}.py" for module in sorted(script_modules("release"))]
        self.assertTrue({"package", "validate", "devguard_color"} <= script_modules("release"))
        for path in inputs:
            self.assertTrue(starts_dry_run(path), path)
        for path in ("docs/operations.md", "scripts/test_ci_release.py", ".github/workflows/ci.yml"):
            self.assertFalse(starts_dry_run(path), path)
        # An excluded crate is never one the packaged binaries are built from.
        for pattern in path_filters():
            if pattern.startswith("!"):
                crate = re.fullmatch(r"!crates/([a-z]+)/\*\*", pattern).group(1)
                self.assertNotIn(crate, package_crates())
                self.assertFalse(starts_dry_run(f"crates/{crate}/src/main.rs"))

    def test_each_job_holds_only_the_permissions_it_needs(self):
        self.assertEqual({name: permissions(job) for name, job in self.jobs.items()}, {
            "reproducible": {"contents": "read"},
            "required": {"checks": "read", "contents": "read"},
            "build": {"contents": "read"},
            "attest": {"attestations": "write", "contents": "read", "id-token": "write"},
            "publish": {"contents": "write"},
        })

    def test_only_a_manual_run_on_main_publishes_and_only_after_the_required_checks(self):
        required = self.jobs["required"]
        self.assertIn("    if: ${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' }}\n",
                      required)
        self.assertIn("run: python3 -B scripts/release.py required", required)
        # Its own check name must never be taken for the gate it reads.
        self.assertNotIn("    name: Required checks\n", required)
        gate = jobs((ROOT / ".github/workflows/ci.yml").read_text())["required"]
        self.assertIn(f"    name: {release.REQUIRED_CHECK}\n", gate)
        self.assertIn("    needs: required\n", self.jobs["build"])
        self.assertIn("    needs: build\n", self.jobs["attest"])
        self.assertIn("    needs: [required, build, attest]\n", self.jobs["publish"])
        self.assertIn("run: python3 -B scripts/release.py publish --dist target/release-dist", self.jobs["publish"])
        for name in ("build", "attest", "publish"):
            self.assertNotIn("\n    if:", self.jobs[name], "a skipped gate skips every later job")
        for name in ("required", "publish"):
            self.assertIn("GH_TOKEN: ${{ github.token }}", self.jobs[name])

    def test_the_release_is_built_once_on_macos_arm64_with_the_pinned_toolchain(self):
        install = (f"run: rustup toolchain install {validate.PINNED_RUST} --profile minimal --component clippy "
                   "--component rustfmt")
        for name in ("reproducible", "build"):
            self.assertIn("    runs-on: macos-14\n", self.jobs[name])
            self.assertIn(install, self.jobs[name])
        build = self.jobs["build"]
        order = [build.index(text) for text in (
            install, "run: python3 -B scripts/package.py --output target/release-package",
            "run: python3 -B scripts/release.py archive --package target/release-package --dist target/release-dist",
            "actions/upload-artifact@v6")]
        self.assertEqual(order, sorted(order))
        for name in ("required", "attest", "publish"):
            self.assertIn("    runs-on: ubuntu-24.04\n", self.jobs[name])

    def test_the_dry_run_builds_twice_from_two_paths_and_homes_and_publishes_nothing(self):
        dry = self.jobs["reproducible"]
        self.assertIn("    if: ${{ github.event_name == 'pull_request' }}\n", dry)
        checkouts = [step for step in steps(dry) if "actions/checkout@" in step]
        self.assertEqual([re.search(r"path: (\S+)", step).group(1) for step in checkouts],
                         ["first/DevGuard", "second/elsewhere/DevGuard"])
        second = [step for step in steps(dry) if "second home" in step or "second package" in step]
        self.assertEqual(len(second), 3)
        for step in second[:2]:
            self.assertIn("CARGO_HOME: ${{ runner.temp }}/second-cargo", step)
            self.assertIn("RUSTUP_HOME: ${{ runner.temp }}/second-rustup", step)
        # rustup lives in the first home only; updating itself from the second one would fail.
        self.assertIn("--component rustfmt --no-self-update\n", second[0])
        self.assertIn("working-directory: second/elsewhere/DevGuard", second[1])
        self.assertIn("run: python3 -B first/DevGuard/scripts/release.py compare first/DevGuard/target/release-dist "
                      "second/elsewhere/DevGuard/target/release-dist", dry)
        for absent in ("release.py publish", "release.py required", "attest", "GH_TOKEN", "write"):
            self.assertNotIn(absent, dry)
        upload = next(step for step in steps(dry) if "upload-artifact" in step)
        self.assertIn("if: failure()", upload)

    def test_provenance_covers_the_archive_and_the_manifest_from_the_build(self):
        attest = self.jobs["attest"]
        self.assertIn("uses: actions/attest-build-provenance@v4", attest)
        self.assertIn("subject-path: |\n            target/release-dist/*.tar.gz\n"
                      "            target/release-dist/MANIFEST.json\n", attest)
        self.assertNotIn("actions/checkout@", attest)
        # Each attempt hands its own assets on; a partial re-run finds none and fails.
        name = "name: release-assets-${{ github.run_attempt }}"
        upload = next(step for step in steps(self.jobs["build"]) if "upload-artifact" in step)
        self.assertIn(name, upload)
        self.assertIn("if-no-files-found: error", upload)
        for job in ("attest", "publish"):
            download = next(step for step in steps(self.jobs[job]) if "download-artifact" in step)
            self.assertIn("uses: actions/download-artifact@v7", download)
            self.assertIn(name, download)
            self.assertIn("path: target/release-dist", download)

    def test_every_checkout_drops_its_credentials(self):
        checkouts = {name: [step for step in steps(job) if "actions/checkout@" in step] for name, job in self.jobs.items()}
        self.assertEqual({name: len(found) for name, found in checkouts.items()},
                         {"reproducible": 2, "required": 1, "build": 1, "attest": 0, "publish": 1})
        for step in sum(checkouts.values(), []):
            self.assertIn("uses: actions/checkout@v5", step)
            self.assertIn("persist-credentials: false", step)

    def test_no_expression_is_expanded_inside_a_script(self):
        self.assertNotRegex(WORKFLOW, r"run: *[|>]")
        for line in WORKFLOW.splitlines():
            if line.strip().startswith("run:"):
                self.assertNotIn("${{", line)


class PackageBuild(unittest.TestCase):
    """package.py's build environment and its refusal of an embedded build path, with Git, rustc and
    Cargo replaced by stand-ins."""

    def build(self, leak=b"", system="Linux"):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name) / "checkout"
        built = root / "target" / "release"
        built.mkdir(parents=True)
        for name, data in binaries().items():
            (built / name).write_bytes(data + (leak.replace(b"ROOT", str(root).encode()) if name == "devguard" else b""))
        answers = {("git", "status", "--porcelain"): "", ("rustc", "-vV"): RUSTC,
                   ("rustc", "--print", "sysroot"): "/opt/rust/toolchains/1.95.0",
                   ("git", "rev-parse", "HEAD^{tree}"): "e" * 40, ("cargo", "-V"): "cargo 1.95.0"}
        calls = []

        def capture(args, environment=None):
            if args[0] == str(built / "devguardd"):
                return json.dumps({"package_version": "0.1.0"})
            return answers[tuple(args)]

        def run(args, **options):
            calls.append((args, options))
            return subprocess.CompletedProcess(args, 0, "tool 1.0\n", "")

        output = Path(temporary.name) / "package"
        environment = {"HOME": "/home/builder", "PATH": os.environ.get("PATH", ""), "RUSTFLAGS": "-C target-cpu=native"}
        with mock.patch.object(package, "ROOT", root), mock.patch.object(package, "capture", capture), \
                mock.patch.object(package.subprocess, "run", run), \
                mock.patch.object(package.platform, "system", lambda: system), \
                mock.patch.object(package, "source_fingerprint", lambda: {"head": COMMIT, "tree_digest": "d" * 64}), \
                mock.patch.dict(os.environ, environment, clear=True), \
                mock.patch.object(sys, "argv", ["package.py", "--output", str(output), "--color", "never"]), \
                contextlib.redirect_stdout(io.StringIO()) as printed, color_choice():
            try:
                package.main()
                refused = None
            except SystemExit as exit:
                refused = exit.code
        cargo = [options["env"] for args, options in calls if args[0] == "cargo"]
        return root, output, cargo, printed.getvalue(), refused

    def test_the_build_remaps_every_build_path_and_pins_the_deployment_target(self):
        root, output, cargo, printed, refused = self.build()
        self.assertIsNone(refused)
        self.assertEqual(len(cargo), 1)
        flags = cargo[0]["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
        for source, target in ((str(root), "/devguard"), ("/home/builder/.cargo", "/cargo"),
                               ("/opt/rust/toolchains/1.95.0", "/toolchain")):
            self.assertIn(f"--remap-path-prefix={source}={target}", flags)
        # Only these flags reach rustc: an inherited RUSTFLAGS is overridden.
        self.assertTrue(all(flag.startswith("--remap-path-prefix=") for flag in flags), flags)
        self.assertEqual((cargo[0]["MACOSX_DEPLOYMENT_TARGET"], cargo[0]["CARGO_TARGET_DIR"],
                          cargo[0]["CARGO_BUILD_JOBS"]), ("14.0", str(root / "target"), "1"))
        text = (output / "MANIFEST.json").read_text()
        manifest = json.loads(text)
        self.assertEqual(manifest["build"]["environment"], {"CARGO_BUILD_JOBS": "1", "MACOSX_DEPLOYMENT_TARGET": "14.0"})
        self.assertEqual(manifest["build"]["remap_path_prefix"], package.REMAPPED)
        self.assertEqual((manifest["build"]["link_flags"], list(manifest["build"]["host_tools"])), ([], ["cc"]))
        for path in (str(root), "/home/builder"):
            self.assertNotIn(path, text)
        self.assertEqual(json.loads(printed.splitlines()[-1]),
                         {"package": str(output), "release_id": manifest["release_id"]})
        release.read_package(output)

    def test_a_macos_link_leaves_out_the_debug_map_and_the_manifest_names_the_tools(self):
        _, output, cargo, _, refused = self.build(system="Darwin")
        self.assertIsNone(refused)
        self.assertEqual(cargo[0]["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")[-1], "-Clink-arg=-Wl,-S")
        build = json.loads((output / "MANIFEST.json").read_text())["build"]
        self.assertEqual(build["link_flags"], ["-Clink-arg=-Wl,-S"])
        self.assertEqual(sorted(build["host_tools"]), ["cc", "ld", "sdk", "sdk_build", "xcode"])

    def test_a_build_that_embeds_a_build_path_is_refused_before_anything_is_written(self):
        for leak, named in ((b"ROOT/crates/cli/src/main.rs", "checkout"),
                            (b"/home/builder/.cargo/registry/src/x.rs", "cargo_home, home"),
                            (b"/opt/rust/toolchains/1.95.0/lib/rustlib", "toolchain")):
            with self.subTest(named=named):
                _, output, _, _, refused = self.build(leak)
                self.assertEqual(refused, f"the built devguard contains a build path ({named}); nothing was packaged")
                self.assertFalse(output.exists())


class BuildPaths(unittest.TestCase):
    def test_each_form_of_each_path_is_remapped_and_the_longest_last(self):
        with tempfile.TemporaryDirectory() as directory:
            real = Path(directory) / "real"
            (real / "DevGuard").mkdir(parents=True)
            link = Path(directory) / "link"
            link.symlink_to(real)
            paths = {"checkout": str(link / "DevGuard"), "cargo_home": str(link / "DevGuard" / ".cargo-home"),
                     "toolchain": "/opt/rust/toolchains/1.95.0"}
            flags = package.remap_flags(paths)
            resolved = os.path.realpath(real / "DevGuard")
        sources = [flag.split("=", 1)[1].rsplit("=", 1)[0] for flag in flags]
        self.assertEqual([len(source) for source in sources], sorted(len(source) for source in sources))
        for source, target in ((paths["checkout"], "/devguard"), (resolved, "/devguard"),
                               (paths["cargo_home"], "/cargo"), (resolved + "/.cargo-home", "/cargo"),
                               (paths["toolchain"], "/toolchain")):
            self.assertIn(f"--remap-path-prefix={source}={target}", flags)
        # A Cargo home inside the checkout keeps its own name.
        self.assertGreater(flags.index(f"--remap-path-prefix={paths['cargo_home']}=/cargo"),
                           flags.index(f"--remap-path-prefix={paths['checkout']}=/devguard"))

    def test_a_relative_cargo_home_is_taken_from_the_checkout(self):
        self.assertEqual(package.build_paths({"HOME": "/home/b", "CARGO_HOME": ".cargo-home"}, "/sysroot"),
                         {"checkout": str(package.ROOT), "cargo_home": str(package.ROOT / ".cargo-home"),
                          "toolchain": "/sysroot"})
        self.assertEqual(package.build_paths({"HOME": "/home/b"}, "/sysroot")["cargo_home"], "/home/b/.cargo")
        self.assertEqual(package.build_paths({"CARGO_HOME": "/c"}, "/sysroot")["cargo_home"], "/c")

    def test_a_build_path_is_found_only_where_a_path_starts(self):
        paths = {"checkout": "/w/DevGuard", "cargo_home": "/home/u/.cargo", "toolchain": "/t/1.95.0"}
        environment = {"HOME": "/home/u"}
        self.assertEqual(package.leaks(b"at /devguard/crates/x.rs and /cargo/registry/src", paths, environment), [])
        self.assertEqual(package.leaks(b"/home/user/elsewhere and /w/DevGuard2/x", paths, environment), [])
        self.assertEqual(package.leaks(b"at /home/u/.cargo/registry/src/x.rs", paths, environment),
                         ["cargo_home", "home"])
        self.assertEqual(package.leaks(b"/w/DevGuard/crates/cli", paths, environment), ["checkout"])
        self.assertEqual(package.leaks(b"/t/1.95.0/lib/rustlib", paths, environment), ["toolchain"])
        # A home at the root names no path of its own.
        self.assertEqual(package.leaks(b"/usr/lib/x", {"checkout": "/w/DevGuard"}, {"HOME": "/"}), [])

    def test_the_release_id_keeps_its_format(self):
        artifacts = {name: {"sha256": sha256(data), "bytes": len(data)} for name, data in binaries().items()}
        digest = sha256(json.dumps(artifacts, sort_keys=True).encode())[:8]
        release_id = package.release_identity("0.1.0", COMMIT, artifacts)
        self.assertEqual(release_id, f"0.1.0-c0ffee0-{digest}")
        self.assertTrue(release.RELEASE_ID.fullmatch(release_id))


class Archive(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)

    def dist(self, name, contents=None, **options):
        manifest = make_package(self.directory / name / "package", contents, **options)
        return manifest, release.archive(self.directory / name / "package", self.directory / name / "dist", env={})

    def test_the_archive_depends_only_on_the_package(self):
        manifest, first = self.dist("first")
        os.utime(self.directory / "first/package/bin/devguard", (1, 1))
        _, second = self.dist("second/elsewhere")
        name = f"devguard-{manifest['release_id']}-aarch64-apple-darwin.tar.gz"
        self.assertEqual(Path(first["archive"]).name, name)
        self.assertEqual(first["archive_sha256"], second["archive_sha256"])
        self.assertEqual((self.directory / "first/dist" / release.SUMS).read_text(),
                         (self.directory / "second/elsewhere/dist" / release.SUMS).read_text())
        data = Path(first["archive"]).read_bytes()
        self.assertEqual(data[4:8], b"\0\0\0\0", "gzip records no time")
        self.assertFalse(data[3] & 0x08, "gzip records no name")
        with tarfile.open(first["archive"]) as archive:
            members = archive.getmembers()
        prefix = manifest["release_id"]
        self.assertEqual([member.name for member in members],
                         [prefix, f"{prefix}/MANIFEST.json", f"{prefix}/bin"]
                         + [f"{prefix}/bin/{binary}" for binary in sorted(package.BINARIES)])
        for member in members:
            self.assertEqual((member.uid, member.gid, member.uname, member.gname, member.mtime), (0, 0, "", "", 0))

    @unittest.skipUnless(shutil.which("tar"), "no tar")
    def test_the_archive_extracts_to_a_package_the_installer_accepts(self):
        manifest, built = self.dist("first")
        extracted = self.directory / "extracted"
        extracted.mkdir()
        subprocess.run(["tar", "-xzf", built["archive"], "-C", str(extracted)], check=True)
        self.assertEqual(sorted(path.name for path in extracted.iterdir()), [manifest["release_id"]])
        raw, found, _, _ = release.read_package(extracted / manifest["release_id"])
        self.assertEqual(raw, (self.directory / "first/package/MANIFEST.json").read_bytes())
        self.assertEqual(found, manifest)

    def test_a_package_that_disagrees_with_its_manifest_is_refused(self):
        def changed(name, change):
            make_package(self.directory / name)
            change(self.directory / name)
            with self.assertRaises(release.ReleaseError):
                release.archive(self.directory / name, self.directory / (name + "-dist"), env={})
            self.assertFalse((self.directory / (name + "-dist")).exists())

        def edit_manifest(field, value):
            def change(directory):
                manifest = json.loads((directory / "MANIFEST.json").read_text())
                manifest[field] = value
                (directory / "MANIFEST.json").write_text(json.dumps(manifest))
            return change

        changed("binary", lambda directory: (directory / "bin/devguard").write_bytes(b"other"))
        changed("extra", lambda directory: (directory / "bin/extra").write_bytes(b"x"))
        changed("missing", lambda directory: (directory / "bin/devguard-launch").unlink())
        changed("mode", lambda directory: os.chmod(directory / "bin/devguardd", 0o644))
        changed("top", lambda directory: (directory / "README").write_text("x"))
        changed("schema", edit_manifest("schema", "other/v1"))
        changed("id", edit_manifest("release_id", "0.1.0-c0ffee0-00000000"))

        def symlink(directory):
            (directory / "bin/devguard").rename(directory / "devguard")
            (directory / "bin/devguard").symlink_to(directory / "devguard")
            (directory / "devguard").rename(directory / "bin/devguard.real")
        changed("symlink", symlink)

    def test_an_existing_dist_directory_is_never_reused(self):
        make_package(self.directory / "package")
        (self.directory / "dist").mkdir()
        with self.assertRaises(FileExistsError):
            release.archive(self.directory / "package", self.directory / "dist", env={})

    def test_assets_that_do_not_match_are_refused(self):
        manifest, built = self.dist("first")
        dist = self.directory / "first/dist"
        archive = Path(built["archive"])
        release.verify_dist(dist)
        raw = (dist / release.MANIFEST).read_bytes()
        cases = {
            "an extra asset": lambda: (dist / "notes.txt").write_text("x"),
            "a changed archive": lambda: archive.write_bytes(bytes(byte ^ (index == 20) for index, byte
                                                               in enumerate(archive.read_bytes()))),
            "a changed binary with matching sums": lambda: (archive.write_bytes(release.archive_bytes(
                manifest["release_id"], raw, binaries(b" changed"))), rewrite_sums(dist)),
            "a changed manifest with matching sums": lambda: (
                (dist / release.MANIFEST).write_bytes(raw.replace(b"2026-10-19", b"2026-10-20")), rewrite_sums(dist)),
        }
        for case, change in cases.items():
            with self.subTest(case=case):
                saved = {path.name: path.read_bytes() for path in dist.iterdir()}
                change()
                with self.assertRaises(release.ReleaseError):
                    release.verify_dist(dist)
                for path in dist.iterdir():
                    if path.name not in saved:
                        path.unlink()
                for name, data in saved.items():
                    (dist / name).write_bytes(data)
                release.verify_dist(dist)

    def test_two_builds_are_one_release_only_with_the_same_binaries(self):
        manifest, _ = self.dist("first")
        self.dist("second", created="2026-10-19T00:01:00+00:00")
        self.dist("other", binaries(b" built elsewhere"))
        same = release.compare(self.directory / "first/dist", self.directory / "second/dist")
        self.assertEqual(same, (manifest["release_id"], []))
        release_id, problems = release.compare(self.directory / "first/dist", self.directory / "other/dist")
        self.assertEqual(len(problems), len(package.BINARIES) + 1)
        self.assertTrue(problems[-1].startswith(f"release ids {release_id} and "))
        for argv, code, text in (
                (["compare", str(self.directory / "first/dist"), str(self.directory / "second/dist")], 0,
                 f"reproducible: passed (both builds make {release_id})\n"),
                (["compare", str(self.directory / "first/dist"), str(self.directory / "other/dist")], 1,
                 "reproducible: failed\n" + "".join(f"  - {problem}\n" for problem in problems))):
            with color_choice(), contextlib.redirect_stdout(io.StringIO()) as printed:
                self.assertEqual(release.main(argv + ["--color", "never"]), code)
            self.assertEqual(printed.getvalue(), text)

    def test_the_archive_command_prints_plain_json_whatever_the_color(self):
        make_package(self.directory / "package")
        with color_choice(), contextlib.redirect_stdout(io.StringIO()) as printed:
            self.assertEqual(release.main(["archive", "--package", str(self.directory / "package"), "--dist",
                                           str(self.directory / "dist"), "--color", "always"]), 0)
        self.assertNotIn("\033", printed.getvalue())
        self.assertEqual(set(json.loads(printed.getvalue())), {"release_id", "archive", "archive_sha256"})


class Required(unittest.TestCase):
    def check(self, run_id, conclusion="success", status="completed", name="Required checks", sha=COMMIT,
              app="github-actions"):
        return {"id": run_id, "name": name, "head_sha": sha, "status": status, "conclusion": conclusion,
                "app": {"slug": app}}

    def test_only_a_manual_run_on_main_may_release(self):
        self.assertEqual(release.actions_context(DISPATCH), (REPOSITORY, COMMIT))
        for change in ({"GITHUB_EVENT_NAME": "push"}, {"GITHUB_EVENT_NAME": "pull_request"},
                       {"GITHUB_REF": "refs/heads/feature"}, {"GITHUB_REF": "refs/tags/v1"},
                       {"GITHUB_SHA": "HEAD"}, {"GITHUB_REPOSITORY": ""}):
            with self.subTest(change=change), self.assertRaises(release.ReleaseError):
                release.actions_context({**DISPATCH, **change})

    def test_the_latest_required_checks_run_of_github_actions_decides(self):
        cases = [
            ([], False), ([self.check(1)], True),
            ([self.check(1), self.check(2, "failure")], False),
            ([self.check(1, "failure"), self.check(2)], True),
            ([self.check(1), self.check(2, None, "in_progress")], False),
            ([self.check(1, app="other-app")], False),
            ([self.check(1, name="Verify the required checks")], False),
            ([self.check(1, sha="1" * 40)], False),
            ([self.check(3, "failure", app="other-app"), self.check(2)], True),
        ]
        for runs, passed in cases:
            with self.subTest(runs=runs):
                self.assertEqual(release.required_result(runs, COMMIT)[0], passed)

    def test_the_check_runs_of_this_commit_are_read(self):
        calls = []

        def run(args, **options):
            calls.append(args)
            return subprocess.CompletedProcess(args, 0, json.dumps({"check_runs": [self.check(7)]}), "")
        self.assertEqual(release.required(DISPATCH, run), (COMMIT, True, "the latest Required checks run passed"))
        self.assertEqual(calls, [["gh", "api", f"repos/{REPOSITORY}/commits/{COMMIT}/check-runs"
                                  "?check_name=Required%20checks&filter=all&per_page=100"]])


class FakeGitHub:
    """`gh` against a repository with no release yet, unless told otherwise."""

    def __init__(self, release_exists=False, view_error="release not found", tags=(), corrupt=False, tag_sha=COMMIT):
        self.release_exists, self.view_error, self.tags = release_exists, view_error, tags
        self.corrupt, self.tag_sha, self.calls, self.assets = corrupt, tag_sha, [], {}

    def __call__(self, args, **options):
        self.calls.append(args)
        done = lambda stdout="", code=0, stderr="": subprocess.CompletedProcess(args, code, stdout, stderr)  # noqa: E731
        if args[1:3] == ["release", "view"]:
            return done('{"tagName": "x"}') if self.release_exists else done(code=1, stderr=self.view_error)
        if args[1] == "api" and "/git/matching-refs/tags/" in args[2]:
            return done(json.dumps([{"ref": "refs/tags/" + tag} for tag in self.tags]))
        if args[1:3] == ["release", "create"]:
            self.notes = Path(args[args.index("--notes-file") + 1]).read_text()
            self.assets = {Path(path).name: Path(path).read_bytes() for path in args[args.index("--notes-file") + 2:]}
            return done()
        if args[1:3] == ["release", "download"]:
            directory = Path(args[args.index("--dir") + 1])
            directory.mkdir(parents=True)
            for name, data in self.assets.items():
                (directory / name).write_bytes(data + (b"\0" if self.corrupt and name == release.MANIFEST else b""))
            return done()
        if args[1:3] in (["release", "edit"], ["release", "delete"]):
            return done()
        if args[1] == "api" and "/git/ref/tags/" in args[2]:
            return done(json.dumps({"ref": "refs/tags/x", "object": {"type": "commit", "sha": self.tag_sha}}))
        raise AssertionError(args)

    def commands(self):
        return [" ".join(args[1:3]) if args[1] != "api" else "api " + args[2].split("/")[-3] for args in self.calls]


class Publish(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.manifest = make_package(self.directory / "package")
        release.archive(self.directory / "package", self.directory / "dist", env={})
        self.release_id = self.manifest["release_id"]

    def publish(self, github, env=None):
        return release.publish({**DISPATCH, **(env or {})}, self.directory / "dist", github)

    def test_a_release_is_published_only_after_its_draft_downloads_intact(self):
        github = FakeGitHub()
        summary = self.directory / "summary.md"
        self.assertEqual(self.publish(github, {"GITHUB_STEP_SUMMARY": str(summary)}), (self.release_id, COMMIT))
        self.assertEqual(github.commands(), ["release view", "api matching-refs", "release create",
                                             "release download", "release edit", "api ref"])
        create = github.calls[2]
        self.assertEqual(create[3:13], [self.release_id, "--repo", REPOSITORY, "--draft", "--prerelease", "--target",
                                        COMMIT, "--title", self.release_id, "--notes-file"])
        self.assertEqual(sorted(github.assets), sorted(path.name for path in (self.directory / "dist").iterdir()))
        self.assertEqual(github.calls[4][3:], [self.release_id, "--repo", REPOSITORY, "--draft=false"])
        self.assertIn(f"https://github.com/{REPOSITORY}/blob/main/docs/operations.md#installing-a-published-release",
                      github.notes)
        self.assertIn((self.directory / "dist" / release.SUMS).read_text(), github.notes)
        self.assertIn("\n## Installing a published release\n", (ROOT / "docs/operations.md").read_text())
        self.assertIn(f"### Published `{self.release_id}`", summary.read_text())

    def test_an_existing_release_or_tag_is_never_replaced(self):
        for github, problem in ((FakeGitHub(release_exists=True), "exists already"),
                                (FakeGitHub(tags=[self.release_id]), "exists already"),
                                (FakeGitHub(view_error="HTTP 502"), "cannot tell whether")):
            with self.subTest(problem=problem):
                with self.assertRaisesRegex(release.ReleaseError, problem):
                    self.publish(github)
                self.assertNotIn("release create", github.commands())
        # Another tag that only shares the prefix is not this release's tag.
        self.publish(FakeGitHub(tags=[self.release_id + "-rc"]))

    def test_a_draft_that_downloads_differently_is_deleted_and_never_published(self):
        github = FakeGitHub(corrupt=True)
        with self.assertRaisesRegex(release.ReleaseError, "differ from the build; the draft was deleted"):
            self.publish(github)
        self.assertEqual(github.commands()[-2:], ["release download", "release delete"])
        self.assertNotIn("release edit", github.commands())

    def test_assets_of_another_commit_or_another_run_are_refused(self):
        for env in ({"GITHUB_SHA": "1" * 40}, {"GITHUB_EVENT_NAME": "push"}, {"GITHUB_REF": "refs/heads/other"}):
            github = FakeGitHub()
            with self.subTest(env=env), self.assertRaises(release.ReleaseError):
                self.publish(github, env)
            self.assertEqual(github.calls, [])

    def test_a_published_tag_must_name_the_commit(self):
        with self.assertRaisesRegex(release.ReleaseError, "does not name"):
            self.publish(FakeGitHub(tag_sha="2" * 40))


if __name__ == "__main__":
    unittest.main()
