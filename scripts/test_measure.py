"""The SLO protocol's analysis: percentiles, missing samples, validity, load, verdicts and the
recomputation promotion relies on."""
import json
import math
from pathlib import Path
import tempfile
import unittest

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

    def test_no_frames_is_not_a_pass(self):
        self.assertFalse(measure.frame_metrics([])["passed"])


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

    def test_the_service_must_run_the_release_under_one_pid(self):
        self.assertEqual(measure.service_reasons([{"runs_release": True, "service_pid": 3}], 3), [])
        self.assertTrue(measure.service_reasons([{"runs_release": True, "service_pid": 4}], 3))
        self.assertTrue(measure.service_reasons([], 3))


class Promotion(unittest.TestCase):
    """verify_run recomputes the verdict from the preserved reports."""

    def run_directory(self, directory, *, rehearsal=False, tamper=False, plan=None, verdicts=None):
        out = Path(directory)
        plan = plan or measure.PROTOCOL
        header = {"rehearsal": rehearsal, "headless": False, "approved_target": True, "plan": plan}
        (out / "run.json").write_text(json.dumps(header))
        combinations = {}
        for combination in plan["combinations"]:
            repetitions = []
            for repetition in range(1, plan["repetitions"] + 1):
                verdict = (verdicts or {}).get((combination, repetition), "pass")
                interval = {"verdict": verdict, "reasons": []}
                report = {"verdict": verdict, "reasons": [], "intervals": {"idle": interval, "load": interval}}
                (out / f"{combination}-{repetition}").mkdir()
                path = out / f"{combination}-{repetition}/report.json"
                path.write_text(json.dumps(report))
                repetitions.append({"repetition": repetition, "verdict": verdict,
                                    "report_sha256": measure.sha256(path)})
            combinations[combination] = {"verdict": measure.combination_verdict(
                [r["verdict"] for r in repetitions], plan["repetitions"]), "repetitions": repetitions}
        verdict = measure.overall_verdict(combinations, plan, header)
        summary = {"verdict": verdict, "combinations": combinations, "run_sha256": measure.sha256(out / "run.json")}
        (out / "summary.json").write_text(json.dumps(summary))
        if tamper:
            path = out / "cold-1/report.json"
            path.write_text(path.read_text().replace('"reasons": []', '"reasons": ["edited"]', 1))
        return out

    def test_a_qualified_run_verifies(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, reasons = measure.verify_run(self.run_directory(directory))
        self.assertEqual(reasons, [])

    def test_a_changed_report_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, reasons = measure.verify_run(self.run_directory(directory, tamper=True))
        self.assertTrue(any("report missing or changed" in reason for reason in reasons))

    def test_a_rehearsal_a_partial_plan_or_a_failed_repetition_is_refused(self):
        cases = [{"rehearsal": True}, {"plan": {**measure.PROTOCOL, "repetitions": 1}},
                 {"verdicts": {("warm", 2): "fail"}}]
        for case in cases:
            with tempfile.TemporaryDirectory() as directory:
                _, _, reasons = measure.verify_run(self.run_directory(directory, **case))
            self.assertTrue(reasons, case)

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
            _, _, reasons = measure.verify_run(out)
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
