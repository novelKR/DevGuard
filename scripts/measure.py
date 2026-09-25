#!/usr/bin/env python3
"""The macOS SLO protocol of DG1-C12: control and foreground responsiveness of the installed
service's release under bounded development load.

This is a measurement, not a functional test. Only the approved target host, in a window its
operator has set aside, can qualify a release. A rehearsal or a headless fixture is always
inconclusive. Promotion recomputes the verdict from the preserved reports and writes a
qualification record only for a run whose every repetition passed, on the measured policy and host,
while the service still runs the measured release.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import select
import shutil
import signal
import sqlite3
import subprocess
import sys
import threading
import time
import traceback

from validate import source_fingerprint

ROOT = Path(__file__).resolve().parents[1]
QUALIFY_SOURCE = ROOT / "crates/qualify"
FIXTURE = QUALIFY_SOURCE / "fixture/foreground.html"
CHROME_APP = Path("/Applications/Google Chrome.app")
CHROME = CHROME_APP / "Contents/MacOS/Google Chrome"
SUPPORT = Path.home() / "Library/Application Support/DevGuard"
SERVICE_LOG = Path.home() / "Library/Logs/DevGuard/devguardd.log"
HOST_CONFIG = Path.home() / ".config/devguard/host.toml"
PINNED_RUST = "1.95.0"
# The approved local target (docs/planning/milestones/DG-1.md, DG1-C12).
APPROVED_TARGET = {"logical_cpus": "8", "memory_bytes": str(16 * 1024 ** 3)}

# The approved acceptance targets (docs/planning/verification.md, "SLO protocol").
STATUS_P99_MS = 500
TERMINATION_P99_MS = 1000
INPUT_P99_MS = 100
INPUT_LIMIT_MS = 1000
FRAME_STALL_MS = 500

PROTOCOL = {"idle_s": 600, "load_s": 1800, "repetitions": 3, "combinations": ["cold", "warm"]}
REHEARSAL = {"idle_s": 30, "load_s": 60, "repetitions": 1, "combinations": ["cold", "warm"]}

INPUT_PERIOD_S = 0.5
INPUT_JITTER_S = 0.1
DRAIN_PERIOD_S = 5.0
VALIDITY_PERIOD_S = 1.0
HOST_PERIOD_S = 5.0
SERVICE_PERIOD_S = 60.0
SETTLE_S = 10.0
CDP_TIMEOUT_S = 10.0
STATUS_INTERVAL_S = 1.0
TERMINATE_PERIOD_S = 20.0
# Fewer samples than this share of an interval's schedule, validity rows included, is not a
# complete observation. Slots missed because an earlier call was still in flight are samples.
MIN_SAMPLES = 0.9
# Terminations whose targets were refused admission for capacity or pressure may lower this share.
MIN_TERMINATIONS = 0.8
# Admitted load must run for at least this share of the load interval, and each consumer for at
# least its own share, with at least one successful run.
MIN_LOAD = 0.5
MIN_CONSUMER = 0.25
# Work started before the load deadline must end within this limit after it.
LOAD_COMPLETION_LIMIT_S = 1800
# How long the harness's own attempts may take to be released after an interval.
JOURNAL_SETTLE_S = 60
# A run that ends this quickly without success is followed by a pause, never a tight loop.
QUICK_FAILURE_S = 2.0
FAILURE_PAUSE_S = 5.0
PROBE_FINISH_S = 420
TRANSPORT = "resource_control_unavailable"
EXIT_CRASH = 3
# One Cargo job: 1 CPU and 2 GiB by the adapter's estimate.
CARGO_BUDGET = ["--cpu", "1000", "--memory", "2GiB", "--tasks", "24"]
# Cargo targets under a ".noindex" directory, which Spotlight leaves alone.
TARGETS = "targets.noindex"
# Build variables recorded for provenance; a compiler wrapper is removed so cold builds stay cold.
BUILD_VARIABLES = ("RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "RUSTFLAGS", "CARGO_HOME", "RUSTUP_TOOLCHAIN")

# Processes the harness started and must settle if it ends early.
ABORT = threading.Event()
LIVE = set()
LIVE_LOCK = threading.Lock()


# ---------------------------------------------------------------- analysis (pure, tested)

def nearest_rank(values, fraction, missing=0):
    """Nearest-rank percentile of raw values. A missing sample ranks above every value."""
    count = len(values) + missing
    if count == 0:
        return None
    ordered = sorted(values) + [math.inf] * missing
    return ordered[max(1, math.ceil(fraction * count)) - 1]


def number(value):
    """A JSON-safe metric: a missing sample (infinite) is reported as the string "missing"."""
    if value is None:
        return None
    return "missing" if math.isinf(value) else round(value, 3)


def worst(values, missing):
    return number(math.inf if missing else max(values, default=None))


def input_metrics(inputs, timing, dispatched, skipped=0):
    """Input to next paint of every scheduled input. The page's estimate is raised to the
    browser's own Event Timing duration when it reported one. A scheduled input that was skipped
    because an earlier dispatch was still in flight, not handled, or handled without a paint, is
    missing, and a missing input also counts as a response over one second."""
    reported = {}
    for entry in timing:
        key = (entry["name"], round(entry["start"], 1))
        reported[key] = max(reported.get(key, 0.0), entry["duration"])
    latencies, matched = [], 0
    for record in inputs:
        if record.get("paint") is None:
            continue
        duration = reported.get((record["type"], round(record["stamp"], 1)))
        matched += duration is not None
        latencies.append(max(record["paint"] - record["stamp"], duration or 0.0))
    scheduled = dispatched + skipped
    missing = max(0, scheduled - len(latencies))
    p99 = nearest_rank(latencies, 0.99, missing)
    over = sum(1 for value in latencies if value > INPUT_LIMIT_MS) + missing
    return {"scheduled": scheduled, "dispatched": dispatched, "skipped_slots": skipped, "handled": len(inputs),
            "painted": len(latencies), "event_timing_matched": matched, "missing": missing,
            "p99_ms": number(p99), "max_ms": worst(latencies, missing), "over_limit": over,
            # None: nothing was scheduled, which is a shortfall, not a failure.
            "passed": None if p99 is None else (p99 <= INPUT_P99_MS and over == 0)}


def frame_metrics(frames):
    frames = sorted(frames)
    gaps = [later - earlier for earlier, later in zip(frames, frames[1:])]
    stalls = sum(1 for gap in gaps if gap > FRAME_STALL_MS)
    return {"frames": len(frames), "max_gap_ms": number(max(gaps, default=None)), "stalls": stalls,
            "span_ms": number(frames[-1] - frames[0]) if len(frames) > 1 else 0,
            "passed": None if len(frames) < 2 else stalls == 0}


def status_metrics(samples, expected):
    """Status of the probe's running target. An answer about a target that no longer runs, an
    error and a missed slot are all missing samples; a transport error is a connection loss."""
    answered = [s["latency_ms"] for s in samples
                if s["outcome"] == "answered" and s.get("phase") == "run_authorized"]
    not_running = sum(1 for s in samples if s["outcome"] == "answered" and s.get("phase") != "run_authorized")
    errors = [s for s in samples if s["outcome"] == "error"]
    missed = sum(1 for s in samples if s["outcome"] == "missed_slot")
    losses = sum(1 for s in errors if s.get("code") == TRANSPORT)
    missing = not_running + len(errors) + missed
    p99 = nearest_rank(answered, 0.99, missing)
    return {"samples": len(answered) + missing, "expected": expected, "answered": len(answered),
            "not_running": not_running, "errors": len(errors), "missed_slots": missed,
            "connection_losses": losses, "p99_ms": number(p99), "max_ms": worst(answered, missing),
            "passed": None if p99 is None else (p99 <= STATUS_P99_MS and losses == 0)}


def effective(sample):
    """An acknowledged termination whose target the service signalled completely and which then
    exited by itself, not by the probe's own kill."""
    return (sample["outcome"] == "acknowledged" and (sample.get("signalled") or 0) >= 1
            and bool(sample.get("complete")) and sample.get("exited_ms") is not None
            and not sample.get("killed_by_probe"))


def termination_metrics(samples, expected):
    """Termination acknowledgement of fresh targets. An acknowledgement that did not end its
    target fails outright; an error or a missed slot is a missing sample, and a transport error a
    connection loss. A target refused admission for capacity or pressure is not a sample. Exit and
    release are reported separately and only for effective terminations."""
    good = [s for s in samples if effective(s)]
    ineffective = sum(1 for s in samples if s["outcome"] == "acknowledged" and not effective(s))
    errors = [s for s in samples if s["outcome"] == "error"]
    missed = sum(1 for s in samples if s["outcome"] == "missed_slot")
    unstarted = sum(1 for s in samples if s["outcome"] == "not_started")
    unsampled = sum(1 for s in samples if s["outcome"] == "not_sampled")
    losses = sum(1 for s in errors if s.get("code") == TRANSPORT)
    missing = ineffective + len(errors) + missed
    acks = [s["ack_ms"] for s in good]
    exits = [s["exited_ms"] for s in good]
    released = [s["released_ms"] for s in good if s.get("released_ms") is not None]
    unreleased = len(good) - len(released)
    p99 = nearest_rank(acks, 0.99, missing)
    return {"samples": len(good) + missing, "expected": expected, "effective": len(good),
            "ineffective": ineffective, "errors": len(errors), "missed_slots": missed, "not_started": unstarted,
            "not_sampled": unsampled,
            "connection_losses": losses, "ack_p99_ms": number(p99), "ack_max_ms": worst(acks, missing),
            "exit_p99_ms": number(nearest_rank(exits, 0.99)), "exit_max_ms": number(max(exits, default=None)),
            "release_p99_ms": number(nearest_rank(released, 0.99, unreleased)),
            "release_max_ms": worst(released, unreleased), "unreleased": unreleased,
            # A refused or ineffective termination fails even without a latency sample; with neither a
            # sample nor a failure, nothing was measured, which is a shortfall.
            "passed": (False if losses or ineffective or unreleased
                       else None if p99 is None else p99 <= TERMINATION_P99_MS)}


