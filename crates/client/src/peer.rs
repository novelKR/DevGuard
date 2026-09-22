use crate::protocol::PeerIdentity;
use devguard_contract::{Error, ErrorCode, Result};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

fn failed() -> Error {
    Error::new(
        ErrorCode::ResourceControlUnavailable,
        "cannot observe local socket peer identity",
    )
}

#[cfg(target_os = "macos")]
pub fn observe(stream: &UnixStream) -> Result<PeerIdentity> {
    let mut uid = 0;
    let mut gid = 0;
    let mut pid: libc::pid_t = 0;
    let mut length = std::mem::size_of_val(&pid) as libc::socklen_t;
    // SAFETY: the stream owns a valid socket and outputs have the native sizes.
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0
        || unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                0,
                0x002,
                (&mut pid as *mut libc::pid_t).cast(),
                &mut length,
            )
        } != 0
        || length as usize != std::mem::size_of_val(&pid)
        || pid <= 0
    {
        return Err(failed());
    }
    // Darwin sys/un.h: SOL_LOCAL=0, LOCAL_PEERPID=0x002.
    Ok(PeerIdentity {
        uid,
        pid: pid as u32,
    })
}

#[cfg(target_os = "linux")]
pub fn observe(stream: &UnixStream) -> Result<PeerIdentity> {
    // SAFETY: ucred is an output C struct filled by SO_PEERCRED.
    let mut credential: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&credential) as libc::socklen_t;
    // SAFETY: valid socket, live output struct and correctly sized length pointer.
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credential as *mut libc::ucred).cast(),
            &mut length,
        )
    } != 0
        || length as usize != std::mem::size_of_val(&credential)
        || credential.pid <= 0
    {
        return Err(failed());
    }
    Ok(PeerIdentity {
        uid: credential.uid,
        pid: credential.pid as u32,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn observe(_: &UnixStream) -> Result<PeerIdentity> {
    Err(Error::new(
        ErrorCode::ResourcePolicyUnsupported,
        "OS peer inspection is unsupported",
    ))
}
