mod support;
use devguard_contract::*;
use devguard_core::*;
use rusqlite::Connection;
use std::sync::{Arc, Barrier, Mutex};
use support::*;

#[test]
fn storage_activation_revalidates_accounting_in_the_recovery_transaction() {
    let h = Harness::new();
    let (mut authority, principal) = h.ready();
    h.prepare(&mut authority, &principal, "activation-gap");
    drop(authority);
    let storage = AuthorityStorage::open(&h.path).unwrap();
    let fault = Connection::open(&h.path).unwrap();
    fault.execute("UPDATE attempts SET charged=0", []).unwrap();
    assert!(matches!(
        TestAuthority::from_storage(
            storage,
            h.policy.clone(),
            h.backend.clone(),
            h.clock.clone()
        ),
        Err(Error {
            code: ErrorCode::JournalInvalid,
            ..
        })
    ));
}

#[test]
fn admission_reply_loss_and_policy_change_replay_one_durable_reservation() {
    let mut h = Harness::new();
    let (mut a, p) = h.ready();
    let first = h.prepare(&mut a, &p, "attempt-1");
    let replay = a.admit(&p, request("attempt-1")).unwrap();
    assert_eq!(first, replay);
    assert_eq!(
        a.committed_budget().unwrap(),
        first.reservation.as_ref().unwrap().quantities
    );
    drop(a);
    h.policy.revision = "policy-2".into();
    let (mut restarted, p) = h.ready();
    assert_eq!(first, restarted.admit(&p, request("attempt-1")).unwrap());
    assert_eq!(first.policy_revision, "policy-1");
}

#[test]
fn changed_execution_or_resources_conflict_even_if_transport_identity_is_reused() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    h.prepare(&mut a, &p, "same");
    let mut changed = request("same");
    changed.execution_digest = digest_bytes(b"different argv");
    assert_eq!(
        a.admit(&p, changed).unwrap_err().code,
        ErrorCode::AttemptConflict
    );
    let mut changed = request("same");
    changed.intent.requested.cpu_milli += 1;
    assert_eq!(
        a.admit(&p, changed).unwrap_err().code,
        ErrorCode::AttemptConflict
    );
}

#[test]
fn closed_attempt_keys_never_become_new_work() {
    for expire in [false, true] {
        let h = Harness::new();
        let (mut a, p) = h.ready();
        let r = h.prepare(&mut a, &p, "terminal");
        if expire {
            h.clock.advance(PREPARED_TTL_MS);
        } else {
            a.cancel(&p, &r.key).unwrap();
        }
        let replay = a.admit(&p, request("terminal")).unwrap();
        assert_eq!(
            replay.phase,
            if expire {
                AttemptPhase::Expired
            } else {
                AttemptPhase::Cancelled
            }
        );
        assert!(a.begin_launch(&p, &r.key).unwrap().permit.is_none());
        assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
    }
}

#[test]
fn no_job_is_forced_into_a_zero_remaining_budget() {
    let mut h = Harness::new();
    h.policy.host_headroom = h.policy.effective_capacity;
    let (mut a, p) = h.ready();
    let denied = a.admit(&p, request("full")).unwrap();
    assert_eq!(denied.phase, AttemptPhase::Denied);
    assert_eq!(denied.denial, Some(ErrorCode::ResourceUnavailable));
    assert!(denied.reservation.is_none());
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn simultaneous_consumers_cannot_overbook_or_duplicate_a_retry() {
    let h = Harness::new();
    let (a, p) = h.ready();
    let a = Arc::new(Mutex::new(a));
    let barrier = Arc::new(Barrier::new(16));
    let handles: Vec<_> = (0..16)
        .map(|n| {
            let (a, p, barrier) = (a.clone(), p.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                a.lock()
                    .unwrap()
                    .admit(&p, request(&format!("attempt-{}", n % 4)))
                    .unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|t| t.join().unwrap()).collect();
    for pair in results
        .iter()
        .flat_map(|a| results.iter().map(move |b| (a, b)))
    {
        if pair.0.key == pair.1.key {
            assert_eq!(pair.0, pair.1);
        }
    }
    let committed = a.lock().unwrap().committed_budget().unwrap();
    assert!(committed.fits(h.policy.work_capacity().unwrap()));
    assert_eq!(committed.cpu_milli, 6_000);
}

#[test]
fn rejected_attempt_is_sticky_when_other_capacity_is_returned() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let first = h.prepare(&mut a, &p, "a");
    h.prepare(&mut a, &p, "b");
    let denied = a.admit(&p, request("c")).unwrap();
    assert_eq!(denied.phase, AttemptPhase::Denied);
    a.cancel(&p, &first.key).unwrap();
    assert_eq!(denied, a.admit(&p, request("c")).unwrap());
    h.prepare(&mut a, &p, "d");
}

#[test]
fn application_evidence_is_absent_at_prepare_and_required_before_authorization() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "phases");
    assert!(r.applied.is_none());
    assert!(r.scope.is_none());
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&r);
    assert!(a.authorize_run(&p, &r.key, &permit, &scope.root).is_err());
    let bound = a.bind_scope(&p, &r.key, &permit, &scope).unwrap();
    assert_eq!(bound.phase, AttemptPhase::ScopeBound);
    assert!(bound.applied.is_some());
    let authorized = a.authorize_run(&p, &r.key, &permit, &scope.root).unwrap();
    assert!(authorized.may_exec);
    assert_eq!(authorized.attempt.phase, AttemptPhase::RunAuthorized);
    // No field claims that the user's executable actually exec'd or succeeded.
    assert!(
        !a.authorize_run(&p, &r.key, &permit, &scope.root)
            .unwrap()
            .may_exec
    );
}

