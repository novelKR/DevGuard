#!/usr/bin/env python3
"""The macOS SLO protocol of DG1-C12: control and foreground responsiveness of the installed
service's release under bounded development load.

This is a measurement, not a functional test. Only the target host, in a window its operator has
set aside, can qualify a release. A rehearsal or a headless fixture is always inconclusive, and
promotion writes a qualification record only for a summary whose every repetition passed.
"""
import argparse
import datetime
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

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "crates/qualify/fixture/foreground.html"
CHROME_APP = Path("/Applications/Google Chrome.app")
CHROME = CHROME_APP / "Contents/MacOS/Google Chrome"
SUPPORT = Path.home() / "Library/Application Support/DevGuard"
SERVICE_LOG = Path.home() / "Library/Logs/DevGuard/devguardd.log"
HOST_CONFIG = Path.home() / ".config/devguard/host.toml"
PINNED_RUST = "1.95.0"

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
SETTLE_S = 10.0
CDP_TIMEOUT_S = 10.0
# Fewer samples than this share of an interval's schedule is not a valid observation.
MIN_SAMPLES = 0.9
# The terminations a pressure refusal left unstarted may lower this share.
MIN_TERMINATIONS = 0.8
# Admitted load must run for at least this share of the load interval.
MIN_LOAD = 0.5
STATUS_INTERVAL_S = 1.0
TERMINATE_PERIOD_S = 20.0
TRANSPORT = "resource_control_unavailable"

# Processes the harness started and must settle if it is interrupted.
ABORT = threading.Event()
LIVE = set()
LIVE_LOCK = threading.Lock()


def track(process):
    with LIVE_LOCK:
        LIVE.add(process)
    return process


def untrack(process):
    with LIVE_LOCK:
        LIVE.discard(process)


def settle_live():
    """Stop starting work and ask every live owner and probe to stop: each forwards SIGTERM to its
    managed scope and settles it, so an interrupted run leaves nothing charged."""
    ABORT.set()
    with LIVE_LOCK:
        processes = list(LIVE)
    for process in processes:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
    for process in processes:
        try:
            process.wait(timeout=180)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


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


def input_metrics(inputs, timing, dispatched):
    """Input to next paint of every dispatched input. The page's estimate is raised to the
    browser's own Event Timing duration when it reported one; an input that was not handled, or
    handled without a paint, is missing."""
    reported = {}
    for entry in timing:
        key = (entry["name"], round(entry["start"], 1))
        reported[key] = max(reported.get(key, 0.0), entry["duration"])
    latencies = []
    for record in inputs:
        if record.get("paint") is None:
            continue
        estimate = record["paint"] - record["stamp"]
        latencies.append(max(estimate, reported.get((record["type"], round(record["stamp"], 1)), 0.0)))
    missing = max(0, dispatched - len(latencies))
    p99 = nearest_rank(latencies, 0.99, missing)
    over = sum(1 for value in latencies if value > INPUT_LIMIT_MS) + missing
    return {"dispatched": dispatched, "handled": len(inputs), "painted": len(latencies), "missing": missing,
            "p99_ms": number(p99), "max_ms": number(max(latencies, default=None) if not missing else math.inf),
            "over_limit": over,
            "passed": p99 is not None and p99 <= INPUT_P99_MS and over == 0}


def frame_metrics(frames):
    gaps = [later - earlier for earlier, later in zip(frames, frames[1:])]
    stalls = sum(1 for gap in gaps if gap > FRAME_STALL_MS)
    return {"frames": len(frames), "max_gap_ms": number(max(gaps, default=None)), "stalls": stalls,
            "span_ms": number(frames[-1] - frames[0]) if len(frames) > 1 else 0,
            "passed": len(frames) > 1 and stalls == 0}


def status_metrics(samples, expected):
    answered = [sample["latency_ms"] for sample in samples if sample["outcome"] == "answered"]
    errors = [sample for sample in samples if sample["outcome"] != "answered"]
    losses = sum(1 for sample in errors if sample.get("code") == TRANSPORT)
    p99 = nearest_rank(answered, 0.99, len(errors))
    return {"samples": len(samples), "expected": expected, "answered": len(answered), "errors": len(errors),
            "connection_losses": losses, "p99_ms": number(p99),
            "max_ms": number(math.inf if errors else max(answered, default=None)),
            "passed": p99 is not None and p99 <= STATUS_P99_MS and losses == 0}


def termination_metrics(samples, expected):
    acknowledged = [sample for sample in samples if sample["outcome"] == "acknowledged"]
    errors = [sample for sample in samples if sample["outcome"] == "error"]
    unstarted = [sample for sample in samples if sample["outcome"] == "not_started"]
    acks = [sample["ack_ms"] for sample in acknowledged]
    released = [sample["released_ms"] for sample in acknowledged if sample.get("released_ms") is not None]
    unreleased = len(acknowledged) - len(released)
    losses = sum(1 for sample in errors if sample.get("code") == TRANSPORT)
    p99 = nearest_rank(acks, 0.99, len(errors))
    return {"samples": len(acknowledged) + len(errors), "expected": expected, "acknowledged": len(acknowledged),
            "errors": len(errors), "not_started": len(unstarted), "connection_losses": losses,
            "ack_p99_ms": number(p99), "ack_max_ms": number(math.inf if errors else max(acks, default=None)),
            # Reported separately: the scope's exit is not the acknowledgement.
            "release_p99_ms": number(nearest_rank(released, 0.99, unreleased)),
            "release_max_ms": number(math.inf if unreleased else max(released, default=None)),
            "unreleased": unreleased,
            "passed": p99 is not None and p99 <= TERMINATION_P99_MS and losses == 0 and unreleased == 0}


