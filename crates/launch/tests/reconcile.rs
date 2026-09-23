#![cfg(target_os = "macos")]
//! DG1-C06: reconciliation of cancellation, expiry and uncertain execution
//! with the real helper. Nothing is released by timeout, owner death or root
//! reap alone; releases need scope termination, an owner's report that no
//! helper exists, or a previous boot.

mod support;
use devguard_client::credential::{take_inherited, CredentialHandoff};
use devguard_client::launch::*;
use devguard_client::protocol::{AbandonReason, CallerCredential, StopSignal};
use devguard_contract::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use support::*;

const DAEMON_DIR: &str = "DEVGUARD_DAEMON_DIR";
const DAEMON_RESTART: &str = "DEVGUARD_DAEMON_RESTART";
const OWNER_CHILD: &str = "DEVGUARD_OWNER_CHILD";
const ESCAPE_PROBE: &str = "DEVGUARD_ESCAPE_PROBE";
const STALL_PROBE: &str = "DEVGUARD_STALL_PROBE";
const EXIT_LIMIT: Duration = Duration::from_secs(30);

fn exe() -> String {
    std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn quiet(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
}

fn quantities(record: &AttemptRecord) -> Budget {
    record.reservation.as_ref().unwrap().quantities
}

/// Start the real helper for `program` and wait until it reports READY and
/// the transcript closes on a successful exec.
fn started(
    owner: &Owner,
    attempt: &str,
    permit: &Secret,
    program: &str,
    args: &[&str],
) -> KillOnDrop {
    let launched = launch(HELPER, &owner.ticket(attempt), permit, program, args, quiet);
    let child = KillOnDrop(launched.child);
    let (outcome, phases) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert_eq!(outcome, LaunchOutcome::Started, "{phases:?}");
    child
}

/// Processes currently in the process group `pgid`.
fn group_members(pgid: u32) -> Vec<u32> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-g", &pgid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

fn secret_of(owner: &Owner) -> Secret {
    match &owner.credential {
        CallerCredential::Consumer { secret, .. } => secret.clone(),
        CallerCredential::Administrator { .. } => unreachable!("owners use consumer credentials"),
    }
}

#[test]
#[ignore = "an owner process that commits a launch and exits without a helper"]
fn owner_child() {
    let Some(config) = std::env::var_os(OWNER_CHILD) else {
        return;
    };
    let config: Value = serde_json::from_str(&config.to_string_lossy()).unwrap();
    // The consumer secret arrives on a private descriptor, never in the environment.
    // SAFETY: the parent passed this descriptor to this process for the secret only.
    let secret = unsafe { take_inherited(config["secret_fd"].as_i64().unwrap() as i32) }.unwrap();
    let generation: String = config["generation"].as_str().unwrap().into();
    let owner = Owner {
        endpoint: config["endpoint"].as_str().unwrap().into(),
        uid: config["uid"].as_u64().unwrap() as u32,
        credential: CallerCredential::Consumer {
            consumer_id: "dev-cli".into(),
            generation: generation.clone(),
            secret,
        },
        generation,
        instance: config["instance"].as_str().unwrap().into(),
    };
    match config["attempt"].as_str() {
        Some(attempt) => {
            owner.grant(attempt);
        }
        None => {
            owner.session();
        }
    }
    std::process::exit(0);
}

#[test]
#[ignore = "a workload member that leaves its process group"]
fn escape_probe() {
    if std::env::var_os(ESCAPE_PROBE).is_none() {
        return;
    }
    std::thread::sleep(Duration::from_millis(1_500));
    // SAFETY: setsid moves this process into a new session and group.
    assert!(unsafe { libc::setsid() } > 0);
    std::thread::sleep(Duration::from_secs(2));
    std::process::exit(0);
}

#[test]
#[ignore = "a helper that never presents its grant"]
fn stall_probe() {
    if std::env::var_os(STALL_PROBE).is_none() {
        return;
    }
    std::thread::sleep(Duration::from_secs(60));
}

#[test]
#[ignore = "an authority served in its own process"]
fn daemon_child() {
    let Some(directory) = std::env::var_os(DAEMON_DIR) else {
        return;
    };
    let directory = Path::new(&directory);
    let authority = if std::env::var_os(DAEMON_RESTART).is_some() {
        devguard_daemon::fixture::TestAuthority::restart(directory)
    } else {
        devguard_daemon::fixture::TestAuthority::start(directory)
    }
    .unwrap();
    authority
        .wait_until_admitting(Duration::from_secs(10))
        .unwrap();
    let mut stdout = std::io::stdout();
    writeln!(stdout, "devguard-ready").unwrap();
    stdout.flush().unwrap();
    // Answer the parent's questions until it closes stdin; a crash test kills
    // this process instead.
    for line in std::io::stdin().lines() {
        if line.unwrap().trim() == "committed" {
            writeln!(
                stdout,
                "{}",
                json!({"committed": authority.committed().unwrap()})
            )
            .unwrap();
            stdout.flush().unwrap();
        }
    }
    authority.stop().unwrap();
    std::process::exit(0);
}

/// An authority in a child process, which a test can kill like a crash. Its
/// receipts (stderr) are kept in a file for inspection.
struct DaemonChild {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
}

impl DaemonChild {
    fn start(directory: &Path, restart: bool, receipts: &Path) -> Self {
        let mut command = Command::new(exe());
        command
            .args(probe_args("daemon_child"))
            .env(DAEMON_DIR, directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(std::fs::File::create(receipts).unwrap()));
        if restart {
            command.env(DAEMON_RESTART, "1");
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut daemon = Self {
            stdin: child.stdin.take(),
            child,
            lines,
        };
        daemon.expect(|line| line.contains("devguard-ready"));
        daemon
    }

    fn expect(&mut self, wanted: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let left = deadline
                .checked_duration_since(Instant::now())
                .expect("the child authority did not answer");
            let line = self
                .lines
                .recv_timeout(left)
                .expect("the child authority did not answer");
            if wanted(&line) {
                return line;
            }
        }
    }

    /// Budget still charged to unreleased attempts, as the authority accounts it.
    fn committed(&mut self) -> Budget {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "committed").unwrap();
        stdin.flush().unwrap();
        let line = self.expect(|line| line.starts_with("{\"committed\""));
        let reply: Value = serde_json::from_str(&line).unwrap();
        serde_json::from_value(reply["committed"].clone()).unwrap()
    }

    fn crash(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for DaemonChild {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = wait_exit(&mut self.child, EXIT_LIMIT);
        }
    }
}

