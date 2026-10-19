#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Isolated direct DRM renderer trial; always restores the previous display service.
The explicitly hashed local executable is a development build, not a signed release.
"""
import argparse,fcntl,hashlib,importlib.util,json,os,pathlib,pwd,signal,struct,subprocess,time

def main():
    p=argparse.ArgumentParser();p.add_argument('--executable',type=pathlib.Path,required=True);p.add_argument('--sha256',required=True);p.add_argument('--apps',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--seconds',type=int,default=20);p.add_argument('--format',action='append',choices=['xrgb8888','rgb565']);p.add_argument('--quick',action='store_true');a=p.parse_args()
    if os.geteuid()!=0 or not 5<=a.seconds<=60:raise SystemExit('Root and 5..60 seconds required')
    if hashlib.sha256(a.executable.read_bytes()).hexdigest()!=a.sha256:raise SystemExit('Development executable digest mismatch')
    a.apps=a.apps.resolve()
    if not (a.apps/'catalog.flex').is_file():raise SystemExit('Explicit admitted app catalog required')
    for sig in [signal.SIGINT,signal.SIGTERM,signal.SIGHUP]:signal.signal(sig,lambda n,_: (_ for _ in ()).throw(SystemExit(128+n)))
    lock=open('/run/otrios-display-trial.lock','a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    spec=importlib.util.spec_from_file_location('agent','/opt/otrios-dashboard/frame-dashboard-agent.py');agent=importlib.util.module_from_spec(spec);spec.loader.exec_module(agent)
    matrix=agent.touch_calibration(pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules'))
    names=[n for n in ['otrios-dashboard-direct','otrios-dashboard','spd-weston','spd-browser'] if subprocess.run(['systemctl','cat',n],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0];active={n:subprocess.run(['systemctl','is-active','--quiet',n]).returncode==0 for n in names}
    kiosk=pwd.getpwnam('kiosk');a.output.mkdir(parents=True,exist_ok=True);os.chown(a.output,kiosk.pw_uid,kiosk.pw_gid);a.output.chmod(0o700)
    console=None;saved=None;results=[]
    def run(command,name,timeout):
        with (a.output/(name+'.log')).open('w')as log:
            child=subprocess.Popen(command,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                code=child.wait(timeout=timeout)
                if code:raise RuntimeError((a.output/(name+'.log')).read_text())
            finally:
                if child.poll() is None:
                    os.killpg(child.pid,signal.SIGTERM)
                    try:child.wait(timeout=5)
                    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
    try:
        subprocess.run(['systemctl','stop',*names],check=True)
        console=os.open('/dev/tty1',os.O_RDWR);buf=bytearray(4);fcntl.ioctl(console,0x4b3b,buf);saved=struct.unpack('i',buf)[0];fcntl.ioctl(console,0x4b3a,1)
        for fmt in a.format or ['xrgb8888','rgb565']:
            base=['runuser','-u','kiosk','--','env','HOME=/home/kiosk','OTRIOS_SCANOUT_FORMAT='+fmt,str(a.executable),'--apps',str(a.apps),'--rotation','90','--touch-matrix',matrix]
            cases=[('tetrahedra-12-idle','3d',12,0,0),('tetrahedra-64-load-stall','3d',64,30,250)] if a.quick else [('motion-12-idle','animation',12,0,0),('motion-64-load-stall','animation',64,30,250),('tetrahedra-12-idle','3d',12,0,0),('tetrahedra-64-idle','3d',64,0,0),('tetrahedra-64-load-stall','3d',64,30,250)]
            for scenario,screen,count,hz,stall in cases:
                name=fmt+'-'+scenario;report=a.output/(name+'.json');print('START '+name,flush=True)
                run(base+['--screen',screen,'--objects',str(count),'--load-hz',str(hz),'--worker-stall-ms',str(stall),'--seconds',str(a.seconds),'--report',str(report)],name,a.seconds+30)
                item=json.loads(report.read_text());item['scenario']=scenario;results.append(item);print(json.dumps(item),flush=True)
            run(base+['--screen','3d','--objects','12','--frames','90','--seconds','10','--capture',str(a.output/(fmt+'.ppm')),'--capture-after','80'],fmt+'-capture',30)
    finally:
        if console is not None:
            if saved is not None:fcntl.ioctl(console,0x4b3a,saved)
            os.close(console)
        subprocess.run(['systemctl','stop','spd-weston','spd-browser'],check=False)
        for n in ['spd-weston','otrios-dashboard','spd-browser','otrios-dashboard-direct']:
            if active.get(n,False):subprocess.run(['systemctl','start',n],check=True)
        (a.output/'results.json').write_text(json.dumps({'development_binary_sha256':a.sha256,'measured_at_unix':time.time(),'scenarios':results},indent=2))
    print('COMPLETE; previous services restored',flush=True)
if __name__=='__main__':main()
