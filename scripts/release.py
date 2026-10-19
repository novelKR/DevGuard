#!/usr/bin/env python3
"""Turn a release package into a GitHub Release; the release workflow runs each step.

  release.py archive --package DIR --dist DIR  write the assets: the archive, MANIFEST.json, SHA256SUMS
  release.py compare DIST DIST                 require two builds of one commit to make one release
  release.py required                          require "Required checks" to have passed on this commit
  release.py publish --dist DIR                publish the assets as the release named by the release id

The archive `devguard-<release-id>-<target>.tar.gz` holds only `<release-id>/MANIFEST.json` and
the three binaries in `<release-id>/bin/`, so it extracts to a package that `devguardd install
--package` accepts. Its entries are sorted, owned by 0:0 with fixed modes and times, and gzip
records no name or time. `required` and `publish` act only in a manual run on main. They read the
repository and commit from the GitHub Actions environment and call the GitHub API through `gh`.
`publish` uploads to a draft, checks each downloaded asset against SHA256SUMS and only then
publishes the draft, which creates the tag on the commit. An existing release or tag is never
replaced.
"""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import devguard_color as color  # noqa: E402
import package  # noqa: E402

MANIFEST = "MANIFEST.json"
SUMS = "SHA256SUMS"
MANIFEST_SCHEMA = "devguard-release-manifest/v1"
REQUIRED_CHECK = "Required checks"
# The installer's rule for a release id.
RELEASE_ID = re.compile(r"(?!\.)[A-Za-z0-9._-]{1,128}")
TARGET = re.compile(r"^host: ([A-Za-z0-9_.-]+)$", re.M)
SUM_LINE = re.compile(r"([0-9a-f]{64})  (\S+)")


class ReleaseError(Exception):
    pass


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def check_manifest(manifest):
    """Refuse a manifest whose id is not the one its version, commit and binaries make."""
    try:
        release_id = manifest["release_id"]
        expected = package.release_identity(manifest["version"], manifest["source"]["commit"],
                                            manifest["artifacts"])
        target = TARGET.search(manifest["build"]["rustc"]).group(1)
    except (KeyError, TypeError, AttributeError):
        raise ReleaseError("the manifest is not a release manifest") from None
    if manifest.get("schema") != MANIFEST_SCHEMA or not RELEASE_ID.fullmatch(release_id):
        raise ReleaseError("the manifest is not a release manifest")
    if sorted(manifest["artifacts"]) != sorted(package.BINARIES) or release_id != expected:
        raise ReleaseError("the release id does not match the manifest's binaries")
    return f"devguard-{release_id}-{target}.tar.gz"


def entries(directory):
    return sorted(path.name for path in Path(directory).iterdir())


def read_package(directory):
    """A package's manifest bytes, manifest and binaries, refused unless they agree."""
    directory = Path(directory)
    if entries(directory) != sorted([MANIFEST, "bin"]) or entries(directory / "bin") != sorted(package.BINARIES):
        raise ReleaseError("a package holds exactly MANIFEST.json and bin/ with the three binaries")
    raw = (directory / MANIFEST).read_bytes()
    try:
        manifest = json.loads(raw)
    except ValueError:
        raise ReleaseError("the manifest is not JSON") from None
    name = check_manifest(manifest)
    binaries = {}
    for binary in package.BINARIES:
        path = directory / "bin" / binary
        mode = path.lstat().st_mode
        if not stat.S_ISREG(mode) or not mode & stat.S_IXUSR:
            raise ReleaseError(f"{binary} is not an executable regular file")
        data = path.read_bytes()
        if manifest["artifacts"][binary] != {"sha256": sha256(data), "bytes": len(data)}:
            raise ReleaseError(f"{binary} does not match its manifest hash")
        binaries[binary] = data
    return raw, manifest, binaries, name


