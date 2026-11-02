#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-only
"""Independent stdlib oracle for published ZIP/checkpoint/causal framing.

FlexBuffers values stay opaque here; Rust's frozen-vector tests check their
canonical bytes and typed semantics. This oracle does not invoke Rust codecs.
"""
import hashlib
import io
import json
import pathlib
import zipfile


def digest(data):
    return hashlib.sha256(data).digest()


def payload(record):
    result = bytes.fromhex(record['hex'])
    assert digest(result).hex() == record['sha256']
    return result


class Reader:
    def __init__(self, data):
        self.data, self.at = data, 0

    def take(self, count):
        assert 0 <= count <= len(self.data) - self.at, 'truncated checkpoint'
        result = self.data[self.at:self.at + count]
        self.at += count
        return result

    def integer(self, size):
        return int.from_bytes(self.take(size), 'little')

    def end(self):
        assert self.at == len(self.data), 'unexplained trailing bytes'


def snapshot(data, expected_root):
    """Check immutable node closure, canonical ordering, causal clocks and hashes."""
    r = Reader(data)
    assert r.take(26) == b'OTriOS\0merkle-transfer\0v1\0'
    assert tuple(r.integer(8) for _ in range(3)) == (0, 0, 1)
    assert r.integer(1) == 0, 'full subtree has no path prefix'
    assert r.integer(1) == 1, 'full snapshot'
    assert r.integer(1) == 0, 'no source base or resume cursor'
    assert r.integer(1) == 1
    root = r.take(32)
    assert root.hex() == expected_root
    nodes, order = {}, []
    for _ in range(r.integer(4)):
        raw = r.take(r.integer(4))
        key = digest(raw)
        assert key not in nodes
        order.append(key)
        n = Reader(raw)
        assert n.take(22) == b'OTriOS\0merkle-node\0v1\0'
        has_register = n.integer(1)
        assert has_register in (0, 1)
        register = None
        if has_register:
            schema = n.take(32)
            clocks = [(n.take(16), n.integer(8)) for _ in range(n.integer(1))]
            assert clocks and clocks == sorted(clocks) and len(dict(clocks)) == len(clocks)
            assert all(clock > 0 for _, clock in clocks)
            values = []
            for _ in range(n.integer(1)):
                actor, counter, present = n.take(16), n.integer(8), n.integer(1)
                assert present in (0, 1)
                value = n.take(n.integer(4)) if present else None
                assert dict(clocks)[actor] == counter
                values.append((actor, counter, value))
            assert values and values == sorted(values, key=lambda value: value[:2])
            assert len({value[:2] for value in values}) == len(values)
            register = (schema, clocks, values)
        children = [(n.integer(8), n.take(32)) for _ in range(n.integer(1))]
        assert children == sorted(children) and len(dict(children)) == len(children)
        n.end()
        nodes[key] = (register, children)
    r.end()
    assert order == sorted(order)
    visited, leaves = set(), {}

    def walk(key, path=()):
        assert len(path) <= 16
        visited.add(key)
        register, children = nodes[key]
        if register is not None:
            assert not children
            leaves[path] = register
        for child, child_hash in children:
            walk(child_hash, path + (child,))
    walk(root)
    assert visited == set(nodes), 'no hidden authority or surplus causal records'
    return leaves


def check():
    v = json.loads(pathlib.Path(__file__).with_name('portable-data-fixtures.json').read_text())
    assert v['format'] == 'otrios-portable-data-fixtures-v1'
    frozen = payload(v['archive'])
    assert digest(frozen).hex() == '730ccd65407c0cc6357ae918c1c3358d2b666310438e3066ba1e1cf0c1e4046e'
    with zipfile.ZipFile(io.BytesIO(frozen)) as z:
        assert z.namelist() == sorted(v['files'])
        for name, record in v['files'].items():
            assert z.read(name) == payload(record)
            if name.startswith('Assets/'):
                assert name[7:] == digest(z.read(name)).hex()
        assert z.read('Info.flex').startswith(b'OTDATA\x01\0')
        assert z.read('State/checkpoint.bin') == payload(v['collection']['checkpoint'])
    collection = Reader(payload(v['collection']['checkpoint']))
    assert collection.take(8) == b'OTCLP\x01\0\0'
    assert collection.integer(2) == 2
    roots = []
    for key in ['auxiliary', 'canvas']:
        name = collection.take(collection.integer(2)).decode('utf-8')
        assert name == v[key]['scope']
        roots.append(name)
        canvas = collection.take(collection.integer(4))
        assert canvas == payload(v[key]['checkpoint']) and canvas.startswith(b'OTCVP\x01\0\0')
        history = collection.take(collection.integer(4))
        assert history == (payload(v['chat']['checkpoint']) if key == 'canvas' else b'')
    assert roots == sorted(set(roots))
    collection.end()
    fields = snapshot(payload(v['canvas']['fields_snapshot']), v['canvas']['fields_root_sha256'])
    assert set(fields) == {(0, 0), (0, 1), (0, 2)}
    for _, clocks, values in fields.values():
        assert clocks == [(bytes(16), 1), (bytes([17]) * 16, 1)]
        assert len(values) == 1 and values[0][:2] == (bytes([17]) * 16, 1)
    signals = snapshot(payload(v['with_signals']['snapshot']), v['with_signals']['root_sha256'])
    assert set(signals) == {(0, 0)}
    assert signals[(0, 0)][1] == [(bytes([17]) * 16, 1)]
    history = Reader(payload(v['chat']['checkpoint']))
    assert history.take(8) == b'OTCHP\x01\0\0'
    message_schema = digest(b'otrios-brass-relay-message-v2')
    assert history.take(32) == message_schema
    assert history.integer(8) == v['chat']['high_water'] == 9
    assert history.integer(4) == len(v['chat']['messages']) == len(v['chat']['rows']) == 2
    for message, row in zip(v['chat']['messages'], v['chat']['rows']):
        assert history.integer(8) == message['sequence'] == row['sequence']
        raw = history.take(history.integer(4))
        assert raw == payload(row['snapshot'])
        causal = snapshot(raw, row['root_sha256'])
        assert set(causal) == {(1,)}
        schema, clocks, values = causal[(1,)]
        assert schema == message_schema and clocks == [(bytes([22]) * 16, 1)]
        assert values == [(bytes([22]) * 16, 1, payload(row['message_flex']))]
    history.end()
    print('Portable data fixtures: ZIP closure, 2 roots, causal fields/signals, history framing, redaction high-water and SHA-256 verified independently')


if __name__ == '__main__':
    check()
