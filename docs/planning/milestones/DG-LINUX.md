# DG-LINUX — actual Linux enforced protection

Owner: DevGuard + CodeSpace. Baseline implementation: `not-started`; qualification: `not-run`. Entry: CSRG-C08. Completion: Required for overall product completion. Qualify actual controller/ancestor/privilege conditions, complete sandbox/proxy scope and consumer control. This does not add a prerequisite to macOS P1 recovery.

All work IDs, commit titles and logical PR labels below are **proposed values**, not future SHAs or GitHub numbers. Module paths describe planned responsibilities until implemented. Each added workspace crate updates the explicit dependency allowlist in the same PR without removing full-graph validation. The ledger owns actual status.

## PR sequence and activation

| Proposed group | Units | Predecessor |
| --- | --- | --- |
| DGL-P1 | DGL-C01, DGL-C02 | CSRG-C08 |
| DGL-P2 | DGL-C03, DGL-C04 | DGL-P1 |
| DGL-P3 | DGL-C05, DGL-C06 | DGL-P2 |

Available regression: `python3 scripts/validate.py --offline` (Rust 1.95.0 contract regression; fake backends do not prove native behavior). The task-specific qualification commands below are **planned and unavailable until implemented**. Each PR must supply real fixtures, nonzero case counts, logs and cleanup, then update command availability. See [verification](../verification.md).

DGL groups are logical boundaries: CodeSpace hooks, if needed, use linked PRs and a joint immutable-head manifest. Existing CodeSpace linux-isolation tests do not substitute for new DevGuard kernel controls. Unavailable delegated runners remain not_run/inconclusive.

### DGL-C01 — probe delegated controllers and effective capacity

- Owner / proposed PR: DevGuard / DGL-P1. Proposed commit: `feat(linux): probe delegated controllers and effective capacity`.
- Problem → behavior: Derive actual capacity from delegated controllers and ancestors instead of overallocating physical host totals.
- Prerequisites: CSRG-C08. Actual cgroup v2, delegated writable subtree, trustworthy executor/boot identity.
- Modules / deliverables: Planned platform-linux probes and receipts for controllers/ancestors/cpuset/memory/pids; extend explicit workspace graph checks.
- Invariants: Separate requested/supported/applied; never allocate beyond ancestors or interpret probe failure as unlimited.
- Tests (normal / failure / race): Normal delegation; missing controllers, read-only and denied writes; ancestor changes/container migration during admission.
- Completion evidence: Mount/controller/ancestor readback, effective-capacity arithmetic and unsupported matrix.
- Rollback: Close Linux admission and preserve/observe scopes; never expand ancestors automatically.
- Handoff: DGL-C02 receives verified capacity and controllable roots.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py linux-probes`.

### DGL-C02 — separate control and workload resource scopes

- Owner / proposed PR: DevGuard / DGL-P1. Proposed commit: `feat(linux): separate control and workload resource scopes`.
- Problem → behavior: Separate control/workload cgroups and apply aggregate/per-lease resource limits within ancestor capacity.
- Prerequisites: DGL-C01. Static control reservation, controller delegation and scope creation rights.
- Modules / deliverables: Linux apply/scope backend, CPU/memory/pids plans/readback and capability matrix.
- Invariants: cpu.max is not an exclusive core; memory.min is not physical preallocation; no kernel claim without actual application.
- Tests (normal / failure / race): Normal limits/refusals; partial writes or inadequate ancestor; concurrent leases/policy updates/scope cleanup.
- Completion evidence: Resource-specific request/application/effect evidence and blocked execution after failure, separately observed control scope.
- Rollback: Remove only empty unstarted scopes; drain live scopes before policy rollback instead of stripping their limits.
- Handoff: DGL-C03 receives the required pre-payload containment boundary.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py linux-controls`.

### DGL-C03 — contain launch helpers sandbox and proxies before exec

