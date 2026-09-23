# Operating the DevGuard service boundary

[English](operations.md) | [한국어](ko/operations.md)

C01 provides explicit bootstrap/canonical storage, C02 authenticated local transport, C03 native macOS boot, process and host pressure evidence, C04 cooperative policy readback and scope evidence, and C05 the fenced launch helper. PR and post-merge main delivery evidence is tracked separately from implementation. `devguardd serve` runs a foreground service that activates the journal with that evidence. Registration over the wire, principals, resource leases and execution remain closed in the normal service until DG1-C06 ships reconciliation with launch. The normal authority is currently macOS-only; Linux CI checks portable contracts and bounded fixtures, not DG-LINUX controls.

## Available commands

Build with Rust 1.95.0 using one Cargo job during bootstrap:

```sh
CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-daemon -p devguard-launch
target/debug/devguardd paths
target/debug/devguardd init
target/debug/devguardd check
target/debug/devguardd serve
```

`paths` only observes the operating account. `init` is an explicit first-time bootstrap, creating a new journal, operator configuration and separate CLI/administrative credentials. It refuses existing state and never overwrites it. `check` obtains exclusive storage ownership, validates configuration/journal and reports `runtime_ready: false`; it does not read a boot clock or claim recovery/application. It cannot acquire a second authority while another owner holds the lock.

`serve` opens the existing validated journal, acquires exclusive ownership and binds the canonical private UDS endpoint. Stop this foreground process with Ctrl-C or SIGTERM; shutdown closes sessions and removes only its own socket inode while preserving the journal, lock and credentials. A stale socket is removed only under the exclusive authority lock after a connection attempt positively returns connection refused and the inode remains unchanged. A live, busy or unobservable endpoint is not removed.

When native evidence is available, `serve` derives the policy from the observed host and runs the journal's boot-aware recovery with the real clock and process identities. It then samples host pressure every two seconds (see [contracts](contracts.md#native-macos-host-evidence)).
- **Sampled volumes.** These are the state volume and every registered project root. A registered root that is missing or unreadable fails every reading and keeps new work closed. Correct or remove a stale registration; it is not skipped.
- **Startup state.** Pressure starts Critical, and the first valid sample needs two readings. If the host reports memory warning when the service starts, the pressure rules keep the state Critical until memory has been normal for 30 seconds. It then becomes Constrained, and Normal after another 30 seconds.
- **Receipts.** The service writes JSON lines to stderr:
  - `native_host` at activation, or `native_host_unavailable` when native evidence cannot be opened
  - `pressure_baseline`
  - `pressure` on each state change and once a minute
  - `pressure_sample_rejected` when the controller refuses a sample
  - `pressure_control_lag` when the loop wakes at least 200 ms late without a state change
  - `pressure_observation_failed` for the first failed reading and once a minute while failures continue
  - `pressure_stopped` if the sampler ends abnormally

  Receipts contain host counters, pressure states, the state and registered project paths, and their mount points. They never contain credentials.

On a platform without native evidence, or when the macOS observation fails, `serve` keeps the storage-only closed behavior and reports which case applies.

Authenticated status reports `storage_validated: true`, `registration_ready: false`, `execution_ready: false`, a reason and the configuration fingerprint. The reason distinguishes active native evidence, an unsupported platform and a failed native observation. `devguard-launch` is built for the launch path, but the normal service does not open that path in C05. Generic execution, installation, a LaunchAgent and repair remain future work. Configuration or a successful handshake alone does not govern commands. Necessary bootstrap builds are not self-use evidence. Do not use the disposable `target` path for a persistent service; protected installation is C09 work.

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

