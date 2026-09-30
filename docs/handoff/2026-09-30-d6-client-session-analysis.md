# D6 option (b): client-session designs, analysed on paper (2026-09-30)

> **Status: dated, non-normative analysis and handoff record.**
> - It analyses option (b) of W3 decision 3 on paper only. No code, experiment or measurement was run for it.
> - It selects no D6 direction and approves no protocol, wire, journal or dependency change, implementation or merge.
>   Decision 3 stays open.
> - It does not redefine N2 or the declared threat model. Where a candidate would need either to change, this record
>   says so and names it as a separate owner decision.

**Direction.** The owner's direction of 2026-09-30 was: analyse on paper option (b) of the
[W3 decision packet](2026-09-28-w3-decision-packet.md) (section 6, decision 3), which is "change DevGuard's
client-session design so an inherited endpoint gains nothing".
- The candidates are the two that W3 named, a per-request connection with a one-shot token and an authenticated or
  encrypted transport, plus any others that emerge.
- Each is assessed against the holder effects W3 observed, the macOS creation window of `connect_timeout`, the threat
  model, N2, compatibility with wire version 1 and the journal, and its interaction with D6 option (a) and the carrier
  choice.

**Sources.** Citations are at DevGuard `main` `4898259a8a5b7c2ed0b3a46a3567cb419b0a73d5`. The other inputs are:
- the [W3 decision packet](2026-09-28-w3-decision-packet.md), sections 3.3, 4 and 6;
- the [W0–W2 packet](2026-09-28-upstream-adapter-packet.md), section 8 (S14) and section 9.3;
- the [work specification](2026-09-28-cs-dg-upstream-adapter-work-spec-1.md), sections 3.2 and 9;
- the CodeSpace #79 run of 2026-09-30.

**Evidence levels** are as in the W3 packet: *record*, *source*, *test* (earlier runs, cited), *inference*.

## 1. Baseline

### 1.1 How a holder gets the endpoint *(source)*

- **The creation window (S14).** `connect_timeout` makes the session socket in four steps:
  1. a raw `socket()` (`crates/client/src/connect.rs:39`);
  2. a close-on-exec duplicate (`:46`);
  3. dropping the inheritable original (`:51`);
  4. connecting the duplicate (`:59`).

  A child spawned between steps 1 and 3, by a spawner that does not exclude unrelated descriptors, holds a descriptor
  for the same socket. The owner then connects, greets, authenticates and registers that socket.
- **One connection per client.** Every `Client` request is one frame exchange on the connection that
  `Client::connect` made (`crates/client/src/lib.rs:25-64`, `:203-235`).
- **The command-line owner** already opens a fresh session for each request (`docs/contracts.md:121`). Each such
  session is still several frames: hello, authenticate, register, request.

### 1.2 What the authority binds to a connection *(source)*

- **Peer identity is observed once.** `session()` observes the peer when the session starts, with `getpeereid` and
  `LOCAL_PEERPID` (`crates/daemon/src/server.rs:1253`; `crates/client/src/peer.rs:14-40`). These name the process
  that connected. Nothing identifies the writer of a later frame.
- **State accumulates across frames.** A session keeps `greeted`, `role`, `consumer` and `principal` from one frame to
  the next (`server.rs:1213`). Every later frame on the socket is handled with that state (`:1347`). `Register` binds
  the instance to the observed peer (`:1462`).
- **Two session kinds already allow a single authorized request.** A helper session presents one grant and makes no
  other request, and a lease-holder session only asks for its lease's status (`server.rs:1348-1358`).
- **Bounds.**
  - A frame is at most 64 KiB.
  - Each frame read or write has an absolute 250 ms deadline, including idle waiting (`docs/contracts.md:19`).
  - At most 32 session workers run; connections accepted beyond that are dropped (`server.rs:1092`).
  - A malformed, truncated, expired or unavailable response implies neither execution nor release, and the client
    does not retry (`docs/contracts.md:21`).

