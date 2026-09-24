//! The `devguard` command-line owner. It runs commands only through the
//! central authority and the fenced launch helper; it never falls back to
//! running a command unmanaged.
//!
//! The binary derives the authority from the operating account and finds the
//! helper beside itself. This library takes both explicitly, so tests can
//! drive it against isolated fixture authorities without the binary
//! accepting any override.

pub mod adapter;
pub mod args;
pub mod authority;
pub mod candidate;
pub mod doctor;
pub mod exec;
pub mod preflight;
pub mod receipt;
pub mod signals;
pub mod terminal;

use devguard_contract::Result;
use devguard_daemon::paths::AuthorityPaths;
pub use receipt::Exit;
use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "\
devguard exec [--project ID] [--adapter auto|generic|cargo|cargo-pipeline] [--wait DURATION]
              [--cpu MILLICPU] [--memory SIZE] [--tasks N] [--receipt PATH]
              [--lease CONSUMER/GENERATION/ATTEMPT --lease-token-fd N] -- PROGRAM [ARGS...]
devguard doctor [--require admission,registration,macos-cooperative] [--project ID]
devguard test-candidate --candidate DIR --report DIR [--cpu MILLICPU] [--memory SIZE] [--tasks N]
              [--ttl DURATION] [--wait DURATION]
devguard --help | --version

exec admits the command at the central authority, starts it through devguard-launch under
the reserved budget and reports its own exit. A command is never run unmanaged: when it
cannot be admitted or started, nothing runs and devguard exits 125. --wait retries a
refused admission until the given time (ms, s, m or h) has passed. With --lease the command
is a child of that parent lease, admitted against its remainder with the token read from
descriptor N. Durations and sizes are whole numbers; sizes take KiB, MiB or GiB. The
authority comes from the operating account; there is no path or authority override.

test-candidate admits one parent lease and runs a candidate tree's build, its applicable
tests and its own candidate authority as children of it, checks the candidate's admission,
ends the lease and writes report.json in the new report directory.";

/// Run the command line `args` (without the program name). `locate` supplies
/// the authority paths and the launch helper, and is consulted only by
/// commands that need them.
pub fn run(
    args: impl IntoIterator<Item = OsString>,
    locate: impl FnOnce() -> Result<(AuthorityPaths, PathBuf)>,
) -> Exit {
    let command = match args::parse(args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("devguard: {}\n\n{USAGE}", error.message);
            return Exit::Code(exec::NOT_STARTED);
        }
    };
    match command {
        args::Command::Help => {
            println!("{USAGE}");
            Exit::Code(0)
        }
        args::Command::Version => {
            println!("devguard {}", env!("CARGO_PKG_VERSION"));
            Exit::Code(0)
        }
        args::Command::Exec(exec) => match locate() {
            Ok((paths, helper)) => exec::run(exec, &paths, &helper),
            Err(error) => {
                eprintln!("devguard: {}; nothing was started", error.message);
                Exit::Code(exec::NOT_STARTED)
            }
        },
        args::Command::Doctor(doctor) => match locate() {
            Ok((paths, helper)) => doctor::run(doctor, &paths, &helper),
            Err(error) => {
                eprintln!("devguard: {}", error.message);
                Exit::Code(1)
            }
        },
        args::Command::TestCandidate(candidate) => match locate() {
            Ok((paths, _)) => candidate::run_args(&candidate, &paths),
            Err(error) => {
                eprintln!("devguard: {}; nothing was started", error.message);
                Exit::Code(exec::NOT_STARTED)
            }
        },
    }
}

/// End the process as the command ended: with its exit code, or by the same
/// signal with its default action. The workload's core dump, if any, is its
/// own; the CLI's core limit is set to zero first so it never writes one.
pub fn terminate(exit: Exit) -> ! {
    match exit {
        Exit::Code(code) => std::process::exit(code),
        Exit::Signal(signal) => {
            // SAFETY: lowering this process's core limit, restoring the
            // default action and raising the signal on this process have no
            // preconditions; SIGSTOP-like signals are not passed here.
            unsafe {
                let none = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                libc::setrlimit(libc::RLIMIT_CORE, &none);
                libc::signal(signal, libc::SIG_DFL);
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, signal);
                libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
                libc::raise(signal);
            }
            std::process::exit(128 + signal)
        }
    }
}
