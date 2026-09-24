//! Candidate authorities: an isolated authority whose whole capacity is a
//! parent lease, used to verify a candidate build before it becomes a parent.
//!
//! A candidate runs as a child workload of its parent's lease, under the
//! parent's launcher. It reads the lease token from a private descriptor,
//! confirms with the parent that the lease is active and holds its capacity,
//! initializes disposable state in its own area and serves admission within
//! that capacity. It never observes the host's capacity for its policy,
//! launches nothing and holds no parent leases, and it never opens the normal
//! state, credentials, releases or recovery copies. It closes as soon as its
//! lease is no longer active, and fails closed when its parent cannot confirm
//! the lease.

use crate::config;
use crate::paths::AuthorityPaths;
use crate::server::{receipt, CandidateMode, Options, Server};
use devguard_client::Client;
use devguard_contract::{
    AttemptKey, Budget, Capability, Compatibility, Error, ErrorCode, LeasePhase, LeaseView, Result,
    Secret,
};
use devguard_macos::HostProbe;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often a candidate confirms its lease with its parent.
pub const LEASE_CHECK_INTERVAL: Duration = Duration::from_secs(1);
/// How long a candidate keeps serving while its parent cannot confirm the
/// lease. A candidate never outlives its lease by more than this.
pub const LEASE_CONFIRMATION_LIMIT: Duration = Duration::from_secs(5);

/// What a candidate is started with. The lease token is passed separately,
/// on a private descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateSpec {
    pub id: String,
    pub lease: AttemptKey,
    pub capacity: Budget,
}

impl CandidateSpec {
    /// `--id ID --lease CONSUMER/GENERATION/ATTEMPT --capacity MILLICPU,BYTES,TASKS`,
    /// each exactly once and in any order.
    pub fn parse(args: &[String]) -> Result<(Self, i32)> {
        let usage = || {
            Error::new(
                ErrorCode::InvalidRequest,
                "candidate takes --id ID --lease CONSUMER/GENERATION/ATTEMPT --capacity MILLICPU,BYTES,TASKS --token-fd N",
            )
        };
        let (mut id, mut lease, mut capacity, mut fd) = (None, None, None, None);
        let mut values = args.iter();
        while let Some(option) = values.next() {
            let value = values.next().ok_or_else(usage)?;
            let slot_taken = match option.as_str() {
                "--id" => id.replace(value.clone()).is_some(),
                "--lease" => lease.replace(parse_key(value)?).is_some(),
                "--capacity" => capacity.replace(parse_budget(value)?).is_some(),
                "--token-fd" => fd
                    .replace(value.parse::<i32>().map_err(|_| usage())?)
                    .is_some(),
                _ => return Err(usage()),
            };
            if slot_taken {
                return Err(usage());
            }
        }
        let spec = Self {
            id: id.ok_or_else(usage)?,
            lease: lease.ok_or_else(usage)?,
            capacity: capacity.ok_or_else(usage)?,
        };
        spec.validate()?;
        let fd = fd.filter(|fd| *fd > 2).ok_or_else(usage)?;
        Ok((spec, fd))
    }

    pub fn validate(&self) -> Result<()> {
        self.lease.validate()?;
        self.capacity.validate_workload()?;
        // The id is checked where the candidate's paths are derived from it.
        Ok(())
    }

    /// The candidate's status reason names it and its parent lease.
    pub fn reason(&self) -> String {
        format!(
            "candidate {} within parent lease {}; admission only, bounded by the leased capacity; nothing is launched here",
            self.id,
            key_text(&self.lease)
        )
    }
}

/// `CONSUMER/GENERATION/ATTEMPT`. Identifiers never contain `/`.
pub fn parse_key(value: &str) -> Result<AttemptKey> {
    let parts: Vec<&str> = value.split('/').collect();
    let [consumer, generation, attempt] = parts[..] else {
        return Err(Error::new(
            ErrorCode::InvalidRequest,
            "a lease is CONSUMER/GENERATION/ATTEMPT",
        ));
    };
    let key = AttemptKey {
        consumer_id: consumer.into(),
        consumer_generation: generation.into(),
        attempt_id: attempt.into(),
    };
    key.validate()?;
    Ok(key)
}