/// The bootstrap consumer of an authority under `directory`, for a test that
/// serves it from a child process.
fn fixture_owner(directory: &Path, instance: &str) -> Owner {
    let paths = devguard_daemon::paths::AuthorityPaths::fixture(directory);
    let config = devguard_daemon::config::HostConfig::load(&paths).unwrap();
    let secret = String::from_utf8(
        devguard_daemon::paths::read_private(&paths.cli_credential(), paths.uid(), 64).unwrap(),
    )
    .unwrap();
    let generation = config.consumers["dev-cli"].generation.clone();
    Owner {
        endpoint: paths.socket(),
        uid: paths.uid(),
        credential: CallerCredential::Consumer {
            consumer_id: "dev-cli".into(),
            generation: generation.clone(),
            secret: Secret::new(secret).unwrap(),
        },
        generation,
        instance: instance.into(),
    }
}

fn receipts_of(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// No service receipt may contain a permit or caller credential.
fn assert_receipts_hold_no_secret(path: &Path, secrets: &[&str]) {
    let text = std::fs::read_to_string(path).unwrap();
    for secret in secrets {
        assert!(!text.contains(secret), "{} holds a secret", path.display());
    }
}

fn private_directory(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in("/private/tmp")
        .unwrap()
}

#[test]
fn prepared_cancellation_and_expiry_return_the_reservation_as_not_started() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-prepared");
    let mut client = owner.session();
    let admitted = client.admit(request(owner.key("cancel-prepared"))).unwrap();
    assert_eq!(admitted.phase, AttemptPhase::Prepared);
    let cancelled = client.cancel(owner.key("cancel-prepared")).unwrap();
    assert_eq!(cancelled.phase, AttemptPhase::Cancelled);
    assert!(cancelled.known_not_started());
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    drop(client);
    let expiring = owner
        .session()
        .admit(request(owner.key("expire-prepared")))
        .unwrap();
    let deadline = expiring.reservation.as_ref().unwrap().prepare_deadline_ms;
    // Until its original deadline the attempt stays Prepared and charged.
    let mut checks_before_deadline = 0;
    while boot_ms() + 300 < deadline {
        assert_eq!(
            owner.lookup("expire-prepared").phase,
            AttemptPhase::Prepared
        );
        assert_eq!(
            fixture.authority.committed().unwrap(),
            quantities(&expiring)
        );
        checks_before_deadline += 1;
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(checks_before_deadline > 5);
    let expired = wait_for(&owner, "expire-prepared", Duration::from_secs(10), |r| {
        r.phase == AttemptPhase::Expired
    });
    let observed_at = boot_ms();
    assert!(observed_at >= deadline);
    assert!(expired.known_not_started());
    assert_eq!(
        expired.reservation.as_ref().unwrap().prepare_deadline_ms,
        deadline
    );
    let late = owner
        .session()
        .begin_launch(owner.key("expire-prepared"))
        .unwrap();
    assert!(late.permit.is_none());
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record(
        "prepared-cancel-expiry",
        json!({"cancelled": cancelled, "expired": expired, "deadline_ms": deadline,
               "checks_before_deadline": checks_before_deadline,
               "expired_observed_at_ms": observed_at, "late_commit_permit": late.permit.is_some()}),
    );
}

#[test]
fn an_owner_without_a_helper_releases_its_grant_as_no_helper_created() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-no-helper");
    let mut cases = Vec::new();
    // 1. Creating the helper fails, so no helper process exists.
    let (_, permit) = owner.grant("spawn-failed");
    let (command, _report) = helper_command(
        Path::new("/nonexistent/devguard-launch"),
        &owner.ticket("spawn-failed"),
        &permit,
        Path::new("/usr/bin/true"),
        &[],
    )
    .unwrap();
    assert!(command.spawn().is_err());
    // 2. The helper cannot reach the authority, exits and is reaped.
    let (_, exited_permit) = owner.grant("helper-exited");
    let mut unreachable = owner.ticket("helper-exited");
    unreachable.endpoint = fixture.directory.path().join("absent.sock");
    let mut launched = launch(
        HELPER,
        &unreachable,
        &exited_permit,
        "/usr/bin/true",
        &[],
        quiet,
    );
    let (outcome, _) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert!(
        matches!(&outcome, LaunchOutcome::NotStarted(HelperPhase::Failed { stage, .. }) if stage == "connect"),
        "{outcome:?}"
    );
    wait_exit(&mut launched.child, EXIT_LIMIT);
    // 3. The commit response with the permit never arrived.
    owner.grant("grant-lost");
    for (attempt, reason) in [
        ("spawn-failed", AbandonReason::SpawnFailed),
        ("helper-exited", AbandonReason::HelperExited),
        ("grant-lost", AbandonReason::GrantNotReceived),
    ] {
        let released = owner
            .session()
            .abandon_launch(owner.key(attempt), reason)
            .unwrap();
        assert_eq!(released.phase, AttemptPhase::Released, "{attempt}");
        assert_eq!(
            released.release_reason,
            Some(ReleaseReason::NoHelperCreated)
        );
        assert!(released.known_not_started());
        cases.push(json!({"attempt": attempt, "reason": reason, "released": released}));
    }
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    // A late helper with a released grant's permit is fenced.
    let mut late = launch(
        HELPER,
        &owner.ticket("spawn-failed"),
        &permit,
        "/usr/bin/true",
        &[],
        quiet,
    );
    let (late_outcome, _) = late.report.wait(REPORT_LIMIT).unwrap();
    assert!(
        matches!(&late_outcome, LaunchOutcome::NotStarted(HelperPhase::Refused { code, .. })
            if *code == ErrorCode::InvalidTransition),
        "{late_outcome:?}"
    );
    wait_exit(&mut late.child, EXIT_LIMIT);
    // A prepared attempt is cancelled, not abandoned.
    let mut client = owner.session();
    client.admit(request(owner.key("still-prepared"))).unwrap();
    assert_eq!(
        client
            .abandon_launch(owner.key("still-prepared"), AbandonReason::SpawnFailed)
            .unwrap_err()
            .code,
        ErrorCode::InvalidTransition
    );
    record(
        "no-helper-created",
        json!({"cases": cases, "late_helper": format!("{late_outcome:?}")}),
    );
}

