use crate::journal::{self, db_error, decode, encode, Journal};
use crate::{
    fresh, Backend, Clock, ConsumerRole, Policy, PressureController, PressureSample, PressureState,
};
use devguard_contract::*;
use rusqlite::{params, OptionalExtension, Transaction};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use uuid::Uuid;

/// Supplied only by a trusted transport's peer-credential check, never JSON input.
#[derive(Debug, Clone, Copy)]
pub struct TrustedPeer {
    pub uid: u32,
    pub pid: u32,
}

#[derive(Debug, Clone)]
pub struct Registration {
    pub consumer_id: String,
    pub generation: String,
    pub instance: InstanceIdentity,
    pub credential: Secret,
}

/// Opaque to callers. Only successful registration can construct a principal.
#[derive(Debug, Clone)]
pub struct Principal {
    consumer_id: String,
    generation: String,
    instance: InstanceIdentity,
    role: ConsumerRole,
}

impl Principal {
    pub fn consumer_id(&self) -> &str {
        &self.consumer_id
    }
    pub fn instance(&self) -> &InstanceIdentity {
        &self.instance
    }
    pub fn role(&self) -> ConsumerRole {
        self.role
    }
}

/// A registered instance that is not retired. `active` is false while it is
/// suspect, for example after a restart until its owner registers again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRecord {
    pub consumer_id: String,
    pub generation: String,
    pub instance: InstanceIdentity,
    pub active: bool,
}

#[derive(Debug)]
pub struct LaunchDecision {
    pub attempt: AttemptRecord,
    /// Emitted once. Replaying the durable result never supplies another spawn grant.
    pub permit: Option<Secret>,
}

/// A newly admitted lease and its token, returned once; a replay of the same
/// admission returns the lease without the token.
#[derive(Debug, Clone)]
pub struct LeaseGrant {
    pub lease: LeaseRecord,
    pub token: Option<Secret>,
}

#[derive(Debug)]
pub struct RunDecision {
    pub attempt: AttemptRecord,
    /// Only the first successful authorization response permits the bound helper to exec.
    pub may_exec: bool,
}

pub struct Authority<B: Backend, C: Clock> {
    journal: Journal,
    policy: Policy,
    backend: B,
    clock: C,
    pressure: PressureController,
    _authority_lock: File,
}

/// Exclusive, validated storage before a real boot clock/backend is available.
/// Opening storage neither recovers attempts nor grants an execution capability.
pub struct AuthorityStorage {
    journal: Journal,
    authority_lock: File,
}

impl AuthorityStorage {
    fn lock(path: &Path) -> Result<File> {
        let parent = path.parent().ok_or_else(|| {
            Error::new(
                ErrorCode::JournalInvalid,
                "journal requires an authority directory",
            )
        })?;
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .mode(0o600)
            .open(parent.join("authority.lock"))
            .map_err(|_| Error::new(ErrorCode::JournalInvalid, "cannot open authority lock"))?;
        let metadata = lock
            .metadata()
            .map_err(|_| Error::new(ErrorCode::JournalInvalid, "cannot observe authority lock"))?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(Error::new(
                ErrorCode::JournalInvalid,
                "authority lock must be a private owned regular file",
            ));
        }
        // SAFETY: flock operates on the valid File descriptor; File retains ownership.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Error::new(
                ErrorCode::ResourceControlUnavailable,
                "another authority owns this state directory",
            ));
        }
        Ok(lock)
    }

    pub fn open(path: &Path) -> Result<Self> {
        Self::open_locked(path, Self::lock(path)?)
    }

    /// Explicit first bootstrap only. Existing/missing/corrupt state is never reset.
    pub fn initialize(path: &Path) -> Result<Self> {
        let lock = Self::lock(path)?;
        Journal::initialize(path)?;
        Self::open_locked(path, lock)
    }

    fn open_locked(path: &Path, authority_lock: File) -> Result<Self> {
        let mut journal = Journal::open(path)?;
        journal.transaction(journal::validate_index)?;
        Ok(Self {
            journal,
            authority_lock,
        })
    }

    /// What the idle journal still charges: attempts not yet settled and
    /// leases not yet released. It is read without activating the journal
    /// and changes nothing.
    pub fn charged(&mut self) -> Result<(Vec<AttemptRecord>, Vec<LeaseRecord>)> {
        self.journal.transaction(|tx| {
            let attempts = journal::active(tx)?;
            let leases = if journal::lease_tables_exist(tx)? {
                journal::validate_leases(tx)?;
                journal::active_leases(tx)?
            } else {
                Vec::new()
            };
            Ok((attempts, leases))
        })
    }

    /// Copy the journal, complete and consistent, into the new empty private
    /// file at `path`, while this storage holds the authority lock.
    pub fn backup(&mut self, path: &Path) -> Result<()> {
        self.journal.backup(path)
    }
}

impl<B: Backend, C: Clock> Authority<B, C> {
    pub fn initialize_journal(path: &Path) -> Result<()> {
        Journal::initialize(path)
    }

    pub fn open(path: &Path, policy: Policy, backend: B, clock: C) -> Result<Self> {
        policy.validate()?;
        Self::from_storage(AuthorityStorage::open(path)?, policy, backend, clock)
    }

