<!-- SPDX-License-Identifier: Apache-2.0 -->
# Slug font prototype validation — 2026-10-19

This prototype replaces embedded tessellated Roboto glyphs with admitted,
compressed quadratic outlines, shared analytic Slug shaders and a disposable
coverage texture cache. It covers the existing dashboard and the separate
Canvas app host. It does not complete their app-loader migration.

## Resource and baseline

The font is 34,043 bytes compressed / 76,530 bytes decoded, containing 213 glyph
records and 5,440 quadratic curves. Its compressed SHA-256 is
`bfcbd60cc3250fa2cb9f7dae9a38c691036efa3ec3a16b01f1d4399c50f1e28b`.

The safe Rust decoder verifies identity before decoding and rejects oversized
windows, invalid frame lengths/checksums, extra frames and malformed metrics or
curve ranges. The only added runtime dependency is pinned pure-Rust `ruzstd`
0.8.2. No new unsafe Rust font boundary is introduced.

Ascent determines the baseline; advances determine logical run width; signed ink
bounds preserve descenders, accents and negative bearings. Ancestor Canvas
clipping controls visible overflow. Text frames no longer clip glyph ink to the
old bitmap alphabet's height. Host setup retains one admitted font and prepared
curve/band textures, rather than decoding or constructing them each frame.

Font bytes share the existing approximately 512 MiB browser / 2 GiB native
immutable cache. Each graphics host has a separate bounded 4 MiB coverage atlas;
that number excludes curve/band textures, retained meshes and framebuffers.
Coverage keys include admitted font generation, glyph identity, exact physical
size/phase and transform. Tint changes reuse coverage. Atlas texels contain premultiplied white `(a,a,a,a)`;
geometry, analytic fallback and cached glyphs use premultiplied source-over.
Premultiplication fixes filtering/compositing representation, not color space.

## Automated and actual browser checks

- Required `scripts/check.sh` passed after the font/resource and host changes:
  92 core unit tests, 11 Canvas integration tests, remaining workspace/process
  checks, release/cache tests and the direct-host source boundary check.
- Strict all-feature/all-target workspace Clippy and
  `scripts/check-canvas-prototype.sh` passed. Both browser Wasm hosts build.
- All three native-worker WebSocket/reconnect/backpressure tests passed on the
  Mac. A first attempted direct-host test filter ran zero tests on macOS because
  that entry point is Linux-only; the native-worker run above executes all three.
- Node checks passed: 22 GPU/cache/release/session tests. The signed release and
  resource-cache suite passed 18 checks, including corrupt cached fonts and
  oversized resource admission. Deterministic font/shader generation checks pass.
- Actual browser Canvas probe at device scale 2 compares cached and direct Slug
  rendering over 817,960 pixels / 39 glyphs. Maximum linear-channel difference
  is 1/255, mean absolute channel difference 0.000682 byte values. After sRGB
  conversion the maximum difference is 6/255 at one dark pixel; this is the
  amplification of one linear-target quantization step, not byte-identical output.
- Actual browser 50%-coverage white over black reads `[188,188,188,255]`.
  Navigation, the loaded Canvas Wasm behavior, the legacy dashboard startup and
  Component Playground render successfully; warm glyph-cache hits increase.

The browser probe uses real GPU readback, rather than a mocked WebGL context.
The unit checks validate caching and retained ordering separately.

## Frame probes and performance

Hardware is the existing Rockchip frame, Mali-400 / Mesa 25.0.7, GLES2,
800 × 1280 RGB565 scanout rotated 90 degrees. Native curves use RGBA16F;
full-float curve textures are unsupported. Fragment high precision and shader
derivatives are available. Test processes use isolated EGL surfaces, or stop the
live service for a bounded on-screen trial and restore it on exit. No two
on-screen dashboard instances are used.

Native pixel probes validate holes, fractional coverage, warm cache reuse,
changed phase and font-generation invalidation. A 50%-white-over-black sample
reads byte 188 with the exact transfer path.

The first integrated implementation used per-fragment powers and failed the
performance gate: the 12-second, 64-tetrahedron / 30 Hz state-load trial measured
6.09 kernel page-flips per second after warm-up (5.48 overall presentation FPS).
It was not published. Moving tint decoding to vertices and replacing powers with
an exact three-fetch byte lookup still made the resolve too expensive.

