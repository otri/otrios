#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Isolated durable authority pair and transparent WebSocket byte counters."""
import argparse,pathlib,json,subprocess,asyncio,os,time
procs=[]
stats={label:{'connections':0,'up_binary':0,'down_binary':0,'up_ws':0,'down_ws':0} for label in ['baseline','merkle']}
class Frames:
 def __init__(self,label,direction):self.buffer=b'';self.headers=False;self.label=label;self.direction=direction
 def feed(self,data):
  self.buffer+=data
  if not self.headers:
   end=self.buffer.find(b'\r\n\r\n')
   if end<0:return
   self.headers=True;self.buffer=self.buffer[end+4:]
  while len(self.buffer)>=2:
   b=self.buffer;n=b[1]&127;offset=2
   if n==126:
    if len(b)<4:return
    n=int.from_bytes(b[2:4],'big');offset=4
   elif n==127:
    if len(b)<10:return
    n=int.from_bytes(b[2:10],'big');offset=10
   if n>2*1024*1024:raise ValueError('Oversized benchmark frame')
   offset+=4 if b[1]&128 else 0
   if len(b)<offset+n:return
   stats[self.label][self.direction+'_ws']+=offset+n
   if b[0]&15 in (0,2):stats[self.label][self.direction+'_binary']+=n
   self.buffer=b[offset+n:]
async def proxy(reader,writer,label,port):
 stats[label]['connections']+=1
 upstream_r,upstream_w=await asyncio.open_connection('127.0.0.1',port)
 async def relay(r,w,direction):
  parser=Frames(label,direction)
  try:
   while data:=await r.read(65536):
    parser.feed(data);w.write(data);await w.drain()
  except (ConnectionError,asyncio.CancelledError):pass
  finally:w.close()
 await asyncio.gather(relay(reader,upstream_w,'up'),relay(upstream_r,writer,'down'))
async def sample():
 while True:
  path=work/'canvas-bench-proxy-stats.json';temp=path.with_suffix('.tmp');temp.write_text(json.dumps({'at':time.monotonic(),'stats':stats}));os.replace(temp,path)
  await asyncio.sleep(.1)
async def main():
 servers=[]
 for label,(release,port,proxy_port,binary) in labels.items():
  base=work/('canvas-bench-authority-'+label);base.mkdir(exist_ok=True)
  releases=base/'releases';releases.mkdir(exist_ok=True)
  link=releases/release
  if not link.exists():link.symlink_to(release_paths[label],target_is_directory=True)
  (releases/'current.json').write_text(json.dumps({'id':release,'web_entry':f'/releases/{release}/web/index.html'}))
  log=open(base/'authority.log','w')
  p=subprocess.Popen([str(binary),'--bind',f'127.0.0.1:{port}','--app-state',str(base/'state'),'--releases',str(releases)],stdout=log,stderr=subprocess.STDOUT);procs.append(p)
  servers.append(await asyncio.start_server(lambda r,w,l=label,p=port:proxy(r,w,l,p),'127.0.0.1',proxy_port))
 print('Two isolated durable authorities and per-native-peer bandwidth proxies running',flush=True)
 await sample()
if __name__=='__main__':
 parser=argparse.ArgumentParser()
 for name in ['baseline-server','merkle-server','baseline-release','merkle-release','output']:parser.add_argument('--'+name,type=pathlib.Path,required=True)
 args=parser.parse_args();work=args.output.resolve();work.mkdir(parents=True,exist_ok=True);procs=[]
 labels={'baseline':(args.baseline_release.name,8901,8911,args.baseline_server.resolve()),'merkle':(args.merkle_release.name,8902,8912,args.merkle_server.resolve())}
 release_paths={'baseline':args.baseline_release.resolve(),'merkle':args.merkle_release.resolve()}
 try:asyncio.run(main())
 finally:
  for p in procs:p.terminate()
  for p in procs:p.wait()
