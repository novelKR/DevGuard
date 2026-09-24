//! `devguard doctor`: report whether managed execution is available here, and
//! why not. DevGuard never falls back to unmanaged execution, so a command
//! either runs through the authority or is refused; doctor shows which.

use crate::args::{DoctorArgs, Requirement};
use crate::authority::Endpoint;
use crate::preflight::resolve_project;
use crate::receipt::Exit;
use devguard_contract::Capability;
use devguard_daemon::paths::AuthorityPaths;
use serde_json::{json, Value};
use std::path::Path;

pub const SCHEMA: &str = "devguard-doctor/v1";

fn check(ok: bool, detail: Value) -> Value {
    json!({"ok": ok, "detail": detail})
}

fn failure(error: &devguard_contract::Error) -> Value {
    json!({"code": error.code, "message": error.message})
}

pub fn run(args: DoctorArgs, paths: &AuthorityPaths, helper: &Path) -> Exit {
    let mut checks = serde_json::Map::new();
    checks.insert(
        "paths".into(),
        check(
            true,
            json!({"config": paths.config(), "socket": paths.socket(),
                   "credentials": paths.credentials(), "state": paths.state()}),
        ),
    );
    let helper_ok = helper.is_absolute() && helper.is_file();
    checks.insert("helper".into(), check(helper_ok, json!({"path": helper})));
    let endpoint = Endpoint::open(paths);
    checks.insert(
        "configuration".into(),
        match &endpoint {
            Ok(endpoint) => check(
                true,
                serde_json::to_value(endpoint.summary()).unwrap_or(Value::Null),
            ),
            Err(error) => check(false, failure(error)),
        },
    );
    let mut capabilities = Vec::new();
    let (mut registration_ready, mut execution_ready) = (false, false);
    if let Ok(endpoint) = &endpoint {
        match endpoint.status() {
            Ok((hello, status)) => {
                capabilities = hello.capabilities.iter().copied().collect();
                registration_ready = status.registration_ready;
                execution_ready = status.execution_ready;
                checks.insert(
                    "service".into(),
                    check(
                        true,
                        json!({"authority": hello.authority, "caller": hello.caller,
                               "protocol": hello.protocol, "capabilities": hello.capabilities,
                               "status": status}),
                    ),
                );
            }
            Err(error) => {
                checks.insert("service".into(), check(false, failure(&error)));
            }
        }
    }
    let mut registered = None;
    if args.require.contains(&Requirement::Registration) {
        if let Ok(endpoint) = &endpoint {
            let result = endpoint.register();
            checks.insert(
                "registration".into(),
                match &result {
                    Ok(identity) => check(true, json!({"observed": identity})),
                    Err(error) => check(false, failure(error)),
                },
            );
            registered = Some(result.is_ok());
        }
    }
    if let (Some(id), Ok(endpoint)) = (&args.project, &endpoint) {
        let project = std::env::current_dir()
            .and_then(|cwd| cwd.canonicalize())
            .map_err(|_| {
                devguard_contract::Error::new(
                    devguard_contract::ErrorCode::InvalidRequest,
                    "the working directory is not accessible",
                )
            })
            .and_then(|cwd| resolve_project(&endpoint.config, id, &cwd));
        checks.insert(
            "project".into(),
            match &project {
                Ok(summary) => check(true, serde_json::to_value(summary).unwrap_or(Value::Null)),
                Err(error) => check(false, failure(error)),
            },
        );
    }
    let has = |capability| capabilities.contains(&capability);
    let managed = helper_ok
        && endpoint.is_ok()
        && has(Capability::DurableAdmission)
        && has(Capability::FencedLaunch)
        && execution_ready;
    let mut requirements = serde_json::Map::new();
    let mut satisfied = true;
    for requirement in &args.require {
        let ok = match requirement {
            Requirement::Admission => managed,
            Requirement::Registration => registration_ready && registered == Some(true),
            Requirement::MacosCooperative => has(Capability::MacosCooperative),
        };
        satisfied &= ok;
        requirements.insert(requirement.name().into(), Value::Bool(ok));
    }
    let project_ok = checks
        .get("project")
        .is_none_or(|project| project["ok"] == Value::Bool(true));
    satisfied &= project_ok;
    let report = json!({
        "schema": SCHEMA,
        "managed_execution": if managed { "available" } else { "unavailable" },
        "unmanaged_fallback": "never",
        "registration_ready": registration_ready,
        "execution_ready": execution_ready,
        "requirements": requirements,
        "satisfied": satisfied,
        "checks": checks,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into())
    );
    Exit::Code(if satisfied { 0 } else { 1 })
}
