# CS-RG — CodeSpace consumption and control protection

Owner: CodeSpace. Baseline implementation: `not-started`; qualification: `not-run`. Entry: DG1-C12, which is complete; release `0.1.0-5daee5d-b3fa569e` is macOS-qualified and is the pin candidate. Completion: Qualify the pinned client/artifact/wire combination on the head left by the CSRG-C09 backend decision, while preserving authorization, approvals, workspace and PTY contracts. Preparation may precede qualification, but required runtime adoption may not.

All work IDs, commit titles and logical PR labels below are **proposed values**, not future SHAs or GitHub numbers. Module paths describe planned responsibilities until implemented. Each added workspace crate updates the explicit dependency allowlist in the same PR without removing full-graph validation. The ledger owns actual status. [Design revision 1](../../design-revision-1.md) (2026-09-27) added CSRG-C00 and CSRG-C09 and revised the execution layer of the other units.

## PR sequence and activation

| Proposed group | Units | Predecessor |
| --- | --- | --- |
| CSRG-P0 | CSRG-C00 | DG1-C12 |
| CSRG-P1 | CSRG-C01, CSRG-C02 | CSRG-P0 |
| CSRG-P2 | CSRG-C03, CSRG-C04 | CSRG-P1 |
| CSRG-P3 | CSRG-C05, CSRG-C06 | CSRG-P2 |
| CSRG-P4 | CSRG-C07, CSRG-C09 | CSRG-P3 |
| CSRG-P5 | CSRG-C08 | CSRG-P4 |

Available regression: CodeSpace `python3 scripts/validate-upstream.py all`, with separate `macos-core dependencies` and actual `linux-isolation` where applicable. Preserve the pinned Codex/toolchain and existing stages; Linux skips on macOS are not Linux evidence. The task-specific qualification commands below are **planned and unavailable until implemented**. Each PR must supply real fixtures, nonzero case counts, logs and cleanup, then update command availability. See [verification](../verification.md).

## DG-1 consumer interface

The 2026-09-26 review checked this plan against DG-1's implemented consumer interface ([contracts](../../contracts.md)). Every unit below follows that interface:

- **Sessions.** Every frame, including idle waiting, has an absolute 250 ms deadline. An owner opens a fresh session for each step: connect, `Hello`, `Authenticate`, `Register` with its one instance ID, then one request. There is no separate `service-exec` path; the Gateway passes the consumer credential to a UDS worker through `CredentialHandoff`.
- **Launch.** `Admit` records a Prepared attempt with a five-second deadline, and `BeginLaunch` commits it; only its first reply carries the one-time permit. The owner starts `devguard-launch` as its direct child through `HelperCommand`, which carries the permit and a transcript on private descriptors and spawns under `spawn_guard`. `helper_command` and `HelperCommand::spawn` take that guard themselves, so a caller never holds it around them. The transcript reports `failed`, `refused`, `ready` or `exec_failed`.
- **Observe before reap.** The owner detects the root's exit without reaping it (`waitid` with `WNOWAIT`), calls `Observe`, then reaps. Survivors of a root reaped first are permanent tracking loss, and the attempt stays Suspect and charged until a reboot.
- **No helper created.** A lost permit reply, a failed helper spawn, or a helper that exits before READY is reported with `AbandonLaunch`. The authority then releases an unclaimed grant as `NoHelperCreated`, the evidence that the executable never started; the report itself is not that evidence, and it cannot release a claimed grant.
- **Provisioning.** A consumer is operator configuration in `host.toml`: role, generation, credential, instance limit and, for a `control_service` consumer only, a static control reservation. The service reads it at start, and a restart leaves committed attempts Suspect.

## Execution ownership

