# Owner instruction: CSRG-U2 (2026-10-02)

> **Status: dated record of an owner instruction; not maintained.** The owner gave this text in a Claude Code
> session's chat on 2026-10-02 at 15:33 UTC. It was not committed anywhere at the time. It is reproduced below
> unchanged, so every fact in it is as of that moment. It approves nothing beyond what it says, and later owner
> decisions may have superseded parts of it: the [CS-RG session handoff of
> 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what happened next and which parts still apply.

| Item | Detail |
| --- | --- |
| Given | In the same session, after #83 had merged and its post-merge CI had passed. |
| Covers | Execution-owner registration and participation readiness: a pre-work cleanup procedure, credential security constraints, the definition of done and the stop point. It is far more detailed than the U2 section of the [U1 to U6 instruction](2026-10-02-cs-rg-u1-u6-instruction.md). |
| Outcome | Implemented in [CodeSpace #84](https://github.com/novelKR/CodeSpace/pull/84), merged at the owner's exact-head approval on 2026-10-03 at 14:02 UTC as `ebed574`; post-merge CI [run 37128224856](https://github.com/novelKR/CodeSpace/actions/runs/37128224856) passed. |
| Still applies | The credential constraints, the list of things a cleanup must not delete, the pins and the merge rule. Its closing "do not start U3" was tied to the U2 merge; the owner's order of 2026-10-03 lists U3 as the step after it. |

Copied from the session transcript byte for byte, except that the chat tool's wrapper around pasted text was removed.

## Original text

````text
CSRG-U2 — Execution-owner registration and participation readiness
Proceed with CSRG-U2 now.
The prerequisite gate has been satisfied:

* CSRG-U1 was merged as CodeSpace #82.
* U1 hardening D-1 through D-5 was completed in CodeSpace #83.
* #83 exact head was `4be00f67ddb87ea86fe316c4392856e1779a4533`.
* #83 was merged as CodeSpace `main` commit `32068656f58afd8d6f4b353eccb62c2bb57feb54`.
* Post-merge `main` CI is green, including Integration, macOS contracts, DevGuard unit tests, dependency/pin checks, Linux isolation, clippy, policy, and documentation checks.
* CodeSpace still uses Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (`rust-v0.154.0`).
* DevGuard client/contract remain pinned to `f1f908429abea962d62d6b53c56a26c17250179b`.

Use live repository state as authoritative. If any of these facts have materially changed, stop before modifying product code and report the difference.
The deliverable is an actual U2 implementation PR, not another investigation package.
1. Normalize the local repositories and reclaim workspace before implementation
Before editing any source file, normalize both CodeSpace and DevGuard working copies.
The repository default branch is `main`; do not assume a `master` branch exists.
For each repository:

1. Fetch current remote state and prune obsolete remote references.
2. Inspect:
   * `git status`;
   * current branch and upstream;
   * local branches and divergence;
   * `git worktree list`;
   * submodule state where applicable.
3. Do not discard unknown or unrelated uncommitted work.
   * If an unrelated dirty working tree or unpushed local commit exists, preserve it and report it rather than deleting or overwriting it.
   * Do not use destructive blanket cleanup such as `git clean -fdx`.
4. Return the primary checkout to `main`.
5. Fast-forward it to `origin/main`.
   * Use a fast-forward-only synchronization.
   * Do not rewrite local history merely to make it match.
   * If `main` has diverged rather than fast-forwarding cleanly, stop and report the divergence.
6. In CodeSpace:
   * synchronize and update submodules to the exact gitlink recorded by `main`;
   * verify `third_party/codex` is still exactly `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`.
7. Re-query and record the normalized starting commits for:
   * CodeSpace `main`;
   * DevGuard `main`;
   * CodeSpace Codex gitlink;
   * DevGuard client pin currently recorded by CodeSpace.

Workspace and cache cleanup
After repository state is safe and normalized, reclaim disk space before the U2 build.
Clean only generated or superseded material.
Preferred cleanup order:

1. Remove obsolete scratch worktrees that belong to completed U1/#81/#82/#83 work only after verifying:
   * their branches contain no unique unpushed work;
   * their commits are already reachable from the intended remote history or otherwise intentionally preserved.
2. Remove scratch build directories and temporary verification copies that are no longer part of active or sealed evidence.
3. Remove repository-local Cargo `target` directories from completed/inactive worktrees.
4. If the normalized CodeSpace worktree itself contains a very large stale `target` tree, it may be cleaned before the U2 build.
5. Do not delete:
   * sealed evidence sets;
   * qualification records;
   * manifests or recorded hashes;
   * source-controlled files;
   * unpushed work;
   * DevGuard durable operational state;
   * global Cargo registry/git caches unless there is a separately justified need.

Record approximate disk usage before and after cleanup so the recovered space is visible in the final report.
After cleanup, create the U2 branch or a fresh U2 worktree from the normalized CodeSpace `main`. Do not base U2 on an old U1/#83 branch.
2. Correct the stale CS-RG milestone state
The current DevGuard milestone ledger still records CS-RG as `not-started`, but this is no longer true because U1 and U1 hardening are merged product code.
Correct this as part of starting U2.
The current state must become:

* CS-RG implementation status: `in-progress`;
* qualification: still `not-run`;
* U1: merged and verified;
* U1 hardening: merged and verified;
* U2: current active implementation unit;
* U3 through U6: not started.

Do not mark CS-RG as implemented or qualified.
Do not rewrite historical design-revision text or pretend historical CSRG-C00/C03/C09 were executed unchanged.
Where historical documents retain suspended directives, preserve them and add only a dated current-state/supersession note as necessary.
Because `milestones.json` belongs to DevGuard while the U2 product implementation belongs to CodeSpace, use the narrowest repository-appropriate change:

* CodeSpace PR: U2 product implementation, regression tests, CI and CodeSpace integration documentation.
* DevGuard: a narrowly scoped U2 bookkeeping/current-state change for `milestones.json` and the current CS-RG tracker/notice if repository separation requires a separate PR.

A DevGuard bookkeeping PR is not the definition of done for U2 and must not turn into another design package.
Update DevGuard #14 and the CodeSpace CS-RG tracker to reflect the live state.
3. U2 objective
Move CodeSpace from:
CodeSpace can inspect DevGuard status.
to:
The actual CodeSpace execution owner can establish and prove a registered DevGuard identity, while no workload is yet admitted or launched through DevGuard.
This remains an implementation task.
U2 does not implement admission or managed launch.
4. Preserve the established architecture
CodeSpace remains the execution owner.
Do not:

* change the Codex pin;
* add a new execution backend;
* transfer spawn ownership;
* transfer PTY ownership;
* transfer process handles;
* transfer output collection;
* transfer timeout or termination ownership;
* transfer reaping or lifecycle ownership;
* add `Admit`;
* add `BeginLaunch`;
* add launch permits;
* add launch carriers;
* add helper-based payload launch;
* implement P1-RECOVERY;
* implement DG-LINUX;
* start U3.

U1 status-only behavior must continue to exist independently.
5. Actual execution-owner registration
Register the process that actually owns CodeSpace executions.
Do not use a convenient process identity when it is not the execution owner.
InProcess mode
The registered instance must correspond to the actual in-process execution owner.
Record and verify its real PID/identity.
UDS mode
The execution owner is the UDS worker that owns execution handles.
Do not register the Gateway as the workload execution owner merely because the Gateway already owns U1 status probing.
Implement only the minimum CodeSpace-owned mechanism necessary for the worker to authenticate and register itself.
A DevGuard session is not a CodeSpace instance.
A new bounded session must continue to refer to the same intended execution-owner identity rather than inventing a new workload identity.
6. Registration contract
Using the currently pinned DevGuard generic client/contract, implement bounded registration with:

* consumer identity;
* generation;
* stable CodeSpace instance identity;
* actual owner PID or equivalent owner identity;
* expected consumer role;
* protocol compatibility;
* capability negotiation;
* registration readiness;
* static control reservation semantics where the current contract requires them.

Use current DevGuard APIs as they exist at the pinned revision.
Do not silently change the DevGuard pin to obtain a different API.
If the pinned contract makes U2 impossible without violating the approved architecture, stop and report the exact missing primitive rather than silently expanding scope.
7. Participation policy
Preserve U1's status-only participation.
Add the explicit resource-participation policy needed by later units, conceptually:

* `off`;
* `required`.

Status participation and resource participation are separate concepts.
U2 does not yet contain admission or managed launch. Therefore `required` must never mean:
registration succeeded, so run the command through the ordinary ungoverned execution path.
Until U3/U4 exist, selecting `required` for a new execution must return a stable explicit result equivalent to:
registration is available, but managed resource admission/launch is not implemented yet.
Do not silently downgrade `required` to:

* `off`;
* status-only;
* legacy execution.

Existing `off` behavior must remain unchanged.
8. UDS credential handoff
For UDS mode, implement only the minimum private credential handoff necessary for the actual worker to authenticate and register.
The credential must never:

* appear in MCP arguments;
* appear in CLI arguments as secret material;
* appear in an environment variable;
* appear in logs;
* appear in status output;
* reach a user payload;
* remain in an unrelated process;
* be inherited by unrelated children.

Build on the credential hardening already merged in #83.
Do not create a generic secret-distribution subsystem.
Test actual worker startup and child-spawn overlap.
9. Failure model
Map and preserve at least:

* authority unavailable;
* peer/identity mismatch;
* protocol mismatch;
* missing required capability;
* credential unavailable;
* credential refused;
* registration refused;
* malformed configuration;
* wrong consumer or generation;
* wrong execution-owner identity;
* unsupported mode/platform.

Use CodeSpace-owned states and errors.
Do not expose arbitrary DevGuard free text.
A registration failure must not break unrelated CodeSpace tools.
Already-running process control must not require a fresh successful DevGuard registration. In particular, existing:

* process status;
* termination;
* timeout handling

must remain CodeSpace-owned and available.
10. Required tests
Use DevGuard's real test authority where practical.
At minimum cover:

* InProcess owner registration;
* UDS worker owner registration;
* feature-off behavior;
* runtime off behavior;
* U1 status-only behavior remaining unchanged;
* stable instance identity across bounded sessions;
* actual owner PID/identity;
* wrong PID/owner;
* wrong consumer;
* wrong generation;
* wrong peer UID;
* incompatible protocol;
* missing capability;
* credential unavailable;
* credential refused;
* registration refused;
* concurrent startup and registration;
* worker startup failure;
* credential handoff cleanup;
* credential non-leakage;
* DevGuard session socket non-inheritance;
* credential/carrier descriptor non-inheritance where U2 introduces one;
* `required` refusing to execute while U3/U4 are absent;
* existing process query/termination remaining independent from fresh registration.

Keep deterministic positive controls for descriptor/leak detectors.
Do not replace testable behavior with source inspection alone.
11. Dependency and CI preservation
U2 must preserve the boundaries established by U1/#83:

* DevGuard product crates come only from the reviewed pin;
* only approved DevGuard product crates are allowed into the product dependency graph;
* DevGuard's own dependency closure introduces no CodeSpace or Codex crates;
* every `codex-*` product dependency remains sourced from the CodeSpace Codex gitlink;
* feature-off dependency graphs remain unchanged except for changes genuinely required by U2's explicit feature path.

Update CI so U2's new code is continuously exercised on the platforms and modes it claims to support.
Keep:

* adapter tests;
* feature-enabled server tests;
* feature-off regression;
* dependency/pin checks;
* macOS descriptor/credential behavior;
* Linux compilation/integration behavior where applicable.

Do not weaken an existing gate merely to get green CI.
Preserve failed validation attempts as evidence.
12. Implementation and evidence discipline
Do not open another research or architecture PR.
Necessary inspection, version checking and experiments belong inside the implementation work and serve verification of the product PR.
The U2 PR body must record:

* exact base and head;
* CodeSpace `main` starting point;
* DevGuard source pin;
* Codex gitlink;
* actual registered execution owner in each mode;
* registration identity mapping;
* participation-policy behavior;
* credential handoff;
* error mapping;
* dependency delta;
* feature-off result;
* descriptor/credential tests;
* supported and unsupported modes;
* local validation failures as well as successes;
* CI results;
* rollback.

13. Stop conditions
If one mode cannot implement correct registration without:

* moving execution ownership;
* adding a new backend;
* changing PTY ownership;
* changing reaping;
* adding admission/launch;
* changing the Codex pin;

do not force the design.
Mark that mode unsupported for U2 and report the exact conflict.
If a DevGuard or Codex pin change becomes necessary, stop and request separate owner authorization.
14. Definition of done
U2 is complete only when:
An implementation PR exists in which the actual CodeSpace execution owner can establish a verified DevGuard registration in each supported mode, with conservative failure handling, private credential transport, stable owner identity, and an explicit participation policy that never executes an ungoverned workload under `required`.
Additionally:

* current DevGuard milestone state records `CS-RG: in-progress`;
* trackers reflect U1 + hardening complete and U2 implemented;
* all required CI is green;
* no Codex pin change occurred;
* no admission or managed launch was added.

At completion:

1. re-query the PR head and base;
2. verify all CI on the exact head;
3. update/read back relevant trackers;
4. report exact CodeSpace and DevGuard commits/pins;
5. report workspace cleanup and reclaimed disk space;
6. request explicit exact-head merge approval;
7. stop.

Do not merge U2 yourself.
Do not start U3.
The next unit after an owner-approved U2 merge and green post-merge CI is:
CSRG-U3 — admission, attempt identity, and pre-spawn slot lifecycle.
````
