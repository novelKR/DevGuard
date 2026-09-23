//! Isolated authorities for workspace tests. Paths live under a caller-chosen
//! private directory and the host probe is synthetic and healthy, because a
//! test cannot assume the real host is free of pressure. Nothing here is
//! reachable from the `devguardd` command.

use crate::config::{self, HostConfig};
use crate::paths::{read_private, AuthorityPaths};
use crate::server::{NativeAuthority, Options, Server};
use devguard_client::protocol::CallerCredential;
use devguard_contract::{AttemptKey, AttemptRecord, Budget, Error, ErrorCode, Result, Secret};
use devguard_core::{InstanceRecord, PressureState};
use devguard_macos::{HostProbe, HostReading, VolumeReading};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const GIB: u64 = 1024 * 1024 * 1024;
/// The workload consumer created by bootstrap.
pub const CONSUMER: &str = "dev-cli";

/// Normal memory, no paging and a comfortable volume.
struct HealthyProbe;

impl HostProbe for HealthyProbe {
    fn read(&mut self) -> Result<HostReading> {
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

fn unavailable(message: &'static str) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}

/// A private authority served on a thread, with registration and launch open.
pub struct TestAuthority {
    paths: AuthorityPaths,
    config: HostConfig,
    authority: Arc<Mutex<NativeAuthority>>,
    reconcile_paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}

impl TestAuthority {
    /// Bootstrap a new authority under `base`, a private directory short
    /// enough for its socket path, and serve it.
    pub fn start(base: &Path) -> Result<Self> {
        let paths = AuthorityPaths::fixture(base);
        let config = config::initialize(&paths)?;
        Self::serve(paths, config)
    }

    /// Serve the existing authority under `base` again, as after a restart.
    pub fn restart(base: &Path) -> Result<Self> {
        let paths = AuthorityPaths::fixture(base);
        let config = HostConfig::load(&paths)?;
        Self::serve(paths, config)
    }

    fn serve(paths: AuthorityPaths, config: HostConfig) -> Result<Self> {
        let reconcile_paused = Arc::new(AtomicBool::new(false));
        let server = Server::open_with(
            &paths,
            Options {
                probe: Some(Box::new(HealthyProbe)),
                reconcile_paused: Some(reconcile_paused.clone()),
            },
        )?;
        let authority = server.native_authority().ok_or_else(|| {
            Error::new(
                ErrorCode::ResourcePolicyUnsupported,
                "a fixture authority needs native host evidence",
            )
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let worker = std::thread::Builder::new()
            .name("devguard-fixture".into())
            .spawn(move || server.run(signal))
            .map_err(|_| unavailable("cannot start the fixture service"))?;
        Ok(Self {
            paths,
            config,
            authority,
            reconcile_paused,
            stop,
            worker: Some(worker),
        })
    }

    fn authority(&self) -> Result<MutexGuard<'_, NativeAuthority>> {
        self.authority
            .lock()
            .map_err(|_| unavailable("fixture authority is unavailable"))
    }

    /// Wait until the synthetic readings open admission, which needs two.
    pub fn wait_until_admitting(&self, limit: Duration) -> Result<()> {
        let deadline = Instant::now() + limit;
        while self.authority()?.pressure() != PressureState::Normal {
            if Instant::now() >= deadline {
                return Err(unavailable("fixture pressure never became normal"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    pub fn paths(&self) -> &AuthorityPaths {
        &self.paths
    }

    pub fn socket(&self) -> PathBuf {
        self.paths.socket()
    }

    pub fn uid(&self) -> u32 {
        self.paths.uid()
    }

    /// The bootstrap workload consumer's credential.
    pub fn consumer(&self) -> Result<CallerCredential> {
        let bytes = read_private(&self.paths.cli_credential(), self.paths.uid(), 64)?;
        let secret = Secret::new(
            String::from_utf8(bytes).map_err(|_| unavailable("invalid fixture credential"))?,
        )?;
        let generation = self
            .config
            .consumers
            .get(CONSUMER)
            .ok_or_else(|| unavailable("fixture consumer is missing"))?
            .generation
            .clone();
        Ok(CallerCredential::Consumer {
            consumer_id: CONSUMER.into(),
            generation,
            secret,
        })
    }

    /// The generation of the bootstrap workload consumer.
    pub fn generation(&self) -> String {
        self.config
            .consumers
            .get(CONSUMER)
            .map(|consumer| consumer.generation.clone())
            .unwrap_or_default()
    }

    /// Budget still charged to unreleased attempts.
    pub fn committed(&self) -> Result<Budget> {
        self.authority()?.committed_budget()
    }

    /// Pause or resume the background reconciler. Owner requests such as
    /// `Observe` still reconcile while it is paused.
    pub fn pause_reconciler(&self, paused: bool) {
        self.reconcile_paused.store(paused, Ordering::Relaxed);
    }

    /// Hold the authority lock on another thread for `duration`, as a slow
    /// transition would. Returns once the lock is held.
    pub fn hold_authority(&self, duration: Duration) -> std::thread::JoinHandle<()> {
        let authority = self.authority.clone();
        let (held, ready) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let guard = authority.lock();
            let _ = held.send(());
            std::thread::sleep(duration);
            drop(guard);
        });
        let _ = ready.recv();
        holder
    }

    /// Run the trusted reconciliation path for one attempt.
    pub fn reconcile(&self, key: &AttemptKey) -> Result<AttemptRecord> {
        self.authority()?.reconcile(key)
    }

    /// Charged attempts, as the reconciler sees them.
    pub fn attempts(&self) -> Result<Vec<AttemptRecord>> {
        self.authority()?.attempts()
    }

    /// Registered instances that are not retired.
    pub fn instances(&self) -> Result<Vec<InstanceRecord>> {
        self.authority()?.instances()
    }

    fn shutdown(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| unavailable("fixture service panicked"))?,
            None => Ok(()),
        }
    }

    /// Stop serving and return the service's own result.
    pub fn stop(mut self) -> Result<()> {
        self.shutdown()
    }
}

impl Drop for TestAuthority {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
