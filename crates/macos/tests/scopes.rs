#![cfg(target_os = "macos")]
//! Native cooperative policy application, scope observation and termination
//! with real processes (DG1-C04). The scope root stands in for the launch
//! helper: it leads its own process group and re-executes itself under the
//! utility QoS clamp, then spawns descendants on command.

mod support;
use devguard_contract::*;
use devguard_core::*;
use devguard_macos::{become_scope_root, exec_with_workload_qos, Sampler, SamplerOutcome};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use support::*;

const MODE: &str = "DEVGUARD_SCOPE_ROOT_MODE";
const CHILD_TEST: &str = "scope_root_child";

/// Entry point of the re-executed scope root, driven over stdin.
#[test]
#[ignore = "invoked as a scope root by its parent integration tests"]
fn scope_root_child() {
    let Ok(mode) = std::env::var(MODE) else {
        return;
    };
    let arguments: Vec<std::ffi::OsString> = ["--exact", CHILD_TEST, "--ignored", "--nocapture"]
        .iter()
        .map(Into::into)
        .collect();
    match mode.as_str() {
        "qos-root" => {
            become_scope_root().unwrap();
            // SAFETY: the process is about to replace its image; no other code
            // reads the environment concurrently in this child.
            unsafe { std::env::set_var(MODE, "run") };
            let error = exec_with_workload_qos(&std::env::current_exe().unwrap(), &arguments);
            panic!("re-exec under the workload QoS clamp failed: {error}");
        }
        "plain-root" => {
            become_scope_root().unwrap();
            serve_commands();
        }
        "shared-root" | "run" => serve_commands(),
        other => panic!("unknown scope root mode {other}"),
    }
}

fn serve_commands() {
    let mut out = std::io::stdout();
    writeln!(out, "ready {}", std::process::id()).unwrap();
    out.flush().unwrap();
    let mut descendants: Vec<Child> = Vec::new();
    for line in std::io::stdin().lines() {
        let line = line.unwrap();
        let mut sleeper = Command::new("/bin/sleep");
        sleeper.arg("30").stdin(Stdio::null()).stdout(Stdio::null());
        match line.as_str() {
            "spawn" => {
                let child = sleeper.spawn().unwrap();
                writeln!(out, "spawned {}", child.id()).unwrap();
                descendants.push(child);
            }
            "escape" => {
                let child = sleeper.process_group(0).spawn().unwrap();
                writeln!(out, "escaped {}", child.id()).unwrap();
                descendants.push(child);
            }
            // Exit without reaping descendants; they are reparented and keep
            // running in the scope's process group.
            "exit" => std::process::exit(0),
            other => panic!("unknown command {other}"),
        }
        out.flush().unwrap();
    }
    std::process::exit(0);
}

struct Root {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    pid: u32,
    /// Descendants with their start identity, recorded when reported.
    spawned: Vec<ProcessIdentity>,
}

impl Root {
    fn start(mode: &str) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
            .env(MODE, mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        let pid = child.id();
        // Constructed with the real PID first so cleanup never targets PID 0.
        let root = Self {
            child: Some(child),
            stdin,
            lines,
            pid,
            spawned: Vec::new(),
        };
        assert_eq!(root.expect("ready "), pid);
        root
    }

    /// Wait for a reply marker and return its PID; harness chatter is skipped.
    fn expect(&self, prefix: &str) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Ok(line) = self.lines.recv_timeout(remaining) else {
                panic!("scope root never replied {prefix:?}; output: {seen:?}");
            };
            // libtest may print its own status on the same line first.
            if let Some((_, rest)) = line.split_once(prefix) {
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                return digits.parse().unwrap();
            }
            seen.push(line);
        }
    }

    fn command(&mut self, command: &str, reply: &str) -> u32 {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{command}").unwrap();
        stdin.flush().unwrap();
        let pid = self.expect(reply);
        let host = devguard_macos::NativeHost::open().unwrap();
        let identity = host.backend().process_identity(pid).unwrap().unwrap();
        self.spawned.push(identity);
        pid
    }

    fn exit_unreaped(&mut self) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "exit").unwrap();
        stdin.flush().unwrap();
    }

    fn reap(&mut self) {
        if let Some(mut child) = self.child.take() {
            child.wait().unwrap();
        }
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        // Test-owned processes only. The root is signalled only while it is
        // still our unreaped child; a descendant only while its PID still names
        // the process recorded when it was reported.
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(host) = devguard_macos::NativeHost::open() {
            let backend = host.backend();
            for descendant in &self.spawned {
                let same = backend
                    .process_identity(descendant.pid)
                    .is_ok_and(|now| now.as_ref() == Some(descendant));
                if same && descendant.pid > 0 {
                    // SAFETY: one positive PID whose start identity was just verified.
                    unsafe { libc::kill(descendant.pid as i32, libc::SIGKILL) };
                }
            }
        }
    }
}

