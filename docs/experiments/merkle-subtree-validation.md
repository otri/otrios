<!-- SPDX-License-Identifier: Apache-2.0 -->
# First materialized Merkle/subtree increment — 2026-10-19

The pre-Merkle Canvas milestone is tagged
[`2026-10-19-canvas-apps-milestone`](https://github.com/otri/otrios/releases/tag/2026-10-19-canvas-apps-milestone)
at `8dd9bc9`. [RFC 0005](../rfcs/0005-merkle-subtree-prototype.md) records the new
bounded library and exact binary protocol. Runtime code measured here is
`fdedd29`; the diagnostic harness is included with this report. No new dependency
was added. The live dashboard still uses scoped Automerge documents.

## Method

Rust 1.90.0 release builds; real immutable trees of 64, 512 and 4,096 one-KiB
registers. Stable base-8 keys create four ancestor levels. Initial payloads are
unique; the initial transfer materializes a real receiver. Repeated edits target
one leaf, using the same actor and advancing its causal counter. Each size gets
100 warm-up edits and 1,000 timed edits. SHA-256 covers all metadata and canonical
bytes, not a synthetic array of cached sibling digests. Allocation/copying of
new registers/nodes is timed; fixture setup and preparation of the caller's
command/payload are outside the timed edit. Allocations were not counted.

The combined measurement includes edit, publish/offer, encode, parse/hash/validate,
receiver materialization, root-equality check and ACK, all in one process. Stage
percentiles are measured separately; the combined p95 is measured directly,
not a sum of p95s. The concurrent-merge stage joins two real concurrent versions
of that leaf, retaining both assignments, with untouched branches shared.

The small tree has fewer populated sibling slots on the same four-level path;
the larger trees fill more of each bounded ancestor. Five hashes per edit stays
constant; ancestor bytes increase with direct fanout, not total history size.
Full-transfer bytes below include all records. Delta bytes include scope/cursor
and canonical records, with no compression. This is not durability, WAN,
physical touch, selector invalidation or display latency.

## Rockchip photo frame

RK3126 Cortex-A7 / armv7, Linux `6.18.57-spd-kiosk`, clocks left unchanged, existing direct
GLES dashboard service left active. Target `armv7-unknown-linux-gnueabihf`, linked
with the existing Linux cross-builder. Single measured series per size, no CPU
isolation or hard realtime guarantee. cpufreq governor/frequency telemetry was
not exposed at the usual cpu0 sysfs paths. Scheduling outliers appear in p99.

| Leaves / nodes | Full bytes | Edit hashes / bytes hashed | Delta bytes |
| --- | ---: | ---: | ---: |
| 64 / 75 | 76,254 | 5 / 1,951 | 2,093 |
| 512 / 586 | 608,730 | 5 / 2,231 | 2,373 |
| 4,096 / 4,681 | 4,869,014 | 5 / 2,511 | 2,653 |

Microseconds, p50 / p95 / p99:

| Stage | 64 leaves | 512 leaves | 4,096 leaves |
| --- | --- | --- | --- |
| Edit and hash | 245.583 / 275.917 / 355.250 | 272.709 / 321.708 / 400.167 | 302.459 / 368.667 / 441.291 |
| Publish/offer/encode | 90.416 / 109.667 / 151.667 | 109.375 / 133.292 / 175.291 | 125.708 / 153.125 / 246.751 |
| Decode/validate/materialize | 305.084 / 384.125 / 604.334 | 346.208 / 429.333 / 526.459 | 387.334 / 484.750 / 595.000 |
| Combined through ACK | 663.834 / 800.334 / 1,248.625 | 753.667 / 905.918 / 1,227.918 | 844.667 / 1,008.292 / 1,395.334 |
| Concurrent leaf join | 364.000 / 432.541 / 493.792 | 403.667 / 474.833 / 543.667 | 442.750 / 517.709 / 657.418 |

The bounded materialized edit meets RFC 0002's sub-millisecond p95 target in
this experiment: **0.369 ms** with 4,096 leaves. The combined CPU-only cycle is
**1.008 ms p95**, not a sub-millisecond end-to-end synchronization claim. Its
2,653-byte delta replaces a roughly 4.9 MB initial subtree transfer for this edit.
No renderer FPS claim follows from this CPU experiment. The dashboard remained
one active native instance; the temporary benchmark binary was removed.

## Mac native and V8 WebAssembly

Apple M2 Max, macOS 27.0.1, normal development load; Rust release and Node
v24.19.0 V8.
The opt-in `MerkleProbe` adapter invokes the same covered core implementation
from actual WebAssembly. It is not a browser screenshot, Safari/iPad result or
live deployment. Its timings also include the Wasm call/returned report. Both
paths assert receiver/authority root equality on every iteration.

Combined CPU cycle in microseconds, p50 / p95 / p99:

| Leaves | Mac native | V8 Wasm |
| --- | --- | --- |
| 64 | 11.416 / 19.917 / 29.584 | 21.292 / 27.334 / 35.042 |
| 512 | 9.958 / 13.791 / 21.375 | 21.250 / 25.833 / 31.583 |
| 4,096 | 8.917 / 11.083 / 18.417 | 22.667 / 28.125 / 35.250 |

These are individual series with other development work active, not statistically
controlled native/Wasm speedup comparisons. Byte counts and five-node work match
across native/frame/Wasm; small timing differences should not be overinterpreted.

## Correctness and process-boundary evidence

- Deterministic independent reconstruction, old pinned roots and actual unchanged
  subtree allocation reuse; two sibling edits rebuild their shared ancestors once.
- Causal join order/idempotence, seeded five-writer schedules (`0xfeedcafe`),
  sibling edits, same-leaf conflicts, deletion/write conflicts, and metadata-only
  changes with equal displayed bytes. Partial edited replicas join back without
  replacing unseen siblings.
- Exact scope grants, no private sibling canary bytes, missing-slot subscriptions,
  unchanged scopes yielding no offer, and one-flight slow-reader coalescing.
- Two competing publishers with the same expected root produce exactly one
  successful commit. Rejected updates expose no partial state.
- Exact replay/ACK, stale epoch/lease/path, wrong baseline, sequence gap, explicit
  full resync and cancellation. Malformed/truncated/oversized/extra data and DAG
  expansion/depth are rejected before exposing a new root.
- A parent plus **two real child receiver processes** exchange canonical frames
  over loopback TCP, each on its own subtree. Roots agree after initial and two
  incremental updates. The unrelated child's stream stays idle for sibling edits.
  This ran on macOS and Linux aarch64; it is not shared-memory IPC evidence.
- Required workspace checks pass with Merkle enabled; strict all-feature Clippy
  passes. Existing Canvas behavior/persistence/process tests remain enabled.

## Reproduction

```sh
cargo test -p otrios-core --features merkle --locked
cargo run -p otrios-core --features merkle --example merkle_tree_cost --release --locked
bash scripts/check.sh
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

For V8 Wasm, with pinned wasm-bindgen-cli 0.2.129 and Node available:

```sh
cargo build -p otrios-web-host --features merkle-probe --target wasm32-unknown-unknown --release --locked
wasm-bindgen --target nodejs --out-name otrios_merkle --out-dir target/merkle-probe target/wasm32-unknown-unknown/release/otrios_web_host.wasm
cp target/merkle-probe/otrios_merkle.js target/merkle-probe/otrios_merkle.cjs
node scripts/benchmark-merkle-wasm.mjs target/merkle-probe
```

For the frame, build the same native example with the existing armhf linker,
copy only that disposable binary, run it while the dashboard stays active,
then remove it. Do not switch the renderer, display mode or governor. Host CI
compiles the optional Wasm adapter and exercises the real-process library tests;
local benchmark results above are not presented as CI performance thresholds.

Next cutover: a policy-qualified, durable Canvas state adapter, pending-intent
rebase, and selector dependency invalidation. Sequence/text CRDTs, tombstone GC,
wide-map radix layout, host-wide memory budgets, authenticated subtree endpoints
and browser/frame app migration remain explicit follow-up work.
