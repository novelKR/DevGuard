# PR delivery, evidence and cleanup

The authorized execution completes documentation preparation, then DG1-C01–C12 through six sequential implementation PRs. Each PR completes review, current-head checks, normal merge, separate main workflows and local cleanup before the next begins. English PR bodies and authoritative documents have reviewed Korean translations. Logical labels are not future GitHub numbers.

## Documentation history and preparation

| Unit | Repository/branch | Content | Handoff |
| --- | --- | --- | --- |
| DGP-D01 | DevGuard `codex/planning-documents` | Source baselines and registration/recovery decisions | Source/approval preservation → D02 |
| DGP-D02 | Same branch | Seven milestones, 46 units/23 groups, tests/rollback | IDs/dependencies/fields → D03 |
| DGP-D03 | Same branch | Readiness, integration, verification and delivery | Scope/commands/SLO review → D04 |
| DGP-D04 | Same branch | Index, README/roadmap and ledger links | Cross-references/DG-0 regression → PR #1 |
| CSP-D01 | CodeSpace `codex/devguard-planning-links` | Bilingual adoption, Runner registration and recovery scope | Reviewed pair → D02 |
| CSP-D02 | Same branch | Immutable DevGuard revision/PR links and registry | Site build/integrity → PR #65 |

These existing documentation commits are separate from the 46 runtime units. DevGuard started from `d59cbd43d206a9a9281328a946eddf1dc199f710`. CodeSpace PR #65 includes roadmap `fb822fc24c98f6628dce62d33a5cc67275f8ca34` over runtime `e94d21475643608ad2a466256fb57266b86faa47`. Preserve the original CodeSpace checkout, user branch and staged `.codex/config.toml`.

