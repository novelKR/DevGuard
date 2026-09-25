# Operating the DevGuard service boundary

[English](operations.md) | [한국어](ko/operations.md)

C01 provides explicit bootstrap/canonical storage, C02 authenticated local transport, C03 native macOS boot, process and host pressure evidence, C04 cooperative policy readback and scope evidence, C05 the fenced launch helper, C06 reconciliation, C07 the `devguard` command-line owner, C08 the Cargo adapters, C09 installation of a packaged release as the current user's LaunchAgent, C10 parent leases with candidate authorities, C11 upgrade and repair, and C12 the SLO qualification harness. PR and post-merge main delivery evidence is tracked separately from implementation. `devguardd serve` runs a foreground service that activates the journal with that evidence and then opens registration over the wire, fenced launch and reconciliation. `devguard exec` runs commands through that service, and `devguard doctor` diagnoses it. The normal authority is currently macOS-only; Linux CI checks portable contracts and bounded fixtures, not DG-LINUX controls.

## Available commands

Build with Rust 1.95.0 using one Cargo job during bootstrap:

```sh
CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-daemon -p devguard-launch -p devguard-cli
target/debug/devguardd paths
target/debug/devguardd init
target/debug/devguardd check
target/debug/devguardd serve
target/debug/devguard doctor --require admission,macos-cooperative
target/debug/devguard exec --wait 30s -- /usr/bin/true
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
  - launch and reconciliation receipts, listed in [contracts](contracts.md#reconciliation)

  Receipts contain host counters, pressure states, the state and registered project paths, and their mount points. They never contain credentials.

On a platform without native evidence, or when the macOS observation fails, `serve` keeps the storage-only closed behavior and reports which case applies.

Authenticated status reports `storage_validated`, `registration_ready`, `execution_ready`, a reason and the configuration fingerprint. With native evidence registration and execution are ready, and admission still follows host pressure and capacity. Otherwise both are false, and the reason distinguishes an unsupported platform from a failed native observation. `devguard` finds `devguard-launch` beside itself, so build them together. Configuration or a successful handshake alone does not govern commands. Necessary bootstrap builds are not self-use evidence. Do not run a persistent service from the disposable `target` path; install a package instead.

A persistent service runs an installed release:

```sh
python3 scripts/package.py --offline
target/package/<release-id>/bin/devguardd install --package target/package/<release-id>
devguardd status
```

`package.py` builds a release package from a clean tree. `install` must run from that package. It refuses while an authority serves, while the agent exists, or without the existing authority state. It copies the release to an immutable `releases/<release-id>` and starts it as the current user's LaunchAgent `io.github.novelkr.devguard`, which runs `releases/<release-id>/bin/devguardd serve`. It selects the release only after verifying that launchd runs exactly that binary. `devguardd status` reports the installed service and exits 1 unless it runs the verified current release. Replacing a release is an upgrade (C11, below). See [contracts](contracts.md#installation-and-the-current-user-service).

A candidate build is verified under a parent lease of the running service:

```sh
devguard test-candidate --candidate /path/to/candidate/tree --report /path/to/new/report --wait 5m
```

`test-candidate` admits one lease (by default 2 CPU, 4 GiB and 96 tasks, with a two-hour deadline) and runs the tree's build, its applicable tests and its own `devguardd candidate` as lease children through `devguard exec --lease`. It checks the candidate's admission from outside, ends the lease and writes `report.json` into the new report directory, exiting 0 only when every step passed. The service must state parent leases, which needs a C10 or later release, and admits no lease while its pressure is Critical. See [contracts](contracts.md#parent-leases-and-candidate-authorities).

A staged release replaces the running one without disturbing charged work:

```sh
target/package/<release-id>/bin/devguardd stage --package target/package/<release-id>
"$HOME/Library/Application Support/DevGuard/releases/<release-id>/bin/devguard" upgrade --release <release-id>
"$HOME/Library/Application Support/DevGuard/releases/<last-known-good>/bin/devguard" repair --use last-known-good
```

`upgrade` closes admission, waits up to `--drain-timeout` (60 s by default) for charged work to end, backs up the journal under `backups/`, starts the new release with admission closed, verifies it and then reopens admission. If the drain times out, the current release keeps serving with its charges; if the new release cannot be verified, the previous one starts again. A release before C11 cannot close admission, so `--stopped` stops it first and proceeds only when nothing is then charged. SIGINT or SIGTERM during the drain cancels the upgrade and reopens admission. An upgrade interrupted after the new release started is completed by running it again, and `devguard admission --open` reopens admission on the serving release. Only one installation, staging, upgrade, repair or reopening runs at a time. `repair` returns the service to the last known good release, the one the last upgrade replaced, or to its recovery copy, only while no authority serves, and keeps a journal that cannot be opened closed. See [contracts](contracts.md#upgrade-and-repair).

The SLO protocol measures the installed release on its host (C12):

```sh
"$HOME/Library/Application Support/DevGuard/releases/<release-id>/bin/devguard" exec --adapter cargo --wait 10m -- cargo build --release --locked -p devguard-qualify
python3 scripts/measure.py macos --release <release-id> --qualify-bin target/release/devguard-qualify --out <new evidence directory> --work <new work directory> [--rehearsal]
python3 scripts/measure.py promote --summary <evidence directory>/summary.json
```

`measure.py macos` refuses to start unless all of these hold:
- the service runs that release, with `devguard doctor` satisfied and nothing charged;
- Rust 1.95.0 is first in `PATH` for the Cargo load;
- Google Chrome is in `/Applications`, and no other Chrome runs.

The host must be set aside for the whole run. It takes about five hours: two combinations of three repetitions, each 10 minutes idle and at least 30 minutes of load. The run keeps one fixture window for the whole run and brings it to the front. If macOS does not let it, click the window once. Afterwards, do not use the host: any other frontmost application, a locked screen or a visibility change makes that interval `inconclusive`. Turning on Do Not Disturb keeps notifications from taking the focus.
- **Output.** Raw samples, receipts and a `report.json` per repetition go under `--out`. Writer threads write the samples, so no sampling thread waits on that disk. Sources, Cargo targets and the I/O workload's files go under `--work`, which can be removed afterwards. The `devguard-qualify` binary, which runs the probe and the bounded workloads, and the browser profile run from a temporary stage under `/private/tmp` on the internal disk, which is removed at the end. The run header records which disk each location is on. `summary.json` gives each combination's verdict, the overall one, and each target's status (measured, not applicable or not run).
- **Exit status.** It exits 0 only when the run is qualified, 1 if a repetition failed, 2 if the run was inconclusive, 130 if interrupted and 3 if the harness itself failed. An interruption or a failure asks every started owner, probe and browser to stop, and each owner settles its scope.
- **Rehearsal.** `--rehearsal` runs one short repetition of each combination to prove the harness and is always inconclusive; `--headless` proves it without taking the front.
- **Promotion.** `promote` recomputes the verdict from the preserved reports. It writes the release's qualification record only for a qualified run whose policy, host and release are unchanged, while the service still runs that release. See [contracts](contracts.md#slo-qualification-c12).

`devguard exec [--project ID] [--adapter auto|generic|cargo|cargo-pipeline] [--wait DURATION] [--cpu MILLICPU] [--memory SIZE] [--tasks N] [--receipt PATH] [--lease CONSUMER/GENERATION/ATTEMPT --lease-token-fd N] -- PROGRAM [ARGS...]` admits the command, starts it through `devguard-launch` and waits for it. The program and its arguments follow `--`, and no shell is used. It exits with the command's status or ends by its signal, and exits 125 when nothing was started. Without `--wait` a denial ends it at once; `--wait` retries capacity and pressure denials until the deadline, and refuses at once a request larger than the host's work capacity. `--receipt` writes a private JSON receipt. `--lease` makes the command a child of that parent lease, admitted against its remainder with the token read from descriptor N. `devguard doctor` prints a JSON diagnosis, and `--require admission,registration,macos-cooperative` makes it fail unless each requirement holds. See [contracts](contracts.md#command-line-owner).

`--adapter cargo` fits `cargo`'s compiler jobs to the reservation, and `--adapter cargo-pipeline` shares one jobserver across the Cargo runs of a program such as a validation script. `auto`, the default, selects the Cargo adapter for `cargo` commands that compile. A reservation that cannot fit one Cargo job (1 CPU and 2 GiB) is refused. See [contracts](contracts.md#cargo-adapter).

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

This is a configuration example, not evidence that execution already consumes a lease. `devguard exec --project ID` needs the project's absolute root registered in operator configuration, and the working directory inside it. Projects and credentials do not create another host budget.

## Client authentication and transport limits

The client library connects to the canonical endpoint, checks the OS-observed authority UID/PID, negotiates wire version 1 and corroborates its own UID/PID in the handshake. Consumer ID/generation/secret authenticate a configured workload or control-service role; a separate secret authenticates the administrative role. UID equality alone grants no role. Helper permits cannot be used as caller credentials, and this authentication does not isolate malicious processes sharing the same UID.

The wire uses a four-byte length prefix, at most 64 KiB per JSON payload, at most 32 active sessions and an absolute 250 ms deadline for each frame read/write. That deadline includes idle waiting before a new frame, so idle connections expire. Use a fresh authenticated session when explicitly beginning a later operation; the client does not automatically reconnect or retry. Its `poll` and nonblocking descriptor I/O handle partial frames, slow readers and buffered final responses without changing Darwin timeout options after peer closure. No response loss is treated as proof of execution, nonexecution or resource release.

The private-FD API passes only a descriptor identifier through startup metadata. The receiver consumes and closes the credential FD before a later `exec`; tests exercise that boundary in real subprocesses. Do not put secrets in argv, environment, debugging or payload-inherited descriptors. These tests do not establish C05 helper authorization or payload startup. OS UID/PID observations are available, and C03 joins them with native boot/start identity inside the authority. With native evidence an authenticated consumer registers its own observed process as an instance, and can then obtain admission and a one-time launch grant. The grant is presented by its helper, never by the caller.

The SDK inspection example can be built with `CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-client --example inspect`. Its interface is `inspect SOCKET UID CONSUMER GENERATION CREDENTIAL_FD`: a parent must supply the secret bytes through that dedicated inherited FD, while arguments carry only the FD number and non-secret connection metadata. It prints observed peers and authenticated status. It is a status example, not the `devguard` execution CLI.

## Compatibility, checks and rollback

Core `AuthorityStorage` holds the original exclusive lock and validates the existing schema-1 journal without changing attempt state. Activation with a real `Backend`/`Clock` revalidates accounting in the same transaction as boot-aware recovery. Existing `Authority::open` preserves its recovery behavior and the DG-0 tests. C01–C06 do not change the journal schema or contract serialization. C05 and C06 add wire requests and responses that clients use only after the service advertises fenced launch. C10 adds lease requests and responses that clients use only after the service states parent leases, and the tables `leases` and `lease_children`, created when absent, without changing the schema. C11 adds administrator requests that close and reopen admission and report what is charged, used only after the service states `upgrade_drain`, and the private marker `state/admission.json`; the journal schema is unchanged. The new local protocol strictly rejects unknown fields, versions and unsupported required capabilities; added fields need explicit compatibility tests.

Available checks:

```sh
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/qualify.py dg1-auth --offline
python3 scripts/qualify.py dg1-probes --offline
python3 scripts/qualify.py dg1-scopes --offline
python3 scripts/qualify.py dg1-launch --offline
python3 scripts/qualify.py dg1-reconcile --offline
python3 scripts/qualify.py dg1-cli --offline
python3 scripts/qualify.py dg1-cargo --offline
python3 scripts/qualify.py dg1-bootstrap --offline
python3 scripts/qualify.py dg1-self-use --offline
python3 scripts/qualify.py dg1-upgrade --offline
python3 scripts/qualify.py dg1-macos --offline
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

