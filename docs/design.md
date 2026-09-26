# DevGuard design reference

Reference date: 2026-09-22; revised 2026-09-27 by [design revision 1](design-revision-1.md). Local source: `/Volumes/DevData/Projects/IdeaProjects/DevGuard`.
This is the **English editorial reference** for ongoing development, with a [maintained Korean counterpart](ko/design.md). The complete historical approval remains byte-for-byte in [design.ko.md](design.ko.md), verified by [design-source.json](design-source.json). This reference consolidates that design, the subsequently approved [decisions](planning/decisions.md) and the design revisions the user directs, such as [revision 1](design-revision-1.md); it does not claim to be the original approval artifact. The historical approval preserves the decisions of its time, this reference is the current editorial baseline, and milestone status changes only with actual implementation and verification. Concrete work boundaries and source mapping are in [planning](planning/README.md); [contracts](contracts.md) describe implemented behavior only.

## Purpose and trust boundary

DevGuard centrally accounts for development resources on the actual execution host. Multiple repositories share one authority rather than independently issuing full host budgets. CodeSpace consumes budgets/policies while retaining authorization, approvals, workspace coordination, process handles, PTY, I/O, timeout and termination. DevGuard also governs its own candidate development under a tested parent.

Separate three claims: accounting excludes reserved control capacity from workloads; the OS actually applies supported controls; measured control/foreground behavior passes SLOs. macOS accounting does not prove tree-wide kernel limits or responsiveness.

The initial trust scope is one operating account and registered **cooperative workloads**. This is not containment against malicious same-UID programs, unrelated UIDs or unregistered apps. External use contributes to observed host pressure. Known process-group escape, identity mismatch or tracking loss remains Suspect. Do not use macOS NOTE_TRACK, which the installed SDK marks unsupported, to claim descendant tracking.

## Responsibilities and identity

The daemon owns capacity, static reservations, admission, leases, pressure policy and eventual registered-cache policy. The launcher verifies pre-payload scope and policy; it is not a general process server. The CLI controls commands it starts. Product adapters translate policy without moving product types into authority core. Contract/client, core, native backend, launcher, daemon/CLI and language adapters remain separate. The contract, core and generic client contain no Codex product types, model sessions, CodeSpace workspace authority or PTY ownership. DevGuard's default distribution and shared client currently build, test and release without CodeSpace or Codex, and CodeSpace pins the small client by full SHA. This is the present engineering choice, not a permanent prohibition: reusing a low-level utility in an execution or platform adapter is decided by the code it actually replaces, contract fit, dependency propagation, recovery path and requalification cost ([revision 1, D3](design-revision-1.md#103-d3--devguard-dependency-policy)).

One normal authority exists per operating account/executor host, using canonical protected state and exclusive ownership. Alternate socket/state paths cannot create another normal budget. Remote workers consume their execution host's authority; physical host and guest capacities are not summed as independent headroom.

`consumer_id` identifies operator registration, `instance_id` binds PID/start/boot, and `attempt_id` identifies an execution independently of transport. Operator-configured maximum instances times per-instance control reservation are always excluded from workload capacity, even disconnected. Active/suspect/retired instances require reconciliation before slot reuse.

Authenticate socket parent 0700, socket 0600, OS peer UID/PID and separate consumer role/generation secret. UID alone cannot grant control_service. Separate caller-registration secrets, one-time helper permits and administrative authority. Transfer credentials through private FDs, then close them before user payload; never leave them in argv/environment/debug/journal. Project settings may tighten operator policy but cannot raise capacity/roles or impersonate consumers.

For CodeSpace, the execution-owning Runner registers one instance: Gateway PID for InProcess, worker PID for UDS. Each bounded session registers that instance again. A single static reservation covers Gateway and Runner. There is no separate service-exec path: the Gateway hands the consumer credential to a UDS worker through `CredentialHandoff`, and InProcess reads it directly. Service-plus-subworker registration is deferred until actual multi-Runner/shared-reservation need.

## Budget and pressure policy

CPU uses integer millicpu (1000 per logical CPU), memory uses bytes. These initial values are policy estimates pending qualification:

