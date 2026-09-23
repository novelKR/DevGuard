//! Converts successive host readings into core pressure samples. Rates use the
//! most recent readings within ten seconds; a sample needs a prior reading, so
//! the controller stays closed until the second valid reading.

use crate::host::{HostProbe, HostReading};
use devguard_contract::{Error, ErrorCode, ObservationTime, Result};
use devguard_core::{Clock, DiskObservation, MemoryPressure, PressureSample};
use serde::Serialize;
use std::collections::VecDeque;

pub const SAMPLE_INTERVAL_MS: u64 = 2_000;
pub const WINDOW_MS: u64 = 10_000;
const MIB: u128 = 1024 * 1024;

/// The derived values for one sample, retained as a host receipt.
#[derive(Debug, Clone, Serialize)]
pub struct SampleReceipt {
    pub at: ObservationTime,
    pub reading: HostReading,
    /// How long the host read took; a stuck probe shows here.
    pub read_ms: u64,
    pub window_ms: u64,
    pub memory: &'static str,
    pub pageout_mib_per_second: u64,
    pub swap_growth_mib_10s: u64,
    pub control_lag_ms: u64,
    pub disk_capacity_bytes: u64,
    pub disk_available_bytes: u64,
}

#[derive(Debug)]
pub enum SamplerOutcome {
    /// A valid reading that cannot yield a rate yet: the first after startup
    /// or a failure, or one taken no later than the previous reading.
    Baseline {
        at: ObservationTime,
        reading: HostReading,
        read_ms: u64,
    },
    Sample {
        sample: PressureSample,
        receipt: SampleReceipt,
    },
    /// The reading failed or was inconsistent. Callers must close new work.
    Failed {
        at: ObservationTime,
        error: Error,
        read_ms: u64,
    },
}

#[derive(Debug, Clone, Copy)]
struct Point {
    monotonic_ms: u64,
    paged_out_bytes: u64,
    swap_used_bytes: u64,
}

pub struct Sampler<P: HostProbe> {
    probe: P,
    boot_id: Option<String>,
    history: VecDeque<Point>,
}

impl<P: HostProbe> Sampler<P> {
    pub fn new(probe: P) -> Self {
        Self {
            probe,
            boot_id: None,
            history: VecDeque::new(),
        }
    }

    /// Read once. `control_lag_ms` is how late the caller's loop woke for
    /// this reading relative to its schedule.
    pub fn sample(&mut self, clock: &impl Clock, control_lag_ms: u64) -> SamplerOutcome {
        let started = clock.now();
        let reading = self.probe.read();
        // Time the observation after the read so its age is never understated.
        let at = clock.now();
        let read_ms = at.monotonic_ms.saturating_sub(started.monotonic_ms);
        match reading.and_then(|reading| self.derive(&at, reading, control_lag_ms, read_ms)) {
            Ok(outcome) => outcome,
            Err(error) => {
                // A gap breaks rate continuity; restart from a new baseline.
                self.history.clear();
                SamplerOutcome::Failed { at, error, read_ms }
            }
        }
    }

