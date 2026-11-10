# RFC 0017: Retained text viewports

Status: implemented for the text scrolling performance milestone (2026-10-26).
Amends the coverage-cache policy in [RFC 0006: Font resources and text presentation](0006-font-resources-and-text-presentation.md) at the user's request.

## Problem and representation

Fractional scrolling previously changed each clipped glyph's cache key. Input Lab
reached only 5.1 kernel-presented fps on the RK3126 frame while dragging text.
Reusing an oversampled glyph atlas removes rasterization churn; retaining the
entire text strip also removes per-frame glyph shaping, validation and upload.

`TextPresentation` owns one bounded canonical mesh. Text quads use coordinates
local to their run. A changed value, font, scale, surrounding geometry or clip
hierarchy rebuilds the mesh. Tint uses independent straight RGBA per ordered draw;
color changes preserve mesh identity and coverage resources. Initial preparation
admits visible primitives plus 64 logical pixels of overscan. Pans retain that
working set until uncached content enters the visible clip, then replace it with
a bounded working set. A changed text origin only updates a small
presentation record: visible first/count, translation x/y and clip l/t/r/b.
These eight scalars correspond to each existing ordered draw. Triangle geometry
can use the same translation and clip records; zero records retain legacy fixed
placement. Both GLES and WebGL retain their vertex buffers across these updates.
The browser worker sends a new mesh only when its presentation generation changes;
coalesced viewport updates carry the latest range and transform. A separate
paint revision sends changed RGBA without recopying vertex buffers. Privacy clearing
also drops the retained runtime cache.

Prefix maximum right bounds and suffix minimum left bounds allow two binary
searches per run, including conservative side bearings and ink overhangs. A run
outside either clip axis draws nothing. Hosts submit only the surviving contiguous
glyph range, translate it in the vertex shader, and apply a scissor rectangle in
physical pixels. Rotated native scanout projects the clip separately from the
translated text. Draw ordering and ancestor clipping remain authoritative.

Canonical meshes retain at most 16,384 glyph quads (the existing 65,536-vertex
index limit). Larger documents use the existing bounded visible-mesh path.
Input fields currently admit at most 256 bytes. This does not add persistence,
change CRDT/state bytes, or change the release-control opening envelope.

## Resource reuse after local content changes

[The subsequent resource cleanup](../experiments/oib-resource-reuse-2026-11-10.md)
caches normalized primitives independently, grouping up to 16 ordered draws in a
resource. Editing a label replaces its group while unchanged groups retain their
GPU buffers. Versions are local identifiers, namespaced by layer and reset epoch;
they are not content hashes. Font/mount changes, privacy clearing and graphics
recovery invalidate the appropriate ownership. Aggregate scene/vertex limits
remain unchanged, with at most 32 resources per layer and a conservative 16 MiB
combined UI budget. Oversized, document and reflow candidates retain the compatible
monolithic path. Painter order, clips and semantic input identities remain intact.

[Content invalidation and identity reuse](../experiments/oib-needs-layout-2026-11-10.md)
separate geometry ownership from semantic node IDs. At most one prior painter
ordinal is considered when IDs change, and exact normalized primitive geometry
and scale must match. Current IDs, responders and semantics remain authoritative;
unchanged ordered meshes may retain their GPU resource. Fixed-bound content and
placement changes can update a retained Scene at presentation, while size,
visibility and uncertain dependency changes request full layout. Captured input
may use a separate immutable tree only under an explicit stable-bounds proof.

## Coverage sampling

The disposable 1,024² RGBA8 atlas remains bounded at 4 MiB and 4,096 entries.
Glyphs rasterize at 2× density by default; 3× is a measured diagnostic option.
The analytic shader still integrates coverage over one destination-pixel footprint;
the larger atlas samples that function more densely. Linear texture filtering
reconstructs fractional placements. Coverage stays linear, and tint stays outside
the cache. This is an approximation to evaluating the analytic shader at each
new phase, not pixel-identical phase caching.

Keys include font generation, band metadata and effective pixel size, quantized
to 1/256 pixel per em for stability. Position and visible crop are excluded.
Full glyph bounds come from admitted band transforms, so a clipped edge reuses
the same entry. A bounded recent-entry hint checks metadata and size before reuse;
hash collisions are resolved by full key comparison. These are disposable lookup
indices, never content identities or substitutes for SHA-256.

Font replacement, shader generation and context loss invalidate coverage and
dependent UV buffers. Exhaustion resets the atlas before dependent drawing;
unsupported mappings fall back to analytic rendering. Native scale `0` retains
the earlier exact-phase diagnostic, using direct analytic draws for translated
viewports. Browser `font-probe` selects the reference path. Unfiltered phase-cache
pixels must not be translated as if they were prefiltered coverage.

## Validation and limits

Scalar admission occurs once in the safe Rust display boundary. C still checks
dimensions, allocations, draw ranges and culled ranges. Repeated metadata can
reuse validated values only after bit equality; nonuniform metadata is validated
fully and uses analytic fallback. Cached vertices pack tint as normalized bytes,
and CPU staging/index capacity is reused rather than allocated on every frame.
There is no new unsafe Rust or dependency.

[Measurements and reproduction](../experiments/text-rendering-performance.md)
include kernel-presented FPS, independent CPU timings, native pixel comparisons,
and the intentionally different direct-versus-filtered browser edge pixels.
The float32 retained renderer is the baseline for the separate fixed point/SIMD
stretch milestone. Neither FP16/FP32 shader precision nor LUT cache residency is
inferred from an achieved frame rate.
