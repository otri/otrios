# Native retained coordinates, SIMD packing and coverage storage

Historical experiment record. The fixed-point pilot and alternate GPU position
formats were subsequently retired; see [RFC 0019](../rfcs/0019-native-fixed-simd-pipeline.md).
Commands and feature names below refer to the evaluated revision, not the current
checkout. Retained float rendering and adaptive coverage storage remain selected.

This implements the native work in [RFC 0019](../rfcs/0019-native-fixed-simd-pipeline.md).
The Wasm SIMD module selection milestone remains separate. Measurements use the
actual Cortex-A7/Mali-400 frame, Rust 1.90.0 and the existing font, text density,
scanout format, calibration and maximum channel error gate of eight. No new
dependencies or state protocol changes are involved.

## Retained CPU work

The fixed path retains preorder slots containing admitted source fields, checked
Q32.32 placements/clips and their GPU-rounded counterparts. Changed fields and
dependent placements are recomputed; topology changes invalidate the affected
suffix. A failed admission clears the cache and falls back for the whole frame.
Unchanged scrolling no longer clones a normalized scene or rebuilds placement
maps. Whole offscreen runs are rejected before binary horizontal culling.

The shared Canvas client also retains resolved layout and editor bounds during
panning. Presentation changes invalidate the presented scene; edits, focus,
selectors, font and layout changes still invalidate layout. Candidate admission
and tentative resize validation resolve fresh inputs. Captured pans use stable
layout coordinates and avoid building a redundant presented scene.

Glyph packing gathers admitted local i32 lanes into contiguous scratch arrays
before interleaving attributes. Integer-to-float conversion followed by an exact
power-of-two exponent adjustment enables vectorization without fast-math or
unsafe intrinsics. Checked wide setup, scale multiplication and division remain
scalar. There is no per-glyph geometry translation during retained scrolling.

On Cortex-A7 with `-C target-cpu=cortex-a7 -C target-feature=+neon`, disassembly
confirms four-lane `VADD.i32` and `VCVT.f32.s32` in the placement kernel. This is
instruction evidence, not an attribution of all frame savings to SIMD.

Three isolated repetitions, microseconds per complete synthetic CPU frame:

| Path | Coasting p50 | Coasting p95 | Rebuilding p50 | Rebuilding p95 |
| --- | --- | --- | --- | --- |
| Prior float control | 3878–3932 | 3988–4616 | 14045–14051 | 14321–14787 |
| Prior fixed/NEON | 4196–4228 | 4483–4918 | 15225–15247 | 15962–17017 |
| Retained fixed/NEON | 720–732 | 782–968 | 12795–12890 | 13128–14036 |
| Retained float control | 718–728 | 780–872 | 10710–10724 | 11287–11703 |

This harness includes the historical Input Lab client update and dense synthetic
scene construction; it excludes GPU/network work. It is separate from the real
full-screen demo below. Fixed coasting is approximately 5.8 times faster than its
previous implementation. The shared retention changes explain most of that
gain: optimized float and fixed coasting are effectively tied, and fixed rebuilds
remain about 20% slower. **Keep fixed math opt-in.**

Warm profiled fixed frames spend roughly 500 us in source construction, 145–149 us
in placement/cache work and 64–65 us in culling. Scrolling changes 28 of 58
placements, publishes 928 bytes of viewport data and publishes zero geometry
payload bytes. Payload counters describe serialized mesh data, not allocator
traffic or GPU-only DMA time. Normal composition reads no profiling clocks.

## GPU positions

The adapter compares existing float attributes, 16.16 `GL_FIXED` and rebased
`GL_SHORT`. Integer encodings prove the entire mesh range before writing; an
unsupported mesh falls back to float as a whole. Reusable staging is populated
only when immutable geometry/cache data changes. Painter order, draw count,
viewport culling and retained snapshots are preserved.

