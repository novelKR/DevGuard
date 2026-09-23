use crate::{config::HostConfig, paths::AuthorityPaths};
use devguard_client::{connect::connect_timeout, framing, peer, protocol::*};
use devguard_contract::{
    validate_id, AppliedResources, AttemptKey, AttemptPhase, AttemptRecord, Capability, Error,
    ErrorCode, InstanceIdentity, ProcessIdentity, Result, ScopeIdentity, Secret, PROTOCOL_VERSION,
};
use devguard_core::{
    Authority, AuthorityStorage, Backend as _, Clock, ConsumerRole, PressureState, Principal,
    Registration, TrustedPeer,
};
use devguard_macos::{
    BootClock, CpuReadback, HostProbe, NativeBackend, NativeHost, NativeProbe, Sampler,
    SamplerOutcome, SAMPLE_INTERVAL_MS,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::Duration;

pub(crate) type NativeAuthority = Authority<NativeBackend, BootClock>;

const LAUNCH_REASON: &str = "native registration, fenced launch and reconciliation are open; admission follows host pressure and capacity";
/// How often the reconciler observes charged attempts and registered owners.
pub(crate) const RECONCILE_INTERVAL_MS: u64 = 1_000;
const UNSUPPORTED_REASON: &str = "native host evidence is unsupported on this platform; registration and execution remain closed";
const FAILED_REASON: &str =
    "native host observation failed; registration and execution remain closed";

/// How the service opens. The normal service uses the defaults.
#[derive(Default)]
pub(crate) struct Options {
    /// A substitute host probe for isolated fixtures, which cannot assume the
    /// real host is free of pressure.
    pub probe: Option<Box<dyn HostProbe>>,
    /// Lets an isolated fixture pause reconciler passes, so a test can tell an
    /// owner's own observation apart from the background pass.
    pub reconcile_paused: Option<Arc<AtomicBool>>,
}

/// Exclusive storage, activated with actual host evidence when it is available.
enum Evidence {
    Native {
        authority: Arc<Mutex<NativeAuthority>>,
        clock: BootClock,
        backend: NativeBackend,
        probe: Option<Box<dyn HostProbe>>,
    },
    Closed {
        _storage: AuthorityStorage,
    },
}

pub struct Server {
    listener: UnixListener,
    socket: PathBuf,
    socket_identity: (u64, u64),
    config: Arc<HostConfig>,
    uid: u32,
    status: ServiceStatus,
    evidence: Evidence,
    launcher: Option<Launcher>,
}

/// Service receipts are JSON lines on stderr; they never include credentials.
/// A failing stderr is ignored rather than allowed to panic a service thread.
#[cfg(not(test))]
fn receipt(value: serde_json::Value) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr().lock(), "{value}");
}

/// Unit tests keep receipts in the harness's captured output.
#[cfg(test)]
fn receipt(value: serde_json::Value) {
    eprintln!("{value}");
}

/// Stops the service when the sampler thread ends for any reason, including
/// a panic, so admission never outlives its pressure evidence silently.
struct StopOnExit(Arc<AtomicBool>);

