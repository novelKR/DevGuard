//! Raw host observations. Interpretation into pressure samples happens in
//! `sampler`; this module only reads kernel counters and reports failures.

use devguard_contract::Result;
use serde::Serialize;
use std::path::PathBuf;

/// Observed hardware capacity, before any headroom or reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HostCapacity {
    pub logical_cpus: u32,
    pub memory_bytes: u64,
}

impl HostCapacity {
    #[cfg(target_os = "macos")]
    pub fn observe() -> Result<Self> {
        // SAFETY: both sysctls return plain integers of these sizes.
        let logical_cpus: i32 = unsafe { crate::ffi::sysctl_value(c"hw.logicalcpu")? };
        // SAFETY: as above.
        let memory_bytes: u64 = unsafe { crate::ffi::sysctl_value(c"hw.memsize")? };
        let logical_cpus = u32::try_from(logical_cpus)
            .ok()
            .filter(|cpus| *cpus > 0)
            .ok_or_else(|| crate::failed("logical CPU count is invalid"))?;
        if memory_bytes == 0 {
            return Err(crate::failed("physical memory size is invalid"));
        }
        Ok(Self {
            logical_cpus,
            memory_bytes,
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn observe() -> Result<Self> {
        Err(crate::unsupported())
    }
}

/// One observed volume, identified by its mount point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VolumeReading {
    pub mount: String,
    pub capacity_bytes: u64,
    pub available_bytes: u64,
}

/// Cumulative counters and levels at one instant. Rates need two readings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostReading {
    /// `kern.memorystatus_vm_pressure_level`: 1 normal, 2 warning, 4 critical.
    pub memory_level: u32,
    /// Pages written out since boot (`pageouts + swapouts`) times the kernel page size.
    pub paged_out_bytes: u64,
    pub swap_used_bytes: u64,
    pub volumes: Vec<VolumeReading>,
}

/// A source of host readings. Tests inject readings; the daemon uses `NativeProbe`.
pub trait HostProbe: Send {
    fn read(&mut self) -> Result<HostReading>;
}

/// Kernel counters plus `statfs` for each configured workload/state path.
/// Any failed path fails the whole reading; no volume is silently dropped.
pub struct NativeProbe {
    #[cfg(target_os = "macos")]
    paths: Vec<PathBuf>,
    #[cfg(target_os = "macos")]
    page_bytes: u64,
}

impl NativeProbe {
    #[cfg(target_os = "macos")]
    pub fn new(paths: Vec<PathBuf>) -> Result<Self> {
        if paths.is_empty() {
            return Err(crate::failed("host probe requires at least one volume"));
        }
        // Host VM counters count kernel pages, which can differ from this
        // process's page size (for example under translation).
        let page_bytes = crate::ffi::kernel_page_size()?;
        Ok(Self { paths, page_bytes })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn new(_: Vec<PathBuf>) -> Result<Self> {
        Err(crate::unsupported())
    }
}

#[cfg(target_os = "macos")]
impl HostProbe for NativeProbe {
    fn read(&mut self) -> Result<HostReading> {
        // SAFETY: the level is a plain integer and xsw_usage a plain C structure.
        let memory_level: i32 =
            unsafe { crate::ffi::sysctl_value(c"kern.memorystatus_vm_pressure_level")? };
        // SAFETY: as above.
        let swap: libc::xsw_usage = unsafe { crate::ffi::sysctl_value(c"vm.swapusage")? };
        // SAFETY: vm_statistics64 is a plain C output structure.
        let mut vm: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        // SAFETY: `count` states the capacity of `vm` in integer_t units.
        let status = unsafe {
            libc::host_statistics64(
                crate::ffi::host_port(),
                libc::HOST_VM_INFO64,
                (&mut vm as *mut libc::vm_statistics64).cast(),
                &mut count,
            )
        };
        if status != libc::KERN_SUCCESS {
            return Err(crate::failed("host VM statistics are unavailable"));
        }
        let paged_out_bytes = vm
            .pageouts
            .checked_add(vm.swapouts)
            .and_then(|pages| pages.checked_mul(self.page_bytes))
            .ok_or_else(|| crate::failed("page-out counter overflowed"))?;
        let mut volumes: Vec<VolumeReading> = Vec::new();
        for path in &self.paths {
            let volume = statfs(path)?;
            if !volumes.iter().any(|known| known.mount == volume.mount) {
                volumes.push(volume);
            }
        }
        Ok(HostReading {
            memory_level: u32::try_from(memory_level)
                .map_err(|_| crate::failed("memory pressure level is invalid"))?,
            paged_out_bytes,
            swap_used_bytes: swap.xsu_used,
            volumes,
        })
    }
}

#[cfg(not(target_os = "macos"))]
impl HostProbe for NativeProbe {
    fn read(&mut self) -> Result<HostReading> {
        Err(crate::unsupported())
    }
}

#[cfg(target_os = "macos")]
fn statfs(path: &std::path::Path) -> Result<VolumeReading> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| crate::failed("volume path is not representable"))?;
    // SAFETY: statfs is a plain C output structure.
    let mut stats: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is NUL-terminated and `stats` is a valid output buffer.
    if unsafe { libc::statfs(name.as_ptr(), &mut stats) } != 0 {
        return Err(crate::failed("volume capacity observation failed"));
    }
    let block = u64::from(stats.f_bsize);
    let capacity_bytes = stats.f_blocks.checked_mul(block);
    let available_bytes = stats.f_bavail.checked_mul(block);
    // SAFETY: the kernel NUL-terminates f_mntonname within its fixed buffer.
    let mount = unsafe { std::ffi::CStr::from_ptr(stats.f_mntonname.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    match (capacity_bytes, available_bytes) {
        (Some(capacity_bytes), Some(available_bytes)) if !mount.is_empty() => Ok(VolumeReading {
            mount,
            capacity_bytes,
            available_bytes,
        }),
        _ => Err(crate::failed("volume capacity observation is invalid")),
    }
}
