//! Strict command-line parsing. The program and its arguments follow `--` and
//! are passed as an argv vector; nothing is interpreted by a shell.

use devguard_contract::{validate_id, AttemptKey, Error, ErrorCode, Result};
use devguard_daemon::candidate::parse_key;
use devguard_daemon::config::AdapterSelection;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// The longest explicit wait accepted. Waiting is a bounded convenience, never
/// an implicit queue.
pub const MAX_WAIT: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Exec(ExecArgs),
    Doctor(DoctorArgs),
    TestCandidate(CandidateArgs),
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecArgs {
    pub project: Option<String>,
    pub adapter: Option<AdapterSelection>,
    pub wait: Option<Duration>,
    pub cpu_milli: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub tasks: Option<u64>,
    pub receipt: Option<PathBuf>,
    /// Run as a child of this parent lease, admitted against its remainder
    /// with the token read from `lease_token_fd`.
    pub lease: Option<AttemptKey>,
    pub lease_token_fd: Option<i32>,
    pub program: OsString,
    pub args: Vec<OsString>,
}

/// `devguard test-candidate`: a candidate tree verified under a parent lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateArgs {
    pub candidate: PathBuf,
    pub report: PathBuf,
    pub cpu_milli: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub tasks: Option<u64>,
    pub ttl: Option<Duration>,
    pub wait: Option<Duration>,
}

/// What `doctor --require` can demand of the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Requirement {
    /// Durable admission and fenced launch are advertised and execution is ready.
    Admission,
    /// Registration of this owner is open.
    Registration,
    /// The macOS cooperative CPU policy is advertised.
    MacosCooperative,
}

impl Requirement {
    pub fn name(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Registration => "registration",
            Self::MacosCooperative => "macos-cooperative",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DoctorArgs {
    pub require: BTreeSet<Requirement>,
    pub project: Option<String>,
}

fn usage(message: &'static str) -> Error {
    Error::new(ErrorCode::InvalidRequest, message)
}

fn text(value: OsString, message: &'static str) -> Result<String> {
    value.into_string().map_err(|_| usage(message))
}

/// `500ms`, `30s`, `2m` or `1h`. Zero is allowed and means a single attempt.
pub fn parse_duration(value: &str) -> Result<Duration> {
    let invalid = || usage("durations are a whole number followed by ms, s, m or h");
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(invalid)?;
    let (number, unit) = value.split_at(split);
    if number.is_empty() || number.len() > 12 {
        return Err(invalid());
    }
    let number: u64 = number.parse().map_err(|_| invalid())?;
    let millis = match unit {
        "ms" => Some(number),
        "s" => number.checked_mul(1_000),
        "m" => number.checked_mul(60_000),
        "h" => number.checked_mul(3_600_000),
        _ => None,
    }
    .ok_or_else(invalid)?;
    let duration = Duration::from_millis(millis);
    if duration > MAX_WAIT {
        return Err(usage("an explicit wait may not exceed 24h"));
    }
    Ok(duration)
}

/// Whole bytes, or a whole number of KiB, MiB or GiB.
pub fn parse_size(value: &str) -> Result<u64> {
    let invalid = || usage("sizes are whole bytes or a whole number of KiB, MiB or GiB");
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    if number.is_empty() || number.len() > 20 {
        return Err(invalid());
    }
    let number: u64 = number.parse().map_err(|_| invalid())?;
    let factor: u64 = match unit {
        "" | "B" => 1,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        _ => return Err(invalid()),
    };
    number.checked_mul(factor).ok_or_else(invalid)
}

fn parse_count(value: &str, message: &'static str) -> Result<u64> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(usage(message));
    }
    value.parse().map_err(|_| usage(message))
}

fn parse_adapter(value: &str) -> Result<AdapterSelection> {
    Ok(match value {
        "auto" => AdapterSelection::Auto,
        "generic" => AdapterSelection::Generic,
        "cargo" => AdapterSelection::Cargo,
        "cargo-pipeline" => AdapterSelection::CargoPipeline,
        _ => return Err(usage("adapters are auto, generic, cargo or cargo-pipeline")),
    })
}

fn parse_requirements(value: &str) -> Result<BTreeSet<Requirement>> {
    value
        .split(',')
        .map(|name| match name {
            "admission" => Ok(Requirement::Admission),
            "registration" => Ok(Requirement::Registration),
            "macos-cooperative" => Ok(Requirement::MacosCooperative),
            _ => Err(usage(
                "requirements are admission, registration and macos-cooperative",
            )),
        })
        .collect()
}

/// Take the value of an option exactly once.
fn once<T>(slot: &mut Option<T>, value: T) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(usage("an option was given more than once"));
    }
    Ok(())
}

fn value(raw: &mut impl Iterator<Item = OsString>) -> Result<String> {
    text(
        raw.next()
            .ok_or_else(|| usage("an option is missing its value"))?,
        "option values must be UTF-8",
    )
}

