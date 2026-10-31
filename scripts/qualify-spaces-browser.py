#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Qualify Space admission in Chromium with disposable state and test credentials.

Requires dashboard-server, bundle_counter/space_ink_probe examples and web pkg.
The signed fixture contains real browser/app bytes; it has no native artifact and
does not establish ARM or Frame deployment acceptance. Never uses live state.
"""
import argparse
import hashlib
import http.client
import json
import pathlib
import shutil
import socket
import subprocess
import tempfile
import time

REPO = pathlib.Path(__file__).resolve().parent.parent
# Public Argon2 reference vector for the literal test password "password":
# argon2 0.5.3 tests/kat.rs reference_argon2id_v0x13_2_16_1 (PHC reference suite).
TEST_HASH = '$argon2id$v=19$m=65536,t=2,p=1$c29tZXNhbHQ$CTFhFdXPJO1aFaMaO6Mm5c8y7cJHAph8ArZWb2GRPPc'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def private(path, value):
    with path.open('x') as output:
        path.chmod(0o600)
        output.write(value)


def port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


def qualify(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
    dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=REPO, text=True).strip())
    protocol_source = (REPO / 'crates/otrios-interfaces/src/session.rs').read_text()
    protocol = protocol_source.split('STATE_PROTOCOL: &str = "')[1].split('"')[0]
    with tempfile.TemporaryDirectory(prefix='otrios-spaces-browser-') as temporary:
        root = pathlib.Path(temporary)
        subprocess.run([str(args.bin_dir / 'examples/bundle_counter'), str(root / 'apps')], check=True, stdout=subprocess.DEVNULL)
        subprocess.run([str(args.bin_dir / 'examples/space_ink_probe'), str(root / 'apps')], check=True)
        subprocess.run([str(args.bin_dir / 'examples/space_chat_probe'), str(root / 'apps')], check=True)
        shutil.copytree(REPO / 'web/canvas', root / 'web', ignore=shutil.ignore_patterns('pkg'))
        shutil.copytree(REPO / 'web/shared', root / 'shared')
        shutil.copytree(args.wasm_pkg, root / 'web/pkg')
        for file in (root / 'shared').rglob('*'):
            if file.is_file():
                destination = root / 'web' / file.relative_to(root / 'shared')
                if not destination.exists():
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(file, destination)
        (root / 'web/fonts').mkdir(exist_ok=True)
        for name in ['Roboto.ttf', 'OFL.txt']:
            shutil.copyfile(REPO / 'assets/fonts/roboto' / name, root / 'web/fonts' / name)
        private(root / 'auth.json', json.dumps({'version': 1, 'users': [
            {'username': name + '@spaces.test', 'identity': name, 'password_hash': TEST_HASH}
            for name in ['alice', 'bob']], 'native': [
                {'identity': 'alice', 'token_sha256': digest(('a' * 64).encode())}]}))
        private(root / 'admin.identity', 'a' * 64)
        private(root / 'provision.json', json.dumps({'version': 1, 'spaces': [
            {'name': 'Browser A', 'admins': ['alice'], 'members': ['bob'], 'apps': [{'id': 'bundle-counter'}, {'id': 'space-ink-probe'}, {'id': 'brass-relay'}]},
            {'name': 'Browser B', 'admins': ['alice'], 'members': [], 'apps': [{'id': 'bundle-counter'}, {'id': 'space-ink-probe'}, {'id': 'brass-relay'}]}]}))
        command = [str(args.bin_dir / 'dashboard-server'), '--apps', str(root / 'apps'),
                   '--app-state', str(root / 'state'), '--spaces-registry', str(root / 'spaces.flex')]
        provision = subprocess.run(command + ['--provision-spaces', str(root / 'provision.json')], check=True, capture_output=True, text=True)
        spaces = json.loads(provision.stdout)['spaces']
        files = {}
        for file in (root / 'web').rglob('*'):
            if file.is_file() and not file.name.endswith('.d.ts'):
                data = file.read_bytes()
                if file.name == 'index.html':
                    data = data.replace(b'href="/style.css"', b'href="./style.css"').replace(b'src="/app.js"', b'src="./app.js"')
                files['web/' + file.relative_to(root / 'web').as_posix()] = data
        files['apps/catalog.flex'] = (root / 'apps/catalog.flex').read_bytes()
        for file in (root / 'apps').glob('*/package/*'):
            if file.is_file():
                files['authority/apps/' + file.parent.parent.name + '/' + file.name] = file.read_bytes()
        item = lambda data: {'bytes': len(data), 'sha256': digest(data)}
        manifest = {'format': 'otrios-release-v1', 'state_schema': 'otrios-canvas-merkle-state-v1', 'sync_protocol': protocol,
                    'assets': {k: item(v) for k, v in files.items() if not k.startswith('authority/')},
                    'authority': {k: item(v) for k, v in files.items() if k.startswith('authority/')}}
        raw = json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()
        release = digest(raw)
        folder = root / 'releases' / release
        folder.mkdir(parents=True)
        for name, data in files.items():
            destination = folder / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        (folder / 'manifest.json').write_bytes(raw)
        subprocess.run(['openssl', 'genpkey', '-algorithm', 'ED25519', '-out', str(root / 'fixture-private.pem')], check=True, capture_output=True)
        subprocess.run(['openssl', 'pkey', '-in', str(root / 'fixture-private.pem'), '-pubout', '-out', str(root / 'fixture-public.pem')], check=True, capture_output=True)
        subprocess.run(['openssl', 'pkeyutl', '-sign', '-rawin', '-inkey', str(root / 'fixture-private.pem'), '-in', str(folder / 'manifest.json'), '-out', str(folder / 'manifest.sig')], check=True, capture_output=True)
        (root / 'releases/current.json').write_text(json.dumps({'id': release, 'web_entry': f'/releases/{release}/web/index.html', 'sync_protocol': protocol}))
        public, internal = port(), port()
        while internal == public:
            internal = port()
        url = f'http://127.0.0.1:{public}'
        fixture = {'root': str(root), 'url': url, 'space_a': spaces[0]['space'], 'space_b': spaces[1]['space'],
                   'release': release, 'source': source, 'source_dirty': dirty, 'output': str(output),
                   'authority_sha256': digest((args.bin_dir / 'dashboard-server').read_bytes()),
                   'wasm_sha256': digest((args.wasm_pkg / 'otrios_dashboard_bg.wasm').read_bytes())}
        (root / 'fixture.json').write_text(json.dumps(fixture))
        with (output / 'authority.log').open('wb') as log:
            server = subprocess.Popen(command + ['--bind', f'127.0.0.1:{internal}', '--public-bind', f'127.0.0.1:{public}',
                                     '--auth-file', str(root / 'auth.json'), '--origin', url, '--web-dir', str(root / 'web'),
                                     '--releases', str(root / 'releases'), '--bundle-key', str(root / 'fixture-public.pem')], stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 10
                while True:
                    try:
                        connection = http.client.HTTPConnection('127.0.0.1', public, timeout=1)
                        connection.request('GET', '/health')
                        if connection.getresponse().status == 200:
                            break
                    except OSError:
                        pass
                    finally:
                        connection.close()
                    if server.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError('Disposable authority failed to become ready; see authority.log')
                    time.sleep(.05)
                subprocess.run(['node', str(REPO / 'scripts/test-spaces-browser.mjs'), str(root / 'fixture.json')], check=True)
            finally:
                if server.poll() is None:
                    server.terminate()
                server.wait(timeout=10)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=pathlib.Path, default=REPO / 'target/debug')
    parser.add_argument('--wasm-pkg', type=pathlib.Path, default=REPO / 'target/spaces-web/pkg')
    parser.add_argument('--output', type=pathlib.Path, default=REPO / 'target/spaces-browser')
    qualify(parser.parse_args())
