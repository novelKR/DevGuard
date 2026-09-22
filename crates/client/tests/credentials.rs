//! Private-FD transport checks, not qualification of the future launch helper.
use devguard_client::credential::{read_owned, take_inherited, CredentialHandoff};
use devguard_client::protocol::CallerCredential;
use devguard_contract::{ErrorCode, Secret};
use std::io::Write;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const TEST_SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CHILD_MODE: &str = "DEVGUARD_CREDENTIAL_TEST_MODE";
const CHILD_FD: &str = "DEVGUARD_CREDENTIAL_TEST_FD";

fn assert_closed(fd: i32) {
    // SAFETY: F_GETFD only inspects the numeric descriptor; it does not consume it.
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EBADF)
    );
}

fn wait_bounded(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "credential subprocess exceeded its test deadline: {:?}",
                output.status
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn child_command(test: &str, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--ignored", "--nocapture"])
        .env(CHILD_MODE, mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[test]
fn credential_crosses_only_its_private_fd_and_is_closed_before_later_exec() {
    let secret = Secret::new(TEST_SECRET.into()).unwrap();
    let handoff = CredentialHandoff::new(&secret).unwrap();
    let mut command = child_command("credential_receiver", "receiver");
    let raw = handoff.attach(&mut command);
    command.env(CHILD_FD, raw.to_string());
    assert!(raw >= 3);
    // SAFETY: the Command closure owns this descriptor until Command is dropped.
    let parent_flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
    assert_ne!(parent_flags & libc::FD_CLOEXEC, 0);
    assert!(!format!("{command:?}").contains(TEST_SECRET));
    let child = command.spawn().unwrap();
    drop(command);
    assert_closed(raw);
    let output = wait_bounded(child);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("payload has no credential FD"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(TEST_SECRET));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(TEST_SECRET));
}

#[test]
#[ignore = "invoked with a dedicated descriptor by its parent integration test"]
fn credential_receiver() {
    assert_eq!(std::env::var(CHILD_MODE).unwrap(), "receiver");
    assert!(!std::env::args_os().any(|arg| arg.to_string_lossy().contains(TEST_SECRET)));
    assert!(!std::env::vars_os().any(|(key, value)| {
        key.to_string_lossy().contains(TEST_SECRET) || value.to_string_lossy().contains(TEST_SECRET)
    }));
    let fd: i32 = std::env::var(CHILD_FD).unwrap().parse().unwrap();
    // SAFETY: this subprocess exclusively owns the dedicated descriptor inherited
    // through CredentialHandoff; no Rust object has taken its ownership.
    let secret = unsafe { take_inherited(fd) }.unwrap();
    assert!(secret.expose() == TEST_SECRET);
    assert!(!format!("{secret:?}").contains(TEST_SECRET));
    let credential = CallerCredential::Consumer {
        consumer_id: "test-consumer".into(),
        generation: "test-generation".into(),
        secret,
    };
    assert!(!format!("{credential:?}").contains(TEST_SECRET));
    assert_closed(fd);
    drop(credential);

    // This exec occurs only after the transport has consumed and closed the FD.
    // It checks descriptor hygiene, not a C05 permit or payload-start contract.
    let error = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "credential_payload", "--ignored", "--nocapture"])
        .env(CHILD_MODE, "payload")
        .exec();
    panic!("could not exec descriptor-check payload: {error}");
}

#[test]
#[ignore = "exec target of the private-FD integration test"]
fn credential_payload() {
    assert_eq!(std::env::var(CHILD_MODE).unwrap(), "payload");
    let fd: i32 = std::env::var(CHILD_FD).unwrap().parse().unwrap();
    assert_closed(fd);
    println!("payload has no credential FD");
}

#[test]
fn malformed_credentials_are_redacted_and_always_close_the_owned_fd() {
    let malformed = [
        Vec::new(),
        TEST_SECRET.as_bytes()[..63].to_vec(),
        vec![b'g'; 64],
        vec![0xff; 64],
        [TEST_SECRET.as_bytes(), b"extra"].concat(),
    ];
    for bytes in malformed {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(&bytes).unwrap();
        drop(writer);
        let raw = reader.as_raw_fd();
        let owned: OwnedFd = reader.into();
        let error = read_owned(owned).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert!(!error.to_string().contains(TEST_SECRET));
        assert!(!format!("{error:?}").contains(TEST_SECRET));
        assert_closed(raw);
    }
}

#[test]
fn a_full_secret_without_writer_eof_expires_and_closes_the_fd() {
    let (reader, mut writer) = UnixStream::pair().unwrap();
    writer.write_all(TEST_SECRET.as_bytes()).unwrap();
    let raw = reader.as_raw_fd();
    let began = Instant::now();
    let error = read_owned(reader.into()).unwrap_err();
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert!(began.elapsed() < Duration::from_secs(2));
    assert_closed(raw);
    drop(writer);
}
