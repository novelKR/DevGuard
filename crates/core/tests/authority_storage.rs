use devguard_contract::ErrorCode;
use devguard_core::AuthorityStorage;
use rusqlite::Connection;

#[test]
fn storage_is_exclusive_without_inventing_a_boot_clock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let first = AuthorityStorage::initialize(&path).unwrap();
    assert!(
        matches!(AuthorityStorage::open(&path), Err(e) if e.code == ErrorCode::ResourceControlUnavailable)
    );
    drop(first);
    let reopened = AuthorityStorage::open(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    let count: u32 = connection
        .query_row("SELECT count(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    drop(reopened);
    assert!(AuthorityStorage::initialize(&path).is_err());
}

#[test]
fn storage_open_does_not_initialize_or_repair_journals() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    assert!(AuthorityStorage::open(&path).is_err());
    assert!(!path.exists());
    std::fs::write(&path, b"corrupt journal evidence").unwrap();
    assert!(AuthorityStorage::open(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt journal evidence");
}

#[test]
fn storage_rejects_unknown_schema_without_changing_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    drop(AuthorityStorage::initialize(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE metadata SET value='2' WHERE name='schema'", [])
        .unwrap();
    assert!(matches!(AuthorityStorage::open(&path), Err(e) if e.code == ErrorCode::JournalInvalid));
    let schema: String = connection
        .query_row("SELECT value FROM metadata WHERE name='schema'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(schema, "2");
}
