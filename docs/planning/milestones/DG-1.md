# DG-1 — macOS development and bounded self-use

Owner: DevGuard. Baseline implementation: `not-started`; qualification: `not-run`. Entry: DG-0. Completion: DG1-C12 qualifies actual macOS launch/reconciliation, generic/Cargo consumption, parent-budget self-use, independent repair and development/foreground SLO for the measured artifact/policy/environment.

All work IDs, commit titles and logical PR labels below are **proposed values**, not future SHAs or GitHub numbers. Module paths describe planned responsibilities until implemented. Each added workspace crate updates the explicit dependency allowlist in the same PR without removing full-graph validation. The ledger owns actual status.

## PR sequence and activation

| Proposed group | Units | Predecessor |
| --- | --- | --- |
| DG1-P1 | DG1-C01, DG1-C02 | DG-0 |
| DG1-P2 | DG1-C03, DG1-C04 | DG1-P1 |
| DG1-P3 | DG1-C05, DG1-C06 | DG1-P2 |
| DG1-P4 | DG1-C07, DG1-C08 | DG1-P3 |
| DG1-P5 | DG1-C09, DG1-C10, DG1-C11 | DG1-P4 |
| DG1-P6 | DG1-C12 | DG1-P5 |

Available regression: `python3 scripts/validate.py --offline` (Rust 1.95.0 contract regression; fake backends do not prove native behavior). The task-specific qualification commands below are **planned and unavailable until implemented**. Each PR must supply real fixtures, nonzero case counts, logs and cleanup, then update command availability. See [verification](../verification.md).

DG1-P1 keeps runtime readiness closed; P2 provides actual probes; P3 ships launch with safe cleanup; P4 provides development entrypoints; P5 installation/parent-budget/repair; P6 measures and promotes. Through C08 use foreground daemons and minimum one-job/one-thread bootstrap. Preserve the P4 bundle outside disposable output. At C10, first test and freeze a parent containing parent-budget support, then immediately begin bounded real self-use; C12 alone establishes SLO qualification.

### DG1-C01 — define canonical authority and bounded test configuration

- Owner / proposed PR: DevGuard / DG1-P1. Proposed commit: `feat(daemon): define canonical authority and bounded test configuration`.
- Problem → behavior: Wrap per-directory library locks in one canonical service boundary so alternate paths cannot duplicate host capacity.
- Prerequisites: DG-0. Actual executor host, configured service UID and protected state/socket parents; a test authority cannot start without parent proof.
- Modules / deliverables: Daemon configuration loader, canonical path resolver, diagnostics and examples around Authority::open; ownership and symlink checks.
- Invariants: Preserve core arithmetic, separate journal initialize/open, static reservations and closed execution capabilities; settings cannot turn a live consumer into another slot.
- Tests (normal / failure / race): Normal single startup; bad permissions, aliases and missing journals rejected; two processes and alternate socket names compete for one authority.
- Completion evidence: Observed paths/UID/lock owner, rejected duplicate-start logs and configuration fingerprint; redact credentials and private path detail.
- Rollback: Stop service startup while preserving the journal; do not shrink live-instance policy arbitrarily.
- Handoff: DG1-C02 receives the canonical endpoint and administrative/test-mode boundary; deliver both in DG1-P1.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-authority`.

### DG1-C02 — authenticate local peers and transfer scoped credentials

- Owner / proposed PR: DevGuard / DG1-P1. Proposed commit: `feat(client): authenticate local peers and transfer scoped credentials`.
- Problem → behavior: Authenticate OS-observed peers rather than caller-declared UID/PID, with bounded versioned client communication.
- Prerequisites: DG1-C01. Real UDS peer inspection and installed generation/registration secrets; user execution is not yet enabled.
- Modules / deliverables: Daemon/client framing, private credential FD API, handshake/strict-decoding fixtures; construct TrustedPeer only at the trusted daemon boundary.
- Invariants: Caller payload cannot declare peer identity/admin Principal; UID alone grants no role; keep secrets out of argv/env/journal/debug and bound frames, sessions and deadlines.
- Tests (normal / failure / race): Normal registration/reconnect; wrong UID/PID, secret, generation, wire or capability rejected; concurrent registration/retransmission and FD/log leakage tests.
- Completion evidence: OS peer-to-instance correspondence, authorization error matrix and old/new decoding results, including added-field incompatibility.
- Rollback: Close new connections/admission, preserve existing leases and rotate credentials only after live-generation reconciliation.
- Handoff: DG1-C03–C06 consume authenticated principals/private FD APIs; never activate auth separately from canonical ownership.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-auth`.

