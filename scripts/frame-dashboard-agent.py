#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Unprivileged native update supervisor. Only a pinned publisher may supply code."""
import argparse, fcntl, hashlib, json, math, os, pathlib, re, signal, subprocess, tempfile, time, urllib.request, urllib.parse, uuid, shutil, socket, select
MAX_BINARY=32*1024*1024
CACHE_BYTES=2*1024*1024*1024
running=True

def validate_origin(base):
    parsed=urllib.parse.urlsplit(base)
    if parsed.username or parsed.password or parsed.query or parsed.fragment or parsed.path not in ('','/'):
        raise ValueError('Expected an origin without credentials, path, query or fragment')
    if not parsed.hostname or not parsed.port and parsed.netloc.endswith(':'):
        raise ValueError('Invalid origin')
    if parsed.scheme!='https' and not (parsed.scheme=='http' and parsed.hostname in ('127.0.0.1','::1')):
        raise ValueError('HTTPS or literal loopback HTTP is required')
    return parsed

def fetch(base,path,limit):
    validate_origin(base)
    url=urllib.parse.urljoin(base,path)
    if urllib.parse.urlsplit(url)[:2]!=urllib.parse.urlsplit(base)[:2]:raise ValueError('Cross-origin release URL')
    request=urllib.request.Request(url,headers={'User-Agent':'OTriOS-Release/1','Accept':'application/octet-stream'})
    with urllib.request.urlopen(request,timeout=10) as r:
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
    native={name for name in manifest['assets'] if name.startswith('native/')}
    if native!={'native/armv7/dashboard-native'}:raise ValueError('Release must contain one direct native executable')
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

def cached(cache,release,key,openssl):
    valid_release(release);dest=cache/release
    cache_size(dest)
    raw=(dest/'manifest.json').read_bytes();sig=(dest/'manifest.sig').read_bytes()
    manifest=verify_manifest(raw,sig,release,cache,key,openssl)
    verify_binary((dest/'dashboard-native').read_bytes(),manifest['assets']['native/armv7/dashboard-native'])
    for _,local,item in resource_items(manifest):verify_font((dest/local).read_bytes(),item)
    return release,dest/'dashboard-native'

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

def ui_only_change(previous,next,behavior=False):
    if previous.get('ui_abi')!='otrios-app-ui-v1' or next.get('ui_abi')!=previous['ui_abi']:return False
    if {k:v for k,v in previous.items() if k not in ('assets','authority')}!={k:v for k,v in next.items() if k not in ('assets','authority')}:return False
    for section,allowed in [('assets','apps/catalog.flex'),('authority',None)]:
        a,b=previous.get(section,{}),next.get(section,{})
        if not a or a.keys()!=b.keys() or any(a[k]!=b[k] for k in a if not (k==allowed or (section=='authority' and re.fullmatch(r'authority/apps/[a-z-]+/ui\.flex',k)) or (behavior and section=='authority' and re.fullmatch(r'authority/apps/[a-z-]+/behavior\.wasm',k)))):return False
    return True

def app_only_change(previous,next):
    behavior=previous.get('behavior_abi')=='otrios-app-behavior-v1' and next.get('behavior_abi')==previous['behavior_abi']
    return ui_only_change(previous,next,behavior=behavior)

def hot_reload_ready(info):
    if info.get('input_busy',True):return False
    # Current PoC policy: disconnected RAM edits may be dropped for an upgrade.
    if info.get('stream_ready') is False:return True
    if 'reload_ready' in info:return info['reload_ready'] is True
    return bool(info.get('stream_ready')) and info.get('state_confirmed',True) and info.get('pending_commands',0)==0

def qualify(base,cache,key,openssl,release=None,limit=CACHE_BYTES,pins=()):
    if release is None:release=json.loads(fetch(base,'/release',8192))['id']
    valid_release(release)
    try:
        result=cached(cache,release,key,openssl)
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
    item=manifest['assets']['native/armv7/dashboard-native']
    if not 0<item['bytes']<=MAX_BINARY:raise ValueError('Invalid binary size')
    expected+=item['bytes']
    expected+=sum(item['bytes']for _,_,item in resource_items(manifest))
    trim_cache(cache,limit,reserve=expected,pins=pins)
    # Stage outside digest namespace, accounted by reservation; publish at once.
    with tempfile.TemporaryDirectory(prefix='.download-',dir=cache) as staging:
        stage=pathlib.Path(staging)
        path='native/armv7/dashboard-native';item=manifest['assets'][path]
        data=fetch(base,root+path,MAX_BINARY);verify_binary(data,item)
        atomic(stage/'dashboard-native',data,0o700)
        for remote,local,item in resource_items(manifest):
            data=fetch(base,root+remote,1024*1024);verify_font(data,item)
            (stage/local).parent.mkdir(parents=True,exist_ok=True);atomic(stage/local,data)
        atomic(stage/'manifest.json',raw);atomic(stage/'manifest.sig',sig)
        stage.rename(dest)
    trim_cache(cache,limit,pins=(*pins,release));os.utime(dest,None)
    return cached(cache,release,key,openssl)

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

