//! `devguard exec`: one managed execution through the central authority.
//!
//! The CLI is the registered owner. It admits the command, commits the launch,
//! starts the `devguard-launch` helper as its direct child and waits for it.
//! It never runs a command outside that path: every failure before READY ends
//! with nothing started. A new attempt is made only after the previous one is
//! known not to have started, and only while an explicit `--wait` lasts.

use crate::args::ExecArgs;
use crate::authority::{Endpoint, LeaseHold, Service};
use crate::preflight::{self, Preflight, Refusal};
use crate::receipt::{
    AttemptEntry, AuthoritySummary, BudgetSummary, ExecReceipt, ExecResult, Exit, LaunchSummary,
    ReceiptFile,
};
use crate::signals::Signals;
use crate::terminal::Terminal;
use devguard_client::launch::{helper_command, HelperPhase, LaunchOutcome, NOT_AUTHORIZED_STATUS};
use devguard_client::protocol::AbandonReason;
use devguard_contract::{
    AdmissionRequest, AttemptKey, AttemptPhase, AttemptRecord, ErrorCode, ReleaseReason,
    ResourceIntent, Secret,
};
use devguard_daemon::paths::AuthorityPaths;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The status of a command DevGuard did not start, as `env` and `timeout` use.
pub const NOT_STARTED: i32 = 125;
/// A shell's status for a program that is not executable.
pub const NOT_EXECUTABLE: i32 = 126;
/// A shell's status for a program that was not found.
pub const NOT_FOUND: i32 = 127;
/// How long the transcript may stay open once the root has exited. Only a
/// helper that passed its end on to another process can hold it open.
const TRANSCRIPT_GRACE: Duration = Duration::from_secs(5);
const FIRST_BACKOFF: Duration = Duration::from_millis(250);
/// Each refused admission leaves a denied attempt in the journal until its
/// generation is retired, so a long wait backs off to one attempt per 10 s.
const MAX_BACKOFF: Duration = Duration::from_secs(10);
/// The stops a terminal's job control causes; only these are mirrored.
const JOB_CONTROL_STOPS: [libc::c_int; 3] = [libc::SIGTSTP, libc::SIGTTIN, libc::SIGTTOU];

/// What an admission request led to.
enum Admission {
    Admitted,
    /// Denied for capacity or pressure, or the instance pool is full; a new
    /// attempt may be made while the wait lasts.
    Waitable(String),
    Refused(String),
}

/// What one admitted attempt led to.
enum Step {
    /// The run is over; this is the CLI's exit.
    Finished(Exit),
    /// Nothing started and the attempt is settled; another may be admitted
    /// while the wait lasts.
    Retry(String),
    /// Nothing started, and no further attempt should be made.
    Stop(String),
}

struct Run {
    receipt: ExecReceipt,
    started: Instant,
    /// When the last admission succeeded: the time spent waiting before it.
    admitted_after: Option<Duration>,
    deadline: Option<Instant>,
    backoff: Duration,
}

fn error_text(error: &devguard_contract::Error) -> String {
    format!("{:?}: {}", error.code, error.message)
}

impl Run {
    fn note(&mut self, key: &AttemptKey, record: Option<AttemptRecord>, note: impl Into<String>) {
        self.receipt.attempts.push(AttemptEntry {
            key: key.clone(),
            record,
            note: note.into(),
        });
    }

    fn not_started(&mut self, reason: impl Into<String>) -> Exit {
        let reason = reason.into();
        eprintln!("devguard: {reason}; nothing was started");
        self.receipt.result = ExecResult::NotStarted;
        self.receipt.reason = Some(reason);
        Exit::Code(NOT_STARTED)
    }

    /// Sleep before the next attempt, or report why no further attempt is made.
    fn pause(&mut self, signals: &Signals) -> std::result::Result<(), Exit> {
        let Some(deadline) = self.deadline else {
            return Err(Exit::Code(NOT_STARTED));
        };
        let now = Instant::now();
        if now >= deadline {
            self.receipt.wait.deadline_reached = true;
            return Err(Exit::Code(NOT_STARTED));
        }
        let pause = self.backoff.min(deadline - now);
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
        if let Some(signal) = signals.sleep(pause) {
            self.receipt.wait.cancelled_by_signal = Some(signal);
            return Err(Exit::Signal(signal));
        }
        Ok(())
    }

    fn admit(
        &mut self,
        service: &dyn Service,
        key: &AttemptKey,
        digest: &str,
        intent: &ResourceIntent,
    ) -> Admission {
        let request = AdmissionRequest {
            key: key.clone(),
            execution_digest: digest.into(),
            intent: intent.clone(),
        };
        let call = || service.admit(request.clone());
        // A lost reply is replayed once with the same key, which returns the
        // original result rather than admitting twice.
        let result = match call() {
            Err(error) if error.code == ErrorCode::ResourceControlUnavailable => call(),
            other => other,
        };
        match result {
            Ok(record) if record.phase == AttemptPhase::Prepared => {
                self.note(key, Some(record), "admitted");
                Admission::Admitted
            }
            Ok(record) => {
                let waitable = record.denial == Some(ErrorCode::ResourceUnavailable);
                let note = match record.denial {
                    Some(code) => format!("admission denied: {code:?}"),
                    None => format!("unexpected admission phase {:?}", record.phase),
                };
                self.note(key, Some(record), note.clone());
                if waitable {
                    Admission::Waitable(note)
                } else {
                    Admission::Refused(note)
                }
            }
            // A full instance pool is a capacity condition like a denial.
            Err(error) if error.code == ErrorCode::ResourceUnavailable => {
                let note = error_text(&error);
                self.note(key, None, note.clone());
                Admission::Waitable(note)
            }
            Err(error) => {
                let note = error_text(&error);
                self.note(key, None, note.clone());
                Admission::Refused(note)
            }
        }
    }

