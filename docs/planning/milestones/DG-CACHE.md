# DG-CACHE — safe registered-cache reclamation

Owner: DevGuard. Baseline implementation: `not-started`; qualification: `not-run`. Entry: DG1-C12. Completion: Qualify exclusive active-use/reclaim, zero protected deletion and actual free-space change. This is not a P1-RECOVERY prerequisite.

All work IDs, commit titles and logical PR labels below are **proposed values**, not future SHAs or GitHub numbers. Module paths describe planned responsibilities until implemented. Each added workspace crate updates the explicit dependency allowlist in the same PR without removing full-graph validation. The ledger owns actual status.

## PR sequence and activation

| Proposed group | Units | Predecessor |
| --- | --- | --- |
| DGC-P1 | DGC-C01, DGC-C02 | DG1-C12 |
| DGC-P2 | DGC-C03, DGC-C04 | DGC-P1 |
| DGC-P3 | DGC-C05, DGC-C06 | DGC-P2 |

Available regression: `python3 scripts/validate.py --offline` (Rust 1.95.0 contract regression; fake backends do not prove native behavior). The task-specific qualification commands below are **planned and unavailable until implemented**. Each PR must supply real fixtures, nonzero case counts, logs and cleanup, then update command availability. See [verification](../verification.md).

### DGC-C01 — register reclaim roots and protected data classes

- Owner / proposed PR: DevGuard / DGC-P1. Proposed commit: `feat(cache): register reclaim roots and protected data classes`.
- Problem → behavior: Require registered roots/classes so names resembling caches cannot authorize deleting state or evidence.
- Prerequisites: DG1-C12. Operator roots, owner/filesystem identity, protection policy and confirmed rebuildability.
- Modules / deliverables: Cache registry/classifier and read-only preview; protect Git/journal/evidence/reference/recovery, environments such as .venv/node_modules and CodeSpace target/upstream-reports/local.
- Invariants: No scope expansion by autodiscovery; reject symlink/root replacement; environments are not disposable caches.
- Tests (normal / failure / race): Normal registration/preview; overlapping protection, permission and symlink rejection; root inode replacement during scan.
- Completion evidence: Identity/classification/protection matrix and zero protected candidates, with no deletion.
- Rollback: Disable registration/discard previews; no file mutation.
- Handoff: DGC-C02 receives exact identities/protection; deletion remains closed.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py cache-roots`.

### DGC-C02 — exclude active users from reclamation

- Owner / proposed PR: DevGuard / DGC-P1. Proposed commit: `feat(cache): exclude active users from reclamation`.
- Problem → behavior: Mutually exclude active use and reclaim to close the gap between idle inspection and deletion.
- Prerequisites: DGC-C01. Actual use-lease acquisition at cache entrypoints; untracked users exclude automatic reclaim.
- Modules / deliverables: Use/reclaim lease API, generation/fence, restart reconciliation and adapter hooks.
- Invariants: Active/suspect users prevent reclaim; TTL/disconnect alone does not prove inactivity.
- Tests (normal / failure / race): Reclaim after completed use; unregistered user/lease failure/restart suspect; simultaneous acquire and mark.
- Completion evidence: Single winner and explicit loser, zero active deletion and durable restart fence.
- Rollback: Stop reclaim and reconcile users; do not remove locks to bypass state.
- Handoff: DGC-C03 receives reclaim authority/protection snapshot and rechecks identity before rename.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py cache-leases`.

### DGC-C03 — mark entries and move them atomically to local trash