Append preparation changes to [DevGuard #1](https://github.com/novelKR/DevGuard/pull/1); do not rewrite existing commits or immutable links. Preserve `docs/design.ko.md` and its checksum as historical approval, add an English design reference and maintained Korean counterpart, convert planning to English authority and enforce reviewed translation hashes. Update C10 early self-use guidance.

Update [CodeSpace #65](https://github.com/novelKR/CodeSpace/pull/65) to the resulting **actual full documentation SHA**, review both languages and refresh only the relevant registry entry. Do not link an unmerged path on main. A documentation revision is independent of runtime dependency pins.

Validate and merge DevGuard #1 first, verify its push-main workflows, then validate/merge CodeSpace #65 and verify runtime main CI plus the existing documentation publication workflow. Existing checks are not waived because a PR changes documentation. Only after both delivery cycles finish may DG1-P1 begin.

## One implementation PR at a time

Use the persistent DevGuard checkout for repository state and protected evidence. Create one clean task-owned worktree and branch `codex/dg1-p1` through `codex/dg1-p6` from freshly verified main.

1. Record base/head/index, previous merge and post-merge results; read current instructions/contracts and affected call paths.
2. Implement only the group's commits, meaningful tests, English docs and reviewed Korean translations. Update explicit workspace dependency allowlists whenever crates change; retain full-graph validation.
3. Run applicable local checks and review the complete diff for behavior, failure/race paths, compatibility, dependency direction and cleanup. Preserve evidence before publication.
4. Re-query existing PRs to avoid duplication. Submit/update an English body describing concrete behavior, validation, evidence, limitations and rollback. Attach every created PR to the task.
5. Re-query **current full head**, draft/mergeability/review state, all expected checks and repository policy. Results from an older head do not qualify the new head. Fix failures without bypasses or weaker checks.
6. Merge normally using `gh pr merge <actual-number> --merge --match-head-commit <verified-full-sha>` with the appropriate repository. Do not use admin bypass or delete remote branches.
7. Verify MERGED state, merge OID, expected parent ancestry/tree and fetched main. Wait for **separate push-triggered main workflows**; PR CI is not post-merge CI.
8. Copy and verify evidence, confirm no task-owned process uses worktree binaries, clean the completed group's disposable output/worktree/local branch, then start the next group.

If merge or post-merge verification fails, keep the relevant worktree and evidence and resolve the cause before advancing. Do not change review requirements to get a merge through. New material scope/privilege/ownership/compatibility/workload/acceptance changes require a decision; routine details within the approved direction do not.

## Groups and activation boundaries

| Group | Units | Boundary |
| --- | --- | --- |
| DG1-P1 | DG1-C01/C02 | Canonical authority and authenticated bounded transport; readiness closed without host evidence |
| DG1-P2 | DG1-C03/C04 | Actual native probes and observed application/termination evidence |
| DG1-P3 | DG1-C05/C06 | Launch and all cancellation/expiry/uncertain cleanup together |
| DG1-P4 | DG1-C07/C08 | Generic/Cargo entrypoints, command semantics and shared jobserver |
| DG1-P5 | DG1-C09/C10/C11 | Protected installation, parent-budget self-use and independent recovery |
| DG1-P6 | DG1-C12 | Real functional/failure/foreground qualification and measured promotion |

Other logical groups remain future work: CSRG four, P1R three, DGL three, DGC three, DGA four. Cross-repository Linux work may require linked PRs; 23 is a logical grouping, not a promised count of GitHub PR objects. Do not enable partial launch without cleanup, lanes without aggregate bounds or cache rename without restart-safe sweep.

## Bootstrap and early self-use

Until available, perform the minimum necessary bootstrap builds with one Cargo job and one test thread and label them bootstrap. Through C08 use foreground daemons. Before P4 cleanup preserve a functionally tested artifact bundle outside target; it is bootstrap/recovery material, not an SLO release.

C09 establishes manifests, protected release/recovery paths and the current user's LaunchAgent. Services must execute protected release artifacts, never worktree target binaries. Reopen/reconcile state on restart; missing/corrupt journals fail closed. Explicit bootstrap/repair exceptions are documented, never ordinary unmanaged fallback.

C10 first validates the new parent-budget mechanism, freezes a parent **containing that capability**, then immediately runs a separate candidate through it. Candidate capacity is at most its parent lease, with isolated state/socket/credentials/cache and stable-launcher mediation of actual workloads. It cannot create a second normal budget or read normal control-service credentials. Govern subsequent applicable builds/tests and retain real admission/launch/reconciliation receipts. C11 exercises crash, failed upgrade, drain timeout and repair without candidate admission. C12 qualifies under the reference and promotes only the measured combination.

## Cleanup and evidence preservation

Before removal, inventory exact task-owned paths, sizes and active use. Copy reports, raw measurements, manifests and relevant logs to protected storage outside the worktree and verify hashes. Preserve installed functional/recovery artifacts, credentials, operational journals, approved source documents, shared toolchains and Cargo downloads.

Remove the completed PR's regenerable target and temporary docs build output. Remove only clean, task-owned worktrees and local branches proven merged; never force-remove unknown files or unmerged work. Keep remote branches. `.local` may contain toolchains and recovery artifacts and is not a disposable directory as a whole.

After PR #65's full cycle, clean only its task-owned documentation worktree/branch. Leave the original CodeSpace checkout/index and user-owned branch untouched. Record `du` directory sizes separately from before/after filesystem free space; APFS logical deletion is not measured physical recovery.

## PR body, rollback and completion

Use a body file with real newlines. Include scope/base/head, behavior and invariants, source/client/artifact/wire compatibility, commands/toolchains/results, report locations and CI links/events, dependencies, limitations and rollback. Do not publish secrets, raw private payloads or memory citations.

Documentation rollback appends a revert/new revision and updates paired links/hashes; never erase an already referenced commit by force push. Runtime rollback follows the task-specific procedure: close new admission, observe/drain live scopes, reconcile journal/attempt state and select a compatible artifact. Turning settings off does not settle leases or descendants. After new writes, never restore a stale journal snapshot. Repair must work independently of broken candidate admission.

Completion requires preparation and all six implementation PRs merged and verified on main, task-owned branches/worktrees/build outputs cleaned, English/Korean docs current, protected evidence/recovery artifacts retained, and a verified user service running the measured artifact. Record implementation and qualification separately. CodeSpace runtime integration, Linux enforcement, cache and extra adapters remain outside this sequence and unqualified.
