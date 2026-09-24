//! Installed-service fixtures shared by the installer and upgrade tests. The
//! test binary is copied into each package as its `devguardd`, re-executed as
//! `daemon_child` to serve an isolated fixture authority, and started by a
//! fake manager as launchd would.

#![allow(dead_code)]

use devguard_contract::{digest_bytes, ErrorCode};
use devguard_daemon::fixture::{
    serve_until_terminated, serve_without_upgrade_drain_until_terminated, TestAuthority,
};
use devguard_daemon::install::{
    self, Artifact, InstallOptions, Manifest, ServiceManager, ServiceSpec, ServiceState, ARTIFACTS,
    MANIFEST_SCHEMA,
};
use devguard_daemon::paths::AuthorityPaths;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Asks the service program to act as a release before C11.
pub const WITHOUT_UPGRADE_DRAIN: &str = "DEVGUARD_TEST_WITHOUT_UPGRADE_DRAIN";

pub const DAEMON: &str = "DEVGUARD_TEST_DAEMON";

/// The service program's role: serve the fixture authority under the base
/// named in the environment until SIGTERM. A release whose id ends in
/// `-nodrain` states no upgrade drain, as a release before C11; one whose id
/// ends in `-broken` exits at once, as a release that cannot start.
pub fn daemon_child() {
    let Some(base) = std::env::var_os(DAEMON) else {
        return;
    };
    let program = std::env::current_exe().unwrap_or_default();
    let release = program
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if release.ends_with("-broken") {
        std::process::exit(1);
    }
    let result =
        if std::env::var_os(WITHOUT_UPGRADE_DRAIN).is_some() || release.ends_with("-nodrain") {
            serve_without_upgrade_drain_until_terminated(Path::new(&base))
        } else {
            serve_until_terminated(Path::new(&base))
        };
    if let Err(error) = result {
        eprintln!("fixture daemon failed: {error:?}");
        std::process::exit(1);
    }
}

pub fn harness_args() -> Vec<String> {
    [
        "--exact",
        "daemon_child",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    .map(String::from)
    .to_vec()
}

/// A private base directory. Installed releases are read-only, so their
/// directories are made writable again before the directory is removed.
pub struct Base(pub tempfile::TempDir);

impl Base {
    pub fn new() -> Self {
        Self(
            tempfile::Builder::new()
                .prefix("dg-i-")
                .tempdir_in("/private/tmp")
                .unwrap(),
        )
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

pub fn make_writable(path: &Path) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.is_dir() && !meta.file_type().is_symlink() {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                make_writable(&entry.path());
            }
        }
    }
}

impl Drop for Base {
    fn drop(&mut self) {
        make_writable(self.0.path());
    }
}

/// Bootstrap an authority under `base`, then stop serving it.
pub fn initialized(base: &Path) -> AuthorityPaths {
    drop(TestAuthority::start(base).unwrap());
    AuthorityPaths::fixture(base)
}

