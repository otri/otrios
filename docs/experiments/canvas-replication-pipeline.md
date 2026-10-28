<!-- SPDX-License-Identifier: Apache-2.0 -->
# Bounded Canvas replication increments — 2026-10-19

This report separates four changes: negotiated opaque binary fields, opt-in
signal retention, reusable hash-encoding workspace and explicit TCP socket
flushing. The first three implement RFC 0007's initial increments; the socket
change is an independently measured latency experiment. Packed FlatBuffers
transport, configurable dispatch deadlines and optional compression remain later
increments. No new dependency or unsafe boundary is introduced.

At a requested 10 Hz, binary wrapping reduces observer payload from 8,157 to
2,106 B/update (74%) and measured paired-client CPU from 26.19 to 17.82 ms/op
(32%, including mount/warm-up). Opt-in history saves another 22% of payload
in its separate short trials. No consistent remote p95 latency win appears across the
remaining increments; socket flushing is not demonstrated as a latency fix.
These changes were committed on the feature branch and **not deployed at
measurement time**. The [subsequent rollout](release-discovery-deployment.md)
deploys them together with independent release discovery.

## Versions and semantics

| Increment | Commit | Control |
| --- | --- | --- |
| Binary envelopes | `4ce703e` | Same executable/authority with legacy response encoding selected |
| Requested history only | `4db4ead` | Binary envelopes, eager history (`4ce703e`) |
| Reusable workspace | `64996e9` | Opt-in history (`4db4ead`) |
| Interactive socket flushing | `54f0cdd` | Workspace version (`64996e9`); frame executables are byte-identical |

The adapter still accepts bounded absolute assignments from clients, assigns
causal metadata at the single authority, validates field policy and flushes the
journal before confirmation. Pending edits stay in RAM. Current text fields are
atomic strings. Touch trails and presentation clocks remain ephemeral/local.
No CRDT metadata is pruned or issued transaction silently discarded.

Binary responses require a `Mount.binary` opt-in for that epoch. Missing/false
uses the original vector representation. New decoders accept vectors and blobs;
older authorities ignore the added mount field and continue sending vectors.
Signed catalogs/UI, canonical node bytes and stored roots keep their encodings.
The negotiated inner and outer response envelopes both use blobs; client write
requests retain their existing representation. This comparison includes the
reduced per-byte validation/decoding work, not just network packet size.

The server derives signal retention from WindowSignal selectors in its retained
admitted UI catalogs. Layout Lab retains its requested toggle signal; Playground,
3D count and unrelated fields do not maintain unused history. Catalog retention
is currently bounded to eight versions. Existing history remains readable but
stops changing when no retained catalog requests it; reads age samples out without
writes. This is not yet general selector/subtree subscription narrowing, chunked
signal storage or per-writer receipt replication.

Workspace reuse applies to node hashing. One size-class buffer is borrowed
exclusively and retained per authority document, shared safely by its staged
clones. Classes range from 512 B to 256 KiB. Published nodes never borrow scratch
bytes. Shared ancestors are still rebuilt once per atomic batch. Transfer node
buffers are presized. Work counters cover hash-buffer growth requests, **not all
allocator calls**: register values, BTreeMaps, Arcs and transport still allocate.

The authority explicitly enables TCP_NODELAY on both listeners so ready
interactive writes/ACKs are not subject to TCP small-packet buffering. App-level
semantic coalescing and durable ordering remain responsible for batching; this
is not a new dispatch policy or a durability bypass.

## Controlled frame clients

Rust 1.90.0 release ARMv7, RK3126 Cortex-A7, Linux 6.18.57-spd-kiosk. The Mac
M2 Max hosts disposable durable authorities; two real Canvas clients execute on
the frame in one measured headless process. The normal direct GLES renderer
remains running. No second renderer, Wayland session or browser is started.

