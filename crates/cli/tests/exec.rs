#![cfg(target_os = "macos")]
//! DG1-C07: `devguard exec` as a real owner process against isolated
//! authorities, starting real workloads through the real `devguard-launch`.

mod support;
use devguard_contract::{AttemptPhase, Budget, ErrorCode, ReleaseReason};
use devguard_daemon::fixture::TestAuthority;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::time::{Duration, Instant};
use support::*;

/// A value that must reach the workload's environment but never a receipt.
const MARK: &str = "fixture-mark-7d41";

#[test]
#[ignore = "the CLI process of the exec tests"]
fn cli_child() {
    support::cli_child();
}

#[test]
#[ignore = "the workload of the exec tests"]
fn payload() {
    support::payload();
}

fn small(extra: &[&str]) -> Vec<String> {
    let mut args = vec!["--cpu", "50", "--memory", "16MiB", "--tasks", "2"];
    args.extend_from_slice(extra);
    exec_payload(&args)
}

fn phase(record: &Value) -> String {
    record["phase"].as_str().unwrap_or_default().to_string()
}

#[test]
fn a_managed_command_keeps_its_arguments_directory_environment_and_exit_status() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let out = work.join("payload.json");
    let receipt = work.join("receipt.json");
    // A small explicit budget fits the smallest supported host; the adapter
    // default is covered by the preparation tests.
    let args = small(&["--receipt", receipt.to_str().unwrap()]);
    let mut child = cli(&fixture, &args, |command| {
        quiet(command);
        command
            .current_dir(&work)
            .env(PROBE, json!({"out": out, "exit": 7}).to_string())
            .env("DEVGUARD_FIXTURE_MARK", MARK);
    });
    let cli_pid = child.id();
    let status = wait_exit(&mut child, EXIT_LIMIT);
    assert_eq!(status.code(), Some(7));
    let seen = read_json(&out);
    let mut argv = vec![exe()];
    argv.extend(probe_args("payload"));
    assert_eq!(seen["argv"], json!(argv));
    assert_eq!(seen["cwd"], json!(work));
    assert_eq!(seen["marks"]["DEVGUARD_FIXTURE_MARK"], MARK);
    // The workload's environment is the caller's, without the harness's own
    // CLI configuration: nothing added, nothing else removed.
    let mut expected: BTreeSet<String> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    expected.extend([PROBE.to_string(), "DEVGUARD_FIXTURE_MARK".to_string()]);
    expected.remove(CLI_CHILD);
    let names: BTreeSet<String> =
        serde_json::from_value(seen["environment_names"].clone()).unwrap();
    assert_eq!(names, expected);
    // Only the standard descriptors reach the workload: no credential,
    // permit or transcript descriptor of the CLI or the helper.
    assert_eq!(seen["descriptors"], json!([0, 1, 2]));
    // The workload leads its own group, a child of the CLI's helper.
    assert_eq!(seen["pgid"], seen["pid"]);
    assert_eq!(seen["ppid"], json!(cli_pid));
    let text = std::fs::read_to_string(&receipt).unwrap();
    assert_no_secret(&text, &fixture.secret());
    assert!(
        !text.contains(MARK),
        "a receipt names variables, never their values"
    );
    let receipt: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(receipt["result"], "completed");
    assert_eq!(receipt["managed"], true);
    assert_eq!(receipt["exit"], json!({"code": 7}));
    assert_eq!(receipt["adapter"]["adapter"], "generic");
    assert_eq!(receipt["budget"]["source"], "command_line");
    assert_eq!(
        receipt["budget"]["requested"],
        json!({"cpu_milli": 50, "memory_bytes": 16 * MIB, "tasks": 2})
    );
    assert_eq!(phase(&receipt["observed_after_reap"]), "released");
    assert_eq!(
        receipt["observed_after_reap"]["release_reason"],
        "scope_terminated"
    );
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record(
        "exec-lifecycle",
        json!({"payload": {"argv": seen["argv"], "cwd": seen["cwd"], "marks": seen["marks"],
                           "pgid": seen["pgid"], "pid": seen["pid"], "ppid": seen["ppid"],
                           "descriptors": seen["descriptors"],
                           "environment_names_match_caller": true},
               "cli_pid": cli_pid, "cli_status": status.code(), "receipt": receipt}),
        &fixture.secret(),
    );
}

