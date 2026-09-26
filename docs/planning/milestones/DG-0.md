# DG-0 — Historical foundation record

Owner: DevGuard. Implementation: `implemented`; qualification: exact-source report required. This records one actual initial commit rather than inventing historical work commits or PRs.

## Actual history and scope

Commit [`d59cbd43d206a9a9281328a946eddf1dc199f710`](https://github.com/novelKR/DevGuard/commit/d59cbd43d206a9a9281328a946eddf1dc199f710), `feat: establish DG-0 resource contracts and durable authority`, supplied all records below. DG0-R IDs are historical classifications, excluded from the 48 proposed runtime units.

| Record | Actual deliverable | Behavior/invariant | Normal/failure/race coverage |
| --- | --- | --- | --- |
| DG0-R01 | contract/src/lib.rs and public_contract tests | Separate intent/reservation/plan/applied, meaning digest, identity and compatibility | Deterministic digest, overflow/unknown fields and invalid levels |
| DG0-R02 | core/policy.rs and authority.rs | Subtract static reservations; refuse non-fitting work | Concurrent sums, duplicate attempts, zero capacity, policy shrink preserves leases |
| DG0-R03 | Authority registration/generations | Trusted peer, consumer secret and exact PID; shared slots | UID-only refusal, reconnect, PID reuse, active generation mutation rejected |
| DG0-R04 | journal.rs and authority.rs | Durable attempts/launch fence/tombstones; restart Suspect | Lost replies, commit/cancel races, journal/lock/accounting-index failures |
| DG0-R05 | pressure.rs and pressure_contract tests | Closed until valid probe, pressure escalation and stepwise recovery | Stale/future/reboot samples, observation errors, 30-second recovery and disk watermarks |
| DG0-R06 | Validator, CI, design/contracts/ledger | Exact source/toolchain/dependency evidence | Checksum, graph, fmt/Clippy, 44 tests; runtime remains not_run |

Entry required approved design, Apache-2.0 repository, Rust 1.95.0 and Python 3.11+. The baseline has two crates: contract/core. Backend is the trusted evidence interface; tests use a fake. Transport authentication, daemon/helper/CLI, actual macOS policy, real cgroups, self-use and CodeSpace runtime were not implemented in this commit.

## Verified evidence and reproduction

[Initial macOS/Ubuntu CI](https://github.com/novelKR/DevGuard/actions/runs/35671367559) passed 44 tests on both platforms: seven public-contract, 30 authority and seven pressure tests. Ubuntu fake-backend success is not kernel-controller qualification.

Protected local reports: `evidence/dg0-independent-d59cbd4/report.json`, `evidence/ci-35671367559/dg0-macos-14-1/report.json`, `evidence/ci-35671367559/dg0-ubuntu-24.04-1/report.json`. These ignored paths are not assumed publicly distributed. Correlate CI run/artifact with report source/head, tree digest, tests_passed and toolchain; rerun the same source if hosted artifacts expire.

Available command: `python3 scripts/validate.py --offline`, with Rust 1.95.0/rustfmt/Clippy and Python 3.11+. Remove offline only to download locked dependencies. Bootstrap uses one Cargo job/one test thread and a new ignored report path. `--allow-toolchain-mismatch` is supplemental, not pinned qualification.

## Completion, rollback and handoff

Completion evidence preserves checksum/license/graph, passes fmt/Clippy and the 44-test baseline, and accurately leaves runtime scopes not_run. Rerunning on a later docs commit proves that head's regression, not new OS qualification or revised historical CI.

At this pre-consumer baseline rollback is source comparison/reproduction in a new checkout, with isolated test journals. Never later overwrite operational journals with baseline state. DG1-C01 receives contracts, fixtures and validator plus explicit native gaps. Every added crate requires a same-PR expansion of the explicit workspace allowlist while retaining full-graph checks.