#[test]
fn commit_reply_loss_never_grants_a_second_spawn_and_restart_is_suspect() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "lost");
    assert!(a.begin_launch(&p, &r.key).unwrap().permit.is_some());
    assert!(a.begin_launch(&p, &r.key).unwrap().permit.is_none());
    drop(a);
    let (mut a, p) = h.ready();
    let replay = a.begin_launch(&p, &r.key).unwrap();
    assert_eq!(replay.attempt.phase, AttemptPhase::Suspect);
    assert!(replay.permit.is_none());
    h.clock.advance(60_000);
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Suspect);
    assert_eq!(
        a.committed_budget().unwrap(),
        r.reservation.unwrap().quantities
    );
}

#[test]
fn cancellation_fences_a_late_helper_before_any_reclamation() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "late");
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&r);
    assert_eq!(a.cancel(&p, &r.key).unwrap().phase, AttemptPhase::Draining);
    assert_eq!(
        a.bind_scope(&p, &r.key, &permit, &scope).unwrap_err().code,
        ErrorCode::InvalidTransition
    );
    assert_eq!(
        a.committed_budget().unwrap(),
        r.reservation.unwrap().quantities
    );
}

#[test]
fn cancellation_and_commit_races_cannot_spawn_after_a_returned_reservation() {
    for commit_first in [true, false] {
        let h = Harness::new();
        let (mut a, p) = h.ready();
        let r = h.prepare(&mut a, &p, "race");
        if commit_first {
            assert!(a.begin_launch(&p, &r.key).unwrap().permit.is_some());
            assert_eq!(a.cancel(&p, &r.key).unwrap().phase, AttemptPhase::Draining);
            assert_ne!(a.committed_budget().unwrap(), Budget::ZERO);
        } else {
            a.cancel(&p, &r.key).unwrap();
            assert!(a.begin_launch(&p, &r.key).unwrap().permit.is_none());
            assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
        }
    }
}

#[test]
fn expiry_boundary_is_inclusive_and_does_not_expire_committed_work() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "active");
    h.clock.advance(PREPARED_TTL_MS - 1);
    assert!(a.begin_launch(&p, &r.key).unwrap().permit.is_some());
    h.clock.advance(10_000);
    assert_eq!(
        a.lookup(&p, &r.key).unwrap().phase,
        AttemptPhase::LaunchCommitted
    );
    assert_ne!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn root_reap_does_not_return_resources_while_descendants_remain() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (r, _, scope) = h.bound(&mut a, &p, "descendants");
    h.closed(&scope);
    h.backend
        .0
        .lock()
        .unwrap()
        .observations
        .get_mut(&scope.scope_id)
        .unwrap()
        .empty = false;
    assert_ne!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Released);
    assert_ne!(a.committed_budget().unwrap(), Budget::ZERO);
    h.closed(&scope);
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Released);
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
    assert_eq!(a.lookup(&p, &r.key).unwrap().phase, AttemptPhase::Released);
}

#[test]
fn known_escape_is_sticky_until_explicit_reconciliation_covers_it() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (r, _, scope) = h.bound(&mut a, &p, "escape");
    h.closed(&scope);
    h.backend
        .0
        .lock()
        .unwrap()
        .observations
        .get_mut(&scope.scope_id)
        .unwrap()
        .known_escape = true;
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Suspect);
    h.closed(&scope);
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Suspect);
    h.backend
        .0
        .lock()
        .unwrap()
        .observations
        .get_mut(&scope.scope_id)
        .unwrap()
        .prior_tracking_loss_resolved = true;
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Released);
}

