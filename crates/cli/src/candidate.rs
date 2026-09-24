//! `devguard test-candidate`: verify a candidate build under a parent lease of
//! the stable authority.
//!
//! The stable CLI admits one lease from the host and runs every candidate
//! workload as a child of it, each through `devguard exec --lease` and the
//! stable launch helper: first the candidate tree's applicable build and
//! tests, then the candidate authority itself (`devguardd candidate`), whose
//! admission is checked from here. It then ends the lease, waits for the
//! candidate to close and the lease to be released, and writes a report.
//! Nothing runs outside the lease, and any failure ends the lease before
//! anything else happens.
//!
//! Workloads that create their own process groups or sessions would leave a
//! child's scope and stay Suspect under the cooperative macOS model, so the
//! candidate's native launch, CLI, terminal and scope suites are not run
//! here; they remain bootstrap and CI qualification runs.

use crate::args::CandidateArgs;
use crate::authority::{Endpoint, Service};
use crate::receipt::{unix_ms, Exit};
use devguard_client::credential::CredentialHandoff;
use devguard_client::protocol::CallerCredential;
use devguard_client::Client;
use devguard_contract::{
    AdmissionRequest, AttemptKey, AttemptPhase, AttemptRecord, Budget, Capability, Compatibility,
    Error, ErrorCode, LeasePhase, LeaseRecord, LeaseView, ResourceIntent, ResourceLevels, Result,
    Secret,
};
use devguard_daemon::candidate::{budget_text, key_text, CandidateSpec};
use devguard_daemon::config::{self, HostConfig};
use devguard_daemon::paths::{read_private, AuthorityPaths};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::Write;
use std::os::fd::RawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const SCHEMA: &str = "devguard-test-candidate/v1";
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
/// The lease when none is given: two Cargo jobs, then the candidate.
pub const DEFAULT_LEASE: Budget = Budget {
    cpu_milli: 2_000,
    memory_bytes: 4 * GIB,
    tasks: 96,
};
/// The candidate authority's capacity, where the lease holds it.
pub const DEFAULT_CAPACITY: Budget = Budget {
    cpu_milli: 1_000,
    memory_bytes: GIB,
    tasks: 64,
};
pub const DEFAULT_TTL: Duration = Duration::from_secs(2 * 60 * 60);
/// Crates whose tests create no process group or session and observe no
/// host capacity, so they stay within a lease child's scope.
pub const APPLICABLE_TESTS: [&str; 4] = [
    "devguard-contract",
    "devguard-core",
    "devguard-client",
    "devguard-cargo",
];
/// How long the candidate may take to serve its endpoint.
const ENDPOINT_LIMIT: Duration = Duration::from_secs(60);
/// How long the candidate's admission may stay closed by its pressure.
const ADMISSION_LIMIT: Duration = Duration::from_secs(90);
/// How long the candidate may take to close once its lease has ended, and
/// again after it is asked to stop.
const CLOSE_LIMIT: Duration = Duration::from_secs(30);
/// How long the lease may take to be released once its children end.
const RELEASE_LIMIT: Duration = Duration::from_secs(60);
const FIRST_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(10);

/// One governed workload, run by `devguard exec --lease` as its own process.
#[derive(Debug, Clone, Serialize)]
pub struct Workload {
    pub name: String,
    pub cwd: PathBuf,
    /// `devguard exec` options, apart from the lease and the receipt.
    pub options: Vec<String>,
    pub program: String,
    pub args: Vec<String>,
    /// Variables set for the workload; the rest of the environment is
    /// inherited. They are recorded, so they never carry a secret.
    pub env: Vec<(String, String)>,
}

/// The candidate authority's workload for its spec and token descriptor.
pub type DaemonWorkload = Box<dyn Fn(&CandidateSpec, RawFd) -> Workload>;

/// What `test-candidate` runs.
pub struct Plan {
    pub id: String,
    pub lease: Budget,
    pub ttl: Duration,
    /// How long a lease refused for capacity or pressure is asked for again.
    pub wait: Duration,
    pub capacity: Budget,
    /// Run in order, each to its end, before the candidate authority.
    pub children: Vec<Workload>,
    pub daemon: DaemonWorkload,
    pub report: PathBuf,
    /// The candidate tree, when the plan was made from one.
    pub tree: Option<PathBuf>,
}

