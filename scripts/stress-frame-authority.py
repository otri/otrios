#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Ramp real sessions and Component Playground writes on a disposable frame authority."""
import argparse, hashlib, io, json, os, pathlib, resource, shlex, shutil, subprocess, tarfile, threading, time, uuid

SAMPLER = r'''
import json,pathlib,sys,time,urllib.request,os
pid=int(sys.argv[1]);stop=pathlib.Path(sys.argv[2]);cg=pathlib.Path(sys.argv[3]);hertz=os.sysconf('SC_CLK_TCK')
previous=None;started=time.monotonic();proxy=pathlib.Path(sys.argv[4])
while not stop.exists() and time.monotonic()-started<240:
 row={'unix_ms':int(time.time()*1000),'elapsed':time.monotonic()-started,'server_alive':pathlib.Path(f'/proc/{pid}').exists()}
 try:
  stat=pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(') ',1)[1].split()
  row.update(server_ticks=int(stat[11])+int(stat[12]),server_rss_kib=int(stat[21])*os.sysconf('SC_PAGE_SIZE')//1024)
 except OSError:pass
 try:
  relay=int((proxy/'cgroup.procs').read_text().split()[0]);stat=pathlib.Path(f'/proc/{relay}/stat').read_text().rsplit(') ',1)[1].split();row.update(proxy_ticks=int(stat[11])+int(stat[12]),proxy_rss_kib=int(stat[21])*os.sysconf('SC_PAGE_SIZE')//1024)
 except (OSError,IndexError):pass
 try:row['proxy_memory_bytes']=int((proxy/'memory.current').read_text())
 except OSError:pass
 try:row['io.stat']=(cg/'io.stat').read_text().strip()
 except OSError:pass
 cpu=list(map(int,pathlib.Path('/proc/stat').read_text().splitlines()[0].split()[1:9]));row.update(total_ticks=sum(cpu),idle_ticks=cpu[3]+cpu[4])
 mem={l.split(':')[0]:int(l.split()[1]) for l in pathlib.Path('/proc/meminfo').read_text().splitlines()}
 row['mem_available_kib']=mem['MemAvailable'];row['swap_used_kib']=mem['SwapTotal']-mem['SwapFree']
 for name in ['memory.current','memory.peak','memory.swap.current','memory.events']:
  try:
   raw=(cg/name).read_text();row[name]=int(raw) if name!='memory.events' else dict((k,int(v)) for k,v in (l.split() for l in raw.splitlines()))
  except OSError:pass
 if previous:
  dt=row['elapsed']-previous['elapsed'];row['server_cpu_one_core_percent']=100*(row.get('server_ticks',previous.get('server_ticks',0))-previous.get('server_ticks',0))/hertz/dt
  row['proxy_cpu_one_core_percent']=100*(row.get('proxy_ticks',previous.get('proxy_ticks',0))-previous.get('proxy_ticks',0))/hertz/dt
  total=row['total_ticks']-previous['total_ticks'];row['system_cpu_percent']=100*(1-(row['idle_ticks']-previous['idle_ticks'])/max(1,total))
 if int(row['elapsed'])%5==0:
  at=time.monotonic()
  try:
   with urllib.request.urlopen('http://127.0.0.1:8876/health',timeout=2) as r:row['production_healthy']=json.load(r)['status']=='ok'
  except Exception:row['production_healthy']=False
  row['production_health_ms']=(time.monotonic()-at)*1000
 print(json.dumps(row),flush=True);previous=row
 if row['mem_available_kib']<256*1024:break
 time.sleep(1)
'''

