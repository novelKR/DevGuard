# Implemented authority contract

This document describes the implemented DG-0 authority, C01/C02 service, storage and transport behavior, C03 native macOS host evidence, C04 cooperative policy and scope evidence, the C05 fenced launch helper, C06 reconciliation, the C07 command-line owner, the C08 Cargo adapters, C09 installation as the current user's LaunchAgent, C10 parent leases with candidate authorities, C11 upgrade and repair, and the C12 SLO qualification harness. PR and post-merge main delivery evidence is tracked separately from implementation. [Operations](operations.md) lists actual command availability. With native host evidence the service opens registration, launch and reconciliation; CodeSpace integration remains unimplemented. [Korean translation](ko/contracts.md).

## Authority and transport boundaries

The core is a Rust library. `Authority::register` accepts a `TrustedPeer` and verifies the configured UID, consumer credential, generation and exact process identity through `Backend`. Registration returns an opaque `Principal`; a workload cannot construct a control-service principal through the public API. A workload consumer cannot configure a control reservation. The administrative reconciliation and generation-retirement methods belong to a trusted daemon/operator path and must not be exposed as workload RPCs.

DG-0 proves the library registration boundary with a fake peer and backend. C02 supplies real local UDS authentication and a small client, but does not activate native registration. The server observes peer UID/PID through OS socket credentials; the client independently corroborates the authority UID/PID and its own identity in the handshake. Neither accepts caller-declared peer identity. C03 adds native boot/start identity, so an OS-observed peer UID/PID joined with the kernel's start identity forms a complete trusted registration observation; native tests exercise this path in-process. With native host evidence the service opens registration over the wire and the fenced launch helper described below, together with reconciliation. Without it, on another platform or after a failed macOS observation, workload/control-service registration returns `ResourceControlUnavailable`. Authentication alone issues no `Principal`, instance slot, lease or host budget. Administrative credentials cannot perform registration.

The handshake negotiates contract compatibility and wire version 1. A service with native evidence advertises durable admission, fenced launch, per-resource evidence, static control reservations and macOS cooperative control; without it, the service advertises none. Parent leases (C10) are stated only to a client that requires them. Consumer generation and credential digest determine workload/control-service roles; an independent administrative digest grants only the explicitly exposed administrative role. All consumer and administrative digests must differ. Caller credentials, one-time helper permits and administrative operations are separate boundaries: a helper credential variant is not accepted as caller authentication. This remains a cooperative operating-account model, not isolation from a malicious same-UID process.

The authority holds an exclusive no-follow lock in the journal's parent directory. All journals in that authority directory share the lock. The lock is opened with `O_NONBLOCK`, and its opened descriptor must identify a private, current-UID regular file with exactly one link, including during explicit initialization. A FIFO or linked file cannot stand in for the lock. C01 derives canonical normal-service paths from the OS account rather than caller HOME/XDG values and refuses state/socket overrides. Project configuration cannot carry authority credentials or capacity. Candidate paths exist only under a parent lease (C10): `devguardd candidate` derives them from the account's own authority and accepts no other.

`AuthorityStorage` exclusively opens and validates a journal without inventing a boot clock, recovering attempts or granting capabilities. `Authority::from_storage` activates it with an actual Backend/Clock and revalidates the accounting index inside the recovery transaction. `Authority::open` preserves that behavior through the same path. Explicit bootstrap remains separate from ordinary open; missing/corrupt/future-schema state is not repaired automatically.

## Bounded local protocol and credential transport

A frame has a four-byte length prefix and at most 64 KiB of JSON payload. Frames, message variants and nested wire types reject unknown fields; version/request identity and required capabilities are checked separately. Added fields are not automatically backward compatible. The server accepts at most 32 active session workers. Each frame read or write has an absolute 250 ms deadline, including idle waiting before the next frame; receiving another byte does not restart the deadline. An expired idle session is closed. These transport bounds do not establish the later end-to-end admission budget or C12 responsiveness qualification.

Framing uses `poll`, descriptor `O_NONBLOCK` and per-call nonblocking socket I/O. Descriptor nonblocking mode also bounds large writes on Darwin, where a per-call flag alone is insufficient. It drains buffered final data on peer closure without changing socket timeout options, which can fail with `EINVAL` on Darwin after the peer has closed. A malformed, truncated, expired or unavailable response cannot imply execution or release. The client does not retry automatically, and never substitutes an unmanaged authority or execution.

`CredentialHandoff` transfers one caller secret through a dedicated inherited descriptor, keeping the parent copy close-on-exec. `take_inherited`/`read_owned` consume and close the receiver descriptor on success or error, with bounded length and a 250 ms read deadline. Secret serialization is deliberate for the local authentication exchange; debugging and parser errors redact credentials. Subprocess tests observe that the FD is closed before a subsequent `exec` and the secret is absent from argv/environment/output. They validate transport hygiene, not C05 helper authorization, READY, payload startup or containment.

## Native macOS host evidence

`devguard-macos` supplies the core `Clock` and `Backend` inputs from the running host; the core receives them only through those traits. On other platforms opening it returns `ResourcePolicyUnsupported`, which differs from a failed macOS observation (`ResourceControlUnavailable`). In either case the service keeps its storage closed and states which one applies. If the observed host cannot form a valid policy, or boot-aware recovery of the journal fails, `serve` refuses to start instead.

- **Boot-relative time.** `boot_id` is `kern.bootsessionuuid`, read once per process. `monotonic_ms` is `CLOCK_MONOTONIC_RAW` (mach continuous time), which advances during sleep and ignores calendar adjustment. Observations from another boot are never compared. A clock failure after the startup check aborts the process rather than fabricating time.
- **Process identity.** `start_ticks` is `ri_proc_start_abstime` from `proc_pid_rusage`, in mach absolute time units. `exec` does not change it, and a reused PID has a different value. Each read brackets the process-table snapshot with two start reads and repeats if the PID changed in between. Only a live process has an identity; a zombie and a reaped PID are both absent. A refused observation, such as another user's process, is an error and never absence.
- **Capacity and policy.** Capacity is `hw.logicalcpu` and `hw.memsize`. Host headroom is max(ceil(25% of logical CPUs), 2 CPUs) and max(25% of memory, 4 GiB), plus operator `additional_headroom`. The system reservation is 0.5 CPU and 256 MiB (daemon plus aggregate CLI pool) with the configured `system_tasks`. On the 8-CPU/16-GiB target this leaves 5,500 mCPU, 11.75 GiB and 144 tasks of workload capacity. These are accounting quantities, not kernel limits.
- **Host pressure.** Every two seconds the service reads four sources:
  - `kern.memorystatus_vm_pressure_level` (1 normal, 2 warning, 4 critical; any other value fails the reading)
  - page-outs from `host_statistics64` (`pageouts + swapouts` times the kernel page size from `host_page_size`)
  - `vm.swapusage`
  - `statfs` for the state volume and every registered project root

  Rates cover the readings from the last ten seconds and round up, so truncation cannot hide a threshold. Swap growth over a shorter startup window is extrapolated to ten seconds. Control-loop lag is how late the sampler woke, or how far the previous reading overran its schedule, whichever is larger; each reading also records how long the host read took (`read_ms`). After an overrun the cadence resumes one interval after the completed reading instead of catching up in a burst, and a reading in the same millisecond as the previous one yields no rate rather than a failure. Among several volumes, the sample uses the one the disk watermarks treat most severely. Linux memory PSI does not exist on macOS.
