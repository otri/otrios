// SPDX-License-Identifier: Apache-2.0
import {ArtifactCache,CACHE,eligible,boundedBody} from './artifact-cache.mjs';
function metadata(){return new Promise((resolve,reject)=>{
  const open=indexedDB.open(CACHE,1);open.onupgradeneeded=()=>open.result.createObjectStore('entries',{keyPath:'url'});
  open.onerror=()=>reject(open.error);open.onsuccess=()=>{
    const db=open.result;db.onversionchange=()=>db.close();
    const op=(mode,method,value)=>new Promise((yes,no)=>{const tx=db.transaction('entries',mode),r=tx.objectStore('entries')[method](value);tx.oncomplete=()=>yes(r.result);tx.onerror=()=>no(tx.error);tx.onabort=()=>no(tx.error);});
    resolve({all:()=>op('readonly','getAll'),put:e=>op('readwrite','put',e),delete:url=>op('readwrite','delete',url)});
  };
});}
let storage;
const lock=work=>navigator.locks.request(CACHE,work);
function cache(){return storage??=lock(async()=>{
  const [cache,index]=await Promise.all([caches.open(CACHE),metadata()]);
  // Repair orphan payloads after browser eviction or an interrupted write.
  const known=new Set((await index.all()).map(e=>e.url));for(const r of await cache.keys())if(!known.has(r.url))await cache.delete(r);
  const store=new ArtifactCache({cache,index,lock,estimate:()=>navigator.storage.estimate()});await store.trim();return store;
});}
self.addEventListener('install',event=>event.waitUntil(self.skipWaiting()));
self.addEventListener('activate',event=>event.waitUntil(self.clients.claim()));
self.addEventListener('fetch',event=>{
  if(!eligible(event.request,self.location.origin))return;
  event.respondWith((async()=>{
    // These routes serve public immutable artifacts; never attach session cookies.
    let store;try{store=await cache();const hit=await store.match(event.request.url);if(hit)return hit;}catch{/* Storage is optional. */}
    const response=await fetch(new Request(event.request,{credentials:'omit'}));
    if(store&&response.status===200&&!response.redirected&&/\bpublic\b/.test(response.headers.get('cache-control')||'')&&/\bimmutable\b/.test(response.headers.get('cache-control')||'')){
      event.waitUntil((async()=>{try{
        const bytes=await boundedBody(response.clone());
        const u=new URL(event.request.url);
        if(u.pathname.startsWith('/objects/')){
          const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');
          if(digest!==u.pathname.split('/').pop())return;
        }
        const headers=new Headers(response.headers);headers.delete('content-encoding');headers.delete('transfer-encoding');headers.set('content-length',String(bytes.length));
        await store.put(event.request.url,new Response(bytes,{status:200,headers}),bytes.length);
      }catch{/* Quota, oversized payloads and unavailable storage never block rendering. */}})());
    }
    return response;
  })());
});
