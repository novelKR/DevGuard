# Session handoff: CS-RG integration-boundary revalidation (2026-09-27)

> **Status: record for the owner's decision; not a design decision.** It approves no architecture, starts no work
> unit, changes no ledger status and authorizes no implementation. CS-RG implementation stays suspended:
> CSRG-C00, CSRG-C03 and CSRG-C09 and the CodeSpace-owned managed execution transport they assume are not
> implementation directives.
>
> Review status: the review record is submitted; candidate contracts are **not** verified; product support is
> **not** established. The detailed, reviewable reasoning is in the
> [hardened candidate analysis](2026-09-27-cs-rg-boundary-revalidation-analysis.md).

## 1. Why this review happened
The owner's directive CS-DG-REASSESS-2026-09-27 and the owner's clarifications of the same day placed the
managed-PTY C00 path on hold and asked for a boundary review before any CS-RG implementation. "CS-RG
integration-boundary revalidation" names that review; it is not a work unit.

Fixed by the owner (not open for re-decision):
- CodeSpace constraints (N1: product semantics, responsibility boundaries, selective use of Codex upstream for
  execution mechanisms, opt-in orthogonality) and DevGuard safety invariants (N2) are joint hard constraints with
  no priority between them. A combination that cannot meet both is unsupported; neither is weakened.
- DevGuard is the opt-in that adapts. CS-RG alone never justifies moving a Codex-delegated mechanism (notably PTY)
  into CodeSpace or refactoring unaffected CodeSpace paths. CodeSpace changes stay a thin adapter.
- Current code is evidence, not a constraint. The CS-RG planning documents are audit targets.
- The review stops before normative design and implementation.

