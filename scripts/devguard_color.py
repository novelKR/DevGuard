"""Color for the lines of DevGuard's scripts that a person reads.

Only status words, warnings, details and headers of those lines are colored: green for a pass,
bold red for a failure, yellow for an inconclusive result or a warning, magenta for a failure or
an interruption of the tool itself, dim for details and bold for headers. JSON, reports, evidence,
manifests and GitHub workflow commands are never colored.

`--color auto`, the default, colors a line only when its stream is a terminal, NO_COLOR is not set
and TERM is not `dumb`. `always` colors those lines on any stream, and `never` writes exactly the
text written without color. Nothing is colored until a script's main chooses, so a program that
imports a script, as CodeSpace's harness imports measure.py, sees the same text as without color.
The module name carries the project's name so that it never shadows a module of such a program.
"""
import os
import sys

CHOICES = ("auto", "always", "never")
STYLES = {"pass": "32", "fail": "1;31", "warn": "33", "internal": "35", "dim": "2", "bold": "1"}
# What each status word means. A word missing here is written plain.
WORDS = {
    "pass": "pass", "passed": "pass", "qualified": "pass", "verified": "pass", "promoted": "pass",
    "fail": "fail", "failed": "fail",
    "inconclusive": "warn", "incomplete": "warn", "not_run": "warn",
    "harness_failed": "internal", "interrupted": "internal",
}
_choice = "never"


def add_option(parser):
    parser.add_argument("--color", choices=CHOICES, default="auto",
                        help="color the status words of the lines a person reads: auto (default) only on a "
                             "terminal without NO_COLOR or TERM=dumb, always, or never")


def choose(choice):
    """Color this process's lines as CHOICE says; a script's main calls this with its --color."""
    global _choice
    if choice not in CHOICES:
        raise ValueError("unknown color choice: " + str(choice))
    _choice = choice


def enabled(stream=None):
    """Whether lines written to STREAM, standard output by default, are colored."""
    if _choice != "auto":
        return _choice == "always"
    if "NO_COLOR" in os.environ or os.environ.get("TERM") == "dumb":
        return False
    stream = sys.stdout if stream is None else stream
    try:
        return stream.isatty()
    except (AttributeError, ValueError):
        return False


def paint(text, style, stream=None):
    """TEXT in STYLE when lines on STREAM are colored; otherwise TEXT itself."""
    if not text or style is None or not enabled(stream):
        return text
    return "\033[" + STYLES[style] + "m" + text + "\033[0m"


def status(word, stream=None):
    """A status word in the color of what it means."""
    return paint(word, WORDS.get(word), stream)


def refuse(message):
    """End the script as SystemExit(MESSAGE) does, with the message colored as a failure."""
    raise SystemExit(paint(message, "fail", sys.stderr))
