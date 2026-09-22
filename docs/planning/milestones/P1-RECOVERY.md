# P1-RECOVERY — Gateway recovery with a surviving independent Runner

Owner: CodeSpace. Baseline implementation: `not-started`; qualification: `not-run`. Entry: CSRG-C08. Completion: Qualify repeated Gateway restart/reconnect while the same independent Runner retains processes, I/O and original deadlines. Preserve existing modes; exclude InProcess and I/O restoration after Runner restart.

All work IDs, commit titles and logical PR labels below are **proposed values**, not future SHAs or GitHub numbers. Module paths describe planned responsibilities until implemented. Each added workspace crate updates the explicit dependency allowlist in the same PR without removing full-graph validation. The ledger owns actual status.

## PR sequence and activation

| Proposed group | Units | Predecessor |
| --- | --- | --- |
| P1R-P1 | P1R-C01, P1R-C02 | CSRG-C08 |
| P1R-P2 | P1R-C03, P1R-C04 | P1R-P1 |
| P1R-P3 | P1R-C05, P1R-C06 | P1R-P2 |

Available regression: CodeSpace `python3 scripts/validate-upstream.py all`, with separate `macos-core dependencies` and actual `linux-isolation` where applicable. Preserve the pinned Codex/toolchain and existing stages; Linux skips on macOS are not Linux evidence. The task-specific qualification commands below are **planned and unavailable until implemented**. Each PR must supply real fixtures, nonzero case counts, logs and cleanup, then update command availability. See [verification](../verification.md).

### P1R-C01 — add an explicit independent Runner lifecycle

- Owner / proposed PR: CodeSpace / P1R-P1. Proposed commit: `feat(recovery): add an explicit independent Runner lifecycle`.
- Problem → behavior: Add operator-selected independent Runner survival while preserving current Gateway-owned kill-on-drop behavior.
- Prerequisites: CSRG-C08. Independent startup/service ownership, protected endpoint and live Runner timeout management.
- Modules / deliverables: Server config/runtime, Runner hello/startup, lifecycle intent/capabilities and shutdown matrix.
- Invariants: Preserve defaults/managed UDS/InProcess; no live recovery claim for the same PID; detach is not new admission.
- Tests (normal / failure / race): Detach/restart; explicit stop and invalid mode rejection; shutdown/disconnect races and Gateway crash during original timeout.
- Completion evidence: Process/Runner survival and termination per intent, capability gates and unchanged deadline.
- Rollback: Close new-mode startup and drain Runners; do not move live work into Gateway-owned mode implicitly.
- Handoff: P1R-C02 receives owner/epoch/deadline persistence requirements; P1 alone is not reconnect qualification.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py recovery-lifecycle`.

### P1R-C02 — persist process identity separately from patch operations

- Owner / proposed PR: CodeSpace / P1R-P1. Proposed commit: `feat(recovery): persist process identity separately from patch operations`.
- Problem → behavior: Identify the same execution after Gateway memory loss without pretending stored PID/DB recreates handles.
- Prerequisites: P1R-C01. Live Runner remains I/O owner; define process-store single writer/transaction owner.
- Modules / deliverables: Separate process store/schema/migrations: process ID, epoch, boot/start identity, attempt/lease, deadline, output cursor and terminal reason.
- Invariants: Keep patch operations separate; no secrets or executable replay queue; a row alone proves neither execution nor termination.
- Tests (normal / failure / race): Normal queries/terminal records; torn write, unknown schema and PID reuse; crashes between spawn/persist/exit and concurrent queries.
- Completion evidence: Schema fixtures and durable/unknown results at each crash point, unchanged patch ledger and no sensitive content.
- Rollback: Stop new mode and preserve compatible readers; no historical snapshot over new process records.
- Handoff: P1R-C03 gets identity/epoch APIs; P1R-C04 gets approval/workspace linkage.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py recovery-store`.

### P1R-C03 — fence stale Gateways during authenticated reconnect

