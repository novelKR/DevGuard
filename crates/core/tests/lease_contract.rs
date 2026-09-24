//! DG1-C10 parent leases: a budget reserved from the host once, from which the
//! lease's children are admitted. Children never draw on the host again, their
//! sum never exceeds the lease, and a lease is released only after every child
//! is settled.

// The shared harness also serves the authority contract, whose steps this file does not all use.
#[allow(dead_code)]
mod support;
use devguard_contract::*;
use devguard_core::*;
use rusqlite::Connection;
use support::*;

fn lease_key(id: &str) -> AttemptKey {
    AttemptKey {
        consumer_id: "codespace-runtime".into(),
        consumer_generation: "generation-1".into(),
        attempt_id: id.into(),
    }
}

fn budget(cpu_milli: u64) -> Budget {
    Budget {
        cpu_milli,
        memory_bytes: 8 * GIB,
        tasks: 256,
    }
}

fn granted(a: &mut TestAuthority, p: &Principal, id: &str, cpu_milli: u64) -> (AttemptKey, Secret) {
    let key = lease_key(id);
    let grant = a.admit_lease(p, &key, budget(cpu_milli), None).unwrap();
    assert_eq!(grant.lease.phase, LeasePhase::Active);
    (key, grant.token.unwrap())
}

fn bound_child(
    h: &Harness,
    a: &mut TestAuthority,
    p: &Principal,
    lease: &AttemptKey,
    token: &Secret,
    id: &str,
) -> (AttemptRecord, ScopeIdentity) {
    let record = a.admit_child(p, lease, token, request(id)).unwrap();
    assert_eq!(record.phase, AttemptPhase::Prepared);
    let permit = a.begin_launch(p, &record.key).unwrap().permit.unwrap();
    let scope = h.provide_binding(&record);
    let bound = a.bind_scope(p, &record.key, &permit, &scope).unwrap();
    (bound, scope)
}

#[test]
fn a_lease_is_charged_once_and_its_children_draw_only_on_it() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    // The host's work capacity is 7,500 mCPU; the lease takes 7,000.
    let (lease, token) = granted(&mut a, &p, "lease-1", 7_000);
    assert_eq!(a.committed_budget().unwrap(), budget(7_000));
    // Only 500 mCPU is left for work outside the lease.
    let outside = a.admit(&p, request("outside")).unwrap();
    assert_eq!(outside.phase, AttemptPhase::Denied);
    assert_eq!(outside.denial, Some(ErrorCode::ResourceUnavailable));
    // Children of 3,000 mCPU each fit the lease, not the host's remainder,
    // and are not counted against the host a second time.
    for id in ["child-1", "child-2"] {
        let child = a.admit_child(&p, &lease, &token, request(id)).unwrap();
        assert_eq!(child.phase, AttemptPhase::Prepared, "{id}");
        assert_eq!(a.committed_budget().unwrap(), budget(7_000), "{id}");
    }
    let view = a.lease(&lease).unwrap();
    assert_eq!(view.remaining.cpu_milli, 1_000);
    assert_eq!(view.children.len(), 2);
    // A third child would exceed the lease: its sum never passes the budget.
    let third = a
        .admit_child(&p, &lease, &token, request("child-3"))
        .unwrap();
    assert_eq!(third.phase, AttemptPhase::Denied);
    assert_eq!(third.denial, Some(ErrorCode::ResourceUnavailable));
    assert_eq!(a.lease(&lease).unwrap().remaining.cpu_milli, 1_000);
    // A lease that does not fit the host is refused and never recorded.
    assert_eq!(
        a.admit_lease(&p, &lease_key("lease-2"), budget(1_000), None)
            .unwrap_err()
            .code,
        ErrorCode::ResourceUnavailable
    );
}