#[test]
fn wrong_scope_stale_evidence_and_pid_reuse_never_authorize_or_release() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "identity");
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&r);
    h.backend
        .0
        .lock()
        .unwrap()
        .processes
        .get_mut(&scope.root.pid)
        .unwrap()
        .start_ticks += 1;
    assert!(a.bind_scope(&p, &r.key, &permit, &scope).is_err());
    h.backend
        .0
        .lock()
        .unwrap()
        .processes
        .insert(scope.root.pid, scope.root.clone());
    a.bind_scope(&p, &r.key, &permit, &scope).unwrap();
    h.closed(&scope);
    h.clock.advance(2_001);
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Suspect);
    h.closed(&scope);
    h.backend
        .0
        .lock()
        .unwrap()
        .observations
        .get_mut(&scope.scope_id)
        .unwrap()
        .scope
        .scope_id = "other".into();
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Suspect);
    assert_ne!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn unbound_spawn_requires_positive_absence_evidence_not_owner_death() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "no-scope");
    a.begin_launch(&p, &r.key).unwrap();
    h.backend.0.lock().unwrap().processes.remove(&100);
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Suspect);
    h.backend.0.lock().unwrap().unbound.insert(
        r.key.clone(),
        UnboundLaunchEvidence {
            key: r.key.clone(),
            owner: r.owner.clone(),
            observed_at: h.clock.now(),
            helper_creation_ruled_out: true,
            no_pending_spawn: true,
        },
    );
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Released);
}

#[test]
fn kernel_requirement_cannot_be_satisfied_by_a_macos_plan() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let mut req = request("kernel");
    req.intent.minimum = ResourceLevels::KERNEL;
    assert_eq!(
        a.admit(&p, req).unwrap().denial,
        Some(ErrorCode::ResourcePolicyUnsupported)
    );
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn cgroup_scope_uses_the_same_identity_and_termination_contract() {
    let h = Harness::new();
    h.backend.0.lock().unwrap().kernel = true;
    let (mut a, p) = h.ready();
    let (r, _, scope) = h.bound(&mut a, &p, "fake-linux");
    assert_eq!(scope.kind, ScopeKind::ContainedCgroup);
    h.closed(&scope);
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Released);
    // This is a fake backend contract test, not Linux OS qualification.
}

#[test]
fn static_control_reservation_survives_disconnect_and_registration_retries() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let available = a.available_budget().unwrap();
    h.register(&mut a);
    assert_eq!(a.available_budget().unwrap(), available);
    a.disconnected(&p).unwrap();
    assert_eq!(a.available_budget().unwrap(), available);
    let mut second = h.registration.clone();
    second.instance.instance_id = "second-instance".into();
    assert_eq!(
        a.register(TrustedPeer { uid: 501, pid: 100 }, second)
            .unwrap_err()
            .code,
        ErrorCode::ResourceUnavailable
    );
    assert_eq!(
        a.admit(&p, request("stale-session")).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    let p = h.register(&mut a);
    h.prepare(&mut a, &p, "reconnected");
}