def load_metrics(runs, started, ended):
    """What the consumers did, and the share of the interval in which admitted load ran."""
    consumers = {}
    spans = []
    for run in runs:
        entry = consumers.setdefault(run["consumer"], {"runs": 0, "completed": 0, "not_started": 0,
                                                      "uncertain": 0, "nonzero_exit": 0, "refusal_reasons": {},
                                                      "seconds": []})
        entry["runs"] += 1
        result = run.get("result")
        if result == "completed":
            entry["completed"] += 1
            entry["seconds"].append(round(run["ended"] - run["started"], 3))
            if run.get("exit_code") not in (0, None):
                entry["nonzero_exit"] += 1
        elif result == "not_started":
            entry["not_started"] += 1
            reason = run.get("reason") or "unknown"
            entry["refusal_reasons"][reason] = entry["refusal_reasons"].get(reason, 0) + 1
        elif result == "uncertain":
            entry["uncertain"] += 1
        if run.get("running_from") is not None:
            spans.append((max(run["running_from"], started), min(run["ended"], ended)))
    covered, cursor = 0.0, started
    for begin, end in sorted(spans):
        begin = max(begin, cursor)
        if end > begin:
            covered += end - begin
            cursor = end
    duration = max(ended - started, 1e-9)
    losses = sum(1 for run in runs if run.get("connection_loss"))
    uncertain = sum(entry["uncertain"] for entry in consumers.values())
    return {"consumers": consumers, "applied_fraction": round(covered / duration, 4),
            "connection_losses": losses, "uncertain": uncertain,
            "duplicate_launches": sum(1 for run in runs if run.get("launches", 0) > 1)}


def interval_verdict(kind, validity, metrics):
    """pass, fail or inconclusive, with every reason. Invalid observation is never a pass."""
    reasons = list(validity)
    status, termination = metrics["status"], metrics["termination"]
    if status["samples"] < MIN_SAMPLES * status["expected"]:
        reasons.append(f"status samples {status['samples']} of {status['expected']}")
    if termination["samples"] < MIN_TERMINATIONS * termination["expected"]:
        reasons.append(f"termination samples {termination['samples']} of {termination['expected']}")
    inputs = metrics["input"]
    if inputs["dispatched"] < MIN_SAMPLES * metrics["input_expected"]:
        reasons.append(f"inputs dispatched {inputs['dispatched']} of {metrics['input_expected']}")
    if kind == "load" and metrics["load"]["applied_fraction"] < MIN_LOAD:
        reasons.append(f"load ran for {metrics['load']['applied_fraction']:.0%} of the interval")
    if reasons:
        return "inconclusive", reasons
    failures = [name for name in ("status", "termination", "input", "frames") if not metrics[name]["passed"]]
    if kind == "load":
        load = metrics["load"]
        if load["connection_losses"] or load["uncertain"]:
            failures.append("load connection losses or uncertain executions")
        if load["duplicate_launches"]:
            failures.append("duplicate launches")
    if metrics.get("journal", {}).get("charged_after"):
        failures.append("attempts still charged after the interval")
    return ("fail", failures) if failures else ("pass", [])


def repetition_verdict(idle, load):
    """A failing idle baseline is inconclusive, never a failure of the load."""
    if idle["verdict"] != "pass":
        return "inconclusive", [f"idle baseline {idle['verdict']}"] + idle["reasons"]
    if load["verdict"] != "pass":
        return load["verdict"], load["reasons"]
    return "pass", []


def combination_verdict(verdicts, required):
    """Qualified only when every required repetition passed; values are never pooled."""
    if any(verdict == "fail" for verdict in verdicts):
        return "failed"
    if len(verdicts) >= required and all(verdict == "pass" for verdict in verdicts):
        return "qualified"
    return "inconclusive"


# ---------------------------------------------------------------- host observation

def run_text(*args, timeout=10):
    result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    return result.stdout.strip() if result.returncode == 0 else None


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def frontmost_pid():
    """The process of the frontmost application, from LaunchServices."""
    front = run_text("lsappinfo", "front")
    if not front:
        return None
    info = run_text("lsappinfo", "info", "-only", "pid", front) or ""
    match = re.search(r'"?pid"?\s*=\s*(\d+)', info)
    return int(match.group(1)) if match else None


def screen_locked():
    text = run_text("ioreg", "-n", "Root", "-d1", "-k", "IOConsoleUsers") or ""
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
    """Calls `sample` every `period` seconds until stopped, writing JSON lines to `path`."""

    def __init__(self, path, period, sample):
        super().__init__(daemon=True)
        self.path, self.period, self.sample = path, period, sample
        self.halt = threading.Event()
        self.rows = []

    def run(self):
        with open(self.path, "a") as out:
            next_at = time.monotonic()
            while not self.halt.is_set():
                row = {"unix": time.time(), **self.sample()}
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


# ---------------------------------------------------------------- the foreground fixture

