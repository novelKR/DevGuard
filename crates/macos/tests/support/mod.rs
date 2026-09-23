#![allow(dead_code)]

use devguard_contract::*;
use devguard_core::*;
use devguard_macos::{BootClock, HostProbe, HostReading, NativeBackend, NativeHost, VolumeReading};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

pub const GIB: u64 = 1024 * 1024 * 1024;
pub type NativeAuthority = Authority<NativeBackend, BootClock>;

/// Qualification runs collect raw receipts; ordinary test runs write nothing.
pub fn record(name: &str, value: serde_json::Value) {
    if let Some(directory) = std::env::var_os("DEVGUARD_EVIDENCE_DIR") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}

/// Independent corroboration through the system `sysctl` tool.
pub fn sysctl(name: &str) -> String {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", name])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }
}

/// Healthy synthetic readings for tests that must not depend on the host's
/// actual pressure, which may legitimately be warning or critical.
pub struct Healthy {
    pub paged_out_bytes: u64,
}

impl HostProbe for Healthy {
    fn read(&mut self) -> Result<HostReading> {
        Ok(HostReading {
            memory_level: 1,
            paged_out_bytes: self.paged_out_bytes,
            swap_used_bytes: 0,
            volumes: vec![VolumeReading {
                mount: "/synthetic".into(),
                capacity_bytes: 100 * GIB,
                available_bytes: 60 * GIB,
            }],
        })
    }
}

pub struct Fixture {
    pub _directory: tempfile::TempDir,
    pub path: PathBuf,
    pub host: NativeHost,
    pub policy: Policy,
    pub credential: Secret,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("authority.sqlite");
        NativeAuthority::initialize_journal(&path).unwrap();
        let credential = Secret::new("b".repeat(64)).unwrap();
        let policy = Policy {
            revision: "native-fixture".into(),
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
                "native-test".into(),
                ConsumerDefinition {
                    generation: "generation-1".into(),
                    uid: uid(),
                    role: ConsumerRole::Workload,
                    credential_digest: credential.digest(),
                    max_instances: 2,
                    control_reservation: Budget::ZERO,
                },
            )]),
        };
        Self {
            _directory: directory,
            path,
            host: NativeHost::open().unwrap(),
            policy,
            credential,
        }
    }

    pub fn open(&self) -> NativeAuthority {
        NativeAuthority::open(
            &self.path,
            self.policy.clone(),
            self.host.backend(),
            self.host.clock(),
        )
        .unwrap()
    }

    pub fn registration(&self, process: ProcessIdentity) -> Registration {
        Registration {
            consumer_id: "native-test".into(),
            generation: "generation-1".into(),
            instance: InstanceIdentity {
                instance_id: "native-instance".into(),
                process,
            },
            credential: self.credential.clone(),
        }
    }

    /// Register this test process using its OS-observed identity.
    pub fn register_self(&self, authority: &mut NativeAuthority) -> Principal {
        let pid = std::process::id();
        let identity = self.host.backend().process_identity(pid).unwrap().unwrap();
        authority
            .register(TrustedPeer { uid: uid(), pid }, self.registration(identity))
            .unwrap()
    }
}

pub fn request(id: &str) -> AdmissionRequest {
    AdmissionRequest {
        key: AttemptKey {
            consumer_id: "native-test".into(),
            consumer_generation: "generation-1".into(),
            attempt_id: id.into(),
        },
        execution_digest: digest_bytes(b"native fixture command"),
        intent: ResourceIntent {
            profile: "interactive".into(),
            requested: Budget {
                cpu_milli: 1,
                memory_bytes: 1,
                tasks: 1,
            },
            minimum: ResourceLevels::MACOS,
        },
    }
}
