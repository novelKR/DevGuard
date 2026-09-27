# Session handoff: close of the 2026-09-27 session

> **Status: dated record for the next session; not a design decision.** It approves no architecture, starts no
> work unit, changes no ledger status and authorizes no implementation or merge. The CS-RG implementation hold
> stays in force.

This document closes one working session so that another agent or a reviewer can continue it without access to
that session. It is a snapshot dated 2026-09-27. It is not maintained afterwards and it is not an authoritative
planning document. The living trackers are [issue #14](https://github.com/novelKR/DevGuard/issues/14) and its
CodeSpace counterpart [novelKR/CodeSpace#76](https://github.com/novelKR/CodeSpace/issues/76); re-query GitHub and
git before acting. It builds on the two earlier handoffs of the same day,
[the design revision 1 handoff](2026-09-27-cs-rg-revision-and-codespace-ci.md) and
[the delivery follow-up](2026-09-27-cs-rg-delivery-followup.md), and on the revalidation record of
[#13](https://github.com/novelKR/DevGuard/pull/13). English only, with no Korean counterpart; personal information
is left out.

Evidence levels: *record* (a PR body, CI run or document), *source* (code or a document read at a fixed revision),
*test* (a test or CI result), *inference*, and *not run*.

## 1. Picking up the work

### 1.1 Order
1. Open [#14](https://github.com/novelKR/DevGuard/issues/14) and read its latest comments. The tracker can be newer
   than this file.
2. Re-query the state and compare it with section 3:
   - `git fetch origin` in both repositories, then `git rev-parse origin/main`;
   - `gh pr view <n> --repo <owner/repo> --json state,headRefOid,baseRefOid,mergeStateStatus,statusCheckRollup` for
     every PR in section 3.2;
   - `git ls-tree origin/main third_party/codex` in CodeSpace;
   - `gh run list --repo novelKR/CodeSpace --event schedule --limit 3`.

   Post any difference on #14 before acting.
3. Read section 2 (the governing instruction) and section 4 (terms) of this file.
4. Read #13's [handoff](https://github.com/novelKR/DevGuard/blob/1af1921fb72bb969b4f19ff859bd59fdb361ca55/docs/handoff/2026-09-27-cs-rg-boundary-revalidation.md)
   and [analysis](https://github.com/novelKR/DevGuard/blob/1af1921fb72bb969b4f19ff859bd59fdb361ca55/docs/handoff/2026-09-27-cs-rg-boundary-revalidation-analysis.md).
   Once #13 has merged, the same files are in this directory.
5. Read [novelKR/CodeSpace#76](https://github.com/novelKR/CodeSpace/issues/76) and the CodeSpace note
   [`.github/notes/pr75-cs-rg-suspension-handoff.md`](https://github.com/novelKR/CodeSpace/blob/codex/cs-rg-suspension-handoff/.github/notes/pr75-cs-rg-suspension-handoff.md)
   for the CodeSpace-side constraints and checks.
6. Look for a new owner instruction on #14, #76 or the PRs. A merge approval names the PR and its full head SHA.
   Without one, only the actions in section 2.3 under "allowed now" are open.
7. Follow the procedures in section 8. Record every state change on #14, and on #76 when CodeSpace is affected.

### 1.2 Where each kind of information lives
| Need | DevGuard | CodeSpace |
| --- | --- | --- |
| Current state, open approvals, checklists | [#14](https://github.com/novelKR/DevGuard/issues/14) (living) | [#76](https://github.com/novelKR/CodeSpace/issues/76) (living) |
| Dated snapshot of this session | this file | the note named in step 5 |
| CS-RG reasoning, candidates and evidence levels | #13 analysis | refers to DevGuard |
| Hold notices on the CS-RG documents | [#12](https://github.com/novelKR/DevGuard/pull/12) | [#75](https://github.com/novelKR/CodeSpace/pull/75) |
| Independent test fix | [#11](https://github.com/novelKR/DevGuard/pull/11) | — |
| Raw evidence | `<DEVGUARD_CHECKOUT>/evidence/`, local and git-ignored (section 7) | kept in the same DevGuard location |

## 2. Governing instruction

### 2.1 The directive
- **Source.** The owner's directive CS-DG-REASSESS-2026-09-27, issued at 10:46Z and revised by the owner at 11:10Z
  and 11:34Z. The owner's reference review of the first decision packet then led to the hardening recorded in #13.
  The directive document is kept locally (section 7, `source/`). What follows is a summary.
- **Hold.** CSRG-C00 (managed PTY through `HelperCommand`), CSRG-C03 (a runner-owned transport outside the Codex
  adapter) and CSRG-C09 (legacy-backend convergence) are suspended as implementation directives, and no
  replacement is approved. CS-RG gets no CodeSpace-owned PTY or process backend. Until the hold PRs merge, nobody
  implements from the old text.
- **Constraints.** N1 (CodeSpace) and N2 (DevGuard safety) are joint hard constraints with no priority between
  them (section 4.1). A combination that cannot meet both is unsupported, and neither is weakened.
- **Evidence roles.** Current code is evidence (E1), not a constraint. The CS-RG planning documents are audit
  targets (E2), not requirement sources.
- **Investigation order.** For each gap the order is: the existing path; an existing public primitive in current or
  newer Codex; a DevGuard contract adjusted to it; a minimal generic Codex extension; unsupported. This is an order
  of investigation, not "first feasible wins". Every surviving candidate stays in the comparison. "Unsupported" is
  narrow (per platform, transport and capability) and is a normal outcome.

### 2.2 Gates and the stop point
| Gate | Meaning (directive) | State at close |
| --- | --- | --- |
| G0 | hold propagated, state captured, evidence protected | done; one lapse corrected afterwards (FL-12, section 6.2) |
| G1 | requirements, findings and contrary evidence recorded | done (local `requirements.md`, `findings.md`; public summary in section 4 and the analysis) |
| G2 | fixed-version API matrix, ownership map, DevGuard mechanism map, alternatives | done (local; public summary in sections 4.2 to 4.4 and the analysis) |
| G3 | bounded diagnostics complete or explicitly not run; recommendation and fallback justified | record submitted. Candidate contracts are **not** verified, and product support is **not** established (section 7.1) |
| G4 | owner approves the boundary and any exception; corrective documentation reviewed and merged | not reached |
| G5 | only then an explicitly authorized replacement C00 or successor implementation | not started |

The review stopped at the hardened decision packet. A normative design or contract PR comes only after the
owner's decisions in section 5.2, and product work only after that PR merges and a separate approval.

### 2.3 Boundaries
- **Allowed now:** read-only research; evidence preservation; local records; documentation commits, pushes, PRs
  and issues that record facts; bounded diagnostics whose protocol is written first. No diagnostic is approved to
  run yet.
- **Separate explicit approval:** every merge, tied to its exact head; implementation or prototypes; a normative
  design or contract PR; a Codex pin change; a product dependency; a wire or MCP change; consumer provisioning;
  service restarts; credentials; journal or release replacement; rulesets; force-push; destructive cleanup; an
  external upstream PR.
- **Public records** carry no local absolute paths; use `<DEVGUARD_CHECKOUT>/...` and `<CODESPACE_CHECKOUT>/...`.

### 2.4 Identifiers
Design provenance is the PR #8 merge `d4981b4c241cff42687f5c2c681b583c7847776e`. The DevGuard source a unit builds on
is recorded separately; at close it was `7e3cbda91308f527d6cc34fba908375e6332bc58`. The qualified release
`0.1.0-5daee5d-b3fa569e` is a release ID, not a source commit.

## 3. State at close (re-queried 2026-09-27 15:05 UTC)

### 3.1 Repositories and checkouts
| Item | Value |
| --- | --- |
| DevGuard `main` | `7e3cbda91308f527d6cc34fba908375e6332bc58` = `origin/main`; push CI run 36306089707 success |
| CodeSpace `main` | `794867ef52f530be6bc0d91aa10416d5195367b7` = `origin/main`; CI run 36308518615 and documentation run 36308519199 success |
| Codex pin | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (rust-v0.154.0), unchanged |
| DevGuard worktrees | `.local/worktrees/dg-stuck-probe` (#11), `cs-rg-hold` (#12), `cs-rg-handoff` (#13), `session-handoff` (this record) |
| CodeSpace worktrees | `.local/worktrees/cs-rg-hold` (#75), `cs-rg-suspension-handoff` (the CodeSpace note) |
| Kept branches | every remote task branch. CodeSpace keeps `codex/ci-fixture-docs-only` and `codex/ci-fixture-pty-only` (closed #71 and #72) and `codex/diag-etxtbsy-fixture` (diagnostic run 36302744023 only, never to be merged) |
| Untracked | `.kiro/` in the persistent DevGuard checkout (tool settings, kept) |

### 3.2 Pull requests
| Repository | PR | Purpose | Head | CI on that head | State |
| --- | --- | --- | --- | --- | --- |
| DevGuard | [#12](https://github.com/novelKR/DevGuard/pull/12) | hold notices on 21 English and Korean documents, `AGENTS.md` and the translation hash registry; additive only | `ae85ebb95b68821361ae59d3c60b5344a1d8ab03` | 4/4 success (runs 36317741152, 36317743945) | open, CLEAN |
| CodeSpace | [#75](https://github.com/novelKR/CodeSpace/pull/75) | counterpart hold notices on five documents and their Korean versions | `1bee230595698b0974df43561bccfce67d7e8cb9` | 5 success, 6 not selected by the CI plan (runs 36317763003, 36317763516) | open, CLEAN |
| DevGuard | [#11](https://github.com/novelKR/DevGuard/pull/11) | test-only fix of a false failure (section 6.1) | `2bbe7c5ed88ad3170bc76985bdecb1fd7434d501` | 4/4 success (runs 36311129096, 36311154406) | open, CLEAN |
| DevGuard | [#13](https://github.com/novelKR/DevGuard/pull/13) | revalidation record and hardened analysis | `1af1921fb72bb969b4f19ff859bd59fdb361ca55` | 4/4 success (runs 36323946292, 36323949351) | open, CLEAN |
| DevGuard | this record | session-close handoff | see the PR | see the PR | open |
| CodeSpace | the note's PR | CodeSpace session-close note | see [#76](https://github.com/novelKR/CodeSpace/issues/76) | see the PR | open |

All share their repository's `main` as base, and no base has moved since review. The DevGuard PRs touch disjoint
files, as do the two CodeSpace PRs.

### 3.3 What the session did
| Time (UTC) | Work | Result | Evidence |
| --- | --- | --- | --- |
| 08:24 | DevGuard #10 (DGP-D07) merged | `7e3cbda`; main CI passed | run 36306089707, preserved |
| 08:59, 09:10 | CodeSpace #74 (ETXTBSY fixture race) and #73 (CSP-D04) merged | `701e2b1`, `794867e`; CI and documentation publication passed | runs 36307948862, 36307949175, 36308518615, 36308519199, preserved |
| 10:00 | #11 opened for the false failure of run 36303237592 | reviewed; body corrected twice; head unchanged | PR body; local review records |
| 10:05 | a read-only C00 harness plan was delegated under the instruction then current | ran late, after the hold (FL-12) | local `csrg-c00/` |
| 10:46 | directive CS-DG-REASSESS-2026-09-27 | hold and boundary revalidation | local `source/` |
| 12:03-12:04 | stage 1: hold PRs #12 and #75 opened | CI green on both | PRs; local `stage1/` |
| by 12:46 | stages 2-6: requirements, ownership map, upstream matrix, DevGuard mechanism map, gap resolution | local records (file times 12:18Z to 12:46Z) | section 7 |
| about 12:50 | stage 7: decision packet; review stopped | first packet, later superseded | local `decision-packet.md` |
| 12:52-12:58 | the delayed delegated run completed; records corrected (FL-12) | no repository or product effect | local `intake.md`, `findings.md`, `csrg-c00/` |
| 13:13 | #13 opened to put the results on GitHub | public record | #13 |
| about 13:55 | hardened after the owner's reference review | #13 head `1af1921`; no route presented as supportable | #13 analysis |
| 15:01 | session-close request | #14, [novelKR/CodeSpace#76](https://github.com/novelKR/CodeSpace/issues/76), this record, the CodeSpace note | this file |

## 4. Terms used in the records
Definitions from the local requirements ledger, the gap resolution and the upstream matrix. The analysis in #13
uses them without restating them.

### 4.1 Normative constraints
| ID | Meaning |
| --- | --- |
| T0 | the owner's requirements |
| N1.1 | product authority stays in CodeSpace: MCP semantics, workspace authorization, approvals, process identity, logical process control, output and result semantics, Gateway and Runner behaviour |
| N1.2 | selective upstream delegation: a mechanism delegated to Codex (notably PTY through `codex-utils-pty`) is not re-owned by CodeSpace because of DevGuard; following Codex upstream (a pin update or a general upstream API) is the normal answer to a missing primitive |
| N1.3 | unaffected paths are not rewritten for DevGuard or for symmetry (the pipe path uses Tokio, not Codex) |
| N1.4 | opt-in orthogonality: with governance `off`, CodeSpace needs no DevGuard service or credential, and its execution architecture is structurally unchanged by the integration |
| N1.5 | independent evolution: a CodeSpace change unrelated to DevGuard stays possible through its own proposal and approval, and is never justified by CS-RG |
| N2.1 | no workload starts without authorization bound to its owner and attempt |
| N2.2 | no automatic replay of an uncertain execution |
| N2.3 | no lease release without termination evidence |
| N2.4 | no action on a stale or reused process identity |
| N2.5 | credential confinement: no secret reaches the payload, logs, MCP or an unrelated child |
| N2.6 | durable accounting before responses; attempt identity and terminal tombstones preserved |
| N2.7 | honest evidence and capability: OS evidence only through the Backend boundary; reservation, planned policy, applied evidence and startup stay distinct; no backend claims a capability it lacks |
| E1 | implementation evidence: current and pre-CS-RG facts, recorded without turning them into constraints |
| E2 | documents under audit: design revision 1, the CS-RG units C00, C03 and C09, and the related CSP-D04 text |

The thin CodeSpace adapter may do: admission RPC, attempt identity binding, resource request, capability and
version negotiation, error and result translation, execution metadata, evidence and outcome submission, the
`resources=off|required` setting, and consuming generic backend events. It may not do: PTY allocation, native spawn,
a new reaper, terminal, session or process-group setup, Codex PTY replacement, an `off`-path semantics change, or a
generic lifecycle redesign. A new event added for CS-RG must be generic to CodeSpace's execution model, and
consuming an event never justifies taking over spawn, PTY, reaping or lifecycle to create it. Open interpretation:
a generic capability added to an existing backend (for example "pass these extra inherited descriptors to the
child") is neither an event nor an ownership transfer; each such case is judged against N1.2, N1.3 and the
generic-only rule.

The owner also clarified that the safety goals are abstract: helper topology, descriptor handoff, the observation
protocol, registration and callback placement may change if the goals still hold. The current mechanisms
(`HelperCommand`, the permit and transcript descriptors, observe-before-reap, `spawn_guard`, the frame deadlines)
are therefore candidates, not constraints.

### 4.2 Directions excluded before comparison
| ID | Direction | Violates |
| --- | --- | --- |
| E-1 | CodeSpace allocates the PTY and DevGuard's `HelperCommand` spawns the helper (design revision 1's A1) | N1.2 |
| E-2 | `off` on Codex, `required` on a CodeSpace-owned transport | T0, N1.2, N1.4 |
| E-3 | converge the `off` backends onto a new backend (the old C09) | N1.4, N1.2 |
| E-4 | CodeSpace holds DevGuard's `spawn_guard` around its own spawns | N1.3 (needs an explicit owner exception) |
| E-5 | make grant descriptors inheritable in the parent without protection | N2.5 |
| E-6 | deliver the permit through the terminal or stdio stream | N2.5, N1.1 |
| E-7 | CodeSpace builds a Codex `ProcessDriver` for governed PTY | N1.2, N1.1 |
| E-8 | remove the permit without an equal identity proof | N2.1 |
| E-9 | release a lease on root reap without observing the scope | N2.3 |
| E-10 | a supervising helper for pipe with CodeSpace's terminate unchanged | N1.1 |

### 4.3 Codex capability IDs
Pin rust-v0.154.0, stable rust-v0.157.1 (`36650394c5b38c2990ccf2a3457165ca3e9d9726`) and `main`
`41f9084b30812db321a0b592def4f500d1e79cf4` (looked up at 12:27Z).

| ID | Capability | Pin | Stable | Main |
| --- | --- | --- | --- | --- |
| K1 | launch a caller-chosen program on a PTY with size, resize, `setsid` and a controlling terminal | yes | yes | yes |
| K2 | deliver close-on-exec descriptors only to the intended child | no (listed descriptors must already be inheritable) | no | yes (`ChildFds::Attached`, openai/codex#47797, 2026-09-24) |
| K3 | exit observation before reap, or owner-controlled reap, for a PTY child | no | no | no |
| K3a | a caller-owned child wrapped by Codex I/O (`ProcessDriver`) | partial | partial | partial (its bridge drops lagged output) |
| K3b | a caller-owned generic child with an explicit `wait` | no | yes | yes |
| K4 | identity-safe termination (never signal a stale numeric pid or pgid) | no | no | no |
| K5 | a new session or process group for group-level signals | yes | yes | yes |
| K6 | exclude unrelated descriptors from the child | yes for PTY children | yes | yes |
| K7 | output transport with backpressure and honest loss accounting | yes for the PTY path | yes, same design (not re-read line by line) | yes for the PTY path (not re-read) |
| K8 | exit detail for PTY children (signal versus code) | code only | code only | code only |
| K9 | a Linux spawn helper for fork-safe launches | no | no | yes (Linux only) |

Neither the pin nor `main` exposes a PTY child PID (analysis F-8); stable was not checked separately for this.

### 4.4 CodeSpace execution paths
P1 pipe spawn (Tokio, `env_clear`, piped stdio, `kill_on_drop`, no process group); P2 PTY spawn through Codex
`spawn_pty_process`; P3 pipe exit observation (`try_wait` every 20 ms); P4 PTY exit observation (Codex
`spawn_blocking(child.wait())`); P5 timeout; P6 terminate and workspace termination; P7 completed-slot eviction
after 15 minutes; P8 normal shutdown; P9 post-spawn live-limit rejection; P10 patch helper; P11 Linux sandbox probe
and prepare; P12 Linux sandbox run. W1 to W5 are the UDS worker paths: spawn, exit watch, shutdown and disconnect,
setup failure, and `connect_existing`.

### 4.5 Labels used in the analysis
R (governed pipe first), X (a Codex PTY child PID accessor), Y (`ChildFds::Attached`), RA (a supervising helper),
RB (owner-confirmed identity instead of a permit), RC (results through the authority), RE (accept reap-first), RG
(a group anchor), D6 (the client session-socket window), G-RB1 to G-RB4 (gaps of RB), RR-1 to RR-15 (points of the
owner's review) and F-1 to F-11 (corrected findings) are defined in the analysis, sections 1 to 11.

Local finding IDs map to the public findings like this:
- FL-01 to FL-12 (how the drift entered the plan, and the agent's own role) are summarized as F-1 and F-2.
- UR-01 is F-5.
- UR-02, as corrected in stage 3, is K2 at the pin and E-5.
- UR-03 (the pipe path reaps at several sites) is P3, P5 and P6.
- UR-04 and UR-07 are F-7 and D6.
- UR-05 and UR-06 are OX-02 and OX-03.
- OX-01 to OX-03 are F-11.

## 5. Open decisions

### 5.1 Merge track
Each merge needs the owner's approval naming the exact head (section 8.1). Suggested order: #12 and CodeSpace #75
first, because until they merge the CS-RG documents on `main` still read as directives; the others are independent.

| PR | Head | What merging does |
| --- | --- | --- |
| #12 | `ae85ebb95b68821361ae59d3c60b5344a1d8ab03` | puts the hold notices on `main`; deletes or rewords nothing |
| CodeSpace #75 | `1bee230595698b0974df43561bccfce67d7e8cb9` | the same in CodeSpace; publishes the notices on the documentation site |
| #11 | `2bbe7c5ed88ad3170bc76985bdecb1fd7434d501` | test-only fix; no product change |
| #13 | `1af1921fb72bb969b4f19ff859bd59fdb361ca55` | adds the revalidation record and analysis; approves nothing |
| this record | see the PR | adds this file; approves nothing |
| the CodeSpace note | see the PR | adds the CodeSpace note; approves nothing |

### 5.2 Architecture track
No route (R, X or Y) is contract-complete, so governed execution is unsupported under the current constraints. The
owner has four open choices; the analysis, section 13, gives the reasoning.
1. **D6 direction:** a DevGuard-side reduction of the window plus an independent CodeSpace descriptor-hygiene
   proposal under N1.5, or exposure evidence first and a decision afterwards.
2. **Reap-first operability:** a bounded RG experiment (DevGuard fixture, isolated worktree, not committed), a
   generic upstream reap-control primitive, or governed execution only for a restricted, documented workload
   class after frequency evidence.
3. **Upstream engagement:** approval to submit generic Codex proposals (a PTY child PID accessor, owner-controlled
   reap). Drafting needs no approval.
4. **UDS mode:** defer it (InProcess first) or choose a worker credential path now.

Without a decision nothing else is blocked; governed execution simply stays unsupported.

### 5.3 Settled; do not ask again
- N1 and N2 are joint hard constraints.
- DevGuard is opt-in and is the side that adapts.
- CS-RG alone never justifies CodeSpace taking over a Codex-delegated mechanism.
- The thin adapter's scope is fixed (section 4.1).
- The directions in section 4.2 are not candidates.
- The review stops before normative design and implementation.
- Choosing between X and Y is premature while neither is contract-complete.

## 6. Known flaky tests and unresolved issues

### 6.1 Intermittent CI failures
| Test or step | Where | Cause and level | Status |
| --- | --- | --- | --- |
| `server::tests::native::a_stuck_probe_does_not_hold_the_authority_and_its_delay_closes_admission` (DG1-C03) | DevGuard run 36303237592, macOS, "Native host evidence functional checks"; 1 failure in 33 macOS attempts | the test measured from the probe's read time instead of the sampler's `sample.at`; mechanism shown by an instrumented reproduction (test); the hosted runner's gap was not reproduced | fix in #11, open |
| survivor race in `crates/launch/tests/reconcile.rs` (DG1-C06) | DevGuard run 36260660317, attempt 1, macOS | test read order; reproduced (test) | fixed by #9 (`30b5fa6`); main run 36294359318 passed |
| `linux_sandbox::tests::prepare_unread_large_stdin_times_out` (Rust / Integration) | CodeSpace scheduled run 36275459398 on `b6e7ed2` | **suspected**: ETXTBSY when a concurrently forked test child holds a write handle to a freshly written fixture script. The mechanism was shown directly (42 of 1600 starts failed with an in-process writer and none with a child writer), but the failing run's holder was not captured | fix in CodeSpace #74 (`701e2b1`); **not confirmed**: no scheduled run on `794867e` has been read yet |
| `hashFiles('**/Cargo.lock')` Actions template error | CodeSpace runs 36218741807 (attempt 1) and 35628255763 | the directory walk failed intermittently (record) | fixed by CodeSpace #67 (`339ae8e`, explicit lockfile list) |

### 6.2 Unresolved issues
| Issue | Level | Owner of the next step |
| --- | --- | --- |
| D6: DevGuard's client session socket is inheritable between `socket()` and `F_DUPFD_CLOEXEC` (connect.rs L39-L51 at `7e3cbda`), outside `spawn_guard`. It is a prerequisite of every CS-RG route and a defect of the current design. Exposure is not demonstrated; Linux could use `SOCK_CLOEXEC` | source; exposure not run | owner decision 5.2-1 |
| Reap-first leaves a charged, sticky Suspect attempt until reboot on macOS when a survivor was not adopted before the reap | source and test (analysis section 3); frequency not run | owner decision 5.2-2 |
| OX-01 to OX-03, CodeSpace-only (see [#76](https://github.com/novelKR/CodeSpace/issues/76)) | source; runtime not run | separate CodeSpace proposals |
| Rust's standard pipe and socket-pair creation on macOS is believed non-atomic, which would let concurrent CodeSpace spawns leak each other's stdio pipes | not read (unverified) | a later review |
| Codex `main` routes Linux PTY launches through a setup helper, which may break DevGuard's direct-child check | inference | DG-LINUX |
| UDS mode needs a worker credential path, and a killed worker leaves Suspect attempts | source | owner decision 5.2-4 |
| #12 and CodeSpace #75 are not merged, so `main` still reads as directive text | record | merge track |
| Evidence gaps: PR #9's original instrumentation logs were lost before preservation; #11's per-case invocation records are incomplete (its body says so) | record | none; keep the statements |
| FL-12: a delegated read-only run whose status read `not found` was waiting in a queue, started after the hold and wrote a superseded C00 plan. No repository or product effect. It is material of the suspended path, not a mandate | record | none; see section 8.4 |

## 7. Local-only evidence
Evidence stays out of commits by repository rule. It lives in `<DEVGUARD_CHECKOUT>/evidence/`, and every directory
has a `MANIFEST.json` with SHA-256 digests and sizes. At 15:10Z every manifest in the table except the last row (17
manifests, 2,368 entries; a file listed by both a directory and its parent counts twice) was re-hashed against its
files: no mismatch, no missing file, and no credential-pattern hit.

| Directory under `evidence/` | Content | `MANIFEST.json` SHA-256 |
| --- | --- | --- |
| `cs-rg/boundary-revalidation-2026-09-27/` | intake, plan, requirements, findings, ownership map, upstream matrix, DevGuard mechanism map, gap resolution, decision packet, local handoff (90 files) | `b377b3c5ddb854738a160bfd14a73d1eded8e53b2424ce45b4f92233f75e8de1` |
| `.../source/` | the owner's directive (English, as received) | `36c6b1e00781808627f3689af7e3b7f6cc9d51812f4f32c2e8526137edd587de` |
| `.../stage1/` | hold PR bodies and the notice tool | `309b210c1cf87b795797236355dde8ad97ee776cfb2bbaa339e3d618ccd1a2aa` |
| `.../stage4/` | 62 Codex source snapshots (pin, stable, `main`) | `b58a308fbbb2243680847c2c350f8d24323513a5128206801cdeb76ad1842cc4` |
| `.../handoff-pr/` | #13 bodies and the before and after copies of the #12 and #75 bodies | `d675d3c5f7d232d53845ad9676ff03495367911ade2b7489de54fac7b34c8156` |
| `codespace-delivery/csrg-c00/` | the C00 pre-investigation at `b6e7ed2` and the late C00 plan (suspended-path material) | `c1690398c85ff02b1178c51dc6d9613a155171835055f54f49c37e546828ea2b` |
| `dg1-delivery/stuck-probe-sample-time/` | #11 experiments (78 files) | `138dc8b513096b80802b6fc54d790dac92eec9475429e2a34b2e9096b68e51a0` |
| `.../review-2026-09-27/` | #11 review and body copies | `adf8a2adb63380777dee5fb7fa487098d4efa03d53a4b9a5e0b18ad996222200` |
| `.../ci-push-36311129096-2bbe7c5/` | #11 push CI, including the native reconcile log cited for reap-first | `9f9a8e86edf1579041c0250a6d1ce40edc4aee674834a519457dcb13d7e576bc` |
| `.../ci-pull_request-36311154406-2bbe7c5/` | #11 pull_request CI | `debf3e5eca76ef3c2c7d622a82d43c33c0ded89a75aaaa445861191a9c1b0e04` |
| `dg1-delivery/pr9-reconcile-survivor-race/` | the DG1-C06 flake, PR #9 runs and a new reproduction | `0aabae4c85a22590922a00d868450469f9ea723b497e7b92ca2161772dfd8754` |
| `dg1-delivery/dgp-d07-delivery-conditions/main-ci-7e3cbda/` | post-merge main CI of #10 | `6d8970425323d6acc0a7e62b0fd05c29deaca137eb0eddae4b9d269228863d1c` |
| `codespace-ci/schedule-36275459398-b6e7ed2/` | the scheduled CodeSpace failure | `2d131d292966f39b4841f0aea80eb0a88996dca9f358a719658e1a696ed5baf1` |
| `codespace-delivery/etxtbsy/main-ci-701e2b1/`, `.../main-docs-701e2b1/` | post-merge CI and documentation of CodeSpace #74 | `7589d73e378ccb69eabee74cc8e374ef880b2f07ea3dae12430179f0d60d33df`, `dac41d2d1287f872d2e169d1f24be58083a5b6eac1cc5814df47da42838b8b96` |
| `codespace-delivery/csp-d04/main-ci-794867e/`, `.../main-docs-794867e/` | post-merge CI and documentation of CodeSpace #73 | `dcae029356bbde0c3add87028bf64c9c70bcb01c881c064ab97ba6c455d720ac`, `e4dcadd258ae17060dcf431a2ff1ed97368c244310dffc708a9b8373ac433777` |
| `session-close-2026-09-27/` | the tracker and PR bodies written at close, their templates and renderer | its own manifest |

Not yet preserved: the CI of the open PRs #12, #13 and CodeSpace #75. Preserve it when each PR is delivered
(section 8.3).

### 7.1 Bounded diagnostics (protocols recorded, none run)
| ID | Question | Protocol and blocker |
| --- | --- | --- |
| BD-1 | does Codex `main`'s `Attached` path deliver close-on-exec descriptors at their numbers, exactly once, under concurrent spawns and on error paths? | a scratch crate outside both repositories spawns a PTY child with two attached descriptors while another thread forks; it checks descriptor numbers and closure at exec. Blocked: building the Codex `main` workspace was not safe with the host's free memory at the time |
| BD-2 | reap-first leak | the **existence** of the sequence is established by existing native tests (analysis section 3). The **frequency** in real agent workloads was not measured: it needs a governed prototype, which is not authorized |
| BD-3 | does a supervising helper preserve exit status, signals, job control and the process-group pin? | a prototype of a DevGuard helper change; premature before the owner chooses a direction |
| RG | can a group anchor let the authority adopt survivors after the reap? | a DevGuard fixture in an isolated worktree, not committed; it needs the owner's choice 5.2-2 |

## 8. Procedures

### 8.1 Merging
1. Get the owner's approval naming the PR and its full head SHA.
2. Right before merging, re-check head and base, and re-ask if the head changed.
3. Merge with `gh pr merge <n> --repo <owner/repo> --merge --match-head-commit <sha>`. Never use `--admin`, `--auto`
   or `--delete-branch`.
4. Check the merged state, the merge commit's parents and its tree.
5. Read the post-merge `main` CI once it finishes, and for CodeSpace also the documentation publication.
6. Preserve the evidence (section 8.3).
7. Fast-forward local `main`, remove only that PR's worktree, and `git branch -d` its merged local branch. Remote
   branches are kept.

Merging one PR advances `main` but does not change another PR's head. The owner's rule re-verifies a PR whose base
moved, and a head updated with the new base (merge or rebase) needs a new approval; that is a project rule, not a
Git or GitHub necessity. A green CI run does not replace review of a changed head.

### 8.2 Reading CI
Read CI once per state transition, not in a polling loop. For a failure:
1. Read the failed step's log.
2. Classify the cause as the PR's change, a known flaky recurrence (section 6.1), or the environment, using the same
   job's history and the other OS's result.
3. Propose a fix. Apply, commit, push or rerun only after the owner confirms.

Fix a flaky test by removing the race while keeping the test's intent. Do not add retries or longer timeouts, and
verify with repeated local runs.

### 8.3 Preserving evidence
Use `python3 evidence/tools/collect_ci_evidence.py collect <owner/repo> <run id> <destination> [attempt]` for each
run, then `python3 evidence/tools/collect_ci_evidence.py manifest <directory> '<json metadata>'`. Evidence is
preserved before any cleanup. Removing temporary instrumentation never deletes its evidence, and a regenerated log
is a new record, not a restoration.

### 8.4 Delegated work
- Give each delegated task the instruction revision it was issued under. When it starts, and again before its
  result is accepted, check that the revision is still current.
- A status of `not found` is indeterminate: it does not show that a run completed, was cancelled, is queued or is
  gone.
- Sub-agent starts can be deferred while host memory is low, so delegated work can start long after it was
  requested.
- Put inputs meant for another agent under a git-ignored path of its task worktree, not in a private scratch
  directory.

### 8.5 Environment
- DevGuard needs Rust 1.95.0 (a local toolchain under `.local/toolchains/1.95.0/bin` is used first). Run Cargo with
  `CARGO_BUILD_JOBS=1`, `RUST_TEST_THREADS=1` and `--locked`, and run one Cargo job at a time when memory is tight.
- Documentation checks: `python3 scripts/check_docs.py` (DevGuard) and `python3 -B scripts/check_docs.py`
  (CodeSpace). The CodeSpace documentation site needs exactly Node 24.21.0.
- In CodeSpace, a change under `.github/**` triggers a full CI run by policy (`scripts/ci-policy.json`), and a
  `docs/**` change runs no Rust leg.
- In agent sandboxes, DevGuard's SQLite-based script tests and CodeSpace's script tests need `TMPDIR` inside the
  worktree.

## 9. Pitfalls met
- The agent session restarted once without its conversation memory. The state was rebuilt from git, the PRs, the
  local evidence and a working note. Keep every decision and state change in a durable place, such as the trackers.
- A first decision packet overstated its conclusions ("pipe needs nothing else"). The owner's review corrected
  this, and #13 records the corrections. Keep candidate, proven property and product support separate.
- "Not released" is not "operable": check whether a conservative state can ever be left without a reboot.
- CodeSpace's site documentation avoids past-session narrative and unexplained work-package numbers, which is why
  CodeSpace's handoff lives under `.github/notes/`.
