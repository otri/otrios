<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Apply: actual Frame hotspots and parallelism — 2026-11-09

The Frame already exceeds 100,000 elements/second for the measured arithmetic
layout workload. The long Accent Apply follows a much larger document pipeline:
serialization/admission, repeated compilation, state validation and inspector
reconstruction. A color change also rebuilds geometry, but that is a smaller
part of this particular delay.

## Device layout and geometry throughput

An optimized ARMv7 executable ran on the actual Frame, with profiling disabled.
The machine reports four online CPUs (`0-3`), `armv7l`, Linux
`6.18.57-spd-kiosk`, and 994,736 kB total RAM. Clock/governor sysfs readings were
unavailable; no governor, affinity or hardware settings were changed.

The benchmark repeats an admitted 128-node scene containing bound color and
position expressions, a clipping root and 127 leaves. It preserves all existing
admission limits. This measures repeated bounded layouts, not a simultaneously
admitted 100,000-node document, general intrinsic text layout, or GL throughput.

| Rounded-shape workload | Time per 128-node scene |
| --- | ---: |
| Resolve expressions, construct and validate Scene | 0.614 ms |
| Encode layout and admit its representation | 28.817 ms |
| Decode and admit layout | 18.687 ms |
| Retained composition after a color change | 14.385 ms |

The resolve result is **208,615 nodes/second**, including scene validation.
The encode/decode rows are separately timed operations and are not additive:
encoding itself includes a decode/admission pass. Resolve uses 200 iterations;
each codec uses five; changing-color composition uses 20 actual color changes.

Visibility currently saves drawing more reliably than geometry construction:

| Retained rounded-shape case | Visible leaves | Generated triangle vertices | Mesh bytes |
| --- | ---: | ---: | ---: |
| All visible | 127 | 19,812 | 477,012 |
| Most outside the viewport | 16 | 19,812 | 477,012 |

Cold retained rebuild took 17.605 ms versus 17.350 ms in those cases. The fixed
clipped painter's second case produced 2,496 vertices, although its primitive
tessellation still lacks an early bounds rejection. Existing Designer row slabs
already limit the retained scene; this result concerns work inside that scene.

`TextPresentation` clones/normalizes the complete scene and uses one key that
includes paint values. A changed color invalidates that key. Retained painting
then traverses empty clips and constructs local geometry before clipping draw
ranges. Each rounded superellipse builds 52 outline points using trigonometry
and `powf`, followed by 156 triangle vertices. Visible content plus a small
overscan/cache margin should determine geometry/resource preparation.

Sources: [retained_text.rs](../../crates/otrios-core/src/dashboard/retained_text.rs),
[render.rs](../../crates/otrios-core/src/dashboard/render.rs), and
[benchmark](../../crates/otrios-dashboard/examples/layout_throughput.rs).

## Actual native Apply trace

The signed instrumentation release is
`242ddb8df90a4499525544fdc1f56a5417c527c88f7472fc9931844213f0bfc2`,
from implementation commit `c10f453`. It changes diagnostics only. App UI/library
bytes, protocol v30, release-control v1 and mutation semantics remain unchanged.

The same calibrated evdev probe selected Display's explicit Edit action, changed
Accent to blue, applied it, then restored gold. It waited for the expected OIB
generation in a confirmed native presentation. Coarse nested spans captured
authority and client phases; the existing native input/worker/renderer trace
captured handoffs, uploads and completed presentation. All trace buffers report
**zero dropped events**. The services' temporary diagnostic environments were
removed and both processes were verified to have no profiling variables afterward.

| Serial path segment | Blue, first new color | Restore gold, warm authority cache |
| --- | ---: | ---: |
| Operator's software touch hold | 80.4 ms | 80.4 ms |
| Apply release handler | 42.1 ms | 44.0 ms |
| Request offer to authority response arrival | 292.8 ms | 136.2 ms |
| Native client receive: validation and UI reconstruction | 375.6 ms | 331.7 ms |
| Final CPU composition | 16.6 ms | 15.0 ms |
| Renderer preparation/upload and GL draw submission | 13.3 ms | 12.0 ms |
| Blocking presentation wait | 18.0 ms | 25.9 ms |
| Remaining queues, handoffs and metadata | 11.8 ms | 8.6 ms |
| Input sample to completed presentation | 850.6 ms | 653.7 ms |

The authority interval includes processing, persistence, scheduling and loopback
delivery. Its `authority.process` spans were 276.7 and 123.3 ms; disk sync inside
them took 18.1 and 16.5 ms. These are elapsed durations, not CPU hardware-counter
samples. GL submission and presentation waiting do not separately identify GPU
execution versus scanout wait.

The native side did the following **per Apply**, across input handling through
the confirmed frame:

- **14 layout decodes:** 140.0 / 123.9 ms exclusive.
- **6 layout encodes:** 32.1 / 27.9 ms exclusive, in addition to their nested
  decode/admission cost. Adding inclusive encode and decode totals double counts.
- **2 full compiler misses:** 221.4 / 195.3 ms inclusive, substantially containing
  those codec phases. Validation and presentation ask for the same effective
  graph using `None` versus selected-instance cache keys. Current canonical
  `graph::expand` ignores its `_selected` argument, so this key distinction
  prevents useful sharing.
- **25 instance-metadata expansions:** 38.1 / 34.7 ms exclusive.
- **31 authored-seed reads/clones:** 25.6 / 24.5 ms exclusive.
- **3 information projections:** 79.7 / 73.7 ms inclusive, including nested
  metadata/property/baseline work.
