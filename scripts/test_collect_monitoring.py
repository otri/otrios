#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Collector publication remains local when an authority uses another port."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    'collector', pathlib.Path(__file__).with_name('collect-monitoring.py'))
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


class PublicationOrigin(unittest.TestCase):
    def test_default_and_alternate_loopback_ports(self):
        self.assertEqual(collector.presentation_url({}),
                         'http://127.0.0.1:8876/presentation')
        for origin in ['http://127.0.0.1:8894', 'http://[::1]:8894/']:
            with self.subTest(origin=origin):
                self.assertEqual(collector.presentation_url({'authority_url': origin}),
                                 origin.rstrip('/') + '/presentation')

    def test_credentials_cannot_be_published_to_another_origin_or_path(self):
        for origin in [None, 'https://127.0.0.1:8894', 'http://localhost:8894',
                       'http://192.168.1.1:8894', 'http://127.0.0.1.example',
                       'http://user:password@127.0.0.1:8894',
                       'http://127.0.0.1:8894/other', 'http://127.0.0.1:8894?query=1',
                       'http://127.0.0.1:8894#fragment', 'http://127.0.0.1:0',
                       'http://127.0.0.1:65536']:
            with self.subTest(origin=origin), self.assertRaises(ValueError):
                collector.presentation_url({'authority_url': origin})


if __name__ == '__main__':
    unittest.main()
