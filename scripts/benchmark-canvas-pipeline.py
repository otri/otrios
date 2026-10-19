#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Fresh-store headless comparison on the RK3126, with guarded temporary routes."""
import argparse, asyncio, hashlib, json, os, pathlib, shlex, signal, subprocess, tarfile, time, urllib.request
p=argparse.ArgumentParser()
for name in ['reference','candidate','output']:p.add_argument('--'+name,type=pathlib.Path,required=True)
p.add_argument('--frame',required=True);p.add_argument('--known-hosts',type=pathlib.Path,required=True);p.add_argument('--release',type=pathlib.Path,required=True);p.add_argument('--tailscale',default='tailscale')
p.add_argument('--legacy-reference',action='store_true');p.add_argument('--rates',default='10,30');p.add_argument('--updates',type=int,default=180);p.add_argument('--repetitions',type=int,default=3)
a=p.parse_args();repo=pathlib.Path(__file__).resolve().parent.parent;out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
if (out/'results.jsonl').exists():raise RuntimeError('Choose a fresh output directory')
tailscale=a.tailscale;frame=a.frame;known=str(a.known_hosts.resolve())
ssh=['ssh','-oBatchMode=yes','-oConnectTimeout=8','-oStrictHostKeyChecking=yes','-oUserKnownHostsFile='+known,frame]
release=a.release.resolve()
remote='/var/lib/otrios-dashboard/bench-pipeline-'+str(os.getpid());routes=[];relay=None;server=None
before=json.loads(subprocess.check_output([tailscale,'serve','status','--json']))
for port in [18901,18902]:
 if str(port) in before.get('TCP',{}):raise RuntimeError('Benchmark route already exists')
upstream=subprocess.check_output([tailscale,'ip','-4'],text=True).strip()
# This runner reports child wait4 CPU/RSS, not the relay/SSH or render service.
remote_runner='''import json,os,pathlib,subprocess,sys,time
root=pathlib.Path(sys.argv[1]);label=sys.argv[2];hz=sys.argv[3];updates=sys.argv[4];legacy=sys.argv[5]=='yes'
port=18901 if label=='reference' else 18902
env=dict(os.environ)
if legacy:env['OTRIOS_BENCH_LEGACY_TRANSPORT']='1'
args=[str(root/label/'canvas-adapter-bench'),f'ws://127.0.0.1:{port}/apps/stream',str(root/'app-catalog.flex'),updates,hz,'20','paired']
start=time.monotonic();child=subprocess.Popen(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
stdout,stderr=child.stdout.read(),child.stderr.read();_,status,usage=os.wait4(child.pid,0);elapsed=time.monotonic()-start
child.returncode=os.waitstatus_to_exitcode(status)
if child.returncode:raise RuntimeError(stderr.decode())
r=json.loads(stdout);r.update(process_seconds=elapsed,cpu_user_seconds=usage.ru_utime,cpu_system_seconds=usage.ru_stime,cpu_one_core_percent=100*(usage.ru_utime+usage.ru_stime)/elapsed,max_rss_kib=usage.ru_maxrss)
print(json.dumps(r),flush=True)
'''
runner=out/'run-frame.py';runner.write_text(remote_runner)
archive=out/'frame-input.tar'
with tarfile.open(archive,'w') as t:
 for label,folder in [('reference',a.reference),('candidate',a.candidate)]:t.add(folder/'canvas-adapter-bench',arcname=label+'/canvas-adapter-bench')
 t.add(release/'apps/catalog.flex',arcname='app-catalog.flex');t.add(repo/'scripts/relay-canvas-frame-benchmark.py',arcname='relay.py');t.add(runner,arcname='run-frame.py')
