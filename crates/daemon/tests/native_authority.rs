#![cfg(target_os = "macos")]
//! OS-observed socket peers joined with native start identity (DG1-C03).
//! The service still refuses registration over the wire until DG1-P3.

use devguard_client::peer;
use devguard_contract::*;
use devguard_core::*;
use devguard_macos::{BootClock, NativeBackend, NativeHost};
use std::collections::BTreeMap;
use std::os::unix::net::UnixStream;

const GIB: u64 = 1024 * 1024 * 1024;

fn authority(
    directory: &tempfile::TempDir,
    host: &NativeHost,
    uid: u32,
    credential: &Secret,
) -> Authority<NativeBackend, BootClock> {
    let path = directory.path().join("authority.sqlite");
    Authority::<NativeBackend, BootClock>::initialize_journal(&path).unwrap();
    let policy = Policy {
        revision: "native-peer".into(),
        effective_capacity: Budget {
            cpu_milli: 8_000,
            memory_bytes: 16 * GIB,
            tasks: 256,
        },
        host_headroom: Budget {
            cpu_milli: 2_000,
            memory_bytes: 4 * GIB,
            tasks: 64,
        },
        system_reservation: Budget {
            cpu_milli: 500,
            memory_bytes: GIB / 4,
            tasks: 48,
        },
        consumers: BTreeMap::from([(
            "peer-test".into(),
            ConsumerDefinition {
                generation: "generation-1".into(),
                uid,
                role: ConsumerRole::Workload,
                credential_digest: credential.digest(),
                max_instances: 1,
                control_reservation: Budget::ZERO,
            },
        )]),
    };
    Authority::open(&path, policy, host.backend(), host.clock()).unwrap()
}

fn registration(process: ProcessIdentity, credential: &Secret) -> Registration {
    Registration {
        consumer_id: "peer-test".into(),
        generation: "generation-1".into(),
        instance: InstanceIdentity {
            instance_id: "peer-instance".into(),
            process,
        },
        credential: credential.clone(),
    }
}

#[test]
fn an_observed_socket_peer_registers_only_with_its_native_start_identity() {
    let (server_side, _client_side) = UnixStream::pair().unwrap();
    let observed = peer::observe(&server_side).unwrap();
    assert_eq!(observed.pid, std::process::id());
    let host = NativeHost::open().unwrap();
    let identity = host
        .backend()
        .process_identity(observed.pid)
        .unwrap()
        .unwrap();
    assert_eq!(identity.boot_id, host.clock().boot_id());
    let directory = tempfile::tempdir().unwrap();
    let credential = Secret::new("c".repeat(64)).unwrap();
    let mut authority = authority(&directory, &host, observed.uid, &credential);
    let peer = TrustedPeer {
        uid: observed.uid,
        pid: observed.pid,
    };
    // Another UID, a different start or a stale boot cannot use this peer.
    let mut restarted = identity.clone();
    restarted.start_ticks -= 1;
    let mut previous_boot = identity.clone();
    previous_boot.boot_id = "00000000-0000-0000-0000-000000000000".into();
    for (peer, process) in [
        (
            TrustedPeer {
                uid: observed.uid + 1,
                pid: observed.pid,
            },
            identity.clone(),
        ),
        (peer, restarted),
        (peer, previous_boot),
    ] {
        assert_eq!(
            authority
                .register(peer, registration(process, &credential))
                .unwrap_err()
                .code,
            ErrorCode::Unauthorized
        );
    }
    let principal = authority
        .register(peer, registration(identity.clone(), &credential))
        .unwrap();
    assert_eq!(principal.instance().process, identity);
    // The single configured slot is now occupied by this live instance.
    let mut second = registration(identity, &credential);
    second.instance.instance_id = "second-instance".into();
    assert_eq!(
        authority.register(peer, second).unwrap_err().code,
        ErrorCode::ResourceUnavailable
    );
}
