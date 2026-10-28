#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Check that unsupported historical options fail before benchmark side effects."""
import pathlib
import shlex
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).with_name('benchmark-canvas-pipeline.py')


class PipelineCli(unittest.TestCase):
    def test_legacy_rejected_before_output_or_external_command(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            marker = root / 'called'
            tailscale = root / 'tailscale'
            tailscale.write_text('#!/bin/sh\ntouch ' + shlex.quote(str(marker)) + '\n')
            tailscale.chmod(0o755)
            subprocess.run([str(tailscale)], check=True)
            self.assertTrue(marker.exists())
            marker.unlink()
            output = root / 'output'
            result = subprocess.run([
                sys.executable, str(SCRIPT), '--legacy-reference',
                '--reference', str(root / 'reference'), '--candidate', str(root / 'candidate'),
                '--output', str(output), '--release', str(root / 'release'),
                '--frame', 'unused', '--known-hosts', str(root / 'known-hosts'),
                '--tailscale', str(tailscale),
            ], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('ed3c86b', result.stderr)
            self.assertFalse(output.exists())
            self.assertFalse(marker.exists())

    def test_help(self):
        result = subprocess.run([sys.executable, str(SCRIPT), '--help'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0)
        self.assertIn('--reference-session', result.stdout)


if __name__ == '__main__':
    unittest.main()