def covered(spans, started, ended):
    """The share of [started, ended] covered by the union of spans."""
    total, cursor = 0.0, started
    for begin, end in sorted((max(b, started), min(e, ended)) for b, e in spans):
        begin = max(begin, cursor)
        if end > begin:
            total += end - begin
            cursor = end
    return round(total / max(ended - started, 1e-9), 4)


def load_metrics(runs, started, ended, consumers=()):
    """What each consumer did, the share of the interval in which its admitted work ran, and what
    makes the load not the declared one (issues) or wrong (failures)."""
    entries = {name: [] for name in consumers}
    for run in runs:
        entries.setdefault(run["consumer"], []).append(run)
    report, issues = {}, []
    for name, own in sorted(entries.items()):
        ok = [r for r in own if r.get("result") == "completed" and r.get("exit_code") == 0]
        spans = [(r["running_from"], r["ended"]) for r in own if r.get("running_from") is not None]
        entry = {"runs": len(own), "succeeded": len(ok),
                 "nonzero_exit": sum(1 for r in own if r.get("result") == "completed" and r.get("exit_code") != 0),
                 "exec_failed": sum(1 for r in own if r.get("result") == "exec_failed"),
                 "not_started": sum(1 for r in own if r.get("result") == "not_started"),
                 "uncertain": sum(1 for r in own if r.get("result") == "uncertain"),
                 "unknown": sum(1 for r in own if r.get("result") not in
                                ("completed", "exec_failed", "not_started", "uncertain")),
                 "refusal_reasons": {}, "share": covered(spans, started, ended),
                 "seconds": [round(r["ended"] - r["started"], 3) for r in ok]}
        for r in own:
            if r.get("result") == "not_started":
                reason = r.get("reason") or "unknown"
                entry["refusal_reasons"][reason] = entry["refusal_reasons"].get(reason, 0) + 1
        report[name] = entry
        if not ok:
            issues.append(f"{name}: no successful run")
        if entry["share"] < MIN_CONSUMER:
            issues.append(f"{name}: ran for {entry['share']:.0%} of the interval")
        for field in ("nonzero_exit", "exec_failed", "unknown"):
            if entry[field]:
                issues.append(f"{name}: {entry[field]} {field.replace('_', ' ')}")
    applied = covered([(r["running_from"], r["ended"]) for r in runs if r.get("running_from") is not None],
                      started, ended)
    if applied < MIN_LOAD:
        issues.append(f"admitted load ran for {applied:.0%} of the interval")
    losses = sum(1 for r in runs if r.get("connection_loss"))
    uncertain = sum(entry["uncertain"] for entry in report.values())
    duplicates = sum(1 for r in runs if (r.get("launched_attempts") or 0) > 1)
    failures = []
    if losses:
        failures.append(f"{losses} load runs lost their connection")
    if uncertain:
        failures.append(f"{uncertain} load runs are uncertain")
    if duplicates:
        failures.append(f"{duplicates} load runs launched more than one attempt")
    return {"consumers": report, "applied_fraction": applied, "connection_losses": losses,
            "uncertain": uncertain, "duplicate_launches": duplicates, "issues": issues, "failures": failures}


def interval_verdict(kind, invalid, metrics):
    """inconclusive when the observation is invalid; otherwise fail when a target was missed,
    and inconclusive when too little was observed or the load was not the declared one."""
    if invalid:
        return "inconclusive", list(invalid)
    names = ("status", "termination", "input", "frames")
    failures = [name for name in names if metrics[name]["passed"] is False]
    if metrics.get("fixture", {}).get("crashed"):
        failures.append("the fixture browser crashed")
    if metrics.get("service", {}).get("failure"):
        failures.append(metrics["service"]["failure"])
    if metrics["termination"].get("status_target_effective") is False:
        failures.append("the status target's termination was not effective")
    if kind == "load":
        failures += metrics["load"]["failures"]
    if metrics["journal"].get("still_charged"):
        failures.append("harness attempts still charged after the interval")
    if metrics["journal"].get("duplicate_launches"):
        failures.append("a load run launched more than one attempt not proven unstarted")
    if failures:
        return "fail", failures
    shortfalls = [f"{name}: nothing was measured" for name in names if metrics[name]["passed"] is None]
    status, termination, inputs = metrics["status"], metrics["termination"], metrics["input"]
    if status["samples"] < MIN_SAMPLES * status["expected"]:
        shortfalls.append(f"status samples {status['samples']} of {status['expected']}")
    if termination["samples"] < MIN_TERMINATIONS * termination["expected"]:
        shortfalls.append(f"termination samples {termination['samples']} of {termination['expected']}")
    if inputs["scheduled"] < MIN_SAMPLES * metrics["input_expected"]:
        shortfalls.append(f"inputs scheduled {inputs['scheduled']} of {metrics['input_expected']}")
    if kind == "load":
        shortfalls += metrics["load"]["issues"]
    return ("inconclusive", shortfalls) if shortfalls else ("pass", [])


def repetition_verdict(idle, load):
    """A failing idle baseline is inconclusive, never a failure of the load."""
    if idle["verdict"] != "pass":
        return "inconclusive", [f"idle baseline {idle['verdict']}"] + idle["reasons"]
    if load["verdict"] != "pass":
        return load["verdict"], load["reasons"]
    return "pass", []


def combination_verdict(verdicts, required):
    """Qualified only when every required repetition passed; values are never pooled."""
    if required < 1 or not verdicts:
        return "inconclusive"
    if any(verdict == "fail" for verdict in verdicts):
        return "failed"
    if len(verdicts) >= required and all(verdict == "pass" for verdict in verdicts):
        return "qualified"
    return "inconclusive"


def overall_verdict(combinations, plan, run):
    """The run's verdict: qualified only for the full protocol on the approved target."""
    verdicts = [entry["verdict"] for entry in combinations.values()]
    full = (not run["rehearsal"] and not run["headless"] and run["approved_target"]
            and set(plan["combinations"]) >= set(PROTOCOL["combinations"])
            and plan["repetitions"] >= PROTOCOL["repetitions"]
            and plan["idle_s"] >= PROTOCOL["idle_s"] and plan["load_s"] >= PROTOCOL["load_s"])
    if "failed" in verdicts:
        return "failed"
    if full and verdicts and all(verdict == "qualified" for verdict in verdicts):
        return "qualified"
    return "inconclusive"


def validity_reasons(rows, headless, expected):
    """The host-side conditions, at every sample: the fixture frontmost and the screen unlocked."""
    reasons = []
    if len(rows) < MIN_SAMPLES * expected:
        reasons.append(f"validity samples {len(rows)} of {expected}")
    if any(row.get("error") for row in rows):
        reasons.append("a validity sample could not be taken")
    if not headless and any(row.get("front_pid") != row.get("fixture_pid") for row in rows):
        reasons.append("the fixture was not the frontmost application")
    if any(row.get("locked") for row in rows):
        reasons.append("the screen was locked")
    return reasons


def service_findings(rows, release_id, service_pid):
    """(invalid, failure) from the service checks. Another release selected, or a check that could
    not be made, is invalid observation; the measured release restarting or unhealthy is a failure."""
    invalid = []
    if not rows:
        invalid.append("the service was not observed")
    if any(row.get("error") for row in rows):
        invalid.append("a service check could not be made")
    if any(row.get("current") not in (None, release_id) for row in rows):
        invalid.append("another release was selected during the interval")
    ours = [row for row in rows if not row.get("error") and row.get("current") in (None, release_id)]
    failure = None
    if any(not row.get("runs_release") or row.get("service_pid") != service_pid for row in ours):
        failure = "the measured release restarted or was unhealthy during the interval"
    return invalid, failure