### 1.3 Where secrets travel *(source)*

| Direction | Secret | Message |
| --- | --- | --- |
| client → authority | caller credential | `Authenticate` (`crates/client/src/protocol.rs:61-63`, `:39-48`) |
| authority → client | one-time helper permit | the `LaunchGranted` reply to `BeginLaunch` (`LaunchGrant.permit`, `protocol.rs:258-261`) |
| client → authority | permit | `Launch`, in the helper's own session (`:106-110`) |
| authority → client | lease token | the `LeaseGranted` reply to `AdmitLease` (`:185-188`) |
| client → authority | lease token | `AdmitChild` (`:121-125`), `LeaseStatus` (`:132-135`) |

- **No other reply carries a secret.** No other reply type contains a `Secret`, including the contract types that
  replies carry (`AttemptRecord`, `InstanceIdentity`, `LeaseRecord`, `LeaseView`).
- **The authority generates both secrets and journals only their digests.** For the permit: generated at
  `crates/core/src/authority.rs:416`, digest stored at `:419-420`, checked at `:1236`. For the lease token: generated
  at `:851`, digest stored at `:866`.
- **What a client-side holder can see.** It shares the owner's end of the connection, so it can read what the
  authority sends and write what the authority receives. It cannot read what the owner sends. *(stream-socket
  semantics; W3 observed replies being read, and nothing reading requests.)*

### 1.4 Holder effects observed by W3 *(test, W3 packet section 3.3)*

This record names them:

| Name | Effect | W3 |
| --- | --- | --- |
| E1 | read the authority's replies, including the permit, while the owner's own read fails | 5/5 |
| E2 | inject a well-formed request that acts with the owner's session state (a `Cancel` under the owner's principal) | 3/3 |
| E3 | break the session with a malformed frame or `shutdown`, after which the owner fails closed | 6/6 |
| E4 | keep using the session after the owner closed its copy, until the idle deadline | 3/3 |
| W | receive the endpoint at all, through the creation window | inheritance matrix |
| X | channel exposure: another process holds the owner's endpoint | follows from W |

- **W3's other result.** A second `Authenticate` on a session was refused (3/3).
- **Inheritance.** In W3's matrix the session socket reached 34/400 and 34/400 std children, and 42/400 and 40/400
  Tokio children, in two samples. It reached 0/400 in both samples with Codex's kernel close-by-default
  (`DescriptorPolicy::Explicit`). `spawn_guard` prevented the leak only when both the socket's creation and the spawn
  took it.
- **X and N2.** The W0–W2 packet reads accidental exposure as falling under N2, as "credential isolation, no channel
  exposure" (section 9.3). The threat model is cooperative workloads of one operating account, not containment of
  malicious same-UID programs (`docs/design.md:14`).
- **The CodeSpace side.** [CodeSpace #79's run](https://github.com/novelKR/CodeSpace/issues/79#issuecomment-5917806992)
  of 2026-09-30 found that CodeSpace's own Runner pipe path, at
  `6b9826e`, gives its children other executions' transient descriptors: PTY pairs often, and pipe pairs twice. That
  path is therefore a spawner of the kind that would receive S14's window in an InProcess Gateway. *(test for
  CodeSpace's descriptors; inference for DevGuard's socket, which that run did not hold.)*

## 2. Candidates

### 2.1 (b1) Per-request connection with a one-shot token

The name admits two shapes. They behave differently.

**b1-t, a token issued in a reply** and presented once in the next request, on a new connection.
- **It closes nothing against E1.** A holder of the connection that carried the token can read it and present it first.
  So a reply-delivered token limits replay, not theft. It also adds a secret to the reply stream. *(inference)*
- **Everything else stays open.** E2, E3, E4, W and X are as in 1.4, on every connection.

