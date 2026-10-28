# Roboto Regular resource

Roboto Regular is a separate, content-addressed, Zstandard-compressed quadratic outline resource. Font bytes are not built into executable code. Both the native GLES and browser WebGL hosts load the same packet, verify its admitted SHA-256, decode and prepare it once, then retain its immutable font metrics and GPU atlas. The existing bounded artifact cache holds the compressed resource alongside code and blobs.

The renderer emits an indexed quad for each glyph. Shared Slug shaders evaluate its quadratic curves at presentation time using derivative-based antialiasing and horizontal/vertical band acceleration. Font decoding and curve/band preparation occur once per admission. A GPU cache miss rasterizes a glyph into the coverage atlas; later frames reuse it. There is no CPU glyph triangulation. The native GLES host can upload the shared atlas as half-float curves when full-float texture support is unavailable.

Source files were retrieved on 2026-10-19 from the Google Fonts repository:

| File | URL | SHA-256 |
| --- | --- | --- |
| `Roboto.ttf` | https://raw.githubusercontent.com/google/fonts/main/ofl/roboto/Roboto%5Bwdth,wght%5D.ttf | `d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134` |
| `OFL.txt` | https://raw.githubusercontent.com/google/fonts/main/ofl/roboto/OFL.txt | `061402327a96aadb0bfb694a960ed289ecd38d383e396243831ab81feb109c41` |

The source font and prepared subset retain the SIL Open Font License 1.1 and Roboto copyright. The complete notice travels inside the compressed font packet and is available through `Font::notice()` after admission.

## Reproduction

Use Python 3.9 or later, FontTools 4.60.1, and Zstandard CLI 1.5.6:

```sh
python3 -m venv /tmp/otrios-font-tools
/tmp/otrios-font-tools/bin/pip install -r tools/fonts/requirements.txt
/tmp/otrios-font-tools/bin/python tools/fonts/generate_roboto.py
/tmp/otrios-font-tools/bin/python tools/fonts/generate_roboto.py --check
```

Generation verifies the original TTF hash and instantiates weight 400, width 100. It retains quadratic TrueType curves, including winding and overlapping contours, and represents straight segments with midpoint quadratic controls. Coordinates and metrics use half-font-unit precision, so `unitsPerEm` becomes 4096 while all normalized geometry remains unchanged.

Generated outputs:

- `web/dashboard/fonts/Roboto-Regular.otq.zst`: the compressed resource, 34,043 bytes; decoded packet 76,530 bytes.
- `assets/fonts/roboto/resource.json`: source/resource hashes, sizes and metric metadata for review.
- `crates/otrios-core/src/dashboard/font_reference.rs` and `web/shared/font-reference.mjs`: small resource identity references, containing no glyph bytes.

Normal Rust builds require none of the generation dependencies or the original TTF. The pure Rust `ruzstd` decoder is pinned to 0.8.2, enabling the same resource admission in Wasm and native code without a C compression binding.

## Private packet contract

`OTRQFNT1` is a bounded prototype resource format, not a stable public font ABI. Integers are little endian. The 40-byte header contains total decoded size, units per em, signed ascent/descent/line gap/cap height, a reserved zero, glyph count, curve count, notice length, and fallback glyph ID. Each sorted 32-byte glyph record contains Unicode scalar, original glyph ID, contiguous curve start/count, advance, signed ink bounds and reserved zeros. Each 12-byte curve contains three signed `i16` point pairs. The UTF-8 license notice follows the curves; no trailing data is permitted.

`Font::from_zstd` requires a known admitted SHA-256 and verifies it before decompression. It rejects compressed resources over 1 MiB, decoded content/windows over 2 MiB, unknown decoded lengths, dictionaries, extra frames, invalid checksums, invalid metrics, malformed ranges, glyph bounds that exclude curve extrema, and oversized shader bands. The packet admits at most 1,024 glyph records and 16,384 curves, with at most 128 curves per band and a 16-bit texture address space. Current Roboto uses 213 glyph records, 5,440 curves and at most 31 curves in a band.

`Font::atlas()` holds two RGBA curve texels per quadratic, normalized by units per em: `(p0.xy,p1.xy)` then `(p2.xy,0,0)`. The RGBA8 band texture holds byte-sized coordinates: `(count,listX,listY,0)` metadata and `(curveX,curveY,0,0)` list entries. Shaders never reconstruct a large linear index. Glyph references contain `(metadataX,metadataY,7,7)`. Each glyph has eight horizontal bands followed by eight vertical bands. Horizontal lists are sorted by descending maximum x; vertical lists by descending maximum y. Parallel straight segments are omitted from the corresponding ray direction. Adjacent bands overlap by 1/1024 em to cover boundary rounding on half-float hosts. Both textures are 256 texels wide; current heights are 43 and 94 respectively.

## Metrics and scope

The bounded subset includes printable ASCII, available Latin-1 glyphs, common punctuation, available directional arrows, and a minus sign. Unsupported Unicode scalars select original glyph ID 0 (`.notdef`). Complex-script shaping, kerning, bidirectional text, fallback font stacks, and general Unicode layout remain future work.

Text scale 1 retains the demo's eight-logical-unit capital height. Layout uses explicit ascent, descent and line gap for its line box; glyph ink bounds independently retain accents, descenders and negative side bearings. The old eight-unit bitmap rectangle is no longer a vertical ink clip. Ancestor canvases continue to clip the final quads, with glyph coordinates interpolated to preserve correct coverage at clipped edges.

The runtime ownership, glyph coverage cache, color pipeline and acceptance gates are specified in [RFC 0006: Font resources and text presentation](../../../docs/rfcs/0006-font-resources-and-text-presentation.md).
