use devguard_contract::ErrorCode;
use devguard_core::AuthorityStorage;
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt};

fn private_lock(path: &std::path::Path) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(b"preserved lock identity").unwrap();
}

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

#[test]
fn storage_bootstrap_rejects_a_fifo_lock_without_creating_a_journal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let lock = directory.path().join("authority.lock");
    let native = std::ffi::CString::new(lock.as_os_str().as_bytes()).unwrap();
    // SAFETY: native is a live, terminated pathname in this private test directory.
    assert_eq!(unsafe { libc::mkfifo(native.as_ptr(), 0o600) }, 0);
    // Keep a nonblocking reader open so this regression also stays bounded if a
    // future change accidentally removes O_NONBLOCK from the write-side open.
    let _reader = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&lock)
        .unwrap();
    assert!(
        matches!(AuthorityStorage::initialize(&path), Err(e) if e.code == ErrorCode::JournalInvalid)
    );
    assert!(!path.exists());
    assert!(fs::symlink_metadata(&lock).is_ok());
}

#[test]
fn storage_bootstrap_rejects_shared_linked_and_symlink_locks() {
    for kind in ["shared", "hard_link", "symlink"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite");
        let lock = directory.path().join("authority.lock");
        let source = directory.path().join("original.lock");
        match kind {
            "shared" => {
                private_lock(&lock);
                fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap();
            }
            "hard_link" => {
                private_lock(&source);
                fs::hard_link(&source, &lock).unwrap();
            }
            "symlink" => {
                private_lock(&source);
                symlink(&source, &lock).unwrap();
            }
            _ => unreachable!(),
        }
        let before = fs::symlink_metadata(&lock).unwrap();
        assert!(
            matches!(AuthorityStorage::initialize(&path), Err(e) if e.code == ErrorCode::JournalInvalid),
            "unsafe {kind} lock was accepted"
        );
        assert!(!path.exists(), "unsafe {kind} lock created a journal");
        let after = fs::symlink_metadata(&lock).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert_eq!(fs::read(&lock).unwrap(), b"preserved lock identity");
        assert_eq!(before.mode(), after.mode());
    }
}

#[test]
fn storage_bootstrap_and_reopen_preserve_an_existing_private_lock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let lock = directory.path().join("authority.lock");
    private_lock(&lock);
    let before = fs::metadata(&lock).unwrap();
    drop(AuthorityStorage::initialize(&path).unwrap());
    drop(AuthorityStorage::open(&path).unwrap());
    let after = fs::metadata(&lock).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    assert_eq!(fs::read(&lock).unwrap(), b"preserved lock identity");
    assert_eq!(after.mode() & 0o777, 0o600);
}