    /// Activate exclusively held storage using actual backend/boot observations.
    pub fn from_storage(
        storage: AuthorityStorage,
        policy: Policy,
        backend: B,
        clock: C,
    ) -> Result<Self> {
        policy.validate()?;
        let AuthorityStorage {
            mut journal,
            authority_lock,
        } = storage;
        let now = clock.now();
        journal.transaction(|tx| {
            // Storage may have waited for host readiness. Recheck atomically with
            // recovery so intervening corruption cannot hide a charged attempt.
            journal::validate_index(tx)?;
            journal::ensure_lease_tables(tx)?;
            journal::validate_leases(tx)?;
            validate_registration_policies(tx, &policy)?;
            journal::expire_prepared(tx, &now)?;
            for mut record in journal::active(tx)? {
                if record.phase != AttemptPhase::Prepared {
                    record.phase = AttemptPhase::Suspect;
                    journal::save(tx, &record)?;
                }
            }
            tx.execute(
                "UPDATE instances SET state='suspect' WHERE state='active'",
                [],
            )
            .map_err(db_error)?;
            Ok(())
        })?;
        Ok(Self {
            journal,
            policy,
            backend,
            clock,
            pressure: PressureController::default(),
            _authority_lock: authority_lock,
        })
    }

    pub fn observe_pressure(&mut self, sample: PressureSample) -> Result<PressureState> {
        self.pressure.observe(sample, &self.clock.now())
    }

    pub fn pressure(&mut self) -> PressureState {
        self.pressure.current(&self.clock.now())
    }

    /// A host probe failed or was incomplete. New work closes without inventing a sample.
    pub fn pressure_observation_failed(&mut self) {
        self.pressure.observation_failed();
    }