def archive_bytes(release_id, raw_manifest, binaries):
    """The release archive, whose bytes depend only on its contents."""
    members = [(release_id, None, 0o755), (f"{release_id}/{MANIFEST}", raw_manifest, 0o644),
               (f"{release_id}/bin", None, 0o755)]
    members += [(f"{release_id}/bin/{name}", binaries[name], 0o755) for name in sorted(binaries)]
    tar = io.BytesIO()
    with tarfile.open(fileobj=tar, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, data, mode in members:
            info = tarfile.TarInfo(name)
            info.mode, info.mtime, info.uid, info.gid, info.uname, info.gname = mode, 0, 0, 0, "", ""
            if data is None:
                info.type = tarfile.DIRTYPE
                archive.addfile(info)
            else:
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
    compressed = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=compressed, mtime=0, compresslevel=9) as stream:
        stream.write(tar.getvalue())
    return compressed.getvalue()


def archive_contents(data):
    """The members of a release archive: name to (type, mode, bytes or None)."""
    found = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for info in archive.getmembers():
            if info.name in found or not (info.isdir() or info.isfile()):
                raise ReleaseError("the archive holds a duplicate or special entry")
            body = archive.extractfile(info).read() if info.isfile() else None
            found[info.name] = ("dir" if info.isdir() else "file", info.mode, body)
    return found


def sums(files):
    return "".join(f"{sha256(Path(path).read_bytes())}  {name}\n" for name, path in sorted(files.items()))


def archive(package_dir, dist, env=os.environ):
    raw, manifest, binaries, name = read_package(package_dir)
    dist = Path(dist)
    dist.mkdir(parents=True)
    (dist / name).write_bytes(archive_bytes(manifest["release_id"], raw, binaries))
    (dist / MANIFEST).write_bytes(raw)
    text = sums({name: dist / name, MANIFEST: dist / MANIFEST})
    (dist / SUMS).write_text(text)
    if env.get("GITHUB_STEP_SUMMARY"):
        with open(env["GITHUB_STEP_SUMMARY"], "a") as handle:
            handle.write(f"### Release package `{manifest['release_id']}`\n\n```\n{text}```\n")
    return {"release_id": manifest["release_id"], "archive": str(dist / name),
            "archive_sha256": sha256((dist / name).read_bytes())}


def verify_dist(dist):
    """The manifest and assets of a dist directory, refused unless every asset agrees."""
    dist = Path(dist)
    raw = (dist / MANIFEST).read_bytes()
    try:
        manifest = json.loads(raw)
    except ValueError:
        raise ReleaseError("the manifest is not JSON") from None
    name = check_manifest(manifest)
    files = {name: dist / name, MANIFEST: dist / MANIFEST}
    if entries(dist) != sorted([*files, SUMS]):
        raise ReleaseError(f"the assets are exactly {name}, MANIFEST.json and SHA256SUMS")
    listed = {}
    for line in (dist / SUMS).read_text().splitlines():
        found = SUM_LINE.fullmatch(line)
        if not found or found.group(2) in listed:
            raise ReleaseError("SHA256SUMS is malformed")
        listed[found.group(2)] = found.group(1)
    if listed != {asset: sha256(path.read_bytes()) for asset, path in files.items()}:
        raise ReleaseError("SHA256SUMS does not match the assets")
    release_id = manifest["release_id"]
    expected = {release_id: ("dir", 0o755, None), f"{release_id}/bin": ("dir", 0o755, None),
                f"{release_id}/{MANIFEST}": ("file", 0o644, raw)}
    contents = archive_contents((dist / name).read_bytes())
    for binary in package.BINARIES:
        body = contents.get(f"{release_id}/bin/{binary}", (None, None, b""))[2] or b""
        if manifest["artifacts"][binary] != {"sha256": sha256(body), "bytes": len(body)}:
            raise ReleaseError(f"the archive's {binary} does not match the manifest")
        expected[f"{release_id}/bin/{binary}"] = ("file", 0o755, body)
    if contents != expected:
        raise ReleaseError("the archive holds other entries than the package")
    return manifest, files