#[test]
fn a_workload_ended_by_a_signal_ends_the_cli_by_the_same_signal() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let receipt = work.join("receipt.json");
    let args = small(&["--receipt", receipt.to_str().unwrap()]);
    let mut child = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"signal": libc::SIGTERM}).to_string());
    });
    let status = wait_exit(&mut child, EXIT_LIMIT);
    assert_eq!(status.signal(), Some(libc::SIGTERM), "{status:?}");
    let receipt = read_json(&receipt);
    assert_eq!(receipt["exit"], json!({"signal": libc::SIGTERM}));
    assert_eq!(receipt["result"], "completed");
    record(
        "signal-death",
        json!({"cli_signal": status.signal(), "receipt": receipt}),
        &fixture.secret(),
    );
}

#[test]
fn signals_sent_to_the_cli_reach_every_member_of_the_workload_group() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let mut cases = Vec::new();
    for signal in [libc::SIGTERM, libc::SIGHUP] {
        let (out, ready, release, receipt) = (
            work.join(format!("payload-{signal}.json")),
            work.join(format!("ready-{signal}")),
            work.join(format!("release-{signal}")),
            work.join(format!("receipt-{signal}.json")),
        );
        let args = small(&["--receipt", receipt.to_str().unwrap()]);
        let mut child = cli(&fixture, &args, |command| {
            quiet(command);
            command.env(
                PROBE,
                json!({"out": out, "ready": ready, "survivor_release": release, "release": release})
                    .to_string(),
            );
        });
        wait_for(EXIT_LIMIT, || ready.exists());
        let seen = read_json(&out);
        let (root, member) = (
            seen["pid"].as_u64().unwrap() as u32,
            seen["survivor"].as_u64().unwrap() as u32,
        );
        // SAFETY: signalling the CLI process this test started.
        assert_eq!(unsafe { libc::kill(child.id() as i32, signal) }, 0);
        let status = wait_exit(&mut child, EXIT_LIMIT);
        assert_eq!(status.signal(), Some(signal), "{status:?}");
        // The whole group received it: the member ends too, and is reaped by
        // the system once its parent is gone. Neither was released.
        wait_for(Duration::from_secs(10), || !alive(member));
        assert!(!alive(root));
        assert!(!release.exists());
        let receipt = read_json(&receipt);
        assert_eq!(receipt["signals"]["received"], json!([signal]));
        assert_eq!(receipt["signals"]["forwarded"], json!([signal]));
        assert_eq!(receipt["signals"]["ignored"], json!([]));
        wait_for(Duration::from_secs(10), || {
            fixture.authority.committed().unwrap() == Budget::ZERO
        });
        cases.push(json!({"signal": signal, "root": root, "member": member,
                          "cli_signal": status.signal(), "signals": receipt["signals"],
                          "released": true}));
    }
    record(
        "signal-forwarding",
        json!({"cases": cases}),
        &fixture.secret(),
    );
}

#[test]
fn signals_the_caller_ignored_stay_ignored_and_are_not_forwarded() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (out, ready, release, receipt) = (
        work.join("payload.json"),
        work.join("ready"),
        work.join("release"),
        work.join("receipt.json"),
    );
    let args = small(&["--receipt", receipt.to_str().unwrap()]);
    let mut child = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(
            PROBE,
            json!({"out": out, "ready": ready, "release": release}).to_string(),
        );
        // As under nohup: the CLI starts with SIGHUP ignored.
        // SAFETY: setting a disposition in the child before exec.
        unsafe {
            command.pre_exec(|| {
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
                Ok(())
            });
        }
    });
    wait_for(EXIT_LIMIT, || ready.exists());
    let seen = read_json(&out);
    assert_eq!(seen["ignored_signals"], json!([libc::SIGHUP]));
    // SAFETY: signalling the CLI process this test started.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGHUP) }, 0);
    std::thread::sleep(Duration::from_millis(200));
    assert!(alive(seen["pid"].as_u64().unwrap() as u32));
    support::release(&release);
    let status = wait_exit(&mut child, EXIT_LIMIT);
    assert_eq!(status.code(), Some(0), "{status:?}");
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "completed");
    assert_eq!(receipt["signals"]["ignored"], json!([libc::SIGHUP]));
    assert_eq!(receipt["signals"]["received"], json!([]));
    assert_eq!(receipt["signals"]["forwarded"], json!([]));
    record(
        "ignored-signals",
        json!({"workload_ignored": seen["ignored_signals"], "signals": receipt["signals"],
               "cli_status": status.code()}),
        &fixture.secret(),
    );
}

