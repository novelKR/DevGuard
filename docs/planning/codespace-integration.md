# CodeSpace integration specification

This is a future consumer specification grounded in CodeSpace `e94d21475643608ad2a466256fb57266b86faa47`. It does not install a runtime pin. DG-1 remains independent; CS-RG begins adoption after DG1-C12. Preserve Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`, existing authorization, approval, workspace, PTY and shutdown behavior.

## Source mapping

All paths in this table refer to the fixed [CodeSpace source tree](https://github.com/novelKR/CodeSpace/tree/e94d21475643608ad2a466256fb57266b86faa47).

| Path | Observed baseline | Planned boundary |
| --- | --- | --- |
| `crates/server/src/mcp.rs` | Authorization/workspace acquisition precedes Runner; approval becomes resuming before execution | Prepare before approval consumption, CSRG-C03/C04 |
| `crates/store/src/approvals.rs` | Only selected occupancy failures return to queued | Same-attempt CAS and confirmed non-start evidence, CSRG-C04 |
| `crates/server/src/runtime.rs` | Owns managed worker and current shutdown behavior | Single execution-owner registration, CSRG-C02; separate opt-in recovery, P1R-C01 |
| `crates/runner/src/process.rs` | Owns child/PTY/I/O; pipe and PTY slot checks follow spawn | Acquire slots before spawn, maintain distinct lifetimes, CSRG-C03/C04 |
| `crates/runner/src/wire.rs` | Wire 6, 16 MiB frames, 32 completed replay entries, serial dispatch/shared writer | Control/data lanes, inflight single-flight and aggregate bounds, CSRG-C05/C06 |
| `crates/pty/src/lib.rs` | Supplies empty inherited-FD list | Private selected credential handoff, CSRG-C02/C07 |
| `crates/runner/src/files.rs` | Whole-file read/hash before response window | Separate memory-bound improvement; bound qualification fixtures meanwhile |
| `scripts/validate-upstream.py` | Fixed target/report paths; all and macos-core separate; platform skips | Preserve existing gates and add consumer qualification, CSRG-C08/DGL-C06 |

The [pinned Codex PTY](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/utils/pty/src/pty.rs) already supports selected inherited FDs. Use that API without updating the pin solely for this purpose.

## Registration and startup

The execution owner registers exactly once: the internal Runner uses the Gateway PID in InProcess, and the worker uses its own PID in UDS. OS UID/PID/boot/start observations establish identity. Gateway cannot declare the worker's identity. One static control reservation includes Gateway and Runner costs. The SDK `service-exec` path prepares credentials and startup; the selected Runner registers. [ADR-001](decisions.md#adr-001--one-registration-by-the-execution-owner) refines the historical generic service-exec description.

Pipe and PTY startup retain credential FDs only through the necessary trusted helper stages, then close them before user exec. Test actual payload FD lists and failure cleanup. Distinguish required jobserver FDs from credentials; do not put secrets in argv, environment, MCP input, logs or journals.

Missing resource settings retain `off`. Operators explicitly choose `required`; unavailable authentication, capacity or capability cannot silently fall back to off. CSRG-C02 owns the eventual schema and wire change. The initial wire target is 7 relative to baseline 6; use the next free version if another change has already consumed it.

## Execution and approval sequence

1. Apply existing authorization, workspace/network policy, argv and approval checks.
2. Acquire the existing workspace FIFO reservation for the process ID.
3. `PrepareExec` obtains a Runner execution slot **before spawn**, then requests admission for immutable attempt meaning.
4. If preparation cannot finish within the approved 250 ms budget, return promptly, cancel unstarted preparation and release its guards. Do not add a long resource queue inside CodeSpace. CLI's explicit wait is separate.
5. After preparation succeeds, compare-and-set the same approval attempt to `resuming`. Failure to persist that transition cancels preparation.
6. `ExecPrepared` atomically consumes the token within its original five-second lifetime. An expired token never silently creates another lease.
7. Runner owns the managed helper handle. The helper verifies scope/policy/binding/authorization, reports READY, then attempts user exec.
8. Reconcile execution slot, workspace and resource lease against their respective completion evidence. Retain completed output independently.

The server-minted process ID maps one-to-one to the attempt. This does not add public persistent idempotency for arbitrary repeated MCP calls.

| Observation | Meaning | Approval/retry rule |
| --- | --- | --- |
| Preparation refused | No user code started | Keep hold queued; later explicit new attempt is possible |
| Confirmed failure before helper creation | `known_not_started` with matching evidence | Reuse only through same-attempt approval CAS |
| `confirmed` | A managed helper is tracked | Application may still be pending; executable success is separate |
| Helper READY | Pre-exec boundaries completed | Not evidence of executable success or final exit |
| Executable fails after READY | Failure of an already managed execution | Do not unconditionally restore queued approval |
| Launch/authorization reply lost | Execution cannot be ruled out | Preserve unknown; no automatic re-execution |
| Resource lease released | Accounting reconciled with scope/noncreation/previous-boot evidence | Alone does not prove approval is reusable |

CSRG-C04 provides nullable `resume_attempt_id`, migration and old/new fixtures. Existing NULL/uncertain resuming rows are not presumed unstarted. Keep execution outside the patch-operations ledger and do not expose it through `operation_status`.

## Errors, control and lifetime

| Error/state | Meaning | Handling |
| --- | --- | --- |
| `RESOURCE_UNAVAILABLE` | Insufficient budget or preparation expired | Explicit retry only with non-start evidence |
| `RESOURCE_CONTROL_UNAVAILABLE` | Authority connection, recovery or persistence unavailable | Refuse new execution; retain control of existing handles |
| `RESOURCE_POLICY_UNSUPPORTED` | Required resource capability unsupported | Operator/support-combination correction |
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

Root reap does not repair tracking loss. Measure control request-to-response separately from actual scope termination and remote RTT.

## Recovery mode

Preserve all current mode termination policies. In the new independent Runner mode, distinguish normal shutdown, explicit service stop, planned detach and unexpected Gateway disconnect. Stop drains/terminates and reconciles; detach/disconnect lets the live Runner maintain original deadlines and I/O. InProcess is excluded. Runner/host loss remains uncertain until observed; records cannot recreate pipes or PTYs.

An authenticated Gateway obtains an epoch/fence and reconciles workspace occupancy, approval and DevGuard leases before mutations reopen. Stale Gateway mutations fail. Reconnecting to an existing execution needs no new workload budget. Restoring I/O after Runner restart is separate future work.

## Required changes and rollout

| Recommendation/confidence | Observed cause | Minimum change and alternatives | Cost, validation and rollback |
| --- | --- | --- | --- |
| Required/high | PID-bound authority and Runner handle ownership | Single Runner registration; subordinate registrations only if later needed | Moderate; identity/FD tests; close admission and drain |
| Required/high | Post-spawn slots and pre-prepare approval transition | Prespawn permits, PrepareExec/ExecPrepared and same-attempt CAS | High; cancel/loss races; preserve unknown during rollback |
| Required/high | Empty FD list at adapter | Use existing pinned private FD API | Moderate; pipe/PTY leak tests; rotate credentials only after reconciliation |
| Required/high | Serial dispatch/shared writer/aggregate buffers | Bound the complete processing and transport path | High; saturation, latency and lock traces; drain sessions |
| Required/high | Per-directory authority lock | Canonical normal path plus parent-budget candidates | Moderate; alias/concurrent-start tests; preserve journals |
| Required/high | Strict contract/journal decoding | Real old/new fixtures and explicit migration | Moderate; reject incompatible downgrade; no stale snapshot after new writes |
| Strongly Recommended/high | Whole-file allocations exceed response window | Explicit size bound or streaming read/hash, separate follow-up | Moderate; large/changing-file memory tests and API review |

The current DG-1 sequence does not modify CodeSpace runtime. File-size/concurrency bounds must constrain qualification until the separate file-memory work is complete. Do not claim arbitrary-file protection.

Adoption order: DG1-C12 → CSRG-C01/C02 pin/registration → C03/C04 preparation/approval → C05/C06 control/replay → C07/C08 product qualification → P1R-C01–C06 recovery. Add DGL-C01–C06 for actual Linux enforcement. Keep upstream qualification and pin unchanged; record DevGuard results separately. [Readiness](consumer-readiness.md), [verification](verification.md) and [delivery](pr-delivery.md) determine permissible rollout.
