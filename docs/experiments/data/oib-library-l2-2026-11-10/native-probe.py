#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Focused existing-Frame evdev input; accepts existing private config, output and JSON tap points.

Example: python3 native-probe.py --config /private/staging-config.json --output /tmp/native.json --points '[[270,200]]'
Coordinates are logical 1280x800. Does not capture pixels or change calibration.
"""
import argparse,json,pathlib,shlex,subprocess
REMOTE = r'''
import fcntl,json,os,pathlib,re,struct,time
base=pathlib.Path('/run/otrios-direct-console/otrios')
path=max(base.glob('ready-*.json'),key=lambda p:p.stat().st_mtime)
def ready():return json.loads(path.read_text())
def idle(q):return q['state_confirmed'] and q['stream_ready'] and not q['input_busy'] and q['pending_commands']==0
q=ready();assert q['ui_ref'] in ['app:oib-designer/ui','app:spaces-host/ui'] and idle(q) and not q['keyboard_visible']
assert q['rotation']==90 and q['output']==[800,1280] and q['touch_calibrated']
expected=q['release']

rule=pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules').read_bytes()
a,b,c,d,e,f=map(float,re.search(rb'LIBINPUT_CALIBRATION_MATRIX[^=]*=\s*"([^"]+)"',rule).group(1).split());det=a*e-b*d
fd=os.open('/dev/input/by-path/platform-2005a000.i2c-event',os.O_RDWR)
keys=bytearray(96);fcntl.ioctl(fd,0x80604518,keys);assert not keys[330//8]&(1<<(330%8)),'Physical touch busy'
def axis(code):
 data=bytearray(24);fcntl.ioctl(fd,0x80184540+code,data);return struct.unpack('6i',data)[1:3]
ranges=[axis(0),axis(1)]
def touch(x,y,down):
 px=(800-y)/800-c;py=x/1280-f
 coords=[(e*px-b*py)/det,(-d*px+a*py)/det]
 vals=[round(lo+v*(hi-lo))for v,(lo,hi)in zip(coords,ranges)]
 def ev(t,k,v):return struct.pack('llHHi',0,0,t,k,v)
 os.write(fd,ev(3,0,vals[0])+ev(3,1,vals[1])+ev(1,330,int(down))+ev(0,0,0))
def tap(x,y):
 touch(x,y,True);time.sleep(.08);touch(x,y,False)
def wait_presented(after, test, timeout=15):
 deadline=time.monotonic()+timeout
 while time.monotonic()<deadline:
  q=ready();assert q['release']==expected
  if path.stat().st_mtime_ns>after and idle(q) and test(q):return q
  time.sleep(.005)
 raise RuntimeError('Native confirmed presentation timeout')

initial=q['runtime_resources']['component_behavior_calls']
try:
 if q['ui_ref']=='app:spaces-host/ui':
  stamp=path.stat().st_mtime_ns;tap(542,544)
  q=wait_presented(stamp,lambda q:q['ui_ref']=='app:oib-designer/ui',timeout=30)
 for x,y in points:
  stamp=path.stat().st_mtime_ns;tap(x,y)
  q=wait_presented(stamp,lambda q:q['ui_ref']=='app:oib-designer/ui',timeout=30)
  time.sleep(.25)
 q=ready()
 assert pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules').read_bytes()==rule
 print(json.dumps({'passed':True,'method':'software evdev injection through existing calibrated native input; no grab or calibration change','release':expected,'points':points,'before_calls':initial,'after_calls':q['runtime_resources']['component_behavior_calls'],'resources':q['runtime_resources'],'input_idle':idle(q)}))
finally:
 touch(0,0,False);os.close(fd)
'''

def main():
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('--config',type=pathlib.Path,required=True)
 parser.add_argument('--output',type=pathlib.Path,required=True)
 parser.add_argument('--points',required=True)
 args=parser.parse_args();points=json.loads(args.points)
 if not isinstance(points,list) or len(points)>8 or any(not isinstance(p,list) or len(p)!=2 or not all(isinstance(v,(int,float))for v in p) or not(0<=p[0]<1280 and 0<=p[1]<800)for p in points):parser.error('Use at most eight bounded logical coordinate pairs')
 config=json.loads(args.config.read_text())
 remote='points='+repr(points)+'\n'+REMOTE
 result=subprocess.run(config['frame']['ssh']+['python3 -c '+shlex.quote(remote)],capture_output=True,text=True,timeout=120)
 if result.returncode:raise SystemExit(result.stderr[-2000:])
 record=json.loads(result.stdout);args.output.write_text(json.dumps(record,indent=2)+'\n')
 print(json.dumps({k:v for k,v in record.items()if k!='resources'}))

if __name__=='__main__':main()
