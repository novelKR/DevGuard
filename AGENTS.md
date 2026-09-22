# DevGuard development

Read `docs/design.ko.md`, `docs/contracts.md`, and `milestones.json` before changing a contract or milestone boundary. The user's current instructions take precedence over this file.

- Keep CodeSpace/Codex types, process ownership, PTY and output management outside this repository's authority core.
- Keep reservation, planned policy, applied evidence, and successful user executable startup distinct.
- Preserve attempt identity and terminal tombstones. Never turn timeout, disconnect, root reap or a missing scope into proof of whole-workload termination.
- OS evidence is trusted only through the Backend boundary. Tests using the fake backend do not qualify OS enforcement or responsiveness.
- Preserve the local `.codex` directory; do not commit credentials, journals, toolchains, runtime artifacts or test evidence.
- Use `python3 scripts/validate.py` with Rust 1.95.0. During bootstrap use one Cargo job and one test thread. Once DG-1 has qualified a stable executable, use that stable version to govern candidate development.
- Update implementation state separately from platform qualification. Keep DG-LINUX required, and keep cache and additional adapters off the P1-RECOVERY prerequisite path.
- Keep the approved design intact. Record implementation clarifications in `docs/contracts.md` and the milestone ledger.
