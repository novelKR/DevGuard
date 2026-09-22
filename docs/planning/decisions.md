# Decisions and source baselines

Reference date: 2026-09-22. English is authoritative; see the [reviewed Korean translation](../ko/planning/decisions.md). Approved decisions and completed implementation/qualification are separate facts.

## Baselines and document authority

| Subject | Fixed reference | Meaning |
| --- | --- | --- |
| DevGuard implementation | [`d59cbd43d206a9a9281328a946eddf1dc199f710`](https://github.com/novelKR/DevGuard/tree/d59cbd43d206a9a9281328a946eddf1dc199f710) | DG-0 contract/core and fake-backend tests |
| CodeSpace runtime | [`e94d21475643608ad2a466256fb57266b86faa47`](https://github.com/novelKR/CodeSpace/tree/e94d21475643608ad2a466256fb57266b86faa47) | Source inspection baseline; no DevGuard integration |
| Original CodeSpace roadmap | `fb822fc24c98f6628dce62d33a5cc67275f8ca34` | Existing local documentation commit included in PR #65 |
| Codex pin | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` | Preserved; independent of this documentation revision |
| Historical approval | [design.ko.md](../design.ko.md), [source metadata](../design-source.json) | Immutable bytes and checksum |
| Approval SHA-256 | `97b67a1f9518c1781156a4b3b26829b285f84f5c9a44da60f3c5dcf1bc768df8` | Clarifications do not modify the approved artifact |
| License | [Apache-2.0](../../LICENSE) | Same license as CodeSpace; preserve LICENSE and NOTICE |

The [English design reference](../design.md) is the editorial source for subsequent work; [its Korean counterpart](../ko/design.md) is maintained separately. It does not replace the historical approval bytes. `docs/contracts.md` owns implemented contracts; `milestones.json` owns IDs/dependencies/status; milestone documents own commit/test/PR boundaries. A documentation revision is not a runtime dependency pin or artifact promotion.

## ADR-001 — One registration by the execution owner

**Accepted; Required; high confidence.** This specializes approved design §§2.2 and 4.3 for CodeSpace. DG-0 `crates/core/src/authority.rs` validates the OS peer PID against registration identity. CodeSpace `crates/runner/src/process.rs` owns child/PTY handles. A Gateway declaring a worker PID cannot satisfy that contract.

| Alternative | Benefit | Cost or limitation | Decision |
| --- | --- | --- | --- |
| Single Runner registration | Matches execution ownership and PID validation; one lease owner | Gateway costs must fit the shared static reservation; private credential handoff | Adopt initially |
| Gateway registers and worker shares its Principal | Superficially simple startup | Different peer PID and execution owner; ambiguous authority/lifecycle | Reject |
| Service registration plus subordinate workers | Supports multiple Runners and shared reservations | New parent/child registration, budget transfer, generations and partial-failure contracts | Revisit on demonstrated deployment need |

InProcess registers inside the Runner using the Gateway PID. UDS registers in the worker. One static reservation includes Gateway and Runner control costs. Configured instance limits remain binding; no worker or Gateway receives another full host budget.

For an SDK consumer, `service-exec` prepares credentials and startup; the selected Runner completes registration. This refines the original generic service-exec wording and prohibits double registration by launcher and Runner. Non-SDK service support needs its own contract rather than weakening PID validation.

The smallest change is a private Runner client/startup adapter and credential FD. Core does not acquire CodeSpace process IDs, PTY or workspace policy. CodeSpace's PTY wrapper currently supplies an empty inherited-FD list; the pinned Codex PTY already accepts selected FDs. Extend the wrapper, close credentials before payload exec and test both pipe/PTY. Do not expose credentials/FDs in public MCP input or change the Codex pin for this feature.

Compatibility affects operator settings, capabilities and private startup. Cost is moderate; authentication, FD leakage and under-reservation are the main risks. Verify UID/PID/start identity, PID reuse, both modes, concurrent slots and payload FD absence. Roll back by closing new admission, reconciling live leases and returning to a compatible combination; never erase credentials or journals to reset accounting. Revisit service/subworker registration only when multiple independent Runners or shared control reservations are actually needed.

## ADR-002 — Gateway recovery while an independent Runner survives

**Accepted; Required; high confidence.** This specializes approved design §§2.1, 3.4 and 5.2. `crates/server/src/runtime.rs` owns the managed worker; Runner handles and I/O are in memory. Recording a PID cannot reconstruct that ownership.

| Alternative | Recoverable scope | Cost or risk | Decision |
| --- | --- | --- | --- |
| Live independent Runner; Gateway reconnects | Same processes, PTYs and I/O owner | Operator mode, authentication, epoch/fence and reconciliation | Initial scope |
| Restart Runner and restore I/O | Loss of the I/O owner too | Separate long-lived I/O owner, spool, handle transfer and retention | Separate future design |
| Replay stored argv | Creates a new process | Duplicate side effects; not the same execution | Prohibited recovery mechanism |

Introduce an operator-selected mode with an explicit capability. Preserve termination contracts of existing InProcess, managed UDS and connection modes. InProcess shares the Gateway PID and cannot provide live recovery. Changing the default rollout is a separate decision.

Distinguish normal shutdown, explicit service stop, restart detach and unexpected connection loss. Explicit stop drains/terminates and reconciles evidence. In the new independent mode, detach or unexpected Gateway loss leaves the Runner managing original deadlines, output limits and processes. Do not infer intent from a signal alone.

Authenticated reconnect issues an epoch/control fence and rejects stale Gateway mutations. Reconnection to the same execution needs no new workload budget. Reconcile workspace occupancy, approvals, attempts and leases without extending timeout. Runner/host loss stays uncertain until actual evidence; PID/DB records alone do not restore pipes/PTYs.

The smallest change comprises independent lifecycle, a process-specific durable store separate from patch operations, authentication and reconciliation. Cost is high; split-brain, stale approval and output gaps are risks. Test repeated restart, simultaneous reconnect, stale epochs and Runner loss. Rollback closes new-mode starts and drains surviving Runners before reverting modes; it never replays argv.

## ADR-003 — One normal authority and bounded candidate testing

**Accepted; Required; high confidence.** Approved design §§2.3, 4.4 and 4.5 apply. DG-0 locks each journal parent; this library guard does not authorize allocating full host capacity in arbitrary directories.

DG1-C01 fixes canonical normal paths, ownership, permissions and exclusive authority. Alternate test paths become available only through DG1-C10 parent leases and scoped credentials. Remote execution consumes the actual executor host authority. A governor per checkout duplicates capacity; fault injection in the normal journal undermines recovery. The chosen design is one normal authority plus isolated, bounded candidates.

Cost is moderate. Parent loss/expiry fencing is the principal risk. Validate aliases, concurrent startup and parent-loss races. Rollback reconciles candidate scopes under the parent while preserving the normal journal. Recovery cannot depend on the broken candidate's admission.

Necessary bootstrap builds use one Cargo job and one test thread. Preserve a tested C08 bundle outside build output. C09 establishes installation and recovery artifacts. C10 first implements and functionally validates parent-budget mechanisms, then freezes a **C10-capable parent artifact**. Immediately run a separate candidate under that parent and govern subsequent applicable builds/tests. Do not assume the earlier bundle supports newly added operations. Record functional qualification separately from C12's SLO-qualified release; no circular requirement for an already SLO-qualified first version.

## ADR-004 — Compatibility and the complete control path

**Required; high confidence:** strict decoding (`deny_unknown_fields`) requires actual old/new reader/writer fixtures for contract, wire and journal changes. Additive fields are not automatically compatible. Validate source/client, daemon/helper and CodeSpace wire independently. Include migration and downgrade rules in the changing PR; never restore an old DB snapshot after new admission.

**Required; high confidence:** CodeSpace's post-spawn slot check, serial dispatch, shared writer and completed-response replay need changes before integration. Acquire slots before spawn; bound concurrent work, queues, total bytes and retention across control/data paths. Two sockets cannot solve shared mutex/callback starvation. CSRG-C03–C08 group preparation with cleanup and saturation/lost-reply testing. Main risks are duplicate execution/approval and leaked lifecycle state.

**Strongly Recommended; high confidence:** `crates/runner/src/files.rs` reads/hashes whole files before applying a response window. Response size is not an allocation bound. An explicit file-size rejection is the smallest remedy; streaming read/hash preserves wider functionality. API errors/hash consistency need review; cost is moderate. Test large/concurrent/changing files and peak memory. This remains a separate follow-up, outside this sequence; until resolved, bound qualification file size/concurrency and do not advertise arbitrary-file protection.

## ADR-005 — Sequential delivery, language and operational ownership

**Accepted; Required for this execution.** Append to DevGuard PR #1, then update CodeSpace PR #65 to the resulting immutable English revision. Validate and merge in that order and verify separate main workflows, including CodeSpace publication, before DG1-P1.

English PR bodies and authoritative documents have reviewed Korean counterparts with source/translation hashes. Preserve all existing commits/links and the approved design checksum. P1–P4 use foreground daemons. P5 introduces a current-user LaunchAgent, never a privileged daemon. Service restart reopens/reconciles existing state and fails closed for missing/corrupt journals; explicit bootstrap/repair exceptions must be recorded.

Use `codex/dg1-p1` through `codex/dg1-p6`, each from freshly verified main. Complete review, current-head CI, normal exact-head merge, main CI and evidence-preserving cleanup before the next group. Keep remote branches, original CodeSpace checkout/index and protected toolchains/journals/artifacts. [Delivery](pr-delivery.md) specifies commands and cleanup checks. Request a further decision only for a material change in scope, privilege, ownership, compatibility, workload assumptions or acceptance criteria.

The critical path remains DG-0 → DG-1 → CS-RG → P1-RECOVERY. Linux remains required for overall completion; cache/additional adapters do not become P1 prerequisites. Record later decisions and affected IDs without rewriting historical approval or measurement results.