**b1-s, a self-contained request.** Each connection carries exactly one request. That request frame carries its own
authorization, and the authority closes the connection after the reply. The authorization is the caller credential
that exists today, sent only from client to authority, so no new cryptography is involved. No authority accumulates on
a connection.

| Aspect | Assessment *(inference)* |
| --- | --- |
| E2 | **Closed for a holder without a credential.** A frame without the proof is refused, and a holder cannot read the owner's request. Today's gap, after `Register` and before the owner's request, disappears because there is no session state to borrow |
| E4 | **Closed.** Nothing remains after the reply |
| E1 | **Open.** The permit and the lease token still arrive in replies, unless combined with b3a |
| E3 | **Open, limited to one request.** The client already fails closed and does not retry |
| W, X | **Open.** Every request now passes through `connect_timeout`, so there are more windows, each shorter-lived |
| Holder with its own credential | **Open, and existing today.** A holder that is itself a registered consumer could send a complete request of its own on the owner's connection before the owner does. The authority would then register an instance with the owner's observed process identity. This is not run; W3 tested only that a second `Authenticate` is refused |
| Changes | Wire: a request form that carries its authorization. Version 1 has `Authenticate` as a separate frame and rejects unknown fields, so this needs a new version or a capability-gated request shape. Authority: one request per connection, and registration per connection. Client: accordingly. Journal: none. Sessions are not journaled, and registering again per session is the existing model (`docs/contracts.md:71`). Dependency: none |
| Cost | One accept, peer observation, credential check and registration per request, within the 32-session bound |
| N2 | Identity is unchanged: each connection's peer is observed, and the owner is the one that connects. The credential is sent more often, but only in the owner's own writes, which a holder cannot read. One-time execution is unaffected |
| (a) and carriers | Independent of both. Where (a) is adopted, W disappears for that spawner |

### 2.2 (b2) An authenticated or encrypted transport

A reviewed library would provide confidentiality and integrity between client and authority over the local socket. No
library is selected (W3; work specification section 9.3).

| Aspect | Assessment *(inference)* |
| --- | --- |
| E1 | **Theft closed.** A holder that reads gets ciphertext. The owner's read still fails, so E1 becomes E3 |
| E2 | **Closed.** A holder cannot author a valid record, and its bytes corrupt the stream (E3) |
| E4 | **Authority closed.** The holder can still keep the connection, and a session worker, until the deadline |
| E3 | **Open**, including interference with the handshake |
| W, X | **Open.** The specification says a transport does not prevent "inheritance itself, endpoint interference, inherited in-memory secrets in all threat models, or every denial of service" (section 9.3) |
| Design problem | Key establishment. Each side must authenticate the other before any secret moves, which needs trust anchors and key storage, for example the caller credential as a pre-shared key, or certificates. The construction has to come from a reviewed library; the specification forbids inventing a cryptographic protocol to avoid a dependency (section 9.3) |
| Changes | Client and authority gain a transport layer under framing. Wire: not compatible with version 1; a version or negotiation step is needed before encryption. Dependency: new, in the authority's transport, under the adapter policy of design revision 2 as proposed in [#20](https://github.com/novelKR/DevGuard/pull/20) (not merged): justified by what it replaces and how its cost is contained. Journal: none |
| N2 | Unchanged by itself. It must not be used to reword "no channel exposure" (section 9.3) |
| (a) and carriers | Complements (a). Independent of the carrier choice. The helper's own session is made in the helper's process, which spawns nothing while it connects and closes the session before exec (`docs/contracts.md:94`). *(inference)* |

Compared with b1-s combined with b3a, b2 additionally keeps non-secret replies, such as attempt records, confidential.
It costs a dependency and a handshake.

### 2.3 (b3) Other candidates that emerged

**b3a, no secret in any reply.** The owner generates the one-time permit and the lease token itself, with the
operating system's random source, and sends only their digests, in `BeginLaunch` and `AdmitLease`. The authority
stores the digest as it does today. No reply carries a secret.