    pub fn register(&mut self, peer: TrustedPeer, request: Registration) -> Result<Principal> {
        validate_id(&request.consumer_id)?;
        validate_id(&request.generation)?;
        validate_id(&request.instance.instance_id)?;
        let definition = self
            .policy
            .consumers
            .get(&request.consumer_id)
            .ok_or_else(unauthorized)?;
        let actual = self.backend.process_identity(peer.pid)?;
        if definition.generation != request.generation
            || definition.uid != peer.uid
            || peer.pid == 0
            || peer.pid != request.instance.process.pid
            || actual.as_ref() != Some(&request.instance.process)
            || request.instance.process.boot_id != self.clock.now().boot_id
            || !same_digest(&definition.credential_digest, &request.credential.digest())
        {
            return Err(unauthorized());
        }
        let principal = Principal {
            consumer_id: request.consumer_id,
            generation: request.generation,
            instance: request.instance,
            role: definition.role,
        };
        self.journal.transaction(|tx| {
            if retired(tx, &principal.consumer_id, &principal.generation)? { return Err(unauthorized()); }
            let existing: Option<(String, String)> = tx.query_row(
                "SELECT identity,state FROM instances WHERE consumer=?1 AND generation=?2 AND instance=?3",
                params![principal.consumer_id, principal.generation, principal.instance.instance_id],
                |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(db_error)?;
            if let Some((identity, state)) = existing {
                if decode::<InstanceIdentity>(&identity)? != principal.instance || state == "retired" {
                    return Err(Error::new(ErrorCode::AttemptConflict, "instance identity cannot be reused"));
                }
            } else {
                let count: u32 = tx.query_row(
                    "SELECT COUNT(*) FROM instances WHERE consumer=?1 AND state!='retired'",
                    params![principal.consumer_id], |r| r.get(0)).map_err(db_error)?;
                if count >= definition.max_instances {
                    return Err(Error::new(ErrorCode::ResourceUnavailable, "consumer instance pool is occupied"));
                }
            }
            tx.execute("INSERT INTO instances(consumer,generation,instance,identity,state,registration_policy) VALUES (?1,?2,?3,?4,'active',?5)
                ON CONFLICT(consumer,generation,instance) DO UPDATE SET state='active'",
                params![principal.consumer_id, principal.generation, principal.instance.instance_id, encode(&principal.instance)?, registration_policy(definition)?])
                .map_err(db_error)?;
            Ok(())
        })?;
        Ok(principal)
    }

    pub fn disconnected(&mut self, principal: &Principal) -> Result<()> {
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            tx.execute("UPDATE instances SET state='suspect' WHERE consumer=?1 AND generation=?2 AND instance=?3",
                params![principal.consumer_id, principal.generation, principal.instance.instance_id]).map_err(db_error)?;
            // Connection liveness is not evidence that any execution has ended.
            Ok(())
        })
    }

    pub fn admit(
        &mut self,
        principal: &Principal,
        request: AdmissionRequest,
    ) -> Result<AttemptRecord> {
        let fingerprint = request.fingerprint()?;
        check_key(principal, &request.key)?;
        let now = self.clock.now();
        let target = self
            .pressure
            .current(&now)
            .target(self.policy.work_capacity()?);
        let policy = &self.policy;
        let backend = &self.backend;
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            journal::expire_prepared(tx, &now)?;
            if let Some(record) = journal::load(tx, &request.key)? {
                if record.request_fingerprint != fingerprint || record.owner != principal.instance {
                    return Err(Error::new(
                        ErrorCode::AttemptConflict,
                        "attempt identity has different meaning or owner",
                    ));
                }
                return Ok(record);
            }
            let plan = backend.plan(&request.intent);
            let denial = match &plan {
                Ok(plan)
                    if request.intent.profile == "interactive"
                        && plan.validate().is_ok()
                        && plan.levels().satisfies(request.intent.minimum) =>
                {
                    if request
                        .intent
                        .requested
                        .fits(target.remaining_after(journal::committed(tx)?))
                    {
                        None
                    } else {
                        Some(ErrorCode::ResourceUnavailable)
                    }
                }
                Err(error) if error.code == ErrorCode::ResourceControlUnavailable => {
                    Some(error.code)
                }
                _ => Some(ErrorCode::ResourcePolicyUnsupported),
            };
            let record = AttemptRecord {
                key: request.key,
                request_fingerprint: fingerprint,
                owner: principal.instance.clone(),
                policy_revision: policy.revision.clone(),
                phase: if denial.is_some() {
                    AttemptPhase::Denied
                } else {
                    AttemptPhase::Prepared
                },
                reservation: if denial.is_some() {
                    None
                } else {
                    Some(ResourceReservation {
                        lease_id: Uuid::new_v4().to_string(),
                        quantities: request.intent.requested,
                        prepared_at: now.clone(),
                        prepare_deadline_ms: now
                            .monotonic_ms
                            .checked_add(PREPARED_TTL_MS)
                            .ok_or_else(|| {
                                Error::new(ErrorCode::InvalidRequest, "monotonic deadline overflow")
                            })?,
                    })
                },
                plan: if denial.is_some() { None } else { Some(plan?) },
                scope: None,
                applied: None,
                denial,
                tracking_lost: false,
                release_reason: None,
            };
            journal::save(tx, &record)?;
            Ok(record)
        })
    }

    pub fn lookup(&mut self, principal: &Principal, key: &AttemptKey) -> Result<AttemptRecord> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            journal::expire_prepared(tx, &now)?;
            owned_record(tx, principal, key)
        })
    }

    pub fn begin_launch(
        &mut self,
        principal: &Principal,
        key: &AttemptKey,
    ) -> Result<LaunchDecision> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            journal::expire_prepared(tx, &now)?;
            let mut record = owned_record(tx, principal, key)?;
            if record.phase != AttemptPhase::Prepared {
                return Ok(LaunchDecision { attempt: record, permit: None });
            }
            let permit = Secret::new(format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()))?;
            record.phase = AttemptPhase::LaunchCommitted;
            journal::save(tx, &record)?;
            tx.execute("UPDATE attempts SET launch_hash=?4 WHERE consumer=?1 AND generation=?2 AND attempt=?3",
                params![key.consumer_id, key.consumer_generation, key.attempt_id, permit.digest()]).map_err(db_error)?;
            Ok(LaunchDecision { attempt: record, permit: Some(permit) })
        })
    }

    /// Check that `permit` is this owner's launch grant without changing the
    /// attempt, so a helper can be refused before any policy is applied to it.
    pub fn verify_launch(
        &mut self,
        principal: &Principal,
        key: &AttemptKey,
        permit: &Secret,
    ) -> Result<AttemptRecord> {
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            let record = owned_record(tx, principal, key)?;
            check_permit(tx, key, permit)?;
            Ok(record)
        })
    }

    /// The first helper to present the launch grant claims it, and its scope is
    /// recorded durably before binding. A claimed helper whose application or
    /// binding fails is then reconciled through that scope, and no other scope
    /// can claim, bind or be authorized for the attempt. Presenting the same
    /// scope again returns the stored attempt.
    pub fn claim_launch(
        &mut self,
        principal: &Principal,
        key: &AttemptKey,
        permit: &Secret,
        scope: &ScopeIdentity,
    ) -> Result<AttemptRecord> {
        let backend = &self.backend;
        let clock = &self.clock;
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            let mut record = owned_record(tx, principal, key)?;
            check_permit(tx, key, permit)?;
            if record.scope.as_ref() == Some(scope) {
                return Ok(record);
            }
            // A cancelled, expired, suspect or already claimed grant is fenced.
            if record.phase != AttemptPhase::LaunchCommitted || record.scope.is_some() {
                return Err(invalid_transition());
            }
            let kind = record
                .plan
                .as_ref()
                .ok_or_else(invalid_transition)?
                .scope_kind;
            if scope.kind != kind
                || scope.root.boot_id != clock.now().boot_id
                || backend.process_identity(scope.root.pid)?.as_ref() != Some(&scope.root)
            {
                return Err(Error::new(
                    ErrorCode::ResourcePolicyUnsupported,
                    "helper scope was not verified",
                ));
            }
            record.scope = Some(scope.clone());
            journal::save(tx, &record)?;
            Ok(record)
        })
    }

    pub fn bind_scope(
        &mut self,
        principal: &Principal,
        key: &AttemptKey,
        permit: &Secret,
        scope: &ScopeIdentity,
    ) -> Result<AttemptRecord> {
        let backend = &self.backend;
        let clock = &self.clock;
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            let mut record = owned_record(tx, principal, key)?;
            check_permit(tx, key, permit)?;
            if record.phase == AttemptPhase::ScopeBound && record.scope.as_ref() == Some(scope) {
                return Ok(record);
            }
            // A grant claimed by one helper can never bind another scope.
            if record.phase != AttemptPhase::LaunchCommitted
                || record
                    .scope
                    .as_ref()
                    .is_some_and(|claimed| claimed != scope)
            {
                return Err(invalid_transition());
            }
            let evidence = backend.binding(scope)?;
            // Judge freshness after the backend returns: evidence observed during
            // this call is not from the future, and stale evidence stays rejected.
            let now = clock.now();
            let plan = record.plan.as_ref().ok_or_else(invalid_transition)?;
            let quantities = record
                .reservation
                .as_ref()
                .ok_or_else(invalid_transition)?
                .quantities;
            if evidence.key != *key
                || evidence.owner != record.owner
                || evidence.applied.scope != *scope
                || scope.root.boot_id != now.boot_id
                || backend.process_identity(scope.root.pid)?.as_ref() != Some(&scope.root)
                || !fresh(&evidence.applied.observed_at, &now, 2_000)
                || !evidence.applied.confirms(plan, quantities)
            {
                return Err(Error::new(
                    ErrorCode::ResourcePolicyUnsupported,
                    "scope binding or applied policy was not verified",
                ));
            }
            record.scope = Some(scope.clone());
            record.applied = Some(evidence.applied);
            record.phase = AttemptPhase::ScopeBound;
            journal::save(tx, &record)?;
            Ok(record)
        })
    }

    pub fn authorize_run(
        &mut self,
        principal: &Principal,
        key: &AttemptKey,
        permit: &Secret,
        helper: &ProcessIdentity,
    ) -> Result<RunDecision> {
        let backend = &self.backend;
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            let mut record = owned_record(tx, principal, key)?;
            check_permit(tx, key, permit)?;
            if record.scope.as_ref().map(|s| &s.root) != Some(helper)
                || backend.process_identity(helper.pid)?.as_ref() != Some(helper)
            {
                return Err(unauthorized());
            }
            if record.phase == AttemptPhase::RunAuthorized {
                return Ok(RunDecision {
                    attempt: record,
                    may_exec: false,
                });
            }
            if record.phase != AttemptPhase::ScopeBound {
                return Err(invalid_transition());
            }
            record.phase = AttemptPhase::RunAuthorized;
            journal::save(tx, &record)?;
            Ok(RunDecision {
                attempt: record,
                may_exec: true,
            })
        })
    }

    /// Cancellation fences late helpers before reclamation. Post-commit cancellation
    /// keeps the reservation until a separately verified scope termination.
    ///
    /// Draining is already fenced, Suspect stays Suspect until reconciliation
    /// settles it and a terminal attempt stays terminal: none is rewritten.
    pub fn cancel(&mut self, principal: &Principal, key: &AttemptKey) -> Result<AttemptRecord> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            journal::expire_prepared(tx, &now)?;
            let mut record = owned_record(tx, principal, key)?;
            let before = record.phase;
            record.phase = match before {
                AttemptPhase::Prepared => AttemptPhase::Cancelled,
                AttemptPhase::LaunchCommitted
                | AttemptPhase::ScopeBound
                | AttemptPhase::RunAuthorized => AttemptPhase::Draining,
                unchanged => unchanged,
            };
            if record.phase != before {
                journal::save(tx, &record)?;
            }
            Ok(record)
        })
    }

    /// Trusted path for closing admission before an upgrade: cancel every
    /// Prepared attempt, which is known not to have started. Launched work is
    /// left to finish and be reconciled.
    pub fn cancel_prepared(&mut self) -> Result<Vec<AttemptRecord>> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            journal::expire_prepared(tx, &now)?;
            let mut cancelled = Vec::new();
            for mut record in journal::active(tx)? {
                if record.phase == AttemptPhase::Prepared {
                    record.phase = AttemptPhase::Cancelled;
                    journal::save(tx, &record)?;
                    cancelled.push(record);
                }
            }
            Ok(cancelled)
        })
    }

    /// Trusted reconciliation path; it is not an unauthenticated client RPC.
    /// There is deliberately no `release(lease_id)` operation without evidence.
    pub fn reconcile(&mut self, key: &AttemptKey) -> Result<AttemptRecord> {
        key.validate()?;
        let now = self.clock.now();
        let backend = &self.backend;
        let clock = &self.clock;
        self.journal.transaction(|tx| {
            journal::expire_prepared(tx, &now)?;
            let mut record = journal::load(tx, key)?.ok_or_else(not_found)?;
            if record.phase.terminal() || record.phase == AttemptPhase::Prepared {
                return Ok(record);
            }
            let before = record.clone();
            match &record.scope {
                // Every process of an earlier boot has ended, including any
                // member whose tracking was lost; the history is kept.
                Some(scope) if scope.root.boot_id != now.boot_id => {
                    record.phase = AttemptPhase::Released;
                    record.release_reason = Some(ReleaseReason::PreviousBoot);
                }
                Some(scope) => match backend.observe_scope(scope) {
                    Ok(observation) => {
                        // Freshness is judged after the observation completes.
                        let now = clock.now();
                        let loss_resolved = !record.tracking_lost
                            || observation.prior_tracking_loss_resolved
                            || scope.root.boot_id != now.boot_id;
                        if loss_resolved && observation.supports_release(scope, &now) {
                            record.phase = AttemptPhase::Released;
                            record.release_reason = Some(ReleaseReason::ScopeTerminated);
                        } else if observation.known_escape || !observation.tracking_complete {
                            record.phase = AttemptPhase::Suspect;
                            record.tracking_lost = true;
                        } else if !fresh(&observation.observed_at, &now, 2_000)
                            || observation.scope != *scope
                        {
                            record.phase = AttemptPhase::Suspect;
                        }
                    }
                    Err(_) => {
                        record.phase = AttemptPhase::Suspect;
                        record.tracking_lost = true;
                    }
                },
                None => {
                    // No registered scope is ambiguous. Only a reboot or positive
                    // launcher evidence can resolve it; owner death/timeout cannot.
                    let previous_boot = record
                        .reservation
                        .as_ref()
                        .is_some_and(|r| r.prepared_at.boot_id != now.boot_id);
                    let ruled_out =
                        backend
                            .unbound_launch(key, &record.owner)
                            .is_ok_and(|evidence| {
                                evidence.key == *key
                                    && evidence.owner == record.owner
                                    && fresh(&evidence.observed_at, &clock.now(), 2_000)
                                    && evidence.helper_creation_ruled_out
                                    && evidence.no_pending_spawn
                            });
                    record.phase = if previous_boot || ruled_out {
                        AttemptPhase::Released
                    } else {
                        AttemptPhase::Suspect
                    };
                    if previous_boot {
                        record.release_reason = Some(ReleaseReason::PreviousBoot);
                    } else if ruled_out {
                        record.release_reason = Some(ReleaseReason::NoHelperCreated);
                    }
                }
            }
            // A periodic pass changes nothing most of the time; skip the write.
            if record != before {
                journal::save(tx, &record)?;
            }
            Ok(record)
        })
    }

    /// Charged attempts, after expiring Prepared attempts past their deadline.
    /// For the trusted reconciler; it changes no other attempt.
    pub fn attempts(&mut self) -> Result<Vec<AttemptRecord>> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            journal::expire_prepared(tx, &now)?;
            journal::active(tx)
        })
    }

    /// Instances that are not retired, with their registered identities.
    pub fn instances(&mut self) -> Result<Vec<InstanceRecord>> {
        self.journal.transaction(|tx| {
            let mut statement = tx
                .prepare("SELECT consumer,generation,identity,state FROM instances WHERE state!='retired'")
                .map_err(db_error)?;
            let mut rows = statement.query([]).map_err(db_error)?;
            let mut instances = Vec::new();
            while let Some(row) = rows.next().map_err(db_error)? {
                let state: String = row.get(3).map_err(db_error)?;
                instances.push(InstanceRecord {
                    consumer_id: row.get(0).map_err(db_error)?,
                    generation: row.get(1).map_err(db_error)?,
                    instance: decode(&row.get::<_, String>(2).map_err(db_error)?)?,
                    active: match state.as_str() {
                        "active" => true,
                        "suspect" => false,
                        _ => {
                            return Err(Error::new(
                                ErrorCode::JournalInvalid,
                                "invalid instance state",
                            ))
                        }
                    },
                });
            }
            Ok(instances)
        })
    }

    pub fn committed_budget(&mut self) -> Result<Budget> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            journal::expire_prepared(tx, &now)?;
            journal::committed(tx)
        })
    }

    pub fn available_budget(&mut self) -> Result<Budget> {
        let target = self
            .pressure
            .current(&self.clock.now())
            .target(self.policy.work_capacity()?);
        Ok(target.remaining_after(self.committed_budget()?))
    }

    /// Trusted operator/daemon reconciliation, not caller-declared death.
    pub fn reconcile_instance(
        &mut self,
        consumer: &str,
        generation: &str,
        instance: &str,
    ) -> Result<bool> {
        let backend = &self.backend;
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            let raw: String = tx.query_row("SELECT identity FROM instances WHERE consumer=?1 AND generation=?2 AND instance=?3",
                params![consumer, generation, instance], |r| r.get(0)).optional().map_err(db_error)?.ok_or_else(not_found)?;
            let identity: InstanceIdentity = decode(&raw)?;
            // A process of an earlier boot has ended, whatever holds its PID now.
            let alive = identity.process.boot_id == now.boot_id
                && backend.process_identity(identity.process.pid)?.as_ref() == Some(&identity.process);
            let occupied = journal::active(tx)?.iter().any(|r| r.key.consumer_id == consumer
                && r.key.consumer_generation == generation && r.owner == identity);
            let retired = !alive && !occupied;
            tx.execute("UPDATE instances SET state=?4 WHERE consumer=?1 AND generation=?2 AND instance=?3",
                params![consumer, generation, instance, if retired { "retired" } else { "suspect" }]).map_err(db_error)?;
            Ok(retired)
        })
    }

    /// Operator-only generation retirement and terminal tombstone compaction.
    /// Runtime transports must not expose this through the workload role.
    pub fn retire_generation(&mut self, consumer: &str, generation: &str) -> Result<()> {
        validate_id(consumer)?;
        validate_id(generation)?;
        self.journal.transaction(|tx| {
            let instances: u64 = tx.query_row("SELECT COUNT(*) FROM instances WHERE consumer=?1 AND generation=?2 AND state!='retired'",
                params![consumer, generation], |r| r.get(0)).map_err(db_error)?;
            let charged = journal::active(tx)?.iter().any(|r| r.key.consumer_id == consumer && r.key.consumer_generation == generation)
                || journal::active_leases(tx)?.iter().any(|l| l.key.consumer_id == consumer && l.key.consumer_generation == generation);
            if instances != 0 || charged { return Err(Error::new(ErrorCode::ReconciliationRequired, "generation still has live or suspect ownership")); }
            tx.execute("INSERT OR IGNORE INTO retired_generations(consumer,generation) VALUES (?1,?2)", params![consumer, generation]).map_err(db_error)?;
            tx.execute("DELETE FROM attempts WHERE consumer=?1 AND generation=?2 AND charged=0", params![consumer, generation]).map_err(db_error)?;
            Ok(())
        })
    }
}

