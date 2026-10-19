// SPDX-License-Identifier: Apache-2.0
// The state authority can announce code before this code host has replicated it.
// Keep the running runtime/state session until the host's atomic pointer is ready.
export function validRelease(r) {
  return r && /^[0-9a-f]{64}$/.test(r.id) && r.web_entry===`/releases/${r.id}/web/index.html`;
}
// The code host is independent of the state protocol, including during cutovers.
export async function readRelease({signal}={}) {
  const response=await fetch('/release',{cache:'no-store',credentials:'omit',redirect:'error',signal});
  if(!response.ok)throw Error('Code host not ready');
  const reader=response.body.getReader(),parts=[];let size=0;
  try {
    for(;;){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;if(size>8192)throw Error('Oversized release descriptor');parts.push(value);}
  } catch(e){await reader.cancel().catch(()=>{});throw e;}
  const bytes=new Uint8Array(size);let at=0;for(const part of parts){bytes.set(part,at);at+=part.length;}
  const release=JSON.parse(new TextDecoder().decode(bytes));
  if(!validRelease(release))throw Error('Invalid release descriptor');
  return release;
}
export function watchReleases(onRelease,{
  read=readRelease,intervalMs=5000,timeoutMs=5000,schedule=setTimeout,cancel=clearTimeout,
}={}) {
  let stopped=false,timer,controller,last;
  async function poll(){
    const request=new AbortController();controller=request;
    const deadline=schedule(()=>request.abort(),timeoutMs);
    try{
      const release=await read({signal:request.signal});
      if(!stopped&&!request.signal.aborted&&validRelease(release)&&release.id!==last){last=release.id;onRelease(release);}
    }catch{/* A missing code host preserves the current runtime and retries. */}
    finally{cancel(deadline);if(!stopped)timer=schedule(poll,intervalMs);}
  }
  void poll();
  return ()=>{stopped=true;cancel(timer);controller?.abort();};
}
export async function awaitRelease(target, {
  isCurrent=()=>true, signal,
  read=async()=>{
    const response=await fetch('/release',{cache:'no-store',credentials:'same-origin',signal});
    if(!response.ok)throw Error('Code host not ready');return response.json();
  },
  wait=()=>new Promise(resolve=>setTimeout(resolve,500)),
}={}) {
  if(!validRelease(target))throw Error('Invalid release descriptor');
  while(isCurrent()&&!signal?.aborted) {
    try {
      const ready=await read();
      if(isCurrent()&&!signal?.aborted&&validRelease(ready)&&ready.id===target.id)return true;
    } catch { /* Unavailable/unauthorized host: preserve the existing runtime. */ }
    if(isCurrent()&&!signal?.aborted)await wait();
  }
  return false;
}