| Aspect | Assessment *(inference)* |
| --- | --- |
| E1 | **Secret theft closed.** No reply contains a secret. The request that carries the digest cannot be read by a client-side holder, and a digest is not the secret |
| E2 | **Must be closed underneath.** A holder that could inject `BeginLaunch` with a digest of its own, under the owner's principal, would own that grant. So b3a needs b1-s or b2 |
| E3, E4, W, X | As the transport underneath |
| Changes | Wire: new request fields, and replies without `permit` or `token` (both are optional in version 1). Core: generation moves to the client; the digest check at presentation is unchanged (`authority.rs:1236`). Journal: format unchanged. Semantics: a lost `BeginLaunch` reply no longer loses the permit. Whether an owner that then confirms the commit with `Lookup` may start its helper is new behaviour. It touches "no automatic replay of uncertain work" and would need its own analysis; this record does not propose it |
| N2 | One-time execution is unchanged: single use by digest. The permit's unpredictability now depends on the client's random source |
| Carriers | Coupled to decision 4. The carrier is the owner-to-helper leg; b3a changes only the authority-to-owner leg |

**b3b, the session outside the owner's process.** The owner never creates the session socket. A dedicated DevGuard
client process connects, authenticates and makes the request. DevGuard starts it with kernel close-by-default, and it
spawns nothing else. Only objects that macOS creates close-on-exec atomically cross between the owner and that
process: regular files opened `O_CLOEXEC` (S5), and the child's standard descriptors set up by the spawn.

| Aspect | Assessment *(inference)* |
| --- | --- |
| W, X, E1–E4 in the owner's process | **Closed for the session socket,** because no such socket exists in the owner's process |
| What moves | The owner-to-connector channel. Pipes and socket pairs have the macOS window (S1, S3). Regular files are atomic, but they put the exchanged data, including a permit, on disk, with the concerns the W0–W2 packet lists for a file-backed permit (section 9.3). FIFOs opened `O_CLOEXEC` are atomic, but W3 found that this macOS host's `poll` does not report EOF on a FIFO once its writers have closed, so today's readers cannot use them |
| Identity | The authority registers the process that connects (`server.rs:1462`), which would be the connector, not the owner, and an instance lives as long as its registered process (`docs/contracts.md:110`). Keeping the owner as the instance needs a new identity rule, for example the authority observing the connector's parent and its start identity. That changes N2's stable-identity mechanism and is **a separate owner decision** |
| Changes | Client: a connector and its spawn. std `Command` offers no kernel close-by-default (S7). Authority: the identity rule. Wire: possibly none for the requests themselves. Journal: instance-identity semantics. Dependency: none |
| Cost | One process start per request, or a long-lived connector whose channel must itself be atomic |
| (a) and carriers | Independent of CodeSpace's spawners for the session socket. Shares the file-carrier questions with decision 4 |

**b3c, binding requests to their connection: rejected.** An inherited descriptor is the connection itself. Today's
per-session binding is exactly what E2 and E4 use. DevGuard observes identity once per session (1.2). Whether macOS
offers a per-message sender identity for stream sockets was not assessed.

**Not assessed.** Other macOS IPC mechanisms, such as Mach messages, XPC and file ports. Their descriptor-creation
semantics need their own source reading. The specification warns against generalising from specific `socket()` or
`pipe()` paths (section 9.1).

## 3. Summary *(inference)*

| Candidate | E1 secrets | E2 | E3 | E4 | W, X | Wire | Journal | New dependency | Separate owner decision |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| b1-t | open (adds one) | open | open | open | open | new | none | no | — |
| b1-s | open | closed | open, one request | closed | open, more windows | new | none | no | — |
| b2 | closed; becomes E3 | closed | open | authority closed | open | new, not v1 | none | yes (library) | — (N2 must not be reworded) |
| b3a on b1-s or b2 | closed | as underneath | as underneath | as underneath | open | new fields | format unchanged | no | if the lost-reply behaviour is proposed |
| b3b | closed in the owner | closed in the owner | closed in the owner | closed in the owner | closed in the owner; moved to the connector channel | possibly none | identity semantics | no | yes (identity rule) |
| b3c | rejected | | | | | | | | |

