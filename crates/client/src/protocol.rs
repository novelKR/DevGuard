use devguard_contract::{Capability, Compatibility, Error, ErrorCode, InstanceIdentity, Secret};
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
    /// The server derives the process identity from its OS-observed peer.
    /// C02 deliberately refuses this operation until C03 supplies native identity.
    Register {
        instance_id: String,
    },
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
    Error(WireError),
}
