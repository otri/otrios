#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Prepare a bounded quadratic UI resource from Roboto and pinned OFL fallbacks.

Requires tools/fonts/requirements.txt and Zstandard CLI 1.5.6.
Run python tools/fonts/generate_roboto.py [--check].
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess

from fontTools.pens.basePen import BasePen
from fontTools.pens.boundsPen import BoundsPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/fonts/roboto/Roboto.ttf"
OUTPUT = ROOT / "web/shared/fonts/Roboto-Regular.otq.zst"
METADATA = ROOT / "assets/fonts/roboto/resource.json"
PUBLIC_NOTICE = ROOT / "web/shared/fonts/OFL.txt"
RUST_REF = ROOT / "crates/otrios-core/src/dashboard/font_reference.rs"
JS_REF = ROOT / "web/shared/font-reference.mjs"
SOURCE_SHA256 = "d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134"

EMOJI_SOURCE = ROOT / "assets/fonts/noto-emoji/NotoEmoji.ttf"
EMOJI_SHA256 = "de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551"
SYMBOL_SOURCE = ROOT / "assets/fonts/noto-symbols/NotoSansSymbols2-Regular.ttf"
SYMBOL_SHA256 = "7d5fb73b7ca67a6798101741f5d280a3d016a56a197afcd4199dbb57b4b82a21"
ARROW_SOURCE = ROOT / "assets/fonts/noto-symbols/NotoSansSymbols.ttf"
ARROW_SHA256 = "f7e7e04b4a24b6c78893d50cbfd2b2f6cae49617ab047bfef668d252adb128f7"
# Explicit, reviewed scalar repertoire: no budget-dependent omission. Sequences
# (ZWJ families, flags and skin-tone combinations) require a shaping contract.
EMOJI_CATEGORIES = {
    "expressive": "🤣🥰🥳🤔🤗🤖🥺",
    "gestures": "👍👎👏👋💪👌✌🙏",
    "reactions": "❤✅🎉🔥✨⭐⚠❌💯",
    "objects": "🚀💡",
    "animal": "🦀",
}
EMOJI_CODEPOINTS = set(range(0x1F600, 0x1F650)) | {
    ord(char) for characters in EMOJI_CATEGORIES.values() for char in characters
}
SYMBOL_CODEPOINTS = set(range(0x2190, 0x219A)) | {
    0x25A0, 0x25A1, 0x25AA, 0x25AB, 0x25B2, 0x25B3, 0x25B6, 0x25B7,
    0x25BC, 0x25BD, 0x25C0, 0x25C1, 0x25C6, 0x25C7, 0x25C9, 0x25CB,
    0x25CE, 0x25CF, 0x25EF, 0x2605, 0x2606, 0x2610, 0x2611, 0x2612,
    0x2715, 0x2716, 0x2713, 0x2714, 0x279C,
}
DEFAULT_IGNORABLES = {0xFE0E, 0xFE0F}
MAX_GLYPHS, MAX_CURVES, MAX_GLYPH_CURVES, MAX_BAND_CURVES = 1024, 32768, 512, 128


class Quadratics(BasePen):
    def __init__(self, glyphs):
        super().__init__(glyphs)
        self.curves = []
        self.first = None

    def _moveTo(self, p):
        self.first = p

    def _lineTo(self, end):
        start = self._getCurrentPoint()
        if start != end:
            middle = tuple((a + b) / 2 for a, b in zip(start, end))
            self.curves.append((start, middle, end))

    def _qCurveToOne(self, control, end):
        self.curves.append((self._getCurrentPoint(), control, end))

    def _curveToOne(self, *points):
        raise ValueError("This bounded packet accepts quadratic TrueType outlines only")

    def _closePath(self):
        if self.first is not None:
            self._lineTo(self.first)
        self.first = None

    def _endPath(self):
        raise ValueError("Font outline contour must be closed")


def checked_font(path, sha, axes=None):
    if hashlib.sha256(path.read_bytes()).hexdigest() != sha:
        raise ValueError(f"Font source changed; review source and license first: {path}")
    font = TTFont(path)
    return instantiateVariableFont(font, axes, inplace=True) if axes else font


