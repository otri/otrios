# Fixed point and SIMD evaluation — 2026-10-27

The candidate is **experimental and disabled by default**. Dense Mali scrolling
still clears 30 fps, and the integer packing kernel uses verified NEON, but the
new pixel comparison gate fails at large scales and browser DPR 2. Checked layout
arithmetic is also slower. These results do not sign off the stretch milestone.
The existing float32 renderer remains the production reference; nothing was deployed.

The reference is commit `9ba5fce4a097d792f1cc612e131e29da47672104`.
The regenerated dense float32 fixture has exactly the committed baseline's SHA-256,
`388be1fa8ed800bb17646d2cf86c79d24c7bfffc09b43bb21f421862114493f0`.
Both paths use the same admitted Roboto font, 2x prefiltered coverage, authored sRGB
blend, projection, clipping and renderer. This comparison does not mix in the
separate direct-versus-filtered approximation from RFC 0017.

## Implementation under evaluation

`otrios-core/fixed-ui-math` selects checked i64 Q48.16 coordinate arithmetic for
pins/springs and i64 Q32.32 arithmetic for numeric layout expressions and accumulated
font advances. Products/divides use checked i128 intermediates. Ties round away
from zero. Ratios retain extra fractional bits until after a dimension is applied;
spring inputs share an exact power-of-two normalization, including subnormals.
The public float contracts, coordinate limits and state protocol are unchanged.
Overflow, unsupported glyph scales and positive sub-grid dimensions use the
original float32 path. Expression underflow falls back as a whole expression.

Glyph styles cache scaled font metadata. A complete run is range checked once,
then groups of four ink glyphs use a safe integer packing kernel. Immutable inputs
and private admitted bounds justify wrapping lane operations without introducing
unsafe Rust. i32 Q16 lanes are local and bounded; the public ±100,000 coordinate
limit has not been replaced with i32 Q16's smaller range. Conversion to float32
happens at existing scene/GPU boundaries. Parent transforms, flow composition,
hit testing, viewport offsets, culling bounds and animation physics still use
float32. This is a pilot, not an entirely integer UI engine or an FP16 shader fix.

The separately useful CPU mesh capacity reuse applies to both comparison paths.
It only takes buffers when no immutable frame snapshot retains them; the web host
releases its obsolete presentation snapshot before composing a replacement.
Its allocation savings must not be attributed to fixed arithmetic.

## Cortex-A7 measurements

Rust 1.90.0, release ARMv7 builds, no new dependencies. Scalar flags:
`-C target-cpu=cortex-a7 -C no-vectorize-loops -C no-vectorize-slp`.
SIMD flags: `-C target-cpu=cortex-a7 -C target-feature=+neon`.
The explicit NEON codegen switch produces Rust's unstable-target-feature warning;
this uses stable safe Rust, not nightly portable SIMD or unsafe intrinsics.
CPU name alone did not enable NEON. Disassembly verifies `vadd.i32` in the
integer kernel called by glyph emission, and in admitted bulk translation.
Full runtime disassembly is an ignored local artifact, not copied into Apache docs.

Final CPU measurements held the display-trial lock and paused/restored only the
direct display service. Each coordinate entry is the median of nine samples of
1,000 iterations over 2,048 quads. Admission is outside the hot loop.

| Coordinate kernel | Scalar build, ns/quad | NEON build, ns/quad |
| --- | ---: | ---: |
| Float32 | 26.51 | 26.51 |
| Four checked scalar additions | 36.44 | 36.79 |
| Checked four-lane overflow mask | 59.17 | 56.96 |
| Pre-admitted integer batch | 30.11 | 17.42 |

Admission for 2,048 quads cost 104–122 µs. The fast result requires amortizing that
cost; checking overflow on every quad was slower than float32. Outputs agree for
the synthetic coordinate workload. The scalar and NEON full dense fixed fixtures
are byte identical: `c01ab7662f73e782f3f9f418417f8d86330f50e6617a247e4144b20e0466a462`.

Complete synthetic scene/client/mesh processing, 120 frames per case:

| Math/build | Cold first frame, ms | Unchanged viewport p50/p95, ms | Forced content rebuild p50/p95, ms |
| --- | ---: | ---: | ---: |
| Float32 scalar | 19.15 | 3.85 / 4.28 | 13.82 / 14.19 |
| Float32 NEON-enabled | 19.46 | 3.90 / 4.17 | 14.10 / 15.12 |
| Fixed scalar | 19.07 | 4.02 / 4.29 | 13.44 / 13.79 |
| Fixed NEON | 18.48 | 4.01 / 4.44 | 13.20 / 14.05 |

These are one ordered sweep, not a statistical claim that the small scalar/NEON
mesh difference is significant. Rebuild improvement is modest; unchanged scrolling
has no new glyph math to accelerate. Layout admission and wide arithmetic cost:

| Two-axis resize, NEON-enabled build | Float32, µs/frame | Fixed, µs/frame |
| --- | ---: | ---: |
| Pins | 1.12 | 3.04 |
| Springs | 1.36 | 8.45 |

Fixed arithmetic improves cancellation behavior but does not automatically improve
throughput. Tests retain a 1/65,536 step across ±100,000 integer arithmetic and
compare long fractional glyph runs against an f64 font-metric reference before
presentation. They cover signed ties, overflow, tiny ratios and fallback admission.

