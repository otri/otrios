#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Independent byte-count expectations, including arbitrary TCP chunk boundaries."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location('counter', pathlib.Path(__file__).with_name('serve-canvas-benchmark-authorities.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class Counter(unittest.TestCase):
    def setUp(self):
        for key in module.stats['baseline']:
            module.stats['baseline'][key] = 0

    def test_binary_masked_extended_and_text_frames_across_chunks(self):
        parser = module.Frames('baseline', 'up')
        small = bytes([0x82, 3]) + b'abc'
        medium = bytes([0x82, 0x80 | 126]) + (256).to_bytes(2, 'big') + b'mask' + bytes(256)
        large = bytes([0x82, 127]) + (70000).to_bytes(8, 'big') + bytes(70000)
        text = bytes([0x81, 5]) + b'hello'
        wire = b'GET /apps/stream HTTP/1.1\r\nUpgrade: websocket\r\n\r\n' + small + medium + large + text
        for i in range(0, len(wire), 17):
            parser.feed(wire[i:i + 17])
        self.assertEqual(module.stats['baseline']['up_binary'], 3 + 256 + 70000)
        self.assertEqual(module.stats['baseline']['up_ws'], len(small) + len(medium) + len(large) + len(text))

    def test_oversized_length_is_rejected_before_waiting_for_payload(self):
        parser = module.Frames('baseline', 'down')
        with self.assertRaises(ValueError):
            parser.feed(b'HTTP/1.1 101 Switching Protocols\r\n\r\n' + bytes([0x82, 127]) + (2 * 1024 * 1024 + 1).to_bytes(8, 'big'))

if __name__ == '__main__':
    unittest.main()
