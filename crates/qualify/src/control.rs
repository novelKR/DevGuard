//! Standalone control probes: the status and termination latency an owner
//! observes from the authority, measured on the probe's own managed targets.
//!
//! The probe is an ordinary owner of the `dev-cli` consumer. It admits small
//! targets, commits their launch and starts them through the stable helper,
//! observes each exited target before reaping it, and never runs anything
//! unmanaged. A status sample is a `Lookup` of its running target through a
//! fresh registered session, the way the command-line owner makes every
//! call. A termination sample is a `Terminate` request answered by
//! `Terminated`; the target's exit and the attempt's release are reported
//! separately, because delivery of a signal is not release evidence.
//!
//! Every scheduled sample is written, including one the probe could not take:
//! a slot that passed while an earlier call was still in flight is recorded
//! as missed, and a target that could not be started is recorded with the
//! stage and error that stopped it. Only an admission refused for capacity or
//! pressure until the wait ends is not a sample.

use devguard_cli::authority::{Endpoint, Service};
use devguard_cli::preflight::NO_TIMEOUT_MS;
use devguard_client::launch::{helper_command, LaunchOutcome};
use devguard_client::protocol::{AbandonReason, StopSignal};
use devguard_contract::{
    AdmissionRequest, AttemptKey, AttemptPhase, Budget, Error, ErrorCode, ExecutionMeaning,
    ResourceIntent, ResourceLevels, Result,
};
use devguard_daemon::paths::AuthorityPaths;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Every target runs this program for a bounded time, so a target outlives
/// a probe that dies abruptly by at most its own lifetime.
const TARGET: &str = "/bin/sleep";
/// The working directory the targets run in, as their execution meaning says.
const TARGET_DIRECTORY: &str = "/";
/// A termination target lives at most this long.
const TERMINATION_TARGET_LIFETIME: Duration = Duration::from_secs(600);
/// The status target lives this much longer than the sampling.
const STATUS_TARGET_MARGIN: Duration = Duration::from_secs(600);
/// How long a started helper may take to report that the target runs.
const READY_LIMIT: Duration = Duration::from_secs(10);
/// How long a signalled target may take to exit and its attempt to be released.
const RELEASE_LIMIT: Duration = Duration::from_secs(30);
/// Pauses between admissions refused for capacity or pressure.
const FIRST_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(2);
const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Options {
    /// How long samples are taken; the probe then stops its status target.
    pub duration: Duration,
    pub status_interval: Duration,
    pub terminate_period: Duration,
    /// How long a target's admission is retried while refused for capacity
    /// or pressure. The wait is recorded but is not a control latency.
    pub admission_wait: Duration,
    pub target: Budget,
}

impl Options {
    /// The protocol's sampling: a status call every second and a
    /// termination every 20 seconds, on targets of 50 mCPU, 16 MiB, 2 tasks.
    pub fn protocol(duration: Duration) -> Self {
        Self {
            duration,
            status_interval: Duration::from_secs(1),
            terminate_period: Duration::from_secs(20),
            admission_wait: Duration::from_secs(120),
            target: Budget {
                cpu_milli: 50,
                memory_bytes: 16 * MIB,
                tasks: 2,
            },
        }
    }
}

/// Raw samples, one JSON object per line, written by the probe's threads.
struct Recorder<'a> {
    out: Mutex<&'a mut (dyn Write + Send)>,
    started: Instant,
    written: AtomicU64,
    failed: AtomicBool,
}

impl<'a> Recorder<'a> {
    fn new(out: &'a mut (dyn Write + Send)) -> Self {
        Self {
            out: Mutex::new(out),
            started: Instant::now(),
            written: AtomicU64::new(0),
            failed: AtomicBool::new(false),
        }
    }

