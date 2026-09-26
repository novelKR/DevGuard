# CodeSpace integration specification

This is a future consumer specification grounded in CodeSpace `e94d21475643608ad2a466256fb57266b86faa47`. It does not install a runtime pin. DG-1 remains independent; CS-RG begins adoption after DG1-C12, which is complete: release `0.1.0-5daee5d-b3fa569e` is macOS-qualified and is the pin candidate. Preserve Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`, existing authorization, approval, workspace, PTY and shutdown behavior. The 2026-09-26 review aligned this specification with DG-1's implemented consumer interface ([contracts](../contracts.md)).

## Source mapping

All paths in this table refer to the fixed [CodeSpace source tree](https://github.com/novelKR/CodeSpace/tree/e94d21475643608ad2a466256fb57266b86faa47).

| Path | Observed baseline | Planned boundary |
| --- | --- | --- |
| `crates/server/src/mcp.rs` | Authorization/workspace acquisition precedes Runner; approval becomes resuming before execution | Prepare before approval consumption, CSRG-C03/C04 |
| `crates/store/src/approvals.rs` | Only selected occupancy failures return to queued | Same-attempt CAS and confirmed non-start evidence, CSRG-C04 |
| `crates/server/src/runtime.rs` | Owns managed worker and current shutdown behavior | Single execution-owner registration, CSRG-C02; separate opt-in recovery, P1R-C01 |
| `crates/runner/src/process.rs` | Owns child/PTY/I/O; pipe and PTY slot checks follow spawn; waiting reaps the child | Acquire slots before spawn, observe before reap, maintain distinct lifetimes, CSRG-C03/C04 |
| `crates/runner/src/wire.rs` | Wire 6, 16 MiB frames, 32 completed replay entries, serial dispatch/shared writer | Control/data lanes, inflight single-flight and aggregate bounds, CSRG-C05/C06 |
| `crates/pty/src/lib.rs` | Supplies empty inherited-FD list; the pinned spawn reaps internally and reports only an exit code | Stays for `off`; managed launches use Runner-owned pipes or PTY around DG-1's `HelperCommand`, CSRG-C03/C07 |
| `crates/runner/src/files.rs` | Whole-file read/hash before response window | Separate memory-bound improvement; bound qualification fixtures meanwhile |
| `scripts/validate-upstream.py` | Fixed target/report paths; all and macos-core separate; platform skips | Preserve existing gates and add consumer qualification, CSRG-C08/DGL-C06 |

The [pinned Codex PTY](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/utils/pty/src/pty.rs) supports selected inherited FDs, but it cannot carry a DG-1 launch.

- Its pipe and PTY spawn functions reap the child in their own task, so an owner could not observe the scope before the reap.
- It keeps preserved descriptors open only if they are already inheritable, while DG-1 keeps the permit and transcript descriptors close-on-exec until its own `pre_exec`.

Managed launches therefore start `devguard-launch` through DG-1's `HelperCommand` with Runner-owned pipes or a Runner-opened PTY (`setsid` and `TIOCSCTTY` in `pre_exec`). The Codex adapter remains for `off`, and the pin is unchanged.

## Registration and startup

Each execution owner has one registered instance: the internal Runner uses the Gateway PID in InProcess, and the worker uses its own PID in UDS. OS UID/PID/boot/start observations establish identity. Gateway cannot declare the worker's identity, and no launcher registers a second instance. One static control reservation includes Gateway and Runner costs.

DG-1 gives every frame, including idle waiting, an absolute 250 ms deadline. The owner therefore opens a fresh session for each step: connect, `Hello`, `Authenticate`, `Register` with the same instance ID, then one request. DG-1 has no separate `service-exec` path. The Gateway passes the consumer credential to a UDS worker through `CredentialHandoff`, and InProcess reads it directly. [ADR-001](decisions.md#adr-001--one-registration-by-the-execution-owner) records this refinement.

The worker consumes and closes the credential descriptor at startup. The permit and transcript descriptors exist only for one helper and close at its exec. Every other Runner spawn holds DG-1's `spawn_guard` or creates its descriptors close-on-exec, so no other child inherits a grant. Test actual payload FD lists and failure cleanup. Distinguish required jobserver FDs from credentials; do not put secrets in argv, environment, MCP input, logs or journals.

Missing resource settings retain `off`. Operators explicitly choose `required`; unavailable authentication, capacity or capability cannot silently fall back to off. This includes Linux, where DG-1 reports `ResourcePolicyUnsupported` until DG-LINUX.

- **Consumer provisioning.** The operator provisions a `codespace` consumer in `host.toml`: role `control_service`, a generation, a private credential file, an instance limit and the static control reservation. The service reads this file only at start, and a restart leaves committed attempts Suspect. Apply it while nothing is charged, using `CloseAdmission` and `Quiescence`.
- **Workspace settings.** Workspace `resources` settings name the profile, the requested budget and the minimum levels for each exec. They are checked against work capacity when loaded, and a call cannot override them.
- **Schema and wire.** CSRG-C02 owns the eventual schema and wire change. The initial wire target is 7 relative to baseline 6; use the next free version if another change has already consumed it.

## Execution and approval sequence

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
7. Runner starts `devguard-launch` as its direct child through `HelperCommand` and owns that handle. The helper verifies scope/policy/binding/authorization, writes `ready` to its transcript, then attempts user exec.
8. When the root exits, the Runner detects it without reaping (`waitid` with `WNOWAIT`), calls `Observe`, and only then reaps. Reconcile execution slot, workspace and resource lease against their respective completion evidence. Retain completed output independently.

The server-minted process ID maps one-to-one to the attempt. This does not add public persistent idempotency for arbitrary repeated MCP calls.

| Observation | Meaning | Approval/retry rule |
| --- | --- | --- |
| Preparation refused (`Admit` error) | No user code started | Keep hold queued; later explicit new attempt is possible |
| Permit reply lost, helper not created, or helper exited before READY | `AbandonLaunch` releases the unclaimed grant as `NoHelperCreated` (`known_not_started`) | Reuse only through same-attempt approval CAS |
| Transcript `failed` or `refused` | The helper never execs; a grant it claimed is settled only by its scope | Not started; reuse only through same-attempt approval CAS |
| Transcript `ready` | Pre-exec boundaries completed | Not evidence of executable success or final exit |
| Transcript `exec_failed` | Failure of an already managed execution | Do not unconditionally restore queued approval |
| `ready` without a final report, or the transcript or exit cannot be read | Execution cannot be ruled out | Preserve unknown; no automatic re-execution |
| Resource lease released | Accounting reconciled with scope/noncreation/previous-boot evidence | Alone does not prove approval is reusable |

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

Separate control/data transport and processing capacity, then audit shared mutexes, writers and callbacks for propagated stalls. Bound concurrent dispatch, queued requests/bytes, response and replay bytes, inflight entries, retention, stdin and lifecycle callbacks. Existing 16 MiB frames, 32 replay entries and 256 KiB output rings alone do not bound total memory. Baseline planned work dispatch is eight concurrent requests and 64 queued requests, with independent control capacity; termination must not starve behind status queries.

Pair control/data sockets into one authenticated worker session. Control carries handshake/status/termination/resize/essential lifecycle events; data carries file operations, prepare/exec, stdin and output reads. Preserve existing disconnect policy for existing modes. Backpressure on a live data lane must not block control. Inflight replay shares the same dispatch result; changed meaning conflicts; completed retention eviction cannot authorize another execution. Essential lifecycle events require delivery or reconciliation rather than silent loss.

| Lifetime | Owner | Release evidence |
| --- | --- | --- |
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

## Required changes and rollout

| Recommendation/confidence | Observed cause | Minimum change and alternatives | Cost, validation and rollback |
| --- | --- | --- | --- |
| Required/high | PID-bound authority and Runner handle ownership | Single Runner registration; subordinate registrations only if later needed | Moderate; identity/FD tests; close admission and drain |
| Required/high | Post-spawn slots and pre-prepare approval transition | Prespawn permits, PrepareExec/ExecPrepared and same-attempt CAS | High; cancel/loss races; preserve unknown during rollback |
| Required/high | Empty FD list at adapter; the pinned spawn reaps internally and drops close-on-exec descriptors | Managed launch through `HelperCommand` with Runner-owned pipes or PTY, observe before reap and `spawn_guard` for every other spawn; Codex adapter kept for `off` | Moderate to high; pipe/PTY leak, survivor and escape tests; rotate credentials only after reconciliation |
| Required/high | The default 1 CPU CodeSpace reservation exceeds a 3-CPU hosted runner's 500 mCPU work capacity | Run real-authority cases on the qualification host; hosted runners run fixtures, or record cases as `not_run` | Low; record where each case ran |
| Required/high | Serial dispatch/shared writer/aggregate buffers | Bound the complete processing and transport path | High; saturation, latency and lock traces; drain sessions |
| Required/high | Per-directory authority lock | Canonical normal path plus parent-budget candidates | Moderate; alias/concurrent-start tests; preserve journals |
| Required/high | Strict contract/journal decoding | Real old/new fixtures and explicit migration | Moderate; reject incompatible downgrade; no stale snapshot after new writes |
| Strongly Recommended/high | Whole-file allocations exceed response window | Explicit size bound or streaming read/hash, separate follow-up | Moderate; large/changing-file memory tests and API review |

The current DG-1 sequence does not modify CodeSpace runtime. File-size/concurrency bounds must constrain qualification until the separate file-memory work is complete. Do not claim arbitrary-file protection.

Adoption order: DG1-C12 → CSRG-C01/C02 pin/registration → C03/C04 preparation/approval → C05/C06 control/replay → C07/C08 product qualification → P1R-C01–C06 recovery. Add DGL-C01–C06 for actual Linux enforcement. Keep upstream qualification and pin unchanged; record DevGuard results separately. [Readiness](consumer-readiness.md), [verification](verification.md) and [delivery](pr-delivery.md) determine permissible rollout.
