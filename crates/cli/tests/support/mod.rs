//! Real CLI processes against isolated authorities. The test binary is
//! re-executed in two roles: as the CLI (`cli_child`), which runs the library
//! entry point against a fixture authority, and as the workload (`payload`),
//! which records what it inherited. The shipped `devguard` binary accepts no
//! authority override; only this compiled test harness constructs one.

#![allow(dead_code)]

use devguard_client::protocol::CallerCredential;
use devguard_contract::Budget;
use devguard_daemon::config::HostConfig;
use devguard_daemon::fixture::TestAuthority;
use devguard_daemon::paths::AuthorityPaths;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Configuration for the CLI role; removed before the workload starts.
pub const CLI_CHILD: &str = "DEVGUARD_TEST_CLI";
/// Configuration for the workload role.
pub const PROBE: &str = "DEVGUARD_TEST_PROBE";
pub const MIB: u64 = 1024 * 1024;
pub const EXIT_LIMIT: Duration = Duration::from_secs(30);

pub struct Fixture {
    pub authority: TestAuthority,
    // Dropped after the authority, which serves from it.
    pub directory: tempfile::TempDir,
}

pub fn private_directory(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in("/private/tmp")
        .unwrap()
}

impl Fixture {
    pub fn start() -> Self {
        Self::start_with(|_| {})
    }

    pub fn start_with(configure: impl FnOnce(&mut HostConfig)) -> Self {
        let directory = private_directory("dg-c-");
        let authority = TestAuthority::start_with(directory.path(), configure).unwrap();
        authority
            .wait_until_admitting(Duration::from_secs(10))
            .unwrap();
        Self {
            authority,
            directory,
        }
    }

    pub fn base(&self) -> &Path {
        self.directory.path()
    }

    /// A private working directory for one test's files.
    pub fn work(&self, name: &str) -> PathBuf {
        let path = self.base().join(name);
        std::fs::create_dir(&path).unwrap();
        path.canonicalize().unwrap()
    }

    pub fn secret(&self) -> String {
        match self.authority.consumer().unwrap() {
            CallerCredential::Consumer { secret, .. } => secret.expose().to_string(),
            _ => unreachable!("the fixture consumer is a workload consumer"),
        }
    }

    pub fn capacity(&self) -> Budget {
        self.authority.work_capacity().unwrap()
    }
}

