//! Resource accounting, execution evidence and compatibility are distinct contracts.
//! This crate neither starts processes nor grants OS enforcement capabilities.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const PROTOCOL_VERSION: u32 = 1;
pub const ADMISSION_DEADLINE_MS: u64 = 250;
pub const PREPARED_TTL_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    InvalidRequest,
    AttemptConflict,
    ResourceUnavailable,
    ResourceControlUnavailable,
    ResourcePolicyUnsupported,
    InvalidTransition,
    NotFound,
    ReconciliationRequired,
    JournalInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// CPU and memory are quantities, not dedicated cores or preallocated pages.
/// Tasks count kernel tasks (including threads) when a pids controller applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub cpu_milli: u64,
    pub memory_bytes: u64,
    pub tasks: u64,
}

impl Budget {
    pub const ZERO: Self = Self {
        cpu_milli: 0,
        memory_bytes: 0,
        tasks: 0,
    };

    pub fn fits(self, limit: Self) -> bool {
        self.cpu_milli <= limit.cpu_milli
            && self.memory_bytes <= limit.memory_bytes
            && self.tasks <= limit.tasks
    }

    pub fn checked_add(self, rhs: Self) -> Result<Self> {
        let overflow = || Error::new(ErrorCode::InvalidRequest, "resource total overflow");
        Ok(Self {
            cpu_milli: self
                .cpu_milli
                .checked_add(rhs.cpu_milli)
                .ok_or_else(overflow)?,
            memory_bytes: self
                .memory_bytes
                .checked_add(rhs.memory_bytes)
                .ok_or_else(overflow)?,
            tasks: self.tasks.checked_add(rhs.tasks).ok_or_else(overflow)?,
        })
    }

    pub fn checked_mul(self, count: u32) -> Result<Self> {
        let count = u64::from(count);
        let overflow = || Error::new(ErrorCode::InvalidRequest, "resource reservation overflow");
        Ok(Self {
            cpu_milli: self.cpu_milli.checked_mul(count).ok_or_else(overflow)?,
            memory_bytes: self.memory_bytes.checked_mul(count).ok_or_else(overflow)?,
            tasks: self.tasks.checked_mul(count).ok_or_else(overflow)?,
        })
    }

    pub fn remaining_after(self, used: Self) -> Self {
        Self {
            cpu_milli: self.cpu_milli.saturating_sub(used.cpu_milli),
            memory_bytes: self.memory_bytes.saturating_sub(used.memory_bytes),
            tasks: self.tasks.saturating_sub(used.tasks),
        }
    }

    pub fn half(self) -> Self {
        Self {
            cpu_milli: self.cpu_milli / 2,
            memory_bytes: self.memory_bytes / 2,
            tasks: self.tasks / 2,
        }
    }