/// Parent leases: a bounded budget reserved from the host once, from which the
/// lease's children are admitted. Children never draw on the host again, and
/// their sum never exceeds the lease.
impl<B: Backend, C: Clock> Authority<B, C> {
    /// Reserve `budget` from host capacity as a lease owned by `principal`,
    /// fenced after `ttl_ms` if given. The token is returned once.
    pub fn admit_lease(
        &mut self,
        principal: &Principal,
        key: &AttemptKey,
        budget: Budget,
        ttl_ms: Option<u64>,
    ) -> Result<LeaseGrant> {
        check_key(principal, key)?;
        budget.validate_workload()?;
        let now = self.clock.now();
        let target = self
            .pressure
            .current(&now)
            .target(self.policy.work_capacity()?);
        let policy_revision = self.policy.revision.clone();
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            journal::expire_prepared(tx, &now)?;
            if let Some((existing, _)) = journal::load_lease(tx, key)? {
                if existing.owner != principal.instance || existing.budget != budget {
                    return Err(Error::new(
                        ErrorCode::AttemptConflict,
                        "lease identity has a different meaning or owner",
                    ));
                }
                return Ok(LeaseGrant {
                    lease: existing,
                    token: None,
                });
            }
            if !budget.fits(target.remaining_after(journal::committed(tx)?)) {
                return Err(Error::new(
                    ErrorCode::ResourceUnavailable,
                    "the lease does not fit the host's capacity at the current pressure",
                ));
            }
            let deadline_ms = match ttl_ms {
                Some(ttl) => Some(now.monotonic_ms.checked_add(ttl).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidRequest, "lease deadline overflow")
                })?),
                None => None,
            };
            let token = Secret::new(format!(
                "{}{}",
                Uuid::new_v4().simple(),
                Uuid::new_v4().simple()
            ))?;
            let lease = LeaseRecord {
                key: key.clone(),
                owner: principal.instance.clone(),
                policy_revision,
                budget,
                phase: LeasePhase::Active,
                created_at: now.clone(),
                deadline_ms,
                end_reason: None,
            };
            journal::save_lease(tx, &lease, &token.digest())?;
            Ok(LeaseGrant {
                lease,
                token: Some(token),
            })
        })
    }

    /// Admit a child execution under `lease`, presented with its token, against
    /// the lease's remainder. The child is then an ordinary attempt of
    /// `principal`, launched and released by the usual evidence.
    pub fn admit_child(
        &mut self,
        principal: &Principal,
        lease: &AttemptKey,
        token: &Secret,
        request: AdmissionRequest,
    ) -> Result<AttemptRecord> {
        let fingerprint = request.fingerprint()?;
        check_key(principal, &request.key)?;
        if lease.consumer_id != request.key.consumer_id
            || lease.consumer_generation != request.key.consumer_generation
        {
            return Err(unauthorized());
        }
        let now = self.clock.now();
        let policy = &self.policy;
        let backend = &self.backend;
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            journal::expire_prepared(tx, &now)?;
            let (mut parent, token_hash) = journal::load_lease(tx, lease)?.ok_or_else(not_found)?;
            if !same_digest(&token_hash, &token.digest()) {
                return Err(unauthorized());
            }
            if let Some(record) = journal::load(tx, &request.key)? {
                if record.request_fingerprint != fingerprint
                    || record.owner != principal.instance
                    || journal::child_lease(tx, &request.key)?.as_ref() != Some(lease)
                {
                    return Err(Error::new(
                        ErrorCode::AttemptConflict,
                        "attempt identity has different meaning, owner or lease",
                    ));
                }
                return Ok(record);
            }
            if fence_if_due(&mut parent, &now) {
                journal::save_lease(tx, &parent, &token_hash)?;
            }
            let remaining = lease_remaining(tx, &parent)?;
            let plan = backend.plan(&request.intent);
            let denial = if parent.phase != LeasePhase::Active {
                // A fenced lease admits nothing; waiting cannot change that.
                Some(ErrorCode::InvalidTransition)
            } else {
                match &plan {
                    Ok(plan)
                        if request.intent.profile == "interactive"
                            && plan.validate().is_ok()
                            && plan.levels().satisfies(request.intent.minimum) =>
                    {
                        (!request.intent.requested.fits(remaining))
                            .then_some(ErrorCode::ResourceUnavailable)
                    }
                    Err(error) if error.code == ErrorCode::ResourceControlUnavailable => {
                        Some(error.code)
                    }
                    _ => Some(ErrorCode::ResourcePolicyUnsupported),
                }
            };
            let record = AttemptRecord {
                key: request.key,
                request_fingerprint: fingerprint,
                owner: principal.instance.clone(),
                policy_revision: policy.revision.clone(),
                phase: if denial.is_some() {
                    AttemptPhase::Denied
                } else {
                    AttemptPhase::Prepared
                },
                reservation: if denial.is_some() {
                    None
                } else {
                    Some(ResourceReservation {
                        lease_id: Uuid::new_v4().to_string(),
                        quantities: request.intent.requested,
                        prepared_at: now.clone(),
                        prepare_deadline_ms: now
                            .monotonic_ms
                            .checked_add(PREPARED_TTL_MS)
                            .ok_or_else(|| {
                                Error::new(ErrorCode::InvalidRequest, "monotonic deadline overflow")
                            })?,
                    })
                },
                plan: if denial.is_some() { None } else { Some(plan?) },
                scope: None,
                applied: None,
                denial,
                tracking_lost: false,
                release_reason: None,
            };
            journal::save(tx, &record)?;
            journal::link_child(tx, &record.key, lease)?;
            Ok(record)
        })
    }

    /// End an owned lease: it admits no new child and is released once every
    /// child is settled.
    pub fn end_lease(&mut self, principal: &Principal, lease: &AttemptKey) -> Result<LeaseView> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            validate_principal(tx, principal)?;
            journal::expire_prepared(tx, &now)?;
            let (mut record, token_hash) = journal::load_lease(tx, lease)?.ok_or_else(not_found)?;
            if record.owner != principal.instance {
                return Err(unauthorized());
            }
            let before = record.clone();
            if record.phase == LeasePhase::Active {
                record.phase = LeasePhase::Ending;
                record.end_reason = Some(LeaseEndReason::Ended);
            }
            settle(tx, &mut record)?;
            if record != before {
                journal::save_lease(tx, &record, &token_hash)?;
            }
            lease_view(tx, record)
        })
    }

    /// A lease holder's view, authorized by the lease token alone.
    pub fn lease_status(&mut self, lease: &AttemptKey, token: &Secret) -> Result<LeaseView> {
        lease.validate()?;
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            journal::expire_prepared(tx, &now)?;
            let (mut record, token_hash) =
                journal::load_lease(tx, lease)?.ok_or_else(unauthorized)?;
            if !same_digest(&token_hash, &token.digest()) {
                return Err(unauthorized());
            }
            if fence_if_due(&mut record, &now) {
                journal::save_lease(tx, &record, &token_hash)?;
            }
            lease_view(tx, record)
        })
    }

    /// Trusted reconciliation: fence a lease whose owner has ended, which
    /// belongs to an earlier boot or whose deadline passed, and release it once
    /// every child is settled. Suspect children keep it charged.
    pub fn reconcile_lease(
        &mut self,
        lease: &AttemptKey,
        owner_running: bool,
    ) -> Result<LeaseRecord> {
        let now = self.clock.now();
        self.journal.transaction(|tx| {
            journal::expire_prepared(tx, &now)?;
            let (mut record, token_hash) = journal::load_lease(tx, lease)?.ok_or_else(not_found)?;
            let before = record.clone();
            if record.phase == LeasePhase::Active && !owner_running {
                record.phase = LeasePhase::Ending;
                record.end_reason = Some(LeaseEndReason::OwnerGone);
            }
            fence_if_due(&mut record, &now);
            settle(tx, &mut record)?;
            if record != before {
                journal::save_lease(tx, &record, &token_hash)?;
            }
            Ok(record)
        })
    }

    /// Leases that still hold their budget, for the reconciler.
    pub fn leases(&mut self) -> Result<Vec<LeaseRecord>> {
        self.journal.transaction(journal::active_leases)
    }

    /// A lease and its children, for trusted diagnostics and tests.
    pub fn lease(&mut self, lease: &AttemptKey) -> Result<LeaseView> {
        self.journal.transaction(|tx| {
            let (record, _) = journal::load_lease(tx, lease)?.ok_or_else(not_found)?;
            lease_view(tx, record)
        })
    }
}