    /// Report that this owner holds no helper for the grant, and learn
    /// whether the attempt is now known not to have started.
    fn abandon(&mut self, service: &dyn Service, key: &AttemptKey, reason: AbandonReason) -> bool {
        match service.abandon(key, reason) {
            Ok(record) => {
                let settled = record.release_reason == Some(ReleaseReason::NoHelperCreated);
                self.note(key, Some(record), format!("reported no helper: {reason:?}"));
                settled
            }
            Err(error) => {
                self.note(
                    key,
                    None,
                    format!("no-helper report failed: {}", error_text(&error)),
                );
                false
            }
        }
    }

    /// Commit the launch and obtain the one-time permit. A lost reply is
    /// reconciled: the grant is settled as never received, or the uncertainty
    /// is reported; it is never recreated.
    fn commit(
        &mut self,
        service: &dyn Service,
        key: &AttemptKey,
    ) -> std::result::Result<Secret, Step> {
        let begin = || service.begin_launch(key);
        if let Ok(grant) = begin() {
            if let Some(permit) = grant.permit {
                return Ok(permit);
            }
        }
        let lookup = service.lookup(key);
        match lookup {
            Ok(record) if record.phase == AttemptPhase::Prepared => {
                self.note(
                    key,
                    Some(record),
                    "launch commit reply lost before commit; replayed",
                );
                match begin().map(|grant| grant.permit) {
                    Ok(Some(permit)) => Ok(permit),
                    _ => self.grant_lost(service, key),
                }
            }
            Ok(record)
                if record.phase == AttemptPhase::LaunchCommitted && record.scope.is_none() =>
            {
                self.note(key, Some(record), "launch commit reply lost after commit");
                self.grant_lost(service, key)
            }
            Ok(record) => {
                let known = record.known_not_started();
                let note = format!("launch not committed: {:?}", record.phase);
                self.note(key, Some(record), note.clone());
                Err(if known {
                    Step::Retry(note)
                } else {
                    Step::Stop(note)
                })
            }
            Err(error) => {
                let note = format!(
                    "cannot confirm the launch commit ({}); the attempt stays charged until reconciled",
                    error_text(&error)
                );
                self.note(key, None, note.clone());
                Err(Step::Stop(note))
            }
        }
    }

    /// Commit an admitted attempt, recording how long the wait for it took.
    fn commit_admitted(
        &mut self,
        service: &dyn Service,
        key: &AttemptKey,
    ) -> std::result::Result<Secret, Step> {
        self.admitted_after = Some(self.started.elapsed());
        self.commit(service, key)
    }

    fn grant_lost(
        &mut self,
        service: &dyn Service,
        key: &AttemptKey,
    ) -> std::result::Result<Secret, Step> {
        if self.abandon(service, key, AbandonReason::GrantNotReceived) {
            Err(Step::Retry("the launch grant was lost and released".into()))
        } else {
            Err(Step::Stop(
                "the launch grant was lost and could not be released; it stays charged until reconciled"
                    .into(),
            ))
        }
    }