    fn derive(
        &mut self,
        at: &ObservationTime,
        reading: HostReading,
        control_lag_ms: u64,
        read_ms: u64,
    ) -> Result<SamplerOutcome> {
        let memory = match reading.memory_level {
            1 => MemoryPressure::Normal,
            2 => MemoryPressure::Warning,
            4 => MemoryPressure::Critical,
            _ => return Err(invalid("unrecognized memory pressure level")),
        };
        let disks: Vec<DiskObservation> = reading
            .volumes
            .iter()
            .map(|volume| DiskObservation {
                capacity_bytes: volume.capacity_bytes,
                available_bytes: volume.available_bytes,
            })
            .collect();
        let disk = DiskObservation::most_constrained(&disks)
            .filter(|disk| disk.capacity_bytes > 0 && disk.available_bytes <= disk.capacity_bytes)
            .ok_or_else(|| invalid("volume observation is missing or inconsistent"))?;
        if self
            .boot_id
            .as_deref()
            .is_some_and(|boot| boot != at.boot_id)
        {
            return Err(invalid("host clock changed boot identity"));
        }
        self.boot_id = Some(at.boot_id.clone());
        if let Some(last) = self.history.back() {
            if at.monotonic_ms < last.monotonic_ms || reading.paged_out_bytes < last.paged_out_bytes
            {
                return Err(invalid("host clock or cumulative counter moved backwards"));
            }
            // A reading in the same millisecond is not a regression, but no
            // time has elapsed for a rate; the earlier point is kept.
            if at.monotonic_ms == last.monotonic_ms {
                return Ok(SamplerOutcome::Baseline {
                    at: at.clone(),
                    reading,
                    read_ms,
                });
            }
        }
        while self
            .history
            .front()
            .is_some_and(|oldest| at.monotonic_ms - oldest.monotonic_ms > WINDOW_MS)
        {
            self.history.pop_front();
        }
        let base = self.history.front().copied();
        // Growth is measured from the lowest swap use in the window, so a dip
        // followed by growth is not hidden by a higher starting point.
        let swap_floor = self.history.iter().map(|point| point.swap_used_bytes).min();
        self.history.push_back(Point {
            monotonic_ms: at.monotonic_ms,
            paged_out_bytes: reading.paged_out_bytes,
            swap_used_bytes: reading.swap_used_bytes,
        });
        let Some(base) = base else {
            return Ok(SamplerOutcome::Baseline {
                at: at.clone(),
                reading,
                read_ms,
            });
        };
        let window_ms = at.monotonic_ms - base.monotonic_ms;
        // Round up: a threshold must not be missed through truncation.
        let paged = u128::from(reading.paged_out_bytes - base.paged_out_bytes);
        let pageout_mib_per_second = ceil_div(paged * 1_000, u128::from(window_ms) * MIB);
        // Swap growth is normalized to ten seconds; a shorter startup window is
        // extrapolated upward rather than under-reported.
        let floor = swap_floor.unwrap_or(base.swap_used_bytes);
        let grown = u128::from(reading.swap_used_bytes.saturating_sub(floor));
        let swap_growth_mib_10s =
            ceil_div(grown * u128::from(WINDOW_MS), u128::from(window_ms) * MIB);
        let pageout_mib_per_second = u64::try_from(pageout_mib_per_second).unwrap_or(u64::MAX);
        let swap_growth_mib_10s = u64::try_from(swap_growth_mib_10s).unwrap_or(u64::MAX);
        let sample = PressureSample {
            at: at.clone(),
            memory,
            pageout_mib_per_second,
            swap_growth_mib_10s,
            control_lag_ms,
            memory_full_basis_points: None,
            disk_capacity_bytes: disk.capacity_bytes,
            disk_available_bytes: disk.available_bytes,
        };
        let receipt = SampleReceipt {
            at: at.clone(),
            reading,
            read_ms,
            window_ms,
            memory: match memory {
                MemoryPressure::Normal => "normal",
                MemoryPressure::Warning => "warning",
                MemoryPressure::Critical => "critical",
            },
            pageout_mib_per_second,
            swap_growth_mib_10s,
            control_lag_ms,
            disk_capacity_bytes: disk.capacity_bytes,
            disk_available_bytes: disk.available_bytes,
        };
        Ok(SamplerOutcome::Sample { sample, receipt })
    }
}