#[test]
fn a_stop_that_is_not_job_control_is_left_to_whoever_sent_it() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (out, receipt) = (work.join("payload.json"), work.join("receipt.json"));
    let args = small(&["--receipt", receipt.to_str().unwrap()]);
    let mut child = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out, "stop_self": true}).to_string());
    });
    wait_for(EXIT_LIMIT, || out.exists());
    let root = read_json(&out)["pid"].as_u64().unwrap() as u32;
    wait_for(EXIT_LIMIT, || stopped(root));
    // The CLI keeps waiting instead of stopping itself.
    std::thread::sleep(Duration::from_millis(300));
    assert!(!stopped(child.id()), "the CLI mirrored a SIGSTOP");
    // SAFETY: continuing the workload this test started.
    assert_eq!(unsafe { libc::kill(root as i32, libc::SIGCONT) }, 0);
    let status = wait_exit(&mut child, EXIT_LIMIT);
    assert_eq!(status.code(), Some(0), "{status:?}");
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "completed");
    assert_eq!(receipt["launch"]["stops_mirrored"], 0);
    record(
        "sigstop-not-mirrored",
        json!({"receipt_launch": receipt["launch"], "cli_status": status.code()}),
        &fixture.secret(),
    );
}

#[test]
fn the_cli_observes_before_reaping_so_a_survivor_is_tracked_until_it_ends() {
    let fixture = Fixture::start();
    // Only the CLI's own observation may adopt the survivor.
    fixture.authority.pause_reconciler(true);
    let work = fixture.work("work");
    let (out, receipt) = (work.join("payload.json"), work.join("receipt.json"));
    let args = small(&["--receipt", receipt.to_str().unwrap()]);
    let release = work.join("release");
    let mut child = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(
            PROBE,
            json!({"out": out, "survivor_release": release}).to_string(),
        );
    });
    let status = wait_exit(&mut child, EXIT_LIMIT);
    assert_eq!(status.code(), Some(0));
    let survivor = read_json(&out)["survivor"].as_u64().unwrap() as u32;
    assert!(
        alive(survivor),
        "the survivor outlives the root until released"
    );
    let receipt = read_json(&receipt);
    let before = &receipt["observed_before_reap"];
    let after = &receipt["observed_after_reap"];
    assert_eq!(before["tracking_lost"], false, "{before}");
    assert_eq!(after["tracking_lost"], false, "{after}");
    assert_ne!(phase(after), "released", "{after}");
    let committed = fixture.authority.committed().unwrap();
    assert_ne!(committed, Budget::ZERO);
    support::release(&release);
    fixture.authority.pause_reconciler(false);
    wait_for(Duration::from_secs(15), || !alive(survivor));
    let key = serde_json::from_value(receipt["attempts"][0]["key"].clone()).unwrap();
    let mut settled = fixture.authority.reconcile(&key).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while settled.phase != AttemptPhase::Released && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
        settled = fixture.authority.reconcile(&key).unwrap();
    }
    assert_eq!(settled.phase, AttemptPhase::Released);
    assert_eq!(settled.release_reason, Some(ReleaseReason::ScopeTerminated));
    assert!(!settled.tracking_lost);
    record(
        "observe-before-reap",
        json!({"before_reap": before, "after_reap": after, "charged_while_survivor_lived": committed,
               "settled": settled}),
        &fixture.secret(),
    );
}

#[test]
fn a_budget_the_host_cannot_fit_starts_nothing() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (out, receipt) = (work.join("payload.json"), work.join("receipt.json"));
    let too_much = (fixture.capacity().cpu_milli + 1_000).to_string();
    let args = exec_payload(&[
        "--cpu",
        &too_much,
        "--memory",
        "16MiB",
        "--tasks",
        "2",
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    assert!(fixture.authority.pressure_normal().unwrap());
    let started = Instant::now();
    let mut child = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out}).to_string());
    });
    let status = wait_exit(&mut child, EXIT_LIMIT);
    assert_eq!(status.code(), Some(125));
    assert!(started.elapsed() < Duration::from_secs(10));
    // The denial was for capacity: pressure stayed Normal throughout.
    assert!(fixture.authority.pressure_normal().unwrap());
    assert!(!out.exists(), "the workload must not start");
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "not_started");
    assert_eq!(receipt["wait"]["admissions"], 1);
    assert_eq!(receipt["attempts"][0]["record"]["phase"], "denied");
    assert_eq!(
        receipt["attempts"][0]["record"]["denial"],
        json!(ErrorCode::ResourceUnavailable)
    );
    assert!(receipt["launch"].is_null());
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record(
        "refused-budget",
        json!({"cli_status": status.code(), "receipt": receipt}),
        &fixture.secret(),
    );
}

/// Hold half of the CPU capacity with a workload that runs until the
/// returned release file is written.
fn hold_half(
    fixture: &Fixture,
    work: &std::path::Path,
) -> (std::process::Child, std::path::PathBuf) {
    let half = (fixture.capacity().cpu_milli / 2).to_string();
    let (ready, release) = (work.join("holder-ready"), work.join("holder-release"));
    let args = exec_payload(&["--cpu", &half, "--memory", "16MiB", "--tasks", "2"]);
    let holder = cli(fixture, &args, |command| {
        quiet(command);
        command.env(
            PROBE,
            json!({"ready": ready, "release": release}).to_string(),
        );
    });
    wait_for(EXIT_LIMIT, || ready.exists());
    (holder, release)
}

