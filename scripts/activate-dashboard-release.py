#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Verify a complete local signed release, then activate the single host store."""
import argparse,fcntl,gzip,hashlib,importlib.util,io,json,os,pathlib,re,subprocess,time,urllib.request,urllib.parse
spec=importlib.util.spec_from_file_location('frame_agent',pathlib.Path(__file__).with_name('frame-dashboard-agent.py'));agent=importlib.util.module_from_spec(spec);spec.loader.exec_module(agent)
WEB={'web/index.html','web/style.css','web/app.js','web/state-worker.js','web/pkg/otrios_dashboard.js','web/pkg/otrios_dashboard_bg.wasm','apps/catalog.flex'}
OPTIONAL={'web/text-input.mjs','web/fonts/Roboto.ttf','web/fonts/OFL.txt','web/document-transfer.mjs','web/presentation-transfer.mjs','web/app-transfers.mjs','web/touch-input.mjs','web/app-catalog.mjs','web/Slug-LICENSE.txt','web/gpu-scene.mjs','web/viewport.mjs','web/ui-update.mjs','web/navigation-timings.mjs','web/slug-shaders.mjs','web/font-resource.mjs','web/font-reference.mjs','web/fonts/Roboto-Regular.otq.zst','web/release-gate.mjs','web/session.mjs','web/monitoring-session.mjs','web/artifact-cache.mjs','web/artifact-cache-worker.js'}
NATIVE={'native/armv7/dashboard-native'}
def read(path,limit):
    with path.open('rb')as f:data=f.read(limit+1)
    if len(data)>limit:raise ValueError('Release item exceeds budget')
    return data

def verify(releases,key,openssl,pointer,bundle_checker=None):
    if len(pointer)>8192:raise ValueError('Oversized release pointer')
    current=json.loads(pointer);release=current['id'];agent.valid_release(release)
    if current.get('web_entry')!=f'/releases/{release}/web/index.html':raise ValueError('Unexpected code entry')
    folder=releases/release
    if folder.is_symlink():raise ValueError('Release directory symlink rejected')
    raw=read(folder/'manifest.json',65536);sig=read(folder/'manifest.sig',64)
    manifest=agent.verify_manifest(raw,sig,release,releases,key,openssl);assets=manifest['assets'];authority=manifest.get('authority',{})
    compressed={name for name in assets if name.endswith('.gz') and name[:-3] in WEB|OPTIONAL and name.startswith('web/')}
    if not (WEB|{'native/armv7/dashboard-native'}).issubset(assets) or set(assets)-WEB-OPTIONAL-NATIVE-compressed:raise ValueError('Unexpected public assets')
    if not ({'web/session.mjs','web/release-gate.mjs'}&assets.keys()):raise ValueError('Missing bootstrap module')
    bundled=manifest.get('bundle_abi')
    if bundled not in (None,'otrios-app-bundle-v1'):raise ValueError('Unsupported bundle ABI')
    bundle_names=set();total=0
    for name,item in list(assets.items())+list(authority.items()):
        private=name in authority
        if private:
            if bundled:
                if not re.fullmatch(r'authority/bundles/([a-z-]+\.otapp)/(?:[A-Za-z0-9_.-]+/)*[A-Za-z0-9_.-]+',name) or any(part in ('.','..') for part in pathlib.PurePosixPath(name).parts):raise ValueError('Unexpected private bundle artifact')
                bundle_names.add(pathlib.PurePosixPath(name).parts[2])
            elif not re.fullmatch(r'authority/apps/[a-z-]+(?:/[a-z-]+)*/(app.flex|ui.flex|behavior.wasm)',name):raise ValueError('Unexpected private artifact')
        size=item['bytes'];total+=size
        if not (0 if private and bundled else 1)<=size<=(512*1024 if private else 32*1024*1024) or total>128*1024*1024:raise ValueError('Release exceeds activation budget')
        path=folder/name
        if path.is_symlink() or not path.resolve().is_relative_to(folder.resolve()):raise ValueError('Release path escaped store')
        data=read(path,size)
        if len(data)!=size or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('Release asset digest rejected: '+name)
        if name in compressed:
            original=assets.get(name[:-3])
            if not original or not 0<original['bytes']<=32*1024*1024:raise ValueError('Missing/oversized encoded source')
            with gzip.GzipFile(fileobj=io.BytesIO(data))as stream:decoded=stream.read(original['bytes']+1)
            if len(decoded)!=original['bytes'] or hashlib.sha256(decoded).hexdigest()!=original['sha256']:raise ValueError('Encoded source digest rejected: '+name)
    if bundled:
        if not 1<=len(bundle_names)<=16:raise ValueError('Installed bundle count rejected')
        checker=bundle_checker or os.environ.get('OTRIOS_BUNDLE_CHECKER')
        if not checker:
            local=releases.parent/'build/mac-target/debug/canvas-bundle'
            checker=local if local.is_file() else pathlib.Path('/opt/otrios-authority/canvas-bundle')
        for name in sorted(bundle_names):
            result=subprocess.run([str(checker),'verify',str(folder/'authority/bundles'/name),'--key',str(key),'--openssl',openssl],capture_output=True,timeout=30)
            if result.returncode:raise ValueError('Installed bundle publisher/closure rejected: '+name)
    return release