#[test]
fn abandonment_never_releases_a_claimed_grant() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-claimed-abandon");
    let (committed, permit) = owner.grant("claimed");
    let mut child = started(&owner, "claimed", &permit, "/bin/sleep", &["2"]);
    // A mistaken report cannot release a grant that a helper claimed.
    let reported = owner
        .session()
        .abandon_launch(owner.key("claimed"), AbandonReason::HelperExited)
        .unwrap();
    assert_eq!(reported.phase, AttemptPhase::RunAuthorized);
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    assert!(wait_exit(&mut child.0, EXIT_LIMIT).success());
    let released = wait_released(&owner, "claimed");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(!released.known_not_started());
    record(
        "claimed-abandonment",
        json!({"after_report": reported, "released": released}),
    );
}

#[test]
fn observing_before_reap_tracks_survivors_and_release_waits_for_them() {
    let fixture = Fixture::start();
    // Only the owner's own observation may adopt the survivor here.
    fixture.authority.pause_reconciler(true);
    let owner = fixture.owner("owner-observe");
    let (committed, permit) = owner.grant("survivor");
    let mut child = started(
        &owner,
        "survivor",
        &permit,
        "/bin/sh",
        &["-c", "/bin/sleep 3 & exit 0"],
    );
    let root = child.0.id();
    // The root exits at once, leaving its child in the group. While the
    // unreaped root still holds its PID, the owner asks for an observation.
    exited_unreaped(root);
    let observed = owner.session().observe(owner.key("survivor")).unwrap();
    assert_eq!(observed.phase, AttemptPhase::RunAuthorized);
    wait_exit(&mut child.0, EXIT_LIMIT);
    fixture.authority.pause_reconciler(false);
    let mut charged_while_alive = 0;
    while !group_members(root).is_empty() {
        let current = owner.lookup("survivor");
        assert_ne!(current.phase, AttemptPhase::Released, "{current:?}");
        assert_eq!(
            fixture.authority.committed().unwrap(),
            quantities(&committed)
        );
        charged_while_alive += 1;
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(charged_while_alive > 0);
    let released = wait_released(&owner, "survivor");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    record(
        "observe-before-reap",
        json!({"observed": observed, "checks_while_member_alive": charged_while_alive,
               "released": released}),
    );
}

#[test]
fn a_root_reaped_before_observation_leaves_its_survivor_as_tracking_loss() {
    let fixture = Fixture::start();
    fixture.authority.pause_reconciler(true);
    let owner = fixture.owner("owner-reaped-first");
    let (committed, permit) = owner.grant("reaped-first");
    let mut child = started(
        &owner,
        "reaped-first",
        &permit,
        "/bin/sh",
        &["-c", "/bin/sleep 3 & exit 0"],
    );
    let root = child.0.id();
    // Reaped before any observation: the survivor cannot be proven the scope's.
    wait_exit(&mut child.0, EXIT_LIMIT);
    assert!(!group_members(root).is_empty());
    fixture.authority.pause_reconciler(false);
    let lost = wait_for(&owner, "reaped-first", Duration::from_secs(10), |r| {
        r.phase == AttemptPhase::Suspect
    });
    assert!(lost.tracking_lost);
    let mut checks = 0;
    while !group_members(root).is_empty() {
        assert_ne!(owner.lookup("reaped-first").phase, AttemptPhase::Released);
        checks += 1;
        std::thread::sleep(Duration::from_millis(200));
    }
    // The loss is sticky after the survivor ends.
    std::thread::sleep(Duration::from_millis(2_500));
    let settled = owner.lookup("reaped-first");
    assert_eq!(settled.phase, AttemptPhase::Suspect);
    assert!(settled.tracking_lost);
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    record(
        "reaped-before-observation",
        json!({"checks_while_member_alive": checks, "outcome": "suspect_with_tracking_loss",
               "attempt": settled}),
    );
}

#[test]
fn a_known_escape_keeps_the_attempt_suspect_after_everything_exits() {
    let fixture = Fixture::start();
    fixture.authority.pause_reconciler(true);
    let owner = fixture.owner("owner-escape");
    let (committed, permit) = owner.grant("escape");
    let probe = probe_args("escape_probe").join(" ");
    let script = format!(
        "{ESCAPE_PROBE}=1 '{}' {probe} >/dev/null 2>&1 & wait",
        exe()
    );
    let mut child = started(&owner, "escape", &permit, "/bin/sh", &["-c", &script]);
    // Observe while the member is still in the group, so it becomes known.
    std::thread::sleep(Duration::from_millis(500));
    let known = owner.session().observe(owner.key("escape")).unwrap();
    assert_eq!(known.phase, AttemptPhase::RunAuthorized);
    fixture.authority.pause_reconciler(false);
    let escaped = wait_for(&owner, "escape", Duration::from_secs(10), |record| {
        record.phase == AttemptPhase::Suspect
    });
    assert!(escaped.tracking_lost);
    assert!(wait_exit(&mut child.0, EXIT_LIMIT).success());
    std::thread::sleep(Duration::from_millis(2_500));
    let settled = owner.session().observe(owner.key("escape")).unwrap();
    assert_eq!(settled.phase, AttemptPhase::Suspect);
    assert!(settled.tracking_lost);
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    record(
        "known-escape",
        json!({"known": known, "escaped": escaped, "after_exit": settled}),
    );
}

#[test]
fn termination_signals_every_member_and_release_follows_their_end() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-terminate");
    // Without a scope there is nothing to signal.
    owner.grant("unbound");
    assert_eq!(
        owner
            .session()
            .terminate(owner.key("unbound"), StopSignal::Terminate)
            .unwrap_err()
            .code,
        ErrorCode::InvalidTransition
    );
    let (_, permit) = owner.grant("terminated");
    let began = Instant::now();
    let mut child = started(
        &owner,
        "terminated",
        &permit,
        "/bin/sh",
        &["-c", "/bin/sleep 30 & /bin/sleep 30"],
    );
    std::thread::sleep(Duration::from_millis(300));
    owner.session().observe(owner.key("terminated")).unwrap();
    let termination = owner
        .session()
        .terminate(owner.key("terminated"), StopSignal::Terminate)
        .unwrap();
    // The shell and both sleeps, or a shell that became its last command.
    assert!(termination.signalled >= 2, "{termination:?}");
    assert!(termination.complete);
    // Delivery alone changes nothing.
    assert_eq!(termination.attempt.phase, AttemptPhase::RunAuthorized);
    assert!(wait_exit(&mut child.0, EXIT_LIMIT).signal().is_some());
    let released = wait_released(&owner, "terminated");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(began.elapsed() < Duration::from_secs(20));
    record(
        "terminate-scope",
        json!({"signalled": termination.signalled, "complete": termination.complete,
               "released": released}),
    );
}

