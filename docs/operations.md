# Operating the DevGuard service boundary

[English](operations.md) | [한국어](ko/operations.md)

C01 provides explicit bootstrap/canonical storage and C02 provides authenticated local transport. PR and post-merge main delivery evidence is tracked separately from implementation. `devguardd serve` runs a foreground service; native registration, principals, resource leases and execution remain closed until P2/P3 provide actual host identity/probes and launch/reconciliation. The normal authority is currently macOS-only; Linux CI checks portable contracts and bounded fixtures, not DG-LINUX controls.

## Available commands

Build with Rust 1.95.0 using one Cargo job during bootstrap:

```sh
CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-daemon
target/debug/devguardd paths
target/debug/devguardd init
target/debug/devguardd check
target/debug/devguardd serve
```

`paths` only observes the operating account. `init` is an explicit first-time bootstrap, creating a new journal, operator configuration and separate CLI/administrative credentials. It refuses existing state and never overwrites it. `check` obtains exclusive storage ownership, validates configuration/journal and reports `runtime_ready: false`; it does not fabricate a boot clock or claim recovery/application. It cannot acquire a second authority while another owner holds the lock.

`serve` opens the existing validated journal, acquires exclusive ownership and binds the canonical private UDS endpoint. Stop this foreground process with Ctrl-C or SIGTERM; shutdown closes sessions and removes only its own socket inode while preserving the journal, lock and credentials. A stale socket is removed only under the exclusive authority lock after a connection attempt positively returns connection refused and the inode remains unchanged. A live, busy or unobservable endpoint is not removed.

Authenticated status reports `storage_validated: true`, `registration_ready: false`, `execution_ready: false`, a reason and the configuration fingerprint. Generic execution, installation, a LaunchAgent and repair remain future work. Configuration or a successful handshake alone does not govern commands. Necessary bootstrap builds are not self-use evidence. Do not use the disposable `target` path for a persistent service; protected installation is C09 work.

## Canonical ownership and paths

Normal paths derive from the OS account database, independent of caller `HOME`, XDG, socket or state overrides. Root/setuid execution is refused. There is no production alternate-path or unparented test-budget argument. Candidate paths require the later C10 parent-budget protocol; only compiled test fixtures can construct isolated fixture roots.

| Purpose | macOS path |
| --- | --- |
| Operator configuration | `~/.config/devguard/host.toml` |
| Persistent authority | `~/Library/Application Support/DevGuard/state/authority.sqlite` |
| Persistent exclusive lock | `~/Library/Application Support/DevGuard/state/authority.lock` |
| Registration/admin credentials | Separate files under `~/Library/Application Support/DevGuard/credentials/` |
| Runtime endpoint | `/private/tmp/devguard-<uid>/authority.sock` |
| Future cache | `~/Library/Caches/DevGuard/` |

DevGuard directories are private (0700), files 0600, with owner/type and symlink checks. Parent traversal, unsafe ancestors, linked private files and shared permissions fail closed; the service does not silently chmod user paths. Shared sticky tmp is permitted only as an ancestor of the private runtime directory. The journal and lock remain outside tmp.

Lock opening uses `O_NONBLOCK`; after opening, descriptor metadata must confirm a current-UID private regular file with one link, including during explicit initialization. FIFO and hard-link fixtures must fail without blocking or granting authority ownership.

Ordinary startup requires existing persistent directories, journal and lock. Missing/corrupt/unsupported state is never initialized implicitly. If explicit bootstrap partially fails, preserve partial files for diagnosis and explicit repair; rerunning init is not repair. Do not remove an active lock, journal, credential or recovery artifact to make startup succeed.

## Configuration authority

The bounded UTF-8 TOML operator file is schema 1 and rejects unknown fields. It defines policy revision, the interactive profile, consumer generations/credential digests/roles/instance limits, static control reservations and registered project roots. Plaintext credentials are stored only in their separate private files. All consumer and administrative credential digests must differ; a workload role cannot grant itself a control reservation. Parsing errors never echo source text or secrets.

