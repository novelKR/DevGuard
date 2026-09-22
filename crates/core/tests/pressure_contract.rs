use devguard_contract::*;
use devguard_core::*;

const GIB: u64 = 1024 * 1024 * 1024;
fn time(ms: u64) -> ObservationTime {
    ObservationTime {
        boot_id: "boot".into(),
        monotonic_ms: ms,
    }
}
fn sample(ms: u64) -> PressureSample {
    PressureSample {
        at: time(ms),
        memory: MemoryPressure::Normal,
        pageout_mib_per_second: 0,
        swap_growth_mib_10s: 0,
        control_lag_ms: 0,
        memory_full_basis_points: None,
        disk_capacity_bytes: 100 * GIB,
        disk_available_bytes: 40 * GIB,
    }
}

#[test]
fn startup_and_missing_measurements_are_fail_closed() {
    let mut p = PressureController::default();
    assert_eq!(p.current(&time(0)), PressureState::Critical);
    assert_eq!(
        p.observe(sample(0), &time(0)).unwrap(),
        PressureState::Normal
    );
    assert_eq!(p.current(&time(6_000)), PressureState::Normal);
    assert_eq!(p.current(&time(6_001)), PressureState::Critical);
}

#[test]
fn soft_pressure_requires_two_observations_but_critical_memory_is_immediate() {
    let mut p = PressureController::default();
    p.observe(sample(0), &time(0)).unwrap();
    let mut warn = sample(2_000);
    warn.memory = MemoryPressure::Warning;
    assert_eq!(
        p.observe(warn, &time(2_000)).unwrap(),
        PressureState::Normal
    );
    let mut warn = sample(4_000);
    warn.memory = MemoryPressure::Warning;
    assert_eq!(
        p.observe(warn, &time(4_000)).unwrap(),
        PressureState::Constrained
    );
    let mut critical = sample(6_000);
    critical.memory = MemoryPressure::Critical;
    assert_eq!(
        p.observe(critical, &time(6_000)).unwrap(),
        PressureState::Critical
    );
}

#[test]
fn recovery_requires_thirty_seconds_per_step_without_a_sample_gap() {
    let mut p = PressureController::default();
    let mut critical = sample(0);
    critical.memory = MemoryPressure::Critical;
    p.observe(critical, &time(0)).unwrap();
    for ms in (2_000..32_000).step_by(2_000) {
        assert_eq!(
            p.observe(sample(ms), &time(ms)).unwrap(),
            PressureState::Critical
        );
    }
    assert_eq!(
        p.observe(sample(32_000), &time(32_000)).unwrap(),
        PressureState::Constrained
    );
    for ms in (34_000..62_000).step_by(2_000) {
        assert_eq!(
            p.observe(sample(ms), &time(ms)).unwrap(),
            PressureState::Constrained
        );
    }
    assert_eq!(
        p.observe(sample(62_000), &time(62_000)).unwrap(),
        PressureState::Normal
    );
}

#[test]
fn repeated_lag_or_linux_full_psi_trip_critical() {
    for psi in [false, true] {
        let mut p = PressureController::default();
        p.observe(sample(0), &time(0)).unwrap();
        for ms in [2_000, 4_000] {
            let mut stressed = sample(ms);
            if psi {
                stressed.memory_full_basis_points = Some(1_000);
            } else {
                stressed.control_lag_ms = 1_000;
            }
            p.observe(stressed, &time(ms)).unwrap();
        }
        assert_eq!(p.current(&time(4_000)), PressureState::Critical);
    }
}

#[test]
fn disk_watermarks_apply_absolute_and_relative_limits() {
    let mut p = PressureController::default();
    p.observe(sample(0), &time(0)).unwrap();
    let mut disk = sample(2_000);
    disk.disk_available_bytes = 5 * GIB;
    assert_eq!(
        p.observe(disk, &time(2_000)).unwrap(),
        PressureState::Critical
    );
    for ms in (4_000..80_000).step_by(2_000) {
        let mut not_clear = sample(ms);
        not_clear.disk_available_bytes = 10 * GIB;
        assert_eq!(
            p.observe(not_clear, &time(ms)).unwrap(),
            PressureState::Critical
        );
    }
}

#[test]
fn replayed_future_or_rebooted_samples_cannot_establish_health() {
    let mut p = PressureController::default();
    p.observe(sample(0), &time(0)).unwrap();
    assert!(p.observe(sample(0), &time(2_000)).is_err());
    assert_eq!(p.current(&time(2_000)), PressureState::Critical);
    assert!(p.observe(sample(4_000), &time(2_000)).is_err());
    let mut rebooted = sample(3_000);
    rebooted.at.boot_id = "other-boot".into();
    assert!(p.observe(rebooted, &time(3_000)).is_err());
}

#[test]
fn missing_measurement_gap_does_not_count_as_a_recovery_interval() {
    let mut p = PressureController::default();
    let mut critical = sample(0);
    critical.memory = MemoryPressure::Critical;
    p.observe(critical, &time(0)).unwrap();
    p.observe(sample(2_000), &time(2_000)).unwrap();
    assert_eq!(
        p.observe(sample(32_000), &time(32_000)).unwrap(),
        PressureState::Critical
    );
}
