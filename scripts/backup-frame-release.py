#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Preserve a quiescent Frame rollback kit and a private Mac copy.

Uses the existing pinned deployment SSH configuration. Stops writers only while
copying, then restores active services before the archive transfer. Inventories
and private payloads remain on the Frame or in the configured private runtime.
"""
import argparse
import json
import os
import pathlib
import re
import shlex
import subprocess
import tarfile
import hashlib

REMOTE = r'''
import hashlib,json,os,pathlib,shutil,subprocess
os.umask(0o077)
root=pathlib.Path('/var/lib/otrios-authority');snap=root/'rollback'/tag/'pre-cutover'
assert not snap.exists(),'Backup already exists; refusing to overwrite it'
services=['otrios-monitoring.timer','otrios-monitoring.service','otrios-dashboard-direct','otrios-authority']
active={s:subprocess.run(['systemctl','is-active','--quiet',s]).returncode==0 for s in services}
try:
 for service in services:subprocess.run(['systemctl','stop',service],check=True)
 snap.mkdir(mode=0o700,parents=True)
 managed=json.loads(subprocess.check_output(['runuser','-u','otrios','--','/opt/otrios-authority/dashboard-server','--spaces-registry',str(root/'spaces.flex'),'--app-state',str(root/'state.apps'),'--spaces-storage','backup'],text=True))
 (snap/'managed-backup.json').write_text(json.dumps(managed))
 shutil.copytree(root/'spaces.flex.backups'/managed['backup'],snap/'managed-snapshot')
 shutil.copy2(root/'spaces.flex.backups.root',snap/'spaces.flex.backups.root')
 pointer=(root/'releases/current.json').read_bytes();release=json.loads(pointer)['id']
 (snap/'current.json').write_bytes(pointer)
 shutil.copytree(root/'releases'/release,snap/'release')
 for name in ['state.apps','state.presentation.flex','monitoring.flex','spaces.flex','spaces.flex.latest','spaces.flex.owner.lock','spaces.flex.moves','spaces.flex.profiles','spaces.flex.restore','collector-binding.json']:
  path=root/name
  if path.is_dir():shutil.copytree(path,snap/name,symlinks=True)
  elif path.exists():shutil.copy2(path,snap/name)
 for name,path in [('authority-bin','/opt/otrios-authority'),('native-bin','/opt/otrios-dashboard'),('authority-config','/etc/otrios-authority')]:shutil.copytree(path,snap/name,symlinks=True)
 (snap/'services').mkdir()
 for path in pathlib.Path('/etc/systemd/system').glob('otrios*'):
  if path.is_file():shutil.copy2(path,snap/'services'/path.name)
 for name,path in [('native.conf','/etc/otrios-dashboard.conf'),('native.identity','/etc/otrios-dashboard.identity'),('touch.rules','/etc/udev/rules.d/99-spd-touch.rules')]:shutil.copy2(path,snap/name)
 inventory={}
 for path in snap.rglob('*'):
  if path.is_file() and not path.is_symlink():
   inventory[str(path.relative_to(snap))]={'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'bytes':path.stat().st_size}
   with path.open('rb') as f:os.fsync(f.fileno())
 (snap/'inventory.json').write_text(json.dumps(inventory,sort_keys=True))
 with (snap/'inventory.json').open('rb') as f:os.fsync(f.fileno())
 fd=os.open(snap,os.O_DIRECTORY);os.fsync(fd);os.close(fd)
 print(json.dumps({'backed_up':True,'release':release,'files_verified':len(inventory)}))
finally:
 for service in reversed(services):
  if active[service]:subprocess.run(['systemctl','start',service],check=True)
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--tag', required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}-[a-z0-9-]+', args.tag):
        parser.error('Use an ISO-date-first backup tag')
    config = json.loads(args.config.read_text())
    runtime = pathlib.Path(config['runtime']).resolve()
    destination = runtime / 'rollback' / args.tag / 'pre-cutover.tar'
    if runtime == pathlib.Path(config['repo']).resolve() or runtime.is_relative_to(pathlib.Path(config['repo']).resolve()):
        parser.error('The runtime backup destination must be outside the repository')
    if destination.exists():
        parser.error('Private backup already exists')
    result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote('tag=' + repr(args.tag) + '\n' + REMOTE)],
                            capture_output=True, text=True, check=True, timeout=180)
    record = json.loads(result.stdout)
    remote = '/var/lib/otrios-authority/rollback/' + args.tag + '/pre-cutover'
    archive = "import tarfile,sys;t=tarfile.open(fileobj=sys.stdout.buffer,mode='w|');t.add(" + repr(remote) + ",arcname='pre-cutover');t.close()"
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with destination.open('xb') as output:
        destination.chmod(0o600)
        subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(archive)], stdout=output, check=True, timeout=180)
        output.flush()
        os.fsync(output.fileno())
    with tarfile.open(destination) as contents:
        inventory = json.load(contents.extractfile('pre-cutover/inventory.json'))
        for name, item in inventory.items():
            data = contents.extractfile('pre-cutover/' + name).read()
            if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
                raise RuntimeError('Private backup verification failed')
    print(json.dumps(dict(record, private_mac_backup=True, private_inventory_exported=False, services_restored=True)))


if __name__ == '__main__':
    main()