## 2. State at a glance (2026-09-27 13:45 UTC)
| Item | Value |
| --- | --- |
| DevGuard `main` | `7e3cbda91308f527d6cc34fba908375e6332bc58` |
| CodeSpace `main` | `794867ef52f530be6bc0d91aa10416d5195367b7` |
| Codex pin in CodeSpace | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (rust-v0.154.0), unchanged |
| Hold notices | [DevGuard #12](https://github.com/novelKR/DevGuard/pull/12) (head `ae85ebb95b68821361ae59d3c60b5344a1d8ab03`), [CodeSpace #75](https://github.com/novelKR/CodeSpace/pull/75) (head `1bee230595698b0974df43561bccfce67d7e8cb9`) |
| Independent test fix | [DevGuard #11](https://github.com/novelKR/DevGuard/pull/11) (head `2bbe7c5ed88ad3170bc76985bdecb1fd7434d501`) |
| This record | [DevGuard #13](https://github.com/novelKR/DevGuard/pull/13); its head changes with each revision |
| Product code, pins, dependencies, services, credentials | unchanged |

All four PRs are open and none is merged. Until #12 and #75 merge, the CS-RG documents on `main` still read as
directives; no agent implements from them.

## 3. Main conclusions (evidence levels in the analysis)
- The priority inversion is documented, not executed: design revision 1 made CodeSpace's Codex use changeable and
  DevGuard's mechanisms fixed, and that premise shaped C00/C03/C09 and CodeSpace's reuse text. (source)
- The core mismatch is DevGuard's launch API: `HelperCommand` makes DevGuard's client perform the spawn. (source)
- Reap-first is safe against false release but not operable: when the backend reaps the root before the
  authority adopts a surviving descendant, the attempt stays Suspect with sticky tracking loss even after the
  descendant exits, and on macOS only a reboot releases it. Existing DevGuard native tests demonstrate this and
  passed on macOS CI. Both CodeSpace backends reap at once. (source + test)
- DevGuard's client session socket is inheritable for a short span before it becomes close-on-exec, outside
  `spawn_guard`; CodeSpace's spawns neither take that guard nor close descriptors in the child. This affects every
  candidate and the current design; exposure is not demonstrated. (source; exposure unresolved)
- Codex offers no pre-reap observation for PTY children in any checked revision; close-on-exec descriptor
  attachment exists only on main and in prereleases; no PTY child PID is exposed. (source)
- No proposed route (R, X, Y) is contract-complete. Governed execution is unsupported under the current
  constraints. (analysis sections 11-12)

## 4. What is open
Architecture track (the analysis, section 13, explains each):
1. Direction for the session-socket window: a DevGuard-side reduction plus an independent CodeSpace
   descriptor-hygiene proposal, or evidence on exposure first.
2. Direction for reap-first leaks: a bounded experiment of a DevGuard-side group anchor, a generic upstream
   reap-control primitive, or a restricted, documented workload class after frequency evidence.
3. Whether to submit generic upstream proposals (drafting is allowed without approval).
4. UDS mode: defer (InProcess first) or choose a credential path now.

Merge track (separate; each needs the owner's explicit approval of the exact head): #12, CodeSpace #75, #11 and
this record #13. See the analysis, section 13, for heads, bases and CI.

## 5. How to continue
- Confirm state first: both `main` heads, the Codex pin, and the heads and bases of the open PRs.
- Do not implement, prototype or plan a CodeSpace-owned PTY or process backend for CS-RG. Do not start a CS-RG
  unit. Do not open a normative design PR, change the Codex pin or dependencies, or submit upstream PRs before the
  owner decides.
- Merging: only with an explicit approval tied to the exact head. Immediately before merging, re-check head and
  base (the owner's merge rule). Merging one PR advances `main` but does not change another PR's head; this
  project nevertheless re-verifies a PR whose base moved, and an updated head needs a new approval. Merge with
  `--match-head-commit`, read the post-merge `main` CI once, preserve it, then fast-forward local `main` and clean
  up without deleting remote branches. A green CI run does not replace review of a changed head.
- Delegated work: carry the instruction revision with each delegated task; when it starts and before its result
  is accepted, check that the revision is still current. A status of `not found` is indeterminate, not proof of
  completion, cancellation, queueing or disappearance.
- Public records carry no local absolute paths.

## 6. Evidence
Raw evidence and full review records stay outside commits by repository rule; the analysis document carries the
reviewable reasoning. Locations (manifests list every file with its SHA-256):

| Record | Location | Manifest |
| --- | --- | --- |
| Review records, directive copy, upstream source snapshots | `<DEVGUARD_CHECKOUT>/evidence/cs-rg/boundary-revalidation-2026-09-27/` | `MANIFEST.json` there (regenerated as records are added) |
| Codex source snapshots (pin, rust-v0.157.1, main `41f9084`) | same, `stage4/` | `b58a308fbbb2243680847c2c350f8d24323513a5128206801cdeb76ad1842cc4` |
| Hold PR records | same, `stage1/` | `309b210c1cf87b795797236355dde8ad97ee776cfb2bbaa339e3d618ccd1a2aa` |
| DevGuard #11 experiments, review and exact-head CI | `<DEVGUARD_CHECKOUT>/evidence/dg1-delivery/stuck-probe-sample-time/` | `138dc8b513096b80802b6fc54d790dac92eec9475429e2a34b2e9096b68e51a0` (experiments) |
| Superseded C00 materials | `<DEVGUARD_CHECKOUT>/evidence/codespace-delivery/csrg-c00/` | `c1690398c85ff02b1178c51dc6d9613a155171835055f54f49c37e546828ea2b` |

Diagnostics: the reap-first sequence is covered by existing DevGuard native tests (analysis section 3). Not run:
Codex main `Attached` behaviour; frequency of reap-first leaks in real workloads; exposure through the session
socket window; a group-anchor or supervising-helper experiment.

## 7. Pitfalls met
- A delegated run's `not found` status is indeterminate; a hold must reach delayed work before its result is used.
- Korean counterparts are maintained documents with their own structure; match sections by content.
- CodeSpace documentation avoids unexplained work-package numbers.
- On macOS every descriptor created with `pipe()`, `socketpair()` or `socket()` is inheritable until it is
  duplicated or flagged; any descriptor design must account for concurrent forks in the same process.
- "Not released" is not "operable": check whether a conservative state can ever be left without a reboot.
