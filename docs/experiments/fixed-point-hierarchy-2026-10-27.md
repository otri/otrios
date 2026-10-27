# Fixed hierarchy, culling and flow evaluation — 2026-10-27

Historical experiment record. The fixed-point pilot and alternate GPU position
formats were subsequently retired; see [RFC 0019](../rfcs/0019-native-fixed-simd-pipeline.md).
Commands and feature names below refer to the evaluated revision, not the current
checkout. Retained float rendering and adaptive coverage storage remain selected.

The bounded coordinate experiment is qualified for review behind `fixed-ui-math`.
Pixel comparisons pass the unchanged error gate, actual glyph packing contains
NEON integer instructions, and dense scanout stays above 30 fps. **Float32 remains
the default:** full checked layout is slower on this Cortex-A7. The measurements
support a precision option and a faster admitted SIMD kernel, rather than a
claim that integer arithmetic makes the whole UI faster. No deployment occurred.

## Coordinate scope

The candidate adds checked i64 Q32.32 hierarchy origins, accumulated viewport
scales, clip intersections and flow allocation. Small offsets survive through
large cancelling ancestors until the final float presentation conversion.
Retained glyph bounds are admitted once; binary searches and whole-run rejection
compare integer clip coordinates corresponding to the actual GPU attributes.
Scrolling still changes only the viewport, with immutable glyph geometry and
visible draw ranges. It does not translate or rebuild each glyph every frame.

The existing Q48.16 pins/springs, Q32.32 expression/advance intermediates and local
i32 Q16 glyph packing remain. All operations are safe Rust with checked overflow
and explicit nearest/ties-away admission. Integer and unit-scale multiplications
avoid wide divisions. Flex allocation cancels exact common powers of two before
multiplying and divides once after applying the share. Wrap counts floor the
integer ratio. These shortcuts are compared to independent wide arithmetic over
signed extrema, rounding and overflow boundaries.

An unsupported hierarchy falls back as a complete frame; an unsupported flow
falls back as a complete plan. No partially resolved output is published and no
public coordinate bound is narrowed. Flow preserves the existing 0.01 fit
tolerance. A matrix of nested wraps, all alignments, flexible minima and viewport
sizes compares checked allocation to the independent float32 resolver within
0.001 logical pixels. Public float contracts, state codec and the v1 release
opening remain unchanged. Nonlinear shape tessellation, GPU curve evaluation,
interaction hit testing and gesture physics remain float based; this evaluation
does not claim a wholesale integer UI or shader implementation.

## Pixel controls

All five original float32 OTRTEXT3 fixtures remain byte-identical to the saved
[canonical-scale controls](fixed-point-canonical-scale-2026-10-27.md), including
metadata and viewport bytes. The added hierarchy fixture uses two viewport scales
(1.25 and 0.66), negative/descender text, all-edge clipping, and an admitted
+8192/−8176 parent cancellation with a 0.0001 offset. It has **120 distinct
fractional viewport positions**. Float32 starts at x=16; fixed starts at
x=16.0000991821, preserving the intended small offset. The separate arithmetic
unit test also exercises cancellation through a 100,000 intermediate coordinate.

| Fixture | Mali maximum channel error, rotations 0/90 | WebGL maximum, DPR 1/2/3.2 |
| --- | ---: | ---: |
| Dense | 6 | 1 |
| Small, 1 × 0.33 | 4 | 1 |
| Nested, 3 × 1.25 | 7 | 1 |
| Large, 8 × 2 | 4 | 1 |
| Input Lab | 5 | 1 |
| Two scales + cancellation + scrolling | 7 | 2 |

Each stream compares frames 0, 1, 17, 59 and its last frame. Maximum channel error
must remain ≤8; no threshold was loosened. Raw results include foreground and
edge-only means, changed pixels and complete cache diagnostics. Native pixels
use headless RGB888 readback; physical scanout uses RGB565. Both arithmetic paths
keep the same filtered coverage, colors, projection, font and clip workload.
Scalar fixed, NEON and host-generated fixed streams are byte-identical across
all six workloads. Their dense stream SHA-256 is:
`9a990f4507601aa7c44f769f5e934320cb9bbe62ff92cd2c8ac77ccbe057cbc1`.

[Native nested reference](assets/hierarchy-mali-reference.png),
[candidate](assets/hierarchy-mali-candidate.png),
[8x difference](assets/hierarchy-mali-difference-8x.png), and
[the DPR 3.2 browser strip](assets/hierarchy-browser-dpr3.2.png) preserve reviewable
captures. Historical failed captures remain in the earlier reports.

## Cortex-A7 timings

