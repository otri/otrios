<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0006: Font resources and text presentation

Status: scoped implementation requested by the user. Resource admission, metrics,
shared quadratic rendering and bounded coverage caching are implemented for the
browser and native GLES hosts. Actual pixel checks and frame timing are recorded
in [the validation report](../experiments/slug-font-validation.md). This extends the rendering contract in RFC 0001 and the
app/resource ownership in RFC 0004. It does not complete the Canvas migration.

## Ownership and the small entry point

A font is an immutable resource referenced by content hash. Glyph outlines,
metrics and license notices belong in that resource, not executable code or an
app's state document. The entry point initializes a generic platform host,
resource provider and presentation engine, then requests the root app. Font
admission, decoding, GPU preparation and text rendering are shared services;
individual apps do not acquire their own platform font parsers or shaders.

The target bootstrap is a trusted Home app reference followed by admission of
its UI, State, behavior and resource dependencies, then `Canvas(uiRef, stateRef)`.
The graph supplies font references just as it supplies executable or image
references. Policy must permit access before private resource bytes are served.
A content hash alone is not a capability or trusted provenance.

Today the deployed dashboard still starts its legacy Shell. A generated Rust
reference and browser module identify the initial Roboto resource; they contain
only its filename and SHA-256, not font data. Native host setup loads the resource
through `font_resource::load`; browser setup uses `loadFontResource`. Both give
verified bytes to the same safe Rust `Font::from_zstd` admission path. The native
host retains `Arc<Font>`, and the browser retains its equivalent worker/runtime
owner. `Shell::set_font` supplies shared metrics. The generic GPU host receives
the admitted font atlas independently of app state.

These are transitional bootstrap references. Moving them into an admitted Home
resource graph removes the font-specific entry dependency; replacing it with
embedded glyph arrays or app-specific loading branches would move away from the
target pattern. The existing standalone Canvas app prototype and signed live
release host are still distinct boot paths.

## Immutable transport and cache

The initial resource is `fonts/Roboto-Regular.otq.zst`. The signed release manifest
binds its byte count and SHA-256, and host-generated references bind the expected
identity. Verify that identity before decompression. Invalid or missing required
font data fails activation explicitly, rather than substituting a host-installed
font or the old bitmap alphabet.

Compressed font bytes use the existing immutable code/blob cache, with the same
approximately 512 MiB web / 2 GiB native budgets. They are evictable and
re-fetchable. The native release agent places admitted bytes beside the cached
executable; the browser artifact cache admits the signed resource path. These
transport caches are separate from the GPU coverage cache described below.

The private `OTRQFNT1` packet holds line metrics, sorted Unicode-to-glyph records,
original glyph IDs, advances, signed ink bounds, contiguous quadratic curves and
the complete OFL notice. It retains TrueType quadratic outlines; it contains no
CPU-tessellated glyph triangles or pre-baked image atlas. Unknown scalars use the
font's glyph ID 0. The packet is a bounded prototype format, not the proprietary
`.slug` format or a stable public font ABI.

Admission checks compressed identity, a single complete Zstandard frame, known
length, checksum, bounded history window, exact packet lengths, valid metrics,
curve ranges and ink bounds. Limits are 1 MiB compressed, 2 MiB decoded/window,
1,024 glyph records, 16,384 curves and 128 curves in one shader band. Memory
sensitive frame-header fields are checked before creating the decoder. The pure
Rust decoder is pinned to `ruzstd` 0.8.2; core runtime code retains its unsafe ban.

Current Roboto Regular is 34,043 bytes compressed and 76,530 bytes decoded, with
213 glyph records and 5,440 curves. Source identities, the full packet layout,
licenses and reproducible generation are recorded in the
[font resource guide](../../assets/fonts/roboto/README.md). Generated resource
metadata does not replace signature verification or server access policy.

## Line boxes, glyph ink and clipping

The font supplies units per em, ascent, descent, line gap, cap height, per-glyph
advance and signed ink bounds. Font coordinates use positive y upward and a
negative descent. The current packet uses half-font-unit precision, with 4,096
units per em. Presentation converts these metrics into logical coordinates once
per text run.

The current UI's text scale retains an eight-logical-unit capital height at
scale 1. That compatibility unit is not a general typography point-size system.
The baseline comes from ascent; line height uses ascent minus descent plus line
gap. Advance moves the pen. Ink bounds locate the drawn outline, including
accents, descenders, negative left bearings and right overhang.

