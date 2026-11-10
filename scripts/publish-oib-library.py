#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Publish one signed component-library pin through the unchanged-host app workflow."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import pathlib
import re
import subprocess
import tempfile
from types import SimpleNamespace


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, pathlib.Path(__file__).with_name(file))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


installer = module('library_app_installer', 'install-app.py')


def publish(args):
    key = args.runtime / 'publisher/public.pem'
    private = args.runtime / 'publisher/private.pem'
    baseline = args.baseline or args.runtime / 'releases/current.json'
    pointer = baseline.read_bytes()
    release = installer.activation.verify(args.runtime / 'releases', key, args.openssl,
                                          pointer, bundle_checker=args.checker)
    if args.expected_release and args.expected_release != release:
        raise ValueError('Reviewed library publication baseline changed')
    app = args.runtime / 'releases' / release / 'authority/bundles' / (args.app + '.otapp')

    def tool(*values):
        return subprocess.check_output([str(args.checker), *map(str, values)], text=True).strip()

    before = tool('verify', app, '--key', key, '--openssl', args.openssl).split()
    if len(before) != 3 or before[:2] != ['VERIFIED', args.app]:
        raise ValueError('Installed app verification identity mismatch')
    # Freeze only requested authoring inputs; no app state or keys are copied.
    source = args.source.read_bytes()
    bindings = args.bindings.read_bytes()
    source_metadata = json.loads(source)
    component_name = source_metadata.get('component_wat')
    component_path = None
    component = None
    if component_name is not None:
        relative = pathlib.PurePosixPath(component_name)
        if relative.is_absolute() or '..' in relative.parts or not relative.parts:
            raise ValueError('Component WAT must be a relative source path')
        component_path = args.source.parent.joinpath(*relative.parts)
        component = component_path.read_bytes()
        if len(component) > 64 * 1024:
            raise ValueError('Component WAT input budget exceeded')
    if len(source) > 256 * 1024 or len(bindings) > 256 * 1024:
        raise ValueError('Library authoring input budget exceeded')
    with tempfile.TemporaryDirectory(prefix='oib-library-', dir=args.runtime / 'build') as directory:
        stage = pathlib.Path(directory)
        (stage / 'source.json').write_bytes(source)
        (stage / 'bindings.json').write_bytes(bindings)
        if component is not None:
            staged_component = stage.joinpath(*relative.parts)
            staged_component.parent.mkdir(parents=True, exist_ok=True)
            staged_component.write_bytes(component)
        tool('library-pack', stage / 'source.json', stage / 'unsigned.otlib')
        tool('library-sign', stage / 'unsigned.otlib', stage / 'signed.otlib',
             '--key', private, '--openssl', args.openssl)
        library = tool('library-verify', stage / 'signed.otlib', '--key', key,
                       '--openssl', args.openssl).split()
        tool('library-bind', app, stage / 'signed.otlib', stage / 'bindings.json',
             stage / 'unsigned.otapp', '--key', key, '--openssl', args.openssl)
        tool('sign', stage / 'unsigned.otapp', stage / 'signed.otapp',
             '--key', private, '--openssl', args.openssl)
        candidate = tool('verify', stage / 'signed.otapp', '--key', key,
                         '--openssl', args.openssl).split()
        if (args.source.read_bytes() != source or args.bindings.read_bytes() != bindings
                or (component_path is not None and component_path.read_bytes() != component)):
            raise ValueError('Library authoring source changed during publication')
        result = installer.install(SimpleNamespace(
            runtime=args.runtime, baseline=baseline, bundle=stage / 'signed.otapp',
            remove=None, checker=args.checker, openssl=args.openssl,
            deploy_config=args.deploy_config, expected_release=release,
            expected_bundle=before[2], expected_candidate=candidate[2], expected_app=args.app))
        receipt = {'format': 'otrios-oib-library-publication-v1', 'app': args.app,
                   'library': library[2], 'library_revision': library[3],
                   'source_sha256': hashlib.sha256(source).hexdigest(),
                   'bindings_sha256': hashlib.sha256(bindings).hexdigest(),
                   'component_source_sha256': hashlib.sha256(component).hexdigest() if component is not None else None,
                   'baseline': release, 'release': result['id'],
                   'host_assets_unchanged': True, 'deployed': bool(args.deploy_config)}
        installer.publisher.atomic(args.runtime / 'build/oib-library-publication.json',
                                   (json.dumps(receipt, indent=2) + '\n').encode())
        print(json.dumps(receipt))
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=pathlib.Path, required=True)
    parser.add_argument('--source', type=pathlib.Path, required=True)
    parser.add_argument('--bindings', type=pathlib.Path, required=True)
    parser.add_argument('--app', required=True)
    parser.add_argument('--checker', type=pathlib.Path, required=True)
    parser.add_argument('--baseline', type=pathlib.Path)
    parser.add_argument('--expected-release')
    parser.add_argument('--openssl', default='openssl')
    parser.add_argument('--deploy-config', type=pathlib.Path)
    args = parser.parse_args()
    if not re.fullmatch(r'[a-z0-9-]{1,64}', args.app) or args.app == 'home':
        parser.error('Select an installed app ID other than Home')
    if args.expected_release and not re.fullmatch(r'[a-f0-9]{64}', args.expected_release):
        parser.error('Expected release must be an exact SHA-256')
    with (args.runtime / 'watcher.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        publish(args)


if __name__ == '__main__':
    main()
