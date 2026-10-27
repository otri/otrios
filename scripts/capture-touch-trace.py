#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Opt-in, read-only evdev recording over the existing pinned frame SSH.

Keep the requested app/control visible during capture. No grab, input injection,
service stop or calibration write occurs. Replay uses disposable RAM-only apps.
"""
import argparse
import json
from pathlib import Path
import shlex
import subprocess

REMOTE = r'''
import fcntl,json,os,pathlib,re,select,struct,time
fd=os.open(DEVICE,os.O_RDONLY|os.O_NONBLOCK)
try:
 try:fcntl.ioctl(fd,0x400445a0,struct.pack('i',1)) # EVIOCSCLOCKID, CLOCK_MONOTONIC
 except OSError:pass # Only timestamp differences are recorded.
 def axis(code):
  buf=bytearray(24);fcntl.ioctl(fd,0x80184540+code,buf);v=struct.unpack('6i',buf);return v[0],v[1],v[2]
 x,xmin,xmax=axis(0);y,ymin,ymax=axis(1);assert xmax>xmin and ymax>ymin
 keys=bytearray(96);fcntl.ioctl(fd,0x80604518,keys);down=bool(keys[330//8]&(1<<(330%8)));suppressed=down;reported=False
 matrix=re.search(r'LIBINPUT_CALIBRATION_MATRIX[^=]*=\s*"([^"]+)"',pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules').read_text()).group(1)
 calibration=list(map(float,matrix.split()));assert len(calibration)==6
 event=struct.Struct('llHHi');points=[];origin=None;last=0;end=time.monotonic()+SECONDS;poll=select.poll();poll.register(fd,select.POLLIN)
 while time.monotonic()<end and len(points)<16383:
  if not poll.poll(min(100,max(1,int((end-time.monotonic())*1000)))):continue
  try:data=os.read(fd,event.size*128)
  except BlockingIOError:continue
  if not data or len(data)%event.size:raise RuntimeError('Short touch device read')
  for sec,usec,kind,code,value in event.iter_unpack(data):
   if len(points)>=16383:break
   if kind==0 and code==3:
    if reported:points.append({'at_ms':last,'id':1,'phase':3,'position':[(x-xmin)/(xmax-xmin),(y-ymin)/(ymax-ymin)]})
    reported=False;suppressed=True;continue
   if kind==3 and code==0:x=value
   if kind==3 and code==1:y=value
   if kind==1 and code==330:down=value!=0
   if kind==0 and code==0:
    if suppressed:
     if not down:suppressed=False
     continue
    if down or reported:
     stamp=sec*1000+usec//1000
     if origin is None:origin=stamp
     last=max(last,stamp-origin)
     points.append({'at_ms':last,'id':1,'phase':(1 if reported else 0)if down else 2,'position':[(x-xmin)/(xmax-xmin),(y-ymin)/(ymax-ymin)]})
     reported=down
 if reported:points.append({'at_ms':last,'id':1,'phase':3,'position':[(x-xmin)/(xmax-xmin),(y-ymin)/(ymax-ymin)]})
 print(json.dumps({'format':'otrios-touch-trace-v1','source':'captured-evdev','screen':SCREEN,'control':CONTROL,'viewport':VIEWPORT,'mapping':{'rotation':ROTATION,'surface':SURFACE,'calibration':calibration},'points':points}))
finally:os.close(fd)
'''

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--config', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--seconds', type=int, default=15)
    p.add_argument('--screen', required=True)
    p.add_argument('--control', required=True)
    p.add_argument('--rotation', type=int, choices=[0, 90, 180, 270], default=90)
    p.add_argument('--viewport', default='1280,800')
    p.add_argument('--surface', default='1280,800')
    p.add_argument('--device', default='/dev/input/by-path/platform-2005a000.i2c-event')
    a = p.parse_args()
    if not 1 <= a.seconds <= 120:
        p.error('Capture duration must be 1–120 seconds')
    config = json.loads(a.config.read_text())
    ssh = config['frame']['ssh']
    if not isinstance(ssh, list) or not ssh or ssh[0] != 'ssh':
        p.error('Use the existing pinned frame SSH configuration')
    values = {'SECONDS': a.seconds, 'SCREEN': a.screen, 'CONTROL': a.control,
              'ROTATION': a.rotation, 'VIEWPORT': list(map(int, a.viewport.split(','))),
              'SURFACE': list(map(int, a.surface.split(','))), 'DEVICE': a.device}
    # Prepend data as Python literals; values are never interpolated into shell code.
    script = '\n'.join(k + '=' + repr(v) for k, v in values.items()) + '\n' + REMOTE
    print(f'Recording {a.control} for {a.seconds}s; keep {a.screen} visible.', flush=True)
    result = subprocess.run(ssh + ['python3 -c ' + shlex.quote(script)],
                            check=True, capture_output=True, text=True, timeout=a.seconds+30)
    trace = json.loads(result.stdout)
    if not trace['points']:
        raise SystemExit('No contacts recorded; no trace saved.')
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(trace, indent=2)+'\n')
    print(f"Saved {len(trace['points'])} timestamped samples to {a.output}")

if __name__ == '__main__':
    main()
