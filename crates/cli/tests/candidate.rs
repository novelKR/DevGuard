#![cfg(target_os = "macos")]
//! DG1-C10: parent leases from the owner's side. Real `devguard exec --lease`
//! processes run real workloads as lease children through the real
//! `devguard-launch`, and `test-candidate` runs a real candidate authority
//! process as one of them, against isolated fixture parents.

mod support;
use devguard_cli::candidate::{self, DaemonWorkload, Environment, Plan, Workload};
use devguard_cli::Exit;
use devguard_client::credential::CredentialHandoff;
use devguard_client::Client;
use devguard_contract::{AttemptKey, Budget, Capability, Compatibility, LeasePhase, Secret};
use devguard_daemon::candidate::{budget_text, key_text};
use devguard_daemon::paths::AuthorityPaths;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::*;

#[test]
#[ignore = "the CLI process of the candidate tests"]
fn cli_child() {
    support::cli_child();
}

#[test]
#[ignore = "the workload of the candidate tests"]
fn payload() {
    support::payload();
}

#[test]
#[ignore = "the candidate authority of the candidate tests"]
fn candidate_child() {
    support::candidate_child();
}

/// Fits a 3-CPU host's 500 mCPU of work capacity.
const LEASE: Budget = Budget {
    cpu_milli: 450,
    memory_bytes: 512 * MIB,
    tasks: 96,
};
/// Leaves 150 mCPU, 128 MiB and 16 tasks after the candidate's own daemon.
const CAPACITY: Budget = Budget {
    cpu_milli: 400,
    memory_bytes: 256 * MIB,
    tasks: 64,
};
const SMALL: [&str; 6] = ["--cpu", "50", "--memory", "16MiB", "--tasks", "2"];

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn payload_workload(name: &str, cwd: &Path, probe: Value) -> Workload {
    Workload {
        name: name.into(),
        cwd: cwd.into(),
        options: strings(&SMALL),
        program: exe(),
        args: probe_args("payload"),
        env: vec![(PROBE.into(), probe.to_string())],
    }
}

/// The candidate authority as a lease child: this harness in its role.
fn daemon_workload(base: PathBuf, cwd: PathBuf) -> DaemonWorkload {
    Box::new(move |spec, fd| Workload {
        name: "candidate".into(),
        cwd: cwd.clone(),
        options: strings(&[
            "--cpu",
            &CAPACITY.cpu_milli.to_string(),
            "--memory",
            &CAPACITY.memory_bytes.to_string(),
            "--tasks",
            &CAPACITY.tasks.to_string(),
        ]),
        program: exe(),
        args: probe_args("candidate_child"),
        env: vec![(
            CANDIDATE.into(),
            json!({"base": base, "id": spec.id, "lease": key_text(&spec.lease),
                   "capacity": budget_text(spec.capacity), "token_fd": fd})
            .to_string(),
        )],
    })
}

fn plan(work: &Path, children: Vec<Workload>, daemon: DaemonWorkload) -> Plan {
    Plan {
        id: "c-1".into(),
        lease: LEASE,
        ttl: Duration::from_secs(600),
        wait: Duration::ZERO,
        capacity: CAPACITY,
        children,
        daemon,
        report: work.join("report"),
        tree: None,
    }
}

/// Run `plan` in this process, as the lease owner, against the fixture.
fn run_plan(fixture: &Fixture, plan: &Plan) -> (Exit, Value) {
    let paths = AuthorityPaths::fixture(fixture.base());
    let base = fixture.base().to_path_buf();
    let cli = move |args: &[String]| cli_command(&base, args);
    let exit = candidate::run(
        plan,
        &Environment {
            paths: &paths,
            cli: &cli,
        },
    );
    let report = read_json(&plan.report.join("report.json"));
    (exit, report)
}

/// Every file a run left in `directory`, as text.
fn texts(directory: &Path) -> Vec<(PathBuf, String)> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect()
}

