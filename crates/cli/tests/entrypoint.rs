//! The shipped `devguard` binary: its usage, and that malformed invocations
//! start nothing. None of these contact an authority.

use std::process::Command;

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
    ] {
        let output = devguard(args);
        assert_eq!(output.status.code(), Some(125), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.starts_with("devguard: "), "{args:?}: {stderr}");
    }
}
