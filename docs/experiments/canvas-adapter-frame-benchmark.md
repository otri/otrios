<!-- SPDX-License-Identifier: Apache-2.0 -->
# Canvas adapter comparison on the photo frame — 2026-10-19

The deployed Merkle adapter reduces application CPU in this small app, but does
not yet improve end-to-end peer latency. Its current transport uses substantially
more bytes. Native presentation feedback improves modestly in these runs, while
whole-device CPU does not fall with application CPU. Treat this as a measured
mixed result, rather than a general Merkle speedup claim.

## Versions and control

- Baseline: `2026-10-19-canvas-apps-milestone` (`8dd9bc9`), the Automerge Canvas backend.
- Merkle: `3f60260` / deployed signed release `0b52c65a500d3026a9de4825199329383d5269fed359cf54945a1890fd2c2fd7`.
- Rust 1.90.0 release ARMv7; RK3126 Cortex-A7, Linux `6.18.57-spd-kiosk`.
- Same 3D Lab catalog, behavior, state schema, compressed font and GLES renderer.
  Both catalogs hash to `3944cecef69c77868c4f2a85a269edd744d42ffd149cf6061d2463b26c113dbd`.
- Direct DRM/GBM/EGL/GLES2, RGB565, unchanged accepted rotation/touch calibration.
  Exactly one native renderer during display tests; no Weston or browser renderer.
- Independent durable authorities on the same M2 Max Mac, with disposable stores
  initially empty. Live app state and signed release pointers were not benchmark fixtures.
- Three alternating repetitions. Stores retain history and writer receipts between
  repetitions; each new client gets a new writer identity. These are successive
  trials, not independent cold-store trials. Clock/governor/thermal state was not pinned.

Only identical host-side probe/telemetry code was added to each version. Runtime,
policy, durability, receipts and codecs were left as shipped. Native binary hashes
and the fixture metadata are in [metadata.json](data/canvas-adapter-frame-2026-10-19/metadata.json).

## End-to-end clients on the frame

Two separate real Canvas/WebSocket clients run in one measured Linux process on
the frame: writer and observer. Each mounts the real UI/schema, and the writer
invokes its count control. Updates travel to the central durable authority and
back. A common local monotonic clock measures input-to-durable-confirmation and
input-to-peer-observation without clock synchronization assumptions. No browser,
physical finger or GPU presentation is included in this part.

The probe uses nonblocking sockets with one millisecond polling pauses per peer;
small latency differences below that polling scale are not meaningful. It waits
for both confirmation and peer observation before issuing the next assignment,
paced at the requested maximum rate. Thus the 30 Hz run measures a stop-and-wait
limit, not an open-loop/coalesced drag stream. There are 20 warm-up edits followed
by 180 edits at 10 Hz or 300 at 30 Hz. CPU/RSS are exact child `wait4` usage and
include mount, warm-up and waiting; CPU per operation divides by all edits.

Numbers are **medians of three per-run statistics**, including median per-run p95,
not a pooled p95. Full per-run values and ranges are retained in
[headless.jsonl](data/canvas-adapter-frame-2026-10-19/headless.jsonl) and
[summary.json](data/canvas-adapter-frame-2026-10-19/summary.json).

| Requested 10 Hz | Pre-Merkle | Merkle |
| --- | ---: | ---: |
| Durable ACK p50 / ms | 49.40 | 63.20 |
| Durable ACK p95 / ms | 123.14 | 124.79 |
| Peer observation p50 / ms | 48.46 | 63.79 |
| Peer observation p95 / ms | 122.17 | 125.14 |
| Two-client CPU / % of one core | 40.3 | 28.9 |
| CPU ms / operation, including mount | 43.40 | 30.91 |
| Observer received payload / bytes per update | 271.8 | 12437.0 |
| Actual updates / second | 9.56 | 9.53 |

| Requested 30 Hz | Pre-Merkle | Merkle |
| --- | ---: | ---: |
| Actual updates / second | 16.56 | 12.83 |
| Peer observation p95 / ms | 127.40 | 141.62 |
| Two-client CPU / % of one core | 57.0 | 38.3 |
| CPU ms / operation, including mount | 35.82 | 30.15 |
| Observer received payload / bytes per update | 335.9 | 14551.9 |

The 10 Hz rates are closely matched, making application CPU comparison useful.
At 30 Hz neither backend sustains the requested rate; CPU percent alone would
hide the different throughput, so per-operation CPU and achieved rates are also
reported. This small four-field app does not exercise a 4,096-leaf hierarchy.
The separate [materialized tree benchmark](merkle-subtree-validation.md) measures
that algorithmic case and excludes durability/network/rendering.

## Actual native rendering on the frame

One version at a time runs through the existing systemd graphics service with an
instrumented binary. A separate frame process drives 10 warm-up plus 150 real
control assignments at 5 Hz. This lower rate keeps both versions paced equally.
The native process must finish at the expected count (59), confirmed with no
pending commands. Its local timestamps measure packet receipt to processing of
the post-page-flip presentation notification, including queueing, composition,
GPU submission, flip wait and notification delivery. This is an **upper bound on
receipt-to-present**, not physical touch-to-photon latency. The first 12 presented
samples are discarded. There is no framebuffer capture stall during timed runs.

| Native 3D Lab, matched 5 Hz | Pre-Merkle | Merkle |
| --- | ---: | ---: |
| Receipt-to-presentation feedback p50 / ms | 51.80 | 44.15 |
| Receipt-to-presentation feedback p95 / ms | 68.12 | 58.35 |
| Receipt-to-presentation feedback p99 / ms | 78.53 | 67.48 |
| Native renderer CPU / % of one core | 51.4 | 36.3 |
| Whole frame CPU / % of one core | 121.3 | 133.2 |
| Kernel page flips / second | 55.00 | 52.61 |
| Native received payload / bytes per update | 255.3 | 17942.0 |
| Native WebSocket traffic, both directions / bytes per update | 377.4 | 18134.0 |
| Actual input updates / second | 5.01 | 5.00 |

