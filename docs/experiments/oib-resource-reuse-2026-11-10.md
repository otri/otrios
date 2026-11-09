<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB resource reuse — 2026-11-10

This follows [incremental presentation](oib-incremental-presentation-2026-11-10.md).
Implementation and focused checks are complete; deployment evidence follows below.

The normalized binding now retains its resolved baseline graph and canonical
library byte count under its exact binding leaf and admitted UI identity. Current
property addresses skip retired-baseline resolution. Property and source changes
continue to run raw causal, schema, closure, presence and descriptor validation;
immutable resident expansion is shared through the existing exact source cache.
Authority journal sync remains required before durable publication.

Retained rendering caches exact normalized primitives and groups consecutive runs
into immutable resources of at most 16 draws. Changed labels reshape/tessellate
only their affected primitive and replace its resource when ordered group
membership stays stable; insertions/removals may regroup neighboring resources.
Unchanged groups retain geometry and GPU ownership. A compatibility mesh remains available to exporters.
Native and Web hosts preserve painter order and move per-frame clips/paints
separately. Resources are namespaced by layer and composer reset epoch, and cleared
on font/mount/privacy/graphics resets. Existing scene, vertex and glyph limits
remain aggregate limits, with at most 32 resources per layer and a conservative
16 MiB combined UI bound. Document, reflow and oversized candidates use their
existing compatible path. Native UI and document slot ownership is exclusive.

There is no new dependency, state wire format, executable ABI or worker pool.

## Focused validation before deployment

- Immutable import cache tests: **13 passed** (4 new plus 9 preceding cache checks).
  Existing library-adoption integration tests: **10 passed**, including replay,
  rollback, held receipts, malformed witnesses and reset after adoption.
- Dashboard/layout/render tests: **75 passed**. Designer tests: **49 passed**,
  one existing timing diagnostic ignored. Chunk tests cover primitive/resource
  reuse, exact compatibility reconstruction, order, pans, color, font invalidation,
  held snapshots, visibility and aggregate fallback.
- Native resource-owner tests: **3 passed** using a graphics-command test double,
  not a GPU or device. Composer capacity/budget tests: **3 passed**. Existing
  presentation integration test with namespace/reset assertions: **1 passed**.
- Web Rust check passed. GPU, transfer and worker tests: **37 passed**; navigation
  timing tests: **5 passed**; signed release/publisher tests: **38 passed**.
- Focused core Clippy with warnings denied, formatting and diff checks passed.
  The throughput example compiled with the new single-label resource probe.

The previously accepted task's builder maintenance completed before these builds:
fixed disposable caches and 14 inactive local releases were removed, retaining the
active release, keys/config/state/source/toolchains and historical rollback files.
A compatible active code rollback was retained for this development pass. The same
watcher configuration was restored, then paused again for candidate staging.

A focused live HTTP check caught an initial 404 for the new module: publishing and
signature admission included it, but the authority's explicit MIME allowlist did
not. The allowlist was corrected and a focused server admission test passed before
restaging. The native timing probe stopped before editing because newer external
edits had changed Accent from gold to orange and Width to 420; its restoration
logic now uses the current admitted palette color. These were acceptance findings,
not successful trials, and no app state was reset.

## Signed deployment and live acceptance

Implementation commits `b77a226` and `f613aae` built signed release
`50af827f924ed27ebc47ed2a86dd4f3c9f06ee7c190d4c726e943479c2f4d724`.
The server-only route correction preserves that release content hash; the matching
rebuilt authority helper was transferred through the existing pinned SSH/checksum
workflow. Frame authority/native activation and existing service/readiness checks
passed. Public health and the actual new module HTTP response passed, including
JavaScript MIME and the exact signed SHA-256. The rebuilt Wasm export check passed.
No state reset, pixel export, new service, protocol or trust root change occurred.
State protocol remains `otrios-canvas-sync-v30`, with the v1 opening envelope.

Before the timing trial, newer external edits had advanced generation 28→37,
changed Display Width 400→420 and Accent gold→orange. The probe now derives and
restores the current palette color. It passed blue Apply and orange restoration
under normal and traced environments. Profiling variables were removed from both
running processes, task drop-ins/tmpfs buffers removed, and Monitoring resumed.
The repeated normal-environment trial also passed, ending at confirmed/presented
generation **43**, Display selected, orange restored and Width 420 retained.

## Measurements

| Profiling-off software trial, ms | Blue Apply | Restore original |
| --- | ---: | ---: |
| Preceding release (Width 400, gold) | 397.5 | 288.9 |
| Current release (Width 420, orange) | 310.4 | 258.4 |
| Current release, normal environment restored | 328.6 | 234.3 |

These include the intentional 80 ms touch hold, readiness publication and 5 ms
polling. The changed width/restored color and small ordered samples mean this is
not a controlled distribution or photon-latency comparison.

Actual native presentation in the separate traced pair completed in **339.1 /
243.9 ms**, or **258.7 / 163.5 ms after touch release**. The preceding traced pair
was 356.9 / 277.4 ms. Trace/control differences do not measure profiling overhead.