def compare(first, second):
    """Problems that make two builds of one commit different releases."""
    one, _ = verify_dist(first)
    other, _ = verify_dist(second)
    problems = []
    if one["source"]["commit"] != other["source"]["commit"]:
        problems.append("the builds are of different commits")
    for binary in package.BINARIES:
        if one["artifacts"][binary] != other["artifacts"][binary]:
            problems.append(f"{binary}: {one['artifacts'][binary]['sha256'][:12]} and "
                            f"{other['artifacts'][binary]['sha256'][:12]}")
    if one["release_id"] != other["release_id"]:
        problems.append(f"release ids {one['release_id']} and {other['release_id']}")
    return one["release_id"], problems


def actions_context(env):
    """The repository and commit of a manual run on main; any other run is refused."""
    if env.get("GITHUB_EVENT_NAME") != "workflow_dispatch" or env.get("GITHUB_REF") != "refs/heads/main":
        raise ReleaseError("a release is made only by a manual run on refs/heads/main")
    repository, commit = env.get("GITHUB_REPOSITORY") or "", env.get("GITHUB_SHA") or ""
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ReleaseError("GITHUB_REPOSITORY or GITHUB_SHA is missing")
    return repository, commit


def gh(run, *args):
    completed = run(["gh", *args], capture_output=True, text=True)
    if completed.returncode != 0:
        raise ReleaseError(f"gh {' '.join(args[:2])} failed: {completed.stderr.strip()[-500:]}")
    return completed.stdout


def required_result(check_runs, commit):
    """Whether the latest "Required checks" run of GitHub Actions on COMMIT passed, and why not."""
    runs = [run for run in check_runs if run.get("name") == REQUIRED_CHECK and run.get("head_sha") == commit
            and (run.get("app") or {}).get("slug") == "github-actions"]
    if not runs:
        return False, "no Required checks run on this commit"
    latest = max(runs, key=lambda run: run.get("id") or 0)
    if latest.get("status") != "completed":
        return False, f"the latest Required checks run is {latest.get('status')}"
    if latest.get("conclusion") != "success":
        return False, f"the latest Required checks run concluded {latest.get('conclusion')}"
    return True, "the latest Required checks run passed"


def required(env, run=subprocess.run):
    repository, commit = actions_context(env)
    found = json.loads(gh(run, "api", f"repos/{repository}/commits/{commit}/check-runs"
                                      "?check_name=Required%20checks&filter=all&per_page=100"))
    passed, reason = required_result(found.get("check_runs") or [], commit)
    return commit, passed, reason


def release_notes(manifest, sums_text, repository):
    target = TARGET.search(manifest["build"]["rustc"]).group(1)
    toolchain = re.search(r"^release: (\S+)$", manifest["build"]["rustc"], re.M)
    return (f"DevGuard release package `{manifest['release_id']}`, built by the release workflow from "
            f"{manifest['source']['commit']} for {target} with Rust {toolchain.group(1) if toolchain else 'unknown'}.\n\n"
            f"It is a functional build (`scope: {manifest['scope']}`, `slo_qualified: "
            f"{str(manifest['slo_qualified']).lower()}`); the milestone ledger names the qualified release. "
            f"To verify and install it, see [Installing a published release](https://github.com/{repository}"
            "/blob/main/docs/operations.md#installing-a-published-release).\n\n"
            f"SHA-256:\n\n```\n{sums_text}```\n")


def check_downloaded(directory, files):
    found = {path.name: sha256(path.read_bytes()) for path in Path(directory).iterdir()}
    if found != {name: sha256(Path(path).read_bytes()) for name, path in files.items()}:
        raise ReleaseError("the uploaded assets differ from the build")


