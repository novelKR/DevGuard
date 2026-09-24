//! Adapter transformation interface, separate from authority admission. An
//! adapter may change a command's arguments and environment to fit the
//! reservation it will run under; it never admits, launches or releases work.
//! The generic adapter changes nothing and says so.

use devguard_contract::{Budget, Error, ErrorCode, Result};
use devguard_daemon::config::AdapterSelection;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

const MIB: u64 = 1024 * 1024;

/// The command as the owner resolved it, before any transformation.
pub struct Invocation<'a> {
    /// The absolute executable.
    pub program: &'a Path,
    pub args: &'a [OsString],
    /// The environment the executable would inherit unchanged.
    pub environment: &'a BTreeMap<OsString, OsString>,
}

/// What an adapter did. `set` and `removed` are the only environment changes;
/// everything else is inherited as it was.
pub struct Transformation {
    pub args: Vec<OsString>,
    pub set: BTreeMap<String, String>,
    pub removed: BTreeSet<String>,
    pub report: AdapterReport,
    /// Resources the executable uses that must outlive it, such as a shared
    /// jobserver. Dropped after the run ends.
    pub hold: Vec<Box<dyn std::any::Any>>,
}

/// Recorded in the receipt: the adapter, whether it changed parallelism and why.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AdapterReport {
    pub adapter: &'static str,
    /// How the adapter was chosen: explicitly, by the project or by `auto`.
    pub selected_by: &'static str,
    pub parallelism: &'static str,
    pub detail: serde_json::Value,
}

pub trait Adapter {
    fn name(&self) -> &'static str;
    /// The request when neither the command line nor the project sets one.
    fn default_budget(&self) -> Budget;
    /// Transform the command for a reservation of exactly `budget`, or refuse
    /// before admission when that budget cannot fit the minimum work.
    fn transform(
        &self,
        invocation: &Invocation,
        budget: Budget,
        selected_by: &'static str,
    ) -> Result<Transformation>;
}

/// Accounting and cooperative scheduling only; parallelism is left as it is.
pub struct Generic;

impl Adapter for Generic {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn default_budget(&self) -> Budget {
        Budget {
            cpu_milli: 1_000,
            memory_bytes: 1024 * MIB,
            tasks: 32,
        }
    }

    fn transform(
        &self,
        invocation: &Invocation,
        _budget: Budget,
        selected_by: &'static str,
    ) -> Result<Transformation> {
        Ok(Transformation {
            args: invocation.args.to_vec(),
            set: BTreeMap::new(),
            removed: BTreeSet::new(),
            report: AdapterReport {
                adapter: self.name(),
                selected_by,
                parallelism: "not_transformed",
                detail: serde_json::json!({
                    "reason": "a generic command is accounted and scheduled cooperatively; its own parallelism is not changed"
                }),
            },
            hold: Vec::new(),
        })
    }
}

/// Choose the adapter. `selected_by` records whether the choice came from the
/// command line, the project or `auto`.
pub fn select(selection: &AdapterSelection, _program: &Path) -> Result<Box<dyn Adapter>> {
    match selection {
        AdapterSelection::Auto | AdapterSelection::Generic => Ok(Box::new(Generic)),
        AdapterSelection::Cargo | AdapterSelection::CargoPipeline => Err(Error::new(
            ErrorCode::ResourcePolicyUnsupported,
            "the Cargo adapter is not available in this build; select generic explicitly",
        )),
    }
}
