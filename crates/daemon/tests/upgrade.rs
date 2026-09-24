#![cfg(target_os = "macos")]
//! DG1-C11: replacing and repairing the installed service without losing or
//! duplicating charged work.
//!
//! As in the installer tests, the test binary is copied into each package as
//! its `devguardd` and `devguard`, so an upgrade really runs from the release
//! it installs, and each service is that copy serving an isolated fixture
//! authority, started by a fake manager as launchd would. A release
//! whose id ends in `-nodrain` acts as one before C11; one that ends in
//! `-broken` cannot start.

mod support;
use devguard_client::protocol::{AbandonReason, CallerCredential};
use devguard_client::Client;
use devguard_contract::{
    AdmissionRequest, AttemptKey, AttemptPhase, AttemptRecord, Budget, Capability, Compatibility,
    ErrorCode, ReleaseReason, ResourceIntent, ResourceLevels, Secret,
};
use devguard_core::AuthorityStorage;
use devguard_daemon::config::HostConfig;
use devguard_daemon::install::{self, InstallOptions, Manifest, ServiceManager};
use devguard_daemon::paths::{read_private, AuthorityPaths};
use devguard_daemon::upgrade::{self, DrainMode, UpgradeOptions};
use serde_json::json;
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use support::*;

#[test]
#[ignore = "the service program of the upgrade tests"]
fn daemon_child() {
    support::daemon_child();
}

const MIB: u64 = 1024 * 1024;

/// An installed fixture service. The manager is dropped first, stopping the
/// service before the base directory is removed.
struct Installed {
    manager: FakeLaunchd,
    options: InstallOptions,
    paths: AuthorityPaths,
    packages: PathBuf,
    _base: Base,
}

impl Installed {
    fn new(release_id: &str) -> Self {
        let base = Base::new();
        let paths = initialized(base.path());
        let packages = base.path().join("packages");
        let package = package_with(
            &packages,
            "installed",
            release_id,
            "2026-09-25T00:00:00Z",
            &std::env::current_exe().unwrap(),
        );
        let manager = FakeLaunchd::default();
        let mut options = options(&paths, base.path(), LABEL);
        options.start_deadline = Duration::from_secs(10);
        install::install(&paths, &package, &manager, &options).unwrap();
        Self {
            manager,
            options,
            paths,
            packages,
            _base: base,
        }
    }

    /// Stage a release from a new package of this test binary.
    fn stage(&self, name: &str, release_id: &str) {
        let package = package_with(
            &self.packages,
            name,
            release_id,
            "2026-09-25T01:00:00Z",
            &std::env::current_exe().unwrap(),
        );
        let staged = upgrade::stage(&self.paths, &package).unwrap();
        assert_eq!(staged.release_id, release_id);
    }

    fn upgrade(
        &self,
        to: &str,
        drain: Duration,
        stopped: bool,
    ) -> devguard_contract::Result<upgrade::UpgradeReport> {
        upgrade::upgrade(
            &self.paths,
            to,
            &self.manager,
            &self.options,
            &UpgradeOptions {
                drain_timeout: drain,
                stopped,
            },
        )
    }

    fn pid(&self) -> u32 {
        self.manager.state(LABEL).unwrap().unwrap().pid.unwrap()
    }

    fn executable(&self) -> PathBuf {
        let pid = self.pid();
        fs::canonicalize(devguard_macos::executable_path(pid).unwrap().unwrap()).unwrap()
    }

    fn release(&self, id: &str) -> PathBuf {
        fs::canonicalize(self.paths.releases().join(id).join("bin/devguardd")).unwrap()
    }

    fn backups(&self) -> Vec<PathBuf> {
        fs::read_dir(self.paths.backups())
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default()
    }

    fn current(&self) -> String {
        install::read_selection(&self.paths)
            .unwrap()
            .unwrap()
            .current
    }
}

fn connect(paths: &AuthorityPaths, required: &[Capability]) -> devguard_contract::Result<Client> {
    Client::connect(
        &paths.socket(),
        paths.uid(),
        Compatibility {
            minimum_protocol: 1,
            maximum_protocol: 1,
            required: required.iter().copied().collect(),
        },
    )
}

