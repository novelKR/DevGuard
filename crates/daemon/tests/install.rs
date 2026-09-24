#![cfg(target_os = "macos")]
//! DG1-C09: installing a release as the current user's LaunchAgent.
//!
//! The test binary is copied into each package as its `devguardd`, so the
//! installer really runs from the package it installs. The installed service
//! is that copy, re-executed as `daemon_child` to serve an isolated fixture
//! authority. A fake manager starts the program as launchd would; the native
//! test uses launchd itself, with a unique label and a plist under the test's
//! own directory, and always boots the job out.

use devguard_contract::{digest_bytes, ErrorCode};
use devguard_daemon::fixture::{serve_until_terminated, TestAuthority};
use devguard_daemon::install::{
    self, Artifact, InstallOptions, Launchctl, Manifest, ServiceManager, ServiceSpec, ServiceState,
    ARTIFACTS, MANIFEST_SCHEMA,
};
use devguard_daemon::paths::AuthorityPaths;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DAEMON: &str = "DEVGUARD_TEST_DAEMON";

#[test]
#[ignore = "the service program of the installer tests"]
fn daemon_child() {
    let Some(base) = std::env::var_os(DAEMON) else {
        return;
    };
    if let Err(error) = serve_until_terminated(Path::new(&base)) {
        eprintln!("fixture daemon failed: {error:?}");
        std::process::exit(1);
    }
}

