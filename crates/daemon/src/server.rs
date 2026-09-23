use crate::{config::HostConfig, paths::AuthorityPaths};
use devguard_client::{connect::connect_timeout, framing, peer, protocol::*};
use devguard_contract::{validate_id, Error, ErrorCode, Result, PROTOCOL_VERSION};
use devguard_core::{Authority, AuthorityStorage, Clock, ConsumerRole, PressureState};
use devguard_macos::{
    BootClock, HostProbe, NativeBackend, NativeHost, NativeProbe, Sampler, SamplerOutcome,
    SAMPLE_INTERVAL_MS,
};
use serde_json::json;
use std::collections::BTreeSet;
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

type NativeAuthority = Authority<NativeBackend, BootClock>;

const NATIVE_REASON: &str = "native boot, process and host pressure evidence is observed; registration and execution remain closed until launch and reconciliation are installed";
const UNSUPPORTED_REASON: &str = "native host evidence is unsupported on this platform; registration and execution remain closed";
const FAILED_REASON: &str =
    "native host observation failed; registration and execution remain closed";

/// Exclusive storage, activated with actual host evidence when it is available.
enum Evidence {
    Native {
        authority: Arc<Mutex<NativeAuthority>>,
        clock: BootClock,
        probe: Option<NativeProbe>,
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
    let probe = match NativeProbe::new(volumes.clone()) {
        Ok(probe) => probe,
        Err(error) => {
            receipt(json!({"event": "native_host_unavailable", "error": error}));
            return Ok((Evidence::Closed { _storage: storage }, FAILED_REASON));
        }
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
            probe: Some(probe),
        },
        NATIVE_REASON,
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

impl Server {
    pub fn open(paths: &AuthorityPaths) -> Result<Self> {
        paths.validate_existing()?;
        let config = HostConfig::load(paths)?;
        let storage = AuthorityStorage::open(&paths.journal())?;
        let (evidence, reason) = activate(storage, &config, paths)?;
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
            registration_ready: false,
            execution_ready: false,
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
        })
    }

    pub fn run(mut self, stop: Arc<AtomicBool>) -> Result<()> {
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        let mut result = Ok(());
        let sampler = match &mut self.evidence {
            Evidence::Native {
                authority,
                clock,
                probe,
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
                    let stop = stop.clone();
                    if let Ok(worker) = std::thread::Builder::new()
                        .name("devguard-session".into())
                        .spawn(move || {
                            let _ = session(stream, uid, &config, &status, &stop);
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
        if let Some((handle, done)) = sampler {
            // A probe blocked in the kernel (for example statfs on a hung
            // network volume) must not hold shutdown hostage.
            match done.recv_timeout(Duration::from_secs(3)) {
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    let outcome = handle
                        .join()
                        .unwrap_or_else(|_| Err(unavailable("host pressure sampler panicked")));
                    if let Err(error) = outcome {
                        receipt(json!({"event": "pressure_stopped", "error": error}));
                        if result.is_ok() {
                            result = Err(error);
                        }
                    }
                }
                _ => {
                    // The stuck thread still holds the authority and its lock
                    // until the process exits; report the shutdown as failed.
                    let error =
                        unavailable("host pressure sampler did not finish within three seconds");
                    receipt(json!({"event": "pressure_stopped", "error": error}));
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

fn authenticate(config: &HostConfig, credential: CallerCredential) -> Result<SessionRole> {
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
            Ok(match consumer.role {
                ConsumerRole::Workload => SessionRole::Workload,
                ConsumerRole::ControlService => SessionRole::ControlService,
            })
        }
        CallerCredential::Administrator { secret } => {
            if !digest_matches(&config.admin_credential_sha256, &secret.digest()) {
                return Err(unauthorized());
            }
            Ok(SessionRole::Administrator)
        }
    }
}

fn session(
    mut stream: UnixStream,
    uid: u32,
    config: &HostConfig,
    status: &ServiceStatus,
    stop: &AtomicBool,
) -> Result<()> {
    // Framing uses poll and per-call nonblocking I/O with an absolute deadline;
    // it is independent of Darwin's inherited listener O_NONBLOCK flag.
    let caller = peer::observe(&stream)?;
    if caller.uid != uid {
        return Err(unauthorized());
    }
    let timeout = Duration::from_millis(FRAME_DEADLINE_MS);
    let mut greeted = false;
    let mut role = None;
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
        let response: Result<Response> = (|| match frame.body {
            Request::Hello { compatibility } => {
                if greeted {
                    return Err(Error::new(
                        ErrorCode::InvalidTransition,
                        "session already negotiated",
                    ));
                }
                let capabilities = BTreeSet::new();
                compatibility.check(PROTOCOL_VERSION, &capabilities)?;
                greeted = true;
                Ok(Response::Hello(Hello {
                    protocol: PROTOCOL_VERSION,
                    authority: PeerIdentity {
                        uid,
                        pid: std::process::id(),
                    },
                    caller,
                    capabilities,
                    max_frame_bytes: MAX_FRAME_BYTES,
                    frame_deadline_ms: FRAME_DEADLINE_MS,
                    max_sessions: MAX_SESSIONS,
                }))
            }
            Request::Authenticate { credential } => {
                if !greeted {
                    return Err(unauthorized());
                }
                if role.is_some() {
                    return Err(Error::new(
                        ErrorCode::InvalidTransition,
                        "session already authenticated",
                    ));
                }
                let authenticated = authenticate(config, credential)?;
                role = Some(authenticated);
                Ok(Response::Authenticated {
                    role: authenticated,
                })
            }
            Request::Status => {
                if role.is_none() {
                    return Err(unauthorized());
                }
                Ok(Response::Status(status.clone()))
            }
            Request::Register { instance_id } => {
                validate_id(&instance_id)?;
                match role {
                    Some(SessionRole::Workload | SessionRole::ControlService) => Err(unavailable(
                        "native registration is not ready; no principal or budget was issued",
                    )),
                    _ => Err(unauthorized()),
                }
            }
        })();
        let body = response.unwrap_or_else(|error| Response::Error(error.into()));
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
    fn authenticated_peer_is_os_observed_but_registration_and_execution_stay_closed() {
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
        assert!(!status.registration_ready && !status.execution_ready);
        let expected = if cfg!(target_os = "macos") {
            NATIVE_REASON
        } else {
            UNSUPPORTED_REASON
        };
        assert_eq!(status.reason, expected);
        assert_eq!(
            client.register("owner".into()).unwrap_err().code,
            ErrorCode::ResourceControlUnavailable
        );
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
        let required = Compatibility {
            required: BTreeSet::from([Capability::DurableAdmission]),
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
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let path = fixture.paths.socket();
                let uid = fixture.paths.uid();
                let credential = fixture.credential();
                std::thread::spawn(move || {
                    let mut client = Client::connect(&path, uid, Fixture::compatibility()).unwrap();
                    client.authenticate(credential).unwrap();
                    assert_eq!(
                        client.register(format!("owner-{n}")).unwrap_err().code,
                        ErrorCode::ResourceControlUnavailable
                    );
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
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
                let (evidence, reason) = activate(storage, &config, &paths).unwrap();
                assert_eq!(reason, NATIVE_REASON);
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
