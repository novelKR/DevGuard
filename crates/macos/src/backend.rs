use crate::clock::BootClock;
use crate::scope::{CpuReadback, Establishment, ScopeTracker, SignalReceipt};
use crate::table::NativeTable;
use devguard_contract::*;
use devguard_core::{Backend, BindingEvidence, Clock, ScopeObservation, UnboundLaunchEvidence};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Owner reports kept at once; beyond this, new reports are refused rather
/// than older evidence dropped.
const NO_HELPER_REPORTS: usize = 4096;

/// Native evidence behind the core `Backend` trait: kernel process identity,
/// observed process-group scopes with policy readback, termination by
/// rechecked identity, and the launcher evidence that an unclaimed grant has
/// no helper.
#[derive(Clone)]
pub struct NativeBackend {
    clock: BootClock,
    scopes: Arc<ScopeTracker<NativeTable>>,
    /// Grants whose owner reported that no helper exists, by attempt.
    no_helper: Arc<Mutex<BTreeMap<AttemptKey, InstanceIdentity>>>,
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
            no_helper: Arc::default(),
        }
    }

    /// Record the owner's first-hand report that it holds no helper for this
    /// grant and will start none: creating the helper failed, the helper it
    /// created exited and was reaped before READY, or the grant's response
    /// never reached it. The core releases the grant as `NoHelperCreated`
    /// only while the journal also shows that no helper claimed it. A late
    /// helper is refused, because a released or suspect grant is fenced.
    pub fn record_no_helper(&self, key: &AttemptKey, owner: &InstanceIdentity) -> Result<()> {
        let mut reports = self
            .no_helper
            .lock()
            .map_err(|_| crate::failed_evidence())?;
        if reports.len() >= NO_HELPER_REPORTS && !reports.contains_key(key) {
            return Err(Error::new(
                ErrorCode::ResourceUnavailable,
                "too many unreconciled launch reports",
            ));
        }
        reports.insert(key.clone(), owner.clone());
        Ok(())
    }

    /// Whether the owner reported that this grant has no helper.
    pub fn no_helper_reported(&self, key: &AttemptKey) -> bool {
        self.no_helper
            .lock()
            .is_ok_and(|reports| reports.contains_key(key))
    }

    /// Drop a report once its attempt is released.
    pub fn forget_no_helper(&self, key: &AttemptKey) {
        if let Ok(mut reports) = self.no_helper.lock() {
            reports.remove(key);
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

    /// The identity of a process presenting a launch grant: a live process of
    /// this user created by the attempt owner, which must still be running.
    pub fn helper_process(&self, pid: u32, owner: &ProcessIdentity) -> Result<ProcessIdentity> {
        self.scopes.helper(pid, owner, &self.clock)
    }

    /// Stop tracking a scope that no attempt claimed, or whose attempt was
    /// released. A claimed scope stays tracked until its termination is observed.
    pub fn forget_scope(&self, scope: &ScopeIdentity) -> Result<()> {
        self.scopes.forget(scope)
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

fn not_reported() -> Error {
    Error::new(
        ErrorCode::ReconciliationRequired,
        "the owner has not reported that no helper exists",
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
        key: &AttemptKey,
        owner: &InstanceIdentity,
    ) -> Result<UnboundLaunchEvidence> {
        // Only the launcher side can positively rule out helper creation: the
        // owner that alone held the permit reports it, bound to its identity.
        let reported = self
            .no_helper
            .lock()
            .map_err(|_| crate::failed_evidence())?
            .get(key)
            == Some(owner);
        if !reported {
            return Err(not_reported());
        }
        Ok(UnboundLaunchEvidence {
            key: key.clone(),
            owner: owner.clone(),
            observed_at: self.clock.now(),
            helper_creation_ruled_out: true,
            no_pending_spawn: true,
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn key(attempt: &str) -> AttemptKey {
        AttemptKey {
            consumer_id: "consumer".into(),
            consumer_generation: "generation".into(),
            attempt_id: attempt.into(),
        }
    }

    fn owner(pid: u32) -> InstanceIdentity {
        InstanceIdentity {
            instance_id: format!("instance-{pid}"),
            process: ProcessIdentity {
                boot_id: "boot".into(),
                pid,
                start_ticks: 1,
            },
        }
    }

    #[test]
    fn unbound_launch_evidence_needs_the_owners_own_report() {
        let backend = NativeBackend::new(BootClock::for_tests("boot"));
        let unreported = backend.unbound_launch(&key("a"), &owner(1));
        assert_eq!(
            unreported.unwrap_err().code,
            ErrorCode::ReconciliationRequired
        );
        backend.record_no_helper(&key("a"), &owner(1)).unwrap();
        assert!(backend.no_helper_reported(&key("a")));
        // Another owner's report cannot stand in for this owner's.
        assert!(backend.unbound_launch(&key("a"), &owner(2)).is_err());
        let evidence = backend.unbound_launch(&key("a"), &owner(1)).unwrap();
        assert!(evidence.helper_creation_ruled_out && evidence.no_pending_spawn);
        assert_eq!(evidence.observed_at.boot_id, "boot");
        backend.forget_no_helper(&key("a"));
        assert!(backend.unbound_launch(&key("a"), &owner(1)).is_err());
    }

    #[test]
    fn launch_reports_are_bounded_without_dropping_evidence() {
        let backend = NativeBackend::new(BootClock::for_tests("boot"));
        for n in 0..NO_HELPER_REPORTS {
            backend
                .record_no_helper(&key(&format!("k{n}")), &owner(1))
                .unwrap();
        }
        assert_eq!(
            backend
                .record_no_helper(&key("overflow"), &owner(1))
                .unwrap_err()
                .code,
            ErrorCode::ResourceUnavailable
        );
        // A repeated report for a kept attempt is still accepted.
        backend.record_no_helper(&key("k0"), &owner(1)).unwrap();
        assert!(backend.unbound_launch(&key("k0"), &owner(1)).is_ok());
    }
}
