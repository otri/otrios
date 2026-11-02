#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Carry accepted app imports to the existing Mac signer through pinned SSH.

The authority owns authorization, the durable decision and attachment recovery.
This adapter keeps signing on the Mac and deploys only complete verified releases.
The caller must hold the existing watcher.lock across service().
"""
import argparse,fcntl,importlib.util,json,pathlib,re,shlex,subprocess,tempfile
from types import SimpleNamespace

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,pathlib.Path(__file__).with_name(file))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
installer=module('app_import_installer','install-app.py')
activation=installer.activation
publisher=installer.publisher
REMOTE=r'''
import json,sys,urllib.request
args=json.loads(sys.stdin.buffer.read(2049))
limit=args['limit']
if not 0<limit<=17*1024*1024:raise ValueError('Bridge response budget')
headers={'X-OTriOS-Request':'signer'}
data=args.get('data')
if data is not None:headers['Content-Type']='application/json';data=json.dumps(data).encode()
request=urllib.request.Request('http://127.0.0.1:8876'+args['path'],data=data,headers=headers,method=args['method'])
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
with opener.open(request,timeout=30)as response:raw=response.read(limit+1)
if len(raw)>limit:raise ValueError('Bridge response budget')
sys.stdout.buffer.write(raw)
'''
class Bridge:
    def __init__(self,config):
        self.ssh=config['frame']['ssh']
        if not isinstance(self.ssh,list) or not self.ssh or self.ssh[0]!='ssh':raise ValueError('Configure the existing pinned SSH argument list')
    def request(self,path,data=None,raw=False):
        if not re.fullmatch(r'/admin/app-import(?:/[a-f0-9]{32}(?:/(?:payload|reject))?)?',path):raise ValueError('Invalid bridge path')
        limit=17*1024*1024 if raw else 16384
        args={'path':path,'data':data,'method':'POST' if data is not None else 'GET','limit':limit}
        output=subprocess.check_output(self.ssh+['python3 -c '+shlex.quote(REMOTE)],input=json.dumps(args).encode(),timeout=45)
        if len(output)>limit:raise ValueError('Bridge response budget')
        return output if raw else json.loads(output)

def service(config,bridge=None,deploy=None):
    """One bounded decision pass. Re-read outcomes before resuming armed work."""
    if not config.get('frame'):return False
    bridge=bridge or Bridge(config)
    runtime=pathlib.Path(config['runtime']);build=runtime/'build'
    receipt_file=build/'app-import-bridge.json'
    receipt=json.loads(activation.read(receipt_file,32768)) if receipt_file.exists() else None
    def save(value):publisher.atomic(receipt_file,json.dumps(value,sort_keys=True).encode())
    def reconcile(value,state):
        if not state or state['operation']!=value['operation']:raise ValueError('Import outcome unknown; retain the prepared release for operator recovery')
        phase=state['phase']
        if phase=='applied':
            if state['release']!=value['result']['id']:raise ValueError('Applied import differs from the prepared release')
            raw=json.dumps(value['result'],sort_keys=True).encode()
            activation.verify(runtime/'releases',runtime/'publisher/public.pem',config['openssl'],raw,bundle_checker=build/'mac-target/debug/canvas-bundle')
            baseline=runtime/'releases/current.json'
            current=json.loads(baseline.read_bytes())['id']
            if current in (value['review']['release'],state['release']):publisher.atomic(baseline,raw)
        if phase in ('applied','rejected'):
            value['terminal']=phase;save(value)
        return phase
    if receipt and not receipt.get('terminal'):
        reconcile(receipt,bridge.request('/admin/app-import/'+receipt['operation']))
    job=bridge.request('/admin/app-import')
    if not job:return False
    review=job['review'];operation=review['stage']
    if not re.fullmatch(r'[a-f0-9]{32}',operation) or job['state']['operation']!=operation:raise ValueError('Invalid import job')
    if not receipt or receipt['operation']!=operation or receipt.get('terminal'):
        if job['state']['phase']!='queued':raise ValueError('Armed import has no local prepared-release receipt; operator recovery is required')
        payload_bytes=bridge.request('/admin/app-import/'+operation+'/payload',raw=True)
        try:
            with tempfile.TemporaryDirectory(prefix='app-import-',dir=build)as tmp:
                payload=pathlib.Path(tmp)/'candidate.otapp.zip'
                payload.write_bytes(payload_bytes)
                result=installer.install(SimpleNamespace(runtime=runtime,baseline=None,payload=payload,bundle=None,remove=None,
                    expected_release=review['release'],expected_bundle=review['installed'] or 'absent',expected_candidate=review['digest'],expected_app=review['app'],
                    checker=build/'mac-target/debug/canvas-bundle',openssl=config['openssl'],deploy_config=None))
            receipt={'format':'otrios-app-import-bridge-v1','operation':operation,'review':review,'result':result}
            save(receipt) # durable before arming or starting a remote pointer switch
        except (ValueError,subprocess.CalledProcessError):
            state=bridge.request('/admin/app-import/'+operation)
            if state and state['phase']=='queued':bridge.request('/admin/app-import/'+operation+'/reject',{})
            raise
    result=receipt['result'];raw=json.dumps(result,sort_keys=True).encode()
    activation.verify(runtime/'releases',runtime/'publisher/public.pem',config['openssl'],raw,bundle_checker=build/'mac-target/debug/canvas-bundle')
    pointer=build/'app-import-activation.json';publisher.atomic(pointer,raw)
    # The remote deployer verifies/flushes the complete immutable candidate, then
    # revalidates and arms under .activation.lock immediately before the switch.
    (deploy or installer.deployer.deploy)(config,pointer,build/'frame-dashboard-server',build/'frame-dashboard-monitoring',build/'session-bootstrap',ui_base=review['release'],app_import=operation)
    phase=reconcile(receipt,bridge.request('/admin/app-import/'+operation))
    print('APP IMPORT '+operation+' '+phase,flush=True)
    return True

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--config',type=pathlib.Path,required=True);a=p.parse_args()
    config=json.loads(a.config.read_bytes());runtime=pathlib.Path(config['runtime'])
    with (runtime/'watcher.lock').open('a')as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB);service(config)
if __name__=='__main__':main()