#[test]
fn cancellation_after_authorization_keeps_the_reservation_until_the_scope_ends() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-cancel-running");
    let (committed, permit) = owner.grant("cancel-running");
    let mut child = started(&owner, "cancel-running", &permit, "/bin/sleep", &["2"]);
    let draining = owner.cancel("cancel-running");
    assert_eq!(draining.phase, AttemptPhase::Draining);
    let mut checks_while_alive = 0;
    while matches!(child.0.try_wait(), Ok(None)) {
        assert_eq!(owner.lookup("cancel-running").phase, AttemptPhase::Draining);
        assert_eq!(
            fixture.authority.committed().unwrap(),
            quantities(&committed)
        );
        checks_while_alive += 1;
        std::thread::sleep(Duration::from_millis(200));
    }
    // try_wait above reaped it; the run was charged until then.
    assert!(checks_while_alive > 3);
    let released = wait_released(&owner, "cancel-running");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    record(
        "cancel-after-authorization",
        json!({"draining": draining, "checks_while_alive": checks_while_alive,
               "released": released}),
    );
}

/// Start an owner process that registers `instance` (and commits `attempt`)
/// and exits. Its consumer secret travels on a private descriptor.
fn start_owner_child(fixture: &Fixture, instance: &str, attempt: Option<&str>) -> Child {
    let owner = fixture.owner(instance);
    let mut command = Command::new(exe());
    command.args(probe_args("owner_child"));
    let secret_fd = CredentialHandoff::new(&secret_of(&owner))
        .unwrap()
        .attach(&mut command);
    command.env(
        OWNER_CHILD,
        json!({"endpoint": owner.endpoint, "uid": owner.uid, "generation": owner.generation,
               "instance": instance, "attempt": attempt, "secret_fd": secret_fd})
        .to_string(),
    );
    quiet(&mut command);
    command.spawn().unwrap()
}

