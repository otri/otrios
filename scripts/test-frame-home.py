#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Exercise home scrolling and corner return through the calibrated Frame reader.

Injects navigation-only evdev contacts into the existing rotated 800x1280 Frame.
Does not export pixels, state, Space names or credentials, or change calibration.
This is physical-device software acceptance, not a human touch observation.
"""
import argparse
import json
import pathlib
import shlex
import subprocess

REMOTE = r'''
import fcntl,hashlib,json,os,pathlib,re,struct,time
base=pathlib.Path('/run/otrios-direct-console/otrios')
def ready():
 return json.loads(max(base.glob('ready-*.json'),key=lambda p:p.stat().st_mtime).read_text())
latencies=[]
def wait(predicate):
 start=time.monotonic()
 while time.monotonic()-start<10:
  q=ready()
  assert q['release']==q['runtime_release']==expected,'Unexpected release'
  if predicate(q):latencies.append(round((time.monotonic()-start)*1000,1));return q
  time.sleep(.02)
 raise RuntimeError('Native input acceptance deadline')
def idle(q):return not q['input_busy'] and q['state_confirmed'] and q['pending_commands']==0 and q['stream_ready']
def home(q):return q['ui_ref']=='app:spaces-host/ui' and idle(q)
def scroll(q):return q['runtime_resources']['map_scroll']
q=ready();assert q['rotation']==90 and q['output']==[800,1280] and q['touch_calibrated']
rule=pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules');calibration=rule.read_bytes()
a,b,c,d,e,f=map(float,re.search(rb'LIBINPUT_CALIBRATION_MATRIX[^=]*=\s*"([^"]+)"',calibration).group(1).split());det=a*e-b*d
fd=os.open(device,os.O_RDWR)
keys=bytearray(96);fcntl.ioctl(fd,0x80604518,keys)
assert not keys[330//8]&(1<<(330%8)),'A person is touching the display; retry when idle'
def axis(code):
 buf=bytearray(24);fcntl.ioctl(fd,0x80184540+code,buf);v=struct.unpack('6i',buf);return v[1:3]
ranges=[axis(0),axis(1)]
def event(t,code,v):return struct.pack('llHHi',0,0,t,code,v)
def touch(x,y,down):
 px=(800-y)/800-c;py=x/1280-f
 normalized=[(e*px-b*py)/det,(-d*px+a*py)/det]
 vals=[round(lo+v*(hi-lo))for v,(lo,hi)in zip(normalized,ranges)]
 os.write(fd,event(3,0,vals[0])+event(3,1,vals[1])+event(1,330,int(down))+event(0,0,0))
def tap(x,y):
 touch(x,y,True);time.sleep(.08);touch(x,y,False)
def bound(bottom):
 for _ in range(16):
  q=ready();s=scroll(q)
  if abs(s['offset'][1]-(s['limit'][1] if bottom else 0))<1:return
  start,end=(680,120)if bottom else(120,680)
  touch(20,start,True);time.sleep(.04);touch(20,end,True);time.sleep(.12);touch(20,end,False)
  wait(lambda q: home(q) and not scroll(q)['active'])
 raise RuntimeError('Map did not reach scroll bound')
try:
 if q['ui_ref']!='app:spaces-host/ui':tap(5,5)
 q=wait(home);epoch=q['epoch'];limit=scroll(q)['limit'][1]
 assert limit>0,'Use a home map with content below the viewport'
 for _ in range(3):
  bound(True)
  touch(20,600,True);time.sleep(.04);touch(20,200,True)
  wait(lambda q: abs(scroll(q)['offset'][1]-limit)<1)
  touch(20,220,True)
  wait(lambda q: 10<limit-scroll(q)['offset'][1]<30)
  touch(20,200,True);time.sleep(.12);touch(20,200,False);wait(home)
  bound(False)
  touch(20,200,True);time.sleep(.04);touch(20,600,True)
  wait(lambda q: scroll(q)['offset'][1]<1)
  touch(20,580,True)
  wait(lambda q: 10<scroll(q)['offset'][1]<30)
  touch(20,600,True);time.sleep(.12);touch(20,600,False)
  q=wait(home);assert q['epoch']==epoch,'Scroll remounted the map'
 # Known public app position is explicit so this never guesses a private tile.
 tap(*app_point)
 wait(lambda q:q['ui_ref']==app_ui and idle(q))
 tap(5,5)
 wait(home)
 assert rule.read_bytes()==calibration,'Calibration changed'
 print(json.dumps({'passed':True,'release':expected,'scroll_round_trips':3,
  'stable_map_epoch':True,'immediate_bound_reversal':True,'corner_return':[5,5],
  'app_ui':app_ui,'input':'synthetic evdev through calibrated physical-device reader',
  'human_touch_observation':False,'app_writes':0,'pixels_exported':False,
  'calibration_unchanged':True,'readiness_wait_ms_max':max(latencies)}))
finally:
 touch(20,600,False);os.close(fd)
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--pointer', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--app-point', default='700,132')
    parser.add_argument('--app-ui', default='app:brass-relay/ui')
    parser.add_argument('--device', default='/dev/input/by-path/platform-2005a000.i2c-event')
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    pointer = args.pointer or pathlib.Path(config['runtime']) / 'build/staged-release.json'
    values = {'expected': json.loads(pointer.read_text())['id'], 'device': args.device,
              'app_point': list(map(float, args.app_point.split(','))), 'app_ui': args.app_ui}
    script = '\n'.join(k + '=' + repr(v) for k, v in values.items()) + '\n' + REMOTE
    result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(script)],
                            capture_output=True, text=True, check=True, timeout=180)
    record = json.loads(result.stdout)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
