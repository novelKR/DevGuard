#![cfg(target_os = "macos")]
//! Native host pressure readings and their effect on admission (DG1-C03).

mod support;
use devguard_contract::*;
use devguard_core::*;
use devguard_macos::{HostProbe, NativeProbe, Sampler, SamplerOutcome, SAMPLE_INTERVAL_MS};
use serde_json::json;
use std::time::{Duration, Instant};
use support::*;

#[test]
fn native_readings_are_structurally_valid_and_produce_samples() {
    let host = devguard_macos::NativeHost::open().unwrap();
    let clock = host.clock();
    let directory = tempfile::tempdir().unwrap();
    // Two paths on one volume are observed once.
    let mut probe =
        NativeProbe::new(vec![directory.path().to_path_buf(), std::env::temp_dir()]).unwrap();
    let raw = probe.read().unwrap();
    assert_eq!(raw.volumes.len(), 1);
    let mut sampler = Sampler::new(probe);
    let (baseline_at, first) = match sampler.sample(&clock, 0) {
        SamplerOutcome::Baseline { at, reading, .. } => (at, reading),
        other => panic!("expected a baseline, got {other:?}"),
    };
    let mut receipts = Vec::new();
    for _ in 0..2 {
        std::thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
        match sampler.sample(&clock, 0) {
            SamplerOutcome::Sample { sample, receipt } => {
                assert!([1, 2, 4].contains(&receipt.reading.memory_level));
                assert!(receipt.reading.paged_out_bytes >= first.paged_out_bytes);
                assert!(sample.disk_capacity_bytes > 0);
                assert!(sample.disk_available_bytes <= sample.disk_capacity_bytes);
                assert_eq!(sample.memory_full_basis_points, None);
                assert_eq!(sample.at.boot_id, clock.boot_id());
                // Both samples fall within ten seconds of the baseline, which is
                // therefore the start of their rate window.
                assert_eq!(
                    receipt.window_ms,
                    sample.at.monotonic_ms - baseline_at.monotonic_ms
                );
                assert!(receipt.window_ms > 0 && receipt.window_ms <= devguard_macos::WINDOW_MS);
                receipts.push(receipt);
            }
            other => panic!("expected a sample, got {other:?}"),
        }
    }
    record(
        "native-pressure-readings",
        json!({"baseline": first, "samples": receipts,
               "sources": {"memory": "kern.memorystatus_vm_pressure_level",
                           "page_out": "host_statistics64 HOST_VM_INFO64 (pageouts + swapouts) x kernel page size (host_page_size)",
                           "swap": "vm.swapusage xsu_used", "disk": "statfs f_blocks/f_bavail x f_bsize"}}),
    );
}

#[test]
fn a_missing_volume_fails_the_whole_reading_instead_of_dropping_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut probe = NativeProbe::new(vec![
        directory.path().to_path_buf(),
        directory.path().join("missing"),
    ])
    .unwrap();
    let error = probe.read().unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceControlUnavailable);
    assert!(NativeProbe::new(Vec::new()).is_err());
}

#[test]
fn admission_is_closed_until_a_sample_and_closes_when_sampling_stops() {
    let fixture = Fixture::new();
    let clock = fixture.host.clock();
    let mut authority = fixture.open();
    let principal = fixture.register_self(&mut authority);
    // Closed before the first valid sample.
    assert_eq!(authority.pressure(), PressureState::Critical);
    let before = authority
        .admit(&principal, request("before-sample"))
        .unwrap();
    assert_eq!(before.phase, AttemptPhase::Denied);
    assert_eq!(before.denial, Some(ErrorCode::ResourceUnavailable));

    let mut sampler = Sampler::new(Healthy { paged_out_bytes: 0 });
    assert!(matches!(
        sampler.sample(&clock, 0),
        SamplerOutcome::Baseline { .. }
    ));
    std::thread::sleep(Duration::from_millis(200));
    let SamplerOutcome::Sample { sample, .. } = sampler.sample(&clock, 0) else {
        panic!("expected a sample");
    };
    let sampled_at = sample.at.clone();
    assert_eq!(
        authority.observe_pressure(sample).unwrap(),
        PressureState::Normal
    );
    let admitted = authority.admit(&principal, request("healthy")).unwrap();
    assert_eq!(admitted.phase, AttemptPhase::Prepared);

    // The sampler stops. Admission must stay open while the last sample is at
    // most six seconds old and close once it is older, without any failure
    // being reported. Each open poll is timed before the check and the closed
    // poll after it, so scheduler latency cannot fake either boundary.
    let started = Instant::now();
    let mut last_open = sampled_at.clone();
    let closed_at = loop {
        let before = clock.now();
        if authority.pressure() == PressureState::Critical {
            break clock.now();
        }
        last_open = before;
        assert!(started.elapsed() < Duration::from_secs(12), "never closed");
        std::thread::sleep(Duration::from_millis(20));
    };
    let open_until = last_open.monotonic_ms - sampled_at.monotonic_ms;
    let time_to_closed = closed_at.monotonic_ms - sampled_at.monotonic_ms;
    assert!(open_until <= 6_000, "open {open_until} ms after the sample");
    assert!(
        time_to_closed > 6_000,
        "closed {time_to_closed} ms after the sample"
    );
    assert_eq!(authority.available_budget().unwrap(), Budget::ZERO);
    let after = authority.admit(&principal, request("stale")).unwrap();
    assert_eq!(after.phase, AttemptPhase::Denied);
    record(
        "admission-closes-on-stale-samples",
        json!({"last_sample": sampled_at, "last_open_poll_ms": open_until, "closed_at": closed_at,
               "time_to_closed_ms": time_to_closed, "freshness_limit_ms": 6_000}),
    );
}