#[test]
fn a_candidate_and_its_workloads_run_as_children_of_one_lease_that_is_then_released() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let out = work.join("payload.json");
    let plan = plan(
        &work,
        vec![payload_workload("workload", &work, json!({"out": out}))],
        daemon_workload(fixture.base().to_path_buf(), work.clone()),
    );
    let (exit, report) = run_plan(&fixture, &plan);
    assert_eq!(exit, Exit::Code(0), "{report:#}");
    assert_eq!(report["verdict"], "passed");
    assert_eq!(report["failures"], json!([]));
    // The workload ran through the stable helper and inherited no token.
    let seen = read_json(&out);
    assert_eq!(seen["descriptors"], json!([0, 1, 2]));
    assert_eq!(seen["pgid"], seen["pid"]);
    let children = report["children"].as_array().unwrap();
    let names: Vec<&str> = children
        .iter()
        .map(|child| child["workload"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["workload", "candidate"]);
    for child in children {
        assert_eq!(child["passed"], true, "{child:#}");
        assert_eq!(child["within_lease"], true);
        assert_eq!(child["result"]["lease"], report["lease"]["key"]);
        assert_eq!(child["attempt"]["phase"], "released");
        assert_eq!(child["attempt"]["release_reason"], "scope_terminated");
    }
    assert_eq!(
        children[1]["reserved"],
        serde_json::to_value(CAPACITY).unwrap()
    );
    for check in [
        "capabilities",
        "status",
        "admission_within_capacity",
        "launch_refused",
        "cancelled",
        "admission_beyond_capacity",
        "parent_lease_refused",
    ] {
        assert_eq!(report["smoke"][check]["ok"], true, "{check}");
    }
    assert_eq!(report["lease"]["token_issued"], true);
    assert_eq!(report["lease"]["ended"]["lease"]["end_reason"], "ended");
    assert_eq!(report["lease"]["released"]["lease"]["phase"], "released");
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    // The candidate's disposable area is gone, and the parent's state is its own.
    let paths = AuthorityPaths::fixture(fixture.base());
    assert!(!paths.candidates().join("c-1").exists());
    assert!(!paths.candidate_runtime().join("c-1").exists());
    assert!(paths.journal().is_file());
    for (path, text) in texts(&plan.report) {
        assert!(!text.contains(&fixture.secret()), "{path:?}");
    }
    let log = std::fs::read_to_string(plan.report.join("candidate.log")).unwrap();
    assert!(log.contains("\"event\":\"candidate_opened\""));
    assert!(log.contains("\"closure\":\"lease_ended\""));
    record("test-candidate", report, &fixture.secret());
}

fn owner(fixture: &Fixture) -> Client {
    let mut client = Client::connect(
        &fixture.authority.socket(),
        fixture.authority.uid(),
        Compatibility {
            minimum_protocol: 1,
            maximum_protocol: 1,
            required: [Capability::DurableAdmission, Capability::ParentLease]
                .into_iter()
                .collect(),
        },
    )
    .unwrap();
    client
        .authenticate(fixture.authority.consumer().unwrap())
        .unwrap();
    client.register("lease-owner".into()).unwrap();
    client
}

fn holder(fixture: &Fixture) -> Client {
    Client::connect(
        &fixture.authority.socket(),
        fixture.authority.uid(),
        Compatibility {
            minimum_protocol: 1,
            maximum_protocol: 1,
            required: [Capability::ParentLease].into_iter().collect(),
        },
    )
    .unwrap()
}