### DG1-C03 — observe boot identity and host pressure

- Owner / proposed PR: DevGuard / DG1-P2. Proposed commit: `feat(macos): observe boot identity and host pressure`.
- Problem → behavior: Replace fake clock, process and pressure inputs with actual boot-relative time, PID/start identity and host observations.
- Prerequisites: DG1-C02. Explicit supported macOS/privilege conditions and reportable observation failures.
- Modules / deliverables: Native platform Backend, pressure sampler/pressure.rs integration and host receipts; separate unsupported capability from failed observation.
- Invariants: Closed before the first valid sample; reject stale/future evidence; retain two-second sampling, six-second freshness and 30-second stepwise recovery.
- Tests (normal / failure / race): Normal load/recovery; failed/delayed probes and boot changes; PID reuse/sample replay races and admission closing during sampler delay.
- Completion evidence: Raw samples, clock units, repeatable process identity, injected failures and time to closed admission.
- Rollback: Block new work on probe failure without shrinking live leases or substituting fabricated values.
- Handoff: DG1-C04 receives fresh identity/pressure inputs; review native scope evidence in the same P2 group.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-probes`.

### DG1-C04 — verify resource policy and scope termination

- Owner / proposed PR: DevGuard / DG1-P2. Proposed commit: `feat(macos): verify resource policy and scope termination`.
- Problem → behavior: Report actual per-resource policy application and scope termination rather than treating plans as applied facts.
- Prerequisites: DG1-C03. Supported QoS/nice and observation rights; cooperative workloads remain in their observed process group.
- Modules / deliverables: Scope tracker and native apply/readback adapters connected to binding/reconciliation; supported/failure capability matrix.
- Invariants: No applied success before real readback; root reap alone cannot reclaim; tracking loss is sticky and identity checked. Do not use unsupported macOS NOTE_TRACK or claim kernel memory/task containment.
- Tests (normal / failure / race): Normal policy/descendant exit; denied/unsupported/missing observations; root exits while descendants survive, member escape and tracking-loss/reclaim races.
- Completion evidence: Requested/supported/applied results by resource, exact scope evidence and rejected kernel-level requirements.
- Rollback: Clean failed application before authorization; preserve Suspect and charges whenever scope tracking is uncertain.
- Handoff: DG1-C05 receives actual binding and termination APIs for the pre-exec boundary.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-scopes`.

### DG1-C05 — fence helper preparation and executable start

- Owner / proposed PR: DevGuard / DG1-P3. Proposed commit: `feat(launcher): fence helper preparation and executable start`.
- Problem → behavior: Create at most one helper from a durable launch grant, then bind/apply/authorize/READY before attempting user exec.
- Prerequisites: DG1-C04. Authenticated client, actual scope/application and private credential FD; execution stays disabled until DG1-C06 cleanup also ships.
- Modules / deliverables: Launch helper, private pipe/PTY integration API, launch transcript and separate exec-failure channel.
- Invariants: begin_launch replay yields no fresh permit; lost may_exec reply stays uncertain; RunAuthorized, READY and executable success differ; close credentials before payload.
- Tests (normal / failure / race): Normal argv/exit; failures before helper, before READY and after READY; lost permits, duplicate/delayed helper and cancellation races.
- Completion evidence: Zero/one helper per attempt, phase observations, payload FD inventory and no secret leakage.
- Rollback: Fence new launches and reconcile surviving helpers/scopes through DG1-C06; never release unconditionally.
- Handoff: DG1-C06 receives committed/unknown branches and cancellation hooks; review together in P3.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-launch`.

### DG1-C06 — reconcile cancellation expiry and uncertain execution

- Owner / proposed PR: DevGuard / DG1-P3. Proposed commit: `feat(launcher): reconcile cancellation expiry and uncertain execution`.
- Problem → behavior: Distinguish Prepared cleanup from committed execution so cancellation or TTL cannot prematurely reallocate capacity.
- Prerequisites: DG1-C05. Actual helper/scope observations, durable journal and restartable isolated fixtures.
- Modules / deliverables: Daemon reconciler, helper cancel/stop, known_not_started/release_reason mapping and incident procedures.
- Invariants: Original five-second Prepared deadline; no TTL release after commitment; missing root/owner is not termination; preserve tombstones.
- Tests (normal / failure / race): Prepared cancel/expiry; journal failure, daemon crash and unresponsive helper; commit/cancel, late bind and restart/reclaim races.
- Completion evidence: Zero duplicate execution/early release; distinct termination, NoHelperCreated and previous-boot evidence; invariant restart accounting totals.
- Rollback: Close admission and drain with a compatible reconciler; reject unsupported in-place schema downgrade.
- Handoff: DG1-C07 receives explicit refusal/non-start/unknown receipts and safe termination APIs.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-reconcile`.

