#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Generate OTEMO002 tintable emoji masks from pinned OpenMoji and Unicode data."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import zipfile

from PIL import Image, ImageDraw, ImageFont
import zstandard

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/fonts/openmoji/black-72x72.zip'
DATA = ROOT / 'assets/fonts/unicode-emoji/emoji-test-17.0.txt'
NOTO = ROOT / 'assets/fonts/noto-emoji/NotoEmoji.ttf'
NOTICE = ROOT / 'assets/fonts/openmoji/NOTICE.txt'
LICENSE = ROOT / 'assets/fonts/openmoji/LICENSE.txt'
PUBLIC_NOTICE = ROOT / 'web/shared/fonts/Emoji-NOTICE.txt'
PUBLIC_LICENSE = ROOT / 'web/shared/fonts/Emoji-LICENSE.txt'
OUT = ROOT / 'web/shared/fonts/Emoji.otemo'
INDEX = ROOT / 'web/shared/fonts/Emoji.index'
META = ROOT / 'assets/fonts/openmoji/resource.json'
RUST_REF = ROOT / 'crates/otrios-core/src/dashboard/emoji_reference.rs'
JS_REF = ROOT / 'web/shared/emoji-reference.mjs'
HASHES = {
    SOURCE: 'b025bc2de99018c1438d6542a94541d537f9c8fd06f88b1d565c667056cbad4e',
    DATA: '1d8a944f88d7952f7ef7c5167fef3c67995bcae24543949710231b03a201acda',
    NOTO: 'de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551',
    LICENSE: '5e436ff8ffbb77d8607220e9bce20c8915d860010feeb6c1ebef5a85688e9b39',
}
# Unicode defines these mixed-tone sequences without corresponding unmodified
# sequences. Choose the semantically equivalent neutral or gendered base art.
ALIASES = {
    (0x1FAF1, 0x200D, 0x1FAF2): (0x1F91D,),  # handshake
    (0x1F9D1, 0x200D, 0x1F430, 0x200D, 0x1F9D1): (0x1F46F,),  # people with bunny ears
    (0x1F468, 0x200D, 0x1F430, 0x200D, 0x1F468): (0x1F46F, 0x200D, 0x2642),  # men with bunny ears
    (0x1F469, 0x200D, 0x1F430, 0x200D, 0x1F469): (0x1F46F, 0x200D, 0x2640),  # women with bunny ears
    (0x1F9D1, 0x200D, 0x1FAEF, 0x200D, 0x1F9D1): (0x1F93C,),  # people wrestling
    (0x1F468, 0x200D, 0x1FAEF, 0x200D, 0x1F468): (0x1F93C, 0x200D, 0x2642),  # men wrestling
    (0x1F469, 0x200D, 0x1FAEF, 0x200D, 0x1F469): (0x1F93C, 0x200D, 0x2640),  # women wrestling
    (0x1F469, 0x200D, 0x1F91D, 0x200D, 0x1F469): (0x1F46D,),  # women holding hands
    (0x1F469, 0x200D, 0x1F91D, 0x200D, 0x1F468): (0x1F46B,),  # woman and man holding hands
    (0x1F468, 0x200D, 0x1F91D, 0x200D, 0x1F468): (0x1F46C,),  # men holding hands
    (0x1F9D1, 0x200D, 0x2764, 0x200D, 0x1F48B, 0x200D, 0x1F9D1): (0x1F48F,),  # kiss
    (0x1F9D1, 0x200D, 0x2764, 0x200D, 0x1F9D1): (0x1F491,),  # couple with heart
}


def normalize(codes):
    return tuple(c for c in codes if c != 0xFE0F and not 0x1F3FB <= c <= 0x1F3FF)


