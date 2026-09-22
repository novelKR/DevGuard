# Operating the DevGuard service boundary

[English](operations.md) | [한국어](ko/operations.md)

DG1-C01 provides explicit bootstrap, canonical paths and configuration/storage checks. Runtime execution remains closed: native host observations, authenticated transport and launch/reconciliation are separate implementation units. The normal authority is currently macOS-only; Linux CI checks portable contracts and bounded fixtures, not DG-LINUX controls.

## Available commands

Build with Rust 1.95.0 using one Cargo job during bootstrap:

```sh
CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-daemon
target/debug/devguardd paths
target/debug/devguardd init
target/debug/devguardd check
```

`paths` only observes the operating account. `init` is an explicit first-time bootstrap, creating a new journal, operator configuration and separate CLI/administrative credentials. It refuses existing state and never overwrites it. `check` obtains exclusive storage ownership, validates configuration/journal and reports `runtime_ready: false`; it does not fabricate a boot clock or claim recovery/application. It cannot acquire a second authority while another owner holds the lock.

`serve`, generic execution, installation, a LaunchAgent and repair become available through subsequent work. Configuration alone does not govern commands. Necessary bootstrap builds are not self-use evidence.

## Canonical ownership and paths

Normal paths derive from the OS account database, independent of caller `HOME`, XDG, socket or state overrides. Root/setuid execution is refused. There is no production alternate-path or unparented test-budget argument. Candidate paths require the later C10 parent-budget protocol; only compiled test fixtures can construct isolated fixture roots.

| Purpose | macOS path |
| --- | --- |
| Operator configuration | `~/.config/devguard/host.toml` |
| Persistent authority | `~/Library/Application Support/DevGuard/state/authority.sqlite` |
| Persistent exclusive lock | `~/Library/Application Support/DevGuard/state/authority.lock` |
| Registration/admin credentials | Separate files under `~/Library/Application Support/DevGuard/credentials/` |
| Runtime endpoint | `/private/tmp/devguard-<uid>/authority.sock` (transport supplied in C02) |
| Future cache | `~/Library/Caches/DevGuard/` |

DevGuard directories are private (0700), files 0600, with owner/type and symlink checks. Parent traversal, unsafe ancestors, linked private files and shared permissions fail closed; the service does not silently chmod user paths. Shared sticky tmp is permitted only as an ancestor of the private runtime directory. The journal and lock remain outside tmp.

Ordinary startup requires existing persistent directories, journal and lock. Missing/corrupt/unsupported state is never initialized implicitly. If explicit bootstrap partially fails, preserve partial files for diagnosis and explicit repair; rerunning init is not repair. Do not remove an active lock, journal, credential or recovery artifact to make startup succeed.

## Configuration authority

The bounded UTF-8 TOML operator file is schema 1 and rejects unknown fields. It defines policy revision, the interactive profile, consumer generations/credential digests/roles/instance limits, static control reservations and registered project roots. Plaintext credentials are stored only in their separate private files. Administrative and registration credentials must differ; a workload role cannot grant itself a control reservation. Parsing errors never echo source text or secrets.

The initial `dev-cli` registration allows eight instances and adds no per-instance control reservation: the approved aggregate CLI control pool is counted centrally when native accounting becomes available. The task fields start at an **accounting estimate** of capacity 256, host headroom 64 and system reservation 16. These are configurable operator ceilings, not macOS kernel limits or measured sufficiency; C12 must record and qualify the selected values. CPU/memory headroom and control defaults retain the approved design and will use actual native capacity in C03. Additional headroom can only subtract capacity.

A project `.devguard.toml` has only schema, project ID, profile, adapter selection and optional tighter `Budget` limits (`cpu_milli`, `memory_bytes`, `tasks`). It cannot specify credentials, roles, another authority, a consumer identity or host capacity. The project limit must fit the operator limit; unsupported fields/adapters/versions are refused. No project-specific settings enter authority core.

```toml
schema = 1
project_id = "devguard-dev"
profile = "interactive"
adapter = "cargo"

[limits]
cpu_milli = 1000
memory_bytes = 2147483648
tasks = 32
```

This is a configuration example, not evidence that execution already consumes a lease. Register actual absolute project roots in operator configuration before later CLI adoption. Projects and credentials do not create another host budget.

## Compatibility, checks and rollback

Core `AuthorityStorage` holds the original exclusive lock and validates the existing schema-1 journal without changing attempt state. Activation with a real `Backend`/`Clock` revalidates accounting in the same transaction as boot-aware recovery. Existing `Authority::open` preserves its recovery behavior and the DG-0 tests. No journal or contract wire format changes in C01.

Available checks:

```sh
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/validate.py --offline
```

The functional suite rejects zero executed cases and records source fingerprints, toolchain, bootstrap mode and logs. It checks exclusive startup, aliases/permissions, absent/corrupt/future journals, activation-time corruption, strict configuration versions, project escalation and concurrent bootstrap. It leaves native launch, Linux enforcement, self-use and foreground SLO `not_run`. The full validator retains the 44 original tests and extends the explicit workspace dependency graph for the daemon/TOML layer; core and contract remain independent of service/configuration dependencies.

To roll back this boundary, stop its task-owned foreground process and select the prior compatible source/artifact, preserving persistent state and credentials. No workloads can have started through C01. Later live-lease rollback requires actual reconciliation; it cannot use this early empty-state assumption.
