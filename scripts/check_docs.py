#!/usr/bin/env python3
"""Check English planning, reviewed Korean hashes, local links and work dependencies."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
# Numbering is explicit: design revision 1 added CSRG-C00 in group CSRG-P0.
UNITS = {"DG1": range(1, 13), "CSRG": range(0, 10), "P1R": range(1, 7), "DGL": range(1, 7),
         "DGC": range(1, 7), "DGA": range(1, 9)}
GROUPS = {"DG1": range(1, 7), "CSRG": range(0, 6), "P1R": range(1, 4), "DGL": range(1, 4),
          "DGC": range(1, 4), "DGA": range(1, 5)}
FIELDS = ("Owner / proposed PR", "Problem → behavior", "Prerequisites", "Modules / deliverables",
          "Invariants", "Tests (normal / failure / race)", "Completion evidence", "Rollback",
          "Handoff", "Verification command")
WORK_ID = r"(?:DG1|CSRG|P1R|DGL|DGC|DGA)-C\d{2}"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def acyclic(graph):
    visiting, done = set(), set()

    def visit(node):
        if node in visiting:
            raise ValueError("dependency cycle at " + node)
        if node in done:
            return
        if node not in graph:
            raise ValueError("undefined prerequisite: " + node)
        visiting.add(node)
        for dependency in graph[node]:
            visit(dependency)
        visiting.remove(node)
        done.add(node)

    for node in graph:
        visit(node)


def local_path(root, name):
    path = (root / name).resolve()
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError("missing or external document: " + str(name))
    return path


def registry(root):
    data = json.loads((root / "docs/translations.json").read_text())
    if (data.get("schema"), data.get("source_language"), data.get("translation_language")) != (
            "devguard-translations/v1", "en", "ko"):
        raise ValueError("unsupported translation registry")
    ids, sources, translations = set(), set(), set()
    for pair in data["pairs"]:
        for field, seen in [("id", ids), ("source", sources), ("translation", translations)]:
            if pair[field] in seen:
                raise ValueError("duplicate translation " + field + ": " + pair[field])
            seen.add(pair[field])
        local_path(root, pair["source"])
        local_path(root, pair["translation"])
    expected = {p.relative_to(root).as_posix() for p in (root / "docs/planning").rglob("*.md")}
    expected.add("docs/design.md")
    expected.update({"docs/contracts.md", "docs/operations.md"})
    if not expected <= sources:
        raise ValueError("unpaired authoritative documents: " + str(sorted(expected - sources)))
    return data


def check_translations(root):
    data = registry(root)
    for pair in data["pairs"]:
        for field in ("source", "translation"):
            if digest(root / pair[field]) != pair["reviewed_" + field + "_sha256"]:
                raise ValueError("unreviewed " + field + ": " + pair["id"])
        source = (root / pair["source"]).read_text()
        if re.search(r"[가-힣]", source.replace("[한국어](", "[Korean](")):
            raise ValueError("authoritative prose must be English: " + pair["source"])
        source_ids = re.findall(r"^### (" + WORK_ID + r")\b", source, re.M)
        translated_ids = re.findall(r"^### (" + WORK_ID + r")\b",
                                    (root / pair["translation"]).read_text(), re.M)
        if source_ids != translated_ids:
            raise ValueError("translated work definitions differ: " + pair["id"])
    return len(data["pairs"])


def check_planning(root):
    graph, memberships, assignments = {}, set(), {}
    for path in sorted((root / "docs/planning/milestones").glob("*.md")):
        text = path.read_text()
        # PR tables provide the unique authoritative group membership.
        for group, contents in re.findall(r"^\| ((?:DG1|CSRG|P1R|DGL|DGC|DGA)-P\d+) \| ([^|]+)\|", text, re.M):
            if group in memberships:
                raise ValueError("duplicate PR group: " + group)
            memberships.add(group)
            for work in re.findall(WORK_ID, contents):
                if work in assignments:
                    raise ValueError("work assigned twice: " + work)
                assignments[work] = group
        for work, section in re.findall(r"^### (" + WORK_ID + r")\b[^\n]*\n(.*?)(?=^### |\Z)", text, re.M | re.S):
            if work in graph:
                raise ValueError("duplicate work definition: " + work)
            for field in FIELDS:
                if not re.search(r"^- " + re.escape(field) + r": \S", section, re.M):
                    raise ValueError(work + " missing field: " + field)
            owner = re.search(r"^- Owner / proposed PR: (.+)$", section, re.M).group(1)
            if work not in assignments or assignments[work] not in owner:
                raise ValueError("PR membership mismatch: " + work)
            prerequisites = re.search(r"^- Prerequisites: ([^.]+)", section, re.M).group(1)
            graph[work] = re.findall(WORK_ID, prerequisites)
    expected = {f"{prefix}-C{i:02}" for prefix, numbers in UNITS.items() for i in numbers}
    expected_groups = {f"{prefix}-P{i}" for prefix, numbers in GROUPS.items() for i in numbers}
    if set(graph) != expected or set(assignments) != expected or memberships != expected_groups:
        raise ValueError(f"expected {len(expected)} work definitions/assignments and "
                         f"{len(expected_groups)} logical groups")
    acyclic(graph)
    ledger = json.loads((root / "milestones.json").read_text())
    milestone_graph = {}
    for item in ledger["milestones"]:
        if item["id"] in milestone_graph:
            raise ValueError("duplicate milestone: " + item["id"])
        milestone_graph[item["id"]] = item["requires"]
        local_path(root, item["planning_document"])
    acyclic(milestone_graph)
    local_path(root, ledger["design"])
    return {"work_units": len(graph), "logical_pr_groups": len(memberships)}


def check_links(root):
    files = [root / "README.md", root / "AGENTS.md", *sorted((root / "docs").rglob("*.md"))]
    for path in files:
        # The approved historical artifact is immutable; its references are historical too.
        if path == root / "docs/design.ko.md":
            continue
        text = re.sub(r"```.*?```", "", path.read_text(), flags=re.S)
        for target in re.findall(r"\]\(([^)\s]+)\)", text):
            if re.match(r"[a-z]+:", target) or target.startswith("#"):
                continue
            relative = unquote(target.split("#", 1)[0].strip("<>"))
            if relative:
                resolved = path.parent / relative
                if not resolved.exists() or not resolved.resolve().is_relative_to(root.resolve()):
                    raise ValueError(f"broken local link: {path.relative_to(root)} -> {target}")


def check(root=ROOT):
    pairs = check_translations(root)
    counts = check_planning(root)
    check_links(root)
    return {"status": "passed", "reviewed_pairs": pairs, **counts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command")
    record = sub.add_parser("record", help="Record hashes only after reviewing both documents")
    record.add_argument("--id", required=True)
    args = parser.parse_args()
    try:
        if args.command == "record":
            data = registry(ROOT)
            pairs = [pair for pair in data["pairs"] if pair["id"] == args.id]
            if len(pairs) != 1:
                raise ValueError("unknown translation pair: " + args.id)
            pair = pairs[0]
            for field in ("source", "translation"):
                pair["reviewed_" + field + "_sha256"] = digest(ROOT / pair[field])
            (ROOT / "docs/translations.json").write_text(json.dumps(data, indent=2) + "\n")
            print("Recorded reviewed pair: " + args.id)
        else:
            print(json.dumps(check()))
    except (ValueError, KeyError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
