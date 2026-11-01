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
import fcntl,hashlib,json,os,pathlib,re,struct,time,urllib.request,subprocess
base=pathlib.Path('/run/otrios-direct-console/otrios')
def ready():
 return json.loads(max(base.glob('ready-*.json'),key=lambda p:p.stat().st_mtime).read_text())
latencies=[]
def wait(predicate):
 start=time.monotonic()
 while time.monotonic()-start<10:
  q=ready()
  assert q['runtime_release']==expected,'Unexpected native release'
  # A handoff can briefly report the loading UI before its signed view arrives.
  if q['release']==expected and predicate(q):latencies.append(round((time.monotonic()-start)*1000,1));return q
  time.sleep(.02)
 raise RuntimeError('Native input acceptance deadline: '+json.dumps({k:q.get(k)for k in ['ui_ref','input_busy','pending_commands']}|{'map_scroll':q.get('runtime_resources',{}).get('map_scroll')}))
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
def title_points(metadata):
 assert runtime_probe,'Use --runtime-probe for edit acceptance'
 root=pathlib.Path('/var/lib/otrios-authority')
 result=subprocess.run([runtime_probe,str(root/'releases'/expected/'apps/catalog.flex')],input=json.dumps(metadata).encode(),capture_output=True)
 assert result.returncode==0,'Private runtime coordinate probe failed'
 points=json.loads(result.stdout)
 return {k:[v[0]+16,v[1]+24]for k,v in points.items()if k in ['start','end']}

try:
 if not skip_navigation:
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
 reorder_passed=False;reorder_changed=False
 if reorder:
  token=pathlib.Path('/etc/otrios-dashboard.identity').read_text().strip()
  def metadata():
   req=urllib.request.Request('http://127.0.0.1:8877/spaces',headers={'Authorization':'Bearer '+token,'Origin':'https://otrios.otri.ca'})
   return json.load(urllib.request.urlopen(req,timeout=5))
  before=metadata()
  source=next(s for s in before['spaces']if s['name']==source_name)
  destination=next(s for s in before['spaces']if s['name']==destination_name)
  assert source['role']==destination['role']=='admin','Both Spaces must be administered by Frame'
  order=[space['id']for space in before['spaces']]
  if order.index(source['id'])<order.index(destination['id']):reorder_passed=True
  else:
   before_edit=ready()['epoch'];tap(1220,756)
   q=wait(lambda q:home(q)and q['epoch']>before_edit)
   edit_epoch=q['epoch']
   points=title_points(before)
   # Use a title's blank padding, not a painted glyph. This also checks hit geometry.
   start=points['start'];end=points['end']
   assert 104<=start[1]<660 and 104<=end[1]<660,'Use visible title positions for this native probe'
   touch(*start,True);wait(lambda q:q['input_busy']and q['epoch']==edit_epoch)
   for step in range(1,13):
    touch(*(start[i]+(end[i]-start[i])*step/12 for i in range(2)),True)
    time.sleep(.06)
    q=ready();assert q['epoch']==edit_epoch,'Edit preview remounted and cancelled native capture'
   touch(*end,False)
   deadline=time.monotonic()+10
   while time.monotonic()<deadline:
    current=metadata();order=[s['id']for s in current['spaces']]
    if current['generation']>before['generation']and order.index(source['id'])<order.index(destination['id']):break
    time.sleep(.1)
   else:raise RuntimeError('Native title drag did not place source before destination')
   tap(1220,756);wait(home);reorder_passed=True;reorder_changed=True
 if edit_capture:
  token=pathlib.Path('/etc/otrios-dashboard.identity').read_text().strip()
  def metadata():
   req=urllib.request.Request('http://127.0.0.1:8877/spaces',headers={'Authorization':'Bearer '+token,'Origin':'https://otrios.otri.ca'})
   return json.load(urllib.request.urlopen(req,timeout=5))
  initial=metadata();before_edit=ready()['epoch'];tap(1220,756)
  q=wait(lambda q:home(q)and q['epoch']>before_edit);edit_epoch=q['epoch']
  start=title_points(initial)['start']
  touch(*start,True);wait(lambda q:q['input_busy']and q['epoch']==edit_epoch)
  for dx in [15,30,45,30,15,0]:
   touch(start[0]+dx,start[1]+16,True);time.sleep(.12)
   assert ready()['epoch']==edit_epoch,'Edit feedback broke native capture'
  touch(*start,False);wait(lambda q:home(q)and q['epoch']==edit_epoch)
  assert ready()['epoch']==edit_epoch and not ready()['input_busy'],'Same-title drop should release without a write'
  assert metadata()['generation']==initial['generation'],'Cancelled title drag changed metadata'
  tap(1220,756);wait(home)
 if retained_review:
  token=pathlib.Path('/etc/otrios-dashboard.identity').read_text().strip()
  def metadata():
   req=urllib.request.Request('http://127.0.0.1:8877/spaces',headers={'Authorization':'Bearer '+token,'Origin':'https://otrios.otri.ca'})
   return json.load(urllib.request.urlopen(req,timeout=5))
  initial=metadata()
  result=subprocess.run([runtime_probe,str(pathlib.Path('/var/lib/otrios-authority/releases')/expected/'apps/catalog.flex'),'--retained-review'],input=json.dumps(initial).encode(),capture_output=True)
  assert result.returncode==0,'Private retained-data coordinate probe failed'
  points=json.loads(result.stdout);assert points['offline_cancel_no_commands']
  if ready()['ui_ref']!='app:spaces-host/ui':tap(5,5)
  wait(lambda q:home(q)and scroll(q)is not None);bound(False)
  before_edit=ready()['epoch'];tap(1220,756)
  wait(lambda q:home(q)and q['epoch']>before_edit)
  tap(*points['delete_app']);wait(lambda q:home(q)and scroll(q)is None)
  before_toggle=ready()['epoch'];tap(*points['remove_data'])
  wait(lambda q:home(q)and scroll(q)is None and q['epoch']>before_toggle)
  tap(*points['cancel']);wait(lambda q:home(q)and scroll(q)is not None)
  assert metadata()['generation']==initial['generation'],'Cancelled removal changed metadata'
  tap(1220,756);wait(home)
 if move_review:
  token=pathlib.Path('/etc/otrios-dashboard.identity').read_text().strip()
  def metadata():
   req=urllib.request.Request('http://127.0.0.1:8877/spaces',headers={'Authorization':'Bearer '+token,'Origin':'https://otrios.otri.ca'})
   return json.load(urllib.request.urlopen(req,timeout=5))
  initial=metadata()
  result=subprocess.run([runtime_probe,str(pathlib.Path('/var/lib/otrios-authority/releases')/expected/'apps/catalog.flex'),'--move-review'],input=json.dumps(initial).encode(),capture_output=True)
  assert result.returncode==0,'Private cross-Space coordinate probe failed'
  points=json.loads(result.stdout);assert points['offline_cancel_no_commands']
  if ready()['ui_ref']!='app:spaces-host/ui':tap(5,5)
  wait(lambda q:home(q)and scroll(q)is not None);bound(False)
  before_edit=ready()['epoch'];tap(1220,756)
  q=wait(lambda q:home(q)and q['epoch']>before_edit);epoch=q['epoch']
  start=points['source'];end=points['destination']
  touch(*start,True);wait(lambda q:q['input_busy']and q['epoch']==epoch)
  for step in range(1,13):
   touch(*(start[i]+(end[i]-start[i])*step/12 for i in range(2)),True);time.sleep(.06)
   assert ready()['epoch']==epoch,'App drag remounted the native map'
  touch(*end,False);wait(lambda q:home(q)and scroll(q)is None)
  tap(*points['cancel']);wait(lambda q:home(q)and scroll(q)is not None)
  assert metadata()['generation']==initial['generation'],'Cancelled move changed metadata'
  tap(1220,756);wait(home)
 assert rule.read_bytes()==calibration,'Calibration changed'
 print(json.dumps({'passed':True,'release':expected,'scroll_round_trips':0 if skip_navigation else 3,'navigation_exercised':not skip_navigation,
  'stable_map_epoch':True,'immediate_bound_reversal':None if skip_navigation else True,
  'corner_return':None if skip_navigation else [5,5],
  'app_ui':app_ui,'input':'synthetic evdev through calibrated physical-device reader',
  'human_touch_observation':False,'app_writes':0,'pixels_exported':False,
  'calibration_unchanged':True,'space_title_reorder':reorder_changed,'requested_order_verified':reorder_passed,'admin_title_capture':edit_capture,
  'capture_probe_metadata_unchanged':edit_capture,'retained_removal_review_cancel':retained_review,'retained_review_metadata_unchanged':retained_review,
  'cross_space_drop_review_cancel':move_review,'move_review_metadata_unchanged':move_review,'space_order_changed':reorder_changed,'readiness_wait_ms_max':max(latencies)}))
