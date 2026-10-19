#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Unprivileged native update supervisor. Only a pinned publisher may supply code."""
import argparse, fcntl, hashlib, json, math, os, pathlib, re, signal, subprocess, tempfile, time, urllib.request, urllib.parse, uuid, shutil, threading
MAX_BINARY=32*1024*1024
CACHE_BYTES=2*1024*1024*1024
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
    if manifest.get('format')!='otrios-release-v1' or manifest.get('native_target')!='armv7-unknown-linux-gnueabihf':raise ValueError('Incompatible bootstrap release')
    # Data protocol compatibility belongs to the signed executable, not this loader.
    for field in ['state_schema','sync_protocol']:
        value=manifest.get(field)
        if not isinstance(value,str) or not 0<len(value)<=128 or any(not 33<=ord(c)<=126 for c in value):raise ValueError('Invalid protocol metadata')
    return manifest

def verify_binary(data,item):
    if not 0<item['bytes']<=MAX_BINARY or len(data)!=item['bytes'] or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('Native artifact digest rejected')
    if len(data)<52 or data[:6]!=b'\x7fELF\x01\x01' or int.from_bytes(data[18:20],'little')!=40:raise ValueError('Wrong executable target')

def verify_font(data,item):
    if not 0<item['bytes']<=1024*1024 or len(data)!=item['bytes'] or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('Font resource digest rejected')

def resource_items(manifest):
    for remote,local in [('apps/catalog.flex','app-catalog.flex'),('web/fonts/Roboto-Regular.otq.zst','fonts/Roboto-Regular.otq.zst'),('web/Slug-LICENSE.txt','Slug-LICENSE.txt')]:
        item=manifest['assets'].get(remote)
        if item:
            if not 0<item['bytes']<=1024*1024:raise ValueError('Resource exceeds item budget')
            yield remote,local,item

def valid_release(release):
    if len(release)!=64 or any(c not in '0123456789abcdef' for c in release):raise ValueError('Invalid release ID')

def cached(cache,release,key,openssl,sdl=False):
    valid_release(release);dest=cache/release
    cache_size(dest)
    raw=(dest/'manifest.json').read_bytes();sig=(dest/'manifest.sig').read_bytes()
    manifest=verify_manifest(raw,sig,release,cache,key,openssl)
    name='dashboard-sdl' if sdl and 'native/armv7/dashboard-sdl' in manifest['assets'] else 'dashboard-native'
    for artifact in ['dashboard-native','dashboard-sdl']:
        if 'native/armv7/'+artifact in manifest['assets']:verify_binary((dest/artifact).read_bytes(),manifest['assets']['native/armv7/'+artifact])
    for _,local,item in resource_items(manifest):verify_font((dest/local).read_bytes(),item)
    return release,dest/name

def cache_size(path):
    if path.is_symlink():raise ValueError('Cache symlink rejected')
    if path.is_file():return path.stat().st_size
    return sum(cache_size(p) for p in path.iterdir())

def trim_cache(cache,limit=CACHE_BYTES,reserve=0,pins=()):
    # Count all bytes, including metadata. Evict only managed immutable entries.
    total=cache_size(cache)+reserve
    entries=[p for p in cache.iterdir() if re.fullmatch('[a-f0-9]{64}',p.name)]
    objects=cache/'objects'
    if objects.exists():
        if objects.is_symlink():raise ValueError('Cache symlink rejected')
        entries.extend(p for p in objects.iterdir() if re.fullmatch('[a-f0-9]{64}',p.name))
    for path in sorted(entries,key=lambda p:p.stat().st_mtime_ns):
        if total<=limit:break
        if path.name in pins:continue
        size=cache_size(path)
        if path.is_dir():shutil.rmtree(path)
        else:path.unlink()
        total-=size
    if total>limit:raise ValueError('Cache budget cannot fit pinned artifacts')

def hot_reload_ready(info):
    if info.get('input_busy',True):return False
    # Current PoC policy: disconnected RAM edits may be dropped for an upgrade.
    if info.get('stream_ready') is False:return True
    if 'reload_ready' in info:return info['reload_ready'] is True
    return bool(info.get('stream_ready')) and info.get('state_confirmed',True) and info.get('pending_commands',0)==0