#[test]
fn a_child_needs_the_lease_token_its_generation_and_a_consistent_replay() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (lease, token) = granted(&mut a, &p, "lease-1", 6_000);
    let wrong = Secret::new("b".repeat(64)).unwrap();
    assert_eq!(
        a.admit_child(&p, &lease, &wrong, request("child-1"))
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    let mut foreign = request("child-1");
    foreign.key.consumer_generation = "generation-2".into();
    assert_eq!(
        a.admit_child(&p, &lease, &token, foreign).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    assert_eq!(
        a.admit_child(&p, &lease_key("unknown"), &token, request("child-1"))
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let first = a
        .admit_child(&p, &lease, &token, request("child-1"))
        .unwrap();
    // A replay returns the same child; the same key as an ordinary attempt
    // or under another lease conflicts.
    assert_eq!(
        a.admit_child(&p, &lease, &token, request("child-1"))
            .unwrap(),
        first
    );
    let (other, other_token) = granted(&mut a, &p, "lease-2", 1_000);
    assert_eq!(
        a.admit_child(&p, &other, &other_token, request("child-1"))
            .unwrap_err()
            .code,
        ErrorCode::AttemptConflict
    );
    let mut changed = request("child-1");
    changed.execution_digest = digest_bytes(b"another command");
    assert_eq!(
        a.admit_child(&p, &lease, &token, changed).unwrap_err().code,
        ErrorCode::AttemptConflict
    );
}

#[test]
fn replaying_a_lease_admission_returns_it_without_its_token() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let key = lease_key("lease-1");
    let first = a
        .admit_lease(&p, &key, budget(4_000), Some(60_000))
        .unwrap();
    assert!(first.token.is_some());
    let replay = a
        .admit_lease(&p, &key, budget(4_000), Some(60_000))
        .unwrap();
    assert_eq!(replay.lease, first.lease);
    assert!(replay.token.is_none(), "the token is returned once");
    assert_eq!(
        a.admit_lease(&p, &key, budget(5_000), None)
            .unwrap_err()
            .code,
        ErrorCode::AttemptConflict
    );
    assert_eq!(a.committed_budget().unwrap(), budget(4_000));
}

#[test]
fn lease_status_is_authorized_by_the_token_alone_and_reports_the_remainder() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (lease, token) = granted(&mut a, &p, "lease-1", 6_000);
    a.admit_child(&p, &lease, &token, request("child-1"))
        .unwrap();
    let view = a.lease_status(&lease, &token).unwrap();
    assert_eq!(view.lease.phase, LeasePhase::Active);
    assert_eq!(view.remaining.cpu_milli, 3_000);
    assert_eq!(view.children, vec![request("child-1").key]);
    let wrong = Secret::new("c".repeat(64)).unwrap();
    assert_eq!(
        a.lease_status(&lease, &wrong).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    // An unknown lease is indistinguishable from a wrong token.
    assert_eq!(
        a.lease_status(&lease_key("unknown"), &token)
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
}

#[test]
fn ending_a_lease_fences_new_children_and_releases_it_after_they_settle() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (lease, token) = granted(&mut a, &p, "lease-1", 6_000);
    let (child, scope) = bound_child(&h, &mut a, &p, &lease, &token, "child-1");
    let ended = a.end_lease(&p, &lease).unwrap();
    assert_eq!(ended.lease.phase, LeasePhase::Ending);
    assert_eq!(ended.lease.end_reason, Some(LeaseEndReason::Ended));
    // A fenced lease admits nothing, and waiting cannot change that.
    let late = a
        .admit_child(&p, &lease, &token, request("child-2"))
        .unwrap();
    assert_eq!(late.phase, AttemptPhase::Denied);
    assert_eq!(late.denial, Some(ErrorCode::InvalidTransition));
    // The running child keeps the lease charged.
    assert_eq!(
        a.reconcile_lease(&lease, true).unwrap().phase,
        LeasePhase::Ending
    );
    assert_eq!(a.committed_budget().unwrap(), budget(6_000));
    h.closed(&scope);
    assert_eq!(
        a.reconcile(&child.key).unwrap().phase,
        AttemptPhase::Released
    );
    let released = a.reconcile_lease(&lease, true).unwrap();
    assert_eq!(released.phase, LeasePhase::Released);
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
    assert!(a.leases().unwrap().is_empty());
}

#[test]
fn an_owner_that_ended_or_a_passed_deadline_fences_the_lease() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let key = lease_key("lease-1");
    let token = a
        .admit_lease(&p, &key, budget(3_000), Some(1_000))
        .unwrap()
        .token
        .unwrap();
    h.clock.advance(1_000);
    let late = a.admit_child(&p, &key, &token, request("child-1")).unwrap();
    assert_eq!(late.denial, Some(ErrorCode::InvalidTransition));
    let view = a.lease_status(&key, &token).unwrap();
    assert_eq!(view.lease.end_reason, Some(LeaseEndReason::Expired));
    // Without charged children an expired lease is released at once.
    assert_eq!(
        a.reconcile_lease(&key, true).unwrap().phase,
        LeasePhase::Released
    );
    // A lease whose owner is gone is fenced and, with no child, released.
    let (orphan, _) = granted(&mut a, &p, "lease-2", 3_000);
    let reconciled = a.reconcile_lease(&orphan, false).unwrap();
    assert_eq!(reconciled.phase, LeasePhase::Released);
    assert_eq!(reconciled.end_reason, Some(LeaseEndReason::OwnerGone));
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn a_suspect_child_keeps_its_lease_charged() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (lease, token) = granted(&mut a, &p, "lease-1", 6_000);
    let (child, scope) = bound_child(&h, &mut a, &p, &lease, &token, "child-1");
    a.end_lease(&p, &lease).unwrap();
    h.closed(&scope);
    h.backend
        .0
        .lock()
        .unwrap()
        .observations
        .get_mut(&scope.scope_id)
        .unwrap()
        .known_escape = true;
    let suspect = a.reconcile(&child.key).unwrap();
    assert_eq!(suspect.phase, AttemptPhase::Suspect);
    assert_eq!(
        a.reconcile_lease(&lease, false).unwrap().phase,
        LeasePhase::Ending
    );
    assert_eq!(a.committed_budget().unwrap(), budget(6_000));
}