#[test]
fn peer_uid_alone_does_not_grant_consumer_authority() {
    let h = Harness::new();
    let mut a = h.open();
    let mut wrong = h.registration.clone();
    wrong.credential = Secret::new("b".repeat(64)).unwrap();
    assert_eq!(
        a.register(TrustedPeer { uid: 501, pid: 100 }, wrong)
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    assert_eq!(
        a.register(TrustedPeer { uid: 502, pid: 100 }, h.registration.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    assert_eq!(
        a.register(TrustedPeer { uid: 501, pid: 101 }, h.registration.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    let p = h.register(&mut a);
    let mut other = request("other");
    other.key.consumer_id = "dev-cli".into();
    assert_eq!(
        a.admit(&p, other).unwrap_err().code,
        ErrorCode::Unauthorized
    );
}

#[test]
fn generation_retirement_cannot_clear_suspect_work_and_old_keys_stay_rejected() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "old");
    assert_eq!(
        a.retire_generation("codespace-runtime", "generation-1")
            .unwrap_err()
            .code,
        ErrorCode::ReconciliationRequired
    );
    a.cancel(&p, &r.key).unwrap();
    h.backend.0.lock().unwrap().processes.remove(&100);
    assert!(a
        .reconcile_instance("codespace-runtime", "generation-1", "instance-1")
        .unwrap());
    a.retire_generation("codespace-runtime", "generation-1")
        .unwrap();
    h.backend
        .0
        .lock()
        .unwrap()
        .processes
        .insert(100, h.registration.instance.process.clone());
    assert_eq!(
        a.register(TrustedPeer { uid: 501, pid: 100 }, h.registration.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
}

#[test]
fn shrinking_target_does_not_shrink_an_existing_lease() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "running");
    a.begin_launch(&p, &r.key).unwrap();
    h.clock.advance(2_000);
    let mut sample = healthy(h.clock.now());
    sample.memory = MemoryPressure::Critical;
    assert_eq!(a.observe_pressure(sample).unwrap(), PressureState::Critical);
    assert_eq!(a.available_budget().unwrap(), Budget::ZERO);
    assert_eq!(
        a.committed_budget().unwrap(),
        r.reservation.unwrap().quantities
    );
    assert_eq!(
        a.admit(&p, request("new")).unwrap().phase,
        AttemptPhase::Denied
    );
}

#[test]
fn failure_to_persist_launch_hash_rolls_back_phase_and_never_returns_a_permit() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "durability");
    let fault = Connection::open(&h.path).unwrap();
    fault.execute_batch("CREATE TRIGGER fail_grant BEFORE UPDATE OF launch_hash ON attempts BEGIN SELECT RAISE(ABORT,'injected durable failure'); END;").unwrap();
    assert_eq!(
        a.begin_launch(&p, &r.key).unwrap_err().code,
        ErrorCode::JournalInvalid
    );
    assert_eq!(a.lookup(&p, &r.key).unwrap().phase, AttemptPhase::Prepared);
    fault.execute_batch("DROP TRIGGER fail_grant;").unwrap();
    assert!(a.begin_launch(&p, &r.key).unwrap().permit.is_some());
}

#[test]
fn missing_corrupt_or_existing_journal_is_never_silently_initialized() {
    let h = Harness::new();
    let missing = h.path.with_file_name("missing.sqlite");
    assert!(TestAuthority::open(
        &missing,
        h.policy.clone(),
        h.backend.clone(),
        h.clock.clone()
    )
    .is_err());
    assert!(!missing.exists());
    assert!(TestAuthority::initialize_journal(&h.path).is_err());
    let corrupt = h.path.with_file_name("corrupt.sqlite");
    std::fs::write(&corrupt, b"not a database").unwrap();
    assert!(TestAuthority::open(
        &corrupt,
        h.policy.clone(),
        h.backend.clone(),
        h.clock.clone()
    )
    .is_err());
    assert_eq!(std::fs::read(corrupt).unwrap(), b"not a database");
}

#[test]
fn authority_directory_lock_prevents_a_second_full_budget() {
    let h = Harness::new();
    let _first = h.open();
    let other = h.path.with_file_name("another.sqlite");
    TestAuthority::initialize_journal(&other).unwrap();
    assert!(matches!(
        TestAuthority::open(&other, h.policy.clone(), h.backend.clone(), h.clock.clone()),
        Err(Error {
            code: ErrorCode::ResourceControlUnavailable,
            ..
        })
    ));
}

#[test]
fn recovery_rejects_accounting_inconsistent_records() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    h.prepare(&mut a, &p, "corrupt-row");
    drop(a);
    let db = Connection::open(&h.path).unwrap();
    db.execute(
        "UPDATE attempts SET record=json_set(record,'$.reservation',NULL)",
        [],
    )
    .unwrap();
    assert!(TestAuthority::open(
        &h.path,
        h.policy.clone(),
        h.backend.clone(),
        h.clock.clone()
    )
    .is_err());
}

#[test]
fn a_reboot_resolves_unbound_old_processes_without_resetting_history() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "reboot");
    a.begin_launch(&p, &r.key).unwrap();
    drop(a);
    h.clock.reboot();
    let mut a = h.open();
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Released);
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
    let db = Connection::open(&h.path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
fn accounting_index_corruption_cannot_hide_a_live_reservation_at_restart() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    h.prepare(&mut a, &p, "index-corrupt");
    drop(a);
    let db = Connection::open(&h.path).unwrap();
    db.execute("UPDATE attempts SET charged=0", []).unwrap();
    assert!(TestAuthority::open(
        &h.path,
        h.policy.clone(),
        h.backend.clone(),
        h.clock.clone()
    )
    .is_err());
}

#[test]
fn a_reclaimed_budget_is_not_automatically_a_retryable_execution() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (r, _, scope) = h.bound(&mut a, &p, "finished");
    h.closed(&scope);
    let released = a.reconcile(&r.key).unwrap();
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(!released.known_not_started());
    let unbound = h.prepare(&mut a, &p, "never-created");
    a.begin_launch(&p, &unbound.key).unwrap();
    h.backend.0.lock().unwrap().unbound.insert(
        unbound.key.clone(),
        UnboundLaunchEvidence {
            key: unbound.key.clone(),
            owner: unbound.owner.clone(),
            observed_at: h.clock.now(),
            helper_creation_ruled_out: true,
            no_pending_spawn: true,
        },
    );
    assert!(a.reconcile(&unbound.key).unwrap().known_not_started());
    assert!(a.begin_launch(&p, &unbound.key).unwrap().permit.is_none());
}

