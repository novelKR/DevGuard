use devguard_contract::{Error, ErrorCode, Result, Secret};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

fn failed() -> Error {
    Error::new(
        ErrorCode::Unauthorized,
        "invalid, missing or expired private credential FD",
    )
}

/// One read-only carrier. Its writer is already closed; argv contains only the FD number.
pub struct CredentialHandoff(OwnedFd);
impl CredentialHandoff {
    pub fn new(secret: &Secret) -> Result<Self> {
        let (reader, mut writer) = UnixStream::pair().map_err(|_| failed())?;
        writer
            .write_all(secret.expose().as_bytes())
            .map_err(|_| failed())?;
        drop(writer);
        // Keep credentials out of stdio even if the caller has closed a standard FD.
        // SAFETY: reader is live; fcntl duplicates it to a new close-on-exec descriptor.
        let fd = unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            return Err(failed());
        }
        // SAFETY: successful F_DUPFD_CLOEXEC returned a newly owned descriptor.
        Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    /// Command owns the carrier until dropped. Only its child clears close-on-exec.
    pub fn attach(self, command: &mut Command) -> RawFd {
        let raw = self.0.as_raw_fd();
        let owned = self.0;
        // SAFETY: the post-fork callback uses only async-signal-safe fcntl and errno;
        // owned keeps the descriptor valid until spawn and is not manipulated there.
        unsafe {
            command.pre_exec(move || {
                let fd = owned.as_raw_fd();
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        raw
    }
}

/// Consume and close the inherited credential descriptor on success and every error.
///
/// # Safety
/// `fd` must be a dedicated inherited descriptor owned by this caller, with no other
/// Rust owner. It must not be a descriptor borrowed from another object/thread.
pub unsafe fn take_inherited(fd: RawFd) -> Result<Secret> {
    if fd < 3 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        return Err(failed());
    }
    // SAFETY: ownership is the caller's explicit precondition above.
    read_owned(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn read_owned(fd: OwnedFd) -> Result<Secret> {
    if fd.as_raw_fd() < 3 {
        return Err(failed());
    }
    let raw = fd.as_raw_fd();
    // SAFETY: all operations use the descriptor held by fd. CLOEXEC is restored
    // before reading any secret, and the descriptor is dropped on all paths.
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
    let status = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    if flags < 0
        || status < 0
        || unsafe { libc::fcntl(raw, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0
        || unsafe { libc::fcntl(raw, libc::F_SETFL, status | libc::O_NONBLOCK) } < 0
    {
        return Err(failed());
    }
    let mut file = File::from(fd);
    let mut bytes = [0; 65];
    let mut size = 0;
    let deadline = Instant::now() + Duration::from_millis(250);
    while size < bytes.len() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(failed)?;
        if remaining.is_zero() {
            return Err(failed());
        }
        let mut poll = libc::pollfd {
            fd: raw,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll points to one live pollfd and timeout is at most 250 ms.
        let ready = unsafe { libc::poll(&mut poll, 1, remaining.as_millis().max(1) as i32) };
        if ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if ready <= 0 {
            return Err(failed());
        }
        match file.read(&mut bytes[size..]) {
            Ok(0) => break,
            Ok(n) => size += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => return Err(failed()),
        }
    }
    if size != 64 {
        return Err(failed());
    }
    Secret::new(String::from_utf8(bytes[..size].to_vec()).map_err(|_| failed())?)
        .map_err(|_| failed())
}