def publish(env, dist, run=subprocess.run):
    repository, commit = actions_context(env)
    manifest, files = verify_dist(dist)
    files[SUMS] = Path(dist) / SUMS
    release_id = manifest["release_id"]
    if manifest["source"]["commit"] != commit:
        raise ReleaseError("the package was built from another commit")
    existing = run(["gh", "release", "view", release_id, "--repo", repository, "--json", "tagName"],
                   capture_output=True, text=True)
    if existing.returncode == 0:
        raise ReleaseError(f"the release {release_id} exists already; a release is never replaced")
    if "not found" not in existing.stderr.lower():
        raise ReleaseError(f"cannot tell whether the release {release_id} exists: {existing.stderr.strip()[-300:]}")
    refs = json.loads(gh(run, "api", f"repos/{repository}/git/matching-refs/tags/{release_id}"))
    if any(ref.get("ref") == f"refs/tags/{release_id}" for ref in refs):
        raise ReleaseError(f"the tag {release_id} exists already; a release is never replaced")
    with tempfile.TemporaryDirectory() as temporary:
        notes = Path(temporary) / "notes.md"
        notes.write_text(release_notes(manifest, files[SUMS].read_text(), repository))
        gh(run, "release", "create", release_id, "--repo", repository, "--draft", "--prerelease",
           "--target", commit, "--title", release_id, "--notes-file", str(notes),
           *[str(files[name]) for name in sorted(files)])
        try:
            downloaded = Path(temporary) / "downloaded"
            gh(run, "release", "download", release_id, "--repo", repository, "--dir", str(downloaded))
            check_downloaded(downloaded, files)
        except (ReleaseError, OSError) as error:
            run(["gh", "release", "delete", release_id, "--repo", repository, "--yes"], capture_output=True, text=True)
            raise ReleaseError(f"{error}; the draft was deleted") from None
        gh(run, "release", "edit", release_id, "--repo", repository, "--draft=false")
    tag = json.loads(gh(run, "api", f"repos/{repository}/git/ref/tags/{release_id}"))
    if (tag.get("object") or {}).get("sha") != commit:
        raise ReleaseError(f"the published tag {release_id} does not name {commit}")
    if env.get("GITHUB_STEP_SUMMARY"):
        with open(env["GITHUB_STEP_SUMMARY"], "a") as handle:
            handle.write(f"### Published `{release_id}`\n\nA pre-release with its tag on `{commit}`.\n")
    return release_id, commit


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=["archive", "compare", "required", "publish"])
    parser.add_argument("dists", nargs="*", type=Path, help="compare: the two directories of release assets")
    parser.add_argument("--package", type=Path, help="archive: the package directory")
    parser.add_argument("--dist", type=Path, help="archive: a new directory for the release assets; publish: that directory")
    color.add_option(parser)
    args = parser.parse_args(argv)
    color.choose(args.color)
    wanted = {"archive": (["package", "dist"], 0), "compare": ([], 2), "required": ([], 0), "publish": (["dist"], 0)}
    options, count = wanted[args.command]
    given = [name for name in ("package", "dist") if getattr(args, name) is not None]
    if given != options or len(args.dists) != count:
        parser.error(args.command + " takes " + (" and ".join("--" + name for name in options) or
                                                 (f"{count} directories" if count else "no arguments")))
    try:
        if args.command == "archive":
            print(json.dumps(archive(args.package, args.dist)))
            return 0
        if args.command == "compare":
            release_id, problems = compare(*args.dists)
            if problems:
                print("reproducible: " + color.status("failed")
                      + "".join("\n  - " + color.paint(problem, "dim") for problem in problems), flush=True)
                return 1
            print("reproducible: " + color.status("passed") + color.paint(f" (both builds make {release_id})", "dim"),
                  flush=True)
            return 0
        if args.command == "required":
            commit, passed, reason = required(os.environ)
            print(f"{REQUIRED_CHECK} on {commit}: " + color.status("passed" if passed else "failed")
                  + color.paint(f" ({reason})", "dim"), flush=True)
            return 0 if passed else 1
        release_id, commit = publish(os.environ, args.dist)
        print(f"release {release_id}: " + color.paint("published", "pass") + color.paint(f" (tag on {commit})", "dim"),
              flush=True)
        return 0
    except (ReleaseError, OSError, ValueError, KeyError, tarfile.TarError) as error:
        print(color.paint(f"release: {error}", "fail", sys.stderr), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
