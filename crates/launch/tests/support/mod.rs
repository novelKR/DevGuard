//! Real helper launches through an isolated authority served in this process.
//! The test process is the registered owner and the helper's parent.

#![allow(dead_code)]

use devguard_client::launch::{helper_command, HelperReport, HelperTicket};
use devguard_client::protocol::CallerCredential;
use devguard_client::Client;
use devguard_contract::*;
use devguard_daemon::fixture::{TestAuthority, CONSUMER};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

pub const HELPER: &str = env!("CARGO_BIN_EXE_devguard-launch");
pub const MIB: u64 = 1024 * 1024;
/// How long a helper may take to report before a test treats it as lost.
pub const REPORT_LIMIT: Duration = Duration::from_secs(10);

pub struct Fixture {
    pub authority: TestAuthority,
    // Dropped after the authority, which serves from it.
    pub directory: tempfile::TempDir,
}

impl Fixture {
    pub fn start() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("dg-l-")
            .tempdir_in("/private/tmp")
            .unwrap();
        let authority = TestAuthority::start(directory.path()).unwrap();
        authority
            .wait_until_admitting(Duration::from_secs(10))
            .unwrap();
        Self {
            authority,
            directory,
        }
    }

    pub fn owner(&self, instance: &str) -> Owner {
        Owner {
            endpoint: self.authority.socket(),
            uid: self.authority.uid(),
            credential: self.authority.consumer().unwrap(),
            generation: self.authority.generation(),
            instance: instance.into(),
        }
    }
}

pub fn compatibility() -> Compatibility {
    Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 1,
        required: BTreeSet::from([Capability::DurableAdmission, Capability::FencedLaunch]),
    }
}

/// A registered owner instance. Each call uses a fresh bounded session.
pub struct Owner {
    pub endpoint: PathBuf,
    pub uid: u32,
    pub credential: CallerCredential,
    pub generation: String,
    pub instance: String,
}

impl Owner {
    pub fn session(&self) -> Client {
        let mut client = Client::connect(&self.endpoint, self.uid, compatibility()).unwrap();
        client.authenticate(self.credential.clone()).unwrap();
        client.register(self.instance.clone()).unwrap();
        client
    }

    pub fn key(&self, attempt: &str) -> AttemptKey {
        AttemptKey {
            consumer_id: CONSUMER.into(),
            consumer_generation: self.generation.clone(),
            attempt_id: attempt.into(),
        }
    }

    /// Admit and commit a launch, returning the one-time permit.
    pub fn grant(&self, attempt: &str) -> (AttemptRecord, Secret) {
        let mut client = self.session();
        let admitted = client.admit(request(self.key(attempt))).unwrap();
        assert_eq!(admitted.phase, AttemptPhase::Prepared, "{admitted:?}");
        let grant = client.begin_launch(self.key(attempt)).unwrap();
        assert_eq!(grant.attempt.phase, AttemptPhase::LaunchCommitted);
        (
            grant.attempt,
            grant.permit.expect("the first commit carries a permit"),
        )
    }

    pub fn ticket(&self, attempt: &str) -> HelperTicket {
        HelperTicket {
            endpoint: self.endpoint.clone(),
            key: self.key(attempt),
            instance_id: self.instance.clone(),
        }
    }

    pub fn lookup(&self, attempt: &str) -> AttemptRecord {
        self.session().lookup(self.key(attempt)).unwrap()
    }

    pub fn cancel(&self, attempt: &str) -> AttemptRecord {
        self.session().cancel(self.key(attempt)).unwrap()
    }
}

pub fn request(key: AttemptKey) -> AdmissionRequest {
    AdmissionRequest {
        key,
        execution_digest: digest_bytes(b"launch fixture command"),
        intent: ResourceIntent {
            profile: "interactive".into(),
            requested: Budget {
                cpu_milli: 1_000,
                memory_bytes: 512 * MIB,
                tasks: 16,
            },
            minimum: ResourceLevels::MACOS,
        },
    }
}

pub struct Launched {
    pub child: Child,
    pub report: HelperReport,
}

/// Start the real helper for `program`, letting the test configure the
/// standard descriptors, directory and environment first.
pub fn launch(
    helper: &str,
    ticket: &HelperTicket,
    permit: &Secret,
    program: &str,
    args: &[&str],
    configure: impl FnOnce(&mut Command),
) -> Launched {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let (mut command, report) =
        helper_command(Path::new(helper), ticket, permit, Path::new(program), &args).unwrap();
    for arg in command.args() {
        assert!(!arg.to_string_lossy().contains(permit.expose()));
    }
    configure(command.command_mut());
    let child = command.spawn().unwrap();
    Launched { child, report }
}

