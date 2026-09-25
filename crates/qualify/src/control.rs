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

/// Every target runs this program, which does not end by itself during a probe.
const TARGET: &str = "/bin/sleep";
const TARGET_SECONDS: &str = "86400";
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

/// A target this probe started and must settle.
struct Target {
    key: AttemptKey,
    child: Child,
}

/// How a signalled target ended.
struct Settled {
    exited_ms: Option<f64>,
    released_ms: Option<f64>,
    phase: Option<AttemptPhase>,
    release_reason: Value,
    observed: Value,
}

/// Run the probes for `options.duration` against the authority at `paths`,
/// starting targets through `helper`, and write raw samples to `out`. A set
/// `stop` ends sampling early; the probe still settles its targets. Returns
/// a summary of what was written.
pub fn run(
    paths: &AuthorityPaths,
    helper: &Path,
    options: &Options,
    out: &mut (dyn Write + Send),
    stop: &AtomicBool,
) -> Result<Value> {
    let endpoint = Endpoint::open(paths)?;
    endpoint.register()?;
    let recorder = Recorder::new(out);
    recorder.record(
        "header",
        json!({"schema": "devguard-control-probe/v1", "endpoint": endpoint.summary(),
               "helper": helper, "target": [TARGET, TARGET_SECONDS],
               "options": {"duration_ms": ms(options.duration),
                           "status_interval_ms": ms(options.status_interval),
                           "terminate_period_ms": ms(options.terminate_period),
                           "admission_wait_ms": ms(options.admission_wait),
                           "target": options.target}}),
    );
    let mut status_target = start(&endpoint, helper, options, &recorder, "status", stop)
        .map_err(|note| Error::new(ErrorCode::ResourceUnavailable, note))?;
    let deadline = Instant::now() + options.duration;
    let key = status_target.key.clone();
    std::thread::scope(|scope| {
        scope.spawn(|| statuses(&endpoint, &key, options, &recorder, deadline, stop));
        scope.spawn(|| terminations(&endpoint, helper, options, &recorder, deadline, stop));
    });
    let asked = Instant::now();
    let answer = terminate(&endpoint, &status_target.key);
    let settled = settle(&endpoint, &mut status_target, asked, answer.is_ok());
    recorder.record(
        "status_target",
        json!({"key": status_target.key, "terminated": answer.is_ok(),
               "exited_ms": settled.exited_ms, "released_ms": settled.released_ms,
               "phase": settled.phase, "release_reason": settled.release_reason}),
    );
    if recorder.failed.load(Ordering::Relaxed) {
        return Err(Error::new(
            ErrorCode::ResourceControlUnavailable,
            "cannot write the probe samples",
        ));
    }
    Ok(json!({"samples": recorder.written.load(Ordering::Relaxed),
              "status_target_released": settled.phase == Some(AttemptPhase::Released)}))
}

