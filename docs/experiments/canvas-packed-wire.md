<!-- SPDX-License-Identifier: Apache-2.0 -->
# Packed Canvas wire costs on the frame — 2026-10-20

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/canvas-packed-wire.md) remain available at the pinned historical revision.

Frozen codec/diagnostics source: `157ca5b00fa14c28ecb95f4895a1d061cee8b99d`.
This microbenchmark selects a default; it is not live or end-to-end acceptance.
Normal sync-v4 replication stays **raw packed**. Explicit bounded LZ4 remains
available for a future bulk transfer adapter. Canonical Merkle bytes do not change.

Median of nine measurements per case (three processes × three repetitions):

| Canonical frame case | Flex opaque bytes | Raw packed bytes | Optional LZ4 bytes | Flex encode µs | Packed owned encode µs | Packed reusable encode µs | Packed decode µs | LZ4-only encode µs | LZ4 + packed decode µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Scalar delta, 837 bytes | 930 | 871 | 692 | 36.045 | 6.887 | 1.851 | 4.945 | 114.167 | 85.945 |
| Scalar mount frame, 1,145 bytes | 1,238 | 1,179 | 869 | 40.167 | 7.489 | 2.390 | 5.712 | 148.220 | 112.212 |
| Series mount frame, 17,299 bytes | 17,392 | 17,333 | 9,037 | 74.838 | 39.527 | 31.903 | 37.426 | 1,414.878 | 1,307.171 |

The scalar delta removes 59 envelope bytes (6.3%) without changing its canonical
nodes. Packed owned encoding is roughly 5× cheaper in this small fixture. The
actual runtime currently returns independently owned Vec packets; reusable-buffer
numbers are an available builder cost, not a claim that the deployed path reuses
all allocations. Both inner and outer envelopes are included in encode/decode.
LZ4 encode is additional to raw packet assembly, with its own bounded scratch.

Compression saves ~21% on the small delta, but its ~0.1 ms encode cost greatly
exceeds raw assembly. The series frame nearly halves in size but adds ~1.3 ms
receiver work on this Cortex-A7, largely from compression/integrity processing.
This does not establish a universal bandwidth crossover: slow links or occasional
large mounts may benefit. It does justify avoiding mandatory per-edit compression
without an end-to-end win on the deployed path.

For a conservative old decode comparison, the fixture also reads both old Flex
maps/blobs and copies the final frame **without** the old recursive validation
or full serde materialization. Those accessor medians are 6.367 / 6.552 / 23.187 µs.
The larger raw packed decode (37.426 µs) is slower than that cheaper accessor
baseline. Do not represent this as an across-the-board decode win, or compare it
to full old decode. Flex encode similarly excludes the old recursive post-encode
validator, so its time is a lower bound on that deployed encoder's work.

The canonical scalar delta and scalar/series initial frames come from real
Document/session/ACK operations. All three codecs see the **same frame bytes** in
each repetition, wrapped as Update/Sync to isolate envelope cost. “Mount frame”
here describes the frame payload, not a complete app Mounted transfer (which also
contains UI/code). New authority nonces change some hashes/compression lengths
between repetitions; report medians, not one exact compressed byte contract.

Rust 1.90 release ARMv7, RK3126 Cortex-A7, live renderer left running, clocks and
thermals unpinned. Each timing performs 20 warm-up and 300 measured calls in the
same fixed order. No tree generation, network, fsync or GPU submission is timed.
Temporary frame binaries were removed. Results remain useful directionally;
complete-client delay/convergence/CPU measurements are a separate gate.

[Raw results](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/results.jsonl) and
[archive, binary and harness hashes](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/metadata.json).
The implementation decision and exact packet contract are in [RFC 0011](../rfcs/0011-packed-canvas-transport.md).

## Complete-client comparison and deployment

Twenty-four fresh-authority trials compare frozen foundation `c15221a` with
`157ca5b`: three repetitions × two input rates × two binaries × direct or delayed
transport. Same signed catalog/UI hashes, 20 warm-up and 120 open-loop numeric
assignments, final release flush, two real clients in one frame process. The live
renderer remains running. Delay adds 40 ms per relay chunk in **each direction**,
not a precisely simulated 80 ms packet RTT. Values are latest assignments;
coalescing deliberately omits intermediate inputs under backpressure.

Medians of three runs (foundation → packed):

