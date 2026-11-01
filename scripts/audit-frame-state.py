#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Compare a private on-Frame deployment baseline; export aggregate results only."""
import argparse
import json
import pathlib
import re
import shlex
import subprocess

REMOTE = r'''
import hashlib,json,os,pathlib,urllib.request
os.umask(0o077)
root=pathlib.Path('/var/lib/otrios-authority')
work=root/'deployment-audits'/tag
work.mkdir(parents=True,mode=0o700,exist_ok=True)
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
token=pathlib.Path('/etc/otrios-dashboard.identity').read_text().strip()
request=urllib.request.Request('http://127.0.0.1:8877/spaces',headers={'Authorization':'Bearer '+token})
metadata=json.load(urllib.request.urlopen(request,timeout=10))
for space in metadata['spaces']:space.pop('cleanup',None)
collector=json.loads((root/'collector-binding.json').read_text())['binding']['collection']
files={str(p.relative_to(root/'state.apps')):digest(p)for p in (root/'state.apps').rglob('*')if p.is_file()and not p.name.endswith('.owner.lock')and not str(p.relative_to(root/'state.apps')).startswith('collection-'+collector+'.')}
current={'metadata':metadata,'files':files}
for name,path in [('auth','/etc/otrios-authority/monitoring-auth.json'),('identity','/etc/otrios-dashboard.identity'),('calibration','/etc/udev/rules.d/99-spd-touch.rules'),('collector_binding',str(root/'collector-binding.json'))]:current[name]=digest(pathlib.Path(path))
baseline=work/'before.json'
if phase=='before':
 with baseline.open('x')as out:json.dump(current,out,sort_keys=True)
 result={'captured':True,'private_metadata_exported':False}
else:
 before=json.loads(baseline.read_text())
 assert before['metadata']==current['metadata'],'Space lifecycle, layout, membership or collection metadata changed'
 assert before['files']==current['files'],'Confirmed non-collector payloads changed'
 for name in ['auth','identity','calibration','collector_binding']:assert before[name]==current[name],'Deployment configuration changed'
 assert all((root/name).read_bytes().startswith(framing.encode())for name in ['spaces.flex','spaces.flex.latest']),'Unexpected registry framing'
 result={'passed':True,'space_metadata_preserved':True,'confirmed_noncollector_files_byte_identical':len(files),'auth_identity_calibration_and_collector_binding_preserved':True,'registry_framing':framing,'private_metadata_exported':False,'data_restored':False}
 (work/'after.json').write_text(json.dumps(current,sort_keys=True))
 (work/'aggregate.json').write_text(json.dumps(result))
print(json.dumps(result))
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--phase', choices=['before', 'after'], required=True)
    parser.add_argument('--framing', default='OTSP8')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}-[a-z0-9-]+', args.tag) or not re.fullmatch(r'OTSP[0-9]', args.framing):
        parser.error('Use an ISO-date-first tag and an explicit registry framing')
    config = json.loads(args.config.read_text())
    setup = f'tag={args.tag!r}\nphase={args.phase!r}\nframing={args.framing!r}\n'
    result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(setup + REMOTE)], capture_output=True, text=True, timeout=60)
    if result.returncode:
        # Do not echo remote tracebacks that might identify private payload paths.
        raise SystemExit('Frame state audit failed; inspect its private baseline on Frame')
    record = json.loads(result.stdout)
    args.output.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
