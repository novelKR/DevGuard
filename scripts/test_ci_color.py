"""Terminal color in the scripts: only the lines a person reads, only on a terminal or when asked,
never in JSON or a report, and without color exactly the text the scripts wrote before."""
import contextlib
import importlib
import io
import json
import os
from pathlib import Path
import pty
import runpy
import shutil
import subprocess
import sys
import unittest
from unittest import mock
import uuid

import check_ci_results
import ci_plan
import ci_run
import devguard_color as color
import qualify

ROOT = ci_plan.ROOT
ESCAPE = "\033["
# Every script with a human-readable line, and the arguments that print its help.
SCRIPTS = {"check_ci_results.py": [], "check_docs.py": [], "ci_plan.py": [], "ci_run.py": [],
           "measure.py": ["macos"], "package.py": [], "qualify.py": [], "release.py": [], "validate.py": []}


class Terminal(io.StringIO):
    def isatty(self):
        return True


class Closed(io.StringIO):
    def isatty(self):
        raise ValueError("I/O operation on closed file")


@contextlib.contextmanager
def chosen(choice):
    previous = color._choice
    color.choose(choice)
    try:
        yield
    finally:
        color.choose(previous)


def without_color_variables(**extra):
    environment = {key: value for key, value in os.environ.items() if key not in ("NO_COLOR", "TERM")}
    return mock.patch.dict(os.environ, {**environment, **extra}, clear=True)


def script(name, *args, env=None):
    return subprocess.run([sys.executable, "-B", str(ROOT / "scripts" / name), *args], cwd=ROOT,
                          env=env, capture_output=True, text=True, timeout=120)


def on_terminal(args, env):
    """What ARGS write to standard output when it is a new pseudo-terminal."""
    master, slave = pty.openpty()
    process = subprocess.Popen(args, cwd=ROOT, env=env, stdin=subprocess.DEVNULL, stdout=slave,
                               stderr=subprocess.DEVNULL)
    os.close(slave)
    written = b""
    while True:
        try:
            data = os.read(master, 4096)
        except OSError:  # Linux ends a pseudo-terminal whose other side closed with EIO.
            break
        if not data:
            break
        written += data
    os.close(master)
    process.wait(timeout=120)
    return written.decode().replace("\r\n", "\n")


class Choice(unittest.TestCase):
    def test_nothing_is_colored_until_a_script_chooses(self):
        previous = color._choice
        self.addCleanup(color.choose, previous)
        importlib.reload(color)
        self.assertEqual(color.paint("failed", "fail", Terminal()), "failed")
        # As CodeSpace's harness does: measure.py loaded by path, with the scripts on sys.path.
        harness = runpy.run_path(str(ROOT / "scripts/measure.py"), run_name="devguard_measure")
        with mock.patch.object(sys, "stdout", Terminal()), without_color_variables(TERM="xterm"):
            self.assertEqual(harness["repetition_line"]("cold-1", "fail", ["late"]), "measure: cold-1 fail ['late']")

    def test_auto_colors_a_terminal_without_no_color_or_a_dumb_term(self):
        with chosen("auto"):
            with without_color_variables(TERM="xterm-256color"):
                self.assertTrue(color.enabled(Terminal()))
                self.assertFalse(color.enabled(io.StringIO()))
                self.assertFalse(color.enabled(Closed()))
                self.assertFalse(color.enabled(object()))
            with without_color_variables():
                self.assertTrue(color.enabled(Terminal()))
            for variables in ({"NO_COLOR": "1"}, {"NO_COLOR": ""}, {"TERM": "dumb"}):
                with self.subTest(variables=variables), without_color_variables(**variables):
                    self.assertFalse(color.enabled(Terminal()))

    def test_always_and_never_ignore_the_stream_and_the_environment(self):
        with without_color_variables(NO_COLOR="1", TERM="dumb"), chosen("always"):
            self.assertTrue(color.enabled(io.StringIO()))
        with without_color_variables(TERM="xterm"), chosen("never"):
            self.assertFalse(color.enabled(Terminal()))
        with self.assertRaises(ValueError):
            color.choose("sometimes")

    def test_each_status_word_has_the_color_of_what_it_means(self):
        with chosen("always"):
            for word, code in (("passed", "32"), ("pass", "32"), ("qualified", "32"), ("failed", "1;31"),
                               ("fail", "1;31"), ("inconclusive", "33"), ("incomplete", "33"), ("not_run", "33"),
                               ("harness_failed", "35"), ("interrupted", "35")):
                self.assertEqual(color.status(word), f"\033[{code}m{word}\033[0m")
            self.assertEqual(color.status("not_selected_by_plan"), "not_selected_by_plan")
            self.assertEqual(color.paint("details", "dim"), "\033[2mdetails\033[0m")
            self.assertEqual(color.paint("CI plan:", "bold"), "\033[1mCI plan:\033[0m")
            self.assertEqual(color.paint("", "fail"), "")

    def test_a_refusal_keeps_its_text(self):
        for choice, expected in (("never", "package requires a clean tree"),
                                 ("always", "\033[1;31mpackage requires a clean tree\033[0m")):
            with chosen(choice), self.assertRaises(SystemExit) as refused:
                color.refuse("package requires a clean tree")
            self.assertEqual(refused.exception.code, expected)
        with chosen("auto"), mock.patch.object(sys, "stderr", io.StringIO()), self.assertRaises(SystemExit) as refused:
            color.refuse("package requires a clean tree")
        self.assertEqual(refused.exception.code, "package requires a clean tree")


