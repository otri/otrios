#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Record a qualified runtime, or publish an app UI or behavior module using that verified runtime."""
import argparse,hashlib,importlib.util,json,pathlib,re,shutil,subprocess,tempfile

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
        if (p.parts[0]=='crates' and 'tests' in p.parts) or (p.parts[0]=='scripts' and p.name.startswith('test')):continue
        if (len(p.parts)==3 and p.parts[0]=='apps' and p.name in ['ui.rs','components.rs']) or (len(p.parts)==4 and p.parts[0]=='apps' and p.parts[2]=='behavior' and p.name=='main.wat') or 'pkg' in p.parts or 'package' in p.parts:continue
        if name in ['Cargo.toml','Cargo.lock','rust-toolchain.toml'] or (p.parts[0] in ['apps','crates','tools','web','scripts'] and p.suffix in ['.rs','.toml','.py','.sh','.js','.mjs','.wat','.html','.css','.c','.h','.vert','.frag','.zst','.txt']):
            selected.append((name,file_hash(repo/name)))
    return hashlib.sha256(json.dumps(selected,separators=(',',':')).encode()).hexdigest()

def verified(a,pointer):
    return activation.verify(a.runtime/'releases',a.runtime/'publisher/public.pem',a.openssl,pointer.read_bytes())
def record(a):
    release=verified(a,a.baseline)
    files={name:{'path':str(path.resolve()),'sha256':file_hash(path)}for name,path in [('server',a.server),('monitoring',a.monitoring),('bootstrap',a.bootstrap)]}
    value={'format':'otrios-app-runtime-v1','release':release,'runtime_inputs':runtime_inputs(a.repo),'binaries':files,'source_dirty':bool(subprocess.check_output(['git','status','--porcelain'],cwd=a.repo,text=True).strip()),'source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.repo,text=True).strip()}
    publisher.atomic(a.record,json.dumps(value,indent=2).encode());print('RECORDED '+release)

def publish(a):
    receipt=json.loads(a.record.read_text())
    if receipt.get('format')!='otrios-app-runtime-v1' or runtime_inputs(a.repo)!=receipt['runtime_inputs']:raise ValueError('Runtime inputs changed; qualify and record a full build')
    for item in receipt['binaries'].values():
        if file_hash(pathlib.Path(item['path']))!=item['sha256']:raise ValueError('Reused authority binary changed')
    source_names=['behavior/main.wat'] if a.kind=='behavior' else ['ui.rs','components.rs']
    source_root=a.repo/'apps'/a.app
    if not (source_root/'app.rs').exists():source_root=a.repo/'apps'/a.app.split('/')[0]
    ui_sources={name:file_hash(source_root/name) for name in source_names if (source_root/name).exists()}
    if a.kind=='behavior' and not ui_sources:raise ValueError('App has no behavior source')
    release=verified(a,a.baseline);base=a.runtime/'releases'/release
    recorded=a.runtime/'releases'/receipt['release']
    # Baselines after successive UI edits must retain the recorded executable closure.
    old=json.loads((recorded/'manifest.json').read_bytes());manifest=json.loads((base/'manifest.json').read_bytes())
    if not agent.app_only_change(old,manifest):raise ValueError('Baseline is not compatible with the qualified runtime')
    with tempfile.TemporaryDirectory(prefix='app-ui-',dir=a.runtime/'build') as tmp:
        stage=pathlib.Path(tmp);shutil.copytree(base/'web',stage/'web')
        apps=stage/'apps';apps.mkdir();shutil.copyfile(base/'apps/catalog.flex',apps/'catalog.flex')
        for source in (base/'authority/apps').rglob('app.flex'):
            folder=source.parent;dest=apps/folder.relative_to(base/'authority/apps')/'package';dest.mkdir(parents=True)
            for name in ['app.flex','ui.flex','behavior.wasm']:
                if (folder/name).is_file():shutil.copyfile(folder/name,dest/name)
        command=['cargo','run','-p','otrios-dashboard','--bin','canvas-pack','--features','canvas','--locked','--offline','--',str(apps),('--app-behavior' if a.kind=='behavior' else '--app-ui'),a.app,str(base)]
        if a.kind=='behavior':command.append(str(a.repo/'apps'/a.app/'behavior/main.wat'))
        subprocess.run(command,cwd=a.repo,check=True)
        if {name:file_hash(source_root/name) for name in ui_sources}!=ui_sources:raise ValueError('App UI changed during packing; retry')
        if runtime_inputs(a.repo)!=receipt['runtime_inputs']:raise ValueError('Runtime inputs changed during packing')
        result=publisher.publish(stage/'web',base/'native/armv7/dashboard-native',a.runtime/'releases',a.runtime/'publisher/private.pem',a.openssl,activate=False,catalog=apps/'catalog.flex')
        next_manifest=json.loads((a.runtime/'releases'/result['id']/'manifest.json').read_bytes())
        compatible=agent.app_only_change if a.kind=='behavior' else agent.ui_only_change
        if not compatible(manifest,next_manifest):raise ValueError('App update changed host runtime assets')
        changed={name for section in ['assets','authority'] for name,item in next_manifest[section].items()if manifest[section].get(name)!=item}
        allowed={'apps/catalog.flex',f'authority/apps/{a.app}/ui.flex'}
        if a.kind=='behavior':allowed.add(f'authority/apps/{a.app}/behavior.wasm')
        if not changed<=allowed:raise ValueError('App update changed another package')
        output=a.runtime/'build/app-release.json';publisher.atomic(output,json.dumps(result,sort_keys=True).encode());verified(a,output)
        if a.kind=='ui':publisher.atomic(a.runtime/'build/app-ui-release.json',output.read_bytes())
        if result['id']==release:
            print('UNCHANGED '+release);return result
        if a.deploy_config:
            deployer=module('deployer','deploy-dashboard-frame.py');b=receipt['binaries']
            deployer.deploy(json.loads(a.deploy_config.read_text()),output,b['server']['path'],b['monitoring']['path'],b['bootstrap']['path'],ui_base=release)
            publisher.atomic(a.baseline,json.dumps(result,sort_keys=True).encode())
        print(('DEPLOYED 'if a.deploy_config else'STAGED ')+result['id'])
        return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('action',choices=['record','publish']);p.add_argument('--repo',type=pathlib.Path,default=pathlib.Path(__file__).resolve().parent.parent);p.add_argument('--runtime',type=pathlib.Path,required=True);p.add_argument('--baseline',type=pathlib.Path,required=True);p.add_argument('--record',type=pathlib.Path,required=True);p.add_argument('--openssl',default='openssl');p.add_argument('--app');p.add_argument('--kind',choices=['ui','behavior'],default='ui');p.add_argument('--deploy-config',type=pathlib.Path)
    for name in ['server','monitoring','bootstrap']:p.add_argument('--'+name,type=pathlib.Path)
    a=p.parse_args()
    import fcntl
    lock=(a.runtime/'watcher.lock').open('a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    if a.action=='record':
        if not all([a.server,a.monitoring,a.bootstrap]):p.error('Recording requires matching server, monitoring and bootstrap binaries')
        record(a)
    else:
        if not a.app or not re.fullmatch(r'[a-z-]+(?:/[a-z-]+)*',a.app) or a.app.split('/')[0] not in [p.parent.name for p in (a.repo/'apps').glob('*/app.rs')]:p.error('Select an existing app/screen with --app')
        publish(a)

if __name__=='__main__':main()
