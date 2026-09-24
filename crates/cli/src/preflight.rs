//! Everything decided before the authority is asked: the absolute program,
//! the project, the adapter, the budget and the canonical meaning digest.
//! A failure here starts nothing and reserves nothing.

use crate::adapter::{self, AdapterReport, Held, Invocation};
use crate::args::ExecArgs;
use devguard_contract::{
    Budget, Error, ErrorCode, ExecutionMeaning, ResourceIntent, ResourceLevels, Result,
};
use devguard_daemon::config::{AdapterSelection, HostConfig, ProjectSettings, CONFIG_BYTES};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The profile every workload uses.
const PROFILE: &str = "interactive";
/// The CLI enforces no timeout of its own; the meaning records that as the
/// largest value, because zero is not a valid timeout.
pub const NO_TIMEOUT_MS: u64 = u64::MAX;
/// Marks a removed variable in the meaning's environment changes. A real
/// environment value can never contain a NUL byte.
pub const REMOVED: &str = "\u{0}";

/// Why a command could not be prepared. `NotFound` and `NotExecutable` keep
/// the shell's statuses so scripts see the usual meaning.
#[derive(Debug)]
pub enum Refusal {
    NotFound(String),
    NotExecutable(String),
    Invalid(Error),
}

impl From<Error> for Refusal {
    fn from(error: Error) -> Self {
        Self::Invalid(error)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub root: PathBuf,
    pub adapter: AdapterSelection,
    pub limits: Option<Budget>,
}

pub struct Preflight {
    pub program: PathBuf,
    pub original_args: Vec<OsString>,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub set: BTreeMap<String, String>,
    pub removed: BTreeSet<String>,
    pub tty: bool,
    pub intent: ResourceIntent,
    pub budget_source: &'static str,
    pub digest: String,
    pub adapter: AdapterReport,
    pub project: Option<ProjectSummary>,
    pub hold: Vec<Box<dyn Held>>,
}

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorCode::InvalidRequest, message)
}

fn utf8(value: &OsStr, message: &'static str) -> Result<String> {
    value
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| invalid(message))
}

fn executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Resolve `program` as a shell would, without running a shell: a name with a
/// slash is a path relative to `cwd`; a bare name is searched on `PATH`, where
/// an empty entry means the current directory.
pub fn resolve_program(
    program: &OsStr,
    path: Option<&OsStr>,
    cwd: &Path,
) -> std::result::Result<PathBuf, Refusal> {
    let shown = program.to_string_lossy().into_owned();
    if program.as_bytes().contains(&b'/') {
        let candidate = cwd.join(program);
        if !candidate.exists() {
            return Err(Refusal::NotFound(shown));
        }
        if !executable(&candidate) {
            return Err(Refusal::NotExecutable(shown));
        }
        return Ok(candidate);
    }
    let mut found_unexecutable = false;
    for directory in std::env::split_paths(path.unwrap_or_default()) {
        let directory = if directory.as_os_str().is_empty() {
            cwd.to_path_buf()
        } else {
            cwd.join(directory)
        };
        let candidate = directory.join(program);
        if executable(&candidate) {
            return Ok(candidate);
        }
        found_unexecutable |= candidate.is_file();
    }
    Err(if found_unexecutable {
        Refusal::NotExecutable(shown)
    } else {
        Refusal::NotFound(shown)
    })
}

/// A stable description of a file's identity: path, device, inode, size and
/// modification time. A replaced executable changes the meaning.
fn identity(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)
        .map_err(|_| invalid("cannot read the identity of the program or directory"))?;
    let path = utf8(
        path.as_os_str(),
        "paths must be UTF-8 for the execution meaning",
    )?;
    Ok(format!(
        "{path}|dev={}|ino={}|size={}|mtime={}.{:09}",
        meta.dev(),
        meta.ino(),
        meta.size(),
        meta.mtime(),
        meta.mtime_nsec()
    ))
}

fn read_project_settings(root: &Path) -> Result<ProjectSettings> {
    let file = std::fs::File::open(root.join(".devguard.toml")).map_err(|_| {
        Error::new(
            ErrorCode::InvalidRequest,
            "the registered project root has no readable .devguard.toml",
        )
    })?;
    let mut bytes = Vec::new();
    file.take(CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("cannot read the project configuration"))?;
    ProjectSettings::parse(&bytes)
}

