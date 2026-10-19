#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run isolated native animation scenarios on the photo frame, restoring its service.
Invoke on the frame as root. No --state or --sync is passed to the benchmark host.
"""
import argparse,json,os,pathlib,pwd,subprocess,time

def main():
    p=argparse.ArgumentParser();p.add_argument('--cache',type=pathlib.Path,default=pathlib.Path('/var/lib/otrios-dashboard'));p.add_argument('--seconds',type=int,default=20);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
    if not 5<=a.seconds<=60:raise ValueError('Choose 5..60 seconds per scenario')
    release=json.loads((a.cache/'last-good.json').read_text())['id']
    if len(release)!=64 or any(c not in '0123456789abcdef' for c in release):raise ValueError('Invalid cached release')
    exe=a.cache/release/'dashboard-native'
    a.output.mkdir(parents=True,exist_ok=True);kiosk=pwd.getpwnam("kiosk");os.chown(a.output,kiosk.pw_uid,kiosk.pw_gid);a.output.chmod(0o700)
    active=subprocess.run(['systemctl','is-active','--quiet','otrios-dashboard']).returncode==0
    reports=[]
    try:
        if active:subprocess.run(['systemctl','stop','otrios-dashboard'],check=True)
        for name,screen,objects,hz,stall in [('motion-12-idle','animation',12,0,0),('motion-64-load-stall','animation',64,30,250),('cubes-12-idle','3d',12,0,0),('cubes-64-idle','3d',64,0,0),('cubes-64-load-stall','3d',64,30,250)]:
            print('START '+name,flush=True);report=a.output/(name+'.json')
            command=['runuser','-u','kiosk','--','env','HOME=/home/kiosk','XDG_RUNTIME_DIR=/run/spd-kiosk','WAYLAND_DISPLAY=wayland-0','XDG_CACHE_HOME=/var/lib/otrios-dashboard','LD_LIBRARY_PATH=/opt/otrios-dashboard/lib','SDL_RENDER_DRIVER=opengles2','OTRIOS_RELEASE_ID='+release,str(exe),'--fullscreen','--screen',screen,'--objects',str(objects),'--load-hz',str(hz),'--worker-stall-ms',str(stall),'--seconds',str(a.seconds),'--report',str(report)]
            with (a.output/(name+'.log')).open('w') as log:subprocess.run(command,check=True,stdout=log,stderr=subprocess.STDOUT,timeout=a.seconds+30)
            item=json.loads(report.read_text());item['scenario']=name;item['observed_load_hz']=round(item['load_updates']/item['seconds'],2);reports.append(item);print(json.dumps(item),flush=True)
    finally:
        if active:subprocess.run(['systemctl','start','otrios-dashboard'],check=True)
    (a.output/'results.json').write_text(json.dumps({'release':release,'measured_at_unix':time.time(),'scenarios':reports},indent=2))
    a.output.chmod(0o700)
if __name__=='__main__':main()