fn consumer(paths: &AuthorityPaths) -> CallerCredential {
    let config = HostConfig::load(paths).unwrap();
    let secret = Secret::new(
        String::from_utf8(read_private(&paths.cli_credential(), paths.uid(), 64).unwrap()).unwrap(),
    )
    .unwrap();
    CallerCredential::Consumer {
        consumer_id: "dev-cli".into(),
        generation: config.consumers["dev-cli"].generation.clone(),
        secret,
    }
}

/// A new registered workload session; every frame has a short deadline.
fn workload(paths: &AuthorityPaths) -> Client {
    let mut client = connect(
        paths,
        &[Capability::DurableAdmission, Capability::FencedLaunch],
    )
    .unwrap();
    client.authenticate(consumer(paths)).unwrap();
    client.register("upgrade-owner".into()).unwrap();
    client
}

fn administrator(paths: &AuthorityPaths) -> Client {
    let secret = Secret::new(
        String::from_utf8(read_private(&paths.admin_credential(), paths.uid(), 64).unwrap())
            .unwrap(),
    )
    .unwrap();
    let mut client = connect(paths, &[Capability::UpgradeDrain]).unwrap();
    client
        .authenticate(CallerCredential::Administrator { secret })
        .unwrap();
    client
}

fn request(paths: &AuthorityPaths, id: &str) -> AdmissionRequest {
    AdmissionRequest {
        key: AttemptKey {
            consumer_id: "dev-cli".into(),
            consumer_generation: HostConfig::load(paths).unwrap().consumers["dev-cli"]
                .generation
                .clone(),
            attempt_id: id.into(),
        },
        execution_digest: devguard_contract::digest_bytes(b"upgrade"),
        intent: ResourceIntent {
            profile: "interactive".into(),
            requested: Budget {
                cpu_milli: 50,
                memory_bytes: 16 * MIB,
                tasks: 2,
            },
            minimum: ResourceLevels::MACOS,
        },
    }
}

/// Admit `id`, waiting while a new service's pressure is still closed.
fn admitted(paths: &AuthorityPaths, id: &str) -> AttemptRecord {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let key = format!("{id}-{attempt}");
        let result = workload(paths).admit(request(paths, &key));
        match result {
            Ok(record) if record.phase == AttemptPhase::Prepared => return record,
            other => assert!(Instant::now() < deadline, "{other:?}"),
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// A committed launch whose grant no helper claims: charged until its owner
/// reports that no helper exists.
fn committed(paths: &AuthorityPaths, id: &str) -> AttemptRecord {
    let record = admitted(paths, id);
    let grant = workload(paths).begin_launch(record.key.clone()).unwrap();
    assert!(grant.permit.is_some());
    grant.attempt
}

fn settle(paths: &AuthorityPaths, record: &AttemptRecord) {
    let settled = workload(paths)
        .abandon_launch(record.key.clone(), AbandonReason::GrantNotReceived)
        .unwrap();
    assert_eq!(settled.release_reason, Some(ReleaseReason::NoHelperCreated));
}

#[test]
fn an_upgrade_drains_backs_up_starts_the_release_closed_and_then_reopens_admission() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    // A tombstone and a Prepared attempt that the drain cancels.
    let finished = admitted(paths, "finished");
    workload(paths).cancel(finished.key.clone()).unwrap();
    installed.stage("b", "0.1.0-test-b");
    let prepared = admitted(paths, "prepared");
    let before = installed.pid();
    let report = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), false)
        .unwrap();
    assert_eq!(
        (report.from.as_str(), report.to.as_str()),
        ("0.1.0-test-a", "0.1.0-test-b")
    );
    assert_eq!(report.drain.mode, DrainMode::ClosedAdmission);
    assert!(report.drain.at_close.as_ref().unwrap().quiet());
    assert!(!report.compatibility.downgrade);
    // The quiescent backup holds the journal, the selection and the manifest.
    let files: Vec<&str> = report.backup.files.keys().map(String::as_str).collect();
    assert_eq!(
        files,
        ["MANIFEST.json", "authority.sqlite", "selection.json"]
    );
    let mut backed_up =
        AuthorityStorage::open(&report.backup.directory.join("authority.sqlite")).unwrap();
    let (attempts, leases) = backed_up.charged().unwrap();
    assert!(attempts.is_empty() && leases.is_empty());
    // The new release started closed and idle, and serves now.
    assert!(report.before_reopening.closure.is_some());
    assert!(report.before_reopening.quiet());
    assert_ne!(installed.pid(), before);
    assert_eq!(installed.executable(), installed.release("0.1.0-test-b"));
    // The release it replaced stays the one repair returns to.
    assert_eq!(report.selection.current, "0.1.0-test-b");
    assert_eq!(
        report.selection.last_known_good.as_deref(),
        Some("0.1.0-test-a")
    );
    assert!(report.recovery.join("bin/devguardd").is_file());
    assert!(!paths.admission_marker().exists());
    // Tombstones survive. The Prepared attempt was cancelled by the drain, or
    // expired first; either way it is known not to have started.
    assert_eq!(
        workload(paths).lookup(finished.key.clone()).unwrap().phase,
        AttemptPhase::Cancelled
    );
    let settled = workload(paths).lookup(prepared.key.clone()).unwrap();
    assert!(
        matches!(
            settled.phase,
            AttemptPhase::Cancelled | AttemptPhase::Expired
        ),
        "{settled:?}"
    );
    assert!(settled.known_not_started());
    admitted(paths, "after");
    // Old and new clients are served alike.
    let plain = connect(paths, &[]).unwrap();
    assert!(!plain.hello.capabilities.contains(&Capability::UpgradeDrain));
    assert!(connect(paths, &[Capability::UpgradeDrain, Capability::ParentLease]).is_ok());
    let status = install::status(paths, &installed.manager, &installed.options).unwrap();
    assert!(status.healthy, "{status:?}");
    record(
        "upgrade-normal",
        json!({"report": report, "status": status}),
    );
}