Controlled full-resolution `glFinish` diagnostics, with the live service still
running, measured approximately 41.7 FPS with no output transfer, 15 FPS for the
exact three-fetch lookup, 18.8 FPS for a two-fetch lookup, 16.7 FPS for the user's
single-square-root suggestion, and 11 FPS for Chilliant's more accurate chained
square-root approximation. These isolate a shader cost; they are not scanout
acceptance or panel-refresh measurements. The suggested formula also changes the
sRGB toe from 12.92 to 2 and maps half intensity to approximately 176 instead of
188. See [Chilliant's original approximation](https://chilliant.blogspot.com/2012/08/srgb-approximations-for-hlsl.html)
and the [sRGB definition](https://www.w3.org/Graphics/Color/srgb).

The user chose the direct atlas path, avoiding the extra full-screen conversion.
It is now the default; exact linear-light rendering remains an opt-in comparison.
The baseline/geometry and coverage cache are identical in both modes. Conventional
default framebuffer blending is not exact gamma-correct blending; no coverage
power is applied as a substitute. The first direct-atlas 12-second exclusive-display trial used 64 rotating tetrahedra and
30 Hz isolated app-state load. After the two-second warm-up it measured **54.54
kernel page-flips per second**, with 17.654 ms median / 17.733 ms p95 flip
intervals and 21 missed vblanks over 9.99 seconds. The nominal panel cadence is
about 56.65 Hz, not 60 Hz. Overall cadence including startup/capture was 48.77
FPS, so this is not a claim of a locked refresh rate. Composition submission was
3.417 ms median / 5.299 ms p95. The cache recorded 119,643 hits / 297 misses,
with 4,194,304 atlas bytes. See [the raw report](slug-font-frame-results.json)
for the candidate binary digest and complete measurements. This was a local
isolated state-load test, not upstream network-latency evidence.

Actual native acceptance passed all three ignored EGL pixel tests in both direct
and linear-reference modes on Mali400. These cover all 256 coverage levels,
holes, fractional phases, cache generation, RGB888/RGB565 output and half-alpha
tints without double alpha. The user-approved direct mode avoids the slower
full-screen target and resolve.

Inspection of that trial's actual Roboto screenshot then exposed fragmented
glyphs on native rotation, despite the synthetic shape tests passing. The timing
above therefore describes a **rejected visual candidate**, not rendering
acceptance. The repair keeps atlas addresses as separate byte-sized x/y values, uses normal
FP16 guards, scales local curve differences by 16, and evaluates quadratic roots
without subtractive cancellation. The persistent compressed resource is unchanged.
See [Lima's documented hardware limits](https://docs.mesa3d.org/drivers/lima.html).
The real Roboto regression now passes at 0° and 90° in both direct and reference
modes: all 187 conservative interior and 764 exterior samples match, and a second
warm-cache draw is byte-identical. All four actual ARM pixel tests pass in each
mode; strict ARM all-target Clippy also passes. Final integrated capture and signed
deployment evidence follow below.

## Explicit limits

- One Roboto Regular subset; no OpenType shaping, kerning or bidirectional text.
- Grayscale analytic coverage at fractional positions; no physical RGB/LCD
  subpixel ordering assumption on the rotated panel.
- Default framebuffer blending is conventional sRGB-valued alpha blending. The
  optional linear RGBA8 reference path quantizes dark colors and translucent
  layers and costs a full-screen pass. Panel scanout is RGB565, not HDR.
- The scene limit is 16,384 glyph quads. Larger scenes require virtualization or
  an explicit larger budget; this prototype drops glyphs beyond that limit.
- Coverage cache pressure can invoke the analytic fallback. Caching is bounded,
  not a promise that every animated phase stays resident.
- The generic signed Home resource graph is still future work; small generated
  references identify the initial font. Hashes do not grant access.
- User-approved new releases omit SDL recovery binaries. Previous validated
  signed releases remain available for rollback. SDL is not claimed to render
  the new glyph stream.

## Repaired integrated candidate

The repaired source (`08025a6`) completed the same exclusive 12-second,
64-tetrahedron / 30 Hz load trial. Kernel page-flips after two-second warm-up:
**54.65 FPS**, 17.654 ms median / 17.675 ms p95, 20 missed vblanks over 9.99 s.
Overall cadence including startup and capture was 48.72 FPS; this is not a
refresh-lock claim. Composition submission measured 3.365 ms median / 4.968 ms
p95. The coverage cache recorded 119,519 hits / 313 misses and 4,194,304 bytes.
Raw measurements and the tested binary digest are in the linked JSON report.
Signed production deployment is recorded after final screenshot review.