pub fn key_text(key: &AttemptKey) -> String {
    format!(
        "{}/{}/{}",
        key.consumer_id, key.consumer_generation, key.attempt_id
    )
}

/// `MILLICPU,BYTES,TASKS`, each a positive whole number.
pub fn parse_budget(value: &str) -> Result<Budget> {
    let invalid = || {
        Error::new(
            ErrorCode::InvalidRequest,
            "a capacity is MILLICPU,BYTES,TASKS in whole numbers",
        )
    };
    let parts: Vec<u64> = value
        .split(',')
        .map(|part| {
            if part.is_empty() || part.len() > 20 || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            part.parse().map_err(|_| invalid())
        })
        .collect::<Result<_>>()?;
    let [cpu_milli, memory_bytes, tasks] = parts[..] else {
        return Err(invalid());
    };
    let budget = Budget {
        cpu_milli,
        memory_bytes,
        tasks,
    };
    budget.validate_workload()?;
    Ok(budget)
}

pub fn budget_text(budget: Budget) -> String {
    format!(
        "{},{},{}",
        budget.cpu_milli, budget.memory_bytes, budget.tasks
    )
}

/// The handshake a lease holder makes: the parent must state parent leases.
pub fn parent_compatibility() -> Compatibility {
    Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 1,
        required: BTreeSet::from([Capability::ParentLease]),
    }
}

/// The lease as its parent reports it to a token holder.
pub fn lease_status(
    parent: &AuthorityPaths,
    lease: &AttemptKey,
    token: &Secret,
) -> Result<LeaseView> {
    let mut client = Client::connect(&parent.socket(), parent.uid(), parent_compatibility())?;
    client.lease_status(lease.clone(), token.clone())
}

/// A candidate ready to serve.
pub struct Candidate {
    server: Server,
    parent: AuthorityPaths,
    paths: AuthorityPaths,
    spec: CandidateSpec,
    token: Secret,
}

/// Why a candidate stopped serving.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "closure")]
pub enum Closure {
    /// It was asked to stop.
    Stopped,
    /// Its lease is no longer active.
    LeaseEnded { phase: LeasePhase },
    /// Its parent could not confirm the lease within the limit.
    Unconfirmed { error: Error },
}

impl Candidate {
    /// Confirm the lease with the parent at `parent`, then initialize the
    /// candidate's own state and open its service. The capacity must fit the
    /// lease; the parent charges it when it admits the candidate as a child.
    pub fn open(
        parent: &AuthorityPaths,
        spec: CandidateSpec,
        token: Secret,
        probe: Option<Box<dyn HostProbe>>,
    ) -> Result<Self> {
        spec.validate()?;
        let paths = parent.candidate(&spec.id)?;
        // Checked before anything is created: the candidate's own daemon
        // keeps a reservation out of the capacity.
        spec.capacity
            .remaining_after(config::candidate_reservation(
                config::BOOTSTRAP_SYSTEM_TASKS,
            ))
            .validate_workload()
            .map_err(|_| config::too_small_for_a_candidate())?;
        let view = lease_status(parent, &spec.lease, &token)?;
        if view.lease.phase != LeasePhase::Active {
            return Err(Error::new(
                ErrorCode::InvalidTransition,
                "the parent lease is not active",
            ));
        }
        if !spec.capacity.fits(view.lease.budget) {
            return Err(Error::new(
                ErrorCode::ResourceUnavailable,
                "the candidate capacity exceeds its parent lease",
            ));
        }
        // Each candidate starts from new state; an earlier candidate's area is
        // never reused, whatever it holds.
        if std::fs::symlink_metadata(paths.root()).is_ok() {
            return Err(Error::new(
                ErrorCode::InvalidRequest,
                "the candidate area already exists; each candidate uses a new id",
            ));
        }
        config::initialize(&paths)?;
        let server = Server::open_with(
            &paths,
            Options {
                probe,
                reconcile_paused: None,
                candidate: Some(CandidateMode {
                    capacity: spec.capacity,
                    reason: spec.reason(),
                }),
            },
        )?;
        receipt(json!({"event": "candidate_opened", "candidate": spec,
                       "parent_lease": view.lease, "remaining": view.remaining,
                       "state": paths.root(), "socket": paths.socket()}));
        Ok(Self {
            server,
            parent: parent.clone(),
            paths,
            spec,
            token,
        })
    }

