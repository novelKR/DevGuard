# DevGuard

DevGuard centralizes resource admission for development workloads while preserving the resources needed to inspect and stop them.

The repository implements **DG-0 contracts and a durable authority core**, with DG-1 now in progress. C01 supplies canonical paths/bootstrap/storage checks; C02 supplies authenticated, bounded local communication with a small client and private credential-FD transfer; C03 supplies the native macOS boot clock, process identity and host pressure evidence that activate the service's journal; C04 supplies cooperative QoS/nice application with readback and observed process-group scope evidence; C05 supplies registration over the wire and the fenced `devguard-launch` helper; C06 supplies reconciliation, and with native evidence the service opens all three; C07 supplies the `devguard` command-line owner with doctor diagnostics, receipts and explicit bounded waits; C08 supplies the Cargo adapters, which fit compiler jobs to the reservation and share one jobserver; C09 installs a packaged release as the current user's LaunchAgent, selected only after launchd is verified to run it. PR and post-merge main delivery evidence is tracked separately from implementation. Linux cgroups are not yet available. Use the operating guide for actual command availability; the design also contains future interfaces.

- [Authoritative design reference](docs/design.md) · [Korean translation](docs/ko/design.md)
- [Historical approved design (Korean, immutable)](docs/design.ko.md)
- [Implemented contracts and trust boundaries](docs/contracts.md)
- [Service boundary operations](docs/operations.md)
- [Milestones and the CodeSpace dependency path](docs/milestones.md)
- [Detailed execution plans, adoption gates and PR delivery](docs/planning/README.md)
- [Machine-readable milestone state](milestones.json)

## Validate the current implementation

Use Rust **1.95.0**, including rustfmt and Clippy, and Python 3.11 or newer. A rustup installation honors `rust-toolchain.toml`. A standalone toolchain can be placed first in `PATH`. The validator checks the compiler it actually executes.

```sh
python3 scripts/validate.py --offline
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/qualify.py dg1-auth --offline
python3 scripts/qualify.py dg1-probes --offline   # macOS only
python3 scripts/qualify.py dg1-scopes --offline   # macOS only
python3 scripts/qualify.py dg1-launch --offline   # macOS only
python3 scripts/qualify.py dg1-reconcile --offline   # macOS only
python3 scripts/qualify.py dg1-cli --offline   # macOS only
python3 scripts/qualify.py dg1-cargo --offline   # macOS only
python3 scripts/qualify.py dg1-bootstrap --offline   # macOS only
```

Omit `--offline` when the locked crates have not been downloaded. Builds and tests use one Cargo job and one test thread by default. Results, source fingerprints and logs are written under `target/qualification/`. A newer compiler can be used with `--allow-toolchain-mismatch` for a supplemental check, which never counts as qualification for 1.95.0.

DG-0 tests use an explicitly fake OS backend. Their passing reports establish accounting, persistence and state-transition contracts. The additional C01–C09 suites check actual local storage, peer observations, credential transport, native macOS host evidence, cooperative scope evidence, the launch helper, reconciliation, the command-line owner, the Cargo adapters and installation within bounded fixtures. They do not qualify Linux enforcement, browser responsiveness or self-governed execution; these remain `not_run`.

## Repository boundaries

`devguard-contract` provides transport-independent resource types, execution identities, evidence and compatibility requirements. `devguard-core` provides static accounting, authenticated consumer registration against trusted peer observations, durable admission, fenced launch transitions, pressure policy and evidence-based reconciliation. It starts no processes and contains no CodeSpace or Codex dependency.

The approved local checkout is `/Volumes/DevData/Projects/IdeaProjects/DevGuard`. Existing `.codex` settings are preserved locally and ignored by Git. Build output, journals, qualification evidence and local toolchains are also ignored. The approved design is preserved byte-for-byte; its checksum is recorded in `docs/design-source.json`.

`devguard-macos` supplies native boot, process, host pressure and process-group scope observations to the core only through its `Clock` and `Backend` traits. `devguard-daemon` provides the canonical configuration/storage boundary and foreground `devguardd serve`. `devguard-client` supplies versioned UDS communication and private credential handoff without depending on the authority core. `devguard-launch` is the fenced helper between a launch grant and the user's executable. `devguard-cli` provides `devguard`, the command-line owner that runs commands only through the authority. `devguard-cargo` fits Cargo's compiler jobs to a reservation and shares one jobserver across nested Cargo runs. Successful authentication is not an instance registration or a workload lease, and does not isolate malicious processes sharing the operating UID.

The public source repository is [novelKR/DevGuard](https://github.com/novelKR/DevGuard). CodeSpace runtime consumption begins at CS-RG after DG-1 qualification. Crates are not published, and no installer or LaunchAgent is available yet.

The detailed plan defines 46 proposed implementation commit units in 23 logical PR groups. It records single registration by the execution-owning Runner and opt-in Gateway restart recovery while an independent Runner remains alive. Planning completion does not change runtime milestone status. See the [consumer readiness gates](docs/planning/consumer-readiness.md), [CodeSpace mapping](docs/planning/codespace-integration.md), and [verification and evidence rules](docs/planning/verification.md).

English is the editorial source for maintained design/planning documents. See the [translation registry](docs/translations.json); `python3 scripts/check_docs.py` validates reviewed hashes and planning references. DG-1 delivery is sequential through normal merge/main CI and cleanup. Bounded real self-use begins at C10 after freezing a functionally tested parent containing parent-budget support; C12 separately qualifies and promotes the measured release.

## License

Apache License 2.0, matching CodeSpace. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Dependency crates retain their own licenses.