The initial `dev-cli` registration allows eight instances and adds no per-instance control reservation: the approved aggregate CLI control pool is counted centrally when native accounting becomes available. The task fields start at an **accounting estimate** of capacity 256, host headroom 64 and system reservation 48. `system_tasks` must be at least 48, covering 32 bounded session workers plus 16 tasks of service/control headroom. These are configurable operator ceilings, not macOS kernel limits or measured sufficiency; C12 must record and qualify the selected values. CPU/memory headroom and control defaults retain the approved design and will use actual native capacity in C03. Additional headroom can only subtract capacity.

C01 configurations that used the former `system_tasks = 16` default are rejected by C02. Configuration schema remains 1; this is a stricter semantic requirement, not automatic compatibility or migration. Before starting C02, an operator must review total task capacity, headroom and all reservations, then explicitly choose `system_tasks >= 48` within that capacity. Preserve the existing journal and credentials. Do not rerun `init`, silently enlarge host capacity or reduce a live reservation to make validation pass.

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

## Client authentication and transport limits

The client library connects to the canonical endpoint, checks the OS-observed authority UID/PID, negotiates wire version 1 and corroborates its own UID/PID in the handshake. Consumer ID/generation/secret authenticate a configured workload or control-service role; a separate secret authenticates the administrative role. UID equality alone grants no role. Helper permits cannot be used as caller credentials, and this authentication does not isolate malicious processes sharing the same UID.

The wire uses a four-byte length prefix, at most 64 KiB per JSON payload, at most 32 active sessions and an absolute 250 ms deadline for each frame read/write. That deadline includes idle waiting before a new frame, so idle connections expire. Use a fresh authenticated session when explicitly beginning a later operation; the client does not automatically reconnect or retry. Its `poll` and nonblocking descriptor I/O handle partial frames, slow readers and buffered final responses without changing Darwin timeout options after peer closure. No response loss is treated as proof of execution, nonexecution or resource release.

The private-FD API passes only a descriptor identifier through startup metadata. The receiver consumes and closes the credential FD before a later `exec`; tests exercise that boundary in real subprocesses. Do not put secrets in argv, environment, debugging or payload-inherited descriptors. These tests do not establish C05 helper authorization or payload startup. OS UID/PID observations are available now; native boot/start identity and instance registration require C03. No authenticated caller can obtain a principal, lease or execution grant from C02.

The SDK inspection example can be built with `CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-client --example inspect`. Its interface is `inspect SOCKET UID CONSUMER GENERATION CREDENTIAL_FD`: a parent must supply the secret bytes through that dedicated inherited FD, while arguments carry only the FD number and non-secret connection metadata. It prints observed peers and authenticated status. It is a status example, not the future generic execution CLI.

## Compatibility, checks and rollback

Core `AuthorityStorage` holds the original exclusive lock and validates the existing schema-1 journal without changing attempt state. Activation with a real `Backend`/`Clock` revalidates accounting in the same transaction as boot-aware recovery. Existing `Authority::open` preserves its recovery behavior and the DG-0 tests. C01/C02 do not change the journal or existing contract serialization. The new local protocol strictly rejects unknown fields, versions and unsupported required capabilities; added fields need explicit compatibility tests.

Available checks:

```sh
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/qualify.py dg1-auth --offline
python3 scripts/validate.py --offline
```

Functional suites reject zero executed cases and record source fingerprints, toolchain, bootstrap mode and logs. `dg1-authority` checks exclusive startup, aliases/permissions, absent/corrupt/future journals, activation-time corruption, strict configuration versions, project escalation and concurrent bootstrap. `dg1-auth` checks actual peer observations, authentication roles, strict frames, bounded communication and private FD hygiene, including concurrent requests whose registration remains closed. These leave native registration/launch, Linux enforcement, self-use and foreground SLO `not_run`. The full validator retains the 44 original tests and validates all four workspace crates and their explicit dependency graph. Core/contract remain independent of daemon configuration, and client does not depend on core.

To roll back this boundary, stop its task-owned foreground process and select a source/artifact compatible with the preserved configuration and schema-1 journal. Keep persistent state and credentials. No workloads can have started through C01/C02. Later live-lease rollback requires actual reconciliation; it cannot use this early empty-state assumption.
