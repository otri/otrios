<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0007: Bounded allocation and batched replication

Status: accepted direction; implement and measure the increments below separately.
Extends RFCs 0002, 0005 and 0006. Main remains at the dashboard milestone;
experiments use disposable authorities and do not change the live room.

## Pipeline and invariants

RAM intents → semantic consolidation → atomic tree batch → packed delta →
optional compression → bounded transport. Local presentation never waits for
dispatch, compression, network or fsync. The authority checks policy, persists
the entire accepted transaction and its receipt, then permits confirmation.

Consolidate unissued absolute assignments by field. Counter deltas must add;
ordered edits/events must preserve their meaning. Never discard issued causal
operations, transaction boundaries, uncertain intent sequences or receipts.
The current Canvas adapter supports absolute assignments only. Its one-flight
write plus bounded pending map already consolidates those assignments. Merkle
`Snapshot::edit` already rebuilds shared ancestors once per atomic edit batch.
These are existing mechanisms, not new optimization claims.

## Memory ownership

Keep immutable published nodes pinned until all readers release them. Reuse
working memory separately: node-encoding scratch, dirty-path lists, serializer
builders, packet buffers and compression workspace. Pools have byte/count caps;
an oversized request gets bounded temporary storage or explicit rejection. Never
recycle bytes while a reader or asynchronous send still owns them. No raw-pointer
arena, unsafe allocator or new FFI boundary is authorized by this RFC.

Prefer a few buffer size classes to a pool of mutable finished FlatBuffers.
FlatBuffer offsets are local to their backing buffer. Reset/reuse builders only
after the finished message is no longer referenced. Measure retained capacity
and allocations before replacing safe Arc/BTreeMap nodes with a slab. Any slab
must preserve generations, snapshot lifetime and bounded slow-reader retention.

## Payload granularity and envelope

Do not maintain history for an unrequested field. Enable five-minute signal
retention only from an admitted UI's WindowSignal selectors. Existing persisted
history may remain readable but must stop changing when no longer requested.
Longer-term, history uses independent bounded chunks/slots, not one growing leaf.
Receipts must stay atomically durable with fields; later selective replication
must prove the writer's own receipt without broadcasting all receipt changes.

Use binary blobs for opaque transfer bytes, not widened integer vectors. Preserve
canonical node bytes/hash identity independently of the transport envelope.
Compare corrected FlexBuffers with one FlatBuffers envelope carrying identical
canonical deltas. FlatBuffers is not automatically a canonical hash encoding.
New byte representations require explicit negotiation or a coordinated protocol
version; silently emitting blobs to old Vec-only readers is forbidden.

## Dispatch, packing and compression

Use a monotonic deadline and a maximum byte/operation count for each dispatch.
Continuous controls can replace unsent assignments; release requests a final
flush. ACK/resync/admission control traffic must not wait behind drag throttling.
Keep local rendering at display cadence. Backpressure retains the latest bounded
pending state and explicitly resyncs readers whose acknowledged base was lost.

Pack related updates into one bounded message with shared envelope metadata.
Never merge independent transactions into a partially observable publication.
Benchmark uncompressed and LZ4 candidates first. Compress only above a measured
threshold and only when smaller. Independent compressed batches bind codec and
decoded length; reject unknown codecs, excessive lengths and corrupt payloads
before materialization. Hash the canonical decoded nodes, not compressed bytes.
Use bounded reusable output/scratch buffers. Compression dictionaries, if any,
need explicit identity and reconnect/bootstrap handling. No indefinite wait to
fill a packet, hidden double compression or cross-private/public batch is allowed.

## Incremental sequence and evidence

1. Fix opaque binary wrapping with legacy decode tests and explicit negotiation;
   compare the exact same workload on the frame before changing state content.
2. Make signal retention opt-in from admitted UI; prove Layout Lab still retains
   its requested signal and unrelated apps emit none. Benchmark separately.
3. Reuse bounded node-encoding workspace without changing canonical bytes,
   hashes, snapshot isolation or merge semantics. Measure allocation/work counts
   and frame CPU, throughput and latency against increment 2.
4. Prototype the schema-defined packed envelope and dispatch deadlines. Exercise
   burst input, slow peers, reconnect, final release and conflicting peer edits.
5. Compare optional compression with the identical uncompressed packed bytes;
   choose defaults from frame/browser results rather than compression ratio alone.

Each increment gets a logical commit and recorded revision, pinned toolchain,
hardware, repetitions, warm-up and raw data. Frame measurements distinguish
local CPU/tree work, durable ACK, remote observation, renderer feedback and wire
bytes. Use fresh matched stores for causal comparisons; do not mistake history
growth or lower achieved throughput for an optimization. Track allocation/work
counts, pool capacity, queue age, p50/p95/p99, bytes per useful edit and actual
rate. Small/sparse/burst workloads and slow-subscriber bounds matter as much as
large batches. Main/deployment decisions remain separate from benchmark success.

CRDT metadata reclamation is separate from input coalescing: prune only after a
specified causal-stability/resumption rule. This work adds no federation,
sequence CRDT, exactly-once event delivery or Linux shared-memory claim.
