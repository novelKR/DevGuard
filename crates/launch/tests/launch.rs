#![cfg(target_os = "macos")]
//! DG1-C05: fenced helper preparation and executable start with the real
//! `devguard-launch` helper and an isolated authority. The normal service
//! keeps launch closed until reconciliation ships in the same group.

mod support;
use devguard_client::launch::*;
use devguard_contract::*;
use serde_json::{json, Value};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::Stdio;
use std::time::Duration;
use support::*;

#[test]
#[ignore = "payload child of the launch tests"]
fn payload_probe() {
    support::payload_probe();
}

#[test]
#[ignore = "bare helper child of the launch tests"]
fn raw_helper() {
    support::raw_helper_probe();
}

fn exe() -> String {
    std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn args(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

fn inventory(path: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn quiet(command: &mut std::process::Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
}

fn quantities(record: &AttemptRecord) -> Budget {
    record.reservation.as_ref().unwrap().quantities
}

#[test]
fn an_authorized_helper_reports_ready_and_becomes_the_executable() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-normal");
    let (committed, permit) = owner.grant("normal");
    let out = fixture.directory.path().join("normal.json");
    let probe = probe_args("payload_probe");
    let began = std::time::Instant::now();
    let mut launched = launch(
        HELPER,
        &owner.ticket("normal"),
        &permit,
        &exe(),
        &args(&probe),
        |command| {
            quiet(command);
            command
                .env(PROBE_OUT, &out)
                .env(PROBE_EXIT, "7")
                .current_dir(fixture.directory.path());
        },
    );
    let helper = launched.child.id();
    let (outcome, phases) = launched.report.wait(REPORT_LIMIT).unwrap();
    let ready_ms = began.elapsed().as_millis() as u64;
    assert_eq!(outcome, LaunchOutcome::Started, "{phases:?}");
    assert_eq!(phases, [HelperPhase::Ready {}]);
    // An exited but unreaped root still holds its PID: the run stays charged.
    exited_unreaped(helper);
    let authorized = owner.lookup("normal");
    assert_eq!(authorized.phase, AttemptPhase::RunAuthorized);
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    let status = launched.child.wait().unwrap();
    assert_eq!(status.code(), Some(7));
    // The executable replaced the helper: same PID and process group, with
    // the workload nice value applied before it started.
    let seen = inventory(&out);
    assert_eq!(seen["pid"], helper);
    assert_eq!(seen["pgid"], helper);
    assert!(
        seen["nice"].as_i64().unwrap() >= 10,
        "{}",
        inventory_summary(&seen)
    );
    let scope = authorized.scope.clone().unwrap();
    assert_eq!(scope.root.pid, helper);
    let applied = authorized.applied.clone().unwrap();
    assert!(applied.confirms(authorized.plan.as_ref().unwrap(), quantities(&committed)));
    // Once reaped, the reconciler observes the scope's end and releases it.
    let released = wait_released(&owner, "normal");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(!released.known_not_started());
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record(
        "launch-lifecycle",
        json!({"helper_pid": helper, "phases": phases, "exit_status": status.code(),
               "authorized": authorized, "released": released, "ready_ms": ready_ms,
               "payload": inventory_summary(&seen)}),
    );
}

#[test]
fn the_executable_inherits_only_its_standard_descriptors_and_no_launch_secret() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-inventory");
    let (_, permit) = owner.grant("inventory");
    let out = fixture.directory.path().join("inventory.json");
    let probe = probe_args("payload_probe");
    let expected_fds = expected_payload_fds();
    let mut launched = launch(
        HELPER,
        &owner.ticket("inventory"),
        &permit,
        &exe(),
        &args(&probe),
        |command| {
            quiet(command);
            command
                .env(PROBE_OUT, &out)
                .env("DEVGUARD_FIXTURE_MARK", "kept")
                .current_dir(fixture.directory.path());
        },
    );
    let (outcome, _) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert_eq!(outcome, LaunchOutcome::Started);
    assert!(launched.child.wait().unwrap().success());
    let seen = inventory(&out);
    // Only what the owner left inheritable: no permit carrier, transcript or socket.
    assert_eq!(seen["fds"], expected_fds);
    let mut argv = vec![exe()];
    argv.extend(probe.iter().cloned());
    assert_eq!(seen["argv"], json!(argv));
    assert_eq!(
        seen["cwd"],
        json!(fixture.directory.path().canonicalize().unwrap())
    );
    let environment = seen["environment"].as_array().unwrap();
    assert!(environment.contains(&json!(["DEVGUARD_FIXTURE_MARK", "kept"])));
    let text = seen.to_string();
    let permit_in_payload = text.contains(permit.expose()) || text.contains(&permit.digest());
    assert!(!permit_in_payload);
    record(
        "payload-inventory",
        json!({"fds": seen["fds"], "owner_inheritable": expected_fds, "argv": seen["argv"],
               "cwd": seen["cwd"],
               "environment_keys": environment.iter().map(|pair| pair[0].clone()).collect::<Vec<_>>(),
               "permit_in_payload": permit_in_payload}),
    );
}

#[test]
fn one_grant_starts_at_most_one_helper_and_one_executable() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-duplicate");
    let (_, permit) = owner.grant("duplicate");
    // A replayed commit returns the stored attempt without another permit.
    let replay = owner
        .session()
        .begin_launch(owner.key("duplicate"))
        .unwrap();
    assert!(replay.permit.is_none());
    // Two helpers race with the same permit, as a faulty owner might start.
    let runs = fixture.directory.path().join("runs");
    let script = format!("echo run >> '{}'", runs.display());
    let helpers: Vec<Launched> = (0..2)
        .map(|_| {
            launch(
                HELPER,
                &owner.ticket("duplicate"),
                &permit,
                "/bin/sh",
                &["-c", &script],
                quiet,
            )
        })
        .collect();
    let mut outcomes = Vec::new();
    for mut helper in helpers {
        let (outcome, phases) = helper.report.wait(REPORT_LIMIT).unwrap();
        let status = helper.child.wait().unwrap();
        outcomes.push((outcome, phases, status.code()));
    }
    let started = outcomes
        .iter()
        .filter(|(outcome, ..)| *outcome == LaunchOutcome::Started)
        .count();
    assert_eq!(started, 1, "{outcomes:?}");
    let refused = outcomes
        .iter()
        .find(|(outcome, ..)| *outcome != LaunchOutcome::Started)
        .unwrap();
    assert!(
        matches!(&refused.0, LaunchOutcome::NotStarted(HelperPhase::Refused { code, .. })
            if *code == ErrorCode::InvalidTransition),
        "{refused:?}"
    );
    assert_eq!(refused.2, Some(NOT_AUTHORIZED_STATUS));
    assert_eq!(std::fs::read_to_string(&runs).unwrap(), "run\n");
    // A helper arriving after the run is refused as well.
    let mut late = launch(
        HELPER,
        &owner.ticket("duplicate"),
        &permit,
        "/usr/bin/true",
        &[],
        quiet,
    );
    let (late_outcome, _) = late.report.wait(REPORT_LIMIT).unwrap();
    assert!(matches!(late_outcome, LaunchOutcome::NotStarted(_)));
    late.child.wait().unwrap();
    assert_eq!(std::fs::read_to_string(&runs).unwrap(), "run\n");
    record(
        "duplicate-helpers",
        json!({"replayed_commit_permit": replay.permit.is_some(),
               "racing_helpers": outcomes.iter().map(|(outcome, phases, code)|
                   json!({"outcome": format!("{outcome:?}"), "phases": phases, "exit_status": code}))
                   .collect::<Vec<_>>(),
               "late_helper": format!("{late_outcome:?}"),
               "executions": std::fs::read_to_string(&runs).unwrap().lines().count()}),
    );
}

