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