/// A status call every interval until the deadline. Calls are serial: a
/// slow reply delays the next call and is itself the slow sample.
fn statuses(
    endpoint: &Endpoint,
    key: &AttemptKey,
    options: &Options,
    recorder: &Recorder,
    deadline: Instant,
    stop: &AtomicBool,
) {
    let mut next = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if !pause_until(next, deadline, stop) {
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
        next += options.status_interval;
        next = next.max(Instant::now());
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
    stop: &AtomicBool,
) {
    // Offset from the status calls so both rarely start in the same instant.
    let mut next = Instant::now() + options.terminate_period / 2;
    while !stop.load(Ordering::Relaxed) {
        if !pause_until(next, deadline, stop) {
            break;
        }
        next += options.terminate_period;
        let mut target = match start(endpoint, helper, options, recorder, "terminate", stop) {
            Ok(target) => target,
            Err(note) => {
                recorder.record("terminate", json!({"outcome": "not_started", "note": note}));
                continue;
            }
        };
        let asked = Instant::now();
        let answer = terminate(endpoint, &target.key);
        let ack = asked.elapsed();
        let settled = settle(endpoint, &mut target, asked, answer.is_ok());
        let mut sample = json!({"key": target.key, "ack_ms": ms(ack),
                                "exited_ms": settled.exited_ms, "released_ms": settled.released_ms,
                                "phase": settled.phase, "release_reason": settled.release_reason,
                                "observed": settled.observed});
        match answer {
            Ok(termination) => {
                sample["outcome"] = "acknowledged".into();
                sample["signalled"] = termination.signalled.into();
                sample["complete"] = termination.complete.into();
            }
            Err(error) => {
                sample["outcome"] = "error".into();
                sample["code"] = json!(error.code);
                sample["message"] = error.message.into();
            }
        }
        recorder.record("terminate", sample);
        next = next.max(Instant::now());
    }
}

/// Sleep until `at`, returning false once the deadline or a stop comes first.
fn pause_until(at: Instant, deadline: Instant, stop: &AtomicBool) -> bool {
    loop {
        let now = Instant::now();
        if stop.load(Ordering::Relaxed) || now >= deadline {
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

/// Admit, commit and start one target through the stable helper. Admission
/// refused for capacity or pressure is retried until `admission_wait`; any
/// other failure is reported to the authority before it is returned.
fn start(
    endpoint: &Endpoint,
    helper: &Path,
    options: &Options,
    recorder: &Recorder,
    role: &str,
    stop: &AtomicBool,
) -> std::result::Result<Target, String> {
    let key = endpoint.key(format!("q-{}", uuid::Uuid::new_v4().simple()));
    let intent = ResourceIntent {
        profile: "interactive".into(),
        requested: options.target,
        minimum: ResourceLevels::MACOS,
    };
    let digest = ExecutionMeaning {
        executable_identity: TARGET.into(),
        cwd_identity: "/".into(),
        argv: vec![TARGET.into(), TARGET_SECONDS.into()],
        environment_changes: BTreeMap::new(),
        tty: false,
        timeout_ms: NO_TIMEOUT_MS,
        resources: intent.clone(),
    }
    .digest()
    .map_err(|error| error.message)?;
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
                return Err(format!(
                    "admission refused: {:?} {:?}",
                    record.phase, record.denial
                ))
            }
            // A full instance pool is a capacity condition like a denial.
            Err(error) if error.code == ErrorCode::ResourceUnavailable => error.message,
            Err(error) => return Err(format!("admission failed: {}", error.message)),
        };
        refusals += 1;
        if stop.load(Ordering::Relaxed) || asked.elapsed() + backoff > options.admission_wait {
            recorder.record(
                "admission",
                json!({"role": role, "outcome": "refused", "refusals": refusals,
                       "waited_ms": ms(asked.elapsed()), "last": refused}),
            );
            return Err(format!("not admitted within the wait: {refused}"));
        }
        std::thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
    recorder.record(
        "admission",
        json!({"role": role, "outcome": "admitted", "refusals": refusals,
               "waited_ms": ms(asked.elapsed()), "key": key}),
    );
    let permit = match endpoint.begin_launch(&key) {
        Ok(grant) => match grant.permit {
            Some(permit) => permit,
            None => {
                return Err(abandoned(
                    endpoint,
                    &key,
                    "the launch grant carried no permit",
                ))
            }
        },
        Err(error) => {
            return Err(abandoned(
                endpoint,
                &key,
                &format!("launch commit failed: {}", error.message),
            ))
        }
    };
    let prepared = helper_command(
        helper,
        &endpoint.ticket(&key),
        &permit,
        Path::new(TARGET),
        &[OsString::from(TARGET_SECONDS)],
    );
    drop(permit);
    let (mut command, report) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            return Err(spawn_failed(
                endpoint,
                &key,
                &format!("cannot prepare the helper: {}", error.message),
            ))
        }
    };
    command
        .command_mut()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // The helper leads its own group, which becomes the target's scope.
        .process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(spawn_failed(
                endpoint,
                &key,
                &format!("cannot start the helper: {error}"),
            ))
        }
    };
    match report.wait(READY_LIMIT) {
        Ok((LaunchOutcome::Started, _)) => Ok(Target { key, child }),
        outcome => {
            // Whatever the helper did, its group is this probe's own child:
            // stop it, observe the attempt before reaping, then report.
            // SAFETY: kill has no memory preconditions; the group is the
            // helper's own, created by process_group(0) above.
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = exited_unreaped(child.id() as libc::pid_t, RELEASE_LIMIT);
            let _ = endpoint.observe(&key);
            let _ = child.wait();
            let note = match outcome {
                Ok((outcome, phases)) => {
                    format!("the target did not start: {outcome:?} {phases:?}")
                }
                Err(error) => format!("cannot read the helper report: {}", error.message),
            };
            Err(note)
        }
    }
}

/// Report that no helper exists for the grant, returning `note`.
fn abandoned(endpoint: &Endpoint, key: &AttemptKey, note: &str) -> String {
    match endpoint.abandon(key, AbandonReason::GrantNotReceived) {
        Ok(record) => format!("{note}; reported, now {:?}", record.phase),
        Err(error) => format!("{note}; the report failed: {}", error.message),
    }
}

fn spawn_failed(endpoint: &Endpoint, key: &AttemptKey, note: &str) -> String {
    match endpoint.abandon(key, AbandonReason::SpawnFailed) {
        Ok(record) => format!("{note}; reported, now {:?}", record.phase),
        Err(error) => format!("{note}; the report failed: {}", error.message),
    }
}

/// Wait for a signalled target to exit without reaping it, observe the
/// attempt while its root is unreaped, reap it, and wait for the release.
/// If the authority did not signal it, the probe stops its own child so no
/// target outlives the probe; that exit is then not a termination sample.
fn settle(endpoint: &Endpoint, target: &mut Target, since: Instant, signalled: bool) -> Settled {
    let pid = target.child.id() as libc::pid_t;
    let exited = if signalled {
        exited_unreaped(pid, RELEASE_LIMIT).then(|| ms(since.elapsed()))
    } else {
        None
    };
    if exited.is_none() {
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
        std::thread::sleep(Duration::from_millis(10));
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