#[test]
fn a_drain_that_does_not_finish_in_time_keeps_the_current_release_and_its_charges() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    let running = committed(paths, "running");
    installed.stage("b", "0.1.0-test-b");
    let before = installed.pid();
    let started = Instant::now();
    let error = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(1), false)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceUnavailable, "{error:?}");
    assert!(error.message.contains("did not finish"), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    // Nothing was replaced or backed up; the charge is kept and admission reopened.
    assert_eq!(installed.pid(), before);
    assert_eq!(installed.current(), "0.1.0-test-a");
    assert!(installed.backups().is_empty());
    assert!(!paths.admission_marker().exists());
    let quiescence = administrator(paths).quiescence().unwrap();
    assert!(quiescence.closure.is_none());
    assert_eq!(quiescence.attempts.len(), 1);
    assert_eq!(quiescence.attempts[0].key, running.key);
    let during = admitted(paths, "during");
    workload(paths).cancel(during.key).unwrap();
    // Once the work is settled, the same upgrade proceeds.
    settle(paths, &running);
    let report = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), false)
        .unwrap();
    assert_eq!(report.selection.current, "0.1.0-test-b");
    record(
        "drain-timeout",
        json!({"error": error, "kept": quiescence, "retried": report}),
    );
}

#[test]
fn a_release_that_cannot_be_verified_gives_way_to_the_previous_one_on_the_same_journal() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    let finished = admitted(paths, "finished");
    workload(paths).cancel(finished.key.clone()).unwrap();
    installed.stage("c", "0.1.0-test-c-broken");
    let error = installed
        .upgrade("0.1.0-test-c-broken", Duration::from_secs(20), false)
        .unwrap_err();
    assert!(error.message.contains("serves again"), "{error:?}");
    // The previous release serves the same journal with admission open.
    assert_eq!(installed.executable(), installed.release("0.1.0-test-a"));
    assert_eq!(installed.current(), "0.1.0-test-a");
    assert!(!paths.admission_marker().exists());
    assert_eq!(
        workload(paths).lookup(finished.key.clone()).unwrap().phase,
        AttemptPhase::Cancelled
    );
    admitted(paths, "after");
    // The backup was taken before the switch; it is kept, never restored.
    assert_eq!(installed.backups().len(), 1);
    let selection = install::read_selection(paths).unwrap().unwrap();
    assert!(selection
        .history
        .iter()
        .all(|event| event.release_id != "0.1.0-test-c-broken"));
    record(
        "upgrade-unverified",
        json!({"error": error, "selection": selection}),
    );
}

