# CS-RG session handoff: owner instructions, decisions and state (2026-10-03)

> **Status: dated, non-normative handoff record; not maintained.**
> - It is a snapshot of 2026-10-03. GitHub state was re-read between 15:15 and 17:30 UTC that day.
> - It approves nothing: no merge, unit start, pin or dependency change, tracker update or service change.
> - Where it summarizes an owner instruction, the verbatim record listed in section 4 prevails.

This record hands off the CS-RG work of five Claude Code sessions, labelled A to E in section 2, so that the owner, a
reviewer, another agent or a later session can continue it without access to those sessions. The owner gave every
instruction in session chat, so until this record none of them was in either repository; the records listed in
section 4 now hold them verbatim. At the owner's request it was compiled from the sessions' chat records, re-checked
against GitHub and published as a shared resource. It is written in English only and has no Korean counterpart.
Personal information about the repository owner, local paths and session links are deliberately left out. Re-query
GitHub and git before acting.

Claims are *records* (a PR, issue, commit or CI run, linked where useful) unless marked *transcript* (what a session's
chat shows) or *inference*. Times are UTC.

## 1. State at a glance

| Item | State when written | Next action | Decided by |
| --- | --- | --- | --- |
| CodeSpace `main` | `ebed574`, the merge of [#84](https://github.com/novelKR/CodeSpace/pull/84) (CSRG-U2) on 2026-10-03 at 14:02; post-merge CI [run 37128224856](https://github.com/novelKR/CodeSpace/actions/runs/37128224856) passed | — | — |
| DevGuard `main` | `f1f9084`, the merge of [#21](https://github.com/novelKR/DevGuard/pull/21) on 2026-10-01 at 06:49; unchanged since | — | — |
| Pins in CodeSpace | Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (`rust-v0.154.0`) and the DevGuard client and contract at `f1f908429abea962d62d6b53c56a26c17250179b`, both unchanged | Change only with a separate approval | Owner |
| CSRG-U1 | Merged and verified: [#82](https://github.com/novelKR/CodeSpace/pull/82) as `f0a3322` (2026-10-02 12:52) and its hardening [#83](https://github.com/novelKR/CodeSpace/pull/83) as `3206865` (14:42) | — | — |
| CSRG-U2 | Merged and verified: #84 at head `2239085` as `ebed574` | Record it in DevGuard's ledger and the trackers (section 6) | Owner |
| CSRG-U3 | Not started. It is next in the owner's order of 2026-10-03 and is defined in the U1–U6 instruction | Decide whether that section is the instruction or a detailed one follows, as it did for U2 | Owner |
| CSRG-U4 to U6 | Not started | — | Owner |
| Independent fixes, not counted as CS-RG progress | [#81](https://github.com/novelKR/CodeSpace/pull/81), descriptor hygiene, as `dcf8c51` (2026-10-02 07:59); [#86](https://github.com/novelKR/CodeSpace/pull/86), the macOS fork-abort mitigation, as `02cf905` (2026-10-03 09:59) | — | — |
| Open items | [CodeSpace #85](https://github.com/novelKR/CodeSpace/issues/85), [DevGuard #22](https://github.com/novelKR/DevGuard/pull/22), the trackers [DevGuard #14](https://github.com/novelKR/DevGuard/issues/14) and [CodeSpace #76](https://github.com/novelKR/CodeSpace/issues/76), and [CodeSpace #79](https://github.com/novelKR/CodeSpace/issues/79) | Section 6 | Owner |

## 2. Sessions and sources

| Session | Period | Where it ran | What it did |
| --- | --- | --- | --- |
| E, "repository overview" | 2026-09-26 01:49–03:14 | The owner's Mac, through Remote Control | Closed DG1-P6 by merging [DevGuard #7](https://github.com/novelKR/DevGuard/pull/7) |
| D, "current state of both repositories" | 2026-09-26 03:51–05:53 | The owner's Mac, through Remote Control | Surveyed both repositories, merged [CodeSpace #66](https://github.com/novelKR/CodeSpace/pull/66), reviewed the CS-RG plan and opened [DevGuard #8](https://github.com/novelKR/DevGuard/pull/8) and [CodeSpace #67](https://github.com/novelKR/CodeSpace/pull/67) |
| C, "Fix flaky hashFiles cache key in CodeSpace CI" | 2026-09-26 05:48–17:54 and 2026-09-30 08:42–09:06 | The owner's Mac, through Remote Control | Fixed and reworked CodeSpace CI (#67–#70), applied design revision 1 on DevGuard #8, reported the state on 2026-09-30 and analysed documentation drift |
| A, "DevGuard/CodeSpace investigation handoff" | 2026-09-30 08:43 – 2026-10-03 05:11 | The owner's Mac, through Remote Control | Work packages A to C, the #79 fix (#81), U1 (#82, #83) and U2 (#84) |
| B, "Investigate macOS fork aborts in CodeSpace spawns" | 2026-10-03 05:44–15:11 | A cloud session | The #86 mitigation, then U2's revalidation and merge |

- The titles of sessions D and E are translated from Korean.
- The work of 2026-09-27 to 2026-09-29, including the CS-RG hold (DevGuard #9 to #16, #18 and #19; CodeSpace #73 to
  #78), ran outside these five sessions. The handoffs of 2026-09-27 and 2026-09-28 in this directory record it.
- The trackers DevGuard #14 and CodeSpace #76 and the PR descriptions are the other sources. Their comments are
  posted through the same `novelKR` account the agents use, so a comment alone does not prove an approval: an
  approval is the owner's direct instruction to the session doing the work (DevGuard #14's rule).
- The sessions' own reports were used as leads only; values re-read on GitHub carry links.

## 3. CS-RG units

The owner's instruction of 2026-10-02 13:54 ([record](2026-10-02-cs-rg-u1-u6-instruction.md)) defines CS-RG as six
implementation units in CodeSpace. They are separate from the suspended CSRG-C00–C09 plan in this repository, whose
suspension notice still stands. Each unit starts only after the previous one is merged and verified, and stops at a
request for exact-head merge approval. The table summarizes; the record prevails.

| Unit | Scope (the instruction's objective) | Definition of done and prohibitions | State |
| --- | --- | --- | --- |
| U1, status-only opt-in integration | A DevGuard status session (connect, `Hello`, `Authenticate`, `Status`), off by default at build and at run time | An implementation PR that proves the pin and dependency boundaries, preserved feature-off behaviour, status and error mapping, credential protection and no child descriptor inheritance | Merged (#82, hardening #83) |
| U2, execution-owner registration | The actual execution owner (the Gateway for InProcess, the worker for UDS) establishes and proves a registered DevGuard identity; nothing is admitted or launched | Verified registration per mode, conservative failure handling, private credential delivery and a policy under which `required` runs nothing unmanaged; detailed in the [U2 instruction](2026-10-02-cs-rg-u2-instruction.md) | Merged (#84) |
| U3, admission, attempt identity and the pre-spawn slot | Authorization and workspace readiness, then a CodeSpace process slot, then DevGuard admission, then a one-shot prepared execution. Admission alone spawns nothing | A one-shot pre-spawn preparation whose slot, resources and attempt identity agree, with conservative handling of uncertainty. No `BeginLaunch`, carrier, helper launch, observation or reaping integration, or pin change | Next; not started |
| U4, managed launch (`BeginLaunch`, helper, carrier) | Consume one prepared execution, then `BeginLaunch`, a one-time launch authority, the CodeSpace-owned spawn path, the DevGuard helper and scope, and the payload attempt | Run a prepared attempt exactly once under DevGuard's resource authority and name the supported and unsupported modes. Stop and ask for pin approval if a primitive is missing | Not started |
| U5, observation, reaping, release, approvals and no replay | Exactly one reaper per process, observation distinct from reaping, root exit distinct from scope exit, evidence-based release, no re-run of uncertain work | One consistent lifecycle across success, failure, descendants, cancellation, timeout and loss of authority. P1-RECOVERY does not start | Not started |
| U6, parity, fault qualification and completion | Establish and qualify the supported combinations, distinguishing implemented, feature-verified, platform-verified and qualified | Preserved qualification evidence for the candidate CodeSpace, DevGuard and Codex combination and an updated ledger. P1-RECOVERY, the next milestone, needs separate approval | Not started |

## 4. Owner instructions and decisions

The owner gave all of these in session chat. Each long instruction has its own record; the short instructions and the
answers to the sessions' questions are in [one record](2026-10-03-cs-rg-owner-short-instructions.md), in time order.

| Time | Session | Instruction or decision | Outcome | Record |
| --- | --- | --- | --- | --- |
| 09-30 08:43 | A | Takeover: re-read GitHub directly and report the decision points. Settled policy, the joint constraints N1 and N2, pin policy, evidence hierarchy, prohibitions | Takeover report | [takeover](2026-09-30-takeover-instruction.md) |
| 09-30 11:26 | A | Continuation: work packages A (normative reconciliation PRs, not to be merged), B (#79 steps 1 and 2 and one bounded run) and C (D6 option (b) on paper), in that order; the staged pin strategy of section 12 | DevGuard #20, CodeSpace #80, the #79 run, DevGuard #21 | [continuation](2026-09-30-continuation-instruction.md) |
| 09-30 11:58 | A | Three answers: update current-policy text but annotate dated decision records without rewriting them; include README.md:49 without widening the scope; measure only the Runner's pipe and PTY paths for #79 and record the worker as `not_run` | Applied in packages A and B | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 09-30 14:12 | A | Two answers: one CI classification entry is allowed, inside #20 only; B and C stop until package A is complete | #20's CI passed | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 09-30 15:31 | A | Close package A without merging, then go on to B; keep section 12's 0.154 baseline | B proceeded | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-01 05:43 | A | A pasted recommendation, which the session treated as advice and confirmed with questions | The answers of 06:10 | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-01 06:10 | A | Four answers: merge #20, then #80, then #21 (#21 only as an analysis record); fix #79 in a PR with real code and regression tests; no standalone 0.159.x evaluation; no separate inventory PR | Three merges; #81 opened | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-01 08:11 | A | Harden and complete #81 | #81 at head `2629f18` | [#81 hardening](2026-10-01-codespace-81-hardening-instruction.md) |
| 10-02 06:41 | A | Six closing steps for #81 ending in an exact-head approval request, and a recommendation to ask before starting U1 | #81 merged | [#81 closeout](2026-10-02-codespace-81-closeout-instruction.md) |
| 10-02 08:38 | A | U1 | #82 merged | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-02 13:54 | A | Units U1 to U6 | U1 compared with its section; deviations D-1 to D-5 reported | [U1 to U6](2026-10-02-cs-rg-u1-u6-instruction.md) |
| 10-02 14:13 | A | Close D-1 to D-5 in one narrow hardening PR first; U2 after its merge and green post-merge CI | #83 merged | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-02 15:33 | A | U2 in detail; do not merge it yourself and do not start U3 | #84 | [U2](2026-10-02-cs-rg-u2-instruction.md) |
| 10-03 04:01 | A | Check the #84 macOS timeout test | The fork defect found and split out | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-03 05:44 | B | Investigate and fix the macOS fork aborts | #86 | [#86](2026-10-03-codespace-86-instruction.md) |
| 10-03 09:57 | B | The eight-step order | Steps 1 to 5 done | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |

The owner's order of 2026-10-03 09:57 is: 1 approve the exact-head merge of #86 at `3b1ed596`; 2 verify its post-merge
`main` CI; 3 update #84 against the new `main` and resolve the three conflicts; 4 re-validate #84 with the macOS test
isolation removed; 5 merge U2 on a green exact head; 6 proceed with U3; 7 resolve #85 before U4 at the latest; 8 hold
structural spawn redesigns, such as a trampoline, pending a separate owner decision. Steps 1 to 5 are done.

### 4.1 Decisions still in force

Quoted from the records.

2026-10-01 06:10, the definition of done for further work:

> The completion criteria for the next task are not additional investigation documents or scratch results, but an
> implementation PR containing reviewable product code and regression tests. The CodeSpace #79 fix is counted as an
> independent product defect fix and must not be counted toward CS-RG milestone progress. To advance CS-RG itself, do
> not simply resume the previously suspended C00/C03/C09; instead, concisely define the first implementation unit and
> its completion criteria aligned with currently approved boundaries, and obtain separate approval for implementing
> that unit. Integrate necessary verifications directly into the implementation, and do not treat unresolved safety
> conditions as passed.

2026-10-01 06:10, the 0.159.x evaluation:

> Do not start as a standalone evaluation now. First determine DevGuard's actual consuming components and the code to
> be replaced, then include it as the necessary version comparison and verification within that component's
> implementation PR. This does not cancel the direction of adopting DevGuard early; it means avoiding repeating
> evaluations as isolated tasks without a consumer.

2026-10-01 06:10, DevGuard #21:

> Merge #21 strictly as an analytical record, without treating its conclusions or candidates as implementation
> approval.

2026-10-02 06:41, work kept out of the #81 and U1 sequence:

> I intend not to mix 0.159.x evaluation, D6 implementation, and carrier implementation into this sequence. Performing
> each of those within its respective implementation PR as needed—once actual consumption points and launch boundaries
> materialize after U1—best aligns with the principle I have established: "an implementation PR, not investigation
> itself, is the definition of done."

2026-10-02 14:13, U1 hardening and U2:

> Close D-1 through D-5 first in one narrowly scoped U1-hardening implementation PR. Do not add registration,
> admission, launch, reaping, or pin changes. After that PR is merged and its post-merge CI is green, proceed with
> CSRG-U2 exactly as defined, including updating current milestone/tracker state to CS-RG: in-progress.

2026-10-03 09:57, the principle behind the order:

> Close verified product defects with actual code, but do not unnecessarily mix independent structural changes into
> CS-RG work.

## 5. Standing rules and constraints

Rules that recur across the instructions and still apply. The source is in parentheses.

**Approval and merges**
- Merge only at the head the owner approved, with `gh pr merge <n> --merge --match-head-commit <sha>`. If `main` has
  moved, re-verify first on a local simulation of the exact merge; read the separate post-merge `main` CI; keep remote
  branches ([continuation](2026-09-30-continuation-instruction.md), section 9).
- A green CI, a mergeable state or a successful experiment is not an approval. DevGuard `main` has no branch
  protection, so GitHub's CLEAN state is not a gate (same section).
- Tracker comments go through the shared `novelKR` account and are not evidence of approval by themselves; session A
  said so on each DevGuard #14 comment it posted (*transcript*).
- A CS-RG unit starts only when its predecessors are merged on `main`; otherwise stop and report before changing code.
  No unit merges without exact-head approval ([U1 to U6](2026-10-02-cs-rg-u1-u6-instruction.md)).

**Pins and dependencies**
- Codex stays at `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (`rust-v0.154.0`) and the DevGuard client and contract at
  `f1f908429abea962d62d6b53c56a26c17250179b`. If a change becomes necessary, stop and ask for a separate approval
  ([U2](2026-10-02-cs-rg-u2-instruction.md), [#86](2026-10-03-codespace-86-instruction.md)).
- Pins move in stages, not in lockstep. CodeSpace stays on its reviewed 0.154 baseline; DevGuard may evaluate and, if
  separately approved, consume a reviewed `rust-v0.159.x`; a later CodeSpace move to 0.159.x is its own reviewed pin
  decision. Matching version strings alone do not show convergence
  ([continuation](2026-09-30-continuation-instruction.md), section 12).
- The 0.159.x evaluation, D6 implementation and carrier implementation are done inside the implementation PR that
  needs them, not as standalone work (answers of 2026-10-01;
  [#81 closeout](2026-10-02-codespace-81-closeout-instruction.md)).
- DevGuard may use Codex and other external dependencies behind explicit adapter boundaries. That settles permission
  only: each dependency, tag, patch, pin move, merge, runtime integration, upstream submission or release still needs
  its own approval ([continuation](2026-09-30-continuation-instruction.md), section 3).
- DevGuard #21's D6 candidates are an analysis record, not implementation approval (answers of 2026-10-01).

**Design invariants** ([continuation](2026-09-30-continuation-instruction.md), section 4)
- N1 and N2 are jointly mandatory; neither has priority. A combination that cannot satisfy both is classified
  unsupported; neither invariant is weakened to make it fit.
- N1, CodeSpace: preserve its product semantics and responsibility boundaries, selective Codex delegation and
  independent, governance-free `off` behaviour. Introduce none of these merely for DevGuard: a CodeSpace-owned PTY
  backend or reaper, a replacement process runtime, a rewrite of the pipe path, a transfer of lifecycle ownership, a
  hidden backend inside a thin adapter.
- N2, DevGuard: preserve admission and accounting integrity, stable attempt identity, one-time launch authorization,
  credential and permit safety, honest uncertainty with no replay of uncertain execution, correct resource evidence
  and conservative reconciliation, one actual reaper, no false release, and normal recovery for supported healthy
  workloads.
- CodeSpace remains the execution owner ([U1 to U6](2026-10-02-cs-rg-u1-u6-instruction.md)).

**Scope of work**
- The definition of done is an implementation PR with reviewable product code and regression tests, not an
  investigation document (answers of 2026-10-01).
- Close verified product defects with real code, but keep independent structural changes out of CS-RG work.
  Independent fixes do not count as CS-RG progress (the order of 2026-10-03; answers of 2026-10-01).
- Do not weaken a test or gate merely to obtain green CI. Do not weaken descriptor hygiene, fall back to automatic
  retry, change a pin or add a backend merely to close a task
  ([#81 hardening](2026-10-01-codespace-81-hardening-instruction.md), [#86](2026-10-03-codespace-86-instruction.md)).
- Do not change CI or tests as a side task; the only exceptions are those the owner names
  ([continuation](2026-09-30-continuation-instruction.md), section 9; answers of 2026-09-30 14:12).
- Do not simply resume the suspended CSRG-C00, C03 and C09, and do not present them as executed unchanged (answers of
  2026-10-01; [U2](2026-10-02-cs-rg-u2-instruction.md)).

**Evidence and records** ([continuation](2026-09-30-continuation-instruction.md), section 9)
- Evidence hierarchy, highest first: owner decisions; live GitHub and merged source; the W0–W3 records and
  experiments; local raw evidence; agent inference.
- Evidence sets are new and additive, carry `MANIFEST.json` SHA-256 digests and are never rewritten once sealed.
  Failed and invalid attempts are kept, and a hypothesis is stated before any re-run. The sets are under
  `<DEVGUARD_CHECKOUT>/evidence/` on the owner's host (*transcript*).
- Public records carry no private paths or credentials; use `<DEVGUARD_CHECKOUT>`. Tracker comments are rendered from
  a template, and the posted body is checked against the read-back body.

**Security and the working environment**
- A credential must never appear in MCP arguments, in CLI arguments as secret material, in an environment variable,
  in logs, in status output or in a user payload, remain in an unrelated process, or be inherited by unrelated
  children ([U2](2026-10-02-cs-rg-u2-instruction.md)).
- No destructive blanket cleanup such as `git clean -fdx`. Never delete sealed evidence sets, qualification records,
  manifests or recorded hashes, source-controlled files, unpushed work or DevGuard's durable operational state, nor the
  global Cargo registry and git caches without a separately justified need ([U2](2026-10-02-cs-rg-u2-instruction.md)).
- Diagnostics do not use the installed DevGuard or CodeSpace services and record the DevGuard LaunchAgent's pid before
  and after ([continuation](2026-09-30-continuation-instruction.md), section 7).

**Next milestone**
- P1-RECOVERY does not start without separate approval, even after U6. Structural spawn redesigns wait for a separate
  owner decision ([U1 to U6](2026-10-02-cs-rg-u1-u6-instruction.md); step 8 of the order of 2026-10-03).

## 6. Open decisions and next actions

None of these was acted on while this record was written; each waits on the owner. States were read on GitHub on
2026-10-03.

| Item | State when written | Decision or next action |
| --- | --- | --- |
| Starting U3 | Step 6 of the owner's order is "Proceed with U3". The start condition of the U2 instruction, an approved U2 merge with green post-merge CI, is met by #84 and run 37128224856. The scope is the CSRG-U3 section of the U1–U6 instruction | Decide whether that section is the instruction or a detailed instruction follows, as it did for U2 |
| [DevGuard #22](https://github.com/novelKR/DevGuard/pull/22), the CS-RG ledger | Open at head `afa9108`, unchanged since 2026-10-02 19:36; it records U2 as `active`. U2's definition of done includes DevGuard's milestone state recording `CS-RG: in-progress`, which `main` does not do until #22 merges. AGENTS.md's "CS-RG implementation is suspended" bullet, to which #22 adds a dated note, is unchanged on `main` | Update #22 for the U2 merge, then decide on an exact-head merge |
| Trackers [DevGuard #14](https://github.com/novelKR/DevGuard/issues/14) and [CodeSpace #76](https://github.com/novelKR/CodeSpace/issues/76) | Last updated 2026-10-02 14:03, so #86 and the U2 merge are missing. U2's definition of done also includes "trackers reflect U1 + hardening complete and U2 implemented" | Decide whether to post a current-state comment |
| [CodeSpace #85](https://github.com/novelKR/CodeSpace/issues/85), signal-death reporting | Open: `process_status` reports a signal death that CodeSpace did not cause as a normal exit | Resolve it as a separate product fix or contract decision before U4 at the latest (step 7) |
| DevGuard's 250 ms credential deadline | Found during U2's verification: the UDS worker reads its handed secret once, so a stall longer than 250 ms leaves it `credential_unavailable` until it restarts. U2's tests retry sessions that stop at the deadline. Session B judged that this matters once registration gates execution (*inference*) | Decide whether to file a DevGuard product issue |
| Diagnostic branch [`claude/magical-euler-nvqrxp`](https://github.com/novelKR/CodeSpace/tree/claude/magical-euler-nvqrxp) | #86's head branch. After the merge it carries temporary diagnostic workflow commits, the latest `0127711`. Remote branches are kept by rule | Decide whether to reset it to `main` or leave it |
| [CodeSpace #79](https://github.com/novelKR/CodeSpace/issues/79), descriptor inheritance | Fixed by #81 and left open on 2026-10-02 for the owner to close | Close it |
| Structural spawn redesigns, such as a trampoline | On hold (step 8) | None until a separate decision |
| P1-RECOVERY | The critical-path milestone after CS-RG | Does not start without separate approval |

## 7. Earlier sessions C, D and E (2026-09-26 to 2026-09-30)

These sessions precede session A. Their instructions were short, and session A's records supersede them except where
noted below.

| Session | Period | Work | Result |
| --- | --- | --- | --- |
| E | 2026-09-26 01:49–03:14 | Finished DG1-P6: pushed a test fix and merged #7 at its exact head | [DevGuard #7](https://github.com/novelKR/DevGuard/pull/7) merged as `395315d` (02:44) and `main` CI passed. DG-1 complete; release `0.1.0-5daee5d-b3fa569e` SLO-qualified on macOS only |
| D | 2026-09-26 03:51–05:53 | Surveyed both repositories under four owner instructions | [CodeSpace #66](https://github.com/novelKR/CodeSpace/pull/66) merged as `a116687`; a local CS-RG plan review report and the plan-revision PR [DevGuard #8](https://github.com/novelKR/DevGuard/pull/8); the `.codex/config.toml` draft PR [CodeSpace #67](https://github.com/novelKR/CodeSpace/pull/67) |
| C | 2026-09-26 05:48–17:54; 2026-09-30 08:42–09:06 | The CI cache-key fix and a CI overhaul; two design re-reviews; design revision 1 and an English handoff on DevGuard #8; a status report and documentation-drift analysis on 2026-09-30 | #67 (`339ae8e`), #68 (`0579c50`), #69 (`af895c5`) and #70 (`b6e7ed2`) merged; a full CI run fell from about 17 to 4–5 minutes; the check PRs #71 and #72 were closed unmerged. DevGuard #8 merged outside the session on 2026-09-26 at 18:46 as `d4981b4` |

Session E's record is a copy of a local conversation reopened on 2026-10-03; only its last 200 events were readable
(*transcript*).

### 7.1 Owner instructions and decisions

Most instructions in sessions C, D and E were in Korean; quoted words below are translations (*transcript*).
- E: four short instructions, such as "judge the merge"; the session merged #7 at its exact head.
- D, 04:12: "For CodeSpace, align the checkout with origin/main, clean up the local branches, update the
  current-state wording of the integration documents, then start the CS-RG plan review." Asked how far to take the
  integration documents and what the review should produce, the owner answered "through to merge" and "a report and a
  plan-revision PR"; the session had recommended a local report and a chat summary.
- D, 05:51–05:52: the owner asked for the long-staged `.codex/config.toml` to go up as a draft PR (the session had
  recommended not creating one), with its purpose stated as unifying the project's working environment.
- C, 05:48: fix the intermittent failure of `hashFiles('**/Cargo.lock')` in CodeSpace CI by hashing only the named
  lockfile paths, change nothing else, and ask before merging. At the owner's choice the fix went into #67 rather
  than a new PR.
- C, 07:09–09:39: a review of CI improvements. The owner said another repository was only a reference for CI
  structure, not something to change, and chose per-job rust-cache, measuring the cache before deciding, and checks
  selected by changed paths (the session had recommended concurrency limits only). The plan was approved at 09:39,
  and #68, #69 and #70 were each approved for merge separately.
- C, 15:50–16:33: the owner pasted a long analysis of the current state and structure for review, then pasted a
  rebuttal of that review and asked for a re-review.
- C, 16:52: apply the pasted "DevGuard PR #8 revision and CodeSpace execution-layer design revision spec" to #8 as a
  new baseline in English and Korean. This became [design revision 1](../design-revision-1.md).
- C, 17:48: add an English handoff without personal information, as a separate commit, so that another person or
  session could continue. This became [the handoff of 2026-09-27](2026-09-27-cs-rg-revision-and-codespace-ci.md).
- C, 2026-09-30 08:42–09:02: a current status report and an investigation of how the state differed from the
  previous session's and why the documents drifted; the owner then pasted the takeover instruction they had written
  and asked for the analysis to be corrected against it. Session A started from the same instruction at 08:43, and
  the work continued there.

### 7.2 What still matters

- Of the documentation drift session C found on 2026-09-30, README.md:49 was fixed by #20 (R2). The AGENTS.md
  suspension bullet is covered by the DevGuard #22 decision in section 6.
- Session C noted that #18 and #19 were merged through the shared account without review and that #14 recorded no
  approval for them. Whether that was corrected later was not checked.
- DevGuard #8's remote branch was deleted 16 seconds after its merge, contrary to the rule that remote branches are
  kept. Session C asked whether to recreate it; no answer is recorded.
- Session C's seven pending decisions of 2026-09-30 were settled the same day by the continuation instruction: #17
  merged at 09:12, D6 went to a paper analysis (#21), the upstream proposal and the carrier direction were deferred,
  the policy documents became package A (#20, #80), #79 became package B, and the local-only branch
  `codex/exp-a-preparation` on the owner's host was to be kept unchanged (section 5 of the continuation).
- `.codex/config.toml` reached CodeSpace `main` with #67.

## 8. Timeline

| Time | Event | Record |
| --- | --- | --- |
| 09-26 02:44 | DevGuard #7 merged; DG-1 complete | [DevGuard #7](https://github.com/novelKR/DevGuard/pull/7) |
| 09-26 04:44 | CodeSpace #66 merged | [CodeSpace #66](https://github.com/novelKR/CodeSpace/pull/66) |
| 09-26 06:37–11:59 | CodeSpace #67 to #70 merged | [CodeSpace #70](https://github.com/novelKR/CodeSpace/pull/70) |
| 09-26 18:46 | DevGuard #8, design revision 1, merged | [DevGuard #8](https://github.com/novelKR/DevGuard/pull/8) |
| 09-27 17:15 | CodeSpace #75, recording the CS-RG hold, merged; the trackers DevGuard #14 and CodeSpace #76 had opened at 15:21 and 15:22 | [CodeSpace #75](https://github.com/novelKR/CodeSpace/pull/75) |
| 09-30 08:43 | Session A starts with the takeover instruction | [takeover](2026-09-30-takeover-instruction.md) |
| 09-30 09:12 | DevGuard #17 merged | [DevGuard #17](https://github.com/novelKR/DevGuard/pull/17) |
| 09-30 11:26 | Continuation instruction: packages A to C and the staged pin strategy | [continuation](2026-09-30-continuation-instruction.md) |
| 09-30 11:58 | Answers on held text, README.md:49 and the scope of #79 | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 09-30 14:00 | Package A opened: DevGuard #20 (design revision 2) and CodeSpace #80 | [DevGuard #20](https://github.com/novelKR/DevGuard/pull/20), [CodeSpace #80](https://github.com/novelKR/CodeSpace/pull/80) |
| 09-30 14:12 | Answers: one CI classification entry inside #20; B and C stop until A is complete | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 09-30 15:42 | The #79 inventory and frozen protocol posted | [#79 comment](https://github.com/novelKR/CodeSpace/issues/79#issuecomment-5914631573) |
| 09-30 19:03 | The #79 bounded-run results posted: pipe children inherited other executions' descriptors | [#79 comment](https://github.com/novelKR/CodeSpace/issues/79#issuecomment-5917806992) |
| 09-30 19:16 | Package C opened: DevGuard #21, D6 option (b) on paper | [DevGuard #21](https://github.com/novelKR/DevGuard/pull/21) |
| 10-01 06:10 | Answers: merge #20, #80 and #21; fix #79 in a real PR; 0.159.x only inside a consumer's PR; an implementation PR as the next definition of done | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-01 06:13–06:49 | DevGuard #20, CodeSpace #80 and DevGuard #21 merged; DevGuard `main` is `f1f9084` | [DevGuard #21](https://github.com/novelKR/DevGuard/pull/21) |
| 10-01 06:50 | CodeSpace #81 opened for #79 | [CodeSpace #81](https://github.com/novelKR/CodeSpace/pull/81) |
| 10-01 08:11 | #81 hardening instruction | [#81 hardening](2026-10-01-codespace-81-hardening-instruction.md) |
| 10-02 06:41 | #81 closeout instruction | [#81 closeout](2026-10-02-codespace-81-closeout-instruction.md) |
| 10-02 07:59 | #81 merged as `dcf8c51` | [CodeSpace #81](https://github.com/novelKR/CodeSpace/pull/81) |
| 10-02 08:38 | U1 instruction | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-02 12:52 | U1, #82, merged as `f0a3322` | [CodeSpace #82](https://github.com/novelKR/CodeSpace/pull/82) |
| 10-02 13:54 | U1–U6 instruction; session A compares U1 with it and reports D-1 to D-5 | [U1 to U6](2026-10-02-cs-rg-u1-u6-instruction.md) |
| 10-02 14:13 | Close D-1 to D-5 in a narrow PR, then U2 | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-02 14:42 | U1's hardening, #83, merged as `3206865` | [CodeSpace #83](https://github.com/novelKR/CodeSpace/pull/83) |
| 10-02 15:33 | U2 instruction | [U2](2026-10-02-cs-rg-u2-instruction.md) |
| 10-02 19:36 | DevGuard #22, the CS-RG ledger, opened; still open | [DevGuard #22](https://github.com/novelKR/DevGuard/pull/22) |
| 10-02 19:54 | U2's PR, #84, opened | [CodeSpace #84](https://github.com/novelKR/CodeSpace/pull/84) |
| 10-03 04:01 | "Check for #84 macOS timeout test"; session A finds children dying between `fork()` and `exec` and splits the defect out | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-03 05:11 | Session A's last activity | *transcript* |
| 10-03 05:44 | Session B starts with the #86 instruction | [#86](2026-10-03-codespace-86-instruction.md) |
| 10-03 06:48 | Signal-death reporting filed as #85 | [CodeSpace #85](https://github.com/novelKR/CodeSpace/issues/85) |
| 10-03 08:08 | #86 opened | [CodeSpace #86](https://github.com/novelKR/CodeSpace/pull/86) |
| 10-03 09:57 | The owner's eight-step order | [short instructions](2026-10-03-cs-rg-owner-short-instructions.md) |
| 10-03 09:59 | #86 merged as `02cf905` | [CodeSpace #86](https://github.com/novelKR/CodeSpace/pull/86) |
| 10-03 14:02 | U2, #84, merged as `ebed574`; post-merge CI passed | [run 37128224856](https://github.com/novelKR/CodeSpace/actions/runs/37128224856) |
| 10-03 15:11 | Session B's final report | *transcript* |
