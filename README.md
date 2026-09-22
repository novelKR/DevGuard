# DevGuard

DevGuard centralizes resource admission for development workloads while preserving the resources needed to inspect and stop them.

The repository currently implements **DG-0: contracts and a durable authority core**. It does not yet install a daemon, launch user commands, apply macOS policies, or enforce Linux cgroups. The CLI examples in the approved design describe DG-1 and later work.

- [Approved independent design (Korean)](docs/design.ko.md)
- [Implemented contracts and trust boundaries](docs/contracts.md)
- [Milestones and the CodeSpace dependency path](docs/milestones.md)
- [Detailed execution plans, adoption gates and PR delivery (Korean)](docs/planning/README.md)
- [Machine-readable milestone state](milestones.json)

## Validate DG-0

Use Rust **1.95.0**, including rustfmt and Clippy, and Python 3.11 or newer. A rustup installation honors `rust-toolchain.toml`. A standalone toolchain can be placed first in `PATH`. The validator checks the compiler it actually executes.

```sh
python3 scripts/validate.py --offline
```

Omit `--offline` when the locked crates have not been downloaded. Builds and tests use one Cargo job and one test thread by default. Results, source fingerprints and logs are written under `target/qualification/`. A newer compiler can be used with `--allow-toolchain-mismatch` for a supplemental check, which never counts as qualification for 1.95.0.

DG-0 tests use an explicitly fake OS backend. A passing report establishes the tested accounting, persistence and state-transition contracts. macOS launch, Linux enforcement, browser responsiveness and self-governed execution remain `not_run`.

## Repository boundaries

`devguard-contract` provides transport-independent resource types, execution identities, evidence and compatibility requirements. `devguard-core` provides static accounting, authenticated consumer registration against trusted peer observations, durable admission, fenced launch transitions, pressure policy and evidence-based reconciliation. It starts no processes and contains no CodeSpace or Codex dependency.

The approved local checkout is `/Volumes/DevData/Projects/IdeaProjects/DevGuard`. Existing `.codex` settings are preserved locally and ignored by Git. Build output, journals, qualification evidence and local toolchains are also ignored. The approved design is preserved byte-for-byte; its checksum is recorded in `docs/design-source.json`.

The public source repository is [novelKR/DevGuard](https://github.com/novelKR/DevGuard). CodeSpace runtime consumption begins at CS-RG after DG-1 qualification. The foundation does not publish crates or install a running host service.

The detailed plan defines 46 proposed implementation commit units in 23 logical PR groups. It records single registration by the execution-owning Runner and opt-in Gateway restart recovery while an independent Runner remains alive. Planning completion does not change runtime milestone status. See the [consumer readiness gates](docs/planning/consumer-readiness.md), [CodeSpace mapping](docs/planning/codespace-integration.md), and [verification and evidence rules](docs/planning/verification.md).

## License

Apache License 2.0, matching CodeSpace. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Dependency crates retain their own licenses.