| Position format | Dense kernel-presented fps, three repeats | Cached VBO bytes | First packing/upload time |
| --- | --- | --- | --- |
| Float | 35.91 / 35.82 / 35.63 | 403200 | 5.327 ms |
| Fixed | 35.91 / 34.10 / 35.26 | 403200 | 14.859 ms |
| Short | 35.72 / 35.91 / 35.91 | 322560 | 13.250 ms |

These trials retain the prior RGBA coverage atlas to isolate position encoding.
First packing/upload includes CPU encoding and driver submission; it does not
isolate GPU transfer time. This prototype packs existing retained float vertices,
so it does not establish a benefit from a future direct integer producer.

Across six cases and native rotations 0/90, fixed CPU geometry with float GPU
attributes has maximum channel errors 6/4/7/4/5/7 for dense/small/nested/large/
input/hierarchy. GL_FIXED alone has maxima 0/4/0/0/0/4. GL_SHORT has
14/6/27/101/13/10 and fails the unchanged gate. **Retain GL_FLOAT by default.**
`OTRIOS_TEXT_POSITION_FORMAT=fixed|short` remains an explicit experiment.

## Coverage atlas and actual presentation

CPU improvements alone leave the full-screen Layout Lab demo at 27.40 fps, with
35.306 ms median kernel flip intervals. Removing the disabled vertex tint branch
(27.48 fps) and rebasing draw indices (27.46 fps) do not fix this. Neither
experiment is selected.