    fn record(&self, kind: &str, mut sample: Value) {
        sample["kind"] = kind.into();
        sample["t_ms"] = ms(self.started.elapsed()).into();
        sample["unix_ms"] = unix_ms().into();
        let mut out = self
            .out
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if writeln!(out, "{sample}").and_then(|_| out.flush()).is_err() {
            self.failed.store(true, Ordering::Relaxed);
        } else {
            self.written.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// When sampling must end: the caller's stop, or this process's parent going
/// away, so an orphaned probe settles its targets instead of sampling on.
struct Watch<'a> {
    stop: &'a AtomicBool,
    parent: libc::pid_t,
}

impl Watch<'_> {
    fn stopped(&self) -> bool {
        // SAFETY: getppid has no preconditions.
        self.stop.load(Ordering::Relaxed) || unsafe { libc::getppid() } != self.parent
    }
}

/// A target this probe started and must settle.
struct Target {
    key: AttemptKey,
    child: Child,
}

/// Why no target was started.
enum Unstarted {
    /// Admission stayed refused for capacity or pressure until the wait
    /// ended. This is not a control sample.
    Refused(String),
    /// Any other failure: a control operation that did not succeed, with the
    /// stage it failed at and the error's code when there was one.
    Failed {
        stage: &'static str,
        code: Option<ErrorCode>,
        note: String,
    },
}

impl Unstarted {
    fn failed(stage: &'static str, error: &Error, note: String) -> Self {
        Self::Failed {
            stage,
            code: Some(error.code),
            note,
        }
    }

    fn sample(&self) -> Value {
        match self {
            Self::Refused(note) => json!({"outcome": "not_started", "note": note}),
            Self::Failed { stage, code, note } => {
                json!({"outcome": "error", "stage": stage, "code": code, "message": note})
            }
        }
    }