def repertoire():
    return sorted(set(range(32, 127)) | set(range(160, 256))
                  | set(range(0x2010, 0x2028)) | {0x2212, 0xFFFD}
                  | EMOJI_CODEPOINTS | SYMBOL_CODEPOINTS | DEFAULT_IGNORABLES)


def generate():
    version = subprocess.check_output(["zstd", "--version"], text=True)
    if "v1.5.6" not in version:
        raise ValueError("Use pinned Zstandard CLI 1.5.6 for reproducible resource bytes")
    font = checked_font(SOURCE, SOURCE_SHA256, {"wght": 400, "wdth": 100})
    emoji = checked_font(EMOJI_SOURCE, EMOJI_SHA256, {"wght": 400})
    symbols = checked_font(SYMBOL_SOURCE, SYMBOL_SHA256)
    arrows = checked_font(ARROW_SOURCE, ARROW_SHA256, {"wght": 400})
    sources = [(font, 0), (emoji, len(font.getGlyphOrder())),
               (symbols, len(font.getGlyphOrder()) + len(emoji.getGlyphOrder())),
               (arrows, len(font.getGlyphOrder()) + len(emoji.getGlyphOrder()) + len(symbols.getGlyphOrder()))]
    prepared = [(source, source.getGlyphSet(), source.getBestCmap(), offset)
                for source, offset in sources]
    empty_id = sum(len(source.getGlyphOrder()) for source, _ in sources)
    entries = [(0, prepared[0], ".notdef")]
    fallback_codes = {"Noto Emoji": [], "Noto Sans Symbols 2": [], "Noto Sans Symbols": []}
    for code in repertoire():
        if code in DEFAULT_IGNORABLES:
            entries.append((code, None, None))
            continue
        selected = next(((record, record[2][code]) for record in prepared if code in record[2]), None)
        if selected is None:
            if code in EMOJI_CODEPOINTS | SYMBOL_CODEPOINTS:
                raise ValueError(f"Requested fallback scalar is missing: U+{code:04X}")
            continue  # Preserve the admitted Latin/punctuation intersection.
        record, name = selected
        entries.append((code, record, name))
        if record is prepared[1]:
            fallback_codes["Noto Emoji"].append(code)
        elif record is prepared[2]:
            fallback_codes["Noto Sans Symbols 2"].append(code)
        elif record is prepared[3]:
            fallback_codes["Noto Sans Symbols"].append(code)
    if len(entries) > MAX_GLYPHS:
        raise ValueError("Font glyph budget exceeded")
    records, curves = bytearray(), bytearray()
    max_band, band_texels = 0, len(entries) * 16
    for code, record, name in entries:
        if record is None:
            records.extend(struct.pack("<IIIIhhhhhHI", code, empty_id, len(curves) // 12, 0, 0, 0, 0, 0, 0, 0, 0))
            empty_id += 1
            continue
        source, glyphs, _, offset = record
        pen = Quadratics(glyphs)
        glyphs[name].draw(pen)
        bounds = BoundsPen(glyphs)
        glyphs[name].draw(bounds)
        xmin, ymin, xmax, ymax = bounds.bounds or (0, 0, 0, 0)
        factor = 2 * font["head"].unitsPerEm / source["head"].unitsPerEm
        ink = (math.floor(xmin * factor), math.floor(ymin * factor),
               math.ceil(xmax * factor), math.ceil(ymax * factor))
        points = [tuple(tuple(round(value * factor) for value in point) for point in curve)
                  for curve in pen.curves]
        if len(points) > MAX_GLYPH_CURVES:
            raise ValueError(f"Glyph exceeds curve budget: U+{code:04X}")
        for direction in range(2):
            axis = 1 - direction
            step = (ink[axis + 2] - ink[axis]) / 8
            for band in range(8):
                minimum = ink[axis] + step * band
                maximum = minimum + step
                epsilon = 2 * font["head"].unitsPerEm / 1024
                count = 0
                for curve in points:
                    values = [point[axis] for point in curve]
                    if values[0] == values[1] == values[2]:
                        continue
                    count += max(values) >= minimum - epsilon and min(values) <= maximum + epsilon
                max_band = max(max_band, count)
                band_texels += count
        records.extend(struct.pack("<IIIIhhhhhHI", code, offset + source.getGlyphID(name),
                                   len(curves) // 12, len(points), round(source["hmtx"][name][0] * factor),
                                   *ink, 0, 0))
        for curve in points:
            curves.extend(struct.pack("<hhhhhh", *(value for point in curve for value in point)))
    if len(curves) // 12 > MAX_CURVES or max_band > MAX_BAND_CURVES or band_texels > 65535:
        raise ValueError(f"Font atlas budget exceeded: {len(curves) // 12} curves, {max_band} per band, {band_texels} band texels")
    notice = b"\n\n".join(
        title.encode() + b"\n\n" + (ROOT / path).read_bytes()
        for title, path in [
            ("Roboto", "assets/fonts/roboto/OFL.txt"),
            ("Noto Emoji", "assets/fonts/noto-emoji/OFL.txt"),
            ("Noto Sans Symbols 2", "assets/fonts/noto-symbols/OFL.txt"),
            ("Noto Sans Symbols", "assets/fonts/noto-symbols/Symbols-OFL.txt"),
        ])
    cap = font["OS/2"].sCapHeight * 2
    metrics = [font["head"].unitsPerEm * 2, font["hhea"].ascent * 2, font["hhea"].descent * 2, font["hhea"].lineGap * 2, cap]
    length = 40 + len(records) + len(curves) + len(notice)
    header = b"OTRQFNT1" + struct.pack("<IHhhhhHIIII", length, *metrics, 0, len(entries), len(curves) // 12, len(notice), 0)
    raw = header + records + curves + notice
    compressed = subprocess.run(["zstd", "-q", "-3", "--check", "--single-thread", "--stream-size=" + str(len(raw)), "-c"], input=raw, capture_output=True, check=True).stdout
    metadata = {
        "format": "otrios-quadratic-font-v1", "family": "OTriOS UI (Roboto + Noto fallback)", "style": "Regular",
        "sourceSha256": SOURCE_SHA256, "compressedSha256": hashlib.sha256(compressed).hexdigest(),
        "decodedSha256": hashlib.sha256(raw).hexdigest(), "compressedBytes": len(compressed),
        "decodedBytes": len(raw), "glyphs": len(entries), "curves": len(curves) // 12,
        "maxBandCurves": max_band, "bandTexels": band_texels,
        "unitsPerEm": metrics[0], "ascent": metrics[1], "descent": metrics[2],
        "lineGap": metrics[3], "capHeight": metrics[4], "license": "OFL-1.1",
        "resource": "fonts/Roboto-Regular.otq.zst", "generator": "FontTools 4.60.1 / Zstandard 1.5.6 level 3",
        "fallbacks": [{"family": family, "sourceSha256": sha,
                       "unicodeScalars": [f"U+{code:04X}" for code in fallback_codes[family]]}
                      for family, sha in [("Noto Emoji", EMOJI_SHA256), ("Noto Sans Symbols 2", SYMBOL_SHA256), ("Noto Sans Symbols", ARROW_SHA256)]],
        "defaultIgnorables": [f"U+{code:04X}" for code in sorted(DEFAULT_IGNORABLES)],
        "limitations": "Monochrome scalar glyphs only; no ZWJ, flag or skin-tone sequence shaping.",
    }
    return compressed, (json.dumps(metadata, indent=2, sort_keys=True) + "\n").encode(), notice


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    compressed, metadata, notice = generate()
    sha = hashlib.sha256(compressed).hexdigest()
    rust_ref = f'// SPDX-License-Identifier: Apache-2.0\n// Generated resource identity; font bytes are fetched and cached separately.\npub const RESOURCE_SHA256: &str =\n    "{sha}";\npub const RESOURCE_FILE: &str = "Roboto-Regular.otq.zst";\n'.encode()
    js_ref = f'// SPDX-License-Identifier: Apache-2.0\n// Generated resource identity; font bytes are fetched and cached separately.\nexport const FONT_SHA256 = "{sha}";\nexport const FONT_PATH = "fonts/Roboto-Regular.otq.zst";\n'.encode()
    for path, data in [(OUTPUT, compressed), (METADATA, metadata), (RUST_REF,rust_ref), (JS_REF,js_ref), (PUBLIC_NOTICE,notice)]:
        if args.check:
            if path.read_bytes() != data:
                raise SystemExit(f"Stale generated font resource: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    print(metadata.decode())
