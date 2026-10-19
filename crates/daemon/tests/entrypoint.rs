use std::fs::File;
use std::io::Read;
use std::os::fd::{FromRawFd, OwnedFd};
use std::process::{Command, Stdio};

#[test]
fn alternate_authority_and_unparented_candidate_arguments_are_rejected() {
    for arguments in [
        vec!["check", "--state", "/tmp/second-authority"],
        vec!["check", "--socket", "/tmp/second.sock"],
        vec!["init", "--test-capacity", "8000"],
        vec!["candidate"],
        vec!["stage"],
        vec!["stage", "--package"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_devguardd"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(!result.status.success());
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(stderr.contains("no alternate authority arguments"));
        assert!(!stderr.contains('\u{1b}'), "a pipe gets no color");
    }
}

#[test]
fn a_candidate_needs_its_lease_token_descriptor_and_names_no_other_authority() {
    let run = |extra: &[&str]| {
        let mut arguments = vec![
            "candidate",
            "--id",
            "c-entrypoint",
            "--lease",
            "dev-cli/g/lease-1",
            "--capacity",
            "1000,1073741824,64",
            // Far above anything a test harness holds open.
            "--token-fd",
            "200",
        ];
        arguments.extend_from_slice(extra);
        Command::new(env!("CARGO_BIN_EXE_devguardd"))
            .args(arguments)
            .output()
            .unwrap()
    };
    let named = run(&["--socket", "/tmp/second.sock"]);
    assert!(!named.status.success());
    assert!(String::from_utf8(named.stderr)
        .unwrap()
        .contains("no alternate authority arguments"));
    // Without the inherited token nothing is contacted or created.
    let untokened = run(&[]);
    assert!(!untokened.status.success());
    if cfg!(target_os = "macos") {
        assert!(String::from_utf8(untokened.stderr)
            .unwrap()
            .contains("private credential FD"));
    }
}

#[test]
fn help_states_what_serve_opens_and_that_bootstrap_starts_no_workload() {
    let result = Command::new(env!("CARGO_BIN_EXE_devguardd"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(result.status.success());
    let help = String::from_utf8(result.stdout).unwrap();
    assert!(help.contains("with it, opens registration, fenced launch"));
    assert!(help.contains("install copies a package into a protected release"));
    assert!(help.contains("init and check never start workloads."));
    assert!(help.contains("candidate serves an isolated candidate authority"));
}

#[cfg(target_os = "macos")]
#[test]
fn normal_paths_ignore_home_and_xdg_overrides() {
    let run = |override_home: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_devguardd"));
        command.arg("paths");
        if override_home {
            command
                .env("HOME", "/tmp/alternate-home")
                .env("XDG_STATE_HOME", "/tmp/alternate-state");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let observed = run(false);
    assert_eq!(observed, run(true));
    assert!(!observed["home"]
        .as_str()
        .unwrap()
        .starts_with("/tmp/alternate"));
}

/// What `devguardd ARGS` writes to its standard error when that is a new pseudo-terminal, with
/// `TERM` set to `term` and `NO_COLOR` set only when `no_color` gives a value.
fn on_terminal(args: &[&str], term: &str, no_color: Option<&str>) -> String {
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: openpty fills two new descriptors that are owned below.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(result, 0, "{}", std::io::Error::last_os_error());
    // SAFETY: the descriptors are new and owned here; keep them out of other children.
    let (mut master, slave) = unsafe {
        libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave, libc::F_SETFD, libc::FD_CLOEXEC);
        (File::from_raw_fd(master), OwnedFd::from_raw_fd(slave))
    };
    let mut command = Command::new(env!("CARGO_BIN_EXE_devguardd"));
    command
        .args(args)
        .env("TERM", term)
        .env_remove("NO_COLOR")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(slave));
    if let Some(value) = no_color {
        command.env("NO_COLOR", value);
    }
    let mut child = command.spawn().unwrap();
    // The terminal ends for this reader only once no process here holds its other side.
    drop(command);
    let mut written = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        match master.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => written.extend_from_slice(&buffer[..count]),
            // Linux reports a pseudo-terminal whose other side closed as EIO.
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) => panic!("{error}"),
        }
    }
    assert!(!child.wait().unwrap().success());
    String::from_utf8(written).unwrap().replace("\r\n", "\n")
}

#[test]
fn an_error_is_red_on_a_terminal_and_plain_without_color() {
    let colored = on_terminal(&["check", "--state", "/tmp/x"], "xterm-256color", None);
    assert!(colored.starts_with("\u{1b}[1;31m"), "{colored}");
    assert!(colored.ends_with("\u{1b}[0m\n"), "{colored}");
    assert!(colored.contains("no alternate authority arguments"));
    let piped = Command::new(env!("CARGO_BIN_EXE_devguardd"))
        .args(["check", "--state", "/tmp/x"])
        .output()
        .unwrap();
    let piped = String::from_utf8(piped.stderr).unwrap();
    for plain in [
        on_terminal(&["check", "--state", "/tmp/x"], "xterm-256color", Some("1")),
        on_terminal(&["check", "--state", "/tmp/x"], "dumb", None),
    ] {
        assert_eq!(plain, piped);
    }
}
