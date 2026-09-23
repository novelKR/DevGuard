//! Consumer side of a managed launch. The owner starts at most one
//! `devguard-launch` helper per grant: the one-time permit travels on a
//! private descriptor, and the helper reports its phases on a second private
//! descriptor, separate from the executable's own output. The helper presents
//! the grant to the authority, which verifies it, applies and reads back the
//! scope policy and authorizes the run. Only then does the helper report
//! READY and attempt the executable.
//!
//! macOS cannot create a pipe or socket pair close-on-exec atomically. This
//! module therefore creates a grant's descriptors and spawns helpers under one
//! process-wide guard, so helpers launched from several threads never inherit
//! each other's descriptors. An owner that also spawns other processes from
//! other threads must hold [`spawn_guard`] around those spawns too, or spawn
//! them with close-on-exec as the default.

use crate::credential::CredentialHandoff;
use devguard_contract::{validate_id, AttemptKey, Error, ErrorCode, Result, Secret};
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static SPAWN_GUARD: Mutex<()> = Mutex::new(());

/// Serializes creating a grant's descriptors with spawning processes. Hold it
/// around any other spawn in an owner that launches helpers from several threads.
pub fn spawn_guard() -> MutexGuard<'static, ()> {
    SPAWN_GUARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Exit status of a helper that did not start the executable because the
/// launch was not authorized or could not be presented.
pub const NOT_AUTHORIZED_STATUS: i32 = 125;
/// Exit status of a helper whose exec failed for a reason other than a
/// missing program.
pub const EXEC_FAILED_STATUS: i32 = 126;
/// Exit status of a helper whose program does not exist.
pub const NOT_FOUND_STATUS: i32 = 127;
/// Upper bound on a helper's whole launch transcript.
pub const TRANSCRIPT_BYTES: usize = 8 * 1024;

/// Phases a helper reports, one JSON object per line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum HelperPhase {
    /// The helper could not present the grant, so it did not claim it.
    Failed { stage: String, message: String },
    /// The helper was not authorized: the authority refused it or its reply
    /// was lost. It will not exec, but it may have claimed the grant first.
    Refused { code: ErrorCode, message: String },
    /// Credentials are closed and every pre-exec boundary is complete.
    /// This is not evidence that the executable started. (An empty struct
    /// variant, so strict decoding also rejects unknown fields here.)
    Ready {},
    /// The executable could not be started after READY.
    ExecFailed { errno: i32 },
}

/// What the owner learns when the transcript ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// READY was reported and the report descriptor then closed, which is how
    /// a successful exec appears. A helper killed between the two looks the
    /// same, so the exit status still decides what the executable did.
    Started,
    /// READY was reported and then exec failed.
    ExecFailed { errno: i32 },
    /// The helper failed or was refused before READY; it never attempted the
    /// executable.
    NotStarted(HelperPhase),
    /// The transcript ended without a terminal report, for example because the
    /// helper was killed. The owner must reconcile rather than assume either way.
    Lost { ready: bool },
}

/// Everything the helper needs besides the permit. None of it is secret.
#[derive(Debug, Clone)]
pub struct HelperTicket {
    /// The authority endpoint the owner used for this grant.
    pub endpoint: PathBuf,
    pub key: AttemptKey,
    /// The owner's registered instance.
    pub instance_id: String,
}

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorCode::InvalidRequest, message)
}

fn io_failed(message: &'static str) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}

/// Move a new descriptor above the standard three, close-on-exec.
fn private(fd: OwnedFd) -> Result<OwnedFd> {
    // SAFETY: fcntl duplicates the live descriptor held by fd, which closes
    // when dropped; the duplicate is newly owned below.
    let moved = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    if moved < 0 {
        return Err(io_failed("cannot protect a private descriptor"));
    }
    // SAFETY: a successful F_DUPFD_CLOEXEC returned a descriptor nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(moved) })
}

/// A pipe for the helper's transcript. The owner keeps the read end; the
/// helper inherits only the write end, and closes it on a successful exec.
fn report_pipe() -> Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    // SAFETY: fds is a live two-element array; each descriptor is owned below.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io_failed("cannot create the helper report pipe"));
    }
    // SAFETY: pipe returned two new descriptors that nothing else owns.
    let (reader, writer) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    Ok((private(reader)?, private(writer)?))
}

/// One grant's helper invocation. Configure the executable's standard
/// descriptors, directory and environment with [`HelperCommand::command_mut`],
/// then start it once with [`HelperCommand::spawn`].
pub struct HelperCommand {
    command: Command,
}

impl HelperCommand {
    /// The command to configure. Spawn through [`HelperCommand::spawn`], not
    /// this command, so the private descriptors are closed afterwards.
    pub fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    /// The helper's arguments; they carry no secret.
    pub fn args(&self) -> impl Iterator<Item = &OsStr> {
        self.command.get_args()
    }

    /// Start the helper once. On success a helper exists and must be waited
    /// for; on failure none exists. Either way the owner's copies of the
    /// permit carrier and transcript writer are closed on return, so the
    /// transcript reaches its end when the helper execs or exits.
    pub fn spawn(self) -> std::io::Result<Child> {
        let mut command = self.command;
        let _guard = spawn_guard();
        command.spawn()
    }
}

