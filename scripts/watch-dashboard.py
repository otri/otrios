#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Rebuild a frozen source tree, then atomically publish the signed host pair."""
import argparse, fcntl, hashlib, io, json, os, pathlib, shutil, subprocess, sys, tarfile, tempfile, time
from importlib.machinery import SourceFileLoader
publisher=SourceFileLoader('otrios_publisher',str(pathlib.Path(__file__).with_name('publish-dashboard.py'))).load_module()

def inputs(repo):
    exact=['Cargo.toml','Cargo.lock','rust-toolchain.toml']
    files=[repo/x for x in exact]
    for tree in ['crates','scripts','web','tools/dashboard']:
        for p in (repo/tree).rglob('*'):
            if p.is_file() and 'pkg' not in p.parts and '__pycache__' not in p.parts and p.suffix in ('.rs','.toml','.py','.sh','.js','.html','.css','.fbs','.flex','.c','.h'):files.append(p)
    return sorted(set(files))
def fingerprint(repo):
    h=hashlib.sha256()
    for p in inputs(repo):h.update(str(p.relative_to(repo)).encode()+b'\0');h.update(p.read_bytes());h.update(b'\0')
    return h.hexdigest()
def run(args,**kw):subprocess.run([str(x) for x in args],check=True,**kw)
def rebuild(config,wanted):
    repo=pathlib.Path(config['repo']);runtime=pathlib.Path(config['runtime']);build=runtime/'build';build.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='input-',dir=build) as tmp:
        temp=pathlib.Path(tmp)
        for p in inputs(repo):
            target=temp/p.relative_to(repo);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,target)
        if fingerprint(repo)!=wanted or fingerprint(temp)!=wanted:raise RuntimeError('Source changed while freezing; retrying')
        source=build/'source';old=build/'previous-source'
        if old.exists():shutil.rmtree(old)
        if source.exists():os.replace(source,old)
        os.replace(temp,source);temp.mkdir()
        if old.exists():shutil.rmtree(old)
    env=dict(os.environ,**config['build_env'],CARGO_TARGET_DIR=str(build/'mac-target'))
    run(['cargo','test','-p','otrios-core','--features','dashboard','--locked'],cwd=source,env=env)
    run(['bash','scripts/build-dashboard-web.sh'],cwd=source,env=env)
    if config.get('node'):
        for script in ['app.js','state-worker.js']:run([config['node'],'--check','web/dashboard/'+script],cwd=source)
    name='otrios-dashboard-live-builder'
    probe=subprocess.run(['docker','inspect','--format','{{.State.Running}}',name],capture_output=True,text=True)
    if probe.returncode:
        run(['docker','create','--name',name,'otrios-dashboard-armv7:rust1.90','sleep','infinity'])
        run(['docker','start',name])
    elif probe.stdout.strip()!='true':run(['docker','start',name])
    archive=build/'source.tar'
    with tarfile.open(archive,'w') as t:
        for item in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','crates']:t.add(source/item,arcname=item)
    run(['docker','cp',archive,name+':/tmp/source.tar'])
    run(['docker','exec',name,'rm','-rf','/src/crates'])
    run(['docker','exec',name,'tar','-xf','/tmp/source.tar','-C','/src'])
    host=config.get('native_host','sdl')
    if host not in ('sdl','direct'):raise ValueError('native_host must be sdl or direct')
    feature,binary=('direct','dashboard-direct') if host=='direct' else ('native','dashboard-native')
    run(['docker','exec','-e','CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc',name,'cargo','build','--release','--locked','-p','otrios-dashboard','--features',feature,'--bin',binary,'--target','armv7-unknown-linux-gnueabihf'])
    native=build/'dashboard-native';run(['docker','cp',name+':/src/target/armv7-unknown-linux-gnueabihf/release/'+binary,native])
    fallback=None
    if host=='direct':
        run(['docker','exec','-e','CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc',name,'cargo','build','--release','--locked','-p','otrios-dashboard','--features','native','--bin','dashboard-native','--target','armv7-unknown-linux-gnueabihf'])
        fallback=build/'dashboard-sdl';run(['docker','cp',name+':/src/target/armv7-unknown-linux-gnueabihf/release/dashboard-native',fallback])
    if fingerprint(repo)!=wanted:raise RuntimeError('Source changed during compilation; stale build not published')
    release=publisher.publish(source/'web/dashboard',native,runtime/'releases',runtime/'publisher/private.pem',config['openssl'],fallback)
    publisher.atomic(build/'source-id',wanted.encode());print('PUBLISHED '+release['id'],flush=True)
    return release

def feed_active(runtime):
    with open(runtime/'feed.lock','a') as f:
        try:fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:return True
    return False

def refresh_git(config):
    remote=config.get('git_remote')
    if not remote or feed_active(pathlib.Path(config['runtime'])):return
    runtime=pathlib.Path(config['runtime']);cache=runtime/'git-cache';repo=pathlib.Path(config['repo']);branch=config['git_branch']
    if not cache.exists():run(['git','clone','--no-checkout','--single-branch','--branch',branch,remote,cache])
    run(['git','-C',cache,'fetch','--depth','1','origin',branch],stdout=subprocess.DEVNULL)
    commit=subprocess.check_output(['git','-C',cache,'rev-parse','FETCH_HEAD'],text=True).strip();marker=runtime/'git-commit'
    if marker.exists() and marker.read_text()==commit:return
    raw=subprocess.check_output(['git','-C',cache,'archive','FETCH_HEAD'])
    with tempfile.TemporaryDirectory(prefix='git-export-',dir=runtime) as tmp:
        tmp=pathlib.Path(tmp)
        with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
            for member in archive.getmembers():
                if not (member.isfile() or member.isdir()) or pathlib.PurePosixPath(member.name).is_absolute() or '..' in pathlib.PurePosixPath(member.name).parts:raise ValueError('Unsafe repository archive entry')
            archive.extractall(tmp)
        old=runtime/'previous-inbox'
        if old.exists():shutil.rmtree(old)
        if repo.exists():os.replace(repo,old)
        os.replace(tmp,repo);tmp.mkdir()
        if old.exists():shutil.rmtree(old)
    publisher.atomic(marker,commit.encode());print('SOURCE '+commit,flush=True)

def main(config,once):
    runtime=pathlib.Path(config['runtime']);lock=open(runtime/'watcher.lock','a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    repo=pathlib.Path(config['repo']);marker=runtime/'build/source-id';last=marker.read_text() if marker.exists() else None
    retry_at=0;next_git=0
    while True:
        try:
            if time.monotonic()>=next_git:
                refresh_git(config);next_git=time.monotonic()+10
            wanted=fingerprint(repo)
            if once or (wanted!=last and time.monotonic()>=retry_at):
                time.sleep(.5)
                if fingerprint(repo)!=wanted:continue
                rebuild(config,wanted);last=wanted
            if once:return
        except Exception as e:
            print('BUILD KEPT PREVIOUS RELEASE: '+str(e),flush=True)
            if once:raise
            retry_at=time.monotonic()+10
        time.sleep(1)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--config',required=True);p.add_argument('--once',action='store_true');a=p.parse_args();main(json.loads(pathlib.Path(a.config).read_text()),a.once)