def summarize_run(consumer, receipt, code, started, ended, output_bytes):
    """One load run from its receipt. A missing or unreadable receipt leaves the run unknown."""
    run = {"consumer": consumer, "started": started, "ended": ended, "exit_code": code,
           "output_bytes": output_bytes, "receipt": receipt.name}
    try:
        data = json.loads(receipt.read_text())
    except (OSError, ValueError):
        run["result"] = "unknown"
        return run
    result = data.get("result")
    notes = " ".join(entry.get("note", "") for entry in data.get("attempts") or [])
    reason = data.get("reason") or ""
    waited = (data.get("wait") or {}).get("waited_ms") or 0
    keys = {entry["key"]["attempt_id"] for entry in data.get("attempts") or [] if entry.get("key")}
    observed = (data.get("observed_after_reap") or {}).get("key")
    if observed:
        keys.add(observed["attempt_id"])
    exit_code = (data.get("exit") or {}).get("code")
    run.update(result=result, reason=reason or None, waited_ms=waited, attempt_ids=sorted(keys),
               exit_code=exit_code if exit_code is not None else code,
               exit_signal=(data.get("exit") or {}).get("signal"),
               connection_loss=result == "uncertain" or "ResourceControlUnavailable" in notes + reason,
               running_from=started + waited / 1000.0 if result in ("completed", "exec_failed") else None)
    return run


def verify_run(summary_path):
    """Recompute a run's verdict from its preserved reports and raw files, over every repetition the
    plan requires, whatever the summary lists. Returns (summary, run, reasons)."""
    out = summary_path.parent
    reasons = []
    summary = json.loads(summary_path.read_text())
    run = json.loads((out / "run.json").read_text())
    if sha256(out / "run.json") != summary.get("run_sha256"):
        reasons.append("the run header changed after the summary")
    if (run.get("harness") or {}).get("dirty"):
        reasons.append("the harness checkout had uncommitted changes")
    plan = run["plan"]
    listed = {combination: {entry["repetition"]: entry for entry in value.get("repetitions", [])}
              for combination, value in summary.get("combinations", {}).items()}
    combinations = {}
    for combination in plan["combinations"]:
        verdicts = []
        for repetition in range(1, plan["repetitions"] + 1):
            name = f"{combination}-{repetition}"
            entry = listed.get(combination, {}).get(repetition)
            path = out / name / "report.json"
            if entry is None or not path.exists() or sha256(path) != entry.get("report_sha256"):
                reasons.append(f"{name}: report missing, unlisted or changed")
                verdicts.append("inconclusive")
                continue
            report = json.loads(path.read_text())
            for interval in (report.get("intervals") or {}).values():
                for relative, digest in (interval.get("raw") or {}).items():
                    raw = out / name / relative
                    if not raw.is_file() or sha256(raw) != digest:
                        reasons.append(f"{name}: raw evidence missing or changed: {relative}")
            verdict = report.get("verdict")
            intervals = report.get("intervals") or {}
            if "idle" in intervals and "load" in intervals:
                recomputed, _ = repetition_verdict(intervals["idle"], intervals["load"])
                if verdict == "pass" and recomputed != "pass":
                    reasons.append(f"{name}: recorded pass does not recompute")
                    verdict = recomputed
            elif verdict == "pass":
                reasons.append(f"{name}: pass without intervals")
                verdict = "inconclusive"
            verdicts.append(verdict)
        combinations[combination] = {"verdict": combination_verdict(verdicts, plan["repetitions"])}
    verdict = overall_verdict(combinations, plan, run)
    if verdict != summary.get("verdict"):
        reasons.append(f"the summary says {summary.get('verdict')} but the reports give {verdict}")
    if verdict != "qualified":
        reasons.append(f"the run is {verdict}")
    return summary, run, reasons


def host_identity(environment):
    return {key: environment.get(key) for key in ("build", "cpu", "logical_cpus", "memory_bytes")}


def policy_identity(policy):
    doctor = policy.get("doctor") or {}
    return {"host_toml_sha256": policy.get("host_toml_sha256"),
            "configuration_fingerprint": doctor.get("configuration_fingerprint"),
            "capabilities": doctor.get("capabilities")}


# ---------------------------------------------------------------- host observation

