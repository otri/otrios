<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB content invalidation and layout — 2026-11-10

This follows [OIB resource reuse](oib-resource-reuse-2026-11-10.md).

The dependency determines the work. Accent changes paint. A slider can change
its thumb position, fill and value label inside admitted bounds without changing
surrounding layout. Text that changes measured width, wrapping or flow, and shape
or image dimensions that affect placement, require layout. This pass does not add
an image primitive or intrinsic text measurement to the current explicit-frame
contract.

Apple's [`setNeedsLayout`](https://developer.apple.com/documentation/uikit/uiview/setneedslayout%28%29)
records pending layout and defers it to the next update cycle. The equivalent
principle here is to coalesce invalidation until presentation and keep input
validation independent when its dependencies have not changed.

## Implementation

Previously, each changed slider sample invalidated the mount. The next Move
requested a fresh scene to validate capture, resolving layout before renderer
capacity became available. Designer Tick also requested layout to synchronize
panel bounds, even when no local panel gesture needed it.

The mount now retains an unprojected scene under exact admitted Layout source
identity, viewport, order and state bindings. At presentation it can update
content through the same frame, primitive, semantics and responder evaluators as
full layout, then validate the resulting Scene. Position, fixed-bound text,
paint, radius and series content qualify. Changed width/height, visibility,
unknown parameters, slots and unsupported template structures fall back to full
layout. Static composition frames can be retained only with unchanged source,
viewport and order; flow is not recomputed for these content updates.

Captured app drags retain a separate immutable validation scene only when all
input dependencies remain stable. Changes to visibility, responsive bounds,
source, order, handlers, permissions, projection or modal/local input state revoke
that proof. A fresh Down always uses current geometry. Pending viewport and UI
activation still wait for gesture/confirmation barriers. Panel bounds synchronize
at presentation or when their local gesture requires them. This allows repeated
Moves and Ticks to coalesce while the renderer is busy.

Compatible inspector rows retain their typed control identities through palette,
Saving and confirmation updates. Geometry reuse is independent of semantic node
IDs: exact primitives at the same bounded painter ordinal can retain immutable
resources while current IDs, responders and semantics remain authoritative.
Changed geometry or font state rejects that reuse.

Repeated configuration of the same complete admitted UI retains its existing Arc,
preserving the exact binding cache key. New miss-reason spans distinguish empty,
leaf and admission misses. This fixes an identified cache-miss mechanism; earlier
traces did not establish which reason caused their cold miss. Compiling a valid
paint edit shares one exact canonical boundary encoding between Layout and Ui.
The exact-input check includes signed zero throughout float-bearing expressions
and fields, so equality cannot substitute noncanonical bytes.

No new dependency, unsafe code, state protocol, executable ABI or worker pool is
introduced. Runtime implementation remains in the LGPL core.

## Validation and deployment

Focused checks before staging passed:

- Dashboard/layout/render/input tests: **87 passed**, including 9 content-update
  tests and actual imported OIB Slider parity. An additional signed-zero raw
  binding cache regression passed after review tightened that shortcut.
- Designer tests: **54 passed**, one existing timing diagnostic ignored. The
  slider filter passed **7 checks**, including 100 Moves with interleaved Ticks,
  deferred viewport/library activation, visibility/responder cancellation and
  handler field rebinding after generic invalidation. The older input-router
  filter separately passed **9 checks**; those overlap the dashboard total.
- Binding/compiler cache checks: **18 passed**, one isolated global-profiler
  diagnostic ignored in the parallel group. The diagnostic passed separately
  with one thread, proving exactly one encode, one paint compilation and one
  shared-encoding event, with no decode. Exact artifact/runtime parity and
  signed-zero input-family checks also passed.
- Composer capacity tests: **3 passed**. Shared presentation integration:
  **1 passed**. Core and both changed benchmark/probe examples passed Clippy
  with warnings denied. Both examples compiled; formatting and diff checks passed.
  The rebuilt Wasm host export check passed (**1 test**).

Commands used the existing pinned toolchain wrapper, including:

```sh
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --lib dashboard:: --locked --offline
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --lib slider --locked --offline
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --lib canvas::presentation::tests --locked --offline
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --test canvas shared_presentation_preserves --locked --offline
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --lib cold_paint_compilation_encodes_identical --locked --offline -- --ignored --test-threads=1
python3 /tmp/otrios-d2-cargo.py clippy -p otrios-core --features canvas --lib --locked --offline -- -D warnings
python3 /tmp/otrios-d2-cargo.py clippy -p otrios-dashboard --features canvas,host-sync --example oib_instance_probe --example layout_throughput --locked --offline -- -D warnings
```

Full repository preflight and a browser qualification campaign were intentionally
not repeated; this uses focused core checks and existing signed deployment/live
acceptance. Native Apply trials and the separate ARM synthetic pointer-burst
probe measure different workloads.

After acceptance of the preceding pass, builder maintenance reviewed and removed
disposable caches and one inactive local release. The active signed code and
matching native/server helpers were retained as a compatible development rollback.
The existing builder configuration was restored after maintenance, then paused
for this candidate's stage/cutover. No app state was reset.

## Signed deployment and live acceptance

Implementation `e8c26fa8ac25639e8787ce35a22c6f8407ef3c9e` built signed release
`56307a589d7c7e8f1ecee83063e593d34b2bf4a039d6a733f28056602f632d68`.
The established stage-only builder compiled Mac tools, Wasm and ARM artifacts and
signed 14 apps. Signed activation passed on both authority and native Frame.
Public health and actual module delivery passed, including JavaScript MIME and
its exact manifest SHA-256. State protocol stays `otrios-canvas-sync-v30`; the v1
opening release-control envelope is unchanged. The Wasm build emitted three
existing feature-specific dead-code warnings in Chat/transfers; it completed.

