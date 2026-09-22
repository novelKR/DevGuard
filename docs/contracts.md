# Implemented authority contract

This document describes the implemented DG-0 authority and C01/C02 service, storage and transport behavior. PR and post-merge main delivery evidence is tracked separately from implementation. [Operations](operations.md) lists actual command availability; native registration/launch, OS policies and CodeSpace integration remain unimplemented. [Korean translation](ko/contracts.md).

## Authority and transport boundaries

The core is a Rust library. `Authority::register` accepts a `TrustedPeer` and verifies the configured UID, consumer credential, generation and exact process identity through `Backend`. Registration returns an opaque `Principal`; a workload cannot construct a control-service principal through the public API. A workload consumer cannot configure a control reservation. The administrative reconciliation and generation-retirement methods belong to a trusted daemon/operator path and must not be exposed as workload RPCs.

DG-0 proves the library registration boundary with a fake peer and backend. C02 supplies real local UDS authentication and a small client, but does not activate native registration. The server observes peer UID/PID through OS socket credentials; the client independently corroborates the authority UID/PID and its own identity in the handshake. Neither accepts caller-declared peer identity. C03 must still provide boot/start identity before the daemon may construct a complete trusted registration observation. Authentication alone issues no `Principal`, instance slot, lease or host budget. Workload/control-service registration requests currently return `ResourceControlUnavailable`; administrative credentials cannot perform registration.

The handshake negotiates contract compatibility and wire version 1, with no runtime capabilities advertised by C02. Consumer generation and credential digest determine workload/control-service roles; an independent administrative digest grants only the explicitly exposed administrative role. All consumer and administrative digests must differ. Caller credentials, future one-time helper permits and administrative operations are separate boundaries: a helper credential variant is not accepted as caller authentication. This remains a cooperative operating-account model, not isolation from a malicious same-UID process.

The authority holds an exclusive no-follow lock in the journal's parent directory. All journals in that authority directory share the lock. The lock is opened with `O_NONBLOCK`, and its opened descriptor must identify a private, current-UID regular file with exactly one link, including during explicit initialization. A FIFO or linked file cannot stand in for the lock. C01 derives canonical normal-service paths from the OS account rather than caller HOME/XDG values and refuses state/socket overrides. Project configuration cannot carry authority credentials or capacity. Production candidate paths remain unavailable until C10 supplies a parent-lease boundary.

`AuthorityStorage` exclusively opens and validates a journal without inventing a boot clock, recovering attempts or granting capabilities. `Authority::from_storage` activates it with an actual Backend/Clock and revalidates the accounting index inside the recovery transaction. `Authority::open` preserves that behavior through the same path. Explicit bootstrap remains separate from ordinary open; missing/corrupt/future-schema state is not repaired automatically.

## Bounded local protocol and credential transport

A frame has a four-byte length prefix and at most 64 KiB of JSON payload. Frames, message variants and nested wire types reject unknown fields; version/request identity and required capabilities are checked separately. Added fields are not automatically backward compatible. The server accepts at most 32 active session workers. Each frame read or write has an absolute 250 ms deadline, including idle waiting before the next frame; receiving another byte does not restart the deadline. An expired idle session is closed. These transport bounds do not establish the later end-to-end admission budget or C12 responsiveness qualification.

Framing uses `poll`, descriptor `O_NONBLOCK` and per-call nonblocking socket I/O. Descriptor nonblocking mode also bounds large writes on Darwin, where a per-call flag alone is insufficient. It drains buffered final data on peer closure without changing socket timeout options, which can fail with `EINVAL` on Darwin after the peer has closed. A malformed, truncated, expired or unavailable response cannot imply execution or release. The client does not retry automatically, and never substitutes an unmanaged authority or execution.

`CredentialHandoff` transfers one caller secret through a dedicated inherited descriptor, keeping the parent copy close-on-exec. `take_inherited`/`read_owned` consume and close the receiver descriptor on success or error, with bounded length and a 250 ms read deadline. Secret serialization is deliberate for the local authentication exchange; debugging and parser errors redact credentials. Subprocess tests observe that the FD is closed before a subsequent `exec` and the secret is absent from argv/environment/output. They validate transport hygiene, not C05 helper authorization, READY, payload startup or containment.

## Durable admission and launch

