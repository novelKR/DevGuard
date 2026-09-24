//! `devguard exec`: one managed execution through the central authority.
//!
//! The CLI is the registered owner. It admits the command, commits the launch,
//! starts the `devguard-launch` helper as its direct child and waits for it.
//! It never runs a command outside that path: every failure before READY ends
//! with nothing started. A new attempt is made only after the previous one is
//! known not to have started, and only while an explicit `--wait` lasts.

use crate::args::ExecArgs;
use crate::authority::{Endpoint, Service};
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
/// How long the helper may take to report READY or its refusal.
const TRANSCRIPT_LIMIT: Duration = Duration::from_secs(30);
const FIRST_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(2);

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
        signals.attach(root);
        let mut terminal = Terminal::open();
        let handed = terminal
            .as_mut()
            .is_some_and(|terminal| terminal.give(root));
        let (outcome, phases) = report
            .wait(TRANSCRIPT_LIMIT)
            .unwrap_or((LaunchOutcome::Lost { ready: false }, Vec::new()));
        let stops = wait_for_exit(root, terminal.as_mut(), signals);
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
        if let Some(terminal) = terminal.as_mut() {
            terminal.take_back();
        }
        self.receipt.launch = Some(LaunchSummary {
            helper_pid: root as u32,
            phases: phases.clone(),
            outcome: format!("{outcome:?}"),
            terminal_handed: handed,
            stops_mirrored: stops,
        });
        let exit = match status {
            Ok(status) => match (status.code(), status.signal()) {
                (Some(code), _) => Exit::Code(code),
                (None, Some(signal)) => Exit::Signal(signal),
                _ => Exit::Code(NOT_STARTED),
            },
            Err(_) => Exit::Code(NOT_STARTED),
        };
        self.receipt.exit = Some(exit);
        let ready = phases.contains(&HelperPhase::Ready {});
        if !ready {
            // READY is written before the exec, so the executable never ran.
            let refusal = match &outcome {
                LaunchOutcome::NotStarted(HelperPhase::Refused { code, message }) => {
                    Some((*code, message.clone()))
                }
                _ => None,
            };
            let released = self.abandon(service, key, AbandonReason::HelperExited);
            self.observe_after_reap(service, key);
            let reason = match &refusal {
                Some((code, message)) => format!("the launch was refused: {code:?}: {message}"),
                None => format!("the helper ended before READY ({outcome:?})"),
            };
            if let Exit::Signal(signal) = exit {
                eprintln!("devguard: interrupted by signal {signal} before the command started");
                self.receipt.reason = Some(reason);
                return Step::Finished(Exit::Signal(signal));
            }
            let transient = matches!(
                refusal,
                Some((
                    ErrorCode::ResourceControlUnavailable | ErrorCode::ResourceUnavailable,
                    _
                ))
            );
            return if released && transient && exit == Exit::Code(NOT_AUTHORIZED_STATUS) {
                Step::Retry(reason)
            } else {
                Step::Stop(reason)
            };
        }
        self.observe_after_reap(service, key);
        self.receipt.result = match outcome {
            LaunchOutcome::ExecFailed { .. } => ExecResult::ExecFailed,
            _ => ExecResult::Completed,
        };
        Step::Finished(exit)
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

/// Wait until the root exits, without reaping it. A stopped workload is
/// mirrored: the CLI takes the terminal back and stops itself, so the shell
/// that started it regains control; when continued it hands the terminal
/// back and continues the workload. Returns how many stops were mirrored.
fn wait_for_exit(root: libc::pid_t, mut terminal: Option<&mut Terminal>, signals: &Signals) -> u32 {
    let mut stops = 0;
    loop {
        // SAFETY: info is plain data filled by waitid; WNOWAIT leaves the
        // root waitable, so it is reaped only after the observation.
        let (result, code) = unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let result = libc::waitid(
                libc::P_PID,
                root as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WSTOPPED | libc::WNOWAIT,
            );
            (result, info.si_code)
        };
        if result != 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return stops;
        }
        match code {
            libc::CLD_STOPPED | libc::CLD_TRAPPED => {
                // Consume the stop report without reaping anything.
                // SAFETY: as above; WNOHANG returns at once if it is gone.
                unsafe {
                    let mut info: libc::siginfo_t = std::mem::zeroed();
                    libc::waitid(
                        libc::P_PID,
                        root as libc::id_t,
                        &mut info,
                        libc::WSTOPPED | libc::WNOHANG,
                    );
                }
                stops += 1;
                if let Some(terminal) = terminal.as_deref_mut() {
                    terminal.take_back();
                }
                // SAFETY: raise has no preconditions; the default action of
                // SIGTSTP stops this process until it is continued.
                unsafe {
                    libc::raise(libc::SIGTSTP);
                }
                if let Some(terminal) = terminal.as_deref_mut() {
                    terminal.give(root);
                }
                signals.send(libc::SIGCONT);
            }
            libc::CLD_EXITED | libc::CLD_KILLED | libc::CLD_DUMPED => return stops,
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
    let signals = Signals::install();
    let exit = match &signals {
        Ok(signals) => execute(&mut run, &args, paths, helper, signals),
        Err(error) => run.not_started(format!("cannot install signal forwarding: {error}")),
    };
    if let Ok(signals) = &signals {
        run.receipt.signals.received = signals.received();
        run.receipt.signals.forwarded = signals.forwarded();
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
    paths: &AuthorityPaths,
    helper: &Path,
    signals: &Signals,
) -> Exit {
    if !helper.is_absolute() || !helper.is_file() {
        return run.not_started(format!("the launch helper {} is missing", helper.display()));
    }
    let endpoint = match Endpoint::open(paths) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            return run.not_started(format!("cannot use the authority: {}", error_text(&error)))
        }
    };
    let environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    let pre = match preflight::prepare(args, &endpoint.config, &environment) {
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
    loop {
        if let Some(signal) = signals.cancelled() {
            run.receipt.wait.cancelled_by_signal = Some(signal);
            run.not_started(format!("cancelled by signal {signal}"));
            return Exit::Signal(signal);
        }
        let key = endpoint.key(uuid::Uuid::new_v4().to_string());
        run.receipt.wait.admissions += 1;
        let step = match run.admit(&endpoint, &key, &pre.digest, &pre.intent) {
            Admission::Admitted => match run.commit_admitted(&endpoint, &key) {
                Ok(permit) => {
                    if let Some(signal) = signals.cancelled() {
                        drop(permit);
                        run.abandon(&endpoint, &key, AbandonReason::SpawnFailed);
                        run.receipt.wait.cancelled_by_signal = Some(signal);
                        run.not_started(format!("cancelled by signal {signal}"));
                        return Exit::Signal(signal);
                    }
                    run.launch(&endpoint, &key, permit, &pre, helper, signals)
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
        fn admit(&self, _request: AdmissionRequest) -> Result<AttemptRecord> {
            self.calls.borrow_mut().push("admit".into());
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
        }
    }
}
