//! Adapter transformation interface, separate from authority admission. An
//! adapter may change a command's arguments and environment to fit the
//! reservation it will run under; it never admits, launches or releases work.
//! The generic adapter changes nothing and says so. The Cargo adapters fit
//! Cargo's compiler jobs to the reservation (`devguard_cargo`).

use devguard_contract::{Budget, Error, ErrorCode, Result};
use devguard_daemon::config::AdapterSelection;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
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

/// A resource the executable uses that must outlive it, such as a shared
/// jobserver. It is finished after the run and reports what it observed.
pub trait Held {
    fn finish(self: Box<Self>) -> Value;
}

/// What an adapter did. `set` and `removed` are the only environment changes;
/// everything else is inherited as it was.
pub struct Transformation {
    pub args: Vec<OsString>,
    pub set: BTreeMap<String, String>,
    pub removed: BTreeSet<String>,
    pub report: AdapterReport,
    pub hold: Vec<Box<dyn Held>>,
}

/// Recorded in the receipt: the adapter, whether it changed parallelism and why.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AdapterReport {
    pub adapter: &'static str,
    /// How the adapter was chosen: by the command line, the project or `auto`.
    pub selected_by: &'static str,
    pub parallelism: String,
    pub detail: Value,
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
                parallelism: "not_transformed".into(),
                detail: json!({
                    "reason": "a generic command is accounted and scheduled cooperatively; its own parallelism is not changed"
                }),
            },
            hold: Vec::new(),
        })
    }
}

fn is_cargo(program: &Path) -> bool {
    program.file_name() == Some(OsStr::new("cargo"))
}

fn cargo_default_budget() -> Budget {
    devguard_cargo::budget_for(devguard_cargo::DEFAULT_JOBS, devguard_cargo::DEFAULT_TASKS)
}

/// Direct mode: the program is `cargo`, whose jobs are fitted in its arguments.
pub struct CargoDirect;

impl Adapter for CargoDirect {
    fn name(&self) -> &'static str {
        "cargo"
    }

    fn default_budget(&self) -> Budget {
        cargo_default_budget()
    }

    fn transform(
        &self,
        invocation: &Invocation,
        budget: Budget,
        selected_by: &'static str,
    ) -> Result<Transformation> {
        if !is_cargo(invocation.program) {
            return Err(Error::new(
                ErrorCode::ResourcePolicyUnsupported,
                "the cargo adapter runs cargo itself; select cargo-pipeline for a program that runs Cargo",
            ));
        }
        let plan = devguard_cargo::direct(invocation.args, invocation.environment, budget)?;
        let parallelism = plan.report["jobs"]["action"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(Transformation {
            args: plan.args,
            set: plan.set,
            removed: plan.removed,
            report: AdapterReport {
                adapter: self.name(),
                selected_by,
                parallelism,
                detail: plan.report,
            },
            hold: Vec::new(),
        })
    }
}

/// Pipeline mode: the program runs Cargo itself; every nested Cargo shares one
/// jobserver sized to the reservation.
pub struct CargoPipeline;

struct HeldJobserver(devguard_cargo::Jobserver);

impl Held for HeldJobserver {
    fn finish(self: Box<Self>) -> Value {
        let jobserver = &self.0;
        let report = json!({"jobserver": {
            "path": jobserver.path(),
            "jobs": jobserver.jobs(),
            "tokens_expected": jobserver.jobs() - 1,
            "tokens_after_run": jobserver.available(),
        }});
        // Dropping it removes the FIFO and its private directory.
        drop(self);
        report
    }
}

impl Adapter for CargoPipeline {
    fn name(&self) -> &'static str {
        "cargo-pipeline"
    }

    fn default_budget(&self) -> Budget {
        cargo_default_budget()
    }

    fn transform(
        &self,
        invocation: &Invocation,
        budget: Budget,
        selected_by: &'static str,
    ) -> Result<Transformation> {
        let plan = devguard_cargo::pipeline(invocation.environment, budget)?;
        let parallelism = if plan.jobserver.is_some() {
            "shared_jobserver"
        } else {
            "inherited_jobserver"
        };
        Ok(Transformation {
            args: invocation.args.to_vec(),
            set: plan.set,
            removed: plan.removed,
            report: AdapterReport {
                adapter: self.name(),
                selected_by,
                parallelism: parallelism.into(),
                detail: plan.report,
            },
            hold: plan
                .jobserver
                .map(|jobserver| Box::new(HeldJobserver(jobserver)) as Box<dyn Held>)
                .into_iter()
                .collect(),
        })
    }
}