`dg1-reconcile` also runs only on macOS. It drives the real helper through isolated authorities, one of them served in a child process that the test kills and restarts. It checks:
- prepared cancellation, and expiry at the original deadline after the attempt stayed Prepared and charged until then, both known not started
- an owner's reports that no helper exists (a failed spawn, a helper that exited before READY and a lost grant response), released as `NoHelperCreated`, with a late helper refused
- a report that cannot release a claimed grant
- release only after every member of a scope ends, with the owner observing before it reaps the root while the background reconciler is paused, so only that observation adopts the survivor
- a root reaped before any observation, whose survivor is tracking loss: the attempt stays Suspect and is never released
- a known escape that keeps the attempt Suspect after everything exits
- termination of every member of a scope, with release following their end
- cancellation after authorization, which keeps the reservation until the scope ends
- a dead owner's unclaimed grant turning Suspect and keeping its instance, and an exited owner without work being retired
- an unresponsive helper that time never settles, even past the Prepared deadline, and its owner's report settling it
- a daemon crash: the committed totals are the same before the crash and after the restart, every committed attempt is Suspect, a late helper is refused, a running scope with lost tracking is not released after it ends, and the owner's report releases an unclaimed grant
- journal writes that fail: a failed bind write stops the claimed helper before exec and settles it through its scope, and a failed release write is reported and retried until it is written