/// The authority and the CLI a plan runs with.
pub struct Environment<'a> {
    pub paths: &'a AuthorityPaths,
    /// This CLI with the given arguments, as its own process.
    pub cli: &'a dyn Fn(&[String]) -> Command,
}

#[derive(Debug, Default, Serialize)]
struct LeaseSection {
    key: Option<AttemptKey>,
    budget: Option<Budget>,
    ttl_ms: u64,
    admissions: u32,
    granted: Option<LeaseRecord>,
    token_issued: bool,
    ended: Option<LeaseView>,
    released: Option<LeaseView>,
}

#[derive(Debug, Serialize)]
struct ChildReport {
    workload: Workload,
    receipt: PathBuf,
    log: PathBuf,
    /// How the `devguard exec` process ended.
    status: Option<String>,
    /// The receipt's result, reason and exit.
    result: Value,
    /// The admitted attempt as last observed, with its reservation.
    attempt: Option<AttemptRecord>,
    reserved: Option<Budget>,
    within_lease: bool,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    managed: bool,
    candidate: Value,
    lease: LeaseSection,
    children: Vec<ChildReport>,
    smoke: BTreeMap<String, Value>,
    cleanup: Value,
    failures: Vec<String>,
    verdict: &'static str,
    started_unix_ms: u128,
    finished_unix_ms: u128,
}

fn failed(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}

fn text(error: &Error) -> String {
    format!("{:?}: {}", error.code, error.message)
}

/// The candidate authority's own credential, from its own area.
fn candidate_credential(paths: &AuthorityPaths) -> Result<CallerCredential> {
    let config = HostConfig::load(paths)?;
    let generation = config
        .consumers
        .get(crate::authority::CONSUMER)
        .ok_or_else(|| failed("the candidate has no dev-cli consumer"))?
        .generation
        .clone();
    let bytes = read_private(&paths.cli_credential(), paths.uid(), 1024)?;
    let secret = Secret::new(
        String::from_utf8(bytes).map_err(|_| failed("the candidate credential is not valid"))?,
    )?;
    Ok(CallerCredential::Consumer {
        consumer_id: crate::authority::CONSUMER.into(),
        generation,
        secret,
    })
}

fn requiring(required: &[Capability]) -> Compatibility {
    Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 1,
        required: required.iter().copied().collect(),
    }
}

/// A new authenticated and registered session with the candidate: every
/// frame has a short deadline, so each check uses its own.
fn candidate_session(paths: &AuthorityPaths, credential: &CallerCredential) -> Result<Client> {
    let mut client = Client::connect(&paths.socket(), paths.uid(), requiring(&[]))?;
    client.authenticate(credential.clone())?;
    client.register("test-candidate".into())?;
    Ok(client)
}

fn request(key: AttemptKey, requested: Budget) -> AdmissionRequest {
    AdmissionRequest {
        key,
        execution_digest: devguard_contract::digest_bytes(b"devguard test-candidate smoke"),
        intent: ResourceIntent {
            profile: "interactive".into(),
            requested,
            minimum: ResourceLevels::MACOS,
        },
    }
}

fn wait_with_limit(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => return None,
        }
    }
}

fn status_text(status: std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        _ => "unknown".into(),
    }
}

struct Orchestrator<'a> {
    plan: &'a Plan,
    env: &'a Environment<'a>,
    report: Report,
    endpoint: Option<Endpoint>,
    token: Option<Secret>,
    /// The candidate authority's `devguard exec`, while it may run.
    candidate: Option<(Child, Workload)>,
}

