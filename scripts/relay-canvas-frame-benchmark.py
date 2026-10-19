#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Private loopback relay; use a tailnet-only upstream, then remove after testing."""
import argparse,asyncio
p=argparse.ArgumentParser();p.add_argument("--upstream",required=True);a=p.parse_args()
async def relay(r,w):
 try:
  while b:=await r.read(65536):w.write(b);await w.drain()
 except ConnectionError:pass
 finally:w.close()
async def connected(r,w,port):
 ur,uw=await asyncio.open_connection(a.upstream,port)
 await asyncio.gather(relay(r,uw),relay(ur,w))
async def main():
 servers=[]
 for port in [18901,18902,18911,18912]:
  servers.append(await asyncio.start_server(lambda r,w,p=port:connected(r,w,p),'127.0.0.1',port))
 print('Matched loopback benchmark proxies ready',flush=True)
 await asyncio.Future()
asyncio.run(main())