A text frame's logical advance limit does not cut glyph ink at its x edges, and
the old eight-unit bitmap height is not a vertical clip. Explicit ancestor Canvas
clipping controls field and panel overflow. Clipped glyph quads interpolate their
font coordinates so clipping does not stretch the remaining glyph. Tests cover
`j` in a one-unit-wide frame, `g`/`y` descenders and accented capitals.

Text measurement is read-only presentation work. It must not write CRDT state,
restart an animation clock, preserve touch trails or add per-frame storage writes.

## Shared analytic path

The core parses and prepares each admitted font once. `Font::atlas()` provides
normalized quadratic control points and accelerated horizontal/vertical band
lists. Two RGBA float texels describe each curve. Each glyph has eight bands in
each direction. RGBA8 metadata stores `(count,listX,listY,0)` and list entries
store `(curveX,curveY,0,0)`, keeping every fragment address component byte-sized.
The current atlas is 256×43 curve texels and 256×94 band texels, with at most 31
curves in a band. Adjacent bands overlap by 1/1,024 em to conservatively include
boundary curves when native half-float coordinates round. The shared CPU
preparation avoids separate Rust and JavaScript font implementations.

`render_scene_with_font` emits retained geometry plus indexed glyph quads and an
ordered drawing plan. Each glyph has four vertices; the plan preserves geometry
and text painter order. The fragment shader calculates quadratic ray
intersections, winding and antialiased coverage. It does not calculate a signed
distance field or tessellate outlines into glyph triangles.