#[test]
fn journal_and_debug_output_do_not_store_registration_or_launch_secrets() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "secrets");
    let grant = a.begin_launch(&p, &r.key).unwrap();
    let permit = grant.permit.as_ref().unwrap();
    assert!(!format!("{grant:?}").contains(permit.expose()));
    assert!(!format!("{:?}", h.registration).contains(h.registration.credential.expose()));
    let launch_secret = permit.expose().as_bytes().to_vec();
    let registration_secret = h.registration.credential.expose().as_bytes().to_vec();
    drop(a);
    for entry in std::fs::read_dir(h.path.parent().unwrap()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(path).unwrap();
            assert!(!bytes
                .windows(launch_secret.len())
                .any(|s| s == launch_secret));
            assert!(!bytes
                .windows(registration_secret.len())
                .any(|s| s == registration_secret));
        }
    }
}

#[test]
fn live_control_registration_prevents_policy_rotation_from_underaccounting_it() {
    for change in ["reservation", "generation", "remove"] {
        let mut h = Harness::new();
        let (a, _) = h.ready();
        drop(a);
        match change {
            "reservation" => {
                h.policy
                    .consumers
                    .get_mut("codespace-runtime")
                    .unwrap()
                    .control_reservation
                    .memory_bytes /= 2
            }
            "generation" => {
                h.policy
                    .consumers
                    .get_mut("codespace-runtime")
                    .unwrap()
                    .generation = "generation-2".into()
            }
            _ => {
                h.policy.consumers.remove("codespace-runtime");
            }
        }
        assert!(matches!(
            TestAuthority::open(
                &h.path,
                h.policy.clone(),
                h.backend.clone(),
                h.clock.clone()
            ),
            Err(Error {
                code: ErrorCode::ReconciliationRequired,
                ..
            })
        ));
    }
}

#[test]
fn evidence_observed_during_a_transition_is_fresh_but_stale_evidence_is_not() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let record = h.prepare(&mut a, &p, "observed-during");
    let permit = a.begin_launch(&p, &record.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&record);
    // A real backend timestamps evidence when its observation completes, after
    // the transition sampled its own clock.
    {
        let mut fake = h.backend.0.lock().unwrap();
        let binding = fake.bindings.get_mut(&scope.scope_id).unwrap();
        binding.applied.observed_at.monotonic_ms += 5;
        fake.advance_during_evidence = Some((h.clock.clone(), 5));
    }
    let bound = a.bind_scope(&p, &record.key, &permit, &scope).unwrap();
    assert_eq!(bound.phase, AttemptPhase::ScopeBound);
    assert!(
        a.authorize_run(&p, &record.key, &permit, &scope.root)
            .unwrap()
            .may_exec
    );
    h.closed(&scope);
    h.backend
        .0
        .lock()
        .unwrap()
        .observations
        .get_mut(&scope.scope_id)
        .unwrap()
        .observed_at
        .monotonic_ms += 5;
    let released = a.reconcile(&record.key).unwrap();
    assert_eq!(released.phase, AttemptPhase::Released);
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );

    // Evidence that is older than two seconds when the backend returns is not
    // fresh, however it was produced.
    let other = h.prepare(&mut a, &p, "stale-after-return");
    let permit = a.begin_launch(&p, &other.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&other);
    h.backend.0.lock().unwrap().advance_during_evidence = Some((h.clock.clone(), 2_001));
    assert_eq!(
        a.bind_scope(&p, &other.key, &permit, &scope)
            .unwrap_err()
            .code,
        ErrorCode::ResourcePolicyUnsupported
    );

    // Positive launcher evidence for an unbound launch is also judged after
    // the backend returns.
    h.backend.0.lock().unwrap().advance_during_evidence = None;
    let unbound = h.prepare(&mut a, &p, "unbound-during");
    a.begin_launch(&p, &unbound.key).unwrap();
    {
        let mut fake = h.backend.0.lock().unwrap();
        let mut observed_at = h.clock.now();
        observed_at.monotonic_ms += 5;
        fake.unbound.insert(
            unbound.key.clone(),
            UnboundLaunchEvidence {
                key: unbound.key.clone(),
                owner: unbound.owner.clone(),
                observed_at,
                helper_creation_ruled_out: true,
                no_pending_spawn: true,
            },
        );
        fake.advance_during_evidence = Some((h.clock.clone(), 5));
    }
    let resolved = a.reconcile(&unbound.key).unwrap();
    assert_eq!(resolved.phase, AttemptPhase::Released);
    assert_eq!(
        resolved.release_reason,
        Some(ReleaseReason::NoHelperCreated)
    );
}