#[test]
fn a_helper_not_created_by_the_owner_or_without_the_grant_claims_nothing() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-refusals");
    let (committed, permit) = owner.grant("refusals");
    // A shell starts the helper as its own child, so the helper's parent is
    // not the registered owner.
    let wrapper = fixture.directory.path().join("wrapper.sh");
    std::fs::write(&wrapper, format!("#!/bin/sh\n'{HELPER}' \"$@\"\nexit $?\n")).unwrap();
    std::fs::set_permissions(
        &wrapper,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let mut other_instance = owner.ticket("refusals");
    other_instance.instance_id = "someone-else".into();
    let wrong_permit = Secret::new("f".repeat(64)).unwrap();
    let cases = [
        (
            "not-owner-child",
            wrapper.to_str().unwrap(),
            owner.ticket("refusals"),
            &permit,
        ),
        ("unregistered-instance", HELPER, other_instance, &permit),
        (
            "wrong-permit",
            HELPER,
            owner.ticket("refusals"),
            &wrong_permit,
        ),
    ];
    let mut observed = Vec::new();
    for (name, helper, ticket, grant) in cases {
        let mut launched = launch(helper, &ticket, grant, "/usr/bin/true", &[], quiet);
        let (outcome, phases) = launched.report.wait(REPORT_LIMIT).unwrap();
        let status = launched.child.wait().unwrap();
        assert!(
            matches!(&outcome, LaunchOutcome::NotStarted(HelperPhase::Refused { code, .. })
                if *code == ErrorCode::Unauthorized),
            "{name}: {outcome:?}"
        );
        assert_eq!(status.code(), Some(NOT_AUTHORIZED_STATUS), "{name}");
        let unclaimed = owner.lookup("refusals");
        assert_eq!(unclaimed.phase, AttemptPhase::LaunchCommitted, "{name}");
        assert!(unclaimed.scope.is_none(), "{name}");
        observed.push(json!({"case": name, "phases": phases, "exit_status": status.code()}));
    }
    // None of those consumed the grant: the owner's own helper still runs.
    let mut launched = launch(
        HELPER,
        &owner.ticket("refusals"),
        &permit,
        "/usr/bin/true",
        &[],
        quiet,
    );
    let (owner_outcome, _) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert_eq!(owner_outcome, LaunchOutcome::Started);
    exited_unreaped(launched.child.id());
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    assert!(launched.child.wait().unwrap().success());
    wait_released(&owner, "refusals");
    record(
        "helper-refusals",
        json!({"refusals": observed, "owner_helper_after_refusals": format!("{owner_outcome:?}")}),
    );
}