Child-process authorities keep their receipts, which are checked to hold no permit or caller credential.

Core, launcher-evidence, service and wire tests cover previous-boot release, previous-boot instance retirement whatever holds the old PID, release-reason record invariants, write-free reconciliation, attempt and instance listings, owner-bound reports, the owner liveness rule, a reconciler that stops the service on a poisoned authority, and strict request decoding.

`dg1-cli` also runs only on macOS. It first builds `devguard-launch`, then runs the CLI as a separate owner process against isolated authorities. It checks:
- a command's arguments, working directory, environment and exit status, with release after its scope ends
- a workload ended by a signal, which ends the CLI by the same signal
- SIGTERM and SIGHUP sent to the CLI reaching every member of the workload's group, and a signal the caller ignored staying ignored and unforwarded
- a SIGSTOP of the workload, which the CLI does not mirror
- observation before reap, which keeps a survivor tracked and charged until it ends
- a budget the host cannot fit, which starts nothing, and a wait for one, refused at once without an attempt
- an explicit wait that is admitted once capacity is released, one that ends at its deadline and one that a signal cancels
- an unavailable authority and one without the fenced launch capability, which start nothing, and doctor diagnostics with and without requirements
- a registered project's limits and the working directory it must contain
- at most eight concurrent owners, and owners that have ended never exhausting the pool
- a missing or unexecutable program, an existing receipt path and a missing helper, which admit nothing
- on a pseudo-terminal, an interrupt key reaching the workload directly, including when only output is on the terminal, and a stop mirrored so a job-control shell regains the terminal

