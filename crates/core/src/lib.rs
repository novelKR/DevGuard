//! DG-0 authority and contract tests. No OS process launcher or daemon is installed.
//! All process identity, applied policy and exit claims require a Backend witness.

mod authority;
mod journal;
mod policy;
mod pressure;

pub use authority::{
    Authority, AuthorityStorage, InstanceRecord, LaunchDecision, LeaseGrant, Principal,
    Registration, RunDecision, TrustedPeer,
};
pub use journal::JOURNAL_SCHEMA;
pub use policy::{ConsumerDefinition, ConsumerRole, Policy};
pub use pressure::{
    DiskObservation, MemoryPressure, PressureController, PressureSample, PressureState,
};

use devguard_contract::*;

/// Must use one boot-relative monotonic clock, never process-relative Instant values.
pub trait Clock {
    fn now(&self) -> ObservationTime;
}

/// Implemented by an OS adapter in DG-1/DG-LINUX. The fake test backend is not
/// proof that a real OS control is supported or that a workload has terminated.
pub trait Backend {
    fn process_identity(&self, pid: u32) -> Result<Option<ProcessIdentity>>;
    fn plan(&self, intent: &ResourceIntent) -> Result<ExecutionPlan>;
    fn binding(&self, scope: &ScopeIdentity) -> Result<BindingEvidence>;
    fn observe_scope(&self, scope: &ScopeIdentity) -> Result<ScopeObservation>;
    /// Positive, identity-bound evidence from the launcher that no helper was
    /// created and no pending spawn can still create one. A missing PID is insufficient.
    fn unbound_launch(
        &self,
        key: &AttemptKey,
        owner: &InstanceIdentity,
    ) -> Result<UnboundLaunchEvidence>;
}

#[derive(Debug, Clone)]
pub struct UnboundLaunchEvidence {
    pub key: AttemptKey,
    pub owner: InstanceIdentity,
    pub observed_at: ObservationTime,
    pub helper_creation_ruled_out: bool,
    pub no_pending_spawn: bool,
}

#[derive(Debug, Clone)]
pub struct BindingEvidence {
    pub key: AttemptKey,
    pub owner: InstanceIdentity,
    pub applied: AppliedResources,
}

#[derive(Debug, Clone)]
pub struct ScopeObservation {
    pub scope: ScopeIdentity,
    pub observed_at: ObservationTime,
    pub root_reaped: bool,
    pub empty: bool,
    pub known_members_gone: bool,
    pub tracking_complete: bool,
    pub known_escape: bool,
    /// Explicit reconciliation covers the previously missing processes.
    /// An ordinary empty-group observation must leave this false.
    pub prior_tracking_loss_resolved: bool,
}

impl ScopeObservation {
    pub fn supports_release(&self, expected: &ScopeIdentity, now: &ObservationTime) -> bool {
        self.scope == *expected
            && fresh(&self.observed_at, now, 2_000)
            && self.root_reaped
            && self.empty
            && self.known_members_gone
            && self.tracking_complete
            && !self.known_escape
    }
}

fn fresh(observation: &ObservationTime, now: &ObservationTime, maximum_age: u64) -> bool {
    observation.boot_id == now.boot_id
        && now
            .monotonic_ms
            .checked_sub(observation.monotonic_ms)
            .is_some_and(|age| age <= maximum_age)
}