#[test]
fn cancellation_before_a_claim_fences_a_late_helper() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-cancel");
    let (committed, permit) = owner.grant("cancelled");
    assert_eq!(owner.cancel("cancelled").phase, AttemptPhase::Draining);
    let mut launched = launch(
        HELPER,
        &owner.ticket("cancelled"),
        &permit,
        "/usr/bin/true",
        &[],
        quiet,
    );
    let (outcome, phases) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert!(
        matches!(&outcome, LaunchOutcome::NotStarted(HelperPhase::Refused { code, .. })
            if *code == ErrorCode::InvalidTransition),
        "{outcome:?}"
    );
    assert_eq!(
        launched.child.wait().unwrap().code(),
        Some(NOT_AUTHORIZED_STATUS)
    );
    let fenced = owner.lookup("cancelled");
    assert_eq!(fenced.phase, AttemptPhase::Draining);
    assert!(fenced.scope.is_none());
    // Cancellation after commitment keeps the reservation until evidence.
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    record(
        "cancelled-before-claim",
        json!({"phases": phases, "attempt": fenced}),
    );
}

#[test]
fn exec_failure_after_ready_is_reported_apart_from_refusal() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-exec");
    let (_, permit) = owner.grant("missing-program");
    let mut launched = launch(
        HELPER,
        &owner.ticket("missing-program"),
        &permit,
        "/nonexistent/devguard-program",
        &[],
        quiet,
    );
    let (outcome, phases) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert_eq!(
        outcome,
        LaunchOutcome::ExecFailed {
            errno: libc::ENOENT
        }
    );
    assert_eq!(
        phases,
        [
            HelperPhase::Ready {},
            HelperPhase::ExecFailed {
                errno: libc::ENOENT
            }
        ]
    );
    // An authorized run whose executable failed is a managed execution:
    // its release is scope termination, not proof that nothing started.
    exited_unreaped(launched.child.id());
    assert_eq!(
        owner.lookup("missing-program").phase,
        AttemptPhase::RunAuthorized
    );
    assert_eq!(
        launched.child.wait().unwrap().code(),
        Some(NOT_FOUND_STATUS)
    );
    let released = wait_released(&owner, "missing-program");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(!released.known_not_started());
    record(
        "exec-failure",
        json!({"phases": phases, "released": released}),
    );
}

