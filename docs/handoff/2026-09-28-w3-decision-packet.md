# W3 decision packet: experiments A, B and C (2026-09-28)

> **Status: dated, non-normative research and handoff record.**
> - Nothing here approves an architecture, dependency, Codex pin, runtime contract, implementation, upstream
>   submission, service change or merge.
> - The CS-RG implementation hold stays in force.
> - Every experiment ran in scratch or on a local branch that was never pushed. No product code, pin, dependency,
>   gate, service or credential changed.
> - It stops at the next owner decision (section 6).

This record reports W3 of CS-DG-UPSTREAM-ADAPTER-WORK-SPEC-1 v1.0, following the
[W0–W2 packet](2026-09-28-upstream-adapter-packet.md), called "the packet" below. Experiments A, B and C are the
requests of the packet's section 13. The living trackers are [#14](https://github.com/novelKR/DevGuard/issues/14) and
[novelKR/CodeSpace#76](https://github.com/novelKR/CodeSpace/issues/76). Re-query them and git before acting.

Evidence levels are as in the packet: *record*, *source*, *test*, *inference*, *not run*. Raw evidence is local and
git-ignored under `<DEVGUARD_CHECKOUT>/evidence/w3-2026-09-28/`; digests are in section 7.

## 1. Owner decisions executed

The owner gave these decisions on 2026-09-28 at 06:18Z, directly to the executing agent. The wording is kept in the
local record `w3-2026-09-28/authorization.md`. A comment written through the shared account is not independent proof
of approval.

**Merges.** Each was made at its exact approved head after an immediate head and base re-check. *(record)*

| PR | Approved head | Merge commit | Pre-merge check | Post-merge `main` |
| --- | --- | --- | --- | --- |
| #13 | `edf5e2f20f88feed55822a078abffc18af5f9a4e` | `8f425ecb8438fea20897976446fadc9e7e142436` | base moved from `7e3cbda` to `1bb085a`. Its PR run 36336399416 had already tested exactly that merge (`499a74e`), whose tree equals a local merge simulation | 36387864117, success |
| #16 | `a67facbbebb192d83126c5afa1a9ef2d713c2e2e` | `9e21cc8f707f16b7da98490293b5ea7e4548916d` | re-verified against the moved `main`: disjoint files. On the exact merge tree `check_docs.py`, the `test_check_docs.py` and `test_measure.py` unit tests and `git diff --check` passed | 36387939532, success |
| novelKR/CodeSpace#78 | `c8912abf50ee090bdc6eaa6054a0ab524210f276` | `ab0341b5cf5eda8c82730487d4475f1ac922daf5` | the note names the merged #16 head; CodeSpace `main` had not moved; gitlink `6b9826e` | CI 36388007427 (16/16) and documentation 36388008255, success |

- **Duplicate push event.** CodeSpace recorded a second push event on `main` with the same before and after
  (`326bdcb` → `ab0341b`) at 06:50Z, which started a second successful CI and documentation pair. The cause is
  undetermined; nothing was pushed by the executing agent.
- **Local cleanup.** Local `main` was fast-forwarded in both repositories. Only the merged PRs' worktrees and local
  branches were removed. Remote branches are kept.

**Housekeeping.**
- The W0–W2 PRs' final CI was recorded on the trackers.
- The completed runs were preserved in a post-seal evidence set.
- The independent audit's 20 itemized notes were reconstructed verbatim from the session transcript and marked as a
  reconstruction; no original file had been kept.
- The whole evidence directory, as it stood at 06:38Z before the merges and experiments, was backed up to a second
  local disk and verified file by file. It is not published. Later records are not in that copy.

**Also executed:**
- the experiments (sections 2–4);
- the CodeSpace descriptor-hygiene issue [novelKR/CodeSpace#79](https://github.com/novelKR/CodeSpace/issues/79),
  which is non-implementation and justified on CodeSpace's own terms.

**Not done, by instruction:**
- normative policy or design PRs;
- upstream submission;
- any pin, dependency, integration, service or credential change.

## 2. Method

- **Protocols.** All three were frozen with SHA-256 digests before any build (07:11:16Z). Changes after the freeze
  are dated amendments, each written before the runs it affects:
  - amendment 1 (the authority in its own process);
  - amendment 2 (carriers, after A's first attempt);
  - amendment 3 (B's survivor payload and fixtures, before any B build).

  Harness defects fixed after a run are logged separately. Failed and invalid attempts are kept.
- **Revisions.** Codex `72b8d8b` (rust-v0.159.0-alpha.11), unmodified for A and C. For B, a scratch copy of it with
  one generic observer patch. DevGuard at `9e21cc8`, plus the experimental preparation module of section 3.1, which
  all three experiments used: A for its launches, B for its helper launches, and C for its carrier creators. The runs
  used the module uncommitted; it was committed afterwards, unchanged, as the local commit of section 7.
- **Build and authority.** Toolchain 1.95.0; one Cargo job at a time. The authority was DevGuard's `TestAuthority`
  fixture in a private directory. The installed LaunchAgent was not touched: the same pid 90515 was running before
  and after.
- **Credentials.** Synthetic only. The receipt scans of every completed run found none of the held secrets. The
  first attempt of A ended in a harness panic before its scan ran, so its receipt log was not scanned.
- **Host.** Apple M1, 16 GiB, macOS 27.0 (26A428). Every count is "k/N observed on this host": behavioural evidence,
  not proof.

## 3. Results

### 3.1 Experiment A: preparation consumed by an existing backend (packet 13.1)

**Setup.** An experimental preparation module creates the permit carrier, the transcript carrier and the helper argv
and hands them to the caller's backend. It lives on a local branch that was never pushed; the helper, wire and
authority were unchanged. Backends:
- Codex's unmodified PTY `ChildFds::Attached`;
- a Tokio spawn with a child-side step, as a diagnostic stand-in for a pipe backend.

**First attempt.** On this macOS host, `poll` did not report EOF on a FIFO once its writers had closed and its data
had been read. So FIFO carriers cannot be read by the unchanged helper (`read_owned`) or by the owner's
`HelperReport::wait`. Every FIFO permit launch failed closed at the permit stage (exit 125), and a FIFO transcript
ended `Lost { ready: true }`. A regular file, a socket pair and a pipe all signal EOF. *(test; diagnostic kept)*

**Frozen versus amended.** The frozen protocol used FIFO carriers for the launch cases. Their frozen expectations
were **not met**: every FIFO-permit launch failed closed in the first attempt (A1-P 20/20 and A1-T 16/16 of the
trials that ran), and the FIFO transcript ended `Lost` (A1-F 0/10). Amendment 2, written after that attempt, reran
the semantics cases with today's socket-pair and pipe carriers. It added a file-permit case (A1-FP), and reframed the
concurrency case A9 per carrier object. So the rows below marked *(amended)* are results against amendment 2, not
against the frozen text. The frozen trial counts and expected outcomes were otherwise kept. One frozen detail could
not hold as written under amendment 1: with the authority as a child process, "no waitable child" replaced
"`waitid` gives `ECHILD`".

**Second attempt: every expectation met, except the frozen FIFO-transcript case (0/10).** *(test)*

| Property | Observed |
| --- | --- |
| Launch through Codex PTY *(amended carriers)* | 30/30 with today's carriers, 10/10 with a regular-file permit (atomic `O_CLOEXEC`) plus pipe transcript (A1-FP, added). Payload descriptors {0, 1, 2}; session and group leader; released `ScopeTerminated`; the owner's descriptor table was restored |
| Launch through the Tokio stand-in *(amended carriers)* | 20/20; group leader, not session leader |
| Binding without a PID accessor | the authority recorded the payload's own pid as the scope root in all 70 launches, including the 10 FIFO-transcript ones. The harness's strict count, which also requires `Started`, is 60/70; the frozen A2 was 60 trials |
| Single use, first claim wins *(amended carriers)* | two consumers of one grant: exactly one `Started`, one `Refused`, one payload run (10/10). A late helper after release is refused `InvalidTransition` |
| Cancellation *(amended carriers)* | before carriers, with carriers and unconsumed, and after spawn but before presentation: all `NoHelperCreated`, known not started; the payload never ran (15/15) |
| Setup errors *(amended carriers)* | fail point, invalid attachment (`EBADF`), missing helper (`ENOENT`): no child was created; `NoHelperCreated` (9/9) |
| No replay *(amended carriers)* | a reply that misses its deadline never leads to exec (5/5); a replayed presentation gets `may_exec = false` (3/3); an owner that loses the result cannot release a claimed grant (3/3) |
| Uncertainty *(amended carriers)* | with `Attached`, a PTY payload killed after READY reports exit 1, the same as `exit(1)` (5/5 each). The pipe stand-in keeps the signal (5/5 signal 9; `exit 1` distinguishable, 5/5) |
| Invariants | no attempt had more than one `helper_authorized` receipt (159 attempts had one, of 197 committed); no `NoHelperCreated` with a scope; no secret in receipts |
| Concurrency (A9) *(amended: file permit + pipe transcript, reported per object)* | 50 attached launches with 1000 concurrent unrelated std and Tokio children: 0/1000 held the file permit, 0/1000 a transcript pipe end. For the pipe, a leak is possible (section 3.3); zero here is "not demonstrated" |

### 3.2 Experiment B: backend-owned pre-reap observation (packet 13.2)

**Patch.** The scratch patch adds 263 lines and removes 5 in three files (`observe.rs` +191, `pty.rs` +68/−5, `lib.rs` +4). It is generic and contains no DevGuard type. (The sealed `results-b.md` states `pty.rs` +73/−5; the correction is in the
local `w3-2026-09-28/errata.md`.) The existing single macOS waiter:
1. detects the exit with `waitid(WEXITED | WNOWAIT)`;
2. publishes one opaque opportunity, which carries no PID;
3. waits for an acknowledgement or a bound fixed at registration;
4. reaps once through its original child object.

The observer is registered in the spawn request. Launches other than a non-empty macOS `Attached` launch are refused
before spawning. The harness never reaped.

**Every expectation was met on the first run.** *(test)* The expectation texts are the frozen ones, but amendment 3
changed the setup before the B build:
- a survivor payload that ignores SIGHUP (see the PTY finding below);
- today's carriers instead of the frozen FIFO preparation;
- the reconciler paused in every case except B12;
- separate fixtures for the cases that end `Suspect`.

B12's "no `Suspect` attempt at the end" refers to the main fixture. B2h was added by amendment 3 with a
source-derived expectation.

| Property | Observed |
| --- | --- |
| Survivor recovery | a descendant that outlives the root: with the observer, `Observe` inside the window adopted it; it stayed charged while alive and was released `ScopeTerminated` after it ended (10/10). Without the observer: sticky `Suspect` with tracking loss, still charged after the descendant ended (3/3), as DevGuard's native `reconcile.rs:521` shows |
| Healthy repetition | 50 consecutive survivor runs with the reconciler at its normal cadence: 50/50 released; committed budget zero afterwards |
| Fast exit | 20/20 observed; window 7–12 ms including the `Observe` round trip |
| Stalled observer | reaped at the bound (503–505 ms for a 500 ms bound); `TimedOut`; a late acknowledgement refused; conservative `Suspect` when a survivor existed (6/6) |
| Observer crash | `Failed`; reaped in the same millisecond as the drop (5/5) |
| Terminate, `Drop`, runtime shutdown in the window | one reap each time; the group signal reached the scope's group while the zombie held its ID; runtime shutdown waited for the bound (2002–2004 ms); no zombie (13/13) |
| Output | 262,144 bytes delivered with and without the observer; added exit-report latency about 1–3 ms at the median |
| Authority stopped | `Failed`; after a restart `Suspect`; no release (2/2) |
| Reaping | 125/125 observed children reaped exactly once, by the backend's waiter |
| Pipe | not run: no backend-owned pipe facility is identified (packet section 9.2) |

**PTY finding.** When a session leader with a controlling terminal exits, the kernel sends SIGHUP to the terminal's
foreground group, so a same-group background descendant ends with the root (3/3). On the PTY path the reap-first
survivor pattern therefore needs a descendant that ignores SIGHUP. A descendant that moves to another group is an
escape, which is `Suspect` regardless.

### 3.3 Experiment C: attachments and client sessions (packet 13.3)

In the first attempt, U1, U3, U5 and the reply half of U2 were invalid because of a logged harness read defect. Its
inheritance matrix and U4, U6 and U7 were valid, and both samples are kept. *(test)*

**Inheritance.** 400 unrelated children per cell, two samples (first, second). Bold means an expected 0 that was met.
The expectations of rows S to R are frozen. Row FP was added by amendment 2.

| Creator \ spawner | std `Command` | Tokio `Command` | Codex `Command`, `Explicit` | std inside `spawn_guard` |
| --- | --- | --- | --- | --- |
| client session socket (`connect_timeout`, D6) | 34, 34 | 42, 40 | **0, 0** | 34, 46 |
| the same inside `spawn_guard` | 44, 25 | 49, 34 | **0, 0** | **0, 0** |
| today's launch carriers (under `spawn_guard`) | 5, 1 | 7, 0 | **0, 0** | **0, 0** |
| FIFO carriers, and file permit + FIFO transcript (atomic; frozen rows F and R) | **0, 0** | **0, 0** | **0, 0** | **0, 0** |
| file permit + pipe transcript (amendment 2) | 6, 2 (pipe only) | 1, 2 (pipe only) | **0, 0** | **0, 0** |

**What a holder of an inherited object could do.** All cases met their frozen expectations. The holders were
deliberate; nothing here shows accidental misuse by real children.

| Holder of | Effect |
| --- | --- |
| a session socket | read the owner's `launch_granted` reply, including the 64-character permit, while the owner's read failed (5/5); inject a well-formed `Cancel`, which the authority executed with the owner's principal (3/3); break the session with a malformed frame or `shutdown`, after which the owner failed closed (6/6); keep using the session after the owner closed its copy, until the server's 250 ms per-frame deadline expired while it was idle (3/3). A second `Authenticate` was refused (3/3) |
| the permit carrier | present the grant itself and be authorized as the scope root; the intended helper then failed at the permit stage and the payload never ran (3/3) |
| the transcript writer | forge READY, turning a known non-start into `Lost { ready: true }` (3/3) |

**Remedies compared:**

| Remedy | Closes | Leaves open |
| --- | --- | --- |
| kernel close-by-default in each *unrelated* spawner (`POSIX_SPAWN_CLOEXEC_DEFAULT`, Codex `DescriptorPolicy::Explicit`) | every class, in both samples | spawners that do not use it. For CodeSpace's own std and Tokio spawns this is #79's subject; Codex's own local spawns already have it |
| one shared lock taken by *every* creator and spawner | every class, but only for code that takes it (the session socket leaked even to guarded spawners, because `connect_timeout` does not take the guard) | every spawner outside DevGuard. Imposing it on CodeSpace is rejected (spec §9.4) |
| atomic carriers | the permit carrier (regular file opened `O_CLOEXEC`) | the transcript (a FIFO is unusable by today's readers) and the session socket (no atomic `socket()` on macOS) |
| results through the authority (RC) | the transcript descriptor, and with it the forged READY | a wire and journal change |
| authenticated or encrypted session transport | by analysis, reading the permit in clear and injecting frames | inheritance itself, denial of service by malformed frames or `shutdown`, in-memory exposure. Not run; no library selected |

## 4. Candidate status after W3

"Satisfies both N1 and N2" is not claimed for any row. The table states what the evidence changed.

| Candidate | Before W3 (packet section 14) | After W3 | Still needed |
| --- | --- | --- | --- |
| Y + A, PTY | incomplete; delivery observed (BD-1) | **Launch path demonstrated** with the unchanged helper, wire and authority, through Codex's unmodified `Attached` (3.1). Binding needs no PID accessor | a pin of the explicit-commit class (or a stable release with `Attached`); the carrier choice (3.3); the session-socket remedy (D6); B for survivors; the exit-status uncertainty with `Attached` (K8) |
| B, PTY | proposal | **Contract and patch demonstrated in scratch** (3.2): single waiter, bounded, generic, no CodeSpace reaper; turns the survivor case into normal recovery | upstream acceptance and release (submission deferred by the owner), then a pin; an adapter-side observer policy (thread, bound); Linux |
| A, pipe | incomplete | launch semantics observed only through a Tokio stand-in with a child-side step. CodeSpace's real pipe path was not changed | the delivery mechanism (owner interpretation against N1); an observation facility (none identified); D6 |
| B, pipe | no thin facility identified | unchanged; not run | unchanged |
| D6 | prerequisite; unresolved | **Exposure demonstrated** (3.3): an inherited session gives reply theft (including the permit) and request injection. Kernel close-by-default in the spawner removed it in every sample | a remedy for DevGuard's own session socket that does not depend on CodeSpace changing its spawns, or a CodeSpace hygiene change justified independently (#79); or a transport change as a partial measure |
| Launch carriers | under `spawn_guard`; atomicity listed as still needed in the Y + A row | the permit can be atomic (regular file, 0 leaks everywhere, works with the unchanged helper). The transcript cannot be atomic and stay compatible with today's reader | choose: file permit + pipe transcript + spawner hygiene; or RC; or a reader that works with FIFOs. Assess file-permit persistence and cleanup |
| X, RB, RC, RE, RA, RG | as in #13 | unchanged; not tested | unchanged |

Taken together, the smallest complete candidate for **macOS, PTY, InProcess** is now concrete but not proven or
approved:
- Y + A through Codex `Attached`;
- B, as an upstream generic observation;
- an atomic file permit;
- a D6 remedy.

The D6 remedy is the open item that determines feasibility under N1: either every same-process spawner excludes
unrelated descriptors (CodeSpace's own spawns are #79's subject), or DevGuard changes its session transport so that
an inherited socket gains nothing — which, by the evidence, a transport change alone does not fully achieve. *(inference)*

## 5. Limits

- One host, a small number of runs per case, synthetic workloads. No representative rate was measured.
- macOS only; Linux was not run.
- CodeSpace's own processes, and its real pipe and PTY adapters, were not modified or run.
- Experiment B's patch is a scratch experiment: it is not reviewed upstream and not a candidate pin.
- Experiment A's module is experimental code, not a proposed API; its branch stays local.
- The Tokio pipe stand-in is not CodeSpace's pipe path.

## 6. Decisions requested

1. **This packet.** Merge the DevGuard PR that carries this record at its exact head, as a non-normative record, or
   request changes. Merging approves nothing proposed in it.
2. **Upstream proposal for B.** Decide whether to prepare a generic upstream proposal to Codex based on the scratch
   contract of section 3.2. This needs a separate submission approval; the patch is not submitted.
3. **D6 direction.** Choose which remedy to develop further as a protocol, not an implementation:
   - (a) rely on kernel close-by-default in every same-process spawner. CodeSpace's side stays under #79 and its own
     justification;
   - (b) change DevGuard's client-session design so an inherited endpoint gains nothing. Candidates: a per-request
     connection with a one-shot token, or an authenticated transport. Each needs its own protocol and threat
     analysis. By inference, neither removes denial of service by an inherited socket; section 3.3 analysed only
     the transport case;
   - (c) both;
   - (d) neither yet: first run the #79 diagnostic in CodeSpace's own processes.
4. **Carrier direction.** Choose among: a regular-file permit with a pipe transcript plus spawner hygiene; RC (results
   through the authority); or changing the helper's and owner's readers to accept FIFOs. Any choice is a later
   reviewed change.
5. **Normative reconciliation.** Decide whether to prepare the reviewed DevGuard and CodeSpace policy PRs (the
   packet's R2, CS and G) now. W3 changes none of them.
6. **Local experimental branch.** Keep `codex/exp-a-preparation` (local only; head in section 7) for reference, or
   approve removing it. Its commit is not on `main`, so removal needs `git worktree remove` and `git branch -D`.
7. **CodeSpace #79.** Decide whether its proposed bounded diagnostic should be authorized. It is non-implementation
   and CodeSpace-owned.

Settled, and not asked again (spec §20): whether DevGuard may use Codex or other external dependencies, and whether
CodeSpace's pin is immutable.

## 7. Handoff and evidence

**Heads at writing:**
- DevGuard `main`: `9e21cc8f707f16b7da98490293b5ea7e4548916d`.
- CodeSpace `main`: `ab0341b5cf5eda8c82730487d4475f1ac922daf5`, gitlink `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`.
- The local experimental branch `codex/exp-a-preparation` is at `b50e437`, on `9e21cc8`, and was never pushed.

**Evidence** is local, git-ignored and not published, under `<DEVGUARD_CHECKOUT>/evidence/`:

| Set | Contents | `MANIFEST.json` SHA-256 |
| --- | --- | --- |
| `upstream-adapter-2026-09-28-postseal/` | final CI of the W0–W2 PRs; reconstructed audit notes; audit-fix diff | `d6372359483dcad874a29e67884ac8f286c372fa14f1bcf66b7d919be718bde0` |
| `w3-2026-09-28/post-merge-ci/` | post-merge CI of the three W3 merges | `b7c74ba9fe59a7f21e1da55b392781833f0758d8c591e0618037b17343648a3e` |
| `w3-2026-09-28/experiments/` | protocols and amendments; builds; the experimental diff and observer patch; harness sources; all runs, including invalid attempts; results | `ce342e98d54eba653f6400489d0b4569f3bc1edfda9d76b3fe0b55ca151bf1bb` |
| `w3-2026-09-28/` | seal written after this PR opens, over the above plus the authorization, pre-flight, merge, cleanup, tracker, backup, issue and errata records | in the final tracker comments |

- Protocol digests: common `a3b12e43…`, A `b453b662…`, B `1fa2799a…`, C `df8e9bb2…`. Amendments: 1 `301ed92b…`,
  2 `a46ad7f1…`, 3 `3e116274…`.
- Observer patch: `7ca475aeddc079b55375cf3aea0faa22aaf15a5e79c90f6d5863d88866383752`.

**Next allowed action:** the owner's decisions in section 6.

**Stop boundary:** until then there is:
- no production Codex pin change, no production dependency, and no CodeSpace/DevGuard integration;
- no service or credential change;
- no normative policy PR;
- no upstream submission.