impl Drop for StopOnExit {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Activate the exclusively held journal with the observed host, or keep it
/// closed. Recovery runs only with a real boot clock and process identity.
fn activate(
    storage: AuthorityStorage,
    config: &HostConfig,
    paths: &AuthorityPaths,
    substitute: Option<Box<dyn HostProbe>>,
) -> Result<(Evidence, &'static str)> {
    let host = match NativeHost::open() {
        Ok(host) => host,
        Err(error) => {
            receipt(json!({"event": "native_host_unavailable", "error": error}));
            let reason = if error.code == ErrorCode::ResourcePolicyUnsupported {
                UNSUPPORTED_REASON
            } else {
                FAILED_REASON
            };
            return Ok((Evidence::Closed { _storage: storage }, reason));
        }
    };
    let mut volumes = vec![paths.state()];
    volumes.extend(config.projects.values().map(|project| project.root.clone()));
    let probe: Box<dyn HostProbe> = match substitute {
        Some(probe) => probe,
        None => match NativeProbe::new(volumes.clone()) {
            Ok(probe) => Box::new(probe),
            Err(error) => {
                receipt(json!({"event": "native_host_unavailable", "error": error}));
                return Ok((Evidence::Closed { _storage: storage }, FAILED_REASON));
            }
        },
    };
    let policy = config.policy(host.capacity(), paths.uid())?;
    let work_capacity = policy.work_capacity()?;
    let authority = Authority::from_storage(storage, policy, host.backend(), host.clock())?;
    receipt(json!({
        "event": "native_host",
        "boot_id": host.clock().boot_id(),
        "clock": "CLOCK_MONOTONIC_RAW milliseconds since boot",
        "started_at": host.clock().now(),
        "capacity": host.capacity(),
        "work_capacity": work_capacity,
        "volumes": volumes,
        "sample_interval_ms": SAMPLE_INTERVAL_MS,
    }));
    Ok((
        Evidence::Native {
            authority: Arc::new(Mutex::new(authority)),
            clock: host.clock(),
            backend: host.backend(),
            probe: Some(probe),
        },
        LAUNCH_REASON,
    ))
}

/// Feed the pressure controller every two seconds. A failed reading closes new
/// work immediately; a late wake-up is reported as control-loop lag. Receipts
/// go to `sink` after the authority lock is released.
fn sample_pressure<P: HostProbe>(
    authority: Arc<Mutex<NativeAuthority>>,
    clock: BootClock,
    probe: P,
    stop: Arc<AtomicBool>,
    mut sink: impl FnMut(serde_json::Value),
) -> Result<()> {
    enum Event {
        Baseline(serde_json::Value),
        Sample(Result<PressureState>, serde_json::Value),
        Failed(serde_json::Value),
    }
    let mut sampler = Sampler::new(probe);
    let mut next = clock.now().monotonic_ms;
    // How far the previous iteration overran the schedule, reported as the
    // next sample's control-loop lag even though no burst catches up.
    let mut carried_lag = 0;
    let mut previous: Option<PressureState> = None;
    let mut failures: u64 = 0;
    let mut samples: u64 = 0;
    while !stop.load(Ordering::Relaxed) {
        let now = clock.now().monotonic_ms;
        if now < next {
            std::thread::sleep(Duration::from_millis((next - now).min(50)));
            continue;
        }
        let lag = (now - next).max(carried_lag);
        carried_lag = 0;
        let outcome = sampler.sample(&clock, lag);
        let event = {
            let Ok(mut authority) = authority.lock() else {
                return Err(unavailable("authority lock poisoned"));
            };
            match outcome {
                SamplerOutcome::Baseline {
                    at,
                    reading,
                    read_ms,
                } => Event::Baseline(json!({"at": at, "reading": reading, "read_ms": read_ms})),
                SamplerOutcome::Sample {
                    sample,
                    receipt: derived,
                } => Event::Sample(authority.observe_pressure(sample), json!(derived)),
                SamplerOutcome::Failed { at, error, read_ms } => {
                    authority.pressure_observation_failed();
                    Event::Failed(json!({"at": at, "error": error, "read_ms": read_ms}))
                }
            }
        };
        match event {
            Event::Baseline(detail) => {
                sink(json!({"event": "pressure_baseline", "baseline": detail}));
            }
            Event::Sample(Ok(state), derived) => {
                samples += 1;
                // Transitions, recovery from failure and a one-minute heartbeat.
                if previous != Some(state) || failures > 0 || samples % 30 == 1 {
                    sink(json!({"event": "pressure", "state": state, "sample": derived}));
                } else if derived["control_lag_ms"].as_u64().unwrap_or(0) >= 200 {
                    // A late wake-up matters even when the state is unchanged.
                    sink(
                        json!({"event": "pressure_control_lag", "state": state, "sample": derived}),
                    );
                }
                previous = Some(state);
                failures = 0;
            }
            Event::Sample(Err(error), derived) => {
                // The controller closed admission on this rejected sample.
                sink(json!({"event": "pressure_sample_rejected", "error": error,
                               "state": PressureState::Critical, "sample": derived}));
                previous = Some(PressureState::Critical);
            }
            Event::Failed(detail) => {
                // The first failure and then one line a minute while it persists.
                if failures.is_multiple_of(30) {
                    sink(
                        json!({"event": "pressure_observation_failed", "failure": detail,
                                   "consecutive_failures": failures + 1,
                                   "state": PressureState::Critical}),
                    );
                }
                previous = Some(PressureState::Critical);
                failures += 1;
            }
        }
        // Keep the cadence. After an overrun (a late wake-up or a slow read),
        // resume one interval after the completed reading instead of catching
        // up in a burst, which would yield degenerate rate windows.
        next += SAMPLE_INTERVAL_MS;
        let finished = clock.now().monotonic_ms;
        if next <= finished {
            carried_lag = finished - next;
            next = finished + SAMPLE_INTERVAL_MS;
        }
    }
    Ok(())
}

fn unavailable(message: &'static str) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}
fn unauthorized() -> Error {
    Error::new(
        ErrorCode::Unauthorized,
        "caller credential or role is not authorized",
    )
}
fn fenced(message: &'static str) -> Error {
    Error::new(ErrorCode::InvalidTransition, message)
}
fn poisoned() -> Error {
    unavailable("authority state is unavailable")
}

/// The capabilities of a service with registration and fenced launch open.
fn launch_capabilities() -> BTreeSet<Capability> {
    BTreeSet::from([
        Capability::DurableAdmission,
        Capability::FencedLaunch,
        Capability::PerResourceEvidence,
        Capability::StaticControlReservations,
        Capability::MacosCooperative,
    ])
}

/// A consumer's authenticated credential, kept only to register its instance.
struct ConsumerCredential {
    id: String,
    generation: String,
    secret: Secret,
}

/// Instances registered in this service lifetime, by consumer, generation and
/// instance ID. A helper finds its owner here, so after a restart an owner
/// must register again before any helper of its can proceed.
type Principals = BTreeMap<(String, String, String), Principal>;

/// The native authority as sessions use it once registration and launch are open.
#[derive(Clone)]
struct Launcher {
    authority: Arc<Mutex<NativeAuthority>>,
    backend: NativeBackend,
    principals: Arc<Mutex<Principals>>,
    clock: BootClock,
    paused: Arc<AtomicBool>,
}

/// Whether `identity` still names a running process. A process of an earlier
/// boot has ended, whatever holds its PID now. Within this boot an observation
/// that fails, as for a setuid exec, counts as running, which only defers
/// reconciliation.
fn still_running(
    identity: &ProcessIdentity,
    boot_id: &str,
    observe: impl FnOnce() -> Result<Option<ProcessIdentity>>,
) -> bool {
    identity.boot_id == boot_id
        && observe().map_or(true, |current| current.as_ref() == Some(identity))
}

impl Launcher {
    fn authority(&self) -> Result<std::sync::MutexGuard<'_, NativeAuthority>> {
        self.authority.lock().map_err(|_| poisoned())
    }

    /// Register the OS-observed peer with its native start identity.
    fn register(
        &self,
        caller: PeerIdentity,
        consumer: &ConsumerCredential,
        instance_id: String,
    ) -> Result<Principal> {
        let process = self
            .backend
            .process_identity(caller.pid)?
            .ok_or_else(unauthorized)?;
        let instance = InstanceIdentity {
            instance_id: instance_id.clone(),
            process,
        };
        let principal = self.authority()?.register(
            TrustedPeer {
                uid: caller.uid,
                pid: caller.pid,
            },
            Registration {
                consumer_id: consumer.id.clone(),
                generation: consumer.generation.clone(),
                instance: instance.clone(),
                credential: consumer.secret.clone(),
            },
        )?;
        self.principals.lock().map_err(|_| poisoned())?.insert(
            (
                consumer.id.clone(),
                consumer.generation.clone(),
                instance_id,
            ),
            principal.clone(),
        );
        receipt(json!({"event": "registered", "consumer": consumer.id, "instance": instance}));
        Ok(principal)
    }

    /// Verify and authorize a helper presenting its owner's launch grant.
    /// Every presentation, including a refusal before any claim, yields one
    /// receipt with how long the authorization took.
    fn authorize_helper(
        &self,
        caller: PeerIdentity,
        key: AttemptKey,
        instance_id: String,
        permit: Secret,
    ) -> Result<LaunchAuthorization> {
        let began = std::time::Instant::now();
        let mut trail = HelperTrail::default();
        let result = self.authorize_under_lock(caller, &key, instance_id, &permit, &mut trail);
        let event = match &result {
            Ok(_) if trail.replayed => "helper_replayed",
            Ok(_) => "helper_authorized",
            Err(_) => "helper_refused",
        };
        receipt(json!({"event": event, "key": key, "helper": trail.helper,
                       "claimed": trail.claimed, "scope": trail.scope, "applied": trail.applied,
                       "cpu": trail.cpu, "may_exec": result.as_ref().ok().map(|a| a.may_exec),
                       "error": result.as_ref().err(), "stopped": trail.stopped,
                       "authorization_ms": began.elapsed().as_millis() as u64}));
        result
    }

    /// Every check and transition runs under one authority lock, so no
    /// cancellation, reconciliation or other helper interleaves, and the owner
    /// is still running when the grant is claimed.
    fn authorize_under_lock(
        &self,
        caller: PeerIdentity,
        key: &AttemptKey,
        instance_id: String,
        permit: &Secret,
        trail: &mut HelperTrail,
    ) -> Result<LaunchAuthorization> {
        key.validate()?;
        validate_id(&instance_id)?;
        let mut authority = self.authority()?;
        let owner = self
            .principals
            .lock()
            .map_err(|_| poisoned())?
            .get(&(
                key.consumer_id.clone(),
                key.consumer_generation.clone(),
                instance_id,
            ))
            .cloned()
            .ok_or_else(unauthorized)?;
        // Only the running owner can have created this helper.
        let helper = self
            .backend
            .helper_process(caller.pid, &owner.instance().process)?;
        trail.helper = Some(helper.clone());
        let record = authority.verify_launch(&owner, key, permit)?;
        match (&record.scope, record.phase) {
            (Some(scope), _) if scope.root != helper => {
                return Err(fenced("the launch grant was claimed by another helper"));
            }
            // A replay after a lost reply is never a second authorization.
            (Some(_), AttemptPhase::RunAuthorized) => {
                trail.claimed = true;
                trail.replayed = true;
                let decision = authority.authorize_run(&owner, key, permit, &helper)?;
                return Ok(LaunchAuthorization {
                    attempt: decision.attempt,
                    may_exec: decision.may_exec,
                });
            }
            (Some(_), AttemptPhase::ScopeBound) | (_, AttemptPhase::LaunchCommitted) => {}
            _ => return Err(fenced("the launch grant is fenced")),
        }
        let (Some(plan), Some(reservation)) = (&record.plan, &record.reservation) else {
            return Err(fenced("the launch grant has no reservation"));
        };
        // Establish (nice and readback) before the claim: a presenter that
        // fails establishment never uses up the grant.
        let established = self.backend.establish_scope(
            key,
            owner.instance(),
            &helper,
            plan,
            reservation.quantities,
        )?;
        trail.scope = Some(established.scope.clone());
        trail.applied = Some(established.applied.clone());
        trail.cpu = established.cpu;
        if let Err(error) = authority.claim_launch(&owner, key, permit, &established.scope) {
            // Nothing claimed this scope, so nothing reconciles through it.
            let _ = self.backend.forget_scope(&established.scope);
            return Err(error);
        }
        trail.claimed = true;
        let decision = authority
            .bind_scope(&owner, key, permit, &established.scope)
            .and_then(|_| authority.authorize_run(&owner, key, permit, &helper));
        match decision {
            Ok(decision) => Ok(LaunchAuthorization {
                attempt: decision.attempt,
                may_exec: decision.may_exec,
            }),
            Err(error) => {
                drop(authority);
                // The claimed helper must not continue. Its scope stays charged
                // and is reconciled like any other until its end is observed.
                trail.stopped = Some(
                    match self.backend.signal_scope(&established.scope, libc::SIGKILL) {
                        Ok(delivered) => json!(delivered),
                        Err(failure) => json!({"error": failure}),
                    },
                );
                Err(error)
            }
        }
    }
}

/// What a helper presentation reached, for its single receipt.
#[derive(Default)]
struct HelperTrail {
    helper: Option<ProcessIdentity>,
    scope: Option<ScopeIdentity>,
    applied: Option<AppliedResources>,
    cpu: Option<CpuReadback>,
    claimed: bool,
    replayed: bool,
    stopped: Option<serde_json::Value>,
}

impl Launcher {
    /// Whether a registered process is still running (see `still_running`).
    fn running(&self, identity: &ProcessIdentity) -> bool {
        still_running(identity, &self.clock.now().boot_id, || {
            self.backend.process_identity(identity.pid)
        })
    }

