//! The execution receipt. It records the command, the authority, every
//! attempt, the wait, the launch and the exit. It never contains a permit, a
//! caller credential or the inherited environment; only the variables the
//! adapter set or removed are named.

use crate::adapter::AdapterReport;
use crate::authority::EndpointSummary;
use crate::preflight::ProjectSummary;
use devguard_client::launch::HelperPhase;
use devguard_contract::{AttemptKey, AttemptRecord, Budget, Capability};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "devguard-exec-receipt/v1";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecResult {
    /// The executable was attempted after READY; the exit is its own.
    Completed,
    /// The exec after READY failed.
    ExecFailed,
    /// Nothing was started.
    NotStarted,
    /// The helper's transcript or exit could not be observed completely, so
    /// whether the executable started is unknown. The exit is the root's own.
    Uncertain,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CommandSummary {
    pub program: Option<PathBuf>,
    pub requested_program: String,
    pub args: Vec<String>,
    pub transformed_args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub tty: bool,
    pub environment_set: BTreeMap<String, String>,
    pub environment_removed: BTreeSet<String>,
    pub execution_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetSummary {
    pub requested: Budget,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthoritySummary {
    pub endpoint: EndpointSummary,
    pub authority_pid: Option<i32>,
    pub protocol: Option<u32>,
    pub capabilities: BTreeSet<Capability>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttemptEntry {
    pub key: AttemptKey,
    pub record: Option<AttemptRecord>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WaitSummary {
    pub requested_ms: Option<u64>,
    /// Time from the start until the admission that was used, or until the
    /// CLI gave up when none was.
    pub waited_ms: u64,
    pub admissions: u32,
    pub cancelled_by_signal: Option<i32>,
    pub deadline_reached: bool,
    /// The host's work capacity a wait was checked against: a request that
    /// does not fit it can never be admitted.
    pub work_capacity: Option<Budget>,
    /// Why the capacity could not be checked; the authority still decides.
    pub work_capacity_unknown: Option<String>,
    /// The budget of the parent lease a lease child's wait was checked
    /// against, instead of the host's capacity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_budget: Option<Budget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchSummary {
    pub helper_pid: u32,
    pub phases: Vec<HelperPhase>,
    pub outcome: String,
    pub terminal_handed: bool,
    pub stops_mirrored: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SignalSummary {
    pub received: Vec<i32>,
    pub forwarded: Vec<i32>,
    /// Signals the CLI inherited as ignored. They stay ignored for the
    /// workload too, as for any command started under `nohup`.
    pub ignored: Vec<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecReceipt {
    pub schema: &'static str,
    /// Always true: DevGuard never runs a command outside a managed launch.
    pub managed: bool,
    pub result: ExecResult,
    pub reason: Option<String>,
    pub command: CommandSummary,
    pub adapter: Option<AdapterReport>,
    /// What the adapter's held resources reported after the run, such as the
    /// tokens a shared jobserver had once every client ended.
    pub adapter_after: Vec<serde_json::Value>,
    pub budget: Option<BudgetSummary>,
    pub project: Option<ProjectSummary>,
    pub authority: Option<AuthoritySummary>,
    /// The parent lease each attempt was admitted under, for a lease child.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease: Option<AttemptKey>,
    pub attempts: Vec<AttemptEntry>,
    pub wait: WaitSummary,
    pub launch: Option<LaunchSummary>,
    pub exit: Option<Exit>,
    /// The attempt observed after the root exited and before it was reaped.
    pub observed_before_reap: Option<AttemptRecord>,
    /// The attempt observed after the root was reaped.
    pub observed_after_reap: Option<AttemptRecord>,
    pub signals: SignalSummary,
    pub started_unix_ms: u128,
    pub finished_unix_ms: u128,
}

pub fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or(0)
}

impl ExecReceipt {
    pub fn new(requested_program: String) -> Self {
        Self {
            schema: SCHEMA,
            managed: true,
            result: ExecResult::NotStarted,
            reason: None,
            command: CommandSummary {
                requested_program,
                ..CommandSummary::default()
            },
            adapter: None,
            adapter_after: Vec::new(),
            budget: None,
            project: None,
            authority: None,
            lease: None,
            attempts: Vec::new(),
            wait: WaitSummary::default(),
            launch: None,
            exit: None,
            observed_before_reap: None,
            observed_after_reap: None,
            signals: SignalSummary::default(),
            started_unix_ms: unix_ms(),
            finished_unix_ms: 0,
        }
    }
}

/// A receipt file named on the command line. It is created, exclusively and
/// private, before anything is admitted, so a run never ends without a place
/// to record it.
pub struct ReceiptFile {
    file: std::fs::File,
}

impl ReceiptFile {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self { file })
    }

    pub fn write(mut self, receipt: &ExecReceipt) -> std::io::Result<()> {
        let mut text = serde_json::to_vec_pretty(receipt)?;
        text.push(b'\n');
        self.file.write_all(&text)?;
        self.file.sync_all()
    }
}
