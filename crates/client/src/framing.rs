use crate::protocol::MAX_FRAME_BYTES;
use devguard_contract::{Error, ErrorCode, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

fn unavailable() -> Error {
    Error::new(
        ErrorCode::ResourceControlUnavailable,
        "local protocol closed, unavailable or deadline exceeded",
    )
}
fn invalid() -> Error {
    Error::new(
        ErrorCode::InvalidRequest,
        "invalid or oversized protocol frame",
    )
}
fn deadline(timeout: Duration) -> Result<Instant> {
    if timeout.is_zero() || timeout > Duration::from_secs(5) {
        return Err(invalid());
    }
    Instant::now().checked_add(timeout).ok_or_else(invalid)
}
fn remaining(end: Instant) -> Result<Duration> {
    end.checked_duration_since(Instant::now())
        .filter(|t| !t.is_zero())
        .ok_or_else(unavailable)
}
fn configure_nonblocking(stream: &UnixStream) -> Result<()> {
    // SAFETY: stream retains ownership. Fcntl changes file status rather than
    // socket timeout options, so buffered data remains readable after peer close.
    let flags = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || (flags & libc::O_NONBLOCK == 0
            && unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                < 0)
    {
        return Err(unavailable());
    }
    Ok(())
}
fn ready(stream: &UnixStream, events: libc::c_short, end: Instant) -> Result<()> {
    loop {
        let timeout = remaining(end)?;
        let mut descriptor = libc::pollfd {
            fd: stream.as_raw_fd(),
            events,
            revents: 0,
        };
        // SAFETY: one live descriptor and a deadline bounded to at most five seconds.
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout.as_millis().max(1) as i32) };
        if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if result < 0 || descriptor.revents & libc::POLLNVAL != 0 {
            return Err(unavailable());
        }
        if result > 0 {
            remaining(end)?;
            // HUP can accompany buffered final data. Let recv drain it before EOF.
            return Ok(());
        }
    }
}
fn read_exact(stream: &mut UnixStream, mut bytes: &mut [u8], end: Instant) -> Result<()> {
    while !bytes.is_empty() {
        ready(stream, libc::POLLIN, end)?;
        // SAFETY: valid socket and writable slice. Per-call nonblocking avoids
        // socket-option changes after Darwin peer close (which can return EINVAL).
        let count = unsafe {
            libc::recv(
                stream.as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT,
            )
        };
        if count > 0 {
            bytes = &mut bytes[count as usize..];
        } else if count == 0
            || !matches!(
                io::Error::last_os_error().kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            )
        {
            return Err(unavailable());
        }
    }
    Ok(())
}
fn write_all(stream: &mut UnixStream, mut bytes: &[u8], end: Instant) -> Result<()> {
    while !bytes.is_empty() {
        ready(stream, libc::POLLOUT, end)?;
        // SAFETY: valid socket and readable slice; suppress SIGPIPE for callers
        // that do not inherit Rust's default signal disposition.
        let count = unsafe {
            libc::send(
                stream.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
            )
        };
        if count > 0 {
            bytes = &bytes[count as usize..];
        } else if count == 0
            || !matches!(
                io::Error::last_os_error().kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            )
        {
            return Err(unavailable());
        }
    }
    Ok(())
}

/// Reads one bounded frame. The exclusively used stream is left nonblocking.
pub fn read_frame<T: DeserializeOwned>(stream: &mut UnixStream, timeout: Duration) -> Result<T> {
    let end = deadline(timeout)?;
    configure_nonblocking(stream)?;
    let mut header = [0; 4];
    read_exact(stream, &mut header, end)?;
    let size = u32::from_be_bytes(header) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err(invalid());
    }
    let mut body = vec![0; size];
    read_exact(stream, &mut body, end)?;
    // Do not return serde diagnostics: unknown keys and invalid strings may be secrets.
    serde_json::from_slice(&body).map_err(|_| invalid())
}