def run_text(*args, timeout=10):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return result.stdout.strip() if result.returncode == 0 else None


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def tree_sha256(directory):
    digest = hashlib.sha256()
    for path in sorted(p for p in Path(directory).rglob("*") if p.is_file()):
        digest.update(str(path.relative_to(directory)).encode() + b"\0" + hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def frontmost_pid():
    """The process of the frontmost application, from LaunchServices."""
    front = run_text("lsappinfo", "front")
    if not front:
        return None
    info = run_text("lsappinfo", "info", "-only", "pid", front) or ""
    match = re.search(r'"?pid"?\s*=\s*(\d+)', info)
    return int(match.group(1)) if match else None


def screen_locked():
    text = run_text("ioreg", "-n", "Root", "-d1", "-k", "IOConsoleUsers")
    if text is None:
        raise RuntimeError("cannot read the console session")
    return "CGSSessionScreenIsLocked\"=Yes" in text


def environment():
    def sysctl(name):
        return run_text("sysctl", "-n", name)
    return {
        "platform": platform.platform(), "machine": platform.machine(), "macos": platform.mac_ver()[0],
        "build": run_text("sw_vers", "-buildVersion"), "kernel": platform.release(),
        "cpu": sysctl("machdep.cpu.brand_string"), "logical_cpus": sysctl("hw.logicalcpu"),
        "memory_bytes": sysctl("hw.memsize"), "boot_session": sysctl("kern.bootsessionuuid"),
        "power": (run_text("pmset", "-g", "batt") or "").splitlines()[:1],
        "thermal": run_text("pmset", "-g", "therm"),
        "displays": run_text("system_profiler", "SPDisplaysDataType", "-detailLevel", "mini", timeout=30),
        "chrome": run_text(str(CHROME), "--version") if CHROME.exists() else None,
        "python": platform.python_version(),
    }


class Sampler(threading.Thread):
    """Calls `sample` every `period` seconds until stopped, writing JSON lines to `path`. A sample
    that raises is recorded as an error row, never lost."""

    def __init__(self, path, period, sample):
        super().__init__(daemon=True)
        self.path, self.period, self.sample = path, period, sample
        self.halt = threading.Event()
        self.rows = []

    def run(self):
        with open(self.path, "a") as out:
            next_at = time.monotonic()
            while not self.halt.is_set():
                try:
                    row = {"unix": time.time(), **self.sample()}
                except Exception as error:  # every failed sample is kept as evidence
                    row = {"unix": time.time(), "error": f"{type(error).__name__}: {error}"}
                self.rows.append(row)
                out.write(json.dumps(row) + "\n")
                out.flush()
                next_at += self.period
                self.halt.wait(max(0.0, next_at - time.monotonic()))

    def stop(self):
        self.halt.set()
        self.join()
        return self.rows


def host_sample():
    swap = run_text("sysctl", "-n", "vm.swapusage") or ""
    used = re.search(r"used = ([\d.]+)M", swap)
    pageouts = re.search(r"Pageouts:\s+(\d+)", run_text("vm_stat") or "")
    return {"pressure_level": run_text("sysctl", "-n", "kern.memorystatus_vm_pressure_level"),
            "swap_used_mib": float(used.group(1)) if used else None,
            "pageouts": int(pageouts.group(1)) if pageouts else None,
            "load_average": run_text("sysctl", "-n", "vm.loadavg")}


def validity_sample(fixture_pid):
    def sample():
        return {"front_pid": frontmost_pid(), "fixture_pid": fixture_pid, "locked": screen_locked()}
    return sample


def host_peaks(rows):
    levels = {}
    for row in rows:
        levels[row.get("pressure_level")] = levels.get(row.get("pressure_level"), 0) + 1
    swap = [row["swap_used_mib"] for row in rows if row.get("swap_used_mib") is not None]
    pageouts = [row["pageouts"] for row in rows if row.get("pageouts") is not None]
    return {"samples": len(rows), "pressure_levels": levels, "swap_used_mib_max": max(swap, default=None),
            "pageouts_delta": (pageouts[-1] - pageouts[0]) if len(pageouts) > 1 else None}


# ---------------------------------------------------------------- the foreground fixture

class Fixture:
    """Chrome with a fresh profile, driven over --remote-debugging-pipe: private descriptors, no
    listening port, and no flag that changes scheduling or throttling. One instance serves a whole
    run; each repetition reloads the page."""

    def __init__(self, profile, headless):
        self.headless = headless
        to_chrome, self.writer = os.pipe()
        self.reader, from_chrome = os.pipe()

        def descriptors():
            os.dup2(to_chrome, 3)
            os.dup2(from_chrome, 4)

        args = [str(CHROME), f"--user-data-dir={profile}", "--no-first-run", "--no-default-browser-check",
                "--remote-debugging-pipe", "about:blank"]
        if headless:
            args.insert(1, "--headless=new")
        self.process = track(subprocess.Popen(args, pass_fds=(3, 4), preexec_fn=descriptors, close_fds=True,
                                              stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                              stderr=subprocess.DEVNULL))
        os.close(to_chrome)
        os.close(from_chrome)
        self.buffer = b""
        self.sequence = 0
        self.session = None
        self.pending = set()
        self.late = {}
        self.drain_ids = set()
        self.unreadable_replies = 0
        self.crashed = False
        self.crashed_at = None
        self.version = None

    def send(self, method, params=None, session=True):
        self.sequence += 1
        message = {"id": self.sequence, "method": method, "params": params or {}}
        if session and self.session:
            message["sessionId"] = self.session
        os.write(self.writer, json.dumps(message).encode() + b"\0")
        return self.sequence

    def reply(self, wanted, method, timeout=CDP_TIMEOUT_S):
        """The reply to request `wanted`. A reply that arrives after its request timed out is kept
        in `late`, so what it carries is not lost."""
        if wanted in self.late:
            return self.result(self.late.pop(wanted), method)
        deadline = time.monotonic() + timeout
        while True:
            while b"\0" in self.buffer:
                raw, self.buffer = self.buffer.split(b"\0", 1)
                message = json.loads(raw)
                if message.get("id") == wanted:
                    return self.result(message, method)
                if message.get("id") in self.pending:
                    self.pending.discard(message["id"])
                    self.late[message["id"]] = message
                elif message.get("method") in ("Inspector.targetCrashed", "Target.targetCrashed"):
                    self.mark_crashed()
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                self.pending.add(wanted)
                raise TimeoutError(method)
            ready, _, _ = select.select([self.reader], [], [], remaining)
            if ready:
                chunk = os.read(self.reader, 1 << 20)
                if not chunk:
                    self.mark_crashed()
                    raise ConnectionError("the fixture browser closed its pipe")
                self.buffer += chunk

    def mark_crashed(self):
        if not self.crashed:
            self.crashed, self.crashed_at = True, time.time()

    @staticmethod
    def result(message, method):
        if "error" in message:
            raise RuntimeError(f"{method}: {message['error'].get('message')}")
        return message.get("result", {})

    def call(self, method, params=None, session=True, timeout=CDP_TIMEOUT_S):
        return self.reply(self.send(method, params, session), method, timeout)

    def open(self, page):
        self.page = page
        self.version = self.call("Browser.getVersion", session=False)["product"]
        target = self.call("Target.createTarget", {"url": page.as_uri()}, session=False)["targetId"]
        self.session = self.call("Target.attachToTarget", {"targetId": target, "flatten": True},
                                 session=False)["sessionId"]
        for domain in ("Runtime", "Page", "Inspector"):
            self.call(f"{domain}.enable")
        self.ready()

    def ready(self, limit=30):
        """Wait for the current document to load; the marker an earlier document carried is gone."""
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline:
            try:
                if self.evaluate("document.readyState === 'complete' && typeof drain === 'function' "
                                 "&& window.__devguardReloaded === undefined"):
                    self.targets = json.loads(self.evaluate("targets()"))
                    return
            except (RuntimeError, TimeoutError):
                pass
            time.sleep(0.2)
        raise RuntimeError("the fixture page did not load")

    def reload(self):
        """A fresh page for a new repetition in the same browser, which keeps the front."""
        self.evaluate("window.__devguardReloaded = true")
        self.call("Page.reload", {"ignoreCache": True})
        self.ready()
        self.reset_replies()

    def activate(self):
        """Bring this Chrome to the front through LaunchServices. No other Chrome runs."""
        if not self.headless:
            subprocess.run(["open", "-a", str(CHROME_APP)], capture_output=True, timeout=30)
            self.call("Page.bringToFront")

    @staticmethod
    def value(result):
        if "exceptionDetails" in result:
            raise RuntimeError(f"fixture page error: {result['exceptionDetails'].get('text')}")
        return result["result"]["value"]

    def evaluate(self, expression):
        return self.value(self.call("Runtime.evaluate", {"expression": expression, "returnByValue": True}))

    def late_drains(self):
        """Drain replies that arrived after their request timed out; other late replies carry
        nothing the harness needs, and an error reply carries no data."""
        drained = []
        for key in sorted(self.late):
            message = self.late.pop(key)
            if key in self.drain_ids and "error" not in message:
                self.drain_ids.discard(key)
                try:
                    drained.append(json.loads(self.value(message.get("result", {}))))
                except (RuntimeError, ValueError, KeyError):
                    self.unreadable_replies += 1
        return drained

    def reset_replies(self):
        """Forget replies still owed from an earlier interval or document. Returns how many."""
        owed = len(self.pending) + len(self.late)
        self.pending.clear()
        self.late.clear()
        self.drain_ids.clear()
        return owed

    def pending_drains(self):
        return bool(self.pending & self.drain_ids)

    def drain(self):
        wanted = self.send("Runtime.evaluate", {"expression": "drain()", "returnByValue": True})
        self.drain_ids.add(wanted)
        result = self.reply(wanted, "drain")
        self.drain_ids.discard(wanted)
        return json.loads(self.value(result))

    def mouse(self, kind, target, **extra):
        point = self.targets[target]
        self.call("Input.dispatchMouseEvent", {"type": kind, "x": point["x"], "y": point["y"], **extra})

    def prime(self):
        """Focus the text field once; this click is not a sample."""
        for kind in ("mousePressed", "mouseReleased"):
            self.mouse(kind, "field", button="left", clickCount=1)

    def send_input(self, index):
        kind = index % 3
        if kind == 0:
            letter = chr(ord("a") + index % 26)
            code = {"code": "Key" + letter.upper(), "windowsVirtualKeyCode": ord(letter.upper())}
            self.call("Input.dispatchKeyEvent", {"type": "keyDown", "key": letter, "text": letter, **code})
            self.call("Input.dispatchKeyEvent", {"type": "keyUp", "key": letter, **code})
        elif kind == 1:
            self.mouse("mousePressed", "button", button="left", clickCount=1)
            self.mouse("mouseReleased", "button", button="left", clickCount=1)
        else:
            self.mouse("mouseWheel", "scroller", deltaX=0, deltaY=120 if index % 2 else -120)

    def close(self):
        try:
            self.call("Browser.close", session=False, timeout=5)
        except (OSError, RuntimeError, ConnectionError):
            pass
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        untrack(self.process)
        for descriptor in (self.reader, self.writer):
            try:
                os.close(descriptor)
            except OSError:
                pass


class Foreground(threading.Thread):
    """Sends one input per scheduled slot (INPUT_PERIOD_S ± INPUT_JITTER_S) and drains the page
    every DRAIN_PERIOD_S until stopped. A slot that passes while an earlier dispatch is still in
    flight is counted as skipped, which is a missing sample. A drain that times out keeps its reply
    for later, so no data is lost; a closed browser or a crashed page is a crash, a failure."""

    def __init__(self, fixture, raw, seed):
        super().__init__(daemon=True)
        self.fixture, self.raw = fixture, raw
        self.random = random.Random(seed)
        self.halt = threading.Event()
        self.dispatched = 0
        self.skipped = 0
        self.dispatch_timeouts = 0
        self.drain_timeouts = 0
        self.frames, self.inputs, self.timing, self.page_events, self.page_states = [], [], [], [], []
        self.handled_at_start = None
        self.handled = None
        self.error = None
        self.discarded_replies = 0

    def absorb(self, drained):
        with open(self.raw / "fixture.jsonl", "a") as out:
            out.write(json.dumps({"unix": time.time(), **drained}) + "\n")
        self.frames += drained["frames"]
        self.inputs += drained["inputs"]
        self.timing += drained["timing"]
        self.page_events += drained["validity"]
        self.page_states.append((drained["visible"], drained["focus"]))
        handled = drained["handled"]
        if self.handled is None or sum(handled.values()) >= sum(self.handled.values()):
            self.handled = handled

    def collect(self):
        for drained in self.fixture.late_drains():
            self.absorb(drained)
        try:
            self.absorb(self.fixture.drain())
        except TimeoutError:
            self.drain_timeouts += 1

    def run(self):
        self.discarded_replies = self.fixture.reset_replies()
        try:
            self.handled_at_start = self.fixture.drain()["handled"]
            next_input = time.monotonic()
            next_drain = next_input + DRAIN_PERIOD_S
            index = 0
            while not self.halt.is_set():
                now = time.monotonic()
                if now >= next_input:
                    self.dispatched += 1
                    try:
                        self.fixture.send_input(index)
                    except TimeoutError:
                        self.dispatch_timeouts += 1
                    index += 1
                    next_input += INPUT_PERIOD_S + self.random.uniform(-INPUT_JITTER_S, INPUT_JITTER_S)
                    # Slots that passed during the dispatch are missing samples, not a burst.
                    while next_input + INPUT_PERIOD_S <= time.monotonic():
                        next_input += INPUT_PERIOD_S
                        self.skipped += 1
                    next_input = max(next_input, time.monotonic())
                if now >= next_drain:
                    self.collect()
                    next_drain = time.monotonic() + DRAIN_PERIOD_S
                self.halt.wait(max(0.0, min(next_input, next_drain) - time.monotonic()))
            # Inputs sent at the end are reported once painted, or after the page's paint limit.
            time.sleep(5.5)
            self.collect()
            deadline = time.monotonic() + CDP_TIMEOUT_S
            while self.fixture.pending_drains() and time.monotonic() < deadline:
                time.sleep(0.5)
                self.collect()
            for drained in self.fixture.late_drains():
                self.absorb(drained)
        except ConnectionError as error:
            self.fixture.mark_crashed()
            self.error = str(error)
        except (OSError, RuntimeError, ValueError) as error:
            self.error = f"{type(error).__name__}: {error}"

    def stop(self):
        self.halt.set()
        self.join()

    def invalid(self, headless):
        """Reasons the page's own record makes the interval unobservable."""
        reasons = []
        if headless:
            reasons.append("headless fixture")
        if self.error and not self.fixture.crashed:
            reasons.append(f"fixture observation failed: {self.error}")
        if not self.page_states:
            reasons.append("no fixture sample was drained")
        if any(not (visible and focus) for visible, focus in self.page_states):
            reasons.append("fixture page not visible and focused")
        if self.page_events:
            kinds = sorted({event["event"] for event in self.page_events})
            reasons.append("visibility or focus changed: " + ", ".join(kinds))
        received = sum((self.handled or {}).values()) - sum((self.handled_at_start or {}).values())
        if received > self.dispatched:
            reasons.append("inputs the harness did not send")
        return reasons

    def metrics(self):
        return {"input": input_metrics(self.inputs, self.timing, self.dispatched, self.skipped),
                "frames": frame_metrics(self.frames),
                "fixture": {"dispatch_timeouts": self.dispatch_timeouts, "drain_timeouts": self.drain_timeouts,
                            "discarded_replies": self.discarded_replies,
                            "unreadable_replies": self.fixture.unreadable_replies,
                            "crashed": self.fixture.crashed, "error": self.error}}


# ---------------------------------------------------------------- the service, probe and load

def track(process):
    with LIVE_LOCK:
        LIVE.add(process)
    return process


def untrack(process):
    with LIVE_LOCK:
        LIVE.discard(process)


def spawn(args, **options):
    """Start and track a process unless the run is being settled; the check and the start are
    atomic with respect to settle_live."""
    with LIVE_LOCK:
        if ABORT.is_set():
            return None
        process = subprocess.Popen(args, **options)
        LIVE.add(process)
        return process


def settle_live():
    """Stop starting work and ask every live owner, probe and browser to stop: an owner forwards
    SIGTERM to its managed scope and settles it, so an ended run leaves nothing charged. Further
    interrupts are ignored until this is done."""
    for number in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(number, signal.SIG_IGN)
    ABORT.set()
    with LIVE_LOCK:
        processes = list(LIVE)
    for process in processes:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
    for process in processes:
        try:
            # A probe settles its targets within this time; an owner, its scope sooner.
            process.wait(timeout=PROBE_FINISH_S)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        untrack(process)


class Release:
    """The measured release as the service runs it: its own directory, or its recovery copy."""

    def __init__(self, release_id):
        self.id = release_id
        directory = SUPPORT / "releases" / release_id
        for candidate in (directory, SUPPORT / "recovery" / release_id):
            status = run_text(str(candidate / "bin/devguardd"), "status", timeout=60)
            if status is None:
                continue
            try:
                running = (json.loads(status).get("running") or {})
            except ValueError:
                continue
            executable = Path(running.get("executable") or "")
            if executable.name == "devguardd" and executable.parent.parent.name == release_id:
                directory = executable.parent.parent
            break
        self.directory = directory
        self.manifest = directory / "MANIFEST.json"
        self.devguardd = directory / "bin/devguardd"
        self.devguard = directory / "bin/devguard"
        self.helper = directory / "bin/devguard-launch"


def service_state(release):
    """The installed service, as the measured release's own devguardd reports it."""
    manifest = json.loads(release.manifest.read_text())
    text = run_text(str(release.devguardd), "status", timeout=60)
    status = json.loads(text) if text else None
    running = (status or {}).get("running") or {}
    runs = bool(status and status.get("healthy")
                and ((status.get("selection") or {}).get("current")) == release.id
                and running.get("sha256") == manifest["artifacts"]["devguardd"]["sha256"])
    return {"healthy": bool(status and status.get("healthy")),
            "current": ((status or {}).get("selection") or {}).get("current"),
            "running_sha256": running.get("sha256"), "service_pid": running.get("service_pid"),
            "executable": running.get("executable"), "runs_release": runs}


def doctor(release):
    text = run_text(str(release.devguard), "doctor", "--require", "admission,registration,macos-cooperative",
                    timeout=60)
    report = json.loads(text) if text else {}
    service = ((report.get("checks") or {}).get("service") or {}).get("detail") or {}
    return {"satisfied": report.get("satisfied"), "protocol": service.get("protocol"),
            "capabilities": service.get("capabilities"),
            "configuration_fingerprint": (service.get("status") or {}).get("configuration_fingerprint"),
            "authority_pid": (service.get("authority") or {}).get("pid")}


def journal_query(query, parameters=(), database=None):
    path = database or SUPPORT / "state/authority.sqlite"
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True, timeout=5)
    try:
        return connection.execute(query, parameters).fetchall()
    finally:
        connection.close()


