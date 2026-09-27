# Session handoff: CS-RG integration-boundary revalidation (2026-09-27)

> **Status: submitted for the owner's decision; not a design decision.** This handoff records a review and its
> recommendation. It approves no architecture, starts no work unit and changes no ledger status. CS-RG
> implementation stays suspended: CSRG-C00, CSRG-C03 and CSRG-C09 and the CodeSpace-owned managed execution
> transport they assume are not implementation directives. Nothing below authorizes implementation.

## 1. Why this review happened
The owner's directive CS-DG-REASSESS-2026-09-27 and the owner's clarifications of the same day placed the
managed-PTY C00 path on hold and asked for a boundary review before any CS-RG implementation. The review label
"CS-RG integration-boundary revalidation" names an owner-directed review; it is not a work unit.

The owner fixed these inputs:
- CodeSpace's product semantics, responsibility boundaries and selective use of Codex upstream for execution
  mechanisms are constraints of the integration, not options to trade away. DevGuard, a later opt-in, adapts.
- CodeSpace constraints (N1) and DevGuard safety invariants (N2) are joint hard constraints with no priority
  between them. A combination that cannot meet both is unsupported; neither is weakened.
- Current code is evidence, not a constraint. The CS-RG planning documents (design revision 1, CSRG-C00/C03/C09
  and the matching CodeSpace text) are audit targets, not requirement sources.
- CS-RG alone never justifies moving a Codex-delegated mechanism (notably PTY) into CodeSpace or refactoring
  unaffected CodeSpace paths for DevGuard or for symmetry. CodeSpace changes stay a thin adapter.
- The review stops at a decision packet. A normative design PR, new units and any code come only after the
  owner's decision.