/// Fence an Active lease of an earlier boot or past its deadline.
fn fence_if_due(lease: &mut LeaseRecord, now: &ObservationTime) -> bool {
    if lease.phase != LeasePhase::Active {
        return false;
    }
    let reason = if lease.created_at.boot_id != now.boot_id {
        LeaseEndReason::OwnerGone
    } else if lease
        .deadline_ms
        .is_some_and(|deadline| now.monotonic_ms >= deadline)
    {
        LeaseEndReason::Expired
    } else {
        return false;
    };
    lease.phase = LeasePhase::Ending;
    lease.end_reason = Some(reason);
    true
}

/// The budget of a lease's charged children.
fn children_charged(tx: &Transaction<'_>, lease: &LeaseRecord) -> Result<Budget> {
    let mut sum = Budget::ZERO;
    for child in journal::lease_children(tx, &lease.key)? {
        if let Some(record) = journal::load(tx, &child)? {
            if let (true, Some(reservation)) = (record.phase.charged(), &record.reservation) {
                sum = sum.checked_add(reservation.quantities)?;
            }
        }
    }
    Ok(sum)
}

fn lease_remaining(tx: &Transaction<'_>, lease: &LeaseRecord) -> Result<Budget> {
    Ok(lease.budget.remaining_after(children_charged(tx, lease)?))
}