- **Readiness.** A sample needs a prior reading, so admission stays closed until the second valid reading. A failed or inconsistent reading closes new work immediately through `Authority::pressure_observation_failed`, restarts the rate window and then requires the ordinary 30-second recovery steps. Inconsistent readings include an unknown level, a missing volume, and a counter or clock that moved backwards. Stale, future, replayed and other-boot samples are rejected. If sampling stops, admission closes once the last sample is older than six seconds.
- **Activation.** `serve` activates the exclusively held journal with the native clock and backend, so boot-aware recovery uses real identities. The backend returns the macOS plan: cooperative CPU through QoS and priority, accounted memory and tasks, and an observed process group. It also provides the scope evidence described in the next section and the owner-reported evidence that an unclaimed grant has no helper, described under reconciliation. Authenticated status reports `registration_ready` and `execution_ready` true. The service writes JSON-line receipts to stderr for activation, baselines, state transitions, rejected samples, late control-loop wake-ups, failures and a one-minute heartbeat. They contain no credentials. If the sampler stops for any reason, the service stops with an error. A probe that has not returned three seconds after shutdown begins is abandoned rather than allowed to block shutdown, and the service then exits with an error.

## Native policy application and scope evidence

On macOS a scope is the process group led by its root: the launch helper, which then becomes the executable. Workloads are assumed to be cooperative and to stay in that group. The backend never claims containment of arbitrary descendants or kernel memory/task limits, and it does not use `NOTE_TRACK`.

- **Root preparation.** The root makes itself a process-group leader and re-executes itself under the utility QoS class (`posix_spawnattr_set_qos_class_np` with `POSIX_SPAWN_SETEXEC`). This keeps its PID, group, environment and inherited descriptors (`become_scope_root`, `exec_with_workload_qos`). Descendants inherit the QoS clamp and the nice value.
- **Establishment.** `NativeBackend::establish_scope` accepts a root only if it is a live, same-boot process of the same user, leads its own group and is still alone in that group. The authority then raises the root's nice value to +10, never lowering a higher value, and reads the result back:
  - CPU counts as applied only when `pbi_nice` is at least 10, the task priority (`pti_priority`) is at most 20 and the maximum priority (`pth_maxpriority`) of every thread that could be read is at most 20. Twenty is the utility ceiling. Without the clamp, the task priority reads 31 minus nice and threads read 63, so the thread check is what detects a missing clamp.
  - The root's identity is rechecked immediately before nice is applied and after the priorities are read, so a reused PID yields no evidence. The authority process itself is never a scope root.
  - Memory and tasks count as applied through the authority's accounting. Kernel methods are unsupported.
  - A failed application stays tracked so it can be terminated. Binding refuses it because `AppliedResources::confirms` needs every resource applied.
- **Binding.** `binding` repeats the readback for the registered scope instead of trusting establishment. An unknown or altered scope has no evidence. The scope ID is derived from the root's PID and start identity.
- **Observation.** A group ID is trusted only while it provably still names the scope's group. Each observation therefore checks the root before and after listing the group (`PROC_PGRP_ONLY`): while the root holds its PID, alive or unreaped, no other group can use that ID. Every listed member's identity is then rechecked:
  - An unknown member is adopted only if the root held its PID both before and after the listing, or a known member is seen in the group in the same pass. Otherwise it is tracking loss, because the ID may name an unrelated group.
  - An unknown process that was listed but has already moved to another group is also tracking loss; it cannot be told apart from a reused PID.
  - Every known identity is then checked, together with the children (`PROC_PPID_ONLY`) of live known members. A child counts only if its parent's identity is unchanged after the listing. A known identity or child outside the group is an escape. A child outside the group whose parentage cannot be verified is tracking loss.
  - An unreaped zombie still counts as present. The root counts as reaped only when its start identity is gone.
  - An empty group is confirmed by a second listing that must itself be empty. A member it lists counts even if it vanishes before its read, and any member that appears there is handled by the same adoption rule.
  - Identities confirmed gone are pruned, because a (PID, start) pair can never return. Once the group is confirmed empty with its root reaped, its ID is never listed again.
  - A refused or failed read sets tracking loss. This includes a libproc listing that returns zero entries with errno set.

  Escape and tracking loss are both sticky for the scope; no later ordinary observation clears them. An observation is timestamped when it completes.