CPU uses 100% for one fully occupied core; the four-core frame can reach 400%.
Native process CPU includes all its threads. Whole-frame CPU additionally includes
the co-resident writer, loopback relay, Tailscale, kernel work and normal daemons;
it is the benchmark setup's total, not a production-only CPU estimate. Lower
application CPU therefore must not be presented as lower total device CPU.
The measurements do not attribute the remaining CPU to a particular process.

The renderer is unchanged, and these runs do not establish a new locked-refresh
or GPU performance claim. Flip rates reflect the complete benchmark load. Raw
presentation samples, CPU summaries, convergence checks and kernel flip reports
are in [native.jsonl](data/canvas-adapter-frame-2026-10-19/native.jsonl).

## Bandwidth and a confirmed encoding issue

Byte counts are actual binary application payloads and encoded WebSocket bytes.
Headless steady-state counters exclude both initial mounts. Native counters
exclude the native mount but include the effect of admitting its temporary writer
and all ten warm-up edits. All counts exclude TCP/IP,
WireGuard and link-layer overhead. Each version uses the same private tailnet
path through loopback relays; these controlled paths are not the public HTTPS
production route. Native counters come from a transparent stream proxy and
include all native-peer payloads during the 160-edit interval, including ACKs.

Merkle receipts are retained per writer, unlike the earlier client-CRDT protocol.
The app-wide subscription currently carries those metadata changes as well as
field data. Across the 10 Hz client repetitions, received Merkle bytes per edit
rise from 8,432 to 16,739 as writer metadata accumulates; baseline bytes stay near
266–276. This documents the current adapter and its envelopes, not an inherent
requirement that Merkle replication be larger.

A separate [wire-size diagnostic](../../crates/otrios-core/examples/canvas_wire_cost.rs)
confirms that `Vec<u8>` is serialized as an integer vector. FlexBuffers widens a
vector's elements with its length, so two nested envelopes amplify larger opaque
frames. The diagnostic uses the real deployed codecs versus explicit Blob
serialization, with no deployed protocol change:

| Raw opaque frame | Current outer message | Blob-based diagnostic outer message |
| ---: | ---: | ---: |
| 128 B | 197 B | 197 B |
| 256 B | 1,158 B | 348 B |
| 1,024 B | 4,230 B | 1,116 B |
| 2,048 B | 8,326 B | 2,140 B |
| 4,096 B | 16,518 B | 4,188 B |

[Raw size output](data/canvas-adapter-frame-2026-10-19/wire-size.jsonl).
This proves a wrapping-size opportunity, not measured CPU/latency gains from a
Blob deployment. Removing that roughly fourfold penalty still leaves real
Merkle path/receipt overhead. The next bounded optimization should use actual
Blob fields with compatible decoding/version handling, then narrow receipt
traffic while preserving atomic durable acknowledgments, and rerun these tests.

## Reproduction and restoration

1. Archive the dated milestone and apply
   `scripts/prepare-canvas-adapter-benchmark.py BASELINE_FOLDER`. It copies only
   the same host probe and bounded presentation recorder; it does not change core code.
2. Build each `canvas-adapter-bench`, `dashboard-direct` and its matching
   `dashboard-server` with pinned Rust/release settings. After swapping source in
   a shared build directory, invalidate source timestamps or use separate target
   directories; do not trust artifacts cached against the other backend's interfaces.
3. Use separate durable stores, matching app packages and identical catalogs.
   `serve-canvas-benchmark-authorities.py` creates the paired authorities and
   transparent counters; `relay-canvas-frame-benchmark.py` supplies frame loopback
   relays to the temporary tailnet-only upstream. Connect frame loopback ports
   18901/18902 to the respective authorities; ports
   18911/18912 use an additional counting proxy for the native peer. Temporary
   routes are tailnet-only. Do not point probes at the live room.
4. Place each frame build/catalog/font under `baseline/` and `merkle/` beside
   `run-canvas-adapter-headless-benchmark.py` and run it. The script emits CPU,
   bandwidth and latency JSON for all 12 client series.
5. `run-canvas-frame-render-benchmark.py` runs as root on this configured frame;
   `control-canvas-render-benchmark.py` acknowledges JSON start/end markers with
   `go` on stdin and
   snapshots native-peer wire counters. It uses the accepted calibration, a
   bounded service override, bounded telemetry and an automatic restoration guard.
6. Run `cargo run -p otrios-core --features canvas --release --locked --example canvas_wire_cost`
   for the encoding diagnostic. No Blob protocol change is deployed by this example.

Required workspace checks and strict all-feature/all-target Clippy passed. Two
independent counter tests verify masked/extended frames, arbitrary TCP chunk
boundaries and oversized-frame rejection. CLI help and Python syntax checks pass. Both
ARMv7 hosts compiled, the actual clients converged, all six native trials checked
one renderer, and each finished at the expected state. Pilot samples affected by
socket timeout rounding were discarded before the final fresh-store runs.

The regular signed-release supervisor was restored and its Home mount confirmed
with one native renderer, no pending commands and inactive Weston/browser services.
The benchmark override, frame binaries, relays, disposable authority processes
and temporary tailnet ports were removed. The original HTTPS serve configuration
was compared exactly and retained. Main remains at the dashboard baseline
(`9de7618`, whose tree matches `feat/dashboard`); the live Merkle deployment stays
on its existing feature branch, as requested. No benchmark state was imported
into the live room.
