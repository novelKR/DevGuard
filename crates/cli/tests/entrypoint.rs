//! The shipped `devguard` binary: its usage, that malformed invocations
//! start nothing, and that only a terminal sees its lines in color. None of
//! these contact an authority.

use std::fs::File;
use std::io::Read;
use std::os::fd::{FromRawFd, OwnedFd};
use std::process::{Command, Stdio};

fn devguard(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_devguard"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_and_version_describe_the_managed_contract() {
    let help = devguard(&["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    for needle in [
        "devguard exec",
        "devguard doctor",
        "devguard test-candidate",
        "devguard upgrade",
        "devguard repair",
        "devguard admission --open",
        "never run unmanaged",
        "no path or authority override",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    let version = devguard(&["--version"]);
    assert!(version.status.success());
    assert!(String::from_utf8(version.stdout)
        .unwrap()
        .starts_with("devguard "));
}

#[test]
fn malformed_invocations_start_nothing_and_exit_125() {
    for args in [
        &[][..],
        &["exec"],
        &["exec", "true"],
        &["exec", "--"],
        &["exec", "--socket", "/tmp/x", "--", "true"],
        &["exec", "--wait", "forever", "--", "true"],
        &["doctor", "--require", "kernel"],
        &["serve"],
        &["exec", "--lease", "dev-cli/g/lease-1", "--", "true"],
        &[
            "exec",
            "--lease",
            "dev-cli/g/lease-1",
            "--lease-token-fd",
            "200",
            "--",
            "true",
        ],
        &["test-candidate"],
        &["upgrade"],
        &["upgrade", "--release", "../escape"],
        &["repair"],
        &["repair", "--use", "current"],
        &["admission"],
        &["admission", "--close"],
        &[
            "test-candidate",
            "--candidate",
            "/nonexistent",
            "--report",
            "/nonexistent/r",
        ],
    ] {
        let output = devguard(args);
        assert_eq!(output.status.code(), Some(125), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.starts_with("devguard: "), "{args:?}: {stderr}");
        assert!(!stderr.contains('\u{1b}'), "{args:?}: a pipe gets no color");
    }
}

/// What `devguard ARGS` writes to its standard error when that is a new pseudo-terminal, with
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
    let mut command = Command::new(env!("CARGO_BIN_EXE_devguard"));
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
fn a_refusal_is_red_on_a_terminal_and_plain_without_color() {
    let colored = on_terminal(&["exec"], "xterm-256color", None);
    let (line, usage) = colored.split_once("\n\n").unwrap();
    assert!(line.starts_with("\u{1b}[1;31mdevguard: "), "{colored}");
    assert!(line.ends_with("\u{1b}[0m"), "{colored}");
    assert!(usage.starts_with("devguard exec ") && !usage.contains('\u{1b}'));
    let piped = String::from_utf8(devguard(&["exec"]).stderr).unwrap();
    for plain in [
        on_terminal(&["exec"], "xterm-256color", Some("1")),
        on_terminal(&["exec"], "xterm-256color", Some("")),
        on_terminal(&["exec"], "dumb", None),
    ] {
        assert_eq!(plain, piped);
    }
}
