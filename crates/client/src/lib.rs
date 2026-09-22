//! Versioned, bounded local client. A successful handshake is not a workload lease.
pub mod connect;
pub mod credential;
pub mod framing;
pub mod peer;
pub mod protocol;

use devguard_contract::{Compatibility, Error, ErrorCode, InstanceIdentity, Result};
use protocol::*;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub struct Client {
    stream: UnixStream,
    next_id: u64,
    pub hello: Hello,
}

impl Client {
    pub fn connect(path: &Path, expected_uid: u32, compatibility: Compatibility) -> Result<Self> {
        let mut stream = connect::connect_timeout(path, Duration::from_millis(FRAME_DEADLINE_MS))
            .map_err(|_| unavailable())?;
        let observed = peer::observe(&stream)?;
        if observed.uid != expected_uid {
            return Err(Error::new(
                ErrorCode::Unauthorized,
                "wrong authority peer UID",
            ));
        }
        let response = exchange(
            &mut stream,
            1,
            Request::Hello {
                compatibility: compatibility.clone(),
            },
        )?;
        let Response::Hello(hello) = response else {
            return Err(unavailable());
        };
        // Caller/authority identity is corroborated by independent OS observations.
        if hello.authority != observed
            || hello.caller.pid != std::process::id()
            || hello.caller.uid != unsafe { libc::geteuid() }
            || hello.max_frame_bytes != MAX_FRAME_BYTES
            || hello.frame_deadline_ms != FRAME_DEADLINE_MS
            || hello.max_sessions != MAX_SESSIONS
        {
            return Err(Error::new(
                ErrorCode::Unauthorized,
                "authority identity or protocol bounds mismatch",
            ));
        }
        compatibility.check(hello.protocol, &hello.capabilities)?;
        Ok(Self {
            stream,
            next_id: 2,
            hello,
        })
    }

    pub fn authenticate(&mut self, credential: CallerCredential) -> Result<SessionRole> {
        match self.call(Request::Authenticate { credential })? {
            Response::Authenticated { role } => Ok(role),
            _ => Err(unavailable()),
        }
    }
    pub fn status(&mut self) -> Result<ServiceStatus> {
        match self.call(Request::Status)? {
            Response::Status(status) => Ok(status),
            _ => Err(unavailable()),
        }
    }
    pub fn register(&mut self, instance_id: String) -> Result<InstanceIdentity> {
        match self.call(Request::Register { instance_id })? {
            Response::Registered { instance } => Ok(instance),
            _ => Err(unavailable()),
        }
    }
    fn call(&mut self, request: Request) -> Result<Response> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or_else(unavailable)?;
        exchange(&mut self.stream, id, request)
    }
}

fn unavailable() -> Error {
    Error::new(
        ErrorCode::ResourceControlUnavailable,
        "authority response unavailable or invalid; execution is not inferred",
    )
}
fn exchange(stream: &mut UnixStream, id: u64, body: Request) -> Result<Response> {
    let timeout = Duration::from_millis(FRAME_DEADLINE_MS);
    framing::write_frame(
        stream,
        &Frame {
            version: WIRE_VERSION,
            request_id: id,
            body,
        },
        timeout,
    )?;
    let reply: Frame<Response> = framing::read_frame(stream, timeout)?;
    if reply.version != WIRE_VERSION || reply.request_id != id {
        return Err(unavailable());
    }
    match reply.body {
        Response::Error(error) => Err(error.into()),
        response => Ok(response),
    }
}
