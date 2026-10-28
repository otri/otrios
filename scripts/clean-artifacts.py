#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Inventory or remove a fixed set of disposable OTriOS artifacts (Python 3.11+)."""
import argparse
import contextlib
import json
import os
import pathlib
import re
import shutil
import stat
import subprocess
import sys
import tomllib

ARTIFACTS = {
    'incremental': 'target/debug/incremental',
    'python': 'scripts/__pycache__',
    'legacy-web': 'web/dashboard/pkg',
}
BUILD_TOOLS = {'cargo', 'rustc', 'rustdoc', 'clippy-driver', 'cargo-clippy', 'cargo-watch', 'wasm-bindgen', 'wasm-pack'}
BUILD_SCRIPTS = re.compile(r'(?:^|/|\s)(?:watch-dashboard|build-canvas-pipeline-benchmark|build-dashboard-web|build-dashboard-armv7)\.(?:py|sh)(?:\s|$)')
DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW


def git(root, *args):
    result = subprocess.run(['git', '-C', str(root), *args], capture_output=True, check=True)
    return result.stdout


def repository(path):
    root = pathlib.Path(path).resolve(strict=True)
    top = pathlib.Path(os.fsdecode(git(root, 'rev-parse', '--show-toplevel')).strip()).resolve()
    if top != root:
        raise ValueError('--repo must name the checkout root')
    manifest = root / 'crates/otrios-core/Cargo.toml'
    with manifest.open('rb') as stream:
        name = tomllib.load(stream).get('package', {}).get('name')
    if name != 'otrios-core' or not (root / 'scripts/check.sh').is_file():
        raise ValueError('Expected an OTriOS checkout')
    return root


def active_builders():
    """Conservative, system-wide check; callers must still stop automated builders."""
    commands = subprocess.run(['ps', '-A', '-o', 'pid=', '-o', 'comm='], capture_output=True, text=True, check=True)
    arguments = subprocess.run(['ps', '-A', '-o', 'pid=', '-o', 'args='], capture_output=True, text=True, check=True)
    args = dict(line.strip().split(None, 1) for line in arguments.stdout.splitlines() if len(line.strip().split(None, 1)) == 2)
    busy = []
    for line in commands.stdout.splitlines():
        fields = line.strip().split(None, 1)
        if len(fields) != 2 or int(fields[0]) == os.getpid():
            continue
        pid, command = fields
        name = pathlib.Path(command).name
        wrapper = name.lower().startswith(('python', 'bash', 'zsh', 'sh')) and BUILD_SCRIPTS.search(args.get(pid, ''))
        if name in BUILD_TOOLS or wrapper:
            busy.append(f'{pid}: {name}')
    return busy


def parent_fd(root_fd, relative, stack):
    fd = os.dup(root_fd)
    stack.callback(os.close, fd)
    for part in pathlib.PurePosixPath(relative).parts[:-1]:
        fd = os.open(part, DIRECTORY_FLAGS, dir_fd=fd)
        stack.callback(os.close, fd)
    return fd


def mount_points():
    """Include same-device bind mounts, which st_dev alone cannot identify."""
    if sys.platform.startswith('linux'):
        lines = pathlib.Path('/proc/self/mountinfo').read_text().splitlines()
        fields = [line.split() for line in lines]
        if not fields or any(len(row) < 7 for row in fields):
            raise ValueError('Cannot parse mount table')
        paths = [row[4] for row in fields]
        return [pathlib.Path(re.sub(r'\\([0-7]{3})', lambda match: chr(int(match[1], 8)), path)) for path in paths]
    if sys.platform == 'darwin':
        output = subprocess.run(['/sbin/mount'], capture_output=True, text=True, check=True).stdout
        matches = [re.fullmatch(r'.+ on (.+) \([^\n]+\)', line) for line in output.splitlines()]
        if not matches or not all(matches):
            raise ValueError('Cannot parse mount table')
        return [pathlib.Path(match[1]) for match in matches]
    raise ValueError('Mount inspection supports macOS and Linux only')


def check_mounts(root, relative):
    candidate = root / relative
    for mount in mount_points():
        if mount.is_relative_to(candidate) or (mount != root and mount.is_relative_to(root) and candidate.is_relative_to(mount)):
            raise ValueError(f'Mounted filesystem intersects artifact path: {mount}')


def inventory(fd):
    files = logical = allocated = 0
    seen = set()
    device = os.fstat(fd).st_dev

    def scan(directory):
        nonlocal files, logical, allocated
        info = os.fstat(directory)
        if info.st_dev != device:
            raise ValueError('Filesystem boundary in artifact directory')
        identity = info.st_dev, info.st_ino
        if identity not in seen:
            seen.add(identity)
            allocated += info.st_blocks * 512
        with os.scandir(directory) as entries:
            for entry in entries:
                info = entry.stat(follow_symlinks=False)
                if info.st_dev != device:
                    raise ValueError('Filesystem boundary in artifact directory')
                if stat.S_ISDIR(info.st_mode):
                    child = os.open(entry.name, DIRECTORY_FLAGS, dir_fd=directory)
                    try:
                        if (os.fstat(child).st_dev, os.fstat(child).st_ino) != (info.st_dev, info.st_ino):
                            raise ValueError('Directory changed during inventory')
                        scan(child)
                    finally:
                        os.close(child)
                elif stat.S_ISREG(info.st_mode):
                    files += 1
                    logical += info.st_size
                    identity = info.st_dev, info.st_ino
                    if identity not in seen:
                        seen.add(identity)
                        allocated += info.st_blocks * 512
                else:
                    raise ValueError('Symlink or special file in artifact directory')

    scan(fd)
    return {'files': files, 'logical_bytes': logical, 'allocated_bytes': allocated}


