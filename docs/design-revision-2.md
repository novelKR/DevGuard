# Design revision 2: external implementations through declared adapters

Design reference date: 2026-09-30. Applies to DevGuard's dependency and upstream-source policy, and to the statements in
the DevGuard and CodeSpace documents that described DevGuard as free of Codex dependencies. English is authoritative
under the repository's documentation policy; the [Korean text](ko/design-revision-2.md) is its reviewed counterpart.

**Status.**
- On 2026-09-28 the owner decided that DevGuard may use Codex and other external dependencies behind explicit adapter
  boundaries, with reviewed, immutable source identities and flexible pins where justified.
- On 2026-09-30 the owner directed that this decision be recorded as a design revision.
- The [design reference](design.md), `AGENTS.md`, the [decisions](planning/decisions.md) and the
  [planning documents](planning/README.md) apply it.
- The historical approval [design.ko.md](design.ko.md) and its [checksum](design-source.json) are unchanged, and
  [design revision 1](design-revision-1.md) keeps its dated text.
- This revision changes the policy that binds later changes. It changes no implementation, dependency, pin, gate,
  contract, milestone or qualification status.

## 1. Decision

1. **Declared boundaries only.** DevGuard consumes external implementations, Codex included, only through declared
   adapter or binding boundaries, each with a reviewed, immutable pin (section 3).
2. **No product types in the authority.** The authority core (`devguard-contract`, `devguard-core`) and the generic
   client contract that CodeSpace would link carry no CodeSpace or Codex product types, model sessions, CodeSpace
   workspace authority or PTY ownership.
3. **Justified and contained.** Each dependency is justified by what it replaces and by how its cost is contained
   (section 4).
4. **The gate changes with the first component.** The executable dependency gate in `scripts/validate.py` changes only
   in the reviewed PR that adds the first real adapter component and tests its boundary: gate change G (section 5).
   Until then the gate rejects every `codex-` and `codespace-` package.
5. **Nothing is selected.** This revision selects no dependency, pin, candidate architecture or implementation. It does
   not revive CSRG-C00, CSRG-C03 or CSRG-C09, and the CS-RG implementation hold stays in force.

