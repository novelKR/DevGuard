//! The owner's connection to the central authority. Every step opens a fresh
//! authenticated session, because each frame, including idle waiting, has an
//! absolute 250 ms deadline. The caller credential is read from the private
//! credential file and is never placed in arguments, environment or receipts.

use devguard_client::launch::HelperTicket;
use devguard_client::protocol::{
    AbandonReason, CallerCredential, Hello, LaunchGrant, ServiceStatus,
};
use devguard_client::Client;
use devguard_contract::{
    AdmissionRequest, AttemptKey, AttemptRecord, Capability, Compatibility, Error, ErrorCode,
    InstanceIdentity, Result, Secret,
};
use devguard_daemon::config::HostConfig;
use devguard_daemon::paths::{read_private, AuthorityPaths};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The workload consumer the CLI authenticates as.
pub const CONSUMER: &str = "dev-cli";
/// Upper bound on the credential file.
const CREDENTIAL_BYTES: usize = 1024;

/// What a managed execution needs from the service.
pub fn compatibility() -> Compatibility {
    Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 1,
        required: BTreeSet::from([Capability::DurableAdmission, Capability::FencedLaunch]),
    }
}

/// A handshake that demands nothing, for diagnostics.
pub fn diagnostic_compatibility() -> Compatibility {
    Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 1,
        required: BTreeSet::new(),
    }
}

/// The authority as this operating account sees it: canonical paths for the
/// `devguard` binary, isolated fixture paths only in compiled tests.
pub struct Endpoint {
    pub socket: PathBuf,
    pub uid: u32,
    pub config: HostConfig,
    pub generation: String,
    secret: Secret,
    /// This process's registered instance, unique per CLI process because an
    /// instance identity can never be reused.
    pub instance_id: String,
}

/// The non-secret part of an endpoint, for receipts and diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointSummary {
    pub socket: PathBuf,
    pub uid: u32,
    pub consumer: &'static str,
    pub generation: String,
    pub instance_id: String,
    pub configuration_fingerprint: Option<String>,
}

impl Endpoint {
    pub fn open(paths: &AuthorityPaths) -> Result<Self> {
        let config = HostConfig::load(paths)?;
        let consumer = config.consumers.get(CONSUMER).ok_or_else(|| {
            Error::new(
                ErrorCode::Unauthorized,
                "the operator configuration has no dev-cli consumer",
            )
        })?;
        let generation = consumer.generation.clone();
        let bytes = read_private(&paths.cli_credential(), paths.uid(), CREDENTIAL_BYTES)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::new(ErrorCode::Unauthorized, "the CLI credential is not valid"))?;
        // The file holds exactly the secret bytes, as bootstrap wrote them.
        let secret = Secret::new(text)?;
        Ok(Self {
            socket: paths.socket(),
            uid: paths.uid(),
            config,
            generation,
            secret,
            instance_id: format!("cli-{}", uuid::Uuid::new_v4().simple()),
        })
    }

    pub fn summary(&self) -> EndpointSummary {
        EndpointSummary {
            socket: self.socket.clone(),
            uid: self.uid,
            consumer: CONSUMER,
            generation: self.generation.clone(),
            instance_id: self.instance_id.clone(),
            configuration_fingerprint: self.config.fingerprint().ok(),
        }
    }

    fn credential(&self) -> CallerCredential {
        CallerCredential::Consumer {
            consumer_id: CONSUMER.into(),
            generation: self.generation.clone(),
            secret: self.secret.clone(),
        }
    }

    /// Connect and authenticate, without registering.
    pub fn authenticated(&self, compatibility: Compatibility) -> Result<Client> {
        let mut client = Client::connect(&self.socket, self.uid, compatibility)?;
        client.authenticate(self.credential())?;
        Ok(client)
    }

    /// A fresh session registered as this process's instance.
    pub fn session(&self) -> Result<Client> {
        let mut client = self.authenticated(compatibility())?;
        client.register(self.instance_id.clone())?;
        Ok(client)
    }

    /// Register and report the identity the authority observed.
    pub fn register(&self) -> Result<InstanceIdentity> {
        let mut client = self.authenticated(compatibility())?;
        client.register(self.instance_id.clone())
    }

    /// The handshake and authenticated status, requiring nothing.
    pub fn status(&self) -> Result<(Hello, ServiceStatus)> {
        let mut client = self.authenticated(diagnostic_compatibility())?;
        let status = client.status()?;
        Ok((client.hello.clone(), status))
    }
}

/// The owner's calls to the authority. Each call stands alone; the real
/// endpoint opens a fresh session for it. Tests script lost replies through
/// this seam, which a real authority cannot produce on demand.
pub trait Service {
    /// The key of a new attempt of this owner.
    fn key(&self, attempt_id: String) -> AttemptKey;
    fn admit(&self, request: AdmissionRequest) -> Result<AttemptRecord>;
    /// Only the first successful commit carries the permit.
    fn begin_launch(&self, key: &AttemptKey) -> Result<LaunchGrant>;
    fn lookup(&self, key: &AttemptKey) -> Result<AttemptRecord>;
    /// Report that this owner holds no helper for the grant and will start none.
    fn abandon(&self, key: &AttemptKey, reason: AbandonReason) -> Result<AttemptRecord>;
    /// Reconcile the attempt now, before an exited root is reaped.
    fn observe(&self, key: &AttemptKey) -> Result<AttemptRecord>;
    /// What the helper needs to present a grant of `key`.
    fn ticket(&self, key: &AttemptKey) -> HelperTicket;
}

impl Service for Endpoint {
    fn key(&self, attempt_id: String) -> AttemptKey {
        AttemptKey {
            consumer_id: CONSUMER.into(),
            consumer_generation: self.generation.clone(),
            attempt_id,
        }
    }

    fn admit(&self, request: AdmissionRequest) -> Result<AttemptRecord> {
        self.session()?.admit(request)
    }

    fn begin_launch(&self, key: &AttemptKey) -> Result<LaunchGrant> {
        self.session()?.begin_launch(key.clone())
    }

    fn lookup(&self, key: &AttemptKey) -> Result<AttemptRecord> {
        self.session()?.lookup(key.clone())
    }

    fn abandon(&self, key: &AttemptKey, reason: AbandonReason) -> Result<AttemptRecord> {
        self.session()?.abandon_launch(key.clone(), reason)
    }

    fn observe(&self, key: &AttemptKey) -> Result<AttemptRecord> {
        self.session()?.observe(key.clone())
    }

    fn ticket(&self, key: &AttemptKey) -> HelperTicket {
        HelperTicket {
            endpoint: self.socket.clone(),
            key: key.clone(),
            instance_id: self.instance_id.clone(),
        }
    }
}
