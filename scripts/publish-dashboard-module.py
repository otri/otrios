#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Replicate a verified immutable module into the independent host's Docker volume.
Only the short-lived publisher gets a writable mount. The public server is read-only.
"""
import argparse,fcntl,hashlib,importlib.util,io,json,os,pathlib,subprocess,tarfile,tempfile,time,urllib.request,urllib.parse
IMAGE='caddy:2.11.7-alpine@sha256:d8542f48d34a9cf4e4c11a478865229840e87e4c96ea3f439101f31a5d35f75f'
spec=importlib.util.spec_from_file_location('frame_agent',pathlib.Path(__file__).with_name('frame-dashboard-agent.py'));agent=importlib.util.module_from_spec(spec);spec.loader.exec_module(agent)
WEB={'web/index.html','web/style.css','web/app.js','web/state-worker.js','web/pkg/otrios_dashboard.js','web/pkg/otrios_dashboard_bg.wasm','apps/catalog.flex'}
OPTIONAL={'web/app-catalog.mjs','web/Slug-LICENSE.txt','web/gpu-scene.mjs','web/slug-shaders.mjs','web/font-resource.mjs','web/font-reference.mjs','web/fonts/Roboto-Regular.otq.zst','web/release-gate.mjs','web/monitoring-session.mjs','web/artifact-cache.mjs','web/artifact-cache-worker.js'}
NATIVE={'native/armv7/dashboard-native','native/armv7/dashboard-sdl'}
def bundle(releases,key,openssl,pointer=None):
    if pointer is None:pointer=(releases/'current.json').read_bytes()
    if len(pointer)>8192:raise ValueError('Oversized release pointer')
    current=json.loads(pointer);release=current['id'];agent.valid_release(release)
    if current.get('web_entry')!=f'/releases/{release}/web/index.html':raise ValueError('Unexpected module entry')
    folder=releases/release;raw=(folder/'manifest.json').read_bytes();sig=(folder/'manifest.sig').read_bytes()
    manifest=agent.verify_manifest(raw,sig,release,releases,key,openssl)
    assets=manifest['assets']
    if not WEB.issubset(assets) or set(assets)-WEB-OPTIONAL-NATIVE:raise ValueError('Unexpected module assets')
    payload={'manifest.json':raw,'manifest.sig':sig};total=0
    for name,item in assets.items():
        size=item['bytes']
        if not 0<size<=32*1024*1024:raise ValueError('Invalid module asset size')
        total+=size
        if total>128*1024*1024:raise ValueError('Module exceeds transfer limit')
        path=folder/name
        if path.is_symlink() or not path.resolve().is_relative_to(folder.resolve()):raise ValueError('Module asset escaped release folder')
        data=path.read_bytes()
        if len(data)!=size or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('Module asset digest rejected: '+name)
        payload[name]=data
    return release,pointer,payload

def notify(url):
    parsed=urllib.parse.urlsplit(url)
    if parsed.scheme!='http' or parsed.hostname not in ('127.0.0.1','::1') or parsed.path!='/publish' or parsed.query:raise ValueError('Use the loopback control publisher')
    for attempt in range(3):
        try:
            request=urllib.request.Request(url,data=b'',method='POST')
            with urllib.request.urlopen(request,timeout=5) as response:
                if response.status!=204:raise ValueError('Release control did not acknowledge activation')
            return
        except Exception:
            if attempt==2:raise
            time.sleep(1)

def publish(a):
    release,pointer,payload=bundle(a.releases,a.key,a.openssl,a.pointer.read_bytes()if a.pointer else None)
    with tempfile.TemporaryFile()as archive:
        with tarfile.open(fileobj=archive,mode='w')as tar:
            checks=[]
            def add(name,data):
                info=tarfile.TarInfo(name);info.size=len(data);info.mode=0o644;tar.addfile(info,io.BytesIO(data))
            for name,data in sorted(payload.items()):
                path=f'releases/{release}/{name}';add(path,data);checks.append(hashlib.sha256(data).hexdigest()+'  '+path)
            add('checksums.sha256',('\n'.join(checks)+'\n').encode());add('current.pending.json',pointer)
        archive.seek(0)
        # release is a checked lowercase hex digest. No shell interpolation of paths,
        # credentials, caller-controlled commands or payload bytes is performed.
        script=f'''stage=/srv/modules/.staging-{release}
mkdir -p "$stage" /srv/modules/releases
trap 'rm -rf "$stage"' EXIT
cd "$stage"
tar -xf -
sha256sum -c checksums.sha256 >/dev/null
if test -d /srv/modules/releases/{release}; then
  (cd /srv/modules && sha256sum -c "$stage/checksums.sha256" >/dev/null)
else
  mv releases/{release} /srv/modules/releases/{release}
fi
mv current.pending.json /srv/modules/current.pending.json
mv /srv/modules/current.pending.json /srv/modules/current.json
'''
        subprocess.run(['docker','run','--rm','-i','--network','none','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges:true','-v',a.volume+':/srv/modules','--entrypoint','/bin/sh',IMAGE,'-ec',script],stdin=archive,check=True)
    if a.activate_origin:agent.atomic(a.releases/'current.json',pointer)
    notify(getattr(a,'notify_url','http://127.0.0.1:8879/publish'))
    print('MODULE '+release,flush=True);return release

def main():
    p=argparse.ArgumentParser();p.add_argument('--releases',type=pathlib.Path,required=True);p.add_argument('--key',type=pathlib.Path,required=True);p.add_argument('--openssl',default='openssl');p.add_argument('--volume',default='otrios-dashboard-modules');p.add_argument('--watch',action='store_true');p.add_argument('--pointer',type=pathlib.Path);p.add_argument('--activate-origin',action='store_true');p.add_argument('--notify-url',default='http://127.0.0.1:8879/publish');p.add_argument('--marker',type=pathlib.Path,required=True);a=p.parse_args()
    if any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-'for c in a.volume):p.error('Use a named Docker volume')
    if a.activate_origin and (not a.pointer or a.watch):p.error('Origin activation requires one staged pointer')
    lock=open(a.marker.with_suffix('.lock'),'a')
    while True:
        try:
            fcntl.flock(lock,fcntl.LOCK_EX)
            try:
                last=a.marker.read_text()if a.marker.exists()else None
                wanted=json.loads((a.pointer or a.releases/'current.json').read_text())['id'];agent.valid_release(wanted)
                if wanted!=last or a.pointer:
                    release=publish(a);agent.atomic(a.marker,release.encode())
            finally:fcntl.flock(lock,fcntl.LOCK_UN)
            if not a.watch:return
            time.sleep(1)
        except Exception as e:
            print('MODULE PUBLICATION/NOTIFICATION FAILED; inspect active pointer: '+str(e),flush=True)
            if not a.watch:raise
            time.sleep(10)
if __name__=='__main__':main()