## 2. State at a glance (2026-09-27 13:05 UTC)
| Item | Value |
| --- | --- |
| DevGuard `main` | `7e3cbda91308f527d6cc34fba908375e6332bc58` |
| CodeSpace `main` | `794867ef52f530be6bc0d91aa10416d5195367b7` |
| Codex pin in CodeSpace | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` (rust-v0.154.0), unchanged |
| Hold notices | [DevGuard #12](https://github.com/novelKR/DevGuard/pull/12) (head `ae85ebb95b68821361ae59d3c60b5344a1d8ab03`) and [CodeSpace #75](https://github.com/novelKR/CodeSpace/pull/75) (head `1bee230595698b0974df43561bccfce67d7e8cb9`): open, all required checks green, waiting for exact-head merge approval |
| Independent test fix | [DevGuard #11](https://github.com/novelKR/DevGuard/pull/11) (head `2bbe7c5ed88ad3170bc76985bdecb1fd7434d501`): reviewed, green, waiting for exact-head merge approval |
| Product code, pins, services, credentials | unchanged |

Until #12 and #75 merge, the CS-RG documents on `main` still read as directives. No agent implements from them.

## 3. Findings
Evidence levels: confirmed (read at the fixed revision), inferred, requires experiment.

| ID | Finding | Source | Level |
| --- | --- | --- | --- |
| F-1 | The priority inversion is documented, not executed. Revision 1 listed CodeSpace's Codex PTY use and the Codex pin as changeable means and did not list DevGuard's helper, direct-child check, descriptor layout or observe-before-reap; the plan then fixed DG-1's consumer interface for every unit | [design-revision-1.md L96-L107](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/design-revision-1.md#L96-L107), [CS-RG.md L22](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/planning/milestones/CS-RG.md#L22) | confirmed |
| F-1a | That premise became the D1 default and the C00, C03 and C09 definitions, and entered CodeSpace's own reuse policy | [design-revision-1.md L479](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/design-revision-1.md#L479), [CS-RG.md L47](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/planning/milestones/CS-RG.md#L47), [L88](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/planning/milestones/CS-RG.md#L88), [L164](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/planning/milestones/CS-RG.md#L164), [CodeSpace codex-reuse.md L72](https://github.com/novelKR/CodeSpace/blob/794867ef52f530be6bc0d91aa10416d5195367b7/docs/codex-reuse.md#L72) | confirmed |
| F-1b | Before CS-RG, CodeSpace documented selective Codex reuse with PTY through `codex-utils-pty` | [codex-reuse.md L7 and L18 at e94d214](https://github.com/novelKR/CodeSpace/blob/e94d21475643608ad2a466256fb57266b86faa47/docs/codex-reuse.md#L7-L18) | confirmed |
| F-2 | No unauthorized execution was found. One lapse is self-reported: a queued read-only subagent delegated under the old instruction was not stopped and wrote a superseded C00 plan after the hold, without repository or product effect | session records | confirmed |
| F-3 | The core mismatch is DevGuard's launch API: `HelperCommand` makes DevGuard's client perform the spawn, so a governed PTY launch would take PTY and spawn ownership from Codex | [launch.rs L150-L154](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/crates/client/src/launch.rs#L150-L154) | confirmed |
| F-4 | Observation before reap is not a safety invariant: if the owner reaps first, unknown survivors become tracking loss and the attempt stays Suspect; it is never released | [contracts.md, Reconciliation](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/docs/contracts.md#reconciliation) | confirmed; frequency requires experiment |
| F-5 | Codex reaps PTY children internally and reports only an exit code at the pin, at rust-v0.157.1 and on main | [pty.rs L244-L253 at the pin](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/utils/pty/src/pty.rs#L244-L253) | confirmed |
| F-6 | Delivering close-on-exec descriptors only to the intended child (`ChildFds::Attached`) exists on Codex main and in prereleases, first in #47797 (2026-09-24); no stable release has it | [pty.rs L405-L408 at main 41f9084](https://github.com/openai/codex/blob/41f9084b30812db321a0b592def4f500d1e79cf4/codex-rs/utils/pty/src/pty.rs#L405-L408) | confirmed from source; behaviour requires experiment |
| F-7 | macOS cannot create pipes or socket pairs close-on-exec atomically; DevGuard's `spawn_guard` covers the window, but CodeSpace's Tokio pipe, patch-helper and probe spawns neither hold it nor close descriptors in the child | [launch.rs L175-L190](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/crates/client/src/launch.rs#L175-L190) | confirmed |
| F-7a | DevGuard's client session socket has the same window outside `spawn_guard` | [connect.rs L39-L51](https://github.com/novelKR/DevGuard/blob/7e3cbda91308f527d6cc34fba908375e6332bc58/crates/client/src/connect.rs#L39-L51) | window confirmed; exposure suspected |
| F-8 | Codex exposes no PID for PTY children at the pin or on main | `SpawnedProcess` in [process.rs L356-L361](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/utils/pty/src/process.rs#L356-L361) | confirmed |
| F-9 | DevGuard declares Rust 1.95 and CodeSpace 1.88, so a DevGuard client dependency in CodeSpace must stay optional for the `off` build | workspace `Cargo.toml` of both repositories | confirmed |
| F-10 | CodeSpace issues outside CS-RG: the tool text promises subtree termination that the code does not perform; a PTY handle dropped 15 minutes after exit signals a stored numeric pgid (Codex behaviour); worker setup failure leaves the worker and its directory | [mcp.rs L124-L125](https://github.com/novelKR/CodeSpace/blob/794867ef52f530be6bc0d91aa10416d5195367b7/crates/server/src/mcp.rs#L124-L125), [process.rs L33](https://github.com/novelKR/CodeSpace/blob/794867ef52f530be6bc0d91aa10416d5195367b7/crates/runner/src/process.rs#L33), [Codex process.rs L273-L277](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/utils/pty/src/process.rs#L273-L277), [runtime.rs L42-L50](https://github.com/novelKR/CodeSpace/blob/794867ef52f530be6bc0d91aa10416d5195367b7/crates/server/src/runtime.rs#L42-L50) | confirmed from code; not run |

## 4. Gap resolution
| Gap | Conclusion | Change locus |
| --- | --- | --- |
| Helper spawn ownership | DevGuard exposes the helper launch as data (program, argv, attachments or tickets); CodeSpace's existing backends spawn it | DevGuard client; adapter |
| Descriptor delivery, PTY | either Codex `ChildFds::Attached` after a pin update (route Y) or no descriptors at all (route X), which needs a small generic Codex PID accessor | Codex pin or upstream; DevGuard; adapter |
| Descriptor delivery, pipe | no descriptors: the owner confirms the helper's PID, which it already knows for Tokio children | DevGuard wire; adapter |
| Descriptor creation window | removed by the no-descriptor route, or by creating grant descriptors atomically close-on-exec; CodeSpace taking the guard is excluded | DevGuard |
| Observation before reap | accept Suspect for unobserved survivors after measuring it, or let the helper supervise the payload (PTY) | DevGuard helper |
| UDS worker credential | send it over the existing Runner channel, have the worker read an operator-provisioned credential, or leave UDS-mode `required` unsupported | owner decision |
| Lost replies, output contract, macOS parentage, termination | existing paths fit | adapter only |

Excluded before comparison because they break N1 or N2: CodeSpace allocating the PTY for a DevGuard spawn;
`off` on Codex with `required` on a CodeSpace-owned transport; converging the `off` backends; CodeSpace holding
DevGuard's guard; inheritable descriptors without protection; a permit through the terminal; a CodeSpace-built
`ProcessDriver`; removing the permit without an equal proof; releasing on reap; a supervising helper for pipe
while terminate stays unchanged.

Surviving candidates (all designed to meet N1 and N2; none verified by experiment):

| Candidate | What | Upstream dependency |
| --- | --- | --- |
| R | governed pipe first on macOS InProcess: launch as data, owner-confirmed helper identity, transcript through the authority, reap-first accepted with measurement | none |
| X | R's mechanism for PTY too | a generic PTY child PID accessor in Codex, then a release and a pin update |
| Y | PTY with an attached permit file through `ChildFds::Attached` | a stable Codex release containing #47797 and a pin update |

Current verdicts: every macOS `required` combination is unsupported until the DevGuard changes land; pipe needs
nothing else; PTY also needs X or Y; UDS mode also needs the credential decision; Linux waits for DG-LINUX;
`off` is unchanged.

## 5. Recommendation (not a decision)
| Topic | Recommendation | Confidence |
| --- | --- | --- |
| Boundary | DevGuard adapts to CodeSpace's existing execution; CodeSpace adds a thin adapter and generic events; Codex is consumed through normal pin updates and, where missing, a small generic upstream API | high that it meets the constraints as designed |
| First step | R | medium; requires experiment |
| PTY | X, keeping one DevGuard mechanism for both transports; Y as fallback; PTY `required` stays unsupported until one lands | medium-low |
| Observation | accept reap-first at first and measure it | medium |
| UDS mode | credential over the existing Runner channel | low-medium |

## 6. Decisions needed from the owner
1. Exact-head merge approvals for DevGuard #12, CodeSpace #75, DevGuard #11 and this handoff PR. #12, #11 and this
   PR share DevGuard's base; merging one moves the others' base, which needs a branch update and a new head
   approval unless the approval allows that move.
2. The boundary statement in section 5.
3. The route for governed PTY (R then X, R then Y, or another order).
4. Whether governed pipe terminate may also call DevGuard `Terminate` (only needed for a supervising helper on
   pipe).
5. Whether a generic "attach descriptors" input on CodeSpace's Tokio pipe spawn is acceptable (default: no).
6. The UDS worker credential path, or UDS-mode `required` left unsupported.
7. Accept reap-first with measurement, or require the supervising helper.
8. If X: approval to submit the upstream Codex PR (a local draft is allowed without it).
9. Unit identifiers: a proposal to record the hold PRs and the later normative correction as the next
   documentation units (DGP-D08, CSP-D05) and to replace the CS-RG unit sequence with new units instead of
   redefining CSRG-C00.
10. Whether to fix the client session-socket window (F-7a) as a DevGuard-only change.

## 7. How to continue
- Start by confirming state: both `main` heads, the Codex pin and the heads of the open PRs above.
- Do not implement, prototype or plan a CodeSpace-owned PTY or process backend for CS-RG. Do not start any CS-RG
  unit. Do not open a normative design PR before the owner's decisions.
- Merge only with an explicit approval tied to the exact head: recheck head and base, merge with
  `--match-head-commit`, read the post-merge `main` CI once, preserve it, then fast-forward local `main` and clean
  up without deleting remote branches.
- After the decisions: the next task is a normative design correction in DevGuard and CodeSpace that records the
  approved boundary, the thin adapter contract, the chosen route and the new unit sequence, each merged only with
  its own approval. Product implementation needs a further explicit approval.
- Public records carry no local absolute paths.

## 8. Evidence
Raw evidence and the full review records stay outside commits by repository rule; this handoff carries their
conclusions. Location and manifests (SHA-256 of `MANIFEST.json`):

| Record | Location | Manifest |
| --- | --- | --- |
| Review records (requirements, findings, ownership map, upstream matrix, DevGuard decomposition, gap resolution, decision packet, handoff, directive copy, upstream source snapshots) | `<DEVGUARD_CHECKOUT>/evidence/cs-rg/boundary-revalidation-2026-09-27/` | `MANIFEST.json` in that directory lists every file with its SHA-256; it is regenerated when records are added, so its own hash is not pinned here |
| Codex source snapshots (pin, rust-v0.157.1, main `41f9084`) | same, `stage4/` | `b58a308fbbb2243680847c2c350f8d24323513a5128206801cdeb76ad1842cc4` |
| Hold PR records | same, `stage1/` | `309b210c1cf87b795797236355dde8ad97ee776cfb2bbaa339e3d618ccd1a2aa` |
| DevGuard #11 local experiments, review and exact-head CI | `<DEVGUARD_CHECKOUT>/evidence/dg1-delivery/stuck-probe-sample-time/` | `138dc8b513096b80802b6fc54d790dac92eec9475429e2a34b2e9096b68e51a0` (experiments) |
| Superseded C00 materials (pre-investigation, late harness plan) | `<DEVGUARD_CHECKOUT>/evidence/codespace-delivery/csrg-c00/` | `c1690398c85ff02b1178c51dc6d9613a155171835055f54f49c37e546828ea2b` |

Not run: behaviour of Codex main's `Attached` path (needs a Codex workspace build the host's memory did not
allow); frequency of reap-first Suspect cases (needs real workloads); a supervising helper prototype (would
precede the decision). The DevGuard #11 reproduction kept its run helper but not each case's invocation; that
limit is stated in the PR.

## 9. Pitfalls met
- A subagent whose status reads "not found" is queued, not gone. A hold must reach it when it starts.
- Korean counterparts are maintained documents with their own structure; match sections by content, not by
  position.
- CodeSpace documentation avoids unexplained work-package numbers; explain a CS-RG item where it is mentioned.
- On macOS every per-grant descriptor created with `pipe()`, `socketpair()` or `socket()` is inheritable until
  it is duplicated or flagged; any descriptor design has to account for concurrent forks.
