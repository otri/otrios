#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Paired Weston/direct KMS trials. Run as root on the frame; restores services.
DRM event bytes are traced only from /dev/dri/card0, never input or network payloads.
"""
import argparse,collections,fcntl,importlib.util,json,os,pathlib,pwd,re,signal,struct,subprocess,time

def flip_events(text):
    events=[]
    for line in text.splitlines():
        match=re.search(r'read\(\d+, "((?:\\x[0-9a-f]{2})+)".*?= (\d+)',line)
        if not match:continue
        data=bytes.fromhex(match[1].replace('\\x',''));at=0
        while at+8<=len(data):
            kind,size=struct.unpack_from('<II',data,at)
            if size<8 or at+size>len(data):break
            if kind==2 and size==32:
                sec,usec,sequence,crtc=struct.unpack_from('<IIII',data,at+16)
                if usec<1_000_000:events.append((sec+usec/1e6,sequence,crtc))
            at+=size
    return events

def flip_summary(events,warmup=2.):
    if len(events)<3:raise ValueError('Insufficient DRM page-flip completion events')
    crtc=collections.Counter(e[2] for e in events).most_common(1)[0][0]
    events=[e for e in events if e[2]==crtc];begin=events[0][0]
    events=[e for e in events if e[0]>=begin+warmup]
    if len(events)<3:raise ValueError('Insufficient page flips after warm-up')
    intervals=[(b[0]-a[0])*1000 for a,b in zip(events,events[1:])]
    if min(intervals)<=0:raise ValueError('Non-monotonic flip timestamps')
    ordered=sorted(intervals);p=lambda f:ordered[round((len(ordered)-1)*f)]
    return {'measurement':'kernel DRM_EVENT_FLIP_COMPLETE timestamps from SDL KMS DRM reads','warmup_seconds':warmup,'crtc':crtc,'events':len(events),'seconds':events[-1][0]-events[0][0],'presented_fps':(len(events)-1)/(events[-1][0]-events[0][0]),'p50_ms':p(.5),'p95_ms':p(.95),'p99_ms':p(.99),'max_ms':max(intervals),'missed_vblanks':sum(max(0,((b[1]-a[1])&0xffffffff)-1)for a,b in zip(events,events[1:]))}

def run(command,log,timeout):
    with log.open('w') as out:
        process=subprocess.Popen(command,stdout=out,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            code=process.wait(timeout=timeout)
            if code:raise subprocess.CalledProcessError(code,command)
        finally:
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGTERM)
                try:process.wait(timeout=4)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()

def main():
    p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--seconds',type=int,default=20);p.add_argument('--cache',type=pathlib.Path,default=pathlib.Path('/var/lib/otrios-dashboard'));p.add_argument('--agent',type=pathlib.Path,default=pathlib.Path('/opt/otrios-dashboard/frame-dashboard-agent.py'));p.add_argument('--touch-rule',type=pathlib.Path,default=pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules'));p.add_argument('--trace-tool',type=pathlib.Path);p.add_argument('--renderer-batching',choices=['0','1']);p.add_argument('--flip-audit',type=pathlib.Path);p.add_argument('--release');p.add_argument('--direct-only',action='store_true');p.add_argument('--completion-probe',action='store_true');p.add_argument('--scenario',action='append',choices=['motion-12-idle','motion-64-load-stall','cubes-12-idle','cubes-64-idle','cubes-64-load-stall']);a=p.parse_args()
    if os.geteuid()!=0 or not 5<=a.seconds<=60:raise SystemExit('Run as root; choose 5..60 seconds per case')
    def interrupted(number,_frame):raise SystemExit(128+number)
    for number in (signal.SIGTERM,signal.SIGINT,signal.SIGHUP):signal.signal(number,interrupted)
    lock=open('/run/otrios-display-trial.lock','a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    spec=importlib.util.spec_from_file_location('frame_agent',a.agent);agent=importlib.util.module_from_spec(spec);spec.loader.exec_module(agent)
    release=a.release or json.loads((a.cache/'last-good.json').read_text())['id'];release,exe=agent.cached(a.cache,release,pathlib.Path('/opt/otrios-dashboard/publisher.pem'),'/usr/bin/openssl',sdl=True)
    matrix=agent.touch_calibration(a.touch_rule)
    a.output.mkdir(parents=True,exist_ok=True);kiosk=pwd.getpwnam('kiosk');os.chown(a.output,kiosk.pw_uid,kiosk.pw_gid);a.output.chmod(0o700)
    names=['otrios-dashboard-direct','otrios-dashboard','spd-weston','spd-browser'];active={n:subprocess.run(['systemctl','is-active','--quiet',n]).returncode==0 for n in names}
    console=None;saved_mode=None;results=[];flips=None
    try:
        subprocess.run(['systemctl','stop','otrios-dashboard-direct','otrios-dashboard','spd-browser'],check=False)
        if not a.direct_only:subprocess.run(['systemctl','start','spd-weston'],check=True)
        for driver in (['KMSDRM'] if a.direct_only else ['wayland','KMSDRM']):
            if driver=='KMSDRM':
                subprocess.run(['systemctl','stop','spd-weston','spd-browser'],check=True)
                console=os.open('/dev/tty1',os.O_RDWR);b=bytearray(4);fcntl.ioctl(console,0x4b3b,b);saved_mode=struct.unpack('i',b)[0];fcntl.ioctl(console,0x4b3a,1)
            base=['runuser','-u','kiosk','--','env','HOME=/home/kiosk','XDG_RUNTIME_DIR=/run/spd-kiosk','WAYLAND_DISPLAY=wayland-0','XDG_CACHE_HOME=/var/lib/otrios-dashboard','LD_LIBRARY_PATH=/opt/otrios-dashboard/lib','SDL_RENDER_DRIVER=opengles2','SDL_VIDEODRIVER='+driver,'SDL_KMSDRM_DEVICE_INDEX=0','SDL_KMSDRM_REQUIRE_DRM_MASTER=1','OTRIOS_RELEASE_ID='+release,str(exe),'--fullscreen']
            if driver=='KMSDRM':base+=['--rotation','90','--touch-matrix',matrix]
            if a.renderer_batching is not None:base.insert(base.index(str(exe)),'SDL_RENDER_BATCHING='+a.renderer_batching)
            if a.completion_probe:base+=['--completion-probe']
            for name,screen,count,hz,stall in [('motion-12-idle','animation',12,0,0),('motion-64-load-stall','animation',64,30,250),('cubes-12-idle','3d',12,0,0),('cubes-64-idle','3d',64,0,0),('cubes-64-load-stall','3d',64,30,250)]:
                if a.scenario and name not in a.scenario:continue
                name=driver.lower()+'-'+name;print('START '+name,flush=True);report=a.output/(name+'.json')
                command=base+['--screen',screen,'--objects',str(count),'--load-hz',str(hz),'--worker-stall-ms',str(stall),'--seconds',str(a.seconds),'--report',str(report)]
                audit=a.output/(name+'-flips.json')
                if a.flip_audit:
                    at=command.index(str(exe));command[at:at]=['LD_AUDIT='+str(a.flip_audit),'OTRIOS_FLIP_REPORT='+str(audit)]
                    audit.unlink(missing_ok=True)
                run(command,a.output/(name+'.log'),a.seconds+30);item=json.loads(report.read_text());
                if a.flip_audit:
                    feedback=json.loads(audit.read_text())
                    if len(feedback['events'])>=3:
                        item['page_flips']=flip_summary(feedback['events']);item['page_flips']['measurement']=feedback['measurement']
                    else:item['page_flips_error']='Counter did not capture kernel flip events';item['audit_diagnostic']=feedback
                item['scenario']=name;item['observed_load_hz']=item['load_updates']/item['seconds'];results.append(item);print(json.dumps(item),flush=True)
            if driver=='KMSDRM':
                if a.trace_tool:
                    raw=a.output/'drm-events.strace';report=a.output/'traced-host.json'
                    command=[str(a.trace_tool),'-f','-e','trace=read','-P','/dev/dri/card0','-s','4096','-xx','-o',str(raw)]+base+['--screen','3d','--objects','12','--seconds','12','--report',str(report)]
                    run(command,a.output/'traced-host.log',40);flips=flip_summary(flip_events(raw.read_text()));print('PAGE_FLIPS '+json.dumps(flips),flush=True)
                run(base+['--screen','3d','--objects','12','--frames','110','--seconds','8','--capture',str(a.output/'direct-physical.ppm'),'--capture-after','80'],a.output/'capture.log',30)
    finally:
        if console is not None:
            if saved_mode is not None:fcntl.ioctl(console,0x4b3a,saved_mode)
            os.close(console)
        subprocess.run(['systemctl','stop','spd-weston','spd-browser'],check=False)
        for n in ['spd-weston','otrios-dashboard','spd-browser','otrios-dashboard-direct']:
            if active[n]:subprocess.run(['systemctl','start',n],check=True)
    data={'completion_probe':a.completion_probe,'release':release,'measured_at_unix':time.time(),'rotation_direct':90,'physical_extent':[800,1280],'logical_extent':[1280,800],'touch_rule_unchanged':True,'scenarios':results,'page_flips':flips}
    (a.output/'results.json').write_text(json.dumps(data,indent=2));print('COMPLETE',flush=True)
if __name__=='__main__':main()