fn harness_args() -> Vec<String> {
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
struct Base(tempfile::TempDir);

impl Base {
    fn new() -> Self {
        Self(
            tempfile::Builder::new()
                .prefix("dg-i-")
                .tempdir_in("/private/tmp")
                .unwrap(),
        )
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

fn make_writable(path: &Path) {
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
fn initialized(base: &Path) -> AuthorityPaths {
    drop(TestAuthority::start(base).unwrap());
    AuthorityPaths::fixture(base)
}

fn copy_executable(from: &Path, to: &Path) {
    fs::copy(from, to).unwrap();
    fs::set_permissions(to, fs::Permissions::from_mode(0o755)).unwrap();
}

fn manifest_for(dir: &Path, release_id: &str, created_at: &str) -> Manifest {
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

fn write_manifest(dir: &Path, manifest: &Manifest) {
    fs::write(
        dir.join("MANIFEST.json"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

/// A package whose `devguardd` is this test binary.
fn package(parent: &Path, name: &str, release_id: &str, created_at: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(dir.join("bin")).unwrap();
    copy_executable(
        &std::env::current_exe().unwrap(),
        &dir.join("bin/devguardd"),
    );
    copy_executable(Path::new("/usr/bin/true"), &dir.join("bin/devguard"));
    copy_executable(Path::new("/usr/bin/true"), &dir.join("bin/devguard-launch"));
    let manifest = manifest_for(&dir, release_id, created_at);
    write_manifest(&dir, &manifest);
    dir
}

fn options(paths: &AuthorityPaths, base: &Path, label: &str) -> InstallOptions {
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
struct FakeLaunchd {
    job: Mutex<Option<(String, Child)>>,
    bootstraps: Mutex<u32>,
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

fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "condition not met in {limit:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

/// Qualification runs collect raw receipts; ordinary runs write nothing.
fn record(name: &str, value: Value) {
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

const LABEL: &str = "io.github.novelkr.devguard.test";

#[test]
fn an_installed_release_is_verified_running_before_it_is_selected() {
    let base = Base::new();
    let paths = initialized(base.path());
    let packages = base.path().join("packages");
    let pkg = package(&packages, "a", "0.1.0-test-a", "2026-09-24T00:00:00Z");
    let manager = FakeLaunchd::default();
    let options = options(&paths, base.path(), LABEL);
    let report = install::install(&paths, &pkg, &manager, &options).unwrap();
    let release = paths.releases().join("0.1.0-test-a");
    assert_eq!(report.release, release);
    assert!(!report.reused_release);
    // The service launchd runs is the release's own devguardd, serving the endpoint.
    assert_eq!(report.running.service_pid, report.running.authority_pid);
    assert_eq!(
        fs::canonicalize(&report.running.executable).unwrap(),
        fs::canonicalize(release.join("bin/devguardd")).unwrap()
    );
    assert_eq!(report.selection.current, "0.1.0-test-a");
    assert_eq!(
        report.selection.last_known_good.as_deref(),
        Some("0.1.0-test-a")
    );
    // The release and its recovery copy are immutable.
    for copy in [release.clone(), paths.recovery().join("0.1.0-test-a")] {
        assert_eq!(mode(&copy), 0o500);
        assert_eq!(mode(&copy.join("bin")), 0o500);
        assert_eq!(mode(&copy.join("MANIFEST.json")), 0o400);
        for name in ARTIFACTS {
            assert_eq!(mode(&copy.join("bin").join(name)), 0o500);
        }
    }
    let plist = fs::read_to_string(&options.plist).unwrap();
    assert!(plist.contains(&release.join("bin/devguardd").to_string_lossy().into_owned()));
    assert_eq!(mode(&options.plist), 0o644);
    assert_eq!(mode(&paths.selection()), 0o600);
    let status = install::status(&paths, &manager, &options).unwrap();
    assert!(status.healthy, "{status:?}");
    assert_eq!(status.releases, ["0.1.0-test-a"]);
    // While it runs, a second installation is refused.
    let again = install::install(&paths, &pkg, &manager, &options).unwrap_err();
    assert_eq!(again.code, ErrorCode::ResourceUnavailable, "{again:?}");
    assert_eq!(*manager.bootstraps.lock().unwrap(), 1);
    record(
        "install-verified",
        json!({"report": report, "status": status, "second_install": again.message}),
    );
}

#[test]
fn an_identical_release_is_reused_and_a_different_one_never_overwrites_it() {
    let base = Base::new();
    let paths = initialized(base.path());
    let packages = base.path().join("packages");
    let first = package(&packages, "a", "0.1.0-test-a", "2026-09-24T00:00:00Z");
    let manager = FakeLaunchd::default();
    let options = options(&paths, base.path(), LABEL);
    install::install(&paths, &first, &manager, &options).unwrap();
    let uninstall = |manager: &FakeLaunchd| {
        manager.bootout(LABEL).unwrap();
        fs::remove_file(&options.plist).unwrap();
    };
    uninstall(&manager);
    let reused = install::install(&paths, &first, &manager, &options).unwrap();
    assert!(reused.reused_release);
    uninstall(&manager);
    // The same release id with different contents is refused, and the
    // installed release is unchanged.
    let installed = fs::read(paths.releases().join("0.1.0-test-a/MANIFEST.json")).unwrap();
    let other = package(&packages, "b", "0.1.0-test-a", "2026-09-25T00:00:00Z");
    let refused = install::install(&paths, &other, &manager, &options).unwrap_err();
    assert_eq!(refused.code, ErrorCode::InvalidRequest, "{refused:?}");
    assert!(refused.message.contains("never overwritten"), "{refused:?}");
    assert_eq!(
        fs::read(paths.releases().join("0.1.0-test-a/MANIFEST.json")).unwrap(),
        installed
    );
    assert!(!options.plist.exists());
    assert!(manager.state(LABEL).unwrap().is_none());
    record(
        "install-reuse",
        json!({"reused": reused.reused_release, "different_release": refused.message}),
    );
}

#[test]
fn packages_that_do_not_match_their_manifest_are_refused_before_anything_is_written() {
    let base = Base::new();
    let paths = initialized(base.path());
    let packages = base.path().join("packages");
    let manager = FakeLaunchd::default();
    let options = options(&paths, base.path(), LABEL);
    let mut cases = Vec::new();
    /// (case, how the package is changed, the refusal)
    type Tamper = (&'static str, Box<dyn Fn(&Path)>, ErrorCode);
    let tamper: Vec<Tamper> = vec![
        (
            "modified-binary",
            Box::new(|dir: &Path| {
                let path = dir.join("bin/devguard");
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(0);
                fs::write(path, bytes).unwrap();
            }),
            ErrorCode::InvalidRequest,
        ),
        (
            "extra-file",
            Box::new(|dir: &Path| fs::write(dir.join("bin/extra"), b"x").unwrap()),
            ErrorCode::InvalidRequest,
        ),
        (
            "missing-binary",
            Box::new(|dir: &Path| fs::remove_file(dir.join("bin/devguard-launch")).unwrap()),
            ErrorCode::InvalidRequest,
        ),
        (
            "symlinked-binary",
            Box::new(|dir: &Path| {
                fs::remove_file(dir.join("bin/devguard")).unwrap();
                std::os::unix::fs::symlink("/usr/bin/true", dir.join("bin/devguard")).unwrap();
            }),
            ErrorCode::InvalidRequest,
        ),
        (
            "other-capabilities",
            Box::new(|dir: &Path| {
                let mut manifest: Manifest =
                    serde_json::from_slice(&fs::read(dir.join("MANIFEST.json")).unwrap()).unwrap();
                manifest.compatibility.capabilities.clear();
                write_manifest(dir, &manifest);
            }),
            ErrorCode::ResourcePolicyUnsupported,
        ),
        (
            "inconsistent-scope",
            Box::new(|dir: &Path| {
                let mut manifest: Manifest =
                    serde_json::from_slice(&fs::read(dir.join("MANIFEST.json")).unwrap()).unwrap();
                manifest.slo_qualified = true;
                write_manifest(dir, &manifest);
            }),
            ErrorCode::InvalidRequest,
        ),
        (
            "installer-not-in-package",
            Box::new(|dir: &Path| {
                copy_executable(Path::new("/usr/bin/true"), &dir.join("bin/devguardd"));
                let manifest = manifest_for(dir, "0.1.0-test-x", "2026-09-24T00:00:00Z");
                write_manifest(dir, &manifest);
            }),
            ErrorCode::InvalidRequest,
        ),
    ];
    for (name, change, code) in tamper {
        let pkg = package(&packages, name, "0.1.0-test-x", "2026-09-24T00:00:00Z");
        change(&pkg);
        let error = install::install(&paths, &pkg, &manager, &options).unwrap_err();
        assert_eq!(error.code, code, "{name}: {error:?}");
        assert!(!paths.releases().exists(), "{name}: nothing is installed");
        assert!(!options.plist.exists(), "{name}");
        cases.push(json!({"case": name, "code": error.code, "message": error.message}));
    }
    assert_eq!(*manager.bootstraps.lock().unwrap(), 0);
    record("install-refusals", json!({"cases": cases}));
}

#[test]
fn installation_is_refused_while_an_authority_serves_or_without_its_state() {
    let base = Base::new();
    let authority = TestAuthority::start(base.path()).unwrap();
    let paths = AuthorityPaths::fixture(base.path());
    let pkg = package(
        &base.path().join("packages"),
        "a",
        "0.1.0-test-a",
        "2026-09-24T00:00:00Z",
    );
    let manager = FakeLaunchd::default();
    let options = options(&paths, base.path(), LABEL);
    let serving = install::install(&paths, &pkg, &manager, &options).unwrap_err();
    assert_eq!(serving.code, ErrorCode::ResourceUnavailable, "{serving:?}");
    assert!(!options.plist.exists());
    drop(authority);
    // Installation never creates the authority state it needs.
    let empty = Base::new();
    let missing = AuthorityPaths::fixture(empty.path());
    let pkg = package(
        &empty.path().join("packages"),
        "a",
        "0.1.0-test-a",
        "2026-09-24T00:00:00Z",
    );
    let unbootstrapped = install::install(&missing, &pkg, &manager, &options).unwrap_err();
    assert!(!missing.releases().exists());
    assert!(!missing.journal().exists());
    assert_eq!(*manager.bootstraps.lock().unwrap(), 0);
    record(
        "install-preconditions",
        json!({"authority_serving": serving.message, "missing_state": unbootstrapped.message}),
    );
}

#[test]
fn a_service_that_never_proves_itself_is_unloaded_and_nothing_is_selected() {
    let base = Base::new();
    let paths = initialized(base.path());
    let pkg = package(
        &base.path().join("packages"),
        "a",
        "0.1.0-test-a",
        "2026-09-24T00:00:00Z",
    );
    let manager = FakeLaunchd::default();
    let mut options = options(&paths, base.path(), LABEL);
    // The program exits at once without serving anything.
    options.program_args = vec!["--exact".into(), "no_such_test".into()];
    options.start_deadline = Duration::from_secs(2);
    let error = install::install(&paths, &pkg, &manager, &options).unwrap_err();
    assert!(
        manager.state(LABEL).unwrap().is_none(),
        "the unverified job is unloaded"
    );
    assert!(!options.plist.exists());
    assert!(
        install::read_selection(&paths).unwrap().is_none(),
        "nothing is selected"
    );
    assert!(!paths.recovery().join("0.1.0-test-a").exists());
    let status = install::status(&paths, &manager, &options).unwrap();
    assert!(!status.healthy);
    record(
        "install-unverified",
        json!({"error": error.message, "status": status}),
    );
}

#[test]
fn concurrent_installers_leave_exactly_one_service() {
    let base = Base::new();
    let paths = initialized(base.path());
    let pkg = package(
        &base.path().join("packages"),
        "a",
        "0.1.0-test-a",
        "2026-09-24T00:00:00Z",
    );
    let manager = Arc::new(FakeLaunchd::default());
    let options = options(&paths, base.path(), LABEL);
    let outcomes: Vec<_> = (0..2)
        .map(|_| {
            let (paths, pkg, manager, options) =
                (paths.clone(), pkg.clone(), manager.clone(), options.clone());
            std::thread::spawn(move || install::install(&paths, &pkg, &*manager, &options))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    let succeeded = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    assert_eq!(succeeded, 1, "{outcomes:?}");
    assert_eq!(*manager.bootstraps.lock().unwrap(), 1);
    let entries: Vec<String> = fs::read_dir(paths.releases())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        entries.iter().all(|name| !name.starts_with(".incoming")),
        "{entries:?}"
    );
    assert!(
        install::status(&paths, &*manager, &options)
            .unwrap()
            .healthy
    );
    let refused = outcomes
        .iter()
        .find_map(|outcome| outcome.as_ref().err())
        .unwrap();
    record(
        "install-concurrent",
        json!({"succeeded": succeeded, "refused": refused.message, "releases": entries}),
    );
}

/// Boots the transient job out even when the test fails.
struct Bootout {
    manager: Launchctl,
    label: String,
}

impl Drop for Bootout {
    fn drop(&mut self) {
        let _ = self.manager.bootout(&self.label);
    }
}

fn gui_domain_available(uid: u32) -> bool {
    Command::new("/bin/launchctl")
        .args(["print", &format!("gui/{uid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn launchd_runs_the_release_restarts_it_after_a_crash_and_leaves_a_failed_start_down() {
    let base = Base::new();
    let paths = initialized(base.path());
    if !gui_domain_available(paths.uid()) {
        record(
            "launchd-lifecycle",
            json!({"status": "not_run", "reason": "this session has no launchd gui domain"}),
        );
        return;
    }
    let label = format!("{LABEL}.{}", std::process::id());
    let manager = Launchctl::new(paths.uid());
    let _bootout = Bootout {
        manager: Launchctl::new(paths.uid()),
        label: label.clone(),
    };
    let options = options(&paths, base.path(), &label);
    let pkg = package(
        &base.path().join("packages"),
        "a",
        "0.1.0-test-a",
        "2026-09-24T00:00:00Z",
    );
    let report = install::install(&paths, &pkg, &manager, &options).unwrap();
    let first = report.running.service_pid;
    let loaded = manager.state(&label).unwrap().unwrap();
    assert_eq!(loaded.pid, Some(first));
    // A crash, as an abort after a panic, is restarted after the throttle
    // interval, and the new process reopens and reconciles the same journal.
    // (A SIGSEGV sent with kill only resets the handler Rust installs for stack
    // overflows, so the process would survive it.)
    // SAFETY: crashing the service this test installed.
    assert_eq!(unsafe { libc::kill(first as i32, libc::SIGABRT) }, 0);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !install::status(&paths, &manager, &options).is_ok_and(|status| {
        status.healthy
            && status
                .running
                .is_some_and(|running| running.service_pid != first)
    }) {
        assert!(
            Instant::now() < deadline,
            "no healthy restart: {:?}\n{}",
            manager.state(&label),
            fs::read_to_string(&options.log).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let restarted = manager.state(&label).unwrap().unwrap();
    assert!(restarted.runs >= Some(2), "{restarted:?}");
    assert!(
        restarted
            .last_exit
            .as_deref()
            .is_some_and(|exit| exit.contains("Abort trap")),
        "{restarted:?}"
    );
    // Stopping through launchd sends SIGTERM: a clean exit that removes the endpoint.
    manager.bootout(&label).unwrap();
    wait_for(Duration::from_secs(10), || {
        manager.state(&label).unwrap().is_none() && !paths.socket().exists()
    });
    // With its journal made unreadable the service fails closed, and launchd
    // does not restart a failed exit.
    let journal = fs::read(paths.journal()).unwrap();
    fs::write(paths.journal(), vec![0x55; journal.len()]).unwrap();
    let spec = ServiceSpec {
        label: label.clone(),
        plist: options.plist.clone(),
        program: report.release.join("bin/devguardd"),
        args: options.program_args.clone(),
        environment: options.environment.clone(),
    };
    manager.bootstrap(&spec).unwrap();
    wait_for(Duration::from_secs(15), || {
        manager.state(&label).unwrap().is_some_and(|state| {
            !state.running && state.last_exit.as_deref() == Some("last exit code = 1")
        })
    });
    std::thread::sleep(Duration::from_secs(3));
    let failed = manager.state(&label).unwrap().unwrap();
    assert!(!failed.running, "{failed:?}");
    assert_eq!(
        failed.runs,
        Some(1),
        "a failed start is not restarted: {failed:?}"
    );
    assert!(!paths.socket().exists());
    manager.bootout(&label).unwrap();
    record(
        "launchd-lifecycle",
        json!({"label": label, "installed": report, "after_crash": restarted, "failed_start": failed}),
    );
}