- Owner / proposed PR: DevGuard (CodeSpace linked implementation/verification as needed) / DGL-P2. Proposed commit: `feat(linux): contain launch helpers sandbox and proxies before exec`.
- Problem → behavior: Include launcher, sandbox and proxies in scope before payload rather than moving only the final PID.
- Prerequisites: DGL-C02. DG1 launch fence and inspected CodeSpace process/linux-sandbox/proxy spawn paths.
- Modules / deliverables: Linux launch binding order, linked CodeSpace adapter hooks if needed, membership and credential-FD traces.
- Invariants: All managed scope prepared before user executable; no credential leakage; sandbox policy remains CodeSpace-owned.
- Tests (normal / failure / race): Normal pipe/PTY/proxy; bind/sandbox failures; delayed helper/cancel/fork races with no out-of-scope payload.
- Completion evidence: Helper/sandbox/proxy/payload membership, READY versus exec and exact linked heads/artifacts.
- Rollback: Stop new launches and reconcile all scopes via DGL-C04; do not roll back hooks while losing live ownership.
- Handoff: Ship DGL-C04 cleanup in the same logical P2; cross-repository changes use linked PRs.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py linux-launch`.

### DGL-C04 — reconcile descendant termination and resource failures

- Owner / proposed PR: DevGuard / DGL-P2. Proposed commit: `feat(linux): reconcile descendant termination and resource failures`.
- Problem → behavior: Reclaim after actual descendant termination rather than root exit or an OOM notification alone.
- Prerequisites: DGL-C03. Exact scope identity, membership/termination rights and retryable observation.
- Modules / deliverables: Linux reconciliation, OOM/termination reasons, permission failures and shutdown fixtures.
- Invariants: Require cgroup populated=0 and managed-helper exit; tracking loss stays sticky; OOM is not successful exit or safe replay.
- Tests (normal / failure / race): Whole-scope exit; surviving descendants, OOM, denied signal/read; restart/termination/fork races and PID reuse.
- Completion evidence: Charges retained while descendants live, actual release reasons/timing/permission failures and zero duplicate allocation.
- Rollback: Close new admission and preserve uncertain charges; absent cgroup directory alone is not termination evidence.
- Handoff: DGL-C05 receives fault points and release/unknown expectations.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py linux-reconcile`.

### DGL-C05 — exercise real controllers under pressure and faults

- Owner / proposed PR: DevGuard / DGL-P3. Proposed commit: `test(linux): exercise real controllers under pressure and faults`.
- Problem → behavior: Obtain actual kernel/ancestor/permission evidence beyond fake-backend tests.
- Prerequisites: DGL-C04. Bounded Linux test host, control reserve, fixed fixtures and explicit delegation/reboot fault scope.
- Modules / deliverables: Linux qualification harness, control/workload metrics, faults and executor capability detection.
- Invariants: Unavailable controllers cannot pass; no out-of-scope ancestor changes; preserve raw evidence.
- Tests (normal / failure / race): CPU/memory/pids pressure; OOM, revoked delegation and daemon crash; concurrent consumers/cancel/restart/controller changes.
- Completion evidence: Kernel/OS/ancestor/rights/clock/source/artifact, resource effects, control survival and accounting consistency.
- Rollback: Stop fixtures, drain managed scopes and verify restored test configuration; evidence is protected from sweep.
- Handoff: DGL-C06 receives only the actual passed environment/resource levels and limitations.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py linux-faults`.

### DGL-C06 — qualify Linux consumer and control protection

- Owner / proposed PR: DevGuard (CodeSpace linked implementation/verification as needed) / DGL-P3. Proposed commit: `test(qualification): qualify Linux consumer and control protection`.
- Problem → behavior: Qualify the real Runner/MCP/sandbox combination separately from standalone kernel control.
- Prerequisites: DGL-C05 and CSRG-C08. Fixed client/artifact/wire/policy on actual Linux executor; local control separated from remote RTT.
- Modules / deliverables: Combination manifest, existing CodeSpace Linux/upstream results, integrated report/support matrix.
- Invariants: Linux is required for overall product completion; macOS evidence cannot substitute; recovery capability depends separately on P1R.
- Tests (normal / failure / race): Pipe/PTY/control/sandbox/proxy; authority/permissions/OOM failures; saturated replay/approval/termination and three SLO repetitions.
- Completion evidence: Both exact repository heads, controller readback, raw latency and 10-minute idle/at least 30-minute load three times; no skipped required supported scope.
- Rollback: Stop new required use and drain before qualified rollback; never silently downgrade kernel requirements to accounting.
- Handoff: Feed overall product completion and DGA-C07 Linux executor prerequisites; absolute responsiveness remains measured, not inferred.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py linux-consumer`.
