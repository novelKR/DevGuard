"""The SLO protocol's analysis: percentiles, missing samples, validity, load, verdicts, the journal
checks and the recomputation promotion relies on."""
import json
import math
from pathlib import Path
import sqlite3
import tempfile
import time
import types
import unittest
from unittest import mock

import measure


def status(latency=10.0, outcome="answered", code=None, phase="run_authorized"):
    sample = {"kind": "status", "outcome": outcome, "latency_ms": latency, "phase": phase}
    if code:
        sample["code"] = code
    return sample


def termination(ack=20.0, exited=40.0, released=60.0, signalled=1, complete=True, killed=False,
                outcome="acknowledged", code=None):
    sample = {"kind": "terminate", "outcome": outcome, "ack_ms": ack, "exited_ms": exited,
              "released_ms": released, "signalled": signalled, "complete": complete, "killed_by_probe": killed}
    if code:
        sample["code"] = code
    return sample


def painted(stamp, latency, kind="keydown"):
    return {"type": kind, "stamp": stamp, "handled": stamp + 1, "paint": stamp + latency}


def run(consumer, started=0.0, ended=60.0, result="completed", exit_code=0, running_from=0.0, **extra):
    return {"consumer": consumer, "result": result, "started": started, "ended": ended,
            "running_from": running_from, "exit_code": exit_code, **extra}


CONSUMERS = ["cargo", "cpu", "memory", "io", "output", "slow-input"]


def passing_metrics(kind="idle"):
    metrics = {
        "input": measure.input_metrics([painted(i * 500.0, 20.0) for i in range(100)], [], 100),
        "input_expected": 100,
        "frames": measure.frame_metrics([i * 16.7 for i in range(1000)]),
        "fixture": {"crashed": False},
        "status": measure.status_metrics([status() for _ in range(50)], 50),
        "termination": measure.termination_metrics([termination() for _ in range(5)], 5),
        "journal": {"still_charged": [], "duplicate_launches": 0},
        "service": {"failure": None},
    }
    if kind == "load":
        metrics["load"] = measure.load_metrics([run(name) for name in CONSUMERS], 0.0, 60.0, CONSUMERS)
    return metrics


class Percentiles(unittest.TestCase):
    def test_nearest_rank_uses_the_ceiling_rank(self):
        values = list(range(1, 101))
        self.assertEqual(measure.nearest_rank(values, 0.99), 99)
        self.assertEqual(measure.nearest_rank(values, 1.0), 100)
        self.assertEqual(measure.nearest_rank([5], 0.99), 5)
        self.assertIsNone(measure.nearest_rank([], 0.99))

    def test_a_missing_sample_ranks_above_every_value(self):
        self.assertEqual(measure.nearest_rank([1] * 99, 0.99, missing=1), 1)
        self.assertTrue(math.isinf(measure.nearest_rank([1] * 98, 0.99, missing=2)))
        self.assertEqual(measure.number(math.inf), "missing")


class Inputs(unittest.TestCase):
    def test_event_timing_raises_the_page_estimate(self):
        inputs = [painted(100.0, 20.0)]
        timing = [{"name": "keydown", "start": 100.0, "duration": 150.0}]
        metrics = measure.input_metrics(inputs, timing, 1)
        self.assertEqual(metrics["p99_ms"], 150.0)
        self.assertEqual(metrics["event_timing_matched"], 1)
        self.assertFalse(metrics["passed"])

    def test_unhandled_and_unpainted_inputs_are_missing_and_fail(self):
        inputs = [painted(i * 500.0, 10.0) for i in range(98)]
        inputs.append({"type": "wheel", "stamp": 1.0, "handled": 2.0, "paint": None})
        metrics = measure.input_metrics(inputs, [], 100)
        self.assertEqual(metrics["missing"], 2)
        self.assertEqual(metrics["over_limit"], 2)
        self.assertFalse(metrics["passed"])

    def test_slots_skipped_behind_a_slow_dispatch_are_missing(self):
        inputs = [painted(i * 500.0, 10.0) for i in range(100)]
        metrics = measure.input_metrics(inputs, [], 100, skipped=3)
        self.assertEqual((metrics["scheduled"], metrics["missing"]), (103, 3))
        self.assertFalse(metrics["passed"])

    def test_a_response_over_a_second_fails_even_below_the_p99_target(self):
        inputs = [painted(i * 500.0, 10.0) for i in range(199)] + [painted(1.0e6, 1200.0)]
        metrics = measure.input_metrics(inputs, [], 200)
        self.assertLessEqual(metrics["p99_ms"], 100)
        self.assertEqual(metrics["over_limit"], 1)
        self.assertFalse(metrics["passed"])

    def test_fast_inputs_pass(self):
        self.assertTrue(measure.input_metrics([painted(i * 500.0, 30.0) for i in range(10)], [], 10)["passed"])


