use crate::paths::{read_private, write_new_private, AuthorityPaths};
use devguard_contract::{validate_digest, validate_id, Budget, Error, ErrorCode, Result, Secret};
use devguard_core::{AuthorityStorage, ConsumerDefinition, ConsumerRole, Policy};
use devguard_macos::HostCapacity;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use uuid::Uuid;

pub const CONFIG_SCHEMA: u32 = 1;
pub const CONFIG_BYTES: usize = 64 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;
/// Approved control reservations, counted once: the daemon (0.25 CPU, 128 MiB)
/// plus the aggregate CLI control pool (0.25 CPU, 128 MiB for up to eight CLIs).
const SYSTEM_CPU_MILLI: u64 = 250 + 250;
const SYSTEM_MEMORY_BYTES: u64 = (128 + 128) * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerConfig {
    pub generation: String,
    pub role: ConsumerRole,
    pub credential_sha256: String,
    pub max_instances: u32,
    pub control_reservation: Budget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRegistration {
    pub root: PathBuf,
}

/// Operator-only policy. There are deliberately no state/socket/HOME overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub schema: u32,
    pub policy_revision: String,
    pub default_profile: String,
    pub admin_credential_sha256: String,
    /// An operator accounting ceiling, not a macOS kernel task limit.
    pub task_capacity: u64,
    pub task_headroom: u64,
    pub system_tasks: u64,
    #[serde(default)]
    pub additional_headroom: Budget,
    #[serde(default)]
    pub consumers: BTreeMap<String, ConsumerConfig>,
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectRegistration>,
}

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorCode::InvalidRequest, message)
}