/// Writes one bounded frame. The exclusively used stream is left nonblocking.
pub fn write_frame<T: Serialize>(
    stream: &mut UnixStream,
    value: &T,
    timeout: Duration,
) -> Result<()> {
    let end = deadline(timeout)?;
    configure_nonblocking(stream)?;
    let bytes = serde_json::to_vec(value).map_err(|_| invalid())?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(invalid());
    }
    write_all(stream, &(bytes.len() as u32).to_be_bytes(), end)?;
    write_all(stream, &bytes, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Frame, Request};
    use std::io::Write;

    #[test]
    fn final_frame_survives_peer_close_before_the_first_read() {
        let (mut left, mut right) = UnixStream::pair().unwrap();
        write_frame(
            &mut left,
            &serde_json::json!({"final":true}),
            Duration::from_millis(250),
        )
        .unwrap();
        drop(left);
        let result: serde_json::Value = read_frame(&mut right, Duration::from_millis(250)).unwrap();
        assert_eq!(result["final"], true);
    }

    #[test]
    fn slow_reader_cannot_extend_write_deadline() {
        let (mut left, right) = UnixStream::pair().unwrap();
        let (finished, completion) = std::sync::mpsc::channel::<()>();
        // A regression must fail, not hang the entire qualification indefinitely.
        let watchdog = std::thread::spawn(move || {
            let _ = completion.recv_timeout(Duration::from_secs(1));
            drop(right);
        });
        let small: libc::c_int = 4096;
        // SAFETY: live socket and correctly sized integer socket-option input.
        assert_eq!(
            unsafe {
                libc::setsockopt(
                    left.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_SNDBUF,
                    (&small as *const libc::c_int).cast(),
                    std::mem::size_of_val(&small) as libc::socklen_t,
                )
            },
            0
        );
        let began = Instant::now();
        let result = write_frame(
            &mut left,
            &"x".repeat(MAX_FRAME_BYTES - 2),
            Duration::from_millis(100),
        );
        let elapsed = began.elapsed();
        let _ = finished.send(());
        watchdog.join().unwrap();
        assert_eq!(
            result.unwrap_err().code,
            ErrorCode::ResourceControlUnavailable
        );
        assert!(elapsed < Duration::from_millis(800));
    }

    #[test]
    fn framing_roundtrip_and_truncated_reply_preserve_uncertainty() {
        let (mut left, mut right) = UnixStream::pair().unwrap();
        write_frame(
            &mut left,
            &serde_json::json!({"value":1}),
            Duration::from_millis(250),
        )
        .unwrap();
        let result: serde_json::Value = read_frame(&mut right, Duration::from_millis(250)).unwrap();
        assert_eq!(result["value"], 1);
        left.write_all(&10u32.to_be_bytes()).unwrap();
        left.write_all(b"{").unwrap();
        drop(left);
        assert_eq!(
            read_frame::<serde_json::Value>(&mut right, Duration::from_millis(250))
                .unwrap_err()
                .code,
            ErrorCode::ResourceControlUnavailable
        );
    }

    #[test]
    fn framing_rejects_oversize_before_reading_payload_and_redacts_invalid_input() {
        let (mut left, mut right) = UnixStream::pair().unwrap();
        left.write_all(&((MAX_FRAME_BYTES + 1) as u32).to_be_bytes())
            .unwrap();
        assert_eq!(
            read_frame::<serde_json::Value>(&mut right, Duration::from_millis(250))
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
        let (mut left, mut right) = UnixStream::pair().unwrap();
        let secret = "DO-NOT-ECHO-THIS-INPUT";
        write_frame(
            &mut left,
            &serde_json::json!({"version":1,"request_id":1,"body":{"method":secret}}),
            Duration::from_millis(250),
        )
        .unwrap();
        let error =
            read_frame::<Frame<Request>>(&mut right, Duration::from_millis(250)).unwrap_err();
        assert!(!error.to_string().contains(secret));
    }

    #[test]
    fn framing_deadline_is_for_the_whole_frame_not_each_byte() {
        let (mut left, mut right) = UnixStream::pair().unwrap();
        let writer = std::thread::spawn(move || {
            for byte in [0, 0, 0, 2, b'{', b'}'] {
                std::thread::sleep(Duration::from_millis(40));
                if left.write_all(&[byte]).is_err() {
                    break;
                }
            }
        });
        let began = Instant::now();
        assert!(read_frame::<serde_json::Value>(&mut right, Duration::from_millis(100)).is_err());
        assert!(began.elapsed() < Duration::from_secs(1));
        drop(right);
        writer.join().unwrap();
    }
}