def generate():
    for path, expected in HASHES.items():
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f'Source hash mismatch: {path}')
    if zstandard.__version__ != '0.24.0' or Image.__version__ != '11.3.0':
        raise ValueError('Use pinned Pillow 11.3.0 and zstandard 0.24.0')
    rows = []
    for line in DATA.read_text(encoding='utf8').splitlines():
        if not line or line.startswith('#'):
            continue
        codes, status = line.split('#', 1)[0].split(';')
        status = status.strip()
        if status in ('fully-qualified', 'minimally-qualified', 'unqualified', 'component'):
            rows.append((tuple(int(code, 16) for code in codes.split()), status))
    with zipfile.ZipFile(SOURCE) as source:
        names = {}
        for name in sorted(source.namelist()):
            if not name.endswith('.png'):
                continue
            codes = tuple(int(part, 16) for part in name[:-4].split('-'))
            # Do not fold skin variants into lookup: use only neutral source art.
            if any(0x1F3FB <= c <= 0x1F3FF for c in codes):
                continue
            key = normalize(codes)
            if key in names:
                raise ValueError(f'Ambiguous normalized art: {name} and {names[key]}')
            names[key] = name
        mapping, counts = {}, {}
        for codes, status in rows:
            if status == 'component' and len(codes) == 1 and 0x1F3FB <= codes[0] <= 0x1F3FF:
                continue  # Standalone tone modifiers are ignored by display policy.
            key = normalize(codes)
            key = ALIASES.get(key, key)
            if key not in names:
                raise ValueError(f'Missing art for {codes!r} ({status}); normalized {key!r}')
            if 0xFE0E in codes:
                raise ValueError('Text selector cannot select emoji art')
            mapping[''.join(chr(c) for c in codes).encode('utf8')] = names[key]
            counts[status] = counts.get(status, 0) + 1
        if counts.get('fully-qualified') != 3944:
            raise ValueError(f'Unexpected Unicode 17 full sequence count: {counts}')
        paths = sorted(set(mapping.values()))
        ids = {name: n + 1 for n, name in enumerate(paths)}
        seq_table = bytearray()
        for sequence, name in sorted(mapping.items()):
            seq_table += struct.pack('<HHI', len(sequence), 0, ids[name]) + sequence
        payload_offset = 24 + len(seq_table) + len(paths) * 24
        records, payload = bytearray(), bytearray()
        compressor = zstandard.ZstdCompressor(level=3, write_checksum=True, write_content_size=True, threads=0)
        for name in paths:
            image = Image.open(io.BytesIO(source.read(name))).convert('RGBA').getchannel('A')
            if name == '1F195.png':
                # Upstream OpenMoji 17.0.0 black SVG and PNG are both empty.
                # Use the pinned OFL Noto Emoji outline instead.
                font = ImageFont.truetype(str(NOTO), 48)
                bounds = font.getbbox('🆕')
                canvas = Image.new('L', (64, 64), 0)
                ImageDraw.Draw(canvas).text(((64 - (bounds[2] - bounds[0])) // 2 - bounds[0],
                                             (64 - (bounds[3] - bounds[1])) // 2 - bounds[1]),
                                            '🆕', font=font, fill=255)
            else:
                image.thumbnail((64, 64), Image.Resampling.LANCZOS)
                canvas = Image.new('L', (64, 64), 0)
                canvas.paste(image, ((64 - image.width) // 2, (64 - image.height) // 2))
            if canvas.getbbox() is None:
                raise ValueError(f'Emoji artwork produced a blank mask: {name}')
            encoded = compressor.compress(canvas.tobytes())
            records += struct.pack('<IIIHHhhhH', ids[name], payload_offset + len(payload), len(encoded), 64, 64, 0, 0, 64, 0)
            payload += encoded
    data = struct.pack('<8sIIII', b'OTEMO002', payload_offset + len(payload), len(mapping), len(paths), len(seq_table)) + seq_table + records + payload
    metadata = {'format': 'OTEMO002', 'unicodeVersion': '17.0', 'source': 'OpenMoji 17.0.0 black PNG',
                'sourceSha256': HASHES[SOURCE], 'emojiTestSha256': HASHES[DATA], 'resourceSha256': hashlib.sha256(data).hexdigest(),
                'resourceBytes': len(data), 'sequenceCount': len(mapping), 'sequencesByQualification': counts,
                'omittedStandaloneSkinToneComponents': 5, 'glyphCount': len(paths), 'tileWidth': 64, 'tileHeight': 64,
                'pixelFormat': 'alpha8', 'compression': 'zstd level 3 per tile',
                'license': 'CC-BY-SA-4.0', 'generator': 'Pillow 11.3.0 / python-zstandard 0.24.0'}
    return data, (json.dumps(metadata, indent=2, sort_keys=True) + '\n').encode()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    resource, metadata = generate()
    _, _, _, glyph_count, seq_bytes = struct.unpack_from('<8sIIII', resource)
    index = resource[:24 + seq_bytes + glyph_count * 24]
    sha = hashlib.sha256(resource).hexdigest()
    rust_ref = (f'// SPDX-License-Identifier: Apache-2.0\n'
                f'// Generated resource identity; emoji bytes are fetched separately.\n'
                f'pub const RESOURCE_SHA256: &str =\n    "{sha}";\n'
                f'pub const RESOURCE_FILE: &str = "Emoji.otemo";\n').encode()
    js_ref = (f'// SPDX-License-Identifier: Apache-2.0\n'
              f'// Generated resource identity; emoji bytes are fetched separately.\n'
              f'export const EMOJI_SHA256 = "{sha}";\n'
              f'export const EMOJI_PATH = "fonts/Emoji.otemo";\n').encode()
    for path, content in ((OUT, resource), (INDEX, index), (META, metadata),
                          (RUST_REF, rust_ref), (JS_REF, js_ref),
                          (PUBLIC_NOTICE, NOTICE.read_bytes()), (PUBLIC_LICENSE, LICENSE.read_bytes())):
        if args.check:
            if path.read_bytes() != content:
                raise SystemExit(f'Stale generated resource: {path}')
        else:
            path.write_bytes(content)
    print(metadata.decode())
