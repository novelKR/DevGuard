# CS-RG boundary revalidation: hardened candidate analysis (2026-09-27)

> **Status: candidate analysis for the owner's decision; not a design decision.** It approves no architecture,
> starts no work unit, changes no ledger status and authorizes no implementation. The CS-RG implementation hold
> stays in force. Companion to the [session handoff](2026-09-27-cs-rg-boundary-revalidation.md).

Revisions: DevGuard `7e3cbda91308f527d6cc34fba908375e6332bc58`, CodeSpace `794867ef52f530be6bc0d91aa10416d5195367b7`,
Codex pin `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (rust-v0.154.0), Codex stable rust-v0.157.1
`36650394c5b38c2990ccf2a3457165ca3e9d9726`, Codex main `41f9084b30812db321a0b592def4f500d1e79cf4` (looked up 2026-09-27
12:27Z). Line references are to those revisions.

Evidence levels: **source** (confirmed source fact), **test** (confirmed diagnostic result), **inference**,
**candidate** (unverified candidate), **experiment** (requires experiment), **owner** (owner decision),
**unsupported** (unsupported under current constraints).

## 1. Review disposition
The owner's reference review of 2026-09-27 was used as a checklist and checked against source; where they differ,
source decides.

| ID | Review point | Checked | Verdict | Correction |
| --- | --- | --- | --- | --- |
| RR-1 | Conclusions ("pipe needs nothing else", "PTY needs only a small precondition") are stronger than the evidence | the earlier handoff text; sections 3-12 below | confirmed | every route is an incomplete candidate; no combination is shown supportable (section 12) |
| RR-2 | Reap-first safety and operability were conflated | contracts.md Observation, Limits, Reconciliation; crates/launch/tests/reconcile.rs L470-L560; crates/macos/src/scope.rs L628; macOS CI run 36311129096 | confirmed and strengthened: the leak is deterministic and has no recovery before reboot on macOS | section 3; RE reclassified |
| RR-3 | RB/RC were called required without a protocol | earlier packet; contracts.md Fenced launch helper | confirmed | RB/RC are incomplete candidates; protocol and gaps in sections 4-5 |
| RR-4 | Authority-mediated results need defined meaning | contracts.md Transcript, Command-line owner | confirmed | section 6 |
| RR-5 | The client socket window gates R; a DevGuard-local mutex is not enough | crates/client/src/connect.rs L39-L51; crates/client/src/launch.rs L29-L36, L150-L154 | confirmed; the window also exists in the current DevGuard design, not only in R | section 7; D6 becomes a prerequisite of every CodeSpace route |
| RR-6 | X and Y each solve a narrow slice | upstream sources (pty.rs, process.rs) | confirmed | section 8 |
| RR-7 | A supervising helper changes CodeSpace-visible semantics | CodeSpace process.rs L360-L384, L706-L740; Codex pty.rs L55-L92 | confirmed for pipe; open for PTY | section 9 (also adds a group-anchor variant) |
| RR-8 | "Existing termination paths fit" is not established | Codex process.rs L273-L277; CodeSpace process.rs L33, L208-L211; runtime.rs L42-L50 | partially confirmed: DevGuard release never relies on CodeSpace signals, but UDS worker kill and eviction-time signalling affect operability and unrelated processes | section 10 |
| RR-9 | The public handoff was too summarized to review | earlier handoff | confirmed | this document |
| RR-10 | "G3 complete" could be read as technical verification complete | earlier report wording | confirmed | status is: review record submitted; candidate contracts not verified; product support pending |
| RR-11 | "`not found` means queued" overgeneralizes one incident | spawn records of 2026-09-27 | confirmed | section 14 |
| RR-12 | Base movement is not head movement | Git and GitHub behaviour; the owner's 08:35Z merge rule | confirmed | section 14 |
| RR-13 | The owner should not be asked to re-approve fixed principles or to pick X or Y prematurely | owner instructions of 2026-09-27 | confirmed | section 13 lists only open choices |
| RR-14 | Merge approvals and architecture decisions must be separate tracks | same | confirmed | section 13 |
| RR-15 | The hold PRs and the handoff PR are appropriate records | DevGuard #12, CodeSpace #75, DevGuard #13 | confirmed | none |

No review point was contradicted by source. One point was strengthened: RR-2 (section 3).

## 2. Corrected findings
| ID | Finding | Level |
| --- | --- | --- |
| F-1 | Design revision 1 listed CodeSpace's Codex PTY use and pin as changeable and kept DevGuard's helper, direct-child check, descriptor layout and observe-before-reap fixed (design-revision-1.md L96-L107; CS-RG.md L22); this became the D1 default and C00/C03/C09 (L479; CS-RG.md L47, L88, L164) and entered CodeSpace's reuse policy (codex-reuse.md L72) | source |
| F-2 | No unauthorized execution. One self-reported lapse: a delegated read-only run whose status was unresolved was not stopped and wrote a superseded C00 plan after the hold; no repository or product effect | source (records) |
| F-3 | `HelperCommand` makes DevGuard's client perform the spawn (launch.rs L150-L154), so a governed PTY launch would take PTY and spawn ownership from Codex (N1.2) | source |
| F-4 | Reap-first cannot release a lease falsely, and it leaves a charged Suspect attempt that no later ordinary observation clears; on macOS only a reboot releases it (section 3) | source + test |
| F-5 | No Codex revision checked offers pre-reap observation or owner-controlled reap for PTY children (pin pty.rs L244-L253; main pty.rs L447-L449) | source |
| F-6 | `ChildFds::Attached` (main and prereleases, #47797) delivers close-on-exec descriptors only to the intended child (main pty.rs L405-L408, L526); no stable release has it; runtime behaviour untested | source; behaviour: experiment |
| F-7 | macOS cannot create pipes or sockets close-on-exec atomically. DevGuard's grant descriptors are created under `spawn_guard` (launch.rs L175-L190), its session sockets are not (connect.rs L39-L51), and CodeSpace's Tokio pipe, patch-helper and probe spawns neither take the guard nor close descriptors in the child | source |
| F-8 | Codex exposes no PTY child PID at the pin or on main (`SpawnedProcess`, process.rs L356-L361) | source |
| F-9 | Scope establishment accepts a root only while it is alone in its group (contracts.md, Native policy application) | source |
| F-10 | DevGuard needs Rust 1.95, CodeSpace declares 1.88; a DevGuard client dependency in CodeSpace must stay optional for the `off` build | source |
| F-11 | CodeSpace-only issues, outside CS-RG and not relied on by DevGuard's release rules: tool text promises subtree termination the code does not perform (mcp.rs L124-L125); a PTY handle dropped at eviction, up to 15 minutes after exit, signals a stored numeric pgid (Codex process.rs L273-L277 with CodeSpace process.rs L33); worker setup failure leaves the worker and its directory (runtime.rs L51-L64) | source; runtime not run |

## 3. Reap-first: safety versus operability
Scenario: the payload root starts a child that stays in the group (`/bin/sh -c "/bin/sleep 3 & exit 0"`), the
root exits, and the existing backend reaps it at once (Codex PTY: immediately; CodeSpace pipe: within about 20 ms).

| Step | Known | Unknown | Tracking loss | Suspect | Release possible | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 1. root exits | the root is a zombie holding its PID and group ID | — | no | no | not yet | source (contracts.md Observation) |
| 2. backend reaps the root | root start identity gone | whether group members belong to the scope | — | — | — | source |
| 3. a descendant is in the group | a process with that group ID exists | whether it is the scope's or a reused group | — | — | — | source |
| 4. no observation adopted it while the root held its PID | — | its membership | — | — | — | source: adoption needs the root to hold its PID, or a known member in the same pass |
| 5. authority observes afterwards | unknown member present, no known member | — | yes, sticky | yes | no | test: `a_root_reaped_before_observation_leaves_its_survivor_as_tracking_loss` (crates/launch/tests/reconcile.rs L521-L560) |
| 6. the descendant exits | group empty | — | still sticky | yes | no | test: same, "The loss is sticky after the survivor ends" (L548-L552) |
| 7. later Observe and reconciler passes | same | — | sticky | yes | no | source: "no later ordinary observation clears them"; the macOS backend always reports `prior_tracking_loss_resolved: false` (crates/macos/src/scope.rs L628) |
| 8. daemon restart | every committed attempt becomes Suspect; bound scopes stay Suspect until reboot | — | lost | yes | no | source (contracts.md Restart); test: `a_daemon_crash_keeps_every_charge_and_restart_fences_old_grants` |
| 9. reboot | the scope is from an earlier boot | — | irrelevant | — | yes, as previous-boot termination | source (contracts.md Reconciliation); test: `reconcile_releases_a_bound_scope_after_a_reboot_as_previous_boot` |

Contrast: when the owner observes while the root is still unreaped, the survivor is adopted and the attempt is
released after it ends (test `observing_before_reap_tracks_survivors_and_release_waits_for_them`, L470-L516).

Diagnostic status: the three native tests above ran in the macOS job of DevGuard CI run 36311129096 (head
`2bbe7c5`, native reconciliation suite "13 passed; 0 failed; 4 ignored"). The pathological sequence is therefore
established by an existing diagnostic; no new diagnostic was needed.

Conclusions:
- Safety against false release: holds (source, test).
- Operability: fails for this pattern. A normally completed workload that leaves a short-lived background
  descendant born after the last observation keeps its reservation charged until the host reboots. There is no
  unconditional release (contracts.md Reclamation evidence) and no native path that resolves prior tracking loss.
  Repeated occurrences shrink admission capacity although the host is idle. (source, test)
- With both CodeSpace backends reaping at once, the observe-before-reap window is effectively zero, so any
  survivor not already adopted by an earlier reconciler pass (1-second cadence) triggers this outcome. (inference)
- Frequency in representative agent workloads: unknown (not measured). The existence of the sequence does not
  depend on that measurement.

## 4. Authorization: current mechanism and the RB candidate
| Property | Current design (contracts.md Fenced launch helper) | RB candidate (owner-confirmed identity, no permit) | Status |
| --- | --- | --- | --- |
| One-time launch authorization | one-time permit (secret) + durable claim + first `authorize_run` sets `may_exec` | durable single binding of one OS identity (PID, start, boot) to the attempt, made from the owner's authenticated confirmation; then the same claim and first `authorize_run` | candidate; binding must be durable before any claim |
| Owner/helper relationship | OS: parent is the registered, running owner | same OS check, plus the owner's assertion naming the child | candidate |
| Attempt binding | the permit is the grant's | the presenting peer's OS identity must equal the bound identity; the helper's attempt ticket is a non-secret assertion | candidate |
| Credential isolation | permit on a private descriptor, closed before exec; creation under `spawn_guard` | no permit secret; the owner's session credential and socket window remain (section 7) | partial |
| Duplicate helper refusal | the first claim wins | only the bound identity can claim; a second, different confirmation for the same attempt is refused | candidate |
| Lost replies | lost `BeginLaunch` reply: reconcile, never re-grant; lost authorization reply: no exec, re-presentation gets `may_exec = false` | `BeginLaunch` carries no secret, so a replayed lookup can continue; confirmation is idempotent for the same identity; authorization unchanged | candidate |
| Pre-claim versus post-claim failure | pre-claim refusal leaves the grant unclaimed; post-claim refusal kills the helper and keeps the scope charged | unchanged; plus: a bound but unclaimed identity that is gone could support `NoHelperCreated` | candidate; needs a contract statement |
| Durable claim | scope recorded before binding | unchanged | source |
| Payload-start evidence | transcript READY then descriptor closed at exec, plus exit status | RC (section 6) | candidate |

Uncovered gaps (RB):
- G-RB1 ordering: the helper may present before the owner's confirmation arrives; a wait or refuse-and-retry rule
  within the 250 ms frame deadlines is not defined.
- G-RB2 identity binding: a PID can be reused between spawn and confirmation if the helper dies at once; the
  authority must read start and boot identity itself, check parentage and liveness, and possibly the executable
  path. Misbinding cases (one process bound to two attempts) need explicit rules.
- G-RB3 the owner must know the child PID: available for Tokio pipe children; not available for Codex PTY
  children (F-8).
- G-RB4 wire and journal changes (a confirmation request, presentation without a permit) need version
  negotiation and requalification.
- Equivalence argument: under DevGuard's trust scope (cooperative workloads, design.md L12), RB removes the
  permit-leak surface; it does not yet have a reviewed threat analysis. RB is therefore an incomplete candidate,
  not a required change.

## 5. Candidate R protocol (governed pipe, macOS, InProcess) as a message sequence
Fact types: OS = observed by the authority from the OS; OWN = owner assertion; HLP = helper assertion; DUR =
durable authority state; INF = inference.

| # | Situation | Sequence and outcome | Fact types |
| --- | --- | --- | --- |
| 1 | registration | the owner (Runner process) authenticates in a fresh session and registers its instance; identity from the OS peer | OS, DUR |
| 2 | admission | `Admit(attempt, meaning, intent)` records the attempt | DUR |
| 3 | launch commitment | `BeginLaunch` commits; returns launch data (helper path, argv with non-secret ticket), no secret | DUR |
| 4 | what the authority returns | launch data only; nothing that spawns | DUR |
| 5 | launch as data | program = helper, argv = ticket + `--` + payload argv; no `Command` object | candidate |
| 6 | who spawns | CodeSpace's existing Tokio pipe spawn, unchanged except the argv | source (P1 path) |
| 7 | helper identification | the owner sends `ConfirmHelper(attempt, pid)` right after spawn returns | OWN |
| 8 | PID + start/boot binding | the authority reads start and boot identity and parentage for that PID and binds it once | OS, DUR |
| 9 | one-time authorization | durable single binding + existing claim + first `authorize_run` | DUR |
| 10 | who may claim | only the process whose peer identity equals the binding | OS |
| 11 | two helpers | the second is not bound; refused pre-claim | OS, DUR |
| 12 | cancel before spawn | Draining; no binding accepted; the owner reports no helper (`AbandonLaunch`) | OWN, DUR |
| 13 | cancel after spawn, before authorization | Draining blocks binding and authorization; an unclaimed helper exits 125; a claimed one is killed and its scope settles by evidence | DUR, OS |
| 14 | authority response lost | owner looks the attempt up; no secret was lost; no re-admission | DUR |
| 15 | owner response lost | confirmation is re-sent; idempotent for the same identity | DUR |
| 16 | helper response lost | the helper does not exec; re-presentation gets `may_exec = false`; the attempt is fenced, not replayed | DUR (source rule) |
| 17 | duplicate presentation | same identity: `may_exec = false`; other identity: refused | OS, DUR |
| 18 | helper death before claim | owner observes the exit; `AbandonLaunch`; `NoHelperCreated` | OWN, OS |
| 19 | helper death after claim | scope-based settlement | OS, DUR |
| 20 | payload exec failure | the helper reports `exec_failed` on its session and exits 126/127 | HLP, OS |
| 21 | payload start after a lost reply | cannot happen: exec follows only a received `may_exec = true` | source rule |
| 22 | owner death | helpers fail the parent check; unclaimed grants become Suspect and are never released before reboot | source; operability gap |
| 23 | authority restart | committed attempts Suspect; bound scopes Suspect until reboot | source; operability gap |
| 24 | expiry | Prepared expires after 5 s | source |
| 25 | successful completion | root exits, the backend reaps, the reconciler releases if no unknown survivor | source |
| 26 | descendant survival | section 3: sticky Suspect until reboot unless adopted before reap | source + test |
| 27 | termination | CodeSpace's pipe terminate (SIGKILL to the payload PID, which is the helper PID after exec) unchanged; optional DevGuard `Terminate` for the scope | source; optional: owner |
| 28 | resource release | only scope termination, `NoHelperCreated` or previous-boot termination | source |

R is an incomplete candidate. Its open items: G-RB1..4; RC gaps (section 6); the socket window (section 7); the
operability gaps at rows 22, 23 and 26; UDS mode (credential path, section 10).

## 6. RC: results through the authority
| Question | Candidate answer | Status |
| --- | --- | --- |
| Messages | helper -> authority: presentation, READY (after credentials closed), `exec_failed{errno}`; authority -> helper: `may_exec` | candidate |
| Durable | claim, binding, `RunAuthorized` (existing); `exec_failed` would need a new journal field | source / candidate (schema change) |
| Transport acknowledgements only | READY | candidate |
| Meaning of READY | pre-exec boundaries done; authorization only; never evidence that the payload started | source (current rule) |
| `exec_failed` | helper message plus exit status 126/127 | candidate |
| Recorded but response lost | the helper still exits by its own rule; the owner reads the record by query | candidate |
| Helper disconnect | session EOF after `may_exec` looks like a successful exec and like a helper killed in between; the exit status decides, as today | source (current ambiguity kept) |
| After restart | only durable records survive; READY does not | source / candidate |
| Missing message as evidence | never evidence of non-execution | source (N2.2) |
| Replay of an old message | bound to attempt and helper identity; refused otherwise | candidate |
| Cross-attempt confusion | attempt key plus bound identity | candidate |

## 7. Client session-socket window (D6)
- Descriptor: the AF_UNIX stream socket from `socket()` (connect.rs L39), inheritable until `F_DUPFD_CLOEXEC`
  (L46) and `drop(initial)` (L51); the parent then connects the same socket (L58-L60). (source)
- Platforms: the code is not platform-gated, so macOS and Linux; Linux could use `SOCK_CLOEXEC`. (source)
- Interval: a few system calls; not measured. (source for the span, duration unmeasured)
- Who can inherit it: any fork in the owner process during that span that does not close unintended descriptors:
  CodeSpace Tokio pipe spawns, the patch helper and sandbox probes (the owner is the Gateway in InProcess mode and
  the worker in UDS mode, and both run these spawns); DevGuard's own `HelperCommand::spawn` too, because `connect`
  does not take `spawn_guard`, and the helper passes every inheritable descriptor to the payload. Codex PTY
  children close them. (source)
- Usable in the child: the inherited descriptor names the same socket, which becomes the connected, authenticated
  session; the child could read replies or inject frames during that session, but cannot read the owner's
  outbound credential. (inference from descriptor semantics; not demonstrated)
- R opens such sessions at every step. (design)
- N2: possible N2.1 (requests in the owner's session) and N2.5 (authenticated channel exposure). Deliberate misuse
  is outside DevGuard's cooperative trust scope; accidental interference would fail the request closed.
  (inference; not demonstrated)
- Protection boundary: a DevGuard-local mutex covers only forks that take it; CodeSpace's do not. On macOS a
  complete fix needs every forking path in the owner process either to serialize with descriptor creation or to
  close unintended descriptors in the child. That is a CodeSpace change, which CS-RG alone cannot justify (N1.3);
  it could come from an independent CodeSpace descriptor-hygiene change (N1.5). On Linux, `SOCK_CLOEXEC` closes
  the window atomically. (source for the mechanisms; the CodeSpace-side remedy is an owner matter)
- Related, unverified: Rust's standard pipe and socket-pair creation on macOS is believed to be non-atomic too,
  which would let concurrent CodeSpace spawns leak each other's stdio pipes. Not read in this review.
- Classification: prerequisite of every CodeSpace route (R, X, Y), and a defect of the current DevGuard design
  independent of CS-RG.

## 8. X and Y: exact contribution
| Candidate capability | Solves | Does not solve | Evidence | Remaining prerequisites |
| --- | --- | --- | --- | --- |
| X: generic PTY child PID accessor | the owner can name the numeric PID of the PTY child it asked Codex to spawn | start/boot identity binding (the authority must read it), authorization (RB), lifetime tracking (DevGuard scope evidence), termination safety (K4), pre-reap observation (section 3) | source: absent at pin and main | upstream acceptance, a release, a pin update; plus every R prerequisite |
| Y: `ChildFds::Attached` | delivery of selected close-on-exec descriptors only to the intended PTY child | creation-time races for those descriptors (needs atomic creation, for example `O_CLOEXEC` files), the session-socket window (section 7), reap ownership, observation and reconciliation (section 3), authorization semantics (the permit is still needed), release evidence, terminate behaviour | source (main); behaviour: experiment | a stable release with #47797, a pin update, atomic grant-descriptor creation, section 7, section 3 |

Neither X nor Y makes the PTY contract complete.

## 9. Helper supervision (RA) and a group-anchor variant (RG)
RA: the helper forks the payload, waits, and exits with the payload's status after a final observation.
| Aspect | Effect | Status |
| --- | --- | --- |
| Process CodeSpace owns | the helper; the payload becomes a grandchild | source (topology) |
| Terminate and timeout, pipe | CodeSpace sends SIGKILL to the helper PID only (process.rs L360-L384, L706-L740); the payload is orphaned and keeps running | inference: incompatible with N1.1 unless governed terminate also signals the scope, and that equivalence is unproven |
| Terminate and timeout, PTY | Codex signals the group (pty.rs L55-L92), reaching the payload | inference |
| Ctrl+C, foreground group, resize | same group, so the payload receives them; a job-control shell leaves the group (an escape) | inference |
| stdin, EOF, output drain | equivalent only if the helper closes its stdio copies after fork | inference; experiment |
| Exit code | the helper must reproduce the payload's status, including signal deaths | inference; experiment |
| Helper failure after payload start | payload orphaned; CodeSpace reports an exit while work runs | inference: N1.1 violation |
Status: blocked by N1 for pipe; incomplete candidate for PTY, requiring an experiment.

RG (added in this review): instead of supervising, the helper keeps exec-ing the payload (the payload keeps the
helper's PID) and first leaves a small group anchor in the same group that holds no stdio. A known member in the
group lets the authority adopt later members after the root is reaped. Constraints found in source: scope
establishment requires the root to be alone in its group (F-9), so the anchor could only be created after binding,
and it must be observed (adopted) before the payload can exit; that needs a helper-initiated observation, which
the helper's session does not allow today. Open: anchor lifetime and cleanup after authority loss, SIGHUP when a
PTY session leader exits, visibility of an extra process to users, and whether CodeSpace-visible semantics stay
identical. Status: unverified candidate; requires a DevGuard contract change and an experiment.

## 10. Termination and existing CodeSpace behaviour that candidates depend on
- DevGuard's release never relies on CodeSpace's or Codex's signals; it relies on scope evidence. (source)
- CodeSpace-visible terminate for R/X/Y stays the existing mechanism: pipe SIGKILL to the payload PID; PTY group
  signal. Unchanged from `off`. (source)
- Eviction-time signalling of a stored numeric pgid (F-11) is not relied on and not amplified by governance; it can
  still hit an unrelated group, governed or not. (source; reuse effect inference)
- UDS mode: when the Gateway kills the worker (runtime.rs L42-L50), the registered owner dies; its unclaimed grants
  become Suspect with no release before reboot (section 5 row 22). UDS mode therefore adds an operability gap on
  top of the credential path. (source)
- Worker setup failure before the handshake creates no DevGuard state. (source)

## 11. Candidate classification
| Item | Status | Solves | Unresolved | Diagnostic | Product change | Requalification | Coupling | Fallback |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| R | incomplete candidate | pipe launch without a CodeSpace backend change and without an extra Codex primitive (none identified so far) | G-RB1..4, RC schema, D6, operability (rows 22, 23, 26) | frequency of section 3 in real workloads; RG feasibility | DevGuard client, helper, wire | C05, C06, wire; CodeSpace governed parity | CodeSpace pins a DevGuard client revision | governed pipe unsupported |
| X | incomplete candidate | numeric PTY child PID | everything in R plus upstream acceptance | none until R is complete | Codex upstream API; pin | as R plus pin qualification | pin to a release with the accessor | governed PTY unsupported |
| Y | incomplete candidate | descriptor delivery to the PTY child | atomic grant-descriptor creation, D6, section 3, stable release | `Attached` behaviour under concurrent spawns | DevGuard; pin | C05, C06; pin qualification | pin to a release with #47797 | governed PTY unsupported |
| RB | incomplete candidate (route-specific: R, X) | removes the permit secret | G-RB1..4; threat analysis | ordering and PID-reuse cases | DevGuard wire | C05, C06 | wire version | keep the permit (Y) |
| RC | incomplete candidate | removes the transcript descriptor | durability of `exec_failed`; schema | none | DevGuard journal and wire | C05 | journal schema | keep the transcript descriptor |
| RE | safe against false release; fails operability for a deterministic pattern | no change needed | reservations unrecoverable before reboot | frequency only | none | none | none | a pre-reap mechanism (RG, RA, or an upstream primitive) or an explicit restriction |
| RA | blocked by N1 for pipe; incomplete candidate for PTY | observation before the root disappears | N1.1 semantics | exit, signal, EOF fidelity | DevGuard helper | C04, C05, C06 | none | RE or RG |
| RG | unverified candidate | adoption after reap without changing CodeSpace's processes | establishment order, helper-side observation, anchor lifetime, PTY SIGHUP | anchor experiment | DevGuard helper and contract | C04, C05, C06 | none | RE with restriction |
| D6 | prerequisite of every CodeSpace route; defect of the current design | — | no N1-compatible complete macOS fix identified | exposure demonstration | DevGuard client (partial); complete fix needs CodeSpace participation | C01/C02 | none | treat as an accepted risk only by an explicit owner decision after evidence |

No route is contract-complete. Governed execution is unsupported under the current constraints until at least D6
and the operability gap have an answer that holds N1 and N2.

## 12. Support matrix
| Platform | Transport | Runner mode | Current status | Missing proof or change |
| --- | --- | --- | --- | --- |
| macOS | pipe | InProcess | unsupported; source-level candidate R identified; protocol incomplete; not implemented; not qualified | G-RB1..4, RC schema, D6, operability |
| macOS | PTY | InProcess | unsupported; candidates X and Y identified; protocol incomplete; not implemented; not qualified | all of the above plus X (upstream) or Y (atomic creation, stable release) |
| macOS | pipe or PTY | UDS | unsupported; as above | plus the worker credential path and worker-death operability |
| macOS | any | any, governance `off` | unchanged | none |
| Linux | any | any | unsupported pending DG-LINUX | platform milestone; Codex main routes Linux PTY launches through a setup helper, which may break the direct-child check (inference) |

## 13. Decisions that remain
Architecture track (only genuine choices; the fixed principles are not reopened, and no route choice is asked
because no route is complete):
1. D6 direction: pursue a DevGuard-side reduction plus an independent CodeSpace descriptor-hygiene proposal (N1.5),
   or first gather evidence on exposure under the cooperative trust scope and decide afterwards.
2. Operability direction for reap-first leaks: authorize a bounded RG experiment (DevGuard fixture, isolated
   worktree, not committed), pursue a generic upstream reap-control primitive, or accept governed execution only
   for a restricted, documented workload class after frequency evidence.
3. Upstream engagement: approval to submit generic proposals (a PTY child PID accessor; owner-controlled reap) if
   the owner wants them pursued; drafting is allowed without approval.
4. UDS mode: defer it (InProcess first) or choose a credential path now.

Merge track (separate from the above; each needs an explicit approval of its exact head):
| Repository | PR | Head | Base | Base moved since review | Head changed since review | CI |
| --- | --- | --- | --- | --- | --- | --- |
| DevGuard | #12 | `ae85ebb95b68821361ae59d3c60b5344a1d8ab03` | `7e3cbda91308f527d6cc34fba908375e6332bc58` | no | no | 4/4 success |
| CodeSpace | #75 | `1bee230595698b0974df43561bccfce67d7e8cb9` | `794867ef52f530be6bc0d91aa10416d5195367b7` | no | no | 5 success, 6 not selected |
| DevGuard | #11 | `2bbe7c5ed88ad3170bc76985bdecb1fd7434d501` | `7e3cbda91308f527d6cc34fba908375e6332bc58` | no | no | 4/4 success |
| DevGuard | #13 | changes with this document; see the PR | `7e3cbda91308f527d6cc34fba908375e6332bc58` | no | yes (this revision) | pending for the new head |

## 14. Procedural corrections
- Delegated runs: a status of `not found` does not establish that a run is completed, cancelled, queued or gone.
  Treat it as indeterminate until an authoritative state or a result resolves it. Delegated work carries the
  instruction revision it was given; before its result is accepted, and when it starts, check that the revision is
  still current (for example that no hold has been issued since).
- Base versus head: merging one PR advances `main`; it does not change another PR's head. Updating a PR with the
  new base (merge or rebase) creates a new head. This project's rule is stricter: the owner's merge condition
  re-checks head and base immediately before merging, so a moved base triggers re-verification of the PR, and an
  updated head needs a new approval. That is a project approval rule, not a Git or GitHub necessity. A green CI run
  does not replace review of a changed head.
