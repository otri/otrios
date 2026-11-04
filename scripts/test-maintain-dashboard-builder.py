#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Focused accepted-work maintenance boundaries using real temporary directories."""
import fcntl
import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = pathlib.Path(__file__).with_name('maintain-dashboard-builder.py')
spec = importlib.util.spec_from_file_location('maintain_dashboard_builder', SCRIPT)
cleaner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleaner)


class BuilderMaintenance(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = pathlib.Path(temp.name).resolve() / 'builder with spaces'
        self.root.mkdir()
        self.active, self.old = 'a' * 64, 'b' * 64
        for path in ('watcher.lock', 'feed.lock', 'releases/.activation.lock'):
            self.write(path, '')
        self.write('source-inbox/source.rs', 'source')
        self.config = {'runtime': str(self.root), 'repo': str(self.root / 'source-inbox')}
        self.write('config.json', json.dumps(self.config))
        self.write('releases/current.json', json.dumps({'id': self.active}))
        for name in (self.active, self.old):
            self.write('releases/' + name + '/artifact', 'release')
        check = mock.patch.object(cleaner.artifacts, 'active_builders', return_value=[])
        check.start()
        self.addCleanup(check.stop)

    def write(self, name, data='keep'):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data)
        return path

    def cache(self):
        return self.write('build/mac-target/debug/incremental/object.o', 'cache')

    def test_dry_run_then_apply_only_removes_owned_generated_paths(self):
        for path in cleaner.CACHES + ['build/source-before-sample']:
            self.write(path + '/generated', 'cache')
        kept = [self.write(path) for path in (
            'publisher/private.pem', 'publisher/public.pem', 'state.apps/journal',
            'state.presentation.flex', 'rustup/toolchain', 'cargo/registry/package',
            'build/mac-target/debug/canvas-bundle', 'build/mac-target/debug/canvas-pack',
            'build/mac-target/debug/dashboard-server', 'build/source/source.rs',
            'source-inbox/README.md', 'rollback/archive', 'backups/archive',
            'logs/watch.log', 'build/captures/approved.png')]
        before = sorted(str(p.relative_to(self.root)) for p in self.root.rglob('*'))
        plan = cleaner.clean(self.root)
        self.assertEqual(plan['releases']['obsolete'], 1)
        self.assertEqual(before, sorted(str(p.relative_to(self.root)) for p in self.root.rglob('*')))
        self.assertFalse(plan['blocked'])
        result = cleaner.clean(self.root, True)
        self.assertFalse(result['blocked'])
        self.assertTrue(all(row['status'] == 'removed' for row in result['artifacts']))
        self.assertTrue(all(p.read_text() == 'keep' for p in kept))
        self.assertTrue((self.root / 'releases' / self.active).exists())
        self.assertFalse((self.root / 'releases' / self.old).exists())
        again = cleaner.clean(self.root, True)
        self.assertEqual(again['artifacts'], [])
        self.assertEqual(again['releases']['obsolete'], 0)

    def test_busy_owner_lock_blocks_all_deletion(self):
        cache = self.cache()
        for name in ('watcher.lock', 'feed.lock'):
            with self.subTest(name=name), (self.root / name).open('r+') as stream:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    cleaner.clean(self.root, True)
                self.assertTrue(cache.exists())
                self.assertTrue((self.root / 'releases' / self.old).exists())

    def test_active_or_uninspectable_builder_blocks_deletion(self):
        cache = self.cache()
        for effect in (lambda: ['cargo'], lambda: (_ for _ in ()).throw(PermissionError('ps denied'))):
            with mock.patch.object(cleaner.artifacts, 'active_builders', side_effect=effect):
                with self.assertRaises((ValueError, PermissionError)):
                    cleaner.clean(self.root, True)
            self.assertTrue(cache.exists())
        with mock.patch.object(cleaner.artifacts, 'active_builders', side_effect=[[], ['cargo']]):
            self.assertTrue(cleaner.clean(self.root, True)['blocked'])
        self.assertTrue(cache.exists())
        self.assertTrue((self.root / 'releases' / self.old).exists())

    def test_missing_or_invalid_active_release_blocks_cache_deletion(self):
        cache = self.cache()
        for value in ('../escape', 'c' * 64):
            self.write('releases/current.json', json.dumps({'id': value}))
            with self.assertRaises(ValueError):
                cleaner.clean(self.root, True)
            self.assertTrue(cache.exists())

    def test_unresolved_import_is_preserved_until_terminal(self):
        cache = self.cache()
        self.write('build/app-import-bridge.json', json.dumps({'operation': 'pending'}))
        with self.assertRaisesRegex(ValueError, 'pending app import'):
            cleaner.clean(self.root, True)
        self.assertTrue(cache.exists())
        self.write('build/app-import-bridge.json', json.dumps({'terminal': 'applied'}))
        self.assertFalse(cleaner.clean(self.root, True)['blocked'])

    def test_symlink_candidate_ancestor_and_nested_entry_refuse_cleanup(self):
        cache = self.cache()
        outside = self.root.parent / 'outside'
        outside.mkdir()
        marker = outside / 'keep'
        marker.write_text('keep')
        link = self.root / 'build/source-before-sample'
        link.symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            cleaner.clean(self.root, True)
        link.unlink()
        (cache.parent / 'link').symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            cleaner.clean(self.root, True)
        (cache.parent / 'link').unlink()
        cache.unlink()
        cache.parent.rmdir()
        cache.parent.symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            cleaner.clean(self.root, True)
        self.assertEqual(marker.read_text(), 'keep')
        self.assertTrue((self.root / 'releases' / self.old).exists())

    def test_root_config_and_source_boundaries(self):
        cache = self.cache()
        link = self.root.parent / 'alias'
        link.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(ValueError):
            cleaner.clean(link, True)
        self.write('config.json', json.dumps({'runtime': '/', 'repo': '/'}))
        with self.assertRaises(ValueError):
            cleaner.clean(self.root, True)
        self.config['repo'] = str(cache.parent)
        self.write('config.json', json.dumps(self.config))
        with self.assertRaisesRegex(ValueError, 'source intersects'):
            cleaner.clean(self.root, True)
        self.assertTrue(cache.exists())

    def test_mount_or_replaced_directory_stops_removal(self):
        cache = self.cache()
        with mock.patch.object(cleaner.artifacts, 'mount_points', return_value=[cache.parent]):
            with self.assertRaises(ValueError):
                cleaner.clean(self.root, True)
        def replace():
            cache.parent.rename(cache.parent.with_name('saved'))
            self.write('build/mac-target/debug/incremental/replacement')
            return []
        checks = iter([lambda: [], replace])
        with mock.patch.object(cleaner.artifacts, 'active_builders', side_effect=lambda: next(checks)()):
            self.assertTrue(cleaner.clean(self.root, True)['blocked'])
        self.assertTrue((cache.parent / 'replacement').exists())
        self.assertTrue((self.root / 'releases' / self.old).exists())

    def test_partial_removal_stops_before_pruning_releases(self):
        cache = self.cache()
        second = self.write('build/mac-target/debug/incremental/second.o')
        def fail(name, *, dir_fd):
            cache.unlink()
            raise OSError('injected removal failure')
        fail.avoids_symlink_attacks = True
        with mock.patch.object(cleaner.shutil, 'rmtree', fail):
            result = cleaner.clean(self.root, True)
        self.assertTrue(result['blocked'])
        self.assertEqual(result['artifacts'][0]['status'], 'incomplete')
        self.assertTrue(second.exists())
        self.assertTrue((self.root / 'releases' / self.old).exists())

    def test_cli_dry_run_json(self):
        self.cache()
        result = subprocess.run([sys.executable, str(SCRIPT), '--runtime', str(self.root), '--json'],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(result.stdout)['apply_requested'])
        self.assertTrue((self.root / 'releases' / self.old).exists())


if __name__ == '__main__':
    unittest.main()