/// Build the helper invocation for one grant: `helper` runs `program` with
/// `args` once the authority authorizes it. The executable inherits the
/// working directory, environment and standard descriptors (pipes or a
/// pseudo-terminal) configured on the command, and every other descriptor the
/// owner leaves inheritable.
pub fn helper_command(
    helper: &Path,
    ticket: &HelperTicket,
    permit: &Secret,
    program: &Path,
    args: &[OsString],
) -> Result<(HelperCommand, HelperReport)> {
    ticket.key.validate()?;
    validate_id(&ticket.instance_id)?;
    if !helper.is_absolute() || !program.is_absolute() || !ticket.endpoint.is_absolute() {
        return Err(invalid(
            "helper, program and endpoint paths must be absolute",
        ));
    }
    let _guard = spawn_guard();
    let (reader, writer) = report_pipe()?;
    let mut command = Command::new(helper);
    let permit_fd = CredentialHandoff::new(permit)?.attach(&mut command);
    let report_fd = writer.as_raw_fd();
    // SAFETY: the post-fork callback uses only async-signal-safe fcntl; the
    // closure owns the writer, keeping it valid until the command is dropped.
    unsafe {
        command.pre_exec(move || {
            let fd = writer.as_raw_fd();
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
        .arg("--endpoint")
        .arg(&ticket.endpoint)
        .args(["--consumer", &ticket.key.consumer_id])
        .args(["--generation", &ticket.key.consumer_generation])
        .args(["--attempt", &ticket.key.attempt_id])
        .args(["--instance", &ticket.instance_id])
        .args(["--permit-fd", &permit_fd.to_string()])
        .args(["--report-fd", &report_fd.to_string()])
        .arg("--")
        .arg(program)
        .args(args);
    Ok((HelperCommand { command }, HelperReport::new(reader)))
}

/// The owner's end of a helper transcript.
pub struct HelperReport {
    file: File,
    fd: RawFd,
}

impl HelperReport {
    fn new(reader: OwnedFd) -> Self {
        let fd = reader.as_raw_fd();
        Self {
            file: File::from(reader),
            fd,
        }
    }

    /// Read the transcript until the helper closes it or `limit` passes.
    /// A timeout returns what was learned so far as `Lost`.
    pub fn wait(mut self, limit: Duration) -> Result<(LaunchOutcome, Vec<HelperPhase>)> {
        let deadline = Instant::now() + limit;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 1024];
        let mut closed = false;
        while !closed {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let mut poll = libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let wait = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
            // SAFETY: poll points to one live pollfd owned by this frame.
            let ready = unsafe { libc::poll(&mut poll, 1, wait) };
            if ready < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(io_failed("cannot wait for the helper report"));
            }
            if ready == 0 {
                break;
            }
            match self.file.read(&mut buffer) {
                Ok(0) => closed = true,
                Ok(n) => {
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.len() > TRANSCRIPT_BYTES {
                        return Err(invalid("helper transcript exceeds its bound"));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return Err(io_failed("cannot read the helper report")),
            }
        }
        let phases = parse(&bytes)?;
        Ok((outcome(&phases, closed), phases))
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<HelperPhase>> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("helper report is not UTF-8"))?;
    let complete = text.rsplit_once('\n').map_or("", |(lines, _)| lines);
    complete
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|_| invalid("malformed helper report")))
        .collect()
}

fn outcome(phases: &[HelperPhase], closed: bool) -> LaunchOutcome {
    let ready = phases.contains(&HelperPhase::Ready {});
    match phases.last() {
        Some(HelperPhase::ExecFailed { errno }) if ready => {
            LaunchOutcome::ExecFailed { errno: *errno }
        }
        Some(phase @ (HelperPhase::Failed { .. } | HelperPhase::Refused { .. })) if !ready => {
            LaunchOutcome::NotStarted(phase.clone())
        }
        Some(HelperPhase::Ready {}) if closed => LaunchOutcome::Started,
        _ => LaunchOutcome::Lost { ready },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcripts_distinguish_ready_exec_failure_refusal_and_loss() {
        let ready = b"{\"phase\":\"ready\"}\n".to_vec();
        let cases: [(&[u8], bool, LaunchOutcome); 6] = [
            (&ready, true, LaunchOutcome::Started),
            (&ready, false, LaunchOutcome::Lost { ready: true }),
            (
                b"{\"phase\":\"ready\"}\n{\"phase\":\"exec_failed\",\"errno\":2}\n",
                true,
                LaunchOutcome::ExecFailed { errno: 2 },
            ),
            (
                b"{\"phase\":\"refused\",\"code\":\"invalid_transition\",\"message\":\"fenced\"}\n",
                true,
                LaunchOutcome::NotStarted(HelperPhase::Refused {
                    code: ErrorCode::InvalidTransition,
                    message: "fenced".into(),
                }),
            ),
            (b"", true, LaunchOutcome::Lost { ready: false }),
            // A partial final line is ignored rather than trusted.
            (
                b"{\"phase\":\"rea",
                true,
                LaunchOutcome::Lost { ready: false },
            ),
        ];
        for (bytes, closed, expected) in cases {
            assert_eq!(outcome(&parse(bytes).unwrap(), closed), expected);
        }
        for malformed in [
            &b"{\"phase\":\"ready\",\"extra\":1}\n"[..],
            b"{\"phase\":\"authorized\"}\n",
            b"not json\n",
        ] {
            assert!(parse(malformed).is_err());
        }
    }
}
