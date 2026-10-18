#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Bake bounded Roboto Regular outlines into shared renderer triangle geometry.

Requires tools/fonts/requirements.txt (offline authoring tools only).
Run from anywhere: python tools/fonts/generate_roboto.py [--check].
"""
import argparse
import hashlib
from pathlib import Path
import struct

from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTFont
from fontTools.ttLib.removeOverlaps import removeOverlaps
from fontTools.varLib.instancer import instantiateVariableFont
import mapbox_earcut
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/fonts/roboto/Roboto.ttf"
OUTPUT = ROOT / "crates/otrios-core/src/dashboard/roboto_data.rs"
SOURCE_SHA256 = "d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134"
TOLERANCE = 6.0  # font units: <0.14 logical pixels at text scale 4


def flat(control, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    length = (dx * dx + dy * dy) ** 0.5
    if length == 0:
        return all((p[0] - a[0]) ** 2 + (p[1] - a[1]) ** 2 <= TOLERANCE**2 for p in control)
    return all(abs(dx * (a[1] - p[1]) - dy * (a[0] - p[0])) / length <= TOLERANCE for p in control)


def midpoint(a, b):
    return ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)


class Outlines(BasePen):
    def __init__(self, glyphs):
        super().__init__(glyphs)
        self.contours = []
        self.points = []

    def _moveTo(self, p):
        self.points = [p]

    def _lineTo(self, p):
        self.points.append(p)

    def quadratic(self, a, b, c, depth=0):
        if flat([b], a, c) or depth == 16:
            self.points.append(c)
            return
        ab, bc = midpoint(a, b), midpoint(b, c)
        center = midpoint(ab, bc)
        self.quadratic(a, ab, center, depth + 1)
        self.quadratic(center, bc, c, depth + 1)

    def _qCurveToOne(self, b, c):
        self.quadratic(self._getCurrentPoint(), b, c)

    def cubic(self, a, b, c, d, depth=0):
        if flat([b, c], a, d) or depth == 16:
            self.points.append(d)
            return
        ab, bc, cd = midpoint(a, b), midpoint(b, c), midpoint(c, d)
        abc, bcd = midpoint(ab, bc), midpoint(bc, cd)
        center = midpoint(abc, bcd)
        self.cubic(a, ab, abc, center, depth + 1)
        self.cubic(center, bcd, cd, d, depth + 1)

    def _curveToOne(self, b, c, d):
        self.cubic(self._getCurrentPoint(), b, c, d)

    def _closePath(self):
        clean = []
        for p in self.points:
            p = tuple(round(x) for x in p)
            if not clean or p != clean[-1]:
                clean.append(p)
        if len(clean) > 1 and clean[0] == clean[-1]:
            clean.pop()
        if len(clean) >= 3:
            self.contours.append(clean)
        self.points = []

    def _endPath(self):
        self._closePath()


def area(points):
    return sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(points, points[1:] + points[:1])) / 2


def contains(points, p):
    x, y = p
    inside = False
    for a, b in zip(points, points[1:] + points[:1]):
        if (a[1] > y) != (b[1] > y) and x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0]:
            inside = not inside
    return inside


def triangles(contours):
    areas = [abs(area(p)) for p in contours]
    parents = []
    for i, contour in enumerate(contours):
        enclosing = [j for j, other in enumerate(contours) if areas[j] > areas[i] and contains(other, contour[0])]
        parents.append(min(enclosing, key=areas.__getitem__) if enclosing else None)
    depths = []
    for parent in parents:
        depth = 0
        while parent is not None:
            depth += 1
            parent = parents[parent]
        depths.append(depth)
    result = []
    for i, outer in enumerate(contours):
        if depths[i] % 2:
            continue
        rings = [outer] + [p for j, p in enumerate(contours) if parents[j] == i and depths[j] % 2]
        points = [p for ring in rings for p in ring]
        ends = np.cumsum([len(ring) for ring in rings], dtype=np.uint32)
        indices = mapbox_earcut.triangulate_float64(np.array(points, dtype=np.float64), ends)
        for index in indices:
            result.append(points[index])
        # Validate tessellation covers the filled outline, including counters.
        expected = abs(area(outer)) - sum(abs(area(hole)) for hole in rings[1:])
        actual = sum(abs(area([points[k] for k in indices[n:n + 3]])) for n in range(0, len(indices), 3))
        if abs(expected - actual) > max(1, expected * 0.00001):
            raise ValueError(f"Tessellation area mismatch: {expected} != {actual}")
    return result


def generate():
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA256:
        raise ValueError("Roboto source hash changed; review its source and license before regenerating")
    font = instantiateVariableFont(TTFont(SOURCE), {"wght": 400, "wdth": 100}, inplace=True)
    removeOverlaps(font)
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap()
    codepoints = sorted(set(range(32, 127)) | set(range(160, 256)) | set(range(0x2010, 0x2028)) | set(range(0x2190, 0x2194)) | {0x2212, 0xFFFD})
    codepoints = [c for c in codepoints if c in cmap]
    cap = font["OS/2"].sCapHeight
    records, vertices = bytearray(), bytearray()
    for code in codepoints:
        name = cmap[code]
        pen = Outlines(glyphs)
        glyphs[name].draw(pen)
        try:
            points = triangles(pen.contours)
        except ValueError as error:
            raise ValueError(f"{code}: {name}: {error}") from error
        records.extend(struct.pack("<IIIHH", code, len(vertices), len(points), font["hmtx"][name][0], 0))
        for x, y in points:
            vertices.extend(struct.pack("<hh", x, cap - y))
    notice = "\n".join(line.rstrip() for line in (ROOT / "assets/fonts/roboto/OFL.txt").read_text().splitlines())
    data = b"OTRFONT1" + struct.pack("<II", len(codepoints), cap) + records + vertices + notice.encode()
    lines = ["// SPDX-License-Identifier: OFL-1.1", "// Generated by tools/fonts/generate_roboto.py; do not edit.", f"// Source SHA-256: {SOURCE_SHA256}; wght=400, wdth=100; {len(codepoints)} glyphs.", "/*", notice, "*/", "#[rustfmt::skip]", "pub(super) const DATA: &[u8] = &["]
    for i in range(0, len(data), 32):
        lines.append("    " + ",".join(str(n) for n in data[i:i+32]) + ",")
    lines.append("];\n")
    return "\n".join(lines).encode()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data = generate()
    if args.check:
        if OUTPUT.read_bytes() != data:
            raise SystemExit("Roboto mesh is stale; regenerate it")
    else:
        OUTPUT.write_bytes(data)
    print(f"Roboto Regular generated Rust: {len(data):,} bytes; SHA-256 {hashlib.sha256(data).hexdigest()}")