Argument, preparation, entry-point and scripted-authority tests cover strict parsing, program resolution, budget precedence, the meaning digest, lost admission and launch-commit replies, the wait's backoff, deadline and cancellation, the retry rule after a helper ended before READY, a transcript read to its end, and malformed invocations of the real binary.

`dg1-cargo` also runs only on macOS and builds `devguard-launch` first. It runs real Cargo builds of small offline workspaces through the CLI. A compiler wrapper records when each compilation starts and ends, and a `cargo` shim records the descriptors each launch inherited. It checks:
- a direct build within the reserved jobs, with its target directory kept
- explicit jobs clamped to the reservation or kept within it
- conflicting jobs, an unsupported subcommand, a reservation below one job and a non-Cargo program under the Cargo adapter, which start nothing, and `auto` choosing the generic adapter for a Cargo command that compiles nothing
- `cargo test` compiling within the reservation while its test threads are left alone
- a Python pipeline whose concurrent Cargo runs share one FIFO jobserver, with its report path kept, both targets built and its tokens returned
- nested Cargo in a build script sharing the outer jobserver
- inherited FIFO and descriptor-pair jobservers preserved and bounding the build, and stale inherited descriptors removed before Cargo runs
- concurrent consumers, each within its own reservation
- a cancelled pipeline whose builds stop and whose jobserver is removed