#[test]
fn a_failed_probe_closes_admission_immediately() {
    let fixture = Fixture::new();
    let clock = fixture.host.clock();
    let mut authority = fixture.open();
    let principal = fixture.register_self(&mut authority);
    let mut sampler = Sampler::new(Healthy { paged_out_bytes: 0 });
    sampler.sample(&clock, 0);
    std::thread::sleep(Duration::from_millis(100));
    let SamplerOutcome::Sample { sample, .. } = sampler.sample(&clock, 0) else {
        panic!("expected a sample");
    };
    authority.observe_pressure(sample).unwrap();
    assert_eq!(
        authority.admit(&principal, request("open")).unwrap().phase,
        AttemptPhase::Prepared
    );
    // A real probe of a vanished path fails; the failure closes admission now.
    let directory = tempfile::tempdir().unwrap();
    let vanished = directory.path().join("vanishing");
    std::fs::create_dir(&vanished).unwrap();
    let mut failing = Sampler::new(NativeProbe::new(vec![vanished.clone()]).unwrap());
    std::fs::remove_dir(&vanished).unwrap();
    let failure = match failing.sample(&clock, 0) {
        SamplerOutcome::Failed { at, error, .. } => {
            // The library closes admission in the same call; the service loop's
            // time from a failed reading to Critical is measured separately.
            authority.pressure_observation_failed();
            assert_eq!(authority.pressure(), PressureState::Critical);
            let denied = authority
                .admit(&principal, request("after-failure"))
                .unwrap();
            assert_eq!(denied.phase, AttemptPhase::Denied);
            json!({"failed_at": at, "error": error, "state_after_failure": "critical",
                   "admission_after_failure": denied.phase})
        }
        other => panic!("expected a failure, got {other:?}"),
    };
    record("admission-closes-on-probe-failure", failure);
}

fn sample_at(at: ObservationTime) -> PressureSample {
    PressureSample {
        at,
        memory: MemoryPressure::Normal,
        pageout_mib_per_second: 0,
        swap_growth_mib_10s: 0,
        control_lag_ms: 0,
        memory_full_basis_points: None,
        disk_capacity_bytes: 100 * GIB,
        disk_available_bytes: 60 * GIB,
    }
}

#[test]
fn stale_future_and_replayed_native_samples_are_rejected() {
    // On controllers with no earlier sample, only the freshness check can
    // refuse a stale, future or other-boot sample.
    let clock = devguard_macos::NativeHost::open().unwrap().clock();
    let now = clock.now();
    let mut stale = now.clone();
    stale.monotonic_ms = now.monotonic_ms.saturating_sub(2_500);
    let mut future = now.clone();
    future.monotonic_ms = now.monotonic_ms + 1_000;
    let mut other_boot = now.clone();
    other_boot.boot_id = "00000000-0000-0000-0000-000000000000".into();
    for at in [stale, future, other_boot] {
        let fixture = Fixture::new();
        let mut authority = fixture.open();
        let error = authority.observe_pressure(sample_at(at)).unwrap_err();
        assert_eq!(error.message, "invalid or stale host sample");
        assert_eq!(authority.pressure(), PressureState::Critical);
    }
    // After a valid sample, a replay is refused by the monotonic check.
    let fixture = Fixture::new();
    let mut authority = fixture.open();
    let valid = sample_at(clock.now());
    assert_eq!(
        authority.observe_pressure(valid.clone()).unwrap(),
        PressureState::Normal
    );
    let replay = authority.observe_pressure(valid).unwrap_err();
    assert_eq!(replay.message, "host sample clock is not monotonic");
    assert_eq!(authority.pressure(), PressureState::Critical);
}

#[test]
fn registration_requires_the_native_identity_of_the_actual_peer() {
    let fixture = Fixture::new();
    let mut authority = fixture.open();
    let pid = std::process::id();
    let actual = fixture
        .host
        .backend()
        .process_identity(pid)
        .unwrap()
        .unwrap();
    let mut changed_start = actual.clone();
    changed_start.start_ticks += 1;
    let mut foreign_boot = actual.clone();
    foreign_boot.boot_id = "00000000-0000-0000-0000-000000000000".into();
    for forged in [changed_start, foreign_boot] {
        assert_eq!(
            authority
                .register(
                    TrustedPeer { uid: uid(), pid },
                    fixture.registration(forged)
                )
                .unwrap_err()
                .code,
            ErrorCode::Unauthorized
        );
    }
    // A peer PID that differs from the registered process is rejected even
    // when the declared identity is itself a real process.
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let child_identity = fixture
        .host
        .backend()
        .process_identity(child.id())
        .unwrap()
        .unwrap();
    assert_eq!(
        authority
            .register(
                TrustedPeer { uid: uid(), pid },
                fixture.registration(child_identity)
            )
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    child.kill().unwrap();
    child.wait().unwrap();
    let principal = authority
        .register(
            TrustedPeer { uid: uid(), pid },
            fixture.registration(actual.clone()),
        )
        .unwrap();
    assert_eq!(principal.instance().process, actual);
}
