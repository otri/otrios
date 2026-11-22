#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Check an activated Frame release in one pinned SSH round trip.

Uses the existing deployment config and signed pointer. Does not restart the
authority, restore data or roll code back. Optional monitoring resumption happens
only after native readiness matches the expected release. Outputs technical
readiness and aggregate service status; never exports state files or pixels.
Use --subscriber-origin for a Frame following a remote HTTPS authority; this
verifies its signed native cache and requires retired hosting roles to stay off.
"""
import argparse
import json
import pathlib
import shlex
import subprocess
import time
import urllib.parse


REMOTE = r'''
import hashlib,json,pathlib,subprocess,time,urllib.request
root=pathlib.Path('/var/lib/otrios-dashboard' if subscriber else '/var/lib/otrios-authority')
for attempt in range(120):
 pointer=json.loads((root/('last-good.json' if subscriber else 'releases/current.json')).read_text())
 assert pointer['id']==expected,'Frame pointer differs from requested release'
 files=list(pathlib.Path('/run/otrios-direct-console/otrios').glob('ready-*.json'))
 ready=json.loads(max(files,key=lambda p:p.stat().st_mtime).read_text()) if files else {}
 if (ready.get('release')==ready.get('ui_release')==expected
     and ready.get('state_confirmed') and ready.get('stream_ready')
     and not ready.get('input_busy') and ready.get('pending_commands')==0):break
 time.sleep(.5)
else:raise RuntimeError('Native readiness deadline')
folder=(root if subscriber else root/'releases')/expected
manifest=json.loads((folder/'manifest.json').read_text())
if subscriber:
 import importlib.util
 spec=importlib.util.spec_from_file_location('agent','/opt/otrios-dashboard/frame-dashboard-agent.py');agent=importlib.util.module_from_spec(spec);spec.loader.exec_module(agent)
 agent.cached(root,expected,pathlib.Path('/opt/otrios-dashboard/publisher.pem'),'/usr/bin/openssl')
native_sha=hashlib.sha256(pathlib.Path('/proc/'+str(ready['pid'])+'/exe').read_bytes()).hexdigest()
assert native_sha==manifest['assets']['native/armv7/dashboard-native']['sha256'],'Native executable differs from active signed release'
health=json.load(urllib.request.urlopen(urllib.request.Request((subscriber or 'http://127.0.0.1:8876')+'/health',headers={'User-Agent':'OTriOS-Release/1'}),timeout=10))
assert health['status']=='ok' and health['release']==expected,'Authority not ready'
if resume:
 subprocess.run(['systemctl','start','otrios-monitoring.timer'],check=True)
 subprocess.run(['systemctl','start','otrios-monitoring.service'],check=True)
 status=subprocess.check_output(['systemctl','show','otrios-monitoring.service','--property=Result','--property=ExecMainStatus'],text=True)
 assert 'Result=success' in status and 'ExecMainStatus=0' in status,'Collector failed'
services={s:subprocess.check_output(['systemctl','is-active',s],text=True).strip()for s in (['otrios-dashboard-direct'] if subscriber else ['otrios-authority','otrios-dashboard-direct','otrios-tunnel']+(['otrios-monitoring.timer']if resume else []))}
old_hosting={}
if subscriber:
 for unit in ['otrios-authority','otrios-tunnel','otrios-monitoring.timer']:
  old_hosting[unit]=subprocess.run(['systemctl','is-active',unit],capture_output=True,text=True).stdout.strip()
  assert old_hosting[unit]!='active','Obsolete Frame hosting role remains active'
 wants=subprocess.check_output(['systemctl','show','otrios-dashboard-direct','-p','Wants','--value'],text=True)
 assert 'otrios-authority' not in wants,'Display still pulls in local authority'
keys=['ui_ref','release','ui_release','runtime_release','pid','state_confirmed','stream_ready','input_busy','pending_commands','renderer','rotation','output','touch_devices','touch_calibrated','scanout_format','color_bits']
print(json.dumps({'passed':True,'release':expected,'state_protocol':manifest['sync_protocol'],'health':health['status'],'ready':{k:ready[k]for k in keys},'native_executable_sha256':native_sha,'native_executable_matches_active_release':True,'services':services,'collector_resumed':resume,'collector_result':'success' if resume else None,'pixels_exported':False,'subscriber_origin':subscriber,'old_hosting_services':old_hosting,'authority_restarted':False,'data_restored':False}))
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--pointer', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--resume-monitoring', action='store_true')
    parser.add_argument('--subscriber-origin', help='Verify a Frame subscribing to this HTTPS authority instead of hosting locally')
    args = parser.parse_args()
    if args.subscriber_origin:
        origin=urllib.parse.urlsplit(args.subscriber_origin)
        if origin.scheme!='https' or not origin.hostname or origin.username or origin.password or origin.path not in ('','/') or origin.query or origin.fragment:
            parser.error('Subscriber origin must be a plain HTTPS origin')
        args.subscriber_origin=args.subscriber_origin.rstrip('/')
        if args.resume_monitoring:parser.error('Subscriber Frame must not restart a local collector')
    config = json.loads(args.config.read_text())
    pointer = args.pointer or pathlib.Path(config['runtime']) / 'build/staged-release.json'
    expected = json.loads(pointer.read_text())['id']
    if len(expected) != 64 or any(c not in '0123456789abcdef' for c in expected):
        parser.error('Expected a canonical release SHA-256')
    script = 'expected=' + repr(expected) + '\nresume=' + repr(args.resume_monitoring) + '\nsubscriber=' + repr(args.subscriber_origin) + '\n' + REMOTE
    started = time.monotonic()
    result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(script)],
                            capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise SystemExit(result.stderr.strip()[-2000:] or 'Frame readiness command failed')
    record = json.loads(result.stdout)
    record['elapsed_seconds'] = round(time.monotonic() - started, 3)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
