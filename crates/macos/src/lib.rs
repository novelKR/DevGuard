//! macOS host evidence for the authority core: boot-relative time, PID/start
//! identity, host pressure, and observed process-group scopes with cooperative
//! CPU policy readback. Observations enter the core only through its `Clock`
//! and `Backend` traits. Other platforms report the capability as unsupported
//! rather than substituting values.

mod backend;
mod clock;
#[cfg(target_os = "macos")]
mod ffi;
mod host;
mod process;
mod root;
mod sampler;
mod scope;
mod table;

pub use backend::NativeBackend;
pub use clock::BootClock;
pub use host::{HostCapacity, HostProbe, HostReading, NativeProbe, VolumeReading};
pub use process::process_identity;
pub use root::{become_scope_root, exec_with_workload_qos};
pub use sampler::{SampleReceipt, Sampler, SamplerOutcome, SAMPLE_INTERVAL_MS, WINDOW_MS};
pub use scope::{
    CpuReadback, Establishment, SignalReceipt, UTILITY_PRIORITY_CEILING, WORKLOAD_NICE,
};

use devguard_contract::{Error, ErrorCode, Result};

/// Actual host evidence sources, opened once per authority process.
#[derive(Debug, Clone)]
pub struct NativeHost {
    clock: BootClock,
    capacity: HostCapacity,
    backend: NativeBackend,
}

impl NativeHost {
    /// Unsupported platforms return `ResourcePolicyUnsupported`; a failed
    /// observation on macOS returns `ResourceControlUnavailable`.
    pub fn open() -> Result<Self> {
        let clock = BootClock::open()?;
        let capacity = HostCapacity::observe()?;
        let backend = NativeBackend::new(clock.clone());
        Ok(Self {
            clock,
            capacity,
            backend,
        })
    }

    pub fn clock(&self) -> BootClock {
        self.clock.clone()
    }

    /// Every clone shares one scope registry, so scopes established by the
    /// launcher side are the ones the authority binds and observes.
    pub fn backend(&self) -> NativeBackend {
        self.backend.clone()
    }

    pub fn capacity(&self) -> HostCapacity {
        self.capacity
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn unsupported() -> Error {
    Error::new(
        ErrorCode::ResourcePolicyUnsupported,
        "native host evidence is unsupported on this platform",
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn failed(message: &'static str) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}
