#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run as root on the frame. Controller acknowledges JSON start/end markers on stdin."""
import argparse
import json
import os
import pathlib
import subprocess
import sys
import threading
import time

parser = argparse.ArgumentParser(description='Bounded graphics-service benchmark for the configured RK3126 frame; run as root.')
parser.add_argument('--root', type=pathlib.Path, default=pathlib.Path('/var/lib/otrios-dashboard/bench-adapter'))
ROOT = parser.parse_args().root
UNIT = 'otrios-dashboard-direct.service'
OVERRIDE = pathlib.Path('/run/systemd/system') / (UNIT + '.d') / 'canvas-benchmark.conf'
MATRIX = '0.0141800187463,1.57225922787,0.0301964207517,-1.43183371315,0.0108767110266,0.98226433108'
HZ = os.sysconf('SC_CLK_TCK')

def command(*args):
    return subprocess.check_output(args, text=True).strip()

def emit(value):
    print(json.dumps(value), flush=True)

def acknowledge():
    if sys.stdin.readline().strip() != 'go':
        raise RuntimeError('Controller disconnected')

def wait_ready(path):
    for _ in range(150):
        if path.exists():
            data = json.loads(path.read_text())
            if data['state_confirmed'] and data['stream_ready']:
                return data
        time.sleep(.1)
    raise RuntimeError('Native benchmark did not qualify')

def counters(pid):
    stat = pathlib.Path(f'/proc/{pid}/stat').read_text().split(') ', 1)[1].split()
    total = [int(x) for x in pathlib.Path('/proc/stat').read_text().splitlines()[0].split()[1:9]]
    return (time.monotonic(), int(stat[11]) + int(stat[12]), int(stat[21]) * os.sysconf('SC_PAGE_SIZE'), sum(total), total[3] + total[4])

try:
    for repetition in range(3):
        order = ['baseline', 'merkle'] if repetition % 2 == 0 else ['merkle', 'baseline']
        for label in order:
            folder = ROOT / f'render-{repetition}-{label}'
            folder.mkdir(mode=0o700)
            import pwd
            user = pwd.getpwnam('kiosk')
            os.chown(folder, user.pw_uid, user.pw_gid)
            ready = folder / 'ready.json'
            binary = ROOT / label / 'dashboard-direct'
            port = 18911 if label == 'baseline' else 18912
            OVERRIDE.parent.mkdir(parents=True, exist_ok=True)
            OVERRIDE.write_text(f'[Service]\nEnvironment=OTRIOS_BENCH_PRESENT_LOG={folder}/present.jsonl\nExecStart=\nExecStart={binary} --sync ws://127.0.0.1:{port}/apps/stream --screen three-d-lab --rotation 90 --touch-matrix {MATRIX} --seconds 90 --exit-file {folder}/stop --ready-file {ready} --report {folder}/report.json\n')
            subprocess.run(['systemctl', 'daemon-reload'], check=True)
            subprocess.run(['systemctl', 'restart', UNIT], check=True)
            initial = wait_ready(ready)
            if command('pgrep', '-fc', '^/var/lib/otrios-dashboard/.*/dashboard-(native|direct)') != '1':
                raise RuntimeError('Expected exactly one renderer')
            time.sleep(3)
            pid = initial['pid']
            samples = []
            stopping = threading.Event()
            def sample():
                while not stopping.is_set():
                    samples.append(counters(pid))
                    stopping.wait(.1)
            sampler = threading.Thread(target=sample)
            sampler.start()
            emit({'phase':'start','label':label,'repetition':repetition,'pid':pid})
            acknowledge()
            writer = ROOT / label / 'canvas-adapter-bench'
            direct_port = 18901 if label == 'baseline' else 18902
            process = subprocess.Popen([str(writer),f'ws://127.0.0.1:{direct_port}/apps/stream',str(ROOT/label/'app-catalog.flex'),'150','5','10','writer'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            output, errors = process.stdout.read(), process.stderr.read()
            _, status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(status)
            if process.returncode:
                raise RuntimeError(errors.decode())
            writer_report = json.loads(output)
            time.sleep(2)
            final = json.loads(ready.read_text())
            if final['motion']['count'] != 59 or not final['state_confirmed'] or final['pending_commands']:
                raise RuntimeError('Native renderer did not converge to the final synthetic value')
            stopping.set()
            sampler.join()
            (folder/'stop').touch()
            for _ in range(100):
                if not pathlib.Path(f'/proc/{pid}').exists():break
                time.sleep(.1)
            else:raise RuntimeError('Bounded native host failed to exit')
            first,last=samples[0],samples[-1]
            elapsed=last[0]-first[0]
            cpu={'seconds':elapsed,'process_one_core_percent':100*(last[1]-first[1])/HZ/elapsed,'max_rss_bytes':max(s[2] for s in samples),'system_one_core_percent':100*os.cpu_count()*(1-(last[4]-first[4])/(last[3]-first[3])),'samples':len(samples)}
            present=[json.loads(line) for line in (folder/'present.jsonl').read_text().splitlines()]
            emit({'phase':'end','label':label,'repetition':repetition,'writer':writer_report,'writer_cpu_seconds':usage.ru_utime+usage.ru_stime,'native_cpu':cpu,'present':present,'native_report':json.loads((folder/'report.json').read_text()),'final_ready':final})
            acknowledge()
finally:
    OVERRIDE.unlink(missing_ok=True)
    subprocess.run(['systemctl','daemon-reload'],check=True)
    subprocess.run(['systemctl','restart',UNIT],check=True)
    emit({'phase':'restored','service':command('systemctl','is-active',UNIT)})
