# SPDX-License-Identifier: Apache-2.0
# Bounded calibrated Display Accent Apply trial; requires an idle Frame with a gold override.
import json, pathlib, shlex, subprocess, sys
runtime = pathlib.Path.home() / 'Library/Application Support/OTriOSDashboard'
config = json.loads((runtime / 'config.json').read_text())
remote = r'''
import fcntl,json,os,pathlib,re,struct,time
base=pathlib.Path('/run/otrios-direct-console/otrios')
path=max(base.glob('ready-*.json'),key=lambda p:p.stat().st_mtime)
def ready():return json.loads(path.read_text())
def idle(q):return q['state_confirmed'] and q['stream_ready'] and not q['input_busy'] and q['pending_commands']==0
q=ready();assert q['ui_ref']=='app:oib-designer/ui' and idle(q) and not q['keyboard_visible']
assert q['rotation']==90 and q['output']==[800,1280] and q['touch_calibrated']
expected=q['release'];initial=q['runtime_resources']['oib']['active_generation']
original=q['runtime_resources']['oib']['properties']['13']
assert original=='Accent: #DAA520 (override)', 'This bounded trial restores the existing gold override'
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
records=[]
try:
 # A fresh host selects Audio. Select Display's preview background, then its
 # explicit Edit component action, which chooses the owned root and resets
 # inspector scroll. A background child alone has no Accent inspector.
 tap(28,450);time.sleep(.4)
 assert ready()['runtime_resources']['oib']['selected_instance']==30
 tap(1000,162);time.sleep(.4)
 for color,x in [('#5588FF',964),('#DAA520',1044)]:
  stamp=path.stat().st_mtime_ns;tap(x,514)
  wait_presented(stamp,lambda q:q['runtime_resources']['oib']['active_generation']==initial)
  stamp=path.stat().st_mtime_ns;began=time.monotonic();tap(1002,682)
  q=wait_presented(stamp,lambda q:q['runtime_resources']['oib']['active_generation']==initial+1 and q['runtime_resources']['oib']['presented_generation']==initial+1 and color in q['runtime_resources']['oib']['properties']['13'])
  records.append({'color':color,'touch_to_confirmed_presented_ms':round((time.monotonic()-began)*1000,3),'generation':initial+1});initial+=1
 assert pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules').read_bytes()==rule
 print(json.dumps({'passed':True,'release':expected,'measurement':'software evdev touch-down to native confirmed presentation; 80ms touch hold and 5ms polling; not photon latency','original_property':original,'final_property':q['runtime_resources']['oib']['properties']['13'],'records':records,'live_state_reset':False,'input_idle':idle(q)}))
finally:
 touch(1002,682,False);os.close(fd)
'''
result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(remote)], capture_output=True, text=True, timeout=100)
if result.returncode:
    print(result.stderr)
    raise SystemExit(result.returncode)
record=json.loads(result.stdout)
pathlib.Path(sys.argv[1]).write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