#[test]
fn a_replayed_authorization_never_permits_a_second_exec() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-replay");
    let (_, permit) = owner.grant("replay");
    let out = fixture.directory.path().join("replay.json");
    let mut child = start_raw_helper(&owner, "replay", &permit, &out, true, "raw_helper");
    exited_unreaped(child.id());
    assert_eq!(owner.lookup("replay").phase, AttemptPhase::RunAuthorized);
    assert!(child.wait().unwrap().success());
    let evidence = inventory(&out);
    let results = evidence["results"].as_array().unwrap();
    // The first presentation authorizes; a helper that lost that reply and
    // presents again is told not to exec.
    assert_eq!(results[0]["may_exec"], true, "{evidence}");
    assert_eq!(results[1]["may_exec"], false, "{evidence}");
    assert_eq!(results[1]["phase"], "run_authorized");
    let released = wait_released(&owner, "replay");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    record("lost-authorization-replay", evidence);
}

#[test]
fn a_helper_refused_after_its_claim_is_stopped_and_reconciled_through_its_scope() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-unclamped");
    let (committed, permit) = owner.grant("unclamped");
    let out = fixture.directory.path().join("unclamped.json");
    let mut child = start_raw_helper(&owner, "unclamped", &permit, &out, false, "raw_helper");
    exited_unreaped(child.id());
    let before_reap = owner.lookup("unclamped");
    let charged_before_reap = fixture.authority.committed().unwrap();
    let status = child.wait().unwrap();
    let evidence = inventory(&out);
    let clamped = evidence["readback"]["max_thread_priority"]
        .as_i64()
        .is_some_and(|priority| priority <= 20);
    if clamped {
        // Every child of this harness is clamped, so an unclamped helper
        // cannot be produced here; the case is recorded, not passed.
        assert!(status.success());
        record(
            "claimed-then-refused",
            json!({"status": "not_run",
                   "reason": "the environment clamps every child of this harness, so an unclamped helper cannot be produced",
                   "helper": evidence}),
        );
        return;
    }
    // The helper's readback shows no utility clamp: it is claimed, binding is
    // refused, and the authority stops it before any reply. Until it is
    // reaped its claimed scope keeps the reservation.
    assert_eq!(status.signal(), Some(libc::SIGKILL), "{evidence}");
    assert_eq!(before_reap.phase, AttemptPhase::LaunchCommitted);
    let scope = before_reap.scope.clone().unwrap();
    assert_eq!(scope.root.pid, child.id());
    assert!(before_reap.applied.is_none());
    assert_eq!(charged_before_reap, quantities(&committed));
    let released = wait_released(&owner, "unclamped");
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record(
        "claimed-then-refused",
        json!({"status": "passed", "helper": evidence, "signal": status.signal(),
               "claimed": before_reap, "released": released}),
    );
}

/// Open a pseudo-terminal pair: (master, slave).
fn pseudo_terminal() -> (OwnedFd, OwnedFd) {
    let (mut master, mut slave) = (0, 0);
    // SAFETY: openpty writes two new descriptors; name, termios and winsize are optional.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(result, 0);
    // openpty leaves both descriptors inheritable. Like any owner, keep them
    // out of the executable; the helper passes inherited descriptors through.
    for fd in [master, slave] {
        // SAFETY: fd is one of the descriptors openpty just returned.
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
            0
        );
    }
    // SAFETY: openpty returned two new descriptors owned here.
    unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) }
}

#[test]
fn a_helper_on_a_pseudo_terminal_keeps_its_session_group_and_terminal() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-terminal");
    let (_, permit) = owner.grant("terminal");
    let out = fixture.directory.path().join("terminal.json");
    let probe = probe_args("payload_probe");
    let expected_fds = expected_payload_fds();
    let (master, slave) = pseudo_terminal();
    let mut launched = launch(
        HELPER,
        &owner.ticket("terminal"),
        &permit,
        &exe(),
        &args(&probe),
        |command| {
            command
                .stdin(Stdio::from(slave.try_clone().unwrap()))
                .stdout(Stdio::from(slave.try_clone().unwrap()))
                .stderr(Stdio::from(slave.try_clone().unwrap()))
                .env(PROBE_OUT, &out);
            // SAFETY: setsid and ioctl are async-signal-safe; descriptor 0 is
            // the terminal's slave side, which becomes the controlling terminal.
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        },
    );
    drop(slave);
    // Drain the terminal so the child never blocks on its output.
    let mut terminal = std::fs::File::from(master.try_clone().unwrap());
    let drain = std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = std::io::Read::read_to_end(&mut terminal, &mut output);
        output
    });
    let (outcome, phases) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert_eq!(outcome, LaunchOutcome::Started, "{phases:?}");
    assert!(launched.child.wait().unwrap().success());
    let seen = inventory(&out);
    let pid = launched.child.id();
    // A session leader already leads its group: the helper keeps it.
    assert_eq!(seen["pid"], pid);
    assert_eq!(seen["sid"], pid);
    assert_eq!(seen["pgid"], pid);
    assert_eq!(seen["terminal"], true);
    assert_eq!(seen["fds"], expected_fds);
    assert!(master.as_raw_fd() > 2);
    drop(master);
    drain.join().unwrap();
    record(
        "pseudo-terminal",
        json!({"phases": phases, "sid": seen["sid"], "pgid": seen["pgid"],
               "terminal": seen["terminal"], "fds": seen["fds"]}),
    );
}