def ready_contract(info,rotation):
    if info.get('renderer')!='gles2-direct' or info.get('video')!='DRM':raise RuntimeError('Candidate requires the direct GLES2 display service')
    size=[800,1280] if rotation in (90,270) else [1280,800]
    if info.get('rotation')!=rotation or info.get('output')!=size or not info.get('touch_calibrated') or info.get('touch_devices',0)<1:raise RuntimeError('Candidate did not satisfy the direct display/input contract')
    formats={'RGB565':([5,6,5,0],2),'XRGB8888':([8,8,8,0],4)}
    bits,bpp=formats.get(info.get('scanout_format'),([],0))
    if info.get('color_bits')!=bits or info.get('stride',0)<size[0]*bpp or not bpp:raise RuntimeError('Direct framebuffer colour/stride mismatch')

class Host:
    def __init__(self,a,release,exe,screen=None):
        self.a=a;self.release=release;self.stop=a.runtime_dir/('stop-'+uuid.uuid4().hex);self.ready=a.runtime_dir/('ready-'+uuid.uuid4().hex+'.json');self.ui_socket=a.runtime_dir/('ui-'+uuid.uuid4().hex+'.sock');self.offered_ui=None
        env=dict(os.environ,OTRIOS_RELEASE_ID=release)
        env['OTRIOS_RELEASE_SOCKET']=str(a.runtime_dir/'release.sock')
        command=[str(exe),'--sync',a.url.rstrip('/')+'/session','--exit-file',str(self.stop),'--ready-file',str(self.ready)]
        if screen and screen!='locked':command+=['--screen',screen]
        command+=['--catalog-update-socket',str(self.ui_socket)]
        command+=['--rotation',str(a.rotation),'--touch-matrix',a.touch_matrix]
        if a.capture_first_frame:command+=['--capture',str(a.runtime_dir/'first-frame.ppm')]
        self.process=subprocess.Popen(command,env=env)
    def wait_ready(self,require_stream=True):
        deadline=time.monotonic()+15
        while running and time.monotonic()<deadline:
            if self.process.poll() is not None:raise RuntimeError('Native candidate exited before drawing')
            if self.ready.exists():
                info=json.loads(self.ready.read_text())
                ready_contract(info,self.a.rotation)
                if not require_stream or (info.get('stream_ready') and info.get('ui_ref') and info.get('state_confirmed')):return
            time.sleep(.1)
        raise RuntimeError('Native candidate did not become ready')
    def info(self):
        try:return json.loads(self.ready.read_text())
        except (OSError,ValueError):return {}
    def update_ui(self,release,cache):
        if self.offered_ui!=release:
            item=json.loads((cache/release/'manifest.json').read_text())['assets']['apps/catalog.flex']
            notice={'release':release,'catalog':str(cache/release/'app-catalog.flex'),'sha256':item['sha256']}
            with socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM) as channel:channel.sendto(json.dumps(notice).encode(),str(self.ui_socket))
            self.offered_ui=release
        info=self.info()
        return info.get('ui_release')==release and hot_reload_ready(info)
    def close(self):
        self.stop.touch()
        try:self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.stop.unlink(missing_ok=True);self.ready.unlink(missing_ok=True);self.ui_socket.unlink(missing_ok=True)

class ReleaseFeed:
    """Local event delivery from the host's single negotiated network connection."""
    def __init__(self,a,current=None):
        self.path=a.runtime_dir/'release.sock';self.path.unlink(missing_ok=True)
        self.socket=socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM)
        try:self.socket.bind(str(self.path));os.chmod(self.path,0o600)
        except Exception:self.socket.close();raise
    def take(self):
        if not select.select([self.socket],[],[],.1)[0]:return None
        latest=None
        for _ in range(64):
            data=self.socket.recv(8193)
            try:
                if len(data)>8192:raise ValueError('Oversized local release notice')
                event=json.loads(data)
                if event.get('version')!=1 or event.get('type') not in ('hello','upgrade'):raise ValueError('Invalid local hello')
                release=event['release'];valid_release(release['id'])
                if release.get('web_entry')!=f"/releases/{release['id']}/web/index.html":raise ValueError('Invalid local release entry')
                latest=release
            except (ValueError,KeyError,TypeError):pass
            if not select.select([self.socket],[],[],0)[0]:break
        return latest
    def close(self):self.socket.close();self.path.unlink(missing_ok=True)

def bootstrap_release(a,current=None):
    origin=validate_origin(a.url)
    url=urllib.parse.urlsplit(urllib.parse.urljoin(a.url,'/session'))
    endpoint=urllib.parse.urlunsplit(('wss' if origin.scheme=='https' else 'ws',url.netloc,url.path,'',''))
    result=subprocess.run([getattr(a,'session_bootstrap','/opt/otrios-dashboard/session-bootstrap'),endpoint,current or ''],capture_output=True,text=True,timeout=15,check=True)
    if len(result.stdout)>8192:raise ValueError('Oversized bootstrap hello')
    event=json.loads(result.stdout)
    if event.get('version')!=1 or event.get('type')!='hello':raise ValueError('Invalid bootstrap hello')
    release=event['release'];valid_release(release['id'])
    if release.get('web_entry')!=f"/releases/{release['id']}/web/index.html":raise ValueError('Invalid bootstrap entry')
    return release