impl HostConfig {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > CONFIG_BYTES {
            return Err(invalid("operator configuration exceeds byte limit"));
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| invalid("configuration must be UTF-8"))?;
        // Parser errors can contain source text. Never echo configuration/credentials.
        let config: Self = toml::from_str(text)
            .map_err(|_| invalid("invalid or unsupported operator configuration"))?;
        config.validate()?;
        Ok(config)
    }

    pub fn load(paths: &AuthorityPaths) -> Result<Self> {
        Self::parse(&read_private(&paths.config(), paths.uid(), CONFIG_BYTES)?)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != CONFIG_SCHEMA || self.default_profile != "interactive" {
            return Err(Error::new(
                ErrorCode::ResourcePolicyUnsupported,
                "unsupported configuration schema or profile",
            ));
        }
        validate_id(&self.policy_revision)?;
        validate_digest(&self.admin_credential_sha256)?;
        if self.system_tasks < devguard_client::protocol::MAX_SESSIONS as u64 + 16 {
            return Err(invalid(
                "system task accounting must cover bounded sessions and CLI control",
            ));
        }
        if self.task_capacity == 0
            || self
                .task_headroom
                .checked_add(self.system_tasks)
                .is_none_or(|n| n >= self.task_capacity)
        {
            return Err(invalid(
                "task accounting must leave positive workload capacity",
            ));
        }
        if self.consumers.len() > 64 || self.projects.len() > 256 {
            return Err(invalid("configuration inventory exceeds supported bounds"));
        }
        let mut reservations = Budget::ZERO;
        let mut credentials = BTreeSet::from([self.admin_credential_sha256.clone()]);
        for (id, consumer) in &self.consumers {
            validate_id(id)?;
            validate_id(&consumer.generation)?;
            validate_digest(&consumer.credential_sha256)?;
            if consumer.max_instances == 0 || consumer.max_instances > 64 {
                return Err(invalid("consumer instance count must be 1..64"));
            }
            if !credentials.insert(consumer.credential_sha256.clone()) {
                return Err(invalid(
                    "credentials must differ across administration and consumers",
                ));
            }
            match consumer.role {
                ConsumerRole::Workload if consumer.control_reservation != Budget::ZERO => {
                    return Err(invalid("workload cannot reserve control-service capacity"))
                }
                ConsumerRole::ControlService => consumer.control_reservation.validate_workload()?,
                _ => {}
            }
            reservations = reservations.checked_add(
                consumer
                    .control_reservation
                    .checked_mul(consumer.max_instances)?,
            )?;
        }
        for (id, project) in &self.projects {
            validate_id(id)?;
            if !project.root.is_absolute()
                || project
                    .root
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(invalid(
                    "registered project roots must be absolute without parent traversal",
                ));
            }
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<String> {
        Ok(devguard_contract::digest_bytes(
            &serde_json::to_vec(self).map_err(|_| invalid("configuration encoding failed"))?,
        ))
    }

    /// The work capacity this configuration gives the observed host at Normal
    /// pressure. No request larger than this can ever be admitted.
    pub fn observed_work_capacity(&self, uid: u32) -> Result<Budget> {
        self.policy(HostCapacity::observe()?, uid)?.work_capacity()
    }

    /// Core policy for the observed host. Host headroom follows the approved
    /// defaults, max(ceil(25% of logical CPUs), 2 CPUs) and max(25% of physical
    /// memory, 4 GiB), plus any operator headroom; consumers take the account UID.
    pub fn policy(&self, capacity: HostCapacity, uid: u32) -> Result<Policy> {
        let cpus = u64::from(capacity.logical_cpus);
        let overflow = || invalid("host capacity overflow");
        let effective_capacity = Budget {
            cpu_milli: cpus.checked_mul(1_000).ok_or_else(overflow)?,
            memory_bytes: capacity.memory_bytes,
            tasks: self.task_capacity,
        };
        let host_headroom = Budget {
            cpu_milli: cpus.div_ceil(4).max(2) * 1_000,
            memory_bytes: (capacity.memory_bytes / 4).max(4 * GIB),
            tasks: self.task_headroom,
        }
        .checked_add(self.additional_headroom)?;
        let consumers = self
            .consumers
            .iter()
            .map(|(id, consumer)| {
                (
                    id.clone(),
                    ConsumerDefinition {
                        generation: consumer.generation.clone(),
                        uid,
                        role: consumer.role,
                        credential_digest: consumer.credential_sha256.clone(),
                        max_instances: consumer.max_instances,
                        control_reservation: consumer.control_reservation,
                    },
                )
            })
            .collect();
        let policy = Policy {
            revision: self.policy_revision.clone(),
            effective_capacity,
            host_headroom,
            system_reservation: Budget {
                cpu_milli: SYSTEM_CPU_MILLI,
                memory_bytes: SYSTEM_MEMORY_BYTES,
                tasks: self.system_tasks,
            },
            consumers,
        };
        policy.validate()?;
        Ok(policy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterSelection {
    Auto,
    Generic,
    Cargo,
    CargoPipeline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettings {
    pub schema: u32,
    pub project_id: String,
    pub profile: String,
    pub adapter: AdapterSelection,
    pub limits: Option<Budget>,
}

impl ProjectSettings {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > CONFIG_BYTES {
            return Err(invalid("project configuration exceeds byte limit"));
        }
        let settings: Self = toml::from_str(
            std::str::from_utf8(bytes)
                .map_err(|_| invalid("project configuration must be UTF-8"))?,
        )
        .map_err(|_| {
            invalid("invalid project configuration; authority fields are not permitted")
        })?;
        if settings.schema != CONFIG_SCHEMA || settings.profile != "interactive" {
            return Err(Error::new(
                ErrorCode::ResourcePolicyUnsupported,
                "unsupported project schema/profile",
            ));
        }
        validate_id(&settings.project_id)?;
        if let Some(limits) = settings.limits {
            limits.validate_workload()?;
        }
        Ok(settings)
    }

    pub fn restricted_budget(&self, operator_limit: Budget) -> Result<Budget> {
        let result = self.limits.unwrap_or(operator_limit);
        if !result.fits(operator_limit) {
            return Err(invalid("project limits may only tighten operator policy"));
        }
        Ok(result)
    }
}

fn new_secret() -> Result<Secret> {
    Secret::new(format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    ))
}

/// Explicit bootstrap; no ordinary serve/restart path calls this function.
/// A partial failure is preserved for explicit repair, never silently reset.
pub fn initialize(paths: &AuthorityPaths) -> Result<HostConfig> {
    paths.prepare_bootstrap()?;
    let files = [
        paths.config(),
        paths.cli_credential(),
        paths.admin_credential(),
        paths.journal(),
    ];
    if files.iter().any(|p| std::fs::symlink_metadata(p).is_ok()) {
        return Err(invalid(
            "bootstrap requires absent configuration, credentials and journal",
        ));
    }
    // Acquire canonical storage ownership before creating any credentials.
    let storage = AuthorityStorage::initialize(&paths.journal())?;
    let cli = new_secret()?;
    let admin = new_secret()?;
    let config = HostConfig {
        schema: CONFIG_SCHEMA,
        policy_revision: "interactive-v1".into(),
        default_profile: "interactive".into(),
        admin_credential_sha256: admin.digest(),
        task_capacity: 256,
        task_headroom: 64,
        system_tasks: 48,
        additional_headroom: Budget::ZERO,
        consumers: BTreeMap::from([(
            "dev-cli".into(),
            ConsumerConfig {
                generation: Uuid::new_v4().to_string(),
                role: ConsumerRole::Workload,
                credential_sha256: cli.digest(),
                max_instances: 8,
                control_reservation: Budget::ZERO,
            },
        )]),
        projects: BTreeMap::new(),
    };
    config.validate()?;
    write_new_private(&paths.cli_credential(), cli.expose().as_bytes())?;
    write_new_private(&paths.admin_credential(), admin.expose().as_bytes())?;
    let encoded =
        toml::to_string_pretty(&config).map_err(|_| invalid("configuration encoding failed"))?;
    write_new_private(&paths.config(), encoded.as_bytes())?;
    drop(storage);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn fixture() -> (tempfile::TempDir, AuthorityPaths) {
        let directory = tempfile::Builder::new()
            .prefix("dg-config-")
            .tempdir_in(if cfg!(target_os = "macos") {
                "/private/tmp"
            } else {
                "/tmp"
            })
            .unwrap();
        let paths = AuthorityPaths::fixture(directory.path());
        (directory, paths)
    }

    #[test]
    fn authority_bootstrap_keeps_roles_and_files_separate() {
        let (_dir, paths) = fixture();
        let config = initialize(&paths).unwrap();
        paths.validate_existing().unwrap();
        let cli = read_private(&paths.cli_credential(), paths.uid(), 64).unwrap();
        let admin = read_private(&paths.admin_credential(), paths.uid(), 64).unwrap();
        assert_ne!(cli, admin);
        assert_eq!(
            devguard_contract::digest_bytes(&cli),
            config.consumers["dev-cli"].credential_sha256
        );
        assert!(!std::fs::read_to_string(paths.config())
            .unwrap()
            .contains(std::str::from_utf8(&cli).unwrap()));
        assert_eq!(
            config.fingerprint().unwrap(),
            HostConfig::load(&paths).unwrap().fingerprint().unwrap()
        );
        assert!(initialize(&paths).is_err());
    }

    #[test]
    fn authority_configuration_rejects_path_overrides_and_unknown_versions() {
        let (_dir, paths) = fixture();
        let config = initialize(&paths).unwrap();
        let text = toml::to_string_pretty(&config).unwrap();
        for field in [
            "state_dir",
            "socket",
            "home",
            "test_capacity",
            "parent_lease",
        ] {
            assert!(
                HostConfig::parse(format!("{field} = '/tmp/other'\n{text}").as_bytes()).is_err()
            );
        }
        let future = text.replacen("schema = 1", "schema = 2", 1);
        assert_eq!(
            HostConfig::parse(future.as_bytes()).unwrap_err().code,
            ErrorCode::ResourcePolicyUnsupported
        );
        let mut invalid = config.clone();
        invalid
            .consumers
            .get_mut("dev-cli")
            .unwrap()
            .control_reservation
            .cpu_milli = 1;
        assert!(invalid.validate().is_err());
        invalid = config.clone();
        invalid
            .consumers
            .get_mut("dev-cli")
            .unwrap()
            .credential_sha256 = config.admin_credential_sha256;
        assert!(invalid.validate().is_err());
        assert!(HostConfig::parse(&vec![b'a'; CONFIG_BYTES + 1]).is_err());
    }

    #[test]
    fn authority_bootstrap_race_has_one_owner_without_credential_overwrite() {
        let (_dir, paths) = fixture();
        paths.prepare_bootstrap().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let p = paths.clone();
                let b = barrier.clone();
                std::thread::spawn(move || {
                    b.wait();
                    initialize(&p)
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        HostConfig::load(&paths).unwrap();
        paths.validate_existing().unwrap();
    }

    #[test]
    fn authority_configuration_rejects_prior_insufficient_session_reserves() {
        let (_dir, paths) = fixture();
        let config = initialize(&paths).unwrap();
        assert_eq!(config.system_tasks, 48);
        for system_tasks in [16, 47] {
            let mut prior = config.clone();
            prior.system_tasks = system_tasks;
            let encoded = toml::to_string_pretty(&prior).unwrap();
            assert_eq!(
                HostConfig::parse(encoded.as_bytes()).unwrap_err().code,
                ErrorCode::InvalidRequest,
                "configuration with reserve {system_tasks} was accepted"
            );
        }
        for system_tasks in [48, 49] {
            let mut sufficient = config.clone();
            sufficient.system_tasks = system_tasks;
            let encoded = toml::to_string_pretty(&sufficient).unwrap();
            assert_eq!(
                HostConfig::parse(encoded.as_bytes()).unwrap().system_tasks,
                system_tasks
            );
        }
    }

    #[test]
    fn authority_configuration_requires_distinct_admin_and_consumer_credentials() {
        let (_dir, paths) = fixture();
        let config = initialize(&paths).unwrap();
        let mut control = config.consumers["dev-cli"].clone();
        control.generation = "independent-generation".into();
        control.role = ConsumerRole::ControlService;
        control.control_reservation = Budget {
            cpu_milli: 100,
            memory_bytes: 64 * 1024 * 1024,
            tasks: 4,
        };
        let mut duplicated = config.clone();
        duplicated
            .consumers
            .insert("control-service".into(), control.clone());
        assert_eq!(
            duplicated.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );

        control.credential_sha256 = config.admin_credential_sha256.clone();
        duplicated
            .consumers
            .insert("control-service".into(), control.clone());
        assert_eq!(
            duplicated.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );

        control.credential_sha256 = new_secret().unwrap().digest();
        let mut distinct = config;
        distinct.consumers.insert("control-service".into(), control);
        let encoded = toml::to_string_pretty(&distinct).unwrap();
        assert_eq!(
            HostConfig::parse(encoded.as_bytes())
                .unwrap()
                .consumers
                .len(),
            2
        );
    }

    #[test]
    fn authority_project_settings_cannot_grant_roles_or_relax_limits() {
        let text="schema=1\nproject_id='devguard-dev'\nprofile='interactive'\nadapter='cargo'\n[limits]\ncpu_milli=1000\nmemory_bytes=2147483648\ntasks=32\n";
        let settings = ProjectSettings::parse(text.as_bytes()).unwrap();
        let ceiling = Budget {
            cpu_milli: 2000,
            memory_bytes: 4 * 1024 * 1024 * 1024,
            tasks: 64,
        };
        assert_eq!(settings.restricted_budget(ceiling).unwrap().cpu_milli, 1000);
        assert!(settings
            .restricted_budget(Budget {
                cpu_milli: 500,
                ..ceiling
            })
            .is_err());
        for field in [
            "role",
            "credential_sha256",
            "socket",
            "capacity",
            "consumer_id",
        ] {
            assert!(ProjectSettings::parse(
                format!("{field}='control_service'\n{text}").as_bytes()
            )
            .is_err());
        }
        assert!(ProjectSettings::parse(
            text.replace("adapter='cargo'", "adapter='unknown'")
                .as_bytes()
        )
        .is_err());
    }

    #[test]
    fn authority_host_policy_applies_approved_headroom_to_observed_capacity() {
        let (_directory, paths) = fixture();
        let config = initialize(&paths).unwrap();
        let host = HostCapacity {
            logical_cpus: 8,
            memory_bytes: 16 * GIB,
        };
        let policy = config.policy(host, paths.uid()).unwrap();
        assert_eq!(policy.effective_capacity.cpu_milli, 8_000);
        assert_eq!(policy.host_headroom.cpu_milli, 2_000);
        assert_eq!(policy.host_headroom.memory_bytes, 4 * GIB);
        assert_eq!(policy.system_reservation.cpu_milli, 500);
        assert_eq!(policy.system_reservation.memory_bytes, GIB / 4);
        assert_eq!(policy.system_reservation.tasks, 48);
        assert_eq!(
            policy.work_capacity().unwrap(),
            Budget {
                cpu_milli: 5_500,
                memory_bytes: 11 * GIB + 3 * GIB / 4,
                tasks: 144,
            }
        );
        assert!(policy
            .consumers
            .values()
            .all(|consumer| consumer.uid == paths.uid()));
        // Larger hosts use 25%; smaller hosts keep the absolute minimum and may
        // have no workload capacity at all rather than a forced minimum.
        let large = config
            .policy(
                HostCapacity {
                    logical_cpus: 32,
                    memory_bytes: 64 * GIB,
                },
                paths.uid(),
            )
            .unwrap();
        assert_eq!(large.host_headroom.cpu_milli, 8_000);
        assert_eq!(large.host_headroom.memory_bytes, 16 * GIB);
        let small = config
            .policy(
                HostCapacity {
                    logical_cpus: 2,
                    memory_bytes: 4 * GIB,
                },
                paths.uid(),
            )
            .unwrap();
        assert_eq!(small.work_capacity().unwrap().cpu_milli, 0);
        assert_eq!(small.work_capacity().unwrap().memory_bytes, 0);
        // Additional operator headroom can only subtract capacity.
        let mut stricter = config.clone();
        stricter.additional_headroom = Budget {
            cpu_milli: 1_000,
            memory_bytes: GIB,
            tasks: 8,
        };
        let stricter = stricter.policy(host, paths.uid()).unwrap();
        assert_eq!(stricter.work_capacity().unwrap().cpu_milli, 4_500);
        assert_eq!(stricter.work_capacity().unwrap().tasks, 136);
    }
}