class Fixture:
    """Chrome with a fresh profile, driven over --remote-debugging-pipe: private descriptors, no
    listening port, and no flag that changes scheduling or throttling."""

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
        self.process = subprocess.Popen(args, pass_fds=(3, 4), preexec_fn=descriptors, close_fds=True,
                                        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                        stderr=subprocess.DEVNULL)
        os.close(to_chrome)
        os.close(from_chrome)
        self.buffer = b""
        self.sequence = 0
        self.session = None

    def call(self, method, params=None, session=True, timeout=CDP_TIMEOUT_S):
        self.sequence += 1
        wanted = self.sequence
        message = {"id": wanted, "method": method, "params": params or {}}
        if session and self.session:
            message["sessionId"] = self.session
        os.write(self.writer, json.dumps(message).encode() + b"\0")
        deadline = time.monotonic() + timeout
        while True:
            while b"\0" in self.buffer:
                raw, self.buffer = self.buffer.split(b"\0", 1)
                reply = json.loads(raw)
                if reply.get("id") == wanted:
                    if "error" in reply:
                        raise RuntimeError(f"{method}: {reply['error'].get('message')}")
                    return reply.get("result", {})
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(method)
            ready, _, _ = select.select([self.reader], [], [], remaining)
            if ready:
                chunk = os.read(self.reader, 1 << 20)
                if not chunk:
                    raise ConnectionError("the fixture browser closed its pipe")
                self.buffer += chunk

    def open(self, page):
        self.version = self.call("Browser.getVersion", session=False)["product"]
        target = self.call("Target.createTarget", {"url": page.as_uri()}, session=False)["targetId"]
        self.session = self.call("Target.attachToTarget", {"targetId": target, "flatten": True},
                                 session=False)["sessionId"]
        self.call("Runtime.enable")
        self.call("Page.enable")
        self.call("Page.bringToFront")
        time.sleep(1.0)
        self.targets = json.loads(self.evaluate("targets()"))

    def activate(self):
        """Bring this Chrome to the front through LaunchServices. No other Chrome may run."""
        if not self.headless:
            subprocess.run(["open", "-a", str(CHROME_APP)], capture_output=True, timeout=30)
            self.call("Page.bringToFront")

    def evaluate(self, expression):
        result = self.call("Runtime.evaluate", {"expression": expression, "returnByValue": True})
        if "exceptionDetails" in result:
            raise RuntimeError(f"fixture page error: {result['exceptionDetails'].get('text')}")
        return result["result"]["value"]

    def drain(self):
        return json.loads(self.evaluate("drain()"))

    def mouse(self, kind, target, **extra):
        point = self.targets[target]
        self.call("Input.dispatchMouseEvent", {"type": kind, "x": point["x"], "y": point["y"], **extra})

    def prime(self):
        """Focus the text field once; this click is not a sample."""
        for kind in ("mousePressed", "mouseReleased"):
            self.mouse(kind, "field", button="left", clickCount=1)

    def send(self, index):
        kind = index % 3
        if kind == 0:
            letter = chr(ord("a") + index % 26)
            self.call("Input.dispatchKeyEvent", {"type": "keyDown", "key": letter, "text": letter,
                                                 "code": "Key" + letter.upper(),
                                                 "windowsVirtualKeyCode": ord(letter.upper())})
            self.call("Input.dispatchKeyEvent", {"type": "keyUp", "key": letter, "code": "Key" + letter.upper(),
                                                 "windowsVirtualKeyCode": ord(letter.upper())})
        elif kind == 1:
            self.mouse("mousePressed", "button", button="left", clickCount=1)
            self.mouse("mouseReleased", "button", button="left", clickCount=1)
        else:
            self.mouse("mouseWheel", "scroller", deltaX=0, deltaY=120 if index % 2 else -120)

    def close(self):
        try:
            self.call("Browser.close", session=False, timeout=5)
        except (OSError, RuntimeError, TimeoutError, ConnectionError):
            pass
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        for descriptor in (self.reader, self.writer):
            try:
                os.close(descriptor)
            except OSError:
                pass


