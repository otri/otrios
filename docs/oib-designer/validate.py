# SPDX-License-Identifier: Apache-2.0
"""Validate package structure/links and reproducibility, not runtime acceptance."""
from pathlib import Path
import re
root = Path(__file__).resolve().parent
repo = root.parents[1]
files = list(root.glob('*.md')) + [repo / 'DESIGN.md', repo / 'docs/rfcs/0037-oib-design-system.md']
for path in files:
    for target in re.findall(r'\]\(([^)]+)\)', path.read_text()):
        if '://' in target or target.startswith('#'):
            continue
        assert (path.parent / target.split('#')[0]).exists(), (path, target)
journeys = re.findall(r'^## J(\d\d)\.', (root / 'UX_JOURNEYS.md').read_text(), re.M)
assert journeys == [f'{i:02}' for i in range(1, 28)], journeys
technical = re.findall(r'^## T(\d\d)\.', (root / 'TECHNICAL_DESIGN.md').read_text(), re.M)
assert technical == [f'{i:02}' for i in range(1, 14)], technical
matrix = re.findall(r'^\| Q(\d\d) \|', (root / 'CONTROL_MATRIX.md').read_text(), re.M)
assert matrix == [f'{i:02}' for i in range(1, 17)], matrix
prototype = root / 'prototype'
expected = (prototype / 'shell.html').read_text()
for marker, name in [('MODEL', 'model.cjs'), ('UI', 'ui.js')]:
    expected = expected.replace(f'/* {marker} */', (prototype / name).read_text())
assert (prototype / 'index.html').read_text() == expected, 'Rebuild prototype'
assert 'of 96. It has no inserted' not in (root / 'UX_JOURNEYS.md').read_text()
print(f'PASS: local file links in {len(files)} documents, J01–J27, T01–T13, Q01–Q16, reproducible HTML, blank-state consistency')
