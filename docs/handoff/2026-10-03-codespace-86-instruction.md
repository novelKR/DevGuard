# Owner instruction: macOS fork aborts in CodeSpace spawns (2026-10-03)

> **Status: dated record of an owner instruction; not maintained.** The owner gave this text in a Claude Code
> session's chat on 2026-10-03, in two messages at 05:44:28 and 05:44:33 UTC. It was not committed anywhere at the
> time. It is reproduced below unchanged, so every fact in it is as of that moment. It approves nothing beyond what it
> says, and later owner decisions may have superseded parts of it: the [CS-RG session handoff of
> 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what happened next and which parts still apply.

| Item | Detail |
| --- | --- |
| Given | As the first two messages of a new Claude Code cloud session. The short first message was followed at once by the full specification. |
| Covers | The defect found while validating CodeSpace #84: a child spawned on macOS died with SIGKILL before `exec`. It asks for identification, production impact, a fix in a fixed decision order, two layers of verification, and names the constraints and the stop point. |
| Outcome | [CodeSpace #86](https://github.com/novelKR/CodeSpace/pull/86), labeled a mitigation, merged at the owner's exact-head approval (`3b1ed596`) on 2026-10-03 at 09:59 UTC as `02cf905`. The signal-death reporting problem it asks to record separately is [CodeSpace #85](https://github.com/novelKR/CodeSpace/issues/85). |
| Still applies | The descriptor hygiene, pin, architectural and milestone-isolation constraints. Structural spawn redesigns are on hold by step 8 of the owner's order of 2026-10-03. |

Both messages are copied from the session transcript byte for byte.

## Original text: first message, 05:44:28 UTC

````text
Context: CodeSpace repository (novelKR/CodeSpace). While validating PR novelKR/CodeSpace#84 (CSRG-U2), tests on macOS showed a child process spawned by `InProcessRunner::spawn_pipe` (crates/runner/src/process.rs) dying with SIGKILL before it ran. The runner then reported it as `termination: exited` with no exit code, a spawn that "succeeded" but whose process died at once.

Evidence:
- macOS crash reports for these children, which are named after the parent test binary because they die before exec, all show EXC_BREAKPOINT/SIGKILL with the stack: `std::process::Command::spawn > fork > libSystem_atfork_child > _notify_fork_child > _os_alloc_once > _os_once > _os_once_gate_wait > _os_once_gate_corruption_abort`. On macOS, fork() in a multithreaded process runs libSystem's atfork child handlers. If another thread of the parent is in the middle of initializing libnotify's os_once state at that moment, the child aborts.
- Reproduction: in the runner lib tests (`cargo test -p codespace-runner --features devguard --lib -- registration wire --test-threads=3`), a test that spawned `/bin/sleep` failed 2 of 40 runs. It failed 11 of 60 runs when paired only with `wire::tests::replay_returns_cached_response`, which performs a file Read through `InProcessRunner` (`read_file` → `touch_watch` + `codespace_fs`/Codex `LOCAL_FS` reads). It failed 0 of 60 when run alone. In the gateway's DevGuard test filter, which does in-process reads concurrently with spawns, 1 of 12 parallel runs failed with the same signature, and 0 of 6 sequential runs.
- A bare `notify::recommended_watcher` created concurrently with fork-based spawns did NOT reproduce it (0/25). So the libnotify initializer on the read path is not yet identified. Candidates: something in the Codex file-system code, tokio fs, or a libc call that registers with libnotify (timezone, directory services, ...).
- Spawns fork rather than use `posix_spawn` because `exclude_unrelated` (crates/runner/src/descriptors.rs, #79) installs a `pre_exec` step. posix_spawn would not run atfork handlers.

Task:
1. Identify which call on the in-process file read path first initializes libnotify. For example, set a breakpoint on `_os_alloc_once`/`notify_*` under lldb, or bisect the read path.
2. Assess the production impact in the gateway (InProcess) and the UDS worker. This is likely a once-per-process window around the first such initialization, concurrent with an exec_command spawn, and PTY spawns may be affected too.
3. Propose and implement a fix with a deterministic regression test, keeping #79's descriptor-exclusion guarantees. Options: initialize libnotify (or the identified subsystem) before the runner accepts work, use posix_spawn with `POSIX_SPAWN_CLOEXEC_DEFAULT` plus explicit inherit file actions instead of a pre_exec fork, or retry a spawn whose child was killed before exec.

Constraints:
- Do not weaken the descriptor-exclusion tests.
- Do not change the Codex pin.
- This is independent of the CS-RG units.
- The test-side mitigations already in #84 are: the timeout test runs alone in a new process, and the macOS CI leg runs `cargo test -p codespace-server --features devguard --lib -- devguard --test-threads=1`.
````

## Original text: specification, 05:44:33 UTC

````text
Investigate and Resolve macOS Fork Aborts in CodeSpace Spawns
Context
CodeSpace repository (`novelKR/CodeSpace`). While validating PR `novelKR/CodeSpace#84` (CSRG-U2), macOS tests showed children spawned by `InProcessRunner::spawn_pipe` (`crates/runner/src/process.rs`) killed by `SIGKILL` inside `fork()`, before `exec` of the target binary. The defect is independent of CSRG-U2; #84 only exposed it.
The failure is invisible to callers. std's fork path treats an exec-error pipe that closes without data as a successful exec, so a child that dies before `exec` cannot be told apart from one that executed. `spawn` returned success, and the runner reported `termination: exited` with no exit code.
Evidence

* Crash signature. macOS crash reports for these children are named after the parent test binary, because they die before `exec`. All show `EXC_BREAKPOINT` / `SIGKILL` with the stack `std::process::Command::spawn > fork > libSystem_atfork_child > _notify_fork_child > _os_alloc_once > _os_once > _os_once_gate_wait > _os_once_gate_corruption_abort`.
* Mechanism. On macOS, `fork()` in a multithreaded process runs libSystem's atfork child handlers. If another parent thread is inside libnotify's one-time global initialization (`_os_alloc_once`) at that moment, the child finds the once gate held by a thread that does not exist in the child, and aborts.
* Why CodeSpace forks. On macOS, std uses `posix_spawn` only when no `pre_exec` closure is installed, among other conditions. `posix_spawn` runs no atfork handlers.
   * `exclude_unrelated` (`crates/runner/src/descriptors.rs`; issue #79, fixed in #81) installs a `pre_exec` step, so those spawns fork.
   * Other `pre_exec` users fork as well: the PTY path through portable-pty (to be verified), and, on #84's branch, the gateway-to-worker spawn. There, DevGuard's pinned `CredentialHandoff::attach` installs a `pre_exec` (DevGuard `f1f9084`, `crates/client/src/credential.rs`).
* Reproduction, observed on #84's branch. The `devguard` runner feature, the `registration` tests and the `/bin/sleep` timeout test exist only there.
   * `cargo test -p codespace-runner --features devguard --lib -- registration wire --test-threads=3`: the `/bin/sleep` test failed 2 of 40 runs.
   * Paired only with `wire::tests::replay_returns_cached_response`: 11 of 60. That test reads a file through `InProcessRunner` (`read_file` → `touch_watch` + `codespace_fs`/Codex `LOCAL_FS`).
   * Alone: 0 of 60. Paired with tests that do not read files: 0 failures (40–60 runs each).
   * Gateway DevGuard test filter, where in-process reads run concurrently with spawns: 1 of 12 parallel runs failed, 0 of 6 sequential runs.
* Initializer not identified.
   * A bounded experiment created a `notify::recommended_watcher` concurrently with fork-based spawns. It did not reproduce the abort (0 of 25; control 0 of 25), but it never confirmed that the watcher's initialization overlapped any fork.
   * The watcher, including FSEvents/CoreFoundation initialization on macOS, is therefore not excluded.
   * Candidates: the `notify` watcher backend, Codex file-system code, Tokio fs, and libc calls that register with libnotify (time zone, directory services).

Tasks

1. Reproduce on `main` first. Before modifying product code, build a reproduction harness on current `main` and measure the pre-fix failure rate. Examples: `InProcessRunner` file reads concurrent with pipe spawns, or `replay_returns_cached_response` paired with existing spawn tests. This rate is the positive control for every later stress claim.
2. Best-effort identification of the libnotify initializer.
   * Under LLDB, break on the first libnotify entry points (`notify_register_*`, `notify_post`, `notify_get_state`, or the `_os_alloc_once` call for libnotify's globals), and bisect the read path.
   * Neither the structural fix nor the mitigation depends on the answer, because the mitigation can initialize libnotify itself. The answer bounds the trigger and makes the limitation statement exact.
3. Assess production exposure.
   * Actual binaries. Use the production gateway (`codespace-mcp`) and worker (`codespace-codex-runtime`), not test binaries. Determine when libnotify is first initialized relative to serving requests. Production startup (logging, SQLite, HTTP) may already initialize it before any concurrent spawn. Record the result as the production-exposure finding.
   * Boundaries. Assess separately, comparing each boundary's thread state and concurrency:
      * InProcess gateway payload spawns;
      * the gateway-to-UDS-worker spawn (`RuntimeProcess::spawn` / `worker_command`, once at startup);
      * worker-to-payload pipe spawns;
      * patch-helper spawns.
   * Scope of the window. Determine whether the window is truly once per process, and whether other atfork-sensitive initializers create equivalent windows.
   * PTY.
      * Determine from the actual implementation (CodeSpace's PTY crate into Codex's pinned PTY code) whether macOS PTY spawns fork.
      * Do not infer it from the pipe path, because #81's descriptor exclusion does not apply to PTY.
      * The PTY spawn code is inside the Codex pin and cannot be changed here. If it forks, report it as structurally unresolved and state whether the process-level mitigation covers it.
4. Decide and implement, in this order.
   1. Verify std.
      * In the CI toolchain's std source, check whether macOS `posix_spawn` spawns close-by-default (`POSIX_SPAWN_CLOEXEC_DEFAULT`).
      * List every condition under which std falls back from `posix_spawn` to fork. Besides `pre_exec`, that includes, for example, a `PATH` set in the child environment combined with a bare program name. CodeSpace sets child environments and accepts bare program names.
      * If std already spawns close-by-default, remove the `pre_exec` on macOS for the affected paths, keep them off every fallback condition, and prove the three acceptance conditions below.
   2. Direct `posix_spawn`.
      * Otherwise, evaluate `posix_spawn` with `POSIX_SPAWN_CLOEXEC_DEFAULT` and explicit inherit actions.
      * Expected, to be verified: std exposes neither the flag nor `posix_spawn_file_actions_addinherit_np`, and `tokio::process::Child` cannot adopt a process spawned outside `tokio::process::Command`. This would need a custom child handle and reaper.
      * Without close-by-default, `posix_spawn` reopens #79's race.
      * If confirmed, record this as the demonstrated incompatibility and do not implement it.
   3. Trampoline: evaluate only.
      * The design: a small single-threaded exec stage, spawned through std's `posix_spawn` path without `pre_exec`, closes unrelated descriptors and then `exec`s the payload.
      * The pid is unchanged, so `Child`, reaping, kill, timeout, stdio and pumps are preserved.
      * The costs: an extra shipped binary; a protocol that reports payload `exec` failure (to keep `PROCESS_SPAWN_FAILED` and `dispatch_status` semantics); and an extra `exec`.
      * It is not implemented under this task. Report its feasibility and costs as an owner decision.
   4. Mitigation.
      * If no structural option is implemented, deliver a mitigation-only PR.
      * It completes libnotify's global initialization once, single-threaded, at the start of each production process (gateway and worker `main`), before any thread can spawn. Use a harmless libnotify call, and confirm which call completes the initialization.
      * The test harness that measures it must call the same initialization function; otherwise the stress result does not verify the product.
      * Label the PR as a mitigation; see Mitigation Limitation.
   5. Prohibited.
      * Do not automatically retry a child because it died before CodeSpace observed it run.
      * std reports such a child as a successful spawn, so a retry cannot prove the payload never executed, and it would break no-duplicate-execution semantics.
5. Two-layer regression verification.
   * Layer 1, deterministic structure.
      * Ordinary pipe, worker and patch-helper children inherit no descriptor above 2, including high-numbered inheritable descriptors.
      * Add a macOS fork-detection control in an isolated subprocess, because `pthread_atfork` registrations are process-global and irreversible. Register a child handler that writes one byte to a pipe: a path that forks delivers the byte, and `posix_spawn` does not.
      * The positive control is a `Command` with a no-op `pre_exec`.
      * Cover every changed path, including a bare program name with a child `PATH`.
   * Layer 2, concurrency.
      * Run the Task 1 harness before and after the fix.
      * Choose N so that the measured pre-fix rate predicts at least 30 failures; for example, N = 300 at the observed 11 of 60. Require 0 failures after the fix.
      * On macOS CI, upload `~/Library/Logs/DiagnosticReports` from the stress step, and report the count of `_notify_fork_child` crash reports before and after.
      * A synchronized reproduction of the libnotify race is optional: os_once internals cannot be made deterministic with a test barrier.

Constraints

* Descriptor hygiene. Do not weaken the descriptor-exclusion requirements or the coverage of #79 and #81. Any non-stdio descriptor that a new implementation deliberately passes is a new capability and needs individual justification and regression coverage.
* Codex and DevGuard pins. Keep Codex at `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (`rust-v0.154.0`) and DevGuard at `f1f908429abea962d62d6b53c56a26c17250179b`. Do not bypass the issue with an unapproved bump of either.
* Architectural invariant. Any replacement spawn primitive stays an implementation detail of the existing pipe, worker and patch-helper paths. It must not:
   * introduce a new execution backend;
   * change spawn or reap ownership;
   * replace Tokio/std child handles;
   * transfer lifecycle responsibility.
* Milestone isolation. This is an independent CodeSpace correctness fix. It does not count toward CS-RG, and it does not change CSRG-U2 semantics.
* Interaction with #84.
   * If #84 merges first, its gateway-to-worker spawn stays on the fork path, because DevGuard's pinned `attach` installs a `pre_exec`.
   * Do not change U2's handoff or the DevGuard pin. Report that path's exposure (one spawn at gateway start) and whether the delivered fix covers it.
   * If a structural fix touches that spawn, the carrier descriptor needs an explicit, justified allow-list entry.
   * #84's timeout-test isolation and its sequential macOS DevGuard run are test-side isolations, not a product resolution. Once both PRs are merged, remove each of them or justify keeping it, so concurrency coverage is restored.
* Stop condition.
   * Stop and request an owner decision, reporting the smallest demonstrated incompatibility, if neither of these can be delivered within these constraints:
      * a structural fix that meets all three acceptance conditions;
      * the Task 4 mitigation, verified by the before-and-after comparison.
   * Do not weaken descriptor hygiene, fall back to automatic retry, change a pin, or add a backend merely to close the task.

Cold Start and Branch Boundary
Before modifying product code, re-query the live CodeSpace `main`, the Codex gitlink, relevant open PRs (#84 in particular) and current CI. Create the work from current `main`, not from #84. If live state has materially changed (for example, #84 has merged), reassess the affected spawn paths before editing.
Complete Production Spawn Inventory
Inventory every production child-creation site. For each platform, classify whether it takes std's `posix_spawn` path or the fork path.
Any `pre_exec` closure forces fork, whether it comes from `exclude_unrelated`, `exclude_unrelated_std`, portable-pty or DevGuard's `CredentialHandoff::attach`. std's other fallback conditions also force fork.
Include at least:

* InProcess pipe payload spawning;
* gateway-to-UDS-worker spawning;
* worker-to-payload pipe spawning;
* patch-helper spawning, at every production call site;
* PTY spawning, assessed from its actual implementation;
* the Linux sandbox helper (Linux only; listed for completeness, not a macOS exposure).

Preserve the current Linux `close_range` behavior unless the chosen implementation genuinely requires a cross-platform change.
Structural-Fix Acceptance
A structural fix satisfies all three conditions:

1. ordinary CodeSpace children inherit no descriptor above 2;
2. the affected macOS spawn paths do not fork, as shown by the Layer 1 fork-detection control;
3. CodeSpace keeps its process ownership and its `tokio::process::Child` management, reaping, kill, timeout, stdio, pump and lifecycle semantics.

Native `posix_spawn` with close-by-default is a candidate, not a pre-approved architectural change.
Regression Requirements
Preserve every existing #79/#81 descriptor test. Add:

* deterministic coverage for every affected production spawn path;
* high-numbered inheritable-descriptor exclusion;
* the macOS fork-detection control, with its positive control;
* spawn-error, kill, timeout, reaping, stdio and worker behavior: the runner's process tests, `uds_runner`, the gateway's process and end-to-end suites, and the Linux leg;
* the before-and-after concurrency comparison on the Task 1 harness.

Mitigation Limitation
A mitigation-only PR states exactly what it closes: the observed libnotify once-initialization race, in the processes whose `main` performs the initialization. Name the initializer if Task 2 identified it.
It does not establish general multithreaded-fork safety. The fork paths remain, and other atfork-sensitive initializers may open equivalent windows. List PTY and any other remaining fork paths as unresolved.
Out-of-Scope Observation
The runner reports a signal death without a recorded kill intent as `termination: exited` with no exit code, so an external `SIGKILL` reads like a normal exit. This is why the defect looked like success. Record it as a separate issue, and do not change the MCP contract under this task.
Completion and Stop Point
Deliver one CodeSpace product-fix PR at an exact head, not a research or architecture PR. State in its title and description whether it is a structural fix or a mitigation. The PR contains:

* the fix;
* deterministic regression tests and the fork-detection control;
* preserved #79/#81 descriptor guarantees;
* the spawn inventory and coverage of the affected paths;
* exact macOS and Linux CI results;
* the before-and-after concurrency evidence;
* process-lifecycle parity results;
* the production-exposure finding;
* the chosen approach, the rejected alternatives with their demonstrated incompatibilities, and the trampoline assessment;
* known limitations and rollback.

Do not:

* change the Codex or DevGuard pin;
* count this work as CS-RG progress;
* modify CSRG-U2 semantics to make this task pass;
* merge without a new explicit exact-head owner approval.

If the stop condition applies, stop with the demonstrated incompatibility and request an owner decision.
````