## 4. Findings *(inference)*

1. **While the endpoint lives in the owner's process, no (b) candidate removes the creation window or denial of
   service.** With b1-s plus b3a (no cryptography, no dependency), or with b2, an inherited endpoint gains no secret
   and no authority. It still holds the owner's channel (X) and can break it (E3).
2. **So (b) alone cannot make an inherited endpoint gain *nothing*,** under the W0–W2 packet's reading of N2 ("no
   channel exposure"). Accepting the residual X and E3 would change that reading. That is a separate owner decision,
   and this record does not request it.
3. **Only b3b removes the endpoint from the owner's process.** It needs a new identity rule (a separate owner decision)
   and an owner-to-connector channel. Each atomic option for that channel on macOS carries a known cost.
4. **Apart from b3b, only (a) removes W and X,** for each spawner that adopts it. CodeSpace #79's run shows
   that CodeSpace's pipe path currently passes other executions' transient descriptors to its children. Option (c),
   (a) together with (b), would use b1-s plus b3a as defence in depth where (a) is not yet adopted.
5. **If (b) is developed further, the smallest step that closes the authority effects** (E1 secrets, E2, E4) without
   cryptography or a new dependency is b1-s combined with b3a. It changes the wire, but not the journal's format.

## 5. How a chosen candidate would be tested

W3 C's holder cases become acceptance tests, re-run against the candidate, with synthetic credentials only:

| Case | Pass condition |
| --- | --- |
| T1 read (E1) | During a grant and a lease exchange, the holder obtains no permit, token or credential; the receipts are scanned. The owner fails closed without inferring execution or release |
| T2 inject (E2) | Well-formed `Cancel`, `BeginLaunch`, `AdmitLease` and `Register` frames from the holder, including one with a credential of its own, are refused. The journal records no change under the owner's principal |
| T3 break (E3) | After a malformed frame, `shutdown`, or a reply the holder consumed, the owner fails closed and does not retry automatically |
| T4 after close (E4) | The holder's use after the owner closed its copy is refused, or the connection is already closed |
| T5 inheritance (W) | W3's matrix, reported per spawner. W is expected to persist for b1, b2 and b3a, and to be absent from the owner's process for b3b |

**Controls:**
- an inheritable file, visible to std children (as NC1);
- a baseline with no holder;
- the candidate disabled, so the effect returns.

**Candidate-specific tests:**
- **b3a:** every reply is scanned for secret material, and a lost `BeginLaunch` reply is tested.
- **b2:** interference with the handshake, and negotiation with a version-1 peer.
- **b3b:** the instance's identity is the owner's and is retired with it, and the connector channel shows zero
  inheritance across std, Tokio and Codex spawners.

**Compatibility.** A version-1 client with a new authority, and the reverse, fail cleanly, with no execution inferred.
Old and new builds open the same journal.

**Scope.** macOS first. Linux separately (DG-LINUX), where `SOCK_CLOEXEC` would close W for DevGuard's own socket.

## 6. Decisions this record informs

None is taken here.
- **Decision 3 (D6 direction) stays open.** This record adds that (b) reaches "no secret and no authority", not
  "nothing".
- **If (b) is chosen, the owner would decide:**
  - which shape: b1-s with b3a, b2, or b3b;
  - separately, whether N2's reading of channel exposure changes;
  - separately, b3b's identity rule.
- **Decision 4 (carriers)** interacts with b3a (where the permit originates) and with b3b (files as the channel).
- **Unaffected:** the CodeSpace #79 remedy comparison (its own step 3 decision) and the staged pin direction.

**Next allowed action:** the owner's decisions.