    fn launch(
        &mut self,
        service: &dyn Service,
        key: &AttemptKey,
        permit: Secret,
        pre: &Preflight,
        helper: &Path,
        signals: &Signals,
    ) -> Step {
        let ticket = service.ticket(key);
        let prepared = helper_command(helper, &ticket, &permit, &pre.program, &pre.args);
        drop(permit);
        let (mut command, report) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let released = self.abandon(service, key, AbandonReason::SpawnFailed);
                let note = format!("cannot prepare the helper: {}", error_text(&error));
                return if released {
                    Step::Stop(note)
                } else {
                    Step::Stop(note + "; the attempt stays charged")
                };
            }
        };
        // The transcript is read to its end on its own thread, started before
        // any helper exists, while this thread waits for the root from the
        // start: a stop before READY is mirrored at once, and no deadline can
        // make a slow launch look like one that never started.
        let (sender, transcript) = std::sync::mpsc::channel();
        let reader = std::thread::Builder::new()
            .name("devguard-transcript".into())
            .spawn(move || {
                let _ = sender.send(report.read_until_closed());
            });
        if let Err(error) = reader {
            self.abandon(service, key, AbandonReason::SpawnFailed);
            return Step::Stop(format!("cannot read the helper transcript: {error}"));
        }
        {
            let command = command.command_mut();
            for name in &pre.removed {
                command.env_remove(name);
            }
            for (name, value) in &pre.set {
                command.env(name, value);
            }
            // The helper leads its own group from the start, so the terminal
            // can be handed to it before it reports READY.
            command.process_group(0);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.abandon(service, key, AbandonReason::SpawnFailed);
                return Step::Stop(format!("cannot start the launch helper: {error}"));
            }
        };
        let root = child.id() as libc::pid_t;
        let signals_before = signals.received().len();
        signals.attach(root);
        let mut terminal = Terminal::open();
        let handed = terminal
            .as_mut()
            .is_some_and(|terminal| terminal.give(root));
        let (stops, waited) = wait_for_exit(root, terminal.as_mut(), signals);
        // Take the terminal back while the root's group still exists.
        if let Some(terminal) = terminal.as_mut() {
            terminal.take_back();
        }
        // The root has exited, and with it the helper's end of the
        // transcript, so the reader finishes at once.
        let read = transcript
            .recv_timeout(TRANSCRIPT_GRACE)
            .map_err(|_| "the helper transcript stayed open after the helper exited".to_string())
            .and_then(|read| {
                read.map_err(|error| {
                    format!("the helper transcript is not valid: {}", error_text(&error))
                })
            });
        // Observe before reaping: the exited root still holds its PID and
        // group, so members left in the group can still be proven the scope's.
        match service.observe(key) {
            Ok(record) => self.receipt.observed_before_reap = Some(record),
            Err(error) => self.note(
                key,
                None,
                format!("observation before reap failed: {}", error_text(&error)),
            ),
        }
        signals.detach();
        let status = child.wait();
        let exit = match status {
            Ok(status) => match (status.code(), status.signal()) {
                (Some(code), _) => Exit::Code(code),
                (None, Some(signal)) => Exit::Signal(signal),
                _ => Exit::Code(NOT_STARTED),
            },
            Err(_) => Exit::Code(NOT_STARTED),
        };
        self.receipt.exit = Some(exit);
        let (outcome, phases) = match &read {
            Ok((outcome, phases)) => (Some(outcome.clone()), phases.clone()),
            Err(_) => (None, Vec::new()),
        };
        self.receipt.launch = Some(LaunchSummary {
            helper_pid: root as u32,
            phases,
            outcome: outcome
                .as_ref()
                .map_or_else(|| "unknown".to_string(), |outcome| format!("{outcome:?}")),
            terminal_handed: handed,
            stops_mirrored: stops,
        });
        let uncertain = match (&waited, &outcome) {
            (Err(error), _) => Some(format!("cannot wait for the helper: {error}")),
            (_, None) => read.as_ref().err().cloned(),
            (_, Some(LaunchOutcome::Lost { ready: true })) => {
                Some("the helper reported READY and then no final report".to_string())
            }
            _ => None,
        };
        if let Some(reason) = uncertain {
            // The root is reaped, so this owner holds no helper; a grant the
            // helper claimed is still settled only by its scope.
            self.abandon(service, key, AbandonReason::HelperExited);
            self.observe_after_reap(service, key);
            eprintln!("devguard: {reason}; whether the command started is unknown");
            self.receipt.result = ExecResult::Uncertain;
            self.receipt.reason = Some(reason);
            return Step::Finished(exit);
        }
        match outcome {
            Some(LaunchOutcome::Started) => {
                self.observe_after_reap(service, key);
                self.receipt.result = ExecResult::Completed;
                Step::Finished(exit)
            }
            Some(LaunchOutcome::ExecFailed { .. }) => {
                self.observe_after_reap(service, key);
                self.receipt.result = ExecResult::ExecFailed;
                Step::Finished(exit)
            }
            // The transcript ended without READY, which is written before
            // the exec, so the executable never ran.
            refused => {
                let interrupted = signals
                    .received()
                    .get(signals_before..)
                    .and_then(|received| received.first().copied());
                self.not_launched(service, key, refused, exit, interrupted)
            }
        }
    }

    /// The helper ended before READY. Retry only a transient refusal whose
    /// grant is known released, and never after a signal asked the CLI to stop.
    fn not_launched(
        &mut self,
        service: &dyn Service,
        key: &AttemptKey,
        outcome: Option<LaunchOutcome>,
        exit: Exit,
        interrupted: Option<libc::c_int>,
    ) -> Step {
        let refusal = match &outcome {
            Some(LaunchOutcome::NotStarted(HelperPhase::Refused { code, message })) => {
                Some((*code, message.clone()))
            }
            _ => None,
        };
        let released = self.abandon(service, key, AbandonReason::HelperExited);
        self.observe_after_reap(service, key);
        let reason = match (&refusal, &outcome) {
            (Some((code, message)), _) => format!("the launch was refused: {code:?}: {message}"),
            (None, Some(outcome)) => format!("the helper ended before READY ({outcome:?})"),
            (None, None) => "the helper ended before READY".to_string(),
        };
        if let Exit::Signal(signal) = exit {
            eprintln!("devguard: interrupted by signal {signal} before the command started");
            self.receipt.reason = Some(reason);
            return Step::Finished(Exit::Signal(signal));
        }
        // A signal received and forwarded during the launch cancels the run,
        // even when the helper ended for another reason.
        if let Some(signal) = interrupted {
            eprintln!("devguard: cancelled by signal {signal} before the command started");
            self.receipt.wait.cancelled_by_signal = Some(signal);
            self.receipt.reason = Some(format!("{reason}; cancelled by signal {signal}"));
            return Step::Finished(Exit::Signal(signal));
        }
        let transient = matches!(
            refusal,
            Some((
                ErrorCode::ResourceControlUnavailable | ErrorCode::ResourceUnavailable,
                _
            ))
        );
        if released && transient && exit == Exit::Code(NOT_AUTHORIZED_STATUS) {
            Step::Retry(reason)
        } else {
            Step::Stop(reason)
        }
    }

    fn observe_after_reap(&mut self, service: &dyn Service, key: &AttemptKey) {
        match service.observe(key) {
            Ok(record) => self.receipt.observed_after_reap = Some(record),
            Err(error) => self.note(
                key,
                None,
                format!("observation after reap failed: {}", error_text(&error)),
            ),
        }
    }
}

