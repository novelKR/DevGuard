use devguard_contract::{ObservationTime, Result};
use devguard_core::Clock;

/// Boot-relative time: `kern.bootsessionuuid` plus `CLOCK_MONOTONIC_RAW`
/// (mach continuous time, which advances during sleep and ignores clock
/// adjustments). Values from different boots are never comparable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootClock {
    boot_id: String,
}

impl BootClock {
    #[cfg(target_os = "macos")]
    pub fn open() -> Result<Self> {
        let boot_id = crate::ffi::sysctl_string(c"kern.bootsessionuuid")?;
        let uuid = boot_id.len() == 36
            && boot_id.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            });
        if !uuid {
            return Err(crate::failed("boot session identity is malformed"));
        }
        read_monotonic_ms()?;
        Ok(Self { boot_id })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn open() -> Result<Self> {
        Err(crate::unsupported())
    }

    pub fn boot_id(&self) -> &str {
        &self.boot_id
    }

    #[cfg(test)]
    pub(crate) fn for_tests(boot_id: &str) -> Self {
        Self {
            boot_id: boot_id.into(),
        }
    }
}

impl Clock for BootClock {
    fn now(&self) -> ObservationTime {
        // `open` verified the clock. A later failure is an unrecoverable host
        // fault; aborting preserves the journal instead of fabricating time.
        let monotonic_ms = read_monotonic_ms().unwrap_or_else(|_| std::process::abort());
        ObservationTime {
            boot_id: self.boot_id.clone(),
            monotonic_ms,
        }
    }
}

#[cfg(target_os = "macos")]
fn read_monotonic_ms() -> Result<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a valid output structure for clock_gettime.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut time) } != 0
        || time.tv_sec < 0
        || !(0..1_000_000_000).contains(&time.tv_nsec)
    {
        return Err(crate::failed("boot-relative clock is unavailable"));
    }
    (time.tv_sec as u64)
        .checked_mul(1_000)
        .and_then(|ms| ms.checked_add(time.tv_nsec as u64 / 1_000_000))
        .ok_or_else(|| crate::failed("boot-relative clock overflowed"))
}

#[cfg(not(target_os = "macos"))]
fn read_monotonic_ms() -> Result<u64> {
    Err(crate::unsupported())
}
