// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {PointerSamples,sampleTime} from '../web/canvas/touch-input.mjs';
const convert=e=>({epoch:7,viewportRevision:2,args:[1,e.pointerId,e.clientX,0,e.timeStamp]});
test('worker batches retain coalesced source timing once, independently of delivery time',()=>{
  const q=new PointerSamples();q.add({getCoalescedEvents:()=>[10,18,26].map(t=>({pointerId:1,clientX:t,timeStamp:t}))},convert);
  const batch=q.take(1);assert.equal(batch.type,'pointer-batch');assert.deepEqual(batch.samples.map(p=>p[4]),[10,18,26]);assert.equal(q.size,0);
});
test('stalled consumers keep at most 200 ms and 64 points without mixing contacts',()=>{
  const q=new PointerSamples();for(let t=0;t<1000;t++)q.add({pointerId:1,clientX:t,timeStamp:t},convert);
  q.add({pointerId:2,clientX:2,timeStamp:999},convert);
  const one=q.take(1);assert(one.samples.length<=64);assert(one.samples[0][4]>=799);assert(one.samples[0][4]<=803);assert.equal(one.samples.at(-1)[2],999);
  assert.equal(q.take(2).samples.length,1);q.clear();assert.equal(q.size,0);
});
test('cross-realm timestamps use the same absolute monotonic clock and reject future times',()=>{
  const clock={timeOrigin:1700000000000,now:()=>200};
  assert.equal(sampleTime({timeStamp:123},clock),1700000000123);
  assert.equal(sampleTime({timeStamp:1700000000123},clock),1700000000123);
  assert.equal(sampleTime({timeStamp:500},clock),1700000000200);
});
