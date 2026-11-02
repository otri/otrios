# SPDX-License-Identifier: Apache-2.0
# Bounded Frame-only software gesture probe; run through the configured pinned SSH.
import fcntl,json,os,pathlib,re,struct,time,threading,statistics
base=pathlib.Path('/run/otrios-direct-console/otrios')
p=max(base.glob('ready-*.json'),key=lambda p:p.stat().st_mtime)
def ready():return json.loads(p.read_text())
q=ready();assert q['ui_ref']=='app:live-ui/ui' and not q['input_busy'],'Open Live UI and leave touch idle'
pid=q['pid'];epoch=q['epoch'];mode=os.environ.get('OTRIOS_PROBE_GESTURE','panel');assert mode in ['panel','slider']
initial=q['runtime_resources']['panel_scroll']['offset'][1] if mode=='panel' else 0
# Wide Live UI preview position, with its current inherited/overridden spacing.
spacing=float(re.search(r': ([0-9.]+)',q['runtime_resources']['oib']['properties']['203']).group(1))
slider_y=252.+spacing
rule=pathlib.Path('/etc/udev/rules.d/99-spd-touch.rules').read_bytes()
a,b,c,d,e,f=map(float,re.search(rb'LIBINPUT_CALIBRATION_MATRIX[^=]*=\s*"([^"]+)"',rule).group(1).split());det=a*e-b*d
fd=os.open('/dev/input/by-path/platform-2005a000.i2c-event',os.O_RDWR)
keys=bytearray(96);fcntl.ioctl(fd,0x80604518,keys);assert not keys[330//8]&(1<<(330%8)),'Physical touch busy'
def axis(code):
 buf=bytearray(24);fcntl.ioctl(fd,0x80184540+code,buf);return struct.unpack('6i',buf)[1:3]
ranges=[axis(0),axis(1)]
def touch(x,y,down):
 px=(800-y)/800-c;py=x/1280-f
 coords=[(e*px-b*py)/det,(-d*px+a*py)/det]
 vals=[round(lo+v*(hi-lo))for v,(lo,hi)in zip(coords,ranges)]
 def ev(t,k,v):return struct.pack('llHHi',0,0,t,k,v)
 os.write(fd,ev(3,0,vals[0])+ev(3,1,vals[1])+ev(1,330,int(down))+ev(0,0,0))
def cpu():
 v=pathlib.Path(f'/proc/{pid}/stat').read_text().split(') ',1)[1].split();return int(v[11])+int(v[12])
records=[];stop=threading.Event()
def sample():
 seen=0
 while not stop.is_set():
  try:
   stamp=p.stat().st_mtime_ns
   if stamp!=seen:
    v=ready();seen=stamp
    records.append({'at':time.monotonic(),'offset':v['runtime_resources']['panel_scroll']['offset'][1],'sample':v['runtime_resources']['sampled_monotonic_ms'],'input_busy':v['input_busy']})
  except (OSError,ValueError):pass
  stop.wait(.005)
t=threading.Thread(target=sample);t.start();start=time.monotonic();c0=cpu()
try:
 # Begin at the upper bound before measuring a reproducible downward pan.
 if initial>0:
  touch(1200,230,True)
  for i in range(1,31):touch(1200,230+i*12,True);time.sleep(1/30)
  touch(1200,590,False);time.sleep(1.5)
 records.clear();begin=time.monotonic()
 touch(*( [1200.,550.] if mode=='panel' else [120.,slider_y]),True)
 for i in range(1,91):
  due=begin+i/30
  time.sleep(max(0,due-time.monotonic()))
  touch(*( [1200.,550.-i*3.5] if mode=='panel' else [120.+4.*i,slider_y]),True)
 touch(*( [1200.,235.] if mode=='panel' else [480.,slider_y]),False);end=time.monotonic();time.sleep(2)
 after=ready();assert after['pid']==pid and after['epoch']==epoch
 active=[v for v in records if begin<=v['at']<=end and v['input_busy']]
 intervals=[(b['at']-a['at'])*1000 for a,b in zip(active,active[1:])]
 result={'gesture':mode,'release':q['runtime_release'],'pid':pid,'offered_hz':30,'gesture_seconds':end-begin,'presentations_during_gesture':len(active),'observed_hz':len(active)/(end-begin),'frame_interval_ms':intervals,'samples':[{**v,'at':round(v['at']-begin,4)}for v in records],'final_offset':after['runtime_resources']['panel_scroll']['offset'][1],'input_released':not after['input_busy'],'cpu_one_core_percent':100*(cpu()-c0)/os.sysconf('SC_CLK_TCK')/(time.monotonic()-start)}
 print(json.dumps(result))
finally:
 touch(*( [1200.,235.] if mode=='panel' else [480.,slider_y]),False);stop.set();t.join();os.close(fd)