    pub fn validate_workload(self) -> Result<()> {
        if self.cpu_milli == 0 || self.memory_bytes == 0 || self.tasks == 0 {
            return Err(Error::new(
                ErrorCode::InvalidRequest,
                "workload quantities must be positive",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementLevel {
    Accounted,
    Cooperative,
    Kernel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLevels {
    pub cpu: EnforcementLevel,
    pub memory: EnforcementLevel,
    pub pids: EnforcementLevel,
}

impl ResourceLevels {
    pub const MACOS: Self = Self {
        cpu: EnforcementLevel::Cooperative,
        memory: EnforcementLevel::Accounted,
        pids: EnforcementLevel::Accounted,
    };
    pub const KERNEL: Self = Self {
        cpu: EnforcementLevel::Kernel,
        memory: EnforcementLevel::Kernel,
        pids: EnforcementLevel::Kernel,
    };

    pub fn satisfies(self, required: Self) -> bool {
        self.cpu >= required.cpu && self.memory >= required.memory && self.pids >= required.pids
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceIntent {
    pub profile: String,
    pub requested: Budget,
    pub minimum: ResourceLevels,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptKey {
    pub consumer_id: String,
    pub consumer_generation: String,
    pub attempt_id: String,
}

impl AttemptKey {
    pub fn validate(&self) -> Result<()> {
        for value in [
            &self.consumer_id,
            &self.consumer_generation,
            &self.attempt_id,
        ] {
            validate_id(value)?;
        }
        Ok(())
    }
}

pub fn validate_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
    {
        return Err(Error::new(
            ErrorCode::InvalidRequest,
            "identifier must contain 1..128 safe ASCII characters",
        ));
    }
    Ok(())
}

pub fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::new(
            ErrorCode::InvalidRequest,
            "expected a lowercase SHA-256 digest",
        ));
    }
    Ok(())
}

pub fn digest_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

/// The runner computes this digest locally. The authority only persists the digest.
/// The struct field order and BTreeMap order define semantic encoding version 1.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionMeaning {
    pub executable_identity: String,
    pub cwd_identity: String,
    pub argv: Vec<String>,
    pub environment_changes: BTreeMap<String, String>,
    pub tty: bool,
    pub timeout_ms: u64,
    pub resources: ResourceIntent,
}

impl ExecutionMeaning {
    pub fn digest(&self) -> Result<String> {
        if self.argv.is_empty() || self.argv[0].is_empty() || self.timeout_ms == 0 {
            return Err(Error::new(
                ErrorCode::InvalidRequest,
                "invalid execution meaning",
            ));
        }
        self.resources.requested.validate_workload()?;
        let bytes = serde_json::to_vec(&("devguard-execution-v1", self))
            .map_err(|_| Error::new(ErrorCode::InvalidRequest, "execution encoding failed"))?;
        Ok(digest_bytes(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRequest {
    pub key: AttemptKey,
    pub execution_digest: String,
    pub intent: ResourceIntent,
}

impl AdmissionRequest {
    pub fn validate(&self) -> Result<()> {
        self.key.validate()?;
        validate_digest(&self.execution_digest)?;
        validate_id(&self.intent.profile)?;
        self.intent.requested.validate_workload()
    }

    /// Include resource intent independently of the caller's execution digest.
    pub fn fingerprint(&self) -> Result<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(&(
            "devguard-admission-v1",
            &self.execution_digest,
            &self.intent,
        ))
        .map_err(|_| Error::new(ErrorCode::InvalidRequest, "admission encoding failed"))?;
        Ok(digest_bytes(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub boot_id: String,
    pub pid: u32,
    pub start_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceIdentity {
    pub instance_id: String,
    pub process: ProcessIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationTime {
    pub boot_id: String,
    pub monotonic_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    ObservedProcessGroup,
    ContainedCgroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeIdentity {
    pub kind: ScopeKind,
    /// Backend-issued identity, not an arbitrary filesystem path.
    pub scope_id: String,
    pub root: ProcessIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlMethod {
    Accounting,
    QosAndPriority,
    CgroupV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedResource {
    pub level: EnforcementLevel,
    pub method: ControlMethod,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlan {
    pub scope_kind: ScopeKind,
    pub cpu: PlannedResource,
    pub memory: PlannedResource,
    pub pids: PlannedResource,
}

impl ExecutionPlan {
    pub fn levels(&self) -> ResourceLevels {
        ResourceLevels {
            cpu: self.cpu.level,
            memory: self.memory.level,
            pids: self.pids.level,
        }
    }

    pub fn validate(&self) -> Result<()> {
        for resource in [&self.cpu, &self.memory, &self.pids] {
            let valid = match resource.method {
                ControlMethod::Accounting => resource.level == EnforcementLevel::Accounted,
                ControlMethod::QosAndPriority => resource.level == EnforcementLevel::Cooperative,
                ControlMethod::CgroupV2 => {
                    resource.level == EnforcementLevel::Kernel
                        && self.scope_kind == ScopeKind::ContainedCgroup
                }
            };
            if !valid {
                return Err(Error::new(
                    ErrorCode::ResourcePolicyUnsupported,
                    "inconsistent resource capability",
                ));
            }
        }
        if self.memory.method == ControlMethod::QosAndPriority
            || self.pids.method == ControlMethod::QosAndPriority
        {
            return Err(Error::new(
                ErrorCode::ResourcePolicyUnsupported,
                "QoS does not enforce memory or task limits",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationState {
    Planned,
    Applied,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedResources {
    pub scope: ScopeIdentity,
    pub plan: ExecutionPlan,
    pub quantities: Budget,
    pub cpu: ApplicationState,
    pub memory: ApplicationState,
    pub pids: ApplicationState,
    pub observed_at: ObservationTime,
}

impl AppliedResources {
    pub fn confirms(&self, plan: &ExecutionPlan, quantities: Budget) -> bool {
        self.plan == *plan
            && self.scope.kind == plan.scope_kind
            && self.quantities == quantities
            && self.cpu == ApplicationState::Applied
            && self.memory == ApplicationState::Applied
            && self.pids == ApplicationState::Applied
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceReservation {
    pub lease_id: String,
    pub quantities: Budget,
    pub prepared_at: ObservationTime,
    pub prepare_deadline_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptPhase {
    Denied,
    Prepared,
    LaunchCommitted,
    ScopeBound,
    RunAuthorized,
    Draining,
    Suspect,
    Released,
    Cancelled,
    Expired,
}

impl AttemptPhase {
    pub fn charged(self) -> bool {
        matches!(
            self,
            Self::Prepared
                | Self::LaunchCommitted
                | Self::ScopeBound
                | Self::RunAuthorized
                | Self::Draining
                | Self::Suspect
        )
    }
    pub fn terminal(self) -> bool {
        !self.charged()
    }
    pub fn proven_not_started(self) -> bool {
        matches!(self, Self::Denied | Self::Cancelled | Self::Expired)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub key: AttemptKey,
    pub request_fingerprint: String,
    pub owner: InstanceIdentity,
    pub policy_revision: String,
    pub phase: AttemptPhase,
    pub reservation: Option<ResourceReservation>,
    pub plan: Option<ExecutionPlan>,
    pub scope: Option<ScopeIdentity>,
    pub applied: Option<AppliedResources>,
    pub denial: Option<ErrorCode>,
    /// Sticky: an ordinary later empty-group observation cannot clear lost tracking.
    pub tracking_lost: bool,
    pub release_reason: Option<ReleaseReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseReason {
    ScopeTerminated,
    NoHelperCreated,
    PreviousBoot,
}

impl AttemptRecord {
    /// Returning a budget does not by itself establish retry safety.
    pub fn known_not_started(&self) -> bool {
        self.phase.proven_not_started()
            || (self.phase == AttemptPhase::Released
                && self.release_reason == Some(ReleaseReason::NoHelperCreated))
    }
}

/// Wire serialization is deliberate; Debug never prints a credential.
#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Result<Self> {
        validate_digest(&value)?;
        Ok(Self(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn digest(&self) -> String {
        digest_bytes(self.0.as_bytes())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    DurableAdmission,
    FencedLaunch,
    PerResourceEvidence,
    StaticControlReservations,
    MacosCooperative,
    LinuxCgroupV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub minimum_protocol: u32,
    pub maximum_protocol: u32,
    pub required: BTreeSet<Capability>,
}

impl Compatibility {
    pub fn check(&self, protocol: u32, provided: &BTreeSet<Capability>) -> Result<()> {
        if self.minimum_protocol > self.maximum_protocol
            || protocol < self.minimum_protocol
            || protocol > self.maximum_protocol
            || !self.required.is_subset(provided)
        {
            return Err(Error::new(
                ErrorCode::ResourcePolicyUnsupported,
                "protocol or required capability mismatch",
            ));
        }
        Ok(())
    }
}