    pub fn paths(&self) -> &AuthorityPaths {
        &self.paths
    }

    /// Serve until `stop` is set or the lease is no longer confirmed active.
    pub fn serve(self, stop: Arc<AtomicBool>) -> Result<Closure> {
        let Self {
            server,
            parent,
            spec,
            token,
            ..
        } = self;
        let watched = stop.clone();
        let monitor = std::thread::Builder::new()
            .name("devguard-lease".into())
            .spawn(move || {
                let closure = watch_lease(&parent, &spec.lease, &token, &watched);
                // Whatever ended the watch ends the service.
                watched.store(true, Ordering::Relaxed);
                closure
            })
            .map_err(|_| {
                Error::new(
                    ErrorCode::ResourceControlUnavailable,
                    "cannot start the lease monitor",
                )
            })?;
        let served = server.run(stop.clone());
        stop.store(true, Ordering::Relaxed);
        let closure = monitor.join().unwrap_or_else(|_| Closure::Unconfirmed {
            error: Error::new(
                ErrorCode::ResourceControlUnavailable,
                "the lease monitor failed",
            ),
        });
        receipt(json!({"event": "candidate_closed", "closure": closure}));
        served.map(|()| closure)
    }
}

/// Confirm the lease every interval until it ends, the parent cannot confirm
/// it for the limit, or `stop` is set.
fn watch_lease(
    parent: &AuthorityPaths,
    lease: &AttemptKey,
    token: &Secret,
    stop: &AtomicBool,
) -> Closure {
    let mut confirmed = Instant::now();
    loop {
        let next = Instant::now() + LEASE_CHECK_INTERVAL;
        while Instant::now() < next {
            if stop.load(Ordering::Relaxed) {
                return Closure::Stopped;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        match lease_status(parent, lease, token) {
            Ok(view) if view.lease.phase == LeasePhase::Active => confirmed = Instant::now(),
            Ok(view) => {
                return Closure::LeaseEnded {
                    phase: view.lease.phase,
                }
            }
            // An unknown lease or a refused token is final.
            Err(error) if error.code == ErrorCode::Unauthorized => {
                return Closure::Unconfirmed { error }
            }
            Err(error) if confirmed.elapsed() >= LEASE_CONFIRMATION_LIMIT => {
                return Closure::Unconfirmed { error }
            }
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_candidate_is_named_by_its_id_lease_capacity_and_token_descriptor() {
        let (spec, fd) = CandidateSpec::parse(&args(&[
            "--lease",
            "dev-cli/g-1/lease-1",
            "--id",
            "c-1",
            "--capacity",
            "1000,1073741824,64",
            "--token-fd",
            "5",
        ]))
        .unwrap();
        assert_eq!(fd, 5);
        assert_eq!(spec.id, "c-1");
        assert_eq!(key_text(&spec.lease), "dev-cli/g-1/lease-1");
        assert_eq!(budget_text(spec.capacity), "1000,1073741824,64");
        assert!(spec.reason().contains("c-1") && spec.reason().contains("dev-cli/g-1/lease-1"));
        for invalid in [
            &[
                "--id",
                "c-1",
                "--lease",
                "dev-cli/g-1/lease-1",
                "--capacity",
                "1,1,1",
            ][..],
            &[
                "--id",
                "c-1",
                "--lease",
                "dev-cli/g-1",
                "--capacity",
                "1,1,1",
                "--token-fd",
                "5",
            ],
            &[
                "--id",
                "c-1",
                "--lease",
                "a/b/c",
                "--capacity",
                "1,1",
                "--token-fd",
                "5",
            ],
            &[
                "--id",
                "c-1",
                "--lease",
                "a/b/c",
                "--capacity",
                "0,1,1",
                "--token-fd",
                "5",
            ],
            &[
                "--id",
                "c-1",
                "--lease",
                "a/b/c",
                "--capacity",
                "1,1,1",
                "--token-fd",
                "2",
            ],
            &[
                "--id",
                "c-1",
                "--id",
                "c-2",
                "--lease",
                "a/b/c",
                "--capacity",
                "1,1,1",
                "--token-fd",
                "5",
            ],
            &[
                "--id",
                "c-1",
                "--lease",
                "a/b/c",
                "--capacity",
                "1,1,1",
                "--token-fd",
                "5",
                "--socket",
                "/tmp/x",
            ],
        ] {
            assert!(CandidateSpec::parse(&args(invalid)).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn candidate_paths_stay_inside_the_candidate_area() {
        let directory = tempfile::tempdir().unwrap();
        let parent = AuthorityPaths::fixture(directory.path());
        let candidate = parent.candidate("c-1").unwrap();
        assert!(candidate.root().starts_with(parent.candidates()));
        assert!(candidate.socket().starts_with(parent.candidate_runtime()));
        for path in [
            candidate.journal(),
            candidate.config(),
            candidate.cli_credential(),
            candidate.admin_credential(),
        ] {
            assert!(
                path.starts_with(parent.candidates().join("c-1")),
                "{path:?}"
            );
        }
        assert_ne!(candidate.journal(), parent.journal());
        assert_ne!(candidate.socket(), parent.socket());
        for invalid in ["", "C-1", "../state", "a/b", &"c".repeat(33)] {
            assert!(parent.candidate(invalid).is_err(), "{invalid:?}");
        }
    }

    /// Candidates of a fixture parent, served in this process.
    #[cfg(target_os = "macos")]
    mod served {
        use super::*;
        use crate::fixture::{healthy_probe, TestAuthority, CONSUMER};
        use crate::paths::read_private;
        use devguard_client::protocol::CallerCredential;
        use devguard_contract::{AdmissionRequest, AttemptPhase, ResourceIntent, ResourceLevels};

        const MIB: u64 = 1024 * 1024;

        struct Parent {
            authority: TestAuthority,
            _directory: tempfile::TempDir,
        }

        fn parent() -> Parent {
            let directory = tempfile::Builder::new()
                .prefix("dg-cd-")
                .tempdir_in("/private/tmp")
                .unwrap();
            let authority = TestAuthority::start(directory.path()).unwrap();
            authority
                .wait_until_admitting(Duration::from_secs(10))
                .unwrap();
            Parent {
                authority,
                _directory: directory,
            }
        }

        fn compatibility(required: &[Capability]) -> Compatibility {
            Compatibility {
                minimum_protocol: 1,
                maximum_protocol: 1,
                required: required.iter().copied().collect(),
            }
        }

        fn owner(parent: &Parent) -> Client {
            let mut client = Client::connect(
                &parent.authority.socket(),
                parent.authority.uid(),
                compatibility(&[Capability::DurableAdmission, Capability::ParentLease]),
            )
            .unwrap();
            client
                .authenticate(parent.authority.consumer().unwrap())
                .unwrap();
            client.register("candidate-owner".into()).unwrap();
            client
        }

        fn key(parent: &Parent, id: &str) -> AttemptKey {
            AttemptKey {
                consumer_id: CONSUMER.into(),
                consumer_generation: parent.authority.generation(),
                attempt_id: id.into(),
            }
        }

        /// Fits a 3-CPU host's 500 mCPU of work capacity.
        fn lease_budget() -> Budget {
            Budget {
                cpu_milli: 450,
                memory_bytes: 512 * MIB,
                tasks: 96,
            }
        }

        /// Leaves 150 mCPU, 128 MiB and 16 tasks of work after the
        /// candidate's own reservation.
        fn capacity() -> Budget {
            Budget {
                cpu_milli: 400,
                memory_bytes: 256 * MIB,
                tasks: 64,
            }
        }

        fn lease(parent: &Parent, owner: &mut Client, id: &str) -> (AttemptKey, Secret) {
            let lease = key(parent, id);
            let token = owner
                .admit_lease(lease.clone(), lease_budget(), None)
                .unwrap()
                .token
                .unwrap();
            (lease, token)
        }

        fn request(key: AttemptKey, requested: Budget) -> AdmissionRequest {
            AdmissionRequest {
                key,
                execution_digest: devguard_contract::digest_bytes(b"candidate smoke"),
                intent: ResourceIntent {
                    profile: "interactive".into(),
                    requested,
                    minimum: ResourceLevels::MACOS,
                },
            }
        }

        fn candidate_credential(paths: &AuthorityPaths) -> CallerCredential {
            let config = crate::config::HostConfig::load(paths).unwrap();
            let secret = Secret::new(
                String::from_utf8(read_private(&paths.cli_credential(), paths.uid(), 64).unwrap())
                    .unwrap(),
            )
            .unwrap();
            CallerCredential::Consumer {
                consumer_id: CONSUMER.into(),
                generation: config.consumers[CONSUMER].generation.clone(),
                secret,
            }
        }

        #[test]
        fn a_candidate_admits_within_its_lease_launches_nothing_and_closes_with_the_lease() {
            let parent = parent();
            let (lease, token) = lease(&parent, &mut owner(&parent), "lease-1");
            let spec = CandidateSpec {
                id: "c-1".into(),
                lease: lease.clone(),
                capacity: capacity(),
            };
            let candidate =
                Candidate::open(parent.authority.paths(), spec, token, Some(healthy_probe()))
                    .unwrap();
            let paths = candidate.paths().clone();
            assert!(paths
                .root()
                .starts_with(parent.authority.paths().candidates()));
            let stop = Arc::new(AtomicBool::new(false));
            let serving = {
                let stop = stop.clone();
                std::thread::spawn(move || candidate.serve(stop))
            };
            // Admission is stated; launch and parent leases are not.
            let mut client = Client::connect(
                &paths.socket(),
                paths.uid(),
                compatibility(&[Capability::DurableAdmission, Capability::MacosCooperative]),
            )
            .unwrap();
            assert!(!client
                .hello
                .capabilities
                .contains(&Capability::FencedLaunch));
            assert!(!client.hello.capabilities.contains(&Capability::ParentLease));
            for required in [Capability::FencedLaunch, Capability::ParentLease] {
                assert!(
                    Client::connect(&paths.socket(), paths.uid(), compatibility(&[required]))
                        .is_err()
                );
            }
            // The parent's caller credential is not the candidate's.
            let mut stranger =
                Client::connect(&paths.socket(), paths.uid(), compatibility(&[])).unwrap();
            assert_eq!(
                stranger
                    .authenticate(parent.authority.consumer().unwrap())
                    .unwrap_err()
                    .code,
                ErrorCode::Unauthorized
            );
            client.authenticate(candidate_credential(&paths)).unwrap();
            let status = client.status().unwrap();
            assert!(status.registration_ready && !status.execution_ready);
            assert!(status.reason.contains("c-1") && status.reason.contains(&key_text(&lease)));
            // Every frame has a short deadline, so each step uses a new session.
            let smoke = || {
                let mut client =
                    Client::connect(&paths.socket(), paths.uid(), compatibility(&[])).unwrap();
                client.authenticate(candidate_credential(&paths)).unwrap();
                client.register("smoke".into()).unwrap();
                client
            };
            let within = Budget {
                cpu_milli: 100,
                memory_bytes: 64 * MIB,
                tasks: 4,
            };
            let generation = crate::config::HostConfig::load(&paths).unwrap().consumers[CONSUMER]
                .generation
                .clone();
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut attempt = 0;
            let admitted = loop {
                attempt += 1;
                let key = AttemptKey {
                    consumer_id: CONSUMER.into(),
                    consumer_generation: generation.clone(),
                    attempt_id: format!("within-{attempt}"),
                };
                let record = smoke().admit(request(key, within)).unwrap();
                if record.phase == AttemptPhase::Prepared {
                    break record;
                }
                // A new authority starts closed until its pressure is normal.
                assert!(Instant::now() < deadline, "{record:?}");
                std::thread::sleep(Duration::from_millis(250));
            };
            let mut client = smoke();
            assert_eq!(
                client.begin_launch(admitted.key.clone()).unwrap_err().code,
                ErrorCode::ResourcePolicyUnsupported
            );
            let beyond = AttemptKey {
                attempt_id: "beyond".into(),
                ..admitted.key.clone()
            };
            let denied = client.admit(request(beyond, capacity())).unwrap();
            assert_eq!(denied.denial, Some(ErrorCode::ResourceUnavailable));
            assert_eq!(
                client
                    .admit_lease(
                        AttemptKey {
                            attempt_id: "nested".into(),
                            ..admitted.key.clone()
                        },
                        within,
                        None
                    )
                    .unwrap_err()
                    .code,
                ErrorCode::ResourcePolicyUnsupported
            );
            // The candidate charged nothing at the parent.
            assert_eq!(parent.authority.committed().unwrap(), lease_budget());
            // Ending the lease closes the candidate.
            owner(&parent).end_lease(lease).unwrap();
            let closed = Instant::now();
            let closure = serving.join().unwrap().unwrap();
            assert!(closed.elapsed() < LEASE_CHECK_INTERVAL * 3);
            assert!(
                matches!(closure, Closure::LeaseEnded { phase } if phase != LeasePhase::Active),
                "{closure:?}"
            );
            assert!(!paths.socket().exists());
        }

        #[test]
        fn a_candidate_needs_an_active_lease_its_token_a_fitting_capacity_and_a_new_area() {
            let parent = parent();
            let (lease, token) = lease(&parent, &mut owner(&parent), "lease-1");
            let open = |id: &str, lease: &AttemptKey, token: &Secret, capacity: Budget| {
                Candidate::open(
                    parent.authority.paths(),
                    CandidateSpec {
                        id: id.into(),
                        lease: lease.clone(),
                        capacity,
                    },
                    token.clone(),
                    Some(healthy_probe()),
                )
                .map(|_| ())
                .unwrap_err()
                .code
            };
            let forged = Secret::new("f".repeat(64)).unwrap();
            assert_eq!(
                open("c-1", &lease, &forged, capacity()),
                ErrorCode::Unauthorized
            );
            let unknown = key(&parent, "no-such-lease");
            assert_eq!(
                open("c-1", &unknown, &token, capacity()),
                ErrorCode::Unauthorized
            );
            let larger = Budget {
                cpu_milli: lease_budget().cpu_milli + 1,
                ..capacity()
            };
            assert_eq!(
                open("c-1", &lease, &token, larger),
                ErrorCode::ResourceUnavailable
            );
            let too_small = Budget {
                cpu_milli: 250,
                ..capacity()
            };
            assert_eq!(
                open("c-1", &lease, &token, too_small),
                ErrorCode::ResourceUnavailable
            );
            // Nothing was created by a refused candidate.
            assert!(!parent.authority.paths().candidates().join("c-1").exists());
            // An earlier candidate's area is never reused.
            std::fs::create_dir_all(parent.authority.paths().candidates().join("c-2")).unwrap();
            assert_eq!(
                open("c-2", &lease, &token, capacity()),
                ErrorCode::InvalidRequest
            );
            // An ended lease starts no candidate.
            owner(&parent).end_lease(lease.clone()).unwrap();
            assert_eq!(
                open("c-3", &lease, &token, capacity()),
                ErrorCode::InvalidTransition
            );
        }
    }
}