/// Wait until the root exits, without reaping it. A job-control stop of the
/// workload is mirrored: the CLI takes the terminal back and stops itself by
/// the same signal, so the shell that started it regains control; when
/// continued it hands the terminal back and continues the workload. Any other
/// stop, such as SIGSTOP or a tracer's, is left to whoever caused it. Returns
/// how many stops were mirrored, and an error when the root can no longer be
/// waited for: whether it has exited is then unknown.
fn wait_for_exit(
    root: libc::pid_t,
    mut terminal: Option<&mut Terminal>,
    signals: &Signals,
) -> (u32, std::io::Result<()>) {
    let mut stops = 0;
    loop {
        // SAFETY: info is plain data filled by waitid; WNOWAIT leaves the
        // root waitable, so it is reaped only after the observation.
        let (result, code, status) = unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let result = libc::waitid(
                libc::P_PID,
                root as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WSTOPPED | libc::WNOWAIT,
            );
            (result, info.si_code, info.si_status())
        };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return (stops, Err(error));
        }
        match code {
            libc::CLD_STOPPED | libc::CLD_TRAPPED => {
                // Consume the stop report without reaping anything.
                // SAFETY: as above; WNOHANG returns at once if it is gone,
                // and without WEXITED an exit is never consumed here.
                unsafe {
                    let mut info: libc::siginfo_t = std::mem::zeroed();
                    libc::waitid(
                        libc::P_PID,
                        root as libc::id_t,
                        &mut info,
                        libc::WSTOPPED | libc::WNOHANG,
                    );
                }
                if code != libc::CLD_STOPPED || !JOB_CONTROL_STOPS.contains(&status) {
                    continue;
                }
                stops += 1;
                if let Some(terminal) = terminal.as_deref_mut() {
                    terminal.take_back();
                }
                // SAFETY: raise has no preconditions; the default action of
                // each job-control stop stops this process until continued.
                unsafe {
                    libc::raise(status);
                }
                if let Some(terminal) = terminal.as_deref_mut() {
                    terminal.give(root);
                }
                signals.send(libc::SIGCONT);
            }
            libc::CLD_EXITED | libc::CLD_KILLED | libc::CLD_DUMPED => return (stops, Ok(())),
            _ => {}
        }
    }
}

fn lossy(values: &[OsString]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
}

/// Run `devguard exec` against the authority at `paths` with `helper`.
pub fn run(args: ExecArgs, paths: &AuthorityPaths, helper: &Path) -> Exit {
    // A lease token's descriptor is consumed first and closed on every path,
    // so the executable never inherits it.
    let token = args.lease_token_fd.map(|fd| {
        // SAFETY: the caller passed this descriptor to the CLI for the lease
        // token only; nothing else in this process owns it.
        unsafe { devguard_client::credential::take_inherited(fd) }
    });
    let receipt_file = match &args.receipt {
        Some(path) => match ReceiptFile::create(path) {
            Ok(file) => Some(file),
            Err(error) => {
                eprintln!("devguard: cannot create the receipt file: {error}; nothing was started");
                return Exit::Code(NOT_STARTED);
            }
        },
        None => None,
    };
    let mut run = Run {
        receipt: ExecReceipt::new(args.program.to_string_lossy().into_owned()),
        started: Instant::now(),
        admitted_after: None,
        deadline: args.wait.map(|wait| Instant::now() + wait),
        backoff: FIRST_BACKOFF,
    };
    run.receipt.wait.requested_ms = args.wait.map(|wait| wait.as_millis() as u64);
    run.receipt.command.args = lossy(&args.args);
    run.receipt.lease = args.lease.clone();
    let lease = match (args.lease.clone(), token) {
        (Some(key), Some(Ok(token))) => Ok(Some(LeaseHold { key, token })),
        (_, Some(Err(error))) => Err(error),
        _ => Ok(None),
    };
    let signals = Signals::install();
    let exit = match (&signals, lease) {
        (Ok(signals), Ok(lease)) => execute(&mut run, &args, lease, paths, helper, signals),
        (_, Err(error)) => run.not_started(format!(
            "cannot read the lease token: {}",
            error_text(&error)
        )),
        (Err(error), _) => run.not_started(format!("cannot install signal forwarding: {error}")),
    };
    if let Ok(signals) = &signals {
        run.receipt.signals.received = signals.received();
        run.receipt.signals.forwarded = signals.forwarded();
        run.receipt.signals.ignored = signals.ignored();
    }
    if run.admitted_after.is_none() {
        run.admitted_after = Some(run.started.elapsed());
    }
    run.receipt.wait.waited_ms = run.admitted_after.unwrap_or_default().as_millis() as u64;
    run.receipt.finished_unix_ms = crate::receipt::unix_ms();
    if let Some(file) = receipt_file {
        if let Err(error) = file.write(&run.receipt) {
            eprintln!("devguard: cannot write the receipt: {error}");
        }
    }
    exit
}