class PlainLines(unittest.TestCase):
    """Without color, or with auto on a pipe, every line is the line the scripts wrote before."""

    def test_the_lines_without_color_are_unchanged(self):
        plan = {"profile": "affected", "repository_stages": ["whitespace", "documentation"], "rust": False}
        for choice in ("never", "auto"):
            with self.subTest(choice=choice), chosen(choice), mock.patch.object(sys, "stdout", io.StringIO()), \
                    mock.patch.object(sys, "stderr", io.StringIO()), without_color_variables(TERM="xterm"):
                self.assertEqual(qualify.stage_line("cli-entry", {"status": "passed", "tests_passed": 12}),
                                 "cli-entry: passed (12)")
                self.assertEqual(ci_run.suite_line("cli-entry", "failed", ["exit code 1", "no report"]),
                                 "cli-entry: failed\n  - exit code 1\n  - no report")
                self.assertEqual(ci_run.suite_line("cli-entry", "passed", []), "cli-entry: passed")
                self.assertEqual(check_ci_results.verdict_lines(["a job failed", "no evidence"]),
                                 "Required checks: failed\n- a job failed\n- no evidence")
                self.assertEqual(check_ci_results.verdict_lines([]), "Required checks: passed")
                self.assertEqual(ci_plan.plan_line(plan),
                                 "CI plan: affected; repository checks whitespace, documentation; Rust not selected")

    def test_the_lines_with_color_keep_their_text(self):
        with chosen("always"):
            line = ci_run.suite_line("cli-entry", "failed", ["exit code 1"])
        self.assertEqual(line, "cli-entry: \033[1;31mfailed\033[0m\n  - \033[2mexit code 1\033[0m")
        with chosen("always"):
            line = qualify.stage_line("cli-entry", {"status": "incomplete", "tests_passed": 3})
        self.assertEqual(line, "cli-entry: \033[33mincomplete\033[0m\033[2m (3)\033[0m")


class Scripts(unittest.TestCase):
    """The scripts themselves, run as a person or CI runs them."""

    def setUp(self):
        self.outputs = []

    def tearDown(self):
        for output in self.outputs:
            shutil.rmtree(output, ignore_errors=True)

    def validate_args(self, *extra):
        output = ROOT / "target" / "qualification" / ("test-ci-color-" + uuid.uuid4().hex)
        self.outputs.append(output)
        return [sys.executable, "-B", str(ROOT / "scripts/validate.py"), "--stages", "source-contract",
                "--output", str(output), *extra], output

    def test_every_script_takes_the_color_option(self):
        for name, args in SCRIPTS.items():
            with self.subTest(script=name):
                completed = script(name, *args, "--help")
                self.assertEqual(completed.returncode, 0, completed.stderr)
                self.assertIn("--color {auto,always,never}", completed.stdout)

    def test_validate_writes_the_same_text_without_color_and_on_a_pipe(self):
        texts = []
        for extra in ([], ["--color", "never"], ["--color", "auto"]):
            args, output = self.validate_args(*extra)
            completed = subprocess.run(args, cwd=ROOT, capture_output=True, text=True, timeout=120,
                                       env={**os.environ, "TERM": "xterm"})
            self.assertEqual(completed.returncode, 0, completed.stderr)
            texts.append(completed.stdout.replace(str(output), "OUTPUT"))
        self.assertEqual(texts, [texts[0]] * 3)
        self.assertEqual(texts[0], 'source-contract: passed\n{"status": "passed", "report": "OUTPUT/report.json"}\n')

    def test_always_colors_only_the_status_word_and_never_the_json(self):
        args, output = self.validate_args("--color", "always")
        completed = subprocess.run(args, cwd=ROOT, capture_output=True, text=True, timeout=120)
        stage, final = completed.stdout.splitlines()
        self.assertEqual(stage, "source-contract: \033[32mpassed\033[0m")
        self.assertEqual(json.loads(final)["status"], "passed")
        self.assertNotIn(ESCAPE, final)
        self.assertNotIn(ESCAPE, (output / "report.json").read_text())

    def test_auto_colors_a_terminal_unless_no_color_or_a_dumb_term(self):
        environment = {key: value for key, value in os.environ.items() if key not in ("NO_COLOR", "TERM")}
        args, _ = self.validate_args()
        colored = on_terminal(args, {**environment, "TERM": "xterm-256color"})
        self.assertTrue(colored.startswith("source-contract: \033[32mpassed\033[0m\n"), colored)
        self.assertNotIn(ESCAPE, colored.splitlines()[-1])
        for variables in ({"NO_COLOR": "1", "TERM": "xterm"}, {"TERM": "dumb"}):
            with self.subTest(variables=variables):
                args, _ = self.validate_args()
                plain = on_terminal(args, {**environment, **variables})
                self.assertTrue(plain.startswith("source-contract: passed\n"), plain)
                self.assertNotIn(ESCAPE, plain)

    def test_machine_read_output_is_never_colored(self):
        documents = script("check_docs.py", "--color", "always")
        self.assertEqual(documents.returncode, 0, documents.stderr)
        self.assertNotIn(ESCAPE, documents.stdout)
        json.loads(documents.stdout)
        plan = script("ci_plan.py", "--base", "HEAD", "--head", "HEAD", "--color", "always")
        self.assertEqual(plan.returncode, 0, plan.stderr)
        self.assertNotIn(ESCAPE, plan.stdout)
        self.assertEqual(json.loads(plan.stdout)["reasons"], ["empty-diff"])


if __name__ == "__main__":
    unittest.main()