pub fn probe_args(test: &str) -> Vec<String> {
    [
        "--exact",
        test,
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

pub fn exe() -> String {
    std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// `devguard-launch` beside this test's target directory. The workspace test
/// run builds every binary first.
pub fn helper() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile = exe.parent().unwrap().parent().unwrap();
    let helper = profile.join("devguard-launch");
    assert!(
        helper.is_file(),
        "build devguard-launch before these tests: {}",
        helper.display()
    );
    helper
}

pub fn quiet(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
}

/// Start the CLI as its own process against `base`'s fixture authority.
pub fn cli_at(base: &Path, args: &[String], configure: impl FnOnce(&mut Command)) -> Child {
    let mut command = Command::new(exe());
    command.args(probe_args("cli_child"));
    command.env(
        CLI_CHILD,
        json!({"base": base, "helper": helper(), "args": args}).to_string(),
    );
    configure(&mut command);
    command.spawn().unwrap()
}

pub fn cli(fixture: &Fixture, args: &[String], configure: impl FnOnce(&mut Command)) -> Child {
    cli_at(fixture.base(), args, configure)
}

/// The CLI role: run the library entry point against the fixture authority
/// and end the process as the CLI would.
pub fn cli_child() {
    let Some(config) = std::env::var_os(CLI_CHILD) else {
        return;
    };
    std::env::remove_var(CLI_CHILD);
    let config: Value = serde_json::from_str(&config.to_string_lossy()).unwrap();
    let base = PathBuf::from(config["base"].as_str().unwrap());
    let helper = PathBuf::from(config["helper"].as_str().unwrap());
    let args: Vec<OsString> = config["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| OsString::from(arg.as_str().unwrap()))
        .collect();
    let exit = devguard_cli::run(args, || Ok((AuthorityPaths::fixture(&base), helper)));
    devguard_cli::terminate(exit)
}

/// The workload role. It records what it inherited to `out`, can leave a
/// survivor in its group, signal readiness, sleep, and end by exiting or by
/// raising a signal.
pub fn payload() {
    let Some(config) = std::env::var_os(PROBE) else {
        return;
    };
    let config: Value = serde_json::from_str(&config.to_string_lossy()).unwrap();
    // SAFETY: these calls only read this process's identity and descriptor 0.
    let (pgid, ppid, tty, foreground) = unsafe {
        (
            libc::getpgrp(),
            libc::getppid(),
            libc::isatty(0) == 1,
            libc::tcgetpgrp(0),
        )
    };
    let marks: serde_json::Map<String, Value> = std::env::vars()
        .filter(|(name, _)| name.starts_with("DEVGUARD_FIXTURE_"))
        .map(|(name, value)| (name, Value::String(value)))
        .collect();
    let names: Vec<String> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    let mut survivor = None;
    if let Some(seconds) = config["survivor_seconds"].as_u64() {
        let child = Command::new("/bin/sleep")
            .arg(seconds.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        survivor = Some(child.id());
        std::mem::forget(child);
    }
    if let Some(out) = config["out"].as_str() {
        let record = json!({
            "pid": std::process::id(), "pgid": pgid, "ppid": ppid,
            "argv": std::env::args().collect::<Vec<_>>(),
            "cwd": std::env::current_dir().unwrap(),
            "marks": marks, "environment_names": names,
            "tty": tty, "terminal_foreground_group": foreground,
            "survivor": survivor,
        });
        std::fs::write(out, record.to_string()).unwrap();
    }
    if let Some(ready) = config["ready"].as_str() {
        std::fs::write(ready, b"ready").unwrap();
    }
    if let Some(millis) = config["sleep_ms"].as_u64() {
        std::thread::sleep(Duration::from_millis(millis));
    }
    if let Some(signal) = config["signal"].as_i64() {
        // SAFETY: restoring the default action and raising a signal on this
        // process have no preconditions.
        unsafe {
            libc::signal(signal as i32, libc::SIG_DFL);
            libc::raise(signal as i32);
        }
    }
    std::process::exit(config["exit"].as_i64().unwrap_or(0) as i32);
}

/// The CLI arguments that run the payload role with `extra` CLI options.
pub fn exec_payload(extra: &[&str]) -> Vec<String> {
    let mut args = vec!["exec".to_string()];
    args.extend(extra.iter().map(|arg| arg.to_string()));
    args.push("--".into());
    args.push(exe());
    args.extend(probe_args("payload"));
    args
}

pub fn wait_exit(child: &mut Child, limit: Duration) -> ExitStatus {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("process {} did not exit in {limit:?}", child.id());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "condition not met in {limit:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// Receipts may be shared as evidence: no caller credential may appear.
pub fn assert_no_secret(text: &str, secret: &str) {
    assert!(!secret.is_empty());
    assert!(
        !text.contains(secret),
        "a receipt contains the caller credential"
    );
}

/// Qualification runs collect raw receipts; ordinary runs write nothing.
pub fn record(name: &str, value: Value, secret: &str) {
    let text = serde_json::to_string_pretty(&value).unwrap();
    assert_no_secret(&text, secret);
    if let Some(directory) = std::env::var_os("DEVGUARD_EVIDENCE_DIR") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{name}.json")), text).unwrap();
    }
}

/// Whether `pid` still names a live or unreaped process.
pub fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks for existence.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// The doctor's report from a CLI child's standard output. The re-executed
/// test harness prints its own status text around it, which has no braces.
pub fn doctor_report(stdout: &str) -> Value {
    let start = stdout.find('{').expect("a doctor report");
    let end = stdout.rfind('}').expect("a complete doctor report");
    serde_json::from_str(&stdout[start..=end]).unwrap()
}