#[test]
fn a_dead_owners_unclaimed_grant_turns_suspect_and_keeps_its_instance() {
    let fixture = Fixture::start();
    let mut child = start_owner_child(&fixture, "orphaning-owner", Some("orphan"));
    assert!(wait_exit(&mut child, EXIT_LIMIT).success());
    let key = fixture.owner("orphaning-owner").key("orphan");
    let deadline = Instant::now() + Duration::from_secs(10);
    let orphan = loop {
        let found = fixture
            .authority
            .attempts()
            .unwrap()
            .into_iter()
            .find(|record| record.key == key)
            .expect("the orphaned grant stays charged");
        if found.phase == AttemptPhase::Suspect {
            break found;
        }
        assert!(Instant::now() < deadline, "{found:?}");
        std::thread::sleep(Duration::from_millis(100));
    };
    // Owner death is not evidence that no helper exists: nothing is released.
    assert!(orphan.scope.is_none());
    assert_eq!(orphan.release_reason, None);
    assert_eq!(fixture.authority.committed().unwrap(), quantities(&orphan));
    let instance = fixture
        .authority
        .instances()
        .unwrap()
        .into_iter()
        .find(|instance| instance.instance.instance_id == "orphaning-owner")
        .expect("an instance with charged work is kept");
    assert!(!instance.active);
    record(
        "dead-owner",
        json!({"attempt": orphan, "instance_active": instance.active}),
    );
}

