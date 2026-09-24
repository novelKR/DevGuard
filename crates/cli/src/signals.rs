//! Signals sent to the CLI are forwarded to the workload's process group, and
//! only while its root is unreaped: until the owner reaps the root, the root
//! holds its PID and with it the group ID, so neither can name an unrelated
//! process. Before a workload exists a signal cancels the wait instead.
//! A signal the CLI inherited as ignored, as under `nohup` or for a
//! background command of a non-interactive shell, stays ignored: no handler
//! is installed, and the workload inherits the same disposition.
//!
//! Handlers only write the signal number to a self-pipe; a thread does the rest.

use std::os::fd::RawFd;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// The signals the CLI forwards. A terminal's own keys reach a foreground
/// workload directly; these cover signals sent to the CLI process.
pub const FORWARDED: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

static WRITER: AtomicI32 = AtomicI32::new(-1);

extern "C" fn record(signal: libc::c_int) {
    let fd = WRITER.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = signal as u8;
        // SAFETY: write is async-signal-safe; a full pipe drops the signal,
        // which only loses a duplicate of one already pending.
        unsafe {
            libc::write(fd, (&byte as *const u8).cast(), 1);
        }
    }
}

#[derive(Default)]
struct State {
    /// The unreaped root whose group receives forwarded signals.
    root: Option<libc::pid_t>,
    /// Signals received before a root existed.
    pending: Vec<libc::c_int>,
    /// Every signal received, in order, for the receipt.
    received: Vec<libc::c_int>,
    /// Signals delivered to the group.
    forwarded: Vec<libc::c_int>,
    /// Forwarded signals inherited as ignored, left ignored.
    ignored: Vec<libc::c_int>,
}

#[derive(Clone)]
pub struct Signals {
    state: Arc<(Mutex<State>, Condvar)>,
}

fn set_cloexec_nonblocking(fd: RawFd) -> std::io::Result<()> {
    // SAFETY: fcntl only changes the flags of a descriptor this process owns.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0
            || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0
            || libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) < 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

impl Signals {
    /// Install the handlers and start the forwarding thread. Call once, before
    /// any workload exists.
    pub fn install() -> std::io::Result<Self> {
        let mut fds = [0; 2];
        // SAFETY: fds is a live two-element array.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        set_cloexec_nonblocking(fds[0])?;
        set_cloexec_nonblocking(fds[1])?;
        // The reader blocks; only the handler's writes must never block.
        // SAFETY: fds[0] is the pipe's read end owned here.
        unsafe {
            let flags = libc::fcntl(fds[0], libc::F_GETFL);
            libc::fcntl(fds[0], libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
        WRITER.store(fds[1], Ordering::Relaxed);
        let signals = Self {
            state: Arc::new((Mutex::new(State::default()), Condvar::new())),
        };
        let reader = fds[0];
        let forwarder = signals.clone();
        std::thread::Builder::new()
            .name("devguard-signals".into())
            .spawn(move || forwarder.forward(reader))?;
        for signal in FORWARDED {
            // SAFETY: the handler only performs an async-signal-safe write;
            // SA_RESTART keeps interrupted system calls transparent. The
            // current disposition is read first and an ignored one is kept.
            unsafe {
                let mut current: libc::sigaction = std::mem::zeroed();
                if libc::sigaction(signal, std::ptr::null(), &mut current) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if current.sa_sigaction == libc::SIG_IGN {
                    signals.lock().ignored.push(signal);
                    continue;
                }
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = record as *const () as libc::sighandler_t;
                action.sa_flags = libc::SA_RESTART;
                libc::sigemptyset(&mut action.sa_mask);
                if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
        Ok(signals)
    }

    /// Forwarding state without handlers or a thread, for tests that drive
    /// the wait loop: a signal is simulated with [`Signals::simulate`].
    #[cfg(test)]
    pub fn inert() -> Self {
        Self {
            state: Arc::new((Mutex::new(State::default()), Condvar::new())),
        }
    }

    /// Record `signal` as the forwarding thread would.
    #[cfg(test)]
    pub fn simulate(&self, signal: libc::c_int) {
        let mut state = self.lock();
        state.received.push(signal);
        match state.root {
            Some(_) => state.forwarded.push(signal),
            None => state.pending.push(signal),
        }
        drop(state);
        self.state.1.notify_all();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn forward(&self, reader: RawFd) {
        loop {
            let mut byte = 0u8;
            // SAFETY: byte is a live one-byte buffer.
            let read = unsafe { libc::read(reader, (&mut byte as *mut u8).cast(), 1) };
            if read == 1 {
                let signal = libc::c_int::from(byte);
                let mut state = self.lock();
                state.received.push(signal);
                match state.root {
                    Some(root) => {
                        // SAFETY: the root is unreaped while it is recorded,
                        // so its group ID cannot name another group.
                        if unsafe { libc::killpg(root, signal) } == 0 {
                            state.forwarded.push(signal);
                        }
                    }
                    None => state.pending.push(signal),
                }
                drop(state);
                self.state.1.notify_all();
            } else if read < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {
                continue;
            } else {
                return;
            }
        }
    }

    /// Record the unreaped root and deliver any signal received before it existed.
    pub fn attach(&self, root: libc::pid_t) {
        let mut state = self.lock();
        state.root = Some(root);
        for signal in std::mem::take(&mut state.pending) {
            // SAFETY: as in `forward`, the root is unreaped.
            if unsafe { libc::killpg(root, signal) } == 0 {
                state.forwarded.push(signal);
            }
        }
    }

    /// Stop forwarding. Call before reaping the root.
    pub fn detach(&self) {
        self.lock().root = None;
    }

    /// Send `signal` to the attached group, if the root is still unreaped.
    pub fn send(&self, signal: libc::c_int) -> bool {
        let state = self.lock();
        match state.root {
            // SAFETY: the root is unreaped while it is recorded.
            Some(root) => unsafe { libc::killpg(root, signal) == 0 },
            None => false,
        }
    }

    /// The first signal received before any workload existed, if any.
    pub fn cancelled(&self) -> Option<libc::c_int> {
        self.lock().pending.first().copied()
    }

    /// Sleep for `duration` unless a signal cancels the wait first.
    pub fn sleep(&self, duration: Duration) -> Option<libc::c_int> {
        let deadline = Instant::now() + duration;
        let mut state = self.lock();
        loop {
            if let Some(signal) = state.pending.first() {
                return Some(*signal);
            }
            let remaining = deadline.checked_duration_since(Instant::now())?;
            state = self
                .state
                .1
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    pub fn received(&self) -> Vec<libc::c_int> {
        self.lock().received.clone()
    }

    pub fn forwarded(&self) -> Vec<libc::c_int> {
        self.lock().forwarded.clone()
    }

    pub fn ignored(&self) -> Vec<libc::c_int> {
        self.lock().ignored.clone()
    }
}