/// Choose the adapter. `auto` selects the Cargo adapter only for a supported
/// Cargo subcommand and otherwise the generic one, and says why.
pub fn select(
    selection: &AdapterSelection,
    program: &Path,
    args: &[OsString],
) -> Result<(Box<dyn Adapter>, Option<String>)> {
    Ok(match selection {
        AdapterSelection::Generic => (Box::new(Generic), None),
        AdapterSelection::Cargo => (Box::new(CargoDirect), None),
        AdapterSelection::CargoPipeline => (Box::new(CargoPipeline), None),
        AdapterSelection::Auto if !is_cargo(program) => (
            Box::new(Generic),
            Some("auto: the program is not cargo".into()),
        ),
        AdapterSelection::Auto => match devguard_cargo::parse(args)?.subcommand {
            Some(subcommand) if devguard_cargo::supported(&subcommand) => (
                Box::new(CargoDirect),
                Some(format!("auto: cargo {subcommand} compiles")),
            ),
            Some(subcommand) => (
                Box::new(Generic),
                Some(format!(
                    "auto: cargo {subcommand} is not a supported Cargo subcommand"
                )),
            ),
            None => (
                Box::new(Generic),
                Some("auto: cargo without a subcommand compiles nothing".into()),
            ),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn auto_selects_cargo_only_for_a_supported_cargo_subcommand() {
        let cargo = Path::new("/toolchain/bin/cargo");
        for (program, given, expected) in [
            (cargo, &["build"][..], "cargo"),
            (cargo, &["+1.95.0", "test", "--workspace"], "cargo"),
            (cargo, &["fmt"], "generic"),
            (cargo, &["--version"], "generic"),
            (Path::new("/usr/bin/python3"), &["build"], "generic"),
        ] {
            let (adapter, reason) = select(&AdapterSelection::Auto, program, &args(given)).unwrap();
            assert_eq!(adapter.name(), expected, "{given:?}");
            assert!(reason.unwrap().starts_with("auto: "));
        }
        let (explicit, reason) =
            select(&AdapterSelection::Cargo, cargo, &args(&["build"])).unwrap();
        assert_eq!(explicit.name(), "cargo");
        assert!(reason.is_none());
    }

    #[test]
    fn the_cargo_adapter_refuses_other_programs_and_reservations_below_one_job() {
        let environment = BTreeMap::new();
        let invocation = |program: &'static str, given: &'static [&'static str]| {
            (Path::new(program), args(given))
        };
        let (program, given) = invocation("/bin/echo", &["build"]);
        let refused = CargoDirect
            .transform(
                &Invocation {
                    program,
                    args: &given,
                    environment: &environment,
                },
                cargo_default_budget(),
                "command_line",
            )
            .err()
            .unwrap();
        assert_eq!(refused.code, ErrorCode::ResourcePolicyUnsupported);
        let (program, given) = invocation("/toolchain/bin/cargo", &["build"]);
        let small = Budget {
            cpu_milli: 500,
            memory_bytes: 8 << 30,
            tasks: 8,
        };
        let refused = CargoDirect
            .transform(
                &Invocation {
                    program,
                    args: &given,
                    environment: &environment,
                },
                small,
                "command_line",
            )
            .err()
            .unwrap();
        assert_eq!(refused.code, ErrorCode::ResourceUnavailable);
    }

    #[test]
    fn a_pipeline_holds_its_jobserver_until_the_run_is_finished() {
        let environment = BTreeMap::new();
        let given = args(&["scripts/validate.py"]);
        let transformation = CargoPipeline
            .transform(
                &Invocation {
                    program: Path::new("/usr/bin/python3"),
                    args: &given,
                    environment: &environment,
                },
                devguard_cargo::budget_for(3, 16),
                "command_line",
            )
            .unwrap();
        assert_eq!(transformation.report.parallelism, "shared_jobserver");
        assert_eq!(transformation.args, given);
        assert_eq!(transformation.set["CARGO_BUILD_JOBS"], "3");
        let path = transformation.report.detail["jobserver"]["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(Path::new(&path).exists());
        let finished: Vec<Value> = transformation
            .hold
            .into_iter()
            .map(|held| held.finish())
            .collect();
        assert_eq!(finished[0]["jobserver"]["tokens_after_run"], 2);
        assert_eq!(finished[0]["jobserver"]["tokens_expected"], 2);
        assert!(
            !Path::new(&path).exists(),
            "finishing removes the jobserver"
        );
    }
}