/// Wait until a second owner has registered beside the holder, then long
/// enough for its first admission, made in the same session, to be refused.
fn await_first_refusal(fixture: &Fixture) {
    wait_for(EXIT_LIMIT, || {
        fixture.authority.instances().unwrap().len() >= 2
    });
    std::thread::sleep(Duration::from_millis(400));
    assert!(fixture.authority.pressure_normal().unwrap());
}

fn more_than_half(fixture: &Fixture) -> String {
    (fixture.capacity().cpu_milli / 2 + 1).to_string()
}

#[test]
fn an_explicit_wait_admits_the_command_once_capacity_is_released() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (mut holder, holder_release) = hold_half(&fixture, &work);
    let (out, receipt) = (work.join("payload.json"), work.join("receipt.json"));
    let args = exec_payload(&[
        "--cpu",
        &more_than_half(&fixture),
        "--memory",
        "16MiB",
        "--tasks",
        "2",
        "--wait",
        "20s",
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    let started = Instant::now();
    let mut waiter = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out}).to_string());
    });
    await_first_refusal(&fixture);
    release(&holder_release);
    assert!(wait_exit(&mut holder, EXIT_LIMIT).success());
    let status = wait_exit(&mut waiter, EXIT_LIMIT);
    let waited = started.elapsed();
    assert_eq!(status.code(), Some(0));
    assert!(out.exists());
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "completed");
    let admissions = receipt["wait"]["admissions"].as_u64().unwrap();
    assert!(admissions >= 2, "{receipt}");
    let attempts = receipt["attempts"].as_array().unwrap();
    assert_eq!(attempts[0]["record"]["phase"], "denied");
    assert_eq!(receipt["wait"]["work_capacity"], json!(fixture.capacity()));
    assert!(waited >= Duration::from_millis(400), "{waited:?}");
    wait_for(Duration::from_secs(10), || {
        fixture.authority.committed().unwrap() == Budget::ZERO
    });
    record(
        "wait-admitted",
        json!({"waited_ms": waited.as_millis() as u64, "admissions": admissions,
               "attempts": attempts.iter().map(|a| json!({"phase": a["record"]["phase"], "note": a["note"]})).collect::<Vec<_>>(),
               "receipt_wait": receipt["wait"]}),
        &fixture.secret(),
    );
}

#[test]
fn an_explicit_wait_ends_at_its_deadline_without_starting_anything() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (mut holder, holder_release) = hold_half(&fixture, &work);
    let (out, receipt) = (work.join("payload.json"), work.join("receipt.json"));
    let args = exec_payload(&[
        "--cpu",
        &more_than_half(&fixture),
        "--memory",
        "16MiB",
        "--tasks",
        "2",
        "--wait",
        "1s",
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    let started = Instant::now();
    let mut waiter = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out}).to_string());
    });
    let status = wait_exit(&mut waiter, EXIT_LIMIT);
    let waited = started.elapsed();
    assert_eq!(status.code(), Some(125));
    assert!(waited >= Duration::from_secs(1), "{waited:?}");
    assert!(!out.exists());
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "not_started");
    assert_eq!(receipt["wait"]["deadline_reached"], true);
    assert!(receipt["wait"]["admissions"].as_u64().unwrap() >= 2);
    assert!(fixture.authority.pressure_normal().unwrap());
    release(&holder_release);
    assert!(wait_exit(&mut holder, EXIT_LIMIT).success());
    record(
        "wait-deadline",
        json!({"waited_ms": waited.as_millis() as u64, "cli_status": status.code(),
               "receipt_wait": receipt["wait"], "reason": receipt["reason"]}),
        &fixture.secret(),
    );
}

#[test]
fn a_wait_for_more_than_the_host_can_ever_fit_is_refused_without_an_attempt() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (out, receipt) = (work.join("payload.json"), work.join("receipt.json"));
    let capacity = fixture.capacity();
    let too_much = (capacity.cpu_milli + 1_000).to_string();
    let args = exec_payload(&[
        "--cpu",
        &too_much,
        "--memory",
        "16MiB",
        "--tasks",
        "2",
        "--wait",
        "60s",
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    let started = Instant::now();
    let mut child = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out}).to_string());
    });
    let status = wait_exit(&mut child, EXIT_LIMIT);
    let elapsed = started.elapsed();
    assert_eq!(status.code(), Some(125));
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
    assert!(!out.exists());
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "not_started");
    assert_eq!(receipt["wait"]["admissions"], 0);
    assert_eq!(receipt["wait"]["work_capacity"], json!(capacity));
    assert!(receipt["attempts"].as_array().unwrap().is_empty());
    assert!(receipt["reason"]
        .as_str()
        .unwrap()
        .contains("work capacity"));
    // Nothing was registered or charged, and no denied attempt was left.
    assert!(fixture.authority.instances().unwrap().is_empty());
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record(
        "wait-beyond-capacity",
        json!({"elapsed_ms": elapsed.as_millis() as u64, "cli_status": status.code(),
               "receipt_wait": receipt["wait"], "reason": receipt["reason"]}),
        &fixture.secret(),
    );
}