impl Orchestrator<'_> {
    fn fail(&mut self, message: impl Into<String>) {
        self.report.failures.push(message.into());
    }

    fn endpoint(&self) -> Result<&Endpoint> {
        self.endpoint
            .as_ref()
            .ok_or_else(|| failed("the authority is not open"))
    }

    /// Admit the lease, asking again while the wait lasts when it does not
    /// fit the host's capacity at the current pressure.
    fn admit_lease(&mut self) -> Result<()> {
        let endpoint = self.endpoint()?;
        let key = endpoint.key(format!("lease-{}", uuid::Uuid::new_v4().simple()));
        self.report.lease.key = Some(key.clone());
        let ttl_ms = u64::try_from(self.plan.ttl.as_millis()).unwrap_or(u64::MAX);
        let deadline = Instant::now() + self.plan.wait;
        let mut backoff = FIRST_BACKOFF;
        loop {
            self.report.lease.admissions += 1;
            let endpoint = self.endpoint()?;
            let admit = || endpoint.admit_lease(&key, self.plan.lease, Some(ttl_ms));
            // A lost reply is replayed once with the same key; a lease that
            // was granted then comes back without its token.
            let result = match admit() {
                Err(error) if error.code == ErrorCode::ResourceControlUnavailable => admit(),
                other => other,
            };
            match result {
                Ok(granted) => {
                    self.report.lease.token_issued = granted.token.is_some();
                    self.report.lease.granted = Some(granted.lease);
                    self.token = granted.token;
                    if self.token.is_none() {
                        return Err(failed(
                            "the lease token was lost with a reply; the lease is ended unused",
                        ));
                    }
                    return Ok(());
                }
                Err(error)
                    if error.code == ErrorCode::ResourceUnavailable
                        && Instant::now() + backoff < deadline =>
                {
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Start `workload` as a lease child through `devguard exec --lease`.
    /// `extra` is inherited by the workload itself.
    fn start(
        &mut self,
        workload: &Workload,
        extra: Option<CredentialHandoff>,
    ) -> Result<(Child, PathBuf, PathBuf)> {
        let key = self
            .report
            .lease
            .key
            .clone()
            .ok_or_else(|| failed("no lease is held"))?;
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| failed("no lease token is held"))?;
        let handoff = CredentialHandoff::new(token)?;
        let receipt = self
            .plan
            .report
            .join(format!("{}.receipt.json", workload.name));
        let log = self.plan.report.join(format!("{}.log", workload.name));
        let mut args = vec![
            "exec".to_string(),
            "--lease".into(),
            key_text(&key),
            "--lease-token-fd".into(),
            handoff.descriptor().to_string(),
            "--receipt".into(),
            receipt.to_string_lossy().into_owned(),
        ];
        args.extend(workload.options.iter().cloned());
        args.push("--".into());
        args.push(workload.program.clone());
        args.extend(workload.args.iter().cloned());
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&log)
            .map_err(|_| failed("cannot create a workload log"))?;
        let errors = output
            .try_clone()
            .map_err(|_| failed("cannot create a workload log"))?;
        let mut command = (self.env.cli)(&args);
        command
            .current_dir(&workload.cwd)
            .envs(workload.env.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(errors);
        handoff.attach(&mut command);
        if let Some(extra) = extra {
            extra.attach(&mut command);
        }
        let child = command
            .spawn()
            .map_err(|_| failed(format!("cannot start devguard exec for {}", workload.name)))?;
        // Dropping the command closes this process's copies of the tokens.
        drop(command);
        Ok((child, receipt, log))
    }

    /// Record a finished lease child from its receipt.
    fn record(
        &mut self,
        workload: Workload,
        receipt: PathBuf,
        log: PathBuf,
        status: Option<std::process::ExitStatus>,
    ) -> bool {
        let parsed: Option<Value> = std::fs::read(&receipt)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let result = parsed.as_ref().map_or(Value::Null, |receipt| {
            json!({"result": receipt["result"], "reason": receipt["reason"],
                   "exit": receipt["exit"], "lease": receipt["lease"]})
        });
        let attempt: Option<AttemptRecord> = parsed.as_ref().and_then(|receipt| {
            let admitted = receipt["attempts"]
                .as_array()
                .and_then(|attempts| {
                    attempts
                        .iter()
                        .rev()
                        .find(|entry| entry["record"]["reservation"].is_object())
                })
                .map(|entry| entry["record"].clone());
            let observed = receipt["observed_after_reap"].clone();
            let chosen = if observed.is_object() {
                observed
            } else {
                admitted?
            };
            serde_json::from_value(chosen).ok()
        });
        let reserved = attempt
            .as_ref()
            .and_then(|attempt| attempt.reservation.as_ref())
            .map(|reservation| reservation.quantities);
        let within_lease = reserved.is_some_and(|reserved| reserved.fits(self.plan.lease));
        let completed = result["result"] == "completed" && result["exit"] == json!({"code": 0});
        let passed = completed && within_lease && status.is_some_and(|status| status.success());
        if !passed {
            self.fail(format!(
                "the lease child {} did not complete within the lease: {}",
                workload.name, result
            ));
        }
        self.report.children.push(ChildReport {
            workload,
            receipt,
            log,
            status: status.map(status_text),
            result,
            attempt,
            reserved,
            within_lease,
            passed,
        });
        passed
    }

    fn orchestrate(&mut self) -> Result<()> {
        self.endpoint = Some(Endpoint::open(self.env.paths)?);
        self.admit_lease()?;
        for workload in &self.plan.children {
            let (mut child, receipt, log) = self.start(workload, None)?;
            let status = child.wait().ok();
            if !self.record(workload.clone(), receipt, log, status) {
                return Err(failed(format!(
                    "{} failed; later children are not run",
                    workload.name
                )));
            }
        }
        let key = self
            .report
            .lease
            .key
            .clone()
            .ok_or_else(|| failed("no lease is held"))?;
        let spec = CandidateSpec {
            id: self.plan.id.clone(),
            lease: key,
            capacity: self.plan.capacity,
        };
        let token = self
            .token
            .clone()
            .ok_or_else(|| failed("no lease token is held"))?;
        let handoff = CredentialHandoff::new(&token)?;
        let workload = (self.plan.daemon)(&spec, handoff.descriptor());
        let (child, _, _) = self.start(&workload, Some(handoff))?;
        self.candidate = Some((child, workload));
        self.smoke(&spec)
    }

    fn check(&mut self, name: &str, ok: bool, detail: Value) -> Result<()> {
        self.report
            .smoke
            .insert(name.into(), json!({"ok": ok, "detail": detail}));
        if ok {
            Ok(())
        } else {
            Err(failed(format!(
                "the candidate check {name} failed: {detail}"
            )))
        }
    }

    fn candidate_running(&mut self) -> bool {
        self.candidate
            .as_mut()
            .is_some_and(|(child, _)| matches!(child.try_wait(), Ok(None)))
    }

    /// Check the candidate authority from outside, through its own endpoint
    /// and credential.
    fn smoke(&mut self, spec: &CandidateSpec) -> Result<()> {
        let paths = self.env.paths.candidate(&spec.id)?;
        let deadline = Instant::now() + ENDPOINT_LIMIT;
        let hello = loop {
            match Client::connect(&paths.socket(), paths.uid(), requiring(&[])) {
                Ok(client) => break client.hello,
                Err(error) => {
                    if !self.candidate_running() || Instant::now() >= deadline {
                        return Err(failed(format!(
                            "the candidate never served its endpoint: {}",
                            text(&error)
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        };
        let capabilities: BTreeSet<Capability> = hello.capabilities.clone();
        self.check(
            "capabilities",
            capabilities.contains(&Capability::DurableAdmission)
                && !capabilities.contains(&Capability::FencedLaunch)
                && !capabilities.contains(&Capability::ParentLease),
            json!({"authority": hello.authority, "capabilities": capabilities}),
        )?;
        let credential = candidate_credential(&paths)?;
        let status = {
            let mut client = Client::connect(&paths.socket(), paths.uid(), requiring(&[]))?;
            client.authenticate(credential.clone())?;
            client.status()?
        };
        self.check(
            "status",
            status.registration_ready
                && !status.execution_ready
                && status.reason.contains(&spec.id)
                && status.reason.contains(&key_text(&spec.lease)),
            json!(status),
        )?;
        let CallerCredential::Consumer {
            consumer_id,
            generation,
            ..
        } = &credential
        else {
            return Err(failed("the candidate credential is not a consumer's"));
        };
        let key = |attempt: &str| AttemptKey {
            consumer_id: consumer_id.clone(),
            consumer_generation: generation.clone(),
            attempt_id: attempt.into(),
        };
        let work = spec.capacity.remaining_after(config::candidate_reservation(
            config::BOOTSTRAP_SYSTEM_TASKS,
        ));
        let within = Budget {
            cpu_milli: work.cpu_milli.min(100),
            memory_bytes: work.memory_bytes.min(64 * MIB),
            tasks: work.tasks.min(4),
        };
        // A new authority admits nothing until its pressure is normal.
        let deadline = Instant::now() + ADMISSION_LIMIT;
        let mut attempts = 0;
        let admitted = loop {
            attempts += 1;
            let record = candidate_session(&paths, &credential)?
                .admit(request(key(&format!("within-{attempts}")), within))?;
            if record.phase == AttemptPhase::Prepared
                || record.denial != Some(ErrorCode::ResourceUnavailable)
                || Instant::now() >= deadline
            {
                break record;
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        let prepared = admitted.phase == AttemptPhase::Prepared;
        self.check(
            "admission_within_capacity",
            prepared,
            json!({"requested": within, "attempts": attempts, "record": admitted}),
        )?;
        let launch = candidate_session(&paths, &credential)?.begin_launch(admitted.key.clone());
        self.check(
            "launch_refused",
            matches!(&launch, Err(error) if error.code == ErrorCode::ResourcePolicyUnsupported),
            json!(launch.as_ref().err()),
        )?;
        let cancelled = candidate_session(&paths, &credential)?.cancel(admitted.key.clone())?;
        self.check(
            "cancelled",
            cancelled.phase == AttemptPhase::Cancelled,
            json!(cancelled),
        )?;
        let beyond =
            candidate_session(&paths, &credential)?.admit(request(key("beyond"), spec.capacity))?;
        self.check(
            "admission_beyond_capacity",
            beyond.denial == Some(ErrorCode::ResourceUnavailable),
            json!({"requested": spec.capacity, "work_capacity": work, "record": beyond}),
        )?;
        let nested =
            candidate_session(&paths, &credential)?.admit_lease(key("nested-lease"), within, None);
        self.check(
            "parent_lease_refused",
            matches!(&nested, Err(error) if error.code == ErrorCode::ResourcePolicyUnsupported),
            json!(nested.as_ref().err()),
        )
    }

    /// End the lease, let the candidate close, wait for the lease to be
    /// released and remove the candidate's disposable area. Runs after every
    /// outcome.
    fn finish(&mut self) {
        let (Some(key), Some(endpoint)) = (self.report.lease.key.clone(), self.endpoint.take())
        else {
            return;
        };
        self.settle(&key, &endpoint);
        self.endpoint = Some(endpoint);
    }

    fn settle(&mut self, key: &AttemptKey, endpoint: &Endpoint) {
        if self.report.lease.granted.is_some() {
            let ended = endpoint.end_lease(key).or_else(|_| endpoint.end_lease(key));
            match ended {
                Ok(view) => self.report.lease.ended = Some(view),
                Err(error) => self.fail(format!("cannot end the lease: {}", text(&error))),
            }
        }
        if let Some((mut child, workload)) = self.candidate.take() {
            let mut status = wait_with_limit(&mut child, CLOSE_LIMIT);
            if status.is_none() {
                self.fail("the candidate did not close when its lease ended; it was asked to stop");
                // SAFETY: the pid is this process's unreaped child; the CLI
                // forwards the signal to the candidate's group.
                unsafe {
                    libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
                }
                status = wait_with_limit(&mut child, CLOSE_LIMIT);
            }
            let receipt = self
                .plan
                .report
                .join(format!("{}.receipt.json", workload.name));
            let log = self.plan.report.join(format!("{}.log", workload.name));
            match status {
                Some(status) => {
                    self.record(workload, receipt, log, Some(status));
                }
                None => {
                    self.fail("the candidate is still running; its area is kept");
                    // The child stays charged under the ended lease until its
                    // scope is observed empty.
                    self.candidate = Some((child, workload));
                }
            }
        }
        if let Some(token) = self.token.clone() {
            let deadline = Instant::now() + RELEASE_LIMIT;
            loop {
                match endpoint.lease_status(key, &token) {
                    Ok(view) if view.lease.phase == LeasePhase::Released => {
                        self.report.lease.released = Some(view);
                        break;
                    }
                    Ok(view) if Instant::now() >= deadline => {
                        self.fail(format!(
                            "the lease was not released: {:?} with {} children",
                            view.lease.phase,
                            view.children.len()
                        ));
                        self.report.lease.released = Some(view);
                        break;
                    }
                    Err(error) if Instant::now() >= deadline => {
                        self.fail(format!("cannot observe the lease: {}", text(&error)));
                        break;
                    }
                    _ => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        }
        self.report.cleanup = if self.candidate.is_some() {
            json!({"removed": false, "reason": "the candidate is still running"})
        } else {
            match self.env.paths.candidate(&self.plan.id) {
                Ok(paths) => {
                    let mut removed = Vec::new();
                    for path in [paths.root().to_path_buf(), paths.runtime().to_path_buf()] {
                        if std::fs::symlink_metadata(&path).is_ok() {
                            match std::fs::remove_dir_all(&path) {
                                Ok(()) => removed.push(path),
                                Err(_) => self.fail(format!(
                                    "cannot remove the candidate area {}",
                                    path.display()
                                )),
                            }
                        }
                    }
                    json!({"removed": removed})
                }
                Err(error) => json!({"removed": false, "reason": text(&error)}),
            }
        };
    }
}

/// Run `plan` in `env` and report it. Exits 0 only when every child
/// completed within the lease, every candidate check passed, the candidate
/// closed with its lease and the lease was released.
pub fn run(plan: &Plan, env: &Environment) -> Exit {
    let report = Report {
        schema: SCHEMA,
        managed: true,
        candidate: json!({"id": plan.id, "capacity": plan.capacity, "tree": plan.tree}),
        lease: LeaseSection {
            budget: Some(plan.lease),
            ttl_ms: u64::try_from(plan.ttl.as_millis()).unwrap_or(u64::MAX),
            ..LeaseSection::default()
        },
        children: Vec::new(),
        smoke: BTreeMap::new(),
        cleanup: Value::Null,
        failures: Vec::new(),
        verdict: "failed",
        started_unix_ms: unix_ms(),
        finished_unix_ms: 0,
    };
    if let Err(error) = std::fs::DirBuilder::new().mode(0o700).create(&plan.report) {
        eprintln!(
            "devguard: cannot create the report directory {}: {error}; nothing was started",
            plan.report.display()
        );
        return Exit::Code(crate::exec::NOT_STARTED);
    }
    let mut orchestrator = Orchestrator {
        plan,
        env,
        report,
        endpoint: None,
        token: None,
        candidate: None,
    };
    if let Err(error) = orchestrator.orchestrate() {
        orchestrator.fail(text(&error));
    }
    orchestrator.finish();
    let mut report = orchestrator.report;
    let released = report
        .lease
        .released
        .as_ref()
        .is_some_and(|view| view.lease.phase == LeasePhase::Released);
    let candidate_ran = report
        .children
        .iter()
        .any(|child| child.workload.name == "candidate");
    if report.failures.is_empty() && released && candidate_ran {
        report.verdict = "passed";
    }
    report.finished_unix_ms = unix_ms();
    let path = plan.report.join("report.json");
    let written = serde_json::to_vec_pretty(&report)
        .map_err(|error| std::io::Error::other(error.to_string()))
        .and_then(|mut text| {
            text.push(b'\n');
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            file.write_all(&text)?;
            file.sync_all()
        });
    if let Err(error) = written {
        eprintln!("devguard: cannot write the report: {error}");
        return Exit::Code(1);
    }
    println!(
        "{}",
        json!({"report": path, "verdict": report.verdict, "failures": report.failures})
    );
    Exit::Code(if report.verdict == "passed" { 0 } else { 1 })
}

/// The plan for a candidate tree: its build and applicable tests, then its
/// own `devguardd candidate`, all under one lease.
pub fn plan(args: &CandidateArgs, environment: &BTreeMap<OsString, OsString>) -> Result<Plan> {
    let invalid = |message: &'static str| Error::new(ErrorCode::InvalidRequest, message);
    let tree = args
        .candidate
        .canonicalize()
        .map_err(|_| invalid("the candidate tree is not accessible"))?;
    if !tree.join("Cargo.toml").is_file() || !tree.join("Cargo.lock").is_file() {
        return Err(invalid(
            "the candidate tree must be a Cargo workspace with a lock file",
        ));
    }
    // Children run in the tree, so every path handed to them is absolute.
    let report = std::path::absolute(&args.report)
        .map_err(|_| invalid("the report directory cannot be resolved"))?;
    let lease = Budget {
        cpu_milli: args.cpu_milli.unwrap_or(DEFAULT_LEASE.cpu_milli),
        memory_bytes: args.memory_bytes.unwrap_or(DEFAULT_LEASE.memory_bytes),
        tasks: args.tasks.unwrap_or(DEFAULT_LEASE.tasks),
    };
    lease.validate_workload()?;
    let jobs = devguard_cargo::required_jobs(lease)?;
    let capacity = Budget {
        cpu_milli: DEFAULT_CAPACITY.cpu_milli.min(lease.cpu_milli),
        memory_bytes: DEFAULT_CAPACITY.memory_bytes.min(lease.memory_bytes),
        tasks: DEFAULT_CAPACITY.tasks.min(lease.tasks),
    };
    capacity
        .remaining_after(config::candidate_reservation(
            config::BOOTSTRAP_SYSTEM_TASKS,
        ))
        .validate_workload()
        .map_err(|_| config::too_small_for_a_candidate())?;
    let target = match environment.get(&OsString::from("CARGO_TARGET_DIR")) {
        Some(target) => tree.join(target),
        None => tree.join("target"),
    };
    let daemon = target.join("debug/devguardd");
    let quantities = |budget: Budget| {
        vec![
            "--cpu".to_string(),
            budget.cpu_milli.to_string(),
            "--memory".into(),
            budget.memory_bytes.to_string(),
            "--tasks".into(),
            budget.tasks.to_string(),
        ]
    };
    let cargo = |name: &str, args: Vec<String>, env: Vec<(String, String)>| {
        let mut options = vec!["--adapter".to_string(), "cargo".into()];
        options.extend(quantities(lease));
        Workload {
            name: name.into(),
            cwd: tree.clone(),
            options,
            program: "cargo".into(),
            args,
            env,
        }
    };
    let mut build = vec!["build".to_string(), "--offline".into(), "--locked".into()];
    for package in ["devguard-daemon", "devguard-cli", "devguard-launch"] {
        build.extend(["-p".to_string(), package.into()]);
    }
    let mut test = vec!["test".to_string(), "--offline".into(), "--locked".into()];
    for package in APPLICABLE_TESTS {
        test.extend(["-p".to_string(), package.into()]);
    }
    let cwd = tree.clone();
    let daemon_options = {
        let mut options = vec!["--adapter".to_string(), "generic".into()];
        options.extend(quantities(capacity));
        options
    };
    Ok(Plan {
        id: format!("c-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
        lease,
        ttl: args.ttl.unwrap_or(DEFAULT_TTL),
        wait: args.wait.unwrap_or(Duration::ZERO),
        capacity,
        children: vec![
            cargo("build", build, Vec::new()),
            cargo(
                "tests",
                test,
                vec![("RUST_TEST_THREADS".into(), jobs.to_string())],
            ),
        ],
        daemon: Box::new(move |spec, fd| Workload {
            name: "candidate".into(),
            cwd: cwd.clone(),
            options: daemon_options.clone(),
            program: daemon.to_string_lossy().into_owned(),
            args: vec![
                "candidate".into(),
                "--id".into(),
                spec.id.clone(),
                "--lease".into(),
                key_text(&spec.lease),
                "--capacity".into(),
                budget_text(spec.capacity),
                "--token-fd".into(),
                fd.to_string(),
            ],
            env: Vec::new(),
        }),
        report,
        tree: Some(tree),
    })
}

/// `devguard test-candidate` with the running `devguard` as the stable CLI.
pub fn run_args(args: &CandidateArgs, paths: &AuthorityPaths) -> Exit {
    let environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    let plan = match plan(args, &environment) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("devguard: {}; nothing was started", error.message);
            return Exit::Code(crate::exec::NOT_STARTED);
        }
    };
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            eprintln!("devguard: cannot locate this CLI: {error}; nothing was started");
            return Exit::Code(crate::exec::NOT_STARTED);
        }
    };
    let cli = move |args: &[String]| {
        let mut command = Command::new(&executable);
        command.args(args);
        command
    };
    run(&plan, &Environment { paths, cli: &cli })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn args(candidate: &Path) -> CandidateArgs {
        CandidateArgs {
            candidate: candidate.into(),
            report: "/nonexistent/report".into(),
            cpu_milli: None,
            memory_bytes: None,
            tasks: None,
            ttl: None,
            wait: None,
        }
    }

    #[test]
    fn a_tree_is_planned_as_its_build_applicable_tests_and_candidate_under_one_lease() {
        let tree = tempfile::tempdir().unwrap();
        assert!(plan(&args(tree.path()), &BTreeMap::new()).is_err());
        std::fs::write(tree.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(tree.path().join("Cargo.lock"), "").unwrap();
        let planned = plan(&args(tree.path()), &BTreeMap::new()).unwrap();
        assert_eq!(planned.lease, DEFAULT_LEASE);
        // A relative report directory is resolved before any child runs in
        // the tree, where it would name another place.
        let relative = CandidateArgs {
            report: "reports/run-1".into(),
            ..args(tree.path())
        };
        let resolved = plan(&relative, &BTreeMap::new()).unwrap().report;
        assert!(resolved.is_absolute());
        assert_eq!(
            resolved,
            std::env::current_dir().unwrap().join("reports/run-1")
        );
        assert_eq!(planned.capacity, DEFAULT_CAPACITY);
        assert!(planned.capacity.fits(planned.lease));
        let names: Vec<&str> = planned.children.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["build", "tests"]);
        for child in &planned.children {
            assert_eq!(child.program, "cargo");
            assert!(child.args.contains(&"--locked".to_string()));
            assert!(child.args.contains(&"--offline".to_string()));
        }
        let tests = &planned.children[1];
        for package in APPLICABLE_TESTS {
            assert!(tests.args.contains(&package.to_string()));
        }
        // Suites that create process groups or observe the host are not run.
        for package in ["devguard-daemon", "devguard-cli", "devguard-macos"] {
            assert!(!tests.args.contains(&package.to_string()), "{package}");
        }
        assert_eq!(tests.env, [("RUST_TEST_THREADS".into(), "2".into())]);
        let spec = CandidateSpec {
            id: planned.id.clone(),
            lease: AttemptKey {
                consumer_id: "dev-cli".into(),
                consumer_generation: "g".into(),
                attempt_id: "lease-1".into(),
            },
            capacity: planned.capacity,
        };
        let daemon = (planned.daemon)(&spec, 7);
        assert!(daemon.program.ends_with("target/debug/devguardd"));
        assert_eq!(
            daemon.args,
            [
                "candidate",
                "--id",
                &planned.id,
                "--lease",
                "dev-cli/g/lease-1",
                "--capacity",
                "1000,1073741824,64",
                "--token-fd",
                "7"
            ]
        );
        // A lease that fits no Cargo job, or no candidate, is refused.
        for (cpu, memory) in [(999, 4 * GIB), (2_000, GIB)] {
            let small = CandidateArgs {
                cpu_milli: Some(cpu),
                memory_bytes: Some(memory),
                ..args(tree.path())
            };
            assert!(plan(&small, &BTreeMap::new()).is_err(), "{cpu} {memory}");
        }
        let environment =
            BTreeMap::from([(OsString::from("CARGO_TARGET_DIR"), OsString::from("/t/x"))]);
        let planned = plan(&args(tree.path()), &environment).unwrap();
        assert_eq!((planned.daemon)(&spec, 7).program, "/t/x/debug/devguardd");
    }
}
