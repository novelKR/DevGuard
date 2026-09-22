use crate::{config::HostConfig, paths::AuthorityPaths};
use devguard_client::{connect::connect_timeout, framing, peer, protocol::*};
use devguard_contract::{validate_id, Error, ErrorCode, Result, PROTOCOL_VERSION};
use devguard_core::{AuthorityStorage, ConsumerRole};
use std::collections::BTreeSet;
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Server {
    listener: UnixListener,
    socket: PathBuf,
    socket_identity: (u64, u64),
    config: Arc<HostConfig>,
    uid: u32,
    status: ServiceStatus,
    _storage: AuthorityStorage,
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
            reason:
                "native process identity, host probes and launch/reconciliation are not installed"
                    .into(),
            configuration_fingerprint: config.fingerprint()?,
        };
        Ok(Self {
            listener,
            socket,
            socket_identity: (meta.dev(), meta.ino()),
            config: Arc::new(config),
            uid: paths.uid(),
            status,
            _storage: storage,
        })
    }

    pub fn run(self, stop: Arc<AtomicBool>) -> Result<()> {
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        let mut result = Ok(());
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
}