/// A second helper for the same grant: another root and scope identity.
fn other_helper(h: &Harness, first: &ScopeIdentity) -> ScopeIdentity {
    let mut other = first.clone();
    other.scope_id.push_str("-other");
    other.root.pid += 1_000;
    let mut fake = h.backend.0.lock().unwrap();
    fake.processes.insert(other.root.pid, other.root.clone());
    let mut evidence = fake.bindings[&first.scope_id].clone();
    evidence.applied.scope = other.clone();
    fake.bindings.insert(other.scope_id.clone(), evidence);
    other
}

#[test]
fn helper_claim_by_the_first_helper_fences_every_other_scope() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "claimed");
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let wrong = Secret::new("b".repeat(64)).unwrap();
    assert_eq!(
        a.verify_launch(&p, &r.key, &wrong).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    let verified = a.verify_launch(&p, &r.key, &permit).unwrap();
    assert_eq!(verified.phase, AttemptPhase::LaunchCommitted);
    let first = h.provide_binding(&r);
    let second = other_helper(&h, &first);
    let claimed = a.claim_launch(&p, &r.key, &permit, &first).unwrap();
    assert_eq!(claimed.phase, AttemptPhase::LaunchCommitted);
    assert_eq!(claimed.scope.as_ref(), Some(&first));
    assert!(claimed.applied.is_none());
    // The same helper presenting again gets the stored claim.
    assert_eq!(
        a.claim_launch(&p, &r.key, &permit, &first).unwrap(),
        claimed
    );
    // Another helper can neither claim, bind nor be authorized.
    assert_eq!(
        a.claim_launch(&p, &r.key, &permit, &second)
            .unwrap_err()
            .code,
        ErrorCode::InvalidTransition
    );
    assert_eq!(
        a.bind_scope(&p, &r.key, &permit, &second).unwrap_err().code,
        ErrorCode::InvalidTransition
    );
    assert_eq!(
        a.authorize_run(&p, &r.key, &permit, &second.root)
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    // The claimed helper binds and is authorized exactly once.
    let bound = a.bind_scope(&p, &r.key, &permit, &first).unwrap();
    assert_eq!(bound.phase, AttemptPhase::ScopeBound);
    assert!(
        a.authorize_run(&p, &r.key, &permit, &first.root)
            .unwrap()
            .may_exec
    );
    assert!(
        !a.authorize_run(&p, &r.key, &permit, &first.root)
            .unwrap()
            .may_exec
    );
    assert_eq!(
        a.claim_launch(&p, &r.key, &permit, &first).unwrap().phase,
        AttemptPhase::RunAuthorized
    );
}

#[test]
fn helper_claim_needs_an_open_grant_and_a_live_helper_of_this_boot() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let prepared = h.prepare(&mut a, &p, "unclaimable");
    let scope = h.provide_binding(&prepared);
    let any = Secret::new("c".repeat(64)).unwrap();
    // Without a launch grant there is no permit to present.
    assert_eq!(
        a.claim_launch(&p, &prepared.key, &any, &scope)
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    let r = h.prepare(&mut a, &p, "claim-checks");
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let live = h.provide_binding(&r);
    let unverified = |a: &mut TestAuthority, scope: &ScopeIdentity| {
        a.claim_launch(&p, &r.key, &permit, scope).unwrap_err().code
    };
    let mut dead = live.clone();
    dead.root.pid += 5_000;
    assert_eq!(
        unverified(&mut a, &dead),
        ErrorCode::ResourcePolicyUnsupported
    );
    let mut reused = live.clone();
    reused.root.start_ticks += 1;
    assert_eq!(
        unverified(&mut a, &reused),
        ErrorCode::ResourcePolicyUnsupported
    );
    let mut previous_boot = live.clone();
    previous_boot.root.boot_id = "boot-z".into();
    assert_eq!(
        unverified(&mut a, &previous_boot),
        ErrorCode::ResourcePolicyUnsupported
    );
    let mut cgroup = live.clone();
    cgroup.kind = ScopeKind::ContainedCgroup;
    assert_eq!(
        unverified(&mut a, &cgroup),
        ErrorCode::ResourcePolicyUnsupported
    );
    assert!(a.lookup(&p, &r.key).unwrap().scope.is_none());
    // Cancellation fences the grant before any helper claims it.
    assert_eq!(a.cancel(&p, &r.key).unwrap().phase, AttemptPhase::Draining);
    assert_eq!(unverified(&mut a, &live), ErrorCode::InvalidTransition);
    assert!(a.lookup(&p, &r.key).unwrap().scope.is_none());
}

#[test]
fn helper_claim_that_never_binds_is_released_only_through_its_scope() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "claimed-unbound");
    let quantities = r.reservation.as_ref().unwrap().quantities;
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&r);
    a.claim_launch(&p, &r.key, &permit, &scope).unwrap();
    // Application fails: the helper's scope is recorded but never bound.
    h.backend
        .0
        .lock()
        .unwrap()
        .bindings
        .get_mut(&scope.scope_id)
        .unwrap()
        .applied
        .cpu = ApplicationState::Failed;
    assert_eq!(
        a.bind_scope(&p, &r.key, &permit, &scope).unwrap_err().code,
        ErrorCode::ResourcePolicyUnsupported
    );
    // Launcher evidence of an absent helper cannot release a claimed grant.
    h.backend.0.lock().unwrap().unbound.insert(
        r.key.clone(),
        UnboundLaunchEvidence {
            key: r.key.clone(),
            owner: r.owner.clone(),
            observed_at: h.clock.now(),
            helper_creation_ruled_out: true,
            no_pending_spawn: true,
        },
    );
    let alive = ScopeObservation {
        scope: scope.clone(),
        observed_at: h.clock.now(),
        root_reaped: false,
        empty: false,
        known_members_gone: false,
        tracking_complete: true,
        known_escape: false,
        prior_tracking_loss_resolved: false,
    };
    h.backend
        .0
        .lock()
        .unwrap()
        .observations
        .insert(scope.scope_id.clone(), alive.clone());
    let pending = a.reconcile(&r.key).unwrap();
    assert_eq!(pending.phase, AttemptPhase::LaunchCommitted);
    assert_eq!(a.committed_budget().unwrap(), quantities);
    // Its scope ends: the reservation returns as scope termination, which is
    // not evidence that the executable never started.
    h.clock.advance(100);
    h.closed(&scope);
    let released = a.reconcile(&r.key).unwrap();
    assert_eq!(released.phase, AttemptPhase::Released);
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(!released.known_not_started());
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn helper_claim_survives_restart_as_a_fenced_suspect() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let r = h.prepare(&mut a, &p, "claimed-restart");
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&r);
    a.claim_launch(&p, &r.key, &permit, &scope).unwrap();
    drop(a);
    let (mut a, p) = h.ready();
    let recovered = a.lookup(&p, &r.key).unwrap();
    assert_eq!(recovered.phase, AttemptPhase::Suspect);
    assert_eq!(recovered.scope.as_ref(), Some(&scope));
    for code in [
        a.claim_launch(&p, &r.key, &permit, &other_helper(&h, &scope))
            .unwrap_err()
            .code,
        a.bind_scope(&p, &r.key, &permit, &scope).unwrap_err().code,
    ] {
        assert_eq!(code, ErrorCode::InvalidTransition);
    }
    assert_eq!(
        a.committed_budget().unwrap(),
        r.reservation.unwrap().quantities
    );
}