    fn into_error(self) -> Error {
        match self {
            Self::Refused(note) => Error::new(ErrorCode::ResourceUnavailable, note),
            Self::Failed { code, note, .. } => {
                Error::new(code.unwrap_or(ErrorCode::ResourceControlUnavailable), note)
            }
        }
    }
}

/// How a signalled target ended.
struct Settled {
    /// When the target exited after an acknowledged termination, if it did
    /// within the limit.
    exited_ms: Option<f64>,
    /// The probe had to stop its own child, so the release that followed is
    /// not evidence of the termination.
    killed_by_probe: bool,
    released_ms: Option<f64>,
    phase: Option<AttemptPhase>,
    release_reason: Value,
    observed: Value,
}

/// Run the probes for `options.duration` against the authority at `paths`,
/// starting targets through `helper`, and write raw samples to `out`. A set
/// `stop`, or the loss of this process's parent, ends sampling early; the
/// probe still settles its targets. Returns a summary of what was written.
pub fn run(
    paths: &AuthorityPaths,
    helper: &Path,
    options: &Options,
    out: &mut (dyn Write + Send),
    stop: &AtomicBool,
) -> Result<Value> {
    let watch = Watch {
        stop,
        // SAFETY: getppid has no preconditions.
        parent: unsafe { libc::getppid() },
    };
    let endpoint = Endpoint::open(paths)?;
    endpoint.register()?;
    let recorder = Recorder::new(out);
    recorder.record(
        "header",
        json!({"schema": "devguard-control-probe/v2", "endpoint": endpoint.summary(),
               "helper": helper, "target": TARGET, "target_directory": TARGET_DIRECTORY,
               "options": {"duration_ms": ms(options.duration),
                           "status_interval_ms": ms(options.status_interval),
                           "terminate_period_ms": ms(options.terminate_period),
                           "admission_wait_ms": ms(options.admission_wait),
                           "target": options.target}}),
    );
    let lifetime = options.duration + STATUS_TARGET_MARGIN;
    let started = start(
        &endpoint, helper, options, &recorder, "status", lifetime, &watch, &mut None,
    );
    let mut status_target = match started {
        Ok(target) => target,
        Err(unstarted) => {
            recorder.record("status_target", unstarted.sample());
            return Err(unstarted.into_error());
        }
    };
    let deadline = Instant::now() + options.duration;
    let key = status_target.key.clone();
    std::thread::scope(|scope| {
        scope.spawn(|| statuses(&endpoint, &key, options, &recorder, deadline, &watch));
        scope.spawn(|| terminations(&endpoint, helper, options, &recorder, deadline, &watch));
    });
    let asked = Instant::now();
    let answer = terminate(&endpoint, &status_target.key);
    let settled = settle(&endpoint, &mut status_target, asked, answer.is_ok());
    recorder.record(
        "status_target",
        json!({"outcome": "settled", "key": status_target.key, "terminated": answer.is_ok(),
               "exited_ms": settled.exited_ms, "killed_by_probe": settled.killed_by_probe,
               "released_ms": settled.released_ms, "phase": settled.phase,
               "release_reason": settled.release_reason}),
    );
    if recorder.failed.load(Ordering::Relaxed) {
        return Err(Error::new(
            ErrorCode::ResourceControlUnavailable,
            "cannot write the probe samples",
        ));
    }
    Ok(json!({"samples": recorder.written.load(Ordering::Relaxed),
              "status_target_released": settled.phase == Some(AttemptPhase::Released),
              "stopped_early": watch.stopped()}))
}

/// Record every slot of `period` that passed before `now` as missed, and
/// return the next slot to take.
fn missed_slots(
    mut next: Instant,
    period: Duration,
    now: Instant,
    recorder: &Recorder,
    kind: &str,
) -> Instant {
    while next + period <= now {
        next += period;
        recorder.record(kind, json!({"outcome": "missed_slot"}));
    }
    next + period
}

/// A status call every interval until the deadline. Calls are serial: a
/// slow reply is itself the slow sample, and every slot it overran is
/// recorded as missed.
fn statuses(
    endpoint: &Endpoint,
    key: &AttemptKey,
    options: &Options,
    recorder: &Recorder,
    deadline: Instant,
    watch: &Watch,
) {
    let mut next = Instant::now();
    loop {
        if !pause_until(next, deadline, watch) {
            break;
        }
        let asked = Instant::now();
        let answer = endpoint.lookup(key);
        let latency = asked.elapsed();
        recorder.record(
            "status",
            match answer {
                Ok(record) => json!({"outcome": "answered", "latency_ms": ms(latency),
                                      "phase": record.phase}),
                Err(error) => json!({"outcome": "error", "latency_ms": ms(latency),
                                      "code": error.code, "message": error.message}),
            },
        );
        next = missed_slots(
            next,
            options.status_interval,
            Instant::now().min(deadline),
            recorder,
            "status",
        );
    }
}

/// A fresh target every period until the deadline, each signalled and
/// followed until its attempt is released.
fn terminations(
    endpoint: &Endpoint,
    helper: &Path,
    options: &Options,
    recorder: &Recorder,
    deadline: Instant,
    watch: &Watch,
) {
    // Offset from the status calls so both rarely start in the same instant.
    let mut next = Instant::now() + options.terminate_period / 2;
    loop {
        if !pause_until(next, deadline, watch) {
            break;
        }
        let (sample, admitted, answered) =
            terminate_once(endpoint, helper, options, recorder, watch);
        recorder.record("terminate", sample);
        // Slots that passed while this target's admission waited for capacity
        // or pressure are not samples, nor are slots that passed while its exit
        // and release were followed, which are reported separately. Slots its
        // launch and termination request overran are missed samples.
        let now = Instant::now().min(deadline);
        let mut slot = next + options.terminate_period;
        while slot <= now {
            recorder.record("terminate", overrun(slot, admitted, answered));
            slot += options.terminate_period;
        }
        next = slot;
    }
}

/// What a termination slot that passed during an earlier cycle was: waiting
/// for admission (not a sample), overrun by the launch and termination
/// request (a missed sample), or passed while the target's exit and release
/// were followed (not a sample; they are reported separately).
fn overrun(slot: Instant, admitted: Option<Instant>, answered: Option<Instant>) -> Value {
    match (admitted, answered) {
        (Some(_), Some(at)) if slot >= at => {
            json!({"outcome": "not_sampled", "note": "the previous target was still settling"})
        }
        (Some(at), _) if slot >= at => json!({"outcome": "missed_slot"}),
        _ => json!({"outcome": "not_started", "note": "admission waited for capacity or pressure"}),
    }
}

/// One termination sample on a fresh target, when its admission ended and
/// when the termination request was answered.
fn terminate_once(
    endpoint: &Endpoint,
    helper: &Path,
    options: &Options,
    recorder: &Recorder,
    watch: &Watch,
) -> (Value, Option<Instant>, Option<Instant>) {
    let lifetime = TERMINATION_TARGET_LIFETIME;
    let mut admitted = None;
    let started = start(
        endpoint,
        helper,
        options,
        recorder,
        "terminate",
        lifetime,
        watch,
        &mut admitted,
    );
    let mut target = match started {
        Ok(target) => target,
        Err(unstarted) => return (unstarted.sample(), admitted, None),
    };
    let asked = Instant::now();
    let answer = terminate(endpoint, &target.key);
    let ack = asked.elapsed();
    let answered = Instant::now();
    let settled = settle(endpoint, &mut target, asked, answer.is_ok());
    let mut sample = json!({"key": target.key, "ack_ms": ms(ack),
                            "exited_ms": settled.exited_ms, "killed_by_probe": settled.killed_by_probe,
                            "released_ms": settled.released_ms, "phase": settled.phase,
                            "release_reason": settled.release_reason, "observed": settled.observed});
    match answer {
        Ok(termination) => {
            sample["outcome"] = "acknowledged".into();
            sample["signalled"] = termination.signalled.into();
            sample["complete"] = termination.complete.into();
        }
        Err(error) => {
            sample["outcome"] = "error".into();
            sample["stage"] = "terminate".into();
            sample["code"] = json!(error.code);
            sample["message"] = error.message.into();
        }
    }
    (sample, admitted, Some(answered))
}

/// Sleep until `at`, returning false once the deadline or a stop comes first.
fn pause_until(at: Instant, deadline: Instant, watch: &Watch) -> bool {
    loop {
        let now = Instant::now();
        if watch.stopped() || now >= deadline {
            return false;
        }
        if now >= at {
            return true;
        }
        std::thread::sleep(
            (at - now)
                .min(deadline - now)
                .min(Duration::from_millis(50)),
        );
    }
}

fn terminate(
    endpoint: &Endpoint,
    key: &AttemptKey,
) -> Result<devguard_client::protocol::Termination> {
    endpoint
        .session()?
        .terminate(key.clone(), StopSignal::Terminate)
}

/// Admit, commit and start one target that lives `lifetime` through the
/// stable helper, setting `admitted` when admission succeeds. Admission
/// refused for capacity or pressure is retried until `admission_wait`; any
/// other failure is reported to the authority before it is returned.
#[allow(clippy::too_many_arguments)]
fn start(
    endpoint: &Endpoint,
    helper: &Path,
    options: &Options,
    recorder: &Recorder,
    role: &str,
    lifetime: Duration,
    watch: &Watch,
    admitted: &mut Option<Instant>,
) -> std::result::Result<Target, Unstarted> {
    let key = endpoint.key(format!("q-{}", uuid::Uuid::new_v4().simple()));
    let seconds = lifetime.as_secs().max(1).to_string();
    let intent = ResourceIntent {
        profile: "interactive".into(),
        requested: options.target,
        minimum: ResourceLevels::MACOS,
    };
    let digest = ExecutionMeaning {
        executable_identity: TARGET.into(),
        cwd_identity: TARGET_DIRECTORY.into(),
        argv: vec![TARGET.into(), seconds.clone()],
        environment_changes: BTreeMap::new(),
        tty: false,
        timeout_ms: NO_TIMEOUT_MS,
        resources: intent.clone(),
    }
    .digest()
    .map_err(|error| Unstarted::failed("meaning", &error, error.message.clone()))?;
    let request = AdmissionRequest {
        key: key.clone(),
        execution_digest: digest,
        intent,
    };
    let asked = Instant::now();
    let mut backoff = FIRST_BACKOFF;
    let mut refusals = 0u32;
    loop {
        let refused = match endpoint.admit(request.clone()) {
            Ok(record) if record.phase == AttemptPhase::Prepared => break,
            Ok(record) if record.denial == Some(ErrorCode::ResourceUnavailable) => {
                format!("{:?}", record.denial)
            }
            Ok(record) => {
                return Err(Unstarted::Failed {
                    stage: "admit",
                    code: record.denial,
                    note: format!("admission refused: {:?} {:?}", record.phase, record.denial),
                })
            }
            // A full instance pool is a capacity condition like a denial.
            Err(error) if error.code == ErrorCode::ResourceUnavailable => error.message,
            Err(error) => {
                let note = format!("admission failed: {}", error.message);
                return Err(Unstarted::failed("admit", &error, note));
            }
        };
        refusals += 1;
        if watch.stopped() || asked.elapsed() + backoff > options.admission_wait {
            recorder.record(
                "admission",
                json!({"role": role, "outcome": "refused", "refusals": refusals,
                       "waited_ms": ms(asked.elapsed()), "last": refused}),
            );
            return Err(Unstarted::Refused(format!(
                "not admitted within the wait: {refused}"
            )));
        }
        std::thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
    *admitted = Some(Instant::now());
    recorder.record(
        "admission",
        json!({"role": role, "outcome": "admitted", "refusals": refusals,
               "waited_ms": ms(asked.elapsed()), "key": key}),
    );
    let permit = match endpoint.begin_launch(&key) {
        Ok(grant) => match grant.permit {
            Some(permit) => permit,
            None => {
                let note = abandoned(endpoint, &key, AbandonReason::GrantNotReceived);
                return Err(Unstarted::Failed {
                    stage: "begin_launch",
                    code: None,
                    note: format!("the launch grant carried no permit; {note}"),
                });
            }
        },
        Err(error) => {
            let note = abandoned(endpoint, &key, AbandonReason::GrantNotReceived);
            let note = format!("launch commit failed: {}; {note}", error.message);
            return Err(Unstarted::failed("begin_launch", &error, note));
        }
    };
    let prepared = helper_command(
        helper,
        &endpoint.ticket(&key),
        &permit,
        Path::new(TARGET),
        &[OsString::from(&seconds)],
    );
    drop(permit);
    let (mut command, report) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let note = abandoned(endpoint, &key, AbandonReason::SpawnFailed);
            let note = format!("cannot prepare the helper: {}; {note}", error.message);
            return Err(Unstarted::failed("spawn", &error, note));
        }
    };
    command
        .command_mut()
        .current_dir(TARGET_DIRECTORY)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // The helper leads its own group, which becomes the target's scope.
        .process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let note = abandoned(endpoint, &key, AbandonReason::SpawnFailed);
            return Err(Unstarted::Failed {
                stage: "spawn",
                code: None,
                note: format!("cannot start the helper: {error}; {note}"),
            });
        }
    };
    match report.wait(READY_LIMIT) {
        Ok((LaunchOutcome::Started, _)) => Ok(Target { key, child }),
        outcome => {
            // Whatever the helper did, its group is this probe's own child:
            // stop it and observe the attempt before reaping it. Then report,
            // as the command-line owner does, that no helper exists any more,
            // and observe again: an unclaimed grant is released as never
            // started, and a claimed one is settled through its scope.
            let pid = child.id() as libc::pid_t;
            // SAFETY: kill has no memory preconditions; the group is the
            // helper's own, created by process_group(0) above.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let _ = exited_unreaped(pid, RELEASE_LIMIT);
            let _ = endpoint.observe(&key);
            let _ = child.wait();
            let reported = abandoned(endpoint, &key, AbandonReason::HelperExited);
            let _ = endpoint.observe(&key);
            let note = match outcome {
                Ok((outcome, phases)) => {
                    format!("the target did not start: {outcome:?} {phases:?}; {reported}")
                }
                Err(error) => {
                    format!(
                        "cannot read the helper report: {}; {reported}",
                        error.message
                    )
                }
            };
            Err(Unstarted::Failed {
                stage: "helper",
                code: None,
                note,
            })
        }
    }
}

