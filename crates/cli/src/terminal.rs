//! Job control for a command started from an interactive terminal. The helper
//! leads its own process group, so the CLI hands that group the terminal, as a
//! shell would: terminal keys then reach the workload directly, and it can read
//! the terminal. The CLI takes the terminal back when the workload stops or
//! ends, and mirrors a stop by stopping itself so its own shell regains control.

use std::os::fd::RawFd;

/// Standard input, when it is this process's controlling terminal.
const TERMINAL: RawFd = 0;

pub struct Terminal {
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
    /// Present when standard input is this process's controlling terminal.
    pub fn open() -> Option<Self> {
        // SAFETY: isatty and tcgetpgrp only inspect descriptor 0.
        let controlling = unsafe { libc::isatty(TERMINAL) == 1 && libc::tcgetpgrp(TERMINAL) > 0 };
        controlling.then(|| Self {
            // SAFETY: getpgrp has no preconditions.
            own_group: unsafe { libc::getpgrp() },
            handed_to: None,
        })
    }

    /// Whether the CLI's own group is the terminal's foreground group.
    pub fn in_foreground(&self) -> bool {
        // SAFETY: tcgetpgrp only inspects descriptor 0.
        unsafe { libc::tcgetpgrp(TERMINAL) == self.own_group }
    }

    /// Give the terminal to the workload group if the CLI holds it now. The
    /// caller guarantees the group's root is unreaped.
    pub fn give(&mut self, group: libc::pid_t) -> bool {
        if !self.in_foreground() {
            return false;
        }
        // SAFETY: tcsetpgrp only changes descriptor 0's foreground group.
        let given = without_ttou(|| unsafe { libc::tcsetpgrp(TERMINAL, group) } == 0);
        if given {
            self.handed_to = Some(group);
        }
        given
    }

    /// Take the terminal back if the workload group still holds it.
    pub fn take_back(&mut self) {
        if let Some(group) = self.handed_to.take() {
            // SAFETY: tcgetpgrp/tcsetpgrp only inspect or change descriptor 0.
            without_ttou(|| unsafe {
                if libc::tcgetpgrp(TERMINAL) == group {
                    libc::tcsetpgrp(TERMINAL, self.own_group);
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
