# RFC 0002: Incremental CRDT hierarchy and immutable presentation

Status: design with a first bounded implementation in [RFC 0005](0005-merkle-subtree-prototype.md):
immutable hierarchy, causal registers and scoped subtree transport/joins. The
live dashboard now uses the durable Canvas adapter in [RFC 0006](0006-durable-canvas-merkle-adapter.md). The original hash-cost experiment below
remains a hash-only measurement; it is not the new materialized-tree benchmark.

[RFC 0003](0003-module-roots-and-state-selectors.md) develops the module boundary,
typed state selectors, stable collection identity and subscription semantics
needed to make UI and state independently addressable data. Its scope includes
separate UI/State roots per module and value-level presentation invalidation
while causal metadata continues to synchronize.

## Goal and measured problem

A small leaf edit should cost work proportional to that leaf, its merge metadata
and its ancestor path. Document size/history must not determine normal input
latency. Presentation and ephemeral activity must not wait for reconciliation,
network or durable storage. Server acknowledgement defines durability.

On the RK3126 frame, ten disposable remote edits against a captured 14,626-byte
Automerge document averaged 335.046 ms per full-document decode/merge between
native workers, in addition to 90.813 ms for the measured sync exchange. These
are CPU-stage timings, not complete network/physical-display latency. The
single-owner native path removes that handoff; acknowledgement-only messages
also avoid reserializing/projecting unchanged state. The same wire experiment
then measured 62.415 ms. This remains a general-document baseline, not the
intended runtime's performance target.

## State representation

Use immutable typed leaves and bounded-fanout internal nodes. Leaf identity
hashes a versioned, domain-separated canonical envelope containing schema/kind,
value bytes and the CRDT metadata needed to merge it. Internal identity hashes
its version/kind, stable ordered child keys and child revision hashes, with
explicit lengths. Never hash pointers, process-dependent maps, UI caches or
noncanonical encodings.

An edit constructs a new leaf and copies/re-hashes its ancestor path. Unchanged
subtrees retain their hashes and cached views. Publish one new aggregate root
only after its referenced revisions have been validated/staged. Existing readers
keep their pinned root and immutable branches. Repeated edits within one publish
batch rebuild shared ancestors once.

The tree shape must be deterministic from stable keys, not insertion order or a
replica's balancing history. Compare compressed radix tries over ObjectIds with
canonical chunked trees for ordered CRDT sequences. A 32-way tree with four
ancestor levels describes 32^4 leaf slots, but is a benchmark shape, not a
worst-case depth promise for arbitrary identifiers. Specify adversarial-key,
depth and leaf-metadata bounds before adopting a representation.

Photo/blob data stays immutable and content-addressed: metadata edits replace a
small reference leaf rather than hash/transmit the photo again. A new blob is
hashed once while ingesting. Text and scene graphs use suitable fine-grained
CRDT leaves/chunks. A single growing text leaf would merely relocate the old
whole-document bottleneck.

## Synchronization and authority

Compare authorized roots/subtree hashes to find changed branches. Transfer
missing typed revisions or bounded deltas; validate only new material and reuse
validated immutable nodes. Hash equality is a shortcut for equality of covered
bytes, not a merge rule, signature, capability or proof of authorization.

CRDT operations still need causality, stable element identities and explicit
conflict semantics. Equal displayed values can have different merge metadata;
matching value hashes alone must never discard that difference. Define canonical
join state, tombstone handling and causal stability before history reclamation.
Durable journals and periodic compact snapshots should remain off the
presentation path. Safe pruning and federation are separate experiments.

The authority validates policy, persists accepted changes/root publication and
then acknowledges. Clients hold subscribed working state and pending intent in
RAM. Immutable code/blobs use bounded caches. Monitoring/private objects require
separate authorized roots; a public hash is never permission to fetch a private
node. Subscription diffs publish coherent snapshots and bounded latest-view
notifications rather than require rendering every intermediate root.

Continuous absolute controls may coalesce before CRDT commit. Ordered text,
insert/delete, counter and discrete commands require their proper semantics.
Touch trails and presence stay ephemeral; animation clocks remain local.

## Hash-only frame result

On the same RK3126 Cortex-A7 frame with clocks unchanged and the dashboard still
running, 1,000 edits to the benchmark path measured **442.749 us p50 / 473.957 us
p95 / 1,428.580 us max** for five SHA-256 calls (1 KiB leaf + four cached 32-way
ancestors). The occasional scheduling outlier matters: this is not a hard
real-time bound. It excludes merge, canonicalization, allocations, I/O, wire and
rendering, and uses cached sibling digests rather than allocating a full tree.
This establishes hash-work cost only, not implementation of the proposed runtime.

## Acceptance experiments

[RFC 0007](0007-bounded-replication-pipeline.md) defines the incremental allocation,
semantic consolidation, packed transport and optional compression experiment.
It preserves causal metadata and durable acknowledgment while keeping local
presentation independent of dispatch cadence.

- Measure leaf edit, canonicalization, ancestor copying/hashing, merge, view
  invalidation, allocation and wire size separately on this Cortex-A7 and Wasm.
- Vary leaf size, path depth, fanout and populated-tree size. Unrelated branches
  must not increase ordinary leaf-edit work. Record p50/p95/p99 and allocations.
- Compare full snapshot work with 1 KiB leaf + four cached 32-way ancestors;
  `cargo run -p otrios-core --features dashboard --release --example merkle_hash_cost`
  isolates the five SHA-256 calls. It excludes CRDT merge, canonicalization,
  allocation, storage, networking and rendering and allocates no million-leaf tree.
- Prototype target: sub-millisecond p95 for this bounded leaf/path update on the
  frame, including materialization and allocations but excluding durable I/O.
  This is a target to test, not a measured runtime guarantee.
- Test order-independent joins, concurrent siblings/same-leaf edits, deletes,
  hash equivalence with full recomputation, deterministic tree shape, unchanged
  subtree reuse, pinned old roots, invalid deltas and slow-subscriber backpressure.

No general object store, new CRDT family, shared-memory protocol or hardware
accelerator is enabled by this RFC. Those need bounded prototypes and their own
acceptance gates. The immediate dashboard change removes redundant document
reconstruction while keeping current merge semantics and the server's durable
history intact.
