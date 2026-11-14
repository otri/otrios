<!-- SPDX-License-Identifier: Apache-2.0 -->
# Shared monochrome fallback repertoire

`generate_roboto.py` builds one bounded quadratic packet for the shared native and
browser analytic font renderer. Roboto supplies its existing Latin/punctuation
repertoire. Missing requested scalars use pinned, unmodified OFL font sources:

| Source | Repository revision | Local source |
| --- | --- | --- |
| Noto Emoji, weight 400 | Google/fonts `b979dba422e445492b0eb9951ac52ee0b4d648c3` | `assets/fonts/noto-emoji/NotoEmoji.ttf` |
| Noto Sans Symbols 2 | Google/fonts `7b6724ac7ececc713e9ba93af309f7520c9a80a3` | `assets/fonts/noto-symbols/NotoSansSymbols2-Regular.ttf` |
| Noto Sans Symbols, weight 400 | Google/fonts `caa8c83707a5797d62fd02f14301391986f21d75` | `assets/fonts/noto-symbols/NotoSansSymbols.ttf` |

Each source directory's upstream JSON pins its original repository path, font
SHA-256 and license SHA-256. The generator checks font hashes before opening them.
Each original copyright/OFL notice is retained alongside the source, concatenated
into the packet notice and generated public `web/shared/fonts/OFL.txt`. Original
font files are build sources, not runtime font parsers or per-frame dependencies.

The reviewed set includes all 80 U+1F600–U+1F64F emoticons plus explicit newer
expressions, hands (including thumbs-up), hearts/reactions, celebration, rocket,
light bulb and crab. The exact categories are readable Unicode strings in the
generator. Ordinary arrows U+2190–U+2199 and selected circles, squares, triangles,
stars and checkbox/check marks are also requested. Every requested emoji/symbol
scalar must exist in the source chain: unsupported requests fail generation
rather than silently disappearing under a resource cap. The exact fallback
scalar lists are emitted in `assets/fonts/roboto/resource.json`.

All outlines and advances normalize to Roboto's EM and baseline. Glyph-ID ranges
are disjoint between source fonts; each font can retain its own cmap aliases.
U+FE0E/U+FE0F variation selectors have explicit zero-curve, zero-advance entries.
The analytic packet supports these selectors without replacement boxes but does
not itself shape sequences. Unicode 17 emoji sequences, including ZWJ families
and flags, use the separate monochrome OpenMoji resource described in
[EMOJI.md](EMOJI.md). Text presentation uses the analytic font where available.
Full script shaping remains outside this font contract.

The generated packet has 360 glyph records and 18,254 quadratic curves. It remains
within 1,024 records, 32,768 curves (two texels each in a 256×256 atlas), 512 curves
per glyph and 128 curves per band. Its 65,099 band texels fit the existing 16-bit
addressing contract; no shader loop or texture addressing expansion is needed.
Compressed resource: 104,278 bytes; decoded: 248,164 bytes. The generator checks
aggregate curve, per-glyph, per-band and band-texture limits before producing its
packet. Each band curve reference consumes one RGBA8 texel, matching the runtime.

Use existing `tools/fonts/requirements.txt` (FontTools 4.60.1) and Zstandard 1.5.6:

```sh
python3 tools/fonts/generate_roboto.py
```

A generated resource update also refreshes the Rust/JavaScript resource hashes.