#[test]
fn leases_survive_a_restart_and_an_earlier_boot_fences_and_releases_them() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (lease, token) = granted(&mut a, &p, "lease-1", 6_000);
    let (child, _) = bound_child(&h, &mut a, &p, &lease, &token, "child-1");
    drop(a);
    let (mut a, p) = h.ready();
    assert_eq!(a.lease(&lease).unwrap().lease.phase, LeasePhase::Active);
    assert_eq!(a.committed_budget().unwrap(), budget(6_000));
    drop((a, p));
    // After a reboot the owner and the child's scope have ended.
    h.clock.reboot();
    let mut a = h.open();
    assert_eq!(
        a.reconcile(&child.key).unwrap().release_reason,
        Some(ReleaseReason::PreviousBoot)
    );
    let released = a.reconcile_lease(&lease, true).unwrap();
    assert_eq!(released.phase, LeasePhase::Released);
    assert_eq!(released.end_reason, Some(LeaseEndReason::OwnerGone));
    assert_eq!(a.committed_budget().unwrap(), Budget::ZERO);
}

#[test]
fn a_generation_with_a_lease_cannot_be_retired() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let (lease, _) = granted(&mut a, &p, "lease-1", 3_000);
    // The owner has ended and holds no attempt, so its instance is retired;
    // only the lease still holds the generation.
    h.backend.0.lock().unwrap().processes.clear();
    let instance = &h.registration.instance.instance_id;
    assert!(a
        .reconcile_instance("codespace-runtime", "generation-1", instance)
        .unwrap());
    assert_eq!(
        a.retire_generation("codespace-runtime", "generation-1")
            .unwrap_err()
            .code,
        ErrorCode::ReconciliationRequired
    );
    assert_eq!(
        a.reconcile_lease(&lease, false).unwrap().phase,
        LeasePhase::Released
    );
    drop(p);
    a.retire_generation("codespace-runtime", "generation-1")
        .unwrap();
}

#[test]
fn a_journal_written_before_leases_gains_their_tables_on_activation() {
    let h = Harness::new();
    let journal = Connection::open(&h.path).unwrap();
    journal
        .execute_batch("DROP TABLE leases; DROP TABLE lease_children;")
        .unwrap();
    drop(journal);
    let (mut a, p) = h.ready();
    let (lease, token) = granted(&mut a, &p, "lease-1", 3_000);
    assert_eq!(
        a.lease_status(&lease, &token).unwrap().lease.phase,
        LeasePhase::Active
    );
}
