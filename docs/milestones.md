# Delivery milestones

The critical path is **DG-0 → DG-1 → CS-RG → P1-RECOVERY**. The independent local repository lives at `/Volumes/DevData/Projects/IdeaProjects/DevGuard`; CodeSpace owns its integration and process-recovery work.

The [detailed planning index](planning/README.md) owns the 46 proposed commit units and 23 logical PR groups, with entry/exit gates, tests, rollback and handoffs. [Decisions](planning/decisions.md) refine the preserved design; [consumer readiness](planning/consumer-readiness.md) distinguishes contract preparation from operational adoption. Planning documents do not mark future runtime work complete.

| Milestone | Deliverable and gate | Current implementation |
|---|---|---|
| DG-0 | Independent repository, approved design, resource and compatibility contracts, durable state transitions, fake-backend fault tests, reproducible 1.95.0 validation | Implemented; use the exact-source qualification report for validation status |
| DG-1 | Real macOS host probes, daemon and CLI, launch gate, Cargo/generic adapters, parent-budget candidate tests, safe service update/repair, three measured SLO runs per target backend | In progress: C01 configuration/storage boundary; execution and SLO unqualified |
| CS-RG | Full-SHA consumer pin, pre-spawn slots, PrepareExec/ExecPrepared, approval preservation, bounded control/data lanes and replay, InProcess/UDS parity and upstream regression qualification | Not started |
| P1-RECOVERY | Opt-in Gateway restart/reconnection while an independent Runner retains processes and I/O; reconcile workspace, approvals and DevGuard leases | Not started; depends on CS-RG; InProcess and Runner-loss I/O restoration excluded |
| DG-LINUX | Actual Linux controller, ancestor capacity, complete sandbox/proxy scope, control protection and reclaim evidence | Not started; required for product completion |
| DG-CACHE | Registered-root lease/reclaim exclusion, protected artifacts/evidence, interrupted trash sweep and measured physical recovery | Not started; not a P1 prerequisite |
| DG-ADAPTERS | Tool-specific Python, Node/Bun, make/ninja and container/VM verification, without language branches in core | Not started; not a P1 prerequisite |

DG-0's SQLite and fake-backend tests do not qualify a running resource governor. The `devguardd` bootstrap/path/check commands are available at C01. The execution CLI, launch helper, CodeSpace resources settings and Runner wire changes remain future work; see [operations](operations.md). The approved design's examples must not be represented as currently runnable commands.

Record design acceptance, implementation and platform qualification separately. Do not change Linux `not_run` to passed because a fake cgroup test passed on macOS, or call normal Cargo bootstrap self-governed development. The reports from `scripts/validate.py` include those distinctions explicitly.

DG-1 work should proceed in this order:

1. Canonical authority paths, authenticated UDS/client boundaries and consumer credentials (DG1-P1).
2. Real host probes and applied-policy/scope evidence (DG1-P2).
3. Fenced launch together with cancellation and uncertain-scope reconciliation (DG1-P3).
4. Generic/Cargo CLI entrypoints and jobserver coordination (DG1-P4).
5. Protected artifact installation and a user LaunchAgent, then a functionally tested C10-capable parent; begin bounded real self-use immediately at C10 and exercise independent repair at C11 (DG1-P5).
6. Development/self-use SLO qualification and a separately identified stable artifact; only then select a CodeSpace runtime pin (DG1-P6).

The [PR delivery plan](planning/pr-delivery.md) separates the current documentation PRs from later implementation and qualification. Existing `contracts.md` remains the implemented contract; `milestones.json` remains the authoritative milestone state. Follow each milestone's `planning_document` reference for commit and test details.

The source repository is public at [novelKR/DevGuard](https://github.com/novelKR/DevGuard) under Apache-2.0. This source publication does not install a background service or qualify any runtime milestone.
