# Owner instruction: CodeSpace #81 closeout (2026-10-02)

> **Status: dated record of an owner instruction; not maintained.** The owner gave this text in a Claude Code
> session's chat on 2026-10-02 at 06:41 UTC. It was not committed anywhere at the time. It is reproduced below
> unchanged, so every fact in it is as of that moment. It approves nothing beyond what it says, and later owner
> decisions may have superseded parts of it: the [CS-RG session handoff of
> 2026-10-03](2026-10-03-cs-rg-session-handoff.md) says what happened next and which parts still apply.

| Item | Detail |
| --- | --- |
| Given | In the same session, once #81's macOS CI had completed. |
| Covers | Six closing steps for #81 (re-verify head and base, seal the macOS CI evidence, replace the PR body, update #79 and #76 and, if needed, #14, read back the posted body, then request exact-head merge approval and stop) and recommendations for the work after it. |
| Outcome | The owner approved the exact head. #81 merged on 2026-10-02 at 07:59 UTC as `dcf8c51`. |
| Still applies | Its recommendation not to mix the `0.159.x` evaluation, D6 implementation or carrier implementation into this sequence, but to do each inside the implementation PR that needs it. |

Copied from the session transcript byte for byte, except that the chat tool's wrapper around pasted text was removed.

## Original text

````text
Since the awaited macOS CI has now finished, execute the remaining items below precisely:

1. Re-verify that the current `2629f187…` head and base remain unchanged.

2. Collect the completed macOS CI run into final evidence and seal it.

3. Completely replace the body of #81 based on the current code, fallback guarantee, error policy, pre-exec safety rationale, macOS CI, second 0.154 comparison, and performance bounds.

4. Update #79 and #76 to the new head, and if necessary, leave only a pointer/status delta on #14.

5. Read back the posted body to verify that it matches exactly.

6. In that state, request exact-head merge approval for `2629f1875a6ac16c32c23309e948af357f20003e` and stop.

If there are no additional code changes during this process, there is no need to run the comparison harness again. Only if the execution semantics of the code or CI change should the necessary verification be rerun for the new head.

Recommendations for Future Work
At this point, it is advisable not to expand #81 any further. This is because production code, regression tests, macOS continuous coverage, Linux CI, and bounded 0.154 comparative verification that satisfy the hardening scope of the previous directive already exist. If we start seeking out and tacking on new issues now, it is highly likely to regress into a "review process that never finishes."

Therefore, the appropriate next action is to resume the existing work as previously instructed and execute only the close-out. After reading the current state indicating that macOS CI has succeeded, carry out the 6 steps above and stop at the exact-head approval request.

Following that, if the owner review approves `2629f187…`, it is recommended to merge #81 as an exact-head merge and verify only the post-merge main CI to conclude this standalone CodeSpace defect fix.

Once the work above is completed, I recommend that the final report stage ask me whether to approve starting the CSRG-U1 implementation next. The rationale is that CS-RG is no longer blocked by #81, and CSRG-U1 serves as the first actual consumption point of the DevGuard generic client—making it the ideal initial task to transition the milestone itself from "not-started" to the active implementation phase. However, for the upcoming U1, it is best not to include admission, launch, permits, carrier, reaping, or Codex pin changes, and instead constrain it strictly to the already defined status-only opt-in slice.

To summarize, the recommended sequence of work is straightforward:

#81 close-out → exact-head review/merge → post-merge CI verification → separate approval request and request for approval to begin CSRG-U1 implementation.

I intend not to mix 0.159.x evaluation, D6 implementation, and carrier implementation into this sequence. Performing each of those within its respective implementation PR as needed—once actual consumption points and launch boundaries materialize after U1—best aligns with the principle I have established: "an implementation PR, not investigation itself, is the definition of done."
````