#[test]
fn launch_cancel_never_rewrites_suspect_draining_or_terminal_attempts() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (bound, _, _) = h.bound(&mut a, &p, "cancel-bound");
    let draining = h.prepare(&mut a, &p, "cancel-draining");
    a.begin_launch(&p, &draining.key).unwrap();
    assert_eq!(
        a.cancel(&p, &draining.key).unwrap().phase,
        AttemptPhase::Draining
    );
    // A second cancellation of a draining attempt writes nothing.
    let reader = Connection::open(&h.path).unwrap();
    let version = || -> i64 {
        reader
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .unwrap()
    };
    let before = version();
    assert_eq!(
        a.cancel(&p, &draining.key).unwrap().phase,
        AttemptPhase::Draining
    );
    assert_eq!(version(), before);
    drop(a);
    // The restart made both attempts Suspect; cancelling keeps them so.
    let (mut a, p) = h.ready();
    let before = version();
    for key in [&bound.key, &draining.key] {
        assert_eq!(a.cancel(&p, key).unwrap().phase, AttemptPhase::Suspect);
    }
    assert_eq!(version(), before);
    assert_ne!(a.committed_budget().unwrap(), Budget::ZERO);

    let h = Harness::new();
    let (mut a, p) = h.ready();
    // A prepared attempt past its deadline reports its expiry, not a cancel.
    let late = h.prepare(&mut a, &p, "cancel-late");
    h.clock.advance(PREPARED_TTL_MS);
    a.observe_pressure(healthy(h.clock.now())).unwrap();
    assert_eq!(
        a.cancel(&p, &late.key).unwrap().phase,
        AttemptPhase::Expired
    );
    // A terminal attempt stays terminal and is not rewritten.
    let r = h.prepare(&mut a, &p, "cancel-released");
    let permit = a.begin_launch(&p, &r.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&r);
    a.bind_scope(&p, &r.key, &permit, &scope).unwrap();
    h.closed(&scope);
    let released = a.reconcile(&r.key).unwrap();
    assert_eq!(released.phase, AttemptPhase::Released);
    let reader = Connection::open(&h.path).unwrap();
    let before: i64 = reader
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(a.cancel(&p, &released.key).unwrap(), released);
    let after: i64 = reader
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after, before);
}

