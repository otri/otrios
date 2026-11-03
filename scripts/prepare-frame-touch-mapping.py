#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Prepare the Frame GSL1680F host mapping and convert its accepted calibration.

Offline only: no device access or installation. The controller firmware stays
unchanged. See docs/experiments/frame-touch-range-2026-11-03.md.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import struct

ORIGINAL_SHA256 = '307d7a85ac18908b283c79cc70ab070a1c73e7062977d1904a2f1244d5f9ab35'
CORRECTED_SHA256 = '723e0773d6b197d505ed58bc4310cf0af3dd6478a335bc0b9599a6aa0308825a'
WORD_OVERRIDES = {0x12: 0x000a0010, 0x13: 0x000a0010,
                  0x23: 0x10000000, 0x24: 0x10000000}


def correct_config(data):
    if len(data) != 2048 or hashlib.sha256(data).hexdigest() != ORIGINAL_SHA256:
        raise ValueError('Expected the verified original Frame GSL1680F configuration')
    words = list(struct.unpack('<512I', data))
    for index, value in WORD_OVERRIDES.items():
        words[index] = value
    corrected = struct.pack('<512I', *words)
    if hashlib.sha256(corrected).hexdigest() != CORRECTED_SHA256:
        raise ValueError('Corrected configuration digest mismatch')
    return corrected


def convert_matrix(matrix):
    if (len(matrix) != 6 or any(not math.isfinite(v) or abs(v) > 8 for v in matrix)
            or abs(matrix[0] * matrix[4] - matrix[1] * matrix[3]) < 0.00001):
        raise ValueError('Invalid accepted calibration matrix')
    # Express the OLD normalized evdev coordinates as affine functions of NEW
    # coordinates, then compose the accepted matrix with that change of basis.
    # X: 15 -> 16 cells; remove gain 4255/4096 and offset 7 sensor units.
    # Y: retain 10 cells; remove gain 4395/4096 and offset 20 sensor units.
    sx, tx = (16 / 15) * (4255 / 4096), -(7 * 4255 / 4096) * 800 / 960 / 1280
    sy, ty = 4395 / 4096, -(20 * 4395 / 4096) * 480 / 640 / 800
    converted = []
    for start in (0, 3):
        a, b, c = matrix[start:start + 3]
        converted.extend([a * sx, b * sy, c + a * tx + b * ty])
    if any(not math.isfinite(v) or abs(v) > 8 for v in converted):
        raise ValueError('Converted calibration exceeds native bounds')
    return converted


def convert_rules(text):
    pattern = r'(LIBINPUT_CALIBRATION_MATRIX[^=]*=\s*")([^"]+)(")'
    matches = list(re.finditer(pattern, text))
    if len(matches) != 1:
        raise ValueError('Expected exactly one calibration matrix')
    matrix = convert_matrix([float(v) for v in matches[0][2].split()])
    updated = re.sub(pattern, lambda m: m[1] + ' '.join(format(v, '.13g') for v in matrix) + m[3], text)
    return updated, matrix


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--original-config', type=Path, required=True)
    parser.add_argument('--original-rules', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='New output directory')
    args = parser.parse_args()
    corrected = correct_config(args.original_config.read_bytes())
    rules, matrix = convert_rules(args.original_rules.read_text())
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / 'moonka-gsl1680f.config').write_bytes(corrected)
    (args.output / '99-spd-touch.rules').write_text(rules)
    report = {'config_sha256': CORRECTED_SHA256,
              'rules_sha256': hashlib.sha256(rules.encode()).hexdigest(),
              'matrix': matrix, 'installed': False}
    (args.output / 'mapping.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
