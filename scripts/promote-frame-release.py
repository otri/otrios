#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Promote the validated Frame release to the Mac pointer and exact source pin."""
import argparse
import importlib.util
import json
import pathlib
import re
import shlex
import shutil
import subprocess


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--release', required=True)
    parser.add_argument('--source', required=True, help='Validated implementation commit')
    parser.add_argument('--backup-tag', required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[a-f0-9]{64}', args.release) or not re.fullmatch(r'[a-f0-9]{40}', args.source) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}-[a-z0-9-]+', args.backup_tag):
        parser.error('Use exact release/source hashes and an ISO-date-first backup tag')
    config = json.loads(args.config.read_text())
    repo, runtime = pathlib.Path(config['repo']).resolve(), pathlib.Path(config['runtime']).resolve()
    if runtime == repo or repo in runtime.parents:
        parser.error('Private release control must be outside the repository')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    if head != args.source:
        raise SystemExit('Source pin must name the current implementation commit')
    dirty = subprocess.check_output(['git', 'diff', '--name-only', 'HEAD', '--', 'crates', 'scripts', 'web', 'apps', 'tools', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'], cwd=repo)
    untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '--', 'crates', 'scripts', 'web', 'apps', 'tools'], cwd=repo)
    if dirty or untracked:
        raise SystemExit('Commit validated implementation inputs before pinning the watcher')
    activation = module('activation', repo / 'scripts/activate-dashboard-release.py')
    watcher = module('watcher', repo / 'scripts/watch-dashboard.py')
    if watcher.fingerprint(repo) != watcher.fingerprint(runtime / 'build/source'):
        raise SystemExit('Staged source differs from the implementation checkout')
    pointer = (runtime / 'build/staged-release.json').read_bytes()
    actual = activation.verify(runtime / 'releases', runtime / 'publisher/public.pem', config['openssl'], pointer, bundle_checker=runtime / 'build/mac-target/debug/canvas-bundle')
    if actual != args.release:
        raise SystemExit('Staged signed release differs from the expected release')
    remote = "import json,pathlib;print(json.dumps([json.loads(pathlib.Path(p).read_text())['id']for p in ['/var/lib/otrios-authority/releases/current.json','/var/lib/otrios-dashboard/last-good.json']]))"
    active = json.loads(subprocess.check_output(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(remote)], text=True, timeout=30))
    if active != [actual, actual]:
        raise SystemExit('Frame authority and native reader have not accepted the expected release')
    backup = runtime / 'rollback' / args.backup_tag
    if not (backup / 'pre-cutover.tar').is_file():
        raise SystemExit('The paired private pre-cutover backup is required')
    controls = [(runtime / 'config.json', 'mac-config-before.json'), (runtime / 'releases/current.json', 'mac-pointer-before.json'), (runtime / 'build/source-id', 'mac-source-id-before')]
    if any((backup / name).exists() for _, name in controls):
        raise SystemExit('Promotion baseline already exists; inspect it before retrying')
    for source, name in controls:
        if source.exists():
            shutil.copy2(source, backup / name)
            (backup / name).chmod(0o600)
    current = json.loads((runtime / 'config.json').read_text())
    current['git_branch'], current['node'] = args.source, config['node']
    current['build_env'].update(config['build_env'])
    activation.agent.atomic(runtime / 'config.json', (json.dumps(current, indent=2) + '\n').encode(), 0o600)
    activation.agent.atomic(runtime / 'releases/current.json', pointer, 0o600)
    activation.agent.atomic(runtime / 'build/source-id', watcher.fingerprint(runtime / 'build/source').encode(), 0o600)
    record = {'passed': True, 'release': actual, 'source_pin': args.source, 'mac_and_frame_pointers_match': True, 'previous_control_files_preserved_privately': True}
    args.output.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
