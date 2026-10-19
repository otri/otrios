#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Prepare a bounded quadratic Roboto resource; no outline tessellation or embedded font.

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
RUST_REF = ROOT / "crates/otrios-core/src/dashboard/font_reference.rs"
JS_REF = ROOT / "web/shared/font-reference.mjs"
SOURCE_SHA256 = "d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134"


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


def generate():
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA256:
        raise ValueError("Roboto source changed; review its source and license first")
    version = subprocess.check_output(["zstd", "--version"], text=True)
    if "v1.5.6" not in version:
        raise ValueError("Use pinned Zstandard CLI 1.5.6 for reproducible resource bytes")
    font = instantiateVariableFont(TTFont(SOURCE), {"wght": 400, "wdth": 100}, inplace=True)
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap()
    codepoints = sorted(set(range(32, 127)) | set(range(160, 256)) | set(range(0x2010, 0x2028)) | set(range(0x2190, 0x2194)) | {0x2212, 0xFFFD})
    entries = [(0, ".notdef")] + [(c, cmap[c]) for c in codepoints if c in cmap]
    records, curves = bytearray(), bytearray()
    for code, name in entries:
        pen = Quadratics(glyphs)
        glyphs[name].draw(pen)
        bounds = BoundsPen(glyphs)
        glyphs[name].draw(bounds)
        xmin, ymin, xmax, ymax = bounds.bounds or (0, 0, 0, 0)
        # Curve coordinates use signed half-font-units, preserving odd line midpoints.
        # The packet's unitsPerEm and all metrics are doubled accordingly.
        ink = (math.floor(xmin * 2), math.floor(ymin * 2), math.ceil(xmax * 2), math.ceil(ymax * 2))
        records.extend(struct.pack("<IIIIhhhhhHI", code, font.getGlyphID(name), len(curves) // 12, len(pen.curves), font["hmtx"][name][0] * 2, *ink, 0, 0))
        for curve in pen.curves:
            curves.extend(struct.pack("<hhhhhh", *(round(v * 2) for point in curve for v in point)))
    notice = (ROOT / "assets/fonts/roboto/OFL.txt").read_bytes()
    cap = font["OS/2"].sCapHeight * 2
    metrics = [font["head"].unitsPerEm * 2, font["hhea"].ascent * 2, font["hhea"].descent * 2, font["hhea"].lineGap * 2, cap]
    length = 40 + len(records) + len(curves) + len(notice)
    header = b"OTRQFNT1" + struct.pack("<IHhhhhHIIII", length, *metrics, 0, len(entries), len(curves) // 12, len(notice), 0)
    assert len(header) == 40
    raw = header + records + curves + notice
    # Content size and checksum included; one bounded frame, no dictionary.
    compressed = subprocess.run(["zstd", "-q", "-3", "--check", "--single-thread", "--stream-size=" + str(len(raw)), "-c"], input=raw, capture_output=True, check=True).stdout
    metadata = {
        "format": "otrios-quadratic-font-v1", "family": "Roboto", "style": "Regular",
        "sourceSha256": SOURCE_SHA256, "compressedSha256": hashlib.sha256(compressed).hexdigest(),
        "decodedSha256": hashlib.sha256(raw).hexdigest(), "compressedBytes": len(compressed),
        "decodedBytes": len(raw), "glyphs": len(entries), "curves": len(curves) // 12,
        "unitsPerEm": metrics[0], "ascent": metrics[1], "descent": metrics[2],
        "lineGap": metrics[3], "capHeight": metrics[4], "license": "OFL-1.1",
        "resource": "fonts/Roboto-Regular.otq.zst", "generator": "FontTools 4.60.1 / Zstandard 1.5.6 level 3",
    }
    return compressed, (json.dumps(metadata, indent=2, sort_keys=True) + "\n").encode()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    compressed, metadata = generate()
    sha = hashlib.sha256(compressed).hexdigest()
    rust_ref = f'// SPDX-License-Identifier: Apache-2.0\n// Generated resource identity; font bytes are fetched and cached separately.\npub const RESOURCE_SHA256: &str =\n    "{sha}";\npub const RESOURCE_FILE: &str = "Roboto-Regular.otq.zst";\n'.encode()
    js_ref = f'// SPDX-License-Identifier: Apache-2.0\n// Generated resource identity; font bytes are fetched and cached separately.\nexport const FONT_SHA256 = "{sha}";\nexport const FONT_PATH = "fonts/Roboto-Regular.otq.zst";\n'.encode()
    for path, data in [(OUTPUT, compressed), (METADATA, metadata), (RUST_REF,rust_ref), (JS_REF,js_ref)]:
        if args.check:
            if path.read_bytes() != data:
                raise SystemExit(f"Stale generated font resource: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    print(metadata.decode())