finally:
 touch(20,600,False);os.close(fd)
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--pointer', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--app-point', default='120,132')
    parser.add_argument('--app-ui', default='app:brass-relay/ui')
    parser.add_argument('--device', default='/dev/input/by-path/platform-2005a000.i2c-event')
    parser.add_argument('--reorder-spaces', action='store_true',
                        help='Also drag Brass Relay before Household, retaining that requested order')
    parser.add_argument('--edit-capture', action='store_true',
                        help='Exercise a visible administrable title and drop it back without a metadata write')
    parser.add_argument('--retained-review', action='store_true', help='Open Remove App, toggle Remove Data and cancel without a management write')
    parser.add_argument('--move-review', action='store_true', help='Drag an app across Spaces, open the review and cancel without a write')
    parser.add_argument('--runtime-probe', help='Private on-Frame space_map_probe executable')
    parser.add_argument('--skip-navigation', action='store_true', help='Exercise only the open map edit flow')
    args = parser.parse_args()
    if (args.reorder_spaces or args.edit_capture or args.retained_review or args.move_review) and not args.runtime_probe:
        parser.error('Edit probes require --runtime-probe')
    config = json.loads(args.config.read_text())
    pointer = args.pointer or pathlib.Path(config['runtime']) / 'build/staged-release.json'
    values = {'expected': json.loads(pointer.read_text())['id'], 'device': args.device,
              'app_point': list(map(float, args.app_point.split(','))), 'app_ui': args.app_ui,
              'reorder': args.reorder_spaces, 'edit_capture': args.edit_capture,
              'runtime_probe': args.runtime_probe, 'move_review': args.move_review, 'retained_review': args.retained_review, 'skip_navigation': args.skip_navigation, 'source_name': 'Brass Relay', 'destination_name': 'Household'}
    script = '\n'.join(k + '=' + repr(v) for k, v in values.items()) + '\n' + REMOTE
    result = subprocess.run(config['frame']['ssh'] + ['python3 -c ' + shlex.quote(script)],
                            capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise SystemExit(result.stderr.strip())
    record = json.loads(result.stdout)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