def session_host(cache,release):
    return json.loads((cache/release/'manifest.json').read_text()).get('host_session')==1

def main(a):
    global running
    a.cache.mkdir(mode=0o700,parents=True,exist_ok=True);a.runtime_dir.mkdir(mode=0o700,parents=True,exist_ok=True)
    lock=open(a.runtime_dir/'agent.lock','a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    for p in a.cache.iterdir():
        if p.name.startswith(('.download-','.verify-')) and p.is_dir() and not p.is_symlink():shutil.rmtree(p)
    host=None;current=None;last_good=None;pending=None;retry_at=0
    good_file=a.cache/'last-good.json'
    feed=ReleaseFeed(a,current)
    if good_file.exists():
        try:
            value=json.loads(good_file.read_text());last_good=cached(a.cache,value['id'],a.key,a.openssl)
            if session_host(a.cache,last_good[0]):
                host=Host(a,*last_good);host.wait_ready(require_stream=False);current=last_good[0]
                print('RESUMED signed_release='+current,flush=True)
        except Exception as e:
            if host:host.close();host=None
            print('Cached app unavailable: '+str(e),flush=True)
    try:
        while running:
            try:
                event=feed.take()
                if host is not None and host.process.poll() is not None:
                    host.close();host=None;pending=None;event=None;retry_at=0
                if host is None and pending is None and time.monotonic()>=retry_at:event=bootstrap_release(a,current)
                if event is not None:
                    if pending is None or event['id']!=pending['id']:retry_at=0
                    pending=event
                if pending is None or time.monotonic()<retry_at:continue
                release=pending['id']
                if host is not None and host.process.poll() is None and release==current:pending=None;continue
                old=host;info=old.info() if old and old.process.poll() is None else {}
                if old and old.process.poll() is None and not hot_reload_ready(info):continue
                if old and getattr(old,'offered_ui',None)==release:
                    if old.info().get('ui_release')==release and hot_reload_ready(old.info()):
                        current=release;last_good=(release,a.cache/release/'dashboard-native');pending=None
                        atomic(good_file,json.dumps({'id':release}).encode());print('UI ACTIVE signed_release='+release,flush=True)
                    continue
                code_url=getattr(a,'code_url',None) or a.url
                release,exe=qualify(code_url,a.cache,a.key,a.openssl,release,limit=a.cache_bytes,pins=tuple(v for v in (current,last_good[0] if last_good else None) if v))
                if old and old.process.poll() is None and info.get('ui_updates') is True and app_only_change(json.loads((a.cache/current/'manifest.json').read_text()),json.loads((a.cache/release/'manifest.json').read_text())):
                    if old.update_ui(release,a.cache):
                        current=release;last_good=(release,exe);pending=None
                        atomic(good_file,json.dumps({'id':release}).encode());print('UI ACTIVE signed_release='+release,flush=True)
                    continue
                host=None
                if old:old.close()
                candidate=Host(a,release,exe,screen=info.get('screen') if info.get('ui_ref') else None)
                try:candidate.wait_ready(require_stream=old is not None and info.get('stream_ready') is not False)
                except Exception:
                    candidate.close();retry_at=time.monotonic()+30
                    if last_good and last_good[0]!=release:
                        last_good=cached(a.cache,last_good[0],a.key,a.openssl);host=Host(a,*last_good);host.wait_ready(require_stream=False)
                    raise
                host=candidate;current=release;last_good=(release,exe)
                atomic(good_file,json.dumps({'id':release}).encode());print('ACTIVE signed_release='+release,flush=True)
            except Exception as e:
                print('Update kept previous app: '+str(e),flush=True)
                retry_at=max(retry_at,time.monotonic()+5)
    finally:
        if host:host.close()
        feed.close()
        lock.close()
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--code-url');p.add_argument('--session-bootstrap',default='/opt/otrios-dashboard/session-bootstrap');p.add_argument('--cache',type=pathlib.Path,required=True);p.add_argument('--runtime-dir',type=pathlib.Path,default=pathlib.Path(os.environ.get('XDG_RUNTIME_DIR','/run/otrios-direct-console'))/'otrios');p.add_argument('--cache-bytes',type=int,default=CACHE_BYTES);p.add_argument('--key',type=pathlib.Path,required=True);p.add_argument('--openssl',default='/usr/bin/openssl');p.add_argument('--capture-first-frame',action='store_true');p.add_argument('--rotation',type=int,choices=[0,90,180,270],default=90);p.add_argument('--touch-calibration',type=pathlib.Path,required=True);a=p.parse_args()
    if a.cache_bytes<2*MAX_BINARY+65536:p.error('Cache budget must fit one signed release')
    a.touch_matrix=touch_calibration(a.touch_calibration)
    try:
        validate_origin(a.url)
        if a.code_url:validate_origin(a.code_url)
    except ValueError as error:p.error(str(error))
    def stop(*_):
        global running
        running=False
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop);main(a)
