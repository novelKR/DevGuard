//! The Cargo adapter: fits Cargo's compiler parallelism to the reservation a
//! command runs under, without creating an independent nested token pool.
//!
//! Compiler jobs follow the approved estimate: one logical CPU and 1.5 GiB of
//! memory per job, plus a fixed 512 MiB. A reservation that cannot fit one job
//! is refused before admission; a job is never forced. Cargo jobs bound
//! compilation only; they do not cap the threads of the test programs Cargo
//! runs. Memory is an accounting estimate, not measured enforcement.
//!
//! Direct mode rewrites a `cargo` invocation: an explicit `-j`/`--jobs` above
//! the reservation is clamped, an absent one is inserted after the subcommand,
//! and conflicting values are refused. Pipeline mode serves a program that runs
//! Cargo itself: it creates a private FIFO jobserver shared by every nested
//! Cargo, because a FIFO survives helpers that close inherited descriptors.
//! A valid jobserver inherited from the caller is preserved in both modes, and
//! an invalid one is removed so Cargo does not silently fall back to its own
//! pool.

use devguard_contract::{Budget, Error, ErrorCode, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, OsStr, OsString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

pub const MIB: u64 = 1 << 20;
/// The approved fixed memory estimate of a Cargo build.
pub const FIXED_BYTES: u64 = 512 * MIB;
/// The approved memory estimate of one compiler job.
pub const PER_JOB_BYTES: u64 = 1536 * MIB;
/// One logical CPU per compiler job.
pub const PER_JOB_CPU_MILLI: u64 = 1_000;
/// The default reservation fits this many jobs; an unqualified initial value.
pub const DEFAULT_JOBS: u64 = 2;
/// The default task accounting of a Cargo build; an unqualified initial value.
pub const DEFAULT_TASKS: u64 = 64;
/// Variables that carry a jobserver, in the order Cargo reads them.
pub const JOBSERVER_VARIABLES: [&str; 3] = ["CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"];

/// Subcommands whose compilation parallelism `--jobs` governs.
const SUPPORTED: [&str; 16] = [
    "build", "b", "check", "c", "test", "t", "bench", "run", "r", "doc", "d", "clippy", "rustc",
    "rustdoc", "install", "fix",
];
/// Cargo options before the subcommand that take a separate value.
const GLOBAL_WITH_VALUE: [&str; 5] = ["-C", "--config", "-Z", "--color", "--explain"];

fn refused(code: ErrorCode, message: impl Into<String>) -> Error {
    Error::new(code, message)
}

/// Compiler jobs that fit `budget` under the approved estimate.
pub fn jobs_for(budget: Budget) -> u64 {
    let by_cpu = budget.cpu_milli / PER_JOB_CPU_MILLI;
    let by_memory = budget.memory_bytes.saturating_sub(FIXED_BYTES) / PER_JOB_BYTES;
    by_cpu.min(by_memory)
}

/// The smallest reservation that fits `jobs` compiler jobs.
pub fn budget_for(jobs: u64, tasks: u64) -> Budget {
    Budget {
        cpu_milli: jobs * PER_JOB_CPU_MILLI,
        memory_bytes: FIXED_BYTES + jobs * PER_JOB_BYTES,
        tasks,
    }
}

/// The reservation's jobs, refusing one that cannot fit a single job.
pub fn required_jobs(budget: Budget) -> Result<u64> {
    let jobs = jobs_for(budget);
    if jobs == 0 {
        return Err(refused(
            ErrorCode::ResourceUnavailable,
            "the reservation cannot fit one Cargo job (1 CPU and 2 GiB); a job is never forced",
        ));
    }
    Ok(jobs)
}

/// What an adapter changes and reports.
#[derive(Debug, Default)]
pub struct Plan {
    pub args: Vec<OsString>,
    pub set: BTreeMap<String, String>,
    pub removed: BTreeSet<String>,
    pub report: Value,
    pub jobserver: Option<Jobserver>,
}

/// A parsed `cargo` command line, up to what the adapter needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoArgs {
    pub subcommand: Option<String>,
    /// Index of the subcommand in the arguments.
    pub subcommand_index: Option<usize>,
    /// `-j`/`--jobs` occurrences before any `--`: (index, value, form).
    pub jobs: Vec<(usize, String, JobsForm)>,
    /// `build.jobs` values given through `--config`.
    pub config_jobs: Vec<String>,
}