/// Release an Ending lease once none of its children is charged.
fn settle(tx: &Transaction<'_>, lease: &mut LeaseRecord) -> Result<()> {
    if lease.phase != LeasePhase::Ending {
        return Ok(());
    }
    for child in journal::lease_children(tx, &lease.key)? {
        if journal::load(tx, &child)?.is_some_and(|record| record.phase.charged()) {
            return Ok(());
        }
    }
    lease.phase = LeasePhase::Released;
    Ok(())
}

fn lease_view(tx: &Transaction<'_>, lease: LeaseRecord) -> Result<LeaseView> {
    let remaining = if lease.charged() {
        lease_remaining(tx, &lease)?
    } else {
        Budget::ZERO
    };
    let children = journal::lease_children(tx, &lease.key)?;
    Ok(LeaseView {
        lease,
        remaining,
        children,
    })
}

fn same_digest(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

fn registration_policy(definition: &crate::ConsumerDefinition) -> Result<String> {
    Ok(digest_bytes(
        encode(&(
            &definition.generation,
            definition.uid,
            definition.role,
            definition.max_instances,
            definition.control_reservation,
        ))?
        .as_bytes(),
    ))
}

fn validate_registration_policies(tx: &Transaction<'_>, policy: &Policy) -> Result<()> {
    let mut statement = tx
        .prepare("SELECT consumer,registration_policy FROM instances WHERE state!='retired'")
        .map_err(db_error)?;
    let mut rows = statement.query([]).map_err(db_error)?;
    while let Some(row) = rows.next().map_err(db_error)? {
        let consumer: String = row.get(0).map_err(db_error)?;
        let recorded: String = row.get(1).map_err(db_error)?;
        let definition = policy.consumers.get(&consumer).ok_or_else(|| {
            Error::new(
                ErrorCode::ReconciliationRequired,
                "cannot remove a consumer with live or suspect instances",
            )
        })?;
        if registration_policy(definition)? != recorded {
            return Err(Error::new(ErrorCode::ReconciliationRequired,
                "retire existing instances before changing their generation, role or static reservation"));
        }
    }
    Ok(())
}
fn unauthorized() -> Error {
    Error::new(
        ErrorCode::Unauthorized,
        "consumer, peer, credential or instance is not authorized",
    )
}
fn not_found() -> Error {
    Error::new(ErrorCode::NotFound, "execution attempt not found")
}
fn invalid_transition() -> Error {
    Error::new(
        ErrorCode::InvalidTransition,
        "execution phase does not allow this transition",
    )
}
fn check_key(principal: &Principal, key: &AttemptKey) -> Result<()> {
    key.validate()?;
    if key.consumer_id != principal.consumer_id || key.consumer_generation != principal.generation {
        return Err(unauthorized());
    }
    Ok(())
}
fn retired(tx: &Transaction<'_>, consumer: &str, generation: &str) -> Result<bool> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM retired_generations WHERE consumer=?1 AND generation=?2)",
        params![consumer, generation],
        |r| r.get(0),
    )
    .map_err(db_error)
}
fn validate_principal(tx: &Transaction<'_>, principal: &Principal) -> Result<()> {
    if retired(tx, &principal.consumer_id, &principal.generation)? {
        return Err(unauthorized());
    }
    let row: Option<(String,String)> = tx.query_row("SELECT identity,state FROM instances WHERE consumer=?1 AND generation=?2 AND instance=?3",
        params![principal.consumer_id, principal.generation, principal.instance.instance_id],
        |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
    match row {
        Some((identity, state))
            if state == "active"
                && decode::<InstanceIdentity>(&identity)? == principal.instance =>
        {
            Ok(())
        }
        _ => Err(unauthorized()),
    }
}
fn owned_record(
    tx: &Transaction<'_>,
    principal: &Principal,
    key: &AttemptKey,
) -> Result<AttemptRecord> {
    check_key(principal, key)?;
    let record = journal::load(tx, key)?.ok_or_else(not_found)?;
    if record.owner != principal.instance {
        return Err(unauthorized());
    }
    Ok(record)
}
fn check_permit(tx: &Transaction<'_>, key: &AttemptKey, permit: &Secret) -> Result<()> {
    let digest: Option<String> = tx
        .query_row(
            "SELECT launch_hash FROM attempts WHERE consumer=?1 AND generation=?2 AND attempt=?3",
            params![key.consumer_id, key.consumer_generation, key.attempt_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?
        .flatten();
    if digest.is_none_or(|digest| !same_digest(&digest, &permit.digest())) {
        return Err(unauthorized());
    }
    Ok(())
}