#[test]
fn reconcile_releases_a_bound_scope_after_a_reboot_as_previous_boot() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (r, _, _) = h.bound(&mut a, &p, "bound-reboot");
    // Without an observation the scope is tracking loss, sticky and charged.
    let lost = a.reconcile(&r.key).unwrap();
    assert_eq!(lost.phase, AttemptPhase::Suspect);
    assert!(lost.tracking_lost);
    drop(a);
    h.clock.reboot();
    let mut a = h.open();
    let released = a.reconcile(&r.key).unwrap();
    assert_eq!(released.phase, AttemptPhase::Released);
    assert_eq!(released.release_reason, Some(ReleaseReason::PreviousBoot));
    assert!(!released.known_not_started());
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn reconcile_that_changes_nothing_writes_nothing() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (r, _, scope) = h.bound(&mut a, &p, "unchanged");
    h.backend.0.lock().unwrap().observations.insert(
        scope.scope_id.clone(),
        ScopeObservation {
            scope: scope.clone(),
            observed_at: h.clock.now(),
            root_reaped: false,
            empty: false,
            known_members_gone: false,
            tracking_complete: true,
            known_escape: false,
            prior_tracking_loss_resolved: false,
        },
    );
    let reader = Connection::open(&h.path).unwrap();
    let version = || -> i64 {
        reader
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .unwrap()
    };
    let before = version();
    for _ in 0..3 {
        assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::ScopeBound);
    }
    assert_eq!(version(), before);
    h.closed(&scope);
    assert_eq!(a.reconcile(&r.key).unwrap().phase, AttemptPhase::Released);
    assert_ne!(version(), before);
}

#[test]
fn reconcile_listings_show_only_live_accounting() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let prepared = h.prepare(&mut a, &p, "listed-prepared");
    let committed = h.prepare(&mut a, &p, "listed-committed");
    a.begin_launch(&p, &committed.key).unwrap();
    let mut denied = request("listed-denied");
    denied.intent.minimum = ResourceLevels::KERNEL;
    assert_eq!(a.admit(&p, denied).unwrap().phase, AttemptPhase::Denied);
    let keys = |a: &mut TestAuthority| -> Vec<String> {
        let mut keys: Vec<String> = a
            .attempts()
            .unwrap()
            .into_iter()
            .map(|record| record.key.attempt_id)
            .collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&mut a), ["listed-committed", "listed-prepared"]);
    h.clock.advance(PREPARED_TTL_MS);
    assert_eq!(keys(&mut a), ["listed-committed"]);
    assert_eq!(
        a.lookup(&p, &prepared.key).unwrap().phase,
        AttemptPhase::Expired
    );
    let instances = a.instances().unwrap();
    assert_eq!(instances.len(), 1);
    assert!(instances[0].active);
    assert_eq!(instances[0].instance, h.registration.instance);
    drop(a);
    let mut a = h.open();
    assert!(!a.instances().unwrap()[0].active);
    h.register(&mut a);
    assert!(a.instances().unwrap()[0].active);
}

#[test]
fn reconcile_retires_a_previous_boot_instance_whatever_now_holds_its_pid() {
    let h = Harness::new();
    let (a, _) = h.ready();
    drop(a);
    h.clock.reboot();
    // After the reboot another user's process holds the old PID.
    h.backend.0.lock().unwrap().refused.insert(100);
    let mut a = h.open();
    let instance = &h.registration.instance;
    assert!(a
        .reconcile_instance("codespace-runtime", "generation-1", &instance.instance_id)
        .unwrap());
    assert!(a.instances().unwrap().is_empty());
}

#[test]
fn reconcile_rejects_release_reasons_that_contradict_the_recorded_scope() {
    for (name, edit) in [
        (
            "no-helper-with-scope",
            "UPDATE attempts SET record=json_set(record,'$.release_reason','no_helper_created')",
        ),
        (
            "terminated-without-scope",
            "UPDATE attempts SET record=json_set(record,'$.scope',NULL,'$.applied',NULL)",
        ),
    ] {
        let h = Harness::new();
        let (mut a, p) = h.ready();
        let (r, _, scope) = h.bound(&mut a, &p, name);
        h.closed(&scope);
        let released = a.reconcile(&r.key).unwrap();
        assert_eq!(
            released.release_reason,
            Some(ReleaseReason::ScopeTerminated)
        );
        drop(a);
        Connection::open(&h.path)
            .unwrap()
            .execute(edit, [])
            .unwrap();
        let reopened = TestAuthority::open(
            &h.path,
            h.policy.clone(),
            h.backend.clone(),
            h.clock.clone(),
        );
        assert_eq!(
            reopened.err().map(|error| error.code),
            Some(ErrorCode::JournalInvalid),
            "{name}"
        );
    }
}