def journal_counts():
    """Attempt phases and what stays charged, from the service's journal, read-only."""
    try:
        phases = journal_query("SELECT json_extract(record, '$.phase'), COUNT(*) FROM attempts GROUP BY 1")
        attempts = journal_query("SELECT COUNT(*) FROM attempts WHERE charged = 1")[0][0]
        leases = journal_query("SELECT COUNT(*) FROM leases WHERE charged = 1")[0][0]
    except sqlite3.Error as error:
        return {"error": str(error)}
    return {"phases": {str(phase): count for phase, count in phases},
            "charged_attempts": attempts, "charged_leases": leases}


def journal_attempts(ids, database=None):
    """For the harness's own attempt ids: whether each is charged and whether it was launched."""
    found = {}
    ids = sorted(ids)
    for start in range(0, len(ids), 500):
        chunk = ids[start:start + 500]
        marks = ",".join("?" * len(chunk))
        # A committed grant later released as never started (no helper was created) did not run.
        for attempt, charged, launched, phase in journal_query(
                f"SELECT attempt, charged, launch_hash IS NOT NULL AND NOT ("
                f"json_extract(record, '$.phase') = 'released' "
                f"AND json_extract(record, '$.release_reason') = 'no_helper_created'), "
                f"json_extract(record, '$.phase') "
                f"FROM attempts WHERE consumer = 'dev-cli' AND attempt IN ({marks})", chunk, database):
            found[attempt] = {"charged": bool(charged), "launched": bool(launched), "phase": phase}
    return found


def settle_journal(ids, runs, database=None, limit=JOURNAL_SETTLE_S):
    """Wait until none of the harness's attempts is charged, then check each receipt launched at
    most one of its attempts that was not proven never started."""
    deadline = time.monotonic() + limit
    try:
        while True:
            found = journal_attempts(ids, database)
            charged = sorted(attempt for attempt, row in found.items() if row["charged"])
            if not charged or time.monotonic() >= deadline:
                break
            time.sleep(1.0)
    except sqlite3.Error as error:
        return {"error": str(error)}
    duplicates = 0
    for run in runs:
        launched = sum(1 for attempt in run.get("attempt_ids") or [] if found.get(attempt, {}).get("launched"))
        run["launched_attempts"] = launched
        duplicates += launched > 1
    return {"harness_attempts": len(ids), "found": len(found), "still_charged": charged,
            "duplicate_launches": duplicates}