- **One Designer reconstruction:** 70.3 / 62.9 ms inclusive.
- **9 actual layout resolutions:** only 5.11 / 4.74 ms total.

These nested categories overlap. The measured large costs are the document
pipeline around layout, especially codec/admission work and repeated metadata.
Immutable baseline digests were also serialized/hashed four times on each side
of a write (roughly 39 ms combined). Library import resolution revalidates graph
metadata and rederives locked export contracts, although the pinned library has
not changed. This is additional reusable work, rather than the largest single
category in this trace.

Relevant paths: [compiler/cache](../../crates/otrios-core/src/canvas/state/oib.rs),
[import resolution](../../crates/otrios-core/src/canvas/oib/imports.rs),
[canonical graph expansion](../../crates/otrios-core/src/canvas/oib/graph.rs),
[client activation](../../crates/otrios-core/src/canvas/client.rs),
[Designer](../../crates/otrios-core/src/canvas/client/oib_designer.rs), and
[authority persistence](../../crates/otrios-dashboard/src/server_store.rs).

## Execution plan

The highest-value sequence is:

1. **Keep admitted typed layout/compiler data alive.** Remove Rust → encoded
   layout → decoded/admitted layout round trips from ordinary property updates.
   Admission belongs at changed code/structural bytes. Keep validating changed
   causal property data and the coherent publication boundary.
2. **Share one effective compilation.** Remove the unused selected-instance
   distinction from canonical compilation. Reuse resolved library exports,
   baseline digests and instance/property metadata under exact immutable
   baseline/source/residency keys. A property write should not repeat them.
3. **Make Accent a paint update.** Track property dependencies/effects; this
   Accent's color targets do not change geometry. Retain geometry separately
   from paint/style, update affected visible draws, and preserve layout/input
   identities. Changes to width, text or structural source take their appropriate
   layout/compilation path.
4. **Parallelize the remaining preparation.** The Frame already separates input,
   state, networking and GL ownership, but its per-edit preparation is serial.
   Independent visible subtree/mesh jobs can use a bounded worker pool. A client
   can prepare an immutable candidate concurrently with authority validation and
   persistence, then activate it only if its baseline/generation still matches
   the confirmed result and the existing gesture barrier permits activation.
   Discard obsolete jobs. Keep GL context ownership, ordered durable receipts
   and atomic publication with their respective owners. Avoid holding the global
   app hub over expensive preparation or blocking disk work; preserve per-room
   commit order while allowing independent work elsewhere.

Layout dependencies constrain parallel evaluation: child intrinsic measurement
and parent sizing/placement need their dependency order. With a 0.61 ms pass at
this size, scheduling layout nodes across cores is a lower priority than removing
the repeated codec/compilation work. As a counterfactual, perfectly splitting
only the 331.7 ms client stage four ways would leave about **325 ms** of the warm
post-release pipeline, with all other stages held fixed. That is a model, not a
measured speedup. A paint-only path removes whole stages instead.

## Subsequent implementation

The [incremental presentation cleanup](oib-incremental-presentation-2026-11-10.md)
implements typed compilation, exact-key immutable metadata reuse, conservative
Color-only activation and independent paint with visible/overscan geometry.
The measurements above describe the preceding release. Parallel preparation
remains future work; no worker pool or speculative activation was introduced.

## Reproduction and limits

Run the benchmark on each target with profiling disabled:

```sh
cargo run -p otrios-dashboard --features canvas --example layout_throughput \
  --release --locked --offline -- --nodes 128 --iterations 20
```

`OTRIOS_CORE_PROFILE=1` enables the bounded coarse collector. The authority's
`OTRIOS_CORE_PROFILE_OUT` writes technical events to a private tmpfs file.
Native `OTRIOS_NATIVE_PAN_LOG` plus `OTRIOS_NATIVE_PAN_FINISH` allows all trace
lanes to flush without terminating the host. The collector reads no clocks or
allocates records when disabled; Wasm profiling is a no-op. Spans cannot cross
threads or async suspension. Native tracing additionally caps retained core
events at 32,768; the authority sink caps its file at 16 MiB.

The software hold is included in the overall samples but is not application
processing. Remote readiness polling observed 858.1 / 663.1 ms; completed native
presentation was earlier by 7–9 ms. Profiling-off controls after restoration
measured 789.6 / 691.3 ms. Two trials do not establish a distribution or a controlled
profiling-overhead estimate. No photon-latency or GPU hardware-counter claim is
made. No state was reset, no pixel export occurred, and gold was restored.

Validation: isolated profiler test **1 passed**; native recorder tests **3 passed**;
authority tests **9 passed**; core/dashboard focused Clippy with warnings denied
passed; signed ARM/Wasm build, native readiness/collector checks and the profiling-off
live control passed. The full suite and browser interaction campaign were not run.

Raw [ARM benchmark](data/oib-hotspots-2026-11-09/layout-arm.jsonl),
[Mac benchmark](data/oib-hotspots-2026-11-09/layout-mac.jsonl),
[analysis](data/oib-hotspots-2026-11-09/analysis.json),
[native trace](data/oib-hotspots-2026-11-09/native-worker.jsonl),
[renderer trace](data/oib-hotspots-2026-11-09/native-renderer.jsonl),
[authority trace](data/oib-hotspots-2026-11-09/authority.jsonl),
[control](data/oib-hotspots-2026-11-09/control-apply.json), and
[analysis script](data/oib-hotspots-2026-11-09/analyze.py) preserve the evidence.
