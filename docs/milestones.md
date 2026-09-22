# Delivery milestones

The critical path is **DG-0 → DG-1 → CS-RG → P1-RECOVERY**. The independent local repository lives at `/Volumes/DevData/Projects/IdeaProjects/DevGuard`; CodeSpace owns its integration and process-recovery work.

| Milestone | Deliverable and gate | Current implementation |
|---|---|---|
| DG-0 | Independent repository, approved design, resource and compatibility contracts, durable state transitions, fake-backend fault tests, reproducible 1.95.0 validation | Implemented; use the exact-source qualification report for validation status |
| DG-1 | Real macOS host probes, daemon and CLI, launch gate, Cargo/generic adapters, parent-budget candidate tests, safe service update/repair, three measured SLO runs per target backend | Not started |
| CS-RG | Full-SHA consumer pin, pre-spawn slots, PrepareExec/ExecPrepared, approval preservation, bounded control/data lanes and replay, InProcess/UDS parity and upstream regression qualification | Not started |
| P1-RECOVERY | CodeSpace process recovery and DevGuard lease reconciliation with separate ownership | Not started; depends on CS-RG |
| DG-LINUX | Actual Linux controller, ancestor capacity, complete sandbox/proxy scope, control protection and reclaim evidence | Not started; required for product completion |
| DG-CACHE | Registered-root lease/reclaim exclusion, protected artifacts/evidence, interrupted trash sweep and measured physical recovery | Not started; not a P1 prerequisite |
| DG-ADAPTERS | Tool-specific Python, Node/Bun, make/ninja and container/VM verification, without language branches in core | Not started; not a P1 prerequisite |

DG-0's SQLite and fake-backend tests do not qualify a running resource governor. The `devguard` CLI, `devguardd`, `devguard-launch`, CodeSpace `resources` configuration and Runner wire 7 remain future work. The approved design's examples must not be represented as currently runnable commands.

Record design acceptance, implementation and platform qualification separately. Do not change Linux `not_run` to passed because a fake cgroup test passed on macOS, or call normal Cargo bootstrap self-governed development. The reports from `scripts/validate.py` include those distinctions explicitly.

DG-1 work should proceed in this order:

1. Real OS evidence providers, canonical state/UDS boundaries and consumer credentials.
2. Narrow client/daemon protocol and launcher gates consuming the tested core.
3. Generic/Cargo CLI adaptation, cancellation and scope reconciliation on the host.
4. Stable-artifact installation and candidate tests inside a parent budget, including failure recovery.
5. Qualification fixtures and raw SLO evidence; only then select a CodeSpace runtime pin.

The source repository is public at [novelKR/DevGuard](https://github.com/novelKR/DevGuard) under Apache-2.0. This source publication does not install a background service or qualify any runtime milestone.