The canonical GLSL ES sources live under `otrios-linux-display/shaders`;
`generate-slug-shaders.py` produces the browser source module from them. The
GLES2 port follows Eric Lengyel's
[reference Slug shaders](https://github.com/EricLengyel/Slug), with the algorithm
explained in the [2017 JCGT paper](https://jcgt.org/published/0006/02/02/).
This repository includes the reference shader port, not the commercial Slug text
layout library. The port preserves Eric Lengyel's copyright and the selected
[MIT terms](https://github.com/EricLengyel/Slug/blob/main/LICENSE-MIT) in
[Slug-LICENSE.txt](../../crates/otrios-linux-display/shaders/Slug-LICENSE.txt).
Roboto independently retains its OFL terms; covered core code remains LGPLv3.

The frame's Mali-400 probe reports 23-bit fragment precision, but actual real-font
rendering exposed its [documented Lima hardware limit](https://docs.mesa3d.org/drivers/lima.html):
fragment arithmetic is FP16 and `highp` does not change that. Large linear atlas
indices lose bits, and the original 2^-16 guard can flush to zero. The GPU layout
therefore uses separate x/y bytes, while numeric guards are normal FP16 2^-14. The ray solver uses local
power-of-two scaling and cancellation-resistant roots; nonpositive discriminants
retain equal-root cancellation. This protects diagonal strokes and small curves
without a per-frame CPU font path.
Full-float curve uploads are unsupported; native curves use RGBA16F. Browser
hosts can use full-float or half-float textures. Precision reports alone are not
an acceptance test for either correct glyphs or achieved frame rate.

## Bounded reusable glyph coverage

Analytic outlines remain the canonical font representation. A presentation owner
may cache the shader's pixel coverage to avoid repeating curve evaluation for
unchanged text. Both hosts allocate a 1,024×1,024 RGBA8 coverage target: **4 MiB**,
with at most 4,096 entries. Only alpha coverage is consumed; tint is applied when
drawing the cached quad. This is disposable GPU state, not a persistent bitmap
font or a second source of font metrics.

A valid cache key identifies the font/resource generation and glyph, effective
pixel size, exact fractional pixel phase, transform and the clipped font-coordinate
mapping. Integer translation can reuse coverage when those inputs remain equal.
Tint and text opacity are excluded from the key, allowing the same coverage to
be drawn in different colors. Glyph identity is currently carried through the
admitted font's band/curve reference rather than a second character lookup.

Exact phase is required: quarter-pixel key quantization must not silently reuse
coverage computed for a different phase. Rotations, non-axis-aligned quads,
unsupported transforms, oversized glyphs or an exhausted working set use direct
analytic rendering when the cached representation is unsuitable. A new font,
shader/coverage generation or graphics context invalidates cached coverage and
all dependent UV buffers. Eviction/reset happens before drawing dependent runs;
no draw may retain stale atlas coordinates.

The current cache uses bounded shelf allocation and a generation reset when
full. It is not an LRU implementation. Once prepared, unchanged retained scenes
reuse their cached buffers. Cache hit/miss counters describe preparation/reuse;
they are not a count of network cache hits or independently measured GPU cost.
The 4 MiB bound covers this coverage atlas only: font textures, retained vertex
buffers, the scene color target and scanout buffers have separate allocations.

## Coverage and color contract

Coverage is a linear scalar in [0,1]. It is not an sRGB color and is never gamma
encoded in the coverage atlas. Cached texels are premultiplied white `(a,a,a,a)`,
including transparent black at zero coverage. Tint remains outside the cache key:
the draw shader applies premultiplied tint, and source-over uses `ONE,
ONE_MINUS_SRC_ALPHA`. Geometry and analytic fallback use the same premultiplied
contract, so alpha is not multiplied twice. Filtering remains well-defined at
transparent edges; baking a colored glyph separately for every tint is unnecessary.
Premultiplication does not itself convert between sRGB and linear light. The production default draws cached glyphs and
geometry directly into the ordinary sRGB-valued framebuffer, without a full-screen
linear target or conversion pass. This follows the user's performance decision
after the native full-screen conversion proved too expensive. It uses conventional
framebuffer alpha blending; it does **not** claim exact linear-light compositing
of text edges over arbitrary backgrounds. A coverage-only gamma tweak cannot make
that claim either, because the correct output depends on both colors.

An opt-in reference path decodes authored sRGB colors before source-over
composition into a linear RGBA8 scene target, then encodes RGB once for output.
Enable it with native `OTRIOS_LINEAR_SCENE=1` or browser `?linear-light=1`;
`?font-probe=1` selects it for the pixel comparison. Alpha remains linear.
The output transfer uses an exact 256-entry table for the 8-bit linear target;
tint decoding happens per vertex. White at 50% coverage over opaque black reads
approximately byte 188 in this mode; direct conventional blending reads 128.
Both modes preserve the same font geometry, baseline and grayscale coverage.
The reference target can quantize dark colors or repeated translucent layers;
it is not an HDR pipeline, and it carries a measured native performance cost.

A future fast, gamma-correct text path needs either hardware support for linear
blending into sRGB targets or a bounded local composition design with explicit
background information. An atlas alone does not solve this color-space boundary.

Coverage is grayscale. There is no RGB/LCD subpixel coverage, assumed physical
subpixel ordering or host-specific colored text fringe. The same logical scene
may rotate onto the portrait panel or render in a browser without changing its
font asset or coverage semantics.

## Acceptance and remaining work

Core tests exercise digest/checksum failure, truncation, malformed bounds/ranges,
extra frames, oversized decompression/window requests, fallback IDs, line/ink
metrics, band bounds, ancestor clipping and painter order. Browser host tests
exercise retained ordering, cache/tint reuse, changed phase and the sRGB transfer.
Mocked graphics tests are not pixel or refresh-lock evidence.

Actual browser and native pixel checks now compare direct/cached coverage,
fractional phases, tint alpha, holes, font replacement and the two color modes.
The validation report records the real frame's 64-object / 30 Hz load trial,
including warm-up, cache statistics and missed vblanks. This remains a bounded
Roboto prototype, not a full font conformance suite or panel-refresh-lock claim.
Additional faces, transformations, complex shaping and context-loss recovery
still need broader acceptance before general typography support is claimed.

The SDL recovery renderer cannot consume these glyph quads and is quarantined.
Slug releases target the direct GLES host and browser, without publishing a new
SDL recovery executable. Keep the prior validated signed release for rollback;
SDL build/geometry checks do not verify glyph display or this color contract.
Any future SDL recovery must implement its text adapter before claiming rendering
parity. Do not silently start a geometry-only SDL host for a Slug release.

The current subset does not implement OpenType shaping, kerning, ligatures,
combining-mark placement, bidirectional layout or fallback font stacks. Mapping
Unicode scalars directly to the bounded glyph set is a prototype limitation,
not a complete text engine. A future shaping service should produce explicit
glyph IDs and positions while reusing these font resources and presentation
contracts.

The renderer currently admits at most 16,384 glyph quads in one scene buffer,
using 16-bit indices, and silently omits further quads. This is an explicit
prototype limitation to replace with admission failure or bounded batches before
arbitrary app content is supported. Resource/atlas validation does not make that
scene overflow policy complete.

Complete signed Home graph admission and app-by-app Canvas migration before
removing the legacy bootstrap; then remove fixed initial-font references from
entry code. General resource selection, text layout and lifecycle belong in the
runtime and app contracts, not in an expanding collection of host exceptions.