#[test]
fn an_incompatible_downgrade_is_refused_before_anything_changes() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    // The current release reads a newer journal schema than the staged one.
    let future = package_with(
        &installed.packages,
        "future",
        "0.1.0-test-future",
        "2026-09-25T02:00:00Z",
        &std::env::current_exe().unwrap(),
    );
    let mut manifest: Manifest =
        serde_json::from_slice(&fs::read(future.join("MANIFEST.json")).unwrap()).unwrap();
    manifest.compatibility.journal_schema = "2".into();
    write_manifest(&future, &manifest);
    let release = paths.releases().join("0.1.0-test-future");
    fs::create_dir_all(release.join("bin")).unwrap();
    for name in ["devguardd", "devguard", "devguard-launch"] {
        copy_executable(
            &future.join("bin").join(name),
            &release.join("bin").join(name),
        );
    }
    fs::copy(future.join("MANIFEST.json"), release.join("MANIFEST.json")).unwrap();
    let mut selection = install::read_selection(paths).unwrap().unwrap();
    selection.current = "0.1.0-test-future".into();
    fs::write(paths.selection(), serde_json::to_vec(&selection).unwrap()).unwrap();
    installed.stage("b", "0.1.0-test-b");
    let before = installed.pid();
    let error = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), false)
        .unwrap_err();
    assert_eq!(
        error.code,
        ErrorCode::ResourcePolicyUnsupported,
        "{error:?}"
    );
    assert!(error.message.contains("journal schema"), "{error:?}");
    assert_eq!(installed.pid(), before);
    assert!(installed.backups().is_empty());
    assert!(!paths.admission_marker().exists());
    admitted(paths, "after");
    record("downgrade-refused", json!({"error": error}));
}

#[test]
fn a_release_that_cannot_drain_is_replaced_only_when_stopped_and_nothing_is_charged() {
    let installed = Installed::new("0.1.0-test-a-nodrain");
    let paths = &installed.paths;
    assert_eq!(
        connect(paths, &[Capability::UpgradeDrain])
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourcePolicyUnsupported
    );
    let running = committed(paths, "running");
    installed.stage("b", "0.1.0-test-b");
    let before = installed.pid();
    let refused = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), false)
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::ResourcePolicyUnsupported);
    assert!(refused.message.contains("--stopped"), "{refused:?}");
    assert_eq!(installed.pid(), before);
    // Stopped, it finds work still charged and starts the release again.
    let charged = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), true)
        .unwrap_err();
    assert_eq!(charged.code, ErrorCode::ResourceUnavailable, "{charged:?}");
    assert!(charged.message.contains("still charged"), "{charged:?}");
    assert!(charged.message.contains("serves again"), "{charged:?}");
    assert_eq!(
        installed.executable(),
        installed.release("0.1.0-test-a-nodrain")
    );
    assert_eq!(installed.current(), "0.1.0-test-a-nodrain");
    assert!(installed.backups().is_empty());
    // The restarted service kept the charge; once settled, the upgrade proceeds.
    settle(paths, &running);
    let report = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), true)
        .unwrap();
    assert_eq!(report.drain.mode, DrainMode::Stopped);
    assert!(report.compatibility.dropped.is_empty());
    assert_eq!(installed.executable(), installed.release("0.1.0-test-b"));
    assert!(connect(paths, &[Capability::UpgradeDrain]).is_ok());
    assert!(!paths.admission_marker().exists());
    admitted(paths, "after");
    record(
        "upgrade-stopped",
        json!({"refused": refused, "charged": charged, "report": report}),
    );
}

