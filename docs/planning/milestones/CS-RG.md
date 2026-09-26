# CS-RG — CodeSpace consumption and control protection

Owner: CodeSpace. Baseline implementation: `not-started`; qualification: `not-run`. Entry: DG1-C12, which is complete; release `0.1.0-5daee5d-b3fa569e` is macOS-qualified and is the pin candidate. Completion: Qualify the pinned client/artifact/wire combination while preserving authorization, approvals, workspace and PTY contracts. Preparation may precede qualification, but required runtime adoption may not.

All work IDs, commit titles and logical PR labels below are **proposed values**, not future SHAs or GitHub numbers. Module paths describe planned responsibilities until implemented. Each added workspace crate updates the explicit dependency allowlist in the same PR without removing full-graph validation. The ledger owns actual status.

## PR sequence and activation

| Proposed group | Units | Predecessor |
| --- | --- | --- |
| CSRG-P1 | CSRG-C01, CSRG-C02 | DG1-C12 |
| CSRG-P2 | CSRG-C03, CSRG-C04 | CSRG-P1 |
| CSRG-P3 | CSRG-C05, CSRG-C06 | CSRG-P2 |
| CSRG-P4 | CSRG-C07, CSRG-C08 | CSRG-P3 |

Available regression: CodeSpace `python3 scripts/validate-upstream.py all`, with separate `macos-core dependencies` and actual `linux-isolation` where applicable. Preserve the pinned Codex/toolchain and existing stages; Linux skips on macOS are not Linux evidence. The task-specific qualification commands below are **planned and unavailable until implemented**. Each PR must supply real fixtures, nonzero case counts, logs and cleanup, then update command availability. See [verification](../verification.md).

## DG-1 consumer interface

The 2026-09-26 review checked this plan against DG-1's implemented consumer interface ([contracts](../../contracts.md)). Every unit below follows that interface:

- **Sessions.** Every frame, including idle waiting, has an absolute 250 ms deadline. An owner opens a fresh session for each step: connect, `Hello`, `Authenticate`, `Register` with its one instance ID, then one request. There is no separate `service-exec` path; the Gateway passes the consumer credential to a UDS worker through `CredentialHandoff`.
- **Launch.** `Admit` records a Prepared attempt with a five-second deadline, and `BeginLaunch` commits it; only its first reply carries the one-time permit. The owner starts `devguard-launch` as its direct child through `HelperCommand`, which carries the permit and a transcript on private descriptors and spawns under `spawn_guard`. The transcript reports `failed`, `refused`, `ready` or `exec_failed`.
- **Observe before reap.** The owner detects the root's exit without reaping it (`waitid` with `WNOWAIT`), calls `Observe`, then reaps. Survivors of a root reaped first are permanent tracking loss, and the attempt stays Suspect and charged until a reboot.
- **No helper created.** A lost permit reply, a failed helper spawn, or a helper that exits before READY is reported with `AbandonLaunch`. That releases an unclaimed grant as `NoHelperCreated`, the evidence that the executable never started.
- **Provisioning.** A consumer is operator configuration in `host.toml`: role, generation, credential, instance limit and, for a `control_service` consumer only, a static control reservation. The service reads it at start, and a restart leaves committed attempts Suspect.

The pinned Codex pipe and PTY spawn functions reap their child internally and keep only descriptors that are already inheritable. Managed launches therefore cannot go through them; the Codex adapter remains for `off`.

### CSRG-C01 — consume a qualified DevGuard client revision

