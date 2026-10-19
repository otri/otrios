<!-- SPDX-License-Identifier: Apache-2.0 -->
# Coherent covered Merkle views on the frame — 2026-10-19

The generic runtime can now publish several authorized stable-key paths as one
coherent view. Unselected fields, other writers' receipts and ancestor payloads
are absent. This is the building block for admitted UI coverage in
[RFC 0010](../rfcs/0010-runtime-foundation.md); live Canvas still uses app-root
leases. No new signed client release was published by this experiment.

## Method

Frozen source `8ce515e5fb8d2b5ed39291b20673c967e6b75ae6`, Rust 1.90.0,
optimized ARMv7 on the RK3126/Cortex-A7 frame, Linux 6.18.57-spd-kiosk. One
executable compares full roots, all 64 fields plus one writer's receipt, and
eight fields plus that receipt. The synthetic tree has 64 eight-byte field
values and 2/128/1,024 128-byte receipts in Canvas-style 32-way buckets. Leaf
schemas bind their paths, preventing unrealistic deduplication of all default
records. Values and causal metadata use the existing canonical Merkle encoding.

Each trial has 100 warm-up and 500 measured edits. Three repetitions rotate the
three view modes. Scenarios change a selected field and its receipt atomically,
another writer's receipt, or an unselected field. There are 81 trials total.
An earlier engineering run with identical schemas/defaults across slots is
excluded; it understated initial full-snapshot bytes through deduplication.

This measures in-process edit/offer/apply/ACK on the frame. Offer includes
projection, encoding and allocations; receive includes materialization and ACK.
It excludes network, durable storage, Canvas schema validation, rendering and
physical touch latency. CPU frequency, thermals and scheduling were not pinned.
The existing direct service remained active with MainPID 1144 before/after; no
second renderer was started. Temporary probes were removed. No live state or
deployment configuration changed.

## Results

Selected-field + own-receipt updates with 1,024 initial receipts. Values are
medians of three run-level measurements, not pooled latency percentiles.

| View | Initial bytes | Delta bytes/update | Offer p50 µs | Receive p50 µs | Retained canonical bytes* |
| --- | ---: | ---: | ---: | ---: | ---: |
| Full app root | 302,941 | 4,779 | 164.79 | 854.87 | 298,351 |
| All fields + own receipt | 11,324 | 2,299 | 191.04 | 450.04 | 10,950 |
| Eight fields + own receipt | 2,128 | 1,299 | 259.58 | 291.96 | 1,982 |

*Canonical bytes describe the materialized view, not heap RSS. Shared nodes,
container overhead, acknowledged/issued snapshots and projection scratch have
separate ownership and costs.

Changes to another writer's receipt emitted zero offers/bytes in both covered
modes; the full root emitted all 500 offers at 3,240 bytes/update. Changes to an
unselected field emitted zero offers in the eight-field mode; both full-field
modes correctly continued publishing it. Covered views are not a mechanism for
dropping selected values or a durable writer's own confirmation.

Projection has a real cost. With only two receipts, selected updates took
91.58/441.29 µs offer/receive for full roots versus 172.08/442.46 µs for own-receipt
coverage. Eight-field coverage took 238.87/287.29 µs. The current implementation
rebuilds filtered ancestors when the source root changes, including on unrelated
changes. Do not describe this as a universal authority CPU improvement. The
benefits demonstrated here are bounded replication scope, lower retained view
size, fewer bytes and suppressed unrelated deliveries. Dependency-hash checking
and reusable projection scratch are possible later optimizations.

## Acceptance and reproduction

The required repository checks and all-feature lint pass; the new real-process
test confirms two selected paths advance coherently without private sibling or
ancestor payloads. Unit tests cover exact grants, absent targets, corruption,
slow-reader ACK, explicit resync, cancellation and canonical coverage ordering.
This is not browser/native Canvas deployment acceptance.

Build/run `merkle_coverage_cost` with feature `merkle`, release profile and the
pinned toolchain. The executable prints every trial as JSON. Use the ARMv7 target
for frame runs. [Raw frame trials](data/merkle-coverage-2026-10-19/frame-results.jsonl)
and [source/binary/method metadata](data/merkle-coverage-2026-10-19/metadata.json)
are committed. No whole-process memory or end-to-end latency claim is made.
