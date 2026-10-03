# Owner instruction: continuation after the #17 merge (2026-09-30)

> **Status: dated record of an owner instruction; not maintained.** The owner gave this text in a Claude Code
> session's chat on 2026-09-30 at 11:26 UTC. It was not committed anywhere at the time. It is reproduced below
> unchanged, so every fact in it is as of that moment. It approves nothing beyond what it says, and later owner
> decisions may have superseded parts of it: the [CS-RG session handoff of
> 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what happened next and which parts still apply.

| Item | Detail |
| --- | --- |
| Given | In the same session, after its takeover report on the [takeover instruction](2026-09-30-takeover-instruction.md). |
| Covers | The owner decisions of 2026-09-30; work package A (normative reconciliation PRs, not to be merged), B (CodeSpace #79 steps 1 and 2 and one bounded run) and C (D6 option (b) on paper only), in that order; rules carried forward (section 9), prohibitions (section 10), stop conditions (section 11) and the staged Codex pin strategy (section 12). |
| Outcome | Package A became DevGuard #20 and CodeSpace #80, package B the #79 inventory, frozen protocol and bounded run, and package C DevGuard #21. The owner approved merging #20, #80 and #21, in that order, on 2026-10-01. |
| Still applies | The joint constraints (section 4), the merge, evidence and tracker rules (section 9) and the staged pin strategy, in particular keeping CodeSpace on its verified `0.154` baseline (section 12). |

Copied from the session transcript byte for byte, except that the chat tool's wrapper around pasted text was removed.

## Original text

````text
# DevGuard / CodeSpace continuation after the #17 merge (owner decisions of 2026-09-30)

You are continuing the DevGuard / CodeSpace upstream-adapter work (CS-DG-UPSTREAM-ADAPTER-WORK-SPEC-1 v1.0) from a
previous agent. Treat this as a cold start. GitHub is the authoritative continuation surface: local notes, private
memory and earlier agent conclusions do not count until you have re-read GitHub yourself. This prompt carries the
owner's new decisions (section 5). Nothing else is authorized.

## 1. Cold-start order

1. Read the latest comment on novelKR/DevGuard#14 (expected: 5908106348, 2026-09-30). Then read the session-close
   comment 5869132830 and follow its order. Later comments are deltas; issue bodies and older comments are dated history.
2. Read the W3 decision packet on DevGuard `main`: `docs/handoff/2026-09-28-w3-decision-packet.md` (merged by #17).
3. Before any normative, dependency or gate work, read the merged W0–W2 records on DevGuard `main` in full:
   - `docs/handoff/2026-09-28-cs-dg-upstream-adapter-work-spec-1.md`, especially §§3, 5, 6, 9, 12, 15, 16, 19 and 20;
   - `docs/handoff/2026-09-28-upstream-adapter-packet.md`, especially:
     - §5, the inventory of Codex-free wording with draft replacements;
     - §6, the executable gate inventory;
     - §8, macOS descriptor creation;
     - §9.3, attachments and client sessions;
   - `docs/handoff/2026-09-28-cs-dg-upstream-adapter-start-instruction.md`.

   Where later GitHub state shows their dated execution steps were done, treat those steps as done. Keep every
   constraint that still applies.
4. Read the latest comment on novelKR/CodeSpace#76 (expected: 5908117002), then novelKR/CodeSpace#79.
5. Re-query live state: both `main` heads, open PRs in both repositories, the CodeSpace Codex gitlink
   (`third_party/codex`), and the latest `main` CI. If GitHub has moved, do not rely on any SHA in this prompt.

## 2. Expected state (verify every item)

| Item | Expected |
| --- | --- |
| DevGuard `main` | `4898259a8a5b7c2ed0b3a46a3567cb419b0a73d5`, the merge of #17 (2026-09-30 09:12Z). Post-merge run 36694632828 succeeded |
| CodeSpace `main` | `ab0341b5cf5eda8c82730487d4475f1ac922daf5`. Codex gitlink `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (rust-v0.154.0) |
| Open PRs | none in either repository |
| CodeSpace #79 | open; investigation and proposal only |
| Local-only | branch `codex/exp-a-preparation` at `b50e437`, with its worktree. Keep it; never push it |
| Latest local evidence (`MANIFEST.json` SHA-256) | `pr17-merge-2026-09-30/`: `74077ef983335c12a9ab29c2e6a30c49b0d8483767d4eb2aa816c20247a5d6fd`; `pr17-merge-2026-09-30-cleanup/`: `5c8037623ceb74359c1f5c407ae26f8acad0f2a7d32ee011c87bd4c75719409f`; `pr17-merge-2026-09-30-comment/`: `97d0c56a8cbfe2997907a34fc41d891d61b2cb992def0a783e05a4b2cdc4a288` |
| Open owner decisions | 2–7 of the W3 packet, section 6 |

Stop and report before changing anything if live state differs in a way that affects the work below. Examples:
`main` changed a document in scope; a tracker records a new owner decision; an unexpected PR exists.

## 3. Settled policy: do not ask again

- DevGuard may use Codex and other external dependencies behind explicit adapter boundaries, with reviewed, immutable
  source identities and flexible pins where justified.
- The old "DevGuard stays Codex-free" position is historical policy awaiting reconciliation.
- The CodeSpace Codex pin is not immutable in principle.
- This settles permission only. No particular dependency, package, tag, commit, patch, pin move, merge, runtime
  integration, upstream submission or release is approved.

## 4. Joint constraints

N1 and N2 are jointly mandatory; neither has priority.

- **N1, CodeSpace.** Preserve:
  - its product semantics and responsibility boundaries;
  - selective Codex delegation;
  - independent, governance-free `off` behaviour.

  Do not introduce any of the following merely for DevGuard: a CodeSpace-owned PTY backend or reaper; a replacement
  process runtime; a rewrite of the pipe path; a transfer of lifecycle ownership; a hidden backend inside a thin
  adapter.
- **N2, DevGuard.** Preserve:
  - admission and accounting integrity;
  - stable attempt identity;
  - one-time launch authorization;
  - credential and permit safety;
  - honest uncertainty, with no replay of uncertain execution;
  - correct resource evidence and conservative reconciliation;
  - one actual reaper;
  - no false release;
  - normal recovery for supported healthy workloads.
- Classify any combination that cannot satisfy both as unsupported. Never weaken either invariant to make one fit.

## 5. Owner decisions in this prompt

| Decision (W3 packet §6) | Owner direction |
| --- | --- |
| 5. Normative reconciliation | **Authorized: prepare** the DevGuard R2 and CodeSpace CS documentation PRs (work package A). Do not merge |
| 7. CodeSpace #79 | **Authorized:** steps 1–2 (inventory and a frozen diagnostic protocol) and **one bounded run** of that frozen protocol in scratch, on the current `rust-v0.154.0` baseline (work package B). No CodeSpace code, pin or dependency change |
| 3. D6 direction | **Not decided.** Authorized only: a paper-only protocol and threat analysis of option (b), as input to a later decision (work package C) |
| 6. `codex/exp-a-preparation` | **Keep** it unchanged |
| 2. B upstream proposal; 4. carrier direction | **Deferred.** Do not act |

Carry out the work packages in the order A, B, C. Each package ends with a short report and its tracker update;
then continue with the next package. Stop earlier only for a stop condition of section 2 or section 11.

## 6. Work package A: normative reconciliation (documentation only)

**The owner-directed revision.** Record in a new DevGuard design revision that:
- DevGuard consumes external implementations, Codex included, only through declared adapter or binding boundaries
  with reviewed, immutable pins.
- The authority core, and the generic client contract that CodeSpace would link, carry no CodeSpace or Codex product
  types.
- Each dependency is justified by what it replaces and by how its cost is contained.
- The executable dependency gate in `scripts/validate.py` changes only in the reviewed PR that adds the first real
  adapter component and tests its boundary (gate change G).
- The revision selects no dependency, pin, candidate architecture or implementation. It does not revive
  CSRG-C00/C03/C09. The CS-RG implementation hold stays in force.

Carry the adopted pin policy into the revision itself: candidate classes, the same-executable and separate-executable
identity rule, qualification, divergence and rollback. Cite the W0–W2 packet §4 as provenance only; the
non-normative packet must not be the normative source.

**DevGuard PR (R2).** Use packet §5 as the location inventory, re-read at current `main`. Also search both
repositories for any other Codex-free wording; report every hit and skip none silently.
- Add `docs/design-revision-2.md`, with a reviewed Korean counterpart `docs/ko/design-revision-2.md`, registered in
  `docs/translations.json`.
- In the editorial reference `docs/design.md` and `docs/ko/design.md`: link the new revision, and replace the
  Codex-free rationale (packet §5 row 2).
- In `AGENTS.md`: update the revision list in the first paragraph and the Codex-free bullet (row 1).
- In `docs/planning/decisions.md` and its Korean counterpart: update D3 and ADR-006 (row 4).
- Update `docs/planning/codespace-integration.md` (row 5), `docs/planning/README.md` (row 6) and their Korean
  counterparts, and `docs/milestones.md` (row 6; it has no Korean counterpart).
- Wherever a sentence describes the executable gate, describe its current behaviour accurately until G lands: it
  still rejects every `codex-` and `codespace-` package.
- Review each Korean counterpart. Only after reviewing both documents, record the hashes with
  `python3 scripts/check_docs.py record --id <id>`.
- **Do not edit:**
  - `docs/design.ko.md`, the byte-for-byte approval;
  - `docs/design-revision-1.md` and its Korean counterpart, which are dated: supersede them, never rewrite them;
  - the dated handoff records;
  - `docs/planning/milestones/CS-RG.md`, until CS-RG is re-planned;
  - `NOTICE`, which is still true;
  - `docs/contracts.md`, which holds implementation facts only;
  - `scripts/validate.py`, and any CI or test.

**CodeSpace PR (CS).**
- In `docs/codex-reuse.md` and `docs/ko/codex-reuse.md`: mark the D3 row as superseded on 2026-09-28 (packet §5
  row 10). Keep the hold notices.
- In `docs/upstream-update.md` and `docs/ko/upstream-update.md`: apply packet §5 row 11. A DevGuard binding linked
  into a CodeSpace executable consumes Codex through this repository's gitlink. The executable single-identity check
  arrives with the first DevGuard crate in a CodeSpace graph.
- After review, record the registry hashes with CodeSpace's `python3 scripts/check_docs.py record --id <id>`.
- Link the DevGuard revision at an immutable commit.
- CodeSpace has no `AGENTS.md`; do not invent one.
- Never describe a proposed capability as implemented.

**For both PRs:**
- Write the PR bodies in English: scope, what does not change, verification, and rollback (a revert).
- Before opening each PR, run:
  - in DevGuard, the checks the current CI would select (`python3 scripts/ci_plan.py --base origin/main --head HEAD`);
  - the repository's documentation checks;
  - `git diff --check`.
- Write command output outside the checked worktree. `validate.py` has a source-fingerprint guard that fails the run
  when an untracked file changes during it.
- Open both PRs, read their CI, and preserve the evidence in a new, additive set.
- Update the trackers: a dated delta on #14, and on #76 for the CodeSpace PR.
- In the #14 delta, also record section 12 of this prompt as the owner's dated planning direction, marked
  non-normative, and post a one-line pointer to it on #76. It changes no decision in section 5.
- **End of work package A.** Report the PR numbers, exact heads, CI and evidence digests, and request exact-head
  merge approval. Do not merge. Continue with work package B.

## 7. Work package B: CodeSpace #79 steps 1–2 and one bounded run (CodeSpace-owned)

- **Justification: CodeSpace's own correctness only.** The risks are:
  - delayed stdout/stderr EOF;
  - lost stdin EOF;
  - PTY lifetime extension;
  - long-lived inherited descriptors in the UDS worker.

  Do not justify this work by DevGuard, D6 or CS-RG. Spec §9.4 forbids relabelling a DevGuard requirement as a
  CodeSpace initiative.
- **Step 1: inventory (read-only).** At current CodeSpace `main`, list every descriptor-creating and spawning path in
  the Gateway (InProcess) and Runner worker (UDS) processes on macOS. Include library-internal paths: std, Tokio/mio,
  portable-pty, the Codex PTY adapter at the pinned gitlink, and any other crate that spawns processes or creates
  pipes, sockets or PTYs. For each path, record:
  - file and line;
  - the creator or spawner;
  - whether creation is atomic;
  - child-side exclusion;
  - the owning process;
  - which concurrent executions can overlap it.
- **Step 2: frozen protocol.** It contains:
  - a fixed revision and the hypotheses;
  - measurable outcomes, for example:
    - one execution's output-EOF latency while unrelated children spawn;
    - stdin-EOF delivery;
    - PTY-master EOF after the child exits;
    - descriptor inventories of unrelated children;
  - finite trial counts, source-derived expectations and negative controls;
  - a scratch location outside both repositories;
  - no product, pin, dependency, service or credential change;
  - cleanup steps and an evidence manifest.
- **Sequence:**
  1. Freeze the protocol and record its SHA-256 before any build.
  2. Post the inventory and the protocol, with its digest, as a dated comment on #79.
  3. Run the frozen protocol once, in scratch, on the pinned `rust-v0.154.0` baseline.
  4. Seal the evidence and post the results on #79 as a second dated comment.
- **Run limits:**
  - synthetic configuration and workloads only; no operational credentials or tokens;
  - do not use the installed DevGuard or CodeSpace services; record the DevGuard LaunchAgent's pid before and after;
  - one Cargo job; the trial counts and time bounds fixed in the protocol;
  - a harness defect may be fixed and re-run only with a logged hypothesis, keeping the failed attempt; any protocol
    change is a dated amendment written before the runs it affects.
- Do not change either repository.
- Note this without acting on it: stable Codex `rust-v0.159.x` contains `DescriptorPolicy::Explicit`. Using it would
  still need a reviewed pin PR under CodeSpace's pin process.
- **End of work package B.** Post a one-line pointer to the #79 results on #76. Report the inventory, the protocol
  digest, the results and the evidence digest. Do not propose or make a CodeSpace change; comparing remedies (step 3
  of #79) needs its own decision. Continue with work package C.

## 8. Work package C: DevGuard D6 option (b), paper analysis only

- **Candidates.** Analyse client-session designs under which an inherited session endpoint gains nothing:
  - a per-request connection with a one-shot token;
  - an authenticated or encrypted transport (no library selected);
  - any other candidate that emerges.
- **Evaluate each against:**
  - W3 experiment C's holder effects:
    - reply theft, including the permit;
    - request injection under the owner's principal;
    - denial of service by malformed frames or `shutdown`;
    - use after the owner closed its copy;
  - the macOS creation window of `connect_timeout`: a raw `socket()`, then `F_DUPFD_CLOEXEC`, with no atomic
    `socket()` on macOS;
  - the declared threat model: one operating account, cooperative workloads;
  - N2;
  - compatibility with wire version 1 and the journal;
  - interaction with the carrier choice (decision 4) and with D6 option (a).
- **For each candidate, state:**
  - what it closes;
  - what it leaves open;
  - what it would change (client, daemon, wire, journal);
  - the test protocol that would later prove it.
- Do not redefine N2 or the threat model. Any such proposal is a separate owner decision.
- **Deliverable:** a dated, non-normative record under `docs/handoff/`, delivered through a PR, in English only like the
  other handoff records. Do not merge it. No implementation, experiment, dependency or library selection.
- **End of work package C.** Post a dated delta on #14 with the PR number and exact head, the decisions still open
  and the next allowed action. Report the PR number, exact head and CI. Then give the final report of section 11 and
  stop.

## 9. Rules carried forward

- **Evidence hierarchy**, highest first:
  1. owner decisions;
  2. live GitHub and merged source;
  3. the W0–W3 records and experiments;
  4. local raw evidence;
  5. agent inference.

  A green CI, a mergeable state or a successful experiment is not an approval.
- **Evidence handling:**
  - create new, additive sets with `MANIFEST.json` SHA-256 digests, and never rewrite a sealed set;
  - keep failed and invalid attempts, and state a hypothesis before any re-run;
  - put no private paths or credentials in public records; use `<DEVGUARD_CHECKOUT>`.
- **Tracker comments:** render each from a template, then verify that the posted body equals the read-back body.
- **Merges, when later approved:**
  - only at the approved exact head, with `gh pr merge <n> --merge --match-head-commit <sha>`;
  - if `main` has moved, re-verify first on a local simulation of the exact merge, running the current planner's stages;
  - read the separate post-merge `main` CI;
  - keep remote branches.

  DevGuard `main` has no branch protection, so GitHub's CLEAN state is not a gate.
- **CI:**
  - Since #18, pull requests and `main` pushes run only the checks their changed paths affect. Historical and normative
    documents run the non-Rust checks they feed.
  - The daily scheduled run is always full; it is the compensating control.
  - Do not change CI or tests as a side task.
- **Upstream facts recorded on #14** (source level, not qualified; do not act on either here):
  - stable Codex `rust-v0.159.x` includes `ChildFds::Attached`; `utils/pty` at `.0` and `.1` is identical to `72b8d8b`;
  - Codex accepts no external code contributions or pull requests, so decision 2 can only be an issue-based proposal.

## 10. Prohibited in this session

- Merging anything.
- Running the #79 diagnostic beyond the single bounded run of section 7, or on any revision other than the pinned
  baseline.
- Implementing anything.
- Changing a pin, a dependency, `scripts/validate.py`, CI or tests.
- Changing DevGuard's helper, session, permit or wire contracts.
- Changing CodeSpace's process or PTY execution paths.
- Opening upstream issues or PRs.
- Touching installed services, credentials, journals or host settings.
- Deleting or pushing local branches (including `codex/exp-a-preparation`), or deleting evidence.
- Presenting bounded experiments as approved architecture.

## 11. First response and stop conditions

Before changing anything, return a short intake:
- the live `main` heads and the gitlink;
- open PRs;
- the latest tracker comments;
- any difference from section 2.

If nothing blocks, proceed with work package A. Stop and report at once if live state changes in a way that affects
the remaining work, a required check fails, or a verification does not match. At the end, report every PR number and
exact head, the CI results, the evidence digests, the tracker comment links and the decisions still open. Then stop
for owner direction.

## 12. Staged Codex pin strategy after the current A/B/C work

This section records the intended future migration shape, so that later agents do not infer that CodeSpace and
DevGuard must move their Codex pins in lockstep. It is planning direction only. It does not override sections 5 or
10, and it authorizes no pin change, dependency change, implementation, merge or production promotion in the current
session. Work package A records it on #14 as the owner's dated, non-normative planning direction.

### 12.1 Transitional strategy

Until a separately reviewed CodeSpace pin change is approved:
- CodeSpace remains on its existing reviewed baseline, currently Codex gitlink
  `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (`rust-v0.154.0`).
- Continue the currently authorized CodeSpace work, including the bounded #79 work of section 7, against that
  baseline unless a later owner decision says otherwise.
- A future DevGuard work package may independently evaluate and, if separately approved, consume a reviewed stable
  Codex `rust-v0.159.x` source identity.
- A DevGuard-only move to `0.159.x` does not imply, authorize or require an immediate CodeSpace pin move.
- A later CodeSpace move from `0.154` to `0.159.x` is a separate reviewed pin decision, with its own qualification
  and rollback evidence.

The intended migration is staged rather than lockstep:

~~~text
current:                  CodeSpace 0.154     DevGuard current production (no Codex dependency)
future candidate stage:   CodeSpace 0.154     DevGuard 0.159.x candidate
later convergence stage:  CodeSpace 0.159.x   DevGuard 0.159.x
~~~

Each transition is separately reviewed and qualified.

### 12.2 Same-executable versus separate-executable rule

Keep the W0–W2 executable-identity distinction explicit.

**Separate executable or process boundary.** When DevGuard consumes Codex only inside a separate DevGuard executable,
or inside a declared execution or platform adapter that is not linked into the CodeSpace executable:
- DevGuard and CodeSpace do not have to move their Codex pins in lockstep.
- DevGuard may qualify a reviewed `0.159.x` identity while CodeSpace remains at `0.154`.
- The compatibility surface is the product boundary:
  - wire and capability negotiation;
  - attempt identity;
  - helper and launch semantics;
  - permit and transcript semantics;
  - error mapping;
  - result and evidence contracts;
  - artifact provenance.
- Do not require Codex Rust types or source identity to cross that boundary.

**Same executable.** If a DevGuard component becomes linked into a CodeSpace executable:
- that executable must have one reviewed Codex source identity;
- do not create a graph in which CodeSpace brings Codex `0.154` while a linked DevGuard crate independently brings
  Codex `0.159.x`;
- the DevGuard generic client contract that CodeSpace links must remain free of Codex and CodeSpace product types,
  unless a later explicit design decision changes that contract;
- a DevGuard adapter that needs Codex stays outside that shared-client graph, unless the executable's Codex identity
  is deliberately unified and jointly qualified.

This rule is not yet enforced: CodeSpace's gates do not reject a second source of a `codex-*` crate today (W0–W2
packet §6). The first PR that puts a DevGuard crate into a CodeSpace graph adds the executable single-identity check.

Do not solve a version mismatch by moving PTY, spawn, reaping or lifecycle ownership from CodeSpace to DevGuard.

### 12.3 What a DevGuard-first `0.159.x` move would and would not prove

A future DevGuard `0.159.x` qualification may establish facts such as:
- stable `ChildFds::Attached` behaviour;
- stable `DescriptorPolicy::Explicit` behaviour;
- launch-attachment and descriptor-hygiene properties;
- whether W3 A and C evidence still applies to a released revision;
- DevGuard adapter compatibility;
- dependency, feature, MSRV, packaging and rollback effects.

It does not by itself establish that CodeSpace's current PTY path has those capabilities. While CodeSpace remains
pinned to `0.154`:
- its PTY behaviour remains the behaviour of the pinned CodeSpace backend;
- a DevGuard `0.159.x` dependency does not silently upgrade CodeSpace's PTY backend;
- W3 Y + A cannot be called a production CodeSpace path merely because a separate DevGuard process has `Attached`;
- CodeSpace's current pipe path remains its current Tokio path unless separately changed for a CodeSpace-owned reason.

The W3 smallest candidate needs `Attached` in CodeSpace's own Codex, because it is macOS, PTY, InProcess: Y + A
through `ChildFds::Attached` in CodeSpace's backend. It therefore depends on the convergence stage (12.7) and on B,
which no stable release provides. It does not depend on work package D.

Do not report a DevGuard-only `0.159.x` qualification as CodeSpace pin qualification.

### 12.4 Future work package D: DevGuard-only stable candidate refresh

This work package requires a separate owner authorization after the current A/B/C work. When authorized, it is
evidence-first and does not change the CodeSpace gitlink.

**Prerequisites.**
- Work package D can adopt a dependency only after the R2 revision of work package A is merged.
- The adopting PR itself carries gate change G, because `scripts/validate.py` still rejects every `codex-` package.
  Until then, evaluation runs in scratch or an unpushed worktree, as in W3.
- The package names:
  - the DevGuard consumer (executable and adapter) and what the dependency replaces;
  - its runtime cost: a Tokio multi-thread runtime, waiter tasks and the `codex-child-reaper` thread;
  - the negative dependency assertions of spec §15.3;
  - Linux explicitly as `not_run` or unsupported under DG-LINUX, including the K9 spawn-helper question.

**Candidates.** At the start, re-query upstream rather than assuming a particular patch release is still the latest.
Use:
- `rust-v0.159.0` as the stable continuity control for the W3 `rust-v0.159.0-alpha.11` evidence: its `utils/pty`
  tree (`db23e5b`) is identical to W3's;
- the latest non-prerelease `rust-v0.159.x` release at that time as the primary DevGuard candidate, to be reviewed
  within this package, unless source review gives a reason to select another immutable revision.
  - At `rust-v0.159.2`, the `utils/pty` tree (`d348b8c`) differs from W3's, although `pty.rs` and `process.rs` are
    unchanged.
  - Review the changed files before carrying any W3 evidence forward;
- CodeSpace `rust-v0.154.0` as the existing production and control baseline.

**Evaluation.** Cover at least:
1. Source and dependency delta:
   - relevant Codex crates only;
   - lockfile and resolved package identities;
   - features;
   - MSRV and toolchain;
   - packaging, licensing and provenance effects.
2. W3 evidence refresh:
   - `ChildFds::Attached`;
   - `DescriptorPolicy::Explicit`;
   - the experiment A claims actually relied upon;
   - the experiment C descriptor behaviour actually relied upon;
   - signal, EOF and failure behaviour where it matters to the candidate.
3. Boundary validation:
   - no Codex product type enters the authority core;
   - no Codex product type enters the generic client or wire contract;
   - no second Codex source identity appears inside a CodeSpace executable;
   - DevGuard's resource and authorization invariants remain N2-complete.
4. Remaining gaps:
   - generic pre-reap observation;
   - PTY child PID access, if still relevant;
   - carrier requirements;
   - D6 requirements.
5. Rollback:
   - the exact immutable previous identity;
   - the artifacts and qualification evidence needed to return safely;
   - no silent CodeSpace pin movement.

A source match alone is not qualification. Preserve behavioural evidence separately from source identity.

### 12.5 Transitional compatibility gate

Before treating a DevGuard `0.159.x` candidate as usable with the existing CodeSpace baseline, explicitly qualify the
transitional combination:

~~~text
CodeSpace 0.154 + DevGuard 0.159.x
~~~

This gate applies only once an approved integration slice gives CodeSpace a DevGuard consumer. While CS-RG stays
suspended, record it as `not_applicable`, never as `passed`.

It is separate from both DevGuard's own `0.159.x` qualification and a later CodeSpace `0.159.x` qualification. Verify
at least:
- wire and capability compatibility;
- attempt and generation identity;
- authorization and no-replay behaviour;
- helper, permit and transcript semantics;
- error and uncertainty mapping;
- evidence and result compatibility;
- process and resource lifetime boundaries;
- independence of CodeSpace's governance-free `off` mode;
- absence of Codex type or source coupling across the product boundary.

A passing transitional gate does not authorize a CodeSpace pin move.

### 12.6 Preserve the `0.154` CodeSpace baseline as comparison evidence

Run the #79 inventory, protocol and the one bounded run authorized in section 7 against CodeSpace's actual
`rust-v0.154.0` baseline. Preserve these measurements before any CodeSpace pin move. They are the comparison baseline
for CodeSpace's own later pin evaluation, covering:
- descriptor inheritance frequency;
- output EOF latency;
- stdin EOF behaviour;
- PTY lifetime and EOF behaviour;
- long-lived inherited descriptors;
- effects in the actual pipe, PTY and worker paths.

A pin move alone does not change CodeSpace's Tokio-based spawns (pipe execution, patch helper, UDS worker); only the
Codex PTY path moves with the pin. Compare pipe-path results only against a separately justified CodeSpace change.

Do not move CodeSpace to `0.159.x` merely to make the diagnostic easier.

### 12.7 Later CodeSpace convergence is a separate owner gate

The following future CodeSpace pin move must be a separate CodeSpace change:

~~~text
rust-v0.154.0  ->  reviewed rust-v0.159.x
~~~

It requires:
- the exact immutable source identity;
- review of every selectively reused Codex surface the pin affects;
- the coordinated adapter edit: at `rust-v0.159.x`, `spawn_pty_process` takes `ChildFds<'_>` instead of `&[i32]`, so
  `crates/pty/src/lib.rs` changes in the same pin PR. For example, `ChildFds::Inherited(&[])` keeps today's behaviour;
- CodeSpace PTY qualification;
- runtime and process utility qualification;
- filesystem, patch and Linux-sandbox impact review where applicable;
- preservation of the existing pipe-path and `off` semantics unless independently justified;
- CodeSpace CI and regression evidence;
- rollback evidence;
- compatibility qualification against the then-current DevGuard artifact.

Only after that succeeds may the following combination be treated as a convergence candidate:

~~~text
CodeSpace 0.159.x + DevGuard 0.159.x
~~~

Do not infer convergence from matching version strings alone.

### 12.8 Interpretation rule for future agents

The intended strategy is:
1. Keep CodeSpace on the already-qualified `0.154` baseline while the remaining CodeSpace-owned investigation and
   other authorized work proceed.
2. DevGuard may later, under separate authorization, qualify a stable `0.159.x` dependency behind a separate
   executable or adapter boundary.
3. Once it is applicable, qualify `CodeSpace 0.154 + DevGuard 0.159.x` as an explicit transitional state.
4. Move CodeSpace to `0.159.x` only through its own later reviewed pin PR and qualification.

This strategy does not:
- weaken N1 or N2;
- transfer CodeSpace process ownership to DevGuard;
- make DevGuard's Codex dependency part of the CodeSpace shared client by default;
- approve a specific `0.159.x` tag today;
- approve decision 2, 3 or 4;
- authorize W4 or W5 implementation;
- remove the CS-RG hold.

If satisfying the transitional combination would require violating N1 or N2, classify that combination as unsupported
rather than changing either invariant.
````
