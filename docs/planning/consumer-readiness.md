# Minimum consumer readiness

These gates are language/product independent. Only CodeSpace's actual code paths have been inspected. The [ledger](../../milestones.json) owns implementation status; [decisions](decisions.md) own rationale. DG-1 now permits R2 and RS on the measured host with release `0.1.0-5daee5d-b3fa569e`; R3 still requires CS-RG.

## Adoption levels

| Level | Minimum conditions | Permitted scope | Failure or next gate |
| --- | --- | --- | --- |
| R0 contract review | DG-0 source, contracts and 44-test baseline | Design types, errors and state transitions | No daemon, OS control or SLO claim |
| R1 bounded functional testing | DG1-C01–C08 real authentication, launch and reclamation; explicit test environment/budget | Candidate functionality and fault tests | Not everyday development qualification |
| R2 macOS development | DG1-C12 qualification, actual probes, sufficient budget and explicit entrypoint | Qualified generic/Cargo commands on the measured host combination | Reject or qualify other tools/hosts |
| R3 CodeSpace macOS runtime | R2 plus CSRG-C08 on the head left by the CSRG-C09 backend decision: the common execution lifecycle with one reaper, mixed legacy/managed execution in one process verified or that combination unsupported, and supported client/artifact/wire | Required consumption and control protection in qualified modes | No automatic switch from off |
| R4 Linux enforced protection | DGL-C06 in the actual Linux environment | Verified resource-specific kernel controls and scope | Reject missing controller/ancestor/privilege requirements |
| RS bounded self-use | C09 protected artifacts; C10 functionally tested and frozen parent with parent-budget capability, isolated candidate and independent repair | Candidate development/testing under the existing budget, beginning at C10 | Functional parent and C12 SLO release remain distinct |

RS is an independent axis. C08/C09 artifacts are not assumed to support C10 parent operations. At the C10 functional checkpoint, start real bounded self-use immediately; C12 later qualifies everyday use. Standalone DG-1 does not require CodeSpace integration, avoiding a dependency cycle.

## Common required gates

| Gate | Evidence | Owner/work | Failure handling |
| --- | --- | --- | --- |
| G01 execution host/authority | Executor identity, canonical socket/state, UID/lock, one normal authority | Operator; DG1-C01/C02; DGA-C07 for VMs | Reject another full-host budget via alternate paths |
| G02 actual consumption | argv → adapter → attempt → lease → scope receipt | Consumer; DG1-C07/C08, CSRG-C03/C04 | A config file alone leaves execution unmanaged |
| G03 sufficient capacity | Effective capacity minus host headroom and static control reservations fits the minimum job | Authority; DG1-C03, DGL-C01 | Refuse rather than force one worker |
| G04 identity/credentials | OS peer matches the owner/generation; no secret in payload FDs/logs; one spawn protection covers every child-creation path | DG1-C02/C05, CSRG-C00/C02 | Unauthorized; never trust caller-declared peer identity |
| G05 per-resource capability | Requested, supported and applied method/level with fresh evidence | DG1-C04, DGL-C02 | Block execution on unsupported requirement or partial apply failure |
| G06 execution lifecycle | Expiry, lost replies, cancel, restart and tracking-loss fixtures; one reaper per child and observation before reap | DG1-C05/C06, CSRG-C00/C03/C04 | Preserve uncertainty; never auto-replay |
| G07 independent control | Existing handle query/termination while new admission fails | Consumer; CSRG-C05–C08 | Refuse new work without requiring grants to control old work |
| G08 compatibility | Full client SHA, actual daemon/helper hashes, wire/capability fixtures | DG1-C11, CSRG-C01/C08 | Reject unsupported combinations; version labels are insufficient |
| G09 protected state | Journal, Git, evidence and reference/recovery artifacts excluded from cache reclaim | Operator; DGC-C01/C02 | No automatic reclaim of unclassified roots |
| G10 recoverability | Repair with a broken candidate; drain/rollback preserves current ledger | DG1-C09–C11, P1R-C01–C06 | Never overwrite with stale state or replay argv |

All participating consumers on the executor host share the normal budget. A remote workload cannot consume fictitious capacity from a Gateway or development Mac. Nonparticipating processes remain outside managed scopes and contribute to observed pressure and headroom needs.

## Platform claims

| Platform | Intended support | Qualification | Excluded claims |
| --- | --- | --- | --- |
| DG-0 on any OS | Types, accounting, fake backends | R0 | Runtime authentication, OS caps or responsiveness |
| Native macOS | Central admission/accounting, supported QoS/nice, observed groups and launch fencing | R2/R3 | Tree-wide kernel memory/task caps or exclusive physical cores |
| Linux cgroup v2 | Delegated resource controls within effective ancestor capacity | R4 on actual host | Controller presence alone proving application or absolute latency |
| VM/container/remote executor | Actual host/guest binding and parent budget | DGA-C07/C08 plus R4 where required | Docker CLI exit proving container exit; double-counting host and guest |
| Other tools/OS | Additional adapter and qualification | Unsupported until recorded | Generic wrappers providing identical enforcement |

Use [design](../design.md) defaults: integer millicpu and bytes; CPU headroom max(ceil(25% logical CPUs), 2 CPUs), RAM max(25%, 4 GiB); CodeSpace control 1 CPU/512 MiB per configured instance; daemon 0.25 CPU/128 MiB; CLI pool total 0.25 CPU/128 MiB across at most eight CLIs. These are initial policy values, not measured sufficiency. The CodeSpace reservation covers both Gateway and Runner.

Lowering the pressure target never shrinks outstanding leases. Probe failure does not authorize disabling protection. Report requested/supported/applied levels separately. Known escape, identity mismatch or tracking failure stays Suspect under macOS's cooperative-workload assumption.

## Consumer and handoff contract

Consumers own handles, I/O, approvals and workspaces; authority receives a versioned execution-meaning digest and intent. Attempt identity is independent of transport requests. Changed meaning conflicts, terminal attempts cannot become new executions, and lost replies do not authorize a new process.

Reservation, plan, application evidence, helper READY and executable success are distinct. Prepared cancellation differs from committed cleanup. Missing root PID, timeout, disconnect or absent scope cannot by itself prove termination. Existing control uses owner handles and reserved capacity; reconnect restores observation/control of the same execution without another workload grant.

| Compatibility axis | Record | Required tests |
| --- | --- | --- |
| Source/client | Full SHA, dependency graph/lock and adapter revision | Types/errors, strict decoding and capability negotiation |
| Daemon/helper | Binary hashes, host/arch, policy and journal schema | Launch/FD lifecycle, old/new readers/writers, upgrade and repair |
| Product wire | Version and execution mode | Prepare/execute/control/replay/recovery across supported combinations |

Strict serde decoding makes added fields a compatibility event. Schema changes need migration and rejected-downgrade rules. Preserve an adoption report and enable only its qualified scope. Rollback closes new admission, observes/drains live work, reconciles the ledger and selects compatible artifacts/settings. Repair cannot require candidate admission. Link [CodeSpace integration](codespace-integration.md), [verification](verification.md) and [delivery](pr-delivery.md) in the product handoff.