class Foreground(threading.Thread):
    """Sends one input every INPUT_PERIOD_S (±INPUT_JITTER_S) and drains the page every
    DRAIN_PERIOD_S until stopped. A drain that times out while the page is stalled is retried:
    the stall then shows in the frames and inputs, and only a closed browser is unobservable."""

    def __init__(self, fixture, raw, seed):
        super().__init__(daemon=True)
        self.fixture, self.raw = fixture, raw
        self.random = random.Random(seed)
        self.halt = threading.Event()
        self.dispatched = 0
        self.dispatch_timeouts = 0
        self.drain_timeouts = 0
        self.frames, self.inputs, self.timing, self.page_events, self.page_states = [], [], [], [], []
        self.handled_at_start = None
        self.handled = None
        self.error = None

    def collect(self):
        drained = self.fixture.drain()
        with open(self.raw / "fixture.jsonl", "a") as out:
            out.write(json.dumps({"unix": time.time(), **drained}) + "\n")
        self.frames += drained["frames"]
        self.inputs += drained["inputs"]
        self.timing += drained["timing"]
        self.page_events += drained["validity"]
        self.page_states.append((drained["visible"], drained["focus"]))
        self.handled = drained["handled"]

    def run(self):
        try:
            self.handled_at_start = self.fixture.drain()["handled"]
            next_input = next_drain = time.monotonic()
            next_drain += DRAIN_PERIOD_S
            index = 0
            while not self.halt.is_set():
                now = time.monotonic()
                if now >= next_input:
                    self.dispatched += 1
                    try:
                        self.fixture.send(index)
                    except TimeoutError:
                        self.dispatch_timeouts += 1
                    index += 1
                    next_input += INPUT_PERIOD_S + self.random.uniform(-INPUT_JITTER_S, INPUT_JITTER_S)
                    # A delayed dispatch is not made up for with a burst.
                    next_input = max(next_input, time.monotonic())
                if now >= next_drain:
                    try:
                        self.collect()
                    except TimeoutError:
                        self.drain_timeouts += 1
                    next_drain = time.monotonic() + DRAIN_PERIOD_S
                self.halt.wait(max(0.0, min(next_input, next_drain) - time.monotonic()))
            # Inputs sent at the end are reported once painted, or after the page's paint limit.
            time.sleep(5.5)
            self.collect()
        except (OSError, RuntimeError, ValueError, ConnectionError) as error:
            self.error = str(error)

    def stop(self):
        self.halt.set()
        self.join()

    def invalid(self, headless):
        """Reasons the page's own record makes the interval unobservable."""
        reasons = []
        if headless:
            reasons.append("headless fixture")
        if self.error:
            reasons.append(f"fixture observation failed: {self.error}")
        if not self.page_states:
            reasons.append("no fixture sample was drained")
        if any(not (visible and focus) for visible, focus in self.page_states):
            reasons.append("fixture page not visible and focused")
        if self.page_events:
            kinds = sorted({event["event"] for event in self.page_events})
            reasons.append("visibility or focus changed: " + ", ".join(kinds))
        handled = self.handled or {}
        received = sum(handled.values()) - sum((self.handled_at_start or {}).values())
        if received > self.dispatched:
            reasons.append("inputs the harness did not send")
        return reasons

    def metrics(self):
        return {"input": input_metrics(self.inputs, self.timing, self.dispatched),
                "frames": frame_metrics(self.frames),
                "fixture": {"dispatch_timeouts": self.dispatch_timeouts, "drain_timeouts": self.drain_timeouts}}


def validity_sample(fixture_pid):
    def sample():
        return {"front_pid": frontmost_pid(), "fixture_pid": fixture_pid, "locked": screen_locked()}
    return sample


def validity_reasons(rows, headless):
    """The host-side conditions: the fixture frontmost and the screen unlocked, at every sample."""
    reasons = []
    if not rows:
        reasons.append("no validity sample")
    if not headless and any(row["front_pid"] != row["fixture_pid"] for row in rows):
        reasons.append("the fixture was not the frontmost application")
    if any(row["locked"] for row in rows):
        reasons.append("the screen was locked")
    return reasons


# ---------------------------------------------------------------- the service, probe and load

def release_paths(release):
    directory = SUPPORT / "releases" / release
    return {"directory": directory, "manifest": directory / "MANIFEST.json",
            "devguardd": directory / "bin/devguardd", "devguard": directory / "bin/devguard",
            "helper": directory / "bin/devguard-launch"}


def service_state(release):
    """The installed service, as the measured release's own devguardd reports it."""
    paths = release_paths(release)
    manifest = json.loads(paths["manifest"].read_text())
    result = subprocess.run([str(paths["devguardd"]), "status"], capture_output=True, text=True, timeout=60)
    status = json.loads(result.stdout) if result.stdout.strip() else None
    running = (status or {}).get("running") or {}
    return {"returncode": result.returncode, "healthy": bool(status and status.get("healthy")),
            "current": ((status or {}).get("selection") or {}).get("current"),
            "running_sha256": running.get("sha256"), "service_pid": running.get("service_pid"),
            "manifest_devguardd_sha256": manifest["artifacts"]["devguardd"]["sha256"],
            "runs_release": bool(status and status.get("healthy")
                                 and ((status.get("selection") or {}).get("current")) == release
                                 and running.get("sha256") == manifest["artifacts"]["devguardd"]["sha256"])}


def journal_counts():
    """Attempt phases and what stays charged, from the service's journal, read-only."""
    path = SUPPORT / "state/authority.sqlite"
    try:
        connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True, timeout=5)
        try:
            phases = connection.execute(
                "SELECT json_extract(record, '$.phase'), COUNT(*) FROM attempts GROUP BY 1").fetchall()
            attempts = connection.execute("SELECT COUNT(*) FROM attempts WHERE charged = 1").fetchone()[0]
            leases = connection.execute("SELECT COUNT(*) FROM leases WHERE charged = 1").fetchone()[0]
        finally:
            connection.close()
    except sqlite3.Error as error:
        return {"error": str(error)}
    return {"phases": {str(phase): count for phase, count in phases},
            "charged_attempts": attempts, "charged_leases": leases}


