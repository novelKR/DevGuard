# DG-ADAPTERS — additional tools and actual executors

Owner: DevGuard. Baseline implementation: `not-started`; qualification: `not-run`. Entry: DG1-C12. Completion: Qualify tool/version/option/FD semantics and actual child/executor lifetime. Not a P1 prerequisite. P1–P3 share DG-1 prerequisites; P4 additionally requires their qualified combinations and DGL-C06 for Linux enforcement.

All work IDs, commit titles and logical PR labels below are **proposed values**, not future SHAs or GitHub numbers. Module paths describe planned responsibilities until implemented. Each added workspace crate updates the explicit dependency allowlist in the same PR without removing full-graph validation. The ledger owns actual status.

## PR sequence and activation

| Proposed group | Units | Predecessor |
| --- | --- | --- |
| DGA-P1 | DGA-C01, DGA-C02 | DG1-C12 |
| DGA-P2 | DGA-C03, DGA-C04 | DG1-C12 |
| DGA-P3 | DGA-C05, DGA-C06 | DG1-C12 |
| DGA-P4 | DGA-C07, DGA-C08 | DGA-P1, DGA-P2, DGA-P3; DGL-C06 where Linux enforcement is required |

Available regression: `python3 scripts/validate.py --offline` (Rust 1.95.0 contract regression; fake backends do not prove native behavior). The task-specific qualification commands below are **planned and unavailable until implemented**. Each PR must supply real fixtures, nonzero case counts, logs and cleanup, then update command availability. See [verification](../verification.md).

### DGA-C01 — translate supported Python and pytest workloads

- Owner / proposed PR: DevGuard / DGA-P1. Proposed commit: `feat(adapters): translate supported Python and pytest workloads`.
- Problem → behavior: Translate only supported Python/pytest parallel controls in an explicit adapter instead of injecting generic environment guesses.
- Prerequisites: DG1-C12. Pinned tool/plugin inventory, active interpreter/environment, parent lease and explicit adapter selection.
- Modules / deliverables: Python adapter, argv/env diff receipt, supported options and real fixtures.
- Invariants: Preserve interpreter/venv/cwd/test selection/exit; no pluginless worker-limit claim; never GC environments.
- Tests (normal / failure / race): Normal interpreter/pytest; missing plugin, version mismatch and option conflict; concurrent consumers and workers starting during parent cancel.
- Completion evidence: Equivalent test selection/results, justified parallelism and child-budget mapping.
- Rollback: Disable adapter and explicitly choose qualified generic use; do not silently modify environment/options.
- Handoff: Ship DGA-C02 error/nesting tests before listing support.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py python-transform`.

### DGA-C02 — verify Python nesting and unsupported combinations

- Owner / proposed PR: DevGuard / DGA-P1. Proposed commit: `test(adapters): verify Python nesting and unsupported combinations`.
- Problem → behavior: Verify nested Python runners cannot create unchecked independent pools beyond simple option tests.
- Prerequisites: DGA-C01. Pinned interpreter/pytest/plugins, nested fixtures and signal observation.
- Modules / deliverables: Transformation regression, actual worker counts, nested parent-budget tests and unsupported diagnostics.
- Invariants: Report absent limits honestly as unsupported/accounted; preserve explicit-flag precedence/rejection.
- Tests (normal / failure / race): Sequential/nested work; invalid flags/missing plugins/environment; child spawn/cancel/concurrent nesting without duplicated budgets.
- Completion evidence: Same selected tests/exit, worker peaks/lease sums, protected environment identity and version matrix.
- Rollback: Remove failed combinations and reject new work; observe then clean existing children.
- Handoff: Qualified Python combinations become DGA-C07 executor candidates.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py python-adapter`.

### DGA-C03 — translate supported Node Jest and Bun controls

- Owner / proposed PR: DevGuard / DGA-P2. Proposed commit: `feat(adapters): translate supported Node Jest and Bun controls`.
- Problem → behavior: Translate Node/Jest/Bun worker/heap controls by actual supported version.
- Prerequisites: DG1-C12. Pinned runtime/runner/lock, parent lease and explicit option/parser contracts.
- Modules / deliverables: JS adapters, worker/heap plans, argv/env receipts and version-specific fixtures.
- Invariants: Heap is not total RSS/process memory; never blindly pass Node flags to Bun; preserve scripts/selection/lockfiles.
- Tests (normal / failure / race): Each supported tool; unknown version/conflicting flags/unsupported features; concurrent runners spawning extra children.
- Completion evidence: Applied options and verification method per tool, equivalent results and separate requested/observed worker/heap values.
- Rollback: Disable affected support and explicitly select generic use; no package/lock rewriting.
- Handoff: Ship DGA-C04 actual child/memory tests with the transformation.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py javascript-transform`.

### DGA-C04 — distinguish heap settings from process scope limits

- Owner / proposed PR: DevGuard / DGA-P2. Proposed commit: `test(adapters): distinguish heap settings from process scope limits`.
- Problem → behavior: Prevent successful heap flags being reported as memory containment of the full descendant scope.
- Prerequisites: DGA-C03. Child fixtures, distinct heap/RSS measurements and actual OS scope capabilities.
- Modules / deliverables: JS integration, peak memory/worker counts and supported-limit guidance.
- Invariants: Preserve per-resource requested/supported/applied; children cannot issue full-host budgets; reject unsupported hard requirements.
- Tests (normal / failure / race): Worker/heap success; native allocations, child memory and ignored flags; output pressure/cancel/child-exit races.
- Completion evidence: Per-runtime heap/full-scope observations, descendant termination/reclaim and explicitly untested versions.
- Rollback: Withdraw failed claims/combinations, observe/drain live scopes; no silent hard-to-advisory downgrade.
- Handoff: Pass only qualified JS combinations and real limits to DGA-C07.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py javascript-adapter`.