/// How an explicit jobs value was written, so it can be rewritten in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsForm {
    /// `-j N` or `--jobs N`: the value is the next argument.
    Separate,
    /// `-jN`.
    ShortJoined,
    /// `--jobs=N`.
    LongJoined,
}

fn text(arg: &OsStr) -> Result<&str> {
    arg.to_str()
        .ok_or_else(|| refused(ErrorCode::InvalidRequest, "Cargo arguments must be UTF-8"))
}

fn config_jobs(value: &str) -> Option<String> {
    let (key, value) = value.split_once('=')?;
    (key.trim() == "build.jobs").then(|| value.trim().trim_matches('"').to_string())
}

/// Parse `args`, the arguments after the `cargo` program.
pub fn parse(args: &[OsString]) -> Result<CargoArgs> {
    let mut parsed = CargoArgs {
        subcommand: None,
        subcommand_index: None,
        jobs: Vec::new(),
        config_jobs: Vec::new(),
    };
    let mut index = 0;
    // A rustup toolchain override comes first.
    if args
        .first()
        .is_some_and(|arg| arg.as_bytes().starts_with(b"+"))
    {
        index = 1;
    }
    while index < args.len() {
        let arg = text(&args[index])?;
        if GLOBAL_WITH_VALUE.contains(&arg) {
            if arg == "--config" {
                if let Some(value) = args.get(index + 1) {
                    parsed.config_jobs.extend(config_jobs(text(value)?));
                }
            }
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--config=") {
            parsed.config_jobs.extend(config_jobs(value));
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        parsed.subcommand = Some(arg.to_string());
        parsed.subcommand_index = Some(index);
        break;
    }
    let Some(start) = parsed.subcommand_index else {
        return Ok(parsed);
    };
    let mut index = start + 1;
    while index < args.len() {
        let arg = text(&args[index])?;
        if arg == "--" {
            break;
        }
        if arg == "-j" || arg == "--jobs" {
            let value = args.get(index + 1).ok_or_else(|| {
                refused(
                    ErrorCode::InvalidRequest,
                    "a Cargo jobs option has no value",
                )
            })?;
            parsed
                .jobs
                .push((index, text(value)?.to_string(), JobsForm::Separate));
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--jobs=") {
            parsed
                .jobs
                .push((index, value.to_string(), JobsForm::LongJoined));
        } else if let Some(value) = arg.strip_prefix("-j").filter(|value| !value.is_empty()) {
            parsed
                .jobs
                .push((index, value.to_string(), JobsForm::ShortJoined));
        } else if arg == "--config" {
            if let Some(value) = args.get(index + 1) {
                parsed.config_jobs.extend(config_jobs(text(value)?));
            }
            index += 2;
            continue;
        } else if let Some(value) = arg.strip_prefix("--config=") {
            parsed.config_jobs.extend(config_jobs(value));
        }
        index += 1;
    }
    Ok(parsed)
}

pub fn supported(subcommand: &str) -> bool {
    SUPPORTED.contains(&subcommand)
}

/// A jobserver reference found in the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobserverReference {
    pub variable: String,
    pub auth: String,
    pub valid: bool,
    pub reason: String,
}

fn auth_of(flags: &str) -> Option<String> {
    flags
        .split_whitespace()
        .rev()
        .find_map(|flag| {
            flag.strip_prefix("--jobserver-auth=")
                .or_else(|| flag.strip_prefix("--jobserver-fds="))
        })
        .map(str::to_string)
}

