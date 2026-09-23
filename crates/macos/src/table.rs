//! The native process table behind scope tracking: libproc listings and
//! scheduler readback, `setpriority` and `kill`. Other platforms report the
//! operations as unsupported.

use crate::process::Presence;
use crate::scope::{Priorities, ProcessTable};
use devguard_contract::Result;

pub(crate) struct NativeTable;

#[cfg(target_os = "macos")]
impl NativeTable {
    /// List PIDs for a libproc filter, growing the buffer until the result is
    /// known to be complete. libproc reports failure as a zero-length result
    /// with errno set, so an error is never mistaken for an empty listing, and
    /// a result that fills the buffer may be truncated.
    fn list(kind: u32, argument: u32) -> Result<Vec<u32>> {
        let mut capacity = 256usize;
        while capacity <= 65_536 {
            let mut pids = vec![0 as libc::pid_t; capacity];
            let bytes = (capacity * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
            crate::ffi::clear_errno();
            // SAFETY: the buffer holds `bytes` bytes of pid_t values.
            let written =
                unsafe { libc::proc_listpids(kind, argument, pids.as_mut_ptr().cast(), bytes) };
            if written < 0 || (written == 0 && crate::ffi::errno() != 0) {
                return Err(crate::failed("process listing failed"));
            }
            if written < bytes {
                let count = written as usize / std::mem::size_of::<libc::pid_t>();
                return Ok(pids[..count]
                    .iter()
                    .filter(|pid| **pid > 0)
                    .map(|pid| *pid as u32)
                    .collect());
            }
            capacity *= 4;
        }
        Err(crate::failed("process listing exceeded its bound"))
    }
}

#[cfg(target_os = "macos")]
impl ProcessTable for NativeTable {
    fn presence(&self, pid: u32) -> Result<Presence> {
        crate::process::presence(pid)
    }

    fn group_members(&self, pgid: u32) -> Result<Vec<u32>> {
        Self::list(crate::ffi::PROC_PGRP_ONLY, pgid)
    }

    fn children(&self, pid: u32) -> Result<Vec<u32>> {
        Self::list(crate::ffi::PROC_PPID_ONLY, pid)
    }

    fn priorities(&self, pid: u32) -> Result<Priorities> {
        let native =
            libc::pid_t::try_from(pid).map_err(|_| crate::failed("process ID out of range"))?;
        // SAFETY: proc_taskinfo is a plain C output structure.
        let mut task: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        if native <= 0 {
            return Err(crate::failed("process ID out of range"));
        }
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        // SAFETY: the buffer is exactly `size` bytes.
        let written = unsafe {
            libc::proc_pidinfo(
                native,
                libc::PROC_PIDTASKINFO,
                0,
                (&mut task as *mut libc::proc_taskinfo).cast(),
                size,
            )
        };
        if written != size {
            return Err(crate::failed("task scheduler observation failed"));
        }
        let capacity = usize::try_from(task.pti_threadnum).unwrap_or(0) + 16;
        let mut threads = vec![0u64; capacity];
        let bytes = (capacity * std::mem::size_of::<u64>()) as libc::c_int;
        // SAFETY: the buffer holds `bytes` bytes of thread handles.
        let listed = unsafe {
            libc::proc_pidinfo(
                native,
                crate::ffi::PROC_PIDLISTTHREADS,
                0,
                threads.as_mut_ptr().cast(),
                bytes,
            )
        };
        if listed <= 0 || listed >= bytes {
            return Err(crate::failed("thread listing failed or was truncated"));
        }
        let mut observed = 0u32;
        let mut max_thread_priority = i32::MIN;
        for handle in &threads[..listed as usize / std::mem::size_of::<u64>()] {
            // SAFETY: proc_threadinfo is a plain C output structure.
            let mut thread: libc::proc_threadinfo = unsafe { std::mem::zeroed() };
            let size = std::mem::size_of::<libc::proc_threadinfo>() as libc::c_int;
            // SAFETY: the buffer is exactly `size` bytes.
            let written = unsafe {
                libc::proc_pidinfo(
                    native,
                    libc::PROC_PIDTHREADINFO,
                    *handle,
                    (&mut thread as *mut libc::proc_threadinfo).cast(),
                    size,
                )
            };
            // A thread that exited after the listing has no priority to read.
            if written == size {
                observed += 1;
                max_thread_priority = max_thread_priority.max(thread.pth_maxpriority);
            }
        }
        if observed == 0 {
            return Err(crate::failed("no thread scheduler state was observed"));
        }
        Ok(Priorities {
            task_priority: task.pti_priority,
            max_thread_priority,
            threads: observed,
        })
    }

    fn renice(&self, pid: u32, nice: i32) -> Result<()> {
        // PID 0 would address the authority itself.
        if pid == 0 || libc::pid_t::try_from(pid).is_err() {
            return Err(crate::failed("process ID out of range"));
        }
        // SAFETY: setpriority only changes the scheduling priority of `pid`.
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) } != 0 {
            return Err(crate::failed("priority change was refused"));
        }
        Ok(())
    }

    fn signal(&self, pid: u32, signal: i32) -> Result<()> {
        let native =
            libc::pid_t::try_from(pid).map_err(|_| crate::failed("process ID out of range"))?;
        if native <= 0 {
            return Err(crate::failed(
                "refusing to signal a process group or all processes",
            ));
        }
        // SAFETY: a positive PID addresses exactly one process.
        if unsafe { libc::kill(native, signal) } != 0 {
            return Err(crate::failed("signal was not delivered"));
        }
        Ok(())
    }

    fn effective_uid(&self) -> u32 {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() }
    }

    fn own_pid(&self) -> u32 {
        std::process::id()
    }
}

#[cfg(not(target_os = "macos"))]
impl ProcessTable for NativeTable {
    fn presence(&self, _: u32) -> Result<Presence> {
        Err(crate::unsupported())
    }
    fn group_members(&self, _: u32) -> Result<Vec<u32>> {
        Err(crate::unsupported())
    }
    fn children(&self, _: u32) -> Result<Vec<u32>> {
        Err(crate::unsupported())
    }
    fn priorities(&self, _: u32) -> Result<Priorities> {
        Err(crate::unsupported())
    }
    fn renice(&self, _: u32, _: i32) -> Result<()> {
        Err(crate::unsupported())
    }
    fn signal(&self, _: u32, _: i32) -> Result<()> {
        Err(crate::unsupported())
    }
    fn effective_uid(&self) -> u32 {
        u32::MAX
    }
    fn own_pid(&self) -> u32 {
        std::process::id()
    }
}
