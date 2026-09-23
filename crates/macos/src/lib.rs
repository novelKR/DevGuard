//! macOS host evidence for the authority core: boot-relative time, PID/start
//! identity and host pressure. Observations enter the core only through its
//! `Clock` and `Backend` traits. Other platforms report the capability as
//! unsupported rather than substituting values.

mod backend;
mod clock;
#[cfg(target_os = "macos")]
mod ffi;
mod host;
mod process;
mod sampler;

pub use backend::NativeBackend;
pub use clock::BootClock;
pub use host::{HostCapacity, HostProbe, HostReading, NativeProbe, VolumeReading};
pub use process::process_identity;
pub use sampler::{SampleReceipt, Sampler, SamplerOutcome, SAMPLE_INTERVAL_MS, WINDOW_MS};

use devguard_contract::{Error, ErrorCode, Result};

/// Actual host evidence sources, opened once per authority process.
#[derive(Debug, Clone)]
pub struct NativeHost {
    clock: BootClock,
    capacity: HostCapacity,
}

impl NativeHost {
    /// Unsupported platforms return `ResourcePolicyUnsupported`; a failed
    /// observation on macOS returns `ResourceControlUnavailable`.
    pub fn open() -> Result<Self> {
        let clock = BootClock::open()?;
        let capacity = HostCapacity::observe()?;
        Ok(Self { clock, capacity })
    }

    pub fn clock(&self) -> BootClock {
        self.clock.clone()
    }

    pub fn backend(&self) -> NativeBackend {
        NativeBackend::new(self.clock.clone())
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
