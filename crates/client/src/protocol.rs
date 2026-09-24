use devguard_contract::{
    AdmissionRequest, AttemptKey, AttemptRecord, Budget, Capability, Compatibility, Error,
    ErrorCode, InstanceIdentity, LeaseRecord, LeaseView, Secret,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const WIRE_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const FRAME_DEADLINE_MS: u64 = 250;
pub const MAX_SESSIONS: usize = 32;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame<T> {
    pub version: u32,
    pub request_id: u64,
    pub body: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerIdentity {
    pub uid: u32,
    pub pid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    Workload,
    ControlService,
    Administrator,
}

/// These credentials authenticate a caller, never a launch helper or a peer PID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallerCredential {
    Consumer {
        consumer_id: String,
        generation: String,
        secret: Secret,
    },
    Administrator {
        secret: Secret,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Request {
    Hello {
        compatibility: Compatibility,
    },
    Authenticate {
        credential: CallerCredential,
    },
    Status,
    /// The server derives the process identity from its OS-observed peer and
    /// its native start identity. A caller cannot declare either.
    Register {
        instance_id: String,
    },
    /// The requests below act for the instance registered in this session.
    Admit {
        request: AdmissionRequest,
    },
    /// Durably commits the launch. Only the first response carries the
    /// one-time helper permit; a replay returns the stored attempt without one.
    BeginLaunch {
        key: AttemptKey,
    },
    Lookup {
        key: AttemptKey,
    },
    Cancel {
        key: AttemptKey,
    },
    /// The owner reports that it holds no helper for this grant and will
    /// start none. An unclaimed grant is then released as `NoHelperCreated`;
    /// a claimed grant is reconciled through its scope instead.
    AbandonLaunch {
        key: AttemptKey,
        reason: AbandonReason,
    },
    /// Observe the attempt's scope now, for example while its exited root
    /// still holds its PID unreaped, so surviving members are tracked.
    Observe {
        key: AttemptKey,
    },
    /// Signal every rechecked identity of the attempt's scope. Delivery is
    /// not release evidence and changes no attempt phase.
    Terminate {
        key: AttemptKey,
        signal: StopSignal,
    },
    /// A launch helper presents the owner's one-time grant. This is not caller
    /// authentication: the helper's identity is observed by the authority, and
    /// the session can make no other request.
    Launch {
        key: AttemptKey,
        instance_id: String,
        permit: Secret,
    },
    /// Reserve a parent lease of `budget` from host capacity for the
    /// registered owner, fenced after `ttl_ms` if given. Sent only after the
    /// service echoed the `parent_lease` capability.
    AdmitLease {
        key: AttemptKey,
        budget: Budget,
        ttl_ms: Option<u64>,
    },
    /// Admit a child execution under a lease, presenting its token. The child
    /// is then an ordinary attempt of the registered instance.
    AdmitChild {
        lease: AttemptKey,
        token: Secret,
        request: AdmissionRequest,
    },
    /// End an owned lease: no new child, released once every child settles.
    EndLease {
        key: AttemptKey,
    },
    /// A lease holder's view, authorized by the token alone. Like a helper,
    /// a lease holder is not a caller: the session can make no other request.
    LeaseStatus {
        key: AttemptKey,
        token: Secret,
    },
}

/// A new lease with its token, returned once.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseGranted {
    pub lease: LeaseRecord,
    pub token: Option<Secret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub protocol: u32,
    pub authority: PeerIdentity,
    pub caller: PeerIdentity,
    pub capabilities: BTreeSet<Capability>,
    pub max_frame_bytes: usize,
    pub frame_deadline_ms: u64,
    pub max_sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceStatus {
    pub storage_validated: bool,
    pub registration_ready: bool,
    pub execution_ready: bool,
    pub reason: String,
    pub configuration_fingerprint: String,
}

/// Why an owner holds no helper for a grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbandonReason {
    /// Creating the helper failed, so no helper process exists.
    SpawnFailed,
    /// The helper exited and was reaped before READY.
    HelperExited,
    /// The `BeginLaunch` response carrying the permit never arrived.
    GrantNotReceived,
}

/// Signals an owner may send to its attempt's scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopSignal {
    Interrupt,
    Hangup,
    Terminate,
    Kill,
}

impl StopSignal {
    pub fn number(self) -> i32 {
        match self {
            Self::Interrupt => libc::SIGINT,
            Self::Hangup => libc::SIGHUP,
            Self::Terminate => libc::SIGTERM,
            Self::Kill => libc::SIGKILL,
        }
    }
}

/// The result of `Terminate`: how many rechecked identities were signalled,
/// and whether every target could be observed and signalled.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Termination {
    pub attempt: AttemptRecord,
    pub signalled: u32,
    pub complete: bool,
}

/// The result of `BeginLaunch`. `permit` is present only in the first response.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchGrant {
    pub attempt: AttemptRecord,
    pub permit: Option<Secret>,
}

/// The result of a helper's `Launch`. Only the first successful authorization
/// sets `may_exec`; a helper that did not receive it must not exec.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchAuthorization {
    pub attempt: AttemptRecord,
    pub may_exec: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireError {
    pub code: ErrorCode,
    pub message: String,
}
impl From<Error> for WireError {
    fn from(error: Error) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}
impl From<WireError> for Error {
    fn from(error: WireError) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "result",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Response {
    Hello(Hello),
    Authenticated { role: SessionRole },
    Status(ServiceStatus),
    Registered { instance: InstanceIdentity },
    Attempt(AttemptRecord),
    LaunchGranted(LaunchGrant),
    LaunchAuthorized(LaunchAuthorization),
    Terminated(Termination),
    LeaseGranted(LeaseGranted),
    Lease(LeaseView),
    Error(WireError),
}