    /// Reconcile one charged attempt by the service's rules, under the caller's
    /// authority lock. Prepared attempts expire on their own deadline. An
    /// unclaimed grant whose owner is still running is left alone unless the
    /// owner reported that no helper exists, because a helper may still claim
    /// it; once its owner is gone no helper can, and it becomes Suspect.
    fn reconcile_attempt(
        &self,
        authority: &mut NativeAuthority,
        record: &AttemptRecord,
    ) -> Result<Option<AttemptRecord>> {
        if record.phase == AttemptPhase::Prepared
            || (record.scope.is_none()
                && !self.backend.no_helper_reported(&record.key)
                && self.running(&record.owner.process))
        {
            return Ok(None);
        }
        let reconciled = authority.reconcile(&record.key)?;
        if reconciled.phase == AttemptPhase::Released {
            self.backend.forget_no_helper(&reconciled.key);
            if let Some(scope) = &reconciled.scope {
                let _ = self.backend.forget_scope(scope);
            }
        }
        if reconciled.phase != record.phase || reconciled.tracking_lost != record.tracking_lost {
            receipt(json!({"event": "attempt_reconciled", "key": reconciled.key,
                           "from": record.phase, "phase": reconciled.phase,
                           "release_reason": reconciled.release_reason,
                           "tracking_lost": reconciled.tracking_lost}));
        }
        Ok(Some(reconciled))
    }

    /// One reconciler pass: every charged attempt, then every registered
    /// instance whose process is gone. The authority lock is taken per item,
    /// so admission and helpers are never held behind a whole pass.
    fn reconcile_pass(&self) -> Result<()> {
        let attempts = self.authority()?.attempts()?;
        for record in &attempts {
            let result = {
                let mut authority = self.authority()?;
                self.reconcile_attempt(&mut authority, record)
            };
            if let Err(error) = result {
                receipt(json!({"event": "reconcile_failed", "key": record.key, "error": error}));
            }
        }
        let instances = self.authority()?.instances()?;
        let charged = self.authority()?.attempts()?;
        for instance in instances {
            if self.running(&instance.instance.process) {
                continue;
            }
            // Its helpers can no longer be accepted.
            self.principals.lock().map_err(|_| poisoned())?.remove(&(
                instance.consumer_id.clone(),
                instance.generation.clone(),
                instance.instance.instance_id.clone(),
            ));
            let occupied = charged
                .iter()
                .any(|record| record.owner == instance.instance);
            // A suspect instance that still owns charged work needs no rewrite.
            if occupied && !instance.active {
                continue;
            }
            let retired = self.authority()?.reconcile_instance(
                &instance.consumer_id,
                &instance.generation,
                &instance.instance.instance_id,
            )?;
            receipt(
                json!({"event": "instance_reconciled", "consumer": instance.consumer_id,
                           "instance": instance.instance,
                           "state": if retired { "retired" } else { "suspect" }}),
            );
        }
        Ok(())
    }

    /// The owner reports that it holds no helper for the grant. The report and
    /// the reconciliation run under one lock, so no helper claims in between.
    fn abandon(
        &self,
        principal: &Principal,
        key: &AttemptKey,
        reason: AbandonReason,
    ) -> Result<AttemptRecord> {
        let mut authority = self.authority()?;
        let record = authority.lookup(principal, key)?;
        if record.phase == AttemptPhase::Prepared {
            return Err(fenced("a prepared attempt is cancelled, not abandoned"));
        }
        if record.phase.terminal() {
            return Ok(record);
        }
        // A claimed grant has a helper; only its scope can settle it.
        if record.scope.is_none() {
            self.backend.record_no_helper(key, &record.owner)?;
        }
        let reconciled = self
            .reconcile_attempt(&mut authority, &record)?
            .unwrap_or(record);
        drop(authority);
        receipt(
            json!({"event": "launch_abandoned", "key": key, "reason": reason,
                       "phase": reconciled.phase, "release_reason": reconciled.release_reason}),
        );
        Ok(reconciled)
    }

    /// Observe the owner's attempt now, by the same rules as the reconciler.
    fn observe(&self, principal: &Principal, key: &AttemptKey) -> Result<AttemptRecord> {
        let mut authority = self.authority()?;
        let record = authority.lookup(principal, key)?;
        if record.phase.terminal() {
            return Ok(record);
        }
        Ok(self
            .reconcile_attempt(&mut authority, &record)?
            .unwrap_or(record))
    }