Normal, traced and restored-normal native Accent trials all passed and restored
the admitted orange override. The final state was confirmed/presented generation
**49**, with Display selected. Profiling variables were verified absent from both
running services; task drop-ins and tmpfs trace buffers were removed. Monitoring
resumed. No state reset, pixel export, new service or trust-root change occurred.

## Slider coalescing on ARM

The checksum-verified updated `oib_instance_probe` ran against the live loopback
WebSocket authority and current signed catalog on armv7 Linux. It selected an
admitted visible numeric Drag responder by typed action, took capture, delivered
100 Moves with interleaved Ticks at virtual 8 ms sample intervals, and withheld
scene/composition queries until one final core composition.

| Work | Observed |
| --- | ---: |
| Move samples / Ticks | 100 / 100 |
| Full layout resolves between samples | 0 |
| Content updates between samples | 0 |
| Full layout resolves in final composition | 0 |
| Content updates in final composition | 1 |
| Final composition elapsed time | 3.422 ms |
| Changed geometry resources | 1 |
| Changed source vertex bytes | 36,784 |
| Dropped phase events | 0 |

The whole burst executed in **94.964 ms**; virtual sample time advances are not
wall-time waits. This is a separate ARM core/IPC probe, not physical evdev input,
GLES upload, display scanout or photon latency. The probe restored all four
normalized app values, cancelled/flushed capture, and settled confirmation before
exit. The unit test separately compares the updated actual imported Slider Scene
with a full resolve, checks the retained raw Arc across every sample, and covers
pending viewport activation after release/confirmation.

## Native Apply measurements

| Profiling-off software trial, ms | Blue Apply | Restore orange |
| --- | ---: | ---: |
| Preceding release, first pair | 310.443 | 258.357 |
| Preceding release, restored environment | 328.635 | 234.273 |
| Current release, first pair | 274.952 | 235.105 |
| Current release, restored environment | 274.735 | 235.705 |

These include an intentional 80 ms touch hold, readiness publication and 5 ms
polling. These small ordered samples and evolving app state do not establish a
controlled latency distribution. The native trial restores the current admitted
palette color rather than assuming older defaults.

Actual native presentation in the separate traced pair completed in **256.562 /
230.102 ms**, or **176.175 / 149.747 ms after touch release**. The preceding trace
was 339.076 / 243.888 ms. Trace/control differences do not measure profiling cost.

| Traced segment, ms | Current blue / restore |
| --- | ---: |
| Apply handler | 12.509 / 12.215 |
| Offer → authority response arrival | 69.990 / 58.279 |
| Native receive processing | 42.663 / 26.575 |
| Final CPU composition | 7.565 / 7.310 |
| Final renderer preparation/upload | 1.272 / 0.879 |
| Final GL submission | 10.819 / 9.301 |
| Presentation wait | 24.078 / 26.274 |

The native cold trial encoded once in **11.104 ms**, with one exact
shared-encoding event, compared with two encodes totaling 23.459 ms before.
Neither current authority trial contains an encode or binding-normalization miss
inside its response window; the preceding cold trial contained two encodes and
47.439 ms normalization. The complete process trace contains empty/admission miss
labels elsewhere. Absence inside these windows does not establish the cause of
the earlier miss or prove all cold paths are eliminated.

Across **every presentation prepared between Apply touch and confirmed native
presentation**, each current trial uploaded **138,976 bytes in three calls**,
versus 442,496 bytes in nine calls in each preceding traced trial. The four
current presentation byte counts are 45,984 / 0 / 47,008 / 45,984. The final
presentation alone is still 45,984 bytes, so the improvement comes from earlier
presentations. These are host vertex counters, not measured GPU bandwidth.

Native mesh-paint spans totaled **5.018 / 5.127 ms**, previously 12.488 / 14.433.
Three native composition spans totaled **29.925 / 20.288 ms**, previously
27.325 / 32.526: the cold total did not improve, and its cull spans alone totaled
10.298 ms. Final composition also remains around 7 ms. Inclusive spans overlap
children and must not be added to obtain end-to-end time.

Apply still performs one full layout resolve (**0.681 / 0.626 ms**) in these
windows. Layout is a small part of the remaining quarter-second interaction.
Serial durable sync (**24.464 / 23.436 ms**), validation, state exchange, receive
processing and presentation pacing remain. Durable sync is still required before
publication. This pass does not claim all inspector/source transitions avoid
layout or all interaction CPU work has been removed.

## Isolated ARM workload

The 128-node rounded layout resolves expressions, constructs and validates a
Scene at **192,287 nodes/sec (0.666 ms)**, versus 206,068 in the preceding sample.
Retained recolor is **0.744 ms**. These stages exclude OIB/state IO and GL. The
result still exceeds 100,000 nodes/sec on this bounded workload; it is not an
unbounded scene or XR throughput claim.

The new semantic-ID rebind workload changes every leaf ID while retaining exact
geometry. It replaces **zero of eight resources** with all 127 leaves visible,
and **zero of one** with 16 visible leaves. CPU composition is still **4.078 /
1.089 ms** respectively; normalization, culling and compatibility assembly are
not free. No preceding identity-rebind benchmark exists, so these are new
measurements rather than a measured speedup.

## Evidence and limits

Raw evidence is in [the matching data directory](data/oib-needs-layout-2026-11-10/):
readiness/public HTTP checks, normal/traced/restored native trials, bounded native
and authority traces, the existing analyzer's output, whole-window upload sums,
the ARM slider probe and isolated benchmark JSONL. The core example contains the
bounded slider operation; it uses public Client APIs. Deployment followed the
existing signed verification and compatible rollback workflow. No broad preflight
or browser campaign was substituted for the focused checks listed above.