def clean(path, names=None, apply=False):
    root = repository(path)
    names = list(dict.fromkeys(names or ARTIFACTS))
    if any(name not in ARTIFACTS for name in names):
        raise ValueError('Unknown artifact selection')
    report = {'repo': str(root), 'apply_requested': apply, 'artifacts': [], 'blocked': []}
    pending = []
    with contextlib.ExitStack() as stack:
        root_fd = os.open(root, DIRECTORY_FLAGS)
        stack.callback(os.close, root_fd)
        for key in names:
            relative = ARTIFACTS[key]
            row = {'name': key, 'path': relative, 'status': 'absent', 'files': 0, 'logical_bytes': 0, 'allocated_bytes': 0}
            report['artifacts'].append(row)
            try:
                if git(root, 'ls-files', '-z', '--', relative):
                    raise ValueError('Artifact path contains tracked files')
                parent = parent_fd(root_fd, relative, stack)
                name = pathlib.PurePosixPath(relative).name
                info = os.stat(name, dir_fd=parent, follow_symlinks=False)
                if not stat.S_ISDIR(info.st_mode):
                    raise ValueError('Artifact path is not a real directory')
                ignored = subprocess.run(['git', '-C', str(root), 'check-ignore', '-q', '--', relative], capture_output=True)
                if ignored.returncode != 0:
                    raise ValueError('Artifact path is not ignored by Git')
                check_mounts(root, relative)
                fd = os.open(name, DIRECTORY_FLAGS, dir_fd=parent)
                stack.callback(os.close, fd)
                opened = os.fstat(fd)
                if opened.st_dev != os.fstat(root_fd).st_dev:
                    raise ValueError('Artifact is on a different filesystem from checkout')
                if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
                    raise ValueError('Artifact directory changed during inventory')
                row['status'] = 'ready'
                row.update(inventory(fd))
                pending.append((row, parent, name, (opened.st_dev, opened.st_ino)))
            except FileNotFoundError:
                if row['status'] == 'ready':
                    row['status'] = 'blocked'
                    report['blocked'].append(f'{relative}: Artifact changed during inventory')
            except (OSError, ValueError, subprocess.SubprocessError) as error:
                row['status'] = 'blocked'
                report['blocked'].append(f'{relative}: {error}')
        if pending:
            try:
                busy = active_builders()
                if busy:
                    report['blocked'].append('Stop active builders first: ' + ', '.join(busy))
            except (OSError, subprocess.SubprocessError) as error:
                report['blocked'].append(f'Cannot inspect build processes: {error}')
        if apply and pending and not getattr(shutil.rmtree, 'avoids_symlink_attacks', False):
            report['blocked'].append('This Python platform lacks symlink-resistant directory removal')
        if apply and not report['blocked']:
            free_before = shutil.disk_usage(root).free
            for row, parent, name, identity in pending:
                removing = False
                try:
                    if active_builders():
                        raise ValueError('A builder started; remaining cleanup stopped')
                    if git(root, 'ls-files', '-z', '--', row['path']):
                        raise ValueError('Tracked files appeared; remaining cleanup stopped')
                    if subprocess.run(['git', '-C', str(root), 'check-ignore', '-q', '--', row['path']], capture_output=True).returncode != 0:
                        raise ValueError('Ignore rules changed; remaining cleanup stopped')
                    info = os.stat(name, dir_fd=parent, follow_symlinks=False)
                    if not stat.S_ISDIR(info.st_mode) or (info.st_dev, info.st_ino) != identity:
                        raise ValueError('Artifact directory changed; remaining cleanup stopped')
                    check_mounts(root, row['path'])
                    removing = True
                    shutil.rmtree(name, dir_fd=parent)
                    row['status'] = 'removed'
                except (OSError, ValueError, subprocess.SubprocessError) as error:
                    row['status'] = 'incomplete' if removing else 'blocked'
                    report['blocked'].append(f"{row['path']}: {error}")
                    break
            report['filesystem_free_bytes_change'] = shutil.disk_usage(root).free - free_before
    report['completed_removed_allocated_bytes'] = sum(row['allocated_bytes'] for row in report['artifacts'] if row['status'] == 'removed')
    report['incomplete_removals'] = [row['path'] for row in report['artifacts'] if row['status'] == 'incomplete']
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1], help='OTriOS checkout root (defaults to this script\'s checkout)')
    parser.add_argument('--only', choices=ARTIFACTS, action='append', help='Select a group; repeat to combine groups (default: all three)')
    parser.add_argument('--apply', action='store_true', help='Remove eligible artifacts; default is a read-only dry run')
    parser.add_argument('--json', action='store_true', help='Emit a machine-readable inventory/result')
    args = parser.parse_args()
    try:
        result = clean(args.repo, args.only, args.apply)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f'Cleanup refused: {error}\n')
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print('APPLY' if args.apply else 'DRY RUN — add --apply to remove these artifacts')
        for row in result['artifacts']:
            print(f"{row['status']:>7}  {row['path']}  {row['files']} files, {row['allocated_bytes'] / 1024**2:.2f} MiB allocated")
        for reason in result['blocked']:
            print('BLOCKED: ' + reason)
        if args.apply:
            print(f"Completed removals: {result['completed_removed_allocated_bytes']} allocated bytes; filesystem free-space change: {result.get('filesystem_free_bytes_change', 0)} bytes")
            if result['incomplete_removals']:
                print('Incomplete removals: deleted byte count is unknown for ' + ', '.join(result['incomplete_removals']))
    return int(bool(result['blocked']))


if __name__ == '__main__':
    raise SystemExit(main())
