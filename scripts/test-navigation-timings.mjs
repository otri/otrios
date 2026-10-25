// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {NavigationTimings,createTimingDiagnostics,eventTimestamp} from '../web/canvas/navigation-timings.mjs';
const origin=1700000000000;
function fixture(){let time=0;const performance={timeOrigin:origin,now:()=>time},reports=[],timings=new NavigationTimings(performance,data=>reports.push(data));return {performance,timings,reports,at(t){time=t;}};}
test('disabled timings allocate no UI and touch no browser APIs',()=>{
  assert.equal(createTimingDiagnostics({location:{href:'https://example.com/input-lab'},get performance(){throw Error('disabled');},get document(){throw Error('disabled');}}),null);
});
test('navigation phases expose dispatch stalls and only complete after drawing and next RAF',()=>{
  const {timings,reports,at}=fixture();at(100);const t=timings.input({timeStamp:90},origin+60);at(2100);timings.dispatched(t);at(2120);timings.sent({...t,epoch:4,workerStarted:origin+2110,mountQueued:origin+2115});
  const frame={ui:'app:test/ui',screen:'test',epoch:4,vertices:new Float32Array(),resources:{ui_cache_hits:2},timing:{...t,epoch:4,workerStarted:origin+2110,mountQueued:origin+2115,receivedAt:origin+2220,workerReceived:origin+2230,qualified:origin+2240,frameReady:origin+2250,responseBytes:345}};
  at(2260);timings.accept(frame);assert.equal(reports.length,0);at(2270);const drawn=timings.drawn(4);assert.equal(reports.length,0);at(2280);timings.presented(drawn,4);
  const result=reports[0].samples[0];assert.equal(result.totalMs,2190);assert.equal(result.downToRenderMs,2220);assert.equal(result.ms.queue,2000);assert.equal(result.ms.responseWait,100);assert.equal(result.responseBytes,345);assert.equal(result.uiCacheHits,2);assert(!JSON.stringify(reports).includes('app:test/ui'));
});
test('stale or superseded frames never count as completed navigation',()=>{
  const {timings,reports,at}=fixture();const t=timings.input();at(10);timings.accept({vertices:new Float32Array(),ui:'private',epoch:2,timing:{...t,epoch:1}});assert.equal(timings.drawn(2),null);
  timings.accept({vertices:new Float32Array(),ui:'private',epoch:1,timing:{...t,epoch:1}});const sample=timings.drawn(1);timings.presented(sample,2);assert.equal(reports.length,0);
});
test('history and pending interactions stay bounded, and event timestamps support Safari epoch values',()=>{
  const {timings,performance,at}=fixture();at(100);assert.equal(eventTimestamp({timeStamp:origin+80},performance),origin+80);assert.equal(eventTimestamp({timeStamp:1000},performance),origin+100);
  for(let i=0;i<80;i++)timings.input();assert.equal(timings.pending.size,32);
  for(let i=0;i<40;i++){const t=timings.input();timings.accept({ui:'app',screen:'home',epoch:i,vertices:new Float32Array(),timing:{...t,epoch:i}});timings.presented(timings.drawn(i),i);}assert.equal(timings.snapshot().samples.length,30);
});