fn inheritable_pipe(fd: RawFd) -> std::result::Result<(), &'static str> {
    if fd <= 2 {
        return Err("a jobserver descriptor cannot be a standard descriptor");
    }
    // SAFETY: fcntl and fstat only inspect the descriptor.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags < 0 {
            return Err("a jobserver descriptor is closed");
        }
        if flags & libc::FD_CLOEXEC != 0 {
            return Err("a jobserver descriptor is close-on-exec and would not reach Cargo");
        }
        let mut stat: libc::stat = std::mem::zeroed();
        if libc::fstat(fd, &mut stat) != 0 || stat.st_mode & libc::S_IFMT != libc::S_IFIFO {
            return Err("a jobserver descriptor is not a pipe");
        }
    }
    Ok(())
}

/// Whether a jobserver `auth` value names a usable jobserver for this process.
pub fn check_auth(auth: &str) -> std::result::Result<(), &'static str> {
    if let Some(path) = auth.strip_prefix("fifo:") {
        let meta = std::fs::symlink_metadata(path).map_err(|_| "the jobserver FIFO is missing")?;
        if !meta.file_type().is_fifo() {
            return Err("the jobserver path is not a FIFO");
        }
        // SAFETY: geteuid has no preconditions.
        if meta.uid() != unsafe { libc::geteuid() } {
            return Err("the jobserver FIFO belongs to another user");
        }
        return Ok(());
    }
    let (read, write) = auth
        .split_once(',')
        .ok_or("the jobserver reference is not understood")?;
    let read: RawFd = read
        .parse()
        .map_err(|_| "the jobserver reference is not understood")?;
    let write: RawFd = write
        .parse()
        .map_err(|_| "the jobserver reference is not understood")?;
    inheritable_pipe(read)?;
    inheritable_pipe(write)
}

/// Every jobserver reference the environment carries, checked.
pub fn inherited(environment: &BTreeMap<OsString, OsString>) -> Vec<JobserverReference> {
    JOBSERVER_VARIABLES
        .iter()
        .filter_map(|variable| {
            let flags = environment.get(OsStr::new(variable))?.to_str()?.to_string();
            let auth = auth_of(&flags)?;
            let (valid, reason) = match check_auth(&auth) {
                Ok(()) => (true, "the inherited jobserver is usable".to_string()),
                Err(reason) => (false, reason.to_string()),
            };
            Some(JobserverReference {
                variable: variable.to_string(),
                auth,
                valid,
                reason,
            })
        })
        .collect()
}

/// The jobserver Cargo will use: Cargo reads only the first of its variables
/// that is present, after the adapter's own changes, and uses it only when it
/// names a usable jobserver.
fn governing(
    environment: &BTreeMap<OsString, OsString>,
    set: &BTreeMap<String, String>,
    removed: &BTreeSet<String>,
) -> Option<String> {
    let variable = JOBSERVER_VARIABLES.iter().find(|variable| {
        set.contains_key(**variable)
            || (!removed.contains(**variable) && environment.contains_key(OsStr::new(variable)))
    })?;
    let flags = match set.get(*variable) {
        Some(value) => value.clone(),
        None => environment.get(OsStr::new(variable))?.to_str()?.to_string(),
    };
    let auth = auth_of(&flags)?;
    check_auth(&auth).ok().map(|()| variable.to_string())
}

/// A jobserver reference, or a job count that belongs to one.
fn jobserver_flag(flag: &str) -> bool {
    flag.starts_with("--jobserver-auth=")
        || flag.starts_with("--jobserver-fds=")
        || flag
            .strip_prefix("-j")
            .is_some_and(|count| count.bytes().all(|b| b.is_ascii_digit()))
}

/// Remove invalid jobserver references and the job counts that came with
/// them, keeping any other flags. Returns the variables to set and remove.
fn strip_invalid(
    environment: &BTreeMap<OsString, OsString>,
    references: &[JobserverReference],
    set: &mut BTreeMap<String, String>,
    removed: &mut BTreeSet<String>,
) {
    for reference in references.iter().filter(|reference| !reference.valid) {
        let flags = environment
            .get(OsStr::new(&reference.variable))
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let kept: Vec<&str> = flags
            .split_whitespace()
            .filter(|flag| !jobserver_flag(flag))
            .collect();
        if kept.is_empty() {
            removed.insert(reference.variable.clone());
        } else {
            set.insert(reference.variable.clone(), kept.join(" "));
        }
    }
}

