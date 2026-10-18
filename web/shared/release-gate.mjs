// SPDX-License-Identifier: Apache-2.0
// The state authority can announce code before this code host has replicated it.
// Keep the running runtime/state session until the host's atomic pointer is ready.
export function validRelease(r) {
  return r && /^[0-9a-f]{64}$/.test(r.id) && r.web_entry===`/releases/${r.id}/web/index.html`;
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