class Frames(unittest.TestCase):
    def test_a_gap_over_half_a_second_is_a_stall(self):
        metrics = measure.frame_metrics([0.0, 16.7, 33.4, 540.0, 556.7])
        self.assertEqual(metrics["stalls"], 1)
        self.assertFalse(metrics["passed"])
        self.assertTrue(measure.frame_metrics([0.0, 499.0, 998.0])["passed"])

    def test_frames_from_out_of_order_drains_are_sorted(self):
        self.assertTrue(measure.frame_metrics([400.0, 0.0, 200.0, 600.0])["passed"])

    def test_no_frames_is_unmeasured_not_a_pass(self):
        self.assertIsNone(measure.frame_metrics([])["passed"])


class Status(unittest.TestCase):
    def test_errors_are_missing_samples_and_transport_errors_are_losses(self):
        samples = [status() for _ in range(99)] + [status(outcome="error", code="resource_control_unavailable")]
        metrics = measure.status_metrics(samples, 100)
        self.assertEqual(metrics["connection_losses"], 1)
        self.assertFalse(metrics["passed"])

    def test_slow_status_fails_its_target(self):
        samples = [status(600.0) for _ in range(2)] + [status() for _ in range(98)]
        self.assertFalse(measure.status_metrics(samples, 100)["passed"])
        self.assertTrue(measure.status_metrics([status(499.0)], 1)["passed"])

    def test_an_answer_about_a_target_that_no_longer_runs_is_missing(self):
        metrics = measure.status_metrics([status(phase="released")] + [status() for _ in range(9)], 10)
        self.assertEqual((metrics["not_running"], metrics["samples"]), (1, 10))
        self.assertFalse(metrics["passed"])

    def test_missed_slots_count_as_samples_and_fail(self):
        samples = [status() for _ in range(8)] + [{"kind": "status", "outcome": "missed_slot"}] * 2
        metrics = measure.status_metrics(samples, 10)
        self.assertEqual((metrics["missed_slots"], metrics["samples"]), (2, 10))
        self.assertFalse(metrics["passed"])


class Terminations(unittest.TestCase):
    def test_effective_terminations_pass_and_report_exit_and_release_separately(self):
        metrics = measure.termination_metrics([termination(ack=50.0, exited=80.0, released=5000.0)], 1)
        self.assertTrue(metrics["passed"])
        self.assertEqual((metrics["exit_max_ms"], metrics["release_p99_ms"]), (80.0, 5000.0))

    def test_an_acknowledgement_that_ended_nothing_fails(self):
        for sample in (termination(signalled=0), termination(complete=False), termination(exited=None),
                       termination(killed=True)):
            metrics = measure.termination_metrics([termination()] * 20 + [sample], 21)
            self.assertEqual(metrics["ineffective"], 1, sample)
            self.assertFalse(metrics["passed"], sample)

    def test_an_unreleased_effective_target_fails(self):
        metrics = measure.termination_metrics([termination(released=None)], 1)
        self.assertEqual(metrics["unreleased"], 1)
        self.assertFalse(metrics["passed"])

    def test_start_failures_are_errors_and_transport_failures_are_losses(self):
        failed = {"kind": "terminate", "outcome": "error", "stage": "begin_launch",
                  "code": "resource_control_unavailable"}
        metrics = measure.termination_metrics([termination()] * 9 + [failed], 10)
        self.assertEqual((metrics["errors"], metrics["connection_losses"]), (1, 1))
        self.assertFalse(metrics["passed"])

    def test_targets_refused_for_pressure_are_not_samples(self):
        samples = [termination(), {"kind": "terminate", "outcome": "not_started", "note": "pressure"}]
        metrics = measure.termination_metrics(samples, 2)
        self.assertEqual((metrics["samples"], metrics["not_started"]), (1, 1))
        self.assertTrue(metrics["passed"])

    def test_only_refused_targets_measure_nothing_rather_than_fail(self):
        refused = [{"kind": "terminate", "outcome": "not_started", "note": "critical"}] * 5
        metrics = measure.termination_metrics(refused, 5)
        self.assertIsNone(metrics["passed"])
        interval = passing_metrics("load")
        interval["termination"] = metrics
        verdict, reasons = measure.interval_verdict("load", [], interval)
        self.assertEqual(verdict, "inconclusive")
        self.assertIn("termination: nothing was measured", reasons)

    def test_slots_passed_while_a_target_settled_are_not_samples(self):
        samples = [termination()] * 4 + [{"kind": "terminate", "outcome": "not_sampled"}]
        metrics = measure.termination_metrics(samples, 5)
        self.assertEqual((metrics["samples"], metrics["not_sampled"]), (4, 1))
        self.assertTrue(metrics["passed"])