def qualify(base,cache,key,openssl,release=None,sdl=False,limit=CACHE_BYTES,pins=()):
    if release is None:release=json.loads(fetch(base,'/release',8192))['id']
    valid_release(release)
    try:
        result=cached(cache,release,key,openssl,sdl)
        trim_cache(cache,limit,pins=(*pins,release));os.utime(cache/release,None)
        return result
    except (OSError,ValueError,KeyError):pass
    root='/releases/'+release+'/'
    raw=fetch(base,root+'manifest.json',65536);sig=fetch(base,root+'manifest.sig',64)
    manifest=verify_manifest(raw,sig,release,cache,key,openssl)
    dest=cache/release
    if dest.exists():
        if release in pins:raise ValueError('Pinned artifact failed validation')
        if dest.is_symlink():raise ValueError('Cache symlink rejected')
        shutil.rmtree(dest)
    expected=len(raw)+len(sig)
    for name in ['dashboard-native','dashboard-sdl']:
        item=manifest['assets'].get('native/armv7/'+name)
        if item:
            if not 0<item['bytes']<=MAX_BINARY:raise ValueError('Invalid binary size')
            expected+=item['bytes']
    expected+=sum(item['bytes']for _,_,item in resource_items(manifest))
    trim_cache(cache,limit,reserve=expected,pins=pins)
    # Stage outside digest namespace, accounted by reservation; publish at once.
    with tempfile.TemporaryDirectory(prefix='.download-',dir=cache) as staging:
        stage=pathlib.Path(staging)
        for name in ['dashboard-native','dashboard-sdl']:
            path='native/armv7/'+name
            if path not in manifest['assets']:continue
            item=manifest['assets'][path]
            data=fetch(base,root+path,MAX_BINARY);verify_binary(data,item)
            atomic(stage/name,data,0o700)
        for remote,local,item in resource_items(manifest):
            data=fetch(base,root+remote,1024*1024);verify_font(data,item)
            (stage/local).parent.mkdir(parents=True,exist_ok=True);atomic(stage/local,data)
        atomic(stage/'manifest.json',raw);atomic(stage/'manifest.sig',sig)
        stage.rename(dest)
    trim_cache(cache,limit,pins=(*pins,release));os.utime(dest,None)
    return cached(cache,release,key,openssl,sdl)

def cached_blob(base,cache,digest,size,limit=CACHE_BYTES,pins=()):
    """Optional immutable object cache; callers supply the expected SHA-256/size."""
    valid_release(digest)
    if not 0<size<=MAX_BINARY:raise ValueError('Blob exceeds cache item limit')
    objects=cache/'objects'
    if objects.is_symlink():raise ValueError('Cache symlink rejected')
    objects.mkdir(exist_ok=True)
    path=objects/digest
    if path.is_symlink():raise ValueError('Cache symlink rejected')
    if path.exists() and path.stat().st_size==size and hashlib.sha256(path.read_bytes()).hexdigest()==digest:
        trim_cache(cache,limit,pins=(*pins,digest));os.utime(path,None);return path
    if path.exists():
        if digest in pins:raise ValueError('Pinned blob corrupt')
        path.unlink()
    trim_cache(cache,limit,reserve=size,pins=pins)
    data=fetch(base,'/objects/'+digest,MAX_BINARY)
    if len(data)!=size or hashlib.sha256(data).hexdigest()!=digest:raise ValueError('Blob digest rejected')
    atomic(path,data);return path

def touch_calibration(path):
    # Read the accepted live rule; never rewrite udev/libinput calibration.
    match=re.search(r'LIBINPUT_CALIBRATION_MATRIX[^=]*=\s*"([^"]+)"',path.read_text())
    if not match:raise ValueError('Accepted libinput calibration matrix not found')
    values=[float(v) for v in match.group(1).split()]
    if len(values)!=6 or any(not math.isfinite(v) or abs(v)>8 for v in values) or abs(values[0]*values[4]-values[1]*values[3])<.00001:raise ValueError('Invalid accepted calibration')
    return ','.join(format(v,'.13g') for v in values)

