#![cfg(target_os = "macos")]
//! The control probe against isolated fixture authorities, starting its
//! targets through the real launch helper. Raw receipts go to
//! `DEVGUARD_EVIDENCE_DIR` when the qualification harness sets it.

use devguard_contract::{AttemptPhase, Budget};
use devguard_daemon::fixture::TestAuthority;
use devguard_qualify::control::{run, Options};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// `devguard-launch` beside this test's target directory. The suite builds
/// it first.
fn helper() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile = exe.parent().unwrap().parent().unwrap();
    let helper = profile.join("devguard-launch");
    assert!(
        helper.is_file(),
        "build devguard-launch before these tests: {}",
        helper.display()
    );
    helper
}

fn authority() -> (TestAuthority, tempfile::TempDir) {
    let directory = tempfile::Builder::new()
        .prefix("dg-q-")
        .tempdir_in("/private/tmp")
        .unwrap();
    let authority = TestAuthority::start(directory.path()).unwrap();
    authority
        .wait_until_admitting(Duration::from_secs(10))
        .unwrap();
    (authority, directory)
}

fn samples(out: Vec<u8>) -> Vec<Value> {
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn of<'a>(samples: &'a [Value], kind: &str) -> Vec<&'a Value> {
    samples
        .iter()
        .filter(|sample| sample["kind"] == kind)
        .collect()
}

/// No attempt stays charged: the fixture lists charged attempts only.
fn nothing_charged(authority: &TestAuthority) {
    assert_eq!(authority.attempts().unwrap(), Vec::new());
    assert_eq!(authority.committed().unwrap(), Budget::ZERO);
}

fn record(name: &str, value: Value) {
    if let Some(directory) = std::env::var_os("DEVGUARD_EVIDENCE_DIR") {
        let path = PathBuf::from(directory).join(format!("{name}.json"));
        std::fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    }
}

fn fast() -> Options {
    let mut options = Options::protocol(Duration::from_secs(4));
    options.status_interval = Duration::from_millis(100);
    options.terminate_period = Duration::from_millis(1200);
    options.admission_wait = Duration::from_secs(10);
    options
}

#[test]
fn the_probe_measures_status_and_termination_on_its_own_targets() {
    let (authority, _directory) = authority();
    let mut out = Vec::new();
    let summary = run(
        authority.paths(),
        &helper(),
        &fast(),
        &mut out,
        &AtomicBool::new(false),
    )
    .unwrap();
    let samples = samples(out);
    assert_eq!(of(&samples, "header").len(), 1);
    let statuses = of(&samples, "status");
    assert!(statuses.len() >= 20, "{} status samples", statuses.len());
    for status in &statuses {
        assert_eq!(status["outcome"], "answered", "{status}");
        assert_eq!(status["phase"], json!(AttemptPhase::RunAuthorized));
        assert!(status["latency_ms"].as_f64().unwrap() > 0.0);
    }
    let terminations = of(&samples, "terminate");
    assert!(
        terminations.len() >= 2,
        "{} terminations",
        terminations.len()
    );
    for termination in &terminations {
        assert_eq!(termination["outcome"], "acknowledged", "{termination}");
        assert!(termination["signalled"].as_u64().unwrap() >= 1);
        assert_eq!(termination["complete"], true);
        assert_eq!(termination["killed_by_probe"], false, "{termination}");
        assert_eq!(termination["phase"], json!(AttemptPhase::Released));
        assert_eq!(termination["release_reason"], "scope_terminated");
        let ack = termination["ack_ms"].as_f64().unwrap();
        let exited = termination["exited_ms"].as_f64().unwrap();
        let released = termination["released_ms"].as_f64().unwrap();
        assert!(ack <= released && exited <= released, "{termination}");
    }
    let admissions = of(&samples, "admission");
    assert_eq!(admissions.len(), terminations.len() + 1);
    assert!(admissions
        .iter()
        .all(|admission| admission["outcome"] == "admitted"));
    let target = of(&samples, "status_target");
    assert_eq!(target.len(), 1);
    assert_eq!(target[0]["outcome"], "settled");
    assert_eq!(target[0]["killed_by_probe"], false);
    assert_eq!(target[0]["phase"], json!(AttemptPhase::Released));
    assert_eq!(summary["status_target_released"], true);
    nothing_charged(&authority);
    let latencies = |values: &[&Value], field: &str| {
        let mut values: Vec<f64> = values.iter().filter_map(|v| v[field].as_f64()).collect();
        values.sort_by(f64::total_cmp);
        json!({"count": values.len(), "min": values.first(), "max": values.last()})
    };
    record(
        "control-probe",
        json!({"summary": summary, "statuses": latencies(&statuses, "latency_ms"),
               "acknowledgements": latencies(&terminations, "ack_ms"),
               "releases": latencies(&terminations, "released_ms"),
               "nothing_charged_after": true}),
    );
}