| Transport / input rate | Distinct values observed | Observed p95 ms | Final ACK ms | CPU, % one core | Total WebSocket bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Direct / 10 Hz | 120 → 120 | 105.860 → 102.079 | 105.022 → 30.851 | 19.495 → 18.280 | 399,360 → 353,880 |
| Direct / 120 Hz | 25 → 26 | 114.358 → 104.399 | 43.634 → 31.662 | 33.119 → 28.422 | 83,200 → 76,767 |
| Delayed / 10 Hz | 71 → 88 | 289.038 → 253.914 | 228.271 → 254.846 | 16.378 → 16.428 | 236,288 → 259,512 |
| Delayed / 120 Hz | 7 → 8 | 230.677 → 186.178 | 201.394 → 173.396 | 20.986 → 21.957 | 23,296 → 23,592 |

All 24 runs converge to final value 120. At 10 Hz direct, 11.4% fewer wire bytes
carry the same 120 values. Delayed runs deliver more intermediate values, hence
sometimes **more total traffic**; median bytes per distinct value fall from 3,328
to 2,949. This normalization includes writer/observer request/ACK traffic, not
only useful value bytes. Do not describe it as payload compression. The delayed
10 Hz final ACK regresses in the medians while observation latency improves;
this is mixed three-run timing evidence, not a guaranteed latency speedup.

CPU includes both client roles, mount, warm-up and waits, not the Mac authority,
relay/SSH or frame GPU. Process peak RSS is 7,988 KiB in every trial; that coarse
unchanged high-water mark does not establish zero allocation. Clocks/thermals and
network are unpinned. Both temporary Tailscale routes are restored exactly and
frame experiment files removed after each matrix. Raw
[direct](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/direct-results.jsonl) /
[delayed](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/delayed-results.jsonl) records and their
metadata identify frozen sources, binaries, signed releases and scope.

### Browser and frame qualification

- Required workspace check passes: formatting, strict lint, Rust core/Merkle/
  process/server suites, Python release/direct checks. Six direct native-worker
  tests pass; 18 focused Node worker/session/catalog tests pass. Signed build runs
  the complete 40 Node host/render/cache checks successfully.
- Staged browser: Component Playground slider and percentage agree; Layout Lab
  Toggle updates; Input Lab primitive keyboard types `abc`, confirmation clears
  diagnostics, reload retains text but drops local focus. 3D presentation advances
  from 240 to 5,182 frames under state load, reporting ~120 FPS in this browser.
  This browser rate is **not** a frame GPU result. Monitoring requires sign-in and
  Back returns to public demos.
- Event-driven upgrade from staged release `51783651…` to corrected `45782530…`
  preserves route and confirmed Layout Lab values. The correction publishes
  diagnostic-only frames when a receipt changes the queue but not visible values.
- Public conformance passes 81 checks against the staged authority and live host,
  including authenticated Monitoring cookie revocation and private artifact/admin
  isolation. Private mount/policy coverage also passes in real server tests.
- Live signed release: `4578253065f32cc7e2fe8f04f022333756854fab6f46f74eced142cd9fcd5030`.
  Source pin `67d2d586aee1f2642c60139539491bbb15edfd64`; server compiled at `157ca5b`,
  with all Cargo/crates/apps/tools/scripts sources verified identical between the
  two. Only the browser receipt diagnostic changed. Direct trials used the first
  staged web hash, delayed trials the corrected one; native/server sources and
  admitted catalogs are identical. No change to canonical persisted bytes.
- Frame automatically upgrades to one direct GLES renderer (PID 5158), no Weston,
  RGB565/rotation 90/calibrated touch, Home, confirmed state and empty edit queue.
  Its resource report and browser both show one shared selected root, with no
  compression scratch allocated on this path. This is automated process/readiness
  acceptance; no new physical touch or color assessment is claimed.

The first activation rolled back when its unpublished source pin could not be
fetched. Health and the old release were verified restored, and confirmed durable
state was retained. The retry pushes/preflights the exact pin **before** stopping
or activating, then succeeds. Keep this source-availability check ahead of future
cutovers. Rollback never restores an old state backup over newly confirmed writes.

[Deployment attribution](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/deployment.json),
[frame readiness](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/frame-acceptance.json),
[staged resource report](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/staged-resources.json),
[staged 3D image](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/staged-browser.png) and
[live browser](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-packed-wire-2026-10-20/live-browser.png).
