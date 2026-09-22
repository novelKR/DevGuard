#![cfg(any(target_os = "macos", target_os = "linux"))]

use devguard_client::protocol::{
    Frame, Hello, Request, Response, FRAME_DEADLINE_MS, MAX_FRAME_BYTES, MAX_SESSIONS, WIRE_VERSION,
};
use devguard_client::Client;
use devguard_client::{connect::connect_timeout, framing, peer, protocol::PeerIdentity};
use devguard_contract::{Compatibility, ErrorCode, PROTOCOL_VERSION};
use std::collections::BTreeSet;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_MODE: &str = "DEVGUARD_PEER_TEST_MODE";
const ENDPOINT: &str = "DEVGUARD_PEER_TEST_ENDPOINT";
const PARENT_PID: &str = "DEVGUARD_PEER_TEST_PARENT_PID";

#[test]
fn both_socket_ends_observe_the_other_process_without_caller_supplied_identity() {
    // Keep AF_UNIX paths short on macOS, independent of its long default TMPDIR.
    let directory = tempfile::Builder::new()
        .prefix("dg-peer-")
        .tempdir_in("/tmp")
        .unwrap();
    let endpoint = directory.path().join("peer.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "peer_connector", "--ignored", "--nocapture"])
        .env(CHILD_MODE, "connector")
        .env(ENDPOINT, &endpoint)
        .env(PARENT_PID, std::process::id().to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let child_pid = child.id();
    assert_ne!(child_pid, std::process::id());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if child.try_wait().unwrap().is_some() || Instant::now() >= deadline {
                    let _ = child.kill();
                    let output = child.wait_with_output().unwrap();
                    panic!(
                        "peer child failed before connect: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("test listener failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    let observed = peer::observe(&stream).unwrap();
    assert_eq!(observed.pid, child_pid);
    // SAFETY: geteuid has no arguments or memory ownership effects.
    assert_eq!(observed.uid, unsafe { libc::geteuid() });
    let child_observation: PeerIdentity =
        framing::read_frame(&mut stream, Duration::from_secs(1)).unwrap();
    assert_eq!(child_observation.pid, std::process::id());
    assert_eq!(child_observation.uid, observed.uid);
    // Keep the connecting process alive until both native observations finish.
    framing::write_frame(&mut stream, &true, Duration::from_secs(1)).unwrap();
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("peer subprocess exceeded its test deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "spawned by the native peer integration test"]
fn peer_connector() {
    assert_eq!(std::env::var(CHILD_MODE).unwrap(), "connector");
    let endpoint = std::env::var_os(ENDPOINT).unwrap();
    let mut stream =
        connect_timeout(std::path::Path::new(&endpoint), Duration::from_secs(1)).unwrap();
    let raw = stream.as_raw_fd();
    assert!(raw >= 3);
    // SAFETY: stream owns this live socket throughout both non-mutating calls.
    assert_ne!(
        unsafe { libc::fcntl(raw, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    assert_eq!(
        unsafe { libc::fcntl(raw, libc::F_GETFL) } & libc::O_NONBLOCK,
        0
    );
    let observed = peer::observe(&stream).unwrap();
    assert_eq!(
        observed.pid,
        std::env::var(PARENT_PID).unwrap().parse::<u32>().unwrap()
    );
    framing::write_frame(&mut stream, &observed, Duration::from_secs(1)).unwrap();
    assert!(framing::read_frame::<bool>(&mut stream, Duration::from_secs(1)).unwrap());
}

#[test]
fn invalid_endpoint_or_connect_budget_is_rejected_before_connecting() {
    for (endpoint, budget) in [
        (
            std::path::Path::new("relative.sock"),
            Duration::from_millis(250),
        ),
        (std::path::Path::new("/tmp/unused.sock"), Duration::ZERO),
        (
            std::path::Path::new("/tmp/unused.sock"),
            Duration::from_secs(6),
        ),
    ] {
        assert_eq!(
            connect_timeout(endpoint, budget).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
    let long_endpoint = format!("/tmp/{}", "a".repeat(200));
    assert_eq!(
        connect_timeout(
            std::path::Path::new(&long_endpoint),
            Duration::from_millis(250)
        )
        .unwrap_err()
        .kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[test]
fn client_rejects_new_wire_versions_reply_identity_and_forged_peer_information() {
    // Frame deserialization preserves an integer version; Client must explicitly
    // reject a mismatch before accepting the response as a successful handshake.
    for scenario in [
        "version",
        "request_id",
        "authority_pid",
        "caller_pid",
        "bounds",
    ] {
        let directory = tempfile::Builder::new()
            .prefix("dg-wire-")
            .tempdir_in("/tmp")
            .unwrap();
        let endpoint = directory.path().join("wire.sock");
        let listener = UnixListener::bind(&endpoint).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request: Frame<Request> =
                framing::read_frame(&mut stream, Duration::from_secs(1)).unwrap();
            assert!(matches!(request.body, Request::Hello { .. }));
            let observed = peer::observe(&stream).unwrap();
            let mut hello = Hello {
                protocol: PROTOCOL_VERSION,
                authority: PeerIdentity {
                    uid: observed.uid,
                    pid: std::process::id(),
                },
                caller: observed,
                capabilities: BTreeSet::new(),
                max_frame_bytes: MAX_FRAME_BYTES,
                frame_deadline_ms: FRAME_DEADLINE_MS,
                max_sessions: MAX_SESSIONS,
            };
            if scenario == "authority_pid" {
                hello.authority.pid += 1;
            }
            if scenario == "caller_pid" {
                hello.caller.pid += 1;
            }
            if scenario == "bounds" {
                hello.max_sessions += 1;
            }
            framing::write_frame(
                &mut stream,
                &Frame {
                    version: if scenario == "version" {
                        WIRE_VERSION + 1
                    } else {
                        WIRE_VERSION
                    },
                    request_id: if scenario == "request_id" {
                        request.request_id + 1
                    } else {
                        request.request_id
                    },
                    body: Response::Hello(hello),
                },
                Duration::from_secs(1),
            )
            .unwrap();
        });
        // SAFETY: geteuid only observes this process's effective UID.
        let error = Client::connect(
            &endpoint,
            unsafe { libc::geteuid() },
            Compatibility {
                minimum_protocol: PROTOCOL_VERSION,
                maximum_protocol: PROTOCOL_VERSION,
                required: BTreeSet::new(),
            },
        )
        .err()
        .expect("invalid handshake was accepted");
        assert_eq!(
            error.code,
            if matches!(scenario, "version" | "request_id") {
                ErrorCode::ResourceControlUnavailable
            } else {
                ErrorCode::Unauthorized
            }
        );
        server.join().unwrap();
    }
}