metadata={'reference':json.loads((a.reference/'build.json').read_text()),'candidate':json.loads((a.candidate/'build.json').read_text()),'legacy_reference':a.legacy_reference,'rates_hz':a.rates,'updates':a.updates,'warmup':20,'repetitions':a.repetitions,'catalog_sha256':hashlib.sha256((release/'apps/catalog.flex').read_bytes()).hexdigest(),'frame':'RK3126 Cortex-A7 Linux 6.18.57-spd-kiosk','method':'two real clients on frame; fresh durable authority per trial; native live renderer left running; paired stop-and-wait; private Tailscale raw TCP; CPU includes mount/warm-up/waits'}
(out/'metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
try:
 with archive.open('rb') as data:subprocess.run(ssh+['mkdir -m 700 '+shlex.quote(remote)+' && tar -xf - -C '+shlex.quote(remote)],stdin=data,check=True)
 for port,target in [(18901,8901),(18902,8902)]:
  subprocess.run([tailscale,'serve','--bg','--tcp='+str(port),'tcp://127.0.0.1:'+str(target)],check=True,stdout=subprocess.DEVNULL);routes.append(port)
 relay=subprocess.Popen(ssh+['exec python3 '+shlex.quote(remote+'/relay.py')+' --upstream '+shlex.quote(upstream)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 if 'ready' not in relay.stdout.readline().lower():raise RuntimeError('Frame relay failed')
 with (out/'results.jsonl').open('w') as output:
  for rep in range(a.repetitions):
   for hz in map(int,a.rates.split(',')):
    order=['reference','candidate'] if rep%2==0 else ['candidate','reference']
    for label in order:
     folder=(a.reference if label=='reference' else a.candidate).resolve();port=8901 if label=='reference' else 8902
     base=out/f'case-{rep}-{hz}-{label}';base.mkdir();releases=base/'releases';releases.mkdir();(releases/release.name).symlink_to(release,target_is_directory=True);(releases/'current.json').write_text(json.dumps({'id':release.name,'web_entry':f'/releases/{release.name}/web/index.html'}))
     with (base/'server.log').open('w') as log:
      server=subprocess.Popen([str(folder/'dashboard-server'),'--bind',f'127.0.0.1:{port}','--app-state',str(base/'state'),'--releases',str(releases)],stdout=log,stderr=subprocess.STDOUT)
      for attempt in range(100):
       if server.poll() is not None:raise RuntimeError('Disposable authority exited; inspect '+str(base))
       try:
        with urllib.request.urlopen(f'http://127.0.0.1:{port}/health',timeout=.5) as r:
         if r.status==200:break
       except OSError:pass
       time.sleep(.1)
      else:raise RuntimeError('Authority failed readiness')
      time.sleep(1)
      args=['python3',remote+'/run-frame.py',remote,label,str(hz),str(a.updates),'yes' if label=='reference' and a.legacy_reference else 'no']
      r=json.loads(subprocess.check_output(ssh+[' '.join(shlex.quote(x) for x in args)],timeout=150,text=True));r.update(label=label,repetition=rep,fresh_store=True,legacy_transport=label=='reference' and a.legacy_reference)
      output.write(json.dumps(r)+'\n');output.flush();print(json.dumps({'completed':label,'repetition':rep,'hz':hz}),flush=True)
      server.terminate();server.wait(timeout=10);server=None
finally:
 if server is not None:server.terminate();server.wait(timeout=10)
 if relay is not None:
  relay.terminate()
  try:relay.wait(timeout=10)
  except subprocess.TimeoutExpired:relay.kill();relay.wait()
 # Tailscale SSH can briefly retain its session slot after a probe exits.
 cleanup=ssh+['pkill -f '+shlex.quote('^python3 '+remote+'/relay.py')+' || true; rm -rf -- '+shlex.quote(remote)]
 cleaned=False
 for attempt in range(5):
  time.sleep(1)
  if subprocess.run(cleanup,stdout=subprocess.DEVNULL).returncode==0:
   cleaned=True;break
 for port in routes:subprocess.run([tailscale,'serve','--tcp='+str(port),'off'],check=True,stdout=subprocess.DEVNULL)
 after=json.loads(subprocess.check_output([tailscale,'serve','status','--json']))
 if before!=after:raise RuntimeError('Serve configuration differs after cleanup')
 if not cleaned:raise RuntimeError('Frame experiment cleanup failed: '+remote)
 print('Restored original serve configuration; removed frame experiment files',flush=True)
