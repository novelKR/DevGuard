use crate::clock::BootClock;
use devguard_contract::*;
use devguard_core::{Backend, BindingEvidence, ScopeObservation, UnboundLaunchEvidence};

/// Native evidence behind the core `Backend` trait. Process identity is read
/// from the kernel; scopes, applied policy and launcher evidence are not yet
/// installed, so every request for them fails without granting anything.
#[derive(Debug, Clone)]
pub struct NativeBackend {
    clock: BootClock,
}

impl NativeBackend {
    pub(crate) fn new(clock: BootClock) -> Self {
        Self { clock }
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
        "native scope evidence is not installed",
    )
}

impl Backend for NativeBackend {
    fn process_identity(&self, pid: u32) -> Result<Option<ProcessIdentity>> {
        crate::process::process_identity(self.clock.boot_id(), pid)
    }

    fn plan(&self, _: &ResourceIntent) -> Result<ExecutionPlan> {
        Ok(macos_plan())
    }

    fn binding(&self, _: &ScopeIdentity) -> Result<BindingEvidence> {
        Err(not_installed())
    }

    fn observe_scope(&self, _: &ScopeIdentity) -> Result<ScopeObservation> {
        Err(not_installed())
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