#[test]
fn a_signal_cancels_a_wait_and_ends_the_cli_by_that_signal() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (mut holder, holder_release) = hold_half(&fixture, &work);
    let (out, receipt) = (work.join("payload.json"), work.join("receipt.json"));
    let args = exec_payload(&[
        "--cpu",
        &more_than_half(&fixture),
        "--memory",
        "16MiB",
        "--tasks",
        "2",
        "--wait",
        "30s",
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    let mut waiter = cli(&fixture, &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out}).to_string());
    });
    await_first_refusal(&fixture);
    let cancelled = Instant::now();
    // SAFETY: signalling the CLI process this test started.
    assert_eq!(unsafe { libc::kill(waiter.id() as i32, libc::SIGINT) }, 0);
    let status = wait_exit(&mut waiter, EXIT_LIMIT);
    let reaction = cancelled.elapsed();
    assert_eq!(status.signal(), Some(libc::SIGINT), "{status:?}");
    assert!(reaction < Duration::from_secs(3), "{reaction:?}");
    assert!(!out.exists());
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "not_started");
    assert_eq!(receipt["wait"]["cancelled_by_signal"], libc::SIGINT);
    release(&holder_release);
    assert!(wait_exit(&mut holder, EXIT_LIMIT).success());
    // The cancelled waiter left nothing charged once the holder is gone.
    wait_for(Duration::from_secs(10), || {
        fixture.authority.committed().unwrap() == Budget::ZERO
    });
    record(
        "wait-cancelled",
        json!({"reaction_ms": reaction.as_millis() as u64, "cli_signal": status.signal(),
               "receipt_wait": receipt["wait"]}),
        &fixture.secret(),
    );
}

#[test]
fn an_unavailable_authority_starts_nothing_and_never_runs_the_command_unmanaged() {
    let directory = private_directory("dg-u-");
    let authority = TestAuthority::start(directory.path()).unwrap();
    let secret = match authority.consumer().unwrap() {
        devguard_client::protocol::CallerCredential::Consumer { secret, .. } => {
            secret.expose().to_string()
        }
        _ => unreachable!(),
    };
    // Stop serving; the configuration and credential remain.
    drop(authority);
    let out = directory.path().join("payload.json");
    let receipt = directory.path().join("receipt.json");
    let args = small(&["--receipt", receipt.to_str().unwrap()]);
    let mut child = cli_at(directory.path(), &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out}).to_string());
    });
    let status = wait_exit(&mut child, EXIT_LIMIT);
    assert_eq!(status.code(), Some(125));
    assert!(!out.exists(), "nothing may run without the authority");
    let text = std::fs::read_to_string(&receipt).unwrap();
    assert_no_secret(&text, &secret);
    let receipt: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(receipt["result"], "not_started");
    assert_eq!(receipt["managed"], true);
    assert!(receipt["launch"].is_null());
    // The doctor reports the same, and a requirement makes it fail.
    let mut doctor = cli_at(
        directory.path(),
        &["doctor".into(), "--require".into(), "admission".into()],
        |command| {
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
        },
    );
    let (status, text) = output_within(&mut doctor, EXIT_LIMIT);
    assert_eq!(status.code(), Some(1));
    let report = doctor_report(&text);
    assert_eq!(report["managed_execution"], "unavailable");
    assert_eq!(report["unmanaged_fallback"], "never");
    assert_eq!(report["requirements"]["admission"], false);
    record(
        "unavailable-authority",
        json!({"cli_status": status.code(), "reason": receipt["reason"], "doctor": report}),
        &secret,
    );
}