- Owner / proposed PR: CodeSpace / CSRG-P1. Proposed commit: `feat(resources): consume a qualified DevGuard client revision`.
- Problem → behavior: Consume a qualified full DevGuard source revision through a small adapter, distinct from a planning-document reference.
- Prerequisites: DG1-C12 (complete). Selected artifact/license/wire/capability combination, platform matrix and CI executors. The candidate is release `0.1.0-5daee5d-b3fa569e`, built from source `5daee5d`; its consumer crates are identical at `395315d`.
- Modules / deliverables: Planned crates/resource-client, provenance/lock and scripts/upstream_dependencies.py boundary extensions; the `devguard-launch` path resolved from the installed current release; `devguard-daemon`'s `test-fixtures` feature as a dev-only dependency for isolated test authorities.
- Invariants: No CodeSpace/Codex dependency in DevGuard core; authorization stays in CodeSpace; preserve Codex pin.
- Tests (normal / failure / race): Supported handshake; changed/unsupported pin, missing capability or a helper from another release than the running authority rejected; mixed clients and reconnect replay.
- Completion evidence: Full source SHA, binary hashes, license/graph diff and supported-combination results; fill actual pins only after qualification.
- Rollback: Reconcile live leases before disabling consumption; use compatible clients without automatically installing unsupported artifacts.
- Handoff: CSRG-C02 receives client/error/capability contracts; review pin and settings together.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py client`.

### CSRG-C02 — register the execution owner and expose required policy

- Owner / proposed PR: CodeSpace / CSRG-P1. Proposed commit: `feat(resources): register the execution owner and expose required policy`.
- Problem → behavior: Distinguish configured files from actual runtime consumption through default off and explicit required operation.
- Prerequisites: CSRG-C01. Actual executor authority; an operator-provisioned `codespace` consumer (role `control_service`, generation, private credential file, instance limit and static control reservation) applied while nothing is charged; chosen InProcess/UDS mode.
- Modules / deliverables: Server config/start_runner/runtime, Runner initialization, capabilities/errors, `CredentialHandoff` from Gateway to UDS worker, per-step registered sessions and workspace `resources` settings (profile, requested budget and minimum levels).
- Invariants: One instance per execution owner (Gateway PID for InProcess, worker PID for UDS), re-registered in each bounded session and never by a launcher; one Gateway+Runner reservation; credentials never reach the payload; `required` fails closed where DG-1 reports `ResourcePolicyUnsupported`, including Linux; preserve public MCP names.
- Tests (normal / failure / race): Both modes and off behavior; required rejects unavailable authority/privilege/capability; resource settings exceeding work capacity refused at load; worker startup/slot races and pipe/PTY credential leaks.
- Completion evidence: Mode/PID/Principal/reservation mapping, separate requested/supported/applied and shortage/service/unsupported/unknown errors.
- Rollback: Stop registration, reconcile/retire instances and return to compatible settings; required never silently becomes off.
- Handoff: CSRG-C03 receives the authenticated Runner and private handoff; P1 alone does not qualify runtime use.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py registration`.

### CSRG-C03 — reserve process slots before resource preparation

- Owner / proposed PR: CodeSpace / CSRG-P2. Proposed commit: `feat(runner): reserve process slots before resource preparation`.
- Problem → behavior: Move post-spawn process limits before spawn and make preparation guards own slot/resource/workspace cleanup.
- Prerequisites: CSRG-C02. Existing authorization and workspace FIFO acquired before Runner slot and DevGuard preparation.
- Modules / deliverables: runner/process.rs, Runner trait/wire, PrepareExec (`Admit`)/cancel DTO and expiry guards; managed launches through `HelperCommand` with Runner-owned pipes or PTY, outside the Codex spawn adapter.
- Invariants: Zero spawn without a slot; fixed process/attempt meaning, with an execution digest over argv, cwd, environment, tty, workspace and policy; slot, resource lease and completed-output retention are distinct; prepare before consuming approval; observe before reap (`waitid` with `WNOWAIT`, `Observe`, then reap); every other Runner spawn holds `spawn_guard` or is close-on-exec.
- Tests (normal / failure / race): Normal prepare/execute; occupied slots, budget refusal and timeout; survivors after root exit stay tracked; concurrent ninth request, cancel/expiry and lost-reply guard races.
- Completion evidence: Actual spawn/slot counts, correct unstarted cleanup and retained committed/unknown accounting traces.
- Rollback: Close preparation and cancel unstarted reservations; observe committed scopes before reclaim, never transfer live work to old post-spawn checking.
- Handoff: Complete CSRG-C04 approval/commit handling in the same P2 before activation.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py prepare`.

### CSRG-C04 — dispatch prepared attempts without replaying uncertain work

- Owner / proposed PR: CodeSpace / CSRG-P2. Proposed commit: `feat(approvals): dispatch prepared attempts without replaying uncertain work`.
- Problem → behavior: Link durable attempts to approval resume so refusal does not consume a hold and lost replies do not replay execution.
- Prerequisites: CSRG-C03. Prepared slot/lease, phase-specific client result and approval/workspace identity.
- Modules / deliverables: server/mcp.rs, store/approvals.rs, ExecPrepared wire (`BeginLaunch` and the helper transcript), non-start mapping and migration fixtures.
- Invariants: mark_resuming follows preparation; reuse only matching confirmed non-start/NoHelperCreated through CAS; READY/confirmed differ from executable success; a lost permit reply or a helper that ends before READY is reported with `AbandonLaunch`, and a claimed grant is settled only by its scope.
- Tests (normal / failure / race): Normal confirmed execution; refusal keeps queued, post-READY exec failure separate; duplicate resume, lost commit and cancel races yield zero duplicate spawn.
- Completion evidence: Approval/process/attempt mapping, preserved unknown, reuse matrix, DB version fixtures and a mapping of every DG-1 error code, with pressure refusals named.
- Rollback: Stop new resumes and reconcile dispatching/unknown before compatible rollback; never reset approvals in bulk to queued.
- Handoff: CSRG-C05 receives explicit confirmed/uncertain execution and control responses.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py approval`.

### CSRG-C05 — reserve transport and dispatch capacity for control