fn references_report(references: &[JobserverReference]) -> Value {
    Value::Array(
        references
            .iter()
            .map(|reference| {
                json!({"variable": reference.variable, "auth": reference.auth,
                       "valid": reference.valid, "reason": reference.reason})
            })
            .collect(),
    )
}

/// Parse a jobs value the way Cargo reads it: a positive count, or a
/// host-relative value (`default`, zero is invalid, negative counts back from
/// the host's CPUs).
enum JobsValue {
    Count(u64),
    HostRelative,
}

fn jobs_value(value: &str) -> Result<JobsValue> {
    if value == "default" {
        return Ok(JobsValue::HostRelative);
    }
    let parsed: i64 = value.parse().map_err(|_| {
        refused(
            ErrorCode::InvalidRequest,
            format!("the Cargo jobs value {value:?} is not a number"),
        )
    })?;
    match parsed {
        0 => Err(refused(
            ErrorCode::InvalidRequest,
            "Cargo jobs may not be zero",
        )),
        count if count > 0 => Ok(JobsValue::Count(count as u64)),
        _ => Ok(JobsValue::HostRelative),
    }
}

/// Direct mode: `args` follow the `cargo` program.
pub fn direct(
    args: &[OsString],
    environment: &BTreeMap<OsString, OsString>,
    budget: Budget,
) -> Result<Plan> {
    let jobs = required_jobs(budget)?;
    let parsed = parse(args)?;
    let subcommand = parsed.subcommand.clone().ok_or_else(|| {
        refused(
            ErrorCode::ResourcePolicyUnsupported,
            "cargo without a subcommand has no compilation to fit",
        )
    })?;
    if !supported(&subcommand) {
        return Err(refused(
            ErrorCode::ResourcePolicyUnsupported,
            format!("cargo {subcommand} is not supported by the Cargo adapter; select the generic adapter"),
        ));
    }
    if parsed.jobs.len() > 1 {
        return Err(refused(
            ErrorCode::InvalidRequest,
            "Cargo jobs are given more than once; Cargo rejects repeated jobs options",
        ));
    }
    let references = inherited(environment);
    let mut plan = Plan {
        args: args.to_vec(),
        ..Plan::default()
    };
    strip_invalid(environment, &references, &mut plan.set, &mut plan.removed);
    let environment_jobs = environment
        .get(OsStr::new("CARGO_BUILD_JOBS"))
        .map(|value| value.to_string_lossy().into_owned());
    let original = parsed.jobs.first().map(|(_, value, _)| value.clone());
    let (action, applied, reason) = if let Some(variable) =
        governing(environment, &plan.set, &plan.removed)
    {
        // Cargo ignores -j under a jobserver; the inherited pool governs.
        (
            "inherited_jobserver",
            None,
            format!(
                "the jobserver inherited through {variable} bounds parallelism; its size cannot be observed"
            ),
        )
    } else if let Some((index, value, form)) = parsed.jobs.first().cloned() {
        let clamp = |plan: &mut Plan| rewrite_jobs(&mut plan.args, index, form, jobs);
        match jobs_value(&value)? {
            JobsValue::Count(count) if count <= jobs => (
                "kept",
                Some(count),
                format!("{count} jobs fit the reservation of {jobs}"),
            ),
            JobsValue::Count(count) => {
                clamp(&mut plan);
                (
                    "clamped",
                    Some(jobs),
                    format!("{count} jobs exceed the reservation, which fits {jobs}"),
                )
            }
            JobsValue::HostRelative => {
                clamp(&mut plan);
                (
                    "clamped",
                    Some(jobs),
                    format!("the host-relative value {value:?} was replaced by the reservation's {jobs}"),
                )
            }
        }
    } else {
        let at = parsed.subcommand_index.unwrap_or(0) + 1;
        plan.args.splice(
            at..at,
            [OsString::from("--jobs"), OsString::from(jobs.to_string())],
        );
        (
            "inserted",
            Some(jobs),
            "--jobs on the command line takes precedence over CARGO_BUILD_JOBS and configuration"
                .to_string(),
        )
    };
    plan.report = json!({
        "mode": "direct",
        "subcommand": subcommand,
        "reservation_jobs": jobs,
        "jobs": {"action": action, "original": original, "applied": applied, "reason": reason,
                 "environment": environment_jobs, "config": parsed.config_jobs},
        "jobserver": references_report(&references),
        "test_threads": "not capped: Cargo jobs bound compilation, not the threads of the programs it runs",
        "memory": "an accounting estimate of 512 MiB plus 1.5 GiB per job, not measured enforcement",
    });
    Ok(plan)
}

