#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Exclusive RK3126 text replay; restores the prior service and console on exit.

Run on the frame with an explicitly hashed development executable and synthetic
core-produced fixture. This neither installs a release nor changes shared state.
"""
import argparse
import fcntl
import hashlib
import json
import os
import pathlib
import pwd
import signal
import struct
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--executable', type=pathlib.Path, required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--fixture', type=pathlib.Path, required=True)
    parser.add_argument('--fixture-sha256', required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--scale', choices=['0', '2', '3'], action='append')
    parser.add_argument('--mode', choices=['static', 'scroll'], default='scroll')
    parser.add_argument('--repeat', type=int, default=3)
    parser.add_argument('--minimum-fps', type=float, default=30)
    args = parser.parse_args()
    if os.geteuid() != 0 or not 1 <= args.repeat <= 10:
        parser.error('Root and 1..10 repeats required')
    for path, digest in [(args.executable, args.sha256), (args.fixture, args.fixture_sha256)]:
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            parser.error(f'Digest mismatch: {path}')
    args.executable = args.executable.resolve()
    args.fixture = args.fixture.resolve()
    for sig in [signal.SIGINT, signal.SIGTERM, signal.SIGHUP]:
        signal.signal(sig, lambda n, _: (_ for _ in ()).throw(SystemExit(128 + n)))
    lock = open('/run/otrios-display-trial.lock', 'a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    service = 'otrios-dashboard-direct.service'
    active = subprocess.run(['systemctl', 'is-active', '--quiet', service]).returncode == 0
    args.output.mkdir(parents=True, exist_ok=False)
    kiosk = pwd.getpwnam('kiosk')
    os.chown(args.output, kiosk.pw_uid, kiosk.pw_gid)
    args.output.chmod(0o700)
    console = saved = None
    results = []
    try:
        subprocess.run(['systemctl', 'stop', service], check=True)
        console = os.open('/dev/tty1', os.O_RDWR)
        mode = bytearray(4)
        fcntl.ioctl(console, 0x4b3b, mode)
        saved = struct.unpack('i', mode)[0]
        fcntl.ioctl(console, 0x4b3a, 1)
        for scale in args.scale or ['2']:
            for run in range(args.repeat):
                name = f'{args.mode}-{scale}x-{run}'
                command = ['runuser', '-u', 'kiosk', '--', 'env',
                           'OTRIOS_TEXT_PROFILE=1', f'OTRIOS_TEXT_ATLAS_SCALE={scale}',
                           str(args.executable), str(args.fixture), args.mode,
                           str(args.output / (name + '.ppm'))]
                with (args.output / (name + '.log')).open('w') as log:
                    child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT,
                                             start_new_session=True)
                    try:
                        if child.wait(timeout=120):
                            raise RuntimeError((args.output / (name + '.log')).read_text())
                    finally:
                        if child.poll() is None:
                            os.killpg(child.pid, signal.SIGTERM)
                            try:
                                child.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                os.killpg(child.pid, signal.SIGKILL)
                                child.wait()
                lines = (args.output / (name + '.log')).read_text().splitlines()
                if not any('GL renderer: Mali400' in line for line in lines):
                    raise RuntimeError('Acceptance requires actual Mali400 rendering')
                report = json.loads(next(line for line in lines if line.startswith('{')))
                report.update(scale=int(scale), run=run)
                results.append(report)
                print(json.dumps(report), flush=True)
    finally:
        if console is not None:
            if saved is not None:
                fcntl.ioctl(console, 0x4b3a, saved)
            os.close(console)
        if active:
            subprocess.run(['systemctl', 'start', service], check=True)
        report = {'binary_sha256': args.sha256, 'fixture_sha256': args.fixture_sha256,
                  'measured_at_unix': time.time(), 'mode': args.mode, 'runs': results,
                  'minimum_fps': args.minimum_fps,
                  'passed': bool(results) and all(r['kernel_fps'] >= args.minimum_fps for r in results)}
        (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print('COMPLETE; previous display restored', flush=True)
    if not report['passed']:
        raise SystemExit('Frame-rate target not met')


if __name__ == '__main__':
    main()
