# CodeSpace integration specification

This is a future consumer specification. [Design revision 1](../design-revision-1.md) (2026-09-27) re-fixed its confirmation baseline at CodeSpace `b6e7ed22e2c730ac987297455e250cbd6e8e8b0c` and keeps the initial inspection baseline `e94d21475643608ad2a466256fb57266b86faa47` as history; the runtime paths below did not change between the two except for the patch helper's test reuse. It does not install a runtime pin. DG-1 remains independent; CS-RG begins adoption after DG1-C12, which is complete: release `0.1.0-5daee5d-b3fa569e` is macOS-qualified and is the pin candidate. Revision 1 keeps Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`, and any later pin change is a separate decision based on verification. Preserve existing authorization, approval, workspace, PTY and shutdown behavior. The 2026-09-26 review aligned this specification with DG-1's implemented consumer interface ([contracts](../contracts.md)); revision 1 revised its execution layer.

## Source mapping

All paths in this table refer to the fixed [CodeSpace source tree](https://github.com/novelKR/CodeSpace/tree/b6e7ed22e2c730ac987297455e250cbd6e8e8b0c).

| Path | Observed baseline | Planned boundary |
| --- | --- | --- |
| `crates/server/src/mcp.rs` | Authorization/workspace acquisition precedes Runner; approval becomes resuming before execution | Prepare before approval consumption, CSRG-C03/C04 |
| `crates/store/src/approvals.rs` | Only selected occupancy failures return to queued | Same-attempt CAS and confirmed non-start evidence, CSRG-C04 |
| `crates/server/src/runtime.rs` | Owns managed worker and current shutdown behavior | Single execution-owner registration, CSRG-C02; separate opt-in recovery, P1R-C01 |
| `crates/runner/src/process.rs` | Owns child/PTY/I/O; pipe and PTY slot checks follow spawn; besides the waiter, the pipe timeout task and `request_kill` reap through `try_wait` (`:366`, `:714`); pipe children are `kill_on_drop` | One supervisor and one reaper per execution, slots before spawn, observe before reap, distinct lifetimes, CSRG-C00/C03/C04 |
| `crates/runner/src/wire.rs` | Wire 6, 16 MiB frames, 32 completed replay entries, serial dispatch/shared writer | Control/data lanes, inflight single-flight and aggregate bounds, CSRG-C05/C06 |
| `crates/pty/src/lib.rs` | Supplies empty inherited-FD list; the pinned spawn reaps internally and reports only an exit code | Isolates Codex types as the legacy `off` backend (`BackendReaped`) until the CSRG-C09 decision |
| `crates/runner/src/files.rs` | Whole-file read/hash before response window | Separate memory-bound improvement; bound qualification fixtures meanwhile |
| `scripts/validate-upstream.py` | Fixed target/report paths; all and macos-core separate; platform skips | Preserve existing gates and add consumer qualification, CSRG-C08/DGL-C06 |

## Execution ownership

This section applies [design revision 1](../design-revision-1.md). The [pinned Codex PTY](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/utils/pty/src/pty.rs) supports selected inherited FDs, but its high-level spawn cannot carry a DG-1 managed execution unchanged:

- Its pipe and PTY spawn functions reap the child in their own task, so an owner could not observe the scope before the reap.
- It keeps preserved descriptors open only if they are already inheritable, while DG-1 keeps the permit and transcript descriptors close-on-exec until its own `pre_exec`.

The cause is a mismatch in the reap-ownership and descriptor-passing contracts. Do not hide it behind a wrapper, and do not generalize it into "Codex cannot be used". The finding has five parts:

| Part | Problem | Required change |
| --- | --- | --- |
| F1a reap ownership | Termination, timeout and Drop paths besides the waiter can reap the child | Inventory every path and keep one reaper per execution |
| F1b descriptor passing | Not closing descriptors differs from passing close-on-exec descriptors only to one child | Specify descriptor ownership, inheritance and closing from preparation to payload |
| F1c concurrent spawn | Another thread's spawn can race the creation of private descriptors | Process-wide spawn protection, with a proof for each exception |
| F1d output and handle | Loss inside a bridge and Drop behavior can differ from the upper contract | Check output-loss, backpressure and Drop fitness |
| F1e maintenance branch | A common interface over duplicate backends that stay indefinitely | Decide integration or a limited backend in CSRG-C09 |

The Runner coordinates every execution in one layer: execution identity, the link between approval and execution, state transitions, timeout, termination requests, output recording, and completion and release. Backends keep OS child creation, I/O attachment, terminal setup, platform exit observation and the actual reap. The planned responsibilities, named as concepts rather than existing APIs, are an execution coordinator, a resource governor (off or DevGuard), a `PreparedExecution` with its `LaunchPlan`, a process supervisor and backends: the legacy Codex PTY, the legacy Tokio pipe and an owned Unix process with pipe and PTY transports. DevGuard client types and Codex types never reach public MCP types, and the generic coordination layer never mixes the two upstreams' internal types.

| Ownership model | Meaning | Scope |
| --- | --- | --- |
| `BackendReaped` | The backend performs the reap and reports the result | Initial legacy `off` paths; never advertises an unreaped-exit state |
| `OwnerControlledReap` | CodeSpace controls the non-reaping observation and the order of the reap | The DevGuard `required` path |

Keep four state axes independent: preparation and dispatch (Prepared, Committed, HelperSpawned, Uncertain), the OS child (Running, ExitedUnreaped, Reaped, OwnershipLost), input and output (Open, EOF, Truncated, Failed) and the DevGuard resource (Reserved, Active, Suspect, Released). EOF is not exit, exit is not reap, reap is not descendant termination, output expiry is not lease release, and READY is not payload success. DG-1's `LaunchOutcome::Started` is likewise a sign read from the transcript: a helper killed between READY and exec looks the same.

- **One reaper.** On the `required` path, nothing outside the object that owns the child calls `wait`, `try_wait` or `waitpid`. `terminate_process`, timeout and shutdown send intents to the supervisor, which performs a verified termination. An OS exit notification leads to the supervisor's non-reaping observation, then `Observe`, then the reap through the same owning object. Because the pipe timeout task and `request_kill` also reap at the baseline, replacing the waiter alone is not enough. CSRG-C00 records an ownership table naming the owner and the message of each path: spawn, exit watching, timeout, `request_kill`, workspace termination, shutdown, backend Drop, task cancellation and error cleanup.
- **Pre-reap observation.** Detect the exit without reaping (`waitid` with `WNOWAIT`), attempt `Observe` within an overall budget, record success, failure or timeout, reap through the owned child, then observe or reconcile again as needed. The budget is proposed at 1 s and validated in CSRG-C00; it is not a qualified value and never delays status or termination responses. A failure keeps the lease charged, no zombie waits indefinitely for a success, and `ECHILD` or lost ownership is neither a successful observation nor evidence of non-execution. A blocking call wrapped in a timeout future stays tracked while it keeps running.
- **Signals.** Termination needs no new DevGuard grant, but during an authority outage it cannot prove that the whole scope ended. The Runner controls the targets it can identify safely; otherwise it reports an incomplete result and keeps resources charged. A managed backend never signals a stale numeric PID or PGID, and Codex's process-group termination code is reconciled with DevGuard's identity and scope contract before any reuse.
- **Spawn protection.** `helper_command` and `HelperCommand::spawn` take DG-1's `spawn_guard` themselves, and a standard `Mutex` locked again by its holder does not return normally, so no caller holds the guard around them. Every other child-creation path in the same OS process holds the common guard, or a verified equivalent, around descriptor creation, inheritance setup and the spawn, and nothing longer. That covers pipe and PTY spawns, patch and sandbox helpers, auxiliary commands, worker creation and test helpers. No DevGuard request, permit wait, transcript read, child wait, output drain or long write happens under the guard. Making one's own descriptors close-on-exec does not close the race with another thread creating descriptors, and a kernel-level close-on-exec-by-default path is an exception only once its application, error and fallback paths are verified. Two linked versions of the client crate would carry two guards, so verify that one protection object is shared. The Gateway and the UDS Runner are separate processes and are audited separately. Until the legacy Codex PTY's spawn and descriptor-cleanup path is shown safe, mixed legacy and managed execution in one process is not declared verified: replace that backend in the same scope or restrict the supported combinations.
- **PTY composition.** A managed PTY keeps `HelperCommand`'s permit and transcript passing. The parent's private descriptors stay protected until the helper exists. The child receives the permit, the transcript and legitimate jobserver descriptors, kept distinct; the credential closes before the payload and the transcript stays close-on-exec. The PTY slave becomes stdin, stdout and stderr, and session and controlling-terminal setup follow a specified order (`setsid`, then `TIOCSCTTY`). A setup failure cleans up the child, master, slave and private descriptors, and the descriptors and PID meaning survive the helper's QoS re-execution. `pre_exec` callbacks run after fork in registration order, so their data is prepared beforehand, with no allocation, locking or environment lookup. Never drop a descriptor the helper needs from a keep-list, and never apply PTY setup and process-group setup twice blindly.

## Registration and startup

Each execution owner has one registered instance: the internal Runner uses the Gateway PID in InProcess, and the worker uses its own PID in UDS. OS UID/PID/boot/start observations establish identity. Gateway cannot declare the worker's identity, and no launcher registers a second instance. One static control reservation includes Gateway and Runner costs.

DG-1 gives every frame, including idle waiting, an absolute 250 ms deadline. The owner therefore opens a fresh session for each step: connect, `Hello`, `Authenticate`, `Register` with the same instance ID, then one request. DG-1 has no separate `service-exec` path. The Gateway passes the consumer credential to a UDS worker through `CredentialHandoff`, and InProcess reads it directly. [ADR-001](decisions.md#adr-001--one-registration-by-the-execution-owner) records this refinement. The 250 ms preparation budget, the five-second Prepared lifetime and this 250 ms frame deadline are three distinct limits: allowing 250 ms per frame does not bound the whole preparation, and CSRG-C00 verifies deadline propagation from connect through `Admit`.

The worker consumes and closes the credential descriptor at startup. The permit and transcript descriptors exist only for one helper and close at its exec, and the [spawn protection](#execution-ownership) rules keep every other child from inheriting a grant. Test actual payload FD lists and failure cleanup. Distinguish required jobserver FDs from credentials; do not put secrets in argv, environment, MCP input, logs or journals.

Missing resource settings retain `off`. Operators explicitly choose `required`; unavailable authentication, capacity or capability cannot silently fall back to off. This includes Linux, where DG-1 reports `ResourcePolicyUnsupported` until DG-LINUX.

- **Consumer provisioning.** The operator provisions a `codespace` consumer in `host.toml`: role `control_service`, a generation, a private credential file, an instance limit and the static control reservation. The service reads this file only at start, and a restart leaves committed attempts Suspect. Apply it while nothing is charged, using `CloseAdmission` and `Quiescence`.
- **Workspace settings.** Workspace `resources` settings name the profile, the requested budget and the minimum levels for each exec. They are checked against work capacity when loaded, and a call cannot override them.
- **Schema and wire.** CSRG-C02 owns the eventual schema and wire change. The initial wire target is 7 relative to baseline 6; use the next free version if another change has already consumed it.

## Execution and approval sequence

A preparation is an owned, one-time object. It holds the execution identity (process ID to attempt), the command meaning, an execution slot acquired before spawn, workspace occupancy linked to the FIFO and approval flow, the resource state (`off` or Prepared) and the deadline of the first preparation. It can be executed once or cancelled, and its cleanup separates unexecuted work from committed or uncertain work. It is never cloned for a second spawn or rebuilt from serialized argv. The Drop of a cancelled task is not assumed to return a lease; remote cleanup stays an explicit reconciliation task.

1. Apply existing authorization, workspace/network policy, argv and approval checks.
2. Acquire the existing workspace FIFO reservation for the process ID.
3. `PrepareExec` obtains a Runner execution slot **before spawn**, then requests admission for immutable attempt meaning.
   - The request is DG-1's `Admit`, which records a Prepared attempt.
   - The execution digest covers argv, cwd, the environment, tty, workspace and policy.
4. If preparation cannot finish within the approved 250 ms budget, return promptly, cancel unstarted preparation and release its guards. Do not add a long resource queue inside CodeSpace. CLI's explicit wait is separate.
5. After preparation succeeds, compare-and-set the same approval attempt to `resuming`. Failure to persist that transition cancels preparation.
6. `ExecPrepared` commits the attempt with `BeginLaunch` within its original five-second Prepared lifetime.
   - Only the first reply carries the one-time permit.
   - An expired attempt never silently creates another lease.
7. The Runner consumes the launch plan once: it starts `devguard-launch` as its direct child through `HelperCommand`, owns that handle, and tracks the transcript and the process handle separately. The helper verifies scope/policy/binding/authorization, writes `ready` to its transcript, then attempts user exec.
8. When the root exits, the owning supervisor detects it without reaping (`waitid` with `WNOWAIT`), attempts `Observe` within its budget, and only then reaps. Reconcile execution slot, workspace and resource lease against their respective completion evidence. Retain completed output independently.

The server-minted process ID maps one-to-one to the attempt. This does not add public persistent idempotency for arbitrary repeated MCP calls. The table separates evidence of non-execution from evidence of resource cleanup.

| Observation | Execution judgement | Resource and approval handling |
| --- | --- | --- |
| Explicit `Admit` refusal | No user code started | The approval is not consumed and the hold stays queued; clean up the preparation guards. A later explicit new attempt is possible |
| Lost `Admit` reply | No child exists, but the reservation state is unknown | Query or cancel the same attempt; release nothing locally before the authority confirms |
| Lost `BeginLaunch` reply | Local ownership shows whether the permit was never received or already reached a helper | Keep the same attempt; report `AbandonLaunch` only when no helper can exist |
| Helper spawn failed, or the helper exited before READY | Within what the spawn API guarantees, no helper exists; a helper without READY never attempted the executable | Report `AbandonLaunch`. The authority's release as `NoHelperCreated` (`known_not_started`), granted only for an unclaimed grant, is the non-start evidence; a claimed grant is settled only by its scope. Reuse only through same-attempt approval CAS |
| Transcript `failed` or `refused` | The helper reports that it never execs | Settle a claimed scope and an unclaimed grant separately; reuse only through same-attempt approval CAS |
| Transcript `ready` | Pre-exec boundaries completed | Not evidence of executable success or final exit |
| Transcript `exec_failed` after READY | The executable could not be entered after preparation passed | Distinct from a budget refusal; do not automatically restore or reuse the approval |
| `ready` without a final report, or the transcript or exit cannot be read | Execution cannot be ruled out | Preserve unknown; no automatic re-execution |
| Root exited, descendants survive | Only the root's exit is confirmed | Settle scope, workspace and lease separately |
| `Observe` failure or tracking loss | Whole termination is not proven | Reap within the bounded procedure; keep the lease charged |
| Resource lease released | Accounting reconciled with scope/noncreation/previous-boot evidence | Alone does not prove approval is reusable |

Calling `AbandonLaunch` is not itself evidence; what counts is the state the authority returns. A user executable may itself exit with 125, 126 or 127, the helper's own statuses, so an exit code alone never identifies the helper's phase or permits approval reuse.

CSRG-C04 provides nullable `resume_attempt_id`, migration and old/new fixtures. Existing NULL/uncertain resuming rows are not presumed unstarted. Keep execution outside the patch-operations ledger and do not expose it through `operation_status`.

## Errors, control and lifetime

| Error/state | Meaning | Handling |
| --- | --- | --- |
| `RESOURCE_UNAVAILABLE` | Insufficient budget, host pressure or preparation expired | Explicit retry only with non-start evidence. Name a pressure refusal as such: DG-1 keeps admission closed until memory has read normal for 30 s |
| `RESOURCE_CONTROL_UNAVAILABLE` | Authority connection, recovery or persistence unavailable | Refuse new execution; retain control of existing handles |
| `RESOURCE_POLICY_UNSUPPORTED` | Required resource capability unsupported, including Linux before DG-LINUX | Operator/support-combination correction |
| DG-1 `Unauthorized` / `InvalidRequest` | Credential, generation or request rejected | Refuse new execution; operator configuration correction |
| DG-1 `AttemptConflict` / `InvalidTransition` / `NotFound` / `ReconciliationRequired` / `JournalInvalid` | The attempt or authority storage disagrees with the Runner's record | New execution: refuse. Existing attempt: query and reconcile the same attempt as uncertain; never replay |
| `WORKSPACE_BUSY` / `RESOURCE_QUEUE_FULL` | Existing slot/workspace occupancy or FIFO saturation | Preserve meanings; do not conflate with DevGuard budget |
| `dispatch_status=unknown` | Execution uncertain | Query/terminate/reconcile the same process; no blind replay |

Keep existing MCP names, including **`terminate_process`**. During DevGuard failure, Runner handles continue to serve status and termination without new admission.

Separate control/data transport and processing capacity, then audit shared mutexes, writers and callbacks for propagated stalls; spawn and pre-reap `Observe` latency must not reach the control path either. Bound concurrent dispatch, queued requests/bytes, response and replay bytes, inflight entries, retention, stdin and lifecycle callbacks. Existing 16 MiB frames, 32 replay entries and 256 KiB output rings alone do not bound total memory. Baseline planned work dispatch is eight concurrent requests and 64 queued requests, with independent control capacity; termination must not starve behind status queries.

Pair control/data sockets into one authenticated worker session. Control carries handshake/status/termination/resize/essential lifecycle events; data carries file operations, prepare/exec, stdin and output reads. Preserve existing disconnect policy for existing modes. Backpressure on a live data lane must not block control. Inflight replay shares the same dispatch result; changed meaning conflicts; completed retention eviction cannot authorize another execution. Essential lifecycle events require delivery or reconciliation rather than silent loss.

Output converges on one CodeSpace collector with bounded retention, read through `read_process` and `output_lost`. Never mix the output of unrelated executions, keep EOF distinct from exit status, and distinguish intentional retention loss from unknown transport loss. Never report `output_lost=false` when the amount lost is unknown; if the protocol cannot express that, CSRG-C06 designs the compatibility change. At the pin, Codex's `ProcessDriver` bridge skips items lost to broadcast lag and `ProcessHandle`'s Drop terminates the process. `ProcessDriver` therefore qualifies only if all of these hold:

- bridge loss is never hidden from the record;
- a slow reader never blocks status or termination;
- a total byte bound exists, not only a channel count;
- no UI, response or temporary handle Drop causes a wrong termination;
- exit callbacks have no double termination, reap race or stale identity;
- the exit notice and the final output are ordered;
- it costs less than the code it removes.

Broadcast `Lagged(n)` is not a byte count, so exact accounting needs sequence and length metadata or a separate loss path. If fitting `ProcessDriver` needs more wrapping, use CodeSpace's own output and handle abstractions.

| Lifetime | Owner | Release evidence |
| --- | --- | --- |
| OS child | The supervisor's owning object, the only reaper | Reap after the pre-reap observation or its recorded failure |
| Execution slot/workspace occupancy | Runner/store | Defined execution/descendant/output-pump completion; unstarted preparation guard cleanup |
| Resource lease | DevGuard | Exact scope termination, NoHelperCreated or verified prior-boot termination |
| Completed result/output | CodeSpace retention | Existing retention, including 15-minute/64-entry limits; independent of lease release |

Root reap does not repair tracking loss. On macOS a scope is the root's process group, and DG-1 assumes cooperative workloads. The following leave an attempt Suspect and charged until a reboot:

- a workload that leaves its group, for example with `setsid` or a new background group;
- a root reaped before `Observe` while survivors remain;
- lost tracking after a DevGuard restart.

Expose such attempts in diagnostics rather than hiding them. Measure control request-to-response separately from actual scope termination and remote RTT.

## Recovery mode

Preserve all current mode termination policies. In the new independent Runner mode, distinguish normal shutdown, explicit service stop, planned detach and unexpected Gateway disconnect. Stop drains/terminates and reconciles; detach/disconnect lets the live Runner maintain original deadlines and I/O. InProcess is excluded. Runner/host loss remains uncertain until observed; records cannot recreate pipes or PTYs.

An authenticated Gateway obtains an epoch/fence and reconciles workspace occupancy, approval and DevGuard leases before mutations reopen. Stale Gateway mutations fail. Reconnecting to an existing execution needs no new workload budget. Restoring I/O after Runner restart is separate future work.

## Backend decisions

These are three independent decisions ([ADR-006](decisions.md#adr-006--codespace-execution-ownership-and-reuse-policy)). That CodeSpace owns the child directly implies neither a permanent Codex ban in DevGuard nor the replacement of every `off` backend.

- **D1, `required` execution.** The default is A1, a CodeSpace-owned Unix transport at the current pin. What it maintains includes master/slave lifetime, signal and session setup, resize, descriptor failure cleanup, output handling and cancellation and shutdown coordination; it is neither a few lines of opener nor the only possible design. Before new code is written, record the reuse options and their contract differences in this order: an existing public API, an upstream candidate with the same contract, limited adaptation under the ADR-006 policy, then in-house code.
- **D2, legacy `off` backends.** Preserve their behavior and move them to the common contract in stages. Before CSRG-C08, CSRG-C09 decides one of two outcomes. (A) Integrate, and remove the replaced code, branches, fixtures and dependencies for that platform and transport. (B) Keep a limited compatibility backend with its reason, remaining scope, common and duplicated parts, unsupported capabilities, revisit point and removal criteria. Neither "existing code" nor "parity passed" decides alone, and keeping a branch bears the same burden of proof as replacing it. The measurements are in [verification](verification.md#cs-rg-execution-verification).
- **D3, DevGuard.** DevGuard adds no Codex dependency now; the default distribution and shared client stay Codex-free, and adapter reuse follows the conditional policy.

## Required changes and rollout

| Recommendation/confidence | Observed cause | Minimum change and alternatives | Cost, validation and rollback |
| --- | --- | --- | --- |
| Required/high | PID-bound authority and Runner handle ownership | Single Runner registration; subordinate registrations only if later needed | Moderate; identity/FD tests; close admission and drain |
| Required/high | Post-spawn slots and pre-prepare approval transition | Prespawn permits, PrepareExec/ExecPrepared and same-attempt CAS | High; cancel/loss races; preserve unknown during rollback |
| Required/high | Several paths reap; the pinned spawn reaps internally and drops close-on-exec descriptors; concurrent spawns can race private descriptors | Common execution contract with one reaper, one-time launch plans, spawn-protection rules and one output collector; `required` on an owner-reaped backend; legacy `off` backends until the CSRG-C09 decision | High, judged by race and failure coverage rather than code size; ownership-table, FD/PTY composition, mixed-mode, survivor and escape tests; rotate credentials only after reconciliation |
| Required/high | The default 1 CPU CodeSpace reservation exceeds a 3-CPU hosted runner's 500 mCPU work capacity | Run real-authority cases on the qualification host; hosted runners run fixtures, or record cases as `not_run` | Low; record where each case ran |
| Required/high | Serial dispatch/shared writer/aggregate buffers | Bound the complete processing and transport path | High; saturation, latency and lock traces; drain sessions |
| Required/high | Per-directory authority lock | Canonical normal path plus parent-budget candidates | Moderate; alias/concurrent-start tests; preserve journals |
| Required/high | Strict contract/journal decoding | Real old/new fixtures and explicit migration | Moderate; reject incompatible downgrade; no stale snapshot after new writes |
| Strongly Recommended/high | Whole-file allocations exceed response window | Explicit size bound or streaming read/hash, separate follow-up | Moderate; large/changing-file memory tests and API review |

The current DG-1 sequence does not modify CodeSpace runtime. File-size/concurrency bounds must constrain qualification until the separate file-memory work is complete. Do not claim arbitrary-file protection.

Adoption order: DG1-C12 → CSRG-C00 execution-boundary fitness → C01/C02 pin/registration → C03/C04 supervision, preparation and approval → C05/C06 control/replay → C07 parity and C09 backend decision → C08 qualification of the resulting head → P1R-C01–C06 recovery. Add DGL-C01–C06 for actual Linux enforcement. Keep upstream qualification unchanged and record DevGuard results separately. [Readiness](consumer-readiness.md), [verification](verification.md) and [delivery](pr-delivery.md) determine permissible rollout.
