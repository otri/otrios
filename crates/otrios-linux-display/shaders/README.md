<!-- SPDX-License-Identifier: Apache-2.0 -->
# Shared Slug shader contract

`slug.frag` ports Eric Lengyel's MIT reference quadratic coverage algorithm to
GLSL ES 1.00. It keeps root eligibility, horizontal/vertical quadratic ray
intersections, nonzero winding, band pruning and weighted dual-ray coverage.
It does not use a distance field, bitmap source font or tessellated outlines.
See `Slug-LICENSE.txt` for attribution and terms.

The native build embeds these canonical files automatically. Run
`python3 scripts/generate-slug-shaders.py` after shader changes to update the
browser module and its license notice; `--check` verifies parity.

## Immutable resource and retained geometry

- Curve texture: two RGBA texels per quadratic, containing `(p0.x,p0.y,p1.x,p1.y)`
  then `(p2.x,p2.y,0,0)`, in em units. The host packet is float32; the native
  Mali400 upload converts once to RGBA16F (`GL_OES_texture_half_float`).
- Band texture: RGBA8, each texel containing two little-endian uint16 values.
  A glyph has eight horizontal then eight vertical metadata texels, each
  `(curveCount,globalListOffset)`. Each list item is `(globalCurveTexelOffset,0)`.
  Horizontal lists sort by descending maximum x; vertical lists by maximum y.
  Parallel straight curves are excluded from the corresponding ray direction.
- Four vertices per glyph, ordered top-left, top-right, bottom-left, bottom-right.
  Each vertex has 16 floats: position2, emCoordinate2, tintRGBA4,
  bandTransform4 and glyph4. `glyph=(metadataOffset,7,7,0)`;
  bandTransform scales/offsets em coordinates into the eight bands.
- Indexed triangles use `(0,1,2,2,1,3)`. The draw plan is triples
  `(kind,firstVertex,countVertices)`, with kind0 colored triangles and kind1 text.
  Text ranges align to four vertices. Source order is preserved between shapes
  and text. The GLES2 uint16 index limit bounds a mesh to16,384 glyphs.

The interpreter supplies one admitted font resource before drawing. No font
parser, decompressor, outline traversal or network access runs in the display
loop. Texture/index addressing uses texel centers and nearest filtering.
The shader requires fragment highp and standard derivatives, but no integer
textures, integer bit operations or texelFetch.

## Bounded GPU coverage cache and color pipeline

The native adapter retains a1024×1024 RGBA8 coverage atlas (4MiB). A miss runs
Slug into an atlas cell, storing premultiplied white `(a,a,a,a)`. Subsequent
frames draw indexed glyph quads sampling that coverage. The cache key includes font generation, glyph/band metadata,
em-coordinate bounds, physical size, orientation and exact fractional pixel
phase. Color is applied when composing, so recoloring does not rerasterize.
A cache generation resolves every used glyph before painting. A full atlas
flushes and retries once; oversized glyphs/working sets fall back to direct
analytic Slug. No earlier draw can retain UVs from an evicted generation.

By default, authored sRGB colors and cached glyph coverage draw directly into
scanout. Tint is premultiplied internally, with `ONE, ONE_MINUS_SRC_ALPHA`
blending. The public vertex-color payload remains straight authored RGBA.
Coverage multiplies RGB and alpha once; it is not gamma-adjusted.
There is no extra full-screen color conversion or linear scene target in this
mode. Conventional sRGB compositing is a deliberate performance tradeoff;
it is not exact linear-light composition.

`OTRIOS_LINEAR_SCENE=1` enables the slower diagnostic path: tint decoding runs
per vertex, the scene blends in a linear RGBA8 target, and one final pass uses
an exact256-entry sRGB lookup for the8-bit source channel values. Coverage alpha
is never gamma-adjusted. The8-bit linear target can quantize dark colors; this
is not an HDR compositor. `Display::linear_scene_enabled()` reports the mode.

`Display::glyph_cache_stats()` returns cumulative hit/miss counts and allocated
coverage-atlas bytes. Shader/cache tests use the existing headless Mesa path,
without requesting DRM master or interrupting the live service.

Opt-in full-resolution linear composition and resolve add a measurable fill/texture
cost on the Mali400. Shader pixel acceptance does not imply60FPS acceptance;
measure the integrated compositor separately. Approximate square-root and
piecewise-cubic transfer variants were evaluated outside the implementation
and rejected because they did not improve this GPU's throughput. The opt-in
diagnostic retains the exact transfer table. Default scanout avoids that pass,
while both modes keep the same unmodified coverage alpha. Premultiplication
does not make default sRGB compositing linear-light correct.