/// A project named on the command line must be registered by the operator,
/// the working directory must lie inside its root, and its own settings must
/// name the same project.
pub fn resolve_project(config: &HostConfig, id: &str, cwd: &Path) -> Result<ProjectSummary> {
    let registration = config.projects.get(id).ok_or_else(|| {
        Error::new(
            ErrorCode::Unauthorized,
            "the project is not registered in the operator configuration",
        )
    })?;
    let root = registration
        .root
        .canonicalize()
        .map_err(|_| invalid("the registered project root is not accessible"))?;
    if !cwd.starts_with(&root) {
        return Err(invalid(
            "the working directory is outside the registered project root",
        ));
    }
    let settings = read_project_settings(&root)?;
    if settings.project_id != id {
        return Err(invalid(
            "the project configuration names a different project",
        ));
    }
    Ok(ProjectSummary {
        id: id.into(),
        root,
        adapter: settings.adapter,
        limits: settings.limits,
    })
}

/// Explicit quantities win field by field, then the project's limits, then the
/// adapter's default. The result must fit the project's limits.
pub fn budget(
    args: &ExecArgs,
    project: Option<&ProjectSummary>,
    default: Budget,
) -> Result<(Budget, &'static str)> {
    let limits = project.and_then(|project| project.limits);
    let base = limits.unwrap_or(default);
    let explicit = args.cpu_milli.is_some() || args.memory_bytes.is_some() || args.tasks.is_some();
    let requested = Budget {
        cpu_milli: args.cpu_milli.unwrap_or(base.cpu_milli),
        memory_bytes: args.memory_bytes.unwrap_or(base.memory_bytes),
        tasks: args.tasks.unwrap_or(base.tasks),
    };
    requested.validate_workload()?;
    if let Some(limits) = limits {
        if !requested.fits(limits) {
            return Err(Error::new(
                ErrorCode::ResourceUnavailable,
                "the requested budget exceeds the project's limits",
            ));
        }
    }
    let source = match (explicit, limits.is_some()) {
        (true, _) => "command_line",
        (false, true) => "project_limits",
        (false, false) => "adapter_default",
    };
    Ok((requested, source))
}