class Probe:
    """devguard-qualify control for one interval."""

    def __init__(self, binary, release, raw, name, seconds):
        self.path = raw / f"control-{name}.jsonl"
        self.process = spawn(
            [str(binary), "control", "--helper", str(release.helper), "--duration-s", str(int(seconds)),
             "--out", str(self.path), "--status-interval-ms", str(int(STATUS_INTERVAL_S * 1000)),
             "--terminate-period-s", str(int(TERMINATE_PERIOD_S))],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if self.process is None:
            raise KeyboardInterrupt

    def samples(self):
        rows, malformed = [], 0
        if self.path.exists():
            for line in self.path.read_text().splitlines():
                try:
                    rows.append(json.loads(line))
                except ValueError:
                    malformed += 1
        return rows, malformed

    def await_target(self, limit=180):
        """Wait until the probe's status target runs, or the probe gives up."""
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline and self.process.poll() is None and not ABORT.is_set():
            rows, _ = self.samples()
            if any((row.get("kind"), row.get("role"), row.get("outcome")) == ("admission", "status", "admitted")
                   for row in rows):
                # The helper reports READY right after; the first status call follows.
                time.sleep(0.5)
                return True
            time.sleep(0.2)
        return False

    def finish(self, stop=False):
        if stop and self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
        finished = True
        try:
            out, err = self.process.communicate(timeout=PROBE_FINISH_S)
        except subprocess.TimeoutExpired:
            finished = False
            self.process.send_signal(signal.SIGTERM)
            try:
                out, err = self.process.communicate(timeout=120)
            except subprocess.TimeoutExpired:
                self.process.kill()
                out, err = self.process.communicate()
        untrack(self.process)
        rows, malformed = self.samples()
        return {"returncode": self.process.returncode, "finished": finished, "summary": (out or "").strip(),
                "stderr": (err or "").strip()[-2000:], "samples": rows, "malformed_lines": malformed}


class Consumer(threading.Thread):
    """One load consumer: runs its command through `devguard exec` again and again until the
    deadline, never starting after it, and records every run and its receipt. A run that ends at
    once without success is followed by a pause."""

    def __init__(self, name, command, options, devguard, raw, deadline, env=None, cwd=None,
                 feed=None, sink=False, before=None):
        super().__init__(daemon=True)
        self.name, self.command, self.options = name, command, options
        self.devguard, self.raw, self.deadline = devguard, raw, deadline
        self.env, self.cwd, self.feed, self.sink, self.before = env, cwd, feed, sink, before
        self.runs = []
        self.errors = []

    def run(self):
        index = 0
        while time.monotonic() < self.deadline and not ABORT.is_set():
            index += 1
            receipt = self.raw / f"{self.name}-{index:03d}.receipt.json"
            try:
                context = self.before(index) if self.before else {}
            except (OSError, RuntimeError) as error:
                self.errors.append(str(error))
                return
            env = dict(os.environ if self.env is None else self.env, **context.get("env", {}))
            started = time.time()
            try:
                process = spawn([str(self.devguard), "exec", *self.options, "--receipt", str(receipt), "--",
                                 *self.command],
                                stdin=subprocess.PIPE if self.feed else subprocess.DEVNULL,
                                stdout=subprocess.PIPE if self.sink else subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, env=env, cwd=self.cwd)
                if process is None:
                    return
                drained = [0]
                reader = None
                if self.sink:
                    def read(stream=process.stdout):
                        for chunk in iter(lambda: stream.read(1 << 16), b""):
                            drained[0] += len(chunk)
                    reader = threading.Thread(target=read, daemon=True)
                    reader.start()
                if self.feed:
                    self.feed(process)
                code = process.wait()
                untrack(process)
                if reader:
                    reader.join()
            except OSError as error:
                self.errors.append(str(error))
                return
            ended = time.time()
            run = summarize_run(self.name, receipt, code, started, ended, drained[0])
            self.runs.append(run)
            if ended - started < QUICK_FAILURE_S and not (run.get("result") == "completed" and run.get("exit_code") == 0):
                ABORT.wait(FAILURE_PAUSE_S)


def cargo_environment():
    """The Cargo load's environment: this one, without a compiler wrapper."""
    env = dict(os.environ)
    for name in ("RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER"):
        env.pop(name, None)
    return env


def slow_feed(lines, period, marker="slow-reader"):
    """Feed `lines` lines, one per `period`, once the payload runs: lines are not queued in the pipe
    while the owner waits for admission."""
    def feed(process):
        deadline = time.monotonic() + LOAD_COMPLETION_LIMIT_S
        while process.poll() is None and time.monotonic() < deadline and not ABORT.is_set():
            if run_text("pgrep", "-P", str(process.pid), "-f", marker):
                break
            time.sleep(0.2)
        stream = process.stdin
        try:
            for index in range(lines):
                if process.poll() is not None:
                    break
                stream.write(f"slow input line {index}\n".encode())
                stream.flush()
                time.sleep(period)
        except BrokenPipeError:
            pass
        finally:
            try:
                stream.close()
            except BrokenPipeError:
                pass
    return feed


def consumers(args, release, combination, work, raw, deadline, repetition, rehearsal):
    """The six load consumers of the protocol, through the measured release's devguard."""
    qualify = str(args.qualify_bin)
    seconds = "20" if rehearsal else "120"
    env = cargo_environment()
    targets = work / TARGETS

    def cargo_before(index):
        if combination == "warm":
            return {"env": {"CARGO_TARGET_DIR": str(targets / "warm")}}
        target = targets / f"cold-{repetition}-{index:03d}"
        if target.exists():
            raise RuntimeError(f"a cold target directory already exists: {target}")
        return {"env": {"CARGO_TARGET_DIR": str(target)}}

    cargo = ["/bin/sh", "-c", "cargo build --offline --locked --workspace && "
             "cargo test --offline --locked -p devguard-core -p devguard-contract"]
    io_dir = work / "io"
    io_dir.mkdir(exist_ok=True)
    # Every budget together fits half the work capacity, the Constrained capacity, so memory
    # warning neither stops the load nor starves the control probe's targets.
    return [
        Consumer("cargo", cargo, ["--adapter", "cargo-pipeline", *CARGO_BUDGET, "--wait", "30m"], release.devguard,
                 raw, deadline, env=env, cwd=work / "source", before=cargo_before),
        Consumer("cpu", [qualify, "work", "cpu", "--threads", "1", "--seconds", seconds],
                 ["--cpu", "1000", "--memory", "128MiB", "--tasks", "4", "--wait", "30m"], release.devguard, raw,
                 deadline),
        Consumer("memory", [qualify, "work", "memory", "--mib", "1024", "--seconds", seconds],
                 ["--cpu", "100", "--memory", "1280MiB", "--tasks", "4", "--wait", "30m"], release.devguard, raw,
                 deadline),
        Consumer("io", [qualify, "work", "io", "--dir", str(io_dir), "--mib", "256", "--seconds", "30"],
                 ["--cpu", "100", "--memory", "384MiB", "--tasks", "4", "--wait", "30m"], release.devguard, raw,
                 deadline),
        Consumer("output", [qualify, "work", "output", "--mib", "256", "--seconds", "30"],
                 ["--cpu", "250", "--memory", "128MiB", "--tasks", "4", "--wait", "30m"], release.devguard, raw,
                 deadline, sink=True),
        Consumer("slow-input", [qualify, "work", "slow-reader"],
                 ["--cpu", "50", "--memory", "64MiB", "--tasks", "4", "--wait", "30m"], release.devguard, raw,
                 deadline, feed=slow_feed(200 if rehearsal else 600, 0.1)),
    ]


# ---------------------------------------------------------------- the protocol

def measure_interval(args, release, kind, repetition_raw, seconds, fixture, combination, work, repetition,
                     service_pid):
    """One idle or load interval: the fixture, the control probe, validity, service and host samples
    throughout, and for load the six consumers, observed until every started command completes."""
    raw = repetition_raw / kind
    raw.mkdir()
    journal_before = journal_counts()
    invalid = []
    if "error" in journal_before:
        invalid.append(f"the journal could not be read: {journal_before['error']}")
    elif journal_before["charged_attempts"] or journal_before["charged_leases"]:
        invalid.append("work other than the harness's was charged when the interval began")
    # The load probe samples until it is stopped after the load completes, which is bounded.
    probe = Probe(args.qualify_bin, release, raw, kind,
                  seconds + (LOAD_COMPLETION_LIMIT_S + 300 if kind == "load" else 0))
    # The probe's status target runs before any load competes with it for admission.
    probe.await_target()
    started = time.time()
    begun = time.monotonic()
    host = Sampler(raw / "host.jsonl", HOST_PERIOD_S, host_sample)
    validity = Sampler(raw / "validity.jsonl", VALIDITY_PERIOD_S, validity_sample(fixture.process.pid))
    service = Sampler(raw / "service.jsonl", SERVICE_PERIOD_S, lambda: service_state(release))
    foreground = Foreground(fixture, raw, args.seed_source.getrandbits(32))
    for thread in (host, validity, service, foreground):
        thread.start()
    load = []
    if kind == "load":
        (raw / "runs").mkdir()
        load = consumers(args, release, combination, work, raw / "runs", begun + seconds, repetition,
                         args.rehearsal)
        for consumer in load:
            consumer.start()
        limit = begun + seconds + LOAD_COMPLETION_LIMIT_S
        for consumer in load:
            consumer.join(timeout=max(0.0, limit - time.monotonic()))
        if any(consumer.is_alive() for consumer in load):
            invalid.append("the load did not complete within the limit")
            settle_consumers(load)
    else:
        ABORT.wait(max(0.0, seconds - (time.monotonic() - begun)))
    if ABORT.is_set():
        raise KeyboardInterrupt
    ended = time.time()
    elapsed = time.monotonic() - begun
    foreground.stop()
    validity_rows = validity.stop()
    service_rows = service.stop()
    host_rows = host.stop()
    control = probe.finish(stop=kind == "load")
    rows = control["samples"]
    status = [row for row in rows if row.get("kind") == "status"]
    terminations = [row for row in rows if row.get("kind") == "terminate"]
    runs = [run for consumer in load for run in consumer.runs]
    ids = {row["key"]["attempt_id"] for row in rows if isinstance(row.get("key"), dict)}
    ids |= {attempt for run in runs for attempt in run.get("attempt_ids") or []}
    journal = settle_journal(ids, runs)
    metrics = {
        **foreground.metrics(),
        "input_expected": int(elapsed / INPUT_PERIOD_S),
        "status": status_metrics(status, int(elapsed / STATUS_INTERVAL_S)),
        "termination": termination_metrics(terminations, max(1, int(elapsed / TERMINATE_PERIOD_S))),
        "probe": {key: control[key] for key in ("returncode", "finished", "summary", "stderr", "malformed_lines")},
        "journal": {"before": journal_before, "after": journal_counts(), **journal},
        "host": host_peaks(host_rows),
        "validity_rows": len(validity_rows),
    }
    if kind == "load":
        (raw / "runs.json").write_text(json.dumps(runs, indent=1) + "\n")
        metrics["load"] = load_metrics(runs, started, ended, [consumer.name for consumer in load])
        metrics["load"]["consumer_errors"] = {consumer.name: consumer.errors for consumer in load if consumer.errors}
        if metrics["load"]["consumer_errors"]:
            metrics["load"]["issues"].append("a consumer stopped with an error")
    invalid += foreground.invalid(fixture.headless)
    if fixture.crashed and fixture.crashed_at:
        # A crashed browser leaves the front; that consequence is the crash's failure, not invalid
        # observation. Validity counts until the crash.
        observed = [row for row in validity_rows if row["unix"] < fixture.crashed_at]
        expected = int(max(0.0, fixture.crashed_at - started) / VALIDITY_PERIOD_S)
        invalid += validity_reasons(observed, fixture.headless, expected)
    else:
        invalid += validity_reasons(validity_rows, fixture.headless, int(elapsed / VALIDITY_PERIOD_S))
    service_invalid, service_failure = service_findings(service_rows, release.id, service_pid)
    invalid += service_invalid
    metrics["service"] = {"rows": len(service_rows), "failure": service_failure}
    settled = [row for row in rows if row.get("kind") == "status_target" and row.get("outcome") == "settled"]
    if settled:
        row = settled[-1]
        metrics["termination"]["status_target_effective"] = bool(
            row.get("terminated") and row.get("exited_ms") is not None and not row.get("killed_by_probe"))
    if "error" in journal:
        invalid.append(f"the journal could not be read: {journal['error']}")
    if not control["finished"] or control["returncode"] != 0:
        invalid.append(f"the control probe exited {control['returncode']}: {control['stderr'][-300:]}")
    if control["malformed_lines"]:
        invalid.append(f"{control['malformed_lines']} probe samples were unreadable")
    verdict, reasons = interval_verdict(kind, invalid, metrics)
    return {"kind": kind, "started_unix": started, "ended_unix": ended, "seconds": round(elapsed, 3),
            "verdict": verdict, "reasons": reasons, "metrics": metrics,
            "raw": {str(path.relative_to(repetition_raw)): sha256(path)
                    for path in sorted(raw.rglob("*")) if path.is_file()}}


def settle_consumers(load):
    """Stop the consumers that overran the load limit; their owners settle their scopes."""
    for consumer in load:
        consumer.deadline = 0
    with LIVE_LOCK:
        processes = [process for process in LIVE if isinstance(process.args, list)
                     and process.args[1:2] == ["exec"]]
    for process in processes:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
    for consumer in load:
        consumer.join(timeout=180)


def export_source(commit, work):
    source = work / "source"
    source.mkdir()
    archive = subprocess.run(["git", "archive", "--format=tar", commit], cwd=ROOT, capture_output=True, check=True)
    subprocess.run(["tar", "-x", "-C", str(source)], input=archive.stdout, check=True)
    return source


def prewarm(args, release, work, raw):
    """Build the warm target directory before a warm repetition, outside every interval."""
    runs = raw / "prewarm"
    runs.mkdir()
    consumer = Consumer("prewarm", ["/bin/sh", "-c", "cargo build --offline --locked --workspace && "
                                    "cargo test --offline --locked -p devguard-core -p devguard-contract --no-run"],
                        ["--adapter", "cargo-pipeline", *CARGO_BUDGET, "--wait", "30m"], release.devguard, runs,
                        time.monotonic() + 1, env=dict(cargo_environment(), CARGO_TARGET_DIR=str(work / TARGETS / "warm")),
                        cwd=work / "source")
    consumer.run()
    return consumer.runs


def await_front(fixture, limit=120):
    """Bring the fixture to the front and wait until it is; an operator can click it."""
    if fixture.headless:
        return False
    deadline = time.monotonic() + limit
    while time.monotonic() < deadline and not ABORT.is_set():
        if frontmost_pid() == fixture.process.pid:
            return True
        fixture.activate()
        time.sleep(1)
        if frontmost_pid() == fixture.process.pid:
            return True
        print("measure: waiting for the fixture window to be frontmost; click it once", flush=True)
        time.sleep(4)
    return False


def other_chrome():
    """Chrome processes this harness did not start (their activation would be ambiguous)."""
    listing = run_text("pgrep", "-f", str(CHROME)) or ""
    return [int(pid) for pid in listing.split()]


def repetition_run(args, release, plan, combination, repetition, out, work, fixture):
    name = f"{combination}-{repetition}"
    raw = out / name
    raw.mkdir()
    print(f"measure: {name} starting", flush=True)
    report = {"schema": "devguard-slo-repetition/v2", "combination": combination, "repetition": repetition,
              "rehearsal": args.rehearsal, "headless": args.headless, "chrome": fixture.version,
              "fixture_sha256": sha256(FIXTURE), "intervals": {}}
    report["service_before"] = service_state(release)
    service_pid = report["service_before"]["service_pid"]

    def conclude(verdict, reasons):
        report["service_after"] = service_state(release)
        report["verdict"], report["reasons"] = verdict, reasons
        (raw / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"measure: {name} {verdict} {reasons}", flush=True)
        return report

    if not report["service_before"]["runs_release"]:
        return conclude("inconclusive", ["the service did not run the measured release"])
    if combination == "warm":
        report["prewarm"] = prewarm(args, release, work, raw)
        if not any(run.get("result") == "completed" and run.get("exit_code") == 0 for run in report["prewarm"]):
            return conclude("inconclusive", ["the warm target directory could not be built"])
    fixture.reload()
    report["foreground_established"] = fixture.headless or await_front(fixture)
    if not report["foreground_established"]:
        return conclude("inconclusive", ["the fixture could not be brought to the front"])
    fixture.prime()
    time.sleep(SETTLE_S)
    fixture.drain()
    idle = measure_interval(args, release, "idle", raw, plan["idle_s"], fixture, combination, work, repetition,
                            service_pid)
    load = measure_interval(args, release, "load", raw, plan["load_s"], fixture, combination, work, repetition,
                            service_pid)
    report["intervals"] = {"idle": idle, "load": load}
    failed_removals = []
    for target in sorted((work / TARGETS).glob(f"cold-{repetition}-*")):
        shutil.rmtree(target, onerror=lambda *_, path=target: failed_removals.append(str(path)))
    report["cold_targets_not_removed"] = sorted(set(failed_removals))
    verdict, reasons = repetition_verdict(idle, load)
    after = service_state(release)
    if verdict == "pass" and not (after["runs_release"] and after["service_pid"] == service_pid):
        # A change after the last interval cannot be attributed to it; it only withholds a pass.
        verdict, reasons = "inconclusive", reasons + ["the service changed after the intervals"]
    if args.rehearsal:
        verdict, reasons = "inconclusive", reasons + ["rehearsal durations are below the protocol"]
    return conclude(verdict, reasons)


def protocol(args):
    plan = dict(REHEARSAL if args.rehearsal else PROTOCOL)
    if args.combinations:
        plan["combinations"] = args.combinations.split(",")
    if args.repetitions is not None:
        if args.repetitions < 1:
            raise SystemExit("measure: --repetitions must be at least 1")
        plan["repetitions"] = args.repetitions
    out, work = args.out.resolve(), args.work.resolve()
    for path in (out, work):
        if path.exists():
            raise SystemExit(f"measure: {path} already exists")
        if path.is_relative_to(ROOT) and subprocess.run(["git", "check-ignore", "-q", str(path)], cwd=ROOT).returncode:
            raise SystemExit(f"measure: {path} is inside this checkout and not ignored")
    if platform.system() != "Darwin":
        raise SystemExit("measure: the macOS protocol runs only on macOS")
    if not CHROME.exists():
        raise SystemExit("measure: the fixture needs Google Chrome in /Applications")
    # A headful fixture is brought to the front through LaunchServices, which must not find
    # another Chrome; a headless one is never activated.
    if not args.headless and other_chrome():
        raise SystemExit("measure: quit Google Chrome first; the fixture must be the only Chrome")
    rustc = run_text("rustc", "--version") or ""
    if not rustc.startswith(f"rustc {PINNED_RUST} "):
        raise SystemExit(f"measure: the Cargo load needs Rust {PINNED_RUST} first in PATH")
    release = Release(args.release)
    service = service_state(release)
    if not service["runs_release"]:
        raise SystemExit(f"measure: the service does not run {args.release}: {service}")
    charged = journal_counts()
    if "error" in charged or charged["charged_attempts"] or charged["charged_leases"]:
        raise SystemExit(f"measure: the service has charged work or an unreadable journal: {charged}")
    diagnosis = doctor(release)
    if not diagnosis["satisfied"] or not diagnosis["configuration_fingerprint"]:
        raise SystemExit(f"measure: devguard doctor does not report a ready service: {diagnosis}")
    out.mkdir(parents=True)
    work.mkdir(parents=True)
    manifest = json.loads(release.manifest.read_text())
    export_source(manifest["source"]["commit"], work)
    host = environment()
    fingerprint = source_fingerprint()
    header = {
        "schema": "devguard-slo-run/v2", "argv": sys.argv, "rehearsal": args.rehearsal, "headless": args.headless,
        "plan": plan,
        "approved_target": all(host.get(key) == value for key, value in APPROVED_TARGET.items()),
        "release": {"id": release.id, "directory": str(release.directory), "manifest_sha256": sha256(release.manifest),
                    "artifacts": manifest["artifacts"], "source": manifest["source"]},
        "policy": {"host_toml_sha256": sha256(HOST_CONFIG) if HOST_CONFIG.exists() else None,
                   "service": service, "doctor": diagnosis},
        "environment": host,
        "harness": {"head": fingerprint["head"], "tree_digest": fingerprint["tree_digest"],
                    "dirty": bool(subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True,
                                                 text=True).stdout.strip()),
                    "build_variables": {name: os.environ.get(name) for name in BUILD_VARIABLES},
                    "measure_sha256": sha256(Path(__file__)), "fixture_sha256": sha256(FIXTURE),
                    "qualify_source_sha256": tree_sha256(QUALIFY_SOURCE), "qualify_bin": str(args.qualify_bin),
                    "qualify_sha256": sha256(args.qualify_bin), "rustc": rustc, "cargo": run_text("cargo", "--version"),
                    "targets": {"status_p99_ms": STATUS_P99_MS, "termination_ack_p99_ms": TERMINATION_P99_MS,
                                "input_p99_ms": INPUT_P99_MS, "input_limit_ms": INPUT_LIMIT_MS,
                                "frame_stall_ms": FRAME_STALL_MS}},
        "started_unix": time.time(),
        "service_log_offset": SERVICE_LOG.stat().st_size if SERVICE_LOG.exists() else None,
    }
    # The display and the system stay awake for the whole run; caffeinate ends with this process.
    awake = track(subprocess.Popen(["caffeinate", "-d", "-i", "-w", str(os.getpid())],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    header["caffeinate_pid"] = awake.pid
    fixture = Fixture(work / "chrome", args.headless)
    try:
        fixture.open(FIXTURE)
        header["chrome"] = fixture.version
        (out / "run.json").write_text(json.dumps(header, indent=2) + "\n")
        results = {}
        for combination in plan["combinations"]:
            results[combination] = [repetition_run(args, release, plan, combination, repetition, out, work, fixture)
                                    for repetition in range(1, plan["repetitions"] + 1)]
    finally:
        fixture.close()
    summary = summarize(header, results, out)
    print(json.dumps({"verdict": summary["verdict"], "summary": str(out / "summary.json")}), flush=True)
    return 0 if summary["verdict"] == "qualified" else (1 if summary["verdict"] == "failed" else 2)


def summarize(header, results, out):
    combinations = {}
    for combination, reports in results.items():
        verdicts = [report["verdict"] for report in reports]
        combinations[combination] = {
            "verdict": combination_verdict(verdicts, header["plan"]["repetitions"]),
            "repetitions": [{"repetition": report["repetition"], "verdict": report["verdict"],
                             "reasons": report["reasons"],
                             "report_sha256": sha256(out / f"{combination}-{report['repetition']}/report.json")}
                            for report in reports]}
    overall = overall_verdict(combinations, header["plan"], header)
    if SERVICE_LOG.exists() and header["service_log_offset"] is not None:
        with open(SERVICE_LOG, "rb") as log:
            log.seek(header["service_log_offset"])
            (out / "service.log").write_bytes(log.read())
    summary = {"schema": "devguard-slo-summary/v2", "verdict": overall, "combinations": combinations,
               "targets": {
                   "local_status": "measured", "termination_acknowledgement": "measured",
                   "foreground_input_to_paint": "measured", "foreground_frames": "measured",
                   "pressure_connection_loss": "measured", "duplicate_execution": "measured",
                   "automatic_uncertain_restart": "measured: every uncertain load run fails its interval",
                   "protected_data_gc": "not_applicable: DG-1 has no garbage collection",
                   "codespace_mcp_approvals_replay": "not_run: CSRG-C08"},
               "run_sha256": sha256(out / "run.json"), "finished_unix": time.time()}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def fixture_check(args):
    """The fixture alone, for the functional suite: samples must arrive, and the verdict is never a
    pass. Headless, it must also report itself unobservable."""
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if not CHROME.exists():
        (out / "fixture-check.json").write_text(json.dumps(
            {"status": "not_run", "reason": "Google Chrome is not installed"}, indent=2) + "\n")
        print(json.dumps({"status": "not_run"}))
        return 0
    profile = out / "profile"
    fixture = Fixture(profile, args.headless)
    try:
        fixture.open(FIXTURE)
        fixture.prime()
        fixture.drain()
        validity = Sampler(out / "validity.jsonl", VALIDITY_PERIOD_S, validity_sample(fixture.process.pid))
        foreground = Foreground(fixture, out, seed=0)
        validity.start()
        foreground.start()
        time.sleep(args.seconds)
        foreground.stop()
        rows = validity.stop()
        metrics = foreground.metrics()
        version = fixture.version
    finally:
        fixture.close()
        shutil.rmtree(profile, ignore_errors=True)
    invalid = foreground.invalid(args.headless) + validity_reasons(rows, args.headless,
                                                                   int(args.seconds / VALIDITY_PERIOD_S))
    observed = metrics["input"]["painted"] > 0 and metrics["frames"]["frames"] > 1
    report = {"schema": "devguard-fixture-check/v2", "headless": args.headless, "chrome": version,
              "fixture_sha256": sha256(FIXTURE), "seconds": args.seconds, "metrics": metrics,
              "invalid": invalid, "verdict": "inconclusive",
              "status": "passed" if observed and (not args.headless or "headless fixture" in invalid) else "failed"}
    (out / "fixture-check.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in ("status", "verdict", "invalid")}))
    return 0 if report["status"] == "passed" else 1


def promote(args):
    """Record a qualified run's release after recomputing its verdict from the preserved reports and
    checking that the measured policy, host and release still hold."""
    summary, run, reasons = verify_run(args.summary.resolve())
    release = Release(run["release"]["id"])
    if sha256(release.manifest) != run["release"]["manifest_sha256"]:
        reasons.append("the release manifest differs from the measured one")
    service = service_state(release)
    if not service["runs_release"]:
        reasons.append("the service does not run the measured release")
    current_policy = {"host_toml_sha256": sha256(HOST_CONFIG) if HOST_CONFIG.exists() else None,
                      "doctor": doctor(release)}
    if not current_policy["doctor"]["satisfied"]:
        reasons.append("devguard doctor does not report a ready service")
    if policy_identity(current_policy) != policy_identity(run["policy"]):
        reasons.append("the policy differs from the measured one")
    if host_identity(environment()) != host_identity(run["environment"]):
        reasons.append("the host differs from the measured one")
    directory = SUPPORT / "qualifications"
    path = directory / f"{release.id}.json"
    if path.exists():
        reasons.append(f"{path} already exists")
    if reasons:
        print(json.dumps({"promoted": False, "reasons": reasons}, indent=1))
        return 1
    record = {
        "schema": "devguard-release-qualification/v1", "release": release.id,
        "manifest_sha256": run["release"]["manifest_sha256"], "artifacts": run["release"]["artifacts"],
        "source": run["release"]["source"], "policy": run["policy"], "environment": run["environment"],
        "harness": run["harness"], "plan": run["plan"], "combinations": summary["combinations"],
        "targets": summary["targets"],
        "evidence": {"summary": str(args.summary.resolve()), "summary_sha256": sha256(args.summary),
                     "run_sha256": summary["run_sha256"]},
        "verified_running": service,
        "scope": "macOS standalone control, development and bounded self-use on the measured host, artifact and "
                 "policy; not Linux, not CodeSpace integration",
        "promoted_unix": time.time(),
    }
    directory.mkdir(mode=0o700, exist_ok=True)
    os.chmod(directory, 0o700)
    temporary = directory / f".{release.id}.{os.getpid()}.tmp"
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    with os.fdopen(descriptor, "w") as handle:
        handle.write(json.dumps(record, indent=2) + "\n")
        handle.flush()
        os.fsync(handle.fileno())
    try:
        # A hard link never replaces an existing record.
        os.link(temporary, path)
    finally:
        os.unlink(temporary)
    print(json.dumps({"promoted": True, "record": str(path), "record_sha256": sha256(path)}, indent=1))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    macos = commands.add_parser("macos", help="run the SLO protocol against the installed release")
    macos.add_argument("--release", required=True)
    macos.add_argument("--qualify-bin", type=Path, required=True)
    macos.add_argument("--out", type=Path, required=True, help="new directory for the raw evidence")
    macos.add_argument("--work", type=Path, required=True, help="new directory for sources, targets and profiles")
    macos.add_argument("--rehearsal", action="store_true", help="short intervals; always inconclusive")
    macos.add_argument("--headless", action="store_true", help="functional only; always inconclusive")
    macos.add_argument("--combinations")
    macos.add_argument("--repetitions", type=int)
    check = commands.add_parser("fixture-check", help="run the fixture alone for the functional suite")
    check.add_argument("--out", type=Path, required=True)
    check.add_argument("--seconds", type=float, default=15)
    check.add_argument("--headless", action="store_true")
    promotion = commands.add_parser("promote", help="record a qualified run's release")
    promotion.add_argument("--summary", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "fixture-check":
        return fixture_check(args)
    if args.command == "promote":
        return promote(args)
    args.seed_source = random.SystemRandom()

    def interrupted(signum, frame):
        raise KeyboardInterrupt

    for number in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(number, interrupted)
    code = EXIT_CRASH
    try:
        code = protocol(args)
    except KeyboardInterrupt:
        code = 130
        print("measure: interrupted; nothing is promoted", flush=True)
    except SystemExit:
        raise
    except Exception:
        traceback.print_exc()
        print("measure: the harness failed; nothing is promoted", flush=True)
    finally:
        settle_live()
    return code


if __name__ == "__main__":
    sys.exit(main())
