use crate::fresh;
use devguard_contract::*;
use serde::{Deserialize, Serialize};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureState {
    Normal,
    Constrained,
    Critical,
}

impl PressureState {
    pub fn target(self, capacity: Budget) -> Budget {
        match self {
            Self::Normal => capacity,
            Self::Constrained => capacity.half(),
            Self::Critical => Budget::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct PressureSample {
    pub at: ObservationTime,
    pub memory: MemoryPressure,
    pub pageout_mib_per_second: u64,
    pub swap_growth_mib_10s: u64,
    pub control_lag_ms: u64,
    /// Linux memory full avg10 in basis points; None is valid on macOS.
    pub memory_full_basis_points: Option<u32>,
    pub disk_capacity_bytes: u64,
    pub disk_available_bytes: u64,
}

fn disk_critical_threshold(capacity: u64) -> u64 {
    (capacity / 20).max(2 * GIB)
}

fn disk_clear_threshold(capacity: u64) -> u64 {
    (capacity / 10).max(4 * GIB)
}

/// One observed target volume. A host may write workloads to several volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskObservation {
    pub capacity_bytes: u64,
    pub available_bytes: u64,
}

impl DiskObservation {
    /// Select the volume that the pressure rules treat most severely: critical
    /// before not-yet-recovered, then the smallest margin above recovery.
    /// Invalid volumes (zero capacity or impossible availability) are chosen
    /// first so that the resulting sample is rejected rather than hidden.
    pub fn most_constrained(volumes: &[Self]) -> Option<Self> {
        volumes.iter().copied().max_by_key(|volume| {
            let invalid =
                volume.capacity_bytes == 0 || volume.available_bytes > volume.capacity_bytes;
            let severity = if invalid {
                3
            } else if volume.available_bytes <= disk_critical_threshold(volume.capacity_bytes) {
                2
            } else if volume.available_bytes <= disk_clear_threshold(volume.capacity_bytes) {
                1
            } else {
                0
            };
            let margin = i128::from(volume.available_bytes)
                - i128::from(disk_clear_threshold(volume.capacity_bytes));
            (severity, std::cmp::Reverse(margin))
        })
    }
}

impl PressureSample {
    fn critical_now(&self) -> bool {
        self.memory == MemoryPressure::Critical
            || self.disk_available_bytes <= disk_critical_threshold(self.disk_capacity_bytes)
    }
    fn critical_repeated(&self) -> bool {
        self.control_lag_ms >= 1_000 || self.memory_full_basis_points.is_some_and(|x| x >= 1_000)
    }
    fn constrained(&self) -> bool {
        self.memory == MemoryPressure::Warning
            || self.pageout_mib_per_second >= 16
            || self.swap_growth_mib_10s >= 64
            || self.control_lag_ms >= 200
            || self.memory_full_basis_points.is_some_and(|x| x >= 200)
    }
    fn clear(&self) -> bool {
        self.memory == MemoryPressure::Normal
            && self.pageout_mib_per_second < 8
            && self.swap_growth_mib_10s < 32
            && self.control_lag_ms < 100
            && self.memory_full_basis_points.is_none_or(|x| x < 100)
            && self.disk_available_bytes > disk_clear_threshold(self.disk_capacity_bytes)
    }
}

#[derive(Debug)]
pub struct PressureController {
    state: PressureState,
    last: Option<ObservationTime>,
    soft_count: u8,
    hard_count: u8,
    clear_since: Option<u64>,
}

impl Default for PressureController {
    fn default() -> Self {
        Self {
            state: PressureState::Critical,
            last: None,
            soft_count: 0,
            hard_count: 0,
            clear_since: None,
        }
    }
}

impl PressureController {
    /// A failed or incomplete host observation closes new work immediately.
    /// Recovery then requires the ordinary 30-second valid-sample steps.
    pub fn observation_failed(&mut self) {
        self.state = PressureState::Critical;
        self.clear_since = None;
        self.soft_count = 0;
        self.hard_count = 0;
    }

    pub fn current(&mut self, now: &ObservationTime) -> PressureState {
        if self
            .last
            .as_ref()
            .is_none_or(|last| !fresh(last, now, 6_000))
        {
            self.state = PressureState::Critical;
            self.clear_since = None;
            self.soft_count = 0;
            self.hard_count = 0;
        }
        self.state
    }

    pub fn observe(
        &mut self,
        sample: PressureSample,
        now: &ObservationTime,
    ) -> Result<PressureState> {
        if !fresh(&sample.at, now, 2_000)
            || sample.disk_capacity_bytes == 0
            || sample.disk_available_bytes > sample.disk_capacity_bytes
            || sample.memory_full_basis_points.is_some_and(|x| x > 10_000)
        {
            self.state = PressureState::Critical;
            self.clear_since = None;
            return Err(Error::new(
                ErrorCode::ResourceControlUnavailable,
                "invalid or stale host sample",
            ));
        }
        if let Some(last) = &self.last {
            if last.boot_id != sample.at.boot_id || sample.at.monotonic_ms <= last.monotonic_ms {
                self.state = PressureState::Critical;
                self.clear_since = None;
                return Err(Error::new(
                    ErrorCode::ResourceControlUnavailable,
                    "host sample clock is not monotonic",
                ));
            }
            self.current(now);
        } else if !sample.critical_now() && !sample.critical_repeated() && !sample.constrained() {
            // First valid healthy observation establishes startup readiness.
            self.state = PressureState::Normal;
        }
        self.last = Some(sample.at.clone());
        self.soft_count = if sample.constrained() {
            self.soft_count.saturating_add(1)
        } else {
            0
        };
        self.hard_count = if sample.critical_repeated() {
            self.hard_count.saturating_add(1)
        } else {
            0
        };
        if sample.critical_now() || self.hard_count >= 2 {
            self.state = PressureState::Critical;
            self.clear_since = None;
        } else if self.soft_count >= 2 {
            if self.state == PressureState::Normal {
                self.state = PressureState::Constrained;
            }
            self.clear_since = None;
        } else if sample.clear() {
            let start = *self.clear_since.get_or_insert(sample.at.monotonic_ms);
            if sample.at.monotonic_ms - start >= 30_000 {
                self.state = match self.state {
                    PressureState::Critical => PressureState::Constrained,
                    _ => PressureState::Normal,
                };
                self.clear_since = Some(sample.at.monotonic_ms);
            }
        } else {
            self.clear_since = None;
        }
        Ok(self.state)
    }
}