Rust 1.90.0 release, same font and dense input sequence, three repeats per text
mode. Scalar builds disable loop/SLP vectorization and target Cortex-A7. The fixed
SIMD build explicitly adds `-C target-feature=+neon`; CPU naming alone did not
produce the required instructions. The compiler warns that this target feature
flag is unstable; no unstable Rust API or unsafe intrinsic was introduced.

| Complete CPU workload | Scalar float32 | Scalar fixed | Fixed + NEON |
| --- | ---: | ---: | ---: |
| First unchanged-stream frame, median | 19.02 ms | 20.54 ms | 20.33 ms |
| Unchanged frames, median p50 | 3.798 ms | 4.176 ms | 4.158 ms |
| Unchanged frames, median p95 | 4.028 ms | 4.744 ms | 4.820 ms |
| Rebuild every frame, median p50 | 13.876 ms | 14.385 ms | 14.344 ms |
| Rebuild every frame, median p95 | 14.445 ms | 15.107 ms | 15.478 ms |

These probes include synthetic client/layout/mesh work and exclude GPU/network
work. Cold font admission occurs before the timed frame. The small scalar/NEON
full-workload difference does not establish a statistically significant gain.
Before exact integer fast paths, the fixed unchanged median was about 5.1 ms.
They reduce that cost while preserving every output bit.

The isolated admitted four-lane translation kernel measures **18.11 ns/quad**
with NEON, versus **30.07 ns** scalar fixed and **26.27 ns** scalar float32.
Its one-time admission costs 103–113 µs per 2,048 quads; this must be amortized.
Checking overflow per quad measures 36–59 ns and loses the advantage. Generated
runtime glyph `pack_x` contains two `vadd.i32` operations with vector loads/stores.
The translation microbenchmark is separate from the retained viewport mechanism.

A complete eight-item flow costs **40.91 µs** fixed versus **9.17 µs** float32;
the power-of-two cancellation improves its earlier 78.20 µs result. Two-axis
fixed pins/springs cost **2.02/4.13 µs**, versus **1.11/1.35 µs** float32.
Branching, checked wide arithmetic and conversion costs dominate these small
layouts. No layout SIMD speedup is claimed.

Final dense kernel-presented trials: **35.91, 35.44 and 35.63 fps**. The atlas stays **4 MiB**, with
28 cold coverage entries and roughly 2,781 visible of 5,040 retained glyphs.
These renderer replays exclude CPU layout; the separate CPU table prevents
attributing the 35 fps result to integer layout. The first timing attempt filled
the frame's temporary volume with saved comparison images; it is excluded.
Images were archived locally before their synthetic temporary copies were removed,
and all three acceptance trials were repeated with free space.

## Evidence and commands

[Complete pixel/CPU metrics, source hashes, fixture hashes and SIMD evidence](data/fixed-point-hierarchy-2026-10-27.json)
and [kernel scanout trials](data/fixed-point-hierarchy-scanout-2026-10-27.json)
retain the controls and individual repeats.

Generate each fixture independently with feature-disabled float32 and
feature-enabled fixed builds, adding `--fixed` to the latter:

```sh
cargo run --release -p otrios-core --features canvas --example text_scroll_frames -- \
  /tmp/float.mesh --dense --retained --styles
cargo run --release -p otrios-core --features canvas,fixed-ui-math --example text_scroll_frames -- \
  /tmp/fixed.mesh --dense --retained --styles --fixed
```

Use `--scale 1 --viewport-scale 0.33 --origin-x 16.25` for small,
`--scale 3 --viewport-scale 1.25 --origin-x 16.25` for nested,
`--scale 8 --viewport-scale 2 --origin-x 16.75` for large, and `--hierarchy`
for two scales/cancellation. Omit `--dense` for Input Lab. CPU runs add
`--measure-only`, and optionally `--rebuild`. `fixed_coord_probe` records both
scalar reference and checked kernel/layout timings. The browser comparison uses
`--stable-quads --cache-diagnostics --dpr 1,2,3.2`; native probes use
`OTRIOS_TEXT_STABLE_QUADS=1`, the pinned binary/fixture hashes, exclusive display
ownership, 120 warm-up and 240 sampled presentations, and service restoration.

Required default and fixed `bash scripts/check.sh` pass. The fixed core suite has
107 passing tests. Real Chromium input and shared layout integrations pass,
including caret recovery, drag/coasting, reflow during capture and both
orientations. The unchanged GPU adapter's actual Mali suite passes six tests at
each of 2x, 3x and exact sampling. These qualify dashboard rendering only; they do
not establish the planned Linux shared-memory PoC acceptance tests.