Every individual trial starts a **fresh authority store**. The same signed 3D Lab
catalog/UI/schema/behavior is used throughout; its catalog SHA-256 is
`3944cecef69c77868c4f2a85a269edd744d42ffd149cf6061d2463b26c113dbd`.
There are three alternating repetitions, 20 warm-up and 120 measured assignments.
Binary/history/socket comparisons request 10 and 30 Hz; workspace network trials
request 10 Hz. The writer waits for its durable confirmation and the peer's
observation before issuing the next assignment, with a maximum requested rate.
Actual achieved rates are reported; this does **not** measure open-loop drag
coalescing or saturation throughput with multiple writes in flight.

The final matrix runs after builds/checks finish. Earlier pilots and engineering
trials overlapping local compilation are excluded. CPU clocks/governors, thermals
and ordinary background services are not pinned. HTTP readiness precedes each
trial; benchmark authorities and routing never touch the live room's state.
Temporary raw TCP routes are tailnet-only and removed after each comparison;
original HTTPS configuration is compared exactly after cleanup. These are
controlled Tailscale/loopback paths, not public browser HTTPS measurements.

CPU uses exact child wait4 user+system time, including mount, warm-up and waiting.
100% is one core. CPU ms/op divides by all 140 assignments, including warm-up.
Wire values are actual binary application payloads observed by the peer after
warm-up, per measured assignment; initial mounts and TCP/IP/WireGuard overhead
are excluded. WebSocket byte counts remain available in raw output.
ACK/observation share the frame's monotonic clock; one-millisecond per-peer
polling limits sub-millisecond interpretation. There is no GPU presentation or
physical touch-to-photon measurement here, nor a whole-device CPU claim.
Statistics are medians of three per-run values, including median per-run p95,
not pooled percentiles.

### Binary envelopes, requested 10 Hz

| Metric | Immediate control | Increment |
| --- | ---: | ---: |
| Observer payload / B per update | 8156.6 | 2105.8 |
| Paired clients / % of one core | 24.57 | 17.31 |
| CPU / ms per operation, including mount | 26.19 | 17.82 |
| Peer observation p50 / ms | 48.29 | 30.02 |
| Peer observation p95 / ms | 122.31 | 112.25 |
| Durable ACK p95 / ms | 122.34 | 111.22 |
| Actual updates / second | 9.58 | 9.83 |

### Binary envelopes, requested 30 Hz

| Metric | Immediate control | Increment |
| --- | ---: | ---: |
| Observer payload / B per update | 7896.2 | 2023.7 |
| Paired clients / % of one core | 35.96 | 25.33 |
| CPU / ms per operation, including mount | 20.88 | 12.28 |
| Peer observation p50 / ms | 46.52 | 29.13 |
| Peer observation p95 / ms | 118.74 | 111.54 |
| Durable ACK p95 / ms | 118.72 | 111.57 |
| Actual updates / second | 18.07 | 23.03 |

### Opt-in signal history, requested 10 Hz

| Metric | Immediate control | Increment |
| --- | ---: | ---: |
| Observer payload / B per update | 2100.2 | 1642.0 |
| Paired clients / % of one core | 17.55 | 16.51 |
| CPU / ms per operation, including mount | 18.19 | 17.09 |
| Peer observation p50 / ms | 30.25 | 28.98 |
| Peer observation p95 / ms | 110.51 | 114.02 |
| Durable ACK p95 / ms | 112.28 | 113.70 |
| Actual updates / second | 9.77 | 9.82 |

### Opt-in signal history, requested 30 Hz

| Metric | Immediate control | Increment |
| --- | ---: | ---: |
| Observer payload / B per update | 2016.2 | 1642.0 |
| Paired clients / % of one core | 26.26 | 26.00 |
| CPU / ms per operation, including mount | 12.47 | 11.16 |
| Peer observation p50 / ms | 29.97 | 28.93 |
| Peer observation p95 / ms | 113.28 | 101.43 |
| Durable ACK p95 / ms | 113.22 | 105.86 |
| Actual updates / second | 22.92 | 24.22 |

### Workspace reuse, network clients, requested 10 Hz

