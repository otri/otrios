# Retained glyph raster follow-up — 2026-10-27

Historical experiment record. The fixed-point pilot and alternate GPU position
formats were subsequently retired; see [RFC 0019](../rfcs/0019-native-fixed-simd-pipeline.md).
Commands and feature names below refer to the evaluated revision, not the current
checkout. Retained float rendering and adaptive coverage storage remain selected.

This revision improves the pixel comparisons but is **not accepted**. The
experimental corrected-quads mode passes the original DPR 1/2 comparisons, but
fails the new DPR 3.2 control and the 30 fps scanout gate. It remains disabled by
default. No deployment occurred. The [initial evaluation](fixed-point-text-2026-10-27.md)
and its failed captures remain unchanged as the historical reference.

## What the measurements identified

A retained strip has a uniform logical font scale. Its Y coordinates stay local,
while X coordinates accumulate across a long line. Deriving atlas size from an X
subtraction can create several cache sizes for the same glyph after float32
rounding. The experimental mode derives both scales from the local Y span and
the physical projection. It also expands retained quad bounds to whole pixels and
corrects their UVs. CPU-clipped quads without a viewport keep their original
bounds. The atlas stays 1024² RGBA8: **4 MiB**, with no new texture allocation.

The native renderer previously abandoned cached rendering for an entire mesh if
one glyph did not fit. It now keeps a per-glyph validity mask and splits only the
visible range into consecutive cached/analytic draws, preserving painter order.
Fully cached runs keep their single draw. Unsupported entries have zeroed staging
bytes and cannot expose uninitialized memory. The new Mali regression forces a
cached → oversized analytic → cached sequence, compares it to separately drawn
references, checks ink in every clipped region and checks the 4 MiB bound.

With `stableTextQuads: true` in WebGL or `OTRIOS_TEXT_STABLE_QUADS=1` natively:

| Existing workload | Native maximum channel delta | Browser maximum, DPR 1/2 |
| --- | ---: | ---: |
| Dense | 5 | 1 |
| Small | Not remeasured in this revision | 1 |
| Nested | 4 | 1 |
| Large | **0**, both rotations and all selected frames | 1 |
| Input Lab | 4 | 1 |

The large native comparison now retains useful coverage instead of rendering the
whole mesh analytically. [Reference](assets/stable-large-mali-reference.png),
[candidate](assets/stable-large-mali-candidate.png), and
[8x difference](assets/stable-large-mali-difference-8x.png) contain the expected
text. [The browser strip](assets/stable-large-browser-dpr2.png) uses the same
reference/candidate/difference ordering. Scalar fixed and NEON still use the
unchanged dense fixture hash from the initial evaluation.

These results compare float32 and fixed placement within the experimental
presentation path. They do not establish identity to the original renderer's
analytic fallback for large text; selecting prefiltered coverage there is a
separate presentation change. The old captures and committed float32 fixture are
preserved so that change remains visible.

## Gates that still fail

At DPR **3.2**, the dense fixture fills the supported 4096-pixel canvas width.
Its maximum float32/fixed channel difference is **26**, above the unchanged gate
of 8. Both paths report the same 28 atlas misses and zero analytic fallbacks, so
cache exhaustion does not explain this remaining case. The CPU snap occurs before
float32 attribute storage and the final GPU transform; projected vertex rounding
is the next precision path to investigate.

Three isolated Mali runs with corrected quads enabled measured **28.3233 fps**
each, below the target. The same binary and fixture with correction disabled
measured **35.1650 fps** in one control run. CPU submission medians remain about
4.1 ms and there are 28 coverage misses in both modes. A CPU count of pixel-center
rectangle coverage after scissoring finds **17.58% extra fragments** on average
for expanded quads. This is not a GPU hardware counter, but it supports overdraw
as the performance cause. Pixel correctness cannot be accepted by adding that
rendering cost and missing the explicit 30 fps goal.

The next experiment should snap projected vertex bounds to the nearest pixel
edge that preserves the original covered pixel centers, and correct UVs in the
vertex shader. That should avoid extra fragments and avoid CPU-to-GPU projection
rounding changing the sample phase. It requires actual pixel and scanout tests;
this document does not claim it works yet. Coverage support, atlas guard bands,
clipping and large/small-coordinate admission must remain intact. Do not relax
the comparison threshold or reduce the workload to pass.

## Checked arithmetic work

Fixed admission now decodes the exact IEEE f32 mantissa/exponent with integer
shifts and explicit ties-away rounding. A deterministic 65,536-pattern test
compares both fractional precisions to an independent f64 admission reference.
Bounded divisions use native 32/64-bit paths when possible, with the checked i128
path retained. Wide negative extrema return an overflow error instead of risking
a signed-negation overflow. Springs remove a common exact power of two from
proportional segments before multiplying; ratios and their precision are unchanged.
Pins convert only the values their rule actually uses, and an identity rule
preserves the original admitted positive dimensions without rounding.

A subsequent Cortex-A7 probe measured two-axis fixed pins at **2.12 µs**, versus
**1.16 µs** for float32, and fixed springs at **4.12 µs**, versus **1.40 µs** for
float32. This improves the initial fixed 3.04/8.45 µs results, but remains slower.
These are selected arithmetic measurements, not a new full-layout acceptance.
The initial scalar/NEON mesh and kernel results remain the recorded baseline.

## Validation and reproduction

[Raw results](data/fixed-point-raster-followup-2026-10-27.json) include each frame,
cache/fallback counts, the high-DPR failure, the failed FPS trials, the disabled
control and the rectangle-overdraw estimate. Native and browser pixel probes now
both return failure above a maximum channel delta of 8.

```sh
node scripts/compare-fixed-text-browser.mjs --stable-quads --dpr 1,2,3.2 \
  --reference /tmp/reference.mesh --candidate /tmp/candidate.mesh \
  --chromium /path/to/chromium --output /tmp/corrected-quads
```

Omit `--stable-quads` to retain the old browser presentation mode. On the frame,
set `OTRIOS_TEXT_STABLE_QUADS=1` for the pixel probe and the benchmark controller;
use `0` for the same-binary disabled control. Keep binary/fixture SHA-256 checks,
exclusive display ownership and service restoration as in the initial procedure.

Required workspace checks pass with default and fixed features. Ten mock WebGL
checks pass. The actual Mali suite passes six tests at each of 2x, 3x and exact
sampling, including the new mixed cache/raw regression when sampled. Those tests
do not override the new high-DPR and performance failures. The remaining stretch
work also includes fixed viewport transforms/culling and broader layout migration;
flow, hit testing and physics have not silently become integer implementations.