#[test]
fn an_exited_owner_without_work_is_retired() {
    let fixture = Fixture::start();
    let mut child = start_owner_child(&fixture, "passing-owner", None);
    assert!(wait_exit(&mut child, EXIT_LIMIT).success());
    let registered = |fixture: &Fixture| {
        fixture
            .authority
            .instances()
            .unwrap()
            .iter()
            .any(|instance| instance.instance.instance_id == "passing-owner")
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while registered(&fixture) {
        assert!(Instant::now() < deadline, "the instance was never retired");
        std::thread::sleep(Duration::from_millis(100));
    }
    record(
        "retired-instance",
        json!({"instance": "passing-owner", "registered_after": registered(&fixture)}),
    );
}

#[test]
fn an_unresponsive_helper_is_settled_by_its_owner_and_never_by_time() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-stall");
    let (committed, permit) = owner.grant("stalled");
    let deadline = committed.reservation.as_ref().unwrap().prepare_deadline_ms;
    // A helper that never presents the grant.
    let mut command = Command::new(exe());
    command
        .args(probe_args("stall_probe"))
        .env(STALL_PROBE, "1");
    quiet(&mut command);
    let permit_fd = CredentialHandoff::new(&permit)
        .unwrap()
        .attach(&mut command);
    assert!(permit_fd > 2);
    let mut helper = KillOnDrop(command.spawn().unwrap());
    drop(command);
    // Past the original Prepared deadline and further reconciler passes, a
    // committed grant is still charged while its owner runs.
    while boot_ms() < deadline + 2 * 1_000 {
        std::thread::sleep(Duration::from_millis(100));
    }
    let waiting = owner.lookup("stalled");
    assert_eq!(waiting.phase, AttemptPhase::LaunchCommitted);
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    helper.0.kill().unwrap();
    wait_exit(&mut helper.0, EXIT_LIMIT);
    let released = owner
        .session()
        .abandon_launch(owner.key("stalled"), AbandonReason::HelperExited)
        .unwrap();
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::NoHelperCreated)
    );
    record(
        "unresponsive-helper",
        json!({"prepare_deadline_ms": deadline, "while_waiting": waiting, "released": released}),
    );
}

