#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Install/remove a signed app by publishing a complete registry with unchanged host binaries."""
import argparse,fcntl,importlib.util,json,pathlib,re,shutil,subprocess,tempfile

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,pathlib.Path(__file__).with_name(file))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

publisher=module('publisher','publish-dashboard.py')
activation=module('activation','activate-dashboard-release.py')
deployer=module('deployer','deploy-dashboard-frame.py')

def run(checker,*args):
    return subprocess.check_output([str(checker),*[str(arg) for arg in args]],text=True).strip()

def install(a):
    key=a.runtime/'publisher/public.pem';private=a.runtime/'publisher/private.pem'
    baseline=a.baseline or a.runtime/'releases/current.json'
    raw=baseline.read_bytes()
    base_id=activation.verify(a.runtime/'releases',key,a.openssl,raw,bundle_checker=a.checker)
    base=a.runtime/'releases'/base_id
    before=json.loads((base/'manifest.json').read_bytes())
    if before.get('bundle_abi')!='otrios-app-bundle-v1':raise ValueError('Qualify/deploy the bundle-capable host before installing apps')
    if a.bundle:
        result=run(a.checker,'verify',a.bundle,'--key',key,'--openssl',a.openssl).split()
        if len(result)!=3 or result[0]!='VERIFIED' or not re.fullmatch(r'[a-z-]+',result[1]):raise ValueError('Invalid bundle verification result')
        app,digest=result[1:]
    else:
        app=a.remove;digest=None
    if app=='home':raise ValueError('Home is owned by registry composition')
    with tempfile.TemporaryDirectory(prefix='app-install-',dir=a.runtime/'build')as tmp:
        stage=pathlib.Path(tmp);source=stage/'input'
        shutil.copytree(base/'authority/bundles',source,symlinks=True)
        target=source/(app+'.otapp')
        if a.bundle:
            if target.exists():run(a.checker,'compare',target,a.bundle,'--key',key,'--openssl',a.openssl);shutil.rmtree(target)
            shutil.copytree(a.bundle,target,symlinks=True)
        else:
            if not target.is_dir():raise ValueError('App is not installed')
            shutil.rmtree(target)
        installed=stage/'installed'
        print(run(a.checker,'compose',base/'apps/catalog.flex',source,installed,'--key',key,'--sign-key',private,'--openssl',a.openssl),flush=True)
        result=publisher.publish(base/'web',base/'native/armv7/dashboard-native',a.runtime/'releases',private,a.openssl,activate=False,catalog=installed/'catalog.flex',bundles=installed,bundle_checker=a.checker)
        after=json.loads((a.runtime/'releases'/result['id']/'manifest.json').read_bytes())
        if not activation.agent.app_install_change(before,after):raise ValueError('Installation changed qualified host bytes or protocol/policy metadata')
        pointer=a.runtime/'build/app-install.json';publisher.atomic(pointer,json.dumps(result,sort_keys=True).encode())
        activation.verify(a.runtime/'releases',key,a.openssl,pointer.read_bytes(),bundle_checker=a.checker)
        receipt={'format':'otrios-app-install-v1','action':'install' if a.bundle else 'remove','app':app,'bundle':digest,'baseline':base_id,'release':result['id'],'host_assets_unchanged':True}
        publisher.atomic(a.runtime/'build/app-install-receipt.json',json.dumps(receipt,indent=2).encode())
        if result['id']==base_id:print('UNCHANGED '+base_id);return result
        if a.deploy_config:
            config=json.loads(a.deploy_config.read_text())
            deployer.deploy(config,pointer,a.runtime/'build/frame-dashboard-server',a.runtime/'build/frame-dashboard-monitoring',a.runtime/'build/session-bootstrap',ui_base=base_id)
            publisher.atomic(baseline,pointer.read_bytes())
        print(('INSTALLED 'if a.deploy_config else'STAGED ')+result['id'],flush=True)
        return result

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime',type=pathlib.Path,required=True)
    p.add_argument('--baseline',type=pathlib.Path)
    action=p.add_mutually_exclusive_group(required=True);action.add_argument('--bundle',type=pathlib.Path);action.add_argument('--remove')
    p.add_argument('--checker',type=pathlib.Path,required=True);p.add_argument('--openssl',default='openssl')
    p.add_argument('--deploy-config',type=pathlib.Path)
    a=p.parse_args()
    if a.remove and not re.fullmatch(r'[a-z-]+',a.remove):p.error('Invalid app identity')
    lock=(a.runtime/'watcher.lock').open('a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    install(a)

if __name__=='__main__':main()
