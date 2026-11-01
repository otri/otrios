#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Check an activated Frame release in one pinned SSH round trip.

Uses the existing deployment config and signed pointer. Does not restart the
authority, restore data or roll code back. Optional monitoring resumption happens
only after native readiness matches the expected release. Outputs technical
readiness and aggregate service status; never exports state files or pixels.
"""
import argparse
import json
import pathlib
import shlex
import subprocess
import time


REMOTE = r'''
import json,pathlib,subprocess,time,urllib.request
root=pathlib.Path('/var/lib/otrios-authority')
for attempt in range(120):
 pointer=json.loads((root/'releases/current.json').read_text())
 assert pointer['id']==expected,'Frame pointer differs from requested release'
 files=list(pathlib.Path('/run/otrios-direct-console/otrios').glob('ready-*.json'))
 ready=json.loads(max(files,key=lambda p:p.stat().st_mtime).read_text()) if files else {}
 if (ready.get('release')==ready.get('runtime_release')==expected
     and ready.get('state_confirmed') and ready.get('stream_ready')
     and not ready.get('input_busy') and ready.get('pending_commands')==0):break
 time.sleep(.5)
else:raise RuntimeError('Native readiness deadline')
health=json.load(urllib.request.urlopen('http://127.0.0.1:8876/health',timeout=5))
assert health['status']=='ok' and health['release']==expected,'Authority not ready'
if resume:
 subprocess.run(['systemctl','start','otrios-monitoring.timer'],check=True)
 subprocess.run(['systemctl','start','otrios-monitoring.service'],check=True)
 status=subprocess.check_output(['systemctl','show','otrios-monitoring.service','--property=Result','--property=ExecMainStatus'],text=True)
 assert 'Result=success' in status and 'ExecMainStatus=0' in status,'Collector failed'
services={s:subprocess.check_output(['systemctl','is-active',s],text=True).strip()for s in ['otrios-authority','otrios-dashboard-direct','otrios-tunnel']+(['otrios-monitoring.timer']if resume else [])}
keys=['ui_ref','release','runtime_release','state_confirmed','stream_ready','input_busy','pending_commands','renderer','rotation','output','touch_devices','touch_calibrated','scanout_format','color_bits']
print(json.dumps({'passed':True,'release':expected,'state_protocol':pointer['sync_protocol'],'health':health['status'],'ready':{k:ready[k]for k in keys},'services':services,'collector_resumed':resume,'collector_result':'success' if resume else None,'pixels_exported':False,'authority_restarted':False,'data_restored':False}))
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--pointer', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--resume-monitoring', action='store_true')
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    pointer = args.pointer or pathlib.Path(config['runtime']) / 'build/staged-release.json'
    expected = json.loads(pointer.read_text())['id']
    if len(expected) != 64 or any(c not in '0123456789abcdef' for c in expected):
        parser.error('Expected a canonical release SHA-256')
    script = 'expected=' + repr(expected) + '\nresume=' + repr(args.resume_monitoring) + '\n' + REMOTE
    started = time.monotonic()
    result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(script)],
                            check=True, capture_output=True, text=True, timeout=180)
    record = json.loads(result.stdout)
    record['elapsed_seconds'] = round(time.monotonic() - started, 3)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
