# Canonical glyph scale and raster comparison — 2026-10-27

The raster and 30 fps gates now pass in the opt-in experimental path. The full
fixed point milestone remains open: hierarchy transforms, viewport clipping and
culling, and flow composition still need migration and qualification. No default
enablement or deployment occurred. The [failed expansion experiment](fixed-point-raster-followup-2026-10-27.md)
and original float32 fixtures remain unchanged.

The high-DPR failure came from deriving the atlas's em size from rounded quad
coordinates. At DPR 3.2 nine glyph keys differed by 1/256 physical em between
float32 and fixed placement, despite identical cache-miss counts. The shared Rust
compositor now supplies a canonical logical em per retained draw, independent of
quad rounding. Actual atlas readback hashes match between the two arithmetic
paths at every tested DPR. The new local probe stream is OTRTEXT3; readers retain
v1/v2 support. This is diagnostic/presentation metadata, with no state wire or
release-control protocol change.

WebGL corrects projected vertex phase and UVs in the vertex shader while retaining
the original CPU quad bounds. Native Mali retains its simple cached vertex shader
and original quad bounds: an attempted vertex correction still measured 28.32 fps,
so that variant was discarded. Canonical scale metadata alone passes its pixel
gate. Neither accepted path expands draw rectangles. Cold atlas entries have
three physical pixels of clear padding; sampling-domain checks fall back to the
analytic path if filtering or correction could reach a neighboring entry. The
atlas remains 1024² RGBA8, **4 MiB**. Mixed cached/analytic runs preserve order and
only submit the visible draw range.

| Workload | Mali maximum channel error | Chromium maximum, DPR 1/2/3.2 |
| --- | ---: | ---: |
| Dense | 6 | 1 |
| Small | 4 | 1 |
| Nested scale | 7 | 1 |
| Large | 4 | 1 |
| Input Lab | 5 | 1 |

Each workload compares frames 0, 1, 17, 59 and 119; native comparisons include
rotations 0 and 90. The unchanged maximum-error gate is 8. Raw results include
changed pixels, foreground mean errors and edge-only mean errors, avoiding an
average diluted by empty background. Native pixels use RGB888 headless readback;
the scanout measurements below use the frame's actual RGB565 surface. These
comparisons hold the filtered coverage/color path constant between arithmetic
variants; they do not assert identity to exact analytic coverage.

[Native reference](assets/canonical-dense-mali-reference.png),
[candidate](assets/canonical-dense-mali-candidate.png), and
[8x difference](assets/canonical-dense-mali-difference-8x.png) save the selected
worst dense frame at rotation 90. [The high-DPR browser strip](assets/canonical-dense-browser-dpr3.2.png)
stacks reference, candidate and 8x difference.

Three isolated, kernel-timestamped dense scrolling trials measured **35.63,
35.53 and 35.07 fps** after 120 warm-up and 240 sampled presentations. Each uses
28 cold atlas entries, about 2,781 visible of 5,040 retained glyphs, and the same
4 MiB atlas. No CPU layout or network time is included in this replay benchmark.
The immutable float32 geometry and viewport bytes in all ten new metadata-bearing
fixtures round-trip identically to the committed v2 fixtures.

[Pixel metrics and fixture provenance](data/fixed-point-canonical-scale-2026-10-27.json)
and [scanout trials with binary/fixture hashes](data/fixed-point-canonical-scale-scanout-2026-10-27.json)
are retained for review. Reproduce with `text_scroll_frames --styles`, the browser
comparison's `--stable-quads --dpr 1,2,3.2` options, and native
`OTRIOS_TEXT_STABLE_QUADS=1`. Both modes remain disabled by default.

`bash scripts/check.sh` passes with default features and with fixed math enabled.
Eleven mock WebGL tests pass. The actual Mali suite passes six tests at each of
2x, 3x and exact sampling. Real Chromium input tests pass cursor recovery,
preview dragging/coasting, and typing after panning; the shared layout integration
passes both orientations, reflow during capture, and two-client interaction.
These are dashboard rendering checks, not Linux shared-memory PoC acceptance.