/// Qualification runs collect raw receipts; ordinary runs write nothing.
/// Receipts can be shared as evidence, so none may hold a secret.
pub fn record(name: &str, value: serde_json::Value) {
    let text = serde_json::to_string_pretty(&value).unwrap();
    assert_no_secrets(name, &text);
    if let Some(directory) = std::env::var_os("DEVGUARD_EVIDENCE_DIR") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{name}.json")), text).unwrap();
    }
}

/// No value of a credential-like environment variable may appear in `text`.
/// Receipts record environment names, never values.
pub fn assert_no_secrets(name: &str, text: &str) {
    const MARKERS: [&str; 7] = [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "AUTH",
        "KEY",
        "CREDENTIAL",
        "COOKIE",
    ];
    for (variable, value) in std::env::vars() {
        let upper = variable.to_uppercase();
        if value.len() >= 8 && MARKERS.iter().any(|marker| upper.contains(marker)) {
            assert!(
                !text.contains(&value),
                "{name} would record the value of {variable}"
            );
        }
    }
}

/// A payload inventory without environment values.
pub fn inventory_summary(seen: &serde_json::Value) -> serde_json::Value {
    let mut summary = seen.clone();
    if let Some(object) = summary.as_object_mut() {
        if let Some(serde_json::Value::Array(pairs)) = object.remove("environment") {
            let names = pairs.iter().map(|pair| pair[0].clone()).collect();
            object.insert("environment_keys".into(), serde_json::Value::Array(names));
        }
    }
    summary
}

/// Descriptors this owner leaves inheritable, beside the standard three. The
/// helper passes them to the executable as part of the command's meaning.
pub fn expected_payload_fds() -> serde_json::Value {
    let fds: Vec<i32> = (0..4096)
        .filter(|fd| {
            // SAFETY: F_GETFD only queries the descriptor's flags.
            let flags = unsafe { libc::fcntl(*fd, libc::F_GETFD) };
            *fd <= 2 || (flags >= 0 && flags & libc::FD_CLOEXEC == 0)
        })
        .collect();
    serde_json::json!(fds)
}

pub const PROBE_OUT: &str = "DEVGUARD_PROBE_OUT";
pub const PROBE_EXIT: &str = "DEVGUARD_PROBE_EXIT";

/// Arguments that make this test binary run only its payload probe.
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

/// The payload side: record what the executable inherited, then exit with
/// the requested status. Without the probe environment this does nothing.
pub fn payload_probe() {
    let Some(out) = std::env::var_os(PROBE_OUT) else {
        return;
    };
    let fds: Vec<i32> = (0..4096)
        // SAFETY: F_GETFD only queries whether the descriptor is open.
        .filter(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0)
        .collect();
    // SAFETY: these calls only read this process's own identity.
    let (pid, ppid, pgid, sid) = unsafe {
        (
            libc::getpid(),
            libc::getppid(),
            libc::getpgrp(),
            libc::getsid(0),
        )
    };
    // SAFETY: getpriority reads this process's nice value.
    let nice = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
    let environment: Vec<(String, String)> = std::env::vars_os()
        .map(|(k, v)| (k.to_string_lossy().into(), v.to_string_lossy().into()))
        .collect();
    // SAFETY: isatty only queries descriptor 0.
    let terminal = unsafe { libc::isatty(0) } == 1;
    let inventory = serde_json::json!({
        "pid": pid, "ppid": ppid, "pgid": pgid, "sid": sid, "nice": nice,
        "fds": fds, "terminal": terminal,
        "argv": std::env::args().collect::<Vec<_>>(),
        "cwd": std::env::current_dir().unwrap(),
        "environment": environment,
    });
    std::fs::write(out, serde_json::to_vec(&inventory).unwrap()).unwrap();
    let code = std::env::var(PROBE_EXIT)
        .ok()
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    std::process::exit(code);
}

pub const RAW_HELPER: &str = "DEVGUARD_RAW_HELPER";