#[test]
fn a_reply_that_misses_its_deadline_never_leads_to_exec() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-contention");
    let (committed, permit) = owner.grant("contention");
    let marker = fixture.directory.path().join("ran");
    let script = format!("touch '{}'", marker.display());
    // A transition holds the authority lock past the helper's 250 ms deadline.
    let holder = fixture.authority.hold_authority(Duration::from_millis(800));
    let mut launched = launch(
        HELPER,
        &owner.ticket("contention"),
        &permit,
        "/bin/sh",
        &["-c", &script],
        quiet,
    );
    let (outcome, phases) = launched.report.wait(REPORT_LIMIT).unwrap();
    assert!(
        matches!(&outcome, LaunchOutcome::NotStarted(HelperPhase::Refused { code, .. })
            if *code == ErrorCode::ResourceControlUnavailable),
        "{outcome:?}"
    );
    assert_eq!(
        launched.child.wait().unwrap().code(),
        Some(NOT_AUTHORIZED_STATUS)
    );
    holder.join().unwrap();
    // The late request finds the helper gone: nothing claims the grant.
    let after = wait_for_settled_request(&owner, "contention");
    assert!(!marker.exists());
    assert_eq!(after.phase, AttemptPhase::LaunchCommitted);
    assert!(after.scope.is_none());
    assert_eq!(
        fixture.authority.committed().unwrap(),
        quantities(&committed)
    );
    // The owner reaped its helper before READY, so it settles the grant.
    let settled = owner
        .session()
        .abandon_launch(
            owner.key("contention"),
            devguard_client::protocol::AbandonReason::HelperExited,
        )
        .unwrap();
    assert_eq!(settled.release_reason, Some(ReleaseReason::NoHelperCreated));
    record(
        "deadline-missed",
        json!({"phases": phases, "executed": marker.exists(), "attempt": after,
               "settled": settled}),
    );
}

/// Give a request that waited for the authority lock time to finish.
fn wait_for_settled_request(owner: &Owner, attempt: &str) -> AttemptRecord {
    std::thread::sleep(Duration::from_millis(300));
    owner.lookup(attempt)
}

#[test]
fn concurrent_launches_from_several_threads_never_share_descriptors() {
    let fixture = Fixture::start();
    let owner = fixture.owner("owner-concurrent");
    let expected_fds = expected_payload_fds();
    let launches: Vec<Vec<Value>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let (fixture, owner, expected_fds) = (&fixture, &owner, &expected_fds);
                scope.spawn(move || {
                    let mut seen = Vec::new();
                    for round in 0..2 {
                        let attempt = format!("concurrent-{worker}-{round}");
                        let (_, permit) = owner.grant(&attempt);
                        let out = fixture.directory.path().join(format!("{attempt}.json"));
                        let probe = probe_args("payload_probe");
                        let mut launched = launch(
                            HELPER,
                            &owner.ticket(&attempt),
                            &permit,
                            &exe(),
                            &args(&probe),
                            |command| {
                                quiet(command);
                                command.env(PROBE_OUT, &out);
                            },
                        );
                        let (outcome, _) = launched.report.wait(REPORT_LIMIT).unwrap();
                        assert_eq!(outcome, LaunchOutcome::Started, "{attempt}");
                        assert!(launched.child.wait().unwrap().success());
                        let payload = inventory(&out);
                        assert_eq!(&payload["fds"], expected_fds, "{attempt}");
                        fixture.authority.reconcile(&owner.key(&attempt)).unwrap();
                        seen.push(json!({"attempt": attempt, "fds": payload["fds"]}));
                    }
                    seen
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect()
    });
    assert_eq!(launches.iter().map(Vec::len).sum::<usize>(), 8);
    record(
        "concurrent-launches",
        json!({"owner_inheritable": expected_fds, "launches": launches}),
    );
}
