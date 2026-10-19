<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0005: Bounded incremental Merkle hierarchy and subtree transport

Status: implemented bounded library behind `otrios-core/merkle`.
[RFC 0006](0006-durable-canvas-merkle-adapter.md) now integrates it into the live
Canvas backend with durable server receipts and RAM-only client intents.
The verified pre-Merkle baseline is tagged
`2026-10-19-canvas-apps-milestone` at `8dd9bc9`.

## Representation and semantics

`Snapshot` pins an immutable `Arc` root. A node has an optional register and an
ordered map of stable u64 child keys. Keys are schema fields/map/item identities,
never view positions. A node may contain both data and descendants. The shape is
exactly the supplied logical path, independent of insertion/batch/merge order;
there is no replica-local tree balancing. Wide maps will need a deterministic
radix/chunk layer; the first slice rejects more than 32 direct children.

`Snapshot::edit(actor, batch)` validates unique paths, assigns causal registers,
and copies/hashes only edited nodes and ancestors. Shared ancestors are rebuilt
once per batch. Unchanged children keep their actual allocations. Each node
caches its SHA-256, height, expanded node count and canonical byte total; resource
checks do not walk unchanged descendants. Errors leave the old snapshot intact.
A host publishes a complete new root with an expected-root check. This library's
publication is RAM-only, not a durable acknowledgment or a general transaction.

A register binds a schema digest, opaque canonical value bytes, a per-leaf actor
version vector and live assignment dots. Hosts must qualify value bytes against
that schema before accepting a command. Merely carrying a schema digest does not
validate a FlexBuffers payload. Writer incarnation IDs must be unique and cannot
be reused after losing causal state. The leaf-local counter advances on every
assignment, including repeated displayed values. There are no wall clocks.

Join retains a dot if both sides contain it or the other side has not observed
it, and takes the componentwise maximum causal context. Concurrent assignments
remain explicit alternatives. A subsequent assignment after observing all of
them resolves them. `None` is a live deletion assignment, retained with causal
metadata; it is distinct from an uncreated slot. Concurrent delete/write retains
both alternatives. No timestamp winner is invented. Identical dots with unequal
values are an actor collision and reject the join. Schema changes require a new
logical identity or an explicit future migration.

