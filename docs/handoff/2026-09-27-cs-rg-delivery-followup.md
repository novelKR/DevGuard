# Session handoff: CS-RG delivery follow-up

This document hands off the follow-up work of 2026-09-27 that connects [design revision 1](../design-revision-1.md) to both repositories. It is a dated snapshot: it is not maintained afterwards and it is not an authoritative planning document. Re-query GitHub and git before acting, and treat the [planning documents](../planning/README.md) as the sources of truth. It supplements the [earlier handoff of the same date](2026-09-27-cs-rg-revision-and-codespace-ci.md), which stays unchanged as the record of its own moment. English only; personal information is left out.

## 1. Decisions fixed by the owner's instruction of 2026-09-27

These decisions were settled by that instruction. They are not open questions.

- **Identifiers.** Design provenance is the PR #8 merge `d4981b4c241cff42687f5c2c681b583c7847776e`. The DevGuard source a unit builds on is recorded separately; at the time of writing that was `30b5fa6f705f053876a8da8d00882772bcf4c41b`, after PR #9. The qualified release `0.1.0-5daee5d-b3fa569e` is a release ID, not a source commit.
- **CSP-D04 scope.** It covers the five CodeSpace counterpart documents and their Korean versions. CodeSpace's dependency and CI-selection rules are reviewed and documented in it. No product root or component is registered for code that does not exist; the PR that adds a crate or backend registers it together with its tests.
- **Order.** CSP-D04 is delivered before CSRG-P0, and DGP-D07 states this in the CS-RG plan and readiness.
- **CSRG-C00 start.** Starting CSRG-C00 is already authorized, subject to conditions. It begins without another question once all of these hold:
  - CSP-D04 is delivered: merged, with the CodeSpace main CI and documentation publication verified.
  - DGP-D07 is delivered.
  - The CodeSpace test baseline the change affects is fixed (the ETXTBSY fixture race).
  - A safe verification environment is ready.
- **Evidence.** Preserving raw verification evidence is mandatory. [PR delivery](../planning/pr-delivery.md#cleanup-and-evidence-preservation) states the order.
- **Kept state.** Remote branches, the CodeSpace fixture branches of #71 and #72, untracked tool settings, `.codex/`, refs and checkpoints written by other tools, and stashes are all kept.
- **Separate approvals.** Each of these needs its own approval:
  - every PR merge;
  - operational DevGuard changes: host configuration, credentials, consumer registration, the installed release, journals and the user service;
  - repository settings: branch protection, rulesets, required checks and auto-merge;
  - remote branch deletion or restoration, unmerged local branch deletion, force-push, hard reset, broad clean and stash deletion;
  - a Codex pin change, a Codex dependency in the DevGuard product, or a public MCP contract change;
  - CSRG-C01 and later product work, and long CSRG-C08 foreground or SLO measurement.

## 2. State at a glance

| Item | State when written | Next action |
| --- | --- | --- |
| DevGuard [PR #9](https://github.com/novelKR/DevGuard/pull/9) (survivor race in the reconcile test) | Merged as `30b5fa6`; main CI run 36294359318 passed | None |
| DevGuard [PR #10](https://github.com/novelKR/DevGuard/pull/10), DGP-D07 (this document included) | Open | Review; merge on approval; post-merge main CI |
| CodeSpace [PR #73](https://github.com/novelKR/CodeSpace/pull/73), CSP-D04 | Open; documentation build passed, and no Rust leg is selected | Merge on approval; post-merge CI and documentation publication |
| CodeSpace [PR #74](https://github.com/novelKR/CodeSpace/pull/74), ETXTBSY fixture race | Open; CI running when written | Merge on approval; post-merge CI; later, one read of the next scheduled run |
| CodeSpace branch `codex/diag-etxtbsy-fixture` (`defac2a`) | Pushed only for diagnostic run 36302744023; never to be merged; kept | Deleting it needs the owner's approval |
| CSRG-C00 | Not started; its conditions are listed above | Non-changing pre-investigation may proceed |

Findings behind PR #74, at their evidence levels:

- **Direct experiment.** A child held between fork and exec while a fixture script is written in-process makes the script's start fail with ETXTBSY. A child-process writer does not. Under concurrent spawns, 42 of 1600 starts failed with the in-process writer and none with the child writer.
- **Inference.** The failing scheduled run had a concurrently forked test child holding the descriptor. That run's holder was not captured.

## 3. Evidence

All evidence is kept locally in the git-ignored `evidence/` directory of the persistent DevGuard checkout. Each directory has a `MANIFEST.json` with SHA-256 and sizes, and its collector is `evidence/tools/collect_ci_evidence.py`.

| Directory | Content |
| --- | --- |
| `dg1-delivery/pr9-reconcile-survivor-race/` | The PR #8 head's failed and passing runs; PR #9's branch, PR and main runs; `repro-2026-09-27/`, a new reproduction of the race (patch, commands, environment, 21 raw logs, results) and of the zombie comparison |
| `dg1-delivery/pr8-cs-rg-plan-review/` | Post-merge main CI of `d4981b4` |
| `dg1-delivery/dgp-d07-delivery-conditions/` | Local verification logs of this unit |
| `codespace-ci/` | The scheduled failure, and the push and manual runs at `b6e7ed2` |
| `codespace-delivery/csp-d04/`, `codespace-delivery/etxtbsy/` | Local verification of CSP-D04; the ETXTBSY diagnostic run and its patch |

PR #9's original local instrumentation logs, its 30-run, 5-run and launcher-run logs, and the first zombie experiment were deleted before being preserved. They are recorded as lost; the new reproduction is a new record, not a restoration.

## 4. Next steps

1. **Merge each approved PR.** Merge with `gh pr merge <number> --repo <owner/repo> --merge --match-head-commit <verified full SHA>`. Then:
   - check the merged state, the merge commit's parents and tree;
   - read the separate post-merge main workflows once they finish;
   - for CodeSpace, also read the documentation publication;
   - only then remove the task worktree and the merged local branch.
2. **Start CSRG-C00** once its conditions hold. Follow [CS-RG](../planning/milestones/CS-RG.md#csrg-c00--verify-the-managed-execution-boundary) and the [CS-RG execution verification](../planning/verification.md#cs-rg-execution-verification).
   - Begin with the Gateway and UDS Runner spawn, reap and descriptor ownership table.
   - Use fixture authorities first. If a case needs the real host authority in a way that touches the user service or its credentials, request only that change, with its scope, reason and rollback.
3. **Next scheduled CodeSpace run.** After PR #74 merges, read the next scheduled run (daily 19:43 UTC) once. A manual full run is not a scheduled event.

## 5. Pitfalls met

- Agent sandboxes do not share scratch directories. Put inputs meant for another agent under a git-ignored path of its task worktree.
- In this sandbox, DevGuard's SQLite-based script tests and CodeSpace's script tests need `TMPDIR` inside the worktree.
- The documentation site requires exactly Node 24.21.0 (`engine-strict`), and `npm ci` refuses other versions.
- `gh api .../jobs/<id>/logs` output to a terminal is refused; download the run's log archive to a file instead.