Each Cargo launch and pipeline script inherits only its standard descriptors, except the caller's own descriptor-pair jobserver when one is inherited. The shim also records the arguments and the jobserver and fallback variables each Cargo received. Where the host's work capacity cannot fit the Cargo jobs a case needs, as on the hosted macOS 14 runner, the case is recorded as `not_run` and the suite reports `incomplete`; CI runs it with `--allow-incomplete`.

These suites leave Linux enforcement, self-use and foreground SLO `not_run`. `dg1-bootstrap` also runs only on macOS. Its tests copy the test binary into each package as its `devguardd`, so the installer really runs from its package. The installed service is that copy, serving an isolated fixture authority. It checks:
- an installed release verified running before it is selected, with immutable release and recovery copies and a healthy status
- an identical release reused, and a different release under the same id refused without touching the installed one
- packages refused before anything is written: a modified, extra, missing or symlinked binary, other capabilities, an inconsistent scope, and an installer that is not the package's own
- refusal while an authority serves, or without the authority state
- a service that never proves itself, which is unloaded with nothing selected
- concurrent installers, which leave exactly one service
- under launchd itself, a transient job with a unique label: a crash restarted onto the same journal, a SIGTERM stop, and a start with an unreadable journal that fails closed and is not restarted

A session without a launchd gui domain records the launchd case as `not_run`, and the suite reports `incomplete`.

`dg1-self-use` also runs only on macOS and builds `devguard-launch` first. It runs real lease children and a real candidate authority process against isolated authorities. It checks:
- a lease charged once against the host, children admitted only against its remainder, and their sum never above the lease
- the parent-lease capability stated only to clients that require it, a token-only holder session, and a refused wrong token or unknown lease
- fencing when the lease ends, when its owner goes or belongs to an earlier boot and when its deadline passes, a Suspect child that keeps the lease charged, and release once every child is settled, also across a restart
- `devguard exec --lease` running a workload as a lease child, which inherits only its standard descriptors; a child larger than the lease, also with a wait; a forged token; and a child that starts after the lease ended
- a candidate authority that admits within its capacity, launches nothing, holds no lease, closes with its lease and never opens the parent's state, and one refused for a forged token, an unknown or ended lease, a capacity larger than the lease or below its own reservation, or an existing area
- `test-candidate` running a workload and a candidate as children of one lease that is then released, with the candidate's area removed; a candidate that dies before serving; and one that never serves nor closes and is stopped with SIGTERM. Both fail the run while their scopes and the lease are still released

`dg1-upgrade` also runs only on macOS. It installs fixture releases with a fake service manager and replaces or repairs them. It checks:
- a normal upgrade: the drain settles a Prepared attempt, the backup holds the journal, the selection and the manifest, the new release starts closed and idle, tombstones survive and admission reopens
- a drain that times out, keeping the current release and its charge, and the same upgrade succeeding once the work is settled
- a release that cannot start, after which the previous one serves the same journal with admission open
- an incompatible downgrade, refused before anything changes
- a release that cannot drain: refused without `--stopped`, started again when stopping finds work charged, and replaced once nothing is
- repair refused while an authority serves, returning to the release an upgrade replaced on the same journal, using the recovery copy of a damaged release, and keeping a journal that cannot be opened closed
- an upgrade from a release repaired onto its recovery copy
- a failed stop that leaves the drained release serving, whose admission only reopens, and a drain cancelled by a signal
- an interrupted upgrade completed by running it again, admission reopened by the administrator's command, repair reopening admission that an interrupted upgrade left closed, and one operation at a time
- a release that dies while it is verified, which gives way to the previous release, and an interrupted repair completed by repair, never adopted by an upgrade
- admission closed by the administrator only, kept across a restart and reopened, and strict drain wire fixtures and compatibility rules