The journal schema is version 1. Initialization is explicit and only creates a new file. Opening a missing, corrupt, unknown-schema or inconsistent journal fails closed. SQLite uses WAL, FULL synchronous durability and immediate transactions. A startup scan validates the accounting index against every stored record.

The attempt key is `(consumer_id, consumer_generation, attempt_id)`. The request fingerprint incorporates both the caller's versioned execution digest and the resource intent. It excludes transport request IDs and the current policy revision. Replays return the original durable reservation or terminal result; a changed meaning or owner conflicts. A refusal is itself a terminal attempt, so a later independently requested admission uses a new attempt ID.

`begin_launch` durably consumes Prepared and returns a one-time `Secret` permit. Subsequent calls return the stored attempt without another spawn grant. The journal stores only the permit digest. If the first response is lost, the caller must reconcile; it cannot recreate a helper by replaying the transition.

`bind_scope` requires a fresh Backend witness binding the attempt, owner, exact process identity, scope and fully applied resource plan. `authorize_run` requires the bound helper identity and permit. Only its first successful response sets `may_exec=true`. Losing this response therefore leaves an uncertain/fenced execution, not permission to spawn again. A RunAuthorized record is not proof that the user executable started successfully.

Cancellation of Prepared is terminal and returns its reservation. Cancellation after launch commit becomes Draining and blocks late binding/authorization while retaining the reservation. Prepared alone expires after five seconds. Restart preserves Prepared with its original boot-relative deadline, and converts post-commit nonterminal attempts and registered instances to Suspect for reconciliation.

## Reclamation evidence

There is no unconditional `release(lease_id)` API. For a bound execution the backend must identify the exact scope and provide fresh evidence of root termination/reap, an empty scope, no surviving known members, complete tracking and no known escape. A previously lost track remains sticky across ordinary empty-group observations. Clearing it requires explicit reconciliation evidence or verified termination across a host reboot.

For an unbound committed launch, missing PID data or owner death is insufficient. The backend must positively rule out helper creation and every pending spawn. A host reboot can also resolve an old execution. `release_reason` distinguishes scope termination, proven absence of a helper and previous-boot termination. `AttemptRecord::known_not_started()` is true for prelaunch terminal refusals and a released `NoHelperCreated` result; resource release alone is not retry evidence.

No tombstone is deleted by a normal TTL sweep. Operator retirement requires all instances retired and no charged attempts. It records a permanent retired generation before compacting its terminal attempts, so old keys remain rejected.

Static control reservations are subtracted for all configured slots, including disconnected/offline services. Instance retirement makes a slot reusable but does not return the static reservation to workloads. Observed process identity includes boot ID, PID and start ticks to detect PID reuse.

The journal records the registration policy for each instance. Startup rejects removing a consumer or changing its generation, role, UID, instance limit or reservation while any instance remains active/suspect. Reconcile and retire those instances under the existing configuration first. Slot counts span consumer generations. This prevents a configuration restart from allocating a second control service against the same static reservation.

## Pressure and capability interpretation

The controller starts closed until the first valid host sample. A first healthy observation establishes readiness; after observed pressure or a sampling failure, recovery follows the design's 30-second steps. Samples use the same boot-relative monotonic clock, reject replay/future timestamps, and expire after six seconds. Downward targets do not reduce live lease amounts.

Every resource carries its own level and method. Accounting is not an OS memory cap; QoS cannot claim a memory or task limit; kernel controls require a contained cgroup scope. A plan is validated before admission and remains separate from AppliedResources. Compatibility checks require both an accepted protocol version and all requested capabilities; they do not launch a fallback authority.

## Boundaries deliberately left to later milestones

- Remaining DG-1: native boot/start identity and registration, host probes, resource policy application, launch helper, execution CLI, Cargo adaptation, bounded self-use, update/repair and measured macOS SLOs.
- CS-RG: Runner slots and transport lanes, approval migration, pinned client, process status integration and regression qualification.
- DG-LINUX: actual cgroup hierarchy, controllers, ancestor constraints and sandbox/proxy inclusion.
- DG-CACHE / DG-ADAPTERS: registered cache reclamation and additional tool-specific controls.

The journal and library do not own a user's process handle, process output, CodeSpace workspace lease or approval row. None of those lifetimes is inferred from the lifetime of a resource reservation.
