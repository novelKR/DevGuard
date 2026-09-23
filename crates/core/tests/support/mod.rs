use devguard_contract::*;
use devguard_core::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

pub const GIB: u64 = 1024 * 1024 * 1024;
pub type TestAuthority = Authority<FakeBackend, FakeClock>;

#[derive(Clone)]
pub struct FakeClock(pub Arc<Mutex<ObservationTime>>);
impl Clock for FakeClock {
    fn now(&self) -> ObservationTime {
        self.0.lock().unwrap().clone()
    }
}
impl FakeClock {
    pub fn advance(&self, milliseconds: u64) {
        self.0.lock().unwrap().monotonic_ms += milliseconds;
    }
    pub fn reboot(&self) {
        *self.0.lock().unwrap() = ObservationTime {
            boot_id: "boot-b".into(),
            monotonic_ms: 100,
        };
    }
}

#[derive(Default)]
pub struct FakeState {
    pub processes: BTreeMap<u32, ProcessIdentity>,
    pub bindings: BTreeMap<String, BindingEvidence>,
    pub observations: BTreeMap<String, ScopeObservation>,
    pub unbound: BTreeMap<AttemptKey, UnboundLaunchEvidence>,
    pub kernel: bool,
    /// Like a real backend, finish observing after the caller's transition began.
    pub advance_during_evidence: Option<(FakeClock, u64)>,
}

impl FakeState {
    fn observing(&self) {
        if let Some((clock, milliseconds)) = &self.advance_during_evidence {
            clock.advance(*milliseconds);
        }
    }
}
#[derive(Clone, Default)]
pub struct FakeBackend(pub Arc<Mutex<FakeState>>);
impl Backend for FakeBackend {
    fn process_identity(&self, pid: u32) -> Result<Option<ProcessIdentity>> {
        Ok(self.0.lock().unwrap().processes.get(&pid).cloned())
    }
    fn plan(&self, _: &ResourceIntent) -> Result<ExecutionPlan> {
        let kernel = self.0.lock().unwrap().kernel;
        let accounted = PlannedResource {
            level: EnforcementLevel::Accounted,
            method: ControlMethod::Accounting,
        };
        let strong = PlannedResource {
            level: EnforcementLevel::Kernel,
            method: ControlMethod::CgroupV2,
        };
        Ok(ExecutionPlan {
            scope_kind: if kernel {
                ScopeKind::ContainedCgroup
            } else {
                ScopeKind::ObservedProcessGroup
            },
            cpu: if kernel {
                strong.clone()
            } else {
                PlannedResource {
                    level: EnforcementLevel::Cooperative,
                    method: ControlMethod::QosAndPriority,
                }
            },
            memory: if kernel {
                strong.clone()
            } else {
                accounted.clone()
            },
            pids: if kernel { strong } else { accounted },
        })
    }
    fn binding(&self, scope: &ScopeIdentity) -> Result<BindingEvidence> {
        let state = self.0.lock().unwrap();
        state.observing();
        state
            .bindings
            .get(&scope.scope_id)
            .cloned()
            .ok_or_else(missing_evidence)
    }
    fn observe_scope(&self, scope: &ScopeIdentity) -> Result<ScopeObservation> {
        let state = self.0.lock().unwrap();
        state.observing();
        state
            .observations
            .get(&scope.scope_id)
            .cloned()
            .ok_or_else(missing_evidence)
    }
    fn unbound_launch(
        &self,
        key: &AttemptKey,
        _: &InstanceIdentity,
    ) -> Result<UnboundLaunchEvidence> {
        let state = self.0.lock().unwrap();
        state.observing();
        state.unbound.get(key).cloned().ok_or_else(missing_evidence)
    }
}
fn missing_evidence() -> Error {
    Error::new(
        ErrorCode::ReconciliationRequired,
        "fake backend has no affirmative evidence",
    )
}

pub struct Harness {
    pub _directory: TempDir,
    pub path: PathBuf,
    pub policy: Policy,
    pub backend: FakeBackend,
    pub clock: FakeClock,
    pub registration: Registration,
}