#[test]
fn a_daemon_crash_keeps_every_charge_and_restart_fences_old_grants() {
    let directory = private_directory("dg-r-");
    let logs = private_directory("dg-rl-");
    let first_log = logs.path().join("daemon-0.log");
    let mut daemon = DaemonChild::start(directory.path(), false, &first_log);
    let owner = fixture_owner(directory.path(), "crash-owner");
    let (running, permit) = owner.grant("running");
    let mut child = started(&owner, "running", &permit, "/bin/sleep", &["3"]);
    let (unbound, unbound_permit) = owner.grant("unbound");
    let before = [owner.lookup("running"), owner.lookup("unbound")];
    assert_eq!(
        before.iter().map(|record| record.phase).collect::<Vec<_>>(),
        [AttemptPhase::RunAuthorized, AttemptPhase::LaunchCommitted]
    );
    let committed_before = daemon.committed();
    assert_eq!(
        committed_before,
        quantities(&running)
            .checked_add(quantities(&unbound))
            .unwrap()
    );
    daemon.crash();
    let second_log = logs.path().join("daemon-1.log");
    let mut daemon = DaemonChild::start(directory.path(), true, &second_log);
    // The restart keeps every committed attempt charged, now Suspect.
    let committed_after = daemon.committed();
    assert_eq!(committed_after, committed_before);
    let after = [owner.lookup("running"), owner.lookup("unbound")];
    assert!(after
        .iter()
        .all(|record| record.phase == AttemptPhase::Suspect));
    // A late helper for the fenced grant is refused.
    let mut late = launch(
        HELPER,
        &owner.ticket("unbound"),
        &unbound_permit,
        "/usr/bin/true",
        &[],
        quiet,
    );
    let (late_outcome, _) = late.report.wait(REPORT_LIMIT).unwrap();
    assert!(
        matches!(&late_outcome, LaunchOutcome::NotStarted(HelperPhase::Refused { code, .. })
            if *code == ErrorCode::InvalidTransition),
        "{late_outcome:?}"
    );
    wait_exit(&mut late.child, EXIT_LIMIT);
    // The running scope ends, but its tracking was lost with the crash:
    // it stays charged until a reboot proves termination.
    assert!(wait_exit(&mut child.0, EXIT_LIMIT).success());
    std::thread::sleep(Duration::from_millis(2_500));
    let lost = owner.session().observe(owner.key("running")).unwrap();
    assert_eq!(lost.phase, AttemptPhase::Suspect);
    assert!(lost.tracking_lost);
    // The owner, still running as registered, settles the unclaimed grant.
    let settled = owner
        .session()
        .abandon_launch(owner.key("unbound"), AbandonReason::GrantNotReceived)
        .unwrap();
    assert_eq!(settled.release_reason, Some(ReleaseReason::NoHelperCreated));
    assert_eq!(daemon.committed(), quantities(&running));
    drop(daemon);
    let secret = secret_of(&owner);
    for log in [&first_log, &second_log] {
        assert_receipts_hold_no_secret(
            log,
            &[permit.expose(), unbound_permit.expose(), secret.expose()],
        );
    }
    let kinds: Vec<Value> = receipts_of(&second_log)
        .into_iter()
        .map(|receipt| receipt["event"].clone())
        .collect();
    assert!(kinds.contains(&json!("launch_abandoned")));
    record(
        "daemon-crash-restart",
        json!({"before": before, "after_restart": after, "committed_before": committed_before,
               "committed_after_restart": committed_after,
               "late_helper": format!("{late_outcome:?}"), "running_after_exit": lost,
               "unbound_settled": settled, "receipts_hold_secrets": false}),
    );
}

/// Journal writes that the test makes fail, as a full or broken disk would.
struct FailingWrite<'a> {
    journal: &'a rusqlite::Connection,
    name: &'static str,
}

impl<'a> FailingWrite<'a> {
    fn when_phase_becomes(
        journal: &'a rusqlite::Connection,
        name: &'static str,
        phase: &str,
    ) -> Self {
        journal
            .execute_batch(&format!(
                "CREATE TRIGGER {name} BEFORE UPDATE ON attempts \
                 WHEN json_extract(NEW.record, '$.phase') = '{phase}' \
                 BEGIN SELECT RAISE(ABORT, 'injected journal failure'); END;"
            ))
            .unwrap();
        Self { journal, name }
    }
}

impl Drop for FailingWrite<'_> {
    fn drop(&mut self) {
        let _ = self
            .journal
            .execute_batch(&format!("DROP TRIGGER IF EXISTS {}", self.name));
    }
}