This revision supersedes, prospectively:
- design revision 1's D3 choice C0, "add no Codex dependency to the current implementation"
  ([revision 1, section 10.3](design-revision-1.md#103-d3--devguard-dependency-policy)), together with its revisit
  triggers;
- every statement that DevGuard's default distribution and shared client stay free of Codex dependencies as a present
  engineering choice.

It keeps D3's boundary: the layers of point 2 stay free of product types. Revision 1's D1 and D2 and its
execution-ownership sections are not changed; they remain suspended as implementation directives.

When this revision was written, DevGuard had no Codex dependency: the `Cargo.lock` of `main` at `4898259` lists 80
packages and none is named `codex-*` or `codespace-*`. That is a fact about the current graph, not a rule.

## 2. Boundaries and placement

The names describe responsibilities, not crate names or existing APIs.

| Boundary | Owns | Must not acquire |
| --- | --- | --- |
| Authority core (`devguard-contract`, `devguard-core`, the native backend, the daemon) | admission, accounting, attempt state, pressure, evidence rules | CodeSpace or Codex product types; model, session, login or agent semantics |
| Generic client contract (`devguard-client`: transport, framing, credential handoff) | bounded authenticated messages and error translation | Codex types, because CodeSpace links this contract; an implicit unmanaged fallback; a second authority |
| Launch preparation | one-time preparation, helper invocation, attachment ownership, abort and cleanup | PTY allocation, or taking over the caller's spawn |
| Upstream-execution binding (a declared adapter; none exists yet) | the mapping from DevGuard's needs to generic upstream contracts | Codex types outside the binding; reachability from the authority core or the generic client contract |
| CodeSpace resource adapter (planned; CS-RG is suspended) | opt-in configuration, attempt binding, capability and outcome translation | PTY allocation, native spawn or a reaper added only for DevGuard; making `off` depend on DevGuard |
| CodeSpace's existing execution backends | their existing OS mechanisms, extended generically if approved | knowledge of DevGuard leases, requests or credentials |

- **Codex bindings stay out of the shared client graph.** A binding that needs Codex stays outside the generic client
  graph that CodeSpace links, unless that executable's Codex source identity is deliberately unified and jointly
  qualified (section 3.3).
- **Shared and default builds are not excluded.** This revision does not forbid a dependency in a shared or default
  build. Such a dependency must show what it replaces and how its cost is contained.
- **An optional declaration is not isolation.** Cargo feature unification can enable an optional dependency through
  another path.

## 3. Upstream sources and pins

### 3.1 Objective

DevGuard selects, qualifies, updates and rolls back an external implementation behind its adapter.
- Its domain contracts do not change merely because upstream refactored internals.
- When upstream behaviour changes materially, the adapter rejects the change, translates it explicitly, or triggers a
  reviewed contract change. It never pretends that compatibility still holds.
- Flexibility applies to which tested revision is selected, never to building from a floating reference.

### 3.2 Candidate classes

| Class | Use | Promotion requires |
| --- | --- | --- |
| Released tag resolved to a full commit | preferred where sufficient | the actual commit, artifacts, resolved dependency graph, behaviour and target matrix verified |
| Explicit commit not yet released, including prereleases | valid; not banned | the reason waiting for a release is insufficient; the snapshot qualified; rollback evidence kept |
| Small downstream patch, proposed upstream first | temporary, when necessary | the original commit, patch digest, the upstream proposal and its status, divergence and removal tests |
| Registry package | valid | registry, locked version and checksum, features, targets, provenance and update policy |
| Floating branch or mutable PR head | never promoted | resolution to an immutable commit before qualification |

"Upstream first" uses the channel the upstream accepts. Codex accepts no external code contributions or pull requests,
so a proposal to Codex is an issue that carries the analysis. Submitting one needs the owner's separate approval.

### 3.3 Executable identity

- **Same executable.** When a DevGuard component is linked into a CodeSpace executable, that executable has one reviewed
  Codex source identity: CodeSpace's gitlink.
  - A graph in which CodeSpace brings one Codex identity and a linked DevGuard crate brings another is not allowed. Two
    sources of the same crate compile as distinct crates, each with its own process-wide state.
  - The generic client contract that CodeSpace links stays free of Codex and CodeSpace product types unless a later,
    explicit design decision changes that contract.
  - Matching SHAs alone do not prove behavioural compatibility.
- **Separate executables.** DevGuard may consume Codex only inside its own executables (daemon, helper, command line),
  or inside an adapter that is not linked into a CodeSpace executable. Then the two pins need not move together.
  - What is qualified is the tuple of client, wire, helper, capabilities and artifacts.
  - No Codex Rust type or source identity crosses that product boundary.
- **No silent moves.** A DevGuard-only upstream update never moves CodeSpace's gitlink. A CodeSpace-only update never
  replaces the installed DevGuard release.
- **Not yet enforced.** CodeSpace's gates do not yet reject a second source of a `codex-` crate. The first PR that puts a
  DevGuard crate into a CodeSpace graph adds that executable single-identity check.

Resolving a version mismatch never moves PTY, spawn, reaping or lifecycle ownership from CodeSpace to DevGuard.

### 3.4 Pin record

Each consumed source has one authoritative, reviewable record. The gate validates it once the component exists.

| Field group | Content |
| --- | --- |
| Identity | repository or registry, package names, full commit or locked package identity; a tag is only an annotation |
| Selection | baseline and candidate, reason, required capabilities, availability versus contract fit |
| Consumption | adapter owner, product roots, targets, features, normal/build/dev classification |
| Reproducibility | lock digests, source-tree or patch digest, compiler and tool versions, acquisition method |
| Compatibility | adapter contract version, wire/helper/journal/config effects, known incompatible combinations |
| Provenance | licence and `NOTICE` handling, attribution, supply-chain review, advisories checked |
| Qualification | reports, executed/skipped/not-run cases, artifact hashes, baseline comparison |
| Rollback | last accepted combination, schema constraints, drain and reconciliation needs |
| Divergence | local patches, upstream tracking reference, why still needed, removal or reconvergence condition |

A value not selected is written `TBD - not selected` and never appears in a machine-validated production record. No SHA
or checksum is invented.

### 3.5 Qualification and promotion

1. Capture the baseline.
2. Select a candidate and record its class.
3. Read the relevant upstream changes and the resolved graph for each target.
4. Update the adapter on a reviewable branch.
5. Run the adapter's conformance tests, then the affected product regressions.
6. Qualify the required platforms and publish the evidence.
7. Request an exact-head merge approval, then verify the post-merge result.
8. Promote artifacts only through the deployment gate.

Rules for the evidence:
- A compile success is not behavioural compatibility, and a matching tag is not an artifact hash.
- A changed source or lock input invalidates a candidate report unless its continued applicability is shown.
- Each gate records `passed`, `failed`, `skipped-by-plan` or `not_run`, with a reason. A missing platform runner is not
  a pass, and a source review is not a runtime result. Failed attempts are kept.
- Qualifying DevGuard's own executables with a newer upstream revision does not qualify CodeSpace's pin.

### 3.6 Divergence and provenance

- A downstream patch is temporary. It records the original commit, patch digest, scope, reason, intended behavioural
  difference, tests, re-examination condition and removal or reconvergence condition, and it is proposed upstream
  through the accepted channel.
- Adapted upstream code records the same provenance.
- Hidden vendoring, copying crates to escape dependency checks, stays forbidden.
- The PR that adds a dependency adds its licence attribution and `NOTICE` handling.

### 3.7 Rollback

A rollback restores together:
- source selection, locks and patches;
- attribution;
- capabilities and behaviour documentation.

It then reruns the affected gates. A runtime that owns charged work follows the existing close-admission, drain and
reconcile process. A journal schema that an older release cannot read needs an explicit migration or drain procedure;
reverting a commit is not a runtime rollback.

## 4. Justifying a dependency

A dependency PR shows what the dependency replaces in DevGuard, for example part of `HelperCommand`, the launcher or
native observation. It also shows how its cost is contained:
- the resolved normal, build and dev graph on each target, with its features;
- the minimum and actual compiler versions;
- binary size and build time;
- the runtime tasks, threads and buffers it adds.

A package count used as evidence carries the SHA, target, features, graph split and the command run.

The PR proves these negative assertions with target-filtered metadata and separate build invocations:
- `codex-core`, `codex-exec`, `codex-app-server` and `codex-login` are unreachable from every DevGuard root;
- `codex-` packages are reachable only from the declared binding roots;
- the contract crate's dependencies are unchanged;
- CodeSpace's governance-free build and runtime work without a DevGuard service or credential, and without an unintended
  compiler-floor change.

`default-features = false` alone is not evidence.

## 5. The executable gate

Today the dependency-boundary stage of `scripts/validate.py`:
- rejects every package whose name starts with `codex-` or `codespace-` in DevGuard's graph;
- keeps the workspace roots equal to an explicit map and checks intra-workspace edges exactly;
- limits the contract crate's dependencies to `serde`, `serde_json` and `sha2`.

This revision does not change that stage.

Gate change G is made only in the reviewed PR that adds the first real adapter component, together with tests of its
boundary. It replaces the name-prefix rejection with:
1. a forbidden set for every root;
2. per-root allowed upstream crates, so that only a declared binding root reaches its approved upstream crates;
3. a check, on target-filtered, non-dev graphs, that no other root reaches any `codex-` package.

It keeps the workspace map, the edge checks and the contract rule. G never disables the stage and never narrows it
beyond what that component needs.

## 6. What this revision does not change

- No dependency, pin, candidate, library or architecture is selected, and no Codex revision is approved.
- No implementation, contract, wire, journal, gate, CI, test, service, credential or qualification status changes.
- CS-RG stays suspended, and CSRG-C00, CSRG-C03 and CSRG-C09 are not revived. Revision 1's D1, D2 and
  execution-ownership sections are untouched.
- The historical approval, revision 1's dated text and the dated handoff records are not rewritten.
- `NOTICE` stays as it is while it remains true, and `docs/contracts.md` changes only with implemented behaviour.
- CodeSpace's own pin and review process is unchanged.

## 7. Locations updated by this revision

| Location | Treatment |
| --- | --- |
| `AGENTS.md` | The revision list and the dependency bullet state this policy; the gate's current behaviour is described |
| `README.md`, planning paragraph | The Codex-free clause is replaced by a pointer to this revision. Found by the repository-wide search; it was not in the earlier inventory |
| `docs/design.md` and its Korean counterpart | Revision links, and the dependency rationale under "Responsibilities and identity"; the hold notice says so |
| `docs/milestones.md` | The Codex-free sentence is replaced by a pointer to this revision |
| `docs/planning/README.md` and its Korean counterpart | Reference date, reading order and the revision summary; the hold notice says so |
| `docs/planning/decisions.md` and its Korean counterpart | A baseline row for this revision. ADR-006's D3 row and dependency-boundary paragraph keep their 2026-09-27 wording with dated annotations; the notices say so |
| `docs/planning/codespace-integration.md` and its Korean counterpart | The D3 bullet keeps its wording with a dated annotation; the notices say so |
| CodeSpace `docs/codex-reuse.md`, `docs/upstream-update.md` and their Korean counterparts | Updated by a counterpart CodeSpace PR |

Kept deliberately:
- `docs/design.ko.md`;
- `docs/design-revision-1.md` and its Korean text;
- `docs/planning/milestones/CS-RG.md` and its Korean text, until CS-RG is re-planned;
- `NOTICE` and `docs/contracts.md`;
- the dated records in `docs/handoff/`;
- `scripts/validate.py`;
- statements that remain true, such as `README.md`'s description of `devguard-core` and the statements that a later
  Codex pin change is a separate decision based on verification.

## 8. Provenance

- **The decision.** The owner's decision of 2026-09-28 is recorded in the
  [upstream-adapter work specification](handoff/2026-09-28-cs-dg-upstream-adapter-work-spec-1.md), section 3.1, and on
  the trackers. The owner directed on 2026-09-30 that it be recorded as this revision.
- **The pin policy.** Section 3 adopts, with changes, the draft in sections 4–6 of the
  [W0–W2 packet](handoff/2026-09-28-upstream-adapter-packet.md). That packet and the
  [W3 decision packet](handoff/2026-09-28-w3-decision-packet.md) are dated, non-normative records and are cited as
  provenance only. This revision is the normative source.
- **The upstream contribution policy.** Codex's `docs/contributing.md`, policy commit `31f23b6` of 2026-08-17.
