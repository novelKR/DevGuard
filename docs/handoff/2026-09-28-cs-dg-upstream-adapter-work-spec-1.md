# CodeSpace and DevGuard: Upstream-Pinned Adapter Work Specification

**Document identifier:** CS-DG-UPSTREAM-ADAPTER-WORK-SPEC-1
**Version:** 1.0
**Issued:** 2026-09-28
**Audience:** a new Kiro session, another coding agent, maintainers, and reviewers
**Primary trackers:** [DevGuard #14][S01] and [CodeSpace #76][S02]
**Status:** owner-requested work specification and handoff. It records an approved policy direction, specifies work and acceptance gates, and recommends technical candidates. It is not evidence of implementation, a merge authorization, or a deployment authorization.

> **Read this first.** The owner explicitly permits DevGuard to use Codex and other external dependencies. DevGuard is to adopt a reviewed, flexible upstream-pin policy with adapters absorbing upstream changes, analogous in principle to CodeSpace's existing approach. Do not reopen the general question of whether DevGuard may have such dependencies. A particular package, revision, integration mechanism, production change, or merge still requires the applicable technical review and execution authorization.
>
> The old CSRG-C00/C03/C09 execution direction remains suspended. This specification does not restart a CodeSpace-owned PTY implementation or authorize a rewrite of CodeSpace's unaffected execution paths.

## Executive decision

Preserve CodeSpace's selective upstream delegation and reviewed pin mobility. Give DevGuard the same ability to consume, qualify, update, and roll back external implementations behind explicit adapter boundaries. Independence means ownership of domain policy and contracts, not an absolute prohibition on shared implementation libraries.

The recommended first technical investigation is a combination, not a previously approved solution:

1. Separate DevGuard's launch preparation from the operation that actually spawns the helper, preferably retaining the existing one-time permit semantics initially.
2. Keep PTY allocation, platform setup, child handling, and reaping in the existing upstream execution implementation. Investigate a generic, bounded pre-reap observation facility implemented there rather than a CodeSpace-specific reaper.
3. Independently close the launch-attachment and client-session descriptor safety gaps. Evaluate maintained external libraries where they replace real work; do not assume that a library name supplies missing OS guarantees.
4. Qualify the smallest complete platform/transport/Runner combination. Do not choose pipe-first or PTY-first merely because an earlier report preferred it.

These are **candidate engineering directions**. The existence of a PID accessor, descriptor-attachment API, or adapter crate does not prove that the combined authorization, lifecycle, resource recovery, and output contracts are complete.

---

## 1. Verified handoff entry points and scope of verification

### 1.1 Separate records exist in both repositories

The following resources were read through the connected GitHub service while preparing this document. The two tracker issues and the two session-close PRs were confirmed to exist and be open. Their linked snapshot documents were also read. [S01], [S02], [S03], [S04], [S05], [S06]

| Repository | Living tracker | Session-close PR | Snapshot at the verified PR head |
| --- | --- | --- | --- |
| `novelKR/DevGuard` | [Issue #14][S01] | [PR #15][S03], open, head `760af00c685a3ede87e7b5468e509029c97ad9f1` | [`docs/handoff/2026-09-27-session-close.md`][S05] |
| `novelKR/CodeSpace` | [Issue #76][S02] | [PR #77][S04], open, head `6a4461e5445bf51103dacfa8cb08f585732b76b3` | [`.github/notes/pr75-cs-rg-suspension-handoff.md`][S06] |

The trackers cross-reference one another. They distinguish living state from dated session records and explicitly prohibit interpreting those records as architecture or merge approval.

**This publication did not create, comment on, update, merge, or close a GitHub issue or PR.** It confirms the existing handoff locations and provides a new standalone specification for the owner to pass to the receiving agent.

### 1.2 Repository snapshot

The following `main` refs were independently read during preparation. They are a reproducibility checkpoint, not a requirement that future work remain on those commits. [S07], [S08]

| Item | Verified or recorded value | Evidence qualification |
| --- | --- | --- |
| DevGuard `main` | `7e3cbda91308f527d6cc34fba908375e6332bc58` | independently retrieved ref |
| CodeSpace `main` | `794867ef52f530be6bc0d91aa10416d5195367b7` | independently retrieved ref |
| CodeSpace Codex pin | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`, `rust-v0.154.0` | recorded in the verified handoff; re-read the gitlink at intake |
| Earlier upstream comparison | stable `rust-v0.157.1` at `36650394c5b38c2990ccf2a3457165ca3e9d9726`; main snapshot `41f9084b30812db321a0b592def4f500d1e79cf4` | historical 2026-09-27 comparison, not a claim about the newest upstream release when execution begins |
| Design revision 1 provenance | DevGuard merge `d4981b4c241cff42687f5c2c681b583c7847776e` | historical design introduction, now affected by the implementation hold |
| Previously qualified DevGuard artifact | `0.1.0-5daee5d-b3fa569e` | recorded qualification for a particular artifact, host, and policy; not qualification of a new integration |

### 1.3 Related PR queue

These heads are recorded by the session-close trackers. Except for #15 and #77 above, this document does not claim to have re-run every PR-status and CI check. Re-query each PR before acting. [S01], [S02], [S03], [S04], [S05], [S06]

| Repository / PR | Purpose | Recorded head |
| --- | --- | --- |
| DevGuard #11 | independent stuck-probe test correction | `2bbe7c5ed88ad3170bc76985bdecb1fd7434d501` |
| DevGuard #12 | suspend the old CS-RG implementation directives | `ae85ebb95b68821361ae59d3c60b5344a1d8ab03` |
| DevGuard #13 | boundary review and hardened candidate analysis | `1af1921fb72bb969b4f19ff859bd59fdb361ca55` |
| DevGuard #15 | close-of-session handoff | `760af00c685a3ede87e7b5468e509029c97ad9f1` |
| CodeSpace #75 | corresponding suspension notices | `1bee230595698b0974df43561bccfce67d7e8cb9` |
| CodeSpace #77 | CodeSpace session-close note | `6a4461e5445bf51103dacfa8cb08f585732b76b3` |

No CI success reported in these records substitutes for checking the exact candidate under review. No new build, runtime experiment, private evidence rehash, or installed-service inspection was performed to issue this specification.

## 2. Background and terminology

CodeSpace is the external-agent execution service. It owns its MCP meaning, workspace authorization, approvals, logical process handles, output/result behavior, and Gateway/Runner product behavior. It selectively delegates implementation mechanisms to Codex. In the recorded baseline, PTY uses the Codex adapter while ordinary pipe execution uses Tokio. Selective delegation is not a requirement to move every execution path to Codex. [S02, S06, S09]

DevGuard is the host resource-admission and accounting authority. Its contracts track reservations, attempts, launch authorization, scope evidence, pressure, and reconciliation. Its existing command-line consumer is separate from the as-yet-unimplemented CodeSpace integration. [S10]

| Term | Meaning in this document |
| --- | --- |
| `off` | CodeSpace execution without required DevGuard governance; existing behavior must remain available |
| `required` | a requested execution that may not run through an unmanaged fallback |
| InProcess / UDS | Runner inside the Gateway process / a separate Runner worker reached over a Unix-domain socket |
| permit | existing one-time launch authorization material, not a general caller credential |
| attempt | stable identity for one intended execution and its durable resource state |
| reap | consuming a terminated child's wait status and releasing that child's remaining process-table entry |
| `Suspect` | conservative state retaining a reservation when the required termination evidence is missing |
| D6 | reviewed client-session socket creation/inheritance gap; actual exposure was not demonstrated in the handoff |
| adapter | a narrow translation boundary for upstream types and behavior; not a replacement process runtime disguised by a small interface |
| candidate pin | an explicitly identified revision under qualification, not an installed or supported baseline |
| promotion | an explicit transition from a tested candidate to an accepted source/artifact/support combination |

The previous review used R for governed pipe with owner-confirmed identity, X for a PTY child PID accessor, Y for descriptor attachment, RB for replacing a permit with identity confirmation, RC for authority-mediated results, RE for accepting reap-first, RA for a supervising helper, and RG for a group anchor. None is automatically selected by this specification. [S11]

## 3. Authority, policy approval, and execution permissions

### 3.1 The new owner decision

**APPROVED POLICY INPUT:** DevGuard may include Codex and other external dependencies and may use adapters with flexible, reviewed upstream pins. This includes considering dependencies in normal runtime or shared-client builds when justified; the permission is not restricted to an unused optional plugin.

This replaces any earlier premise that DevGuard must remain permanently Codex-free, that adding any external execution library is inherently unacceptable, or that the general permission must be requested again.

The owner has not, by approving this principle, selected Rustls, Tokio, rustix, nix, a particular Codex revision, a new authorization protocol, a supervising helper, or a specific default feature set.

### 3.2 Joint constraints remain

**N1: CodeSpace constraints.** Preserve product semantics, responsibility boundaries, selective Codex delegation, and DevGuard's opt-in character. CS-RG alone does not justify a CodeSpace-native PTY stack, a new CodeSpace reaper, rewriting the existing pipe path for symmetry, or making `off` require DevGuard.

**N2: DevGuard safety.** Preserve authorized one-time execution, credential isolation, stable identity, honest uncertainty, no automatic replay of uncertain work, and no resource release without sufficient scope evidence within the declared threat model.

**Operational acceptance:** a supported normal workload class must have a demonstrated path to resource recovery. A deliberately conservative error state is not, by itself, an acceptable normal execution design. Ordinary supported completion must not routinely depend on rebooting the host to regain reservations.

N1 and N2 are simultaneous constraints, not tradeable scores. An existing implementation detail is evidence, not automatically a normative requirement. Historical CS-RG decisions under review are not allowed to re-enter as immutable premises.

### 3.3 Permission boundaries for the receiving agent

| Action | Treatment |
| --- | --- |
| Read sources, trackers, comments, prior reports; preserve evidence | continue within established read/evidence permissions |
| Record the newly approved external-dependency policy and prepare policy/analysis documentation | within the owner-directed specification work; preserve factual history and submit proposals through the ordinary document-review process when authorized to execute this handoff |
| Choose packages and write candidate lock/provenance plans | engineering proposal, not a new vote on whether dependencies are allowed |
| Run a previously authorized unchanged-code bounded diagnostic | only within its existing protocol and environment limits |
| Modify an upstream utility in a scratch experiment, change a DevGuard fixture implementation, or add diagnostic dependencies | require a specific bounded experiment authorization unless already covered by a direct owner instruction; do not relabel a prototype as mere research |
| Change product dependencies, pins, runtime APIs, wire/journal schemas, or default features | reviewed change package and applicable implementation authorization; dependency admissibility is already settled |
| Merge, deploy, provision credentials, restart services, alter journals or host policy, submit an external PR | retain separate explicit approval requirements |

Possession of this file is not a blanket command to execute every later phase. When the owner forwards it as a work instruction, begin intake and policy/evidence preparation without asking the already-settled dependency-permission question. Request only the exact next action that exceeds the delegated scope.

The older trackers predate this owner approval. Preserve them as historical state and append a dated policy-delta entry when performing an authorized update. Do not claim that the trackers already contain this new permission. A comment written through a shared agent account is not independent proof of owner approval. [S01], [S02]

## 4. Cold-start procedure: no previous session required

1. Read this specification, DevGuard #14 and its latest comments, then CodeSpace #76 and its comments.
2. Read the fixed-revision snapshots in DevGuard #15 and CodeSpace #77, followed by both documents in DevGuard #13. PR-only files may not exist on `main`; fetch their actual PR heads rather than declaring them missing.
3. Re-query both `main` refs, all relevant PR heads/bases/states, the CodeSpace gitlink, and relevant CI identities. Do not wait for or poll unrelated CI.
4. Read actual repository instructions that exist. Do not invent a missing CodeSpace `AGENTS.md`; the previous handoff reported that it did not exist. Search for applicable instructions and document absence accurately.
5. On the local machine, inspect dirty files, stashes, worktrees, unpushed commits, active delegated tasks, and service state without changing them. Historical absolute paths are not portable.
6. Establish an authorization record identifying the owner's latest direct instruction, its permitted scope, and the external-dependency policy delta.
7. Post one substantive intake update on the two existing trackers when authorized. Do not create duplicate umbrella issues or new work-unit IDs merely to restate this handoff.

A new session must not require access to Kiro's private briefing or lessons. Any local-only result used as evidence must be recovered and verified, or marked unavailable. Missing historical logs are not restored by rerunning a test: a rerun is a new record.

If `main` advances, review the intervening changes and record a new execution baseline. Do not reset the repositories to the historical checkpoint merely to make the table match.

## 5. DevGuard's upstream policy: flexible source, strict promotion

### 5.1 Policy objective

DevGuard shall be able to upgrade an external implementation through an adapter without changing its domain contract merely because upstream refactored an internal API. Where upstream behavior changes materially, the adapter must reject, explicitly translate, or trigger a reviewed contract change; it must not pretend that compatibility still holds.

Flexibility applies to **which tested revision can be selected**, not to reproducing builds with a floating branch.

### 5.2 Candidate classes

| Class | Allowed use | Promotion requirement |
| --- | --- | --- |
| Released tag resolved to a full commit | preferred candidate where sufficient | verify the actual commit, artifacts, dependencies, behavior and target matrix |
| Explicit upstream commit not yet released | valid candidate; no automatic stable-only prohibition | record why release waiting is insufficient, qualify the snapshot, retain rollback evidence |
| Small upstream-first downstream patch | temporary option when necessary | identify original commit, patch commits/digest, upstream proposal status, divergence and removal tests |
| Registry package | valid external implementation | record source registry, locked version/checksum, feature and target set, provenance and update policy |
| Floating branch / mutable PR-head reference as a promoted identity | not acceptable | resolve to an immutable commit before qualification and promotion |

CodeSpace's existing policy already permits an explicit tag or commit selected through a reviewed PR and requires coordinated pin/lock/adapter changes. Preserve that policy rather than inventing a permanent release-only rule. [S09]

### 5.3 Required upstream record

Maintain a human-readable pin record plus machine-readable validation input. Prefer one authoritative generated/validated record over several manually synchronized lists. Proposed filenames and fields below are **design suggestions**, not existing files or schemas.

| Field group | Required information |
| --- | --- |
| Identity | upstream repository or registry; package names; full source commit or locked package identity; tag as annotation only |
| Selection | baseline and candidate; reason; required capabilities; availability versus contract fitness |
| Consumption | adapter owner; affected product roots; target triples; features; build/runtime/dev dependency classification |
| Reproducibility | lockfile digests; source-tree or patch-set digest; actual compiler/build-tool versions; source acquisition method |
| Compatibility | adapter contract version; wire/helper/journal/config effects; known incompatible combinations |
| Provenance | license/NOTICE handling; attribution; supply-chain review; relevant security advisories checked |
| Qualification | exact reports, executed/skipped/not-run cases, artifact hashes, baseline comparison |
| Rollback | last accepted pin/artifact combination; schema constraints; drain/reconciliation requirements |
| Divergence | local patches, upstream tracking reference, reason still needed, removal/reconvergence condition |

Do not invent full SHAs or checksums for proposed inputs. Use explicit `TBD - not selected` outside any machine-validated production pin record.

### 5.4 Adapter update workflow

For each pin change: capture the baseline; select a candidate; inspect the relevant upstream changes and actual dependency graph; update the adapter in a reviewable branch; run conformance tests; run affected product regressions; qualify required platforms; publish evidence; request exact-head merge approval; verify the post-merge result; promote artifacts only through the deployment gate.

A compile success is not behavioral compatibility. A matching tag is not an artifact hash. A changed source or lock input invalidates a previous candidate report unless its continued applicability is explicitly established.

### 5.5 Same process versus separate process

If CodeSpace and a DevGuard-specific binding use Codex in the **same executable**, avoid unintentionally loading distinct source identities and duplicate implementations. Inspect resolved package identities, features, types, and shared state. Prefer one reviewed source identity where they must interoperate, without asserting that matching SHAs alone prove safety.

If DevGuard daemon/helper and CodeSpace are **separate executables**, their internal library pins need not move together. Qualify the client/wire/helper/capability/artifact combination instead of imposing unnecessary lockstep release cycles.

A DevGuard-only upstream upgrade must not silently move CodeSpace's gitlink. A CodeSpace-only upstream upgrade must not silently replace the installed DevGuard release.

## 6. Adapter and dependency placement

The following names describe responsibilities, not mandatory crate names.

| Boundary | Owns | Must not acquire merely through this work |
| --- | --- | --- |
| DevGuard authority/contract core | admission, accounting, attempt state, pressure, evidence rules | Codex model/session/login/agent product semantics |
| DevGuard client/transport adapter | bounded authenticated messages and error translation | implicit unmanaged fallback or another full-host authority |
| DevGuard launch-preparation adapter | one-time preparation, helper invocation, attachment ownership, abort/commit cleanup | automatic takeover of the caller's PTY or process backend |
| DevGuard upstream-execution binding | mapping to generic upstream attachment/event contracts; selected CLI/platform utility reuse | a hidden CodeSpace-specific execution runtime |
| CodeSpace resource adapter | opt-in configuration, attempt binding, capability and outcome translation | PTY allocation, native spawn or a new reaper solely for DevGuard |
| Existing execution backend | its existing OS mechanism responsibility, extended generically if approved | direct knowledge of DevGuard leases, RPCs, or credentials |

The recommended packaging isolates Codex-specific types behind a binding crate or module. This is not a blanket requirement that all DevGuard default/shared-client binaries remain external-dependency-free. Place a dependency where it does useful work; show what it replaces and how its costs are contained.

For CodeSpace, runtime `off` and a build without governance support are distinct properties. Test both where they are part of the supported product. An optional dependency declaration alone is not proof of isolation: Cargo feature resolution can enable features through other dependency paths. Examine separate product builds, test builds, build dependencies and procedural macros. [S16], [S17]

Candidate library families include Codex execution utilities, maintained OS wrappers such as rustix/nix, an asynchronous I/O runtime where needed, and an authenticated transport implementation. No family is selected by this document. Do not introduce several overlapping wrappers or an extra runtime per execution without a demonstrated need.

## 7. Preferred investigation A: preserve authorization, separate preparation and spawn

### 7.1 Motivation

At the recorded baseline `HelperCommand` couples DevGuard-specific launch preparation to actual spawning. The prior permit-removal candidate RB introduced a new identity-confirmation protocol at the same time as solving this API coupling. Prefer reducing simultaneous changes: keep the existing one-time authorization meaning while evaluating a separable preparation API. [S10], [S11], [S12]

### 7.2 Candidate interface obligations

A conceptual `LaunchPreparation` must carry an immutable attempt/meaning binding, the helper path and invocation, owned launch attachments where required, original expiry, a single-use consumption right, and explicit abort/spawn-failure handling.

It must not allocate a PTY or spawn a child internally when consumed by CodeSpace's existing backend. The existing DevGuard CLI may retain a convenience function that consumes this lower-level preparation through its own approved execution adapter.

Names such as `LaunchPreparation` and `spawn_attempted` are illustrative. Do not expose them as implemented public APIs or assume existing types can simply be serialized, cloned, or moved across process boundaries.

### 7.3 Required protocol specification before product implementation

Document preparation, launch commitment, attachment installation, actual spawn, helper authentication, durable claim, policy application, authorization, payload exec, exit and reconciliation. For each transition state:

- the owner of the action and each live descriptor;
- the observed fact versus caller/helper assertion;
- durable state and idempotency key;
- whether an executable may already have run;
- cancellation, deadline, crash and lost-reply outcomes;
- how repeated consumption and a second helper are rejected;
- how cleanup works when the task or preparation object is dropped.

Do not delete the permit merely because a helper PID is available. If a different mechanism is later proposed, supply an explicit replacement for every authorization property, not just parentage and a numeric identifier.

### 7.4 Preserve uncertainty

Distinguish authority-to-helper authorization loss from owner-result loss. A helper that never receives authorization must not exec; an owner that misses a result may still have a running payload.

READY establishes pre-exec progress, not successful payload startup. EOF and an exit status do not distinguish every helper-before-exec failure from a payload-after-exec failure. Preserve `uncertain` where the evidence cannot distinguish them. Do not restore an approval or start another attempt merely because a response was missing. [S10], [S11]

## 8. Preferred investigation B: an upstream-owned bounded pre-reap observation contract

### 8.1 Proposed capability, not a current feature

Investigate a generic facility where the execution backend keeps child and reaping ownership but allows an external observer a bounded opportunity to collect evidence after exit and before reap. The proposal is not a claim that the recorded Codex API already provides it.

CodeSpace must not implement its own Unix wait loop to manufacture this event. DevGuard RPCs remain outside the upstream utility. The upstream component supplies a generic lifecycle contract; the resource adapter consumes it.

### 8.2 Candidate behavior

The observer is registered atomically with launch preparation, before a fast-exiting child can escape observation. The backend detects exit without consuming the status, publishes an opaque observation opportunity, waits for acknowledgement or an absolute deadline, and then reaps through its original child object. It records whether observation completed, timed out, failed, or was unavailable.

The callback must not run in a post-fork/pre-exec context. It must not hold a spawn or authority-wide mutex while waiting for remote I/O. Non-governed consumers retain their ordinary behavior when no observer is registered.

### 8.3 Contract checklist

| Condition | Required result |
| --- | --- |
| Very fast exit | observation registration was already installed; no lost-event window |
| Terminate/timeout races | no competing waiter bypasses the observation contract |
| Handle/task Drop | defined owner remains responsible; no double reap or indefinite zombie |
| Observer stalls | bounded cleanup occurs; missing evidence is surfaced, not treated as success |
| Authority unavailable | execution result remains honest; no false lease release |
| Output still draining | observation/reaping coordination does not silently discard output or block control |
| Stale identity | observer cannot use an expired numeric PID/PGID as continuing authority |
| No observer | no new DevGuard dependency, handshake, or mandatory wait on the `off` path |
| Platform lacks capability | explicit unsupported capability; no hidden alternative backend |

A timeout must not be a routine way to claim successful governance. Faults can conservatively retain resources, but the supported healthy workload matrix must demonstrate normal eventual recovery.

### 8.4 Pipe and PTY are evaluated separately

Do not migrate Tokio pipe execution to Codex for symmetry. Determine whether an equally thin, backend-owned observation facility exists or can be supplied within the existing delegation boundaries. A PTY solution does not automatically solve pipe execution, and the reverse is also true.

Do not count `ProcessDriver` as such a facility. Its output and Drop contracts require independent review, and adapting a caller-owned child does not recover an observation opportunity already consumed by a high-level waiter. [S11]

## 9. Preferred investigation C: complete descriptor and session safety

### 9.1 Treat two problems separately

**Launch attachments:** creation, lifetime, intended-child delivery, permit consumption, transcript closure, and failure cleanup.

**Authenticated client sessions:** a transiently inheritable connection endpoint, concurrent spawns, peer/session authentication, key/credential storage, message injection or interference, and cancellation.

Removing one permit descriptor does not remove client sessions. Replacing raw calls with a Rust wrapper does not create kernel guarantees that the platform lacks. Do not generalize a specific `socket()` or `pipe()` path into a universal claim about every macOS IPC mechanism.

### 9.2 Evidence needed

Re-read the actual client path, the relevant compiler/standard-library version, and every same-process spawn family that can overlap descriptor creation. Distinguish a code-level inheritance window from demonstrated endpoint usability, credential exposure, authorized-message forgery, and denial of service.

Use a synthetic, non-secret session for exposure diagnostics. Record child descriptors and actual effects; do not send operational credentials to a test process.

### 9.3 Eligible remedies

Compare platform-supported atomic creation, safer attachment carriers, existing upstream close-by-default facilities, and changes to DevGuard's own transport/authority handshake. Evaluate maintained external libraries for these functions. A file-backed carrier is only a candidate: secret persistence, paths, permissions, unlinking, crash cleanup, and backups must be assessed.

An authenticated/encrypted transport library may help separate possession of an inherited raw endpoint from permission to read or author messages. It does **not** prevent inheritance itself, endpoint interference, inherited in-memory secrets in all threat models, or every denial of service. Treat Rustls or any alternative as a candidate implementation, never a magic D6 fix.

Do not invent a cryptographic protocol to evade a dependency. Equally, do not silently redefine N2 from "no channel exposure" into a weaker claim simply because encryption seems convenient. Any proposed change to the declared threat model is a separate owner decision.

### 9.4 No global-lock or packaging loophole

A mutex only coordinates code that takes the same mutex. A private lock in a second copy of a client crate is not process-wide coordination. The recorded helper-preparation and helper-spawn APIs already acquire their guard; an outer wrapper must not acquire it again. [S12]

Do not solve this by requiring every CodeSpace spawn to take a DevGuard-specific lock. If a generic upstream improvement independently addresses descriptor hygiene, review it as such; do not relabel a DevGuard-only requirement as an unrelated CodeSpace initiative.

Until D6 is resolved or shown not to violate the declared N2 for a particular path, that path is not qualified. An "owner accepts the risk" checkbox is not proof that the unchanged hard constraints hold.

## 10. Resource recovery, identity and lifecycle acceptance

The recorded native tests establish a specific pathological sequence: an unobserved survivor remains after root reap, membership can no longer be proven, tracking loss becomes sticky, and the reservation remains charged after the survivor exits. The comparison test observes before reap and later releases normally. [S13]

This is not a claim that every reap-first execution leaks. If the scope ends with sufficient evidence, or a verified known member anchors adoption in the same observation, behavior can differ. The candidate must identify the conditions rather than flattening all cases into one verdict. [S10], [S11], [S12], [S13]

For a normal supported workload class, demonstrate repeated completed runs returning accounting capacity without reboot. For fault cases, retain honest uncertainty and document the safe recovery mechanism. Never clear the journal, force-release a lease, restart a daemon, or signal a stale group just to make tests green.

Account for owner death, daemon restart, unknown scope after restart, surviving children, job-control group changes, and worker shutdown. A fixed pre-reap event alone does not solve every restart or escape problem.

Anchor and supervising-helper routes remain alternatives, not defaults. They must prove helper failure, extra-process visibility, termination, signal, exit-code, EOF and cleanup semantics. Moving a child reaper into another repository does not remove the engineering obligation.

The exposure of a numeric PID is not a stable identity or a one-time authorization proof. Report any unavoidable signal race at the platform boundary accurately; do not claim an absolute guarantee contradicted by the implementation.

## 11. Supported configurations and explicit non-support

Maintain a support table with separate columns for platform, transport, Runner mode, resource mode, source/artifact set, and evidence stage.

Use distinct stages: `candidate`, `protocol-reviewed`, `experiment-passed`, `implemented`, `qualified`, and `unsupported under the selected constraints`. These are report labels unless an existing repository schema explicitly adopts them.

Never write "Linux: unsupported" when the intended statement is "Linux DevGuard-required integration: not implemented/qualified." Existing CodeSpace `off` behavior is not withdrawn by DG-LINUX's status.

The first product slice is whichever complete combination passes all gates with the smallest justified change. InProcess may reduce worker-credential complexity, but this is a candidate staging choice, not permission to ignore UDS later or rewrite its behavior.

A restricted workload class requires an enforceable or explicitly bounded contract, a way to detect violations, and an honest recovery story. A low observed failure frequency is not proof that arbitrary workloads satisfy the restriction.

## 12. Repository policy reconciliation

### DevGuard

Prepare policy changes addressing the approved direction in the editorial design reference, agent instructions, dependency validation, upstream pin/update documentation, licensing records and mirrored Korean documents as applicable. Keep the historical approved artifact byte-for-byte intact.

Replace blanket rejection of all `codex-` packages with product-root- and adapter-specific allowed reachability **only in the reviewed change that defines and tests those boundaries**. Do not disable the dependency stage or allow all Codex product crates. Keep CodeSpace/Codex domain-type leakage into the resource authority prohibited.

Choose actual crate paths before adding them to executable registries. Update manifests, locks, allowlists, CI selection/coverage and regression tests together when a component exists.

### CodeSpace

Preserve the existing reviewed pin process, all affected-adapter qualification, source attribution, package-reachability inspection and no-hidden-vendoring rule. Reconcile wording that assumed DevGuard could never consume Codex so it describes permitted adapter placement and prohibited transitive leakage accurately. [S09]

A DevGuard runtime adapter can use Codex without forcing CodeSpace's generic resource client to import Codex types. If a different placement is proposed, show its resolved graph and implications explicitly rather than weakening the checker until it passes.

Do not change active user-facing behavior documentation to describe a proposed capability as implemented. Place handoff narrative where each repository already permits it: DevGuard's handoff area and CodeSpace's `.github/notes/`, with links from the existing trackers. [S03], [S04], [S05], [S06]

### Cross-repository record

Append the owner-approved policy delta to #14 and #76 in an authorized execution session. Link this specification at an actual immutable revision once published there. Record what older policy wording is superseded and what hold remains. Do not rewrite the dated session-close snapshots as though the later approval existed at session close.

## 13. Proposed work packages and gates

These labels are local to this specification. They do not create repository milestone IDs or renumber CSRG-C00/C03/C09.

| Package | Work | Deliverable / exit gate |
| --- | --- | --- |
| W0 - Intake and policy delta | refresh state, verify handoff reachability, reconcile new dependency permission with old trackers | baseline and authorization map; no duplicate tracking issue |
| W1 - Upstream policy and adapter inventory | identify affected roots, source-acquisition options, pin schema, adapter responsibilities, dependency checks | reviewable policy proposal and capability-gap matrix; no chosen package presented as proven |
| W2 - Contract design | specify permit-preserving preparation, backend-owned observation, descriptor/session obligations | state/message sequence, failure table, test protocols and exact experiment scopes |
| W3 - Bounded experiments | under specific experiment authorization, test A/B/C in isolated worktrees or scratch upstream branches | raw evidence, negative controls, failure behavior; no installed-service or product activation |
| W4 - Candidate selection and normative integration proposal | compare only N1+N2-compliant candidates, liveness, maintenance cost, release coupling | owner decision packet and, when authorized, coordinated normative design PRs |
| W5 - Production implementation and promotion | only after applicable approvals; implement one complete slice and pin combination | reviewed code, full relevant regressions, platform qualification, artifact manifest, controlled rollout |

### Gate rules

W0-W2 are not an excuse to repeat the entire historical debate. Reuse verified evidence, close specific gaps, and identify the next exact experiment or decision.

W3 does not automatically authorize W5. An upstream scratch patch is not a promoted dependency. Any product dependency or source pin still enters through its reviewed PR.

W4 must not require the owner to choose between candidates that still lack their central authorization or recovery proof. Where no complete route is demonstrated, return the unresolved obligation and the smallest bounded next step, not an architectural impossibility claim.

W5 cannot be inferred from merging the hold, test-fix, analysis or session-close PRs. The old CS-RG implementation plan stays suspended until the owner-approved replacement explicitly defines the new work.

## 14. Experiment protocols and conformance tests

### Experiment A: preparation without taking over spawn

Use non-production permits and an existing safe authority fixture. Verify single consumption, intended helper/attempt binding, attachment lifetime, cancellation before and after creation, setup errors, two competing helpers, lost authorization versus lost owner response, and uncertain exec status.

Required negative controls include an incorrect permit, wrong attempt, duplicate consumption, a failed attachment setup, and an owner that never receives the launch result. Use explicit invariants rather than repeated success alone.

### Experiment B: backend-owned observation

Under authorization, modify only the isolated upstream candidate needed for a generic experiment. Verify fast exit, live descendants, no observer, observer acknowledgement, observer timeout, observer crash, termination/timeout races, Drop and output drain. Trace which code actually reaps.

Compare to the existing DevGuard native survivor tests. Demonstrate both conservative fault behavior and normal repeated reservation recovery. Do not require an actual agent workload to establish a deterministic race; representative frequency is a separate later measurement.

### Experiment C: attachments and client-session safety

Test each platform separately. Inventory intended and unintended child descriptors, creation-time overlap, connection/authentication progression, handshake replay, malformed frames, cancellation, EOF, inherited-endpoint interference and error cleanup. Use dummy secrets and preserve actual command/environment invocation per case.

If a library candidate changes transport security, test its precise boundary. Do not use a successful encrypted handshake as proof that no unrelated child can interfere with the raw endpoint.

### Cross-cutting tests

Test normal completion, explicit terminate, timeout, owner/worker death, daemon restart and failed observation. For PTY also test controlling terminal, initial size/resize, signals, job control, EOF, tail output and exit semantics. For pipe keep existing semantics; do not add a hidden supervising topology.

Existing independent defects remain separate work, but any candidate that relies on an affected behavior must list it as a dependency or limitation. Moving it to an "out of scope" section does not prove candidate fitness.

## 15. Validation and dependency qualification

### 15.1 Preserve CodeSpace's existing gates

A Codex pin PR must validate the pin, root and isolated adapters, patch/runtime/Linux helper binaries, integration/protocol tests, dependency policy, and the required Linux/macOS evidence. A passing patch or PTY subset is insufficient. A document-only workflow does not requalify the runtime. [S09]

### 15.2 Add DevGuard-specific gates rather than replacing old ones

Verify authority/contract regressions, client authentication and framing, launch preparation, helper policy application, reconciliation, descriptor hygiene, operational recovery, and CLI behavior for every changed artifact. Do not claim the old SLO-qualified release covers a newly linked library or helper implementation.

Measure new dependency costs: resolved normal/build/dev graph, activated features, minimum and actual compiler versions, binary size, build time and runtime tasks/threads/buffers. Avoid declaring a package count universal; record the exact command, target and features.

### 15.3 Negative dependency assertions

Prove that unsupported or unwanted product/agent/login/model crates are unreachable from designated product roots. Prove that CodeSpace's governance-free build and runtime work without a DevGuard service or credential, and without an unintended compiler-floor change.

Use target-filtered metadata and separate build invocations where appropriate. Cargo's feature unification and optional edges require inspecting the resolved graph; `default-features = false` alone is not sufficient evidence. [S16], [S17]

### 15.4 Evidence states

For every gate record `passed`, `failed`, `skipped-by-plan`, or `not_run` with a reason. A missing platform runner is not a pass. A source review is not a runtime result. A CI job name alone is not the complete test execution list.

Retain original failed attempts. Do not rerun until green without a stated hypothesis and bounded plan. Scheduled CI is one source of evidence; one successful scheduled run cannot prove that an intermittent race no longer exists.

## 16. Compatibility, rollout and rollback

Record a qualified tuple consisting of CodeSpace source and adapter revisions, Codex source and local patch digest, DevGuard client/daemon/helper identities, wire/capabilities, journal/config schemas, build toolchain, platform, policy and test evidence.

A version handshake must reject unsupported combinations explicitly. Feature availability is not proof of semantic compatibility. No `required` request may silently fall back to unmanaged execution.

Before upgrading a runtime that owns charged work, follow the existing admission-close/drain/reconcile and protected-artifact process. Do not edit the installed release, clear the journal, or restore an old journal just to match a downgraded binary.

If a new journal schema cannot be safely read by an old release, rollback needs an explicit migration/drain procedure; reverting a Git commit is not the whole runtime rollback.

Pin rollback must restore source selection, adapter locks, patches, attribution, capabilities and behavior documentation as a coherent change, then rerun affected gates. Preserve both failed and successful candidate evidence.

## 17. Acceptance checklist

A candidate is eligible for promotion only when all applicable conditions hold:

- **AC-01:** CodeSpace retains product authority and selective upstream delegation; no custom CodeSpace PTY/reaper is introduced solely for CS-RG.
- **AC-02:** DevGuard external dependencies are consumed through declared ownership/adapter boundaries with a reproducible source identity.
- **AC-03:** API availability and behavioral contract fitness are documented separately; no claimed capability is borrowed from another revision or platform.
- **AC-04:** Launch is single-use, authorization-equivalent, identity-bound and honest after lost replies.
- **AC-05:** Launch attachments and client sessions satisfy the declared credential/channel safety requirements under concurrent execution.
- **AC-06:** Actual child ownership, termination and reaping have no unexamined competing paths.
- **AC-07:** Supported healthy workloads release reservations normally, including relevant survivor patterns; fault states never force unsafe release.
- **AC-08:** Output loss, EOF, exit code, payload-start uncertainty, signals and shutdown behavior remain honest and within the supported product contract.
- **AC-09:** CodeSpace `off` works independently of governance, including the promised build/feature configuration.
- **AC-10:** Upstream upgrades are reviewed and reversible, with no hidden vendoring, floating promoted pins or mandatory cross-process lockstep.
- **AC-11:** Relevant platform, dependency and product regressions pass; omitted tests are visible and prevent unsupported claims.
- **AC-12:** The next agent can recover decisions, code, pending gates and evidence limitations from the trackers and linked artifacts, not private session memory.

Failure of an applicable condition means the exact combination is not promoted. It does not justify silently changing the condition or claiming every future solution is impossible.

## 18. Evidence, collaboration and lossless handoff

Preserve raw logs, exact commands and per-case environment, instrumentation patches, test data identifiers, source and lock identities, runner/toolchain, exit status and output classification. Store secrets nowhere in public records. Use logical paths such as `<DEVGUARD_CHECKOUT>`.

Store evidence outside disposable `target` and worktree cleanup paths. Version the manifest or seal a snapshot before appending more material; do not replace an old digest in history and imply that it identifies an unchanged dataset.

The former collector is local-only. If unavailable, use the documented GitHub API/CLI routes to collect run metadata, jobs, attempt logs and artifact listings, then hash the actual downloaded files. Record storage location and access limitations. [S05]

Public analysis must include enough sanitized reasoning to audit a recommendation. Raw evidence restrictions do not automatically prohibit publishing a capability matrix, failure table or state transition analysis.

Give each delegated task this document ID/version, the exact source refs and a bounded purpose. Recheck the current instruction revision before starting and accepting its result. `not found` is indeterminate, not proof of completion, cancellation or queued state. Revoke old implementation tasks explicitly; do not rely on a private lesson reaching another agent.

On session close, update the two existing trackers with actual progress, full PR heads, evidence pointers, unresolved obligations, remaining approval scopes and the next executable step. Do not carry forward obsolete permission questions. Preserve existing dated handoffs and append a successor record rather than rewriting history.

## 19. Merge and operational controls

Keep test-fix, hold-notice, historical-record, policy, experiment, pin-update and runtime implementation changes distinguishable. Do not bundle a new dependency into DevGuard #11 or quietly transform #13's review record into an accepted architecture.

Every merge retains the owner's exact-head approval procedure. Check head and base immediately before merging, verify changed integration inputs, and apply the repository's stricter base-revalidation rule where required. Advancing `main` does not itself change another PR's head; updating that branch does. CI success does not replace review of new code.

After an authorized merge, verify merged state and commit identity, inspect post-merge main checks and applicable documentation publication, preserve evidence, then align local `main`. Keep remote branches. Remove only worktrees and local branches within the explicitly approved cleanup scope, with ancestry and uncommitted work checked first.

No polling loop, unattended follow-up promise, forced reset, branch deletion, rule bypass, operational credential change, or hidden service restart is authorized by the existence of this specification.

## 20. Required next-agent outputs

The next submission must be one coherent review packet, not another unbounded sequence of planning revisions.

| Output | Required content |
| --- | --- |
| Intake delta | actual refs/PRs, handoff accessibility, current authorization and policy delta |
| Adapter map | domain owner, mechanism provider, maintenance owner, source identity and type boundary |
| Upstream policy proposal | reproducible pin record, candidate classes, qualification, divergence, update and rollback rules |
| Capability/gap matrix | current pin, explicit newer candidate, proof status, unresolved contract and change locus |
| Protocol pack | permit-preserving preparation; observation contract; descriptor/session safety; failure table |
| Experiment requests/results | bounded scope, authorization, baseline/negative controls, artifacts and limits |
| Dependency report | actual graph and features; off-build result; MSRV/toolchain and packaging effects |
| Decision packet | surviving complete or incomplete candidates, maintenance and recovery costs, precise remaining owner decisions |
| Handoff | new exact heads, tested states, tracker updates, next allowed action and stop boundary |

Do not ask again whether DevGuard may use Codex/external dependencies or whether CodeSpace's pin is immutable. Those questions are settled here. Ask only about a concrete unresolved architectural choice, an experiment beyond existing permission, a particular implementation/merge, or an operational change.

---

## Appendix A. Ready-to-paste instruction for a new session

You are taking over CodeSpace/DevGuard work from a closed Kiro session. Read the attached **CS-DG-UPSTREAM-ADAPTER-WORK-SPEC-1 v1.0** as the current work specification, then read DevGuard issue #14, CodeSpace issue #76, their comments, DevGuard PR #15's session-close document, CodeSpace PR #77's note, and DevGuard PR #13's two analysis documents at their actual heads.

The owner has explicitly approved DevGuard's use of Codex and other external dependencies, with flexible reviewed pins and adapters. That general permission must not be requested again. The old Codex-free baseline is historical policy to reconcile, not a permanent prohibition. This approval does not merge anything, select a library, authorize a service change, or revive the suspended C00/C03/C09 implementation direction.

Keep N1 and N2 jointly mandatory: preserve CodeSpace's product authority, selective upstream delegation and independent `off` behavior; preserve DevGuard's authorization, identity, credential, uncertainty and resource-evidence guarantees. Require normal resource recovery for supported healthy workloads. Do not build a CodeSpace-native PTY/reaper or rewrite its pipe path for DevGuard.

First refresh repository/PR state and the Codex gitlink. Confirm the existing handoff resources and recover only the evidence actually available. Do not invent repository instruction files or depend on private Kiro memory. Treat old support and CI statements as dated records, not current proof.

Within existing research/documentation permissions, complete W0-W2: record the policy delta, draft the adapter/upstream policy, and specify the three bounded investigations. The preferred starting candidate preserves one-time permit meaning while separating launch preparation from spawn; investigates an upstream-owned bounded pre-reap observation facility; and independently resolves attachment/session descriptor safety. These are hypotheses to test, not approved implementations. Do not default back to R/X/Y, require a PID-only authorization rewrite, or adopt reap-first leaks as ordinary support.

Before any W3 prototype or changed-source experiment not already authorized, present its exact scope, expected evidence, affected paths and cleanup. Keep source pins, product dependencies, installed services and credentials unchanged until the relevant implementation/experiment permission exists. Do not confuse permission to use external libraries with permission to deploy an arbitrary candidate.

Use the existing trackers, preserve original evidence and dated handoffs, and avoid duplicate umbrella issues. When a permitted document update is made, explain the new dependency-policy permission without falsifying the old session's state. Maintain required translations and inspect executable dependency/CI gates before proposing changes to them.

Deliver an evidence-backed policy and contract packet with explicit next tests and any precise approvals needed. Do not restart a broad architecture debate or ask already-resolved permission questions. Do not silently promote source hypotheses to supported capability. If the required combination is still incomplete, state the exact missing proof or implementation and stop at the applicable gate. Every merge, upstream submission and operational change remains separately controlled.

## Appendix B. Sources and verification notes

Owner policy statements in this specification derive from the owner's direct instructions in this conversation: explicit permission for DevGuard to include Codex and other external dependencies, and the request to apply a flexible adapter-based upstream-pin principle while preserving CodeSpace's existing policy. They were **not** found as a new approval in the older tracker comments.

Repository sources support the historical and current-state statements indicated above. Technical candidates and acceptance criteria are proposed by this specification unless explicitly marked as existing implementation. No new runtime experiment was performed for publication.

| ID | Source / scope |
| --- | --- |
| S01 | [DevGuard #14][S01], living primary handoff tracker; body and available comments checked |
| S02 | [CodeSpace #76][S02], counterpart tracker; body and available comments checked |
| S03 | [DevGuard #15][S03], session-close PR metadata at head `760af00...` |
| S04 | [CodeSpace #77][S04], counterpart PR metadata at head `6a4461e...` |
| S05 | [DevGuard session-close snapshot][S05], dated context, permissions, glossary, evidence and procedures |
| S06 | [CodeSpace session-close snapshot][S06], CodeSpace-side boundaries and open items |
| S07 | [DevGuard main ref endpoint][S07], independently retrieved at publication preparation |
| S08 | [CodeSpace main ref endpoint][S08], independently retrieved at publication preparation |
| S09 | [CodeSpace upstream update policy at `794867e`][S09], explicit pin review and full qualification policy; suspended future execution design is not adopted here |
| S10 | [DevGuard implemented contracts at `7e3cbda`][S10], existing authority/helper/reconciliation behavior |
| S11 | [Hardened analysis at `1af1921`][S11], candidate definitions and documented limitations; an audit input, not architecture authority |
| S12 | [DevGuard launch preparation implementation at `7e3cbda`][S12], current helper/permit/spawn coupling |
| S13 | [Native reconciliation tests at `7e3cbda`][S13], specific pre-reap versus reap-first survivor cases |
| S14 | [CodeSpace upstream reuse policy at `794867e`][S14], selective consumption; suspended target architecture remains suspended |
| S15 | [Cargo: specifying dependencies][S15], source/revision and dependency-location semantics |
| S16 | [Cargo: features][S16], optional/default features and unification |
| S17 | [Cargo: dependency resolution][S17], graph/feature behavior relevant to separate product qualification |

[S01]: https://github.com/novelKR/DevGuard/issues/14
[S02]: https://github.com/novelKR/CodeSpace/issues/76
[S03]: https://github.com/novelKR/DevGuard/pull/15
[S04]: https://github.com/novelKR/CodeSpace/pull/77
[S05]: https://github.com/novelKR/DevGuard/blob/760af00c685a3ede87e7b5468e509029c97ad9f1/docs/handoff/2026-09-27-session-close.md
[S06]: https://github.com/novelKR/CodeSpace/blob/6a4461e5445bf51103dacfa8cb08f585732b76b3/.github/notes/pr75-cs-rg-suspension-handoff.md
[S07]: https://api.github.com/repos/novelKR/DevGuard/git/ref/heads/main
[S08]: https://api.github.com/repos/novelKR/CodeSpace/git/ref/heads/main
[S09]: https://github.com/novelKR/CodeSpace/blob/794867ef52f530be6bc0d91aa10416d5195367b7/docs/upstream-update.md
[S10]: https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/contracts.md
[S11]: https://github.com/novelKR/DevGuard/blob/1af1921fb72bb969b4f19ff859bd59fdb361ca55/docs/handoff/2026-09-27-cs-rg-boundary-revalidation-analysis.md
[S12]: https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/crates/client/src/launch.rs
[S13]: https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/crates/launch/tests/reconcile.rs
[S14]: https://github.com/novelKR/CodeSpace/blob/794867ef52f530be6bc0d91aa10416d5195367b7/docs/codex-reuse.md
[S15]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html
[S16]: https://doc.rust-lang.org/cargo/reference/features.html
[S17]: https://doc.rust-lang.org/cargo/reference/resolver.html