This follows causal dot-store/MV-register designs in Almeida, Shoker and
Baquero's [Delta State Replicated Data Types](https://arxiv.org/abs/1603.01529),
and the [state-based register survey](https://www.lip6.fr/Marc.Shapiro/papers/Comprehensive-CRDTs-RR7506-2011-01.pdf).
Our transport sends complete changed register states, not algebraic delta
mutators. SHA equality skips equal subtrees; it never replaces the causal join.
Actors/tombstones are not pruned. The bounded domain rejects an overflowing join
rather than truncating causality. This is not a text, sequence, counter or
observed-remove map implementation; branch membership only grows in this slice.
Deleting a register does not delete its descendants or permit key reuse.
`merge_subtree(path, edited_partial_snapshot)` joins a partial replica back into
its authorized location, retaining unseen sibling allocations and metadata; it
does not replace the global root with an incomplete client tree.

## Subscription, policy and coherence

`Authority::subscribe(trusted_allowed_roots, requested_path)` checks coverage
before capturing the initial subtree. Allowed roots come from embedding server
policy, never the request body. A grant covers its entire descendant subtree;
a private sibling must be outside it. There is deliberately no fetch-by-hash API.
Only the requested node and descendants are transferred: not the global root,
parent metadata or sibling hashes. Known content hashes are not credentials.
The embedding transport must authenticate the authority and protect messages;
the prototype does not implement a new network identity or capability system.

Registration/capture and root publication use the same single owner (`&mut
Authority`). A concurrent embedding server must put both under the same lock.
A captured initial frame remains valid if publication occurs before first send;
the next poll after acknowledgment compares with the latest root. Missing slots
are subscribable and becoming present produces a new revision. Polling is an
embedding API, not a background notification service or selector interpreter.

One subscription retains at most an acknowledged subtree and one in-flight
subtree/frame. While waiting, repeated offers reuse the same immutable bytes.
Intermediate publications do not accumulate a queue. After exact cursor/root ACK,
compare the acknowledged subtree with the latest authority subtree and transmit
only nodes along changed paths. The latest CRDT state subsumes skipped versions;
required causal metadata is never replaced by a displayed-value shortcut.

A receiver verifies every new canonical record and materializes a complete
replacement before exposing a root. Existing nodes at matching paths are reused
by digest. The base root and next lease sequence must match for a delta. Lost
RAM, missing dependencies or sequence gaps require explicit resync; resync sends
a full subtree with a later sequence. Old epochs/leases/paths are rejected.
Epochs must identify a unique authority incarnation and change after lost cursor
state. Cancellation drops the server's pins; the host must also cancel the
client replica and its local presentation scope to reject late frames. Policy
revocation must call cancellation even when the content digest did not change.

This read replica replaces authority state. The [Canvas adapter](0006-durable-canvas-merkle-adapter.md)
keeps optimistic intents in a separate RAM overlay. Incoming untrusted writes
cannot call `publish` directly: the server enforces the admitted schema, field
grants, session policy, durability-before-ACK and intent receipts.

## Canonical binary format v1

All integers are unsigned little endian. Counts/lengths are checked before
allocation. There is no padding, trailing data, implicit text or native pointer.
SHA-256 covers the entire node envelope, including version/domain and metadata.
A hash is identity only, not a signature or authorization proof.

A node is, in order:

- Exact domain bytes `OTriOS\0merkle-node\0v1\0`.
- Register presence u8 (only 0/1). When present: schema digest (32 bytes),
  context count u8, sorted `(actor[16], counter u64)` entries, live count u8,
  sorted `(actor[16], counter u64, value-present u8, [length u32, value bytes])`.
  Zero counters, duplicate actors, live counters unequal to their context maximum,
  and empty register contexts/live sets are rejected. Deleted values omit length.
- Child count u8, then sorted `(key u64, child digest[32])` entries.

A transfer is, in order:

- Exact domain bytes `OTriOS\0merkle-transfer\0v1\0`.
- Authority epoch u64, lease u64, sequence u64 (nonzero), path length u8 and keys.
- Full-snapshot flag u8. Optional base and target digests each use presence u8
  followed by 32 bytes when present. A full snapshot has no base. A missing target
  represents an uncreated slot, with zero records.
- Record count u32 and, in increasing computed digest order, each record's u32
  byte length and complete canonical node bytes. Equal content deduplicates.

A delta's referenced child must be a supplied new record or the matching old
child at that path. Duplicate, unordered, unreachable and unknown records reject
the entire frame. Decoding hashes each supplied record once. Exact replay is checked using a small
control fingerprint over the parsed envelope and verified record digests, not a
second retained full frame. A changed record set at the same cursor is rejected. Materialization
checks depth and charges expanded node/byte budgets while descending; a tiny DAG
must not cause exponential allocation before a later root-size check.
The decoder rejects malformed lengths/flags and extra bytes. Cursor/ACK framing
on the network is still the embedding host's responsibility; process tests use
length-prefixed TCP with explicit cursor/root replies.

## Explicit prototype bounds

| Resource | Limit |
| --- | ---: |
| Logical path depth | 16 |
| Direct children per node | 32 |
| Actors retained per leaf | 32 |
| One live value | 64 KiB |
| Canonical node | 256 KiB |
| Expanded subtree | 16,384 nodes / 16 MiB canonical bytes |
| One atomic edit batch | 256 distinct paths |
| Transfer bytes | 16 MiB + 16,384 record lengths + 1 KiB envelope allowance |

Pinned roots, subscriptions and caller-retained snapshots consume memory. This
library bounds each tree/frame, not aggregate host memory across arbitrary
leases. A deployed host needs per-session lease/pin budgets and reclamation.
No filesystem cache, durable journal, history GC, cross-authority transaction,
general query, stable collection selector or Linux shared-memory IPC is added.

## Acceptance and next cutover

Tests cover independent full reconstruction, pinned roots and allocation reuse,
shared-ancestor batches, concurrent siblings/same-leaf changes, duplicate values
with different metadata, delete/write conflicts, seeded join schedules,
malformed/partial frames, DAG expansion, missing slots, denied siblings, stale
publication, exact ACK/replay, slow-reader coalescing, resync and cancellation.
Two real child processes replicate distinct subtrees over loopback sockets and
compare roots; this is a process-boundary test, not Linux shared-memory evidence.

`merkle_tree_cost` measures materialized edit, encode/offer, decode/validate/apply,
and concurrent join separately, with actual nodes and allocations. The optional
`otrios-web-host/merkle-probe` diagnostic executes the same covered runtime as
Wasm, with no change to the default live app. See the experiment report for
[commands and measurements](../experiments/merkle-subtree-validation.md). Do not equate either with network or screen latency.

The Canvas adapter cutover is deployed and covered by [browser/frame acceptance](../experiments/canvas-merkle-adapter-validation.md).
Next: extend field bindings to general collection/query dependency invalidation
and arbitrary subtree leases. App-scoped state leases, mount qualification and
navigation cleanup are implemented; this does not provide a general query engine.
