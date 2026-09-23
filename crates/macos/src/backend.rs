use crate::clock::BootClock;
use crate::scope::{CpuReadback, Establishment, ScopeTracker, SignalReceipt};
use crate::table::NativeTable;
use devguard_contract::*;
use devguard_core::{Backend, BindingEvidence, ScopeObservation, UnboundLaunchEvidence};
use std::sync::Arc;

/// Native evidence behind the core `Backend` trait: kernel process identity,
/// observed process-group scopes with policy readback, and termination by
/// rechecked identity. Launcher evidence is not installed, so an unbound
/// launch can never be released through this backend.
#[derive(Clone)]
pub struct NativeBackend {
    clock: BootClock,
    scopes: Arc<ScopeTracker<NativeTable>>,
}

impl std::fmt::Debug for NativeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeBackend")
            .field("boot_id", &self.clock.boot_id())
            .finish_non_exhaustive()
    }
}

impl NativeBackend {
    pub(crate) fn new(clock: BootClock) -> Self {
        Self {
            clock,
            scopes: Arc::new(ScopeTracker::new(NativeTable)),
        }
    }

    /// Establish the observed scope of a started root for a prepared launch,
    /// apply nice and read back the resulting policy. Binding must still
    /// confirm the readback; a failed application stays tracked so it can be
    /// terminated before any authorization.
    pub fn establish_scope(
        &self,
        key: &AttemptKey,
        owner: &InstanceIdentity,
        root: &ProcessIdentity,
        plan: &ExecutionPlan,
        quantities: Budget,
    ) -> Result<Establishment> {
        self.scopes
            .establish(key, owner, root, plan, quantities, &self.clock)
    }

    /// Signal the scope's rechecked identities. Delivery is not release evidence.
    pub fn signal_scope(&self, scope: &ScopeIdentity, signal: i32) -> Result<SignalReceipt> {
        self.scopes.signal(scope, signal, &self.clock)
    }

    /// Scheduler readback (nice, task and thread priorities) for a live process,
    /// for diagnostics and evidence. It applies nothing.
    pub fn scheduler_readback(&self, pid: u32) -> Result<CpuReadback> {
        self.scopes.scheduler(pid)
    }
}

/// The macOS plan: cooperative CPU through QoS and priority, accounted memory
/// and tasks, observed process-group scope. A request needing kernel limits
/// is refused by the core before any payload because this plan cannot satisfy it.
pub(crate) fn macos_plan() -> ExecutionPlan {
    let accounted = PlannedResource {
        level: EnforcementLevel::Accounted,
        method: ControlMethod::Accounting,
    };
    ExecutionPlan {
        scope_kind: ScopeKind::ObservedProcessGroup,
        cpu: PlannedResource {
            level: EnforcementLevel::Cooperative,
            method: ControlMethod::QosAndPriority,
        },
        memory: accounted.clone(),
        pids: accounted,
    }
}

fn not_installed() -> Error {
    Error::new(
        ErrorCode::ReconciliationRequired,
        "launcher evidence is not installed",
    )
}

impl Backend for NativeBackend {
    fn process_identity(&self, pid: u32) -> Result<Option<ProcessIdentity>> {
        crate::process::process_identity(self.clock.boot_id(), pid)
    }

    fn plan(&self, _: &ResourceIntent) -> Result<ExecutionPlan> {
        Ok(macos_plan())
    }

    fn binding(&self, scope: &ScopeIdentity) -> Result<BindingEvidence> {
        self.scopes.binding(scope, &self.clock)
    }

    fn observe_scope(&self, scope: &ScopeIdentity) -> Result<ScopeObservation> {
        self.scopes.observe(scope, &self.clock)
    }

    fn unbound_launch(
        &self,
        _: &AttemptKey,
        _: &InstanceIdentity,
    ) -> Result<UnboundLaunchEvidence> {
        // Only the launcher can positively rule out helper creation.
        Err(not_installed())
    }
}