fn rewrite_jobs(args: &mut [OsString], index: usize, form: JobsForm, jobs: u64) {
    match form {
        JobsForm::Separate => args[index + 1] = OsString::from(jobs.to_string()),
        JobsForm::ShortJoined => args[index] = OsString::from(format!("-j{jobs}")),
        JobsForm::LongJoined => args[index] = OsString::from(format!("--jobs={jobs}")),
    }
}

/// Pipeline mode: a program that runs Cargo shares one jobserver sized to the
/// reservation, unless a valid one is inherited.
pub fn pipeline(environment: &BTreeMap<OsString, OsString>, budget: Budget) -> Result<Plan> {
    let jobs = required_jobs(budget)?;
    let references = inherited(environment);
    let mut plan = Plan::default();
    strip_invalid(environment, &references, &mut plan.set, &mut plan.removed);
    let (action, jobserver) = if let Some(variable) =
        governing(environment, &plan.set, &plan.removed)
    {
        (
            format!("the jobserver inherited through {variable} is shared; no new pool is created"),
            None,
        )
    } else {
        let jobserver = Jobserver::create(jobs)?;
        plan.set.insert(
            "CARGO_MAKEFLAGS".into(),
            format!("-j --jobserver-auth={}", jobserver.auth()),
        );
        plan.removed.remove("CARGO_MAKEFLAGS");
        plan.set.insert("CARGO_BUILD_JOBS".into(), jobs.to_string());
        (
            format!("a private FIFO jobserver with {jobs} jobs is shared through CARGO_MAKEFLAGS"),
            Some(jobserver),
        )
    };
    plan.report = json!({
        "mode": "pipeline",
        "reservation_jobs": jobs,
        "jobserver": {"action": action, "path": jobserver.as_ref().map(|js| js.fifo.clone()),
                      "inherited": references_report(&references)},
        "application": "environment: Cargo reads the jobserver and ignores its own -j while it is valid; CARGO_BUILD_JOBS bounds Cargo if the jobserver cannot be opened; each concurrently started top-level Cargo adds its own implicit job",
        "test_threads": "not capped: Cargo jobs bound compilation, not the threads of the programs it runs",
        "memory": "an accounting estimate of 512 MiB plus 1.5 GiB per job, not measured enforcement",
    });
    plan.jobserver = jobserver;
    Ok(plan)
}

/// A private FIFO jobserver. The adapter holds it open for the run, so the
/// pool survives between nested Cargo processes; dropping it removes it.
#[derive(Debug)]
pub struct Jobserver {
    directory: PathBuf,
    fifo: PathBuf,
    /// Holds the pool open between clients.
    fd: OwnedFd,
    /// Counts tokens: on macOS only a read-only descriptor reports a FIFO's
    /// contents through FIONREAD.
    reader: OwnedFd,
    jobs: u64,
}

fn io_error(message: &'static str) -> Error {
    refused(ErrorCode::ResourceControlUnavailable, message)
}

