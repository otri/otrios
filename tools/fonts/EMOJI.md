<!-- SPDX-License-Identifier: Apache-2.0 -->
# Monochrome emoji resource

The separate OTEMO002 resource contains Unicode 17.0 emoji sequences rendered
from OpenMoji 17.0.0 monochrome artwork. OpenMoji – the open-source emoji and
icon project – is designed by [OpenMoji authors](https://openmoji.org) and
licensed under [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).
The images were resized and converted to alpha masks. The license applies to the
artwork and derived art resource, not the runtime code. The source archive,
license, attribution, and generated identity are in `assets/fonts/openmoji`.
Unicode emoji-test data and its license are in `assets/fonts/unicode-emoji`, from
https://unicode.org/Public/17.0.0/emoji/emoji-test.txt.

Every fully qualified, minimally qualified, and unqualified Unicode 17 emoji
sequence maps to a mask. Skin-tone modifiers are ignored for display, without
changing message text or editing; standalone tone components are ignored. Hair
components remain supported. Twelve mixed-tone multi-person normalized forms
without neutral source art map to the semantically equivalent neutral or
gendered base, listed explicitly in `generate_emoji.py`. The OpenMoji black
image alpha channel is resized to 64×64. The OpenMoji U+1F195 black SVG and PNG
are empty upstream, so that mask is rasterized with Pillow from the pinned Noto
Emoji OFL font. The generator rejects every other empty output mask. Public
attribution and licenses are distributed in Emoji-NOTICE.txt, Emoji-LICENSE.txt,
and OFL.txt. Alpha-only tiles use the authored text
color at render time. Source PNGs are build inputs, not runtime parsers.

The little-endian resource starts with `<8sIIII`: magic `OTEMO002`, total bytes,
sequence count, glyph count, and sequence-table byte length. Sequence records,
sorted by UTF-8 byte order, are `<HHI` (UTF-8 byte length, zero, glyph ID)
followed immediately by the UTF-8 bytes. The fixed 24-byte glyph records, sorted
by glyph ID, are `<IIIHHhhhH`: glyph ID, absolute payload offset, compressed
length, width, height, bearing X, bearing Y, advance, zero. Each payload is an
independent checksummed Zstandard frame decoding to exactly 64×64 alpha bytes.
Bearings are zero and advance is 64; layout uses a uniform emoji em.

The runtime compiles the compact `Emoji.index`, comprising the resource header,
sequence table, and glyph table; its header retains the full resource byte length
and glyph offsets address the complete `Emoji.otemo`. Hosts load the full resource
as a separately signed, hash-verified release asset. Glyphs are decoded on demand;
hosts insert 64×64 tiles with a two-pixel transparent gutter into up to four
1024×1024 GPU atlas pages. A bounded least-recently-used cache evicts old tiles
as needed. Startup does not decode or upload the full emoji atlas.

Use Python dependencies from `emoji-requirements.txt`:

```sh
python3 tools/fonts/generate_emoji.py
python3 tools/fonts/generate_emoji.py --check
```


Focused validation covers every one of the 3,944 fully qualified sequences,
all 1,918 nonempty masks, grapheme boundaries, neutral skin-tone aliases, and
wrapped document geometry. The browser cache regression covers lazy allocation,
deduplication, eviction across four pages, and immutable source UVs. An actual
Frame headless EGL/GLES pixel probe verified authored tint, mask alpha, text
opacity, transparent gutters, and repeated tile reuse. A focused release test
covers signed asset publication, resource limits, native caching, and notices.
Broad browser/platform campaigns are intentionally outside the normal loop.

The compressed resource is about 2 MiB, separate from the binaries; only its
151 KiB sequence/offset index is compiled in. GPU allocation starts at zero and
is bounded to 4 MiB. Masks are 64-pixel rasters; ordinary text retains the
analytic renderer. No runtime font parser or shaping library was added.