class Load(unittest.TestCase):
    def test_every_consumer_must_run_and_succeed(self):
        runs = [run(name) for name in CONSUMERS if name != "cargo"]
        runs.append(run("cargo", result="completed", exit_code=101))
        metrics = measure.load_metrics(runs, 0.0, 60.0, CONSUMERS)
        self.assertIn("cargo: no successful run", metrics["issues"])
        self.assertIn("cargo: 1 nonzero exit", metrics["issues"])
        missing = measure.load_metrics([run(name) for name in CONSUMERS[1:]], 0.0, 60.0, CONSUMERS)
        self.assertIn("cargo: no successful run", missing["issues"])

    def test_each_consumer_needs_its_own_share_of_the_interval(self):
        runs = [run(name) for name in CONSUMERS[1:]] + [run("cargo", running_from=50.0)]
        metrics = measure.load_metrics(runs, 0.0, 60.0, CONSUMERS)
        self.assertEqual(metrics["applied_fraction"], 1.0)
        self.assertTrue(any(issue.startswith("cargo: ran for") for issue in metrics["issues"]))

    def test_overlapping_runs_merge_into_the_applied_share(self):
        runs = [run("a", ended=40.0, running_from=10.0), run("b", ended=60.0, running_from=30.0)]
        self.assertEqual(measure.load_metrics(runs, 0.0, 100.0)["applied_fraction"], 0.5)

    def test_losses_uncertain_runs_and_duplicate_launches_fail(self):
        runs = [run(name) for name in CONSUMERS]
        runs.append(run("cpu", result="uncertain", running_from=None, connection_loss=True))
        runs.append(run("io", launched_attempts=2))
        metrics = measure.load_metrics(runs, 0.0, 60.0, CONSUMERS)
        self.assertEqual((metrics["connection_losses"], metrics["uncertain"], metrics["duplicate_launches"]),
                         (1, 1, 1))
        self.assertEqual(len(metrics["failures"]), 3)

    def test_exec_failures_and_unknown_results_make_the_load_undeclared(self):
        runs = [run(name) for name in CONSUMERS]
        runs += [run("io", result="exec_failed"), run("output", result="unknown", running_from=None)]
        issues = measure.load_metrics(runs, 0.0, 60.0, CONSUMERS)["issues"]
        self.assertIn("io: 1 exec failed", issues)
        self.assertIn("output: 1 unknown", issues)