fn execute(
    run: &mut Run,
    args: &ExecArgs,
    lease: Option<LeaseHold>,
    paths: &AuthorityPaths,
    helper: &Path,
    signals: &Signals,
) -> Exit {
    if !helper.is_absolute() || !helper.is_file() {
        return run.not_started(format!("the launch helper {} is missing", helper.display()));
    }
    let mut endpoint = match Endpoint::open(paths) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            return run.not_started(format!("cannot use the authority: {}", error_text(&error)))
        }
    };
    endpoint.lease = lease;
    let environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    let mut pre = match preflight::prepare(args, &endpoint.config, &environment) {
        Ok(pre) => pre,
        Err(Refusal::NotFound(program)) => {
            run.not_started(format!("{program}: command not found"));
            return Exit::Code(NOT_FOUND);
        }
        Err(Refusal::NotExecutable(program)) => {
            run.not_started(format!("{program}: not executable"));
            return Exit::Code(NOT_EXECUTABLE);
        }
        Err(Refusal::Invalid(error)) => return run.not_started(error_text(&error)),
    };
    run.receipt.command.program = Some(pre.program.clone());
    run.receipt.command.transformed_args = lossy(&pre.args);
    run.receipt.command.cwd = Some(pre.cwd.clone());
    run.receipt.command.tty = pre.tty;
    run.receipt.command.environment_set = pre.set.clone();
    run.receipt.command.environment_removed = pre.removed.clone();
    run.receipt.command.execution_digest = Some(pre.digest.clone());
    run.receipt.adapter = Some(pre.adapter.clone());
    run.receipt.budget = Some(BudgetSummary {
        requested: pre.intent.requested,
        source: pre.budget_source,
    });
    run.receipt.project = pre.project.clone();
    run.receipt.authority = Some(match endpoint.status() {
        Ok((hello, _)) => AuthoritySummary {
            endpoint: endpoint.summary(),
            authority_pid: Some(hello.authority.pid as i32),
            protocol: Some(hello.protocol),
            capabilities: hello.capabilities,
        },
        Err(_) => AuthoritySummary {
            endpoint: endpoint.summary(),
            authority_pid: None,
            protocol: None,
            capabilities: Default::default(),
        },
    });
    if let (Some(_), Some(lease)) = (args.wait, &endpoint.lease) {
        // A lease child is admitted against its lease, never the host: a
        // request larger than the lease can never be admitted.
        match endpoint.lease_status(&lease.key, &lease.token) {
            Ok(view) => {
                run.receipt.wait.lease_budget = Some(view.lease.budget);
                if !pre.intent.requested.fits(view.lease.budget) {
                    let budget = view.lease.budget;
                    return run.not_started(format!(
                        "the request exceeds its parent lease's budget of {} mCPU, {} bytes and {} tasks, so no wait can admit it",
                        budget.cpu_milli, budget.memory_bytes, budget.tasks
                    ));
                }
            }
            Err(error) => {
                run.receipt.wait.work_capacity_unknown = Some(error_text(&error));
            }
        }
    } else if args.wait.is_some() {
        // A wait cannot make room the host does not have: refuse at once
        // rather than leave a denied attempt behind every backoff.
        match endpoint.config.observed_work_capacity(endpoint.uid) {
            Ok(capacity) => {
                run.receipt.wait.work_capacity = Some(capacity);
                if !pre.intent.requested.fits(capacity) {
                    return run.not_started(format!(
                        "the request exceeds this host's work capacity of {} mCPU, {} bytes and {} tasks, so no wait can admit it",
                        capacity.cpu_milli, capacity.memory_bytes, capacity.tasks
                    ));
                }
            }
            Err(error) => {
                run.receipt.wait.work_capacity_unknown = Some(error_text(&error));
            }
        }
    }
    let exit = admit_and_run(run, &endpoint, &pre, helper, signals);
    // Held adapter resources, such as a shared jobserver, outlive the run and
    // report what they observed once it has ended.
    for held in std::mem::take(&mut pre.hold) {
        run.receipt.adapter_after.push(held.finish());
    }
    exit
}

/// Admit, commit and launch until the run finishes or no further attempt may
/// be made.
fn admit_and_run(
    run: &mut Run,
    service: &dyn Service,
    pre: &Preflight,
    helper: &Path,
    signals: &Signals,
) -> Exit {
    loop {
        if let Some(signal) = signals.cancelled() {
            run.receipt.wait.cancelled_by_signal = Some(signal);
            run.not_started(format!("cancelled by signal {signal}"));
            return Exit::Signal(signal);
        }
        let key = service.key(uuid::Uuid::new_v4().to_string());
        run.receipt.wait.admissions += 1;
        let step = match run.admit(service, &key, &pre.digest, &pre.intent) {
            Admission::Admitted => match run.commit_admitted(service, &key) {
                Ok(permit) => {
                    if let Some(signal) = signals.cancelled() {
                        drop(permit);
                        run.abandon(service, &key, AbandonReason::SpawnFailed);
                        run.receipt.wait.cancelled_by_signal = Some(signal);
                        run.not_started(format!("cancelled by signal {signal}"));
                        return Exit::Signal(signal);
                    }
                    run.launch(service, &key, permit, pre, helper, signals)
                }
                Err(step) => step,
            },
            Admission::Waitable(note) => Step::Retry(note),
            Admission::Refused(note) => Step::Stop(note),
        };
        match step {
            Step::Finished(exit) => return exit,
            Step::Stop(reason) => return run.not_started(reason),
            Step::Retry(reason) => {
                if let Err(exit) = run.pause(signals) {
                    run.not_started(match exit {
                        Exit::Signal(signal) => {
                            format!("{reason}; the wait was cancelled by signal {signal}")
                        }
                        _ if run.receipt.wait.deadline_reached => {
                            format!("{reason}; the wait ended")
                        }
                        _ => format!("{reason}; retry with --wait to wait for capacity"),
                    });
                    return exit;
                }
            }
        }
    }
}

