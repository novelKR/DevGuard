#![cfg(target_os = "macos")]
//! DG1-C09: installing a release as the current user's LaunchAgent.
//!
//! The test binary is copied into each package as its `devguardd`, so the
//! installer really runs from the package it installs. The installed service
//! is that copy, re-executed as `daemon_child` to serve an isolated fixture
//! authority. A fake manager starts the program as launchd would; the native
//! test uses launchd itself, with a unique label and a plist under the test's
//! own directory, and always boots the job out.

mod support;
use devguard_contract::ErrorCode;
use devguard_daemon::fixture::TestAuthority;
use devguard_daemon::install::{self, Launchctl, Manifest, ServiceManager, ServiceSpec, ARTIFACTS};
use devguard_daemon::paths::AuthorityPaths;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::*;

#[test]
#[ignore = "the service program of the installer tests"]
fn daemon_child() {
    support::daemon_child();
}

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
    // Booting out a service that was never loaded succeeds, although
    // launchctl itself exits 3 for it.
    manager.bootout(&format!("{label}.absent")).unwrap();
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

#[test]
fn a_service_that_cannot_be_recorded_is_unloaded_again() {
    let base = Base::new();
    let paths = initialized(base.path());
    let packages = base.path().join("packages");
    let pkg = package(&packages, "a", "0.1.0-test-a", "2026-09-24T00:00:00Z");
    // A different release already holds the recovery copy's name, so the
    // verified service cannot be recorded.
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(paths.recovery().join("0.1.0-test-a/bin"))
            .unwrap();
    }
    fs::write(paths.recovery().join("0.1.0-test-a/MANIFEST.json"), b"{}").unwrap();
    let manager = FakeLaunchd::default();
    let options = options(&paths, base.path(), LABEL);
    let error = install::install(&paths, &pkg, &manager, &options).unwrap_err();
    assert_eq!(*manager.bootstraps.lock().unwrap(), 1, "{error:?}");
    assert!(manager.state(LABEL).unwrap().is_none());
    assert!(!options.plist.exists());
    assert!(install::read_selection(&paths).unwrap().is_none());
    record("install-unrecorded", json!({"error": error.message}));
}