| Metric | Immediate control | Increment |
| --- | ---: | ---: |
| Observer payload / B per update | 1642.0 | 1642.0 |
| Paired clients / % of one core | 16.34 | 16.40 |
| CPU / ms per operation, including mount | 16.90 | 16.99 |
| Peer observation p50 / ms | 29.30 | 29.12 |
| Peer observation p95 / ms | 103.74 | 113.92 |
| Durable ACK p95 / ms | 104.12 | 113.95 |
| Actual updates / second | 9.88 | 9.79 |

### Interactive socket flushing, requested 10 Hz

| Metric | Immediate control | Increment |
| --- | ---: | ---: |
| Observer payload / B per update | 1642.0 | 1642.0 |
| Paired clients / % of one core | 17.56 | 18.61 |
| CPU / ms per operation, including mount | 18.05 | 19.24 |
| Peer observation p50 / ms | 31.07 | 31.51 |
| Peer observation p95 / ms | 109.46 | 107.47 |
| Durable ACK p95 / ms | 106.67 | 108.95 |
| Actual updates / second | 9.84 | 9.84 |

### Interactive socket flushing, requested 30 Hz

| Metric | Immediate control | Increment |
| --- | ---: | ---: |
| Observer payload / B per update | 1642.0 | 1642.0 |
| Paired clients / % of one core | 26.42 | 26.51 |
| CPU / ms per operation, including mount | 11.27 | 11.41 |
| Peer observation p50 / ms | 28.86 | 29.90 |
| Peer observation p95 / ms | 101.44 | 101.26 |
| Durable ACK p95 / ms | 101.48 | 101.29 |
| Actual updates / second | 24.70 | 23.86 |


## Local materialized-tree work on the frame

The workspace comparison also runs the same generic library workload directly
on the frame: 64, 512 and 4,096 leaves, 1 KiB values, four 8-way ancestors,
100 warm-up and 1,000 measured edits per size, with three alternating repetitions.
Edit/offer/validate/apply/ACK and concurrent merge are measured independently.
No transport, durable journal, selectors or rendering is inside these timers.
The candidate uses its persistent workspace API; both versions include their
normal allocation costs. The live renderer remains running, so scheduling
outliers are retained.

| Leaves | Edit p50 µs, control → workspace | Edit p95 µs, control → workspace | Edit/offer/receive/ACK p95 µs, control → workspace | Delta B, both |
| ---: | ---: | ---: | ---: | ---: |
| 64 | 244.4 → 203.6 | 321.1 → 229.8 | 904.2 → 737.6 | 2093 |
| 512 | 271.5 → 225.7 | 330.2 → 276.5 | 949.1 → 887.8 | 2373 |
| 4,096 | 300.4 → 246.8 | 372.7 → 297.5 | 1081.2 → 960.7 | 2653 |

Each candidate size retains **2,048 B** of encoding scratch and reports **zero hash-buffer growth requests during 1,000 steady edits** in all three repetitions. Both versions hash five nodes per edit. These counts establish buffer reuse, not zero allocations across the transaction. Timing variation remains substantial with the live renderer running; treat the local timings as exploratory rather than a guaranteed latency improvement.


These results distinguish intrinsic tree work from the remote-client path:
clients mostly apply verified nodes, while the authority constructs/hashes them.
A server-only hashing optimization therefore need not lower steady client CPU.
Scratch-buffer growth counters do not establish allocation-free transactions.
Canonical transfer sizes and hash work remain comparable; no new hash format or
CRDT merge rule is responsible for the result.

## Verification and reproduction

Required `bash scripts/check.sh` passes at each runtime increment. Strict
all-feature/all-target Clippy and the browser Wasm target check pass. Tests cover
legacy/new binary interoperation and size boundaries; real-process durable edits
in both encoding modes; requested Layout Lab signal retention and absence of
unrequested history; immutable snapshot pinning; canonical-byte identity with and
without workspace reuse; invalid batches and bounded buffer reuse. Existing
policy, receipt replay, restart, journal failure and slow-reader tests remain in
force.

