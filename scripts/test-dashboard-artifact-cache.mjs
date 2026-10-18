// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {ArtifactCache,eligible,budget,victims,boundedBody,MAX_BYTES,MAX_ITEM} from '../web/dashboard/artifact-cache.mjs';
const origin='https://otrios.example',release=`${origin}/releases/${'a'.repeat(64)}/web/`;
test('only immutable public code/blob paths can enter the cache',()=>{
  for(const p of ['app.js','artifact-cache.mjs','pkg/otrios_dashboard_bg.wasm','fonts/Roboto-Regular.otq.zst'])assert(eligible(new Request(release+p),origin));
  assert(eligible(new Request(`${origin}/objects/${'b'.repeat(64)}`),origin));
  for(const p of ['/sync','/demo/stream','/demo/ink','/monitoring/presentation','/auth/session','/release','/presentation'])assert(!eligible(new Request(origin+p),origin));
  assert(!eligible(new Request(release+'app.js',{headers:{Authorization:'private'}}),origin));
  assert(!eligible(new Request(release+'app.js',{headers:{Range:'bytes=0-1'}}),origin));
  assert(!eligible(new Request(release+'app.js?secret=1'),origin));
  assert(!eligible(new Request(release+'app.js',{method:'POST'}),origin));
  assert(!eligible(new Request(release+'app.js'),'https://other.example'));
});
test('web budget is capped at 512 MiB and shrinks with quota/headroom',()=>{
  assert.equal(budget(),MAX_BYTES);assert.equal(budget({quota:1000,usage:100}),250);
  assert.equal(budget({quota:1000,usage:890},20),30);assert.equal(budget({quota:1000,usage:1100}),0);
});
function harness(){const entries=new Map(),payloads=new Map();let time=0,quota=false;
  const index={all:async()=>[...entries.values()],put:async e=>entries.set(e.url,e),delete:async u=>entries.delete(u)};
  const cache={match:async u=>payloads.get(u)?.clone(),delete:async u=>payloads.delete(u),put:async(u,r)=>{if(quota)throw Error('QuotaExceededError');payloads.set(u,r.clone());}};
  const store=new ArtifactCache({cache,index,estimate:async()=>({quota:40,usage:0}),now:()=>++time});return {store,entries,payloads,fail:()=>quota=true};
}
test('LRU eviction respects exact stored bytes and touches on reads',async()=>{
  const h=harness();await h.store.put('a',new Response('1234'),4);await h.store.put('b',new Response('5678'),4);
  assert.equal(await(await h.store.match('a')).text(),'1234');
  assert(await h.store.put('c',new Response('9012'),4));assert.deepEqual([...h.payloads.keys()],['a','c']);
  assert([...h.entries.values()].reduce((n,e)=>n+e.bytes,0)<=10);
  assert(!await h.store.put('huge',new Response('huge'),MAX_ITEM+1));
});
test('quota failures leave no payload or reservation and requests still work',async()=>{
  const h=harness();h.fail();assert(!await h.store.put('a',new Response('1234'),4));assert.equal(h.entries.size,0);assert.equal(h.payloads.size,0);
});
test('oversized streaming objects are cancelled without unbounded buffering',async()=>{
  let cancelled=false;
  const stream=new ReadableStream({start(c){c.enqueue(new Uint8Array(11));},cancel(){cancelled=true;}});
  await assert.rejects(boundedBody(new Response(stream),10));assert(cancelled);
  const bytes=await boundedBody(new Response(new Uint8Array([0,255,1])));assert.deepEqual([...bytes],[0,255,1]);
});
test('protected entries cannot be evicted to falsely admit an oversized item',()=>{
  assert.deepEqual(victims([{url:'a',bytes:9,used:1}],10,2,'a'),{remove:[],fits:false});
});