fn project_id(value: String) -> Result<String> {
    validate_id(&value)?;
    Ok(value)
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command> {
    let mut raw = args.into_iter();
    let Some(first) = raw.next() else {
        return Err(usage("expected exec, doctor, --help or --version"));
    };
    match first.to_str() {
        Some("exec") => parse_exec(raw).map(Command::Exec),
        Some("doctor") => parse_doctor(raw).map(Command::Doctor),
        Some("test-candidate") => parse_candidate(raw).map(Command::TestCandidate),
        Some("--help" | "-h" | "help") if raw.next().is_none() => Ok(Command::Help),
        Some("--version" | "-V") if raw.next().is_none() => Ok(Command::Version),
        _ => Err(usage(
            "expected exec, doctor, test-candidate, --help or --version",
        )),
    }
}

fn descriptor(value: &str) -> Result<i32> {
    let fd = parse_count(value, "descriptors are whole numbers above 2")?;
    i32::try_from(fd)
        .ok()
        .filter(|fd| *fd > 2)
        .ok_or_else(|| usage("descriptors are whole numbers above 2"))
}

fn parse_exec(mut raw: impl Iterator<Item = OsString>) -> Result<ExecArgs> {
    let (mut project, mut adapter, mut wait, mut cpu, mut memory, mut tasks, mut receipt) =
        (None, None, None, None, None, None, None);
    let (mut lease, mut lease_token_fd) = (None, None);
    loop {
        let option = raw
            .next()
            .ok_or_else(|| usage("expected `-- PROGRAM [ARGS...]`; no shell is used"))?;
        match option.to_str() {
            Some("--") => break,
            Some("--project") => once(&mut project, project_id(value(&mut raw)?)?)?,
            Some("--adapter") => once(&mut adapter, parse_adapter(&value(&mut raw)?)?)?,
            Some("--wait") => once(&mut wait, parse_duration(&value(&mut raw)?)?)?,
            Some("--cpu") => once(
                &mut cpu,
                parse_count(&value(&mut raw)?, "--cpu takes whole millicpu")?,
            )?,
            Some("--memory") => once(&mut memory, parse_size(&value(&mut raw)?)?)?,
            Some("--tasks") => once(
                &mut tasks,
                parse_count(&value(&mut raw)?, "--tasks takes a whole number")?,
            )?,
            Some("--receipt") => once(&mut receipt, PathBuf::from(value(&mut raw)?))?,
            Some("--lease") => once(&mut lease, parse_key(&value(&mut raw)?)?)?,
            Some("--lease-token-fd") => once(&mut lease_token_fd, descriptor(&value(&mut raw)?)?)?,
            _ => {
                return Err(usage(
                    "unknown exec option; the program must follow `--` and no shell is used",
                ))
            }
        }
    }
    if lease.is_some() != lease_token_fd.is_some() {
        return Err(usage("--lease and --lease-token-fd are given together"));
    }
    let program = raw
        .next()
        .ok_or_else(|| usage("expected a program after `--`"))?;
    if program.is_empty() {
        return Err(usage("the program must not be empty"));
    }
    Ok(ExecArgs {
        project,
        adapter,
        wait,
        cpu_milli: cpu,
        memory_bytes: memory,
        tasks,
        receipt,
        lease,
        lease_token_fd,
        program,
        args: raw.collect(),
    })
}

fn parse_candidate(mut raw: impl Iterator<Item = OsString>) -> Result<CandidateArgs> {
    let (mut candidate, mut report, mut cpu, mut memory, mut tasks, mut ttl, mut wait) =
        (None, None, None, None, None, None, None);
    while let Some(option) = raw.next() {
        match option.to_str() {
            Some("--candidate") => once(&mut candidate, PathBuf::from(value(&mut raw)?))?,
            Some("--report") => once(&mut report, PathBuf::from(value(&mut raw)?))?,
            Some("--cpu") => once(
                &mut cpu,
                parse_count(&value(&mut raw)?, "--cpu takes whole millicpu")?,
            )?,
            Some("--memory") => once(&mut memory, parse_size(&value(&mut raw)?)?)?,
            Some("--tasks") => once(
                &mut tasks,
                parse_count(&value(&mut raw)?, "--tasks takes a whole number")?,
            )?,
            Some("--ttl") => once(&mut ttl, parse_duration(&value(&mut raw)?)?)?,
            Some("--wait") => once(&mut wait, parse_duration(&value(&mut raw)?)?)?,
            _ => {
                return Err(usage(
                    "test-candidate accepts --candidate, --report, --cpu, --memory, --tasks, --ttl and --wait",
                ))
            }
        }
    }
    Ok(CandidateArgs {
        candidate: candidate.ok_or_else(|| usage("test-candidate needs --candidate DIR"))?,
        report: report.ok_or_else(|| usage("test-candidate needs --report DIR"))?,
        cpu_milli: cpu,
        memory_bytes: memory,
        tasks,
        ttl,
        wait,
    })
}

fn parse_doctor(mut raw: impl Iterator<Item = OsString>) -> Result<DoctorArgs> {
    let (mut require, mut project) = (None, None);
    while let Some(option) = raw.next() {
        match option.to_str() {
            Some("--require") => once(&mut require, parse_requirements(&value(&mut raw)?)?)?,
            Some("--project") => once(&mut project, project_id(value(&mut raw)?)?)?,
            _ => return Err(usage("doctor accepts only --require and --project")),
        }
    }
    Ok(DoctorArgs {
        require: require.unwrap_or_default(),
        project,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn exec_takes_the_program_after_the_separator_verbatim() {
        let parsed = parse(args(&[
            "exec",
            "--project",
            "devguard-dev",
            "--wait",
            "30s",
            "--",
            "cargo",
            "test",
            "--",
            "--nocapture",
        ]))
        .unwrap();
        let Command::Exec(exec) = parsed else {
            panic!("expected exec")
        };
        assert_eq!(exec.project.as_deref(), Some("devguard-dev"));
        assert_eq!(exec.wait, Some(Duration::from_secs(30)));
        assert_eq!(exec.program, OsString::from("cargo"));
        assert_eq!(exec.args, args(&["test", "--", "--nocapture"]));
    }

    #[test]
    fn lease_children_and_candidate_runs_take_explicit_options() {
        let Command::Exec(exec) = parse(args(&[
            "exec",
            "--lease",
            "dev-cli/g/lease-1",
            "--lease-token-fd",
            "5",
            "--",
            "true",
        ]))
        .unwrap() else {
            panic!("expected exec")
        };
        assert_eq!(exec.lease.unwrap().attempt_id, "lease-1");
        assert_eq!(exec.lease_token_fd, Some(5));
        for invalid in [
            &["exec", "--lease", "dev-cli/g/lease-1", "--", "true"][..],
            &["exec", "--lease-token-fd", "5", "--", "true"],
            &[
                "exec",
                "--lease",
                "dev-cli/g",
                "--lease-token-fd",
                "5",
                "--",
                "true",
            ],
            &[
                "exec",
                "--lease",
                "dev-cli/g/l",
                "--lease-token-fd",
                "2",
                "--",
                "true",
            ],
            &["test-candidate", "--candidate", "/x"],
            &["test-candidate", "--report", "/r"],
            &[
                "test-candidate",
                "--candidate",
                "/x",
                "--report",
                "/r",
                "--cpu",
                "1.5",
            ],
            &[
                "test-candidate",
                "--candidate",
                "/x",
                "--report",
                "/r",
                "--",
                "true",
            ],
        ] {
            assert!(parse(args(invalid)).is_err(), "{invalid:?}");
        }
        let Command::TestCandidate(candidate) = parse(args(&[
            "test-candidate",
            "--candidate",
            "/x",
            "--report",
            "/r",
            "--memory",
            "4GiB",
            "--ttl",
            "2h",
        ]))
        .unwrap() else {
            panic!("expected test-candidate")
        };
        assert_eq!(candidate.memory_bytes, Some(4 << 30));
        assert_eq!(candidate.ttl, Some(Duration::from_secs(7_200)));
        assert_eq!(candidate.wait, None);
    }

    #[test]
    fn exec_refuses_ambiguous_or_shell_like_invocations() {
        for invalid in [
            &["exec", "cargo", "build"][..],
            &["exec", "--"],
            &["exec", "--", ""],
            &["exec", "--wait", "30", "--", "true"],
            &["exec", "--wait", "5s", "--wait", "5s", "--", "true"],
            &["exec", "--project", "a b", "--", "true"],
            &["exec", "--adapter", "shell", "--", "true"],
            &["exec", "--cpu", "-1", "--", "true"],
            &["exec", "--memory", "1GB", "--", "true"],
            &["exec", "--wait", "25h", "--", "true"],
            &["run", "--", "true"],
            &["--help", "extra"],
            &[],
        ] {
            assert!(parse(args(invalid)).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn durations_sizes_and_requirements_are_exact() {
        assert_eq!(parse_duration("0s").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("24h").unwrap(), MAX_WAIT);
        for invalid in ["", "s", "1.5s", "-1s", "10", "1d", "99999999999999h"] {
            assert!(parse_duration(invalid).is_err(), "{invalid}");
        }
        assert_eq!(parse_size("512MiB").unwrap(), 512 << 20);
        assert_eq!(parse_size("2GiB").unwrap(), 2 << 30);
        assert_eq!(parse_size("4096").unwrap(), 4096);
        for invalid in ["", "MiB", "1.5GiB", "2GB", "99999999999GiB"] {
            assert!(parse_size(invalid).is_err(), "{invalid}");
        }
        let Command::Doctor(doctor) = parse(args(&[
            "doctor",
            "--require",
            "admission,macos-cooperative",
        ]))
        .unwrap() else {
            panic!("expected doctor")
        };
        assert_eq!(
            doctor.require,
            BTreeSet::from([Requirement::Admission, Requirement::MacosCooperative])
        );
        assert!(parse(args(&["doctor", "--require", "kernel"])).is_err());
    }
}