#[test]
fn doctor_reports_managed_execution_and_checks_requirements() {
    let fixture = Fixture::start();
    let mut doctor = cli(
        &fixture,
        &[
            "doctor".into(),
            "--require".into(),
            "admission,registration,macos-cooperative".into(),
        ],
        |command| {
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
        },
    );
    let (status, text) = output_within(&mut doctor, EXIT_LIMIT);
    assert_eq!(status.code(), Some(0), "{text}");
    assert_no_secret(&text, &fixture.secret());
    let report = doctor_report(&text);
    assert_eq!(report["managed_execution"], "available");
    assert_eq!(report["unmanaged_fallback"], "never");
    assert_eq!(
        report["requirements"],
        json!({"admission": true, "registration": true, "macos-cooperative": true})
    );
    assert_eq!(report["checks"]["helper"]["ok"], true);
    assert_eq!(report["checks"]["service"]["ok"], true);
    record("doctor", report, &fixture.secret());
}

fn project_root(base: &std::path::Path) -> std::path::PathBuf {
    let root = base.join("project");
    std::fs::create_dir_all(root.join("nested")).unwrap();
    std::fs::write(
        root.join(".devguard.toml"),
        "schema = 1\nproject_id = \"fixture-dev\"\nprofile = \"interactive\"\nadapter = \"generic\"\n\n[limits]\ncpu_milli = 200\nmemory_bytes = 67108864\ntasks = 4\n",
    )
    .unwrap();
    root.canonicalize().unwrap()
}

#[test]
fn a_registered_project_supplies_its_limits_and_must_contain_the_working_directory() {
    let directory = private_directory("dg-p-");
    let root = project_root(directory.path());
    let authority = TestAuthority::start_with(directory.path(), |config| {
        config.projects.insert(
            "fixture-dev".into(),
            devguard_daemon::config::ProjectRegistration { root: root.clone() },
        );
    })
    .unwrap();
    authority
        .wait_until_admitting(Duration::from_secs(10))
        .unwrap();
    let secret = match authority.consumer().unwrap() {
        devguard_client::protocol::CallerCredential::Consumer { secret, .. } => {
            secret.expose().to_string()
        }
        _ => unreachable!(),
    };
    let run = |extra: &[&str], cwd: &std::path::Path, name: &str| {
        let receipt = directory.path().join(format!("{name}.json"));
        let mut args = vec!["--receipt", receipt.to_str().unwrap()];
        args.extend_from_slice(extra);
        let mut child = cli_at(directory.path(), &exec_payload(&args), |command| {
            quiet(command);
            command.current_dir(cwd);
        });
        let status = wait_exit(&mut child, EXIT_LIMIT);
        (status.code(), read_json(&receipt))
    };
    // Inside the root, even in a subdirectory: the project's limits are the request.
    let (code, inside) = run(
        &["--project", "fixture-dev"],
        &root.join("nested"),
        "inside",
    );
    assert_eq!(code, Some(0), "{inside}");
    assert_eq!(inside["budget"]["source"], "project_limits");
    assert_eq!(
        inside["budget"]["requested"],
        json!({"cpu_milli": 200, "memory_bytes": 67108864, "tasks": 4})
    );
    assert_eq!(inside["project"]["id"], "fixture-dev");
    assert_eq!(inside["adapter"]["selected_by"], "project");
    // Outside the root, above the limits, or unregistered: nothing starts.
    let (outside_code, outside) = run(&["--project", "fixture-dev"], directory.path(), "outside");
    let (above_code, above) = run(
        &["--project", "fixture-dev", "--cpu", "201"],
        &root,
        "above",
    );
    let (unknown_code, unknown) = run(&["--project", "unregistered"], &root, "unknown");
    for (code, receipt) in [
        (outside_code, &outside),
        (above_code, &above),
        (unknown_code, &unknown),
    ] {
        assert_eq!(code, Some(125), "{receipt}");
        assert_eq!(receipt["result"], "not_started");
        assert!(receipt["attempts"].as_array().unwrap().is_empty());
    }
    for receipt in [&inside, &outside, &above, &unknown] {
        assert_no_secret(&receipt.to_string(), &secret);
    }
    record(
        "project-resolution",
        json!({"inside": {"budget": inside["budget"], "project": inside["project"], "result": inside["result"]},
               "outside": outside["reason"], "above_limits": above["reason"], "unregistered": unknown["reason"]}),
        &secret,
    );
}