impl Jobserver {
    /// A pool of `jobs` jobs: `jobs - 1` tokens, as each client also holds
    /// the implicit job it started with.
    pub fn create(jobs: u64) -> Result<Self> {
        let template = std::env::temp_dir().join("devguard-jobserver-XXXXXX");
        let mut template = CString::new(template.as_os_str().as_bytes())
            .map_err(|_| io_error("the temporary directory path is not valid"))?
            .into_bytes_with_nul();
        // SAFETY: template is a live NUL-terminated buffer mkdtemp rewrites.
        let created = unsafe { libc::mkdtemp(template.as_mut_ptr().cast()) };
        if created.is_null() {
            return Err(io_error("cannot create the jobserver directory"));
        }
        template.pop();
        let directory = PathBuf::from(OsStr::from_bytes(&template));
        let fifo = directory.join("jobserver");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes())
            .map_err(|_| io_error("the jobserver path is not valid"))?;
        // SAFETY: fifo_c is a live NUL-terminated path.
        if unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) } != 0 {
            let _ = std::fs::remove_dir(&directory);
            return Err(io_error("cannot create the jobserver FIFO"));
        }
        // SAFETY: opening the FIFO created above; the descriptor is owned below.
        let fd = unsafe {
            libc::open(
                fifo_c.as_ptr(),
                libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let _ = std::fs::remove_file(&fifo);
            let _ = std::fs::remove_dir(&directory);
            return Err(io_error("cannot open the jobserver FIFO"));
        }
        // SAFETY: fd was just opened and nothing else owns it.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        // SAFETY: opening the same FIFO read-only, without blocking, for counting.
        let reader = unsafe {
            libc::open(
                fifo_c.as_ptr(),
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if reader < 0 {
            let _ = std::fs::remove_file(&fifo);
            let _ = std::fs::remove_dir(&directory);
            return Err(io_error("cannot open the jobserver FIFO for counting"));
        }
        // SAFETY: reader was just opened and nothing else owns it.
        let reader = unsafe { OwnedFd::from_raw_fd(reader) };
        let jobserver = Self {
            directory,
            fifo,
            fd,
            reader,
            jobs,
        };
        let tokens = vec![b'+'; (jobs - 1) as usize];
        if !tokens.is_empty() {
            // SAFETY: the buffer is live for the call; a FIFO accepts this
            // many bytes without blocking.
            let written = unsafe {
                libc::write(
                    jobserver.fd.as_raw_fd(),
                    tokens.as_ptr().cast(),
                    tokens.len(),
                )
            };
            if written != tokens.len() as isize {
                return Err(io_error("cannot fill the jobserver"));
            }
        }
        Ok(jobserver)
    }

    /// The value for `--jobserver-auth=`.
    pub fn auth(&self) -> String {
        format!("fifo:{}", self.fifo.display())
    }

    pub fn path(&self) -> &Path {
        &self.fifo
    }

    pub fn jobs(&self) -> u64 {
        self.jobs
    }

    /// Tokens in the pool now, read without taking them. After every client
    /// has ended this is `jobs - 1` unless a client died holding tokens.
    pub fn available(&self) -> u64 {
        let mut count: libc::c_int = 0;
        // SAFETY: FIONREAD writes one int for the live descriptor.
        if unsafe { libc::ioctl(self.reader.as_raw_fd(), libc::FIONREAD, &mut count) } != 0 {
            return 0;
        }
        count.max(0) as u64
    }
}

impl Drop for Jobserver {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.fifo);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn strings(values: &[OsString]) -> Vec<String> {
        values
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }

    fn two_jobs() -> Budget {
        budget_for(2, DEFAULT_TASKS)
    }

    #[test]
    fn jobs_fit_both_cpu_and_the_memory_estimate() {
        assert_eq!(jobs_for(budget_for(2, 1)), 2);
        assert_eq!(
            jobs_for(Budget {
                cpu_milli: 8_000,
                memory_bytes: FIXED_BYTES + PER_JOB_BYTES * 3,
                tasks: 1
            }),
            3
        );
        assert_eq!(
            jobs_for(Budget {
                cpu_milli: 1_999,
                memory_bytes: 64 << 30,
                tasks: 1
            }),
            1
        );
        let too_small = Budget {
            cpu_milli: 900,
            memory_bytes: 64 << 30,
            tasks: 1,
        };
        assert_eq!(jobs_for(too_small), 0);
        assert_eq!(
            required_jobs(too_small).unwrap_err().code,
            ErrorCode::ResourceUnavailable
        );
        assert_eq!(
            required_jobs(Budget {
                cpu_milli: 4_000,
                memory_bytes: FIXED_BYTES + PER_JOB_BYTES - 1,
                tasks: 1
            })
            .unwrap_err()
            .code,
            ErrorCode::ResourceUnavailable
        );
    }

    #[test]
    fn global_options_toolchains_and_separators_are_parsed_around_the_subcommand() {
        let parsed = parse(&args(&[
            "+1.95.0",
            "--locked",
            "--config",
            "build.jobs=6",
            "-Z",
            "unstable-options",
            "test",
            "--workspace",
            "-j",
            "8",
            "--",
            "-j",
            "3",
        ]))
        .unwrap();
        assert_eq!(parsed.subcommand.as_deref(), Some("test"));
        assert_eq!(parsed.subcommand_index, Some(6));
        assert_eq!(parsed.jobs, vec![(8, "8".to_string(), JobsForm::Separate)]);
        assert_eq!(parsed.config_jobs, vec!["6".to_string()]);
        let joined = parse(&args(&["build", "-j4", "--release"])).unwrap();
        assert_eq!(
            joined.jobs,
            vec![(1, "4".to_string(), JobsForm::ShortJoined)]
        );
        let long = parse(&args(&["check", "--jobs=3"])).unwrap();
        assert_eq!(long.jobs, vec![(1, "3".to_string(), JobsForm::LongJoined)]);
        assert_eq!(parse(&args(&["--version"])).unwrap().subcommand, None);
    }

    #[test]
    fn direct_mode_inserts_keeps_or_clamps_jobs_and_never_touches_paths() {
        let environment = env(&[
            ("CARGO_TARGET_DIR", "/tmp/target"),
            ("CARGO_BUILD_JOBS", "16"),
        ]);
        let inserted = direct(&args(&["build", "--release"]), &environment, two_jobs()).unwrap();
        assert_eq!(
            strings(&inserted.args),
            ["build", "--jobs", "2", "--release"]
        );
        assert_eq!(inserted.report["jobs"]["action"], "inserted");
        assert_eq!(inserted.report["jobs"]["environment"], "16");
        assert!(inserted.set.is_empty() && inserted.removed.is_empty());
        let kept = direct(&args(&["build", "-j", "1"]), &environment, two_jobs()).unwrap();
        assert_eq!(strings(&kept.args), ["build", "-j", "1"]);
        assert_eq!(kept.report["jobs"]["action"], "kept");
        for (given, expected) in [
            (
                &["test", "-j", "8", "--", "-j", "9"][..],
                &["test", "-j", "2", "--", "-j", "9"][..],
            ),
            (&["check", "-j8"], &["check", "-j2"]),
            (&["build", "--jobs=9"], &["build", "--jobs=2"]),
            (&["build", "--jobs", "default"], &["build", "--jobs", "2"]),
            (&["build", "-j", "-1"], &["build", "-j", "2"]),
        ] {
            let plan = direct(&args(given), &environment, two_jobs()).unwrap();
            assert_eq!(strings(&plan.args), expected, "{given:?}");
            assert_eq!(plan.report["jobs"]["action"], "clamped", "{given:?}");
            assert_eq!(plan.report["jobs"]["applied"], 2);
        }
    }

    #[test]
    fn direct_mode_refuses_conflicts_unsupported_subcommands_and_small_reservations() {
        let environment = env(&[]);
        for (given, code) in [
            (
                &["build", "-j", "2", "--jobs", "3"][..],
                ErrorCode::InvalidRequest,
            ),
            (&["build", "-j", "2", "-j", "2"], ErrorCode::InvalidRequest),
            (&["build", "-j", "0"], ErrorCode::InvalidRequest),
            (&["build", "-j", "many"], ErrorCode::InvalidRequest),
            (&["build", "-j"], ErrorCode::InvalidRequest),
            (&["metadata"], ErrorCode::ResourcePolicyUnsupported),
            (&["fmt"], ErrorCode::ResourcePolicyUnsupported),
            (&["--version"], ErrorCode::ResourcePolicyUnsupported),
        ] {
            assert_eq!(
                direct(&args(given), &environment, two_jobs())
                    .unwrap_err()
                    .code,
                code,
                "{given:?}"
            );
        }
        let small = Budget {
            cpu_milli: 500,
            memory_bytes: 8 << 30,
            tasks: 8,
        };
        assert_eq!(
            direct(&args(&["build"]), &environment, small)
                .unwrap_err()
                .code,
            ErrorCode::ResourceUnavailable
        );
    }

    #[test]
    fn invalid_inherited_jobservers_are_removed_and_valid_ones_govern() {
        // Descriptors that are not open in this process are not a jobserver.
        let stale = env(&[
            ("MAKEFLAGS", "-k -j --jobserver-auth=900,901"),
            (
                "CARGO_MAKEFLAGS",
                "-j4 --jobserver-fds=902,903 --jobserver-auth=902,903",
            ),
        ]);
        let plan = direct(&args(&["build"]), &stale, two_jobs()).unwrap();
        assert_eq!(plan.set.get("MAKEFLAGS").map(String::as_str), Some("-k"));
        assert!(plan.removed.contains("CARGO_MAKEFLAGS"));
        assert_eq!(plan.report["jobs"]["action"], "inserted");
        assert_eq!(plan.report["jobserver"][0]["valid"], false);
        // A FIFO owned by this user is usable and governs parallelism.
        let jobserver = Jobserver::create(3).unwrap();
        let valid = env(&[(
            "CARGO_MAKEFLAGS",
            &format!("-j --jobserver-auth={}", jobserver.auth()),
        )]);
        let plan = direct(&args(&["build", "-j", "16"]), &valid, two_jobs()).unwrap();
        assert_eq!(strings(&plan.args), ["build", "-j", "16"]);
        assert_eq!(plan.report["jobs"]["action"], "inherited_jobserver");
        assert!(plan.set.is_empty() && plan.removed.is_empty());
        // A close-on-exec pipe would not reach Cargo, so it is invalid.
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        unsafe {
            libc::fcntl(read.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(write.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
        }
        let auth = format!("{},{}", read.as_raw_fd(), write.as_raw_fd());
        assert!(check_auth(&auth).unwrap_err().contains("close-on-exec"));
        unsafe {
            libc::fcntl(read.as_raw_fd(), libc::F_SETFD, 0);
            libc::fcntl(write.as_raw_fd(), libc::F_SETFD, 0);
        }
        assert_eq!(check_auth(&auth), Ok(()));
    }

    #[test]
    fn a_pipeline_shares_one_fifo_jobserver_sized_to_the_reservation() {
        let plan = pipeline(&env(&[]), budget_for(3, 8)).unwrap();
        let jobserver = plan.jobserver.as_ref().unwrap();
        assert_eq!(jobserver.jobs(), 3);
        assert_eq!(jobserver.available(), 2);
        assert_eq!(
            plan.set["CARGO_MAKEFLAGS"],
            format!("-j --jobserver-auth={}", jobserver.auth())
        );
        assert_eq!(plan.set["CARGO_BUILD_JOBS"], "3");
        assert!(check_auth(&jobserver.auth()).is_ok());
        for fd in [jobserver.fd.as_raw_fd(), jobserver.reader.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert!(
                flags & libc::FD_CLOEXEC != 0,
                "the pool is shared by path only"
            );
        }
        let path = jobserver.path().to_path_buf();
        drop(plan);
        assert!(!path.exists(), "the jobserver is removed with the plan");
        // An inherited jobserver is shared rather than replaced.
        let outer = Jobserver::create(2).unwrap();
        let inherited_plan = pipeline(
            &env(&[(
                "CARGO_MAKEFLAGS",
                &format!("-j --jobserver-auth={}", outer.auth()),
            )]),
            budget_for(3, 8),
        )
        .unwrap();
        assert!(inherited_plan.jobserver.is_none());
        assert!(inherited_plan.set.is_empty());
        assert_eq!(
            pipeline(
                &env(&[]),
                Budget {
                    cpu_milli: 999,
                    memory_bytes: 8 << 30,
                    tasks: 8
                }
            )
            .unwrap_err()
            .code,
            ErrorCode::ResourceUnavailable
        );
    }
}