impl Harness {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("journal.sqlite");
        TestAuthority::initialize_journal(&path).unwrap();
        let clock = FakeClock(Arc::new(Mutex::new(ObservationTime {
            boot_id: "boot-a".into(),
            monotonic_ms: 100,
        })));
        let backend = FakeBackend::default();
        let identity = ProcessIdentity {
            boot_id: "boot-a".into(),
            pid: 100,
            start_ticks: 50,
        };
        backend
            .0
            .lock()
            .unwrap()
            .processes
            .insert(identity.pid, identity.clone());
        let credential = Secret::new("a".repeat(64)).unwrap();
        let definition = ConsumerDefinition {
            generation: "generation-1".into(),
            uid: 501,
            role: ConsumerRole::ControlService,
            credential_digest: credential.digest(),
            max_instances: 1,
            control_reservation: Budget {
                cpu_milli: 1_000,
                memory_bytes: GIB / 2,
                tasks: 128,
            },
        };
        let policy = Policy {
            revision: "policy-1".into(),
            effective_capacity: Budget {
                cpu_milli: 12_000,
                memory_bytes: 32 * GIB,
                tasks: 4096,
            },
            host_headroom: Budget {
                cpu_milli: 3_000,
                memory_bytes: 8 * GIB,
                tasks: 256,
            },
            system_reservation: Budget {
                cpu_milli: 500,
                memory_bytes: GIB / 4,
                tasks: 128,
            },
            consumers: BTreeMap::from([("codespace-runtime".into(), definition)]),
        };
        let registration = Registration {
            consumer_id: "codespace-runtime".into(),
            generation: "generation-1".into(),
            instance: InstanceIdentity {
                instance_id: "instance-1".into(),
                process: identity,
            },
            credential,
        };
        Self {
            _directory: directory,
            path,
            policy,
            backend,
            clock,
            registration,
        }
    }

    pub fn open(&self) -> TestAuthority {
        TestAuthority::open(
            &self.path,
            self.policy.clone(),
            self.backend.clone(),
            self.clock.clone(),
        )
        .unwrap()
    }
    pub fn ready(&self) -> (TestAuthority, Principal) {
        let mut authority = self.open();
        let principal = self.register(&mut authority);
        authority
            .observe_pressure(healthy(self.clock.now()))
            .unwrap();
        (authority, principal)
    }
    pub fn register(&self, authority: &mut TestAuthority) -> Principal {
        authority
            .register(
                TrustedPeer { uid: 501, pid: 100 },
                self.registration.clone(),
            )
            .unwrap()
    }
    pub fn prepare(
        &self,
        authority: &mut TestAuthority,
        principal: &Principal,
        id: &str,
    ) -> AttemptRecord {
        let record = authority.admit(principal, request(id)).unwrap();
        assert_eq!(record.phase, AttemptPhase::Prepared);
        record
    }
    pub fn bound(
        &self,
        authority: &mut TestAuthority,
        principal: &Principal,
        id: &str,
    ) -> (AttemptRecord, Secret, ScopeIdentity) {
        let record = self.prepare(authority, principal, id);
        let permit = authority
            .begin_launch(principal, &record.key)
            .unwrap()
            .permit
            .unwrap();
        let scope = self.provide_binding(&record);
        let bound = authority
            .bind_scope(principal, &record.key, &permit, &scope)
            .unwrap();
        (bound, permit, scope)
    }
    pub fn provide_binding(&self, record: &AttemptRecord) -> ScopeIdentity {
        let mut fake = self.backend.0.lock().unwrap();
        let root = ProcessIdentity {
            boot_id: self.clock.now().boot_id,
            pid: 200 + fake.bindings.len() as u32,
            start_ticks: 75,
        };
        fake.processes.insert(root.pid, root.clone());
        let scope = ScopeIdentity {
            kind: record.plan.as_ref().unwrap().scope_kind,
            scope_id: format!("scope-{}", record.key.attempt_id),
            root,
        };
        let applied = AppliedResources {
            scope: scope.clone(),
            plan: record.plan.clone().unwrap(),
            quantities: record.reservation.as_ref().unwrap().quantities,
            cpu: ApplicationState::Applied,
            memory: ApplicationState::Applied,
            pids: ApplicationState::Applied,
            observed_at: self.clock.now(),
        };
        fake.bindings.insert(
            scope.scope_id.clone(),
            BindingEvidence {
                key: record.key.clone(),
                owner: record.owner.clone(),
                applied,
            },
        );
        scope
    }
    pub fn closed(&self, scope: &ScopeIdentity) {
        self.backend.0.lock().unwrap().observations.insert(
            scope.scope_id.clone(),
            ScopeObservation {
                scope: scope.clone(),
                observed_at: self.clock.now(),
                root_reaped: true,
                empty: true,
                known_members_gone: true,
                tracking_complete: true,
                known_escape: false,
                prior_tracking_loss_resolved: false,
            },
        );
    }
}

pub fn request(id: &str) -> AdmissionRequest {
    AdmissionRequest {
        key: AttemptKey {
            consumer_id: "codespace-runtime".into(),
            consumer_generation: "generation-1".into(),
            attempt_id: id.into(),
        },
        execution_digest: digest_bytes(b"immutable command meaning"),
        intent: ResourceIntent {
            profile: "interactive".into(),
            requested: Budget {
                cpu_milli: 3_000,
                memory_bytes: 2 * GIB,
                tasks: 64,
            },
            minimum: ResourceLevels::MACOS,
        },
    }
}

pub fn healthy(at: ObservationTime) -> PressureSample {
    PressureSample {
        at,
        memory: MemoryPressure::Normal,
        pageout_mib_per_second: 0,
        swap_growth_mib_10s: 0,
        control_lag_ms: 0,
        memory_full_basis_points: None,
        disk_capacity_bytes: 100 * GIB,
        disk_available_bytes: 40 * GIB,
    }
}
