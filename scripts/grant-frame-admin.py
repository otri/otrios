#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Apply an explicitly authorized Frame membership grant using offline provisioning.

Uses the existing native identity and authenticated operator principal. Private
plans, pre-change metadata and logs remain on Frame. Never edits state payloads,
auth configuration, audiences or touch calibration, and never restores snapshots.
"""
import argparse
import json
import pathlib
import shlex
import subprocess

REMOTE = r'''
import hashlib,json,os,pathlib,shutil,subprocess,time,urllib.request
os.umask(0o077)
root=pathlib.Path('/var/lib/otrios-authority')
token=pathlib.Path('/etc/otrios-dashboard.identity').read_text().strip()
auth=pathlib.Path('/etc/otrios-authority/monitoring-auth.json')
configuration=json.loads(auth.read_text())
native=next(x['identity']for x in configuration['native']if x['token_sha256']==hashlib.sha256(token.encode()).hexdigest())
if login:
 matches=[x.get('identity',x['username'])for x in configuration['users']if x['username'].lower()==login.lower()]
 assert len(matches)==1,'Expected one configured operator login'
 actor=matches[0]
def metadata():
 req=urllib.request.Request('http://127.0.0.1:8877/spaces',headers={'Authorization':'Bearer '+token,'Origin':'https://otrios.otri.ca'})
 for attempt in range(100):
  try:return json.load(urllib.request.urlopen(req,timeout=5))
  except OSError:
   if attempt==99:raise
   time.sleep(.1)
before=metadata()
selected=[]
for index,name in enumerate(names):
 matches=[s for s in before['spaces']if s['name']==name]
 assert len(matches)<=1,'Ambiguous visible Space name'
 expected=expected_ids[index]if expected_ids else None
 if matches:
  assert expected is None or matches[0]['id']==expected,'Visible Space identity changed'
  selected.append(matches[0])
 else:
  assert expected,'Use --space-id for an existing Space not visible to Frame'
  # A supplied ID comes from an authenticated current UI. Also require a prior
  # authority provisioning record, so this maintenance path cannot bootstrap a
  # new Space merely because the device cannot see it yet.
  records=[]
  for path in root.glob('spaces.flex.provisioned-*.json'):
   records.extend(json.loads(path.read_text()).get('spaces',[]))
  collector=root/'collector-binding.json'
  service=json.loads(collector.read_text()).get('binding',{})if collector.exists()else{}
  assert (any(s.get('space')==expected and s.get('name')==name for s in records)
          or service.get('space')==expected),'Existing Space provisioning or service binding not found'
  selected.append({'id':expected,'name':name})
assert len(set(s['id']for s in selected))==len(names),'Ambiguous Space selection'
plan={'version':2,'spaces':[{'name':s['name'],'admins':[actor,native],'members':[],'apps':[]}for s in selected]}
work=root/('provision-'+tag);work.mkdir(mode=0o700,exist_ok=True)
manifest=work/'membership.json'
raw=(json.dumps(plan,sort_keys=True)+'\n').encode()
if manifest.exists():assert manifest.read_bytes()==raw,'Existing grant plan differs'
else:manifest.write_bytes(raw)
shutil.chown(work,user='otrios',group='otrios');shutil.chown(manifest,user='otrios',group='otrios');manifest.chmod(0o600)
services=['otrios-monitoring.timer','otrios-monitoring.service','otrios-dashboard-direct','otrios-authority']
active={s:subprocess.run(['systemctl','is-active','--quiet',s]).returncode==0 for s in services}
def inventory():
 return {str(p.relative_to(root/'state.apps')):hashlib.sha256(p.read_bytes()).hexdigest()for p in (root/'state.apps').rglob('*')if p.is_file()}
try:
 for service in services:subprocess.run(['systemctl','stop',service],check=True)
 backup=root/'rollback'/tag;backup.mkdir(mode=0o700,parents=True,exist_ok=True)
 for name in ['spaces.flex','spaces.flex.latest','spaces.flex.owner.lock','collector-binding.json']:
  destination=backup/name
  if not destination.exists():shutil.copy2(root/name,destination)
 state=inventory();auth_bytes=auth.read_bytes();touch=pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules').read_bytes()
 with (work/'provision.log').open('ab')as log:
  result=subprocess.run(['runuser','-u','otrios','--','/opt/otrios-authority/dashboard-server',
   '--spaces-registry',str(root/'spaces.flex'),'--app-state',str(root/'state.apps'),
   '--releases',str(root/'releases'),'--provision-spaces',str(manifest)],cwd=root,stdout=log,stderr=log)
 assert result.returncode==0,'Offline grant failed; private provisioning log retained on Frame'
 assert inventory()==state,'App state changed during membership-only provisioning'
 assert auth.read_bytes()==auth_bytes,'Auth configuration changed'
 assert pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules').read_bytes()==touch,'Calibration changed'
finally:
 for service in reversed(services):
  if active[service]:subprocess.run(['systemctl','start',service],check=True)
after=metadata()
for old in selected:
 new=next(s for s in after['spaces']if s['id']==old['id'])
 assert new['role']=='admin','Native Admin grant missing'
 for key in ['id','name','layout','audience']:
  if key in old:assert new[key]==old[key],'Unexpected Space configuration change'
 if 'apps'in old:
  old_apps=[a for a in old['apps']if a['attached']];new_apps=[a for a in new['apps']if a['attached']]
  assert old_apps==new_apps,'Active collection bindings changed'
for old in before['spaces']:
 if old['id']in [s['id']for s in selected]:continue
 new=next(s for s in after['spaces']if s['id']==old['id'])
 for key in ['id','name','archived','layout','audience','role','apps','members','retained']:
  if key in old:assert new[key]==old[key],'Unrelated Space changed'
print(json.dumps({'passed':True,'spaces_granted':len(names),'existing_native_identity':True,
 'active_collections_preserved':True,'app_state_byte_identical_during_grant':True,
 'auth_configuration_unchanged':True,'touch_calibration_unchanged':True,
 'audiences_and_layout_unchanged':True,'data_restored':False,'private_metadata_exported':False}))
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    operator=parser.add_mutually_exclusive_group(required=True)
    operator.add_argument('--actor')
    operator.add_argument('--login',help='Resolve an existing human actor from private credential configuration')
    parser.add_argument('--space', action='append', required=True)
    parser.add_argument('--space-id',action='append',help='Expected existing ID for each --space, in the same order')
    parser.add_argument('--tag', required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    import re
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}-[a-z0-9-]+', args.tag):
        parser.error('Use an ISO-date-first maintenance tag')
    if args.space_id and (len(args.space_id)!=len(args.space)or any(not re.fullmatch(r'[a-f0-9]{32}',v)for v in args.space_id)):
        parser.error('Supply one canonical --space-id per --space')
    config = json.loads(args.config.read_text())
    values = {'actor': args.actor,'login':args.login,'expected_ids':args.space_id, 'names': args.space, 'tag': args.tag}
    script = '\n'.join(k + '=' + repr(v) for k, v in values.items()) + '\n' + REMOTE
    result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(script)],
                            capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise SystemExit(result.stderr.strip())
    record = json.loads(result.stdout)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