struct Launch {
    fixture: Fixture,
    authority: NativeAuthority,
    principal: Principal,
    record: AttemptRecord,
    permit: Secret,
}

fn prepared_launch(id: &str) -> Launch {
    let fixture = Fixture::new();
    let clock = fixture.host.clock();
    let mut authority = fixture.open();
    let principal = fixture.register_self(&mut authority);
    let mut sampler = Sampler::new(Healthy { paged_out_bytes: 0 });
    sampler.sample(&clock, 0);
    std::thread::sleep(Duration::from_millis(50));
    let SamplerOutcome::Sample { sample, .. } = sampler.sample(&clock, 0) else {
        panic!("expected a sample");
    };
    authority.observe_pressure(sample).unwrap();
    let record = authority.admit(&principal, request(id)).unwrap();
    assert_eq!(record.phase, AttemptPhase::Prepared);
    let permit = authority
        .begin_launch(&principal, &record.key)
        .unwrap()
        .permit
        .unwrap();
    Launch {
        fixture,
        authority,
        principal,
        record,
        permit,
    }
}

impl Launch {
    fn establish(&self, root: &Root) -> devguard_macos::Establishment {
        let backend = self.fixture.host.backend();
        let identity = backend.process_identity(root.pid).unwrap().unwrap();
        backend
            .establish_scope(
                &self.record.key,
                self.principal.instance(),
                &identity,
                self.record.plan.as_ref().unwrap(),
                self.record.reservation.as_ref().unwrap().quantities,
            )
            .unwrap()
    }

    fn authorize(&mut self, root: &Root) -> ScopeIdentity {
        let established = self.establish(root);
        let bound = self
            .authority
            .bind_scope(
                &self.principal,
                &self.record.key,
                &self.permit,
                &established.scope,
            )
            .unwrap();
        assert_eq!(bound.phase, AttemptPhase::ScopeBound);
        let run = self
            .authority
            .authorize_run(
                &self.principal,
                &self.record.key,
                &self.permit,
                &established.scope.root,
            )
            .unwrap();
        assert!(run.may_exec);
        established.scope
    }