- Owner / proposed PR: CodeSpace / P1R-P2. Proposed commit: `feat(recovery): fence stale Gateways during authenticated reconnect`.
- Problem → behavior: Prevent simultaneous mutating Gateways by authenticating reconnect and issuing a fenced epoch/control owner.
- Prerequisites: P1R-C02. Protected endpoint/credentials, durable epoch ownership and atomic control/data pairing.
- Modules / deliverables: Runner handshake/mutation fence, reconnect/backoff, observer rules and stale-controller error.
- Invariants: Reject stale exec/stdin/resize/terminate; define permitted reads; reconnect needs no fresh workload budget.
- Tests (normal / failure / race): Valid handoff; wrong credentials/reused sessions; simultaneous Gateways, delayed mutations, half-open lane and lost epoch-update reply.
- Completion evidence: One mutation authority per epoch, same process/attempt and zero duplicate execution/reservation.
- Rollback: Close new control grants, use last valid owner to stop/drain and never decrement epochs.
- Handoff: Ship with P1R-C04 reconciliation barrier before enabling new mutations.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py recovery-fence`.

### P1R-C04 — reconcile workspace approvals and resource leases

- Owner / proposed PR: CodeSpace / P1R-P2. Proposed commit: `feat(recovery): reconcile workspace approvals and resource leases`.
- Problem → behavior: Recover workspace occupancy and approval/resource state without overlapping execution due to lost Gateway memory.
- Prerequisites: P1R-C03. P1R-C02 records, CSRG attempt/approval mapping and existing execution query; no new authority admission needed.
- Modules / deliverables: Workspace occupancy restore, approval resume reconciliation, lease observation and recovery barrier.
- Invariants: Preserve live workspace ownership; consumed/unknown never presumed queued; existing handle query/stop remains during authority outage.
- Tests (normal / failure / race): Running/terminal reconnect; stale approval, unavailable authority and mismatched records; exit/event/new workspace request races.
- Completion evidence: Per-execution workspace/approval/lease table, isolated mismatch and mutation barrier, zero duplicate execution.
- Rollback: Stop new mutations and observe/terminate/reconcile existing work; no bulk busy-state clearing.
- Handoff: P1R-C05 receives output cursor, terminal/unknown states and unresolved leases.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py recovery-reconcile`.

### P1R-C05 — preserve uncertainty and output gaps after owner loss

- Owner / proposed PR: CodeSpace / P1R-P3. Proposed commit: `feat(recovery): preserve uncertainty and output gaps after owner loss`.
- Problem → behavior: Expose output gaps and uncertainty after owner loss rather than equating lost Runner/host with successful process exit.
- Prerequisites: P1R-C04. Live Runner bounded output/cursor, actual termination observations and boot identity.
- Modules / deliverables: Read/status/terminal reconciliation, gap/retention metadata, Runner-loss runbook and fixtures.
- Invariants: No pipe/PTY restoration from PID alone; no deadline extension or argv replay; lease release alone does not authorize approval reuse.
- Tests (normal / failure / race): Buffered output/terminal query; expired buffer, Runner crash and reboot; exit/reconnect/eviction races and PID reuse.
- Completion evidence: Distinct gap/unknown/terminal, retained charges without scope evidence, original deadline and zero automatic execution.
- Rollback: Close new-mode execution but preserve observation; never mark unresolved records successful in bulk.
- Handoff: P1R-C06 receives restart/loss matrix and explicit output-retention limits.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py recovery-observation`.

### P1R-C06 — qualify repeated Gateway restarts with a live Runner

- Owner / proposed PR: CodeSpace / P1R-P3. Proposed commit: `test(recovery): qualify repeated Gateway restarts with a live Runner`.
- Problem → behavior: Qualify recovery amid workload, approval, output and competing controllers beyond a single reconnect demonstration.
- Prerequisites: P1R-C05. Fixed source/artifact/policy, independent Runner, restart timing/count fixtures and passed CSRG baseline.
- Modules / deliverables: Recovery harness, existing-mode regression, repeated traces and operator rollout/rollback guidance.
- Invariants: Capability covers Gateway restart with surviving Runner only; preserve existing disconnect semantics, timeout and attempt identity.
- Tests (normal / failure / race): Repeated Gateway restart during long pipe/PTY work; malformed state, Runner loss and authority outage; competing Gateway/approval/exit/output races and control/foreground SLO.
- Completion evidence: Same execution reconnected, zero duplicates/stale mutations/extended timeout, explicit gaps and support manifest; 10-minute idle/30-minute load minimum, three repetitions.
- Rollback: Stop rollout, drain independent Runners and return to prior combination; do not claim Runner restart/I/O restoration passed.
- Handoff: Return to the next CodeSpace priority; broader I/O-owner recovery requires separate design and approval.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify-devguard.py recovery`.