#[test]
fn lease_children_are_admitted_only_within_an_active_lease_with_its_token() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let lease = AttemptKey {
        consumer_id: "dev-cli".into(),
        consumer_generation: fixture.authority.generation(),
        attempt_id: "lease-1".into(),
    };
    let budget = Budget {
        cpu_milli: 100,
        memory_bytes: 64 * MIB,
        tasks: 8,
    };
    let token = owner(&fixture)
        .admit_lease(lease.clone(), budget, None)
        .unwrap()
        .token
        .unwrap();
    let run = |name: &str, token: &Secret, extra: &[&str], probe: Value| -> (i32, Value) {
        let handoff = CredentialHandoff::new(token).unwrap();
        let receipt = work.join(format!("{name}.json"));
        let mut options = strings(&[
            "--lease",
            &key_text(&lease),
            "--lease-token-fd",
            &handoff.descriptor().to_string(),
            "--receipt",
            receipt.to_str().unwrap(),
        ]);
        options.extend(strings(extra));
        let options: Vec<&str> = options.iter().map(String::as_str).collect();
        let mut child = cli(&fixture, &exec_payload(&options), |command| {
            quiet(command);
            command.env(PROBE, probe.to_string());
            handoff.attach(command);
        });
        let status = wait_exit(&mut child, EXIT_LIMIT);
        (status.code().unwrap(), read_json(&receipt))
    };
    let out = work.join("within.out");
    let (code, within) = run("within", &token, &SMALL, json!({"out": out}));
    assert_eq!(code, 0, "{within:#}");
    assert_eq!(within["lease"], serde_json::to_value(&lease).unwrap());
    assert_eq!(within["observed_after_reap"]["phase"], "released");
    // The token's descriptor was the CLI's alone.
    assert_eq!(read_json(&out)["descriptors"], json!([0, 1, 2]));
    // A child larger than the lease is denied, and no wait can admit it.
    let larger = ["--cpu", "150", "--memory", "16MiB", "--tasks", "2"];
    let (code, beyond) = run("beyond", &token, &larger, json!({}));
    assert_eq!(code, 125);
    assert_eq!(
        beyond["attempts"][0]["record"]["denial"],
        "resource_unavailable"
    );
    let mut waiting = vec!["--wait", "5s"];
    waiting.extend(larger);
    let (code, waited) = run("beyond-wait", &token, &waiting, json!({}));
    assert_eq!(code, 125);
    assert_eq!(waited["wait"]["admissions"], 0);
    assert_eq!(
        waited["wait"]["lease_budget"],
        serde_json::to_value(budget).unwrap()
    );
    assert!(waited["reason"].as_str().unwrap().contains("parent lease"));
    // A forged token admits nothing.
    let forged = Secret::new("e".repeat(64)).unwrap();
    let (code, refused) = run("forged", &forged, &SMALL, json!({}));
    assert_eq!(code, 125);
    assert!(
        refused["reason"].as_str().unwrap().contains("Unauthorized"),
        "{refused:#}"
    );
    // Once the lease has ended, a child that starts late is refused.
    let ended = owner(&fixture).end_lease(lease.clone()).unwrap();
    assert_ne!(ended.lease.phase, LeasePhase::Active);
    let (code, late) = run("late", &token, &SMALL, json!({}));
    assert_eq!(code, 125);
    assert_eq!(
        late["attempts"][0]["record"]["denial"],
        "invalid_transition"
    );
    for (path, text) in texts(&work) {
        assert!(!text.contains(token.expose()), "{path:?}");
        assert!(!text.contains(&fixture.secret()), "{path:?}");
    }
    // Every child is settled, so the lease is released and returns its budget.
    wait_for(Duration::from_secs(10), || {
        holder(&fixture)
            .lease_status(lease.clone(), token.clone())
            .unwrap()
            .lease
            .phase
            == LeasePhase::Released
    });
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record(
        "lease-children",
        json!({"within": within, "beyond": beyond, "beyond_wait": waited,
               "forged": refused, "late": late}),
        &fixture.secret(),
    );
}

#[test]
fn a_candidate_that_dies_fails_the_run_and_its_lease_is_still_released() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let crashing: DaemonWorkload = {
        let work = work.clone();
        Box::new(move |_, _| Workload {
            name: "candidate".into(),
            ..payload_workload("candidate", &work, json!({"signal": libc::SIGKILL}))
        })
    };
    let plan = plan(&work, Vec::new(), crashing);
    let (exit, report) = run_plan(&fixture, &plan);
    assert_eq!(exit, Exit::Code(1));
    assert_eq!(report["verdict"], "failed");
    let failures = report["failures"].to_string();
    assert!(failures.contains("never served its endpoint"), "{failures}");
    let candidate = &report["children"][0];
    assert_eq!(candidate["passed"], false);
    assert_eq!(
        candidate["result"]["exit"],
        json!({"signal": libc::SIGKILL})
    );
    // The parent observed the dead candidate's scope and released it.
    assert_eq!(candidate["attempt"]["phase"], "released");
    assert_eq!(report["lease"]["released"]["lease"]["phase"], "released");
    assert_eq!(fixture.authority.committed().unwrap(), Budget::ZERO);
    record("test-candidate-crash", report, &fixture.secret());
}