    /// Signal the owner's attempt scope; the phase does not change.
    fn terminate(
        &self,
        principal: &Principal,
        key: &AttemptKey,
        signal: StopSignal,
    ) -> Result<Termination> {
        let record = self.authority()?.lookup(principal, key)?;
        let scope = record
            .scope
            .clone()
            .ok_or_else(|| fenced("the attempt has no scope to signal"))?;
        let delivered = self.backend.signal_scope(&scope, signal.number())?;
        receipt(
            json!({"event": "scope_signalled", "key": key, "signal": signal,
                       "signalled": delivered.signalled, "complete": delivered.complete}),
        );
        Ok(Termination {
            attempt: record,
            signalled: delivered.signalled.len() as u32,
            complete: delivered.complete,
        })
    }
}

/// Join a service thread that should finish within three seconds of the
/// stop request. A thread that does not still holds the authority and its
/// lock until the process exits, so the shutdown is reported as failed.
fn finish(handle: JoinHandle<Result<()>>, done: std::sync::mpsc::Receiver<()>) -> Result<()> {
    match done.recv_timeout(Duration::from_secs(3)) {
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => handle
            .join()
            .unwrap_or_else(|_| Err(unavailable("a service thread panicked"))),
        _ => Err(unavailable(
            "a service thread did not finish within three seconds",
        )),
    }
}

/// Run reconciler passes until the service stops.
fn reconcile(launcher: Launcher, stop: Arc<AtomicBool>) -> Result<()> {
    let interval = Duration::from_millis(RECONCILE_INTERVAL_MS);
    while !stop.load(Ordering::Relaxed) {
        if launcher.paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        if let Err(error) = launcher.reconcile_pass() {
            receipt(json!({"event": "reconcile_failed", "error": error}));
            // A poisoned authority cannot recover: stop rather than keep
            // launch open without reconciliation.
            if launcher.authority.is_poisoned() {
                return Err(error);
            }
        }
        let began = std::time::Instant::now();
        while began.elapsed() < interval && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    Ok(())
}

impl Server {
    pub fn open(paths: &AuthorityPaths) -> Result<Self> {
        Self::open_with(paths, Options::default())
    }

    pub(crate) fn open_with(paths: &AuthorityPaths, options: Options) -> Result<Self> {
        paths.validate_existing()?;
        let config = HostConfig::load(paths)?;
        let storage = AuthorityStorage::open(&paths.journal())?;
        let (evidence, reason) = activate(storage, &config, paths, options.probe)?;
        // Registration and launch open only with native evidence, together
        // with the reconciler that run() starts.
        let launcher = match &evidence {
            Evidence::Native {
                authority,
                backend,
                clock,
                ..
            } => Some(Launcher {
                authority: authority.clone(),
                backend: backend.clone(),
                principals: Arc::default(),
                clock: clock.clone(),
                paused: options.reconcile_paused.unwrap_or_default(),
            }),
            Evidence::Closed { .. } => None,
        };
        paths.prepare_runtime()?;
        let socket = paths.socket();
        match fs::symlink_metadata(&socket) {
            Ok(meta) => {
                if !meta.file_type().is_socket()
                    || meta.uid() != paths.uid()
                    || meta.mode() & 0o077 != 0
                {
                    return Err(unauthorized());
                }
                // Never unlink a live/busy/unobservable endpoint. An exclusive
                // authority lock plus positive connection refusal permits stale cleanup.
                match connect_timeout(&socket, Duration::from_millis(FRAME_DEADLINE_MS)) {
                    Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
                    _ => {
                        return Err(unavailable(
                            "normal endpoint already exists or cannot be reconciled",
                        ))
                    }
                }
                let current = fs::symlink_metadata(&socket)
                    .map_err(|_| unavailable("endpoint changed during reconciliation"))?;
                if (current.dev(), current.ino()) != (meta.dev(), meta.ino()) {
                    return Err(unavailable("endpoint identity changed"));
                }
                fs::remove_file(&socket)
                    .map_err(|_| unavailable("cannot remove confirmed stale endpoint"))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(unavailable("cannot inspect normal endpoint")),
        }
        let listener =
            UnixListener::bind(&socket).map_err(|_| unavailable("cannot bind normal endpoint"))?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
            .map_err(|_| unavailable("cannot protect normal endpoint"))?;
        // SAFETY: listener owns the bound stream socket; bound backlog and workers
        // independently, including unauthenticated clients and slow readers.
        if unsafe { libc::listen(listener.as_raw_fd(), MAX_SESSIONS as i32) } != 0 {
            return Err(unavailable("cannot bound listener backlog"));
        }
        listener
            .set_nonblocking(true)
            .map_err(|_| unavailable("cannot configure listener"))?;
        let meta = fs::symlink_metadata(&socket)
            .map_err(|_| unavailable("cannot observe bound endpoint"))?;
        let status = ServiceStatus {
            storage_validated: true,
            registration_ready: launcher.is_some(),
            execution_ready: launcher.is_some(),
            reason: reason.into(),
            configuration_fingerprint: config.fingerprint()?,
        };
        Ok(Self {
            listener,
            socket,
            socket_identity: (meta.dev(), meta.ino()),
            config: Arc::new(config),
            uid: paths.uid(),
            status,
            evidence,
            launcher,
        })
    }

    /// The native authority, for isolated fixtures that wait on its pressure.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub(crate) fn native_authority(&self) -> Option<Arc<Mutex<NativeAuthority>>> {
        match &self.evidence {
            Evidence::Native { authority, .. } => Some(authority.clone()),
            Evidence::Closed { .. } => None,
        }
    }

    pub fn run(mut self, stop: Arc<AtomicBool>) -> Result<()> {
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        let mut result = Ok(());
        let sampler = match &mut self.evidence {
            Evidence::Native {
                authority,
                clock,
                probe,
                ..
            } => probe.take().map(|probe| {
                let (authority, clock, stop) = (authority.clone(), clock.clone(), stop.clone());
                let (finished, done) = std::sync::mpsc::channel::<()>();
                std::thread::Builder::new()
                    .name("devguard-pressure".into())
                    .spawn(move || {
                        let _finished = finished;
                        let _stop = StopOnExit(stop.clone());
                        sample_pressure(authority, clock, probe, stop, receipt)
                    })
                    .map(|handle| (handle, done))
                    .map_err(|_| unavailable("cannot start the host pressure sampler"))
            }),
            Evidence::Closed { .. } => None,
        }
        .transpose()?;
        // Launch is open only while its reconciler runs; if it ends, the
        // service stops rather than keep admitting without reconciliation.
        let reconciler = self
            .launcher
            .clone()
            .map(|launcher| {
                let stop = stop.clone();
                let (finished, done) = std::sync::mpsc::channel::<()>();
                std::thread::Builder::new()
                    .name("devguard-reconcile".into())
                    .spawn(move || {
                        let _finished = finished;
                        let _stop = StopOnExit(stop.clone());
                        reconcile(launcher, stop)
                    })
                    .map(|handle| (handle, done))
                    .map_err(|_| unavailable("cannot start the reconciler"))
            })
            .transpose()
            .inspect_err(|_| stop.store(true, Ordering::Relaxed))?;
        while !stop.load(Ordering::Relaxed) {
            let mut index = 0;
            while index < workers.len() {
                if workers[index].is_finished() {
                    let _ = workers.swap_remove(index).join();
                } else {
                    index += 1;
                }
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if workers.len() >= MAX_SESSIONS {
                        drop(stream);
                        continue;
                    }
                    let config = self.config.clone();
                    let uid = self.uid;
                    let status = self.status.clone();
                    let launcher = self.launcher.clone();
                    let stop = stop.clone();
                    if let Ok(worker) = std::thread::Builder::new()
                        .name("devguard-session".into())
                        .spawn(move || {
                            let _ = session(stream, uid, &config, &status, launcher, &stop);
                        })
                    {
                        workers.push(worker);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => {
                    result = Err(unavailable("listener failed"));
                    break;
                }
            }
        }
        stop.store(true, Ordering::Relaxed);
        for worker in workers {
            let _ = worker.join();
        }
        // A probe blocked in the kernel (for example statfs on a hung network
        // volume) or a stuck observation must not hold shutdown hostage.
        for (event, thread) in [
            ("pressure_stopped", sampler),
            ("reconcile_stopped", reconciler),
        ] {
            if let Some((handle, done)) = thread {
                if let Err(error) = finish(handle, done) {
                    receipt(json!({"event": event, "error": error}));
                    if result.is_ok() {
                        result = Err(error);
                    }
                }
            }
        }
        result
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Do not remove an unknown replacement at the original pathname.
        if let Ok(meta) = fs::symlink_metadata(&self.socket) {
            if meta.file_type().is_socket() && (meta.dev(), meta.ino()) == self.socket_identity {
                let _ = fs::remove_file(&self.socket);
            }
        }
    }
}

fn digest_matches(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn authenticate(
    config: &HostConfig,
    credential: CallerCredential,
) -> Result<(SessionRole, Option<ConsumerCredential>)> {
    match credential {
        CallerCredential::Consumer {
            consumer_id,
            generation,
            secret,
        } => {
            validate_id(&consumer_id)?;
            validate_id(&generation)?;
            let consumer = config
                .consumers
                .get(&consumer_id)
                .ok_or_else(unauthorized)?;
            if generation != consumer.generation
                || !digest_matches(&consumer.credential_sha256, &secret.digest())
            {
                return Err(unauthorized());
            }
            let role = match consumer.role {
                ConsumerRole::Workload => SessionRole::Workload,
                ConsumerRole::ControlService => SessionRole::ControlService,
            };
            Ok((
                role,
                Some(ConsumerCredential {
                    id: consumer_id,
                    generation,
                    secret,
                }),
            ))
        }
        CallerCredential::Administrator { secret } => {
            if !digest_matches(&config.admin_credential_sha256, &secret.digest()) {
                return Err(unauthorized());
            }
            Ok((SessionRole::Administrator, None))
        }
    }
}

/// What one session has established so far.
#[derive(Default)]
struct Session {
    greeted: bool,
    role: Option<SessionRole>,
    consumer: Option<ConsumerCredential>,
    principal: Option<Principal>,
    /// A helper session presents one grant and makes no other request.
    helper: bool,
}

struct Context<'a> {
    uid: u32,
    caller: PeerIdentity,
    config: &'a HostConfig,
    status: &'a ServiceStatus,
    launcher: Option<&'a Launcher>,
}

fn session(
    mut stream: UnixStream,
    uid: u32,
    config: &HostConfig,
    status: &ServiceStatus,
    launcher: Option<Launcher>,
    stop: &AtomicBool,
) -> Result<()> {
    // Framing uses poll and per-call nonblocking I/O with an absolute deadline;
    // it is independent of Darwin's inherited listener O_NONBLOCK flag.
    let caller = peer::observe(&stream)?;
    if caller.uid != uid {
        return Err(unauthorized());
    }
    let context = Context {
        uid,
        caller,
        config,
        status,
        launcher: launcher.as_ref(),
    };
    let timeout = Duration::from_millis(FRAME_DEADLINE_MS);
    let mut state = Session::default();
    while !stop.load(Ordering::Relaxed) {
        let frame: Frame<Request> = framing::read_frame(&mut stream, timeout)?;
        if frame.version != WIRE_VERSION || frame.request_id == 0 {
            let body = Response::Error(
                Error::new(
                    ErrorCode::ResourcePolicyUnsupported,
                    "unsupported wire version or request identity",
                )
                .into(),
            );
            framing::write_frame(
                &mut stream,
                &Frame {
                    version: WIRE_VERSION,
                    request_id: frame.request_id,
                    body,
                },
                timeout,
            )?;
            return Ok(());
        }
        let body = handle(&mut state, frame.body, &context)
            .unwrap_or_else(|error| Response::Error(error.into()));
        framing::write_frame(
            &mut stream,
            &Frame {
                version: WIRE_VERSION,
                request_id: frame.request_id,
                body,
            },
            timeout,
        )?;
    }
    Ok(())
}

/// The launcher and the instance registered in this session.
fn registered<'a>(
    session: &'a Session,
    context: &Context<'a>,
) -> Result<(&'a Launcher, &'a Principal)> {
    match (context.launcher, &session.principal) {
        (Some(launcher), Some(principal)) => Ok((launcher, principal)),
        (None, _) if session.consumer.is_some() => Err(unavailable(
            "native registration is not ready; no principal or budget was issued",
        )),
        _ => Err(unauthorized()),
    }
}

fn handle(session: &mut Session, request: Request, context: &Context<'_>) -> Result<Response> {
    if session.helper {
        return Err(Error::new(
            ErrorCode::Unauthorized,
            "a helper session makes no other request",
        ));
    }
    match request {
        Request::Hello { compatibility } => {
            if session.greeted {
                return Err(Error::new(
                    ErrorCode::InvalidTransition,
                    "session already negotiated",
                ));
            }
            let capabilities = if context.launcher.is_some() {
                launch_capabilities()
            } else {
                BTreeSet::new()
            };
            compatibility.check(PROTOCOL_VERSION, &capabilities)?;
            session.greeted = true;
            Ok(Response::Hello(Hello {
                protocol: PROTOCOL_VERSION,
                authority: PeerIdentity {
                    uid: context.uid,
                    pid: std::process::id(),
                },
                caller: context.caller,
                capabilities,
                max_frame_bytes: MAX_FRAME_BYTES,
                frame_deadline_ms: FRAME_DEADLINE_MS,
                max_sessions: MAX_SESSIONS,
            }))
        }
        Request::Authenticate { credential } => {
            if !session.greeted {
                return Err(unauthorized());
            }
            if session.role.is_some() {
                return Err(Error::new(
                    ErrorCode::InvalidTransition,
                    "session already authenticated",
                ));
            }
            let (role, consumer) = authenticate(context.config, credential)?;
            session.role = Some(role);
            session.consumer = consumer;
            Ok(Response::Authenticated { role })
        }
        Request::Status => {
            if session.role.is_none() {
                return Err(unauthorized());
            }
            Ok(Response::Status(context.status.clone()))
        }
        Request::Register { instance_id } => {
            validate_id(&instance_id)?;
            let Some(consumer) = &session.consumer else {
                return Err(unauthorized());
            };
            let Some(launcher) = context.launcher else {
                return Err(unavailable(
                    "native registration is not ready; no principal or budget was issued",
                ));
            };
            if session.principal.is_some() {
                return Err(Error::new(
                    ErrorCode::InvalidTransition,
                    "this session already registered its instance",
                ));
            }
            let principal = launcher.register(context.caller, consumer, instance_id)?;
            let instance = principal.instance().clone();
            session.principal = Some(principal);
            Ok(Response::Registered { instance })
        }
        Request::Admit { request } => {
            let (launcher, principal) = registered(session, context)?;
            let record = launcher.authority()?.admit(principal, request)?;
            receipt(
                json!({"event": "admission", "key": record.key, "phase": record.phase,
                           "denial": record.denial,
                           "quantities": record.reservation.as_ref().map(|r| r.quantities)}),
            );
            Ok(Response::Attempt(record))
        }
        Request::BeginLaunch { key } => {
            let (launcher, principal) = registered(session, context)?;
            let decision = launcher.authority()?.begin_launch(principal, &key)?;
            receipt(
                json!({"event": "launch_committed", "key": decision.attempt.key,
                           "phase": decision.attempt.phase,
                           "permit_issued": decision.permit.is_some()}),
            );
            Ok(Response::LaunchGranted(LaunchGrant {
                attempt: decision.attempt,
                permit: decision.permit,
            }))
        }
        Request::Lookup { key } => {
            let (launcher, principal) = registered(session, context)?;
            let record = launcher.authority()?.lookup(principal, &key)?;
            Ok(Response::Attempt(record))
        }
        Request::Cancel { key } => {
            let (launcher, principal) = registered(session, context)?;
            let record = launcher.authority()?.cancel(principal, &key)?;
            receipt(json!({"event": "cancelled", "key": record.key, "phase": record.phase}));
            Ok(Response::Attempt(record))
        }
        Request::AbandonLaunch { key, reason } => {
            let (launcher, principal) = registered(session, context)?;
            Ok(Response::Attempt(
                launcher.abandon(principal, &key, reason)?,
            ))
        }
        Request::Observe { key } => {
            let (launcher, principal) = registered(session, context)?;
            Ok(Response::Attempt(launcher.observe(principal, &key)?))
        }
        Request::Terminate { key, signal } => {
            let (launcher, principal) = registered(session, context)?;
            Ok(Response::Terminated(
                launcher.terminate(principal, &key, signal)?,
            ))
        }
        Request::Launch {
            key,
            instance_id,
            permit,
        } => {
            // A helper is not a caller: it presents only the grant.
            if !session.greeted || session.role.is_some() {
                return Err(unauthorized());
            }
            session.helper = true;
            let launcher = context
                .launcher
                .ok_or_else(|| unavailable("fenced launch is not open"))?;
            Ok(Response::LaunchAuthorized(launcher.authorize_helper(
                context.caller,
                key,
                instance_id,
                permit,
            )?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::paths::read_private;
    use devguard_client::Client;
    use devguard_contract::{Capability, Compatibility, Secret};

    struct Fixture {
        _directory: tempfile::TempDir,
        paths: AuthorityPaths,
        config: HostConfig,
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<Result<()>>>,
    }
    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::Builder::new()
                .prefix("dg-rpc-")
                .tempdir_in(if cfg!(target_os = "macos") {
                    "/private/tmp"
                } else {
                    "/tmp"
                })
                .unwrap();
            let paths = AuthorityPaths::fixture(directory.path());
            let config = config::initialize(&paths).unwrap();
            let server = Server::open(&paths).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            let worker = Some(std::thread::spawn(move || server.run(signal)));
            Self {
                _directory: directory,
                paths,
                config,
                stop,
                worker,
            }
        }
        fn compatibility() -> Compatibility {
            Compatibility {
                minimum_protocol: 1,
                maximum_protocol: 1,
                required: BTreeSet::new(),
            }
        }
        fn client(&self) -> Client {
            Client::connect(
                &self.paths.socket(),
                self.paths.uid(),
                Self::compatibility(),
            )
            .unwrap()
        }
        fn credential(&self) -> CallerCredential {
            let secret = Secret::new(
                String::from_utf8(
                    read_private(&self.paths.cli_credential(), self.paths.uid(), 64).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
            CallerCredential::Consumer {
                consumer_id: "dev-cli".into(),
                generation: self.config.consumers["dev-cli"].generation.clone(),
                secret,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                worker.join().unwrap().unwrap();
            }
        }
    }

    #[test]
    fn authenticated_peer_is_os_observed_and_registration_opens_only_with_native_evidence() {
        let fixture = Fixture::new();
        let mut client = fixture.client();
        assert_eq!(client.hello.caller.pid, std::process::id());
        assert_eq!(client.hello.caller.uid, fixture.paths.uid());
        assert_eq!(client.status().unwrap_err().code, ErrorCode::Unauthorized);
        assert_eq!(
            client.authenticate(fixture.credential()).unwrap(),
            SessionRole::Workload
        );
        let status = client.status().unwrap();
        assert!(status.storage_validated);
        if cfg!(target_os = "macos") {
            assert!(status.registration_ready && status.execution_ready);
            assert_eq!(status.reason, LAUNCH_REASON);
            let instance = client.register("owner".into()).unwrap();
            assert_eq!(instance.process.pid, std::process::id());
        } else {
            assert!(!status.registration_ready && !status.execution_ready);
            assert_eq!(status.reason, UNSUPPORTED_REASON);
            assert_eq!(
                client.register("owner".into()).unwrap_err().code,
                ErrorCode::ResourceControlUnavailable
            );
        }
        assert!(Server::open(&fixture.paths).is_err());
    }

    #[test]
    fn roles_secrets_generations_and_required_capabilities_cannot_be_substituted() {
        let fixture = Fixture::new();
        let mut client = fixture.client();
        let CallerCredential::Consumer { secret, .. } = fixture.credential() else {
            unreachable!()
        };
        assert_eq!(
            client
                .authenticate(CallerCredential::Administrator {
                    secret: secret.clone()
                })
                .unwrap_err()
                .code,
            ErrorCode::Unauthorized
        );
        assert_eq!(
            client
                .authenticate(CallerCredential::Consumer {
                    consumer_id: "dev-cli".into(),
                    generation: "wrong".into(),
                    secret
                })
                .unwrap_err()
                .code,
            ErrorCode::Unauthorized
        );
        let admin = Secret::new(
            String::from_utf8(
                read_private(&fixture.paths.admin_credential(), fixture.paths.uid(), 64).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            client
                .authenticate(CallerCredential::Administrator { secret: admin })
                .unwrap(),
            SessionRole::Administrator
        );
        assert_eq!(
            client.register("owner".into()).unwrap_err().code,
            ErrorCode::Unauthorized
        );
        // No macOS service provides Linux cgroup control.
        let required = Compatibility {
            required: BTreeSet::from([Capability::LinuxCgroupV2]),
            ..Fixture::compatibility()
        };
        assert!(
            matches!(Client::connect(&fixture.paths.socket(),fixture.paths.uid(),required),Err(e) if e.code==ErrorCode::ResourcePolicyUnsupported)
        );
        assert!(
            matches!(Client::connect(&fixture.paths.socket(),fixture.paths.uid()+1,Fixture::compatibility()),Err(e) if e.code==ErrorCode::Unauthorized)
        );
    }

    #[test]
    fn concurrent_registration_requests_never_create_unobserved_principals() {
        let fixture = Fixture::new();
        // Ten concurrent registrations against the consumer's eight slots.
        let threads: Vec<_> = (0..10)
            .map(|n| {
                let path = fixture.paths.socket();
                let uid = fixture.paths.uid();
                let credential = fixture.credential();
                std::thread::spawn(move || {
                    let mut client = Client::connect(&path, uid, Fixture::compatibility()).unwrap();
                    client.authenticate(credential).unwrap();
                    client.register(format!("owner-{n}"))
                })
            })
            .collect();
        let results: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        if cfg!(target_os = "macos") {
            let registered: Vec<_> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
            assert_eq!(registered.len(), 8, "{results:?}");
            // Every principal is this OS-observed process.
            assert!(registered
                .iter()
                .all(|instance| instance.process.pid == std::process::id()));
            assert!(results
                .iter()
                .filter_map(|r| r.as_ref().err())
                .all(|error| error.code == ErrorCode::ResourceUnavailable));
        } else {
            assert!(results
                .iter()
                .all(|r| r.as_ref().unwrap_err().code == ErrorCode::ResourceControlUnavailable));
        }
    }

    #[test]
    fn wrong_wire_and_unknown_peer_fields_are_rejected_without_leaking_secrets() {
        let fixture = Fixture::new();
        let mut stream = UnixStream::connect(fixture.paths.socket()).unwrap();
        let timeout = Duration::from_millis(250);
        framing::write_frame(
            &mut stream,
            &Frame {
                version: 99,
                request_id: 1,
                body: Request::Hello {
                    compatibility: Fixture::compatibility(),
                },
            },
            timeout,
        )
        .unwrap();
        let result: Frame<Response> = framing::read_frame(&mut stream, timeout).unwrap();
        assert!(
            matches!(result.body,Response::Error(e) if e.code==ErrorCode::ResourcePolicyUnsupported)
        );
        let raw = serde_json::json!({"method":"authenticate","params":{"credential":{"kind":"consumer","consumer_id":"dev-cli","generation":"g","secret":"a".repeat(64),"pid":1,"uid":0}}});
        assert!(serde_json::from_value::<Request>(raw).is_err());
        assert!(!format!("{:?}", fixture.credential()).contains(
            &String::from_utf8(
                read_private(&fixture.paths.cli_credential(), fixture.paths.uid(), 64).unwrap()
            )
            .unwrap()
        ));
    }

    #[test]
    fn endpoints_require_positive_stale_evidence_and_preserve_replacements() {
        let mut fixture = Fixture::new();
        fixture.stop.store(true, Ordering::Relaxed);
        fixture.worker.take().unwrap().join().unwrap().unwrap();
        let path = fixture.paths.socket();
        assert!(!path.exists());
        let live = UnixListener::bind(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            matches!(Server::open(&fixture.paths), Err(e) if e.code == ErrorCode::ResourceControlUnavailable)
        );
        assert!(path.exists());
        drop(live);
        let server = Server::open(&fixture.paths).unwrap();
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_socket());
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"replacement evidence").unwrap();
        drop(server);
        assert_eq!(fs::read(&path).unwrap(), b"replacement evidence");
        assert!(Server::open(&fixture.paths).is_err());
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(fixture.paths.journal(), &path).unwrap();
        assert!(Server::open(&fixture.paths).is_err());
        assert!(fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn partial_frame_pressure_expires_and_shutdown_remains_bounded() {
        use std::io::Write;
        use std::time::Instant;
        let mut fixture = Fixture::new();
        let mut streams = Vec::new();
        for _ in 0..MAX_SESSIONS * 2 {
            if let Ok(mut stream) =
                connect_timeout(&fixture.paths.socket(), Duration::from_millis(250))
            {
                let _ = stream.write_all(&[0]);
                streams.push(stream);
            }
        }
        // Every partial frame must expire, regardless of accepted/backlogged order.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(mut client) = Client::connect(
                &fixture.paths.socket(),
                fixture.paths.uid(),
                Fixture::compatibility(),
            ) {
                if client.authenticate(fixture.credential()).is_ok() && client.status().is_ok() {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "service did not recover after bounded partial frames"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut final_peer =
            connect_timeout(&fixture.paths.socket(), Duration::from_millis(250)).unwrap();
        final_peer.write_all(&[0, 0]).unwrap();
        let began = Instant::now();
        fixture.stop.store(true, Ordering::Relaxed);
        fixture.worker.take().unwrap().join().unwrap().unwrap();
        assert!(began.elapsed() < Duration::from_secs(2));
        assert!(!fixture.paths.socket().exists());
    }

    #[test]
    fn a_process_of_an_earlier_boot_is_never_running() {
        use devguard_contract::ProcessIdentity;
        let identity = ProcessIdentity {
            boot_id: "old".into(),
            pid: 42,
            start_ticks: 7,
        };
        // No observation is made for an identity of an earlier boot.
        assert!(!still_running(
            &identity,
            "new",
            || -> Result<Option<ProcessIdentity>> { panic!("observed an earlier boot") }
        ));
        assert!(still_running(&identity, "old", || Ok(Some(
            identity.clone()
        ))));
        assert!(!still_running(&identity, "old", || Ok(None)));
        let mut reused = identity.clone();
        reused.start_ticks += 1;
        assert!(!still_running(&identity, "old", || Ok(Some(
            reused.clone()
        ))));
        // Within this boot a refused observation defers; it never ends a process.
        assert!(still_running(&identity, "old", || Err(unavailable(
            "refused"
        ))));
    }

    /// Sessions of a service with registration and fenced launch open.
    #[cfg(target_os = "macos")]
    mod launch {
        use super::*;
        use crate::fixture::{TestAuthority, CONSUMER};
        use devguard_contract::{
            AdmissionRequest, AttemptKey, Budget, Capability, Compatibility, ResourceIntent,
            ResourceLevels, Secret,
        };

        struct Open {
            authority: TestAuthority,
            _directory: tempfile::TempDir,
        }

        fn open() -> Open {
            let directory = tempfile::Builder::new()
                .prefix("dg-s-")
                .tempdir_in("/private/tmp")
                .unwrap();
            let authority = TestAuthority::start(directory.path()).unwrap();
            Open {
                authority,
                _directory: directory,
            }
        }

        fn connect(open: &Open) -> Client {
            let compatibility = Compatibility {
                minimum_protocol: 1,
                maximum_protocol: 1,
                required: BTreeSet::from([Capability::DurableAdmission, Capability::FencedLaunch]),
            };
            Client::connect(
                &open.authority.socket(),
                open.authority.uid(),
                compatibility,
            )
            .unwrap()
        }

        fn consumer(open: &Open, instance: &str) -> Client {
            let mut client = connect(open);
            client
                .authenticate(open.authority.consumer().unwrap())
                .unwrap();
            client.register(instance.into()).unwrap();
            client
        }

        fn key(open: &Open, attempt: &str) -> AttemptKey {
            AttemptKey {
                consumer_id: CONSUMER.into(),
                consumer_generation: open.authority.generation(),
                attempt_id: attempt.into(),
            }
        }

        #[test]
        fn launch_open_service_advertises_fenced_launch_and_readiness() {
            let open = open();
            let mut client = connect(&open);
            assert!(client
                .hello
                .capabilities
                .contains(&Capability::FencedLaunch));
            client
                .authenticate(open.authority.consumer().unwrap())
                .unwrap();
            let status = client.status().unwrap();
            assert!(status.registration_ready && status.execution_ready);
            assert_eq!(status.reason, LAUNCH_REASON);
        }

        #[test]
        fn launch_registration_binds_the_observed_peer_and_needs_a_consumer() {
            let open = open();
            let mut anonymous = connect(&open);
            assert_eq!(
                anonymous.register("anonymous".into()).unwrap_err().code,
                ErrorCode::Unauthorized
            );
            let admin = Secret::new(
                String::from_utf8(
                    read_private(
                        &open.authority.paths().admin_credential(),
                        open.authority.uid(),
                        64,
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
            let mut administrator = connect(&open);
            administrator
                .authenticate(CallerCredential::Administrator { secret: admin })
                .unwrap();
            assert_eq!(
                administrator.register("admin".into()).unwrap_err().code,
                ErrorCode::Unauthorized
            );
            let mut client = connect(&open);
            client
                .authenticate(open.authority.consumer().unwrap())
                .unwrap();
            let instance = client.register("peer".into()).unwrap();
            // The identity is the kernel's observation of this very process.
            assert_eq!(instance.process.pid, std::process::id());
            assert_eq!(
                Some(instance.process.clone()),
                devguard_macos::process_identity(&instance.process.boot_id, std::process::id())
                    .unwrap()
            );
            assert_eq!(
                client.register("second".into()).unwrap_err().code,
                ErrorCode::InvalidTransition
            );
            // Another session must register before acting for an instance.
            let mut unregistered = connect(&open);
            unregistered
                .authenticate(open.authority.consumer().unwrap())
                .unwrap();
            assert_eq!(
                unregistered.lookup(key(&open, "any")).unwrap_err().code,
                ErrorCode::Unauthorized
            );
        }

        #[test]
        fn launch_attempts_belong_to_the_registered_instance() {
            let open = open();
            open.authority
                .wait_until_admitting(Duration::from_secs(10))
                .unwrap();
            let mut owner = consumer(&open, "owner");
            let request = AdmissionRequest {
                key: key(&open, "owned"),
                execution_digest: devguard_contract::digest_bytes(b"session test"),
                intent: ResourceIntent {
                    profile: "interactive".into(),
                    requested: Budget {
                        cpu_milli: 500,
                        memory_bytes: 64 * 1024 * 1024,
                        tasks: 4,
                    },
                    minimum: ResourceLevels::MACOS,
                },
            };
            assert!(owner.admit(request).unwrap().reservation.is_some());
            let mut other = consumer(&open, "other");
            for code in [
                other.lookup(key(&open, "owned")).unwrap_err().code,
                other.cancel(key(&open, "owned")).unwrap_err().code,
                other.begin_launch(key(&open, "owned")).unwrap_err().code,
            ] {
                assert_eq!(code, ErrorCode::Unauthorized);
            }
            let grant = owner.begin_launch(key(&open, "owned")).unwrap();
            assert!(grant.permit.is_some());
            assert!(owner
                .begin_launch(key(&open, "owned"))
                .unwrap()
                .permit
                .is_none());
        }

        #[test]
        fn launch_reconciler_stops_the_service_on_a_poisoned_authority() {
            let directory = tempfile::Builder::new()
                .prefix("dg-p-")
                .tempdir_in("/private/tmp")
                .unwrap();
            let paths = AuthorityPaths::fixture(directory.path());
            config::initialize(&paths).unwrap();
            let server = Server::open_with(&paths, Options::default()).unwrap();
            let launcher = server.launcher.clone().unwrap();
            let poisoner = launcher.authority.clone();
            let _ = std::thread::spawn(move || {
                let _held = poisoner.lock().unwrap();
                panic!("poison the authority for this test");
            })
            .join();
            assert!(launcher.authority.is_poisoned());
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            let result = std::thread::spawn(move || {
                let _stop = StopOnExit(signal.clone());
                reconcile(launcher, signal)
            })
            .join()
            .unwrap();
            assert!(result.is_err());
            // The ended reconciler stopped the service.
            assert!(stop.load(Ordering::Relaxed));
            drop(server);
        }

        #[test]
        fn launch_helper_session_presents_one_grant_and_nothing_else() {
            let open = open();
            let permit = Secret::new("d".repeat(64)).unwrap();
            // A caller cannot present a grant on its authenticated session.
            let mut caller = consumer(&open, "caller");
            assert_eq!(
                caller
                    .launch(key(&open, "x"), "caller".into(), permit.clone())
                    .unwrap_err()
                    .code,
                ErrorCode::Unauthorized
            );
            // The authority process is never a helper, and after presenting
            // a grant a session can make no other request.
            let mut helper = connect(&open);
            assert_eq!(
                helper
                    .launch(key(&open, "x"), "caller".into(), permit)
                    .unwrap_err()
                    .code,
                ErrorCode::Unauthorized
            );
            assert_eq!(helper.status().unwrap_err().code, ErrorCode::Unauthorized);
            assert_eq!(
                helper
                    .authenticate(open.authority.consumer().unwrap())
                    .unwrap_err()
                    .code,
                ErrorCode::Unauthorized
            );
        }
    }

    #[cfg(target_os = "macos")]
    mod native {
        use super::*;
        use devguard_contract::{Budget, Result};
        use devguard_macos::{HostReading, VolumeReading};
        use std::sync::atomic::AtomicU64;
        use std::time::Instant;

        const GIB: u64 = 1024 * 1024 * 1024;

        /// Healthy synthetic readings with injectable failures and delays; the
        /// real host may legitimately be under pressure.
        #[derive(Clone, Default)]
        struct Scripted {
            fail: Arc<AtomicBool>,
            /// Milliseconds the next read blocks, as a probe stuck in the kernel.
            delay_next_ms: Arc<AtomicU64>,
            /// When the most recent read finished, in boot-relative milliseconds.
            last_read_ms: Arc<AtomicU64>,
            clock: Option<BootClock>,
        }
        impl HostProbe for Scripted {
            fn read(&mut self) -> Result<HostReading> {
                let delay = self.delay_next_ms.swap(0, Ordering::Relaxed);
                if delay > 0 {
                    std::thread::sleep(Duration::from_millis(delay));
                }
                if self.fail.load(Ordering::Relaxed) {
                    return Err(Error::new(
                        ErrorCode::ResourceControlUnavailable,
                        "injected probe failure",
                    ));
                }
                if let Some(clock) = &self.clock {
                    self.last_read_ms
                        .store(clock.now().monotonic_ms, Ordering::Relaxed);
                }
                Ok(HostReading {
                    memory_level: 1,
                    paged_out_bytes: 0,
                    swap_used_bytes: 0,
                    volumes: vec![VolumeReading {
                        mount: "/synthetic".into(),
                        capacity_bytes: 100 * GIB,
                        available_bytes: 60 * GIB,
                    }],
                })
            }
        }

        /// Qualification runs collect raw receipts; ordinary runs write nothing.
        fn record(name: &str, value: serde_json::Value) {
            if let Some(directory) = std::env::var_os("DEVGUARD_EVIDENCE_DIR") {
                let directory = PathBuf::from(directory);
                fs::create_dir_all(&directory).unwrap();
                fs::write(
                    directory.join(format!("{name}.json")),
                    serde_json::to_vec_pretty(&value).unwrap(),
                )
                .unwrap();
            }
        }

        fn wait_for(authority: &Mutex<NativeAuthority>, state: PressureState, limit: Duration) {
            let deadline = Instant::now() + limit;
            while authority.lock().unwrap().pressure() != state {
                assert!(Instant::now() < deadline, "pressure never became {state:?}");
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        struct Running {
            _directory: tempfile::TempDir,
            config: HostConfig,
            paths: AuthorityPaths,
            authority: Arc<Mutex<NativeAuthority>>,
            clock: BootClock,
            probe: Scripted,
            stop: Arc<AtomicBool>,
            receipts: Arc<Mutex<Vec<serde_json::Value>>>,
            worker: Option<JoinHandle<Result<()>>>,
        }

        impl Running {
            fn start() -> Self {
                let directory = tempfile::Builder::new()
                    .prefix("dg-native-")
                    .tempdir_in("/private/tmp")
                    .unwrap();
                let paths = AuthorityPaths::fixture(directory.path());
                let config = config::initialize(&paths).unwrap();
                let storage = AuthorityStorage::open(&paths.journal()).unwrap();
                let (evidence, reason) = activate(storage, &config, &paths, None).unwrap();
                assert_eq!(reason, LAUNCH_REASON);
                let Evidence::Native {
                    authority, clock, ..
                } = evidence
                else {
                    panic!("native evidence was not activated");
                };
                let probe = Scripted {
                    clock: Some(clock.clone()),
                    ..Scripted::default()
                };
                Self {
                    _directory: directory,
                    config,
                    paths,
                    authority,
                    clock,
                    probe,
                    stop: Arc::new(AtomicBool::new(false)),
                    receipts: Arc::new(Mutex::new(Vec::new())),
                    worker: None,
                }
            }

            fn sample(&mut self) {
                let (authority, clock, stop) = (
                    self.authority.clone(),
                    self.clock.clone(),
                    self.stop.clone(),
                );
                let (probe, receipts) = (self.probe.clone(), self.receipts.clone());
                self.worker = Some(std::thread::spawn(move || {
                    sample_pressure(authority, clock, probe, stop, |receipt| {
                        receipts.lock().unwrap().push(receipt)
                    })
                }));
            }

            fn finish(mut self) {
                self.stop.store(true, Ordering::Relaxed);
                self.worker.take().unwrap().join().unwrap().unwrap();
            }
        }

        #[test]
        fn serve_activates_the_journal_with_native_evidence_and_samples_every_interval() {
            let mut running = Running::start();
            // Native activation holds the exclusive lock like storage alone.
            assert!(AuthorityStorage::open(&running.paths.journal()).is_err());
            let capacity = NativeHost::open().unwrap().capacity();
            let expected = running
                .config
                .policy(capacity, running.paths.uid())
                .unwrap();
            let authority = running.authority.clone();
            assert_eq!(
                authority.lock().unwrap().pressure(),
                PressureState::Critical
            );
            assert_eq!(
                authority.lock().unwrap().available_budget().unwrap(),
                Budget::ZERO
            );
            running.sample();
            // Baseline immediately, first sample one interval later.
            wait_for(&authority, PressureState::Normal, Duration::from_secs(4));
            assert_eq!(
                authority.lock().unwrap().available_budget().unwrap(),
                expected.work_capacity().unwrap()
            );
            // An injected probe failure closes admission at the next reading.
            let clock = running.clock.clone();
            let injected = clock.now();
            running.probe.fail.store(true, Ordering::Relaxed);
            wait_for(&authority, PressureState::Critical, Duration::from_secs(3));
            let closed = clock.now();
            // Recovery needs fresh valid readings; stopping keeps it closed.
            running.probe.fail.store(false, Ordering::Relaxed);
            let receipts = running.receipts.clone();
            running.finish();
            assert_eq!(
                authority.lock().unwrap().pressure(),
                PressureState::Critical
            );
            let receipts = receipts.lock().unwrap().clone();
            assert!(receipts
                .iter()
                .any(|receipt| receipt["event"] == "pressure_observation_failed"));
            let delay = closed.monotonic_ms - injected.monotonic_ms;
            // At most one sampling interval plus scheduling from injection.
            assert!(delay <= SAMPLE_INTERVAL_MS + 1_000, "{delay} ms");
            record(
                "service-probe-failure",
                json!({"injected_at": injected, "critical_observed_at": closed,
                       "service_time_to_critical_ms": delay,
                       "sample_interval_ms": SAMPLE_INTERVAL_MS, "receipts": receipts}),
            );
        }

        #[test]
        fn a_stuck_probe_does_not_hold_the_authority_and_its_delay_closes_admission() {
            let mut running = Running::start();
            let authority = running.authority.clone();
            let clock = running.clock.clone();
            running.sample();
            wait_for(&authority, PressureState::Normal, Duration::from_secs(4));
            // The next read blocks for eight seconds, like statfs on a hung volume.
            running.probe.delay_next_ms.store(8_000, Ordering::Relaxed);
            let blocked_at = clock.now();
            // Wait for the blocked read to begin: no sample completes afterwards.
            std::thread::sleep(Duration::from_millis(2_300));
            let last_read = running.probe.last_read_ms.load(Ordering::Relaxed);
            // The blocked reading holds no authority lock.
            let began = Instant::now();
            drop(authority.lock().unwrap());
            let lock_wait = began.elapsed();
            assert!(lock_wait < Duration::from_millis(100), "{lock_wait:?}");
            // Admission closes six seconds after the last completed sample,
            // while the probe is still blocked.
            let mut last_open = last_read;
            let closed_at = loop {
                let before = clock.now().monotonic_ms;
                if authority.lock().unwrap().pressure() == PressureState::Critical {
                    break clock.now().monotonic_ms;
                }
                last_open = before;
                assert!(before < blocked_at.monotonic_ms + 12_000, "never closed");
                std::thread::sleep(Duration::from_millis(20));
            };
            assert!(last_open - last_read <= 6_000);
            assert!(closed_at - last_read > 6_000);
            // Closed before the stuck read returned (about ten seconds after it).
            assert_eq!(
                running.probe.last_read_ms.load(Ordering::Relaxed),
                last_read
            );
            // When the read returns, its duration is on the next receipt, and
            // sampling resumes without a burst of degenerate readings.
            let deadline = Instant::now() + Duration::from_secs(12);
            let lagged = loop {
                let found = running
                    .receipts
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|receipt| {
                        ["baseline", "sample", "failure"]
                            .iter()
                            .any(|key| receipt[key]["read_ms"].as_u64().unwrap_or(0) >= 7_000)
                    })
                    .cloned();
                if let Some(receipt) = found {
                    break receipt;
                }
                assert!(
                    Instant::now() < deadline,
                    "the slow read was not reported: {:?}",
                    running.receipts.lock().unwrap()
                );
                std::thread::sleep(Duration::from_millis(50));
            };
            // The overrun is reported as control-loop lag on the next sample.
            let deadline = Instant::now() + Duration::from_secs(6);
            let overrun = loop {
                let found = running
                    .receipts
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|receipt| {
                        receipt["sample"]["control_lag_ms"].as_u64().unwrap_or(0) >= 4_000
                    })
                    .cloned();
                if let Some(receipt) = found {
                    break receipt;
                }
                assert!(
                    Instant::now() < deadline,
                    "the overrun was not reported as lag: {:?}",
                    running.receipts.lock().unwrap()
                );
                std::thread::sleep(Duration::from_millis(50));
            };
            // The loop recovers to ordinary samples, never a spurious failure.
            wait_for(&authority, PressureState::Critical, Duration::from_secs(1));
            std::thread::sleep(Duration::from_millis(4_500));
            assert!(!running
                .receipts
                .lock()
                .unwrap()
                .iter()
                .any(|receipt| receipt["event"] == "pressure_observation_failed"));
            running.finish();
            record(
                "service-delayed-probe",
                json!({"last_completed_read_ms": last_read, "closed_at_ms": closed_at,
                       "time_to_closed_ms": closed_at - last_read,
                       "authority_lock_wait_us": lock_wait.as_micros() as u64,
                       "slow_read_receipt": lagged, "overrun_receipt": overrun}),
            );
        }
    }
}
