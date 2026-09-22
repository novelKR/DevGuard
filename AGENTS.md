# DevGuard development

Read the English editorial reference `docs/design.md`, the immutable approval `docs/design.ko.md`, `docs/contracts.md`, and `milestones.json` before changing a contract or milestone boundary. The user's current instructions take precedence over this file.

- Keep CodeSpace/Codex types, process ownership, PTY and output management outside this repository's authority core.
- Keep reservation, planned policy, applied evidence, and successful user executable startup distinct.
- Preserve attempt identity and terminal tombstones. Never turn timeout, disconnect, root reap or a missing scope into proof of whole-workload termination.
- OS evidence is trusted only through the Backend boundary. Tests using the fake backend do not qualify OS enforcement or responsiveness.
- Preserve the local `.codex` directory; do not commit credentials, journals, toolchains, runtime artifacts or test evidence.
- Use `python3 scripts/validate.py` with Rust 1.95.0. During bootstrap use one Cargo job and one test thread. At C10, first validate and freeze a parent artifact containing parent-budget capability, then immediately govern applicable candidate builds/tests with it. Preserve the P4 functional bundle outside target; functional reference and C12 SLO-qualified release are distinct.
- Update implementation state separately from platform qualification. Keep DG-LINUX required, and keep cache and additional adapters off the P1-RECOVERY prerequisite path.
- Keep the approved design intact. Record implementation clarifications in `docs/contracts.md` and the milestone ledger.

- Write PR bodies and authoritative documentation in English; review maintained Korean counterparts and update only their corresponding hashes using `scripts/check_docs.py record --id <id>`.
- Deliver DG1-P1 through P6 sequentially: review/current-head checks, normal exact-head merge, separate main CI, evidence preservation and cleanup before the next PR. Keep remote branches and protected recovery artifacts.
- Use foreground daemons through P4 and a current-user LaunchAgent from P5. Ordinary restart must reopen/reconcile state and fail closed on missing or corrupt journals.