Build each committed revision with `scripts/build-canvas-pipeline-benchmark.py`
using a separate output folder and the configured Rust environment. It freezes
source for both ARM and Mac builds and records source-archive/binary hashes.
For subsequent comparisons use the preceding build as reference. Example
(substitute local paths and the existing frame connection):

```sh
python3 scripts/build-canvas-pipeline-benchmark.py --output /ABS/BUILD \
  --mac-target /ABS/MAC-TARGET --container EXISTING_ARM_BUILDER
python3 scripts/benchmark-canvas-pipeline.py \
  --reference /ABS/REFERENCE --candidate /ABS/CANDIDATE \
  --release /ABS/SIGNED_RELEASE --output /ABS/FRESH_RESULTS \
  --frame USER@FRAME --known-hosts /ABS/KNOWN_HOSTS --tailscale /ABS/TAILSCALE \
  --rates 10,30 --updates 120 --repetitions 3
python3 scripts/benchmark-merkle-workspace.py \
  --reference /ABS/HISTORY_BUILD --candidate /ABS/WORKSPACE_BUILD \
  --output /ABS/FRESH_TREE_RESULTS --frame USER@FRAME --known-hosts /ABS/KNOWN_HOSTS
```

For the historical legacy/binary comparison, use a separate checkout at commit
`ed3c86b` and its `scripts/build-canvas-pipeline-benchmark.py` to build the
reference and candidate from that same revision. Run that checkout's
`scripts/benchmark-canvas-pipeline.py` with `--legacy-reference` and a compatible
signed release and frame setup from that period. That revision contains both the
runner and the benchmark client that honors the flag. Commit `a88767f` removed
the client support; the current runner rejects the flag rather than silently
labeling a current binary as legacy. The archived results remain unchanged.

The current runner defaults to negotiated `/session`. When reproducing the
pre-unified-server builds above, add `--reference-session /apps/stream
--candidate-session /apps/stream`; their older binaries retain their original
handshake/encoding. Use the paired mode for those historical executables.

The tools temporarily bind loopback ports 8901/8902 and tailnet ports 18901/18902;
use an otherwise idle test machine. Frame files use unique bounded experiment
directories and are removed. Source/build hashes, all per-trial quantiles, bytes,
CPU/RSS and unabridged tree output are retained in the linked raw data. Pilot
output and temporary synthetic checkpoints/journals are not published as results.

After all 42 network trials and six local-tree runs, the original Tailscale
HTTPS configuration is restored with no temporary TCP routes. The frame's direct
service remains active with exactly one native renderer (PID 24330); its
temporary `bench-pipeline-*` directories are gone.
The live release pointer remains
`0b52c65a500d3026a9de4825199329383d5269fed359cf54945a1890fd2c2fd7`;
the publisher remains pinned to source `3f60260`. No production authority store
was reset or replaced. Main stays at `9de7618` and the benchmark reference stays
at `abf122ab2dbd2c37c9b66d6c183a1cf860698df6`.


Raw evidence: [binary trials](data/canvas-pipeline-2026-10-19/binary-results.jsonl) / [build metadata](data/canvas-pipeline-2026-10-19/binary-metadata.json), [history trials](data/canvas-pipeline-2026-10-19/history-results.jsonl) / [build metadata](data/canvas-pipeline-2026-10-19/history-metadata.json), [workspace trials](data/canvas-pipeline-2026-10-19/workspace-results.jsonl) / [build metadata](data/canvas-pipeline-2026-10-19/workspace-metadata.json), [socket trials](data/canvas-pipeline-2026-10-19/socket-results.jsonl) / [build metadata](data/canvas-pipeline-2026-10-19/socket-metadata.json), [tree trials](data/canvas-pipeline-2026-10-19/tree-results.jsonl) / [build metadata](data/canvas-pipeline-2026-10-19/tree-metadata.json).
