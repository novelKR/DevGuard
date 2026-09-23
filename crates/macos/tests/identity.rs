#![cfg(target_os = "macos")]
//! Native boot clock, host capacity and process identity (DG1-C03).

mod support;
use devguard_contract::*;
use devguard_core::*;
use devguard_macos::{process_identity, NativeHost};
use serde_json::json;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use support::*;

#[test]
fn boot_clock_matches_the_kernel_boot_session_and_advances_monotonically() {
    let host = NativeHost::open().unwrap();
    let clock = host.clock();
    assert_eq!(clock.boot_id(), sysctl("kern.bootsessionuuid"));
    validate_id(clock.boot_id()).unwrap();
    let mut readings = vec![clock.now()];
    for _ in 0..5 {
        std::thread::sleep(Duration::from_millis(50));
        readings.push(clock.now());
    }
    for pair in readings.windows(2) {
        assert_eq!(pair[0].boot_id, pair[1].boot_id);
        let elapsed = pair[1].monotonic_ms - pair[0].monotonic_ms;
        assert!((45..1_000).contains(&elapsed), "elapsed {elapsed} ms");
    }
    // Boot-relative: the clock cannot exceed the time since kern.boottime by
    // more than calendar adjustment, and is far above zero on a running host.
    let boot_seconds: u64 = sysctl("kern.boottime")
        .split(['=', ','])
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let since_boot_ms = (wall - boot_seconds) * 1_000;
    let now = clock.now().monotonic_ms;
    assert!(
        now.abs_diff(since_boot_ms) < 120_000,
        "{now} vs {since_boot_ms}"
    );
    record(
        "boot-clock",
        json!({"boot_id": clock.boot_id(), "clock": "CLOCK_MONOTONIC_RAW (mach continuous time)",
               "unit": "milliseconds since boot", "readings": readings,
               "wall_seconds_since_kern_boottime": wall - boot_seconds}),
    );
}

#[test]
fn host_capacity_matches_kernel_counts() {
    let capacity = NativeHost::open().unwrap().capacity();
    assert_eq!(capacity.logical_cpus.to_string(), sysctl("hw.logicalcpu"));
    assert_eq!(capacity.memory_bytes.to_string(), sysctl("hw.memsize"));
    record("host-capacity", json!(capacity));
}

#[test]
fn process_identity_is_repeatable_and_distinguishes_exit_from_refusal() {
    let host = NativeHost::open().unwrap();
    let boot = host.clock().boot_id().to_owned();
    let own = std::process::id();
    let first = process_identity(&boot, own).unwrap().unwrap();
    for _ in 0..20 {
        assert_eq!(process_identity(&boot, own).unwrap().as_ref(), Some(&first));
    }
    assert_eq!(first.pid, own);
    assert_eq!(first.boot_id, boot);

    let mut child = Command::new("/bin/sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let child_identity = process_identity(&boot, child.id()).unwrap().unwrap();
    assert_ne!(child_identity.start_ticks, first.start_ticks);
    assert_eq!(
        host.backend().process_identity(child.id()).unwrap(),
        Some(child_identity.clone())
    );
    // SAFETY: signalling our own unreaped child by its PID.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGKILL) }, 0);
    // An exited but unreaped child is not a live identity, and neither is it
    // reusable: its PID stays allocated until reaped.
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_identity(&boot, child.id()).unwrap().is_some() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let zombie = process_identity(&boot, child.id()).unwrap();
    child.wait().unwrap();
    let reaped = process_identity(&boot, child.id()).unwrap();
    assert_eq!((&zombie, &reaped), (&None, &None));

    for absent in [0, 999_999, u32::MAX] {
        assert_eq!(process_identity(&boot, absent).unwrap(), None);
    }
    // SAFETY: geteuid has no preconditions.
    let refused = if unsafe { libc::geteuid() } != 0 {
        let error = process_identity(&boot, 1).unwrap_err();
        assert_eq!(error.code, ErrorCode::ResourceControlUnavailable);
        Some(error.message)
    } else {
        None
    };
    record(
        "process-identity",
        json!({"start_ticks_source": "proc_pid_rusage ri_proc_start_abstime (mach absolute time units)",
               "self": first, "repeated_reads": 20, "child": child_identity,
               "zombie_identity": zombie, "reaped_identity": reaped,
               "launchd_observation_refused": refused}),
    );
}

#[test]
fn native_backend_supplies_identity_and_the_macos_plan_but_no_scope_evidence() {
    let host = NativeHost::open().unwrap();
    let backend = host.backend();
    let own = backend
        .process_identity(std::process::id())
        .unwrap()
        .unwrap();
    let plan = backend.plan(&request("plan").intent).unwrap();
    plan.validate().unwrap();
    assert_eq!(plan.levels(), ResourceLevels::MACOS);
    assert_eq!(plan.scope_kind, ScopeKind::ObservedProcessGroup);
    assert!(!plan.levels().satisfies(ResourceLevels::KERNEL));
    let scope = ScopeIdentity {
        kind: ScopeKind::ObservedProcessGroup,
        scope_id: "unregistered".into(),
        root: own,
    };
    assert!(backend.binding(&scope).is_err());
    assert!(backend.observe_scope(&scope).is_err());
}