The initial `dev-cli` registration allows eight instances and adds no per-instance control reservation: the approved aggregate CLI control pool is counted centrally when native accounting becomes available. The task fields start at an **accounting estimate** of capacity 256, host headroom 64 and system reservation 48. `system_tasks` must be at least 48, covering 32 bounded session workers plus 16 tasks of service/control headroom. These are configurable operator ceilings, not macOS kernel limits or measured sufficiency; C12 must record and qualify the selected values. CPU and memory capacity come from the observed host. Headroom and control reservations follow the approved defaults in [contracts](contracts.md#native-macos-host-evidence). Additional headroom can only subtract capacity.

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

The private-FD API passes only a descriptor identifier through startup metadata. The receiver consumes and closes the credential FD before a later `exec`; tests exercise that boundary in real subprocesses. Do not put secrets in argv, environment, debugging or payload-inherited descriptors. These tests do not establish C05 helper authorization or payload startup. OS UID/PID observations are available, and C03 joins them with native boot/start identity inside the authority. The normal service still refuses instance registration until DG1-C06. No authenticated caller of the normal service can obtain a principal, lease or execution grant.

The SDK inspection example can be built with `CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-client --example inspect`. Its interface is `inspect SOCKET UID CONSUMER GENERATION CREDENTIAL_FD`: a parent must supply the secret bytes through that dedicated inherited FD, while arguments carry only the FD number and non-secret connection metadata. It prints observed peers and authenticated status. It is a status example, not the future generic execution CLI.

## Compatibility, checks and rollback

Core `AuthorityStorage` holds the original exclusive lock and validates the existing schema-1 journal without changing attempt state. Activation with a real `Backend`/`Clock` revalidates accounting in the same transaction as boot-aware recovery. Existing `Authority::open` preserves its recovery behavior and the DG-0 tests. C01–C05 do not change the journal schema or contract serialization. C05 adds wire requests and responses that clients use only after the service advertises fenced launch. The new local protocol strictly rejects unknown fields, versions and unsupported required capabilities; added fields need explicit compatibility tests.

Available checks:

```sh
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/qualify.py dg1-auth --offline
python3 scripts/qualify.py dg1-probes --offline
python3 scripts/qualify.py dg1-scopes --offline
python3 scripts/qualify.py dg1-launch --offline
python3 scripts/validate.py --offline
```

Functional suites reject zero executed cases and record source fingerprints, toolchain, bootstrap mode and logs. `dg1-authority` checks exclusive startup, aliases/permissions, absent/corrupt/future journals, activation-time corruption, strict configuration versions, project escalation, concurrent bootstrap and the policy derived from observed host capacity. `dg1-auth` checks actual peer observations, authentication roles, strict frames, bounded communication and private FD hygiene, including concurrent requests whose registration remains closed.

`dg1-probes` runs only on macOS; other platforms record `not_run`. It checks:
- the boot clock against `kern.bootsessionuuid`
- repeatable process identities, including exited, zombie, absent and refused cases
- host capacity and structurally valid native pressure readings
- closed admission before the first sample and after six seconds without one
- immediate closure on a probe failure
- rejection of stale, future, replayed and other-boot samples
- native registration of an observed socket peer
- the service sampling loop, including an injected failure and a probe stuck in the kernel, which must not hold the authority lock

Each native stage must also leave its declared raw receipts under the report's `raw/` directory, and stage logs are hashed. A case the environment cannot produce is recorded as `not_run`, which makes the suite `incomplete` rather than `passed`. `--allow-incomplete` returns success for an incomplete suite without changing its report; use it only where the environment is known to be unable to produce a case.

`dg1-scopes` also runs only on macOS. It drives real scope roots that lead their own process group and re-execute under the utility QoS clamp. It checks:
- nice and QoS readback, and a failed application for an unclamped root
- binding, run authorization and release only after every member exits
- no release after a root exit or reap while descendants survive
- sticky escape after a descendant leaves the group
- identity-checked termination
- refusal of shared or non-empty groups, another user's process, kernel requirements and unknown scopes

Scripted-table unit tests cover creation races, PID reuse and tracking loss. Scope evidence is exercised through the library only; the service establishes no scopes until P3.

`dg1-launch` also runs only on macOS. It drives the real `devguard-launch` helper through an isolated test authority that opens launch, with a synthetic healthy host probe. It checks:
- READY, then the executable replacing the helper with its argv, working directory, environment and exit status, and release only after its scope ends
- the executable's descriptors: only the standard three, with no permit, transcript or authority session, and no permit in its arguments or environment
- one claimed helper per grant: a replayed commit carries no permit, two racing helpers start one executable, and a late helper is refused
- refusal before any claim of a helper that the owner did not create, of an unregistered instance and of a wrong permit, leaving the grant usable
- cancellation fencing a helper that arrives afterwards
- exec failure after READY, reported apart from refusal
- a bare helper presenting the same grant twice, as after a lost reply, which is told not to exec the second time
- a bare helper without the utility clamp, refused after its claim, which is killed and released only through its scope; where the environment clamps every child, this case is recorded as `not_run`
- a reply that misses the helper's 250 ms deadline because the authority is busy, which never leads to exec
- concurrent launches from several threads, whose executables inherit only what the owner left inheritable
- a helper on a pseudo-terminal that already leads its session and group

Core, scripted-table, transcript, wire and session tests cover the claim transitions, cancellation that leaves Suspect, Draining and terminal attempts unchanged, helper identity checks, strict transcripts and messages, and helper sessions that can make no other request.

These suites leave registration and launch in the normal service, reconciliation, Linux enforcement, self-use and foreground SLO `not_run`. The full validator retains the 44 original tests and validates all six workspace crates and their explicit dependency graph. The launch crate depends on the daemon only for its tests' isolated authorities. Core/contract remain independent of daemon configuration and native adapters, and client does not depend on core.

To roll back this boundary, stop its task-owned foreground process and select a source/artifact compatible with the preserved configuration and schema-1 journal. Keep persistent state and credentials. A C02 artifact can reopen the same state: C03 activation adds no record type and only runs the existing recovery. No workloads can have started through the normal service in C01–C05. Later live-lease rollback requires actual reconciliation; it cannot use this early empty-state assumption.
