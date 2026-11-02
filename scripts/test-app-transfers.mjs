// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {AppTransfers,boundedBody,transferPath} from '../web/canvas/app-transfers.mjs';
const space='a'.repeat(32),collection='b'.repeat(32),schema='c'.repeat(64);
const app={type:'app',space},data={type:'data',dataset:{space,app:'counter',collection,schema}};
function fixture(options={}){
  const replies=[],downloads=[],calls=[];
  const host=new AppTransfers({origin:'https://otrios.example',chooseFile:async()=>new Blob(['code']),download:async(blob,name)=>downloads.push([await blob.text(),name]),copy:async text=>calls.push(['copy',text]),reply:(id,text)=>replies.push([id,text]),fetchImpl:async(...args)=>{calls.push(args);return new Response('{"revision":18446744073709551615}',{headers:{'Content-Type':'application/json'}});},...options});
  return {host,replies,downloads,calls};
}
test('transfer paths admit exact authority-owned targets only',()=>{
  assert.equal(transferPath(app),`/transfers/apps/${space}`);
  assert.equal(transferPath(data),`/transfers/data/${space}/counter/${collection}`);
  for(const target of [{type:'app',space:'../private'}, {...data,dataset:{...data.dataset,app:'counter/../../admin'}},{...data,dataset:{...data.dataset,schema:'bad'}}])assert.throws(()=>transferPath(target));
});
test('file and URL intake post only to this authority and preserve raw u64 reviews',async()=>{
  const {host,replies,calls}=fixture();
  await host.run({id:1,target:app,io:{type:'pick'}});
  assert.equal(calls[0][0].href,`https://otrios.example/transfers/apps/${space}`);
  assert.equal(calls[0][1].method,'POST');assert.equal(calls[0][1].credentials,'same-origin');assert.equal(calls[0][1].redirect,'error');assert.equal(calls[0][1].headers['X-OTriOS-Request'],'transfers');
  assert.equal(await calls[0][1].body.text(),'code');
  assert.equal(replies[0][1],'{"type":"app","review":{"revision":18446744073709551615}}');
  const url=`https://publisher.example/apps/counter/${schema}`;
  await host.run({id:2,target:app,io:{type:'url',url}});
  assert.equal(calls[1][0].origin,'https://otrios.example');assert.equal(calls[1][1].body,url);
});
test('fetch keeps the browser global as its receiver',async()=>{
  const {host,replies}=fixture({fetchImpl:async function(){assert.equal(this,globalThis);return new Response('{}',{headers:{'Content-Type':'application/json'}});}});
  await host.run({id:1,target:app,io:{type:'pick'}});
  assert.equal(JSON.parse(replies[0][1]).type,'app');
});
test('portable export completes bounded bytes before invoking download',async()=>{
  const {host,replies,downloads}=fixture({fetchImpl:async()=>new Response(new ReadableStream({start(c){c.enqueue(new TextEncoder().encode('complete'));c.enqueue(new TextEncoder().encode(' snapshot'));c.close();}}),{headers:{'Content-Type':'application/vnd.otrios.app-data+zip','Content-Length':'17'}})});
  await host.run({id:1,target:data,io:{type:'export'}});
  assert.deepEqual(downloads,[['complete snapshot','counter.otdata']]);
  assert.equal(JSON.parse(replies[0][1]).type,'exported');
});
test('oversized framing and partial responses never download; bounded reader cancels excess',async()=>{
  for(const response of [()=>new Response('partial',{headers:{'Content-Type':'application/vnd.otrios.app-data+zip','Content-Length':'100'}}),()=>new Response('bad',{headers:{'Content-Type':'text/plain'}}),()=>new Response('no',{status:403})]){
    const {host,replies,downloads}=fixture({fetchImpl:async()=>response()});
    await host.run({id:1,target:data,io:{type:'export'}});
    assert.equal(downloads.length,0);assert.equal(JSON.parse(replies[0][1]).type,'failed');
  }
  let cancelled=false;
  const response=new Response(new ReadableStream({start(c){c.enqueue(new Uint8Array(8));},cancel(){cancelled=true;}}));
  await assert.rejects(boundedBody(response,7),/byte budget/);assert(cancelled);
});
test('reset aborts prior actor input and never forwards its review or download',async()=>{
  let finish,signal;
  const {host,replies}=fixture({fetchImpl:async(_url,options)=>{signal=options.signal;return new Promise(resolve=>{finish=resolve;});}});
  const work=host.run({id:1,target:app,io:{type:'pick'}});
  while(!finish)await Promise.resolve();
  host.reset();assert(signal.aborted);
  finish(new Response('{}',{headers:{'Content-Type':'application/json'}}));await work;
  assert.equal(replies.length,0);assert.equal(host.active.size,0);
});
test('resetting a picker suppresses its file and a stale completion cannot clear a newer task',async()=>{
  const pickers=[];
  const {host,replies,calls}=fixture({chooseFile:()=>new Promise(resolve=>pickers.push(resolve))});
  const first=host.run({id:1,target:app,io:{type:'pick'}});
  host.reset();
  const second=host.run({id:1,target:app,io:{type:'pick'}});
  pickers[0](new Blob(['old']));await first;
  assert.equal(host.active.size,1);assert.equal(calls.length,0);assert.equal(replies.length,0);
  pickers[1](null);await second;assert.equal(host.active.size,0);assert.equal(JSON.parse(replies[0][1]).type,'cancelled');
});
test('clipboard accepts only an immutable app URL on the current host',async()=>{
  const {host,replies,calls}=fixture();
  await host.run({id:1,target:app,io:{type:'copy',text:`https://otrios.example/apps/counter/${schema}`}});
  assert.equal(calls.length,1);assert.equal(JSON.parse(replies[0][1]).type,'copied');
  for(const text of [`https://evil.example/apps/counter/${schema}`,`https://otrios.example/apps/counter/${schema}?token=secret`])await host.run({id:2,target:app,io:{type:'copy',text}});
  assert.equal(calls.length,1);assert(replies.slice(1).every(([,s])=>JSON.parse(s).type==='failed'));
});