def ready_contract(info,driver,rotation):
    direct=info.get('renderer')=='gles2-direct' and info.get('video')=='DRM'
    if not direct and info.get('renderer')!='opengles2':raise RuntimeError('Candidate did not use GLES2')
    if direct and driver!='KMSDRM':raise RuntimeError('Direct executable requires the direct display service')
    if driver=='KMSDRM':
        size=[800,1280] if rotation in (90,270) else [1280,800]
        if info.get('video')!=('DRM' if direct else 'KMSDRM') or info.get('rotation')!=rotation or info.get('output')!=size or not info.get('touch_calibrated') or info.get('touch_devices',0)<1:raise RuntimeError('Candidate did not satisfy the direct display/input contract')
        if direct:
            formats={'RGB565':([5,6,5,0],2),'XRGB8888':([8,8,8,0],4)}
            bits,bpp=formats.get(info.get('scanout_format'),([],0))
            if info.get('color_bits')!=bits or info.get('stride',0)<size[0]*bpp or not bpp:raise RuntimeError('Direct framebuffer colour/stride mismatch')

class Host:
    def __init__(self,a,release,exe,screen=None):
        self.a=a;self.release=release;self.stop=a.runtime_dir/('stop-'+uuid.uuid4().hex);self.ready=a.runtime_dir/('ready-'+uuid.uuid4().hex+'.json')
        env=dict(os.environ,OTRIOS_RELEASE_ID=release,SDL_RENDER_DRIVER='opengles2',SDL_VIDEODRIVER=a.video_driver)
        if a.video_driver=='KMSDRM':env.update(SDL_KMSDRM_DEVICE_INDEX='0',SDL_KMSDRM_REQUIRE_DRM_MASTER='1')
        command=[str(exe),'--sync',a.url.rstrip('/')+'/apps/stream','--exit-file',str(self.stop),'--ready-file',str(self.ready),'--release-file',str(a.runtime_dir/'release-notice.json')]
        if screen:command+=['--screen',screen]
        if a.video_driver=='KMSDRM':command+=['--rotation',str(a.rotation),'--touch-matrix',a.touch_matrix]
        if a.capture_first_frame:command+=['--capture',str(a.runtime_dir/'first-frame.ppm')]
        self.process=subprocess.Popen(command,env=env)
    def wait_ready(self,require_stream=True):
        deadline=time.monotonic()+15
        while running and time.monotonic()<deadline:
            if self.process.poll() is not None:raise RuntimeError('Native candidate exited before drawing')
            if self.ready.exists():
                info=json.loads(self.ready.read_text())
                ready_contract(info,self.a.video_driver,self.a.rotation)
                if not require_stream or info.get('stream_ready'):return
            time.sleep(.1)
        raise RuntimeError('Native candidate did not become ready')
    def info(self):
        try:return json.loads(self.ready.read_text())
        except (OSError,ValueError):return {}
    def close(self):
        self.stop.touch()
        try:self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.stop.unlink(missing_ok=True);self.ready.unlink(missing_ok=True)

class ReleaseFeed:
    """Bounded event feed from the stable, TLS-verifying bootstrap companion."""
    def __init__(self,a,current):
        base=getattr(a,'code_url',None) or a.url
        url=urllib.parse.urlsplit(urllib.parse.urljoin(base,'/release/events'))
        endpoint=urllib.parse.urlunsplit(('wss' if url.scheme=='https' else 'ws',url.netloc,url.path,'',''))
        protocol=''
        if current:
            try:protocol=json.loads((a.cache/current/'manifest.json').read_text()).get('sync_protocol','')
            except (OSError,ValueError):pass
        self.condition=threading.Condition();self.latest=None
        self.process=subprocess.Popen([getattr(a,'release_watch','/opt/otrios-dashboard/release-watch'),endpoint,current or '',protocol],stdout=subprocess.PIPE)
        self.reader=threading.Thread(target=self.read,daemon=True);self.reader.start()
    def read(self):
        try:
            while True:
                line=self.process.stdout.readline(8193)
                if not line:return
                if len(line)>8192:raise ValueError('Oversized control event')
                event=json.loads(line)
                if event.get('version')!=1 or event.get('type') not in ('hello','upgrade'):raise ValueError('Invalid control envelope')
                release=event['release'];valid_release(release['id'])
                if release.get('web_entry')!=f"/releases/{release['id']}/web/index.html":raise ValueError('Invalid control release')
                with self.condition:self.latest=release;self.condition.notify()
        except (OSError,ValueError,KeyError,TypeError):self.process.terminate()
    def take(self):
        with self.condition:
            if self.latest is None:self.condition.wait(.1)
            value=self.latest;self.latest=None;return value
    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.reader.join(timeout=1);self.process.stdout.close()