#[test]
fn a_stop_ends_sampling_and_still_settles_the_status_target() {
    let (authority, _directory) = authority();
    let mut options = fast();
    options.duration = Duration::from_secs(60);
    let stop = AtomicBool::new(true);
    let started = Instant::now();
    let mut out = Vec::new();
    let summary = run(authority.paths(), &helper(), &options, &mut out, &stop).unwrap();
    assert!(started.elapsed() < Duration::from_secs(30));
    let samples = samples(out);
    assert!(of(&samples, "status").is_empty());
    assert!(of(&samples, "terminate").is_empty());
    assert_eq!(summary["status_target_released"], true);
    nothing_charged(&authority);
    record(
        "control-stopped",
        json!({"summary": summary, "seconds": started.elapsed().as_secs_f64()}),
    );
}

#[test]
fn a_target_that_is_never_admitted_starts_no_probe_and_charges_nothing() {
    let (authority, _directory) = authority();
    let capacity = authority.work_capacity().unwrap();
    let mut options = fast();
    options.admission_wait = Duration::from_secs(1);
    options.target = Budget {
        cpu_milli: capacity.cpu_milli + 1000,
        ..options.target
    };
    let mut out = Vec::new();
    let started = Instant::now();
    let error = run(
        authority.paths(),
        &helper(),
        &options,
        &mut out,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(10));
    let samples = samples(out);
    let admissions = of(&samples, "admission");
    assert_eq!(admissions.len(), 1);
    assert_eq!(admissions[0]["outcome"], "refused");
    assert!(of(&samples, "status").is_empty());
    nothing_charged(&authority);
    record(
        "control-refused",
        json!({"error": error.message, "admission": admissions[0]}),
    );
}

#[test]
fn a_stop_in_the_middle_of_sampling_settles_every_target() {
    let (authority, _directory) = authority();
    let mut options = fast();
    options.duration = Duration::from_secs(60);
    let stop = AtomicBool::new(false);
    let started = Instant::now();
    let mut out = Vec::new();
    let summary = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(2500));
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        run(authority.paths(), &helper(), &options, &mut out, &stop).unwrap()
    });
    assert!(started.elapsed() < Duration::from_secs(40));
    let samples = samples(out);
    assert!(!of(&samples, "status").is_empty());
    assert!(!of(&samples, "terminate").is_empty());
    assert_eq!(summary["status_target_released"], true);
    assert_eq!(summary["stopped_early"], true);
    nothing_charged(&authority);
    record(
        "control-stopped-mid-run",
        json!({"summary": summary, "statuses": of(&samples, "status").len(),
               "terminations": of(&samples, "terminate").len()}),
    );
}

#[test]
fn a_helper_that_ends_before_ready_is_reported_and_charges_nothing() {
    let (authority, _directory) = authority();
    let mut out = Vec::new();
    // Not a launch helper: it exits at once without reporting READY.
    let error = run(
        authority.paths(),
        std::path::Path::new("/usr/bin/false"),
        &fast(),
        &mut out,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    let samples = samples(out);
    let target = of(&samples, "status_target");
    assert_eq!(target.len(), 1);
    assert_eq!(target[0]["outcome"], "error");
    assert_eq!(target[0]["stage"], "helper");
    assert!(target[0]["message"]
        .as_str()
        .unwrap()
        .contains("reported HelperExited"));
    nothing_charged(&authority);
    record(
        "control-helper-exited",
        json!({"error": error.message, "status_target": target[0]}),
    );
}