/// Report that this owner holds no helper for the grant.
fn abandoned(endpoint: &Endpoint, key: &AttemptKey, reason: AbandonReason) -> String {
    match endpoint.abandon(key, reason) {
        Ok(record) => format!("reported {reason:?}, now {:?}", record.phase),
        Err(error) => format!("the {reason:?} report failed: {}", error.message),
    }
}

/// Wait for a signalled target to exit without reaping it, observe the
/// attempt while its root is unreaped, reap it, and wait for the release.
/// When the authority did not acknowledge the termination, or the target
/// did not exit within the limit, the probe stops its own child so no target
/// outlives the probe; that exit and release are then marked as forced by
/// the probe and are not evidence of the termination.
fn settle(endpoint: &Endpoint, target: &mut Target, since: Instant, answered: bool) -> Settled {
    let pid = target.child.id() as libc::pid_t;
    let exited = if answered {
        exited_unreaped(pid, RELEASE_LIMIT).then(|| ms(since.elapsed()))
    } else {
        None
    };
    let killed_by_probe = exited.is_none();
    if killed_by_probe {
        // SAFETY: as in start: the group is this probe's own child's.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
        exited_unreaped(pid, RELEASE_LIMIT);
    }
    let observed = match endpoint.observe(&target.key) {
        Ok(record) => json!({"phase": record.phase}),
        Err(error) => json!({"code": error.code, "message": error.message}),
    };
    let _ = target.child.wait();
    let mut settled = Settled {
        exited_ms: exited,
        killed_by_probe,
        released_ms: None,
        phase: None,
        release_reason: Value::Null,
        observed,
    };
    let limit = Instant::now() + RELEASE_LIMIT;
    loop {
        if let Ok(record) = endpoint.lookup(&target.key) {
            settled.phase = Some(record.phase);
            if record.phase == AttemptPhase::Released {
                settled.released_ms = Some(ms(since.elapsed()));
                settled.release_reason = json!(record.release_reason);
                return settled;
            }
        }
        if Instant::now() >= limit {
            return settled;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether `pid`, this process's child, exits within `limit`. The exit is
/// left unconsumed so the owner can observe before it reaps.
fn exited_unreaped(pid: libc::pid_t, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        // SAFETY: info is plain data filled by waitid; WNOWAIT leaves the
        // child waitable and WNOHANG returns at once when nothing changed.
        let (result, code) = unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let result = libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
            );
            (result, info.si_code)
        };
        if result == 0 && matches!(code, libc::CLD_EXITED | libc::CLD_KILLED | libc::CLD_DUMPED) {
            return true;
        }
        if result != 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
        {
            return false;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn ms(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1_000_000.0).round() / 1000.0
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_that_passed_during_a_call_are_recorded_as_missed() {
        let mut out = Vec::new();
        let recorder = Recorder::new(&mut out);
        let start = Instant::now();
        let period = Duration::from_millis(100);
        // A call that took 350 ms overran three later slots.
        let next = missed_slots(
            start,
            period,
            start + Duration::from_millis(350),
            &recorder,
            "status",
        );
        assert_eq!(next, start + Duration::from_millis(400));
        assert_eq!(recorder.written.load(Ordering::Relaxed), 3);
        // A call within its slot misses nothing.
        let next = missed_slots(
            next,
            period,
            next + Duration::from_millis(40),
            &recorder,
            "status",
        );
        assert_eq!(next, start + Duration::from_millis(500));
        assert_eq!(recorder.written.load(Ordering::Relaxed), 3);
        drop(recorder);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("\"missed_slot\"").count(), 3);
    }

    #[test]
    fn termination_slots_are_classified_by_what_overran_them() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        // Admitted at 100 ms and answered at 200 ms.
        let (admitted, answered) = (Some(at(100)), Some(at(200)));
        assert_eq!(
            overrun(at(50), admitted, answered)["outcome"],
            "not_started"
        );
        assert_eq!(
            overrun(at(150), admitted, answered)["outcome"],
            "missed_slot"
        );
        assert_eq!(
            overrun(at(250), admitted, answered)["outcome"],
            "not_sampled"
        );
        // A target that failed after admission: its later slots are missed.
        assert_eq!(overrun(at(250), admitted, None)["outcome"], "missed_slot");
        // Never admitted: every slot waited for admission.
        assert_eq!(overrun(at(250), None, None)["outcome"], "not_started");
    }
}
