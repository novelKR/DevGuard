# Owner instruction: CodeSpace #81 hardening and completion (2026-10-01)

> **Status: dated record of an owner instruction; not maintained.** The owner gave this text in a Claude Code
> session's chat on 2026-10-01 at 08:11 UTC. It was not committed anywhere at the time. It is reproduced below
> unchanged, so every fact in it is as of that moment. It approves nothing beyond what it says, and later owner
> decisions may have superseded parts of it: the [CS-RG session handoff of
> 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what happened next and which parts still apply.

| Item | Detail |
| --- | --- |
| Given | In the same session, after the owner chose on 2026-10-01 to fix CodeSpace #79 in an implementation PR. |
| Covers | Hardening of [CodeSpace #81](https://github.com/novelKR/CodeSpace/pull/81), which keeps only the standard descriptors in spawned children: implementation hardening, regression tests, comparative verification against the `0.154` baseline, CI and review, and the completion and stop point. |
| Outcome | #81 was hardened to head `2629f1875a6ac16c32c23309e948af357f20003e`. |
| Still applies | Its rule against weakening a test or gate merely to obtain green CI. |

Copied from the session transcript byte for byte, except that the chat tool's wrapper around pasted text was removed.

## Original text

````text
CodeSpace #81 hardening and completion
Continue the existing CodeSpace #81 product-fix work. This is not a new investigation package and not a new architecture exercise.
The goal is to harden the existing implementation in PR #81, preserve the established CodeSpace 0.154 baseline and product semantics, add the missing regression coverage, and leave #81 at a new exact head that is ready for owner merge approval.
Do not merge #81 without a new explicit exact-head approval.
1. Cold start and authoritative state
Before changing anything:

1. Read the latest comments on:
   * `novelKR/CodeSpace#79`;
   * `novelKR/CodeSpace#76`;
   * `novelKR/DevGuard#14`.
2. Read CodeSpace PR #81 in full, including its current diff and CI.
3. Re-query:
   * CodeSpace `main`;
   * PR #81 head and base;
   * the CodeSpace Codex gitlink;
   * all current checks on #81.

Expected state at the time of this authorization:

* CodeSpace `main`: `1bf99cfc3c81e99ac4f8a4f2b1e5be59cdbb9429`;
* CodeSpace Codex gitlink: `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (`rust-v0.154.0`);
* PR #81: open, ready for review, current head `8afb7015e0d83e28fedd568ac7989858933ce8ca`.

If live state has moved in a way that materially affects the work, stop before modifying the branch and report the difference.
2. Objective and boundaries
The deliverable is the existing PR #81, strengthened in place.
Its product objective remains:
Children spawned by the CodeSpace runner host must not accidentally inherit unrelated descriptors held by CodeSpace or another execution.
Preserve all of the following:

* CodeSpace stays on Codex `rust-v0.154.0`;
* no Codex pin change;
* no new execution backend;
* no transfer of spawn, PTY, reaping, output, timeout, or lifecycle ownership;
* no DevGuard integration;
* no CS-RG implementation;
* no new external dependency unless a hard implementation necessity is found and reported rather than acted upon;
* the existing PTY path remains unchanged unless a concrete regression caused by #81 requires a narrowly scoped correction;
* this remains an independent CodeSpace correctness fix and must not be counted as CS-RG progress.

Do not create another research PR or a separate design document. Put implementation rationale, alternatives, limitations, and verification in #81 itself.
3. Required hardening of the implementation
Review and harden the current descriptor-exclusion implementation rather than assuming the current successful bounded run proves every fallback.
A. Make the fallback coverage claim true
The current fallback walk derives its range from the current descriptor limit.
Explicitly test the case where:

1. a high-numbered inheritable descriptor is already open;
2. the process descriptor limit is then lowered below that descriptor number;
3. the normal fast path is bypassed or forced to fail;
4. the fallback path is exercised.

The child must not inherit that descriptor.
Do not assume the current soft `RLIMIT_NOFILE` value necessarily covers every descriptor that is already open.
Choose the smallest implementation that gives the intended guarantee. The solution must remain safe in the post-fork/pre-exec environment.
If a complete fallback cannot be implemented without introducing a new spawner, changing process ownership, changing the Codex pin, or otherwise crossing the approved architectural boundary, stop and report that exact conflict rather than weakening the guarantee or silently narrowing the claim.
B. Distinguish a closed descriptor from an unexpected `fcntl` failure
Review `mark_close_on_exec`.
An invalid or already-closed descriptor may be ignored where appropriate, but an unexpected failure to inspect or mark an open descriptor must not silently become success.
Add deterministic tests for the error classification where feasible.
The failure policy must be conservative: if the child cannot establish the requested descriptor hygiene before exec, the spawn must not be reported as successfully prepared.
C. Re-review pre-exec safety
Review every operation reachable from the `pre_exec` closure on each supported platform.
For each platform-specific function used there, verify that the implementation does not depend on:

* heap allocation after fork;
* mutexes or process-global locks;
* Rust panics;
* non-reentrant runtime state;
* operations that are unsafe in the forked child of a multi-threaded process.

In particular, re-review the macOS `proc_pidinfo(PROC_PIDLISTFDS)` path and its fallback.
Do not retain an `async-signal-safe` or equivalent safety claim in the PR body merely because the function is a thin wrapper. The claim must be supported by the actual path being called.
If the existing macOS mechanism cannot support the required post-fork guarantee, replace it with the narrowest correct mechanism or stop and report the blocker.
D. Add continuous macOS regression coverage
The current PR changes macOS runner behaviour, but the current CodeSpace macOS CI selection does not run the relevant runner tests.
A narrow CI change is authorized in this work only to ensure the descriptor-hygiene regression tests that exercise the macOS implementation run in macOS CI.
Do not broaden unrelated CI coverage.
The resulting CI policy should make future changes to this implementation exercise the relevant macOS regression tests automatically.
E. Use precise behavioural language
Review the PR body and code comments.
Do not state that there is “no execution-structure change” if that wording hides the fact that installing `pre_exec` changes the underlying std/Tokio spawn mechanism from the `posix_spawn` fast path to fork/exec on affected platforms.
The intended statement is narrower:

* process ownership remains CodeSpace's;
* Tokio/std child management remains in place;
* reaping, process handles, pumps, timeouts, PTY ownership, and product lifecycle semantics remain unchanged;
* the low-level child creation mechanism may change because `pre_exec` is installed.

Likewise, do not claim that fork/exec has zero performance cost.
The current evidence supports only the bounded statement that the measured synthetic #79 workload showed no material change in total case wall time under the measured conditions; tail distributions and host load differed.
4. Regression tests
Keep the existing positive and negative controls.
At minimum, the final PR must cover:

* an unguarded child that demonstrates that the detector really sees an inheritable descriptor;
* a guarded child that excludes it;
* the actual InProcess Runner pipe path;
* the UDS worker command;
* preservation of spawn-error reporting for an invalid executable;
* the fallback path;
* the high-numbered-descriptor / lowered-limit fallback case;
* relevant macOS implementation behaviour;
* existing runner and server behavioural suites.

The tests must fail for the relevant defect when the protection under test is removed.
Do not substitute a source-level argument for a deterministic regression test where the behaviour can be tested.
5. Comparative verification
After the code reaches its intended final form:

1. commit the changes;
2. freeze the verification protocol before the comparative build;
3. rerun the established #79 comparison harness against the new exact head;
4. keep CodeSpace on Codex `6b9826e`;
5. compare against the sealed 0.154 baseline;
6. preserve both successful and failed attempts;
7. do not rewrite sealed evidence sets.

The comparison remains verification of the implementation. It is not a standalone deliverable.
Report observed results without converting bounded measurements into universal probabilities or platform-wide guarantees.
6. CI and review
Run the repository-selected checks for the final diff, including the newly applicable macOS runner regression coverage.
The final PR body must state:

* the actual defect;
* the exact implementation;
* why the chosen fix is smaller than the rejected alternatives;
* the change from `posix_spawn` fast-path eligibility to fork/exec where applicable;
* the fallback guarantee and its tests;
* the pre-exec safety basis;
* macOS and Linux verification;
* the comparative 0.154 results;
* known limitations;
* rollback.

If a required check fails, diagnose and fix the implementation or report the blocker. Do not weaken a test or gate merely to obtain green CI.
7. Completion and stop point
Update #79 and #76 with the new exact head and final verification.
Update DevGuard #14 only as a pointer/status delta if needed; do not describe #81 as CS-RG progress.
The completion criterion for this task is:
PR #81 at a new exact head, containing the corrected product implementation and regression tests, with all required CI and comparative verification complete.
Then stop and request exact-head merge approval.
Do not merge #81 in this authorization.
Do not start CSRG-U1, a Codex 0.159 evaluation, D6 implementation, carrier implementation, or any additional investigation package as part of this task.
````