Three isolated native replay runs of the fixed dense fixture achieved **35.91,
35.72 and 35.53 kernel-presented fps**, 120 warmup plus 240 measured presentations.
There are 5,040 retained glyphs, approximately 2,781 visible. Replay excludes
client/layout/network work; do not add the CPU table to replay FPS. An earlier
trial overlapped a CPU experiment and included a 29.82 fps run; that contaminated
trial is excluded, and the final CPU and display runs were serialized.

## Pixel comparisons and unresolved gate

Five workloads: ordinary dense text, scale 1 inside a 0.33 viewport, scale 3 inside
a 1.25 viewport, scale 8 inside a 2x viewport, and actual Input Lab layout. Each
compares frames 0, 1, 17, 59 and the final frame, including fractional scrolling
and boundary clipping. Native uses rotations 0/90; browser uses DPR 1/2. Metadata
and colors match exactly in the dense CPU meshes. Maximum CPU XY displacement is
0.001221 logical pixels, reflecting changed advance accumulation/rounding.

| Workload | Mali maximum channel delta, 0–255 | Browser maximum, DPR 1/2 combined |
| --- | ---: | ---: |
| Dense | 5 | 14 |
| Small viewport | 4 | 1 |
| Nested viewport | 5 | 12 |
| Large viewport | 41 | 31 |
| Input Lab | 5 | 13 |

The new comparison command fails above 8, conservatively matching the existing
native viewport check's limit. It currently **fails** for four browser workloads
and the large native workload. Existing native pixel tests and their thresholds
are unchanged. Means and changed/above-one pixel counts are recorded in
[the raw comparison data](data/fixed-point-text-2026-10-27.json). Mean errors use
only pixels where either render differs from the background; a large empty
background cannot dilute them. Ordinary dense native mean channel error is at
most 0.028; large-scale differences are localized but their maxima remain failures.

A repeat baseline-versus-itself browser control has zero error. A baseline shifted
by just one Q16 unit (1/65,536 pixel) reaches a channel delta of 12; this WebGL
renderer reports four subpixel bits. A hybrid diagnostic retaining float32 UVs but
using fixed positions retains the dense DPR 2 discrepancy, whereas changing only
UVs stays within 1. This supports rasterizer phase sensitivity as a contributor;
it does not prove every Mali large-scale difference harmless. CPU fixed point
does not make the GPU's rasterization or filtering exact.

Saved strips show reference, candidate and an 8x amplified difference:

- [Dense browser, DPR 2](assets/fixed-dense-browser-dpr2.png).
- [Large browser worst frame, DPR 2](assets/fixed-large-browser-dpr2.png).
- [Large Mali worst reference](assets/fixed-large-mali-reference.png),
  [candidate](assets/fixed-large-mali-candidate.png),
  [8x difference](assets/fixed-large-mali-difference-8x.png).

Before enabling the feature: resolve or explain the remaining scale/phase pixel
differences with a stronger raster reference, retain existing checks, and reduce
checked layout conversion/division overhead or separate that precision choice
from the glyph optimization. Flow/viewport/culling arithmetic would need its own
migration and baselines. No acceptance of those remaining changes is implied.

## Reproduction and validation

Generate float32 and opt-in candidate fixtures separately, so feature-selected
layout arithmetic is covered as well as explicitly selected glyph math:

```sh
cargo run --release --locked --offline -p otrios-core --features canvas \
  --example text_scroll_frames -- /tmp/reference.mesh --dense --retained
cargo run --release --locked --offline -p otrios-core --features canvas,fixed-ui-math \
  --example text_scroll_frames -- /tmp/candidate.mesh --dense --retained --fixed
node scripts/compare-fixed-text-browser.mjs --reference /tmp/reference.mesh \
  --candidate /tmp/candidate.mesh --chromium /path/to/chromium --output /tmp/pixels
```

Use `--scale 1 --viewport-scale 0.33`, `--scale 3 --viewport-scale 1.25`, or
`--scale 8 --viewport-scale 2`; omit `--dense` for Input Lab. Fractional origins
use `--origin-x 16.25`. For CPU timings add `--measure-only`, then `--rebuild`
for content changes. `fixed_coord_probe` includes both coordinate kernels and
complete pins/springs admission. Use the flags above for the two ARM builds.

On the native frame, build `text_pixel_probe` with `otrios-linux-display/platform`,
then run `text_pixel_probe reference.mesh candidate.mesh output-directory` under
the same exclusive display lock, kiosk user and save/restore procedure as
`benchmark-text-rendering.py`. It saves frame 17 and worst-frame captures, reports
metrics but does not itself declare visual acceptance. Use the existing benchmark
controller with explicit binary/fixture SHA-256 and `--minimum-fps 30` for scanout.
Serialize CPU and GPU profiling; do not run them together.

Validation passed: `bash scripts/check.sh` with default math and again with
`OTRIOS_CHECK_FIXED_UI_MATH=1`; ARM platform Clippy; 9 mock WebGL tests; actual
Mali ignored tests (5 each at 2x, 3x and exact sampling); fixed-feature Wasm build;
real browser Input Lab drag/coast/caret recovery; general browser checks including
both orientations, shared edits/ink, resize and Layout Lab. These successful
checks do not override the failed new pixel gate.
