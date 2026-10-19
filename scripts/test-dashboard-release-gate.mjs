// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {awaitRelease,validRelease} from '../web/shared/release-gate.mjs';
const release=c=>({id:c.repeat(64),web_entry:`/releases/${c.repeat(64)}/web/index.html`});
test('lagging code host does not switch a running runtime/state session',async()=>{
  let reads=0,liveEdits=0,switched=false;
  const ready=awaitRelease(release('b'),{read:async()=>++reads<3?release('a'):release('b'),wait:async()=>{assert.equal(switched,false);liveEdits++;}}).then(v=>{switched=v;return v;});
  assert.equal(await ready,true);assert.equal(liveEdits,2);assert.equal(reads,3);
});
test('superseded release never switches after a late network response',async()=>{
  let current=true;
  assert.equal(await awaitRelease(release('a'),{isCurrent:()=>current,read:async()=>{current=false;return release('a');},wait:async()=>assert.fail('obsolete release must not retry')}),false);
});
test('temporary host failure preserves the session and retries',async()=>{
  let attempts=0;
  assert.equal(await awaitRelease(release('b'),{read:async()=>{if(++attempts===1)throw Error('offline');return release('b');},wait:async()=>{}}),true);
  assert.equal(attempts,2);
});
test('untrusted entry URLs and aborted requests cannot trigger navigation',async()=>{
  const invalid={...release('a'),web_entry:'https://unrelated.example/code'};
  assert.equal(validRelease(invalid),false);await assert.rejects(awaitRelease(invalid));
  const controller=new AbortController();controller.abort();
  assert.equal(await awaitRelease(release('a'),{signal:controller.signal,read:async()=>assert.fail('aborted request')}),false);
});

test('release discovery survives an absent state protocol, retries host errors and deduplicates',async()=>{
  const {watchReleases}=await import('../web/shared/release-gate.mjs');
  const timers=new Map(),seen=[];let serial=0,reads=0;
  const stop=watchReleases(r=>seen.push(r.id),{
    read:async()=>{reads++;if(reads===1)throw Error('host unavailable');return release(reads<4?'a':'b');},
    schedule:(fn,ms)=>{const id=++serial;timers.set(id,{fn,ms});return id;},cancel:id=>timers.delete(id),
  });
  const settle=async()=>{for(let i=0;i<5;i++)await Promise.resolve();};
  await settle();
  for(let i=0;i<3;i++){const [id,t]=[...timers].find(([,t])=>t.ms===5000);timers.delete(id);await t.fn();await settle();}
  assert.deepEqual(seen,['a'.repeat(64),'b'.repeat(64)]);assert.equal(reads,4);
  stop();assert.equal(timers.size,0);
});
test('stopped, timed-out and invalid discovery responses cannot offer upgrades',async()=>{
  const {watchReleases}=await import('../web/shared/release-gate.mjs');
  let resolveRead,abort;
  const timers=new Map();let serial=0;
  const stop=watchReleases(()=>assert.fail('late discovery'),{
    read:({signal})=>{abort=signal;return new Promise(resolve=>resolveRead=resolve);},
    schedule:fn=>{timers.set(++serial,fn);return serial;},cancel:id=>timers.delete(id),
  });
  stop();assert(abort.aborted);resolveRead(release('b'));await Promise.resolve();await Promise.resolve();assert.equal(timers.size,0);
  let deadline;
  const stopTimeout=watchReleases(()=>assert.fail('expired response'),{read:async()=>{deadline();return release('b');},schedule:fn=>(deadline=fn),cancel(){}});
  await Promise.resolve();await Promise.resolve();stopTimeout();
  const stopInvalid=watchReleases(()=>assert.fail('invalid entry'),{read:async()=>({...release('b'),web_entry:'https://other.example/code'}),schedule:()=>0,cancel(){}});
  await Promise.resolve();await Promise.resolve();stopInvalid();
});
test('descriptor reads are independent, uncached, same-origin and bounded',async()=>{
  const {readRelease}=await import('../web/shared/release-gate.mjs');
  const original=globalThis.fetch;
  try {
    globalThis.fetch=async(url,options)=>{assert.equal(url,'/release');assert.equal(options.cache,'no-store');assert.equal(options.redirect,'error');assert.equal(options.credentials,'omit');return new Response(JSON.stringify(release('c')));};
    assert.deepEqual(await readRelease(),release('c'));
    globalThis.fetch=async()=>new Response(' '.repeat(8193));await assert.rejects(readRelease(),/Oversized/);
    globalThis.fetch=async()=>new Response(JSON.stringify({...release('c'),web_entry:'/bad'}));await assert.rejects(readRelease(),/Invalid/);
  }finally{globalThis.fetch=original;}
});
