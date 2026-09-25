//! DG1-C11 quiescence: what an upgrade needs from the authority core. Closing
//! admission cancels only Prepared attempts; an idle journal reports what it
//! still charges without being activated; and a backup of it is itself a
//! complete journal.

// The shared harness also serves the authority contract, whose steps this file does not all use.
#[allow(dead_code)]
mod support;
use devguard_contract::*;
use devguard_core::*;
use rusqlite::Connection;
use std::os::unix::fs::OpenOptionsExt;
use support::*;

fn lease_key(id: &str) -> AttemptKey {
    AttemptKey {
        consumer_id: "codespace-runtime".into(),
        consumer_generation: "generation-1".into(),
        attempt_id: id.into(),
    }
}

/// A request smaller than the harness's, so several fit the host.
fn small(id: &str) -> AdmissionRequest {
    let mut request = request(id);
    request.intent.requested.cpu_milli = 1_000;
    request
}

fn empty_private_file(path: &std::path::Path) {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
}

#[test]
fn closing_admission_cancels_only_prepared_attempts_and_returns_their_budget() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let prepared = a.admit(&p, small("prepared")).unwrap();
    assert_eq!(prepared.phase, AttemptPhase::Prepared);
    let committed = a.admit(&p, small("committed")).unwrap();
    a.begin_launch(&p, &committed.key).unwrap();
    // A Prepared child of a lease returns its budget to the lease.
    let lease = lease_key("lease-1");
    let token = a
        .admit_lease(
            &p,
            &lease,
            Budget {
                cpu_milli: 3_000,
                memory_bytes: 8 * GIB,
                tasks: 256,
            },
            None,
        )
        .unwrap()
        .token
        .unwrap();
    let child = a.admit_child(&p, &lease, &token, request("child")).unwrap();
    assert_eq!(child.phase, AttemptPhase::Prepared);
    let before = a.committed_budget().unwrap();
    let cancelled = a.cancel_prepared().unwrap();
    let keys: Vec<&str> = cancelled
        .iter()
        .map(|r| r.key.attempt_id.as_str())
        .collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"prepared") && keys.contains(&"child"));
    for record in &cancelled {
        assert_eq!(record.phase, AttemptPhase::Cancelled);
        assert!(record.known_not_started());
    }
    // The committed launch is untouched and still charged; the lease keeps
    // its whole budget, now with nothing drawn on it.
    let remaining: Vec<AttemptRecord> = a.attempts().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].key, committed.key);
    assert_eq!(remaining[0].phase, AttemptPhase::LaunchCommitted);
    assert_eq!(
        a.committed_budget().unwrap(),
        before.remaining_after(small("prepared").intent.requested)
    );
    assert_eq!(a.lease(&lease).unwrap().remaining.cpu_milli, 3_000);
    // Nothing is left to cancel.
    assert!(a.cancel_prepared().unwrap().is_empty());
}

#[test]
fn an_idle_journal_reports_its_charges_without_activation_and_backs_up_whole() {
    let h = Harness::new();
    let (mut a, p) = h.ready();
    let committed = a.admit(&p, small("committed")).unwrap();
    a.begin_launch(&p, &committed.key).unwrap();
    let lease = lease_key("lease-1");
    a.admit_lease(
        &p,
        &lease,
        Budget {
            cpu_milli: 2_000,
            memory_bytes: GIB,
            tasks: 16,
        },
        None,
    )
    .unwrap();
    drop(a);
    let mut storage = AuthorityStorage::open(&h.path).unwrap();
    let (attempts, leases) = storage.charged().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].key, committed.key);
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].key, lease);
    // A second storage cannot open while this one holds the lock.
    assert!(AuthorityStorage::open(&h.path).is_err());
    let directory = tempfile::tempdir().unwrap();
    let copy = directory.path().join("journal.sqlite");
    empty_private_file(&copy);
    storage.backup(&copy).unwrap();
    // A backup never overwrites a file that already holds data.
    assert!(storage.backup(&copy).is_err());
    drop(storage);
    let mut restored = AuthorityStorage::open(&copy).unwrap();
    let (copied_attempts, copied_leases) = restored.charged().unwrap();
    assert_eq!(copied_attempts, attempts);
    assert_eq!(copied_leases, leases);
    drop(restored);
    // Reading and backing up changed nothing.
    let mut again = AuthorityStorage::open(&h.path).unwrap();
    assert_eq!(again.charged().unwrap(), (attempts, leases));
}

#[test]
fn a_journal_written_before_leases_reports_none() {
    let h = Harness::new();
    let journal = Connection::open(&h.path).unwrap();
    journal
        .execute_batch("DROP TABLE leases; DROP TABLE lease_children;")
        .unwrap();
    drop(journal);
    let mut storage = AuthorityStorage::open(&h.path).unwrap();
    let (attempts, leases) = storage.charged().unwrap();
    assert!(attempts.is_empty() && leases.is_empty());
    drop(storage);
    // Reporting did not add the tables; activation still does.
    let journal = Connection::open(&h.path).unwrap();
    let tables: u64 = journal
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('leases','lease_children')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
}
