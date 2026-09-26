# Design revision 1: CS-RG execution ownership and reuse policy

Design reference date: 2026-09-27. Applies to DevGuard PR #8, CodeSpace's CS-RG integration plan, and the execution and dependency policies of both repositories. English is authoritative under the repository's documentation policy; the [Korean text](ko/design-revision-1.md), prepared from the specification as the user supplied it, is its reviewed counterpart.

**Status.** On 2026-09-27, after a mid-course review, the user adopted this revision by explicit instruction as the new design baseline for CS-RG. The [design reference](design.md), [decisions](planning/decisions.md) and [planning documents](planning/README.md) apply it. The historical approval [design.ko.md](design.ko.md) and its [checksum](design-source.json) are unchanged. The revision changes decisions that bind later implementation; it changes no implementation or qualification status, and CS-RG remains `not-started`. In this text, references to individual review responses are replaced with neutral descriptions, and the [appendix](#appendix-evidence-for-factual-statements) lists the evidence for its factual statements.

## 1. Purpose and scope of the decision

This revision is **not a documentation correction that aligns the initial design with the implementation. It revises the design, at the user's explicit instruction to re-examine it, to reflect current implementation and maintenance conditions.**

An earlier separate synthesis raised the central problem: DevGuard integration might leave CodeSpace maintaining duplicate PTY and process implementations for ordinary execution and for resource-managed execution over the long term. This revision keeps that concern but does not adopt the conclusion that "using more of Codex solves it automatically".

**The final goal of this revision:**

> **CodeSpace manages the policy, state and ownership of executions in common, and confines the differences between PTY, pipe and resource-managed execution to a narrow backend boundary that can be verified. Codex reuse and in-house implementation are both evaluated as means to that goal.**

It adopts the D1/D2/D3 split of the 2026-09-27 review and adds the following.

| Decision | Decision in this specification |
| --- | --- |
| **D1 — implementation of `required` execution** | At the current pin, the default is a path in which CodeSpace owns the child lifecycle. Do not describe it as "the only possible design" or as "work finished in a few hundred lines". |
| **D2 — integration of `off` execution** | Preserve existing behavior and move to the common contract in stages. **Before the final CS-RG qualification, complete the decision on actual integration or on keeping a limited branch.** |
| **D3 — DevGuard's dependence on Codex** | Add no Codex dependency to the current implementation. Instead of a permanent ban across the repository, revise the design rationale into **independence of the core and the shared client, and a conditional reuse policy for execution adapters**. |
| **A4 — code adaptation with stated provenance** | Not unrestricted. The design permits **limited downstream adaptation** that meets this specification's scope, provenance, divergence-tracking and verification conditions. |
| **Codex pin upgrade** | This documentation revision does not change the actual pin. Investigating the fitness of candidate APIs is separate from changing the pin; if needed, the pin changes separately on the basis of verification results. |
| **`ProcessDriver`** | Not a default choice. Select it only when it satisfies the output-loss, Drop and backpressure contract. |

This revision distinguishes **a decision to keep the current implementation from a permanent architectural prohibition**. The existence of an earlier design identifies the documents to change and the verification scope; it is not grounds for rejecting an alternative.

What follows is **the specification applied to the PR revision and to later implementation**. It does not mean that the repositories already reflect this change or that a PR has been merged.

## 2. Source baselines and the changed nature of PR #8

### 2.1 Confirmed baselines

On the reference date, PR #8 was open with head `92a34721d88f39a22cdde4603958d6c447c90e76` and base `395315d34b5d458ea1774446727f0cb14bd8a120`. Its body at that head scoped it as "aligning the CS-RG plan with the DG-1 implementation" and stated that it kept the existing 8 CS-RG work units and 4 PR groups.

CodeSpace `main` is confirmed at `b6e7ed22e2c730ac987297455e250cbd6e8e8b0c`. The revision keeps the earlier `e94d214…` as the historical inspection baseline but **re-fixes the actual starting baseline at the latest confirmed SHA**.

| Reference | Treatment |
| --- | --- |
| CodeSpace `e94d214…` (earlier) | Preserved as the inspection baseline of the initial integration design |
| CodeSpace `b6e7ed22…` | Confirmation baseline of this revision |
| DevGuard `395315d…` | Baseline of the integration plan after DG-1 completion |
| DevGuard PR #8 `92a34721…` | Documentation head under revision |
| Codex `6b9826e…` | Current CodeSpace deployment pin |
| Codex `b334d5b…` and others | **Comparison snapshots** examined in the earlier review. Not a deployment pin or an automatic adoption target |

### 2.2 PR description changes

Recommended PR title:

```text
docs(architecture): revise CS-RG execution ownership and reuse policy
```

Rewrite the PR's purpose statement to carry this meaning:

> This PR revises the CS-RG execution layer at the user's explicit instruction to re-examine the design. Besides consistency with the DG-1 implementation, it addresses CodeSpace's duplicate lifecycles, spawn protection, output contract and backend maintenance cost. It keeps the authority and resource-correctness goals of the existing design but re-evaluates the boundaries and work order that implement them.

Change these expressions in the existing body:

| Existing expression | Revised direction |
| --- | --- |
| "The structure is unchanged" | State that the execution layer and verification order are revised, and that work units and groups change where needed |
| "The Codex adapter stays for `off`" | An initial compatibility measure, to be decided (kept or integrated) before final qualification |
| "The pin is unchanged" | A decision within the scope of this change, not to be read as a ban on future change |
| "The design changes if the user decides" | Record this instruction as the basis of the re-examination and design revision |
| "Documentation only" | Make it precise: **this PR's file changes are limited to documentation and documentation checks, but it changes design decisions that bind later implementation** |

DevGuard's current `AGENTS.md` also states that the user's current instructions take precedence. Revising the editorial design while preserving the historical approval is therefore the appropriate method.

## 3. Goals to keep and implementation choices that may change

The revised documents must not treat the following two as constraints of the same level.

### 3.1 Correctness and security goals to keep

Keep **one clear owner per execution, prevention of duplicate execution, credential protection, conservative resource release and the existing authority model**.

Concretely, these are invariants:

| ID | Invariant |
| --- | --- |
| INV-01 | Each process has exactly one party actually responsible for reaping it. |
| INV-02 | A DevGuard-managed execution never loses a required opportunity to observe before reaping. |
| INV-03 | Permit, credential and transcript descriptors never reach an unrelated execution or the user payload. |
| INV-04 | A lost reply, timeout, EOF or root reap alone never establishes that an execution did not happen or that its whole scope ended. |
| INV-05 | A prepared execution is consumed once, and an uncertain execution is never re-executed automatically. |
| INV-06 | Failure to obtain a new resource grant never blocks the status or termination request path of existing processes. |
| INV-07 | Process termination, output cleanup, release of workspace occupancy and return of the resource lease are distinct. |
| INV-08 | Implementation completion, functional verification, platform verification and SLO qualification are recorded separately. |

These principles are also the core of the current DevGuard [contracts](contracts.md) and the [integration plan](planning/codespace-integration.md).

### 3.2 Implementation choices to re-evaluate

The following are **changeable means**, not goals:

- A structure in which `off` and `required` call different spawn functions.
- Continued use of Codex's high-level `spawn_pty_process()`.
- The source of the implementation of CodeSpace's internal PTY opener.
- A ban on Codex dependencies across the whole DevGuard repository.
- The existing Codex pin.
- The plan's count of 8 work units and 4 PR groups.

**Keep the correctness goals; decide the implementation choices anew within the scope of this optimization.**

## 4. Revision of PR #8's F1

The existing F1 proposes Runner-owned execution because the pinned Codex spawn reaps internally and passes descriptors in a way DG-1 cannot use. The revision splits it into five sub-findings.

| Sub-finding | Problem | Required change |
| --- | --- | --- |
| **F1a — reap ownership** | Termination, timeout and Drop paths other than the waiter can also reap the child | Inventory every reaping path and consolidate them to one responsible party per execution |
| **F1b — descriptor passing** | "Not closing descriptors" differs from "passing close-on-exec descriptors only to one specific child" | Specify descriptor ownership, inheritance and closing from preparation to payload |
| **F1c — concurrent spawn** | A spawn on another thread can race the window in which private descriptors are created | A process-wide spawn protection rule and a proof for each exception |
| **F1d — output and handle contract** | Loss inside a bridge and Drop behavior can differ from the upper contract | Fitness checks for output loss, backpressure and Drop |
| **F1e — maintenance branch** | Only a common interface is built while duplicate implementations persist indefinitely | Before CS-RG ends, decide backend integration or limited retention |

### Conclusion of F1

Replace it with this meaning:

> The currently pinned high-level Codex PTY spawn cannot carry a DG-1 managed execution unchanged. The cause is a mismatch in the reap-ownership and descriptor-passing contracts, and it must not be hidden behind a simple wrapper.\
> CodeSpace introduces a common execution contract and a single lifecycle-coordination layer. `required` uses a backend that can observe before reaping. The existing `off` backend may remain for initial compatibility, but before the final CS-RG qualification the grounds for integration or for keeping a limited branch are settled.

**Limit the scope to "the examined API cannot be used unchanged", not "Codex is impossible".**

## 5. CodeSpace target architecture

### 5.1 The center of commonality is the execution contract, not a PTY function

The target structure is below. **The names are concepts of the new design, not existing APIs.**

```text
CodeSpace Gateway
    │
    │ authorization / workspace / approval
    ▼
Runner ExecutionCoordinator
    │
    ├─ ResourceGovernor
    │    ├─ OffGovernor
    │    └─ DevGuardGovernor
    │
    ├─ PreparedExecution / LaunchPlan
    │
    ├─ ProcessSupervisor
    │    ├─ status / timeout / termination
    │    ├─ exit observation / reap coordination
    │    └─ output / retention / release coordination
    │
    └─ ProcessBackend
         ├─ LegacyCodexPty
         ├─ LegacyTokioPipe
         └─ OwnedUnixProcess
              ├─ PipeTransport
              └─ PtyTransport
```

What stays common: **execution identity, the link between approval and execution, state transitions, timeout, termination requests, output recording, and completion and release coordination**.

What stays in the backend: **OS child creation, I/O attachment, terminal setup, platform-specific means of observing exit, and the actual reap**.

#### An important limit

Building a common `ProcessSupervisor` does not by itself make the Runner the owner of the legacy Codex backend's actual waiter.

Two ownership models are therefore distinguished explicitly:

| Model | Meaning | Usable scope |
| --- | --- | --- |
| `BackendReaped` | The backend performs the actual reap and reports the result | Initial legacy `off` paths |
| `OwnerControlledReap` | CodeSpace controls the non-reaping observation and the order of the actual reap | The DevGuard `required` path |

**Do not invent an `ExitedUnreaped` state on a `BackendReaped` path.** The common interface must not advertise a capability that the backend cannot actually guarantee.

### 5.2 Module responsibilities

Do not create an unnecessary new daemon or separate repository; use CodeSpace-internal modules or narrow crates.

| Proposed location | Responsibility |
| --- | --- |
| `crates/runner/src/execution/` | Execution coordination, plan consumption, state transitions |
| `crates/runner/src/process/` | Linking the supervisor and backends |
| `crates/resource-client/` | DevGuard client, error and capability translation |
| Existing `crates/pty/` | Isolation of Codex types and the legacy PTY path |
| A CodeSpace-internal Unix transport module | The required PTY allocation, stdio and resize implementation |

**Responsibility boundaries take precedence** over fixed directory names. The module layout may be adjusted during implementation, but these conditions hold:

**DevGuard client types and Codex types must not leak into CodeSpace's public MCP types.** The generic execution-coordination layer must not mix the internal types of the two upstreams directly.

## 6. The `PreparedExecution` and `LaunchPlan` contract

### 6.1 A one-time execution preparation, not a notification hook

DevGuard is not an observer that is merely notified before and after execution. It builds the execution target through the helper, carries the permit and transcript, and restricts the creation path.

The actual `HelperCommand` exposes a standard `Command` for configuration while requiring creation through its own `spawn()`.

`prepare()` therefore returns an **owned execution preparation** that contains:

| Part | Contract |
| --- | --- |
| Execution identity | A fixed link between the CodeSpace process ID and the DevGuard attempt |
| Command meaning | Executable, argv, cwd, environment changes, tty, timeout, workspace and policy identification |
| Execution slot | Acquired before the actual spawn |
| Workspace occupancy | Linked to the existing FIFO and approval flow |
| Resource preparation | `off`, or a DevGuard Prepared state |
| Expiry | Keeps the deadline of the first preparation |
| Right of consumption | Executed once, or cancelled |
| Failure cleanup | Distinguishes cleanup of an unexecuted preparation from committed or uncertain cleanup |

Conceptually:

```text
PreparedExecution
    identity
    meaning_digest
    slot_guard
    workspace_guard
    original_deadline
    launch_plan
    resource_state

LaunchPlan
    Plain(...)
    GovernedHelper(...)
```

The actual DevGuard object inside `GovernedHelper` may be isolated inside the adapter. What matters is not the type name but **one-time consumption and responsibility for safe cleanup**.

#### Prohibited implementations

Do not freely `Clone` the preparation and spawn from two places, and do not rebuild a preparation from serialized argv alone.

**Do not assume that the Drop of a cancelled async task returned the resource lease.** A state that needs asynchronous remote cleanup must remain as an explicit reconciliation task.

### 6.2 Execution order

Keep the existing approval and workspace policy and proceed in this order:

```text
Authorization, command and workspace policy checks
    ↓
Acquire the workspace FIFO
    ↓
Acquire a Runner execution slot
    ↓
Resource preparation: Admit
    ↓
Same-attempt approval CAS
    ↓
BeginLaunch
    ↓
Consume the LaunchPlan once to create the helper
    ↓
Track the helper transcript and the process handle separately
    ↓
Observe process exit
    ↓
Required pre-reap Observe
    ↓
Reap
    ↓
Clean up output, workspace and resource lease separately
```

The plan's **250 ms preparation budget and 5-second Prepared lifetime** are different limits, and DevGuard's 250 ms per-frame deadline is a third. The new implementation must not treat these three values as one timeout.

In particular, allowing 250 ms per frame does not make the whole preparation finish within 250 ms. C00 must verify end-to-end deadline propagation through connection, authentication, registration and Admit.

## 7. Process lifecycle and reap contract

### 7.1 Do not merge states into one `finished`

Manage at least these four axes independently:

| Axis | Example states |
| --- | --- |
| Preparation and dispatch | Prepared, Committed, HelperSpawned, Uncertain |
| OS child | Running, ExitedUnreaped, Reaped, OwnershipLost |
| Input and output | Open, EOF, Truncated, Failed |
| DevGuard resource | Reserved, Active, Suspect, Released |

This structurally prevents these misreadings:

```text
stdout EOF ≠ process exit
process exit ≠ reap
reap ≠ descendant termination
output retention expiry ≠ resource lease release
helper READY ≠ payload execution success
```

DevGuard's current `LaunchOutcome::Started` is also an execution sign observed in the transcript, not strong proof of execution success that excludes every case in which the helper ends right after READY. The integration layer must not strengthen the meaning of this name.

### 7.2 A single reaper

On the `required` path:

**Outside the object that owns the actual child, never call a reaping operation such as `wait`, `try_wait` or `waitpid`.**

Termination requests and timeouts do not wait on the child directly; they pass an intent to the supervisor.

```text
terminate_process ─┐
timeout ───────────┼─→ supervisor → verified termination action
shutdown ──────────┘

OS exit notification
    → supervisor's non-reaping observation
    → Observe
    → reap through the same owning object
```

Besides the ordinary waiter, CodeSpace currently calls `try_wait()` in its termination request path. Replacing one waiter function is therefore not enough.

#### Call sites that must be examined

Include `spawn_pipe`, exit watching, timeout, `request_kill`, workspace-wide termination, shutdown, backend Drop, task cancellation and error cleanup.

Do not just list the locations grep finds; record an ownership table showing **who ultimately owns the child and which message each path sends**.

### 7.3 Failure handling of pre-reap observation

The basic flow matches the one the current DevGuard CLI uses:

```text
Confirm exit without reaping
    ↓
Attempt the pre-reap Observe
    ↓
Record success, failure or timeout
    ↓
Reap through the owned Child
    ↓
Follow-up Observe / reconciliation as needed
```

The current CLI also distinguishes observation before reaping from observation after it, records a failed pre-reap observation and then reaps the child.

This design adds these limits:

- The pre-reap observation has an overall deadline.
- A failure does not return the resource lease.
- A zombie is never kept indefinitely while waiting for an observation to succeed.
- While the original child-owning object remains, no other code reaps directly.
- `ECHILD` or lost ownership is neither a successful observation nor evidence of non-execution.

**New policy proposal:** cap the whole pre-reap Observe attempt at **1 second** initially and verify it in C00. This is a cleanup budget proposed by this revision, not a previously qualified value. It must not block responses to status queries or termination requests.

A blocking call that cannot implement this must not simply be wrapped in a timeout future and treated as "cancelled". If the actual call keeps running, its ownership and cleanup responsibility must also keep being tracked.

### 7.4 Concurrency of observation and signals

A termination request must be processable without a new resource grant from DevGuard. This does **not** mean that whole-scope termination can always be proven during an authority outage.

The Runner exercises its existing control over targets it can safely identify; if it cannot confirm that descendants or the whole scope ended, it marks the result incomplete and does not return resources.

A new managed backend must not keep only stale numeric PIDs or PGIDs and signal them unconditionally. When Codex's process-group termination code is brought in, it must be reconciled separately with DevGuard's identity and scope contract.

## 8. Spawn protection and the descriptor contract

### 8.1 A single gate is not a single global lock wrapper

The current `HelperCommand::spawn()` already acquires `spawn_guard()` internally. An outer common gate must therefore not call it while holding the same guard.

Rust's standard `Mutex` does not guarantee a normal return when the same thread acquires it twice, so it must not be treated as a reentrant lock.

What the common gate must guarantee:

> **Every spawn follows the same descriptor-protection rule, and each path has exactly one designated party responsible for that protection.**

| Execution path | Protection responsibility |
| --- | --- |
| DevGuard `HelperCommand` | Uses the helper API's internal protection; no outer double acquisition |
| Directly created pipes and helpers | The common guard, or verified equivalent protection, around the actual spawn |
| Legacy Codex PTY | Examine the actual spawn and descriptor-cleanup path and prove a safe way to participate |
| Kernel-level close-on-exec-by-default paths | An exception only when application, error and fallback paths are verified |

**Making one's own descriptors close-on-exec does not by itself close the race with the window in which another thread creates descriptors.**

A separate version of the client crate linked twice into the same process, each with its own static guard, is also a verification target. Distinguish "uses a mutex of the same name" from "shares the same protection object".

### 8.2 Protection scope

The scope is not only `exec_command`.

It includes **every real child-creation path** that runs in the same OS process: the patch helper, sandbox helpers, auxiliary commands, worker creation, test helpers and so on.

The Gateway and the UDS Runner are different processes, so their descriptor-creation and spawn-protection scopes are examined separately. A global daemon lock does not solve this.

### 8.3 Limits on the critical section

While holding the spawn guard, do not:

```text
Make DevGuard network requests
Wait for permit authorization
Receive a whole transcript
Wait for child exit
Drain output
Write long logs or files
```

The guard protects the necessary descriptor creation, inheritance setup and actual child-creation boundary.

Do not run a whole future under a standard mutex because the external API is an async function. Check where the child is actually created and what waiting is performed.

If that safety cannot be confirmed, **mixed use of legacy and managed paths cannot be declared verified**. Replace the backend in the same scope, or restrict the supported combinations explicitly.

### 8.4 Composing PTY setup with helper descriptors

Adding PTY setup must preserve the existing `HelperCommand` permit and transcript passing.

| Item | Requirement |
| --- | --- |
| Parent's private descriptors | Stay protected until the helper is created |
| Child's required descriptors | Permit, transcript and legitimate jobserver descriptors are passed and kept distinct |
| Payload boundary | The credential descriptor is closed and the transcript stays close-on-exec |
| Stdio | The PTY slave is attached to stdin, stdout and stderr |
| Terminal | The order of session and controlling-terminal setup is specified |
| Failure | A setup failure cleans up the child, master/slave and private descriptors |
| Re-execution | Required descriptors and PID meaning survive the helper's QoS re-execution |

`pre_exec` is not an ordinary Rust execution environment. Prepare data beforehand so that an added callback performs no allocation, mutex acquisition, environment lookup or other unsafe work. Also consider the execution order of registered callbacks.

In particular, **prohibit cleanup that drops descriptors the helper needs from the keep-list**, and **code that blindly applies PTY setup and process-group setup twice**.

## 9. Output, backpressure and the `ProcessDriver` criteria

### 9.1 CodeSpace owns the default handle

CodeSpace owns its public process handle and output record. Reusing Codex's `ProcessHandle` is not a goal in itself.

The current pinned `ProcessDriver` bridge skips items dropped by broadcast lag, and `ProcessHandle`'s Drop performs termination and I/O cleanup.

Adopt `ProcessDriver` only when it satisfies all of these:

| Condition | Acceptance criterion |
| --- | --- |
| Output loss | Output lost inside the bridge is never hidden from the upper record |
| Backpressure | A slow reader never blocks termination or status requests |
| Byte limit | A total byte bound is defined, not only a channel count bound |
| Drop | The Drop of a UI, response or temporary handle never causes a wrong termination |
| Exit callback | No double termination, reap race or stale identity use |
| Output drain | Handles the ordering difference between the exit notice and the final output |
| Cost | Better than the code it removes, counting added conversions, buffers and tasks |

Broadcast's `Lagged(n)` is not directly an exact count of lost bytes. Where exact byte accounting is needed, there must be sequence and length metadata or a separate loss path.

**If meeting these conditions would require wrapping the Codex handle in more complexity, use CodeSpace's existing output and handle abstractions directly.**

### 9.2 Common output contract

Converge output into one CodeSpace recording layer wherever possible.

```text
backend output
    ↓
CodeSpace output collector
    ↓
bounded retention
    ↓
read_process / output_lost
```

Do not mix output of unrelated executions, distinguish EOF from exit status, and distinguish intentional retention loss from unknown loss inside a transport.

Returning `output_lost=false` when the amount of loss is unknown is not allowed. If the existing protocol cannot express this state, CSRG-C06 designs the required compatibility change explicitly.

## 10. D1, D2 and D3: choices and exit conditions

### 10.1 D1 — managed PTY

**The default is A1: a CodeSpace-owned Unix transport, possible at the current pin.**

Do not shrink its scope to "a few lines of PTY opener". What must be maintained includes master/slave lifetime, signal and session setup, resize, descriptor failure cleanup, output handling, and cancellation and shutdown coordination.

Where Codex code is useful, evaluate in this order:

```text
Existing public API
    ↓
An upstream candidate that provides the same contract
    ↓
Limited downstream adaptation
    ↓
In-house implementation of the needed scope
```

This does not require trying every implementation in order; it requires **recording reuse possibilities and contract differences before writing new code**.

#### Scope permitted for A4

This revision permits adaptation with stated provenance that meets these conditions.

**Permitted:** clearly separated execution mechanisms such as PTY allocation, terminal setup, resize and limited I/O helper code.

**Not permitted:** `codex-core` product semantics, session authority, the agent loop, broad crate copies, and code duplication that evades dependency checks.

Each adaptation records:

```text
Source repository, full SHA and file path
Scope taken
Reason for the change
Intentionally different behavior
Corresponding tests
Conditions for re-examination when upstream updates
Conditions for removal or reconvergence with upstream
```

CodeSpace's existing policy prohibits copying upstream crates to hide incompatible dependencies. Keep that prohibition, and **define auditable limited adaptation as a separate policy**.

### 10.2 D2 — the existing `off` path

D2 no longer remains an optional task "to review later if there is time".

**Before the final CS-RG qualification, one of the following must be decided.**

#### Result A: actual backend integration

If behavioral equivalence and a maintenance benefit are confirmed, remove the existing path for that platform and transport.

Do not stop at making the documentation common; actually remove the unused code, branches, test fixtures and dependencies.

#### Result B: keep a limited compatibility backend

If integration is currently a loss, the existing backend may remain, but this information is required:

| Item | Requirement |
| --- | --- |
| Reason to keep | A concrete behavioral difference or cost evidence |
| Remaining scope | Which platform, transport and mode |
| Common parts | State, timeout, errors, output and so on |
| Duplicated parts | Code and tests that must actually be maintained separately |
| Support contract | Capabilities the backend does not provide |
| Revisit point | The next relevant upstream change, extension or defect fix |
| Removal criteria | What must hold for integration |

**Neither "it is existing code, so keep it" nor "parity passed, so it must be replaced" is sufficient grounds.**

Keeping a branch must also prove its maintenance cost. The burden of evaluation must not fall only on the new implementation.

### 10.3 D3 — DevGuard dependency policy

Choose C0, adding no Codex dependency to the current implementation, but change the rationale of the editorial design as follows.

#### Boundary to keep

```text
devguard-contract
devguard-core
generic client contract
```

These layers contain no Codex product types, model sessions, CodeSpace workspace authority or PTY ownership.

#### Boundary to revise

Instead of a permanent ban such as "the DevGuard repository never uses Codex under any circumstances", state:

> DevGuard's default distribution and shared client currently do not depend on Codex. Reuse of low-level utilities in execution and platform adapters is decided by evaluating the actual code replaced, contract fit, dependency propagation, recovery path and requalification cost.

Add no new dependency unless it shows which part of the current `HelperCommand`, launcher or native observation it actually replaces.

#### Revisit triggers

Adopt the triggers of the 2026-09-27 review, but never adopt automatically because a feature name appears.

| Trigger | What to check again |
| --- | --- |
| Start of DG-LINUX | cgroup placement timing, helper layering, parent-child relations, fork safety |
| A public general descriptor-attachment or external reap-ownership API | Whether the contract holds, including actual fallback, Drop and cancellation paths |
| Expansion of DevGuard's child supervision | Whether actual duplicate code has appeared |
| Repeated fixes of the same OS defect | Whether a common implementation reduces fix and verification cost |

The review's **"36 transitive dependencies"** is recorded only as a figure from that review. To use it as an adoption criterion, it must come with the SHA, target, features, the runtime/build/dev split and the command run; the revised documents do not use it as a universal fixed cost.

## 11. Failure, approval and resource-release mapping

The existing PR #8 table groups a lost permit reply, an uncreated helper and a helper ending before READY closely together. The revision **separates evidence of non-execution from evidence of resource cleanup**.

| Situation | Execution judgement | Resource and approval handling |
| --- | --- | --- |
| Explicit `Admit` refusal | User code did not run | Approval not consumed; clean up the preparation guards |
| Lost `Admit` reply | No child exists yet, but the reservation state is unknown | Query or cancel the same attempt; never release on one's own before remote release is confirmed |
| Lost `BeginLaunch` reply | Local ownership must show whether the permit was not received or was already handed to the execution path | Keep the same attempt; `AbandonLaunch` only when safe |
| Definite failure of the helper spawn | Use the failure scope the creation API guarantees | Pass the non-creation evidence to the authority and confirm it |
| Valid `failed` or `refused` transcript | The helper reports that it did not execute | Clean up a claimed scope and an unclaimed grant separately |
| `exec_failed` after READY | The executable could not be entered after preparation passed | Distinct from a plain budget refusal; no automatic approval reuse |
| Transcript lost after READY | Actual execution cannot be ruled out | Keep unknown; no automatic replay |
| Root exited, descendants survive | Only the root's exit is confirmed | Decide scope, workspace and resource cleanup separately |
| Observe failure or tracking loss | Whole termination is not proven | Reap through the bounded procedure, but keep resources conservatively |

**Do not treat the `AbandonLaunch` call itself as `NoHelperCreated` evidence.** What matters is what the authority checked and which state it returned.

A user executable may itself return 125, 126 or 127, so an exit code alone never infers the helper's execution phase or whether an approval can be reused.

## 12. Work units and PR order

The existing CS-RG plan consists of C01–C08 and P1–P4. This revision **adds minimal fitness verification and the D2 exit decision as separate required work**. It does not hide them inside C03 to keep the existing count.

### 12.1 Revised work structure

The IDs and groups below are **proposed values of this specification**.

| Group | Work | Key deliverables |
| --- | --- | --- |
| **CSRG-P0** | **C00 — execution-boundary fitness verification** | Spawn and reap call inventory, minimal managed PTY verification, report on descriptor protection and cleanup fitness |
| **CSRG-P1** | C01, C02 | Client pin, helper provenance, consumer provisioning, registration and settings |
| **CSRG-P2** | C03, C04 | Common supervisor, PreparedExecution/LaunchPlan, consistency of managed execution and approval |
| **CSRG-P3** | C05, C06 | Control/data protection, overall bounds on output, replay and lifecycle |
| **CSRG-P4** | C07, **C09 — backend maintenance convergence decision** | Parity and fault verification, the decision to integrate `off` or keep it limited, and code cleanup |
| **CSRG-P5** | C08 | CodeSpace integration qualification of the final artifact combination |

With this, CS-RG has **10 work units in 6 groups**. Documents that use the overall totals 46/23 become **48/25**, provided no other work changes; recompute them from the actual planning data.

#### Scope of C00

C00 does not build a complete process framework of its own.

**It verifies with a minimal implementation that a managed helper can be attached to a PTY, that its descriptors are safe, and that the owner can control the order of observation and reaping after the root exits.**

The verification code is absorbed into later common tests or deleted. Leaving a C00 backend in the product as a fourth path is prohibited.

#### Scope of C09

C09 cannot be completed by stating "reviewed" in a document.

If integration is chosen, actually remove the unnecessary code and dependencies; if keeping the branch is chosen, submit **a decision record with scope, cost, tests and revisit conditions**.

If C09 changes code, rerun the related C07 parity and **measure that final head in C08**.

### 12.2 Changes to existing work units

| Work | Additions and changes |
| --- | --- |
| C01 | Separate client and helper provenance, check actual runtime and build dependencies, check for transitive Codex dependencies |
| C02 | Distinguish instance from session, validate resource settings, mixed-mode protection rules within the same process |
| C03 | Beyond pre-empting slots: consolidate every reaping path, backend capabilities and cleanup responsibility |
| C04 | One-time LaunchPlan consumption, lost `BeginLaunch` replies, approval handling for each transcript phase |
| C05 | Verify that spawn and Observe latency does not propagate into the control path |
| C06 | Output accounting including bridge loss, a total byte bound, separation of retention from lease |
| C07 | `off`/`required`, pipe/PTY and InProcess/UDS, plus mixed use within one process |
| C08 | Qualify only the final implementation, pin and artifact combination after C09 |

**Do not describe the existing C03 as "adding a small opener".** Revising execution ownership is major work, to be judged by the verification scope of its races and failure paths rather than by code size.

## 13. Verification and completion conditions

### 13.1 Required verification matrix

The base matrix:

```text
resource mode: off / required
transport:     pipe / PTY
runner mode:   InProcess / UDS
```

Add **mixed execution within one process** as a separate axis. Success of each path alone does not verify descriptor and spawn protection.

| Area | Required cases |
| --- | --- |
| Slots and preparation | Concurrency limit exceeded, preparation expiry, cancellation, late worker execution |
| Execution identity | Same-attempt re-request, changed-meaning conflict, lost replies |
| Helper | Lost permit, wrong helper, failures before and after READY |
| Reaping | Root exits at once, surviving descendants, races of timeout, terminate and shutdown |
| Ownership | Waiter cancellation, backend Drop, `ECHILD`, prevention of double reap |
| Descriptors | Unrelated concurrent spawns, payload inspection, jobserver retention, failure cleanup |
| PTY | Initial size, resize, controlling terminal, session and group, EOF |
| Output | Large output, slow readers, bridge lag, tail retention, exceeding the bound |
| Authority failure | Pre-reap Observe failure, daemon restart, control of existing work while new grants fail |
| Rollback | Blocking new starts, draining existing executions, preserving unknown, no stale journal restore |

#### Acceptance criteria for safe failure

A fault-injection test need not return every resource immediately.

**While uncertainty remains, staying Suspect or charged can be the correct result.** That state must be observable and must never lead to false success, automatic re-execution or wrong reallocation of resources.

### 13.2 Measurements for the maintenance optimization

"The code got shorter" alone never decides D2.

| Measurement | Purpose |
| --- | --- |
| Number of spawn entry points in the product | Did bypass paths decrease? |
| Number of sites that actually reap | Is ownership clearer? |
| Number of independent lifecycle implementations | Did duplicate state, timeout and termination logic decrease? |
| Duplicated unsafe and descriptor code | Is it less likely that the same defect must be fixed in several places? |
| Number of bridges, queues and tasks | Does commonality avoid adding new buffering and complexity? |
| Runtime and build dependency changes | Confirm the cost of reuse on the actual graph |
| Test duplication and coverage | Were tests deleted, or merged into the common contract? |
| Scope of work for relevant upstream changes | Did the files and contracts to review on a pin change decrease? |

Mark unquantified development time as an estimate. A relative judgement such as "A1 costs little, A4 costs much" must also state which items it counted.

### 13.3 Platform verification and SLOs

Hosted CI runs the functional, compatibility and fault tests that environment can run. Cases that cannot run because of resource shortage or platform limits are recorded explicitly as `not_run`.

**Do not substitute a qualification-host success for hosted CI success, and do not read a hosted CI skip as a failure of the whole integration.**

The existing plan's status and termination response targets and its long-duration measurement procedure remain a separate integration verification. DevGuard's standalone qualification did not verify CodeSpace's approval, replay, output or control paths.

Record with every verification result:

```text
CodeSpace source SHA
DevGuard client source SHA
daemon/helper release and hash
Codex SHA
backend identification
wire/capability
operating policy
OS/architecture/execution host
tests actually run and not_run reasons
hashes of raw logs and reports
```

A documentation-only change need not invalidate DG-1 qualification. Conversely, if a runtime artifact changed, do not describe reusing the earlier verification as verification of the new implementation.

## 14. Changes by document

### 14.1 DevGuard PR #8

| Document | Revision |
| --- | --- |
| `docs/design.md` and Korean counterpart | The common execution contract, D1–D3, the current default dependency and the conditional reuse policy |
| `docs/planning/decisions.md` | New execution-layer decision and reuse policy; mark superseded scope so past decisions are not presented again as current guidance |
| `docs/planning/codespace-integration.md` | LaunchPlan, reap ownership, descriptors and guard, output contract, failure table, D2 exit condition |
| `docs/planning/milestones/CS-RG.md` | C00 and C09, the revised PR order and clear deliverables for each unit |
| `docs/planning/consumer-readiness.md` | Add the common lifecycle, mixed-mode safety and a completed D2 decision to the R3 entry conditions |
| `docs/planning/verification.md` | Test matrix, evidence levels, dependency measurement, backend fitness rules |
| `docs/planning/pr-delivery.md` | Rationale of this design revision, the distinction between documentation and code PRs, the final-head verification order |
| `docs/planning/README.md` | Summary of the changed decisions and work structure |
| `docs/contracts.md` | Only facts of the current implementation; never insert future APIs as if implemented |
| `README.md`, `docs/milestones.md` | Summary of the design revision and later integration stages |
| `AGENTS.md` | Relationship between the current design baseline and the historical approval; link the adaptation rules |
| `docs/translations.json` | Review and hash updates for the pairs actually changed |

The existing `decisions.md` contains the old `service-exec` description together with later implementation corrections. This revision **separates the rules now in force from historical explanation**, so that a first-time implementer does not take an old sentence as a current instruction.

#### Preserved

`docs/design.ko.md` and its historical approval checksum are unchanged.

The editorial design and the decision record instead state:

```text
Historical approval: preserves the decision of its time
Current editorial baseline: applies the revisions made at this user instruction
Implementation contract documents: describe only implemented behavior
Milestone state: updated as actual implementation and verification progress
```

Revising the design documents does not change CS-RG to `implemented` or `qualified`.

### 14.2 CodeSpace counterpart documentation PR

Changing DevGuard PR #8 alone would leave CodeSpace's existing roadmap as an outdated description. The same revision therefore includes a separate CodeSpace counterpart documentation change.

| Target | Change |
| --- | --- |
| `docs/devguard-integration.md` | Link the new design revision and work order |
| `docs/architecture.md` | Common execution coordination and backend ownership |
| `docs/execution-substrate.md` | Separate exit observation, reaping, output and resource lifetimes |
| `docs/codex-reuse.md` | Distinguish high-level spawn from reuse of low-level utilities |
| `docs/upstream-update.md` | Limited adaptation policy and re-examination procedure |
| Dependency check rules | Boundaries for the new resource client and backend dependencies |
| CI selection policy | Cover new pin, execution contract, guard and backend files |

Record immutable links in new documents **with the SHA of an actual commit once it exists**. Never invent revisions or PR numbers that do not exist yet.

## 15. Completion criteria before merging PR #8

PR #8 counts as a completed documentation revision only when:

**First, the authority and purpose of the review change.** The user's current instruction is recorded as the basis of the design re-evaluation, and past approval is not used to reject alternatives.

**Second, F1 covers the whole execution contract**: not only reaping, but descriptor passing, concurrent spawn, the output bridge and the maintenance branch.

**Third, D1–D3 remain independent decisions.** That CodeSpace owns the child directly does not automatically imply a permanent Codex ban in DevGuard or the replacement of every `off` backend.

**Fourth, D2 is a required exit gate.** The work plan requires the decision on actual integration or limited retention before final qualification.

**Fifth, the documents make no unverified claims.** Sentences such as "all contracts are currently met", "a few hundred lines solve it", "the latest pin solves it" or "ProcessDriver can be used right away" are removed or qualified to their evidence level.

**Sixth, design, implementation and qualification states stay distinct.** PR #8 changes the direction of implementation and verification but never marks runtime verification not yet performed as complete.

Documentation verification runs the existing checker and regression tests and updates the changed Korean counterparts and hashes. Results at the PR head and on main after merge are recorded separately.

## Final direction

This revision is **neither "keeping the earlier review's proposal and only strengthening its explanation" nor "forcing both repositories to change so that Codex becomes a common upstream"**.

It turns these four points into actual design decisions:

> **1. Make CodeSpace's execution state and ownership coordination common.**\
> Backend differences must not spread into duplicated approval, timeout, termination, output and cleanup logic.
>
> **2. Secure safe `required` execution first, but never leave the `off` branch unattended indefinitely.**\
> Before the final CS-RG verification, settle the grounds for integration or limited retention.
>
> **3. Judge Codex reuse by the actual contract, not by feature names.**\
> Reuse an implementation that fits, and manage adaptation of the needed scope traceably.
>
> **4. Keep DevGuard's current Codex-free implementation as the present engineering choice.**\
> Do not fix it as a permanent design prohibition for the whole repository, and document the conditions under which reuse benefits become concrete.

**With this revision, the maintenance optimization problem the user raised is no longer a long-term wish; it is part of this CS-RG's structure, work order and completion conditions.**

## Appendix: evidence for factual statements

Evidence level: code review at the fixed revisions, checked on 2026-09-27. Line numbers refer to those revisions. No runtime test was run for this revision.

| Statement | Source |
| --- | --- |
| The pinned high-level Codex spawns reap their child in their own task | Codex `6b9826e`: `codex-rs/utils/pty/src/pty.rs:244-253` and `:378-387`, `pipe.rs:280-299` |
| The pinned `ProcessDriver` bridge skips lagged items, and `ProcessHandle`'s Drop terminates | Codex `6b9826e`: `codex-rs/utils/pty/src/process.rs:427` and `:273-277` |
| Besides the waiter, CodeSpace's pipe timeout and termination paths call `try_wait` | CodeSpace `b6e7ed2`: `crates/runner/src/process.rs:366` (timeout), `:684` (`reap_child`), `:714` (`request_kill`) |
| The CodeSpace runtime paths mapped at `e94d214` are unchanged at `b6e7ed2`, except the patch helper's test reuse and two build-environment report keys | `git diff e94d214 b6e7ed2 -- crates scripts/validate-upstream.py third_party` in CodeSpace |
| `HelperCommand` exposes a `Command` for configuration and spawns through its own `spawn()`, which takes `spawn_guard`; `helper_command` takes the same guard while it creates the grant's descriptors; the guard is a standard `Mutex` | DevGuard `395315d`: `crates/client/src/launch.rs:130-155`, `:176`, `:29-37` |
| `LaunchOutcome::Started` also describes a helper killed between READY and exec | DevGuard `395315d`: `crates/client/src/launch.rs:69-73` |
| The CLI observes before reaping, records a failed observation, reaps, then observes again | DevGuard `395315d`: `crates/cli/src/exec.rs:345-356`, `:470-478` |
| The helper re-executes under the QoS clamp keeping its PID, process group and inherited descriptors; becoming the scope root is a no-op for a process already leading its group | DevGuard `395315d`: `crates/launch/src/main.rs:172-189`, `crates/macos/src/root.rs:13-25`, `:32-36` |
| The helper's own statuses are 125, 126 and 127 | DevGuard `395315d`: `crates/client/src/launch.rs:39-46` |
| `AbandonLaunch` releases only an unclaimed grant as `NoHelperCreated` | DevGuard [contracts](contracts.md), launch reconciliation |
| A standard `Mutex` locked again by the thread holding it does not return normally; `pre_exec` closures run in registration order in a restricted post-fork environment | Rust standard library documentation for [`Mutex::lock`](https://doc.rust-lang.org/std/sync/struct.Mutex.html#method.lock) and [`CommandExt::pre_exec`](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.pre_exec) |
