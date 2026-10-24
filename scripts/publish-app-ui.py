#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Record a qualified runtime, or publish only app UI using that verified runtime."""
import argparse,hashlib,importlib.util,json,pathlib,shutil,subprocess,tempfile

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,pathlib.Path(__file__).with_name(file));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
publisher=module('publisher','publish-dashboard.py')
activation=module('activation','activate-dashboard-release.py')
agent=module('agent','frame-dashboard-agent.py')

def file_hash(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def runtime_inputs(repo):
    # App UI authoring may change; executable and schema inputs must match.
    names=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard'],cwd=repo,text=True).splitlines()
    selected=[]
    for name in sorted(set(names)):
        p=pathlib.PurePosixPath(name)
        if (len(p.parts)==3 and p.parts[0]=='apps' and p.name in ['ui.rs','components.rs']) or 'pkg' in p.parts or 'package' in p.parts:continue
        if name in ['Cargo.toml','Cargo.lock','rust-toolchain.toml'] or (p.parts[0] in ['apps','crates','tools','web','scripts'] and p.suffix in ['.rs','.toml','.py','.sh','.js','.mjs','.wat','.html','.css','.c','.h','.vert','.frag','.zst','.txt']):
            selected.append((name,file_hash(repo/name)))
    return hashlib.sha256(json.dumps(selected,separators=(',',':')).encode()).hexdigest()

def verified(a,pointer):
    return activation.verify(a.runtime/'releases',a.runtime/'publisher/public.pem',a.openssl,pointer.read_bytes())
def record(a):
    release=verified(a,a.baseline)
    files={name:{'path':str(path.resolve()),'sha256':file_hash(path)}for name,path in [('server',a.server),('monitoring',a.monitoring),('bootstrap',a.bootstrap)]}
    value={'format':'otrios-ui-runtime-v1','release':release,'runtime_inputs':runtime_inputs(a.repo),'binaries':files,'source_dirty':bool(subprocess.check_output(['git','status','--porcelain'],cwd=a.repo,text=True).strip()),'source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.repo,text=True).strip()}
    publisher.atomic(a.record,json.dumps(value,indent=2).encode());print('RECORDED '+release)

def publish(a):
    receipt=json.loads(a.record.read_text())
    if receipt.get('format')!='otrios-ui-runtime-v1' or runtime_inputs(a.repo)!=receipt['runtime_inputs']:raise ValueError('Runtime inputs changed; qualify and record a full build')
    for item in receipt['binaries'].values():
        if file_hash(pathlib.Path(item['path']))!=item['sha256']:raise ValueError('Reused authority binary changed')
    ui_sources={name:file_hash(a.repo/'apps'/a.app/name) for name in ['ui.rs','components.rs'] if (a.repo/'apps'/a.app/name).exists()}
    release=verified(a,a.baseline);base=a.runtime/'releases'/release
    recorded=a.runtime/'releases'/receipt['release']
    # Baselines after successive UI edits must retain the recorded executable closure.
    old=json.loads((recorded/'manifest.json').read_bytes());manifest=json.loads((base/'manifest.json').read_bytes())
    if not agent.ui_only_change(old,manifest):raise ValueError('Baseline is not compatible with the qualified runtime')
    with tempfile.TemporaryDirectory(prefix='app-ui-',dir=a.runtime/'build') as tmp:
        stage=pathlib.Path(tmp);shutil.copytree(base/'web',stage/'web')
        apps=stage/'apps';apps.mkdir();shutil.copyfile(base/'apps/catalog.flex',apps/'catalog.flex')
        for folder in (base/'authority/apps').iterdir():shutil.copytree(folder,apps/folder.name/'package')
        subprocess.run(['cargo','run','-p','otrios-dashboard','--bin','canvas-pack','--features','canvas','--locked','--offline','--',str(apps),'--app-ui',a.app,str(base)],cwd=a.repo,check=True)
        if {name:file_hash(a.repo/'apps'/a.app/name) for name in ui_sources}!=ui_sources:raise ValueError('App UI changed during packing; retry')
        if runtime_inputs(a.repo)!=receipt['runtime_inputs']:raise ValueError('Runtime inputs changed during packing')
        result=publisher.publish(stage/'web',base/'native/armv7/dashboard-native',a.runtime/'releases',a.runtime/'publisher/private.pem',a.openssl,activate=False,catalog=apps/'catalog.flex')
        next_manifest=json.loads((a.runtime/'releases'/result['id']/'manifest.json').read_bytes())
        if not agent.ui_only_change(manifest,next_manifest):raise ValueError('App UI publish changed executable assets')
        output=a.runtime/'build/app-ui-release.json';publisher.atomic(output,json.dumps(result,sort_keys=True).encode());verified(a,output)
        if result['id']==release:
            print('UNCHANGED '+release);return result
        if a.deploy_config:
            deployer=module('deployer','deploy-dashboard-frame.py');b=receipt['binaries']
            deployer.deploy(json.loads(a.deploy_config.read_text()),output,b['server']['path'],b['monitoring']['path'],b['bootstrap']['path'],ui_base=release)
            publisher.atomic(a.baseline,json.dumps(result,sort_keys=True).encode())
        print(('DEPLOYED 'if a.deploy_config else'STAGED ')+result['id'])
        return result

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('action',choices=['record','publish']);p.add_argument('--repo',type=pathlib.Path,default=pathlib.Path(__file__).resolve().parent.parent);p.add_argument('--runtime',type=pathlib.Path,required=True);p.add_argument('--baseline',type=pathlib.Path,required=True);p.add_argument('--record',type=pathlib.Path,required=True);p.add_argument('--openssl',default='openssl');p.add_argument('--app');p.add_argument('--deploy-config',type=pathlib.Path)
    for name in ['server','monitoring','bootstrap']:p.add_argument('--'+name,type=pathlib.Path)
    a=p.parse_args()
    import fcntl
    lock=(a.runtime/'watcher.lock').open('a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    if a.action=='record':
        if not all([a.server,a.monitoring,a.bootstrap]):p.error('Recording requires matching server, monitoring and bootstrap binaries')
        record(a)
    else:
        if not a.app or a.app not in [p.parent.name for p in (a.repo/'apps').glob('*/app.rs')]:p.error('Select an existing app with --app')
        publish(a)
