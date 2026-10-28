#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Exercise deletion boundaries with real temporary Git checkouts."""
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = pathlib.Path(__file__).with_name('clean-artifacts.py')
spec = importlib.util.spec_from_file_location('clean_artifacts', SCRIPT)
cleaner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleaner)
REAL_BUILDERS = cleaner.active_builders


class ArtifactCleanup(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name).resolve() / 'checkout with spaces'
        self.root.mkdir()
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        self.write('crates/otrios-core/Cargo.toml', '[package]\nname="otrios-core"\nversion="0.1.0"\n')
        self.write('scripts/check.sh', '# fixture\n')
        self.write('.gitignore', '/target/\n__pycache__/\n/web/dashboard/pkg/\n')
        subprocess.run(['git', '-C', str(self.root), 'add', '.'], check=True)
        self.builder_check = mock.patch.object(cleaner, 'active_builders', return_value=[])
        self.builder_check.start()
        self.addCleanup(self.builder_check.stop)

    def write(self, name, content='keep'):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def test_dry_run_then_apply_removes_only_allowlisted_caches(self):
        for path in cleaner.ARTIFACTS.values():
            self.write(path + '/generated', 'artifact')
        survivors = [self.write(name) for name in ['target/qualification/measurements.json', 'target/debug/deps/app', 'target/release/app', 'target/python-cache/data.pyc', 'bench-results/results.json', 'web/canvas/pkg/app.wasm', 'apps/home/package/ui.flex', '.env']]
        planned = cleaner.clean(self.root)
        self.assertFalse(planned['blocked'])
        self.assertEqual([row['status'] for row in planned['artifacts']], ['ready'] * 3)
        self.assertTrue(all((self.root / name).exists() for name in cleaner.ARTIFACTS.values()))
        result = cleaner.clean(self.root, apply=True)
        self.assertFalse(result['blocked'])
        self.assertEqual([row['status'] for row in result['artifacts']], ['removed'] * 3)
        self.assertTrue(all(path.read_text() == 'keep' for path in survivors))
        self.assertEqual([row['status'] for row in cleaner.clean(self.root, apply=True)['artifacts']], ['absent'] * 3)

    def test_selection_and_unknown_groups(self):
        self.write('target/debug/incremental/object.o')
        python = self.write('scripts/__pycache__/module.pyc')
        self.assertFalse(cleaner.clean(self.root, ['incremental'], apply=True)['blocked'])
        self.assertTrue(python.exists())
        with self.assertRaises(ValueError):
            cleaner.clean(self.root, ['target'])

    def test_symlink_at_candidate_or_ancestor_blocks_all_deletion(self):
        outside = pathlib.Path(self.temp.name) / 'outside'
        outside.mkdir()
        marker = outside / 'keep'
        marker.write_text('keep')
        cache = self.write('scripts/__pycache__/module.pyc')
        (self.root / 'target').symlink_to(outside, target_is_directory=True)
        result = cleaner.clean(self.root, apply=True)
        self.assertTrue(result['blocked'])
        self.assertTrue(marker.exists())
        self.assertTrue(cache.exists())
        (self.root / 'target').unlink()
        (self.root / 'target/debug').mkdir(parents=True)
        (self.root / 'target/debug/incremental').symlink_to(outside, target_is_directory=True)
        self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        self.assertTrue(marker.exists())

    def test_nested_symlink_and_special_file_are_refused(self):
        folder = self.root / 'target/debug/incremental'
        folder.mkdir(parents=True)
        link = folder / 'link'
        link.symlink_to(self.root / '.git', target_is_directory=True)
        self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        link.unlink()
        os.mkfifo(folder / 'pipe')
        self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])

    def test_tracked_or_unignored_path_blocks_all_deletion(self):
        cache = self.write('scripts/__pycache__/module.pyc')
        tracked = self.write('target/debug/incremental/tracked')
        subprocess.run(['git', '-C', str(self.root), 'add', '-f', str(tracked)], check=True)
        self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        self.assertTrue(cache.exists())
        subprocess.run(['git', '-C', str(self.root), 'rm', '--cached', '-f', str(tracked)], check=True, capture_output=True)
        self.write('.gitignore', '__pycache__/\n')
        self.assertTrue(cleaner.clean(self.root, ['incremental'], apply=True)['blocked'])
        self.assertTrue(tracked.exists())

    def test_mount_boundaries_and_uninspectable_mounts_are_refused(self):
        cache = self.write('target/debug/incremental/nested/object.o')
        for mount in [cache.parent, cache.parent.parent, self.root / 'target']:
            with self.subTest(mount=mount), mock.patch.object(cleaner, 'mount_points', return_value=[mount]):
                result = cleaner.clean(self.root, apply=True)
                self.assertTrue(result['blocked'])
                self.assertTrue(cache.exists())
        with mock.patch.object(cleaner, 'mount_points', side_effect=PermissionError('mount table denied')):
            self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        # A mount introduced between inventory and deletion must also stop removal.
        checks = iter([[], [cache.parent]])
        with mock.patch.object(cleaner, 'mount_points', side_effect=lambda: next(checks)):
            self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        self.assertTrue(cache.exists())

    def test_partial_deletion_is_reported_as_incomplete(self):
        first = self.write('target/debug/incremental/first.o')
        second = self.write('target/debug/incremental/second.o')

        def fail_midway(name, *, dir_fd):
            first.unlink()
            raise OSError('injected I/O failure')

        fail_midway.avoids_symlink_attacks = True
        with mock.patch.object(cleaner.shutil, 'rmtree', fail_midway):
            result = cleaner.clean(self.root, apply=True)
        self.assertTrue(result['blocked'])
        self.assertEqual(result['artifacts'][0]['status'], 'incomplete')
        self.assertEqual(result['incomplete_removals'], ['target/debug/incremental'])
        self.assertEqual(result['completed_removed_allocated_bytes'], 0)
        self.assertFalse(first.exists())
        self.assertTrue(second.exists())

    def test_active_or_uninspectable_builders_prevent_deletion(self):
        cache = self.write('target/debug/incremental/object.o')
        with mock.patch.object(cleaner, 'active_builders', return_value=['123: cargo']):
            self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        with mock.patch.object(cleaner, 'active_builders', side_effect=PermissionError('ps denied')):
            self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        with mock.patch.object(cleaner, 'active_builders', side_effect=[[], ['123: cargo']]):
            self.assertTrue(cleaner.clean(self.root, apply=True)['blocked'])
        self.assertTrue(cache.exists())

    def test_directory_replaced_after_inventory_is_not_deleted(self):
        cache = self.write('target/debug/incremental/object.o')
        old = cache.parent.with_name('saved')

        def swap():
            cache.parent.rename(old)
            self.write('target/debug/incremental/new-file')
            return []

        checks = iter([lambda: [], swap])
        with mock.patch.object(cleaner, 'active_builders', side_effect=lambda: next(checks)()):
            result = cleaner.clean(self.root, apply=True)
        self.assertTrue(result['blocked'])
        self.assertTrue((old / 'object.o').exists())
        self.assertTrue((cache.parent / 'new-file').exists())

    def test_newly_tracked_file_is_preserved_after_inventory(self):
        cache = self.write('target/debug/incremental/object.o')

        def stage():
            subprocess.run(['git', '-C', str(self.root), 'add', '-f', str(cache)], check=True)
            return []

        checks = iter([lambda: [], stage])
        with mock.patch.object(cleaner, 'active_builders', side_effect=lambda: next(checks)()):
            result = cleaner.clean(self.root, apply=True)
        self.assertTrue(result['blocked'])
        self.assertTrue(cache.exists())

    def test_root_validation_and_cli_json(self):
        with self.assertRaises(ValueError):
            cleaner.clean(self.root / 'scripts')
        result = subprocess.run([sys.executable, str(SCRIPT), '--repo', str(self.root), '--json'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(result.stdout)['apply_requested'])

    def test_real_process_inventory_recognizes_a_builder_in_a_spaced_path(self):
        watcher = pathlib.Path(self.temp.name) / 'builder with spaces/watch-dashboard.py'
        watcher.parent.mkdir()
        watcher.write_text('import time\ntime.sleep(10)\n')
        process = subprocess.Popen([sys.executable, str(watcher)])
        try:
            self.assertTrue(any(row.startswith(f'{process.pid}: ') for row in REAL_BUILDERS()))
        finally:
            process.terminate()
            process.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