The pinned high-level Codex spawn cannot carry a DG-1 managed execution unchanged: its pipe and PTY spawn functions reap their child internally and keep only descriptors that are already inheritable. That is a mismatch in the reap-ownership and descriptor-passing contracts, not proof that Codex code is unusable. Every unit below follows the [execution ownership rules](../codespace-integration.md#execution-ownership) of design revision 1:

- **One reaper.** One supervisor object owns each child. On the `required` path nothing else calls `wait`, `try_wait` or `waitpid`; termination, timeout and shutdown send it intents. Legacy `off` backends that reap by themselves (`BackendReaped`) never advertise an unreaped-exit state; the `required` path is `OwnerControlledReap`.
- **One-time launch plan.** A `PreparedExecution` owns the execution identity, meaning, slot, workspace, resource state and original deadline, and its launch plan is consumed once. It is never cloned or rebuilt from argv, and the Drop of a cancelled task is not assumed to return a lease.
- **Spawn protection.** Exactly one party protects each spawn path: `HelperCommand` itself, the common guard or a verified equivalent. The guard covers only descriptor creation, inheritance setup and the spawn.
- **Output.** One CodeSpace collector with bounded retention; unknown loss is never reported as `output_lost=false`.
- **Backend decision.** The legacy `off` backends may stay for initial compatibility. CSRG-C09 decides, before CSRG-C08, between integrating them and keeping a limited compatibility backend.

Do not describe this work as adding a small PTY opener. Judge it by the races and failure paths it must verify, not by code size.

## Work units

### CSRG-C00 — verify the managed execution boundary

- Owner / proposed PR: CodeSpace / CSRG-P0. Proposed commit: `test(runner): verify the managed execution boundary`.
- Problem → behavior: Before building on the common execution contract, prove at the current pin with a minimal implementation that a managed helper can run on a PTY, that its descriptors stay safe, and that the owner controls observation and reaping after the root exits; inventory every spawn and reap path.
- Prerequisites: DG1-C12 (complete). CodeSpace confirmation baseline `b6e7ed2`, the Codex pin `6b9826e` and DG-1's `HelperCommand` from the pin candidate's source.
- Modules / deliverables: An ownership table for the Gateway and the UDS Runner that names, for every child-creation and reaping path, the owner and the message the path sends: pipe and PTY spawns, exit watching, timeout, `request_kill`, workspace termination, shutdown, backend Drop, task cancellation, error cleanup, patch and sandbox helpers, auxiliary commands, worker creation and test helpers. Also a minimal managed PTY through `HelperCommand`; a spawn-guard and descriptor fitness report covering the legacy Codex PTY and duplicate client versions; end-to-end deadline propagation through connect, `Authenticate`, `Register` and `Admit`; and measured pre-reap `Observe` durations against the proposed 1 s budget.
- Invariants: One reaper per child, and no reaping call outside the owner; no caller holds `spawn_guard` around `helper_command` or `HelperCommand::spawn`; no private descriptor reaches an unrelated child or the payload; a blocking call wrapped in a timeout stays tracked while it runs; verification code is absorbed into later common tests or deleted and never kept as a fourth product backend; no product behavior changes.
- Tests (normal / failure / race): A managed helper on a PTY with initial size, resize, controlling terminal, session and group, and EOF; a root that exits at once and one with surviving descendants; `Observe` failure and timeout followed by the reap; unrelated concurrent spawns while the helper is created; a setup failure cleaning up the child, master, slave and private descriptors; the helper's QoS re-execution keeping its descriptors and PID meaning; jobserver descriptors kept.
- Completion evidence: The ownership table, payload descriptor inventories, a verdict for each path (safe, needs change or unsupported), measured `Observe` and deadline traces, and the findings handed to CSRG-C03 and CSRG-C09.
- Rollback: Revert the test-only code; nothing is activated.
- Handoff: CSRG-C01 starts after the report. CSRG-C03 receives the ownership table and the proven PTY and descriptor composition; CSRG-C09 receives the legacy-path findings.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py boundary`.

### CSRG-C01 — consume a qualified DevGuard client revision

- Owner / proposed PR: CodeSpace / CSRG-P1. Proposed commit: `feat(resources): consume a qualified DevGuard client revision`.
- Problem → behavior: Consume a qualified full DevGuard source revision through a small adapter, distinct from a planning-document reference.
- Prerequisites: CSRG-C00 and DG1-C12 (complete). Selected artifact/license/wire/capability combination, platform matrix and CI executors. The candidate is release `0.1.0-5daee5d-b3fa569e`, built from source `5daee5d`; its consumer crates are identical at `395315d`.
- Modules / deliverables: Planned crates/resource-client, provenance/lock and scripts/upstream_dependencies.py boundary extensions, recording client and helper provenance separately; the `devguard-launch` path resolved from the installed current release; `devguard-daemon`'s `test-fixtures` feature as a dev-only dependency for isolated test authorities.
- Invariants: No CodeSpace/Codex dependency in DevGuard core; the client brings no transitive Codex dependency; authorization stays in CodeSpace; design revision 1 keeps the Codex pin.
- Tests (normal / failure / race): Supported handshake; changed/unsupported pin, missing capability or a helper from another release than the running authority rejected; mixed clients and reconnect replay; runtime and build dependency graphs checked for Codex crates.
- Completion evidence: Full source SHA, binary hashes, license/graph diff with runtime, build and dev dependencies separated, and supported-combination results; fill actual pins only after qualification.
- Rollback: Reconcile live leases before disabling consumption; use compatible clients without automatically installing unsupported artifacts.
- Handoff: CSRG-C02 receives client/error/capability contracts; review pin and settings together.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py client`.

### CSRG-C02 — register the execution owner and expose required policy

- Owner / proposed PR: CodeSpace / CSRG-P1. Proposed commit: `feat(resources): register the execution owner and expose required policy`.
- Problem → behavior: Distinguish configured files from actual runtime consumption through default off and explicit required operation.
- Prerequisites: CSRG-C01. Actual executor authority; an operator-provisioned `codespace` consumer (role `control_service`, generation, private credential file, instance limit and static control reservation) applied while nothing is charged; chosen InProcess/UDS mode.
- Modules / deliverables: Server config/start_runner/runtime, Runner initialization, capabilities/errors, `CredentialHandoff` from Gateway to UDS worker, per-step registered sessions, workspace `resources` settings (profile, requested budget and minimum levels) and the rules for legacy and managed spawns in one process.
- Invariants: One instance per execution owner (Gateway PID for InProcess, worker PID for UDS), re-registered in each bounded session and never by a launcher, so an instance is not a session; one Gateway+Runner reservation; credentials never reach the payload; `required` fails closed where DG-1 reports `ResourcePolicyUnsupported`, including Linux; legacy and managed spawns in one process share one protection object, or that mixed combination is unsupported; preserve public MCP names.
- Tests (normal / failure / race): Both modes and off behavior; required rejects unavailable authority/privilege/capability; resource settings exceeding work capacity refused at load; worker startup/slot races, pipe/PTY credential leaks and mixed legacy/managed spawns in one process.
- Completion evidence: Mode/PID/Principal/reservation mapping, separate requested/supported/applied and shortage/service/unsupported/unknown errors.
- Rollback: Stop registration, reconcile/retire instances and return to compatible settings; required never silently becomes off.
- Handoff: CSRG-C03 receives the authenticated Runner and private handoff; P1 alone does not qualify runtime use.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py registration`.

### CSRG-C03 — supervise executions with pre-spawn slots and one reaper

- Owner / proposed PR: CodeSpace / CSRG-P2. Proposed commit: `feat(runner): supervise executions with pre-spawn slots and one reaper`.
- Problem → behavior: Move post-spawn process limits before spawn, and consolidate every reaping path into one supervisor per execution whose preparation guards own slot/resource/workspace cleanup.
- Prerequisites: CSRG-C02. Existing authorization and workspace FIFO acquired before Runner slot and DevGuard preparation; the CSRG-C00 ownership table.
- Modules / deliverables: runner/process.rs execution coordinator and supervisor, backend capabilities (`BackendReaped` or `OwnerControlledReap`), Runner trait/wire, PrepareExec (`Admit`)/cancel DTO and expiry guards; managed launches through `HelperCommand` on the Runner-owned Unix transport (pipes or PTY), outside the Codex spawn adapter.
- Invariants: Zero spawn without a slot; fixed process/attempt meaning, with an execution digest over argv, cwd, environment, tty, workspace and policy; slot, resource lease and completed-output retention are distinct; prepare before consuming approval; one reaper per child, with timeout, `request_kill`, workspace termination and shutdown sending intents; observe before reap (`waitid` with `WNOWAIT`, `Observe` within its budget, then reap); a backend never advertises a capability it cannot guarantee; every other spawn follows the spawn-protection rules.
- Tests (normal / failure / race): Normal prepare/execute; occupied slots, budget refusal and timeout; survivors after root exit stay tracked; concurrent ninth request, cancel/expiry and lost-reply guard races; waiter cancellation, backend Drop, `ECHILD` and double-reap prevention; timeout, terminate and shutdown racing the exit.
- Completion evidence: Actual spawn/slot counts, the final ownership table with one reaper per path, correct unstarted cleanup and retained committed/unknown accounting traces.
- Rollback: Close preparation and cancel unstarted reservations; observe committed scopes before reclaim, never transfer live work to old post-spawn checking.
- Handoff: Complete CSRG-C04 approval/commit handling in the same P2 before activation.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py prepare`.

### CSRG-C04 — dispatch prepared attempts without replaying uncertain work

- Owner / proposed PR: CodeSpace / CSRG-P2. Proposed commit: `feat(approvals): dispatch prepared attempts without replaying uncertain work`.
- Problem → behavior: Link durable attempts to approval resume so refusal does not consume a hold, lost replies do not replay execution and each launch plan is consumed once.
- Prerequisites: CSRG-C03. Prepared slot/lease, phase-specific client result and approval/workspace identity.
- Modules / deliverables: server/mcp.rs, store/approvals.rs, ExecPrepared wire (`BeginLaunch` and the helper transcript), approval handling for each transcript phase, non-start mapping and migration fixtures.
- Invariants: mark_resuming follows preparation; the launch plan is consumed once and never rebuilt from argv; reuse only matching confirmed non-start/NoHelperCreated through CAS; READY/confirmed differ from executable success; a lost `Admit` reply is queried or cancelled for the same attempt, and a lost `BeginLaunch` reply keeps that attempt and reports `AbandonLaunch` only when no helper can exist; `AbandonLaunch` itself is not non-start evidence, only the authority's `NoHelperCreated` release is; a claimed grant is settled only by its scope; `exec_failed` after READY never reuses the approval automatically; exit codes 125–127 never identify the helper's phase.
- Tests (normal / failure / race): Normal confirmed execution; refusal keeps queued, post-READY exec failure separate; lost `Admit` and `BeginLaunch` replies and each transcript phase; duplicate resume, lost commit and cancel races yield zero duplicate spawn.
- Completion evidence: Approval/process/attempt mapping, preserved unknown, reuse matrix, DB version fixtures and a mapping of every DG-1 error code, with pressure refusals named.
- Rollback: Stop new resumes and reconcile dispatching/unknown before compatible rollback; never reset approvals in bulk to queued.
- Handoff: CSRG-C05 receives explicit confirmed/uncertain execution and control responses.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py approval`.

### CSRG-C05 — reserve transport and dispatch capacity for control

- Owner / proposed PR: CodeSpace / CSRG-P3. Proposed commit: `feat(runner): reserve transport and dispatch capacity for control`.
- Problem → behavior: Prevent serial dispatch/shared writers from allowing slow stdin or large responses to block status/termination.
- Prerequisites: CSRG-C04. One authenticated Runner session, bounded executor and static control capacity.
- Modules / deliverables: runner/wire.rs and UDS client, paired control/data lanes, independent dispatch budgets/writers and callback/lock audit.
- Invariants: Preserve process_status and terminate_process; shared locks/callbacks cannot propagate data stalls; spawn and pre-reap `Observe` latency never propagates into the control path; existing disconnect policies remain.
- Tests (normal / failure / race): Normal lane pairing; wrong pairing/half-open/stopped readers; output saturation, slow stdin, dispatch saturation, delayed callbacks, slow spawns and slow `Observe` while controlling processes.
- Completion evidence: Queue/count/byte ownership limits, latency traces and shared-lock hold times.
- Rollback: Close new sessions and clean using current mode policy; do not silently introduce independent-Runner survival here.
- Handoff: Activate lanes only with CSRG-C06 complete buffer/replay/lifecycle bounds.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py control-lanes`.

### CSRG-C06 — bound inflight replay buffers and lifecycle delivery

- Owner / proposed PR: CodeSpace / CSRG-P3. Proposed commit: `feat(runner): bound inflight replay buffers and lifecycle delivery`.
- Problem → behavior: Extend completed-only replay to inflight single-flight, bound total queued/response bytes and retention, and account output loss, including loss inside a bridge.
- Prerequisites: CSRG-C05. Request/attempt ownership, control capacity and completed-output policy.
- Modules / deliverables: Replay registry, one output collector with a total byte bound, bounded stdin/output writers, lifecycle events and workspace release callbacks, and any protocol change needed to report unknown output loss.
- Invariants: Zero duplicate inflight dispatch; preserve 256 KiB output rings; eviction never authorizes fresh execution; output TTL is not lease lifetime; intentional retention loss and unknown transport loss are distinct, and unknown loss is never reported as `output_lost=false`; Suspect attempts (escape, tracking loss) stay visible as charged until a reboot.
- Tests (normal / failure / race): Normal replay/exit events; slow readers, large responses, bridge lag, tail retention after exit and over-limit refusal; retry/completion/eviction/disconnect races without slot/lease leaks.
- Completion evidence: Every queue count/bytes/TTL, memory peaks, repeated same result and event reconciliation; control succeeds under saturation.
- Rollback: Close new execution and drain inflight entries while preserving terminal identities; cache deletion is not tombstone retirement.
- Handoff: CSRG-C07 receives full transport/lifecycle invariants and worst-case fixtures.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py replay-bounds`.

### CSRG-C07 — exercise pipe PTY and Runner failure parity

- Owner / proposed PR: CodeSpace / CSRG-P4. Proposed commit: `test(resources): exercise pipe PTY and Runner failure parity`.
- Problem → behavior: Prove equivalent meaning across `off`/`required`, pipe/PTY and InProcess/UDS, and for mixed legacy and managed execution in one process, instead of generalizing one successful path.
- Prerequisites: CSRG-C06. The installed qualified authority/helper on the qualification host, isolated fixtures for each mode from DG-1's `test-fixtures` (synthetic probe; not OS evidence) and expected disconnect matrix.
- Modules / deliverables: Runner/server/PTY integration and fault suites with payload credential-FD inspection.
- Invariants: Preserve authorization, workspace FIFO, approvals, resize and exit; use Runner handles during authority outage; never replay unknown execution; success of each path alone does not verify spawn and descriptor protection.
- Tests (normal / failure / race): Every combination of resource mode, transport and Runner mode, and mixed execution in one process; helper/daemon failure, unsupported requirements, surviving descendants and a workload that leaves its group (`setsid` or a background group); resume/cancel/replay/exit/disconnect races.
- Completion evidence: Per-mode case counts, explicit unsupported/not-run reasons, zero FD leaks and duplicate executions, and where each case ran. The default 1 CPU control reservation leaves a 3-CPU hosted runner no work capacity, so hosted macOS runs fixtures or records `not_run`.
- Rollback: Remove failed modes from support and block required activation; do not summarize partial results as full product qualification.
- Handoff: Only passed mode/fixture/pin combinations enter CSRG-C09 and then CSRG-C08.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py parity`.

### CSRG-C08 — qualify the pinned DevGuard consumer combination

- Owner / proposed PR: CodeSpace / CSRG-P5. Proposed commit: `test(qualification): qualify the pinned DevGuard consumer combination`.
- Problem → behavior: Qualify actual CodeSpace approvals/replay/control separately from standalone DG-1.
- Prerequisites: CSRG-C09 and DG1-C12 (complete). Exact client/artifact/wire/policy/host on the qualification host, local MCP/foreground fixtures.
- Modules / deliverables: Consumer qualification, existing upstream regressions and operator adoption/rollback guide.
- Invariants: Qualify only the final implementation, pin and artifact combination left by CSRG-C09; preserve the Codex pin/gates of that combination; 10-minute idle plus at least 30-minute load, three repetitions; separate remote RTT; exclude arbitrary-file-size protection; measure the installed qualified release, not a hosted runner.
- Tests (normal / failure / race): Development workload and MCP; authority outage/pressure/large output/slow stdin; saturated replay/approval/termination races and foreground SLO.
- Completion evidence: Raw samples/all required SLOs, exact-head upstream results and support manifest, each recorded with the CodeSpace and client source SHAs, daemon/helper hashes, Codex SHA, backend, wire/capability, policy, host and the reasons for any `not_run`; macOS success is not Linux enforcement.
- Rollback: Close new required consumption, drain and return to prior qualified combination; retain failures and pin candidates.
- Handoff: Hand off to P1R-C01 and DGL-C01 without claiming Gateway recovery is already available.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py codespace-macos`.

### CSRG-C09 — decide and converge the legacy execution backends

- Owner / proposed PR: CodeSpace / CSRG-P4. Proposed commit: `refactor(runner): decide and converge the legacy execution backends`.
- Problem → behavior: A common interface over lasting duplicate backends leaves two lifecycles and two spawn protections to maintain. Before final qualification, integrate the legacy `off` backends or keep a limited compatibility backend on recorded grounds.
- Prerequisites: CSRG-C07. The C07 parity and fault results, the CSRG-C00 legacy-path findings and the maintenance measurements.
- Modules / deliverables: Either (A) removal of the replaced backend's code, branches, test fixtures and dependencies for each platform and transport concerned, or (B) a decision record with the reason, the remaining platform/transport/mode, the common and duplicated parts, the unsupported capabilities, the revisit point and the removal criteria. Both outcomes include the maintenance measurements: spawn entry points, reap sites, lifecycle implementations, duplicated unsafe and descriptor code, bridges/queues/tasks, runtime and build dependency changes, test duplication and coverage, and the upstream change surface.
- Invariants: Neither "existing code" nor "parity passed" decides alone; keeping a branch bears the same burden of proof as replacing it; a document stating "reviewed" does not complete the unit; unmeasured effort is marked as an estimate; mixed legacy and managed use stays unsupported without spawn-protection evidence.
- Tests (normal / failure / race): The affected C07 parity rerun after any code change; checks that removed paths leave no calls, fixtures or dependencies; for a kept backend, refusal of the capabilities it does not provide.
- Completion evidence: The decision with its measurements, the resulting head for CSRG-C08 and the C07 reruns.
- Rollback: Revert the convergence and rerun the affected C07 parity before CSRG-C08; reverse a kept-backend decision only through a new decision record.
- Handoff: CSRG-C08 qualifies only the head this unit leaves.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py backends`, with the affected `python3 scripts/qualify-devguard.py parity` cases rerun.
