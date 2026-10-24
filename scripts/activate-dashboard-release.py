#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Verify a complete local signed release, then activate the single host store."""
import argparse,fcntl,hashlib,importlib.util,json,pathlib,re,time,urllib.request,urllib.parse
spec=importlib.util.spec_from_file_location('frame_agent',pathlib.Path(__file__).with_name('frame-dashboard-agent.py'));agent=importlib.util.module_from_spec(spec);spec.loader.exec_module(agent)
WEB={'web/index.html','web/style.css','web/app.js','web/state-worker.js','web/pkg/otrios_dashboard.js','web/pkg/otrios_dashboard_bg.wasm','apps/catalog.flex'}
OPTIONAL={'web/app-catalog.mjs','web/Slug-LICENSE.txt','web/gpu-scene.mjs','web/navigation.mjs','web/viewport.mjs','web/ui-update.mjs','web/slug-shaders.mjs','web/font-resource.mjs','web/font-reference.mjs','web/fonts/Roboto-Regular.otq.zst','web/release-gate.mjs','web/session.mjs','web/monitoring-session.mjs','web/artifact-cache.mjs','web/artifact-cache-worker.js'}
NATIVE={'native/armv7/dashboard-native'}
def read(path,limit):
    with path.open('rb')as f:data=f.read(limit+1)
    if len(data)>limit:raise ValueError('Release item exceeds budget')
    return data

def verify(releases,key,openssl,pointer):
    if len(pointer)>8192:raise ValueError('Oversized release pointer')
    current=json.loads(pointer);release=current['id'];agent.valid_release(release)
    if current.get('web_entry')!=f'/releases/{release}/web/index.html':raise ValueError('Unexpected code entry')
    folder=releases/release
    if folder.is_symlink():raise ValueError('Release directory symlink rejected')
    raw=read(folder/'manifest.json',65536);sig=read(folder/'manifest.sig',64)
    manifest=agent.verify_manifest(raw,sig,release,releases,key,openssl);assets=manifest['assets'];authority=manifest.get('authority',{})
    if not (WEB|{'native/armv7/dashboard-native'}).issubset(assets) or set(assets)-WEB-OPTIONAL-NATIVE:raise ValueError('Unexpected public assets')
    if not ({'web/session.mjs','web/release-gate.mjs'}&assets.keys()):raise ValueError('Missing bootstrap module')
    total=0
    for name,item in list(assets.items())+list(authority.items()):
        private=name in authority
        if private and not re.fullmatch(r'authority/apps/[a-z-]{1,64}/(app.flex|ui.flex|behavior.wasm)',name):raise ValueError('Unexpected private artifact')
        size=item['bytes'];total+=size
        if not 0<size<=(512*1024 if private else 32*1024*1024) or total>128*1024*1024:raise ValueError('Release exceeds activation budget')
        path=folder/name
        if path.is_symlink() or not path.resolve().is_relative_to(folder.resolve()):raise ValueError('Release path escaped store')
        data=read(path,size)
        if len(data)!=size or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('Release asset digest rejected: '+name)
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
    if not agent.app_only_change(old,new):raise ValueError('App transfer changed runtime executable or policy metadata')
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

def activate(a):
    pointer=a.pointer.read_bytes()
    with (a.releases/'.activation.lock').open('a')as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        release=verify(a.releases,a.key,a.openssl,pointer)
        agent.atomic(a.releases/'current.json',pointer)
        notify(a.notify_url)
    print('ACTIVE '+release,flush=True);return release

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--releases',type=pathlib.Path,required=True);p.add_argument('--pointer',type=pathlib.Path,required=True);p.add_argument('--key',type=pathlib.Path,required=True);p.add_argument('--openssl',default='openssl');p.add_argument('--notify-url',default='http://127.0.0.1:8876/admin/activate');a=p.parse_args();activate(a)