class Probe:
    """devguard-qualify control for one interval."""

    def __init__(self, binary, release, raw, name, seconds):
        self.path = raw / f"control-{name}.jsonl"
        self.process = subprocess.Popen(
            [str(binary), "control", "--helper", str(release_paths(release)["helper"]),
             "--duration-s", str(int(seconds)), "--out", str(self.path),
             "--status-interval-ms", str(int(STATUS_INTERVAL_S * 1000)),
             "--terminate-period-s", str(int(TERMINATE_PERIOD_S))],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        track(self.process)

    def await_target(self, limit=180):
        """Wait until the probe's status target runs, or the probe gives up."""
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline and self.process.poll() is None:
            if self.path.exists():
                for line in self.path.read_text().splitlines():
                    try:
                        sample = json.loads(line)
                    except ValueError:
                        continue
                    if (sample.get("kind"), sample.get("role"), sample.get("outcome")) == \
                            ("admission", "status", "admitted"):
                        # The helper reports READY right after; the first status call follows.
                        time.sleep(0.5)
                        return True
            time.sleep(0.2)
        return False

    def finish(self, stop=False):
        if stop and self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
        out, err = self.process.communicate(timeout=180)
        untrack(self.process)
        samples = [json.loads(line) for line in self.path.read_text().splitlines()] if self.path.exists() else []
        return {"returncode": self.process.returncode, "summary": out.strip(), "stderr": err.strip()[-2000:],
                "samples": samples}


class Consumer(threading.Thread):
    """One load consumer: runs its command through `devguard exec` again and again until the
    deadline, never starting after it, and records every run and its receipt."""

    def __init__(self, name, command, options, devguard, raw, deadline, env=None, cwd=None,
                 feed=None, sink=False, before=None, after=None):
        super().__init__(daemon=True)
        self.name, self.command, self.options = name, command, options
        self.devguard, self.raw, self.deadline = devguard, raw, deadline
        self.env, self.cwd, self.feed, self.sink = env, cwd, feed, sink
        self.before, self.after = before, after
        self.runs = []
        self.errors = []

    def run(self):
        index = 0
        while time.monotonic() < self.deadline and not ABORT.is_set():
            index += 1
            receipt = self.raw / f"{self.name}-{index:03d}.receipt.json"
            context = self.before(index) if self.before else {}
            env = dict(os.environ if self.env is None else self.env, **context.get("env", {}))
            started = time.time()
            try:
                process = subprocess.Popen(
                    [str(self.devguard), "exec", *self.options, "--receipt", str(receipt), "--", *self.command],
                    stdin=subprocess.PIPE if self.feed else subprocess.DEVNULL,
                    stdout=subprocess.PIPE if self.sink else subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL, env=env, cwd=self.cwd)
                track(process)
                drained = [0]
                reader = None
                if self.sink:
                    def read(stream=process.stdout):
                        for chunk in iter(lambda: stream.read(1 << 16), b""):
                            drained[0] += len(chunk)
                    reader = threading.Thread(target=read, daemon=True)
                    reader.start()
                if self.feed:
                    self.feed(process.stdin)
                code = process.wait()
                untrack(process)
                if reader:
                    reader.join()
            except OSError as error:
                self.errors.append(str(error))
                return
            ended = time.time()
            if self.after:
                self.after(index, context)
            self.runs.append(summarize_run(self.name, receipt, code, started, ended, drained[0]))


def summarize_run(consumer, receipt, code, started, ended, output_bytes):
    run = {"consumer": consumer, "started": started, "ended": ended, "exit_code": code,
           "output_bytes": output_bytes, "receipt": receipt.name}
    try:
        data = json.loads(receipt.read_text())
    except (OSError, ValueError):
        run.update(result="unknown", connection_loss=True)
        return run
    result = data.get("result")
    notes = " ".join(entry.get("note", "") for entry in data.get("attempts") or [])
    reason = data.get("reason") or ""
    waited = (data.get("wait") or {}).get("waited_ms") or 0
    run.update(result=result, reason=reason or None, waited_ms=waited,
               exit_code=(data.get("exit") or {}).get("code", code),
               launches=1 if data.get("launch") else 0,
               connection_loss=result == "uncertain" or ("ResourceControlUnavailable" in notes + reason),
               running_from=started + waited / 1000.0 if result == "completed" else None)
    return run


def slow_feed(lines, period):
    def feed(stream):
        try:
            for index in range(lines):
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


def consumers(args, combination, work, raw, deadline, rehearsal):
    """The six load consumers of the protocol, through the measured release's devguard."""
    devguard = release_paths(args.release)["devguard"]
    qualify = str(args.qualify_bin)
    seconds = "20" if rehearsal else "120"
    env = dict(os.environ)
    targets = work / "targets"

    def cargo_before(index):
        target = targets / ("warm" if combination == "warm" else f"cold-{index:03d}")
        return {"env": {"CARGO_TARGET_DIR": str(target)}, "target": target}

    def cargo_after(index, context):
        if combination == "cold":
            shutil.rmtree(context["target"], ignore_errors=True)

    cargo = ["/bin/sh", "-c", "cargo build --offline --locked --workspace && "
             "cargo test --offline --locked -p devguard-core -p devguard-contract"]
    io_dir = work / "io"
    io_dir.mkdir(exist_ok=True)
    return [
        Consumer("cargo", cargo, ["--adapter", "cargo-pipeline", "--wait", "30m"], devguard, raw, deadline,
                 env=env, cwd=work / "source", before=cargo_before, after=cargo_after),
        Consumer("cpu", [qualify, "work", "cpu", "--threads", "2", "--seconds", seconds],
                 ["--cpu", "2000", "--memory", "256MiB", "--tasks", "8", "--wait", "30m"], devguard, raw, deadline),
        Consumer("memory", [qualify, "work", "memory", "--mib", "1536", "--seconds", seconds],
                 ["--cpu", "250", "--memory", "2GiB", "--tasks", "4", "--wait", "30m"], devguard, raw, deadline),
        Consumer("io", [qualify, "work", "io", "--dir", str(io_dir), "--mib", "512"],
                 ["--cpu", "250", "--memory", "256MiB", "--tasks", "4", "--wait", "30m"], devguard, raw, deadline),
        Consumer("output", [qualify, "work", "output", "--mib", "256"],
                 ["--cpu", "250", "--memory", "128MiB", "--tasks", "4", "--wait", "30m"], devguard, raw, deadline,
                 sink=True),
        Consumer("slow-input", [qualify, "work", "slow-reader"],
                 ["--cpu", "100", "--memory", "64MiB", "--tasks", "4", "--wait", "30m"], devguard, raw, deadline,
                 feed=slow_feed(200 if rehearsal else 600, 0.1)),
    ]


# ---------------------------------------------------------------- the protocol

def measure_interval(args, kind, repetition_raw, seconds, fixture, combination, work, rehearsal, seed):
    """One idle or load interval: the fixture, the control probe, validity and host samples
    throughout, and for load the six consumers, observed until every started command completes."""
    raw = repetition_raw / kind
    raw.mkdir()
    journal_before = journal_counts()
    probe = Probe(args.qualify_bin, args.release, raw, kind, seconds + (7200 if kind == "load" else 0))
    # The probe's status target runs before any load competes with it for admission.
    probe.await_target()
    started = time.time()
    begun = time.monotonic()
    host = Sampler(raw / "host.jsonl", HOST_PERIOD_S, host_sample)
    validity = Sampler(raw / "validity.jsonl", VALIDITY_PERIOD_S, validity_sample(fixture.process.pid))
    foreground = Foreground(fixture, raw, seed)
    for thread in (host, validity, foreground):
        thread.start()
    load = []
    if kind == "load":
        (raw / "runs").mkdir()
        load = consumers(args, combination, work, raw / "runs", begun + seconds, rehearsal)
        for consumer in load:
            consumer.start()
        for consumer in load:
            consumer.join()
    else:
        ABORT.wait(max(0.0, seconds - (time.monotonic() - begun)))
    if ABORT.is_set():
        raise KeyboardInterrupt
    ended = time.time()
    elapsed = time.monotonic() - begun
    foreground.stop()
    validity_rows = validity.stop()
    host_rows = host.stop()
    control = probe.finish(stop=kind == "load")
    status = [sample for sample in control["samples"] if sample.get("kind") == "status"]
    terminations = [sample for sample in control["samples"] if sample.get("kind") == "terminate"]
    metrics = {
        **foreground.metrics(),
        "input_expected": int(elapsed / INPUT_PERIOD_S),
        "status": status_metrics(status, int(elapsed / STATUS_INTERVAL_S)),
        "termination": termination_metrics(terminations, max(1, int(elapsed / TERMINATE_PERIOD_S))),
        "probe": {key: control[key] for key in ("returncode", "summary", "stderr")},
        "journal": {"before": journal_before, "after": journal_counts()},
        "host": host_peaks(host_rows),
    }
    after = metrics["journal"]["after"]
    metrics["journal"]["charged_after"] = (
        {"error": after["error"]} if "error" in after
        else {key: after[key] for key in ("charged_attempts", "charged_leases") if after[key]})
    if kind == "load":
        runs = [run for consumer in load for run in consumer.runs]
        (raw / "runs.json").write_text(json.dumps(runs, indent=1) + "\n")
        metrics["load"] = load_metrics(runs, started, ended)
        metrics["load"]["consumer_errors"] = {consumer.name: consumer.errors for consumer in load if consumer.errors}
    reasons = foreground.invalid(fixture.headless) + validity_reasons(validity_rows, fixture.headless)
    if control["returncode"] != 0:
        reasons.append(f"control probe exited {control['returncode']}: {control['stderr'][-300:]}")
    verdict, reasons = interval_verdict(kind, reasons, metrics)
    return {"kind": kind, "started_unix": started, "ended_unix": ended, "seconds": round(elapsed, 3),
            "verdict": verdict, "reasons": reasons, "metrics": metrics,
            "raw": {str(path.relative_to(repetition_raw)): sha256(path)
                    for path in sorted(raw.rglob("*")) if path.is_file()}}


def host_peaks(rows):
    levels = {}
    for row in rows:
        levels[row["pressure_level"]] = levels.get(row["pressure_level"], 0) + 1
    swap = [row["swap_used_mib"] for row in rows if row["swap_used_mib"] is not None]
    pageouts = [row["pageouts"] for row in rows if row["pageouts"] is not None]
    return {"samples": len(rows), "pressure_levels": levels, "swap_used_mib_max": max(swap, default=None),
            "pageouts_delta": (pageouts[-1] - pageouts[0]) if len(pageouts) > 1 else None}


def export_source(commit, work):
    source = work / "source"
    source.mkdir()
    archive = subprocess.run(["git", "archive", "--format=tar", commit], cwd=ROOT, capture_output=True, check=True)
    subprocess.run(["tar", "-x", "-C", str(source)], input=archive.stdout, check=True)
    return source


def prewarm(args, work, raw):
    """Build the warm target directory before a warm repetition, outside every interval."""
    runs = raw / "prewarm"
    runs.mkdir()
    consumer = Consumer("prewarm", ["/bin/sh", "-c", "cargo build --offline --locked --workspace && "
                                    "cargo test --offline --locked -p devguard-core -p devguard-contract --no-run"],
                        ["--adapter", "cargo-pipeline", "--wait", "30m"], release_paths(args.release)["devguard"],
                        runs, time.monotonic() + 1, env=dict(os.environ, CARGO_TARGET_DIR=str(work / "targets/warm")),
                        cwd=work / "source")
    consumer.run()
    return consumer.runs


def await_front(fixture, invalid_after=120):
    """Bring the fixture to the front and wait until it is; an operator can click it."""
    if fixture.headless:
        return False
    deadline = time.monotonic() + invalid_after
    fixture.activate()
    while time.monotonic() < deadline:
        if frontmost_pid() == fixture.process.pid:
            return True
        print("measure: waiting for the fixture window to be frontmost; click it once", flush=True)
        time.sleep(5)
        fixture.activate()
    return False


def other_chrome():
    """Chrome processes this harness did not start (their activation would be ambiguous)."""
    listing = run_text("pgrep", "-f", str(CHROME)) or ""
    return [int(pid) for pid in listing.split()]


def protocol(args):
    plan = dict(REHEARSAL if args.rehearsal else PROTOCOL)
    if args.combinations:
        plan["combinations"] = args.combinations.split(",")
    if args.repetitions:
        plan["repetitions"] = args.repetitions
    out, work = args.out.resolve(), args.work.resolve()
    out.mkdir(parents=True, exist_ok=False)
    work.mkdir(parents=True, exist_ok=False)
    if platform.system() != "Darwin":
        raise SystemExit("measure: the macOS protocol runs only on macOS")
    if not CHROME.exists():
        raise SystemExit("measure: the fixture needs Google Chrome in /Applications")
    if other_chrome():
        raise SystemExit("measure: quit Google Chrome first; the fixture must be the only Chrome")
    rustc = run_text("rustc", "--version") or ""
    if f"rustc {PINNED_RUST} " not in rustc + " ":
        raise SystemExit(f"measure: the Cargo load needs Rust {PINNED_RUST} first in PATH")
    service = service_state(args.release)
    if not service["runs_release"]:
        raise SystemExit(f"measure: the service does not run {args.release}: {service}")
    manifest = json.loads(release_paths(args.release)["manifest"].read_text())
    source = manifest["source"]["commit"]
    export_source(source, work)
    header = {
        "schema": "devguard-slo-run/v1", "rehearsal": args.rehearsal, "headless": args.headless,
        "plan": plan, "release": {"id": args.release, "manifest_sha256": sha256(release_paths(args.release)["manifest"]),
                                  "artifacts": manifest["artifacts"], "source": manifest["source"]},
        "policy": {"host_toml_sha256": sha256(HOST_CONFIG) if HOST_CONFIG.exists() else None,
                   "service": service},
        "environment": environment(),
        "harness": {"measure_sha256": sha256(Path(__file__)), "fixture_sha256": sha256(FIXTURE),
                    "qualify_sha256": sha256(args.qualify_bin), "rustc": rustc,
                    "cargo": run_text("cargo", "--version"),
                    "targets": {"status_p99_ms": STATUS_P99_MS, "termination_ack_p99_ms": TERMINATION_P99_MS,
                                "input_p99_ms": INPUT_P99_MS, "input_limit_ms": INPUT_LIMIT_MS,
                                "frame_stall_ms": FRAME_STALL_MS}},
        "started_unix": time.time(),
        "service_log_offset": SERVICE_LOG.stat().st_size if SERVICE_LOG.exists() else None,
    }
    # The display and the system stay awake for the whole run; caffeinate ends with this process.
    awake = subprocess.Popen(["caffeinate", "-d", "-i", "-w", str(os.getpid())],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    header["caffeinate_pid"] = awake.pid
    (out / "run.json").write_text(json.dumps(header, indent=2) + "\n")
    results = {}
    for combination in plan["combinations"]:
        results[combination] = []
        for repetition in range(1, plan["repetitions"] + 1):
            report = repetition_run(args, plan, combination, repetition, out, work)
            results[combination].append(report)
    summary = summarize(args, header, results, out)
    print(json.dumps({"verdict": summary["verdict"], "summary": str(out / "summary.json")}), flush=True)
    return 0 if summary["verdict"] == "qualified" else (2 if summary["verdict"] == "inconclusive" else 1)


def repetition_run(args, plan, combination, repetition, out, work):
    name = f"{combination}-{repetition}"
    raw = out / name
    raw.mkdir()
    print(f"measure: {name} starting", flush=True)
    report = {"schema": "devguard-slo-repetition/v1", "combination": combination, "repetition": repetition,
              "rehearsal": args.rehearsal}
    report["service_before"] = service_state(args.release)
    if combination == "warm":
        report["prewarm"] = prewarm(args, work, raw)
    fixture = Fixture(work / f"chrome-{name}", args.headless)
    try:
        fixture.open(FIXTURE)
        report["chrome"] = fixture.version
        report["foreground_established"] = await_front(fixture)
        if not report["foreground_established"] and not args.headless:
            report["service_after"] = service_state(args.release)
            report["intervals"] = {}
            report["verdict"], report["reasons"] = "inconclusive", ["the fixture could not be brought to the front"]
            (raw / "report.json").write_text(json.dumps(report, indent=2) + "\n")
            print(f"measure: {name} inconclusive {report['reasons']}", flush=True)
            return report
        fixture.prime()
        time.sleep(SETTLE_S)
        fixture.drain()
        seed = random.SystemRandom().getrandbits(32)
        report["input_seed"] = seed
        idle = measure_interval(args, "idle", raw, plan["idle_s"], fixture, combination, work, args.rehearsal, seed)
        load = measure_interval(args, "load", raw, plan["load_s"], fixture, combination, work, args.rehearsal,
                                seed + 1)
    finally:
        fixture.close()
        shutil.rmtree(work / f"chrome-{name}", ignore_errors=True)
    report["service_after"] = service_state(args.release)
    report["intervals"] = {"idle": idle, "load": load}
    verdict, reasons = repetition_verdict(idle, load)
    if not (report["service_before"]["runs_release"] and report["service_after"]["runs_release"]
            and report["service_before"]["service_pid"] == report["service_after"]["service_pid"]):
        verdict, reasons = "inconclusive", reasons + ["the service did not run the release throughout"]
    if args.rehearsal:
        verdict, reasons = "inconclusive", reasons + ["rehearsal durations are below the protocol"]
    report["verdict"], report["reasons"] = verdict, reasons
    (raw / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"measure: {name} {verdict} {reasons}", flush=True)
    return report


def summarize(args, header, results, out):
    combinations = {}
    for combination, reports in results.items():
        verdicts = [report["verdict"] for report in reports]
        combinations[combination] = {
            "verdict": combination_verdict(verdicts, header["plan"]["repetitions"]),
            "repetitions": [{"repetition": report["repetition"], "verdict": report["verdict"],
                             "reasons": report["reasons"],
                             "report_sha256": sha256(out / f"{combination}-{report['repetition']}/report.json")}
                            for report in reports]}
    verdicts = [entry["verdict"] for entry in combinations.values()]
    overall = ("qualified" if verdicts and all(v == "qualified" for v in verdicts) and not args.rehearsal
               and not args.headless and set(header["plan"]["combinations"]) >= {"cold", "warm"}
               and header["plan"]["repetitions"] >= PROTOCOL["repetitions"]
               else "failed" if "failed" in verdicts else "inconclusive")
    if SERVICE_LOG.exists() and header["service_log_offset"] is not None:
        with open(SERVICE_LOG, "rb") as log:
            log.seek(header["service_log_offset"])
            (out / "service.log").write_bytes(log.read())
    summary = {"schema": "devguard-slo-summary/v1", "verdict": overall, "combinations": combinations,
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
    invalid = foreground.invalid(args.headless) + validity_reasons(rows, args.headless)
    observed = metrics["input"]["painted"] > 0 and metrics["frames"]["frames"] > 1
    report = {"schema": "devguard-fixture-check/v1", "headless": args.headless, "chrome": version,
              "fixture_sha256": sha256(FIXTURE), "seconds": args.seconds, "metrics": metrics,
              "invalid": invalid, "verdict": "inconclusive",
              "status": "passed" if observed and (not args.headless or "headless fixture" in invalid) else "failed"}
    (out / "fixture-check.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in ("status", "verdict", "invalid")}))
    return 0 if report["status"] == "passed" else 1


def promote(args):
    """Write the qualification record of a qualified summary and verify the service runs it."""
    out = args.summary.resolve().parent
    summary = json.loads(args.summary.read_text())
    run = json.loads((out / "run.json").read_text())
    if summary["verdict"] != "qualified" or run["rehearsal"] or run["headless"]:
        raise SystemExit(f"measure: {args.summary} is {summary['verdict']}; nothing is promoted")
    if sha256(out / "run.json") != summary["run_sha256"]:
        raise SystemExit("measure: the run header changed after the summary")
    release = run["release"]["id"]
    if sha256(release_paths(release)["manifest"]) != run["release"]["manifest_sha256"]:
        raise SystemExit("measure: the release manifest differs from the measured one")
    service = service_state(release)
    if not service["runs_release"]:
        raise SystemExit(f"measure: the service no longer runs {release}")
    record = {
        "schema": "devguard-release-qualification/v1", "release": release,
        "manifest_sha256": run["release"]["manifest_sha256"], "artifacts": run["release"]["artifacts"],
        "source": run["release"]["source"], "policy": run["policy"], "environment": run["environment"],
        "harness": run["harness"], "plan": run["plan"], "combinations": summary["combinations"],
        "evidence": {"summary": str(args.summary.resolve()), "summary_sha256": sha256(args.summary),
                     "run_sha256": summary["run_sha256"]},
        "scope": "macOS standalone control, development and bounded self-use on the measured host; "
                 "not Linux, not CodeSpace integration",
        "promoted_unix": time.time(),
    }
    directory = SUPPORT / "qualifications"
    directory.mkdir(mode=0o700, exist_ok=True)
    os.chmod(directory, 0o700)
    path = directory / f"{release}.json"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    with os.fdopen(descriptor, "w") as handle:
        handle.write(json.dumps(record, indent=2) + "\n")
    verified = service_state(release)
    print(json.dumps({"record": str(path), "record_sha256": sha256(path), "service": verified}, indent=1))
    return 0 if verified["runs_release"] else 1


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
    promotion = commands.add_parser("promote", help="record a qualified summary's release")
    promotion.add_argument("--summary", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "macos":
        def interrupted(signum, frame):
            raise KeyboardInterrupt
        signal.signal(signal.SIGTERM, interrupted)
        try:
            return protocol(args)
        except KeyboardInterrupt:
            settle_live()
            print("measure: interrupted; every started owner and probe was settled and nothing is promoted",
                  flush=True)
            return 130
    if args.command == "fixture-check":
        return fixture_check(args)
    return promote(args)


if __name__ == "__main__":
    sys.exit(main())
