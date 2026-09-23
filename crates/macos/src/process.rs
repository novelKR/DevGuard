//! Process identity from the kernel's process table. `start_ticks` is
//! `ri_proc_start_abstime` (mach absolute time at process creation), which is
//! unchanged by `exec` and distinguishes a reused PID within one boot.

use devguard_contract::{ProcessIdentity, Result};

/// A live process observed in one consistent snapshot.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub ppid: u32,
    pub pgid: u32,
    pub uid: u32,
    pub nice: i32,
    pub start_ticks: u64,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Presence {
    Live(Snapshot),
    /// Exited but not yet reaped by its parent (a zombie).
    Exited {
        start_ticks: u64,
    },
    /// Reaped, or never existed. A reused PID appears as a different start.
    Absent,
}

/// The identity of a live process. Zombies and absent PIDs are `None`; an
/// observation the kernel refuses (for example another user's process) is an
/// error, never absence.
pub fn process_identity(boot_id: &str, pid: u32) -> Result<Option<ProcessIdentity>> {
    Ok(match presence(pid)? {
        Presence::Live(snapshot) => Some(ProcessIdentity {
            boot_id: boot_id.to_owned(),
            pid,
            start_ticks: snapshot.start_ticks,
        }),
        Presence::Exited { .. } | Presence::Absent => None,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn presence(pid: u32) -> Result<Presence> {
    let Ok(native) = libc::pid_t::try_from(pid) else {
        return Ok(Presence::Absent);
    };
    if native <= 0 {
        return Ok(Presence::Absent);
    }
    // Bracket the table snapshot with the start identity. A PID reaped and
    // reused between reads shows a different start and is observed again.
    for _ in 0..3 {
        let Some(start) = start_ticks(native)? else {
            return Ok(Presence::Absent);
        };
        let info = bsd_info(native)?;
        if start_ticks(native)? != Some(start) {
            continue;
        }
        return Ok(match info {
            Some(info) if info.pbi_pid == pid => Presence::Live(Snapshot {
                ppid: info.pbi_ppid,
                pgid: info.pbi_pgid,
                uid: info.pbi_uid,
                nice: info.pbi_nice,
                start_ticks: start,
            }),
            Some(_) => return Err(crate::failed("process table returned another PID")),
            None => Presence::Exited { start_ticks: start },
        });
    }
    Err(crate::failed("process identity changed during observation"))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn presence(_: u32) -> Result<Presence> {
    Err(crate::unsupported())
}

#[cfg(target_os = "macos")]
fn start_ticks(pid: libc::pid_t) -> Result<Option<u64>> {
    // SAFETY: rusage_info_v0 is a plain C output structure.
    let mut usage: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
    crate::ffi::clear_errno();
    // SAFETY: flavor V0 writes exactly one rusage_info_v0 into `usage`.
    let status = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V0,
            (&mut usage as *mut libc::rusage_info_v0).cast(),
        )
    };
    if status == 0 {
        if usage.ri_proc_start_abstime == 0 {
            return Err(crate::failed("process start identity is unavailable"));
        }
        return Ok(Some(usage.ri_proc_start_abstime));
    }
    match crate::ffi::errno() {
        libc::ESRCH => Ok(None),
        _ => Err(crate::failed("process observation was refused")),
    }
}

#[cfg(target_os = "macos")]
fn bsd_info(pid: libc::pid_t) -> Result<Option<libc::proc_bsdinfo>> {
    // SAFETY: proc_bsdinfo is a plain C output structure.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    crate::ffi::clear_errno();
    // SAFETY: the buffer is exactly `size` bytes; the kernel writes at most that.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if written == size {
        return Ok(Some(info));
    }
    match crate::ffi::errno() {
        // The kernel reports a zombie as ESRCH here; the caller distinguishes
        // it from a reaped PID using the still-available start identity.
        libc::ESRCH if written == 0 => Ok(None),
        _ => Err(crate::failed("process observation was refused")),
    }
}
