#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Produce independent small ZIP conformance vectors with Python's ZIP reader/writer."""
import argparse
import hashlib
import io
import json
import pathlib
import struct
import zipfile

FILES = {'Info.flex': b'transfer fixture v1\n', 'UI/main.flex': bytes(range(16)) * 3}
DESTINATION = pathlib.Path(__file__).resolve().parent.parent / 'crates/otrios-core/tests/fixtures/transfer-vectors.json'


class Pipe(io.BytesIO):
    def seek(self, *args):
        raise OSError('Unseekable streaming fixture')


def archive(items, method=0, pipe=False, link=False):
    output = Pipe() if pipe else io.BytesIO()
    with zipfile.ZipFile(output, 'w', compression=method) as writer:
        for path, data in items.items():
            info = zipfile.ZipInfo(path, (1980, 1, 1, 0, 0, 0))
            info.compress_type, info.create_system = method, 3
            info.external_attr = (0o120777 if link else 0o100644) << 16
            writer.writestr(info, data)
    return output.getvalue()


def get32(data, at):
    return struct.unpack_from('<I', data, at)[0]


def put32(data, at, value):
    struct.pack_into('<I', data, at, value)


def descriptor_without_signature(original):
    central = get32(original, len(original) - 6)
    result, positions = bytearray(), {}
    with zipfile.ZipFile(io.BytesIO(original)) as reader:
        for info in reader.infolist():
            at = info.header_offset
            name_length = struct.unpack_from('<H', original, at + 26)[0]
            end = at + 30 + name_length + info.compress_size
            assert original[end:end + 4] == b'PK\x07\x08'
            positions[info.filename] = len(result)
            result.extend(original[at:end])
            result.extend(original[end + 4:end + 16])
    new_central = len(result)
    entries = bytearray(original[central:-22])
    at = 0
    for _ in positions:
        length = struct.unpack_from('<H', entries, at + 28)[0]
        name = entries[at + 46:at + 46 + length].decode()
        put32(entries, at + 42, positions[name])
        at += 46 + length
    result.extend(entries)
    end = bytearray(original[-22:])
    put32(end, 16, new_central)
    result.extend(end)
    return bytes(result)


def surplus_deflate_byte(original):
    data = bytearray(original)
    central = get32(data, len(data) - 6)
    length = struct.unpack_from('<H', data, 26)[0]
    compressed = get32(data, 18)
    insert = 30 + length + compressed
    data.insert(insert, 0)
    put32(data, 18, compressed + 1)
    at = central + 1
    while at < len(data) - 22:
        length = struct.unpack_from('<H', data, at + 28)[0]
        offset = get32(data, at + 42)
        if offset == 0:
            put32(data, at + 20, get32(data, at + 20) + 1)
        elif offset >= insert:
            put32(data, at + 42, offset + 1)
        at += 46 + length
    put32(data, len(data) - 6, central + 1)
    return bytes(data)


def generate():
    record = {'format': 'otrios-transfer-vectors-v1', 'files_hex': {path: data.hex() for path, data in FILES.items()}}
    samples = {
        'stored': archive(FILES),
        'deflated': archive(FILES, 8),
        'descriptor': archive(FILES, 8, pipe=True),
        'case_alias': archive({'Readme.txt': b'a', 'README.txt': b'b'}),
        'traversal': archive({'../escape.flex': b'a'}),
        'symlink': archive({'Link': b'../outside'}, link=True),
    }
    samples['descriptor_without_signature'] = descriptor_without_signature(samples['descriptor'])
    samples['deflate_trailing_bytes'] = surplus_deflate_byte(samples['deflated'])
    for name, data in samples.items():
        if name not in ['case_alias', 'traversal', 'symlink']:
            # Python accepts the surplus Deflate byte; OTriOS deliberately rejects
            # it as an unexplained byte in the compressed stream's closure.
            with zipfile.ZipFile(io.BytesIO(data)) as reader:
                assert {path: reader.read(path) for path in reader.namelist()} == FILES
        record[name] = {'hex': data.hex(), 'sha256': hashlib.sha256(data).hexdigest()}
    return json.dumps(record, indent=2) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    result = generate()
    if args.check:
        if DESTINATION.read_text() != result:
            raise SystemExit('Transfer vectors differ; inspect producer/version changes before updating')
        print('Transfer ZIP vectors match the independent producer')
    else:
        DESTINATION.write_text(result)
