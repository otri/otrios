#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Remove obsolete immutable releases, retaining the active pointer. Dry run by default."""
import argparse, fcntl, json, pathlib, re, shutil

def prune(root, apply=False, native_cache=False):
    root=root.resolve(strict=True)
    pointer=root/('last-good.json' if native_cache else 'current.json')
    with (root/'.activation.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        active=json.loads(pointer.read_bytes())['id']
        if not re.fullmatch('[a-f0-9]{64}',active) or not (root/active).is_dir() or (root/active).is_symlink():
            raise ValueError('Missing or invalid active release; nothing pruned')
        obsolete=[]
        for item in root.iterdir():
            if not re.fullmatch('[a-f0-9]{64}',item.name):continue
            if item.is_symlink() or not item.is_dir():raise ValueError('Unexpected release entry')
            if item.name!=active:obsolete.append(item)
        size=sum(p.stat().st_size for item in obsolete for p in item.rglob('*') if p.is_file() and not p.is_symlink())
        for item in obsolete:
            if apply:shutil.rmtree(item)
        return {'active':active,'obsolete':len(obsolete),'bytes':size,'applied':apply}

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--releases',type=pathlib.Path,required=True)
    p.add_argument('--apply',action='store_true');p.add_argument('--native-cache',action='store_true');a=p.parse_args()
    print(json.dumps(prune(a.releases,a.apply,a.native_cache)))