- **Termination.** `signal_scope` sends a positive signal one PID at a time to known identities and known escaped identities. It also signals current group members, but only while the group provably is still the scope's. Each target is rechecked immediately before its signal. An unobservable root, a failed listing, a group that cannot be proven or a failed delivery does not stop the verified identities from being signalled; it marks the receipt incomplete. It never signals a stale PID or a process group, and delivery is not release evidence. macOS offers no process handle, so a PID can still be reused between the recheck and the signal.
- **Release.** Release still requires the core's full evidence. A reaped root with surviving descendants, a zombie member, a known escape and incomplete tracking each prevent it. An unbound committed launch is released only with its owner's report that no helper exists (see [reconciliation](#reconciliation)).
- **Limits.** The authority process holds scope tracking in memory. After a restart, previously bound scopes are unknown to the backend, so their attempts stay Suspect with lost tracking until a reboot proves their termination. A member the authority cannot observe, such as a setuid program, causes permanent tracking loss for its scope. If the root is reaped while no known member remains in the group, surviving unknown members cannot be proven to be the scope's. They are permanent tracking loss and are not signalled, so a scope must be observed before its root is reaped; an owner does this with `Observe` while its exited root is still unreaped.

## Fenced launch helper

DG1-C05 adds the launch helper, `devguard-launch`, and the wire requests that lead to it. The service opens them only with native host evidence, together with the reconciliation described below.

- **Registration.** After authenticating, a consumer session registers its instance. The authority derives the process identity from the OS-observed peer and the kernel's start identity; the caller supplies only an instance ID. A session is a bounded exchange, so each new session registers again, which reactivates the same instance.
- **Admission and grant.** `Admit` records the attempt and `BeginLaunch` commits it. Only the first `BeginLaunch` response carries the one-time permit. Attempts belong to the registered instance; another instance cannot look them up, cancel them or commit them.
- **Starting the helper.** The owner starts at most one helper per grant, as its direct child (`devguard_client::launch::helper_command`). The permit travels on a private descriptor. A second private descriptor carries the helper's transcript, apart from the executable's own output. Arguments and environment carry no secret. The helper then:
  1. leads its own process group, which a session leader on a pseudo-terminal already does;
  2. re-executes itself under the utility QoS clamp, keeping its PID and both descriptors;
  3. consumes and closes the permit descriptor, and marks the transcript close-on-exec;
  4. presents the grant in its own session, which can make no other request;
  5. on the first successful authorization, reports READY and executes the program. The executable keeps the helper's PID and process group, working directory, environment and every other inherited descriptor.
- **Authority checks.** One authority lock covers all of these, so no cancellation, reconciliation or other helper interleaves:
  - the owner must be registered in this service lifetime;
  - the presenting process must be a live process of the same user whose parent is the owner, and the owner must still be running;
  - the permit must be this grant's;
  - the grant must still be committed, or already claimed by this same helper.

  The authority then establishes the helper's scope, applying nice and reading the policy back. The first helper to get this far claims the grant: its scope is recorded durably before binding, so a helper refused afterwards is still settled through its scope, while a helper that fails establishment never uses up the grant. The authority then binds the scope and authorizes the run. A second helper, a helper arriving after cancellation and one arriving after the run are all refused. The same helper presenting again after a lost reply is told `may_exec = false` and does not exec.
- **Refusals.** A helper refused before its claim leaves the grant unclaimed, so the owner's own helper can still use it. A helper refused after its claim, for example because the readback shows no utility clamp, is killed. Its scope stays charged and is released only when its termination is observed; that release is not evidence that the executable never started.
- **Transcript.** One JSON object per line:
  - `failed`: the helper could not present the grant, so it did not claim it;
  - `refused`: not authorized, or the reply was lost; the helper never execs;
  - `ready`: every pre-exec boundary is complete, which is not evidence that the executable started;
  - `exec_failed`: the exec after READY failed.

  The descriptor closes when the exec succeeds. The helper exits with 125 when it is not authorized, 127 when the program does not exist and 126 for other exec failures; otherwise the status is the executable's.
- **Descriptors.** The helper closes only its own descriptors: the permit carrier, the transcript and its authority session. Everything else the owner leaves inheritable reaches the executable as part of the command's meaning, so an owner must keep unrelated descriptors close-on-exec. macOS cannot create a pipe or socket pair close-on-exec atomically. The client therefore creates a grant's descriptors above the standard three and spawns helpers under one process-wide guard, and `HelperCommand::spawn` closes the owner's copies. An owner that spawns other processes from other threads holds the same guard (`spawn_guard`) around them, or spawns them close-on-exec by default.
- **Receipts.** Each presentation yields one receipt, `helper_authorized`, `helper_replayed` or `helper_refused`, including refusals before any claim. It records how long the authorization took; a reply that misses the helper's 250 ms deadline is a refusal to the helper, which never execs.
- **Compatibility.** The journal schema is unchanged. A claimed but unbound attempt is a committed record with a scope and no applied resources; earlier artifacts read it and turn it Suspect on restart like any committed launch. Wire version 1 gains requests and responses that a client uses only after the service advertises fenced launch.

## Reconciliation

DG1-C06 adds the service's reconciler and the owner requests that settle an attempt. Nothing is released by a timeout, a disconnect, the owner's death or a reaped root alone.

- **Reconciler.** Every second the service reconciles each charged attempt, holding the authority lock for one attempt at a time:
  - Prepared attempts expire on their original five-second deadline.
  - A claimed or bound attempt is released only when its scope is observed to have ended: root reaped, group empty, known members gone, tracking complete and no escape. An escape or an incomplete observation makes it Suspect with sticky tracking loss.
  - An unclaimed grant is left alone while its owner runs, because a helper may still claim it. Once the owner is gone no helper can claim it, so it becomes Suspect; it is not released.
  - A scope from an earlier boot is released as previous-boot termination, even after lost tracking.
  - An attempt that a pass does not change is not rewritten.

  A pass that fails is reported (`reconcile_failed`) and retried. If the reconciler stops, including on a poisoned authority, the service stops, so launch never stays open without it.
- **Instances.** A session is a bounded exchange, so closing one does not make its instance suspect; the registered process's lifetime does. When that process is gone, its instance is retired if it owns no charged attempt and becomes suspect otherwise, and helpers naming it are refused. A process of an earlier boot has ended whatever holds its PID now, so after a reboot old owners are settled even when another user's process holds their PID; within one boot an observation the kernel refuses only defers reconciliation. When a registration finds its consumer's instance pool full, the service first applies this rule to the instances whose process has ended, so owners that have already exited free their slots at once rather than at the next pass. An ended owner that still owns charged work keeps its slot until that work is settled.
- **No helper created.** Only the owner holds a grant's permit, so it can report that it holds no helper for the grant and will start none (`AbandonLaunch`): creating the helper failed, the helper exited and was reaped before READY, or the response carrying the permit never arrived. The report needs no permit, is bound to the owner's registered identity and is kept in memory. With it, an unclaimed grant is released as `NoHelperCreated`, which is evidence that the executable never started. A claimed grant has a helper, so the report cannot release it; only its scope can. The release is sound because the report and the release run under the same authority lock as every claim, and a released or suspect grant can never be claimed, bound or authorized, so no late helper can start the executable. The journal also rejects a `NoHelperCreated` record that has a scope and a `ScopeTerminated` record without one.
- **Observing before reap.** `Observe` reconciles the owner's attempt at once, by the same rules. An exited root that is not yet reaped still holds its PID, so members left in its group can still be adopted. If the owner reaps first, surviving members cannot be proven to be the scope's: they are tracking loss, and the attempt stays Suspect.
- **Termination.** `Terminate` sends interrupt, hangup, terminate or kill to every rechecked identity of the attempt's claimed or bound scope. It reports how many identities were signalled and whether every target could be observed. Delivery changes no phase and is not release evidence. After a restart the scope is unknown, so the service cannot signal it.
- **Restart.** A restart makes every committed attempt and registered instance Suspect, and owners register again in new sessions. A previously bound scope then stays Suspect with lost tracking until a reboot, even after its processes end. An unclaimed grant is released only by its running owner's report. Accounting totals are unchanged across the restart.
- **Receipts.** The service writes these JSON lines to stderr, without credentials: `registered`, `admission`, `launch_committed`, `cancelled`, `helper_authorized`, `helper_refused`, `helper_replayed`, `launch_abandoned`, `scope_signalled`, `attempt_reconciled`, `instance_reconciled`, `reconcile_failed` and `reconcile_stopped`.

## Command-line owner

DG1-C07 adds `devguard`, the command-line owner of managed execution. It runs a command only through the authority and the fenced helper. When a command cannot be admitted or started, nothing runs; there is no unmanaged fallback.

- **Authority.** The binary derives the authority from the operating account: the canonical socket, the operator configuration and the `dev-cli` credential file. It accepts no path, socket or authority override. It authenticates as `dev-cli`, and each CLI process registers a new instance, so the configured limit of eight instances also bounds concurrent owners. Owners that have already exited never fill the pool, because a full pool is reconciled before a registration is refused. A pool full of running owners is treated like a capacity denial. Every request uses a fresh session, because each frame has a 250 ms deadline.
- **Preparation.** Before anything is admitted, the CLI resolves the program as a shell would, without running one: a name with a slash is a path, and a bare name is searched on `PATH`. A missing program exits 127, and one that is not executable exits 126. The executable runs with its absolute path as argv[0] and inherits the CLI's working directory, environment and standard descriptors, plus any adapter changes. Non-UTF-8 arguments are refused.
- **Meaning.** The execution digest covers the executable's path, device, inode, size and modification time, the working directory's identity, the argv and environment changes after adaptation, whether standard input is a terminal, and the resource intent. The CLI enforces no timeout and records that as the largest value.
- **Projects and budgets.** `--project ID` requires a project the operator registered, whose root contains the working directory and whose `.devguard.toml` names the same project. The project's `.devguard.toml` is opened without following a symbolic link or blocking, and must be a regular file. The request takes explicit `--cpu`, `--memory` and `--tasks` first, field by field, then the project's limits, then the adapter's default. The generic default is 1,000 mCPU, 1 GiB and 32 tasks, an unqualified initial value. A request above the project's limits is refused. Admission is never forced.
- **Waiting.** Without `--wait`, a denial ends the run. With `--wait DURATION`, at most 24 h, a denial for capacity or pressure, or a full instance pool, is retried as a new attempt. Retries start after 250 ms and double up to 10 s until the deadline. Each denied attempt is a terminal record, known not to have started, that stays in the journal as a tombstone until its generation is retired; the backoff limits a long wait to one such record every 10 s. Before waiting, the CLI compares the request with the host's work capacity, which it derives from the operator configuration and the observed host as the service does. A request that can never fit is refused at once, with no attempt. When that capacity cannot be observed, the receipt says so and the authority decides as usual. A signal cancels the wait and ends the CLI by that signal. Other refusals are not retried.
- **Lost replies.** A lost admission reply is replayed once with the same key. A lost launch-commit reply is looked up. A committed grant is then reported as never received (`AbandonLaunch`); once it is released as `NoHelperCreated`, a new attempt may follow within the wait. A grant that cannot be confirmed or released is reported and never retried. A new attempt only ever follows a known non-start.
- **Launch.** The CLI starts the helper as its direct child, already leading its own process group. When a standard descriptor is the CLI's controlling terminal and the CLI holds the foreground, the CLI hands the terminal to the workload's group, so terminal keys reach the workload directly; input may be redirected while output still reaches the terminal. It takes the terminal back when the workload stops, and once it has exited, before the root is reaped. A helper refused before READY is reaped, and its unclaimed grant is released as `NoHelperCreated`; a transient refusal may be retried within the wait, unless a signal reached the CLI during the launch, which cancels the run.
- **Launch transcript.** The CLI reads the helper's transcript to its end on a separate thread while it waits for the root, so a stop before READY is mirrored at once and no deadline applies. The helper's end closes when the executable starts or when the helper exits. A transcript that ended without READY means the executable never ran. A transcript that cannot be read to a well-formed end, READY without a final report, or a root that can no longer be waited for makes the result `uncertain`: the CLI reports that whether the command started is unknown, exits with the root's own status and never retries. It still reports that it holds no helper, so an unclaimed grant can settle; a claimed grant is settled by its scope.
- **Signals.** SIGINT, SIGTERM, SIGHUP and SIGQUIT sent to the CLI are forwarded to the workload's process group, only while its root is unreaped, so the group ID cannot name another group. A signal the CLI inherited as ignored, as under `nohup`, stays ignored and is not forwarded; the workload inherits the same disposition. A job-control stop of the workload (SIGTSTP, SIGTTIN or SIGTTOU) is mirrored: the CLI takes the terminal back and stops itself by the same signal, so its shell regains control. When continued, it hands the terminal back and continues the workload. Any other stop, such as SIGSTOP or a tracer's, is left to whoever caused it, and the CLI keeps waiting.
- **Observe before reap.** The CLI waits for the root to exit without reaping it, asks the authority to `Observe` the attempt while the exited root still holds its PID, then reaps it and observes again. Survivors left in the group are therefore tracked, and keep the attempt charged until they end.
- **Exit.** The CLI exits with the workload's status, or ends by the same signal; before it ends itself by a signal it sets its own core-file limit to zero, so only the workload may leave a core dump. It exits 125 when nothing was started, as `env` and `timeout` do, and passes the helper's 126 and 127 through.
- **Receipts.** `--receipt PATH` creates a new private file before anything is admitted, and writes `devguard-exec-receipt/v1`: the result (`completed`, `exec_failed`, `not_started` or `uncertain`), the command, the adapter's report, the budget and its source, the project, the authority, every attempt with its reservation and lease ID, the wait with the work capacity it was checked against, the helper's phases, the exit, the observations before and after reap, and the signals received, forwarded and inherited as ignored. It names the variables an adapter set or removed, never their inherited values, and never contains a permit or a caller credential.
- **Doctor.** `devguard doctor` reports the paths, the configuration, the helper, the service's identity, capabilities and status, and optionally registration and a project. It states whether managed execution is available, and that there is no unmanaged fallback. `--require admission,registration,macos-cooperative` makes it exit 1 unless each requirement holds.
- **Nested execution.** A workload that runs `devguard exec` again starts a separate managed execution under its own reservation and scope. It is not charged to, or contained by, the outer scope. Bounding nested work within a parent's budget takes an explicit parent lease (`--lease`, C10).
- **Adapters.** C07 defines the adapter interface, separate from admission. An adapter may change arguments and environment for the reservation it will run under, and it reports what it did. The generic adapter changes nothing. The Cargo adapters are described below.

## Cargo adapter

DG1-C08 adds the Cargo adapters (`devguard-cargo`), which fit Cargo's compiler parallelism to the reservation a command runs under. Cargo jobs bound compilation only; they do not cap the threads of the test programs Cargo runs. Memory is an accounting estimate, not measured enforcement.

- **Estimate.** One compiler job needs one logical CPU and 1.5 GiB, plus a fixed 512 MiB per build. A reservation therefore fits min(CPU ÷ 1,000 mCPU, (memory − 512 MiB) ÷ 1.5 GiB) jobs. A reservation that cannot fit one job is refused before admission; a job is never forced. The Cargo default request fits two jobs (2,000 mCPU, 3.5 GiB and 64 tasks), an unqualified initial value.
- **Selection.** `--adapter cargo` runs `cargo` itself and refuses any other program. `--adapter cargo-pipeline` serves a program that runs Cargo, such as a validation script. `auto` selects the Cargo adapter when the program is `cargo` with a subcommand that compiles (build, check, test, bench, run, doc, clippy, rustc, rustdoc, install, fix and their aliases), and the generic adapter otherwise, recording why.
- **Direct mode.** The adapter reads Cargo's arguments up to `--`. An explicit `-j`/`--jobs`, written `-j N`, `-jN`, `-j=N`, `--jobs N` or `--jobs=N`, is first resolved as Cargo reads it: `default` is the host's logical CPUs, and a negative value counts back from them but never below one. A repeated, zero or unparsable value is refused, whether or not a jobserver governs. A value within the reservation is kept, and a larger one is rewritten in place to the reservation's jobs. Without one, and without a governing jobserver, the adapter inserts `--jobs N` after the subcommand, which takes precedence over `CARGO_BUILD_JOBS` and configuration. `CARGO_TARGET_DIR`, report paths and command selection are untouched, and the receipt records the original and applied jobs and why. An unsupported subcommand under `--adapter cargo` is refused.
- **Pipeline mode.** The adapter creates a private FIFO jobserver with N−1 tokens, for at most 1,024 jobs, in a new 0700 directory; the CLI holds it open for the run and removes it afterwards. It is exported through `CARGO_MAKEFLAGS`. Every Cargo the program starts, directly or nested, shares that pool and ignores its own `-j` while the jobserver is valid. A FIFO is used because it survives programs, such as Python's `subprocess`, that close inherited descriptors. Each of k concurrently started top-level Cargo runs still adds its own implicit job, so together they run at most N−1+k jobs. After the run, the receipt records how many tokens were back in the pool.
- **Fallback.** Both modes set `CARGO_BUILD_JOBS` to the reservation's jobs N, replacing any value the caller set. Cargo uses it only when it opens no jobserver and neither its command line nor `--config` gives jobs, so it bounds a Cargo that could not open the pool, without the warning an explicit `-j` causes under a valid jobserver. The receipt lists it among the variables set.
- **Inherited jobservers.** A jobserver the CLI inherited through `CARGO_MAKEFLAGS`, `MAKEFLAGS` or `MFLAGS` is checked the way Cargo will read it: the first variable present must name an open, inheritable pipe pair or a FIFO the user owns. A valid one is preserved and bounds parallelism in both modes, as the approved design requires, and the receipt states that its size cannot be observed; no second pool is created. Under it, direct mode inserts no jobs value, because Cargo would only warn that it ignores one, but an explicit value is still clamped. A descriptor-pair jobserver reaches only programs that keep inherited descriptors open. In pipeline mode, a Cargo started by a program that closes them, such as Python's `subprocess` by default, falls back to `CARGO_BUILD_JOBS`, and the receipt says so. A stale one, such as closed descriptors, is removed from the environment together with the job counts that came with it, so Cargo does not silently fall back to its own pool.
- **Nested Cargo.** Cargo passes its jobserver to build scripts, so a nested Cargo shares the outer pool rather than creating another.

## Installation and the current-user service

DG1-C09 installs a packaged release as the current user's LaunchAgent. The service still runs the same foreground `devguardd serve`; it is not a privileged daemon, and a worktree `target` binary is never the installed service.

- **Package.** `scripts/package.py` requires a clean tree and Rust 1.95.0. It builds `devguardd`, `devguard` and `devguard-launch` in one release build, labelled bootstrap until C10, and writes `devguard-release-manifest/v1` with:
  - a release id: `<version>-<commit7>-<artifact digest>`;
  - the source commit, tree and qualification tree digest;
  - the build command and toolchain;
  - each binary's SHA-256 and size;
  - the build's compiled compatibility, which `devguardd version --json` prints: package version, wire version, protocol, capabilities, journal schema and configuration schema.

  A package is a functional artifact (`scope: functional`, `slo_qualified: false`); only C12 qualifies a release.
- **Validation.**
  - A package or installed release holds exactly `MANIFEST.json` and `bin/` with the three binaries. They must be regular executable files, not symlinks, whose sizes and hashes match the manifest.
  - The manifest's compatibility must equal the installer's own build.
  - The installer must run from the package: its own executable must hash to the manifest's `devguardd`. Binaries from different builds are therefore never mixed.
- **Install.** `devguardd install --package DIR` refuses while:
  - an authority serves the canonical endpoint or holds the lock;
  - the agent is loaded or its plist exists;
  - the authority state is absent. It never creates, repairs or rewrites the journal.

  It copies the release into a private sibling, syncs it and makes it read-only: directories and binaries 0500, the manifest 0400. Only then does it rename the copy to `releases/<id>`, so a partial copy never has the final name. An existing release is reused only when its manifest is byte-identical; a different release never overwrites it. Finally it writes `~/Library/LaunchAgents/io.github.novelkr.devguard.plist` (0644) and bootstraps it into the user's `gui/<uid>` domain.
- **Agent.**
  - The program is the release's `devguardd serve`, and `RunAtLoad` starts it at load and login.
  - `KeepAlive {Crashed: true}` restarts it only after a crash, after a 10-second throttle.
  - A clean exit leaves it stopped. That includes launchd's SIGTERM and a fail-closed exit, such as a missing or corrupt journal or a second authority refused by the lock.
  - Output goes to `~/Library/Logs/DevGuard/devguardd.log`; rotation is manual.
- **Verification before selection.** Within 15 seconds the installer must observe:
  - the service launchd reports, with the same PID as the endpoint's handshake;
  - that PID's executable image (`proc_pidpath`) being the release's `devguardd`, with the manifest's hash.

  Only then does it record `releases/selection.json` (0600: current and last known good, with a history) and keep an immutable recovery copy under `recovery/<id>`. If verification fails, or the recovery copy or the selection cannot be written, the job is booted out and the plist removed; nothing is selected.
- **Status.** `devguardd status` changes nothing. It reports:
  - the selection and the launchd state;
  - whether the plist is exactly the one this build renders for the current release;
  - the running PID, executable and hash against the manifest;
  - the releases and recovery copies.

  It exits 0 only when the service runs the verified current release, or its recovery copy, with admission open. It reports an admission closure on a release that honours it.
- **Restart.** A restarted service reopens and reconciles the existing journal, as any start does. A missing or corrupt journal fails closed and stays down.
- **Limits.** Replacing the installed release is an upgrade (C11, below); installation refuses it. There is no uninstall command. `launchctl bootout gui/<uid>/io.github.novelkr.devguard` and removing the plist stop the service and keep every release, recovery copy, the selection and the journal.

## Parent leases and candidate authorities

DG1-C10 lets the stable authority lend one bounded budget, a parent lease, to the verification of a candidate build, instead of issuing the host's budget a second time.

- **Capability.** The service states `parent_lease` only to a client whose handshake requires it, so an earlier client never receives a capability it cannot decode. Clients send lease requests only after that statement.
- **Lease.** A registered workload owner requests `AdmitLease {key, budget, ttl}`.
  - The lease is admitted against the host's capacity at the current pressure, like any request. Its whole budget stays charged until it is released.
  - Only the first reply carries a one-time 64-character token; a replay returns the lease without it. The journal keeps only the token's digest, and receipts never contain the token.
- **Children.** `AdmitChild {lease, token, request}` from a registered instance of the lease's consumer and generation admits an ordinary attempt against the lease's remainder: its budget less the reservations of its charged children.
  - The host's capacity and pressure are not consulted again, so a live lease is never shrunk.
  - A wrong token or an unknown lease is `Unauthorized`. A child larger than the remainder is denied `ResourceUnavailable`, and a lease that is no longer active denies every child `InvalidTransition`.
  - An admitted child is launched, reconciled and released like any attempt, through the stable `devguard-launch`. Its release returns its budget to the lease, not to the host.
- **End.** A lease is Active, then Ending, then Released.
  - It becomes Ending when its owner ends it (`EndLease`), when its owner's process ends or belongs to an earlier boot, or when its deadline passes. An Ending lease admits no child.
  - It is Released once none of its children is charged, and its budget returns to the host. A Suspect child keeps the lease charged. The reconciler settles leases on every pass.
- **Holders.** `LeaseStatus {key, token}` is a session of its own: it presents only the token, is never authenticated as a caller and can make no other request. It reports the lease's phase, budget, remainder, deadline and children.
- **Journal.** The tables `leases` and `lease_children` are added when absent, and the journal schema stays 1. Committed capacity is the budget of every unreleased lease plus the reservations of charged attempts that are not lease children. A generation holding an unreleased lease cannot be retired. Activation fails closed unless every child link names a lease the journal holds and no charged child belongs to a released lease. A C09 artifact that opens such a journal ignores the new tables: it counts the children as ordinary attempts and does not hold the unused remainder of a lease.
- **Candidate authority.** `devguardd candidate --id ID --lease CONSUMER/GENERATION/ATTEMPT --capacity MILLICPU,BYTES,TASKS --token-fd N` serves an isolated authority whose whole capacity is a parent lease of the account's authority.
  - It reads the lease token from the inherited descriptor N and closes it. Without the token it contacts and creates nothing.
  - It asks the parent for the lease's status as a holder. The lease must be active and hold the capacity. A capacity that does not exceed the candidate daemon's own reservation (250 mCPU, 128 MiB and the bootstrap system tasks) is refused before anything is created.
  - Its state, configuration, credentials, socket and cache live under `candidates/<id>` in the authority root and runtime directories, and must not exist yet: each candidate starts from new state. It never opens the normal state, credentials, releases or recovery copies.
  - Its policy has the leased capacity and no host headroom; the host's capacity is not used. It samples host pressure like any authority.
  - It advertises durable admission, per-resource evidence, static control reservations and macOS cooperative control, but neither fenced launch nor parent leases. It refuses launch, helper and lease requests with `ResourcePolicyUnsupported`. Its status reports registration ready and execution not ready, with a reason that names the candidate and its lease.
  - It confirms the lease every second and closes as soon as the lease is no longer active. It fails closed when the parent refuses the token, or cannot confirm the lease for five seconds.
  - The parent charges the capacity when it admits the candidate as a lease child, so the candidate's admissions can never exceed the lease.
- **Lease children from the CLI.** `devguard exec --lease CONSUMER/GENERATION/ATTEMPT --lease-token-fd N` admits the command as a child of that lease. It reads the token from descriptor N first and closes it, so the workload never inherits it. It requires the parent-lease capability, records the lease in the receipt, and checks a wait against the lease's budget instead of the host's capacity.
- **Test-candidate.** `devguard test-candidate --candidate DIR --report DIR [--cpu MILLICPU] [--memory SIZE] [--tasks N] [--ttl DURATION] [--wait DURATION]` verifies a candidate tree under one lease of the stable authority:
  1. It admits the lease: by default 2 CPU, 4 GiB and 96 tasks, with a two-hour deadline. `--wait` asks again while capacity or pressure refuses it.
  2. It runs each workload as a lease child through `devguard exec --lease`: the tree's `cargo build` of the daemon, CLI and helper, then `cargo test` of the crates whose tests create no process group or session and observe no host capacity (contract, core, client and the Cargo adapter), then the tree's own `devguardd candidate` with 1 CPU, 1 GiB and 64 tasks.
  3. From outside, it checks the candidate's endpoint: its capabilities and status, an admission within its capacity, a refused launch, a cancellation, a denial beyond its capacity and a refused nested lease.
  4. It ends the lease, waits for the candidate to close and the lease to be released, and removes the candidate's area. It writes `report.json` with the lease, each child's receipt and reservation and every check, and exits 0 only when all of them passed.

  A failure ends the lease first. Children that are running finish, and the lease is released once they are settled. If the command itself is killed, its lease ends by the owner rule.
- **Limits.** Workloads that create their own process groups or sessions would leave a lease child's scope and stay Suspect under the cooperative macOS model. The native launch, CLI, terminal and scope suites therefore remain bootstrap and CI qualification runs, not lease children. A candidate verifies admission only; its workloads never run through it. Real self-use under the installed parent needs a release that states parent leases. While host memory pressure keeps a service Critical, it admits no lease.

## Upgrade and repair

DG1-C11 replaces and repairs the installed service without losing or duplicating charged work, and without depending on a candidate's admission.

- **Staging.** `devguardd stage --package DIR` validates a package and copies it into an immutable `releases/<id>`, as installation does. It must run from the package, and it leaves the service, the selection and the journal untouched.
- **Closing admission.** An administrator session can close admission (`CloseAdmission {reason}`), reopen it (`OpenAdmission`) and ask what is still charged (`Quiescence`). The service states `upgrade_drain` only to a client that requires it.
  - Closing writes a private marker, `state/admission.json`, before it takes effect, so a restarted service or the next release starts with admission still closed. Every Prepared attempt is cancelled and is known not to have started.
  - While admission is closed, admissions, launch commits, parent leases and lease children are refused with `ResourceUnavailable`, which a waiting CLI retries. Queries, cancellations, stops, owner reports and reconciliation continue. The status reports execution not ready, with the closure's reason.
  - Closing again keeps the first closure; reopening removes the marker durably before it takes effect.
  - A report names how many attempts and leases are charged, and at most 16 of each, so it always fits one frame.
  - A release before C11 cannot decode these requests and closes the connection unanswered. Whether a release can close admission is therefore read from what its manifest states; a release serving without native evidence states no capability and admits nothing, so it is treated as one that cannot.
- **One operation at a time.** Installation, staging, upgrade, repair and reopening hold a private operations lock, `operations.lock` in the authority root, and refuse while another holds it.
- **Upgrade.** `devguard upgrade --release ID [--drain-timeout DURATION] [--stopped]` must run from the staged release's own `devguard`.
  1. It refuses a release that speaks another wire version or protocol, reads another journal or configuration schema, or lacks durable admission or fenced launch, which consumers require. A release that states less than the current one is reported as a downgrade and allowed only within those limits. An incompatible downgrade is refused before anything changes.
  2. The current release must be running verified. The staged release's recovery copy is made before anything changes.
  3. It closes admission at the running service and waits until no attempt or lease is charged. If the drain does not finish within the timeout (60 s by default), or SIGINT or SIGTERM cancels it, admission reopens on the current release, which keeps every charge, and nothing is replaced. A signal received before the service is stopped also ends the upgrade with admission reopened; from the stop on, the replacement completes or rolls back, and `launchctl` runs in its own process group so a terminal's interrupt does not reach it. A release before C11 cannot close admission: `--stopped` stops it first and proceeds only if its journal then charges nothing; otherwise that release serves again.
  4. It stops the service. While holding the authority lock, it takes a quiescent backup under `backups/<time>-<from>-to-<to>/`: the journal as a complete SQLite copy, the selection and the current manifest, each hashed.
  5. It writes the closure marker and starts the new release, which therefore starts with admission closed. It verifies that launchd runs the release's own `devguardd`, that a consumer's handshake succeeds, and that the service reports admission closed with nothing charged.
  6. It records the new release as current and keeps the release it replaced as the last known good one, then reopens admission.

  If a step fails once the drain has finished, the previous release serves again on the same journal, with admission reopened. A release that still serves, because stopping it failed, only has its admission reopened. Otherwise the new release, if it started, is booted out, whether it still runs or died while it was verified, and the previous one starts again once the endpoint and the lock are free. The backup is never restored over a journal that a release may have admitted from.

  An upgrade interrupted after the new release started is completed by running it again: it records the release if the selection does not name it yet, provided it serves closed and idle as an upgrade starts it, and reopens admission if it is still closed. A serving last known good release is left to repair. `devguard admission --open` reopens admission on the serving, selected release; for a release before C11 it only removes the marker, which that release never reads.
- **Repair.** `devguard repair --use last-known-good` returns the service to the last known good release: the one the last upgrade replaced, or the installed release when there has been no upgrade. The service runs only that release's own binaries, whichever `devguard` runs the repair.
  - It refuses while any authority serves: it never starts a second one, and a serving release is replaced by an upgrade. The one exception completes an interrupted repair: when the last known good release already serves verified but is not yet selected, repair records it and reopens admission.
  - The journal must open under the authority lock. One that cannot keeps admission closed; repair never creates, resets or restores it.
  - The release must read the journal's schema and serve the same consumers. One without parent leases is refused while the journal holds unreleased leases.
  - If the installed release is damaged, it runs the recovery copy instead, which status and a later upgrade then accept.
  - It replaces any job left loaded once it has let go of the endpoint and the lock, and verifies the running binary. Booting out a service that is not loaded succeeds, although `launchctl` exits 3 for it. It records the repair in the selection as soon as the release serves, and then reopens admission left closed by an interrupted upgrade.
- **Limits.** An upgrade needs the service idle: running work is waited for, never interrupted. A release before C11 has no `upgrade` command. Returning to one uses its own installer after the service is stopped, and such a release ignores the closure marker.

## SLO qualification (C12)

DG1-C12 measures a release instead of inferring responsiveness from functional success. The harness is `devguard-qualify` (`crates/qualify`, never part of a release package) with `scripts/measure.py`. It measures the installed service's own release (or the recovery copy the service runs), its policy and its host; a worktree binary is never the measured artifact.

- **Control probe.** `devguard-qualify control` registers as an ordinary `dev-cli` owner of the canonical authority, like `devguard`, with no path or authority override. It measures only attempts it owns: small `/bin/sleep` targets (50 mCPU, 16 MiB and 2 tasks). Each target is admitted, committed and started through the measured release's `devguard-launch`, in the working directory its execution meaning names, and observed before it is reaped. A target lives at most ten minutes beyond the sampling, so an abruptly killed probe leaves no target behind for long.
  - **Status.** A status sample is a `Lookup` of the running target through a fresh registered session: connect, hello, authenticate, register, lookup. The probe takes one each second, the way the command-line owner makes every call. An answer about a target that no longer runs is not a status sample.
  - **Termination.** Every 20 s the probe starts a fresh target and times `Terminate(SIGTERM)` until `Terminated`. The sample is effective only if the service signalled the scope completely and the target then exited by itself. When the target does not exit, the probe stops its own child, marks the sample as forced, and does not count that release as evidence. The target's exit and the attempt's release are reported separately.
  - **Failures to start.** A target that cannot be started is recorded with its stage and error, and the error is kept as a missing sample. A helper that ends before READY is reported as `HelperExited`, as the command-line owner does, so its grant is settled.
  - **Admission.** An admission refused for capacity or pressure is retried and recorded, but a target that waits out its admission is not a sample.
  - **Missed slots.** Every slot that passes while an earlier call is still in flight is recorded as missed.
  - **Stopping.** SIGINT, SIGTERM, SIGHUP or the loss of its parent ends sampling; the probe still settles every target it started.
- **Foreground fixture.** The fixture is a fixed local page, `crates/qualify/fixture/foreground.html`, hashed in every run and repetition report. It runs in one Google Chrome instance per run with a fresh profile, driven over `--remote-debugging-pipe`: there is no listening port and no flag that changes scheduling or throttling. Each repetition reloads the page.
  - **Input.** The browser synthesizes it with `Input.dispatch*`: a key, a click or a wheel step in each slot of 500 ms ± 100 ms. A slot that passes while an earlier dispatch is still in flight is a missing sample.
  - **Input to next paint.** It runs from the event's timestamp to the next frame after the input's visible change, marked by a message posted from the next animation frame. It is raised to the browser's own Event Timing duration when the browser reports one. The browser reports none for wheel input, so wheel samples rest on the page's estimate alone. The measure excludes the operating system's input path before the browser.
  - **Frame stall.** A gap of more than 500 ms between consecutive animation frames.
  - **Late replies.** A drain whose reply comes late keeps that reply, so its data is not lost.
  - **Crashes.** A crashed page or browser fails the interval.
- **Load.** Six consumers run concurrently through the measured release's `devguard exec --wait`, so admission, pressure and queueing are the service's own. Every run's receipt is kept, and a run that ends at once without success is followed by a pause.

  | Consumer | Workload | Budget |
  | --- | --- | --- |
  | Cargo | The release's own source (`--adapter cargo-pipeline`): a workspace build, then the core and contract tests, with one compiler job | 1 CPU, 2 GiB, 24 tasks |
  | CPU | 1 thread | 1 CPU, 128 MiB, 4 tasks |
  | Memory | 1 GiB touched and held | 100 mCPU, 1.25 GiB, 4 tasks |
  | I/O | 256 MiB written, synced, read back and removed, then idle until 30 s have passed | 100 mCPU, 384 MiB, 4 tasks |
  | Output pressure | 256 MiB to standard output over 30 s | 250 mCPU, 128 MiB, 4 tasks |
  | Slow input | Reads one line per 100 ms, fed once the payload runs | 50 mCPU, 64 MiB, 4 tasks |

  Together with the probe's targets, the budgets total about 2,600 mCPU, 4 GiB and 48 tasks. That fits half the local host's work capacity (5,500 mCPU, 11.75 GiB and 144 tasks), which is the Constrained capacity. Memory warning therefore neither stops the load nor starves the probe's targets. Under Critical pressure nothing is admitted, and the interval is inconclusive.
- **Protocol.**
  - There are two combinations. In `cold`, each Cargo build uses a fresh target directory of its own, which is removed after the interval. In `warm`, the Cargo builds reuse a target directory built before the repetition; a warm repetition whose build fails is not measured.
  - Each combination runs three repetitions: 10 minutes idle, then at least 30 minutes of load.
  - Work started before the load deadline is observed to completion, within 30 minutes. Nothing starts after the deadline.
  - The run refuses to start while anything is charged. It refuses to start unless `devguard doctor` reports a ready service. It records the harness's source, its binary and the service's configuration fingerprint.
- **Percentiles and missing samples.** p99 is nearest-rank over one interval's raw values.
  - A missing sample ranks above every value. Missing samples include: an input skipped, never handled or never painted; a status call without a reply, about a target that no longer runs, or missed; and a termination with an error or a missed slot.
  - A missing input also counts as a response over one second.
- **Validity.** An interval is a valid observation only if all of these hold:
  - the fixture runs headful and stays the frontmost application;
  - its page stays visible and focused, with no visibility or focus change;
  - the screen stays unlocked;
  - validity samples cover 90% of the interval;
  - the service runs the measured release, under the same process, at every check (one a minute);
  - nothing else was charged when the interval began;
  - the probe finished normally;
  - the load completed within its limit.

  `caffeinate` keeps the display and the system awake. An invalid interval is `inconclusive`, never a pass.
- **Verdicts.** An invalid interval is inconclusive. Otherwise an interval fails if any target is missed:
  - status: p99 ≤ 500 ms and no connection loss;
  - termination acknowledgement: p99 ≤ 1 s, no connection loss, every termination effective, and every effective target released;
  - input to next paint: p99 ≤ 100 ms and none over 1 s;
  - frames: no stall, and no crash;
  - load: no uncertain execution, connection loss or duplicate launch, checked against the journal;
  - the harness's own attempts: all released within a minute.

  With every target met, an interval is still inconclusive in two cases. The first is too few samples: under 90% of the schedule, or 80% for terminations whose targets were refused for pressure. The second is load that was not the declared load: every consumer needs a successful run and at least a quarter of the interval, and admitted load needs half of it. A repetition passes only if both its idle baseline and its load pass; a failing idle baseline makes it inconclusive. A combination is qualified only when all three repetitions pass, and values are never pooled across repetitions. A run is qualified only when both combinations are, for the full protocol, on the approved 8-logical-CPU, 16 GiB target. A rehearsal (shortened intervals) or a headless fixture is always inconclusive.
- **Promotion.** `measure.py promote` recomputes the verdict from the preserved repetition reports, checking their hashes against the summary.
  - It refuses unless the run is qualified.
  - It also refuses if the release manifest changed, the service no longer runs the release, the policy differs from the measured one, or the host differs from the measured one. The policy is the host.toml hash, the configuration fingerprint and the capabilities; the host is its OS build, CPU, logical CPUs and memory.
  - It writes `qualifications/<release>.json` in the private state directory: a read-only `devguard-release-qualification/v1` record. It is written through a temporary file and a link, which never replaces an existing record.
  - The record names the release, its manifest and artifact hashes, the policy, the environment, the harness and plan, every repetition's verdict, the evidence hashes, and the verification that the service runs that release. The release manifest stays unchanged: `slo_qualified: false` describes the package, not the promotion.
- **Interruption.** SIGINT, SIGTERM or SIGHUP, or a harness failure, stops new work. The harness then asks every live owner, probe and browser to stop, and each owner settles its scope. It exits 130 when interrupted and 3 when it failed, and nothing is promoted.
- **Limits.**
  - This qualifies standalone control, development and bounded self-use on the measured host, artifact and policy only; another host or release needs its own measurement.
  - Passing the fixture does not guarantee every website.
  - Protected-data GC is not applicable in DG-1, and CodeSpace MCP, approvals and replay belong to CSRG-C08.
  - Linux enforcement remains unqualified.

## Durable admission and launch

The journal schema is version 1. Initialization is explicit and only creates a new file. Opening a missing, corrupt, unknown-schema or inconsistent journal fails closed. SQLite uses WAL, FULL synchronous durability and immediate transactions. A startup scan validates the accounting index against every stored record.

The attempt key is `(consumer_id, consumer_generation, attempt_id)`. The request fingerprint incorporates both the caller's versioned execution digest and the resource intent. It excludes transport request IDs and the current policy revision. Replays return the original durable reservation or terminal result; a changed meaning or owner conflicts. A refusal is itself a terminal attempt, so a later independently requested admission uses a new attempt ID.

`begin_launch` durably consumes Prepared and returns a one-time `Secret` permit. Subsequent calls return the stored attempt without another spawn grant. The journal stores only the permit digest. If the first response is lost, the caller must reconcile; it cannot recreate a helper by replaying the transition. `verify_launch` checks a grant without changing it. `claim_launch` durably records the first helper's established scope before binding; afterwards no other scope can claim, bind or be authorized, and `bind_scope` accepts only the claimed scope.

`bind_scope` requires a fresh Backend witness binding the attempt, owner, exact process identity, scope and fully applied resource plan. Freshness of backend evidence, here and in reconciliation, is judged against the clock read after the backend returns. Evidence observed while the transition runs is therefore not treated as future, while evidence older than two seconds or from another boot is still rejected. C04 corrected this ordering after real-clock evidence exposed it; fake backends with a fixed clock could not. `authorize_run` requires the bound helper identity and permit. Only its first successful response sets `may_exec=true`. Losing this response therefore leaves an uncertain/fenced execution, not permission to spawn again. A RunAuthorized record is not proof that the user executable started successfully.

Cancellation of Prepared is terminal and returns its reservation. Cancellation after launch commit becomes Draining and blocks late binding/authorization while retaining the reservation. Cancelling a Draining, Suspect or terminal attempt changes nothing, and a prepared attempt past its deadline reports its expiry. Prepared alone expires after five seconds. Restart preserves Prepared with its original boot-relative deadline, and converts post-commit nonterminal attempts and registered instances to Suspect for reconciliation.

## Reclamation evidence

There is no unconditional `release(lease_id)` API. For a bound execution the backend must identify the exact scope and provide fresh evidence of root termination/reap, an empty scope, no surviving known members, complete tracking and no known escape. A previously lost track remains sticky across ordinary empty-group observations. Clearing it requires explicit reconciliation evidence or verified termination across a host reboot.

For an unbound committed launch, missing PID data or owner death is insufficient. The backend must positively rule out helper creation and every pending spawn; on macOS that evidence is the owner's report described under reconciliation. A host reboot can also resolve an old execution. `release_reason` distinguishes scope termination, proven absence of a helper and previous-boot termination. `AttemptRecord::known_not_started()` is true for prelaunch terminal refusals and a released `NoHelperCreated` result; resource release alone is not retry evidence.

No tombstone is deleted by a normal TTL sweep. Operator retirement requires all instances retired and no charged attempts. It records a permanent retired generation before compacting its terminal attempts, so old keys remain rejected.

Static control reservations are subtracted for all configured slots, including disconnected/offline services. Instance retirement makes a slot reusable but does not return the static reservation to workloads. Observed process identity includes boot ID, PID and start ticks to detect PID reuse.

The journal records the registration policy for each instance. Startup rejects removing a consumer or changing its generation, role, UID, instance limit or reservation while any instance remains active/suspect. Reconcile and retire those instances under the existing configuration first. Slot counts span consumer generations. This prevents a configuration restart from allocating a second control service against the same static reservation.

## Pressure and capability interpretation

The controller starts closed until the first valid host sample. A first healthy observation establishes readiness; after observed pressure or a sampling failure, recovery follows the design's 30-second steps. Samples use the same boot-relative monotonic clock, reject replay/future timestamps, and expire after six seconds. Downward targets do not reduce live lease amounts.

Every resource carries its own level and method. Accounting is not an OS memory cap; QoS cannot claim a memory or task limit; kernel controls require a contained cgroup scope. A plan is validated before admission and remains separate from AppliedResources. Compatibility checks require both an accepted protocol version and all requested capabilities; they do not launch a fallback authority.

## Boundaries deliberately left to later milestones

- Remaining DG-1: measured macOS SLOs and the promotion of a measured release (C12).
- CS-RG: Runner slots and transport lanes, approval migration, pinned client, process status integration and regression qualification.
- DG-LINUX: actual cgroup hierarchy, controllers, ancestor constraints and sandbox/proxy inclusion.
- DG-CACHE / DG-ADAPTERS: registered cache reclamation and additional tool-specific controls.

The journal and library do not own a user's process handle, process output, CodeSpace workspace lease or approval row. None of those lifetimes is inferred from the lifetime of a resource reservation.
