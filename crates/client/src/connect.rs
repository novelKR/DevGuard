use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

/// Bounded local connect, including a full listener backlog. The returned socket
/// is blocking with CLOEXEC; protocol framing separately supplies absolute deadlines.
pub fn connect_timeout(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid local endpoint or timeout",
        )
    };
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: sockaddr_un is a C output/address struct with a valid all-zero state.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if !path.is_absolute()
        || bytes.contains(&0)
        || bytes.len() >= address.sun_path.len()
        || timeout.is_zero()
        || timeout > Duration::from_secs(5)
    {
        return Err(invalid());
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, source) in address.sun_path.iter_mut().zip(bytes) {
        *target = *source as libc::c_char;
    }
    let length =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1) as libc::socklen_t;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = length as u8;
    }
    // SAFETY: socket creates a fresh descriptor without borrowing memory.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: socket returned an owned descriptor.
    let initial = unsafe { OwnedFd::from_raw_fd(raw) };
    // Do not occupy a missing stdin/stdout/stderr slot with an authenticated socket.
    let duplicate = unsafe { libc::fcntl(initial.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    let owned = unsafe { OwnedFd::from_raw_fd(duplicate) };
    drop(initial);
    let raw = owned.as_raw_fd();
    if unsafe { libc::fcntl(raw, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let end = Instant::now().checked_add(timeout).ok_or_else(invalid)?;
    // SAFETY: address and its native length remain valid for this connect call.
    let result =
        unsafe { libc::connect(raw, (&address as *const libc::sockaddr_un).cast(), length) };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error);
        }
        loop {
            let remaining = end
                .checked_duration_since(Instant::now())
                .filter(|r| !r.is_zero())
                .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
            let mut poll = libc::pollfd {
                fd: raw,
                events: libc::POLLOUT,
                revents: 0,
            };
            // SAFETY: one live pollfd and a bounded millisecond timeout.
            let ready = unsafe { libc::poll(&mut poll, 1, remaining.as_millis().max(1) as i32) };
            if ready < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if ready == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            if ready < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut error: libc::c_int = 0;
            let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
            if unsafe {
                libc::getsockopt(
                    raw,
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    (&mut error as *mut libc::c_int).cast(),
                    &mut size,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            if size as usize != std::mem::size_of_val(&error) {
                return Err(invalid());
            }
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error));
            }
            break;
        }
    }
    if unsafe { libc::fcntl(raw, libc::F_SETFL, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = UnixStream::from(owned);
    stream.peer_addr()?;
    Ok(stream)
}