### DG1-C07 — govern commands with doctor receipts and explicit waits

- Owner / proposed PR: DevGuard / DG1-P4. Proposed commit: `feat(cli): govern commands with doctor receipts and explicit waits`.
- Problem → behavior: Provide an actual central-admission command entrypoint, diagnostics and explicitly bounded waiting.
- Prerequisites: DG1-C06. Real daemon/helper, registered consumer/policy and sufficient minimum budget; no implicit indefinite wait.
- Modules / deliverables: devguard exec, doctor, receipts, explicit --wait, signal/exit forwarding and generic argv API; no implicit shell.
- Invariants: Preserve command meaning, cwd, environment and exit status; existing query/termination are independent of new admission; refuse inadequate budget.
- Tests (normal / failure / race): Normal argv/exit/signals; unavailable daemon, capability mismatch and refused budget; wait cancellation and lost-grant races.
- Completion evidence: Command/attempt/lease/scope receipts, wait deadline/cancellation results and diagnosis distinguishing unmanaged execution.
- Rollback: Stop new CLI launches and drain managed work; never fall back to unmanaged execution automatically.
- Handoff: DG1-C08 receives an adapter transformation interface separate from authority admission.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-cli`.

### DG1-C08 — preserve pipeline semantics and shared jobserver budgets

- Owner / proposed PR: DevGuard / DG1-P4. Proposed commit: `feat(cargo): preserve pipeline semantics and shared jobserver budgets`.
- Problem → behavior: Run direct Cargo and explicit Cargo pipelines within one lease without creating independent nested parallelism pools.
- Prerequisites: DG1-C07. Explicit adapter choice, supported Cargo version and valid inherited jobserver ownership/FDs.
- Modules / deliverables: Cargo adapter, pipeline environment policy and wrapping examples that leave CodeSpace validation entrypoints untouched.
- Invariants: Preserve CARGO_TARGET_DIR/report paths and command selection; explain/clamp supported explicit jobs or reject conflicts; estimate 512 MiB overhead plus 1.5 GiB/compiler job, never claim measured memory enforcement or force a non-fitting job.
- Tests (normal / failure / race): Normal build/test/pipeline; unsupported subcommands, conflicting jobs and closed FDs; nested Cargo, concurrent consumers and cancellation/token return.
- Completion evidence: Original/transformed argv/env, equal results/paths, observed parallelism within parent budget and no credential FD leakage; Cargo jobs do not cap arbitrary test threads.
- Rollback: Disable the adapter and explicitly select qualified generic consumption; preserve existing target/cache.
- Handoff: Freeze a functionally tested C01–C08 bundle outside target before P4 cleanup for DG1-C09 bootstrap/recovery; it is not SLO-qualified.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-cargo`.

### DG1-C09 — identify artifacts and establish a bootstrap reference

