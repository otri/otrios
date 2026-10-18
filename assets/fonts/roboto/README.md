# Roboto Regular

The shared native GLES and browser WebGL renderer uses Roboto Regular vector outlines instead of the former 8×8 bitmap alphabet. The same triangles and proportional advance metrics are used by both hosts; no platform font availability, font downloads, runtime font parser, or new GPU API is involved.

Source files were retrieved on 2026-10-19 from the Google Fonts repository:

| File | URL | SHA-256 |
| --- | --- | --- |
| `Roboto.ttf` | https://raw.githubusercontent.com/google/fonts/main/ofl/roboto/Roboto%5Bwdth,wght%5D.ttf | `d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134` |
| `OFL.txt` | https://raw.githubusercontent.com/google/fonts/main/ofl/roboto/OFL.txt | `061402327a96aadb0bfb694a960ed289ecd38d383e396243831ab81feb109c41` |

These font files and their derived outline subset retain the SIL Open Font License 1.1 and Roboto copyright. The generated Rust data carries the full notice, which is also embedded in its runtime byte table and available through `dashboard::font_notice()`.

Regenerate from the pinned source with Python 3.9 or later:

```sh
python3 -m venv work/font-tools
work/font-tools/bin/pip install -r tools/fonts/requirements.txt
work/font-tools/bin/python tools/fonts/generate_roboto.py
work/font-tools/bin/python tools/fonts/generate_roboto.py --check
```

The checked-in generated file is `crates/otrios-core/src/dashboard/roboto_data.rs`. Ordinary Rust builds and the frozen deployment source do not need Python, the original TTF, or these generation dependencies. The generator verifies the source hash, instantiates weight 400 and width 100, unions overlapping contours, flattens curves with a six-font-unit tolerance, triangulates holes, and checks filled triangle area against each outline before emitting deterministic little-endian geometry. The generated format is a private asset representation, not a public font or UI ABI.

The bounded subset includes printable ASCII, Latin-1 characters present in Roboto, common punctuation, available directional arrows, and a minus sign. Unsupported Unicode scalars use `?`. Complex-script shaping, kerning, bidirectional layout, fallback font stacks, and general Unicode text layout remain future work; this replaces the demo font without claiming those features. Text scale 1 has an eight-logical-unit capital height, with proportional spacing and descenders beneath that baseline. The existing scene rectangle clips glyph triangles as it does other primitives.
