//! Job control for a command started from an interactive terminal. The helper
//! leads its own process group, so the CLI hands that group the terminal, as a
//! shell would: terminal keys then reach the workload directly, and it can read
//! the terminal. The CLI takes the terminal back when the workload stops or
//! ends, and mirrors a stop by stopping itself so its own shell regains control.

use std::os::fd::RawFd;

pub struct Terminal {
    /// The first standard descriptor open on this process's controlling
    /// terminal: input may be redirected while output still reaches it.
    fd: RawFd,
    own_group: libc::pid_t,
    /// The workload group currently holding the terminal, if any.
    handed_to: Option<libc::pid_t>,
}

/// Run `f` with SIGTTOU blocked in this thread, so changing the foreground
/// group from the background does not stop the CLI.
fn without_ttou<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: the sets are live locals; the previous mask is restored below.
    unsafe {
        let mut block: libc::sigset_t = std::mem::zeroed();
        let mut previous: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut block);
        libc::sigaddset(&mut block, libc::SIGTTOU);
        libc::pthread_sigmask(libc::SIG_BLOCK, &block, &mut previous);
        let result = f();
        libc::pthread_sigmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut());
        result
    }
}

impl Terminal {
    /// Present when a standard descriptor is this process's controlling
    /// terminal. `tcgetpgrp` fails for a terminal that is not controlling.
    pub fn open() -> Option<Self> {
        // SAFETY: isatty and tcgetpgrp only inspect the standard descriptors.
        let fd = (0..=2).find(|&fd| unsafe { libc::isatty(fd) == 1 && libc::tcgetpgrp(fd) > 0 })?;
        Some(Self {
            fd,
            // SAFETY: getpgrp has no preconditions.
            own_group: unsafe { libc::getpgrp() },
            handed_to: None,
        })
    }

    /// Whether the CLI's own group is the terminal's foreground group.
    pub fn in_foreground(&self) -> bool {
        // SAFETY: tcgetpgrp only inspects the terminal descriptor.
        unsafe { libc::tcgetpgrp(self.fd) == self.own_group }
    }

    /// Give the terminal to the workload group if the CLI holds it now. The
    /// caller guarantees the group's root is unreaped.
    pub fn give(&mut self, group: libc::pid_t) -> bool {
        if !self.in_foreground() {
            return false;
        }
        // SAFETY: tcsetpgrp only changes the terminal's foreground group.
        let given = without_ttou(|| unsafe { libc::tcsetpgrp(self.fd, group) } == 0);
        if given {
            self.handed_to = Some(group);
        }
        given
    }

    /// Take the terminal back if the workload group still holds it.
    pub fn take_back(&mut self) {
        if let Some(group) = self.handed_to.take() {
            // SAFETY: tcgetpgrp/tcsetpgrp only inspect or change the terminal.
            without_ttou(|| unsafe {
                if libc::tcgetpgrp(self.fd) == group {
                    libc::tcsetpgrp(self.fd, self.own_group);
                }
            });
        }
    }

    pub fn handed(&self) -> bool {
        self.handed_to.is_some()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.take_back();
    }
}
