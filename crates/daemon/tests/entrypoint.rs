use std::process::Command;

#[test]
fn alternate_authority_and_unparented_candidate_arguments_are_rejected() {
    for arguments in [
        vec!["check", "--state", "/tmp/second-authority"],
        vec!["check", "--socket", "/tmp/second.sock"],
        vec!["init", "--test-capacity", "8000"],
        vec!["candidate"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_devguardd"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8(result.stderr)
            .unwrap()
            .contains("no alternate authority arguments"));
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
