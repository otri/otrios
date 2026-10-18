#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Unprivileged native update supervisor. Only a pinned publisher may supply code."""
import argparse, fcntl, hashlib, json, os, pathlib, signal, subprocess, tempfile, time, urllib.request, urllib.parse, uuid
MAX_BINARY=32*1024*1024
running=True

def fetch(base,path,limit):
    url=urllib.parse.urljoin(base,path)
    if urllib.parse.urlsplit(url).netloc!=urllib.parse.urlsplit(base).netloc:raise ValueError('Cross-origin release URL')
    with urllib.request.urlopen(url,timeout=10) as r:
        data=r.read(limit+1)
        if len(data)>limit:raise ValueError('Release exceeds size limit')
        return data

def atomic(path,data,mode=0o600):
    fd,tmp=tempfile.mkstemp(prefix='.pending-',dir=path.parent)
    try:
        with os.fdopen(fd,'wb') as f:f.write(data);f.flush();os.fsync(f.fileno())
        os.chmod(tmp,mode);os.replace(tmp,path)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)

def verify_manifest(raw,sig,release,cache,key,openssl):
    if len(raw)>65536 or hashlib.sha256(raw).hexdigest()!=release or len(sig)!=64:raise ValueError('Invalid manifest digest/signature size')
    with tempfile.TemporaryDirectory(prefix='.verify-',dir=cache) as tmp:
        tmp=pathlib.Path(tmp);(tmp/'manifest').write_bytes(raw);(tmp/'sig').write_bytes(sig)
        result=subprocess.run([openssl,'pkeyutl','-verify','-pubin','-inkey',str(key),'-rawin','-in',str(tmp/'manifest'),'-sigfile',str(tmp/'sig')],capture_output=True)
        if result.returncode:raise ValueError('Publisher signature rejected')
    manifest=json.loads(raw)
    if manifest.get('format')!='otrios-release-v1' or manifest.get('state_schema')!='otrios-dashboard-v1' or manifest.get('sync_protocol')!='automerge-sync-v1' or manifest.get('native_target')!='armv7-unknown-linux-gnueabihf':raise ValueError('Incompatible release')
    return manifest

def verify_binary(data,item):
    if not 0<item['bytes']<=MAX_BINARY or len(data)!=item['bytes'] or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('Native artifact digest rejected')
    if len(data)<52 or data[:6]!=b'\x7fELF\x01\x01' or int.from_bytes(data[18:20],'little')!=40:raise ValueError('Wrong executable target')

def valid_release(release):
    if len(release)!=64 or any(c not in '0123456789abcdef' for c in release):raise ValueError('Invalid release ID')

def cached(cache,release,key,openssl):
    valid_release(release);dest=cache/release
    raw=(dest/'manifest.json').read_bytes();sig=(dest/'manifest.sig').read_bytes()
    manifest=verify_manifest(raw,sig,release,cache,key,openssl)
    exe=dest/'dashboard-native';verify_binary(exe.read_bytes(),manifest['assets']['native/armv7/dashboard-native'])
    return release,exe

def qualify(base,cache,key,openssl,release=None):
    if release is None:release=json.loads(fetch(base,'/release',8192))['id']
    valid_release(release)
    try:return cached(cache,release,key,openssl)
    except (OSError,ValueError,KeyError):pass
    root='/releases/'+release+'/'
    raw=fetch(base,root+'manifest.json',65536);sig=fetch(base,root+'manifest.sig',64)
    manifest=verify_manifest(raw,sig,release,cache,key,openssl)
    item=manifest['assets']['native/armv7/dashboard-native']
    if not 0<item['bytes']<=MAX_BINARY:raise ValueError('Invalid binary size')
    data=fetch(base,root+'native/armv7/dashboard-native',MAX_BINARY);verify_binary(data,item)
    dest=cache/release;dest.mkdir(exist_ok=True)
    atomic(dest/'manifest.json',raw);atomic(dest/'manifest.sig',sig);atomic(dest/'dashboard-native',data,0o700)
    return release,dest/'dashboard-native'

