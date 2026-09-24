# Verification, acceptance and evidence

Preserve the approved [design SLOs](../design.md#verification-and-promotion). Design approval, implementation, fake contracts, native application, product integration and foreground responsiveness are distinct evidence. Documentation-head regression results do not qualify new OS functionality.

## Verification scopes

| Scope | Owner/subject | Acceptance | Baseline availability |
| --- | --- | --- | --- |
| V-DOC-DG | DevGuard docs/metadata | Original checksum/license, English/Korean hashes, IDs/DAG/links, 46 units/23 groups, required fields | Documentation checker and review |
| V-DG0 | Contract/core | Rust 1.95.0 fmt/Clippy, 44-test baseline, full dependency graph and source fingerprint | Existing validator |
| V-DG1-FUNCTION | Real auth/probe/launch/reconcile/CLI/operations | DG1-C01–C11 normal/failure/race cases and functional artifacts | C01 authority, C02 local authentication/transport, C03 native probes and C04 native scopes available; later scopes supplied with each group |
| V-DG1-SLO | Standalone daemon/CLI, development and self-use | DG1-C12 control and foreground measurements | Planned; no CS-RG prerequisite |
| V-CS-DOC | CodeSpace bilingual registry/site | Reviewed hashes, existing tests, pinned build, integrity and visual review | Existing commands |
| V-CS-UPSTREAM | Existing Codex integration | Pin/policy/format/dependencies/adapter/PTY/filesystem/platform gates | Existing; actual platforms required |
| V-CS-RG | Integrated CodeSpace | CSRG-C07/C08 parity, approvals, replay and saturation SLO | Future |
| V-P1 | Independent Runner/Gateway recovery | P1R-C06 identity, fencing, deadlines and output/unknown | Future |
| V-LINUX | Actual Linux scopes/product | DGL-C05/C06 controllers, ancestors, privileges, descendants and SLO | Future; fake cgroups do not qualify |
| V-CACHE / V-ADAPTER | Cache/tools/executors | DGC-C06; DGA-C02/C04/C06/C08 supported combinations | Future |

Keep `passed`, `failed`, `not_run`, `inconclusive` distinct. Preserve an existing tool's `incomplete` status and identify missing requirements. Zero discovered/executed cases cannot pass a suite.

## Available commands

DevGuard requires Rust **1.95.0** with rustfmt/Clippy and Python 3.11 or later:

```sh
python3 scripts/check_docs.py
python3 scripts/validate.py --offline
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/qualify.py dg1-auth --offline
python3 scripts/qualify.py dg1-probes --offline
python3 scripts/qualify.py dg1-scopes --offline
python3 scripts/qualify.py dg1-launch --offline
python3 scripts/qualify.py dg1-reconcile --offline
python3 scripts/qualify.py dg1-cli --offline
python3 scripts/qualify.py dg1-cargo --offline
git diff --check
```

Remove offline only when locked dependencies must be downloaded. Report output must be a new ignored path inside the checkout. Put the installed pinned toolchain first in PATH. Toolchain mismatch results are supplemental/incomplete, not qualification. Bootstrap uses one Cargo job and one test thread. The existing validator leaves unmeasured runtime scopes `not_run`.

CodeSpace requires Node **24.21.0**, npm **11.19.0**, Python 3.11+ (CI 3.14):

```sh
python3 -B scripts/check_docs.py
npm ci --prefix docs-site --ignore-scripts
npm test --prefix docs-site
npm run build --prefix docs-site
python3 -B docs-site/scripts/site.py check
```

Use `DOCS_PYTHON` if necessary. After actually reviewing the pair, record only the changed entry with `python3 -B scripts/check_docs.py record --id devguard-integration`. Inspect source rendering, language navigation, wide tables, desktop/narrow and light/dark presentation. Main publication is a separate post-merge workflow and is required for preparation completion.

Existing CodeSpace runtime gates remain:

```sh
python3 scripts/validate-upstream.py all
python3 scripts/validate-upstream.py macos-core dependencies
python3 scripts/validate-upstream.py linux-isolation
```

`all` does not include macos-core. Linux skips on macOS do not substitute for actual Linux evidence. Preserve `target/upstream-validation` and protected `target/upstream-reports/local` behavior. Existing CI checks still run for documentation PRs.

## Document consistency

Verify original design bytes against `docs/design-source.json`; compare LICENSE/NOTICE and preserved Codex gitlink. Check all 46 definitions (DG1 12, CSRG 8, P1R 6, DGL 6, DGC 6, DGA 8), 23 logical groups (6+4+3+3+3+4), exactly one group per task, defined prerequisites and acyclic task/milestone graphs. DG0-R records and DGP/CSP documentation units are excluded from future runtime counts.

Every task needs owner/title/group, problem→behavior, prerequisites/operating conditions, modules/deliverables, invariants, meaningful normal/failure/race tests, available/planned commands, completion evidence, rollback and handoff. Check local links, ledger references and English/Korean reviewed hashes; manually review semantic detail, since hashes do not prove translation quality. Do not invent future SHAs/PR numbers. Preserve historical status until implementation evidence warrants an update.

For the preparation PRs, preserve runtime/Cargo/journal state. Documentation checks may extend validation without removing existing gates. Preserve the original CodeSpace staged config blob and user branch; only the task-owned worktree changes. The canvas displays actual evidence and keeps future work unstarted.

## Planned suites and fault injection

`scripts/qualify.py dg1-authority --offline` is available for C01 configuration/storage, `scripts/qualify.py dg1-auth --offline` for C02 authentication/transport, `scripts/qualify.py dg1-probes --offline` for C03 native evidence, `scripts/qualify.py dg1-scopes --offline` for C04 policy and scope evidence, `scripts/qualify.py dg1-launch --offline` for the C05 launch helper, `scripts/qualify.py dg1-reconcile --offline` for C06 reconciliation, `scripts/qualify.py dg1-cli --offline` for the C07 command-line owner, and `scripts/qualify.py dg1-cargo --offline` for the C08 Cargo adapters. Other DevGuard suites and CodeSpace `scripts/qualify-devguard.py <suite>` remain planned interfaces until supplied by their work units. Each implementation PR supplies the actual interface, nonzero case inventory, timeouts, logs, isolation and cleanup, then updates its task command documentation. macOS/Ubuntu CI retains the full validator and both portable functional suites, preserving their separate reports and logs. Native suites run on macOS CI only and record `not_run` elsewhere; they never pass on a platform that cannot supply the evidence. Each native stage declares the raw receipts it must produce. A receipt that records a case as `not_run` makes the suite `incomplete`, not `passed`.

C03 evidence is recorded in these files:
- **Raw receipts** (in the report's `raw/` directory): boot ID and clock readings with units, host capacity, repeated process identities, including zombie, reaped and refused observations, native pressure readings with their derived rates, the measured time from the last sample to closed admission, and the service loop's time from an injected failure to Critical and its behavior with a stuck probe.
- **Stage logs**: the injected failures, rejected stale, future, replayed and other-boot samples, and native registration of an observed socket peer.

Tests use synthetic healthy readings wherever the actual host pressure could legitimately differ. They do not assume the host is Normal. The suite does not exercise wire registration, scope binding or applied policy.

C04 evidence covers:
- **Raw receipts**: establishment readbacks for clamped and unclamped roots (`pbi_nice`, task priority, maximum thread priority), the requested/planned/applied capability matrix, a lifecycle timeline, termination receipts, the escaped identity with its sticky Suspect result, and refusals for shared or non-empty groups, another user's process, kernel requirements and unknown scopes.
- **Real processes**: the lifecycle runs through a running root, an unreaped root, a reaped root with surviving descendants, and identity-checked termination, and releases only after every member exits.
- **Scripted-table unit tests**: races that real processes cannot trigger deterministically. These cover a member created during observation, a reused member or parent PID, a root replaced during readback, a root replaced while its group is listed, children whose parentage cannot be verified, a group ID reused after the scope ended, an unknown group that cannot be proven to be the scope's, refused or failed reads, and failed signal delivery.
- **Clamped environment**: when the environment clamps every child of the harness, as governed self-use will, the unclamped-root case cannot be produced. It is recorded as `not_run` with both readbacks, and the suite reports `incomplete` rather than `passed`. The hosted macOS 14 CI runner is such an environment: its harness reads task and thread priority 20. CI therefore runs `dg1-scopes` with `--allow-incomplete`, and its summary shows `incomplete`. The unclamped case must pass on the local qualification host.

The service establishes no scopes before the P3 launch helper, so these results establish library evidence rather than managed execution.

C05 evidence covers:
- **Raw receipts**: the launch lifecycle (phases, time to READY, exit status, the authorized attempt and its release through scope termination), the executable's descriptors, arguments, directory and environment variable names (never values), racing and late helpers against one grant, refusals before any claim, cancellation before a claim, exec failure after READY, a bare helper's replay and its refusal after a claim, a reply that missed its deadline, concurrent launches from several threads, and a helper on a pseudo-terminal. Receipts are checked not to contain credential values from the environment.
- **Real processes**: the real `devguard-launch` binary, started by the test process as the registered owner, presents grants to an isolated authority served in the same process with a synthetic healthy probe.
- **Unit tests**: core claim transitions, scripted helper-identity checks including a PID reused during the owner check, strict transcript parsing, strict launch wire fixtures, and helper sessions that can make no other request.
- **Clamped environment**: a helper refused after its claim needs a helper without the utility clamp. Where every child of the harness is clamped, that case is recorded as `not_run` and the suite reports `incomplete`, as on the hosted macOS 14 runner, where CI runs `dg1-launch` with `--allow-incomplete`.

In C05 alone the normal service kept registration and launch closed; C06 opens them together with reconciliation.

C06 evidence covers:
- **Raw receipts**: prepared cancellation and expiry, `NoHelperCreated` releases from each owner report with a refused late helper, a report that cannot release a claimed grant, observation before reap with charges held while a survivor lives, a root reaped before observation, a known escape, scope termination, cancellation after authorization, a dead owner, a retired instance, an unresponsive helper past the Prepared deadline, a daemon crash with restart and committed totals measured before and after it, and journal writes that fail for binding and for release.
- **Real processes**: the real helper and workloads under isolated authorities, including one served in a child process that the test kills and restarts on the same journal.
- **Unit tests**: previous-boot release of a bound scope after lost tracking, reconciliation that writes nothing when nothing changes, attempt and instance listings, owner-bound launcher evidence with a bounded report set, and strict decoding of the new requests.
- **Determinism**: observation tests pause the background reconciler, so an owner's observation, or its absence before a reap, decides what is tracked. Child-process authorities keep their receipts, which are checked to hold no permit or caller credential.

The suite does not exercise restart re-adoption of running scopes or Linux enforcement.

C07 evidence covers:
- **Raw receipts**: a managed command's lifecycle with its preserved arguments, directory, environment names and descriptors and its receipt; a workload's signal death mirrored by the CLI; SIGTERM and SIGHUP forwarded to every member of the workload's group; a signal the caller ignored staying ignored; a SIGSTOP not mirrored; observation before reap keeping a survivor charged until it ends; a refused budget and a wait beyond the host's work capacity; explicit waits that are admitted, that reach their deadline and that a signal cancels; an unavailable authority and one without fenced launch; doctor diagnostics; project resolution; the eight-owner instance pool and sequential owners that never exhaust it; programs that cannot start; and on a pseudo-terminal, an interrupt key reaching the workload, also with only output on the terminal, and a stop mirrored to a job-control shell. Receipts are checked not to contain the caller credential or an inherited value.
- **Real processes**: the test binary re-executed as the CLI owner against isolated authorities, starting the real `devguard-launch` and workloads, plus a pseudo-terminal session and a minimal job-control shell for the terminal cases. The shipped binary accepts no authority override, so its own entry point is tested only for usage and malformed invocations.
- **Unit tests**: strict argument parsing, program resolution, budget precedence and the meaning digest; transcripts read to their end; and, through a scripted authority, lost admission and launch-commit replies, the wait's backoff, deadline and cancellation, and the retry rule after a helper ended before READY. A committed grant is released as never received and never recreated, and one that cannot be confirmed is neither released nor retried.

C08 evidence covers:
- **Raw receipts**: a direct build within its reserved jobs; explicit jobs clamped and kept; refusals before admission; `cargo test` with its test program's thread settings; a Python pipeline's shared jobserver with its peak beside the reservation and its tokens after the run; nested Cargo in a build script; inherited FIFO and descriptor-pair jobservers with the lowest free token count seen; stale inherited descriptors; concurrent consumers with their peak charges; and a cancelled pipeline. Each records the observed peak of concurrent compilations and what each Cargo received.
- **Real processes**: real Cargo builds of small offline workspaces through the CLI owner and the real helper. A compiler wrapper times each compilation, and a `cargo` shim records the descriptors each launch inherited. Cases whose Cargo jobs the host's work capacity cannot fit are recorded as `not_run`.
- **Unit tests**: the job estimate; argument parsing around the subcommand and `--`; jobs values resolved as Cargo reads them; precedence, clamping and refusals, also under a jobserver; the fallback variable; inherited-jobserver checks for closed, close-on-exec, descriptor-pair and FIFO references; the FIFO pool, its size limit and its token count; and adapter selection.

C02 evidence covers actual OS socket UID/PID observations at both ends, distinct consumer/admin credentials, rejected helper-role authentication, strict current/future wire fixtures, 64 KiB frames, a 32-session limit, absolute 250 ms per-frame deadlines including idle waits, partial/slow/final responses and private credential-FD transport. Dedicated subprocess helpers verify FD closure before a subsequent exec and inspect argv/environment/debug/output for secret leakage. They are executed by parent tests and are not independent ignored qualification successes. Record process cleanup as well as the nonzero parent-case inventory.

The framing implementation uses `poll` with descriptor `O_NONBLOCK` and per-call nonblocking I/O, preserving buffered data after peer closure without Darwin timeout-option mutation. Test slow readers as well as slow writers. Authentication and a closed registration response do not prove boot/start identity, native registration/principals, leases, OS policy application, helper authorization or launch. Those remain unqualified until P2/P3. Likewise, `system_tasks >= 48` is a validated accounting estimate, not a kernel task cap or a measured sufficiency claim. Explicitly test rejection of the previous value 16 under unchanged schema 1; no automatic migration is implied.

| Area | Required faults/invariants | Work |
| --- | --- | --- |
| Authority/authentication | Aliases, duplicate startup, wrong UID/PID/generation, credential leakage | DG1-C01/C02, CSRG-C02 |
| Accounting/durability | Lost admission/commit replies, changed meaning, journal failure and restart | DG1-C05/C06, CSRG-C03/C04 |
| Launch | Before helper, before READY, after READY; cancellation, expiry and late helper | DG1-C05/C06, CSRG-C07 |
| Lifetime | Root exit with descendants, PID reuse, tracking loss, original boot deadline | DG1-C03/C04/C06, DGL-C04 |
| Control | Queue/byte saturation, slow stdin/readers, locks/callbacks, concurrent replay | CSRG-C05–C08 |
| Self-use/upgrade | Candidate crash/over-budget, parent loss, policy/journal failure, strict old/new decoding | DG1-C09–C12 |
| Gateway recovery | Concurrent/stale Gateway, exit/reconnect, Runner loss and output gaps | P1R-C01–C06 |
| Linux | Actual controllers/ancestors/permissions, sandbox/proxy, OOM, descendants | DGL-C01–C06 |
| Cache | Use/reclaim races, root replacement, rename/sweep crashes, trash accounting | DGC-C01–C06 |
| Tools/executors | Option conflicts, nested tokens/FDs, child budgets and actual executor lifetime | DGA-C01–C08 |

Inject faults only in bounded test scopes/roots. Abort correctness or resource-control failures, close new work, reconcile actual scopes and retain failed/uncertain evidence. R1 is not authorization for unbounded host stress.

## SLO protocol

For each declared backend/artifact/policy/environment combination, measure **10 minutes idle plus at least 30 minutes load, repeated three times**. If an actual validation command lasts longer, observe it to completion. Include fixed-source Cargo build/test, multiple consumers, bounded CPU/memory/I/O, output pressure and slow input. Separate cold/warm conditions using test directories; never erase operational caches to manufacture a baseline.

| Metric | Approved initial acceptance |
| --- | --- |
| Development/MCP connections | Zero losses caused by resource pressure |
| Local process status | p99 ≤500 ms |
| Termination acknowledgement | p99 ≤1 second; scope exit duration reported separately |
| Foreground input to next paint | p99 ≤100 ms; zero responses over 1 second |
| Foreground frame progression | Zero stalls over 500 ms |
| Duplicate execution/reservations | Zero |
| Protected-data GC | Zero |
| Automatic restart of uncertain execution | Zero |

DG-1 measures corresponding standalone status/termination and development/foreground behavior. Actual CodeSpace MCP `process_status`/`terminate_process`, approvals, replay and saturation require CSRG-C08. Mark unintegrated product measurements not applicable/not run; do not weaken numeric targets or introduce a DG1/CSRG cycle.

Use a fixed local browser fixture with scrolling, input and paint/frame measurement. Validate foreground visibility and focus **throughout** each interval; invalid observation or a failing idle baseline is `inconclusive`, never a pass. Background throttling is not foreground performance. Passing this fixture does not guarantee every website.

Record local request-to-response separately from network RTT/end-to-end latency. Fix p99 calculation, sample count, interval, clock and missing-sample treatment. Keep each repetition's raw values and verdict; pooled averages/p99 cannot conceal a failed repetition. The current local target is 8 logical CPUs and 16 GiB macOS; record actual OS/build/power conditions for every run.

## Evidence and promotion

Manifest: source heads and dirty fingerprints, actual daemon/helper hashes, client/wire/capabilities, policy revision/journal schema, host/RAM/OS/kernel/arch/power/boot/clock, relevant controllers/ancestors/privileges, fixture revision/cache state, exact commands/timestamps. Unknown values restrict supported claims.

Retain raw latency/pressure/jobs, peak memory, completion time/throughput, refusal reasons, queue/buffer peaks, attempt/slot/lease transitions, fault points and termination/readback evidence. Hash reports and raw files, redact credentials and payloads, and preserve them outside disposable worktrees before cleanup. A documentation-only commit does not remeasure an old binary.

Record CI URL/job/event/head/artifact and distinguish PR checks from merge/push-main checks. At C10 preserve a functionally tested parent and real self-use receipts. At C12 promote only the artifact/policy/environment actually measured. Implementation status and platform qualification remain separate; Linux and CodeSpace runtime stay unqualified by DG-1.
