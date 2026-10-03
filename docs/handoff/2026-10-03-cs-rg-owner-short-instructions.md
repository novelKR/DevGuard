# Owner's short instructions and answers on CS-RG work (2026-09-30 to 2026-10-03)

> **Status: dated record of owner instructions; not maintained.** The owner gave these short instructions and answers
> in Claude Code session chat. None was committed at the time. Each question is the session's question as it was
> asked, shown for context. Each answer or instruction is the owner's text, reproduced unchanged except that the chat
> tool's text around an answer was removed. They approve nothing beyond what they say, and later decisions may have
> superseded parts of them: the [CS-RG session handoff of 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what
> happened next and which parts still apply.

Session A ran from 2026-09-30 08:43 to 2026-10-03 05:11 UTC on the owner's Mac through Remote Control; session B was a
cloud session from 2026-10-03 05:44 to 15:11 UTC. The longer instructions given in the same sessions are separate
records, listed in section 4 of the handoff. Times are UTC.

## 2026-09-30 11:58 UTC, session A: three answers (hold notices, README, #79 scope)

**Hold notices.** Question: Package A: every DevGuard file R2 edits carries a hold notice saying the text below is
'retained unchanged for historical traceability'. D3 also sits inside ADR-006, which records a decision accepted on
2026-09-27, and CodeSpace's D3 bullet is under the same kind of notice. How should R2 treat that held text?

Owner's answer:

````text
Choose option 1, with one constraint: distinguish living normative/editorial text from dated decision records. Update current-policy prose in place, but do not rewrite the historical wording of ADR-006/D3 or other dated accepted decisions. For each preserved historical entry, add a dated annotation that revision 2 supersedes the Codex-free restriction prospectively. Any existing “retained unchanged for historical traceability” notice should be amended only enough to explain that this specific policy sentence is superseded by revision 2; it must not imply that the stale rule is still current.
````

**README hit.** Question: Packet §5 missed one statement: README.md:49 says DevGuard 'stays free of Codex dependencies
as a present engineering choice'. README.md is normative and has no Korean counterpart. Should R2 change it?

Owner's answer:

````text
Choose option 1, with scope discipline: include README.md:49 in R2 because it is living normative/current-policy text and directly conflicts with the settled dependency policy. Preserve README.md:41 unchanged because it describes the authority-core boundary, which revision 2 still requires. Record README.md:49 in the PR body and tracker as an additional hit discovered by the required repository-wide search beyond packet §5; do not treat that discovery as permission to broaden R2 into unrelated README cleanup.
````

**#79 scope.** Question: Package B's bounded run links CodeSpace's own InProcess Runner library at ab0341b, which
means building the Runner graph with one job, about 800 locked crates. Measuring what the UDS worker inherits at
Gateway startup would also require building codespace-server. Which scope should the frozen protocol measure?

Owner's answer:

````text
Choose option 1, with explicit coverage limits: measure only the actual CodeSpace Runner pipe and PTY paths in this first bounded run, at the frozen baseline and the predeclared default/stress concurrency points. Keep the UDS worker and patch helper in the source inventory, but record them explicitly as not measured / not_run, never as negative or passing evidence. Do not infer worker safety from Runner results. If the Runner results or source inventory show that worker inheritance remains decision-relevant, request a separate owner-authorized worker/Gateway diagnostic rather than expanding this frozen run. Any measurement attributed to a CodeSpace path must exercise the actual frozen CodeSpace implementation, not a generic Tokio/Codex stand-in.
````

## 2026-09-30 14:12 UTC, session A: two answers (#20 CI classification, packages B and C)

**#20 CI fix.** Question: PR #20 fails CI because scripts/ci-policy.json doesn't classify the new
docs/design-revision-2.md. Fixing that is a CI configuration change, which this session is not allowed to make. How
should I proceed?

Owner's answer:

````text
Choose option 1: classify it in #20.
I authorize one narrowly scoped exception to the current session's prohibition on CI changes: in PR #20 only, add docs/design-revision-2.md to the normative class in scripts/ci-policy.json, immediately next to docs/design-revision-1.md.
This authorization covers only that path-classification entry. Do not change workflow logic, planner behavior, tests, suites, any other policy entry, or any unrelated CI configuration.
Treat the current failed PR run as valid fail-closed evidence and preserve it. Update #20's PR body so it no longer claims that CI policy is unchanged and so it accurately records why this classification entry is part of the PR.
Re-run the current local planner and the checks selected by the resulting policy, then read the new PR CI. Do not merge #20.
Because #20's exact head will change, update CodeSpace PR #80's immutable reference to the new #20 head before treating #80 as ready. Re-run/read #80 CI after that update and preserve its previous green run as superseded evidence rather than discarding it.
Make no other change under this authorization.
````

**B and C.** Question: The #20 problem doesn't affect packages B (#79 inventory, protocol and one bounded run) or C
(the D6 paper analysis). Should I continue with them while #20 is pending?

Owner's answer:

````text
Choose option 2: stop until #20 is resolved.
Do not continue to work packages B or C yet.
The approved continuation explicitly orders the work as A → B → C, and section 11 requires an immediate stop when a required check fails. PR #20's failed Plan / Required checks therefore leaves work package A incomplete.
First resolve the narrowly scoped #20 classification issue under the separate authorization just given, update #80 to the new immutable #20 head, re-run/read both PRs' CI, seal the Package A evidence, and complete the required #14/#76 tracker updates and Package A report.
Only after Package A is complete and no new stop condition exists should you proceed with Package B's frozen #79 protocol and single bounded rust-v0.154.0 run, followed by Package C.
Do not use the independence of B and C as a reason to bypass the explicit stop condition or reorder the approved work packages.
````

## 2026-09-30 15:31 UTC, session A: close package A

````text
Re-read Package A's current exact heads and CI. If all are successful, seal the evidence as planned, post and read-back the #14/#76 trackers, and then conclude Package A. Do not merge, and proceed to the approved Package B thereafter. Maintain the Section 12 principle of preserving the 0.154 baseline.
````

## 2026-10-01 05:43 UTC, session A: a pasted recommendation

The session read this as a recommendation, not an instruction. Two minutes later it asked the owner the questions
answered at 06:10, below.

````text
I recommend re-verifying the current `head` and `base`, reviewing the document contents, and then approving the merges in the order of **#20 → #80**. For **#21**, it is best to verify that the unresolved conditions and reasoning limitations for each candidate are accurately maintained, and then merge it separately as an analysis record. This ensures that the already settled external dependency policy is reflected in `main` and prevents subsequent agents from re-applying outdated restrictions.

Following that, while maintaining **CodeSpace at 0.154**, it is advisable to approve the solution comparison in **#79 step 3**, define the specific DevGuard consumption points and replacement target code within a bounded scope, and separately proceed with the **0.159.x scratch evaluation for Work package D**.

There is no need to postpone the independent investigation parts of both tasks until after the final D6 decision. However, actual dependency adoption, CodeSpace pin updates, and integrated implementations must each undergo their respective verifications and separate approvals before proceeding.

D6 and the carrier selection should be decided by reviewing both the comparison results from #79 and the remaining risks per candidate from #21. A "promising candidate on paper" must **not** be treated as an "implementation that satisfies N2."

By following this sequence, we can leverage the already secured 0.154 comparison data to progress with the next verification without unnecessary waiting, while simultaneously preventing DevGuard's preliminary evaluation from triggering premature CodeSpace pin changes or execution ownership transfers.
````

## 2026-10-01 06:10 UTC, session A: four answers (merge order, #79 step 3, the 0.159.x evaluation, inventory)

**Merges.** Question: Do you approve the exact-head merges in order? Before each one I'd check head and base, then
read the post-merge main CI and seal evidence before the next.

Owner's answer:

````text
Option 1: #20 → #80 → #21. Wrap up existing policy and record-keeping work. Merge #21 strictly as an analytical record, without treating its conclusions or candidates as implementation approval.
````

**#79 step 3.** Question: How should CodeSpace #79 step 3 (comparing remedies) proceed, with CodeSpace kept at 0.154?

Owner's answer:

````text
Approve Option 1 with modifications. Rather than concluding with comparative and bounded experiments as a standalone final deliverable, extend the scope of work to include submitting a CodeSpace PR that contains actual fixes and regression tests preserving behavioral semantics against baseline 0.154. Perform any necessary comparative experiments as verification for that implementation. If fixes cannot be made within the constraints, report only specific conflicts without arbitrarily changing pins or execution structures.
````

**WP-D.** Question: Should work package D, the 0.159.x scratch evaluation, start now, independent of D6?

Owner's answer:

````text
Option 3: Do not start as a standalone evaluation now. First determine DevGuard's actual consuming components and the code to be replaced, then include it as the necessary version comparison and verification within that component's implementation PR. This does not cancel the direction of adopting DevGuard early; it means avoiding repeating evaluations as isolated tasks without a consumer.
````

**Inventory.** Question: Should I write the bounded, paper-only inventory of DevGuard's Codex consumption points and
the DevGuard code each would replace, per revision 2 §4?

Owner's answer:

````text
Option 2: Do not create a separate documentation PR. Do not omit consumption points, replacement code, costs, or boundary descriptions, but include them directly in the design rationale and body of the actual implementation PR.

The completion criteria for the next task are not additional investigation documents or scratch results, but an implementation PR containing reviewable product code and regression tests. The CodeSpace #79 fix is counted as an independent product defect fix and must not be counted toward CS-RG milestone progress. To advance CS-RG itself, do not simply resume the previously suspended C00/C03/C09; instead, concisely define the first implementation unit and its completion criteria aligned with currently approved boundaries, and obtain separate approval for implementing that unit. Integrate necessary verifications directly into the implementation, and do not treat unresolved safety conditions as passed.
````

## 2026-10-02 08:38 UTC, session A: U1

````text
Implement an actual DevGuard status-only opt-in integration in CodeSpace, and submit an implementation PR that verifies dependency/pin boundaries, feature-off behavior, status/error mapping, credential non-leakage, and actual child descriptor non-inheritance alongside regression tests.
````

## 2026-10-02 14:13 UTC, session A: U1 hardening, then U2

````text
Close D-1 through D-5 first in one narrowly scoped U1-hardening implementation PR. Do not add registration, admission, launch, reaping, or pin changes. After that PR is merged and its post-merge CI is green, proceed with CSRG-U2 exactly as defined, including updating current milestone/tracker state to CS-RG: in-progress.
````

## 2026-10-03 04:01 UTC, session A: the #84 macOS timeout test

During this check the session found that a child process on macOS could die between `fork()` and `exec`. That defect
became [CodeSpace #86](https://github.com/novelKR/CodeSpace/pull/86) in session B.

````text
Check for #84 macOS timeout test
````

## 2026-10-03 05:44 UTC, session B: the #86 investigation

Both messages are long, so they are recorded separately in the [#86
instruction](2026-10-03-codespace-86-instruction.md).

## 2026-10-03 09:57 UTC, session B: the owner's eight-step order

````text
The sequence I have chosen is as follows:

1. Approve the exact-head merge for #86 at `3b1ed596…`

2. Verify post-merge main CI for #86

3. Update #84 against the new main and resolve the three conflicts

4. Re-validate #84 starting with the macOS test isolation removed

5. Merge U2 once CI is green on the new #84 exact head

6. Proceed with U3

7. Resolve #85 as a separate product fix/contract decision before U4 at the latest

8. Hold off on structural spawn redesigns (such as a trampoline) pending a separate owner decision

This order best aligns with our guiding principle so far: “Close verified product defects with actual code, but do not unnecessarily mix independent structural changes into CS-RG work.”
````

## Messages that were not instructions

- Four notes asking the session to continue after a usage limit reset: 2026-09-30 13:41, 2026-09-30 18:41, 2026-10-01
  10:41 and 2026-10-02 18:51 UTC.
- Auto-fix events and an `/auto-mode-setup` command that the desktop app recorded between 2026-09-30 14:02 and 14:20
  UTC.
- A request on 2026-10-03 at 15:09 UTC for a detailed report in Korean (session B).
