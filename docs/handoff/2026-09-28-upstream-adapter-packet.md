# Upstream-adapter packet: W0–W2 of CS-DG-UPSTREAM-ADAPTER-WORK-SPEC-1 v1.0 (2026-09-28)

> **Status: dated, non-normative research and handoff record.**
> - Nothing here approves an architecture, dependency, Codex pin, runtime contract, implementation, upstream
>   submission, service change or merge.
> - The CS-RG implementation hold stays in force.
> - Proposed text is labelled **DRAFT – candidate wording, not adopted**. No normative document, gate or manifest is
>   changed by this record.
> - It stops at the W3 owner gate (section 14).

This record executes W0 (intake and policy delta), W1 (upstream policy and adapter inventory) and W2 (contract
design) of [CS-DG-UPSTREAM-ADAPTER-WORK-SPEC-1 v1.0](2026-09-28-cs-dg-upstream-adapter-work-spec-1.md), which came
with its [start instruction](2026-09-28-cs-dg-upstream-adapter-start-instruction.md). Both committed copies are
normalized derivatives (section 1.4).

The living trackers are [#14](https://github.com/novelKR/DevGuard/issues/14) and
[novelKR/CodeSpace#76](https://github.com/novelKR/CodeSpace/issues/76). Re-query them and git before acting. English
only, with no Korean counterpart, like the other handoffs.

**Evidence levels** (as in the [session-close record](2026-09-27-session-close.md)):
- *record*: a PR, CI run, tracker or document;
- *source*: code or a document read at a fixed revision;
- *test*: a test, CI or diagnostic result;
- *inference*;
- *not run*.

**Raw evidence** is local and git-ignored under `<DEVGUARD_CHECKOUT>/evidence/`. Each directory has a
`MANIFEST.json` with SHA-256 digests, and the digests are listed in section 15.

## 1. Intake delta (W0)

### 1.1 Baseline

Re-queried on 2026-09-28 at 01:20Z, after `git fetch`.

| Item | Value | Change since the spec's snapshot |
| --- | --- | --- |
| DevGuard `main` | `1bb085ad1a75034fe7bd7ffe7dd358b1a336239e` | advanced from `7e3cbda` by merging #12 (`418c0da`), #11 (`85f2dbc`) and #15 (`1bb085a`) |
| CodeSpace `main` | `326bdcb181f62f61bcb3c4e9c3de7535d50b0232` | advanced from `794867e` by merging CodeSpace #75 (`8a5a924`) and #77 (`326bdcb`) |
| Codex gitlink in CodeSpace | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (rust-v0.154.0) | unchanged |
| Open PRs | DevGuard #13 only, head `edf5e2f20f88feed55822a078abffc18af5f9a4e`, CLEAN; none in CodeSpace | #13's head moved from `1af1921` |
| Installed DevGuard service | LaunchAgent running release `0.1.0-5daee5d-b3fa569e` (read-only check) | not changed by this pass |

The spec's PR tables (its sections 1.1 and 1.3) are therefore dated. The five merges and the owner's policy
direction are recorded on the trackers:
- #14 comments [5858014440](https://github.com/novelKR/DevGuard/issues/14#issuecomment-5858014440) and
  [5858033332](https://github.com/novelKR/DevGuard/issues/14#issuecomment-5858033332);
- CodeSpace #76 comments [5858014659](https://github.com/novelKR/CodeSpace/issues/76#issuecomment-5858014659) and
  [5858033575](https://github.com/novelKR/CodeSpace/issues/76#issuecomment-5858033575).

This pass's intake delta is #14 comment
[5861778352](https://github.com/novelKR/DevGuard/issues/14#issuecomment-5861778352) and #76 comment
[5861779036](https://github.com/novelKR/CodeSpace/issues/76#issuecomment-5861779036). The issue bodies are dated
snapshots.

### 1.2 Post-merge verification and local alignment

- **Merge verification.** For each of the five merges, the merge commit's parents are the previous `main` and the
  approved head, and its diff against the first parent equals the PR's own diff (23, 11, 1, 1 and 1 files). *(source)*
- **Post-merge runs** were downloaded once and succeeded. *(test)*
  - DevGuard: 36336284259, 36336313208 and 36336325471 (contracts, macOS and Ubuntu).
  - CodeSpace: CI 36336288225 (documentation-only plan) and documentation 36336288630 on `8a5a924`; CI 36336337315
    (16 jobs, full) and documentation 36336337674 on `326bdcb`. Both documentation runs included deploy.
- **PR-head CI not preserved before** has now been collected: #12, CodeSpace #75, #15 and CodeSpace #77. So has
  #13's current-head CI: push 36336395942 and pull_request 36336399416, 4/4 success.
- **Local alignment.** Local `main` was fast-forwarded in both repositories. The five merged task worktrees and
  their local branches were removed with `git worktree remove` (no `--force`) and `git branch -d`, after checking
  each was clean, at its approved head and contained in `main`. Two local-only files were preserved first.
- **Kept:**
  - every remote branch;
  - the #13 worktree, deliberately left at its old head `1af1921`, two commits behind `edf5e2f`, and not used;
  - CodeSpace's fixture and diagnostic branches;
  - DevGuard's pre-existing untracked `.kiro/`.

### 1.3 Scheduled CodeSpace run

- **Superseded task.** The historical task "read the first scheduled run on `794867e`" can no longer be done:
  `main` moved to `326bdcb` before any scheduled run on `794867e`.
- **Replacement observation.** It is tied to `326bdcb`. The first scheduled run on that commit is
  [36355653598](https://github.com/novelKR/CodeSpace/actions/runs/36355653598), created 2026-09-27T22:32Z. It
  succeeded, 16/16 jobs. *(test)*
- **ETXTBSY check.** `linux_sandbox::tests::prepare_unread_large_stdin_times_out` passed. The Integration log has no
  match for ETXTBSY, "text file busy" or "os error 26".
- **Reading.** The #74 fixture fix showed no recurrence in this run. One run does not prove the race is gone.

### 1.4 Specification copies

No repository rule requires byte-identical copies; the byte-for-byte rule applies only to `docs/design.ko.md`. The
originals contain trailing whitespace, which repository whitespace checks reject. So the committed copies are
**normalized derivatives**, and the byte-identical originals remain in the git-ignored evidence set
(`upstream-adapter-2026-09-28/spec-originals/`).

| Committed file | Normalized SHA-256, bytes | Original file and SHA-256, bytes (stated = verified) | Normalization |
| --- | --- | --- | --- |
| [`2026-09-28-cs-dg-upstream-adapter-work-spec-1.md`](2026-09-28-cs-dg-upstream-adapter-work-spec-1.md) | `25cbf4ba661549c69260cc65b1d840e5513744d3938536792bc96d73d9ea045f`, 57,441 | `codespace_devguard_upstream_adapter_work_spec_en.md`, `21c934d76c59116dacbdcff46e2c31e7046c03cfe3cebdab540e3d6900aae2aa`, 57,451 | trailing spaces removed on lines 3–7 |
| [`2026-09-28-cs-dg-upstream-adapter-start-instruction.md`](2026-09-28-cs-dg-upstream-adapter-start-instruction.md) | `959164e1b0b3966f28d0cd32727c7c707de7e873ce17832c88259249d0f17efa`, 4,468 | `new_agent_upstream_adapter_handoff_en.md`, `7146aa8af276b9c04b0ce3412bfc69a2f471729babb7a1d43728a437283f36e6`, 4,472 | trailing spaces removed on lines 4–5 |

A line-by-line comparison after stripping trailing whitespace is identical for both files. No word changed. The
removed double spaces were Markdown hard line breaks in the two header blocks, so those lines now render as a single
paragraph.

### 1.5 Handoff resources and instructions

- **Resources read:**
  - #14 and #76 with all their comments;
  - the [session-close record](2026-09-27-session-close.md) (now on `main`);
  - CodeSpace's `.github/notes/pr75-cs-rg-suspension-handoff.md` (on CodeSpace `main`);
  - #13's
    [handoff](https://github.com/novelKR/DevGuard/blob/edf5e2f20f88feed55822a078abffc18af5f9a4e/docs/handoff/2026-09-27-cs-rg-boundary-revalidation.md)
    and
    [analysis](https://github.com/novelKR/DevGuard/blob/edf5e2f20f88feed55822a078abffc18af5f9a4e/docs/handoff/2026-09-27-cs-rg-boundary-revalidation-analysis.md)
    at `edf5e2f`.
- **Instructions:** DevGuard's `AGENTS.md` applies. CodeSpace has no `AGENTS.md`, and none was invented.
- **Private memory:** no private session memory was used.
- **Local-only evidence** of the earlier session was reused only where its manifest still verified. For example,
  #11's qualification outputs matched by SHA-256 before its worktree was removed.

## 2. Authorization record

- **Source of approval.** The owner instructed the agent doing this work directly, in the executing session on
  2026-09-28. The wording and times are kept in the local record `upstream-adapter-2026-09-28/authorization.md`.
  Tracker comments are written through the shared `novelKR` account and are not independent proof of approval.
- **Policy delta.** The owner explicitly approves DevGuard using Codex and other external dependencies, with adapters
  and flexible, reviewed, explicit upstream pins. This general permission is settled and is not requested again. It
  does not approve:
  - a specific package, revision, integration mechanism or default feature set;
  - a production change or a merge;
  - reviving the suspended C00/C03/C09 direction.

  N1 and N2 remain joint hard constraints. The older records (the 2026-09-27 handoffs, the session-close record,
  CodeSpace's note and #13's dated analysis) predate this approval. They remain accurate as records of their time and
  are not rewritten.
- **Merges.** The five merges were authorized by the owner in the preceding session with the instruction "proceed
  with the work on the basis of the recommended plan". The merging agent re-checked head and base before each merge.
  This pass completed their post-merge verification, evidence and approved local cleanup.
- **This pass may:**
  - carry out W0–W2 as this non-normative record;
  - open a CodeSpace `.github/notes/` pointer and post tracker deltas;
  - run BD-1 under its bounded-diagnostic conditions (section 11);
  - read the upstream revisions and Rust std/mio/tokio sources (sections 7 and 8).
- **This pass may not:**
  - merge #13 or either new PR;
  - run W3 experiments A, B or C;
  - open normative PRs (`AGENTS.md`, D3/ADR-006, design documents, `NOTICE`, validation policy, CodeSpace
    upstream-policy pages);
  - change any pin, dependency, service, credential, journal or host setting;
  - submit anything upstream.

## 3. Adapter map (W1)

The responsibilities follow spec section 6. The names describe responsibilities; they are not crate names or
existing APIs.

| Boundary | Domain owner | Mechanism today → candidate | Maintenance owner | Source identity | Must not acquire |
| --- | --- | --- | --- | --- | --- |
| Authority and contract core (`devguard-contract`, `-core`, `-daemon`, `-macos`) | DevGuard | DevGuard native backend (libproc, QoS, journal) | DevGuard | DevGuard SHA; registry crates in `Cargo.lock` (80 packages at `1bb085a`, none `codex-*` or `codespace-*`) | Codex or CodeSpace product types, sessions, login or agent semantics. The contract crate's dependencies stay `serde`, `serde_json`, `sha2` (`scripts/validate.py:113`) |
| Client and transport adapter (`devguard-client`: `connect.rs`, framing, `credential.rs`) | DevGuard | std `UnixStream` and libc → an authenticated-transport library is a candidate, not selected (section 9.3) | DevGuard | as above | an implicit unmanaged fallback or a second authority. No Codex types, because CodeSpace would link this crate |
| Launch-preparation adapter (`devguard-client::launch`, `devguard-launch`) | DevGuard | std `Command` spawned by `HelperCommand::spawn` under `spawn_guard` (`launch.rs:150-156`); descriptors created in `helper_command` (`:162-206`) → a preparation separable from spawn (section 9.1) | DevGuard | DevGuard SHA | PTY allocation, or taking over the caller's spawn |
| Upstream-execution binding (new; e.g. an isolated crate, name `TBD - not selected`) | DevGuard (the mapping) | Codex `codex-utils-pty` generic contracts: `ChildFds::Attached` (prerelease/main), `Command` + `DescriptorPolicy::Explicit`, and a future generic pre-reap observation (section 9.2) | DevGuard (binding); Codex upstream (mechanism) | an explicit Codex commit under section 4; `TBD - not selected` | Codex types outside the binding; being reachable from the core or the generic client contract |
| CodeSpace resource adapter (planned; CS-RG suspended) | CodeSpace | DevGuard client calls; consumes generic backend events | CodeSpace | CodeSpace SHA, plus the DevGuard client pinned by full SHA | PTY allocation, native spawn or a new reaper for DevGuard; `off` needing DevGuard |
| Existing execution backends (CodeSpace `crates/pty` for PTY; `crates/runner` Tokio pipe path) | CodeSpace product semantics | Codex PTY through `codespace-pty` (`crates/pty/src/lib.rs:66-77`); Tokio/std for pipe | CodeSpace (adapter); Codex upstream (PTY mechanism) | CodeSpace gitlink `6b9826e` | knowledge of DevGuard leases, RPCs or credentials; extensions must be generic |

### 3.1 Placement by process

- **The CodeSpace Gateway or Runner process** would link CodeSpace's crates, Codex at the gitlink, the DevGuard
  client and possibly the binding.
  - One executable must have one Codex source identity (spec §5.5). So a binding linked there consumes Codex through
    CodeSpace's gitlink, not its own pin.
  - Two sources of the same crate name compile as distinct crates, each with its own process-wide statics. An
    example is the reaper worker (`static REAPER`, prerelease `child_reaper.rs:22`). Neither copy's guarantees then
    cover the other, as with the duplicate-lock caveat in spec §9.4. *(inference)*
- **DevGuard's daemon, helper (`devguard-launch`) and CLI (`devguard`)** are separate executables.
  - A Codex pin there may differ from CodeSpace's.
  - What is qualified is the tuple of client, wire, helper, capabilities and artifacts (spec §5.5, §16), not
    lockstep versions.
- **Runtime cost of the candidate crate.**
  - `codex-utils-pty` depends on Tokio with a multi-thread runtime, `process` and `signal`, and on portable-pty.
    DevGuard's lock contains no Tokio today.
  - Any DevGuard executable that links the binding therefore gains a Tokio runtime and threads.
  - That cost must be justified by what it replaces (section 12). *(source: prerelease `Cargo.toml`, DevGuard
    `Cargo.lock`)*

## 4. Upstream-pin policy

> **DRAFT – candidate wording, not adopted.** This section proposes the policy the owner approved in principle. It
> becomes normative only through a reviewed design revision and the gate change of section 6, merged on exact-head
> approval.

### 4.1 Objective

DevGuard selects, qualifies, updates and rolls back external implementations behind adapters. Its domain contracts
do not change merely because upstream refactored internals. When upstream behaviour changes materially, the adapter
rejects the change, translates it explicitly, or triggers a reviewed contract change. Flexibility applies to *which
tested revision is selected*, never to building from a floating reference.

### 4.2 Candidate classes

| Class | Use | Promotion requires | Example from this packet |
| --- | --- | --- | --- |
| Released tag resolved to a full commit | preferred where sufficient | the actual commit, artifacts, dependency graph, behaviour and target matrix verified | rust-v0.157.1 = `3665039…`; lacks `ChildFds::Attached` |
| Explicit commit not yet released (including prereleases) | valid; not banned | why waiting for a release is insufficient, the snapshot qualified, rollback evidence kept | rust-v0.159.0-alpha.11 = `72b8d8b…`, exercised by BD-1 |
| Small upstream-first downstream patch | temporary | original commit, patch digest, upstream proposal status, divergence and removal tests | none |
| Registry package | valid | registry, locked version and checksum, features, targets, provenance, update policy | e.g. `tokio 1.53.1`, whose `Cargo.lock` checksum was verified in section 8 |
| Floating branch or mutable PR head | never promoted | resolve to an immutable commit first | — |

### 4.3 Pin record

The proposed record is a human-readable document plus one machine-validated input read by `scripts/validate.py`.
Suggested names are `docs/upstream.md` and `upstream.lock.json`; neither exists. Values not selected stay
`TBD - not selected` and never appear in a machine-validated production record.

| Field group | Content | Candidate `codex-utils-pty` (illustration) |
| --- | --- | --- |
| Identity | repository or registry, package names, full commit or locked package identity (a tag is only an annotation) | `https://github.com/openai/codex`, `codex-rs/utils/pty`, commit `TBD - not selected` |
| Selection | baseline and candidate, reason, required capabilities, availability versus contract fit | baseline: none (no Codex dependency at `1bb085a`). Evaluated: `6b9826e`, `3665039`, `72b8d8b` (section 7) |
| Consumption | adapter owner, affected product roots, target triples, features, normal/build/dev classification | `TBD - not selected` (section 3, row 4) |
| Reproducibility | lock digests, source-tree or patch digest, compiler and tool versions, acquisition method | `TBD - not selected` |
| Compatibility | adapter contract version, wire/helper/journal/config effects, known incompatible combinations | `TBD - not selected` |
| Provenance | licence and NOTICE handling, attribution, supply-chain review, advisories checked | Apache-2.0 at all evaluated revisions; review `not_run` |
| Qualification | reports, executed/skipped/not-run cases, artifact hashes, baseline comparison | BD-1 only (section 11) |
| Rollback | last accepted pin and artifact combination, schema constraints, drain/reconcile needs | none yet |
| Divergence | local patches, upstream tracking reference, why still needed, removal condition | none |

### 4.4 Update and promotion workflow

1. Capture the baseline.
2. Select a candidate and record its class.
3. Read the relevant upstream changes and the resolved dependency graph for each target.
4. Update the adapter on a reviewable branch.
5. Run the adapter's conformance tests, then the affected product regressions.
6. Qualify the required platforms and publish the evidence.
7. Request an exact-head merge approval, then verify the post-merge result.
8. Promote artifacts only through the deployment gate.

Rules for the evidence:
- A compile success is not behavioural compatibility.
- A matching tag is not an artifact hash.
- A changed source or lock input invalidates a candidate report unless its continued applicability is shown.

### 4.5 Same process, separate process

- **Same executable** (DevGuard binding inside CodeSpace): one reviewed Codex source identity, CodeSpace's gitlink.
  The CodeSpace pin PR qualifies both adapters together.
- **Separate executables** (DevGuard daemon, helper or CLI versus CodeSpace): pins may differ. Qualify the
  client/wire/helper/capability/artifact tuple.
- **No silent moves in either direction.** A DevGuard-only upgrade never moves CodeSpace's gitlink. A CodeSpace-only
  upgrade never replaces the installed DevGuard release.

### 4.6 Patches and divergence

- **Upstream first.** A local patch is temporary and is proposed upstream. Submitting it needs the owner's separate
  approval.
- **Record.** It records its original commit, patch digest, tests and removal condition.
- **Hidden vendoring** (copying crates to escape dependency checks) stays forbidden, as in CodeSpace's policy.

### 4.7 Rollback

A rollback restores together:
- source selection, locks and patches;
- attribution;
- capabilities and behaviour documentation.

Then it reruns the affected gates. A runtime that owns charged work follows the existing
close-admission / drain / reconcile process. A journal schema an older release cannot read needs an explicit
migration or drain procedure; reverting a commit is not a runtime rollback.

### 4.8 Evidence states

Every gate records `passed`, `failed`, `skipped-by-plan` or `not_run` with a reason:
- a missing platform runner is not a pass;
- a source review is not a runtime result;
- failed attempts are kept.

## 5. Codex-free wording to reconcile later (W1)

The owner's approval supersedes the premise that DevGuard stays Codex-free. The current documents state it as a
present engineering choice with a conditional reuse policy, and `scripts/validate.py` enforces it. None of the
following is changed by this record. The column "vehicle" names the normative change that would carry each
replacement:
- **R2**: a new design revision document with its Korean counterpart and translation hash, plus the editorial
  reference and ADR-006/D3 updates (as `AGENTS.md` directs for user-directed revisions);
- **G**: the gate change of section 6;
- **CS**: a CodeSpace documentation PR with its Korean pages and registry hashes.

| # | Location (DevGuard `1bb085a`, CodeSpace `326bdcb`) | Current wording (excerpt) | Draft replacement (DRAFT – candidate wording, not adopted) | Vehicle |
| --- | --- | --- | --- | --- |
| 1 | DevGuard `AGENTS.md:7` | "Keep the default distribution and the shared client free of Codex dependencies… `scripts/validate.py` rejects `codex-` and `codespace-` packages until such a decision changes that gate." | "Keep CodeSpace/Codex product types out of the authority core and the generic client contract. External implementations, Codex included, enter only through a declared adapter with a reviewed pin; the PR that adds one shows what it replaces and changes `scripts/validate.py` to allow exactly that adapter's reachable set." | R2, G |
| 2 | `docs/design.md:18` (and `ko/design.md:16`) | "…default distribution and shared client currently build, test and release without CodeSpace or Codex… present engineering choice, not a permanent prohibition…" | Keep the first clause while it is true. Replace the rationale with a reference to the adapter and pin policy. | R2 |
| 3 | `docs/design-revision-1.md:27`, `:107`, §10.3 `:565-585`, `:836` (and `ko` `:27`, `:567`, `:838`) | D3 = C0: "Add no Codex dependency to the current implementation"; "Keep DevGuard's current Codex-free implementation as the present engineering choice." | Not edited: revision 1 is a dated revision. A new revision supersedes D3 with: external dependencies permitted behind declared adapters with reviewed pins; core and generic-client boundary unchanged; each dependency justified by what it replaces. | R2 |
| 4 | `docs/planning/decisions.md:106`, `:112` (and `ko` `:116`, `:122`) | D3 row "C0: none; conditional reuse…"; "The validator's dependency-boundary stage … rejects any `codex-` or `codespace-` package … enforces C0" | D3 → "C1: adapter-bounded external dependencies with reviewed pins". Boundary sentence kept. Gate sentence → "the validator allows only declared adapter roots to reach approved upstream crates". | R2, G |
| 5 | `docs/planning/codespace-integration.md:176` (and `ko` `:184`) | "DevGuard adds no Codex dependency now; the default distribution and shared client stay Codex-free…" | "DevGuard may consume Codex behind a declared binding; the client CodeSpace links carries no Codex types, and a binding linked into CodeSpace uses CodeSpace's gitlink." | R2 |
| 6 | `docs/planning/README.md:40`, `docs/milestones.md:5` (and `ko`) | "keeps DevGuard free of Codex dependencies as a present engineering choice…" | Point to the new revision; drop "free of Codex dependencies". | R2 |
| 7 | `docs/planning/milestones/CS-RG.md:77` (and `ko`; CS-RG suspended) | "No CodeSpace/Codex dependency in DevGuard core; the client brings no transitive Codex dependency; … design revision 1 keeps the Codex pin." | Keep the first two clauses. Replace "keeps the Codex pin" with "a pin change follows the reviewed pin policy". Only once CS-RG is re-planned. | R2, after re-planning |
| 8 | DevGuard `NOTICE` | "DevGuard does not include CodeSpace, OpenAI Codex or rmcp as implementation dependencies." | True today; unchanged. The PR that adds a Codex dependency adds its attribution and NOTICE handling (Apache-2.0). | with the dependency |
| 9 | `scripts/validate.py:63-65` | rejects every package whose name starts with `codex-` or `codespace-` | See section 6: per-root allowed reachability, changed only in the PR that defines and tests the boundary. | G |
| 10 | CodeSpace `docs/codex-reuse.md:78` (and `ko` `:96`; section under a hold notice) | "D3, DevGuard. No Codex dependency is added to DevGuard now. Its core and shared client stay Codex-free…" | "D3, DevGuard (superseded 2026-09-28): DevGuard may consume Codex and other external implementations behind declared adapters with reviewed pins. CodeSpace's generic resource client imports no Codex types, and CodeSpace's own pin process is unchanged." | CS |
| 11 | CodeSpace `docs/upstream-update.md:90-131` (and `ko` `:91-128`) | "The planned resource client must bring no transitive Codex dependency… documentation policy, not an executable gate." | Keep, and add: "A DevGuard binding linked into a CodeSpace executable consumes Codex through this repository's gitlink; the same PR adds an executable single-identity check (section 6)." | CS |

Keep intact: `docs/design.ko.md` (byte-for-byte approval), the dated handoffs (`docs/handoff/2026-09-27-*`), the
session-close record, CodeSpace's `.github/notes/` records, #13's documents and the hold notices.

## 6. Executable gate inventory (W1)

Current behaviour is read from source at the baseline. The column "change later" is **DRAFT – candidate wording, not
adopted**. It is made only in the reviewed PR that adds a component, with tests; nothing is disabled.

**DevGuard**

| Gate | Current behaviour | Effect on a DevGuard Codex adapter | Change later |
| --- | --- | --- | --- |
| `validate.py` toolchain | expects Rust `1.95.0` (`PINNED_RUST`, `:17`) | `codex-utils-pty` needs at least 1.88 (section 12) | none |
| `validate.py` source contract | approved-design hash, milestone critical path, DG-LINUX required | none | none |
| `validate.py` documentation | `check_docs.py` plus its unit tests | revision documents need Korean pairs and hashes | none |
| `validate.py` dependency boundary (`:57-115`) | `cargo metadata --locked`. Rejects any package named `codex-*` or `codespace-*` anywhere (`:63-65`). Workspace roots must equal the explicit map (`:86`). Intra-workspace edges exact. Contract dependencies exactly `serde`, `serde_json`, `sha2` | any Codex crate fails. A new binding crate fails until it is mapped | Replace the name-prefix rule with: (i) a forbidden set for every root (`codex-core`, `codex-exec`, `codex-app-server`, `codex-login`, `codespace-*`); (ii) per-root allowed upstream crates, so only the binding root may reach `codex-utils-pty`; (iii) a check that no other root reaches any `codex-*` package, on target-filtered, non-dev graphs as CodeSpace's `upstream_dependencies.py` does. Keep (iv) the contract rule and the workspace map |
| `validate.py` format / clippy / tests (`--workspace`) | whole workspace on macOS and Ubuntu, one job | an in-workspace binding is built and tested on both. An isolated workspace needs its own stage (CodeSpace precedent: one stage per adapter) | a binding stage if isolated |
| CI `contracts` (macOS 14, Ubuntu 24.04) | `validate.py`, then the qualify suites (native suites macOS only) | build time and memory grow with Tokio and portable-pty | none |
| `package.py` | builds `devguardd`, `devguard`, `devguard-launch` from one clean release build (not read further) | the binding ships only if one of these links it | provenance fields if it ships |

**CodeSpace**

| Gate | Current behaviour | Effect | Change later |
| --- | --- | --- | --- |
| `scripts/upstream_dependencies.py` | target-filtered, non-dev graphs for the root and five adapter workspaces. Members must equal `PRODUCTS`. `FORBIDDEN` (`codex-core`, `codex-exec`, `codex-app-server`, `codex-login`) rejected from every root; `RUNNER_FORBIDDEN` from the Runner | a DevGuard client in the root graph adds packages. A binding reaching `codex-utils-pty` from a second source would add a second package key, which is not rejected today | when a DevGuard crate first enters a CodeSpace graph: a single-identity check (each `codex-*` name only from the gitlink path within a product root), and an executable form of "the resource client brings no transitive Codex dependency" |
| `scripts/check-no-model-deps.sh` | core manifests: **direct** `codex-* =` keys rejected (`scan_manifest`, `:222-227`). Adapters: direct keys against per-adapter allowlists (`pty`: `codex-utils-pty` only) | a transitive Codex crate reaching a core crate through a DevGuard client is caught by neither this script nor `FORBIDDEN`. The rule in `upstream-update.md:130` is documentation-only (`:94`) | the transitive check above |
| `scripts/check-upstream-pin.sh` | gitlink equals the `docs/upstream-lock.md` commit | untouched by DevGuard pins | none |
| `scripts/validate-upstream.py`, `ci_plan.py`, `ci-policy.json` | per-adapter stages. Manifests, locks, `third_party/**`, `.github/**` and `scripts/**` run every leg | a manifest change that adds a DevGuard client runs every leg | component entries for a new crate |
| `scripts/check_docs.py`, `docs.yml` | registry pairs; exact-commit publication provenance | none | none |

## 12. Dependency report (W1)

| Item | Finding | Level |
| --- | --- | --- |
| DevGuard graph at `1bb085a` | 80 packages, none `codex-*` or `codespace-*` (`dependencies.json` of post-merge CI 36336325471, macOS and Ubuntu) | test |
| `codex-utils-pty` direct dependencies (Unix) | `anyhow`, `portable-pty`, `tokio` (features `io-util`, `macros`, `net`, `process`, `rt-multi-thread`, `signal`, `sync`, `time`), `libc` (prerelease `Cargo.toml`) | source |
| Lockfile closure | 49 / 95 / 99 packages at pin / stable / prerelease. This is the union over all targets and features in upstream `Cargo.lock`, not a resolved per-target graph. It contains no other `codex-*` crate | source |
| Resolved build on aarch64-apple-darwin | BD-1 harness plus the crate: 38 packages, 36 third-party at the upstream-locked versions (tokio 1.52.3, portable-pty 0.9.0, libc 0.2.186, mio 1.2.0, nix 0.28.0). 27 dependency crates compiled in about 18 s in debug with one job, including 26 downloads into an empty `CARGO_HOME` | test |
| Compiler floor | no `rust-version` key. The crate uses edition 2024 and let-chains, so it needs at least Rust 1.88. Upstream builds with 1.95.0. DevGuard (1.95) and CodeSpace's floor (1.88) both satisfy it | source; floor is inference |
| Runtime cost | a Tokio multi-thread runtime, blocking waiter tasks and the `codex-child-reaper` thread in any DevGuard executable that links the binding. DevGuard has no Tokio today | source; size, threads and memory not_run |
| Licence | Codex workspace Apache-2.0. Closure licence review not_run | source / not_run |
| Binary size, build time for a product, runtime tasks and buffers | not_run: no product change proposed | not run |
| CodeSpace `off` build | not_run: no change proposed. The current evidence for the unchanged build is CI 36336337315 on `326bdcb` | not run |

Negative assertions any dependency PR must prove (spec §15.3):
- `codex-core`, `codex-exec`, `codex-app-server` and `codex-login` are unreachable from every DevGuard root;
- `codex-*` is reachable only from the binding root;
- the contract crate's dependencies are unchanged;
- CodeSpace's governance-free build runs without a DevGuard service or credential, and without a compiler-floor
  change.

These are checked with target-filtered metadata and separate build invocations; `default-features = false` alone is
not evidence.

## Sources

Fixed revisions read for this record. Repository paths are at the baseline commits of section 1.1 unless stated.

| ID | Source |
| --- | --- |
| SP | [CS-DG-UPSTREAM-ADAPTER-WORK-SPEC-1 v1.0](2026-09-28-cs-dg-upstream-adapter-work-spec-1.md) and its [start instruction](2026-09-28-cs-dg-upstream-adapter-start-instruction.md), normalized derivatives (section 1.4) |
| T14, T76 | [#14](https://github.com/novelKR/DevGuard/issues/14), [novelKR/CodeSpace#76](https://github.com/novelKR/CodeSpace/issues/76) and their comments, read 2026-09-28 |
| SC | [Session-close record](2026-09-27-session-close.md) (merged in `1bb085a`) |
| AN | #13 analysis at [`edf5e2f`](https://github.com/novelKR/DevGuard/blob/edf5e2f20f88feed55822a078abffc18af5f9a4e/docs/handoff/2026-09-27-cs-rg-boundary-revalidation-analysis.md) |
| DC | DevGuard [`docs/contracts.md`](../contracts.md), `crates/client/src/{launch,credential,connect}.rs`, `crates/launch/tests/reconcile.rs`, `scripts/validate.py` at `1bb085a` |
| CS | CodeSpace `docs/{codex-reuse,upstream-update}.md`, `crates/pty/src/lib.rs`, `crates/runner/src/process.rs`, `scripts/*` at [`326bdcb`](https://github.com/novelKR/CodeSpace/tree/326bdcb181f62f61bcb3c4e9c3de7535d50b0232) |