    fn reconcile(&mut self) -> AttemptRecord {
        self.authority.reconcile(&self.record.key).unwrap()
    }
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_bound_scope_is_released_only_after_every_member_exits() {
    let mut launch = prepared_launch("lifecycle");
    let backend = launch.fixture.host.backend();
    let mut root = Root::start("qos-root");
    let established = launch.establish(&root);
    let cpu = established.cpu.unwrap();
    assert!(cpu.cooperative(), "{cpu:?}");
    assert!(cpu.nice >= devguard_macos::WORKLOAD_NICE);
    assert!(established.applied.confirms(
        launch.record.plan.as_ref().unwrap(),
        launch.record.reservation.as_ref().unwrap().quantities
    ));
    let scope = launch.authorize(&root);
    assert_eq!(scope, established.scope);

    let first = root.command("spawn", "spawned ");
    let second = root.command("spawn", "spawned ");
    let mut timeline = Vec::new();
    let running = backend.observe_scope(&scope).unwrap();
    assert!(!running.empty && !running.root_reaped && running.tracking_complete);
    assert_eq!(launch.reconcile().phase, AttemptPhase::RunAuthorized);
    timeline.push(json!({"step": "running", "observation": format!("{running:?}")}));

    // The root exits but is not reaped: it is not gone.
    root.exit_unreaped();
    wait_until("root exit", || {
        backend.process_identity(root.pid).unwrap().is_none()
    });
    let zombie = backend.observe_scope(&scope).unwrap();
    assert!(!zombie.root_reaped && !zombie.empty);
    assert_eq!(launch.reconcile().phase, AttemptPhase::RunAuthorized);
    timeline.push(json!({"step": "root-zombie", "observation": format!("{zombie:?}")}));

    // Root reaped while descendants survive: still no release.
    root.reap();
    let orphans = backend.observe_scope(&scope).unwrap();
    assert!(orphans.root_reaped && !orphans.empty && !orphans.known_members_gone);
    assert_eq!(launch.reconcile().phase, AttemptPhase::RunAuthorized);
    timeline.push(
        json!({"step": "root-reaped-descendants-running", "observation": format!("{orphans:?}")}),
    );

    let receipt = backend.signal_scope(&scope, libc::SIGTERM).unwrap();
    let signalled: Vec<u32> = receipt.signalled.iter().map(|(pid, _)| *pid).collect();
    assert!(signalled.contains(&first) && signalled.contains(&second));
    assert!(!signalled.contains(&root.pid));
    // Reparented descendants are reaped by launchd; wait for the group to empty.
    wait_until("scope exit", || {
        let observation = backend.observe_scope(&scope).unwrap();
        observation.empty && observation.known_members_gone
    });
    let released = launch.reconcile();
    assert_eq!(released.phase, AttemptPhase::Released);
    assert_eq!(
        released.release_reason,
        Some(ReleaseReason::ScopeTerminated)
    );
    assert!(!released.known_not_started());
    assert_eq!(launch.authority.committed_budget().unwrap(), Budget::ZERO);
    timeline.push(json!({"step": "released", "record_phase": released.phase}));
    let plan = launch.record.plan.clone().unwrap();
    record(
        "scope-lifecycle",
        json!({"establishment": established, "timeline": timeline,
               "termination": receipt, "descendants": [first, second]}),
    );
    record(
        "capability-matrix",
        json!({"requested_minimum": ResourceLevels::MACOS, "plan": plan,
               "applied": {"cpu": established.applied.cpu, "memory": established.applied.memory,
                           "pids": established.applied.pids},
               "cpu_readback": cpu,
               "meaning": {"cpu": "utility QoS clamp and nice +10, cooperative scheduling only",
                           "memory": "accounted by the authority, no kernel limit",
                           "pids": "accounted by the authority, no kernel limit"}}),
    );
}

#[test]
fn an_escaped_member_keeps_the_attempt_suspect_after_everything_exits() {
    let mut launch = prepared_launch("escape");
    let backend = launch.fixture.host.backend();
    let mut root = Root::start("qos-root");
    let scope = launch.authorize(&root);
    let escaped = root.command("escape", "escaped ");
    let observed = backend.observe_scope(&scope).unwrap();
    assert!(observed.known_escape);
    let suspect = launch.reconcile();
    assert_eq!(suspect.phase, AttemptPhase::Suspect);
    assert!(suspect.tracking_lost);
    // Termination reaches the known escaped identity as well as the root.
    let receipt = backend.signal_scope(&scope, libc::SIGKILL).unwrap();
    let signalled: Vec<u32> = receipt.signalled.iter().map(|(pid, _)| *pid).collect();
    assert!(signalled.contains(&escaped) && signalled.contains(&root.pid));
    root.reap();
    // The orphaned escapee is reaped by launchd; until then it is a zombie and
    // still counts as present.
    wait_until("escaped exit", || {
        let observation = backend.observe_scope(&scope).unwrap();
        observation.empty && observation.root_reaped && observation.known_members_gone
    });
    let after = backend.observe_scope(&scope).unwrap();
    assert!(after.empty && after.root_reaped && after.known_members_gone);
    assert!(after.known_escape);
    let still = launch.reconcile();
    assert_eq!(still.phase, AttemptPhase::Suspect);
    assert!(still.reservation.is_some());
    record(
        "scope-escape",
        json!({"escaped_pid": escaped, "first_observation": format!("{observed:?}"),
               "after_exit": format!("{after:?}"), "phase": still.phase,
               "tracking_lost": still.tracking_lost, "termination": receipt}),
    );
}

#[test]
fn unapplied_or_unsafe_scopes_are_refused_before_authorization() {
    let mut launch = prepared_launch("refused");
    let backend = launch.fixture.host.backend();
    let plan = launch.record.plan.clone().unwrap();
    let quantities = launch.record.reservation.as_ref().unwrap().quantities;
    let key = launch.record.key.clone();
    let owner = launch.principal.instance().clone();

    // A root without the utility clamp: recorded as failed, binding refused,
    // then cleaned up before any authorization. Some environments clamp every
    // child of this harness (governed self-use does), even when the harness
    // itself does not read as clamped; the case then cannot be produced and is
    // recorded as not run, which makes the qualification suite incomplete.
    let harness = backend.scheduler_readback(std::process::id()).unwrap();
    let mut plain = Root::start("plain-root");
    let failed = launch.establish(&plain);
    let readback = failed.cpu.unwrap();
    let unclamped_case = if readback.max_thread_priority <= devguard_macos::UTILITY_PRIORITY_CEILING
    {
        json!({"status": "not_run", "harness_readback": harness, "root_readback": readback,
               "reason": "the environment clamps every child of this harness, so an unclamped root cannot be produced"})
    } else {
        assert_eq!(failed.applied.cpu, ApplicationState::Failed, "{readback:?}");
        assert_eq!(
            launch
                .authority
                .bind_scope(&launch.principal, &key, &launch.permit, &failed.scope)
                .unwrap_err()
                .code,
            ErrorCode::ResourcePolicyUnsupported
        );
        json!({"status": "passed", "harness_readback": harness, "root_readback": readback,
               "binding": "resource_policy_unsupported"})
    };
    let cleanup = backend.signal_scope(&failed.scope, libc::SIGKILL).unwrap();
    assert!(cleanup.complete);
    assert_eq!(
        cleanup.signalled,
        vec![(plain.pid, failed.scope.root.start_ticks)]
    );
    plain.reap();
    assert_eq!(
        launch
            .authority
            .lookup(&launch.principal, &key)
            .unwrap()
            .phase,
        AttemptPhase::LaunchCommitted
    );

    // A root sharing the test's process group is not a scope.
    let shared = Root::start("shared-root");
    let identity = backend.process_identity(shared.pid).unwrap().unwrap();
    let error = backend
        .establish_scope(&key, &owner, &identity, &plan, quantities)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourcePolicyUnsupported);

