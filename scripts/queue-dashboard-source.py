#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Export public source to the live builder; --watch follows local editor changes."""
import argparse, fcntl, importlib.util, os, pathlib, shutil, tempfile, time
spec=importlib.util.spec_from_file_location('dashboard_watch',pathlib.Path(__file__).with_name('watch-dashboard.py'));watcher=importlib.util.module_from_spec(spec);spec.loader.exec_module(watcher)
def queue(repo,runtime,wanted):
    with tempfile.TemporaryDirectory(prefix='source-export-',dir=runtime) as tmp:
        tmp=pathlib.Path(tmp)
        for src in watcher.inputs(repo):
            dest=tmp/src.relative_to(repo);dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest)
        if watcher.fingerprint(tmp)!=wanted or watcher.fingerprint(repo)!=wanted:return False
        with open(runtime/'inbox.lock','a') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX)
            mirror=runtime/'source-inbox';old=runtime/'feed-previous'
            if old.exists():shutil.rmtree(old)
            if mirror.exists():os.replace(mirror,old)
            os.replace(tmp,mirror);tmp.mkdir()
            if old.exists():shutil.rmtree(old)
        print('QUEUED '+wanted,flush=True);return True
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--runtime',type=pathlib.Path,required=True);p.add_argument('--repo',type=pathlib.Path,default=pathlib.Path(__file__).resolve().parents[1]);p.add_argument('--watch',action='store_true');a=p.parse_args();last=None
    with open(a.runtime/'feed.lock','a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        while True:
            try:
                wanted=watcher.fingerprint(a.repo)
                if wanted!=last:
                    time.sleep(.3)
                    if queue(a.repo,a.runtime,wanted):last=wanted
                if not a.watch:break
            except Exception as e:
                if not a.watch:raise
                print('SOURCE RETAINED: '+str(e),flush=True)
            time.sleep(.5)
