//! Declarations the pinned libc crate lacks or marks deprecated, following the
//! macOS SDK headers, plus small checked sysctl readers.

use crate::failed;
use devguard_contract::Result;
use std::ffi::CStr;

/// `<sys/proc_info.h>` listing filters and a flavor absent from libc.
pub const PROC_PGRP_ONLY: u32 = 2;
pub const PROC_PPID_ONLY: u32 = 6;
pub const PROC_PIDLISTTHREADS: libc::c_int = 6;

/// `qos_class_t` from `<sys/qos.h>`.
pub type QosClass = libc::c_uint;
pub const QOS_CLASS_UTILITY: QosClass = 0x11;

extern "C" {
    /// `<mach/mach_init.h>`. libc deprecates its binding in favor of another crate.
    fn mach_host_self() -> libc::mach_port_t;
    /// `<mach/mach_host.h>`: the kernel's page size, which host VM counters use.
    fn host_page_size(host: libc::mach_port_t, size: *mut libc::vm_size_t) -> libc::kern_return_t;
    /// `<pthread/spawn.h>`: the QoS class a spawned image runs under, which
    /// also bounds the classes its threads may request.
    pub fn posix_spawnattr_set_qos_class_np(
        attributes: *mut libc::posix_spawnattr_t,
        class: QosClass,
    ) -> libc::c_int;
}

/// Read a fixed-size sysctl value, rejecting a short or oversized result.
///
/// # Safety
///
/// `T` must be a plain C integer or structure for which every bit pattern,
/// including all zeroes, is a valid value.
pub unsafe fn sysctl_value<T: Copy>(name: &CStr) -> Result<T> {
    // SAFETY: the caller guarantees an all-zero `T` is valid.
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<T>();
    // SAFETY: the output pointer and length describe `value`; no new value is set.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut T).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || length != std::mem::size_of::<T>() {
        return Err(failed("host sysctl observation failed"));
    }
    Ok(value)
}

/// Read a NUL-terminated sysctl string of bounded length.
pub fn sysctl_string(name: &CStr) -> Result<String> {
    let mut buffer = [0u8; 128];
    let mut length = buffer.len();
    // SAFETY: the output pointer and length describe `buffer`; no new value is set.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || length == 0 || length > buffer.len() {
        return Err(failed("host sysctl observation failed"));
    }
    let text = CStr::from_bytes_until_nul(&buffer[..length])
        .map_err(|_| failed("host sysctl string was not terminated"))?;
    text.to_str()
        .map(str::to_owned)
        .map_err(|_| failed("host sysctl string was not UTF-8"))
}

pub fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Clear errno before a libproc call that reports failure only through it.
pub fn clear_errno() {
    // SAFETY: __error returns the calling thread's errno location.
    unsafe { *libc::__error() = 0 };
}

/// The host port, taken once per process rather than once per reading.
pub fn host_port() -> libc::mach_port_t {
    static HOST: std::sync::OnceLock<libc::mach_port_t> = std::sync::OnceLock::new();
    // SAFETY: mach_host_self returns a send right to the host port.
    *HOST.get_or_init(|| unsafe { mach_host_self() })
}

/// The kernel page size in bytes.
pub fn kernel_page_size() -> Result<u64> {
    let mut size: libc::vm_size_t = 0;
    // SAFETY: `size` is a valid output for host_page_size.
    if unsafe { host_page_size(host_port(), &mut size) } != libc::KERN_SUCCESS || size == 0 {
        return Err(failed("kernel page size is unavailable"));
    }
    Ok(size as u64)
}
