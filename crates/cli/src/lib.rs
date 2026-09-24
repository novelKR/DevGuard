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
use devguard_daemon::install::{InstallOptions, Launchctl};
use devguard_daemon::paths::AuthorityPaths;
use devguard_daemon::upgrade::UpgradeOptions;
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
devguard upgrade --release ID [--drain-timeout DURATION] [--stopped]
devguard repair --use last-known-good
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
ends the lease and writes report.json in the new report directory.

upgrade replaces the installed release with a staged one (`devguardd stage --package DIR`)
and must run from that release's own devguard: it closes admission, waits up to the drain
timeout (60s by default) for charged work to end, backs up the journal, starts the new
release closed, verifies it and reopens admission; otherwise the current release keeps
serving. --stopped replaces a release that cannot close admission by stopping it first,
only if nothing is then charged. repair starts the last known good release, or its
recovery copy, while no authority serves; it never reinitializes the journal.";

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
        args::Command::Upgrade(upgrade) => match locate() {
            Ok((paths, _)) => operate(|| {
                devguard_daemon::upgrade::upgrade(
                    &paths,
                    &upgrade.release,
                    &Launchctl::new(paths.uid()),
                    &InstallOptions::canonical(&paths),
                    &UpgradeOptions {
                        drain_timeout: upgrade
                            .drain_timeout
                            .unwrap_or(devguard_daemon::upgrade::DEFAULT_DRAIN_TIMEOUT),
                        stopped: upgrade.stopped,
                    },
                )
            }),
            Err(error) => {
                eprintln!("devguard: {}", error.message);
                Exit::Code(1)
            }
        },
        args::Command::Repair => match locate() {
            Ok((paths, _)) => operate(|| {
                devguard_daemon::upgrade::repair(
                    &paths,
                    &Launchctl::new(paths.uid()),
                    &InstallOptions::canonical(&paths),
                )
            }),
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

/// Run an operation on the installed service and print its JSON report.
fn operate<T: serde::Serialize>(operation: impl FnOnce() -> Result<T>) -> Exit {
    match operation() {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(text) => {
                println!("{text}");
                Exit::Code(0)
            }
            Err(_) => {
                eprintln!("devguard: the report could not be encoded");
                Exit::Code(1)
            }
        },
        Err(error) => {
            eprintln!("devguard: {:?}: {}", error.code, error.message);
            Exit::Code(1)
        }
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
