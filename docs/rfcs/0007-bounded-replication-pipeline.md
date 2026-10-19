<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0007: Bounded allocation and batched replication

Status: initial increments implemented on `feat/canvas-replication-pipeline` and
[deployed on browser and frame](../experiments/release-discovery-deployment.md);
see [frame measurements](../experiments/canvas-replication-pipeline.md).
A [measured dispatch prototype](../experiments/canvas-dispatch.md) consolidates
continuous numeric edits in the adapter's single RAM queue. It was subsequently
[qualified and deployed](../experiments/canvas-foundation.md) as part of the
practical foundation, together with intent bounds and coherent covered views.
Its ownership simplification qualifies separately from speed claims; the combined
foundation lowers traffic without establishing a uniform CPU/latency win.
Packed envelopes and compression remain planned.
Extends RFCs 0002, 0005 and 0006. Main remains at the dashboard milestone;
performance experiments use disposable authorities; the subsequent authorized
rollout retains the live room and its durable stores.

## Pipeline and invariants

RAM intents → semantic consolidation → atomic tree batch → packed delta →
optional compression → bounded transport. Local presentation never waits for
dispatch, compression, network or fsync. The authority checks policy, persists
the entire accepted transaction and its receipt, then permits confirmation.

Consolidate unissued absolute assignments by field. Counter deltas must add;
ordered edits/events must preserve their meaning. Never discard issued causal
operations, transaction boundaries, uncertain intent sequences or receipts.
The current Canvas adapter supports absolute assignments only. Its one-flight
write plus bounded pending map consolidates those assignments. Merkle
`Snapshot::edit` already rebuilds shared ancestors once per atomic edit batch.
These are existing mechanisms, not new optimization claims.

## Memory ownership

The first implementation uses one size-class hash-encoding workspace per Canvas
authority document. Its safe Arc/Mutex ownership permits staged document clones
to reuse scratch without sharing mutable published data. Generic callers can
borrow `merkle::Workspace` across `edit_with_workspace` calls. The retained
classes are 512 B, 2 KiB, 8 KiB, 32 KiB, 128 KiB and 256 KiB. Transfer-node vectors
are presized. Counters measure hash-buffer growth requests only; they do not
claim that registers, maps, immutable nodes or network buffers allocate nothing.

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

Interactive authority sockets now explicitly enable TCP_NODELAY, separately
benchmarked from payload changes. Runtime batching should define when a packet
is ready rather than inherit TCP small-packet delays. Continuous controls now enqueue directly in the state adapter, removing the UI's
second pending map and its separate flush clock. A monotonic ready-to-send deadline
is fixed when a pending group starts: the later of now and last issued write plus
33 ms by default. New samples replace only unsent absolute assignments and cannot
postpone that deadline. A ready group waits behind at most one immutable issued
write; once its receipt arrives it uses the latest pending values. This bounds
scheduler delay, not network/fsync/confirmation delay.

`DispatchPolicy` permits 0..1000 ms and a 1..64 unique-field flush threshold;
configure it before gestures. Reaching the threshold makes the whole group ready,
without splitting a handler's multi-field assignment. Numeric staging uses a
bounded stack array and field validation uses a 64-bit duplicate mask. The pending
map, published nodes and issued vectors still allocate; this is not an allocation-
free transaction claim. Maximum encoded-message limits remain in force.

The first idle edit can issue immediately. Final release/cancel forces pending
intent ready; discrete actions/text, ACK, resync, mount and uncertain replay bypass
continuous throttling. Reconnect preserves the RAM pending deadline and issued
sequence while the process lives. Host ticks supply monotonic milliseconds;
backwards samples cannot postpone an existing group. Packed transport and optional
compression remain separate increments, with no new wire format selected here.

The shared adapter now caps pending plus issued value payloads at 256 KiB, in
addition to the 64-field and existing encoded-message limits. Admission checks
the entire candidate before modifying the overlay; a rejected batch retains the
previous queue. Replacing an unissued value releases its budget, while issued
values remain charged until their durable receipt arrives. `DispatchUsage`
reports pending/issued counts, payload bytes, monotonic ages and deadline delay.
Payload accounting excludes container capacity, trees, codec scratch and GPU
resources; it is not a total-allocation metric. Coalescing, replay and RAM restore
retain the original ages rather than concealing delay by resetting them.

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