An alpha-only sampled atlas is the useful GPU experiment. Glyphs still rasterize
with the existing analytic coverage shader into RGBA8 scratch. A GPU-side
`glCopyTexSubImage2D` transfers alpha into the atlas; composition samples that
eight-bit coverage and applies the existing premultiplied tint. This follows the
[GLES2 framebuffer-to-texture copy rules](https://registry.khronos.org/OpenGL/specs/es/2.0/es_full_spec_2.0.pdf).
No CPU readback, fractional-phase rerasterization or gamma quantization is added.

The 1024-square alpha atlas uses 1 MiB and the 512-square RGBA8 scratch uses
1 MiB, versus the previous 4 MiB RGBA atlas. These are declared texture payload
bytes, including scratch; driver allocation metadata is not measured. Padding
is initialized. A dirty
working-set inspection selects the original RGBA atlas when an otherwise
cacheable entry exceeds 256 texels in either dimension. This conservative cutoff
is below scratch capacity and preserves large-text raster parity and cold cost. Storage switches discard the old target before
allocating its replacement, preserving the 4 MiB maximum. Entries too large for
the original atlas retain the existing ordered analytic fallback. Coasting does
not inspect all glyphs or switch storage. Consecutive text draws also reuse
identical scissor bounds.

This adaptive choice matters: the initial always-alpha experiment changed the
large-text rasterization result, exceeded the original-capture gate (error 17) and
spent about 22.8 seconds in cache preparation. These entries still fit scratch;
the cost belongs to the cold alpha path, not an assumed analytic fallback.
It is rejected. Large text retains RGBA coverage. Alpha scratch rendering also
preserves original atlas-space vertex arithmetic and shifts only the integer
viewport origin, avoiding an extra source of coordinate rounding differences.

Initial full-screen interactive measurement: **42.48 kernel-presented fps**,
17.654 ms median flip interval, versus approximately 27.4 fps with RGBA coverage.
The controller uses the existing calibrated evdev reader, three alternating
two-axis drags, release momentum, RGB565 scanout and the unchanged 64-row demo.
The measured interval includes a deliberate capture/readback stall. These are
actual kernel completions, not worker ticks or a HUD estimate.

Cold coverage preparation is a tradeoff: the initial dense alpha trial records
226.9 ms of key/cache preparation versus 43.0 ms for RGBA, and 5.54 ms versus
5.33 ms for vertex packing/upload. These are total probe counters, including
coverage work and driver stalls, not isolated GPU copy timings. The faster warm
scrolling is selected; cold preparation is not claimed to improve. Larger entries
retain RGBA to avoid the severe cold regression above.

Final conservative-policy qualification:

| Workload / storage | Kernel-presented fps |
| --- | --- |
| Dense fixture, selected alpha, three repeats | 56.41 / 56.65 / 56.65 |
| Full demo, same candidate binary forced to RGBA | 27.52 |
| Full demo, selected alpha, two repeats | 41.98 / 42.63 |
| Full demo, exact signed-build executable, default settings | 42.11 |

The 30 fps native milestone is met in both the dense replay and the real full
screen demo. The demo still misses some refresh intervals; this is not a 60 fps
lock. Its 95th-percentile flip interval is approximately 35.3 ms. No text, rows,
clipping coverage or scanout quality were removed to obtain these results.

All 60 native fixed/reference frame pairs at 2x sampling pass with maximum error
seven. A separate comparison of 24 saved frame-17 captures against the preserved
original RGBA captures also passes, maximum seven. Both large-text reference
captures are byte-identical to the originals. Eight native GPU cases pass in
seven configurations: alpha/RGBA at scales 2, 3 and analytic 0, plus alpha with
linear-light composition at scale 2. Atlas-specific cases return early in the
analytic configuration. The storage-transition case covers medium and large
entries and exact recovery of the original small-text pixels.

The selected defaults are adaptive alpha coverage and GL_FLOAT positions, with
`OTRIOS_TEXT_ALPHA_ATLAS=0` retaining the RGBA control. Fixed CPU math and losing
GPU position formats remain opt-in. The exact signed-build native executable is
SHA-256 `c65e9fc842b27a50f8f37117fb6254191cf87cc4e659207c15527e3e1bc1d373`.

[Original native pixels](assets/native-retained-simd-2026-10-27/original.png),
[selected pixels](assets/native-retained-simd-2026-10-27/selected.png),
[8x difference](assets/native-retained-simd-2026-10-27/difference-8x.png) and the
[full-screen demo capture](assets/native-retained-simd-2026-10-27/full-screen-demo.png)
are retained for inspection. Foreground/edge statistics, binary and fixture
hashes, scalar/NEON parity, all timing repetitions and matched input traces are
in the [measurement record](data/native-retained-simd-2026-10-27.json).

## Verification and evidence

`bash scripts/check.sh` and `OTRIOS_CHECK_FIXED_UI_MATH=1 bash scripts/check.sh`
pass. Both run the real process-boundary tests with local Unix sockets available.
All 112 fixed dashboard unit cases also pass on the actual ARM device. A
separate Cortex-A7 build with loop/SLP vectorization disabled produces
byte-identical fixed meshes to the NEON and host producers in all six cases. Existing
Canvas/input tests and real Chromium momentum, caret follow, resize/navigation
and client-local state checks pass.

Original float fixtures/captures remain untouched. The six existing native float
reference captures at both rotations are byte-identical after the retained CPU
and scissor changes, before the alpha atlas experiment. Real Chromium comparisons
at DPR 1/2/3.2 cover 90 frame pairs: maximum channel error one, or two for the
hierarchy case. The original dense v3 float mesh remains SHA-256
`fe0798d1bb16d0abea9b68c14166cb2b6c4d8d349f8860b88c039503cebccce6`.

Raw binaries, per-phase timing, reports, PPM reference/output/difference captures
and browser captures were originally kept under ignored `target/native-retained-fixed/`.
After retirement they are preserved in the verified compressed
`target/renderer-cleanup/qualification-evidence.tar.gz`, with a per-file hash
manifest; the independent original baseline remains in place. A separate verified
archive preserves the completed remote trial files. Trials hold an exclusive
display lock and restore the prior console mode and dashboard service.

## Deployment

Qualified source commit `0e73043316b464031401b1f6d14c0423904776c4` is deployed as
signed release `e1ec066f12b2deb14c3ada24ee1401190026d89301879d045a27e95c9d3acfce`.
The running native executable matches the exact measured signed-build hash.
Authority, native dashboard and tunnel are active, state is confirmed, release
identities agree, and calibration/rotation remain intact. The public conformance
check passes all 99 anonymous/isolation checks; real Chromium passes the deployed
demo's momentum, cancellation, client-local state, portrait and navigation checks.
See the [deployment record](data/native-retained-deployment-2026-10-27.json).
Operational rollback files are preserved; canonical state was not restored.


## Pilot retirement and cleanup

The user requested removal of the fixed-point pilot after comparing complete
frame costs. The fixed coordinate, hierarchy, glyph packing and composition
modules, feature/check mode, dedicated benchmark and alternate GLES position
formats are removed. The selected float renderer retains viewport scrolling,
range culling, layout/editor retention, canonical font scale caching, immutable
snapshots and adaptive alpha coverage storage. Generic profiling and native
pixel probes remain; the browser comparator is now `compare-text-browser.mjs`.
Canonical scale uses cached f64 intermediate arithmetic and a final f32 result,
without depending on the retired fixed module.

All six original float mesh streams and font packets remain byte-identical on
both the host and Cortex-A7. The cleaned renderer passes 54 native dashboard
runtime tests, seven GPU cases in seven storage/sampling/color configurations,
and 60 native same-fixture frame pairs. Twelve sampled native captures at both
rotations are byte-identical to the previously qualified alpha-reference
captures. Real Chromium passes 90 pixel pairs at DPR 1/2/3.2 with zero error.
`bash scripts/check.sh`, native platform Clippy and the staged browser/native
build pass. The sandbox initially blocked local sockets in the process-boundary
tests; the complete required checks pass with local socket access.

Three dense native replays measure 56.647 / 56.411 / 56.647 kernel-presented fps.
The selected default ARM CPU probe measures coasting p50 740–751 us and p95
796–1038 us; rebuild p50 is 10.684–10.727 ms and p95 11.230–11.994 ms. This is
cleanup verification, not a new SIMD speedup or a 60 fps lock.
The exact signed full-screen build first measured 37.647 fps. A subsequent paired
run measured 41.432 fps for the preserved deployed executable and 42.070 fps for
the cleaned executable, with the same momentum gesture controller and RGB565
scanout. The slower first run is retained in the evidence; its cause is not
established. Both cleaned runs pass the 30 fps gate, and the paired result does
not reproduce a cleanup regression. Native dragging and coasting on both axes
remain functional with no pending state edits.


The original baseline directories are untouched. Old generated trial files are
preserved in compressed archives with verified per-file SHA-256 manifests;
148 completed remote files (121,725,440 bytes) were removed individually only
after verification. Local unpacked evidence was reduced by 343,073,224 bytes
net of its archive. No canonical state or operational rollback was removed.
The [cleanup evidence record](data/native-renderer-cleanup-2026-10-27.json)
records the selected artifacts and acceptance results. Future SIMD work can
operate on float data; adoption requires a measured batch and complete-frame
benefit under the same pixel gates.


Cleanup source `d3eb5e4475e70e8ade6a1b071772f446e27a43ce` is deployed as signed
release `1a66f19cea17426e058f34bd19caddc2b5e536f9312ba1c677bf63a7a0ec6bab`.
The running native executable and authority match the staged hashes. Authority,
native dashboard and tunnel are active; state is confirmed, release identities
agree, pending edits are zero, renderer overrides are absent, and touch
calibration/rotation remain intact. All 99 public/isolation checks and deployed
Chromium momentum/cancel, client-local state, portrait and navigation checks pass.
Authenticated login is outside the public-only check. The previous release is
preserved as an operational rollback; canonical state was retained. The final
93 trial files (61,299,050 bytes) were removed only after matching their hashes
to local copies or the archive of remaining trial logs. The ARM build container
was restored to its original stopped state.

The cleaned build's raw probes, selected executable, captures, reports and trial
controllers are preserved in ignored
`target/renderer-cleanup/current-trials-evidence.tar.gz` with per-file hashes;
unpacked generated copies and temporary experiment controllers are removed.