def complete_ui(releases,installed,key,openssl,pointer,base):
    """Fill a data-only transfer from the verified installed immutable closure."""
    import os,shutil
    previous=(installed/'current.json').read_bytes()
    if json.loads(previous)['id']!=base:raise ValueError('UI deployment baseline changed')
    verify(installed,key,openssl,previous)
    old=json.loads((installed/base/'manifest.json').read_bytes())
    release=json.loads(pointer)['id'];agent.valid_release(release);folder=releases/release
    new=agent.verify_manifest(read(folder/'manifest.json',65536),read(folder/'manifest.sig',64),release,releases,key,openssl)
    if not agent.app_install_change(old,new):raise ValueError('App transfer changed runtime executable or policy metadata')
    for section in ['assets','authority']:
        for name,item in new[section].items():
            dest=folder/name
            if dest.exists():continue
            if old[section].get(name)!=item:raise ValueError('UI transfer omitted changed artifact')
            dest.parent.mkdir(parents=True,exist_ok=True)
            try:os.link(installed/base/name,dest)
            except OSError:shutil.copyfile(installed/base/name,dest)
    return verify(releases,key,openssl,pointer)

def notify(url):
    parsed=urllib.parse.urlsplit(url)
    if parsed.scheme!='http' or parsed.hostname not in ('127.0.0.1','::1') or parsed.path!='/admin/activate' or parsed.query:raise ValueError('Use the trusted loopback activation hook')
    for attempt in range(3):
        try:
            with urllib.request.urlopen(urllib.request.Request(url,data=b'',method='POST'),timeout=5)as response:
                if response.status!=204:raise ValueError('Activation not acknowledged')
            return
        except Exception:
            if attempt==2:raise
            time.sleep(1)

def durable_release(releases,release):
    """Flush the verified immutable closure before advancing its durable pointer."""
    folder=releases/release;manifest=json.loads(read(folder/'manifest.json',65536))
    paths=['manifest.json','manifest.sig',*manifest['assets'],*manifest.get('authority',{})]
    directories={folder,releases}
    for name in paths:
        path=folder/name
        with path.open('rb')as stream:os.fsync(stream.fileno())
        directories.update(parent for parent in path.parents if parent==folder or parent.is_relative_to(folder))
    for directory in sorted(directories,key=lambda path:len(path.parts),reverse=True):
        descriptor=os.open(directory,os.O_RDONLY)
        try:os.fsync(descriptor)
        finally:os.close(descriptor)

def guard_imports(notify_url,release,operation=None):
    """Called while holding .activation.lock, after complete release verification.

    The local authority owns authorization and durable decisions. This adapter
    only carries an exact planned release through that authority's bridge.
    """
    parsed=urllib.parse.urlsplit(notify_url)
    if parsed.scheme!='http' or parsed.hostname not in ('127.0.0.1','::1') or parsed.path!='/admin/activate' or parsed.query or parsed.username or parsed.password:
        raise ValueError('Use the trusted loopback activation hook')
    if operation is not None and not re.fullmatch(r'[a-f0-9]{32}',operation):raise ValueError('Invalid import operation')
    base=urllib.parse.urlunsplit((parsed.scheme,parsed.netloc,'','',''))
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def request(path,data=None):
        headers={'X-OTriOS-Request':'signer'}
        if data is not None:headers['Content-Type']='application/json'
        with opener.open(urllib.request.Request(base+path,data=None if data is None else json.dumps(data).encode(),headers=headers),timeout=10)as response:
            raw=response.read(16385)
        if len(raw)>16384:raise ValueError('Oversized install bridge response')
        return json.loads(raw)
    try:job=request('/admin/app-import')
    except urllib.error.HTTPError as error:
        # The pre-RFC0028 host has no install decision or bridge. Required only
        # for its one-time upgrade; an import never uses this compatibility path.
        if error.code==404 and operation is None:error.close();return
        raise
    if operation is None:
        if job and job['state']['phase']=='armed' and job['state']['release']!=release:
            raise ValueError('An armed app import requires its exact planned release')
        return
    if not job:
        state=request('/admin/app-import/'+operation)
        if state and state['operation']==operation and state['phase']=='applied' and state['release']==release:return
        raise ValueError('Import decision is no longer pending')
    if job['review']['stage']!=operation:raise ValueError('A different app import is pending')
    state=job['state']
    if state['phase']=='queued':state=request('/admin/app-import/'+operation+'/arm',{'release':release})
    if state['operation']!=operation or state['phase']!='armed' or state['release']!=release:
        raise ValueError('Import activation decision changed')

def activate(a):
    pointer=a.pointer.read_bytes()
    with (a.releases/'.activation.lock').open('a')as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        release=verify(a.releases,a.key,a.openssl,pointer)
        durable_release(a.releases,release)
        if getattr(a,'imports_guard',False) or getattr(a,'app_import',None):guard_imports(a.notify_url,release,getattr(a,'app_import',None))
        agent.atomic(a.releases/'current.json',pointer)
        notify(a.notify_url)
    print('ACTIVE '+release,flush=True);return release

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--releases',type=pathlib.Path,required=True);p.add_argument('--pointer',type=pathlib.Path,required=True);p.add_argument('--key',type=pathlib.Path,required=True);p.add_argument('--openssl',default='openssl');p.add_argument('--notify-url',default='http://127.0.0.1:8876/admin/activate');p.add_argument('--app-import');a=p.parse_args();a.imports_guard=True;activate(a)