#[test]
fn at_most_eight_cli_owners_hold_instances_at_once() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let release = work.join("release");
    let mut owners = Vec::new();
    for index in 0..8 {
        let ready = work.join(format!("ready-{index}"));
        let child = cli(&fixture, &small(&[]), |command| {
            quiet(command);
            command.env(
                PROBE,
                json!({"ready": ready, "release": release}).to_string(),
            );
        });
        owners.push((child, ready));
    }
    for (_, ready) in &owners {
        wait_for(EXIT_LIMIT, || ready.exists());
    }
    assert_eq!(fixture.authority.instances().unwrap().len(), 8);
    assert!(fixture.authority.pressure_normal().unwrap());
    let receipt = work.join("ninth.json");
    let out = work.join("ninth-payload.json");
    let mut ninth = cli(
        &fixture,
        &small(&["--receipt", receipt.to_str().unwrap()]),
        |command| {
            quiet(command);
            command.env(PROBE, json!({"out": out}).to_string());
        },
    );
    assert_eq!(wait_exit(&mut ninth, EXIT_LIMIT).code(), Some(125));
    assert!(!out.exists());
    let receipt = read_json(&receipt);
    assert_eq!(receipt["result"], "not_started");
    // Registration was refused, so no attempt record exists.
    assert!(receipt["attempts"][0]["record"].is_null());
    let note = receipt["attempts"][0]["note"].as_str().unwrap().to_string();
    assert!(note.contains("ResourceUnavailable"), "{note}");
    support::release(&release);
    for (mut child, _) in owners {
        assert!(wait_exit(&mut child, EXIT_LIMIT).success());
    }
    record(
        "instance-pool",
        json!({"concurrent_owners": 8, "ninth_note": note, "ninth_result": receipt["result"]}),
        &fixture.secret(),
    );
}

#[test]
fn owners_that_have_ended_never_exhaust_the_instance_pool() {
    let fixture = Fixture::start();
    // Without the reconciler, only registration can retire ended owners.
    fixture.authority.pause_reconciler(true);
    let work = fixture.work("work");
    let mut statuses = Vec::new();
    for index in 0..10 {
        let receipt = work.join(format!("run-{index}.json"));
        let mut child = cli(
            &fixture,
            &small(&["--receipt", receipt.to_str().unwrap()]),
            |command| {
                quiet(command);
                command.env(PROBE, json!({}).to_string());
            },
        );
        let status = wait_exit(&mut child, EXIT_LIMIT);
        assert_eq!(
            status.code(),
            Some(0),
            "run {index}: {}",
            read_json(&receipt)
        );
        statuses.push(status.code());
    }
    assert!(fixture.authority.instances().unwrap().len() <= 8);
    record(
        "sequential-owners",
        json!({"runs": statuses.len(), "statuses": statuses, "reconciler": "paused",
               "instances_after": fixture.authority.instances().unwrap().len()}),
        &fixture.secret(),
    );
}

#[test]
fn programs_that_cannot_start_keep_the_shells_statuses_and_admit_nothing() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let unexecutable = work.join("not-executable");
    std::fs::write(&unexecutable, "#!/bin/sh\nexit 0\n").unwrap();
    let mut cases = Vec::new();
    for (name, program, expected) in [
        ("missing", "/nonexistent/devguard-program".to_string(), 127),
        (
            "not-executable",
            unexecutable.to_string_lossy().into_owned(),
            126,
        ),
    ] {
        let receipt = work.join(format!("{name}.json"));
        let args: Vec<String> = [
            "exec",
            "--cpu",
            "50",
            "--memory",
            "16MiB",
            "--tasks",
            "2",
            "--receipt",
        ]
        .iter()
        .map(|arg| arg.to_string())
        .chain([receipt.to_string_lossy().into_owned(), "--".into(), program])
        .collect();
        let mut child = cli(&fixture, &args, quiet);
        assert_eq!(
            wait_exit(&mut child, EXIT_LIMIT).code(),
            Some(expected),
            "{name}"
        );
        let receipt = read_json(&receipt);
        assert_eq!(receipt["result"], "not_started", "{name}");
        assert!(receipt["attempts"].as_array().unwrap().is_empty(), "{name}");
        cases.push(json!({"case": name, "status": expected, "reason": receipt["reason"]}));
    }
    // An existing receipt path is never overwritten, and nothing starts.
    let existing = work.join("existing.json");
    std::fs::write(&existing, "keep").unwrap();
    let out = work.join("payload.json");
    let mut child = cli(
        &fixture,
        &small(&["--receipt", existing.to_str().unwrap()]),
        |command| {
            quiet(command);
            command.env(PROBE, json!({"out": out}).to_string());
        },
    );
    assert_eq!(wait_exit(&mut child, EXIT_LIMIT).code(), Some(125));
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "keep");
    assert!(!out.exists());
    // Without its launch helper the CLI starts nothing and admits nothing.
    let receipt = work.join("no-helper.json");
    let mut child = cli_with_helper(
        fixture.base(),
        std::path::Path::new("/nonexistent/devguard-launch"),
        &small(&["--receipt", receipt.to_str().unwrap()]),
        |command| {
            quiet(command);
            command.env(PROBE, json!({"out": out}).to_string());
        },
    );
    assert_eq!(wait_exit(&mut child, EXIT_LIMIT).code(), Some(125));
    assert!(!out.exists());
    let receipt = read_json(&receipt);
    assert!(receipt["reason"].as_str().unwrap().contains("helper"));
    assert!(receipt["attempts"].as_array().unwrap().is_empty());
    assert!(fixture.authority.instances().unwrap().is_empty());
    cases.push(json!({"case": "existing-receipt", "status": 125, "kept": true}));
    cases.push(json!({"case": "missing-helper", "status": 125, "reason": receipt["reason"]}));
    record(
        "unstartable-programs",
        json!({"cases": cases}),
        &fixture.secret(),
    );
}