def run(a):
    config=json.loads(a.config.read_text());ssh=config['frame']['ssh']
    if not isinstance(ssh,list) or ssh[0]!='ssh':raise ValueError('Use the existing pinned SSH configuration')
    soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE)
    resource.setrlimit(resource.RLIMIT_NOFILE,(min(8192,hard) if hard!=resource.RLIM_INFINITY else 8192,hard))
    out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
    remote='/var/lib/otrios-authority/.stress-'+uuid.uuid4().hex[:12]
    def command(code,**kwargs):
        for attempt in range(4):
            result=subprocess.run(ssh+['python3 -c '+shlex.quote(code)],**kwargs)
            if result.returncode!=255 or attempt==3:result.check_returncode();return result
            time.sleep(1)
    def remote_json(code):return json.loads(command(code,capture_output=True,text=True).stdout)
    metadata={'runner_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'server_sha256':hashlib.sha256(a.server.read_bytes()).hexdigest(),'client_sha256':hashlib.sha256(a.client.read_bytes()).hexdigest(),'transport':'Tailscale TCP via temporary systemd-socket-proxyd public-listener relay; includes relay/network cost','sampler_interval_seconds':1,'target_memory_max_mib':256,'relay_memory_max_mib':256,'socket_limit':4096,'offered_hz_per_active':a.hz,'seconds_per_case':a.seconds,'open_loop':a.open_loop,'active_cycle':'Linear, Rotary and Touch Pad normalized app events; drag mode issues on the input clock and releases once at the end, mounted mode waits for durable confirmation between cycles','captured_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'production_state':'not used by disposable authority; live renderer and production services remain running'}
    (out/'metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
    payload=io.BytesIO()
    with tarfile.open(fileobj=payload,mode='w') as t:
        t.add(a.server,arcname='server')
        raw=SAMPLER.encode();entry=tarfile.TarInfo('sampler.py');entry.size=len(raw);t.addfile(entry,io.BytesIO(raw))
    install=r'''import sys,pathlib,tarfile,shutil
root=pathlib.Path(REMOTE);root.mkdir(mode=0o750)
with tarfile.open(fileobj=sys.stdin.buffer,mode='r|') as t:
 for m in t:
  if m.name not in ('server','sampler.py') or not m.isfile():raise ValueError('Unexpected test input')
  with t.extractfile(m) as src,(root/m.name).open('wb') as dst:shutil.copyfileobj(src,dst)
(root/'server').chmod(0o755)
shutil.chown(root,user='otrios',group='otrios')
'''.replace('REMOTE',repr(remote))
    command(install,input=payload.getvalue())
    unit=None;proxy='otrios-stress-proxy-'+uuid.uuid4().hex[:10]
    cases=[]
    if a.phase in ('all','sockets'):
        cases.extend((n,0,0,'socket') for n in map(int,a.socket_counts.split(',')) if n<=a.max_sockets)
    if a.phase in ('all','mounted'):
        cases.extend((n,0,0,'mounted') for n in map(int,a.mounted_counts.split(',')) if n<=a.max_mounted)
    if a.phase in ('all','active'):
        cases.extend((a.clients,n,a.hz,'drag' if a.open_loop else 'mounted') for n in map(int,a.active_counts.split(',')) if n<=min(a.clients,a.max_active))
    proxy_setup=r'''import subprocess,pathlib,json
ip=subprocess.check_output(['tailscale','ip','-4'],text=True).strip()
base=pathlib.Path('/run/systemd/system')
(base/(PROXY+'.socket')).write_text('[Unit]\nDescription=Disposable OTriOS stress socket\n[Socket]\nListenStream='+ip+':18878\nBindToDevice=tailscale0\nNoDelay=true\n')
(base/(PROXY+'.service')).write_text('[Service]\nUser=otrios\nExecStart=/usr/lib/systemd/systemd-socket-proxyd --connections-max=4096 127.0.0.1:18877\nLimitNOFILE=65536\nMemoryMax=256M\nCPUWeight=50\nNoNewPrivileges=true\n')
subprocess.run(['systemctl','daemon-reload'],check=True)
subprocess.run(['systemctl','start',PROXY+'.socket'],check=True)
print(json.dumps({'ip':ip}))
'''.replace('PROXY',repr(proxy))
    try:
        address=remote_json(proxy_setup)['ip']
        for index,(clients,active,hz,mode) in enumerate(cases):
            name=f'{index:02}-{mode}-{clients}-{active}-{hz}';unit='otrios-stress-'+uuid.uuid4().hex[:10]
            setup=r'''import pathlib,subprocess,json,time,urllib.request,shutil
root=pathlib.Path(REMOTE);state=root/NAME;state.mkdir();shutil.chown(state,user='otrios',group='otrios')
args=['systemd-run','--quiet','--unit='+UNIT,'--uid=otrios','--property=MemoryMax=256M','--property=MemoryHigh=192M','--property=CPUWeight=50','--property=TasksMax=64','--property=LimitNOFILE=16384','--property=NoNewPrivileges=yes',str(root/'server'),'--bind','127.0.0.1:18876','--public-bind','127.0.0.1:18877','--trusted-peers','4','--public-peers','4096','--app-state',str(state/'state.apps'),'--releases','/var/lib/otrios-authority/releases','--origin','https://otrios.otri.ca']
subprocess.run(args,check=True)
for i in range(100):
 try:
  with urllib.request.urlopen('http://127.0.0.1:18876/health',timeout=1) as r:health=json.load(r)
  if health['status']=='ok':break
 except Exception:pass
 time.sleep(.1)
else:raise RuntimeError('Stress authority failed startup')
pid=int(subprocess.check_output(['systemctl','show',UNIT,'-p','MainPID','--value'],text=True));cg=subprocess.check_output(['systemctl','show',UNIT,'-p','ControlGroup','--value'],text=True).strip()
print(json.dumps({'pid':pid,'cgroup':'/sys/fs/cgroup'+cg,'release':health['release']}))
'''.replace('REMOTE',repr(remote)).replace('NAME',repr(name)).replace('UNIT',repr(unit))
            info=remote_json(setup);samples=[];guard=[]
            stop=remote+'/'+name+'/stop-sampler'
            monitor=subprocess.Popen(ssh+['python3 '+shlex.quote(remote+'/sampler.py')+' '+str(info['pid'])+' '+shlex.quote(stop)+' '+shlex.quote(info['cgroup'])+' '+shlex.quote('/sys/fs/cgroup/system.slice/'+proxy+'.service')],stdout=subprocess.PIPE,stderr=(out/(name+'-sampler.log')).open('w'),text=True)
            def consume():
                with (out/(name+'-samples.jsonl')).open('w') as f:
                    for line in monitor.stdout:
                        row=json.loads(line);samples.append(row);f.write(line);f.flush()
                        if row['mem_available_kib']<256*1024:guard.append('global memory headroom below 256 MiB')
                        if row.get('production_healthy') is False:guard.append('production health probe failed')
            consumer=threading.Thread(target=consume);consumer.start()
            args=[str(a.client),f'ws://{address}:18878/session',str(a.catalog),str(clients),str(active),str(hz),str(a.seconds),mode]
            start=time.monotonic()
            with (out/(name+'-client.json')).open('w') as stdout,(out/(name+'-client.log')).open('w') as stderr:
                client=subprocess.Popen(args,stdout=stdout,stderr=stderr)
                while client.poll() is None:
                    if guard or time.monotonic()-start>210:
                        client.terminate();break
                    time.sleep(.2)
                try:client.wait(timeout=5)
                except subprocess.TimeoutExpired:client.kill();client.wait()
            command('import pathlib;pathlib.Path('+repr(stop)+').touch()',capture_output=True)
            monitor.wait(timeout=10);consumer.join(timeout=5)
            if monitor.returncode or not samples:raise RuntimeError('Sampler failed; refusing an unmeasured result')
            status=remote_json("import subprocess,json;print(json.dumps({k:subprocess.run(['systemctl','show',"+repr(unit)+",'-p',k,'--value'],capture_output=True,text=True).stdout.strip() for k in ['Result','MemoryPeak','CPUUsageNSec','ExecMainStatus']}))")
            try:result=json.loads((out/(name+'-client.json')).read_text())
            except (ValueError,OSError):result={'client_exit':client.returncode,'client_error':(out/(name+'-client.log')).read_text()[-2000:]}
            result.update(case=name,target=info,status=status,guard=guard,wall_seconds=time.monotonic()-start)
            steady=[r for r in samples if result.get('measure_start_unix_ms',float('inf'))<=r['unix_ms']<=result.get('measure_end_unix_ms',0)]
            result['steady_summary']={'samples':len(steady),'mean_server_cpu_one_core_percent':sum(r.get('server_cpu_one_core_percent',0) for r in steady)/max(1,len(steady)),'mean_proxy_cpu_one_core_percent':sum(r.get('proxy_cpu_one_core_percent',0) for r in steady)/max(1,len(steady)),'peak_proxy_rss_kib':max((r.get('proxy_rss_kib',0) for r in samples),default=0),'peak_proxy_cgroup_bytes':max((r.get('proxy_memory_bytes',0) for r in samples),default=0)}
            result['sample_summary']={'peak_rss_kib':max((r.get('server_rss_kib',0) for r in samples),default=0),'peak_cgroup_bytes':max((r.get('memory.peak',0) for r in samples),default=0),'minimum_available_kib':min((r['mem_available_kib'] for r in samples),default=0),'mean_server_cpu_one_core_percent':sum(r.get('server_cpu_one_core_percent',0) for r in samples)/max(1,len(samples)-1),'peak_system_cpu_percent':max((r.get('system_cpu_percent',0) for r in samples),default=0),'production_health_failures':sum(r.get('production_healthy') is False for r in samples),'peak_swap_used_kib':max((r.get('swap_used_kib',0) for r in samples),default=0),'peak_server_swap_bytes':max((r.get('memory.swap.current',0) for r in samples),default=0)}
            (out/(name+'-result.json')).write_text(json.dumps(result,indent=2)+'\n')
            command("import subprocess;subprocess.run(['systemctl','stop',"+repr(unit)+"],check=True);subprocess.run(['systemctl','reset-failed',"+repr(unit)+"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)",capture_output=True);unit=None
            print(json.dumps({'completed':name,'connected':result.get('connected'),'cycles':result.get('confirmed_cycles'),'ack_us':result.get('ack_us'),'summary':result['sample_summary'],'errors':result.get('errors',result.get('client_error')),'guard':guard}),flush=True)
            if guard:break
    finally:
        if unit:command("import subprocess;subprocess.run(['systemctl','stop',"+repr(unit)+"],check=False)",capture_output=True)
        command("import pathlib,subprocess;subprocess.run(['systemctl','stop',"+repr(proxy+'.socket')+","+repr(proxy+'.service')+"],check=False);[(pathlib.Path('/run/systemd/system')/name).unlink(missing_ok=True) for name in "+repr([proxy+'.socket',proxy+'.service'])+"];subprocess.run(['systemctl','daemon-reload'],check=True)",capture_output=True)
        command('import shutil;shutil.rmtree('+repr(remote)+')',capture_output=True)
        print('Removed disposable authority and Tailscale relay; production state was not used',flush=True)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['config','client','server','catalog','output']:p.add_argument('--'+name,type=pathlib.Path,required=True)
    p.add_argument('--phase',choices=['all','sockets','mounted','active'],default='all');p.add_argument('--seconds',type=int,default=15);p.add_argument('--hz',type=int,default=10);p.add_argument('--clients',type=int,default=64);p.add_argument('--max-active',type=int,default=64);p.add_argument('--max-sockets',type=int,default=4096);p.add_argument('--max-mounted',type=int,default=512)
    p.add_argument('--open-loop',action='store_true');p.add_argument('--socket-counts',default='64,256,512,1024,2048,4096');p.add_argument('--mounted-counts',default='32,64,128,256,512');p.add_argument('--active-counts',default='1,2,4,8,16,32,64,128,256')
    a=p.parse_args()
    if not 1<=a.seconds<=120 or not 1<=a.hz<=120:p.error('Use 1..120 seconds and 1..120 Hz')
    try:
        counts=[int(n) for value in [a.socket_counts,a.mounted_counts,a.active_counts] for n in value.split(',')]
        if any(n<1 or n>4096 for n in counts):raise ValueError()
    except ValueError:p.error('Client counts must be comma-separated integers from 1 to 4096')
    run(a)
