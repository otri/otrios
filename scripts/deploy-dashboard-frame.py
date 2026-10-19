#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Transfer a verified release to an installed frame authority over pinned SSH."""
import argparse, importlib.util, io, json, pathlib, shlex, subprocess, tarfile

REMOTE = r'''
import hashlib,importlib.util,io,json,os,pathlib,shutil,subprocess,sys,tarfile,tempfile,time,urllib.request
root=pathlib.Path('/var/lib/otrios-authority');binary=pathlib.Path('/opt/otrios-authority')
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
with tempfile.TemporaryDirectory(prefix='.incoming-',dir=root) as tmp:
 stage=pathlib.Path(tmp);total=0
 with tarfile.open(fileobj=sys.stdin.buffer,mode='r|') as archive:
  for item in archive:
   path=pathlib.PurePosixPath(item.name)
   if path.is_absolute() or '..' in path.parts or not (item.isfile() or item.isdir()):raise ValueError('Unsafe package path')
   total+=item.size
   if item.size>40*1024*1024 or total>160*1024*1024:raise ValueError('Package budget exceeded')
   dest=stage/path
   if item.isdir():dest.mkdir(parents=True,exist_ok=True)
   else:
    dest.parent.mkdir(parents=True,exist_ok=True)
    with archive.extractfile(item) as src,dest.open('wb') as out:shutil.copyfileobj(src,out)
 activation=module('activation',stage/'activate-dashboard-release.py')
 pointer=(stage/'current.json').read_bytes();key=pathlib.Path('/etc/otrios-authority/publisher.pem')
 release=activation.verify(stage/'releases',key,'openssl',pointer)
 checks=json.loads((stage/'transfer.json').read_text())
 for name,digest in checks.items():
  if hashlib.sha256((stage/name).read_bytes()).hexdigest()!=digest:raise ValueError('Transfer digest mismatch')
 store=root/'releases';store.mkdir(exist_ok=True)
 import fcntl
 with (store/'.activation.lock').open('a') as lock:
  fcntl.flock(lock,fcntl.LOCK_EX)
  destination=store/release
  if destination.exists():
   activation.verify(store,key,'openssl',pointer)
  else:os.replace(stage/'releases'/release,destination)
  shutil.chown(destination,user='otrios',group='otrios')
  for path in destination.rglob('*'):shutil.chown(path,user='otrios',group='otrios')
  restart=False
  for name in checks:
   dest=(pathlib.Path('/opt/otrios-dashboard') if name in ('frame-dashboard-agent.py','session-bootstrap') else binary)/name
   data=(stage/name).read_bytes();changed=not dest.exists() or dest.read_bytes()!=data
   if changed:activation.agent.atomic(dest,data,0o755)
   if name=='dashboard-server':restart=changed
  activation.agent.atomic(store/'current.json',pointer)
  shutil.chown(store/'current.json',user='otrios',group='otrios')
 if restart:subprocess.run(['systemctl','restart','otrios-authority.service'],check=True)
 for attempt in range(30):
  try:
   with urllib.request.urlopen('http://127.0.0.1:8876/health',timeout=2) as response:health=json.load(response)
   if health['status']=='ok':break
  except Exception:pass
  time.sleep(.2)
 else:raise RuntimeError('Authority did not recover')
 activation.notify('http://127.0.0.1:8876/admin/activate')
 pruner=module('pruner',stage/'prune-dashboard-releases.py');print('FRAME',json.dumps(pruner.prune(store,True)))
 native=pathlib.Path('/var/lib/otrios-dashboard')
 for attempt in range(50):
  try:
   if json.loads((native/'last-good.json').read_text())['id']==release:
    print('NATIVE',json.dumps(pruner.prune(native,True,True)));break
  except (OSError,ValueError,KeyError):pass
  time.sleep(.2)
 else:print('Native release still activating; its running cache was retained')
'''

def deploy(config,pointer,server,monitoring,bootstrap):
    repo=pathlib.Path(__file__).resolve().parent.parent
    runtime=pathlib.Path(config['runtime']);pointer=pathlib.Path(pointer)
    spec=importlib.util.spec_from_file_location('activation',repo/'scripts/activate-dashboard-release.py')
    activation=importlib.util.module_from_spec(spec);spec.loader.exec_module(activation)
    release=activation.verify(runtime/'releases',runtime/'publisher/public.pem',config['openssl'],pointer.read_bytes())
    import hashlib
    files={name:repo/'scripts'/name for name in ['frame-dashboard-agent.py','activate-dashboard-release.py','prune-dashboard-releases.py','collect-monitoring.py']}
    files.update({'dashboard-server':pathlib.Path(server),'dashboard-monitoring':pathlib.Path(monitoring),'session-bootstrap':pathlib.Path(bootstrap)})
    checks={name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in files.items()}
    payload=io.BytesIO()
    with tarfile.open(fileobj=payload,mode='w') as archive:
        archive.add(runtime/'releases'/release,arcname='releases/'+release)
        archive.add(pointer,arcname='current.json')
        for name,path in files.items():archive.add(path,arcname=name)
        raw=json.dumps(checks).encode();item=tarfile.TarInfo('transfer.json');item.size=len(raw);archive.addfile(item,io.BytesIO(raw))
    ssh=config['frame']['ssh']
    if not isinstance(ssh,list) or not ssh or ssh[0]!='ssh':raise ValueError('Configure a pinned SSH argument list')
    subprocess.run(ssh+['python3 -c '+shlex.quote(REMOTE)],input=payload.getvalue(),check=True)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--config',type=pathlib.Path,required=True);p.add_argument('--pointer',type=pathlib.Path,required=True)
    for name in ['server','monitoring','bootstrap']:p.add_argument('--'+name,type=pathlib.Path,required=True)
    a=p.parse_args();deploy(json.loads(a.config.read_text()),a.pointer,a.server,a.monitoring,a.bootstrap)