/// A bare helper that leads its own group and presents one grant twice
/// without ever executing anything, to observe what a helper whose first reply
/// was lost is told on replay. It first records its own scheduler readback.
pub fn raw_helper_probe() {
    use devguard_client::credential::take_inherited;
    let Some(config) = std::env::var_os(RAW_HELPER) else {
        return;
    };
    let config: serde_json::Value = serde_json::from_str(&config.to_string_lossy()).unwrap();
    let out = PathBuf::from(config["out"].as_str().unwrap());
    devguard_macos::become_scope_root().unwrap();
    let readback = devguard_macos::NativeHost::open()
        .unwrap()
        .backend()
        .scheduler_readback(std::process::id())
        .ok();
    let mut evidence = serde_json::json!({"readback": readback, "results": []});
    std::fs::write(&out, evidence.to_string()).unwrap();
    // SAFETY: the owner passed this descriptor to this process for the grant only.
    let permit = unsafe { take_inherited(config["permit_fd"].as_i64().unwrap() as i32) }.unwrap();
    let endpoint = PathBuf::from(config["endpoint"].as_str().unwrap());
    let key: AttemptKey = serde_json::from_value(config["key"].clone()).unwrap();
    let instance = config["instance"].as_str().unwrap().to_string();
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    for _ in 0..2 {
        let result = Client::connect(&endpoint, uid, compatibility())
            .and_then(|mut client| client.launch(key.clone(), instance.clone(), permit.clone()));
        let result = match result {
            Ok(authorization) => serde_json::json!({"may_exec": authorization.may_exec,
                                              "phase": authorization.attempt.phase}),
            Err(error) => serde_json::json!({"error": error}),
        };
        evidence["results"].as_array_mut().unwrap().push(result);
        std::fs::write(&out, evidence.to_string()).unwrap();
    }
    std::process::exit(0);
}

/// Start the bare helper as a direct child of this owner, optionally under
/// the utility QoS clamp applied by `taskpolicy`, which execs in place.
pub fn start_raw_helper(
    owner: &Owner,
    attempt: &str,
    permit: &Secret,
    out: &Path,
    clamped: bool,
    test: &str,
) -> Child {
    use devguard_client::credential::CredentialHandoff;
    let exe = std::env::current_exe().unwrap();
    let mut command = if clamped {
        let mut command = Command::new("/usr/sbin/taskpolicy");
        command.args(["-c", "utility"]).arg(&exe);
        command
    } else {
        Command::new(&exe)
    };
    command.args(probe_args(test));
    let permit_fd = CredentialHandoff::new(permit).unwrap().attach(&mut command);
    command
        .env(
            RAW_HELPER,
            serde_json::json!({
                "out": out, "permit_fd": permit_fd, "endpoint": owner.endpoint,
                "key": owner.key(attempt), "instance": owner.instance,
            })
            .to_string(),
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = command.spawn().unwrap();
    drop(command);
    child
}

/// Poll the owner's attempt until `done` holds or `limit` passes.
pub fn wait_for(
    owner: &Owner,
    attempt: &str,
    limit: Duration,
    done: impl Fn(&AttemptRecord) -> bool,
) -> AttemptRecord {
    let deadline = std::time::Instant::now() + limit;
    loop {
        let record = owner.lookup(attempt);
        if done(&record) {
            return record;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "attempt {attempt} never reached the expected state: {record:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until the scope's release is recorded by the reconciler.
pub fn wait_released(owner: &Owner, attempt: &str) -> AttemptRecord {
    wait_for(owner, attempt, Duration::from_secs(10), |record| {
        record.phase == AttemptPhase::Released
    })
}

/// Wait for a child to exit without reaping it, so its PID stays held.
pub fn exited_unreaped(pid: u32) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        // SAFETY: siginfo_t is plain data filled by waitid; WNOWAIT leaves the
        // child waitable, so its owner still reaps it later.
        let exited = unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let result = libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
            );
            assert!(
                result == 0
                    || std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted,
                "waitid failed for {pid}"
            );
            result == 0 && info.si_pid == pid as libc::pid_t
        };
        if exited {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "child {pid} did not exit"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Reap a child within `limit`, killing it first if it is still running.
pub fn wait_exit(child: &mut Child, limit: Duration) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child {} did not exit within {limit:?}", child.id());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Kills and reaps a long-lived child if a test fails before it does.
pub struct KillOnDrop(pub Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// The boot-relative clock the authority uses (`CLOCK_MONOTONIC_RAW`), in ms.
pub fn boot_ms() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a valid output structure for clock_gettime.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut time) },
        0
    );
    time.tv_sec as u64 * 1_000 + time.tv_nsec as u64 / 1_000_000
}
