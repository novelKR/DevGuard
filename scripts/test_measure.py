"""The SLO protocol's analysis: percentiles, missing samples, validity and verdicts."""
import math
import unittest

import measure


def status(latency=10.0, outcome="answered", code=None):
    sample = {"kind": "status", "outcome": outcome, "latency_ms": latency}
    if code:
        sample["code"] = code
    return sample


def termination(ack=20.0, released=60.0, outcome="acknowledged", code=None):
    sample = {"kind": "terminate", "outcome": outcome, "ack_ms": ack, "released_ms": released}
    if code:
        sample["code"] = code
    return sample


def painted(stamp, latency, kind="keydown"):
    return {"type": kind, "stamp": stamp, "handled": stamp + 1, "paint": stamp + latency}


def passing_metrics(kind="idle"):
    metrics = {
        "input": measure.input_metrics([painted(i * 500.0, 20.0) for i in range(100)], [], 100),
        "input_expected": 100,
        "frames": measure.frame_metrics([i * 16.7 for i in range(1000)]),
        "status": measure.status_metrics([status() for _ in range(50)], 50),
        "termination": measure.termination_metrics([termination() for _ in range(5)], 5),
        "journal": {"charged_after": {}},
    }
    if kind == "load":
        metrics["load"] = measure.load_metrics(
            [{"consumer": "cpu", "result": "completed", "started": 0.0, "ended": 60.0, "running_from": 0.0,
              "exit_code": 0, "launches": 1}], 0.0, 60.0)
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
        self.assertFalse(metrics["passed"])

    def test_unhandled_and_unpainted_inputs_are_missing_and_fail(self):
        inputs = [painted(i * 500.0, 10.0) for i in range(98)]
        inputs.append({"type": "wheel", "stamp": 1.0, "handled": 2.0, "paint": None})
        metrics = measure.input_metrics(inputs, [], 100)
        self.assertEqual(metrics["missing"], 2)
        self.assertEqual(metrics["over_limit"], 2)
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
        frames = [0.0, 16.7, 33.4, 540.0, 556.7]
        metrics = measure.frame_metrics(frames)
        self.assertEqual(metrics["stalls"], 1)
        self.assertFalse(metrics["passed"])
        self.assertTrue(measure.frame_metrics([0.0, 499.0, 998.0])["passed"])

    def test_no_frames_is_not_a_pass(self):
        self.assertFalse(measure.frame_metrics([])["passed"])


class Control(unittest.TestCase):
    def test_status_errors_are_missing_samples_and_transport_errors_are_losses(self):
        samples = [status() for _ in range(99)] + [status(outcome="error", code="resource_control_unavailable")]
        metrics = measure.status_metrics(samples, 100)
        self.assertEqual(metrics["connection_losses"], 1)
        self.assertFalse(metrics["passed"])

    def test_slow_status_fails_its_target(self):
        samples = [status(600.0) for _ in range(2)] + [status() for _ in range(98)]
        self.assertFalse(measure.status_metrics(samples, 100)["passed"])
        self.assertTrue(measure.status_metrics([status(499.0)], 1)["passed"])

    def test_termination_release_is_reported_but_an_unreleased_target_fails(self):
        metrics = measure.termination_metrics([termination(ack=50.0, released=5000.0)], 1)
        self.assertTrue(metrics["passed"])
        self.assertEqual(metrics["release_p99_ms"], 5000.0)
        metrics = measure.termination_metrics([termination(released=None)], 1)
        self.assertEqual(metrics["unreleased"], 1)
        self.assertFalse(metrics["passed"])

    def test_unstarted_terminations_are_not_samples(self):
        samples = [termination(), {"kind": "terminate", "outcome": "not_started", "note": "pressure"}]
        metrics = measure.termination_metrics(samples, 2)
        self.assertEqual(metrics["samples"], 1)
        self.assertEqual(metrics["not_started"], 1)


class Load(unittest.TestCase):
    def test_applied_load_merges_overlapping_runs(self):
        runs = [
            {"consumer": "a", "result": "completed", "started": 0.0, "ended": 40.0, "running_from": 10.0,
             "exit_code": 0, "launches": 1},
            {"consumer": "b", "result": "completed", "started": 0.0, "ended": 60.0, "running_from": 30.0,
             "exit_code": 0, "launches": 1},
            {"consumer": "b", "result": "not_started", "started": 60.0, "ended": 61.0, "reason": "denied"},
        ]
        metrics = measure.load_metrics(runs, 0.0, 100.0)
        self.assertEqual(metrics["applied_fraction"], 0.5)
        self.assertEqual(metrics["consumers"]["b"]["refusal_reasons"], {"denied": 1})

    def test_uncertain_runs_and_losses_are_counted(self):
        runs = [{"consumer": "a", "result": "uncertain", "started": 0.0, "ended": 1.0, "connection_loss": True}]
        metrics = measure.load_metrics(runs, 0.0, 1.0)
        self.assertEqual((metrics["uncertain"], metrics["connection_losses"]), (1, 1))


class Verdicts(unittest.TestCase):
    def test_an_interval_meeting_every_target_passes(self):
        self.assertEqual(measure.interval_verdict("idle", [], passing_metrics()), ("pass", []))
        self.assertEqual(measure.interval_verdict("load", [], passing_metrics("load")), ("pass", []))

    def test_invalid_observation_is_inconclusive_even_when_targets_fail(self):
        metrics = passing_metrics()
        metrics["status"] = measure.status_metrics([status(900.0) for _ in range(50)], 50)
        verdict, reasons = measure.interval_verdict("idle", ["the screen was locked"], metrics)
        self.assertEqual(verdict, "inconclusive")
        self.assertIn("the screen was locked", reasons)

    def test_too_few_samples_is_inconclusive(self):
        metrics = passing_metrics()
        metrics["status"] = measure.status_metrics([status() for _ in range(10)], 50)
        self.assertEqual(measure.interval_verdict("idle", [], metrics)[0], "inconclusive")

    def test_load_that_barely_ran_is_inconclusive(self):
        metrics = passing_metrics("load")
        metrics["load"]["applied_fraction"] = 0.2
        self.assertEqual(measure.interval_verdict("load", [], metrics)[0], "inconclusive")

    def test_a_missed_target_fails(self):
        metrics = passing_metrics("load")
        metrics["frames"] = measure.frame_metrics([0.0, 700.0])
        self.assertEqual(measure.interval_verdict("load", [], metrics), ("fail", ["frames"]))

    def test_charged_attempts_after_an_interval_fail(self):
        metrics = passing_metrics()
        metrics["journal"]["charged_after"] = {"suspect": 1}
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


class Validity(unittest.TestCase):
    def test_the_fixture_must_stay_frontmost_and_the_screen_unlocked(self):
        rows = [{"front_pid": 7, "fixture_pid": 7, "locked": False}]
        self.assertEqual(measure.validity_reasons(rows, False), [])
        rows.append({"front_pid": 8, "fixture_pid": 7, "locked": True})
        self.assertEqual(len(measure.validity_reasons(rows, False)), 2)
        self.assertEqual(measure.validity_reasons([], False), ["no validity sample"])


if __name__ == "__main__":
    unittest.main()