pub fn copy_executable(from: &Path, to: &Path) {
    fs::copy(from, to).unwrap();
    fs::set_permissions(to, fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn manifest_for(dir: &Path, release_id: &str, created_at: &str) -> Manifest {
    let artifacts = ARTIFACTS
        .iter()
        .map(|name| {
            let bytes = fs::read(dir.join("bin").join(name)).unwrap();
            (
                name.to_string(),
                Artifact {
                    sha256: digest_bytes(&bytes),
                    bytes: bytes.len() as u64,
                },
            )
        })
        .collect();
    Manifest {
        schema: MANIFEST_SCHEMA.into(),
        release_id: release_id.into(),
        version: install::compiled().package_version,
        source: json!({"test": true}),
        build: json!({"profile": "test"}),
        artifacts,
        compatibility: install::compiled(),
        scope: "functional".into(),
        slo_qualified: false,
        created_at: created_at.into(),
    }
}

pub fn write_manifest(dir: &Path, manifest: &Manifest) {
    fs::write(
        dir.join("MANIFEST.json"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

/// A package whose `devguardd` is this test binary.
pub fn package(parent: &Path, name: &str, release_id: &str, created_at: &str) -> PathBuf {
    package_with(
        parent,
        name,
        release_id,
        created_at,
        Path::new("/usr/bin/true"),
    )
}

/// A package whose `devguardd` is this test binary and whose `devguard` is
/// `devguard`; an upgrade or a repair runs from a release's own `devguard`.
pub fn package_with(
    parent: &Path,
    name: &str,
    release_id: &str,
    created_at: &str,
    devguard: &Path,
) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(dir.join("bin")).unwrap();
    copy_executable(
        &std::env::current_exe().unwrap(),
        &dir.join("bin/devguardd"),
    );
    copy_executable(devguard, &dir.join("bin/devguard"));
    copy_executable(Path::new("/usr/bin/true"), &dir.join("bin/devguard-launch"));
    let manifest = manifest_for(&dir, release_id, created_at);
    write_manifest(&dir, &manifest);
    dir
}

pub fn options(paths: &AuthorityPaths, base: &Path, label: &str) -> InstallOptions {
    InstallOptions {
        label: label.into(),
        plist: paths.launch_agents().join(format!("{label}.plist")),
        log: paths.logs().join("devguardd.log"),
        program_args: harness_args(),
        environment: BTreeMap::from([(DAEMON.to_string(), base.to_string_lossy().into_owned())]),
        start_deadline: Duration::from_secs(20),
        throttle_seconds: 1,
    }
}

/// Starts the job's program as launchd would: its own process group, the job's
/// environment added, and a report while it runs.
#[derive(Default)]
pub struct FakeLaunchd {
    pub job: Mutex<Option<(String, Child)>>,
    pub bootstraps: Mutex<u32>,
}

impl ServiceManager for FakeLaunchd {
    fn bootstrap(&self, spec: &ServiceSpec) -> devguard_contract::Result<()> {
        let mut job = self.job.lock().unwrap();
        if job.is_some() {
            return Err(devguard_contract::Error::new(
                ErrorCode::ResourceUnavailable,
                "a job with this label is already loaded",
            ));
        }
        *self.bootstraps.lock().unwrap() += 1;
        let child = Command::new(&spec.program)
            .args(&spec.args)
            .envs(&spec.environment)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        *job = Some((spec.label.clone(), child));
        Ok(())
    }

    fn bootout(&self, label: &str) -> devguard_contract::Result<()> {
        let mut job = self.job.lock().unwrap();
        if job.as_ref().is_some_and(|(loaded, _)| loaded == label) {
            let (_, mut child) = job.take().unwrap();
            // SAFETY: signalling the child this fake started.
            unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            let _ = child.wait();
        }
        Ok(())
    }

    fn state(&self, label: &str) -> devguard_contract::Result<Option<ServiceState>> {
        let mut job = self.job.lock().unwrap();
        Ok(match job.as_mut() {
            Some((loaded, child)) if loaded == label => Some(match child.try_wait().unwrap() {
                None => ServiceState {
                    running: true,
                    pid: Some(child.id()),
                    runs: Some(1),
                    last_exit: None,
                },
                Some(status) => ServiceState {
                    running: false,
                    pid: None,
                    runs: Some(1),
                    last_exit: Some(format!("last exit code = {:?}", status.code())),
                },
            }),
            _ => None,
        })
    }
}

impl Drop for FakeLaunchd {
    fn drop(&mut self) {
        // Release the lock before booting out, which takes it again.
        let label = self
            .job
            .lock()
            .unwrap()
            .as_ref()
            .map(|(label, _)| label.clone());
        if let Some(label) = label {
            let _ = ServiceManager::bootout(self, &label);
        }
    }
}

pub fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "condition not met in {limit:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

/// Qualification runs collect raw receipts; ordinary runs write nothing.
pub fn record(name: &str, value: Value) {
    if let Some(directory) = std::env::var_os("DEVGUARD_EVIDENCE_DIR") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_string_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}

pub const LABEL: &str = "io.github.novelkr.devguard.test";
