# Owner instruction: CS-RG units U1 to U6 (2026-10-02)

> **Status: dated record of an owner instruction; not maintained.** The owner gave this text in a Claude Code
> session's chat on 2026-10-02 at 13:54 UTC. It was not committed anywhere at the time. It is reproduced below
> unchanged, so every fact in it is as of that moment. It approves nothing beyond what it says, and later owner
> decisions may have superseded parts of it: the [CS-RG session handoff of
> 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what happened next and which parts still apply.

| Item | Detail |
| --- | --- |
| Given | In the same session, after U1 ([CodeSpace #82](https://github.com/novelKR/CodeSpace/pull/82)) had merged. |
| Covers | The definitions of CSRG-U1 to CSRG-U6, each with its objective, scope, prohibitions, definition of done and stop point, and the rule that P1-RECOVERY needs separate approval. |
| Outcome | The session compared the merged U1 with its section and reported deviations D-1 to D-5, which [CodeSpace #83](https://github.com/novelKR/CodeSpace/pull/83) closed. U2 was implemented in [CodeSpace #84](https://github.com/novelKR/CodeSpace/pull/84) under the more detailed [U2 instruction](2026-10-02-cs-rg-u2-instruction.md). U3 to U6 have not started. |
| Still applies | All of it for U3 to U6. It is the only written definition of those units. |

Copied from the session transcript byte for byte, except that the chat tool's wrapper around pasted text was removed. The original is one message. It is split here at the unit boundaries under added headings; nothing else is changed.

## Original text: CSRG-U1 — Status-only DevGuard opt-in integration

````text
CSRG-U1 — Status-only DevGuard opt-in integration
Implement the first real CS-RG integration unit in CodeSpace: a status-only, opt-in DevGuard connection.
This is an implementation task, not a research package or architecture exercise. The definition of done is a reviewable implementation PR containing product code, regression tests, dependency and pin enforcement, CI coverage, and a precise statement of what is and is not governed.
If live repository state shows that an equivalent U1 implementation has already been merged, do not duplicate it. Verify the merged implementation against this scope, report the exact merged commit and any material deviation, and stop.
1. Cold start
Before modifying anything:

1. Read the latest relevant status comments in:
   * `novelKR/CodeSpace#76`;
   * `novelKR/DevGuard#14`;
   * any current CS-RG implementation tracker.
2. Read the current CodeSpace:
   * `docs/devguard-integration.md`;
   * `docs/upstream-lock.md`;
   * `docs/upstream-update.md`;
   * Codex reuse and execution-substrate documentation.
3. Read DevGuard design revision 2 and the generic client/contract APIs that will actually be consumed.
4. Re-query:
   * CodeSpace `main`;
   * DevGuard `main`;
   * the CodeSpace Codex gitlink;
   * current CI policy and dependency gates.

Use live repository state as authoritative. Do not assume historical CSRG-C00/C03/C09 instructions have been reactivated.
2. Hard boundaries
Preserve both CodeSpace product semantics and DevGuard safety constraints.
Do not:

* change the CodeSpace Codex pin;
* add a new execution backend;
* transfer spawn, PTY, reaping, output, timeout, process-handle, or lifecycle ownership;
* add admission, `BeginLaunch`, permits, launch helpers, carriers, resource leases, or reaping integration;
* claim that DevGuard governs execution;
* expose DevGuard contract types through public MCP schemas;
* add a transitive Codex dependency through DevGuard;
* create a separate research PR or design-only PR.

CodeSpace must remain the execution owner.
3. Product behavior
Add a small CodeSpace-owned DevGuard adapter using the generic DevGuard client and contract.
The integration must be disabled by default at two levels:

* build-time: DevGuard product code is linked only when the dedicated CodeSpace feature is enabled;
* runtime: even in such a build, no DevGuard session is opened unless the operator explicitly enables status participation.

The enabled status-only path may perform only:

1. connect;
2. `Hello`;
3. `Authenticate`;
4. `Status`;
5. close the bounded session.

Expose the result through CodeSpace-owned runtime status, for example `workspace_info.resource_authority`.
The status must make clear that:

* provider is DevGuard;
* participation is status-only;
* `governs_execution` is false.

A DevGuard status failure must not fail unrelated CodeSpace tools or executions.
4. Status and error mapping
Provide stable CodeSpace-owned states for at least:

* available;
* unavailable;
* untrusted authority / peer mismatch;
* incompatible protocol or capability;
* credential refused;
* credential unavailable or invalid configuration.

Do not expose arbitrary DevGuard error messages to MCP clients.
Use DevGuard's wire error code where appropriate, but translate it into CodeSpace-owned public types.
5. Credential handling
The credential itself must never appear in:

* CLI flags;
* environment variables;
* logs;
* status responses;
* errors;
* child environments;
* child output.

Prefer an operator-provisioned private credential file.
Validate the credential file conservatively, including ownership/type/mode and symlink or equivalent substitution hazards supported by the platform.
Read the secret only when opening the bounded session and send it only in authentication.
6. Dependency and pin boundaries
Pin the exact DevGuard source revision consumed by the product and record it.
The implementation PR must add executable dependency checks proving that:

* every `devguard-*` product dependency comes from the reviewed DevGuard pin;
* every `codex-*` product dependency still comes from the CodeSpace Codex gitlink;
* DevGuard crates do not introduce CodeSpace or Codex crates into their own dependency closure;
* DevGuard is absent from the normal product graph when the feature is disabled;
* the CodeSpace Codex pin is unchanged.

Separate runtime/build dependencies from test-only fixture dependencies.
7. Descriptor and secret non-inheritance
Test against CodeSpace's actual spawners.
While DevGuard sessions are being opened, verify that the DevGuard session socket and credential do not reach:

* a pipe child;
* a PTY child;
* the UDS worker command.

Include:

* a deterministic positive control proving that the detector sees an intentionally inheritable socket;
* the protected CodeSpace spawners proving that the socket is excluded;
* a bounded concurrent stress case as supporting evidence, not as a universal probability claim.

Do not claim N2 launch safety from this test. U1 carries no permit or launch token.
8. Feature-off preservation
With the feature disabled:

* existing product dependency graphs must remain unchanged;
* existing MCP schemas must remain unchanged;
* existing CLI flags must remain unchanged;
* existing test suites must pass.

With the feature compiled but runtime participation disabled:

* no DevGuard session is opened;
* existing execution behavior remains unchanged.

9. CI and tests
Add CI coverage for:

* the DevGuard adapter;
* feature-enabled CodeSpace server tests;
* dependency/pin rules;
* macOS behavior where descriptor creation is relevant;
* normal feature-off regression.

Use DevGuard's real test authority where practical instead of a fake protocol implementation.
Preserve failed validation attempts as evidence. Do not weaken existing tests or timeouts merely to get green CI.
10. Completion
The completion criterion is:
A CodeSpace implementation PR at an exact head that adds a status-only, opt-in DevGuard integration, verifies pin/dependency boundaries, preserves feature-off behavior, maps status and errors, protects the credential, and proves actual child descriptor non-inheritance.
Do not merge without explicit exact-head approval.
Do not start registration, admission, launch, carrier, reaping, Codex pin evaluation, P1-RECOVERY, or Linux enforcement as part of U1.
At the stop point, report:

* exact PR head and base;
* current CodeSpace and DevGuard pins;
* CI results;
* dependency graph delta;
* feature-off result;
* credential and descriptor tests;
* known limitations;
* the exact next unit, U2, but do not start it.
````

## Original text: CSRG-U2 — Execution-owner registration and participation readiness

````text
CSRG-U2 — Execution-owner registration and participation readiness
Implement the next CS-RG unit: register the actual CodeSpace execution owner with DevGuard and establish the explicit resource-participation readiness policy.
This unit builds on a merged and verified U1. It must not implement admission or launch.
If U1 is not present on current CodeSpace `main`, stop before modifying code and report the missing prerequisite.
1. Cold start
Read current:

* CodeSpace `main`;
* U1 implementation and post-merge CI;
* DevGuard client/contract pin used by CodeSpace;
* CodeSpace DevGuard integration documentation;
* DevGuard design revision 2;
* current registration and provisioning contracts;
* current InProcess and UDS execution ownership.

Re-query all relevant pins and CI gates.
Do not resume the historical CSRG-C00/C02 implementation mechanically. Use the current product architecture and the U-series boundary.
2. Objective
Move from “CodeSpace can inspect DevGuard” to:
“The actual CodeSpace execution owner can establish and prove a registered DevGuard identity, while no workload is yet admitted or launched through DevGuard.”
The execution owner differs by mode:

* InProcess: the actual in-process execution owner;
* UDS: the actual worker process that owns execution handles.

Do not falsely register the Gateway as the workload owner when the worker is the execution owner.
3. Participation policy
Preserve the existing status-only mode.
Introduce the resource participation policy needed by later units, conceptually:

* `off`;
* `required`.

However, U2 does not yet implement admission or managed launch.
Therefore:

* do not allow `required` to silently run an ungoverned workload;
* if `required` is selected before the later admission/launch path is available, fail explicitly as not yet supported for execution rather than falling back to legacy execution;
* status-only observation remains separate from required resource participation.

Document this distinction precisely.
4. Registration
Implement bounded owner registration using DevGuard's current contract.
Establish and test:

* consumer identity;
* generation;
* CodeSpace instance identity;
* actual execution-owner PID or equivalent identity;
* expected role;
* capability negotiation;
* protocol compatibility;
* static control reservation semantics where applicable.

A session is not an instance. Reconnecting must preserve the same intended owner identity rather than inventing a new workload identity.
5. InProcess and UDS credential handling
InProcess and UDS must both be correct.
For UDS, design and implement only the minimum private credential handoff required for the execution-owning worker to authenticate.
The credential must:

* never reach the user payload;
* never appear in MCP input;
* never appear in logs or status;
* never remain in an unrelated process;
* never be inherited by unrelated children.

Do not introduce a generic secret transport mechanism beyond what this unit requires.
6. Failure behavior
Distinguish at least:

* authority unavailable;
* peer/identity mismatch;
* protocol mismatch;
* missing required capability;
* credential refused;
* registration refused;
* malformed or invalid configuration;
* unsupported platform or mode.

Do not silently convert `required` into `off` or `status`.
Existing process query and termination paths must not become dependent on a fresh registration request.
7. Tests
Cover at least:

* InProcess registration;
* UDS worker registration;
* feature/runtime off behavior;
* stable identity across bounded sessions;
* wrong PID/owner identity;
* wrong consumer/generation;
* wrong peer UID;
* protocol mismatch;
* capability mismatch;
* credential refusal;
* credential non-leakage;
* concurrent startup and registration;
* worker startup failure;
* session socket and credential non-inheritance;
* required mode refusing to execute while managed admission/launch is not yet implemented.

Use real DevGuard fixture authority behavior where practical.
8. Dependency and architectural constraints
Do not:

* move CodeSpace execution ownership;
* add a managed execution backend;
* change the Codex pin;
* add `Admit` or `BeginLaunch`;
* add permits or carriers;
* alter PTY ownership;
* alter reaping.

If registration cannot be implemented for one supported CodeSpace mode without violating these boundaries, mark that mode unsupported in this unit and report the exact conflict.
9. Milestone bookkeeping
Because U1 has already established real CS-RG product code, update stale milestone/tracker wording so CS-RG is represented as `in-progress`, not `not-started`.
Do not rewrite historical design-revision text. Add dated supersession/current-state wording instead.
10. Completion
The definition of done is:
An implementation PR in which the actual CodeSpace execution owner can establish a verified DevGuard registration in each supported mode, with conservative failure handling, private credential transport, and an explicit participation policy that never runs an ungoverned workload under `required`.
Do not merge without explicit exact-head approval.
Do not start admission or launch work.
At completion, report the exact next unit: U3, admission and attempt/slot preparation.
````

## Original text: CSRG-U3 — Admission, attempt identity, and pre-spawn slot lifecycle

````text
CSRG-U3 — Admission, attempt identity, and pre-spawn slot lifecycle
Implement the CS-RG admission and preparation layer.
This unit builds on merged U1 and U2. It introduces real resource admission state and pre-spawn execution identity, but it does not yet activate the final DevGuard-managed launch path.
If U1 or U2 is absent from current `main`, stop before modifying code and report the missing prerequisite.
1. Objective
Implement the CodeSpace-owned preparation model required before a governed process can be launched:
authorization/workspace readiness → CodeSpace process slot → DevGuard admission → one-shot prepared execution.
No process may be spawned merely because admission state exists.
The prepared object must be a real product abstraction, not a scratch experiment.
2. Preserve ownership boundaries
CodeSpace continues to own:

* authorization;
* workspace policy;
* process slots;
* process handles;
* PTY selection;
* output;
* timeout;
* termination;
* reaping.

DevGuard owns:

* admission;
* reserved resource state;
* durable attempt identity/accounting;
* pressure/resource-policy decision.

Do not move spawn/reaping ownership into DevGuard.
3. Attempt identity
Define a one-shot attempt identity containing the minimum stable meaning needed by both systems.
Include, as appropriate:

* consumer;
* generation;
* attempt ID;
* CodeSpace operation/process identity;
* workspace identity;
* tty/non-tty meaning;
* requested resource policy;
* canonical digest over execution-relevant inputs such as argv, cwd, environment, tty, workspace, and resource policy.

The identity must prevent a prepared attempt from being silently reused for a different command.
Do not use transport request IDs as durable attempt identity.
4. Process slot before spawn
Move the relevant capacity decision before spawn.
Tests must prove:

* no child is created without an available CodeSpace slot;
* no DevGuard admission is treated as a process handle;
* slot lifetime, resource-attempt lifetime, workspace lifetime, and completed-output lifetime are distinct.

A ninth or otherwise over-limit request must be rejected before process creation.
5. Admission
Implement bounded `Admit` preparation through the registered owner.
Record and preserve the distinction among:

* requested;
* supported;
* reserved;
* applied;
* method/scope/required level, where available.

At U3, “applied” may remain not-yet-launched. Do not claim runtime enforcement before U4.
Handle:

* accepted preparation;
* resource shortage;
* unsupported required policy;
* authority unavailable;
* timeout;
* lost reply;
* attempt expiry;
* cancellation before launch.

6. One-shot prepared execution
Create a CodeSpace-owned `PreparedExecution` or equivalent abstraction.
It must:

* own its execution meaning;
* be consumable once;
* not be cloneable into another launch;
* preserve the original deadline/attempt identity;
* have explicit cancellation/expiry behavior;
* never infer lease release merely from local task cancellation.

Do not reconstruct a prepared execution from argv after uncertainty.
7. Required participation
U3 may make `required` admission operational, but it must still not execute through an unverified legacy launch path.
Until U4 is merged:

* either keep actual governed payload launch disabled;
* or return an explicit “managed launch not yet available” result after successful preparation and cleanly cancel the unstarted attempt.

Never admit and then silently run through a non-governed fallback.
8. Failure and uncertainty
Preserve these hard rules:

* timeout is not proof that admission failed;
* lost reply is not proof that nothing was reserved;
* EOF is not proof that no attempt exists;
* an uncertain attempt is not replayed;
* a prepared attempt is never rebound to another command;
* unknown execution state remains conservative.

Use DevGuard's existing cancel/abandon semantics only where their preconditions are actually established.
9. Tests
Cover:

* successful prepare;
* insufficient resources;
* unsupported policy;
* unavailable authority;
* occupied CodeSpace slot;
* concurrent over-limit request;
* attempt expiry;
* cancellation before launch;
* lost reply;
* timeout;
* duplicate/replayed request;
* digest mismatch;
* task cancellation;
* workspace release versus attempt release;
* no child created in any U3-only path.

Add deterministic regression tests rather than source-only arguments.
10. Completion
The definition of done is:
A product implementation PR that creates a one-shot, admitted, pre-spawn CodeSpace execution preparation with correct slot/resource/attempt identity and conservative uncertainty handling, while spawning no governed payload yet.
Do not merge without explicit exact-head approval.
Do not implement `BeginLaunch`, permit carriers, helper launch, observation/reaping integration, or a Codex pin change.
At completion, request approval for U4.
````

## Original text: CSRG-U4 — Managed launch, BeginLaunch, helper, and carrier integration

````text
CSRG-U4 — Managed launch, BeginLaunch, helper, and carrier integration
Implement the first actual DevGuard-governed payload launch in CodeSpace.
This unit builds on merged U1 through U3.
It is the point where `required` participation may become capable of actually executing a workload, so N1 and N2 are equally hard requirements.
If the prerequisites are not present on current `main`, stop before modifying code.
1. Objective
Implement:
consume one prepared CodeSpace execution → `BeginLaunch` → one-time launch authority → CodeSpace-owned spawn path → DevGuard helper/scope setup → payload attempt.
Do not build a second CodeSpace execution subsystem just to satisfy DevGuard.
2. Hard ownership constraints
Preserve:

* CodeSpace owns process and PTY semantics;
* CodeSpace owns process handles;
* CodeSpace owns output collection;
* CodeSpace owns timeout and termination;
* CodeSpace owns reaping;
* Agent/session lifetime remains independent from process lifetime;
* DevGuard remains resource admission/accounting authority, not a generic process server.

Do not introduce a DevGuard-driven replacement PTY backend or generic native process backend.
The existing pipe path remains Tokio/std-based unless a separately justified CodeSpace requirement changes it.
3. BeginLaunch
For one prepared attempt:

* call `BeginLaunch` exactly once;
* treat the first successful launch authorization as one-shot;
* never replay it after an uncertain response;
* bind it to the prepared attempt identity/digest;
* preserve deadline and cancellation meaning.

A lost response must not be converted into a fresh `BeginLaunch`.
4. Helper and carrier
Select and implement the smallest carrier mechanism that satisfies N1 and N2.
It must safely carry only the data required by the DevGuard launch helper, such as:

* one-time permit;
* transcript/evidence channel;
* any required launch metadata.

The private material must:

* be inherited only by the intended helper stage;
* not reach unrelated children;
* not reach the user payload after the helper boundary;
* be closed on every failure path;
* survive only as long as the launch protocol requires.

Use current evidence from #79/#81 and D6 analysis, but make the carrier decision inside this implementation PR rather than opening another paper-only package.
5. Codex 0.154 and 0.159 rule
Start from the current CodeSpace Codex pin.
Do not change the pin merely because 0.159 has a more convenient API.
First attempt the implementation within the approved CodeSpace execution boundary.
If the current pin lacks a primitive that is genuinely necessary to satisfy N1 and N2 simultaneously:

1. identify the exact missing primitive;
2. show why an adapter or current public boundary cannot satisfy it;
3. show why a pin update or small generic upstream capability is the smallest solution;
4. stop and request separate pin-change authorization.

Do not silently update Codex.
Do not treat a successful scratch experiment as pin approval.
6. Launch phases
Represent launch phases distinctly.
At minimum distinguish:

* prepared;
* launch committed;
* helper created;
* scope bound;
* policies applied;
* run authorized / helper ready;
* payload exec attempted;
* payload running or failed;
* draining;
* released.

Do not collapse helper readiness and successful payload exec into one event.
7. No-helper-created evidence
Handle the case where the helper was definitely not created or definitely never reached payload attempt.
Only use a release/abandon result when the evidence satisfies the DevGuard contract.
A timeout, lost permit response, missing process handle, or communication EOF alone must not be treated as proof that execution did not occur.
8. Pipe and PTY
Test both.
For PTY preserve:

* initial size;
* resize;
* session/controlling-terminal semantics;
* process-group behavior;
* EOF/output semantics;
* existing CodeSpace PTY ownership.

For pipe preserve:

* stdin/stdout/stderr;
* output pumps;
* process handles;
* timeout and termination behavior.

Do not claim one mode supports required participation unless it passes the full launch path for that mode.
9. D6 verification
Verify actual concurrent launches against CodeSpace's real spawners.
Include deterministic controls for:

* unrelated descriptor inheritance;
* permit/credential leakage;
* transcript writer/reader leakage;
* helper-only inheritance;
* payload exclusion.

Do not infer complete N2 merely from #81. Test the new carrier itself.
10. Tests
Cover at least:

* normal pipe launch;
* normal PTY launch;
* helper spawn failure;
* helper setup failure;
* `READY`;
* payload exec failure;
* lost `BeginLaunch` response;
* duplicate launch attempt;
* cancellation before helper;
* cancellation after commit;
* concurrent unrelated spawns;
* carrier cleanup;
* credential and permit non-leakage;
* exact one-shot behavior;
* no replay under uncertainty.

11. Completion
The definition of done is:
A reviewable product PR in which a prepared CodeSpace attempt can be launched exactly once under DevGuard resource authority through CodeSpace-owned pipe and/or PTY execution semantics, with safe private carriers and no execution-ownership transfer.
State clearly which modes are supported and which remain unsupported.
Do not merge without explicit exact-head approval.
Do not yet claim final lifecycle/release qualification. That is U5.
````

## Original text: CSRG-U5 — Observation, reaping, release, approvals, and no-replay lifecycle

````text
CSRG-U5 — Observation, reaping, release, approvals, and no-replay lifecycle
Complete the governed execution lifecycle after launch.
This unit builds on merged U1 through U4.
Its purpose is to make exit, observation, reaping, resource release, approval state, and uncertainty consistent across all success and failure races.
1. Objective
Implement one coherent lifecycle satisfying:

* exactly one actual reaper per process;
* observation and reaping remain distinct where required for DevGuard evidence;
* root exit is not automatically resource-scope completion;
* resource release is based on actual scope/evidence state;
* uncertain work is never replayed.

This is product code, not a lifecycle study.
2. Reaper ownership
Inventory all current paths that can cause or observe process exit, including:

* normal wait;
* timeout;
* user termination;
* workspace termination;
* shutdown;
* task cancellation;
* backend drop;
* helper failure;
* UDS disconnect;
* PTY completion.

Modify the implementation so only one owner reaps each child.
Other paths send intents or observe state but do not compete for `wait`/`waitpid`.
Add deterministic double-reap prevention tests.
3. Observation before reap
Where DevGuard requires pre-reap root-exit evidence, provide the smallest generic hook consistent with CodeSpace ownership.
Do not move the reaper into DevGuard.
If the current CodeSpace/Codex public boundary cannot expose the needed observation while preserving the existing reaper:

* identify the exact gap;
* evaluate the smallest generic adapter or upstream primitive;
* do not create a DevGuard-specific replacement backend;
* stop for separate approval if a pin/upstream change is required.

Observation timeout or failure must not create a second reaper.
4. Root exit versus scope end
Preserve separate events for:

* root process exit;
* output EOF;
* process-handle cleanup;
* workspace release;
* DevGuard resource-scope end;
* lease/accounting release.

A surviving descendant means root exit alone is insufficient evidence for release.
Where the execution scope cannot be proven ended, keep the attempt charged/suspect according to DevGuard semantics.
5. Existing-process control during authority failure
After a process has started, DevGuard authority failure must not prevent CodeSpace from:

* reporting process status from its own handle/state;
* accepting termination requests;
* enforcing existing timeout behavior.

New admissions may fail closed while existing process control remains available.
Test this explicitly.
6. Approval and dispatch semantics
Bind CodeSpace approvals to the durable attempt lifecycle.
Preserve:

* admission refusal does not consume an approval hold;
* an attempt proven not started may permit reuse according to existing approval semantics;
* uncertain launch never permits automatic replay;
* lost replies never recreate a new attempt from argv;
* process-handle loss is not proof of no execution;
* helper `READY`, payload exec attempt, and running state are distinct.

If CodeSpace stores a dispatching state, make it correspond to the correct launch boundary.
7. No-replay rules
Test all ambiguity cases:

* timeout after launch commit;
* lost helper transcript;
* connection EOF;
* Gateway-side task cancellation;
* process handle unavailable;
* root exit before observation;
* authority restart;
* duplicate client request.

None may cause automatic execution replay unless there is positive evidence that the original matching attempt never started and the contract explicitly allows reuse.
8. Release handling
Verify release for:

* clean exit;
* exec failure;
* terminated process;
* helper-created-but-no-payload;
* surviving descendants;
* observer timeout;
* authority unavailable;
* shutdown;
* uncertain scope.

Ensure ledger/accounting state is committed before CodeSpace reports a critical release result where the DevGuard contract requires durability.
9. Race tests
Include races among:

* exit and terminate;
* exit and timeout;
* exit and shutdown;
* root exit and surviving descendant;
* Observe and reap;
* helper transcript and process exit;
* UDS disconnect and completion;
* cancellation and launch result;
* authority loss and process termination.

Use actual process trees where practical.
10. Completion
The definition of done is:
A governed CodeSpace execution has one reaper, conservative pre-reap observation, correct approval/no-replay semantics, distinct process/workspace/resource lifetimes, and correct release behavior across normal, failure, descendant, cancellation, timeout, and authority-loss cases.
Do not merge without explicit exact-head approval.
Do not start P1-RECOVERY in this unit.
Do not claim CS-RG qualification yet. Final parity, fault, and qualification work belongs to U6.
````

## Original text: CSRG-U6 — Parity, fault qualification, and CS-RG completion

````text
CSRG-U6 — Parity, fault qualification, and CS-RG completion
Complete and qualify the CS-RG milestone on the implementation produced by U1 through U5.
This is not a new architecture phase. It is the final product qualification and bounded correction stage.
If any prerequisite unit is not merged and verified, stop and report the missing prerequisite.
1. Objective
Establish exactly what CodeSpace + DevGuard combination is supported for actual runtime resource governance and qualify that combination.
The final result must distinguish:

* implemented;
* feature-verified;
* platform-verified;
* qualified.

Do not convert passing unit tests into an unsupported platform-wide claim.
2. Freeze the qualification candidate
Before running qualification, record the exact:

* CodeSpace commit;
* DevGuard client pin;
* DevGuard daemon/helper artifact or source identity as applicable;
* Codex gitlink;
* Rust/toolchain;
* wire/protocol version;
* feature/configuration;
* platform/host;
* supported execution modes.

If code changes after the protocol is frozen, produce a new candidate record and rerun affected qualification.
3. Supported-mode matrix
Qualify each claimed combination separately.
At minimum consider:

* InProcess / UDS;
* pipe / PTY;
* `off` / `required`;
* normal admission;
* resource shortage;
* authority unavailable;
* unsupported capability;
* normal exit;
* termination;
* timeout;
* descendants;
* helper or payload failure.

Do not infer one mode from another.
Linux resource enforcement is not completed by CS-RG unless DG-LINUX has separately qualified it.
If Linux still lacks the required enforcement contract, report `required` Linux governance as unsupported rather than silently cooperative.
4. Legacy/off-path decision
Review the legacy `off` execution path after the required path exists.
Do not force migration merely for architectural neatness or because DevGuard exists.
Either:

* retain the existing off path as a limited compatibility path, documenting its scope and revisit/removal conditions; or
* converge/remove duplicated pieces only where an independent CodeSpace product reason and regression evidence justify it.

Do not move PTY/process ownership just to reduce code duplication.
5. Fault qualification
Run bounded fault cases including:

* admission refusal;
* authority loss before admission;
* authority loss after process start;
* launch response loss;
* helper failure;
* helper `READY` followed by exec failure;
* observer timeout;
* surviving descendant;
* terminate/exit race;
* shutdown/exit race;
* UDS disconnect;
* stale/duplicate request;
* uncertain execution;
* credential failure;
* journal/storage refusal where relevant.

Verify no replay and no premature release.
6. Control-path availability
Measure and verify that resource governance does not prevent control of existing work.
Under pressure and authority failure, measure:

* process status latency;
* termination acknowledgement latency;
* actual scope termination separately;
* output/control responsiveness;
* new-admission rejection latency.

Do not combine acknowledgement latency with actual process-tree termination time.
Preserve the established target definitions rather than silently redefining them.
7. Resource-accounting qualification
Verify:

* requested/reserved/supported/applied reporting;
* headroom and control reservations;
* correct pressure behavior;
* no release on root exit alone when descendants remain;
* conservative accounting for unknown execution state;
* no replay after uncertain launch;
* correct cleanup after definite no-start cases.

Use actual DevGuard state/evidence, not only CodeSpace-visible process status.
8. Regression and parity
Run existing CodeSpace regressions for:

* workspace authorization;
* approvals;
* pipe execution;
* PTY execution;
* stdin/output;
* resize;
* timeout;
* termination;
* shutdown;
* patch helpers;
* sandbox helpers;
* UDS worker;
* feature-off behavior.

The resource-governed path must not weaken these contracts.
9. Evidence discipline
Freeze the protocol before qualification.
Preserve:

* successful runs;
* failed runs;
* amendments;
* raw measurements;
* exact inputs;
* logs;
* artifact/source hashes.

Do not rewrite sealed evidence.
Do not convert bounded empirical results into universal failure probabilities.
10. Milestone records
When and only when the final candidate satisfies the declared CS-RG completion conditions:

* update the current milestone ledger from `in-progress` to the appropriate implemented/qualified state;
* record the exact qualified combination;
* update current integration documentation;
* preserve historical suspended CSRG-C00/C03/C09 text as historical provenance rather than pretending those old work units were executed unchanged.

State explicitly what remains outside CS-RG, especially:

* P1-RECOVERY;
* DG-LINUX enforcement;
* DG-CACHE;
* DG-ADAPTERS;
* any deferred Codex convergence.

11. Rollback
Define a rollback that:

* returns new admissions to a safe supported state;
* does not replay or forget uncertain live work;
* preserves existing process control;
* preserves DevGuard durable accounting;
* does not silently change `required` into `off`.

12. Completion
The definition of done is:
The exact CodeSpace/DevGuard/Codex candidate is qualified for the explicitly supported CS-RG modes, with fault, race, lifecycle, resource-accounting, latency, feature-off, and parity evidence preserved, and the milestone ledger accurately records the result.
Do not merge the final qualification or milestone-state change without explicit exact-head approval.
After CS-RG is completed, stop.
The next critical-path milestone is P1-RECOVERY. Do not start it without a separate owner authorization.
````