- Owner / proposed PR: DevGuard / DGC-P2. Proposed commit: `feat(cache): mark entries and move them atomically to local trash`.
- Problem → behavior: Durably mark and atomically rename within one filesystem after exclusive reclaim and identity checks.
- Prerequisites: DGC-C02. Valid reclaim lease, fresh protection checks, same-filesystem trash and journal durability.
- Modules / deliverables: Mark transaction, identity recheck, rename, trash ledger and recovery states.
- Invariants: No EXDEV copy/delete fallback or symlink traversal; trash bytes are still occupied, not reclaimed.
- Tests (normal / failure / race): Normal mark/rename; cross-device/permission/protection failures; reacquire/root replacement/crash-before-rename races.
- Completion evidence: Old/new identity and ledger phases, partial-failure recovery and unchanged protected paths.
- Rollback: Before sweep restore only matching identity to an empty destination; isolate conflicts without overwrite.
- Handoff: Ship DGC-C04 resumable sweep in the same P2 before enabling deletion.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py cache-mark`.

### DGC-C04 — resume bounded trash sweeps with durable accounting

- Owner / proposed PR: DevGuard / DGC-P2. Proposed commit: `feat(cache): resume bounded trash sweeps with durable accounting`.
- Problem → behavior: Resume bounded trash sweeps after crash/pressure without rediscovering protected paths or overstating free space.
- Prerequisites: DGC-C03. Completed rename ledger, exact trash root, finite batches and cancellation.
- Modules / deliverables: Sweeper/restart scan, progress/errors and actual filesystem free-space observations.
- Invariants: Delete only authorized trash identities; incomplete deletion is not free capacity; never traverse symlink targets.
- Tests (normal / failure / race): Normal/partial-resume sweep; denied/busy files and pressure interruption; cancel/restart/new-trash races.
- Completion evidence: Resume cursor, remaining occupancy, errors, measured free space and unchanged protected data across retries.
- Rollback: Stop sweep immediately; already deleted rebuildable cache has rebuild rather than undo guarantees; operational data was never eligible.
- Handoff: DGC-C05 receives batch cost, trash occupancy and actual free-space measurements.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py cache-sweep`.

### DGC-C05 — budget maintenance and shared filesystem capacity

- Owner / proposed PR: DevGuard / DGC-P3. Proposed commit: `feat(cache): budget maintenance and shared filesystem capacity`.
- Problem → behavior: Budget maintenance load and avoid counting shared backing capacity more than once.
- Prerequisites: DGC-C04. Filesystem/container identity, watermarks, observation rights and low-priority maintenance budget.
- Modules / deliverables: Maintenance admission/batch/pause policy, shared-pool deduplication and capacity receipts.
- Invariants: Do not sum roots sharing APFS/container storage; du is not physical free-space gain; preserve control reserve.
- Tests (normal / failure / race): Low-load reclaim; failed disk probe or insufficient parent lease; concurrent sweeps/external writes/snapshot changes.
- Completion evidence: Pool identity, before/after free space, separate logical deleted bytes and physical change, control latency during maintenance.
- Rollback: Stop maintenance and preserve trash; never weaken protected classes to resolve space shortage.
- Handoff: DGC-C06 receives per-pool fixtures, measurement limitations and concurrent external-change context.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py cache-capacity`.

### DGC-C06 — qualify deletion safety and measured space recovery

- Owner / proposed PR: DevGuard / DGC-P3. Proposed commit: `test(cache): qualify deletion safety and measured space recovery`.
- Problem → behavior: Demonstrate deletion safety and actual space recovery as distinct outcomes.
- Prerequisites: DGC-C05. Dedicated test roots, protected sentinels, actual filesystem and cold/warm fixtures; never erase operational caches for tests.
- Modules / deliverables: Qualification harness, sentinel hashes, lease-race transcripts, before/after capacity and foreground reports.
- Invariants: Zero protected deletion; evidence/recovery survive; uncertain gain is inconclusive for effect independently of safety.
- Tests (normal / failure / race): Reclaim/rebuild; interruption, permissions, symlink/root replacement; active-use/mark/sweep/restart races and load SLO.
- Completion evidence: Unchanged protected hashes, authorized identity/lease evidence, measured physical change/external factors and raw control/foreground; 10-minute idle/30-minute load minimum three times.
- Rollback: Disable automatic reclaim and use protected artifacts; rebuild eligible caches without promising deletion undo.
- Handoff: Roll out only qualified root-class/filesystem combinations; other repositories require individual classification review.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py cache`.