    // A group that already has a member before the payload is not clean.
    let mut early = Root::start("qos-root");
    early.command("spawn", "spawned ");
    let identity = backend.process_identity(early.pid).unwrap().unwrap();
    let dirty = backend
        .establish_scope(&key, &owner, &identity, &plan, quantities)
        .unwrap_err();
    assert_eq!(dirty.code, ErrorCode::ResourcePolicyUnsupported);

    // Another user's process cannot even be observed as a root; the refusal
    // is reported as such, not as an absent process.
    // SAFETY: geteuid has no preconditions.
    let denied = if unsafe { libc::geteuid() } != 0 {
        let clock = launch.fixture.host.clock();
        let launchd = ProcessIdentity {
            boot_id: clock.boot_id().into(),
            pid: 1,
            start_ticks: 1,
        };
        let denied = backend
            .establish_scope(&key, &owner, &launchd, &plan, quantities)
            .unwrap_err();
        assert_eq!(denied.code, ErrorCode::ResourceControlUnavailable);
        assert!(denied.message.contains("refused"), "{denied:?}");
        json!(denied)
    } else {
        json!({"status": "not_run", "reason": "the harness runs as root"})
    };

    // Kernel-level limits are refused at admission, before any launch.
    let mut kernel = request("kernel-required");
    kernel.intent.minimum = ResourceLevels::KERNEL;
    let rejected = launch.authority.admit(&launch.principal, kernel).unwrap();
    assert_eq!(rejected.phase, AttemptPhase::Denied);
    assert_eq!(rejected.denial, Some(ErrorCode::ResourcePolicyUnsupported));

    // An unregistered scope has no binding or observation evidence.
    let unknown = ScopeIdentity {
        kind: ScopeKind::ObservedProcessGroup,
        scope_id: "pg-unknown".into(),
        root: identity,
    };
    let unknown_binding = backend.binding(&unknown).unwrap_err();
    assert_eq!(unknown_binding.code, ErrorCode::ReconciliationRequired);
    assert!(backend.observe_scope(&unknown).is_err());
    record(
        "scope-refusals",
        json!({"unclamped_root": unclamped_case, "shared_group": error,
               "dirty_group": dirty, "other_user_root": denied,
               "kernel_requirement": rejected.denial, "unknown_scope": unknown_binding}),
    );
}