class Journal(unittest.TestCase):
    """The journal checks read a real SQLite journal with the service's attempts table."""

    def journal(self, directory, rows):
        path = Path(directory) / "authority.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE attempts (consumer TEXT NOT NULL, generation TEXT NOT NULL, "
                           "attempt TEXT NOT NULL, charged INTEGER NOT NULL, record TEXT NOT NULL, launch_hash TEXT)")
        for attempt, charged, launched, phase, reason in rows:
            connection.execute("INSERT INTO attempts VALUES ('dev-cli', 'g', ?, ?, ?, ?)",
                               (attempt, charged, json.dumps({"phase": phase, "release_reason": reason}),
                                "h" if launched else None))
        connection.commit()
        connection.close()
        return path

    def test_a_grant_released_as_never_started_is_not_a_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.journal(directory, [("a0", 0, True, "released", "no_helper_created"),
                                            ("a1", 0, True, "released", "scope_terminated")])
            runs = [{"attempt_ids": ["a0", "a1"]}]
            journal = measure.settle_journal({"a0", "a1"}, runs, database=path, limit=0)
        self.assertEqual((journal["duplicate_launches"], runs[0]["launched_attempts"]), (0, 1))

    def test_two_executed_attempts_in_one_run_are_a_duplicate(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.journal(directory, [("a0", 0, True, "released", "scope_terminated"),
                                            ("a1", 0, True, "released", "scope_terminated")])
            runs = [{"attempt_ids": ["a0", "a1"]}]
            journal = measure.settle_journal({"a0", "a1"}, runs, database=path, limit=0)
        self.assertEqual(journal["duplicate_launches"], 1)

    def test_attempts_still_charged_are_named(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.journal(directory, [("q-1", 1, True, "run_authorized", None),
                                            ("q-2", 0, True, "released", "scope_terminated")])
            journal = measure.settle_journal({"q-1", "q-2"}, [], database=path, limit=0)
        self.assertEqual(journal["still_charged"], ["q-1"])

    def test_an_unreadable_journal_is_reported(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = measure.settle_journal({"q-1"}, [], database=Path(directory) / "missing.sqlite", limit=0)
        self.assertIn("error", journal)


class LateReplies(unittest.TestCase):
    """A drain reply that arrives after its request timed out keeps its data; others do not count."""

    def fixture(self):
        fixture = object.__new__(measure.Fixture)
        fixture.pending, fixture.late, fixture.drain_ids, fixture.unreadable_replies = set(), {}, set(), 0
        return fixture

    def test_late_drain_replies_are_absorbed_and_others_dropped(self):
        fixture = self.fixture()
        value = json.dumps({"frames": [1.0]})
        fixture.drain_ids = {7, 9}
        fixture.late = {7: {"id": 7, "result": {"result": {"value": value}}},
                        8: {"id": 8, "result": {}},
                        9: {"id": 9, "error": {"message": "context destroyed"}}}
        self.assertEqual(fixture.late_drains(), [{"frames": [1.0]}])
        self.assertEqual(fixture.late, {})

    def test_replies_owed_from_an_earlier_interval_are_forgotten(self):
        fixture = self.fixture()
        fixture.pending, fixture.late, fixture.drain_ids = {3}, {4: {}}, {3, 4}
        self.assertEqual(fixture.reset_replies(), 2)
        self.assertEqual((fixture.pending, fixture.late, fixture.drain_ids), (set(), {}, set()))


class EvidenceWriter(unittest.TestCase):
    """Sampling threads hand rows to one writer thread per interval and never wait on the disk."""

    def test_rows_are_written_in_order_per_file_and_close_reports_them_once(self):
        with tempfile.TemporaryDirectory() as directory:
            writer = measure.Writer()
            first, second = Path(directory) / "a.jsonl", Path(directory) / "b.jsonl"
            for index in range(200):
                writer.write(first if index % 2 else second, {"index": index})
            report = writer.close()
            rows = [json.loads(line)["index"] for line in first.read_text().splitlines()]
            again = writer.close()
        self.assertEqual((report["lines"], report["error_count"], report["errors"]), (200, 0, []))
        self.assertEqual(rows, list(range(1, 200, 2)))
        self.assertGreaterEqual(report["lag_max_ms"], 0)
        self.assertEqual(again, report)

    def test_every_line_that_cannot_be_written_is_counted_and_others_continue(self):
        with tempfile.TemporaryDirectory() as directory:
            writer = measure.Writer()
            for index in range(3):
                writer.write(Path(directory) / "missing" / "x.jsonl", {"a": index})
            writer.write(Path(directory) / "ok.jsonl", {"b": 2})
            report = writer.close()
            written = (Path(directory) / "ok.jsonl").read_text()
        self.assertEqual((report["lines"], report["error_count"], len(report["errors"])), (1, 3, 3))
        self.assertIn('"b": 2', written)

    def test_the_errors_kept_are_bounded_but_all_are_counted(self):
        with tempfile.TemporaryDirectory() as directory:
            writer = measure.Writer()
            for index in range(measure.Writer.ERRORS_KEPT + 10):
                writer.write(Path(directory) / "missing" / "x.jsonl", {"a": index})
            report = writer.close()
        self.assertEqual(report["error_count"], measure.Writer.ERRORS_KEPT + 10)
        self.assertEqual(len(report["errors"]), measure.Writer.ERRORS_KEPT)

    def test_a_slow_disk_delays_the_evidence_never_the_sampler(self):
        class Slow:
            def __init__(self, *args):
                self.lines = []

            def write(self, line):
                time.sleep(0.05)
                self.lines.append(line)

            def flush(self):
                pass

            def close(self):
                pass

        with mock.patch.object(measure, "open", Slow, create=True):
            writer = measure.Writer()
            began = time.monotonic()
            for index in range(40):
                writer.write("slow.jsonl", {"index": index})
            handed = time.monotonic() - began
            report = writer.close()
        # Forty lines take the disk two seconds; handing them over takes a moment.
        self.assertLess(handed, 0.5)
        self.assertEqual((report["lines"], report["error_count"]), (40, 0))
        self.assertGreaterEqual(report["lag_max_ms"], 50)

    def test_a_writer_that_does_not_finish_is_an_error_not_a_wait(self):
        class Stuck:
            def __init__(self, *args):
                pass

            def write(self, line):
                time.sleep(2)

            def flush(self):
                pass

            def close(self):
                pass

        with mock.patch.object(measure, "open", Stuck, create=True):
            writer = measure.Writer()
            writer.write("stuck.jsonl", {"index": 0})
            began = time.monotonic()
            report = writer.close(limit=0.2)
        self.assertLess(time.monotonic() - began, 1.5)
        self.assertEqual(report["error_count"], 1)
        self.assertIn("did not finish", report["errors"][0])


class Staging(unittest.TestCase):
    """The probe's binary and the fixture's profile run from a stage recorded with its disk."""

    DF = "Filesystem 512-blocks Used Available Capacity Mounted on\n{device} 100 50 50 50% {mount}\n"
    DISKUTIL = ("   Device Node:               {device}\n   Protocol:                  {protocol}\n"
                "   Device Location:           {location}\n   APFS Physical Store:       {store}\n")

    def host(self, volumes):
        outputs = {}
        for path, (device, mount, protocol, location, store) in volumes.items():
            outputs[("/bin/df", "-P", path)] = self.DF.format(device=device, mount=mount)
            outputs[("/usr/sbin/diskutil", "info", device)] = self.DISKUTIL.format(
                device=device, protocol=protocol, location=location, store=store)
        return mock.patch.object(measure, "run_text", lambda *args, timeout=10: outputs.get(args))

    def test_a_placement_names_its_mount_and_physical_disk(self):
        with self.host({"/w": ("/dev/disk7s1", "/Volumes/Dev Data", "USB", "External", "disk6s2")}):
            found = measure.placement("/w")
        self.assertEqual(found, {"device": "/dev/disk7s1", "mount": "/Volumes/Dev Data", "physical_disk": "disk6",
                                 "protocol": "USB", "location": "External"})

    def test_the_stage_is_flagged_when_it_shares_the_load_disk(self):
        internal = ("/dev/disk3s5", "/System/Volumes/Data", "Apple Fabric", "Internal", "disk0s2")
        external = ("/dev/disk7s1", "/Volumes/DevData", "USB", "External", "disk6s2")
        with self.host({"/s": internal, "/w": external, "/o": external}):
            apart = measure.placements(Path("/s"), Path("/w"), Path("/o"))
        with self.host({"/s": internal, "/w": internal, "/o": internal}):
            shared = measure.placements(Path("/s"), Path("/w"), Path("/o"))
        with self.host({"/s": internal}):
            unknown = measure.placements(Path("/s"), Path("/w"), Path("/o"))
        self.assertIs(apart["stage_shares_load_disk"], False)
        self.assertEqual(apart["evidence"]["physical_disk"], "disk6")
        self.assertIs(shared["stage_shares_load_disk"], True)
        self.assertIsNone(unknown["stage_shares_load_disk"])

    def test_the_staged_binary_must_be_the_one_hashed(self):
        with tempfile.TemporaryDirectory() as directory:
            binary, stage = Path(directory) / "qualify", Path(directory) / "stage"
            binary.write_bytes(b"probe")
            stage.mkdir()
            staged, digest = measure.stage_binary(binary, stage, measure.sha256(binary))
            self.assertEqual((staged.read_bytes(), digest), (b"probe", measure.sha256(binary)))
            with self.assertRaises(SystemExit):
                measure.stage_binary(binary, stage, "0" * 64)

    def test_a_warm_run_touches_the_core_crate_and_a_cold_run_builds_fresh(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            args = types.SimpleNamespace(qualify_bin=work / "devguard-qualify")
            release = types.SimpleNamespace(devguard=work / "devguard")
            warm = measure.consumers(args, release, "warm", work, work, 0, 1, False)[0]
            cold = measure.consumers(args, release, "cold", work, work, 0, 2, False)[0]
            (work / measure.TARGETS / "cold-2-004").mkdir(parents=True)
            self.assertEqual(warm.before(1)["env"]["CARGO_TARGET_DIR"], str(work / measure.TARGETS / "warm"))
            self.assertEqual(cold.before(3)["env"]["CARGO_TARGET_DIR"],
                             str(work / measure.TARGETS / "cold-2-003"))
            with self.assertRaises(RuntimeError):
                cold.before(4)
        self.assertTrue(warm.command[2].startswith("/usr/bin/touch crates/core/src/lib.rs && cargo build "))
        self.assertTrue(cold.command[2].startswith("cargo build "))
        self.assertEqual(warm.options[:2], ["--adapter", "cargo-pipeline"])


class Receipts(unittest.TestCase):
    def receipt(self, directory, body):
        path = Path(directory) / "r.receipt.json"
        if body is not None:
            path.write_text(json.dumps(body))
        return path

    def test_a_missing_receipt_leaves_the_run_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            result = measure.summarize_run("io", self.receipt(directory, None), 125, 0.0, 1.0, 0)
        self.assertEqual(result["result"], "unknown")

    def test_a_completed_receipt_gives_its_attempts_wait_and_exit(self):
        key = {"consumer_id": "dev-cli", "consumer_generation": "g", "attempt_id": "a1"}
        body = {"result": "completed", "wait": {"waited_ms": 1500},
                "attempts": [{"key": {**key, "attempt_id": "a0"}, "note": "admission denied: Some(ResourceUnavailable)"},
                             {"key": key, "note": "admitted"}],
                "observed_after_reap": {"key": key}, "exit": {"code": 0}}
        with tempfile.TemporaryDirectory() as directory:
            result = measure.summarize_run("io", self.receipt(directory, body), 0, 10.0, 20.0, 0)
        self.assertEqual(result["attempt_ids"], ["a0", "a1"])
        self.assertEqual(result["running_from"], 11.5)
        self.assertFalse(result["connection_loss"])

    def test_transport_notes_and_uncertain_results_are_connection_losses(self):
        with tempfile.TemporaryDirectory() as directory:
            lost = measure.summarize_run("io", self.receipt(directory, {
                "result": "not_started", "reason": "cannot use the authority: ResourceControlUnavailable: gone",
                "attempts": []}), 125, 0.0, 1.0, 0)
            uncertain = measure.summarize_run("io", self.receipt(directory, {"result": "uncertain", "attempts": [],
                                                                            "exit": {"signal": 15}}), 0, 0.0, 1.0, 0)
        self.assertTrue(lost["connection_loss"])
        self.assertIsNone(lost["running_from"])
        self.assertTrue(uncertain["connection_loss"])
        self.assertEqual(uncertain["exit_signal"], 15)


class Verdicts(unittest.TestCase):
    def test_an_interval_meeting_every_target_passes(self):
        self.assertEqual(measure.interval_verdict("idle", [], passing_metrics()), ("pass", []))
        self.assertEqual(measure.interval_verdict("load", [], passing_metrics("load")), ("pass", []))

    def test_invalid_observation_is_inconclusive_even_when_targets_fail(self):
        metrics = passing_metrics()
        metrics["status"] = measure.status_metrics([status(900.0) for _ in range(50)], 50)
        verdict, reasons = measure.interval_verdict("idle", ["the screen was locked"], metrics)
        self.assertEqual((verdict, reasons), ("inconclusive", ["the screen was locked"]))

    def test_a_missed_target_fails_before_a_sample_shortfall_is_considered(self):
        metrics = passing_metrics("load")
        metrics["frames"] = measure.frame_metrics([0.0, 700.0])
        metrics["status"] = measure.status_metrics([status() for _ in range(10)], 50)
        self.assertEqual(measure.interval_verdict("load", [], metrics), ("fail", ["frames"]))

    def test_too_few_samples_or_an_undeclared_load_is_inconclusive(self):
        metrics = passing_metrics()
        metrics["status"] = measure.status_metrics([status() for _ in range(10)], 50)
        self.assertEqual(measure.interval_verdict("idle", [], metrics)[0], "inconclusive")
        metrics = passing_metrics("load")
        metrics["load"] = measure.load_metrics([run(name) for name in CONSUMERS[1:]], 0.0, 60.0, CONSUMERS)
        self.assertEqual(measure.interval_verdict("load", [], metrics)[0], "inconclusive")

    def test_a_service_restart_and_an_ineffective_status_target_fail(self):
        metrics = passing_metrics()
        metrics["service"]["failure"] = "the measured release restarted or was unhealthy during the interval"
        self.assertEqual(measure.interval_verdict("idle", [], metrics)[0], "fail")
        metrics = passing_metrics()
        metrics["termination"]["status_target_effective"] = False
        self.assertEqual(measure.interval_verdict("idle", [], metrics)[0], "fail")

    def test_load_failures_a_crash_and_charged_attempts_fail(self):
        metrics = passing_metrics("load")
        metrics["load"]["failures"] = ["1 load runs are uncertain"]
        self.assertEqual(measure.interval_verdict("load", [], metrics)[0], "fail")
        metrics = passing_metrics()
        metrics["fixture"]["crashed"] = True
        self.assertEqual(measure.interval_verdict("idle", [], metrics)[0], "fail")
        metrics = passing_metrics()
        metrics["journal"]["still_charged"] = ["q-1"]
        self.assertEqual(measure.interval_verdict("idle", [], metrics)[0], "fail")

    def test_a_failing_idle_baseline_makes_the_repetition_inconclusive(self):
        idle = {"verdict": "fail", "reasons": ["input"]}
        load = {"verdict": "pass", "reasons": []}
        self.assertEqual(measure.repetition_verdict(idle, load)[0], "inconclusive")
        self.assertEqual(measure.repetition_verdict({"verdict": "pass", "reasons": []},
                                                    {"verdict": "fail", "reasons": ["status"]}),
                         ("fail", ["status"]))

    def test_a_combination_needs_every_repetition_to_pass(self):
        self.assertEqual(measure.combination_verdict(["pass"] * 3, 3), "qualified")
        self.assertEqual(measure.combination_verdict(["pass", "pass", "inconclusive"], 3), "inconclusive")
        self.assertEqual(measure.combination_verdict(["pass", "fail", "pass"], 3), "failed")
        self.assertEqual(measure.combination_verdict(["pass"], 3), "inconclusive")
        self.assertEqual(measure.combination_verdict([], 0), "inconclusive")
        self.assertEqual(measure.combination_verdict(["pass"], 0), "inconclusive")

    def test_only_the_full_protocol_on_the_approved_target_qualifies(self):
        qualified = {"cold": {"verdict": "qualified"}, "warm": {"verdict": "qualified"}}
        header = {"rehearsal": False, "headless": False, "approved_target": True}
        self.assertEqual(measure.overall_verdict(qualified, measure.PROTOCOL, header), "qualified")
        for change in ({"rehearsal": True}, {"headless": True}, {"approved_target": False}):
            self.assertEqual(measure.overall_verdict(qualified, measure.PROTOCOL, {**header, **change}),
                             "inconclusive", change)
        for plan in ({**measure.PROTOCOL, "combinations": ["cold"]}, {**measure.PROTOCOL, "repetitions": 2},
                     {**measure.PROTOCOL, "load_s": 60}):
            self.assertEqual(measure.overall_verdict(qualified, plan, header), "inconclusive", plan)


class Validity(unittest.TestCase):
    def test_the_fixture_must_stay_frontmost_and_the_screen_unlocked(self):
        rows = [{"front_pid": 7, "fixture_pid": 7, "locked": False}]
        self.assertEqual(measure.validity_reasons(rows, False, 1), [])
        rows.append({"front_pid": 8, "fixture_pid": 7, "locked": True})
        self.assertEqual(len(measure.validity_reasons(rows, False, 2)), 2)

    def test_validity_rows_must_cover_the_interval_and_errors_invalidate(self):
        rows = [{"front_pid": 7, "fixture_pid": 7, "locked": False}] * 5
        self.assertIn("validity samples 5 of 10", measure.validity_reasons(rows, False, 10))
        self.assertIn("a validity sample could not be taken",
                      measure.validity_reasons(rows + [{"error": "TimeoutExpired"}], False, 6))

    def test_a_restart_of_the_measured_release_fails_and_another_release_is_invalid(self):
        steady = {"runs_release": True, "service_pid": 3, "current": "r1"}
        self.assertEqual(measure.service_findings([steady], "r1", 3), ([], None))
        invalid, failure = measure.service_findings([steady, {**steady, "service_pid": 4}], "r1", 3)
        self.assertEqual(invalid, [])
        self.assertIn("restarted", failure)
        invalid, failure = measure.service_findings([steady, {**steady, "runs_release": False, "current": "r2"}],
                                                    "r1", 3)
        self.assertIn("another release was selected during the interval", invalid)
        self.assertIsNone(failure)
        self.assertIn("the service was not observed", measure.service_findings([], "r1", 3)[0])
        self.assertIn("a service check could not be made",
                      measure.service_findings([steady, {"error": "TimeoutExpired"}], "r1", 3)[0])


class Promotion(unittest.TestCase):
    """verify_run recomputes the verdict from the preserved reports."""

    def run_directory(self, directory, *, rehearsal=False, tamper=False, plan=None, verdicts=None, dirty=False,
                      omit=None):
        out = Path(directory)
        plan = plan or measure.PROTOCOL
        header = {"rehearsal": rehearsal, "headless": False, "approved_target": True, "plan": plan,
                  "harness": {"dirty": dirty}}
        (out / "run.json").write_text(json.dumps(header))
        combinations = {}
        for combination in plan["combinations"]:
            repetitions = []
            for repetition in range(1, plan["repetitions"] + 1):
                verdict = (verdicts or {}).get((combination, repetition), "pass")
                (out / f"{combination}-{repetition}/idle").mkdir(parents=True)
                sample = out / f"{combination}-{repetition}/idle/host.jsonl"
                sample.write_text("{}\n")
                interval = {"verdict": verdict, "reasons": [], "raw": {"idle/host.jsonl": measure.sha256(sample)}}
                report = {"verdict": verdict, "reasons": [], "intervals": {"idle": interval, "load": interval}}
                path = out / f"{combination}-{repetition}/report.json"
                path.write_text(json.dumps(report))
                repetitions.append({"repetition": repetition, "verdict": verdict,
                                    "report_sha256": measure.sha256(path)})
            combinations[combination] = {"verdict": measure.combination_verdict(
                [r["verdict"] for r in repetitions], plan["repetitions"]), "repetitions": repetitions}
        verdict = measure.overall_verdict(combinations, plan, header)
        if omit:
            del combinations[omit]
        summary = {"verdict": verdict, "combinations": combinations, "run_sha256": measure.sha256(out / "run.json")}
        (out / "summary.json").write_text(json.dumps(summary))
        if tamper:
            path = out / "cold-1/report.json"
            path.write_text(path.read_text().replace('"reasons": []', '"reasons": ["edited"]', 1))
        return out

    def test_a_qualified_run_verifies(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, reasons = measure.verify_run(self.run_directory(directory) / "summary.json")
        self.assertEqual(reasons, [])

    def test_a_changed_report_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, reasons = measure.verify_run(self.run_directory(directory, tamper=True) / "summary.json")
        self.assertTrue(any("report missing, unlisted or changed" in reason for reason in reasons))

    def test_a_rehearsal_a_partial_plan_or_a_failed_repetition_is_refused(self):
        cases = [{"rehearsal": True}, {"plan": {**measure.PROTOCOL, "repetitions": 1}},
                 {"verdicts": {("warm", 2): "fail"}}, {"dirty": True}, {"omit": "cold"}]
        for case in cases:
            with tempfile.TemporaryDirectory() as directory:
                _, _, reasons = measure.verify_run(self.run_directory(directory, **case) / "summary.json")
            self.assertTrue(reasons, case)

    def test_changed_raw_evidence_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            out = self.run_directory(directory)
            (out / "warm-3/idle/host.jsonl").write_text("{\"edited\": true}\n")
            _, _, reasons = measure.verify_run(out / "summary.json")
        self.assertTrue(any("raw evidence missing or changed" in reason for reason in reasons))

    def test_a_pass_that_does_not_recompute_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            out = self.run_directory(directory)
            path = out / "cold-1/report.json"
            report = json.loads(path.read_text())
            report["intervals"]["idle"]["verdict"] = "fail"
            path.write_text(json.dumps(report))
            summary = json.loads((out / "summary.json").read_text())
            summary["combinations"]["cold"]["repetitions"][0]["report_sha256"] = measure.sha256(path)
            (out / "summary.json").write_text(json.dumps(summary))
            _, _, reasons = measure.verify_run(out / "summary.json")
        self.assertTrue(any("does not recompute" in reason for reason in reasons))

    def test_host_and_policy_identities_compare_what_was_measured(self):
        environment = {"build": "26A428", "cpu": "Apple M1", "logical_cpus": "8", "memory_bytes": "17179869184",
                       "thermal": "changes"}
        self.assertEqual(measure.host_identity(environment),
                         measure.host_identity({**environment, "thermal": "other"}))
        self.assertNotEqual(measure.host_identity(environment),
                            measure.host_identity({**environment, "build": "26B1"}))
        policy = {"host_toml_sha256": "a", "doctor": {"configuration_fingerprint": "f", "capabilities": ["x"]}}
        self.assertNotEqual(measure.policy_identity(policy),
                            measure.policy_identity({**policy, "host_toml_sha256": "b"}))


if __name__ == "__main__":
    unittest.main()
