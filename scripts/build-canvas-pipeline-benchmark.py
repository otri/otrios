#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
import argparse,pathlib,subprocess,tarfile,hashlib,json,os
parser=argparse.ArgumentParser(description='Build frozen ARMv7 frame probes and matching Mac authority using the configured Rust environment.')
parser.add_argument('--output',type=pathlib.Path,required=True);parser.add_argument('--mac-target',type=pathlib.Path,required=True);parser.add_argument('--container',required=True)
a=parser.parse_args();repo=pathlib.Path(__file__).resolve().parent.parent;out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
container=a.container;archive=out/'source.tar'
revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
status=subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True)
with tarfile.open(archive,'w') as tar:
 for item in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','crates','apps','tools','web/shared/fonts']:tar.add(repo/item,arcname=item)
subprocess.run(['docker','cp',str(archive),container+':/tmp/canvas-pipeline-source.tar'],check=True)
subprocess.run(['docker','exec',container,'rm','-rf','/src/crates','/src/apps','/src/tools'],check=True)
subprocess.run(['docker','exec',container,'tar','-xf','/tmp/canvas-pipeline-source.tar','-C','/src'],check=True)
subprocess.run(['docker','exec',container,'sh','-c','find /src/crates /src/apps /src/tools -type f -exec touch {} +'],check=True)
subprocess.run(['docker','exec','-e','CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc',container,'cargo','build','--release','--locked','-p','otrios-dashboard','--features','direct,canvas','--bin','dashboard-direct','--bin','canvas-adapter-bench','--target','armv7-unknown-linux-gnueabihf'],check=True)
for binary in ['dashboard-direct','canvas-adapter-bench']:
 subprocess.run(['docker','cp',container+':/src/target/armv7-unknown-linux-gnueabihf/release/'+binary,str(out/binary)],check=True)
subprocess.run(['docker','exec',container,'cargo','build','--release','--locked','-p','otrios-core','--features','canvas','--example','merkle_tree_cost','--target','armv7-unknown-linux-gnueabihf'],check=True)
subprocess.run(['docker','cp',container+':/src/target/armv7-unknown-linux-gnueabihf/release/examples/merkle_tree_cost',str(out/'merkle_tree_cost')],check=True)
frozen=out/'source';frozen.mkdir()
with tarfile.open(archive) as tar:tar.extractall(frozen,filter='data')
env=dict(os.environ,CARGO_TARGET_DIR=str(a.mac_target.resolve()))
subprocess.run(['cargo','build','--release','--locked','--offline','-p','otrios-dashboard','--features','server,canvas,host-sync','--bin','dashboard-server'],cwd=frozen,env=env,check=True)
import shutil
shutil.copyfile(a.mac_target/'release/dashboard-server',out/'dashboard-server');(out/'dashboard-server').chmod(0o755)
meta={'commit':revision,'source_status':status,'source_archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'binaries':{n:hashlib.sha256((out/n).read_bytes()).hexdigest() for n in ['dashboard-direct','canvas-adapter-bench','merkle_tree_cost','dashboard-server']}}
(out/'build.json').write_text(json.dumps(meta,indent=2)+'\n');print(json.dumps(meta),flush=True)