- Owner / proposed PR: CodeSpace / CSRG-P3. Proposed commit: `feat(runner): reserve transport and dispatch capacity for control`.
- Problem → behavior: Prevent serial dispatch/shared writers from allowing slow stdin or large responses to block status/termination.
- Prerequisites: CSRG-C04. One authenticated Runner session, bounded executor and static control capacity.
- Modules / deliverables: runner/wire.rs and UDS client, paired control/data lanes, independent dispatch budgets/writers and callback/lock audit.
- Invariants: Preserve process_status and terminate_process; shared locks/callbacks cannot propagate data stalls; existing disconnect policies remain.
- Tests (normal / failure / race): Normal lane pairing; wrong pairing/half-open/stopped readers; output saturation, slow stdin, dispatch saturation and delayed callbacks while controlling processes.
- Completion evidence: Queue/count/byte ownership limits, latency traces and shared-lock hold times.
- Rollback: Close new sessions and clean using current mode policy; do not silently introduce independent-Runner survival here.
- Handoff: Activate lanes only with CSRG-C06 complete buffer/replay/lifecycle bounds.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py control-lanes`.

### CSRG-C06 — bound inflight replay buffers and lifecycle delivery

- Owner / proposed PR: CodeSpace / CSRG-P3. Proposed commit: `feat(runner): bound inflight replay buffers and lifecycle delivery`.
- Problem → behavior: Extend completed-only replay to inflight single-flight and bound total queued/response bytes and retention.
- Prerequisites: CSRG-C05. Request/attempt ownership, control capacity and completed-output policy.
- Modules / deliverables: Replay registry, bounded stdin/output writers, lifecycle events and workspace release callbacks.
- Invariants: Zero duplicate inflight dispatch; preserve 256 KiB output rings; eviction never authorizes fresh execution; output TTL is not lease lifetime; Suspect attempts (escape, tracking loss) stay visible as charged until a reboot.
- Tests (normal / failure / race): Normal replay/exit events; slow readers, large responses and over-limit refusal; retry/completion/eviction/disconnect races without slot/lease leaks.
- Completion evidence: Every queue count/bytes/TTL, memory peaks, repeated same result and event reconciliation; control succeeds under saturation.
- Rollback: Close new execution and drain inflight entries while preserving terminal identities; cache deletion is not tombstone retirement.
- Handoff: CSRG-C07 receives full transport/lifecycle invariants and worst-case fixtures.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py replay-bounds`.

### CSRG-C07 — exercise pipe PTY and Runner failure parity

- Owner / proposed PR: CodeSpace / CSRG-P4. Proposed commit: `test(resources): exercise pipe PTY and Runner failure parity`.
- Problem → behavior: Prove equivalent meaning in pipe/PTY crossed with InProcess/UDS instead of generalizing one successful path.
- Prerequisites: CSRG-C06. The installed qualified authority/helper on the qualification host, isolated four-mode fixtures from DG-1's `test-fixtures` (synthetic probe; not OS evidence) and expected disconnect matrix.
- Modules / deliverables: Runner/server/PTY integration and fault suites with payload credential-FD inspection.
- Invariants: Preserve authorization, workspace FIFO, approvals, resize and exit; use Runner handles during authority outage; never replay unknown execution.
- Tests (normal / failure / race): All four combinations; helper/daemon failure, unsupported requirements, surviving descendants and a workload that leaves its group (`setsid` or a background group); resume/cancel/replay/exit/disconnect races.
- Completion evidence: Per-mode case counts, explicit unsupported/not-run reasons, zero FD leaks and duplicate executions, and where each case ran. The default 1 CPU control reservation leaves a 3-CPU hosted runner no work capacity, so hosted macOS runs fixtures or records `not_run`.
- Rollback: Remove failed modes from support and block required activation; do not summarize partial results as full product qualification.
- Handoff: Only passed mode/fixture/pin combinations enter CSRG-C08.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py parity`.

### CSRG-C08 — qualify the pinned DevGuard consumer combination

- Owner / proposed PR: CodeSpace / CSRG-P4. Proposed commit: `test(qualification): qualify the pinned DevGuard consumer combination`.
- Problem → behavior: Qualify actual CodeSpace approvals/replay/control separately from standalone DG-1.
- Prerequisites: CSRG-C07 and DG1-C12 (complete). Exact client/artifact/wire/policy/host on the qualification host, local MCP/foreground fixtures.
- Modules / deliverables: Consumer qualification, existing upstream regressions and operator adoption/rollback guide.
- Invariants: Preserve Codex pin/gates; 10-minute idle plus at least 30-minute load, three repetitions; separate remote RTT; exclude arbitrary-file-size protection; measure the installed qualified release, not a hosted runner.
- Tests (normal / failure / race): Development workload and MCP; authority outage/pressure/large output/slow stdin; saturated replay/approval/termination races and foreground SLO.
- Completion evidence: Raw samples/all required SLOs, exact-head upstream results and support manifest; macOS success is not Linux enforcement.
- Rollback: Close new required consumption, drain and return to prior qualified combination; retain failures and pin candidates.
- Handoff: Hand off to P1R-C01 and DGL-C01 without claiming Gateway recovery is already available.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py codespace-macos`.