| Policy item | Value |
| --- | --- |
| Host CPU headroom | max(ceil(25% logical CPUs), 2 CPUs) |
| Host memory headroom | max(25% physical RAM, 4 GiB) |
| Configured CodeSpace control instance | 1 CPU / 512 MiB, initially at most one |
| Daemon | 0.25 CPU / 128 MiB |
| CLI control pool | Total 0.25 CPU / 128 MiB, at most eight concurrent CLIs |
| Workload scheduling | Utility QoS and nice +10 on macOS |
| Future GC | Background QoS, low I/O priority, bounded batches |
| Host samples / freshness | Every 2 seconds / at most 6 seconds old |
| Admission / Prepared lifetime | 250 ms / 5 seconds |

Base workload capacity = effective capacity − host headroom − static reservations. New allowance = max(0, pressure-state target − unreclaimed leases). Linux effective capacity also respects ancestors/controllers. Refuse if even the minimum work cannot fit; never force one worker. Record observed overruns and restrict new admission; transient RSS reduction does not release live reservations.

Normal permits base capacity. Constrained targets 50% after two consecutive samples of memory warning, 10-second page-out average ≥16 MiB/s, 10-second swap growth ≥64 MiB, or control-loop lag ≥200 ms; Linux additionally uses memory PSI full avg10 ≥2%. Critical blocks new heavy work on memory critical, target-volume free space ≤max(5%, 2 GiB), samples stale over 6 seconds, or two consecutive control-loop lag ≥1 second / Linux memory PSI full avg10 ≥10%.