/// A fake authority that completes only the handshake, advertising
/// `capabilities`. It records every request body it read.
fn handshake_only_authority(
    socket: &std::path::Path,
    capabilities: BTreeSet<devguard_contract::Capability>,
) -> (
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    std::thread::JoinHandle<()>,
) {
    use devguard_client::framing::{read_frame, write_frame};
    use devguard_client::protocol::*;
    let _ = std::fs::remove_file(socket);
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (flag, requests) = (stop.clone(), seen.clone());
    let server = std::thread::spawn(move || {
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(_) => return,
            };
            let Ok(caller) = devguard_client::peer::observe(&stream) else {
                continue;
            };
            let Ok(request) = read_frame::<Frame<Value>>(&mut stream, Duration::from_secs(1))
            else {
                continue;
            };
            requests.lock().unwrap().push(request.body.clone());
            let hello = Hello {
                protocol: 1,
                // SAFETY: geteuid has no preconditions.
                authority: PeerIdentity {
                    uid: unsafe { libc::geteuid() },
                    pid: std::process::id(),
                },
                caller,
                capabilities: capabilities.clone(),
                max_frame_bytes: MAX_FRAME_BYTES,
                frame_deadline_ms: FRAME_DEADLINE_MS,
                max_sessions: MAX_SESSIONS,
            };
            let _ = write_frame(
                &mut stream,
                &Frame {
                    version: WIRE_VERSION,
                    request_id: request.request_id,
                    body: Response::Hello(hello),
                },
                Duration::from_secs(1),
            );
            // The session ends here, so any further request finds it closed.
        }
    });
    (stop, seen, server)
}

#[test]
fn an_authority_without_fenced_launch_starts_nothing() {
    let directory = private_directory("dg-m-");
    let authority = TestAuthority::start(directory.path()).unwrap();
    let secret = match authority.consumer().unwrap() {
        devguard_client::protocol::CallerCredential::Consumer { secret, .. } => {
            secret.expose().to_string()
        }
        _ => unreachable!(),
    };
    let socket = authority.socket();
    drop(authority);
    let (stop, seen, server) = handshake_only_authority(
        &socket,
        BTreeSet::from([devguard_contract::Capability::DurableAdmission]),
    );
    let out = directory.path().join("payload.json");
    let receipt = directory.path().join("receipt.json");
    let args = small(&["--wait", "5s", "--receipt", receipt.to_str().unwrap()]);
    let started = Instant::now();
    let mut child = cli_at(directory.path(), &args, |command| {
        quiet(command);
        command.env(PROBE, json!({"out": out}).to_string());
    });
    let status = wait_exit(&mut child, EXIT_LIMIT);
    let elapsed = started.elapsed();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();
    assert_eq!(status.code(), Some(125));
    assert!(
        !out.exists(),
        "nothing may run without the fenced launch capability"
    );
    let text = std::fs::read_to_string(&receipt).unwrap();
    assert_no_secret(&text, &secret);
    let receipt: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(receipt["result"], "not_started");
    // The mismatch is a refusal, not a capacity condition: the wait does
    // not retry it.
    assert_eq!(receipt["wait"]["admissions"], 1);
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    let note = receipt["attempts"][0]["note"].as_str().unwrap().to_string();
    assert!(note.contains("ResourcePolicyUnsupported"), "{note}");
    assert!(receipt["attempts"][0]["record"].is_null());
    assert!(receipt["launch"].is_null());
    // Every handshake the CLI made for work required fenced launch.
    let requests = seen.lock().unwrap().clone();
    let demanding: Vec<&Value> = requests
        .iter()
        .filter(|body| {
            body["params"]["compatibility"]["required"]
                .as_array()
                .is_some_and(|required| !required.is_empty())
        })
        .collect();
    assert!(!demanding.is_empty(), "{requests:?}");
    for body in &demanding {
        assert!(body["params"]["compatibility"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("fenced_launch")));
    }
    record(
        "capability-mismatch",
        json!({"cli_status": status.code(), "note": note, "handshakes": requests,
               "receipt_wait": receipt["wait"]}),
        &secret,
    );
}