- Owner / proposed PR: DevGuard / DG1-P5. Proposed commit: `feat(install): identify artifacts and establish a bootstrap reference`.
- Problem → behavior: Identify and protect the installed artifact rather than equating a successful source build with an operational release.
- Prerequisites: DG1-C08. Functional/failure suites passed, protected install paths and control reservation; full product SLO is not yet required.
- Modules / deliverables: Artifact manifests/hashes, daemon/helper compatibility, installer/status and current-user LaunchAgent; immutable recovery copy outside target.
- Invariants: Candidate cannot overwrite reference/recovery; one normal authority; restart opens/reconciles existing journal and fails closed on missing/corrupt state; no privileged daemon.
- Tests (normal / failure / race): Initial install/restart; hash mismatch, partial install and unsupported host; concurrent start/install races.
- Completion evidence: Running binary hashes match manifest, service PID/endpoint and explicit functional-only artifact scope.
- Rollback: Select a preserved compatible copy on pre-start failure; never replace an active journal with stale snapshots.
- Handoff: DG1-C10 receives protected installation and independent repair. Do not assume the C09 artifact already supports C10 parent operations.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-bootstrap`.

### DG1-C10 — constrain candidate authorities within parent leases

- Owner / proposed PR: DevGuard / DG1-P5. Proposed commit: `feat(self-use): constrain candidate authorities within parent leases`.
- Problem → behavior: Constrain candidate development under one parent lease instead of issuing the host budget again.
- Prerequisites: DG1-C09. First implement and pass focused parent-budget tests, then freeze a C10-capable parent; isolated candidate state/socket/credentials/cache and stable-launcher mediation.
- Modules / deliverables: Bounded candidate mode, parent capability validation, aggregate child accounting and actual self-use fixture runner.
- Invariants: Candidate/child CPU, memory and tasks stay within parent capacity; no normal control credentials/journal; parent loss/expiry fences new grants; earlier C08/C09 binaries are not presumed capable.
- Tests (normal / failure / race): Immediately after the functional checkpoint run real candidate build/test under the frozen parent; over-budget/fake parent/policy failure; candidate crash, parent cancellation and delayed child-start races.
- Completion evidence: Frozen parent hashes/capabilities, real admission/launch/reconciliation receipts, parent-child sums and scopes, second full-host authority rejection and protected recovery artifacts.
- Rollback: Parent independently cleans/reconciles candidate scopes; repair does not depend on candidate admission or normal-journal reinitialization.
- Handoff: Govern subsequent applicable builds/tests from this checkpoint. DG1-C11 receives parent-mediated drain/repair and preserved failure evidence; C12 later determines SLO promotion.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-self-use`.

### DG1-C11 — recover upgrades without candidate admission

- Owner / proposed PR: DevGuard / DG1-P5. Proposed commit: `feat(operations): recover upgrades without candidate admission`.
- Problem → behavior: Recover failed candidates/upgrades without candidate admission or stale-journal restoration that could duplicate grants.
- Prerequisites: DG1-C10. Protected reference artifact, independent operator/control capacity, actual old/new fixtures and explicit schema policy.
- Modules / deliverables: Drain/upgrade/repair tooling and runbook, client/artifact/schema compatibility matrix and rejected-downgrade rules.
- Invariants: Close new admission while retaining query/stop/reconciliation; default 60-second drain timeout aborts replacement; preserve tombstones/current accounting; strict additive fields require tests.
- Tests (normal / failure / race): Normal N to N+1; install/policy/journal failure, candidate crash and drain timeout; mixed clients, lost upgrade responses and late helpers.
- Completion evidence: Recovery while candidate is unavailable, unchanged accounting/tombstones, rejected incompatible downgrade and actual runtime identity.
- Rollback: Return only to compatible artifacts; incompatible journals require a supported reader/forward repair; no old snapshot after resumed admission and no work replay.
- Handoff: DG1-C12 receives the functionally qualified installation/self-use/recovery combination.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-upgrade`.

### DG1-C12 — qualify macOS development and bounded self-use

- Owner / proposed PR: DevGuard / DG1-P6. Proposed commit: `test(qualification): qualify macOS development and bounded self-use`.
- Problem → behavior: Identify a measured artifact/policy/environment rather than claiming everyday responsiveness from functional success.
- Prerequisites: DG1-C11. Governing reference, valid foreground fixture, fixed workloads, sufficient host budget and protected raw-evidence storage; local target 8 logical CPUs/16 GiB macOS.
- Modules / deliverables: Native qualification harness, three repetitions, supported-environment/release manifests and operator/consumer handoff.
- Invariants: Each combination: 10-minute idle and at least 30-minute load, three repetitions; cold/warm separate; foreground/focus valid throughout; invalid means inconclusive. DG-1 does not depend on CS-RG.
- Tests (normal / failure / race): Concurrent generic/Cargo consumers; daemon/probe/candidate failures; cancellation/restart/late helper under bounded CPU/memory/I/O, output pressure and slow input; standalone control and foreground SLO.
- Completion evidence: Exact source/artifact/policy/host, valid baselines, raw samples/p99/refusals/peaks/throughput/lifecycle and actual self-use receipts; only measured combination promoted and user service verified.
- Rollback: Return new development to the preserved reference and revoke failed promotion; retain evidence and resolve correctness/resource failures before remeasurement.
- Handoff: CSRG-C01 may select a pin from this combination; DG-CACHE/DG-ADAPTERS follow separately. Record implementation and each platform qualification independently.
- Verification command: available regression above plus **planned, not yet provided** `python3 scripts/qualify.py dg1-macos`.