### DGA-C05 — coordinate build tools through inherited jobservers

- Owner / proposed PR: DevGuard / DGA-P3. Proposed commit: `feat(adapters): coordinate build tools through inherited jobservers`.
- Problem → behavior: Coordinate Cargo/make/ninja token ownership instead of issuing independent nested parallelism budgets.
- Prerequisites: DG1-C12 and DG1-C08 jobserver contract. Supported tool versions, inherited FDs and parent lease.
- Modules / deliverables: make/ninja adapters, jobserver bridge, token/FD lifetime and explicit jobs policy.
- Invariants: Preserve options/MAKEFLAGS/required FDs; no unsupported jobserver claim; nested aggregate stays bounded.
- Tests (normal / failure / race): Each supported build tool; invalid/closed FDs, conflicting jobs and unsupported versions; nested Cargo/make with wait/cancel races.
- Completion evidence: Actual inherited FDs/tokens, identical targets/argv meaning and worker peaks within parent budget.
- Rollback: Close new bridge work, return borrowed tokens and reconcile children; do not replace or close the parent jobserver.
- Handoff: Ship DGA-C06 nested/failure cleanup in the same P3.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py build-transform`.

### DGA-C06 — preserve nested budgets file descriptors and build options

- Owner / proposed PR: DevGuard / DGA-P3. Proposed commit: `test(adapters): preserve nested budgets file descriptors and build options`.
- Problem → behavior: Verify token/FD/option preservation through failure and cancellation, not just successful builds.
- Prerequisites: DGA-C05. Actual nested tool graph, FD fixtures and parent/child lifecycle observation.
- Modules / deliverables: Nested suite, token fault/accounting fixtures, option preservation and version matrix.
- Invariants: Close credentials before payload while preserving jobserver FDs only where needed; no second host budget.
- Tests (normal / failure / race): Equivalent output/exit; child crash, closed FDs and signals; wait/return/parent-cancel races and repeated runs.
- Completion evidence: Before/after token counts, FD inventories, maximum workers and lease tree; zero leaks after repetition.
- Rollback: Exclude failed graphs, clean while parent lives and return tokens; never auto-replay commands.
- Handoff: DGA-C07 receives qualified nested graphs and FD/budget contracts.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py build-adapter`.

### DGA-C07 — bind budgets to actual VM and container executors

- Owner / proposed PR: DevGuard / DGA-P4. Proposed commit: `feat(adapters): bind budgets to actual VM and container executors`.
- Problem → behavior: Bind budgets to actual VM/container executors and avoid confusing local Docker CLI with workload location.
- Prerequisites: DGA-C02, DGA-C04, DGA-C06; DGL-C06 additionally required for Linux enforced combinations. Executor API/privilege/stable identity and guest-parent scope relation.
- Modules / deliverables: Executor binding, host/guest capacity provenance, control mapping and supported remote topologies.
- Invariants: No summing host and guest as independent full-host capacity; use actual executor authority; local observations cannot prove remote capabilities.
- Tests (normal / failure / race): Local/remote identity; missing privilege/identity/capability; endpoint changes, concurrent VMs and parent-cancel races.
- Completion evidence: CLI/executor/host/guest/scope mapping, capacity source, per-resource levels and rejected unsupported topology.
- Rollback: Close executor launch and stop/reconcile through actual API; killing CLI does not release workload accounting.
- Handoff: Ship DGA-C08 actual lifetime verification; binding alone grants no operational qualification.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py executor-binding`.

### DGA-C08 — qualify executor lifetime and enforced resource scopes

- Owner / proposed PR: DevGuard / DGA-P4. Proposed commit: `test(adapters): qualify executor lifetime and enforced resource scopes`.
- Problem → behavior: Distinguish CLI exit from actual container/VM workload termination after disconnect or owner loss.
- Prerequisites: DGA-C07. Supported executor, actual Linux qualification where applicable, observation/independent-stop API and fixed fixtures.
- Modules / deliverables: Executor fault suites, topology manifest, termination/unknown evidence and adapter regressions.
- Invariants: No release on disconnect/CLI exit alone; require actual termination; no duplicated host/guest accounting or automatic replay.
- Tests (normal / failure / race): Normal limits/lifetime; remote loss, VM pause, CLI crash and lost permissions; cancel/reconnect/stop/descendant races and three load repetitions.
- Completion evidence: Actual executor identity/exit readback and raw scope/latency; per topology 10-minute idle/at least 30-minute load three times, explicit untested conditions.
- Rollback: Reject new use of failed topologies, retain actual observe/stop paths; no silent switch to unenforced execution.
- Handoff: List only measured tool/version/executor combinations; inspect new language/product consumers separately.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py executors`.