#[test]
fn repair_never_starts_a_second_authority_and_uses_the_recovery_copy_of_a_damaged_release() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    let serving = upgrade::repair(paths, &installed.manager, &installed.options).unwrap_err();
    assert_eq!(serving.code, ErrorCode::ResourceUnavailable, "{serving:?}");
    installed.manager.bootout(LABEL).unwrap();
    // Damage the installed release.
    let release = paths.releases().join("0.1.0-test-a");
    for directory in [release.clone(), release.join("bin")] {
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let launcher = release.join("bin/devguard-launch");
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&launcher)
        .unwrap()
        .write_all(b"damage")
        .unwrap();
    let report = upgrade::repair(paths, &installed.manager, &installed.options).unwrap();
    assert!(report.from_recovery);
    assert!(report.damaged.is_some());
    assert_eq!(
        installed.executable(),
        fs::canonicalize(paths.recovery().join("0.1.0-test-a/bin/devguardd")).unwrap()
    );
    assert!(report
        .selection
        .history
        .last()
        .unwrap()
        .event
        .contains("recovery copy"));
    admitted(paths, "after");
    let status = install::status(paths, &installed.manager, &installed.options).unwrap();
    assert!(status.healthy, "{status:?}");
    record(
        "repair-recovery",
        json!({"refused_while_serving": serving, "report": report, "status": status}),
    );
}

#[test]
fn repair_keeps_a_journal_that_cannot_be_opened_closed() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    installed.manager.bootout(LABEL).unwrap();
    let mut journal = fs::OpenOptions::new()
        .write(true)
        .open(paths.journal())
        .unwrap();
    journal.seek(SeekFrom::Start(0)).unwrap();
    journal.write_all(&[0xa5; 128]).unwrap();
    drop(journal);
    let error = upgrade::repair(paths, &installed.manager, &installed.options).unwrap_err();
    assert!(
        error.message.contains("journal cannot be opened"),
        "{error:?}"
    );
    // Nothing was started; the installation's own start is the only one.
    assert!(installed.manager.state(LABEL).unwrap().is_none());
    assert_eq!(*installed.manager.bootstraps.lock().unwrap(), 1);
    record("repair-corrupt-journal", json!({"error": error}));
}

#[test]
fn repair_after_an_upgrade_returns_to_the_release_it_replaced() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    installed.stage("b", "0.1.0-test-b");
    installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), false)
        .unwrap();
    let finished = admitted(paths, "finished");
    workload(paths).cancel(finished.key.clone()).unwrap();
    // The new release stops serving, as one that keeps failing would.
    installed.manager.bootout(LABEL).unwrap();
    let report = upgrade::repair(paths, &installed.manager, &installed.options).unwrap();
    assert_eq!(report.release_id, "0.1.0-test-a");
    assert!(!report.from_recovery);
    assert!(!report.compatibility.downgrade);
    assert_eq!(installed.executable(), installed.release("0.1.0-test-a"));
    assert_eq!(report.selection.current, "0.1.0-test-a");
    assert_eq!(
        report.selection.last_known_good.as_deref(),
        Some("0.1.0-test-a")
    );
    assert!(report
        .selection
        .history
        .last()
        .unwrap()
        .event
        .contains("from 0.1.0-test-b"));
    // The same journal is served: tombstones survive and admission is open.
    assert_eq!(
        workload(paths).lookup(finished.key.clone()).unwrap().phase,
        AttemptPhase::Cancelled
    );
    admitted(paths, "after");
    let status = install::status(paths, &installed.manager, &installed.options).unwrap();
    assert!(status.healthy, "{status:?}");
    record(
        "repair-previous",
        json!({"report": report, "status": status}),
    );
}

#[test]
fn an_upgrade_proceeds_from_a_release_repaired_onto_its_recovery_copy() {
    let installed = Installed::new("0.1.0-test-a");
    let paths = &installed.paths;
    installed.manager.bootout(LABEL).unwrap();
    let release = paths.releases().join("0.1.0-test-a");
    for directory in [release.clone(), release.join("bin")] {
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let launcher = release.join("bin/devguard-launch");
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&launcher, b"damaged").unwrap();
    let repaired = upgrade::repair(paths, &installed.manager, &installed.options).unwrap();
    assert!(repaired.from_recovery);
    installed.stage("b", "0.1.0-test-b");
    let report = installed
        .upgrade("0.1.0-test-b", Duration::from_secs(20), false)
        .unwrap();
    assert_eq!(installed.executable(), installed.release("0.1.0-test-b"));
    assert_eq!(report.selection.current, "0.1.0-test-b");
    admitted(paths, "after");
    record(
        "upgrade-after-recovery",
        json!({"repair": repaired, "upgrade": report}),
    );
}