| Traced segment, ms | Preceding blue/restore | Current blue/restore |
| --- | ---: | ---: |
| Offer → authority response arrival | 136.5 / 87.3 | 138.4 / 68.2 |
| Native receive processing | 73.3 / 48.5 | 57.0 / 29.2 |
| Final CPU compose | 11.8 / 11.0 | 6.7 / 7.1 |
| Final renderer preparation/upload | 2.5 / 2.4 | 1.0 / 1.0 |
| Final GL submit | 10.5 / 9.4 | 10.1 / 10.1 |

Per Apply, native and authority import resolution, three export-contract checks
and one library-graph validation dropped to **zero**. Metadata expansion dropped
from 3→2 on native and 1→0 on authority. Native designer rebuild inclusive time
was **7.65 / 7.44 ms**, previously 19.21 / 19.37. Exclusive validation fell to
**7.81 / 7.89 ms** on native and **8.31 / 8.92 ms** on authority, while preserving
raw validation and durable publication.

The final presentation uploaded **45,984 bytes in one resource upload**, previously
202,032 bytes. This counter covers that presentation, not the entire interaction:
an earlier local/cold presentation still uploaded 202,032 bytes in five calls.
Total mesh-paint spans remained **12.49 / 14.43 ms**, versus 12.33 / 11.35 before;
three CPU composition spans totaled **27.32 / 32.53 ms**, versus 27.72 / 28.01.
The final frame improved, but whole-interaction CPU mesh painting was not
eliminated. Inspector node identity/status changes still rebuild earlier groups.

Remaining serial costs include durable sync **22.45 / 21.25 ms**, cold binding
normalization **47.44 ms**, and two cold authority boundary encodes **24.10 ms**
(native encodes **23.46 ms**). Inclusive profiler spans overlap their children and
must not be added together. The next concrete targets are earlier inspector
identity churn and cold boundary encoding/normalization; fsync remains required.

## ARM isolated workload

The same optimized 128-node rounded layout resolves, constructs and validates a
scene at **206,068 nodes/sec** (0.621 ms). Retained recolor is **0.730 ms**. These
stages exclude OIB/state IO and GL/presentation.

The new mixed-shape single-label workload replaces **1 of 8 resources**, uploading
**43,008 source vertex bytes** for that changed group. CPU composition alternates
that existing label in **4.034 ms** with all 127 leaves visible, or **1.327 ms** in
the 16-visible-leaf clipped case. No preceding single-label benchmark exists, so
these are measurements, not a measured speedup. Group membership and exact
painter-order/compatibility reconstruction are separately tested.

## Evidence and limits

Actual focused commands used the configured toolchain wrapper, including:

```sh
python3 /tmp/otrios-d2-cargo.py test -p otrios-dashboard --features canvas --test ui_gpu --locked --offline
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --lib canvas::presentation::tests --locked --offline
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --test canvas shared_presentation_preserves --locked --offline
python3 /tmp/otrios-d2-cargo.py clippy -p otrios-core --features canvas --lib --locked --offline -- -D warnings
python3 /tmp/otrios-d2-cargo.py check -p otrios-dashboard --features canvas --example layout_throughput --locked --offline
python3 /tmp/otrios-d2-cargo.py test -p otrios-dashboard --features server,canvas,host-sync --bin dashboard-server presentation_transfer_has_explicit_public_mime_admission --locked --offline
```

The established stage-only builder compiled signed Mac/Wasm/ARM artifacts;
`deploy-dashboard-frame.py` and `check-frame-release.py` performed deployment and
live readiness. The full suite and browser interaction campaign were deferred;
Web acceptance here is unit/host validation plus live signed-module delivery.
Wasm retained the existing three feature-specific collection-transfer warnings.

Raw [control](data/oib-resource-reuse-2026-11-10/control-apply.json),
[restored-environment control](data/oib-resource-reuse-2026-11-10/control-restored-apply.json),
[traced trial](data/oib-resource-reuse-2026-11-10/traced-apply.json),
[analysis](data/oib-resource-reuse-2026-11-10/analysis.json),
[readiness](data/oib-resource-reuse-2026-11-10/readiness.json),
[public delivery](data/oib-resource-reuse-2026-11-10/public-health.json), and
[environment restoration](data/oib-resource-reuse-2026-11-10/environment-restored.json)
are retained with the bounded native/authority JSONL. The existing
[analyzer](data/oib-hotspots-2026-11-09/analyze.py) accepts this directory and its
traced-trial path. [Native probe](data/oib-resource-reuse-2026-11-10/native-probe.py),
[metadata](data/oib-resource-reuse-2026-11-10/metadata.json), and
[ARM microbenchmark](data/oib-resource-reuse-2026-11-10/layout-arm.jsonl) record the
remaining evidence. The benchmark was SHA-256 checked after transfer, run with
`--nodes 128 --iterations 200`, then removed from the Frame. The automatic builder
is restored to the finished source pin after promotion; no new accepted-work
maintenance is performed at this development closeout.
