#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Clear disposable Dashboard builder artifacts after work is accepted (Python 3.11+)."""
import argparse
import contextlib
import fcntl
import importlib.util
import json
import os
import pathlib
import re
import shutil
import stat
import subprocess


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, pathlib.Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


artifacts = module('dashboard_cleanup_boundaries', 'clean-artifacts.py')
pruner = module('dashboard_release_pruner', 'prune-dashboard-releases.py')
CACHES = [f'build/mac-target/debug/{name}' for name in
          ('incremental', 'deps', 'build', '.fingerprint', 'examples')]
CACHES += ['build/mac-target/release', 'build/mac-target/wasm32-unknown-unknown',
           'build/mac-target/tmp', 'build/previous-source', 'python-cache']


def clean(path, apply=False):
    root = pathlib.Path(path).absolute()
    if root.resolve(strict=True) != root:
        raise ValueError('Use a canonical runtime path without symlink ancestors')
    result = {'runtime': str(root), 'apply_requested': apply, 'artifacts': [], 'blocked': []}
    with contextlib.ExitStack() as stack:
        root_fd = os.open(root, artifacts.DIRECTORY_FLAGS)
        stack.callback(os.close, root_fd)
        config_fd = os.open('config.json', os.O_RDONLY | os.O_NOFOLLOW, dir_fd=root_fd)
        if not stat.S_ISREG(os.fstat(config_fd).st_mode):
            os.close(config_fd)
            raise ValueError('Expected a regular builder configuration')
        with os.fdopen(config_fd) as stream:
            raw = stream.read(65537)
        if len(raw) > 65536:
            raise ValueError('Oversized builder configuration')
        config = json.loads(raw)
        if not isinstance(config, dict) or config.get('runtime') != str(root):
            raise ValueError('Expected a Dashboard builder config naming this runtime')
        # Applying never stops services itself. Existing owners must be stopped first.
        if apply:
            for name in ('watcher.lock', 'feed.lock'):
                fd = os.open(name, os.O_RDWR | os.O_NOFOLLOW, dir_fd=root_fd)
                stack.callback(os.close, fd)
                if not stat.S_ISREG(os.fstat(fd).st_mode):
                    raise ValueError('Expected a regular owner lock')
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if artifacts.active_builders():
                raise ValueError('Stop build tools and automatic builders before cleanup')
            if not shutil.rmtree.avoids_symlink_attacks:
                raise ValueError('Symlink-resistant removal is unavailable')
        build_fd = os.open('build', artifacts.DIRECTORY_FLAGS, dir_fd=root_fd)
        stack.callback(os.close, build_fd)
        # These are generated source copies, never the active source or source inbox.
        paths = CACHES + ['build/' + name for name in sorted(os.listdir(build_fd))
                          if re.fullmatch(r'source-before-[a-z0-9-]+', name)]
        pending = []
        source = pathlib.Path(config['repo']).resolve(strict=True)
        for relative in paths:
            candidate = root / relative
            if candidate == source or candidate in source.parents or source in candidate.parents:
                raise ValueError('Configured source intersects a cache path: ' + relative)
            try:
                parent = artifacts.parent_fd(root_fd, relative, stack)
                name = pathlib.PurePosixPath(relative).name
                info = os.stat(name, dir_fd=parent, follow_symlinks=False)
                if not stat.S_ISDIR(info.st_mode):
                    raise ValueError('Expected a real cache directory: ' + relative)
                artifacts.check_mounts(root, relative)
                fd = os.open(name, artifacts.DIRECTORY_FLAGS, dir_fd=parent)
                stack.callback(os.close, fd)
                identity = info.st_dev, info.st_ino
                if (os.fstat(fd).st_dev, os.fstat(fd).st_ino) != identity:
                    raise ValueError('Cache changed during inventory')
                row = {'path': relative, 'status': 'ready', **artifacts.inventory(fd)}
                result['artifacts'].append(row)
                pending.append((row, parent, name, identity))
            except FileNotFoundError:
                pass
        # Refuse to discard a locally prepared app import whose outcome is unresolved.
        receipt = root / 'build/app-import-bridge.json'
        if receipt.is_symlink():
            raise ValueError('App import receipt must not be a symlink')
        if receipt.exists() and not json.loads(receipt.read_text()).get('terminal'):
            raise ValueError('Finish or reconcile the pending app import before cleanup')
        # Use the existing active-only release policy. No rollback pins or expiry rules.
        releases = root / 'releases'
        if releases.is_symlink() or (releases / 'current.json').is_symlink():
            raise ValueError('Release store must not be a symlink')
        # The existing pruner uses this lock; requiring it keeps dry runs file-creation-free.
        lock_fd = os.open('releases', artifacts.DIRECTORY_FLAGS, dir_fd=root_fd)
        stack.callback(os.close, lock_fd)
        fd = os.open('.activation.lock', os.O_RDWR | os.O_NOFOLLOW, dir_fd=lock_fd)
        stack.callback(os.close, fd)
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError('Expected a regular activation lock')
        result['releases'] = pruner.prune(releases)
        if not apply:
            return result
        for row, parent, name, identity in pending:
            removing = False
            try:
                if artifacts.active_builders():
                    raise ValueError('A builder started; remaining cleanup stopped')
                info = os.stat(name, dir_fd=parent, follow_symlinks=False)
                if not stat.S_ISDIR(info.st_mode) or (info.st_dev, info.st_ino) != identity:
                    raise ValueError('Cache changed after inventory')
                artifacts.check_mounts(root, row['path'])
                removing = True
                shutil.rmtree(name, dir_fd=parent)
                row['status'] = 'removed'
            except (OSError, ValueError, subprocess.SubprocessError) as error:
                row['status'] = 'incomplete' if removing else 'blocked'
                result['blocked'].append(row['path'] + ': ' + str(error))
                break
        if not result['blocked']:
            if artifacts.active_builders():
                raise ValueError('A builder started; release pruning stopped')
            result['releases'] = pruner.prune(releases, True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=pathlib.Path, default=pathlib.Path.home() /
                        'Library/Application Support/OTriOSDashboard')
    parser.add_argument('--apply', action='store_true', help='Apply after acceptance with builders stopped')
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    try:
        result = clean(args.runtime, args.apply)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, f'Cleanup refused: {error}\n')
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print('APPLY' if args.apply else 'DRY RUN — add --apply after acceptance and stopping builders')
        for row in result['artifacts']:
            print(f"{row['status']:>10}  {row['path']}  {row['allocated_bytes']} allocated bytes")
        releases = result['releases']
        print(f"Releases: keep {releases['active']}; {releases['obsolete']} obsolete, {releases['bytes']} bytes")
        for reason in result['blocked']:
            print('BLOCKED: ' + reason)
    return int(bool(result['blocked']))


if __name__ == '__main__':
    raise SystemExit(main())
