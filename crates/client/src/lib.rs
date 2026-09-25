//! Versioned, bounded local client. A successful handshake is not a workload lease.
pub mod connect;
pub mod credential;
pub mod framing;
pub mod launch;
pub mod peer;
pub mod protocol;

use devguard_contract::{
    AdmissionRequest, AttemptKey, AttemptRecord, Budget, Compatibility, Error, ErrorCode,
    InstanceIdentity, LeaseView, Result, Secret,
};
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
    pub fn admit(&mut self, request: AdmissionRequest) -> Result<AttemptRecord> {
        self.attempt(Request::Admit { request })
    }
    /// A lost response leaves the launch committed without a permit here; the
    /// caller must reconcile rather than expect another grant.
    pub fn begin_launch(&mut self, key: AttemptKey) -> Result<LaunchGrant> {
        match self.call(Request::BeginLaunch { key })? {
            Response::LaunchGranted(grant) => Ok(grant),
            _ => Err(unavailable()),
        }
    }
    pub fn lookup(&mut self, key: AttemptKey) -> Result<AttemptRecord> {
        self.attempt(Request::Lookup { key })
    }
    pub fn cancel(&mut self, key: AttemptKey) -> Result<AttemptRecord> {
        self.attempt(Request::Cancel { key })
    }
    /// Report that this owner holds no helper for the grant and will start none.
    pub fn abandon_launch(
        &mut self,
        key: AttemptKey,
        reason: AbandonReason,
    ) -> Result<AttemptRecord> {
        self.attempt(Request::AbandonLaunch { key, reason })
    }
    /// Observe the attempt's scope now, before its exited root is reaped.
    pub fn observe(&mut self, key: AttemptKey) -> Result<AttemptRecord> {
        self.attempt(Request::Observe { key })
    }
    pub fn terminate(&mut self, key: AttemptKey, signal: StopSignal) -> Result<Termination> {
        match self.call(Request::Terminate { key, signal })? {
            Response::Terminated(termination) => Ok(termination),
            _ => Err(unavailable()),
        }
    }
    /// Present a launch grant as its helper. A missing or invalid response is
    /// not permission to exec.
    pub fn launch(
        &mut self,
        key: AttemptKey,
        instance_id: String,
        permit: Secret,
    ) -> Result<LaunchAuthorization> {
        match self.call(Request::Launch {
            key,
            instance_id,
            permit,
        })? {
            Response::LaunchAuthorized(authorization) => Ok(authorization),
            _ => Err(unavailable()),
        }
    }
    /// Reserve a parent lease for this registered owner. Only the first
    /// response carries the token; a replay returns the lease without it.
    pub fn admit_lease(
        &mut self,
        key: AttemptKey,
        budget: Budget,
        ttl_ms: Option<u64>,
    ) -> Result<LeaseGranted> {
        match self.call(Request::AdmitLease {
            key,
            budget,
            ttl_ms,
        })? {
            Response::LeaseGranted(granted) => Ok(granted),
            _ => Err(unavailable()),
        }
    }
    /// Admit a child under a lease, presenting its token.
    pub fn admit_child(
        &mut self,
        lease: AttemptKey,
        token: Secret,
        request: AdmissionRequest,
    ) -> Result<AttemptRecord> {
        self.attempt(Request::AdmitChild {
            lease,
            token,
            request,
        })
    }
    pub fn end_lease(&mut self, key: AttemptKey) -> Result<LeaseView> {
        match self.call(Request::EndLease { key })? {
            Response::Lease(view) => Ok(view),
            _ => Err(unavailable()),
        }
    }
    /// A lease holder's view, in a session that makes no other request.
    pub fn lease_status(&mut self, key: AttemptKey, token: Secret) -> Result<LeaseView> {
        match self.call(Request::LeaseStatus { key, token })? {
            Response::Lease(view) => Ok(view),
            _ => Err(unavailable()),
        }
    }
    /// Administrator only: close admission and cancel Prepared attempts.
    pub fn close_admission(&mut self, reason: String) -> Result<Quiescence> {
        self.quiescence_call(Request::CloseAdmission { reason })
    }
    /// Administrator only: reopen admission.
    pub fn open_admission(&mut self) -> Result<Quiescence> {
        self.quiescence_call(Request::OpenAdmission)
    }
    /// Administrator only: whether admission is open and what is charged.
    pub fn quiescence(&mut self) -> Result<Quiescence> {
        self.quiescence_call(Request::Quiescence)
    }
    fn quiescence_call(&mut self, request: Request) -> Result<Quiescence> {
        match self.call(request)? {
            Response::Quiescence(quiescence) => Ok(quiescence),
            _ => Err(unavailable()),
        }
    }
    fn attempt(&mut self, request: Request) -> Result<AttemptRecord> {
        match self.call(request)? {
            Response::Attempt(attempt) => Ok(attempt),
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
