# Session handoff: CS-RG design revision 1 and CodeSpace CI

> **Status: suspended as an implementation directive.** The CS-RG implementation step recorded in this handoff (starting CSRG-C00 on the owner's instruction) must not be followed. This is pending the CS-RG integration-boundary revalidation, an owner-directed review of the CodeSpace integration plan; it is not a work unit. No replacement architecture has been approved; the owner decides after reviewing its results. This notice suspends directives only and relaxes no safety requirement. The text below is retained unchanged for historical traceability.

This document hands off one working session so that a human reviewer, another agent or a later session can continue it. It is a snapshot dated 2026-09-27, the date the repository documents use for this work. It is not maintained afterwards, and it is not an authoritative design or planning document. Re-query GitHub and git before acting, and treat [design revision 1](../design-revision-1.md), the [design reference](../design.md) and the [planning documents](../planning/README.md) as the sources of truth. It is written in English only and has no Korean counterpart. Personal information about the repository owner is deliberately left out.

Claims below carry one of four evidence levels: *record* (a PR body, CI run or document), *code review* at a fixed revision, *test run*, or *raw data*. Figures are records unless marked otherwise.

## 1. State at a glance

| Item | State when written | Next action | Decided by |
| --- | --- | --- | --- |
| DevGuard [PR #8](https://github.com/novelKR/DevGuard/pull/8), branch `codex/cs-rg-plan-review` | Open and mergeable. At `760f479` the Ubuntu contract jobs had passed and the macOS jobs were running. The commit adding this handoff is the new head and runs CI again | Owner review; merge only on the owner's explicit approval | Owner |
| CodeSpace CI changes, [#67](https://github.com/novelKR/CodeSpace/pull/67)–[#70](https://github.com/novelKR/CodeSpace/pull/70) | Merged. CodeSpace `main` is `b6e7ed2`, and the post-merge push and manual full runs passed | Confirm the first scheduled full run (daily at 19:43 UTC), which had not fired yet | — |
| CodeSpace fixture PRs [#71](https://github.com/novelKR/CodeSpace/pull/71) and [#72](https://github.com/novelKR/CodeSpace/pull/72) | Closed without merge after they verified check selection and cancellation; remote branches kept | None | — |
| CSP-D04, the CodeSpace counterpart documentation of design revision 1 | Not started | Start after PR #8 merges, linking the actual merge SHA | Owner confirms the start and scope |
| CS-RG implementation, CSRG-P0 (CSRG-C00) onward | Not started and not authorized | Only on the owner's instruction | Owner |

## 2. Repositories, references and checkouts

- **DevGuard** ([novelKR/DevGuard](https://github.com/novelKR/DevGuard)). `main` is `395315d`: DG-1 is complete, and release `0.1.0-5daee5d-b3fa569e` is macOS-qualified and is CodeSpace's pin candidate. PR #8 has these commits on base `395315d`:
  - `92a3472` (DGP-D05, from an earlier session): the CS-RG plan aligned with the implemented DG-1 consumer interface.
  - `b17eb4c` (DGP-D06): the design revision 1 baseline pair, committed alone so the baseline has its own SHA.
  - `760f479` (DGP-D06): revision 1 applied to the design reference and the plan.
  - The commit that adds this handoff.
- **CodeSpace** ([novelKR/CodeSpace](https://github.com/novelKR/CodeSpace)). `main` is `b6e7ed22e2c730ac987297455e250cbd6e8e8b0c`, the confirmation baseline of revision 1. Remote task branches are kept on purpose: `codex/project-codex-config`, `codex/ci-rust-cache`, `codex/test-reuse-patch-helper`, `codex/ci-affected-selection`, `codex/ci-fixture-docs-only`, `codex/ci-fixture-pty-only` and the earlier `codex/devguard-dg1-status`.
- **Codex.** CodeSpace pins `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` as the submodule `third_party/codex`, and revision 1 keeps that pin. Upstream `main` at `b334d5b3f2d9441b95286a8c2af8c2152737d977` was only compared, never adopted.
- **Local checkouts**, on the owner's host only. The persistent checkouts are `/Volumes/DevData/Projects/IdeaProjects/DevGuard` and `/Volumes/DevData/Projects/IdeaProjects/CodeSpace`; both were clean on `main`. PR #8's task-owned worktree is `.local/worktrees/cs-rg-plan-review` inside the DevGuard checkout, and CodeSpace had no extra worktree. A git-ignored report, `evidence/cs-rg/plan-review.md`, holds the review findings F1–F11 from before revision 1 and exists only in the DevGuard checkout.

## 3. What the session did

### 3.1 CodeSpace CI (complete)

The owner asked to fix an intermittent CI failure, then to improve the shared cache and CI parallelism. Other repositories' CI structure served only as reference, and none of them was changed. Each PR was merged only after the owner approved it, with `--match-head-commit` on the verified head.

| PR | Change | Recorded effect |
| --- | --- | --- |
| #67, merge `339ae8e` | Cache keys hash six explicit lockfiles instead of `hashFiles('**/Cargo.lock')`, whose directory walk intermittently failed jobs. The PR also carried the owner's project Codex configuration commit | The failing walk is gone; all 15 jobs passed |
| #68, merge `0579c50` | `Swatinem/rust-cache` v2.9.2 pinned by SHA in every compile leg, one anchor workspace per leg, saves only on `main`, `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=0` | Twelve cache entries using 6.6 GB of the 10 GB limit; the next warm run took 285 s against 852 s cold, and the median before #68 was 1030 s |
| #69, merge `af895c5` | `ensure_helper_for_tests()` reuses an exported `CODESPACE_PATCH_BIN` and otherwise builds with `--locked` | Integration stage 815 s → 227 s |
| #70, merge `b6e7ed2` | A `CI / Plan` job selects legs from the changed paths (`scripts/ci-policy.json`, `scripts/ci_plan.py`); the `rust` job becomes an exact gate (`scripts/check_ci_results.py`); daily 19:43 UTC and manual full runs; per-PR cancellation; a read-only token | Full runs took 248–303 s. A documentation-only change runs no Rust leg, a pty-only change runs exactly 7 of 12 legs, and a cancelled run's gate fails by design |

Handing helper binaries between jobs ("PR C") was measured and rejected. Over six warm runs it projected a 16 s saving, and the Integration job led the next-longest job by 106 s, short of the required 120 s. The PR bodies hold the run links and per-job tables.

### 3.2 Design re-evaluation

The owner asked whether the execution split between CodeSpace, DevGuard and Codex is still appropriate. The first review rejected a Codex dependency partly because the approved design forbade it. An external review, supplied by the owner, corrected that and other faults: over-general claims about API visibility, calling `ProcessDriver` reusable without checking its loss and Drop contract, underrating the consolidation of reaping paths, missing the non-reentrant `spawn_guard`, and inconsistent evidence wording. These working rules came out of it:

- In a re-evaluation, an approved design is change-control information, not a reason to reject an alternative.
- Split the question into independent decisions. Compare alternatives on contract fit, code removed and added, dependency cost measured on the real graph, and transition cost; deferral is also a choice with costs.
- Name the evidence level of every claim, and search PR bodies and records before calling something unverified.
- A design change still needs the owner's explicit approval.

### 3.3 Design revision 1 and PR #8

The owner then supplied a revision specification, explicitly approved it after the mid-course review, and asked to record it as a separate bilingual document and new baseline. That is [design revision 1](../design-revision-1.md) with its [Korean text](../ko/design-revision-1.md), committed as `b17eb4c`. English is authoritative by repository policy. The Korean file is the supplied text with citation markers removed, references to individual review responses replaced with neutral wording, statements about PR #8's head dated, and heading levels aligned. An appendix lists code-review evidence at fixed revisions for the factual statements.

`760f479` applies the revision to the documents listed in its §14.1:

- **Decisions.** ADR-006 records execution ownership and the reuse policy. ADR-001 separates its current rule from the superseded `service-exec` wording, and the baselines table gains the new references.
- **Plan.** CSRG-C00 (execution-boundary fitness) forms CSRG-P0, and CSRG-C09 (the legacy-backend decision) joins C07 in CSRG-P4. C08 moves to CSRG-P5 and qualifies only the head C09 leaves. CS-RG has 10 units in 6 groups, and the whole plan has 48 units in 25 groups.
- **Design reference.** The permanent "no Codex" rule became a present engineering choice with a conditional reuse policy. The reference gains an execution ownership section, and its pin statement is scoped to this revision.
- **Other documents.** The integration specification, consumer readiness, verification, delivery plan, indexes, `README.md`, `docs/milestones.md` and `AGENTS.md` follow. Korean counterparts were reviewed and their hashes recorded.
- **Checker.** `scripts/check_docs.py` numbers units and groups explicitly so that CSRG can start at 0, and a regression test covers it.

Judgement calls made while applying the revision, open to the owner's review:

- The delivery plan gained the labels DGP-D06 (this revision) and CSP-D04 (the planned CodeSpace counterpart).
- ADR-006 and `AGENTS.md` note that the validator's dependency-boundary stage rejects any `codex-` or `codespace-` package. That stage enforces the current "no Codex dependency" choice, so a decision to reuse such a crate changes the stage in the same PR.
- The Korean `P1-RECOVERY.md` and `DG-LINUX.md` named predecessor group `CSRG-P4`; it became `CSRG-P5` because C08 moved.
- `docs/contracts.md` was left unchanged because it describes implemented behavior only.

Unchanged: `docs/design.ko.md` (SHA-256 `97b67a1f…`), `docs/design-source.json`, `milestones.json` and every implementation and qualification status. CS-RG remains `not-started`.

## 4. Standing constraints

- Ask the owner before merging any PR. Merge normally with `gh pr merge <number> --repo <owner/repo> --merge --match-head-commit <sha>`. Take the full SHA from `gh pr view <number> --json headRefOid`, compare it with the head you verified, and never reconstruct a SHA.
- Never delete remote branches, use admin bypass or weaken checks.
- Read CI once when needed; do not poll it in loops or with scheduled wake-ups.
- Work in task-owned worktrees under `.local/worktrees/`, and never commit in the persistent checkouts. In a CodeSpace worktree, never run `git submodule deinit`, because submodule configuration is shared with the main checkout.
- CodeSpace's `.codex/config.toml` is the owner's project configuration; keep it out of unrelated changes.
- Without the owner's instruction, do not start CSRG-P0 or any other CS-RG implementation, and do not change the operator configuration `~/.config/devguard/host.toml`, for example to provision a `codespace` consumer.
- `docs/design.ko.md` and its checksum are immutable. Design changes the owner directs go into a revision document and the English design reference.
- In DevGuard documentation, English is authoritative. Every file under `docs/planning/`, and `docs/design.md`, `docs/contracts.md` and `docs/operations.md`, has a Korean counterpart. Record a pair's hashes with `python3 scripts/check_docs.py record --id <id>` only after reviewing both files. `### WORK-ID` headings must match across a pair, and English sources contain no Hangul.
- Never commit credentials, journals, toolchains, runtime artifacts or evidence; `evidence/` is ignored.
- Keep implementation, platform qualification and design approval as separate states.

## 5. How to continue

### 5.1 PR #8 through merge

1. Check the head's CI once with `gh pr checks 8 --repo novelKR/DevGuard`. The workflow "Contracts and service boundary" runs `contracts (ubuntu-24.04)` and `contracts (macos-14)` for both the push and the pull-request events.
2. Wait for the owner's review. Apply requested changes as new commits, and never force-push over referenced commits.
3. On explicit approval, merge as in §4. Verify the merge commit and the push-triggered `main` run, fast-forward the persistent DevGuard checkout if it is clean, and remove the task worktree. Keep the remote branch.

### 5.2 CSP-D04: CodeSpace counterpart documentation

After PR #8 merges, and once the owner confirms, update CodeSpace as revision 1 §14.2 lists, in a new worktree from `origin/main`:

| Target | Change |
| --- | --- |
| `docs/devguard-integration.md` | Link design revision 1 and the new work order at the merged DevGuard SHA |
| `docs/architecture.md` | Common execution coordination and backend ownership |
| `docs/execution-substrate.md` | Separate exit observation, reaping, output and resource lifetimes |
| `docs/codex-reuse.md` | High-level spawn versus reuse of low-level utilities |
| `docs/upstream-update.md` | Limited adaptation policy and re-examination procedure |
| Dependency rules and CI selection policy | New resource-client and backend boundaries; coverage of the new pin, contract, guard and backend files |

Open question for the owner: should CSP-D04 change `scripts/upstream_dependencies.py` and `scripts/ci-policy.json` now, or only document those boundaries until CS-RG units add the crates? CodeSpace documentation also has Korean counterparts under `docs/ko/`, recorded with `python3 -B scripts/check_docs.py record --id <id>`. Its documentation site rejects raw HTML outside code, including comments. Record immutable links only to commits that exist.

### 5.3 First scheduled CodeSpace run

Run `gh run list --repo novelKR/CodeSpace --workflow ci.yml --event schedule --limit 3`. Expect `CI / Plan` to report a full plan (12 of 12 legs), all 16 jobs to pass, and the `rust` gate to print "Required CI: passed" with 14 reports. GitHub can delay or skip scheduled runs, so if none appears after the due time, check that before changing anything.

### 5.4 CS-RG implementation

> **Status: suspended as an implementation directive.** Do not implement from this section while the CS-RG integration-boundary revalidation is pending. No replacement architecture has been approved, and no safety requirement is relaxed. The text is retained unchanged for historical traceability.

Only on the owner's instruction, start with CSRG-C00 in [CS-RG](../planning/milestones/CS-RG.md). Read revision 1, the [CodeSpace integration specification](../planning/codespace-integration.md) and the [CS-RG execution verification](../planning/verification.md#cs-rg-execution-verification) first. Real-authority cases run on the qualification host: CodeSpace's 1 CPU control reservation leaves the three-CPU hosted macOS runner no work capacity, so hosted CI runs fixtures or records `not_run`.

## 6. Verification commands

DevGuard documentation, from the repository root:

```sh
python3 scripts/check_docs.py
python3 -B -m unittest discover -s scripts -p test_check_docs.py
python3 -B -m unittest discover -s scripts -p test_measure.py
git diff --check
```

At `760f479` these reported 17 reviewed pairs, 48 work units and 25 logical groups, then 9 and 62 passing tests and a clean diff (test run). The full validator, `python3 scripts/validate.py --offline`, also runs the Rust suites with Rust 1.95.0, one Cargo job and one test thread; CI runs it.

CodeSpace, from its repository root:

```sh
python3 -B scripts/check_docs.py
python3 -B -m unittest discover -s scripts/tests
python3 scripts/ci_plan.py --base origin/main
npm ci --prefix docs-site --ignore-scripts
npm test --prefix docs-site
npm run build --prefix docs-site
python3 -B docs-site/scripts/site.py check
```

`ci_plan.py --base` previews which CI legs a local change selects.

## 7. Pitfalls met in this session

- In zsh, `"$SHA:path"` applies a history modifier; write `"${SHA}:path"` for `git show`. `noclobber` refuses `>` over an existing file, so use `>|`. zsh has no `PIPESTATUS`.
- In GitHub Actions, `hashFiles('**')` walks the whole workspace, including `.git`, while literal paths avoid the walk. The runner also re-evaluates step inputs in post steps.
- rust-cache prunes a shared target once per listed workspace and keeps only the intersection, so each leg names one anchor workspace.
- A cancelled pull-request run's gate fails on purpose; rerun it rather than treating it as a regression.
- A CodeSpace worktree's Codex submodule can be filled from the main checkout without network access: `git -c protocol.file.allow=always -c "submodule.third_party/codex.url=file://<checkout>/.git/modules/third_party/codex" submodule update --depth 1`.
- In DevGuard, `spawn_guard` is a standard `Mutex` that both `helper_command()` and `HelperCommand::spawn()` take themselves. Locking it again around those calls does not return normally.

## 8. Where the evidence is

- The PR bodies of CodeSpace #67–#72 and DevGuard #8 hold the commands, run links and per-job measurements.
- The appendix of [design revision 1](../design-revision-1.md) lists the file and line evidence for the execution-ownership findings.
- `evidence/cs-rg/plan-review.md`, git-ignored and local only, holds the pre-revision review F1–F11.