Recovery requires normal memory, numeric pressure below half entry thresholds, valid samples for 30 seconds and disk above max(10%, 4 GiB); recover only one step at a time. High CPU utilization alone is not Critical. Lower targets never shrink live leases, change running Cargo jobs or automatically kill/replay arbitrary commands. These thresholds require measurement; [Linux PSI](https://docs.kernel.org/accounting/psi.html) defines the mechanism, not DevGuard's thresholds.

## Resource and durable execution contracts

Keep ResourceIntent, ResourceReservation, ExecutionPlan, AppliedResources and executable result distinct. For each CPU/memory/task resource report requested/reserved quantity, method, scope, required level (`accounted`, `cooperative`, `kernel`), application state and actual backend/timestamp. Default macOS is cooperative CPU and accounted memory/tasks; reject unsupported kernel requirements before payload. QoS/nice are not tree-wide hard caps or exclusive cores. Linux later uses actual delegated cgroup controls/readback; cpu.max is not exclusive cores and memory.min is not physical preallocation. See [Apple QoS](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/PrioritizeWorkWithQoS.html) and [cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html).

Idempotency key is `(consumer_id, consumer_generation, attempt_id)`. A versioned canonical meaning digest includes executable/cwd identity, argv, allowed environment changes, TTY, timeout and intent. Persist digest and accounting metadata, not raw command/environment. Same key/meaning returns the original result/policy; changed meaning conflicts; terminal keys remain terminal; lost replies reuse the same key. Only confirmed non-start allows an explicit new attempt. Uncertain execution retains conservative accounting and is never auto-replayed.

SQLite transactions durably record reservation, launch commitment and execution authorization **before** their respective responses. Restart restores relationships. Preserve terminal tombstones until a fully reconciled consumer generation is explicitly retired, and keep rejecting retired generations. Missing/corrupt/full journals close new execution; never silently initialize an empty ledger. This prevents duplicate allocation/dispatch but does not promise unconditional exactly-once OS execution.

Sequence: Prepared → LaunchCommitted → helper creation → ScopeBound/PoliciesApplied → RunAuthorized → helper READY → user executable attempt → Draining → Released. A one-time permit is bound to owner/attempt; replay supplies no new spawn authority. A lost execution-authorization response remains uncertain. Helper READY and CodeSpace's tracked-helper `confirmed` do not prove user exec success.

Prepared alone expires after its original five seconds. Cancellation/expiry atomically fence late commitment before return of capacity. After commitment, actual noncreation/termination evidence is required. Separate execution slots (acquired before spawn), resource leases and completed-output retention. Root reap/disconnect/timeout/missing scope alone never releases the whole workload.

macOS release assumes cooperative workloads remain in the observed process group and requires root reap, empty observed group, known members gone and no unresolved tracking failure. PID/start/boot identity prevents acting on reused PIDs. Linux later requires managed cgroup populated=0 plus helper exit, including sandbox/proxies. Reboot can prove prior-boot termination but never justifies clearing the journal. Empty scope does not imply instant page-cache/disk reclamation.

## Adapters and future cache management

Initial adapters are generic and Cargo. Generic reports unsupported parallel transformation honestly. Cargo direct and explicit cargo-pipeline modes preserve command meaning, target/report paths and valid inherited jobserver FDs. Jobs fit both CPU and a **512 MiB fixed plus 1.5 GiB/compiler job** memory estimate. Interpret explicit -j/--jobs/environment precedence, report clamping/rejection and preserve nested token ownership. Do not rewrite arbitrary shell strings or claim Cargo jobs limits all test-program threads. Language-specific environment handling stays in adapters. See [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html).

Python, JS, make/ninja, VM/container and learned estimates are later work. Limiting Docker CLI is not controlling the actual executor. Cache reclamation requires registered roots/classes and active-use exclusion. Disposable/cheap-rebuild caches may be eligible; expensive caches need retention policy; environments and persistent artifacts are protected by default. Protect Git, journals, validation evidence, installed/recovery binaries and CodeSpace target/upstream-reports/local.

Future GC: exclusive use/reclaim coordination → durable mark → same-filesystem trash rename → interruptible bounded sweep → measured free space. Validate identity/no symlink escape; trash remains occupied across restart. Initial inactive TTL seven days and cache target min(8% storage, 50 GiB) apply only to eligible classes and remain unqualified defaults. APFS du is not physical space recovery. Do not automatically change CARGO_TARGET_DIR or enable sccache.

## Execution ownership (revision 1)

[Design revision 1](design-revision-1.md) keeps the correctness goals and re-evaluates how they are implemented. The goals: one party reaps each process; a managed execution never loses its observation before reaping; permit, credential and transcript descriptors never reach unrelated executions or payloads; a lost reply, timeout, EOF or root reap alone never proves non-execution or whole-scope termination; a preparation is consumed once and uncertain work never re-executes automatically; a failed new grant never blocks status or termination; termination, output cleanup, workspace release and lease return stay distinct; implementation, functional, platform and SLO evidence are recorded separately.

CodeSpace coordinates execution identity, approval linkage, state, timeout, termination requests, output and release in one layer. It confines the differences between PTY, pipe and resource-managed execution to a narrow backend boundary: OS child creation, I/O attachment, terminal setup, exit observation and the actual reap. A backend that reaps by itself (`BackendReaped`, the initial legacy `off` paths) never advertises an unreaped-exit state. The `required` path uses `OwnerControlledReap` and observes before reaping through the one object that owns the child. A preparation is an owned, one-time object, never cloned or rebuilt from argv.

- **D1.** `required` execution defaults to a CodeSpace-owned Unix transport at the current pin. Before writing new code, record the reuse options and their contract differences: a public API, an upstream candidate with the same contract, bounded adaptation with recorded provenance (A4), then in-house code. Codex's `ProcessDriver` qualifies only under its output-loss, backpressure and Drop criteria.
- **D2.** Before final CS-RG qualification, the legacy `off` backends are either integrated and removed or kept as a limited compatibility backend on recorded grounds.
- **D3.** As stated under [responsibilities and identity](#responsibilities-and-identity).

Revision 1 keeps the Codex pin; changing it is a separate decision based on verification. [ADR-006](planning/decisions.md#adr-006--codespace-execution-ownership-and-reuse-policy) records the adaptation policy and the revisit triggers.

## Consumer integration and recovery

[CodeSpace integration](planning/codespace-integration.md) owns fixed source paths, errors and flow. Revision 1 keeps the existing Codex pin; a later change is a separate decision based on verification. Compatibility has separate source/client SHA, installed daemon/helper hashes and product wire/capability axes. Strict decoding requires real old/new fixtures; added fields are not inherently compatible. Resource settings default off; required never silently degrades.

After existing authorization/workspace FIFO, acquire a Runner slot and PrepareExec before consuming approval. Fail preparation promptly within 250 ms; no long product resource queue. The 250 ms preparation budget, the five-second Prepared lifetime and DG-1's 250 ms per-frame deadline are distinct limits. On success, same-attempt approval CAS precedes ExecPrepared. Track non-start, confirmed helper, READY, executable failure and unknown separately. Resource shortage, control-service failure and unsupported policy have separate errors. Preserve `terminate_process`, existing workspace errors and patch-operation boundaries.

Control/data processing and transport must have separate capacity and bounded aggregate queues/bytes/retention, including locks, writers and callbacks. Planned work concurrency is eight with 64 queued, while control retains independent capacity. Inflight replay shares dispatch; eviction never enables another execution. Existing handles remain queryable/terminable during authority failure.

P1 recovery is opt-in for a surviving independent Runner. Preserve existing mode termination contracts; distinguish normal shutdown, explicit stop, planned detach and unexpected disconnect. Runner retains original timeout/I/O; authenticated Gateway reconnect uses epoch fences and reconciles workspace/approval/lease without new workload budget. InProcess and I/O restoration after Runner restart are excluded. Runner/host loss remains uncertain; never replay stored argv.

## Paths, bootstrap and operations

Rust 1.95.0 is the qualification toolchain. Provide devguard, devguardd, devguard-launch and a small client. macOS operator configuration is `~/.config/devguard/host.toml`; separate state/releases/recovery/evidence under `~/Library/Application Support/DevGuard/`; short socket under `/private/tmp/devguard-<uid>/`; cache under `~/Library/Caches/DevGuard/`. Validate ownership/permissions/symlinks; persistent lock/journal never live in tmp. Project `.devguard.toml` selects project/profile/adapter and tighter limits, not authority credentials/capacity.

Planned command entrypoints include `devguard daemon serve`, `devguard doctor`, `devguard exec --wait`, `devguard test-candidate`, `devguard upgrade` and `devguard repair`. They become available only through their implementation units. Explicit CLI waiting creates a new attempt only after prior non-start is confirmed; failures never cause unmanaged fallback.

Through C08 use foreground daemons and necessary one-job/one-test-thread bootstrap builds. Preserve a functional P4 bundle outside target. C09 installs protected artifacts and a **current-user LaunchAgent**, with the same foreground daemon entrypoint; this is not a privileged system service. Its lifecycle follows the logged-in user. [Apple launchd lifecycle](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).

At C10 first test the new parent-budget mechanisms, freeze a parent containing them, then **immediately start bounded real self-use**. Earlier artifacts are not assumed to support new parent operations. Candidate capacity is bounded by its parent lease; isolate state/socket/credentials/cache; actual workloads use stable-launcher mediation. No second normal budget, normal control credentials or operational cache access. Govern subsequent applicable builds/tests, retain real receipts, and distinguish functional parent from SLO release. C11 repairs without broken candidate admission.

Upgrade verifies compatibility, closes admission, cancels Prepared leases, drains/reconciles active/suspect scopes, makes a quiescent backup, switches protected artifacts, verifies state/handshake and reopens admission. Default drain timeout 60 seconds aborts replacement without erasing charges. Before reopening, a quiescent backup can support rollback; after new admission, never restore stale state. Repair uses protected compatible artifacts/exclusive authority independently; corrupt unresolved state stays closed and incompatible downgrade is rejected.

## Verification and promotion

Record policy defaults, hypotheses and actual measurements separately. For each declared combination run **10 minutes idle and at least 30 minutes load, three times**, observing longer commands through completion. Include fixed-source Cargo, multiple consumers, bounded CPU/memory/I/O, output pressure and slow input. Separate cold/warm test directories, local control time and remote RTT.

Targets: zero pressure-induced connection loss, duplicates, protected-data GC or automatic unknown replay; local status p99 ≤500 ms; termination acknowledgement p99 ≤1 second (actual exit separately); foreground input-to-paint p99 ≤100 ms and zero over one second; zero foreground frame stalls over 500 ms. Verify visibility/focus throughout; invalid conditions or failing idle baseline are inconclusive. Preserve raw latency/pressure/jobs, peaks, throughput/refusals and lifecycle evidence with source/artifact/policy/environment hashes.

DG1-C12 qualifies standalone control/development/self-use on the actual 8-logical-CPU/16-GiB macOS target. Actual CodeSpace MCP/approval/replay qualification belongs to CS-RG. Linux fake tests do not qualify Linux controls. Promote only the measured artifact/policy/environment and verify the user service runs it.

Critical path: DG-0 → DG-1 → CS-RG → P1-RECOVERY. Linux remains required for overall completion; cache and additional adapters do not gate P1. Implementation, platform qualification and planning approval remain separate ledger states. [Verification](planning/verification.md) and [delivery](planning/pr-delivery.md) specify evidence, exact-head sequential merges and cleanup; preserve protected operational/reference/recovery materials.