fn ceil_div(numerator: u128, denominator: u128) -> u128 {
    numerator.div_ceil(denominator.max(1))
}

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::VolumeReading;
    use std::sync::{Arc, Mutex};

    const GIB: u64 = 1024 * 1024 * 1024;

    #[derive(Clone, Default)]
    struct Script(Arc<Mutex<VecDeque<Result<HostReading>>>>);
    impl HostProbe for Script {
        fn read(&mut self) -> Result<HostReading> {
            self.0
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted reading")
        }
    }
    impl Script {
        fn push(&self, reading: Result<HostReading>) {
            self.0.lock().unwrap().push_back(reading);
        }
    }

    struct Time(Mutex<ObservationTime>);
    impl Clock for Time {
        fn now(&self) -> ObservationTime {
            self.0.lock().unwrap().clone()
        }
    }
    impl Time {
        fn at(ms: u64) -> Self {
            Self(Mutex::new(ObservationTime {
                boot_id: "boot".into(),
                monotonic_ms: ms,
            }))
        }
        fn set(&self, ms: u64) {
            self.0.lock().unwrap().monotonic_ms = ms;
        }
    }

    fn reading(level: u32, paged_out: u64, swap: u64) -> HostReading {
        HostReading {
            memory_level: level,
            paged_out_bytes: paged_out,
            swap_used_bytes: swap,
            volumes: vec![VolumeReading {
                mount: "/".into(),
                capacity_bytes: 100 * GIB,
                available_bytes: 40 * GIB,
            }],
        }
    }

    fn sample(outcome: SamplerOutcome) -> (PressureSample, SampleReceipt) {
        match outcome {
            SamplerOutcome::Sample { sample, receipt } => (sample, receipt),
            other => panic!("expected a sample, got {other:?}"),
        }
    }

    #[test]
    fn a_baseline_precedes_the_first_rate_and_windows_are_bounded() {
        let script = Script::default();
        let clock = Time::at(1_000);
        let mut sampler = Sampler::new(script.clone());
        script.push(Ok(reading(1, 0, 0)));
        assert!(matches!(
            sampler.sample(&clock, 0),
            SamplerOutcome::Baseline { .. }
        ));
        // 64 MiB paged out over two seconds is 32 MiB/s; swap growth over a
        // two-second startup window extrapolates to ten seconds.
        clock.set(3_000);
        script.push(Ok(reading(2, 64 << 20, 8 << 20)));
        let (first, receipt) = sample(sampler.sample(&clock, 17));
        assert_eq!(first.memory, MemoryPressure::Warning);
        assert_eq!(first.pageout_mib_per_second, 32);
        assert_eq!(first.swap_growth_mib_10s, 40);
        assert_eq!(first.control_lag_ms, 17);
        assert_eq!(receipt.window_ms, 2_000);
        assert_eq!(first.memory_full_basis_points, None);
        for (index, ms) in (5_000..=13_000).step_by(2_000).enumerate() {
            clock.set(ms);
            script.push(Ok(reading(1, 64 << 20, (8 + index as u64 + 1) << 20)));
            let (_, receipt) = sample(sampler.sample(&clock, 0));
            assert!(receipt.window_ms <= WINDOW_MS);
        }
        // After ten seconds the oldest reading leaves the window.
        let (last, receipt) = sample({
            clock.set(15_000);
            script.push(Ok(reading(1, 64 << 20, 14 << 20)));
            sampler.sample(&clock, 0)
        });
        assert_eq!(receipt.window_ms, WINDOW_MS);
        assert_eq!(last.pageout_mib_per_second, 0);
        assert_eq!(last.swap_growth_mib_10s, 5);
    }

    #[test]
    fn rounding_never_hides_a_threshold_and_shrinking_swap_is_not_growth() {
        let script = Script::default();
        let clock = Time::at(0);
        let mut sampler = Sampler::new(script.clone());
        script.push(Ok(reading(1, 0, 100 << 20)));
        sampler.sample(&clock, 0);
        clock.set(2_000);
        script.push(Ok(reading(1, (31 << 20) + 1, 10 << 20)));
        let (sample, _) = sample(sampler.sample(&clock, 0));
        assert_eq!(sample.pageout_mib_per_second, 16);
        assert_eq!(sample.swap_growth_mib_10s, 0);
    }

    #[test]
    fn swap_growth_after_a_dip_is_measured_from_the_lowest_point() {
        let script = Script::default();
        let clock = Time::at(0);
        let mut sampler = Sampler::new(script.clone());
        for (ms, swap) in [(0u64, 100u64), (2_000, 40), (4_000, 90)] {
            clock.set(ms);
            script.push(Ok(reading(1, 0, swap << 20)));
            sampler.sample(&clock, 0);
        }
        clock.set(6_000);
        script.push(Ok(reading(1, 0, 140 << 20)));
        let (sample, receipt) = sample(sampler.sample(&clock, 0));
        // 100 MiB of growth since the 40 MiB low point, over a six-second
        // window normalized to ten seconds; the net change from the start is
        // only 40 MiB.
        assert_eq!(receipt.window_ms, 6_000);
        assert_eq!(sample.swap_growth_mib_10s, 167);
    }

    #[test]
    fn failures_and_inconsistent_readings_restart_from_a_baseline() {
        let script = Script::default();
        let clock = Time::at(0);
        let mut sampler = Sampler::new(script.clone());
        script.push(Ok(reading(1, 10, 0)));
        sampler.sample(&clock, 0);
        clock.set(2_000);
        script.push(Err(invalid("injected probe failure")));
        assert!(matches!(
            sampler.sample(&clock, 0),
            SamplerOutcome::Failed { .. }
        ));
        // History was discarded: the next reading is a new baseline.
        clock.set(4_000);
        script.push(Ok(reading(1, 20, 0)));
        assert!(matches!(
            sampler.sample(&clock, 0),
            SamplerOutcome::Baseline { .. }
        ));
        // A cumulative counter cannot move backwards within one boot.
        clock.set(6_000);
        script.push(Ok(reading(1, 5, 0)));
        assert!(matches!(
            sampler.sample(&clock, 0),
            SamplerOutcome::Failed { .. }
        ));
        for bad in [
            reading(3, 0, 0),
            HostReading {
                volumes: Vec::new(),
                ..reading(1, 0, 0)
            },
            HostReading {
                volumes: vec![VolumeReading {
                    mount: "/".into(),
                    capacity_bytes: GIB,
                    available_bytes: 2 * GIB,
                }],
                ..reading(1, 0, 0)
            },
        ] {
            clock.set(clock.now().monotonic_ms + 2_000);
            script.push(Ok(bad));
            assert!(matches!(
                sampler.sample(&clock, 0),
                SamplerOutcome::Failed { .. }
            ));
        }
    }

    #[test]
    fn a_repeated_millisecond_has_no_rate_but_a_backward_or_rebooted_clock_fails() {
        let script = Script::default();
        let clock = Time::at(5_000);
        let mut sampler = Sampler::new(script.clone());
        script.push(Ok(reading(1, 0, 0)));
        sampler.sample(&clock, 0);
        // Two readings within one millisecond: no rate, and no failure.
        script.push(Ok(reading(1, 0, 0)));
        assert!(matches!(
            sampler.sample(&clock, 0),
            SamplerOutcome::Baseline { .. }
        ));
        // The earlier point still anchors the next rate.
        clock.set(7_000);
        script.push(Ok(reading(1, 0, 0)));
        let (_, receipt) = sample(sampler.sample(&clock, 0));
        assert_eq!(receipt.window_ms, 2_000);
        clock.set(6_000);
        script.push(Ok(reading(1, 0, 0)));
        assert!(matches!(
            sampler.sample(&clock, 0),
            SamplerOutcome::Failed { .. }
        ));
        script.push(Ok(reading(1, 0, 0)));
        *clock.0.lock().unwrap() = ObservationTime {
            boot_id: "other".into(),
            monotonic_ms: 9_000,
        };
        assert!(matches!(
            sampler.sample(&clock, 0),
            SamplerOutcome::Failed { .. }
        ));
    }

    #[test]
    fn the_most_constrained_volume_is_reported() {
        let script = Script::default();
        let clock = Time::at(0);
        let mut sampler = Sampler::new(script.clone());
        let mut volumes = reading(1, 0, 0);
        volumes.volumes.push(VolumeReading {
            mount: "/Volumes/Work".into(),
            capacity_bytes: 100 * GIB,
            available_bytes: 3 * GIB,
        });
        script.push(Ok(volumes.clone()));
        sampler.sample(&clock, 0);
        clock.set(2_000);
        script.push(Ok(volumes));
        let (sample, _) = sample(sampler.sample(&clock, 0));
        assert_eq!(sample.disk_available_bytes, 3 * GIB);
    }
}