/// Prepare the command. `environment` is the environment the CLI inherited.
pub fn prepare(
    args: &ExecArgs,
    config: &HostConfig,
    environment: &BTreeMap<OsString, OsString>,
) -> std::result::Result<Preflight, Refusal> {
    let cwd = std::env::current_dir()
        .and_then(|cwd| cwd.canonicalize())
        .map_err(|_| invalid("the working directory is not accessible"))?;
    let project = match &args.project {
        Some(id) => Some(resolve_project(config, id, &cwd)?),
        None => None,
    };
    let program = resolve_program(
        &args.program,
        environment.get(OsStr::new("PATH")).map(OsString::as_os_str),
        &cwd,
    )?;
    let (selection, selected_by) = match (&args.adapter, &project) {
        (Some(adapter), _) => (adapter.clone(), "command_line"),
        (None, Some(project)) => (project.adapter.clone(), "project"),
        (None, None) => (AdapterSelection::Auto, "auto"),
    };
    let (adapter, selection_reason) = adapter::select(&selection, &program, &args.args)?;
    let (requested, budget_source) = budget(args, project.as_ref(), adapter.default_budget())?;
    let mut transformation = adapter.transform(
        &Invocation {
            program: &program,
            args: &args.args,
            environment,
        },
        requested,
        selected_by,
    )?;
    if let (Some(reason), Some(detail)) = (
        selection_reason,
        transformation.report.detail.as_object_mut(),
    ) {
        detail.insert("selection".into(), serde_json::Value::String(reason));
    }
    let intent = ResourceIntent {
        profile: PROFILE.into(),
        requested,
        minimum: ResourceLevels::MACOS,
    };
    // SAFETY: isatty only inspects descriptor 0.
    let tty = unsafe { libc::isatty(0) } == 1;
    let mut argv = vec![utf8(program.as_os_str(), "the program path must be UTF-8")?];
    for arg in &transformation.args {
        argv.push(utf8(arg, "arguments must be UTF-8")?);
    }
    let mut environment_changes = transformation.set.clone();
    for name in &transformation.removed {
        environment_changes.insert(name.clone(), REMOVED.into());
    }
    let digest = ExecutionMeaning {
        executable_identity: identity(&program)?,
        cwd_identity: identity(&cwd)?,
        argv,
        environment_changes,
        tty,
        timeout_ms: NO_TIMEOUT_MS,
        resources: intent.clone(),
    }
    .digest()?;
    Ok(Preflight {
        program,
        original_args: args.args.clone(),
        args: transformation.args,
        cwd,
        set: transformation.set,
        removed: transformation.removed,
        tty,
        intent,
        budget_source,
        digest,
        adapter: transformation.report,
        project,
        hold: transformation.hold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(cpu: Option<u64>, memory: Option<u64>, tasks: Option<u64>) -> ExecArgs {
        ExecArgs {
            project: None,
            adapter: None,
            wait: None,
            cpu_milli: cpu,
            memory_bytes: memory,
            tasks,
            receipt: None,
            program: "true".into(),
            args: Vec::new(),
        }
    }

    #[test]
    fn programs_resolve_like_a_shell_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let tool = bin.join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plain = bin.join("plain");
        std::fs::write(&plain, "data").unwrap();
        let path = std::env::join_paths([bin.as_path()]).unwrap();
        assert_eq!(
            resolve_program(OsStr::new("tool"), Some(&path), dir.path()).unwrap(),
            tool
        );
        assert_eq!(
            resolve_program(OsStr::new("bin/tool"), None, dir.path()).unwrap(),
            dir.path().join("bin/tool")
        );
        assert!(matches!(
            resolve_program(OsStr::new("absent"), Some(&path), dir.path()),
            Err(Refusal::NotFound(_))
        ));
        assert!(matches!(
            resolve_program(OsStr::new("plain"), Some(&path), dir.path()),
            Err(Refusal::NotExecutable(_))
        ));
        assert!(matches!(
            resolve_program(OsStr::new("bin/plain"), None, dir.path()),
            Err(Refusal::NotExecutable(_))
        ));
    }

    #[test]
    fn budgets_take_explicit_values_then_project_limits_then_defaults() {
        let default = Budget {
            cpu_milli: 1_000,
            memory_bytes: 1 << 30,
            tasks: 32,
        };
        let (chosen, source) = budget(&exec(None, None, None), None, default).unwrap();
        assert_eq!((chosen, source), (default, "adapter_default"));
        let (chosen, source) = budget(&exec(Some(250), None, None), None, default).unwrap();
        assert_eq!(chosen.cpu_milli, 250);
        assert_eq!(chosen.memory_bytes, 1 << 30);
        assert_eq!(source, "command_line");
        let project = ProjectSummary {
            id: "p".into(),
            root: "/".into(),
            adapter: AdapterSelection::Auto,
            limits: Some(Budget {
                cpu_milli: 500,
                memory_bytes: 256 << 20,
                tasks: 8,
            }),
        };
        let (chosen, source) = budget(&exec(None, None, None), Some(&project), default).unwrap();
        assert_eq!(
            (chosen, source),
            (project.limits.unwrap(), "project_limits")
        );
        assert_eq!(
            budget(&exec(Some(501), None, None), Some(&project), default)
                .unwrap_err()
                .code,
            ErrorCode::ResourceUnavailable
        );
        assert!(budget(&exec(Some(0), None, None), None, default).is_err());
    }

    #[test]
    fn the_meaning_changes_with_the_command_and_its_resources() {
        let dir = tempfile::tempdir().unwrap();
        let intent = ResourceIntent {
            profile: PROFILE.into(),
            requested: Budget {
                cpu_milli: 100,
                memory_bytes: 1 << 20,
                tasks: 1,
            },
            minimum: ResourceLevels::MACOS,
        };
        let meaning = |argv: &[&str], changes: &[(&str, &str)], requested: Budget| {
            ExecutionMeaning {
                executable_identity: identity(Path::new("/bin/sh")).unwrap(),
                cwd_identity: identity(dir.path()).unwrap(),
                argv: argv.iter().map(|arg| arg.to_string()).collect(),
                environment_changes: changes
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                tty: false,
                timeout_ms: NO_TIMEOUT_MS,
                resources: ResourceIntent {
                    requested,
                    ..intent.clone()
                },
            }
            .digest()
            .unwrap()
        };
        let base = meaning(&["/bin/sh", "-c", "true"], &[], intent.requested);
        assert_eq!(
            base,
            meaning(&["/bin/sh", "-c", "true"], &[], intent.requested)
        );
        assert_ne!(
            base,
            meaning(&["/bin/sh", "-c", "false"], &[], intent.requested)
        );
        assert_ne!(
            base,
            meaning(
                &["/bin/sh", "-c", "true"],
                &[("A", REMOVED)],
                intent.requested
            )
        );
        assert_ne!(
            base,
            meaning(
                &["/bin/sh", "-c", "true"],
                &[],
                Budget {
                    cpu_milli: 200,
                    ..intent.requested
                }
            )
        );
    }
}