#[test]
fn journal_failures_never_release_early_or_start_an_unbound_run() {
    let directory = private_directory("dg-j-");
    let logs = private_directory("dg-jl-");
    let log = logs.path().join("daemon.log");
    let mut daemon = DaemonChild::start(directory.path(), false, &log);
    let owner = fixture_owner(directory.path(), "journal-owner");
    let journal_path: PathBuf =
        devguard_daemon::paths::AuthorityPaths::fixture(directory.path()).journal();
    let journal = rusqlite::Connection::open(&journal_path).unwrap();
    journal.busy_timeout(Duration::from_millis(2_000)).unwrap();

    // 1. Binding cannot be written: the claimed helper is stopped before any
    // exec, and its grant is settled only through its scope.
    let (bind_grant, bind_permit) = owner.grant("bind-fails");
    let marker = directory.path().join("ran");
    let script = format!("touch '{}'", marker.display());
    let failing_bind =
        FailingWrite::when_phase_becomes(&journal, "devguard_test_bind", "scope_bound");
    let mut launched = launch(
        HELPER,
        &owner.ticket("bind-fails"),
        &bind_permit,
        "/bin/sh",
        &["-c", &script],
        quiet,
    );
    let (bind_outcome, _) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert_eq!(bind_outcome, LaunchOutcome::Lost { ready: false });
    exited_unreaped(launched.child.id());
    let claimed = owner.lookup("bind-fails");
    assert_eq!(claimed.phase, AttemptPhase::LaunchCommitted);
    assert_eq!(
        claimed.scope.as_ref().unwrap().root.pid,
        launched.child.id()
    );
    assert_eq!(daemon.committed(), quantities(&bind_grant));
    let status = wait_exit(&mut launched.child, EXIT_LIMIT);
    assert_eq!(status.signal(), Some(libc::SIGKILL));
    drop(failing_bind);
    let bind_released = wait_released(&owner, "bind-fails");
    assert_eq!(
        bind_released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(!marker.exists());

    // 2. Release cannot be written: the ended run stays charged and is
    // released by a later pass once writes succeed again.
    let (release_grant, release_permit) = owner.grant("release-fails");
    let failing_release =
        FailingWrite::when_phase_becomes(&journal, "devguard_test_release", "released");
    let mut child = started(
        &owner,
        "release-fails",
        &release_permit,
        "/usr/bin/true",
        &[],
    );
    assert!(wait_exit(&mut child.0, EXIT_LIMIT).success());
    std::thread::sleep(Duration::from_millis(2_500));
    let stuck = owner.lookup("release-fails");
    assert_eq!(stuck.phase, AttemptPhase::RunAuthorized);
    assert_eq!(daemon.committed(), quantities(&release_grant));
    drop(failing_release);
    let retried = wait_released(&owner, "release-fails");
    assert_eq!(retried.release_reason, Some(ReleaseReason::ScopeTerminated));
    assert_eq!(daemon.committed(), Budget::ZERO);
    drop(daemon);

    let receipts = receipts_of(&log);
    let refused = receipts
        .iter()
        .find(|receipt| {
            receipt["event"] == "helper_refused" && receipt["key"]["attempt_id"] == "bind-fails"
        })
        .expect("the refused claimed helper has a receipt");
    assert_eq!(refused["claimed"], true);
    assert!(refused["stopped"]["signalled"].as_array().is_some());
    let failures = receipts
        .iter()
        .filter(|receipt| {
            receipt["event"] == "reconcile_failed"
                && receipt["key"]["attempt_id"] == "release-fails"
        })
        .count();
    assert!(failures >= 1);
    assert_receipts_hold_no_secret(
        &log,
        &[
            bind_permit.expose(),
            release_permit.expose(),
            secret_of(&owner).expose(),
        ],
    );
    record(
        "journal-failure",
        json!({"bind_failure": {"transcript": format!("{bind_outcome:?}"),
                                "exit_signal": status.signal(), "claimed": claimed,
                                "released": bind_released, "executed": marker.exists(),
                                "receipt": refused},
               "release_failure": {"stuck": stuck, "reconcile_failures": failures,
                                   "released": retried}}),
    );
}
