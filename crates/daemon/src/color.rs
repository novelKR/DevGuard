//! Color for the lines of `devguardd` and `devguard` that a person reads: a success in green,
//! a refusal or failure in bold red, an unknown outcome in yellow and a failure or interruption
//! of the tool itself in magenta.
//!
//! A line is colored only when its stream is a terminal, `NO_COLOR` is unset and `TERM` is not
//! `dumb`. JSON, reports, receipts and anything written to a file or a pipe are never colored,
//! so every reader of them, and every log, sees exactly the text it saw before.

use std::ffi::OsStr;
use std::io::IsTerminal;

/// What a line reports, which decides its color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// A step that succeeded.
    Pass,
    /// A refusal or a failure.
    Fail,
    /// An outcome that is not known.
    Warn,
    /// A failure or an interruption of the tool itself.
    Internal,
}

impl Tone {
    fn code(self) -> &'static str {
        match self {
            Tone::Pass => "32",
            Tone::Fail => "1;31",
            Tone::Warn => "33",
            Tone::Internal => "35",
        }
    }
}

/// Whether a stream is colored: it is a terminal, `NO_COLOR` is unset (an empty value counts
/// as set) and `TERM` is not `dumb`.
pub fn colored(terminal: bool, no_color: Option<&OsStr>, term: Option<&OsStr>) -> bool {
    terminal && no_color.is_none() && term.is_none_or(|term| term != "dumb")
}

/// `text` in `tone` when `colored`; otherwise `text` itself.
pub fn paint(colored: bool, tone: Tone, text: &str) -> String {
    if colored && !text.is_empty() {
        format!("\x1b[{}m{text}\x1b[0m", tone.code())
    } else {
        text.to_owned()
    }
}

fn wanted(terminal: bool) -> bool {
    colored(
        terminal,
        std::env::var_os("NO_COLOR").as_deref(),
        std::env::var_os("TERM").as_deref(),
    )
}

/// `text` for standard output, in `tone` when standard output is a terminal that takes color.
pub fn stdout(tone: Tone, text: &str) -> String {
    paint(wanted(std::io::stdout().is_terminal()), tone, text)
}

/// `text` for standard error, in `tone` when standard error is a terminal that takes color.
pub fn stderr(tone: Tone, text: &str) -> String {
    paint(wanted(std::io::stderr().is_terminal()), tone, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_terminal_without_no_color_or_a_dumb_term_is_colored() {
        let term = Some(OsStr::new("xterm-256color"));
        assert!(colored(true, None, term));
        assert!(colored(true, None, None));
        assert!(!colored(false, None, term));
        assert!(!colored(true, Some(OsStr::new("1")), term));
        assert!(!colored(true, Some(OsStr::new("")), term));
        assert!(!colored(true, None, Some(OsStr::new("dumb"))));
    }

    #[test]
    fn each_tone_has_its_color_and_an_uncolored_line_is_the_text_itself() {
        assert_eq!(paint(true, Tone::Pass, "ok"), "\x1b[32mok\x1b[0m");
        assert_eq!(paint(true, Tone::Fail, "no"), "\x1b[1;31mno\x1b[0m");
        assert_eq!(paint(true, Tone::Warn, "?"), "\x1b[33m?\x1b[0m");
        assert_eq!(paint(true, Tone::Internal, "!"), "\x1b[35m!\x1b[0m");
        assert_eq!(
            paint(false, Tone::Fail, "devguard: refused"),
            "devguard: refused"
        );
        assert_eq!(paint(true, Tone::Fail, ""), "");
    }
}
