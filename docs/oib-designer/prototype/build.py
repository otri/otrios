# SPDX-License-Identifier: Apache-2.0
"""Build self-contained browser UX study; no third-party code or network."""
from pathlib import Path
root = Path(__file__).parent
html = (root / 'shell.html').read_text()
for marker, name in [('MODEL', 'model.cjs'), ('UI', 'ui.js')]:
    html = html.replace(f'/* {marker} */', (root / name).read_text())
(root / 'index.html').write_text(html)
print('Built', root / 'index.html')