/// The helper binary next to the running `devguard`.
pub fn sibling_helper() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(exe
        .parent()
        .map(|dir| dir.join("devguard-launch"))
        .unwrap_or_else(|| PathBuf::from("devguard-launch")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use devguard_client::launch::HelperTicket;
    use devguard_client::protocol::LaunchGrant;
    use devguard_contract::{
        digest_bytes, Budget, Error, InstanceIdentity, ProcessIdentity, ResourceLevels, Result,
    };
    use std::cell::RefCell;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Scripted {
        /// The key of every admission request, in order.
        admitted: RefCell<Vec<AttemptKey>>,
        admit: RefCell<VecDeque<Result<AttemptRecord>>>,
        begin: RefCell<VecDeque<Result<LaunchGrant>>>,
        lookup: RefCell<VecDeque<Result<AttemptRecord>>>,
        abandon: RefCell<VecDeque<Result<AttemptRecord>>>,
        calls: RefCell<Vec<String>>,
    }

    fn next<T>(queue: &RefCell<VecDeque<Result<T>>>) -> Result<T> {
        queue
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Err(lost()))
    }

    impl Service for Scripted {
        fn key(&self, attempt_id: String) -> AttemptKey {
            AttemptKey {
                attempt_id,
                ..key()
            }
        }
        fn admit(&self, request: AdmissionRequest) -> Result<AttemptRecord> {
            self.calls.borrow_mut().push("admit".into());
            self.admitted.borrow_mut().push(request.key);
            next(&self.admit)
        }
        fn begin_launch(&self, _key: &AttemptKey) -> Result<LaunchGrant> {
            self.calls.borrow_mut().push("begin_launch".into());
            next(&self.begin)
        }
        fn lookup(&self, _key: &AttemptKey) -> Result<AttemptRecord> {
            self.calls.borrow_mut().push("lookup".into());
            next(&self.lookup)
        }
        fn abandon(&self, _key: &AttemptKey, reason: AbandonReason) -> Result<AttemptRecord> {
            self.calls.borrow_mut().push(format!("abandon:{reason:?}"));
            next(&self.abandon)
        }
        fn observe(&self, _key: &AttemptKey) -> Result<AttemptRecord> {
            self.calls.borrow_mut().push("observe".into());
            Err(lost())
        }
        fn ticket(&self, key: &AttemptKey) -> HelperTicket {
            HelperTicket {
                endpoint: "/nonexistent".into(),
                key: key.clone(),
                instance_id: "cli-test".into(),
            }
        }
    }

    /// A reply that never arrived.
    fn lost() -> Error {
        Error::new(ErrorCode::ResourceControlUnavailable, "reply lost")
    }

    fn key() -> AttemptKey {
        AttemptKey {
            consumer_id: "dev-cli".into(),
            consumer_generation: "g".into(),
            attempt_id: "a".into(),
        }
    }

    fn record(phase: AttemptPhase, release: Option<ReleaseReason>) -> AttemptRecord {
        AttemptRecord {
            key: key(),
            request_fingerprint: "f".into(),
            owner: InstanceIdentity {
                instance_id: "cli-test".into(),
                process: ProcessIdentity {
                    boot_id: "b".into(),
                    pid: 1,
                    start_ticks: 1,
                },
            },
            policy_revision: "r".into(),
            phase,
            reservation: None,
            plan: None,
            scope: None,
            applied: None,
            denial: None,
            tracking_lost: false,
            release_reason: release,
        }
    }

    fn denied(code: ErrorCode) -> AttemptRecord {
        AttemptRecord {
            denial: Some(code),
            ..record(AttemptPhase::Denied, None)
        }
    }

    fn grant(permit: Option<&str>) -> LaunchGrant {
        LaunchGrant {
            attempt: record(AttemptPhase::LaunchCommitted, None),
            permit: permit.map(|value| Secret::new(digest_bytes(value.as_bytes())).unwrap()),
        }
    }

    fn run() -> Run {
        Run {
            receipt: ExecReceipt::new("true".into()),
            started: Instant::now(),
            admitted_after: None,
            deadline: None,
            backoff: FIRST_BACKOFF,
        }
    }

    fn intent() -> ResourceIntent {
        ResourceIntent {
            profile: "interactive".into(),
            requested: Budget {
                cpu_milli: 1,
                memory_bytes: 1,
                tasks: 1,
            },
            minimum: ResourceLevels::MACOS,
        }
    }

    fn calls(service: &Scripted) -> Vec<String> {
        service.calls.borrow().clone()
    }

    fn preflight() -> Preflight {
        Preflight {
            program: PathBuf::from("/usr/bin/true"),
            original_args: Vec::new(),
            args: Vec::new(),
            cwd: PathBuf::from("/"),
            set: BTreeMap::new(),
            removed: Default::default(),
            tty: false,
            intent: intent(),
            budget_source: "command_line",
            digest: "0".repeat(64),
            adapter: crate::adapter::AdapterReport {
                adapter: "generic",
                selected_by: "command_line",
                parallelism: "not_transformed".into(),
                detail: serde_json::Value::Null,
            },
            project: None,
            hold: Vec::new(),
        }
    }

    /// Drive the admission loop; no helper is ever reached in these tests.
    fn drive(service: &Scripted, run: &mut Run, signals: &Signals) -> Exit {
        admit_and_run(
            run,
            service,
            &preflight(),
            Path::new("/nonexistent/devguard-launch"),
            signals,
        )
    }

    #[test]
    fn a_grant_lost_after_its_commit_is_released_as_never_received_and_never_recreated() {
        let service = Scripted::default();
        service.begin.borrow_mut().push_back(Err(lost()));
        service
            .lookup
            .borrow_mut()
            .push_back(Ok(record(AttemptPhase::LaunchCommitted, None)));
        service.abandon.borrow_mut().push_back(Ok(record(
            AttemptPhase::Released,
            Some(ReleaseReason::NoHelperCreated),
        )));
        let outcome = run().commit(&service, &key());
        assert!(matches!(outcome, Err(Step::Retry(_))));
        assert_eq!(
            calls(&service),
            ["begin_launch", "lookup", "abandon:GrantNotReceived"]
        );
    }

    #[test]
    fn a_commit_that_cannot_be_confirmed_is_neither_released_nor_retried() {
        let service = Scripted::default();
        service.begin.borrow_mut().push_back(Err(lost()));
        service.lookup.borrow_mut().push_back(Err(lost()));
        let mut run = run();
        let outcome = run.commit(&service, &key());
        assert!(matches!(outcome, Err(Step::Stop(ref note)) if note.contains("stays charged")));
        assert_eq!(calls(&service), ["begin_launch", "lookup"]);
    }

    #[test]
    fn a_reply_lost_before_the_commit_is_replayed_once_for_the_same_permit() {
        let service = Scripted::default();
        service.begin.borrow_mut().push_back(Err(lost()));
        service
            .lookup
            .borrow_mut()
            .push_back(Ok(record(AttemptPhase::Prepared, None)));
        service
            .begin
            .borrow_mut()
            .push_back(Ok(grant(Some("permit"))));
        let permit = run().commit(&service, &key()).ok().unwrap();
        assert_eq!(permit.expose(), digest_bytes(b"permit"));
        assert_eq!(calls(&service), ["begin_launch", "lookup", "begin_launch"]);
    }

    #[test]
    fn a_lost_grant_that_cannot_be_released_is_not_retried() {
        let service = Scripted::default();
        service.begin.borrow_mut().push_back(Ok(grant(None)));
        service
            .lookup
            .borrow_mut()
            .push_back(Ok(record(AttemptPhase::LaunchCommitted, None)));
        service
            .abandon
            .borrow_mut()
            .push_back(Ok(record(AttemptPhase::Suspect, None)));
        let outcome = run().commit(&service, &key());
        assert!(matches!(outcome, Err(Step::Stop(_))));
        assert_eq!(
            calls(&service),
            ["begin_launch", "lookup", "abandon:GrantNotReceived"]
        );
    }

    #[test]
    fn only_capacity_denials_are_waitable_and_a_lost_admission_reply_is_replayed_once() {
        let cases: Vec<(Vec<Result<AttemptRecord>>, &str, usize)> = vec![
            (
                vec![Ok(denied(ErrorCode::ResourceUnavailable))],
                "waitable",
                1,
            ),
            (
                vec![Ok(denied(ErrorCode::ResourcePolicyUnsupported))],
                "refused",
                1,
            ),
            (
                vec![Err(Error::new(
                    ErrorCode::ResourceUnavailable,
                    "pool occupied",
                ))],
                "waitable",
                1,
            ),
            (
                vec![Err(Error::new(ErrorCode::Unauthorized, "no"))],
                "refused",
                1,
            ),
            (
                vec![Err(lost()), Ok(record(AttemptPhase::Prepared, None))],
                "admitted",
                2,
            ),
            (vec![Err(lost()), Err(lost())], "refused", 2),
        ];
        for (replies, expected, admits) in cases {
            let service = Scripted::default();
            service.admit.borrow_mut().extend(replies);
            let outcome = run().admit(&service, &key(), &"0".repeat(64), &intent());
            let observed = match outcome {
                Admission::Admitted => "admitted",
                Admission::Waitable(_) => "waitable",
                Admission::Refused(_) => "refused",
            };
            assert_eq!(observed, expected);
            assert_eq!(calls(&service).len(), admits);
            // A replay repeats the same key; it never makes a second attempt.
            let keys = service.admitted.borrow();
            assert!(keys.iter().all(|admitted| *admitted == key()));
        }
    }

    #[test]
    fn a_wait_makes_a_new_attempt_after_each_refusal_until_its_deadline() {
        let service = Scripted::default();
        service
            .admit
            .borrow_mut()
            .extend((0..16).map(|_| Ok(denied(ErrorCode::ResourceUnavailable))));
        let mut run = run();
        run.deadline = Some(Instant::now() + Duration::from_millis(700));
        let started = Instant::now();
        let exit = drive(&service, &mut run, &Signals::inert());
        assert_eq!(exit, Exit::Code(NOT_STARTED));
        assert!(started.elapsed() >= Duration::from_millis(700));
        assert!(run.receipt.wait.deadline_reached);
        assert_eq!(run.receipt.result, ExecResult::NotStarted);
        // Backoff of 250 ms, then 500 ms cut to the deadline: at most three
        // admissions, each a new attempt with its own key.
        let keys = service.admitted.borrow().clone();
        assert!((2..=3).contains(&keys.len()), "{keys:?}");
        assert_eq!(run.receipt.wait.admissions as usize, keys.len());
        let distinct: std::collections::BTreeSet<_> =
            keys.iter().map(|key| key.attempt_id.clone()).collect();
        assert_eq!(distinct.len(), keys.len());
        assert!(calls(&service).iter().all(|call| call == "admit"));
    }

    #[test]
    fn a_signal_cancels_a_wait_before_or_during_its_backoff() {
        let service = Scripted::default();
        let signals = Signals::inert();
        signals.simulate(libc::SIGINT);
        let mut early = run();
        early.deadline = Some(Instant::now() + Duration::from_secs(60));
        assert_eq!(
            drive(&service, &mut early, &signals),
            Exit::Signal(libc::SIGINT)
        );
        assert!(calls(&service).is_empty(), "nothing is admitted");
        assert_eq!(early.receipt.wait.cancelled_by_signal, Some(libc::SIGINT));

        let service = Scripted::default();
        service
            .admit
            .borrow_mut()
            .push_back(Ok(denied(ErrorCode::ResourceUnavailable)));
        let signals = Signals::inert();
        let sender = signals.clone();
        let later = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            sender.simulate(libc::SIGTERM);
        });
        let mut waiting = run();
        waiting.backoff = Duration::from_secs(10);
        waiting.deadline = Some(Instant::now() + Duration::from_secs(60));
        let started = Instant::now();
        let exit = drive(&service, &mut waiting, &signals);
        later.join().unwrap();
        assert_eq!(exit, Exit::Signal(libc::SIGTERM));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(calls(&service), ["admit"]);
        assert_eq!(
            waiting.receipt.wait.cancelled_by_signal,
            Some(libc::SIGTERM)
        );
    }

    #[test]
    fn a_refusal_that_waiting_cannot_change_ends_the_wait_at_once() {
        let service = Scripted::default();
        service
            .admit
            .borrow_mut()
            .push_back(Ok(denied(ErrorCode::ResourcePolicyUnsupported)));
        let mut run = run();
        run.deadline = Some(Instant::now() + Duration::from_secs(60));
        assert_eq!(
            drive(&service, &mut run, &Signals::inert()),
            Exit::Code(NOT_STARTED)
        );
        assert_eq!(calls(&service), ["admit"]);
        assert!(!run.receipt.wait.deadline_reached);
    }

    #[test]
    fn a_launch_that_ended_before_ready_is_retried_only_when_released_transient_and_uninterrupted()
    {
        let refused = || {
            Some(LaunchOutcome::NotStarted(HelperPhase::Refused {
                code: ErrorCode::ResourceUnavailable,
                message: "pressure".into(),
            }))
        };
        let released = || {
            Ok(record(
                AttemptPhase::Released,
                Some(ReleaseReason::NoHelperCreated),
            ))
        };
        let settled = Exit::Code(NOT_AUTHORIZED_STATUS);
        /// (transcript outcome, no-helper reply, helper exit, signal received, step)
        type Case = (
            Option<LaunchOutcome>,
            Result<AttemptRecord>,
            Exit,
            Option<i32>,
            &'static str,
        );
        let cases: Vec<Case> = vec![
            (refused(), released(), settled, None, "retry"),
            (refused(), released(), settled, Some(libc::SIGINT), "signal"),
            (
                refused(),
                Ok(record(AttemptPhase::Suspect, None)),
                settled,
                None,
                "stop",
            ),
            (
                refused(),
                released(),
                Exit::Signal(libc::SIGKILL),
                None,
                "signal",
            ),
            (
                Some(LaunchOutcome::Lost { ready: false }),
                released(),
                settled,
                None,
                "stop",
            ),
        ];
        for (outcome, abandon, exit, interrupted, expected) in cases {
            let service = Scripted::default();
            service.abandon.borrow_mut().push_back(abandon);
            let mut run = run();
            let step = run.not_launched(&service, &key(), outcome, exit, interrupted);
            let observed = match step {
                Step::Retry(_) => "retry",
                Step::Stop(_) => "stop",
                Step::Finished(Exit::Signal(_)) => "signal",
                Step::Finished(Exit::Code(_)) => "finished",
            };
            assert_eq!(observed, expected);
            assert_eq!(run.receipt.result, ExecResult::NotStarted);
            if let Some(signal) = interrupted {
                assert_eq!(run.receipt.wait.cancelled_by_signal, Some(signal));
            }
        }
    }
}