`dg1-macos` also runs only on macOS and builds `devguard-launch` first. It checks the SLO protocol's harness, never the SLO itself:
- the bounded workloads
- the control probe against isolated authorities:
  - status samples of its own running target;
  - effective terminations, released by scope termination;
  - a stop before or during sampling that still settles every target;
  - a target never admitted, which leaves nothing charged;
  - a helper that ends before READY, which is reported so its grant is settled
- the probe's missed-slot accounting
- the protocol's rules:
  - nearest-rank percentiles with missing samples;
  - effective terminations and per-consumer load;
  - receipts, validity and the ordering of verdicts;
  - the recomputation that promotion relies on
- the foreground fixture through headless Chrome, whose samples arrive but which is never a valid observation

Where Chrome is absent the fixture case is recorded as `not_run`, and the suite reports `incomplete`. The SLO protocol itself is `measure.py macos`, run only on the target host in a window set aside for it.

The full validator retains the 44 original tests and validates all nine workspace crates and their explicit dependency graph. The daemon depends on itself only to enable its fixtures in its own tests. The launch crate depends on the daemon only for its tests' isolated authorities. The CLI depends on the daemon's paths and configuration and on the Cargo adapter, and on the daemon's fixtures only in tests. The Cargo adapter depends only on the contract. The qualification harness depends on the CLI's endpoint, the daemon's paths, the client and the contract, and on the daemon's fixtures only in tests; it is never packaged in a release. Core/contract remain independent of daemon configuration and native adapters, and client does not depend on core.

To roll back this boundary, stop its task-owned foreground process and select a source/artifact compatible with the preserved configuration and schema-1 journal. Keep persistent state and credentials. A C02 artifact can reopen the same state: C03 activation adds no record type and only runs the existing recovery. From C06, workloads can start through the normal service. Before rolling back to an artifact without reconciliation, stop starting new work and let charged attempts reach a terminal phase. A C05 artifact reads the same journal but keeps launch closed and does not reconcile, so any remaining attempt stays charged and Suspect. If the service stops while scopes run, start it again: their attempts stay Suspect and charged until their owners report unclaimed grants or a reboot proves termination. C07 changes no service state: to roll back the CLI, stop starting commands through it. Commands it already started stay charged until their scopes end, and it never falls back to unmanaged execution. C08 changes no service state either: to stop fitting Cargo's jobs, select `--adapter generic` explicitly; existing targets and caches are kept. Before a release is verified, the C09 installer boots its job out and removes the plist, and nothing is selected. To stop the installed service, run `launchctl bootout gui/<uid>/io.github.novelkr.devguard` and remove the plist. Releases, recovery copies, the selection and the journal are kept, and a preserved artifact's foreground `devguardd serve` reads the same state. C10 adds no service state beyond its lease tables. Before rolling back to a C09 artifact, end every lease and let it be released: a C09 service ignores the lease tables, so it would not hold a lease's unused remainder. A candidate's area under `candidates/` is disposable and never read by the normal service. C11 keeps each upgrade's quiescent backup under `backups/` for manual recovery and never restores one automatically. To return to a C11 or later release, upgrade to it. A release before C11 has no upgrade command: stop the service, remove the plist and run that release's `devguardd install --package` on its release directory. It ignores the closure marker, which a later upgrade writes again. C12 adds no service state except a promotion's record under `qualifications/`, which nothing reads to admit work. Revoking a promotion removes that record and keeps the evidence. Never delete the journal or its tombstones to make a rollback or restart succeed.
