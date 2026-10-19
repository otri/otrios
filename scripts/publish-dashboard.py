#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Publish a complete, immutable, signed pair of web and native app builds."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile

def digest(data): return hashlib.sha256(data).hexdigest()
def atomic(path, data):
    path=pathlib.Path(path); fd,tmp=tempfile.mkstemp(prefix='.'+path.name+'-',dir=path.parent)
    try:
        with os.fdopen(fd,'wb') as f:f.write(data);f.flush();os.fsync(f.fileno())
        os.replace(tmp,path)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)
def publish(web,native,out,key,openssl,sdl=None,activate=True,catalog=None):
    web,native,out,key=map(pathlib.Path,(web,native,out,key));out.mkdir(parents=True,exist_ok=True)
    elf=native.read_bytes()
    if len(elf)<52 or elf[:6]!=b'\x7fELF\x01\x01' or int.from_bytes(elf[18:20],'little')!=40:raise ValueError('Expected ARM32 little-endian ELF app')
    assets={};authority={}
    catalog=pathlib.Path(catalog) if catalog else web.parent.parent/'apps/catalog.flex'
    if catalog.is_symlink():raise ValueError('Catalog symlink rejected')
    with tempfile.TemporaryDirectory(prefix='.staging-',dir=out) as tmp:
        stage=pathlib.Path(tmp)
        names=['index.html','style.css','app.js','state-worker.js','release-gate.mjs','monitoring-session.mjs','artifact-cache.mjs','artifact-cache-worker.js','pkg/otrios_dashboard.js','pkg/otrios_dashboard_bg.wasm']
        names += ['app-catalog.mjs','gpu-scene.mjs','slug-shaders.mjs','font-resource.mjs','font-reference.mjs','fonts/Roboto-Regular.otq.zst','Slug-LICENSE.txt']
        for name in names:
            source=web/name
            if not source.exists() and (name.endswith('.mjs') or name.endswith('.js') or name=='Slug-LICENSE.txt'):source=web.parent/'shared'/name
            if not source.exists() and name.startswith('fonts/'):source=web.parent/'dashboard'/name
            data=source.read_bytes()
            if name.endswith('.wasm') and data[:8]!=b'\0asm\x01\0\0\0':raise ValueError('Invalid Wasm artifact')
            if name=='index.html':data=data.replace(b'href="/style.css"',b'href="./style.css"').replace(b'src="/app.js"',b'src="./app.js"')
            dest=stage/'web'/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
            assets['web/'+name]={'bytes':len(data),'sha256':digest(data)}
        data=catalog.read_bytes()
        if not 0<len(data)<=512*1024:raise ValueError('Invalid app catalog size')
        (stage/'apps').mkdir();(stage/'apps/catalog.flex').write_bytes(data)
        assets['apps/catalog.flex']={'bytes':len(data),'sha256':digest(data)}
        # Private app bytes are signed with this immutable release but never
        # copied into the public module replica or exposed by an HTTP allowlist.
        for folder in sorted(catalog.parent.iterdir()):
            package=folder/'package'
            if folder.is_symlink() or package.is_symlink():raise ValueError('App package directory symlink rejected')
            if not package.is_dir() or not (package/'ui.flex').is_file():continue
            if not folder.name or any(c not in 'abcdefghijklmnopqrstuvwxyz-' for c in folder.name):raise ValueError('Invalid app package name')
            for name in ['app.flex','ui.flex','behavior.wasm']:
                source=package/name
                if not source.exists():continue
                if source.is_symlink() or not source.resolve().is_relative_to(catalog.parent.resolve()):raise ValueError('App package symlink rejected')
                data=source.read_bytes()
                if not 0<len(data)<=512*1024:raise ValueError('App package asset budget exceeded')
                path='authority/apps/'+folder.name+'/'+name;dest=stage/path;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
                authority[path]={'bytes':len(data),'sha256':digest(data)}
        path='native/armv7/dashboard-native';dest=stage/path;dest.parent.mkdir(parents=True);dest.write_bytes(elf)
        assets[path]={'bytes':len(elf),'sha256':digest(elf)}
        if sdl is not None:
            fallback=pathlib.Path(sdl).read_bytes()
            if len(fallback)<52 or fallback[:6]!=b'\x7fELF\x01\x01' or int.from_bytes(fallback[18:20],'little')!=40:raise ValueError('Expected ARM32 SDL recovery executable')
            path='native/armv7/dashboard-sdl';(stage/path).write_bytes(fallback)
            assets[path]={'bytes':len(fallback),'sha256':digest(fallback)}
        manifest={'format':'otrios-release-v1','state_schema':'otrios-canvas-merkle-state-v1','sync_protocol':'otrios-canvas-sync-v2','native_target':'armv7-unknown-linux-gnueabihf','assets':assets,'authority':authority}
        raw=json.dumps(manifest,sort_keys=True,separators=(',',':')).encode();release=digest(raw)
        (stage/'manifest.json').write_bytes(raw)
        subprocess.run([openssl,'pkeyutl','-sign','-rawin','-inkey',str(key),'-in',str(stage/'manifest.json'),'-out',str(stage/'manifest.sig')],check=True,capture_output=True)
        destination=out/release
        if destination.exists():
            if (destination/'manifest.json').read_bytes()!=raw:raise ValueError('Existing release is corrupt')
        else:os.replace(stage,destination);pathlib.Path(stage).mkdir()
        pointer={'id':release,'web_entry':f'/releases/{release}/web/index.html','manifest':f'/releases/{release}/manifest.json','signature':f'/releases/{release}/manifest.sig','state_schema':manifest['state_schema'],'sync_protocol':manifest['sync_protocol']}
        if activate:atomic(out/'current.json',json.dumps(pointer,sort_keys=True).encode())
        return pointer
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--web',required=True);p.add_argument('--native',required=True);p.add_argument('--sdl');p.add_argument('--catalog');p.add_argument('--out',required=True);p.add_argument('--key',required=True);p.add_argument('--openssl',default='openssl');a=p.parse_args()
    print(json.dumps(publish(a.web,a.native,a.out,a.key,a.openssl,a.sdl,catalog=a.catalog)))
