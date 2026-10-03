# Owner instruction: take over the DevGuard and CodeSpace work (2026-09-30)

> **Status: dated record of an owner instruction; not maintained.** The owner gave this text in a Claude Code
> session's chat on 2026-09-30 at 08:43 UTC. It was not committed anywhere at the time. It is reproduced below
> unchanged, so every fact in it is as of that moment. It approves nothing beyond what it says, and later owner
> decisions may have superseded parts of it: the [CS-RG session handoff of
> 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what happened next and which parts still apply.

| Item | Detail |
| --- | --- |
| Given | As the first message of a new Claude Code session on the owner's Mac, used through Remote Control. |
| Covers | The mandatory cold-start reading order, the state expected after #17, settled owner policy (sections 3 to 6), the W3 findings to verify, CodeSpace #79, the CI issue found during #17, the W4/W5 phase gate, documentation obligations and the evidence hierarchy. It asks for a takeover report and a stop at the decision points. |
| Outcome | The session reported back. The owner answered with the [continuation instruction](2026-09-30-continuation-instruction.md) of 11:26 UTC the same day. |
| Still applies | The settled dependency policy and what it does not approve (section 3), the joint constraints N1 and N2 (section 4), the pin and upstream policy (section 6) and the evidence hierarchy (section 13). Its W4/W5 phase gate predates the CS-RG unit plan of 2026-10-02. |

Copied from the session transcript byte for byte, except that the chat tool's wrapper around pasted text was removed.

## Original text

````text
You are taking over an existing DevGuard / CodeSpace engineering investigation from a previous agent.
Treat this as a cold start. Do not assume that any conversational context, local notes, prior-agent memory, or previous agent conclusion is current or authoritative until you re-read the GitHub state yourself.
GitHub is the authoritative continuation surface. Local evidence is supporting audit material, not a substitute for current repository state or owner decisions.
1. Mandatory cold-start order
Start in exactly this order.

1. Read the latest comment on:
   * `novelKR/DevGuard#14`
2. Read:
   * `novelKR/DevGuard#17`
   * its current PR state;
   * exact head SHA;
   * PR body;
   * `docs/handoff/2026-09-28-w3-decision-packet.md`;
   * current checks / CI results.
3. Before doing any W4 work, normative policy/design work, dependency or pin work, upstream work, or production-integration work, read the merged W0–W2 records from DevGuard `main`, especially:
   * `docs/handoff/2026-09-28-cs-dg-upstream-adapter-work-spec-1.md`
   * `docs/handoff/2026-09-28-upstream-adapter-packet.md`
   * `docs/handoff/2026-09-28-cs-dg-upstream-adapter-start-instruction.md`
These files were carried by DevGuard #16 and preserve the detailed:
   * flexible upstream-pin policy;
   * adapter-placement rules;
   * dependency-boundary rules;
   * same-executable versus separate-executable pin rules;
   * qualification and rollback requirements;
   * translation/documentation obligations;
   * executable dependency/CI gate requirements;
   * W4/W5 phase gates.
Treat their W0–W3 execution instructions as dated/completed where later GitHub state says that work has already happened, but keep all still-applicable architecture, dependency, qualification, promotion and safety constraints.
4. Read the latest comment on:
   * `novelKR/CodeSpace#76`
5. Read:
   * `novelKR/CodeSpace#79`
6. Re-query live state:
   * DevGuard `main`;
   * CodeSpace `main`;
   * DevGuard #17 head/base/state/mergeability/checks;
   * CodeSpace #79 state;
   * the CodeSpace Codex gitlink / pinned Codex revision.

Do not rely on issue bodies alone. Bodies and older comments are dated records; later comments are chronological deltas.
Do not rely on SHA values in this instruction if GitHub has moved since it was written.
2. Current expected handoff state
At the previous session close, the expected state was approximately:

* DevGuard #13: merged.
* DevGuard #16: merged.
* CodeSpace #78: merged.
* DevGuard #17:
   * open;
   * unmerged;
   * non-normative W3 decision record;
   * expected exact head:
`70c814c93a96f383e3241b6817fe6d42aea9ea48`;
   * documentation-only repository change;
   * not owner-approved for merge.
* CodeSpace #79:
   * open;
   * investigation/proposal only;
   * no CodeSpace diagnostic or implementation authorized.
* Experiments A, B and C were completed as bounded experiments.
* No production CodeSpace–DevGuard integration was implemented.
* No production Codex pin change was made.
* No new production dependency was adopted.
* No Codex upstream submission was made.

Verify every item against live GitHub before using it.
3. Settled owner policy — do not ask again
The following general permission is already settled:
DevGuard may use Codex and other external dependencies behind explicit adapter boundaries, using reviewed immutable source identities and flexible pins where justified.
Do not ask the owner again whether DevGuard is allowed in principle to depend on Codex or another external implementation.
The old “DevGuard must remain Codex-free” position is historical policy that required reconciliation; it is not a permanent owner prohibition.
Likewise, the CodeSpace Codex pin is not immutable in principle. A reviewed pin update is allowed when justified and qualified.
This settled permission does not mean any of the following are already approved:

* a particular dependency;
* a particular package or crate;
* a particular Codex tag or commit;
* a particular downstream patch;
* a production pin move;
* a merge;
* a service change;
* a runtime integration;
* an upstream submission;
* a release or rollout.

Those remain separately controlled decisions.
4. Joint architectural constraints
Keep N1 and N2 jointly mandatory.
N1 — CodeSpace
Preserve:

* established CodeSpace product semantics;
* existing responsibility boundaries;
* selective Codex upstream delegation;
* independent governance-free `off` behavior;
* CodeSpace ownership of:
   * MCP semantics;
   * workspace authorization;
   * approvals;
   * logical process handles;
   * output/result behavior;
   * Gateway/Runner product behavior.

Do not introduce merely for DevGuard:

* a CodeSpace-owned PTY backend;
* a CodeSpace-owned OS reaper;
* a replacement native process runtime;
* a rewrite of the existing pipe execution path;
* lifecycle ownership transfer from an existing execution backend;
* a hidden backend inside a supposedly thin adapter.

N2 — DevGuard
Preserve:

* admission and accounting integrity;
* stable attempt identity;
* one-time launch authorization;
* credential and permit safety;
* uncertainty semantics;
* no replay of uncertain execution;
* correct resource evidence;
* conservative reconciliation;
* one actual process reaper;
* no false resource release;
* normal resource recovery for supported healthy workloads.

Neither N1 nor N2 has priority over the other.
If a platform / transport / execution-backend combination cannot satisfy both, classify that combination as unsupported rather than weakening either invariant.
5. Adapter and dependency principles that remain applicable
The merged W0–W2 specification contains the detailed version of these rules. Preserve them.
Same executable
If CodeSpace and a DevGuard-specific binding use Codex inside the same executable:

* inspect the resolved source identities;
* inspect Cargo feature unification and package identities;
* avoid unintentionally loading incompatible or duplicate Codex implementations;
* prefer one reviewed source identity where the components must interoperate;
* do not assume matching SHAs alone prove behavioral compatibility.

Separate executables
If CodeSpace and DevGuard daemon/helper/CLI are separate executables:

* their internal dependency pins do not automatically need to move in lockstep;
* qualify the relevant:
   * client;
   * wire;
   * helper;
   * capability;
   * artifact
combination.

A DevGuard-only upstream update must not silently move CodeSpace's gitlink.
A CodeSpace-only upstream update must not silently replace the installed DevGuard release.
Adapter boundary
Codex-specific types should remain isolated behind explicit bindings/adapters where practical.
A thin CodeSpace resource adapter may own things such as:

* opt-in configuration;
* attempt binding;
* capability negotiation;
* error/result translation;
* evidence/outcome submission;
* consumption of generic execution-backend events.

It must not become owner of:

* PTY allocation;
* native process spawn;
* reaping;
* terminal/session/process-group setup;
* generic lifecycle mechanics

merely to satisfy DevGuard.
6. Pin and upstream policy
Preserve the flexible reviewed-pin model from the merged specification.
Valid candidate classes can include:

* released tags resolved to immutable commits;
* explicit unreleased/prerelease commits;
* registry packages with locked versions/checksums;
* small temporary downstream patches when justified and tracked upstream-first.

A floating branch or mutable PR head is not a promoted production identity.
For any real pin/dependency change, require reviewable evidence for:

* source identity;
* dependency graph;
* features;
* target/platform;
* adapter compatibility;
* wire/helper/journal/config compatibility;
* provenance/license implications;
* qualification;
* rollback;
* local divergence;
* removal/reconvergence conditions.

Compile success is not behavioral qualification.
A successful experiment is not a promoted dependency.
7. W3 findings to verify, not blindly inherit
The W3 packet recorded the following bounded results.
Experiment A
It reported that:

* launch preparation could be separated from actual spawn while keeping DevGuard's existing helper/wire/authority;
* Codex's PTY `ChildFds::Attached` path could consume those launch attachments;
* no child PID accessor was required for the demonstrated binding;
* single-use / first-claim-wins / no-replay / known-not-started semantics were preserved in the amended experiment;
* the original frozen FIFO carrier design failed on the tested macOS host;
* later cases used a protocol amendment;
* an atomic regular-file permit was viable in the bounded experiment.

Do not describe the original frozen FIFO protocol as a pass.
Do not treat the experimental preparation module as a production API.
Experiment B
It reported that:

* a scratch generic Codex PTY observation mechanism allowed exit observation before final reap;
* the Codex waiter remained the only actual reaper;
* healthy survivor cases recovered reservations in the bounded experiment;
* observer stalls/crashes/unavailable authority degraded conservatively;
* the patch was scratch-only and was not submitted upstream.

Do not turn this into a CodeSpace reaper.
Do not treat upstream acceptance as established.
Experiment C
It reported that:

* unrelated std/Tokio children could inherit transient DevGuard session/carrier descriptors in the tested concurrent macOS harness;
* an inherited session endpoint could expose permit-bearing replies and permit request injection in the synthetic authority experiment;
* kernel close-by-default spawning removed the tested inheritance classes in the bounded samples;
* an atomic regular-file permit removed the permit-carrier leak class;
* `spawn_guard` protects only code participating in that same process-wide coordination.

These are bounded behavioral observations, not universal proofs.
8. Current unresolved technical area
The smallest macOS PTY/InProcess integration candidate became more concrete after W3, but no candidate has yet been declared to satisfy both N1 and N2.
The major remaining issue is D6 / descriptor and session safety.
Current candidate directions include, without approval:

* close-by-default spawning;
* DevGuard client-session redesign;
* both;
* first running CodeSpace #79's bounded diagnostic.

Do not select a final remedy merely because Experiment C showed one mechanism working in a scratch harness.
CodeSpace-side descriptor hygiene must remain independently justified on CodeSpace's own correctness terms.
9. CodeSpace #79
CodeSpace #79 is an independent CodeSpace correctness/hygiene investigation.
It is not a CS-RG implementation issue.
Its rationale includes possible:

* delayed stdout/stderr EOF;
* lost stdin EOF;
* PTY lifetime extension;
* long-lived inherited descriptors.

At session close:

* the issue was open;
* its proposed bounded diagnostic had not been owner-authorized;
* no CodeSpace implementation had been made.

Any future implementation under #79 requires its own approval.
10. CI issue discovered during #17
PR #17 is a documentation-only W3 record, but the existing DevGuard workflow ran the complete Ubuntu/macOS qualification path.
One PR-triggered macOS run failed in:
`a_drain_that_does_not_finish_in_time_keeps_the_current_release_and_its_charges`
at:
`started.elapsed() < Duration::from_secs(10)`
The expected `ResourceUnavailable` result and `"did not finish"` message had already been produced.
A same-tree push-triggered macOS run succeeded, including the same test in both the workspace-contract stage and native upgrade qualification.
The previous session therefore classified this narrowly as:
a timing-sensitive intermittent CI failure not causally tied to the documentation-only #17 change.
Do not call it a historically known flaky test unless new evidence establishes that.
Two independent follow-up tracks were identified but not started:

1. DevGuard affected-check / selective CI planning
2. DevGuard upgrade-timeout test hardening

Keep both separate from the CS-RG architecture track.
11. W4 / W5 phase gate
The original full specification explicitly distinguishes:
W4 — candidate selection and normative integration proposal
Purpose:

* compare only candidates that can plausibly satisfy N1 and N2;
* evaluate:
   * lifecycle;
   * liveness;
   * maintenance;
   * release coupling;
   * dependency/pin consequences;
* produce an owner decision packet;
* prepare coordinated normative design/policy PRs only when authorized.

W5 — production implementation and promotion
Purpose:

* implement one complete approved integration slice;
* select a reviewed source/dependency combination;
* run relevant full regressions;
* qualify supported platforms;
* produce artifact/provenance/rollback evidence;
* perform controlled promotion.

Important:

* W3 completion does not authorize W5.
* W4 authorization does not automatically authorize W5.
* A scratch upstream patch is not a production dependency.
* Merging a research/handoff record does not authorize implementation.
* A normative policy PR does not by itself authorize deployment.
* Production dependency and pin changes remain separately reviewed.

The project is currently at the post-W3 owner gate, before W4 is fully authorized unless live GitHub state records a later owner decision.
12. Documentation and gate obligations
Before proposing normative policy or dependency changes:

* inspect the actual executable dependency graph;
* inspect Cargo feature resolution;
* inspect existing CodeSpace and DevGuard CI/dependency gates;
* do not assume an optional dependency is isolated merely because it is declared optional;
* preserve required English/Korean documentation synchronization where repository policy requires it;
* do not rewrite dated historical records to make them agree with newer policy;
* instead, add dated superseding/clarifying records.

Do not silently make existing evidence appear as if it had been produced under a later policy.
13. Evidence hierarchy
Keep these categories separate:

1. Owner requirements and explicit decisions — normative.
2. Current merged source / live GitHub state — implementation and repository fact.
3. W0–W3 packets and experiments — evidence and proposals unless separately adopted.
4. Local raw evidence — supporting audit material.
5. Agent interpretation — inference until verified.

A successful experiment does not authorize production implementation.
A green CI run does not authorize a merge.
A mergeable PR does not imply owner approval.
A proposal does not become a requirement merely because it is technically plausible.
14. Local evidence
Only after GitHub intake is complete, inspect local evidence if available.
Requirements:

* verify manifests and digests before trusting copied evidence;
* never rewrite a previously sealed evidence set;
* treat reconstructed audit notes as reconstructed, not original raw notes;
* preserve failed, invalid and inconclusive attempts;
* do not publish private local paths or credentials;
* distinguish historical evidence from current state if GitHub has moved.

The previous session intentionally retained:
`codex/exp-a-preparation`
Expected local commit:
`b50e437`
It was not pushed.
Do not:

* push it;
* merge it;
* delete it;
* force-delete it;
* treat it as product code

without explicit owner direction.
15. Expected unresolved owner decisions
After live intake, reconstruct the current open decision list from the latest DevGuard #14 and #17 records.
Expected unresolved areas include:

1. whether to merge DevGuard #17;
2. whether to prepare a generic Codex upstream proposal based on Experiment B;
3. D6 direction;
4. permit/transcript carrier direction;
5. timing and scope of normative DevGuard / CodeSpace policy PRs;
6. disposition of the local experimental branch;
7. whether to authorize CodeSpace #79's bounded diagnostic;
8. the independent DevGuard selective-CI worktrack;
9. the independent DevGuard upgrade-test-hardening worktrack.

Do not infer answers from prior-agent recommendations.
16. First response required from you
Before changing anything, return a concise cold-start intake report containing:

* live DevGuard `main`;
* live CodeSpace `main`;
* live CodeSpace Codex gitlink;
* #17 exact head/base/state/mergeability/current CI;
* #79 state;
* what is merged;
* what exists only as experimental evidence;
* what remains local-only;
* which owner decisions remain unresolved;
* whether the merged W0–W2 specification is still reachable and consistent with the current trackers;
* any difference between live GitHub state and this handoff;
* what you recommend as the next bounded action.

Then STOP for owner direction unless the message that starts your session already explicitly authorizes a specific next action.
17. Prohibited before that gate
Do not, merely as part of intake:

* merge #17;
* start #79's diagnostic;
* create implementation PRs;
* create normative architecture/policy PRs;
* modify CI as a side task;
* modify the upgrade timing test as a side task;
* change the Codex pin;
* add production dependencies;
* change DevGuard helper/session/permit contracts;
* modify CodeSpace process or PTY execution paths;
* submit anything upstream;
* alter installed services, credentials, journals or host settings;
* delete local branches or evidence;
* reinterpret bounded experiments as approved architecture.

18. Continuation principle
The handoff chain is intentionally layered:
`DevGuard #14 latest`
→ current owner/state delta
`DevGuard #17`
→ W3 evidence, limits and unresolved decisions
`DevGuard #16 merged full specification and W0–W2 packet`
→ detailed persistent pin/adapter/dependency/qualification/W4-W5 rules
`CodeSpace #76 latest`
→ current CodeSpace-side delta
`CodeSpace #79`
→ independent descriptor-hygiene investigation
`live GitHub re-query`
→ current reality
Use all layers.
Do not replace the merged full specification with a short summary when making a decision that depends on its detailed pin, dependency, adapter, qualification or promotion rules.
The purpose of this handoff is to preserve both:

* the current decision point; and
* the original engineering constraints that remain applicable after W0–W3.

It must not silently convert completed research into architecture approval or inherit hidden authority from the previous session.
````