class Host:
    def __init__(self,a,release,exe):
        self.a=a;self.release=release;self.stop=a.cache/('stop-'+uuid.uuid4().hex);self.ready=a.cache/('ready-'+uuid.uuid4().hex+'.json')
        env=dict(os.environ,OTRIOS_RELEASE_ID=release,SDL_RENDER_DRIVER='opengles2',SDL_VIDEODRIVER='wayland')
        command=[str(exe),'--fullscreen','--sync',a.url.rstrip('/')+'/stream','--state',str(a.cache/'state.automerge'),'--exit-file',str(self.stop),'--ready-file',str(self.ready),'--release-file',str(a.cache/'release-notice.json')]
        if a.capture_first_frame:command+=['--capture',str(a.cache/'first-frame.ppm')]
        self.process=subprocess.Popen(command,env=env)
    def wait_ready(self,require_stream=True):
        deadline=time.monotonic()+15
        while running and time.monotonic()<deadline:
            if self.process.poll() is not None:raise RuntimeError('Native candidate exited before drawing')
            if self.ready.exists():
                info=json.loads(self.ready.read_text())
                if info.get('renderer')!='opengles2':raise RuntimeError('Candidate did not use GLES2')
                if not require_stream or info.get('stream_ready'):return
            time.sleep(.1)
        raise RuntimeError('Native candidate did not become ready')
    def close(self):
        self.stop.touch()
        try:self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.stop.unlink(missing_ok=True);self.ready.unlink(missing_ok=True)

def main(a):
    global running
    a.cache.mkdir(mode=0o700,parents=True,exist_ok=True)
    lock=open(a.cache/'agent.lock','a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    host=None;current=None;bad=None;last_good=None
    good_file=a.cache/'last-good.json'
    if good_file.exists():
        try:
            value=json.loads(good_file.read_text());last_good=cached(a.cache,value['id'],a.key,a.openssl)
            host=Host(a,*last_good);host.wait_ready(require_stream=False);current=last_good[0]
            print('RESUMED signed_release='+current,flush=True)
        except Exception as e:
            if host:host.close();host=None
            print('Cached app unavailable: '+str(e),flush=True)
    last_probe=0
    try:
        while running:
            try:
                release=None
                notice=a.cache/'release-notice.json'
                if notice.exists():release=json.loads(notice.read_text())['id']
                if host is not None and host.process.poll() is None and release in (None,current,bad) and time.monotonic()-last_probe<30:
                    time.sleep(.1);continue
                if time.monotonic()-last_probe>=30 or release is None:
                    release=json.loads(fetch(a.url,'/release',8192))['id'];last_probe=time.monotonic()
                release,exe=qualify(a.url,a.cache,a.key,a.openssl,release)
                if (host is None or host.process.poll() is not None or release!=current) and release!=bad:
                    old=host;host=None
                    if old:old.close()
                    candidate=Host(a,release,exe)
                    try:candidate.wait_ready()
                    except Exception:
                        candidate.close();bad=release
                        if last_good and last_good[0]!=release:
                            # Previous accepted release was signature-qualified this session.
                            last_good=cached(a.cache,last_good[0],a.key,a.openssl);host=Host(a,*last_good);host.wait_ready(require_stream=False)
                        raise
                    host=candidate;current=release;last_good=(release,exe);bad=None
                    atomic(good_file,json.dumps({'id':release}).encode())
                    print('ACTIVE signed_release='+release,flush=True)
            except Exception as e:
                print('Update kept previous app: '+str(e),flush=True)
                if host is None:raise SystemExit(1)
            for _ in range(10):
                if not running:break
                time.sleep(.1)
    finally:
        if host:host.close()
    lock.close()
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--cache',type=pathlib.Path,required=True);p.add_argument('--key',type=pathlib.Path,required=True);p.add_argument('--openssl',default='/usr/bin/openssl');p.add_argument('--capture-first-frame',action='store_true');a=p.parse_args()
    if not a.url.startswith('https://'):p.error('HTTPS is required')
    def stop(*_):
        global running
        running=False
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);main(a)