def main(a):
    global running
    a.cache.mkdir(mode=0o700,parents=True,exist_ok=True);a.runtime_dir.mkdir(mode=0o700,parents=True,exist_ok=True)
    lock=open(a.runtime_dir/'agent.lock','a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    for p in a.cache.iterdir():
        if p.name.startswith(('.download-','.verify-')) and p.is_dir() and not p.is_symlink():shutil.rmtree(p)
    host=None;current=None;last_good=None;pending=None;retry_at=0
    good_file=a.cache/'last-good.json'
    if good_file.exists():
        try:
            value=json.loads(good_file.read_text());last_good=cached(a.cache,value['id'],a.key,a.openssl,sdl=a.video_driver=='wayland')
            host=Host(a,*last_good);host.wait_ready(require_stream=False);current=last_good[0]
            print('RESUMED signed_release='+current,flush=True)
        except Exception as e:
            if host:host.close();host=None
            print('Cached app unavailable: '+str(e),flush=True)
    feed=None
    try:
        feed=ReleaseFeed(a,current)
        while running:
            try:
                if feed.process.poll() is not None:
                    feed.close();feed=ReleaseFeed(a,current);time.sleep(.25)
                event=feed.take()
                if event is not None:
                    if pending is None or event['id']!=pending['id']:retry_at=0
                    pending=event
                if pending is None or time.monotonic()<retry_at:continue
                release=pending['id']
                if host is not None and host.process.poll() is None and release==current:continue
                old=host;info=old.info() if old and old.process.poll() is None else {}
                if old and old.process.poll() is None and not hot_reload_ready(info):continue
                code_url=getattr(a,'code_url',None) or a.url
                release,exe=qualify(code_url,a.cache,a.key,a.openssl,release,sdl=a.video_driver=='wayland',limit=a.cache_bytes,pins=tuple(v for v in (current,last_good[0] if last_good else None) if v))
                host=None
                if old:old.close()
                candidate=Host(a,release,exe,screen=info.get('screen'))
                try:candidate.wait_ready(require_stream=info.get('stream_ready') is not False)
                except Exception:
                    candidate.close();retry_at=time.monotonic()+30
                    if last_good and last_good[0]!=release:
                        last_good=cached(a.cache,last_good[0],a.key,a.openssl,sdl=a.video_driver=='wayland');host=Host(a,*last_good);host.wait_ready(require_stream=False)
                    raise
                host=candidate;current=release;last_good=(release,exe)
                atomic(good_file,json.dumps({'id':release}).encode());print('ACTIVE signed_release='+release,flush=True)
                feed.close();feed=ReleaseFeed(a,current)
            except Exception as e:
                print('Update kept previous app: '+str(e),flush=True)
                if host is None:raise SystemExit(1)
                retry_at=max(retry_at,time.monotonic()+5)
    finally:
        if feed:feed.close()
        if host:host.close()
        lock.close()
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--code-url');p.add_argument('--release-watch',default='/opt/otrios-dashboard/release-watch');p.add_argument('--cache',type=pathlib.Path,required=True);p.add_argument('--runtime-dir',type=pathlib.Path,default=pathlib.Path(os.environ.get('XDG_RUNTIME_DIR','/run/otrios-direct-console'))/'otrios');p.add_argument('--cache-bytes',type=int,default=CACHE_BYTES);p.add_argument('--key',type=pathlib.Path,required=True);p.add_argument('--openssl',default='/usr/bin/openssl');p.add_argument('--capture-first-frame',action='store_true');p.add_argument('--video-driver',choices=['wayland','KMSDRM'],default='wayland');p.add_argument('--rotation',type=int,choices=[0,90,180,270],default=90);p.add_argument('--touch-calibration',type=pathlib.Path);a=p.parse_args()
    if a.cache_bytes<2*MAX_BINARY+65536:p.error('Cache budget must fit one signed release')
    if a.video_driver=='KMSDRM':
        if a.touch_calibration is None:p.error('Direct mode requires the accepted touch calibration rule')
        a.touch_matrix=touch_calibration(a.touch_calibration)
    if not a.url.startswith('https://') or (a.code_url and not a.code_url.startswith('https://')):p.error('HTTPS is required')
    def stop(*_):
        global running
        running=False
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);main(a)
