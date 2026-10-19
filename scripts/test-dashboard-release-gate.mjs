// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {watchReleases,validRelease} from '../web/shared/release-gate.mjs';
const release=c=>({id:c.repeat(64),web_entry:`/releases/${c.repeat(64)}/web/index.html`,sync_protocol:'future-crdt-v99'});
function rig(){
  const peers=[],timers=new Map(),seen=[];let serial=0;
  class Peer {constructor(url){this.url=String(url);this.sent=[];peers.push(this);}send(text){this.sent.push(JSON.parse(text));}close(){this.closed=true;this.onclose?.();}event(type,r,version=1){this.onmessage({data:JSON.stringify({type,version,release:r,refresh:true})});}}
  const stop=watchReleases(r=>seen.push(r),{open:url=>new Peer(url),base:'https://otrios.example/releases/'+ 'a'.repeat(64)+'/web/index.html',schedule:(fn,ms)=>{timers.set(++serial,{fn,ms});return serial;},cancel:id=>timers.delete(id)});
  return {peers,timers,seen,stop};
}
test('connect negotiates stable hello and accepts a hash for an unknown CRDT protocol',()=>{
  const r=rig(),p=r.peers[0];p.onopen();assert.equal(p.url,'wss://otrios.example/release/events');
  assert.deepEqual(p.sent,[{type:'hello',version:1,release:'a'.repeat(64),state_protocol:'otrios-canvas-sync-v2'}]);
  p.event('hello',release('a'));p.event('upgrade',release('b'));p.event('upgrade',release('b'));
  assert.deepEqual(r.seen,[release('a'),release('b')]);assert.equal(r.timers.size,0,'healthy connections perform no polling');r.stop();
});
test('reconnect hello recovers a missed upgrade and late old-socket events are ignored',()=>{
  const r=rig(),old=r.peers[0];old.onopen();old.event('hello',release('a'));old.close();
  const [id,t]=[...r.timers][0];assert.equal(t.ms,250);r.timers.delete(id);t.fn();const next=r.peers[1];next.onopen();
  old.event('upgrade',release('c'));next.event('hello',release('b'));assert.deepEqual(r.seen,[release('a'),release('b')]);r.stop();assert.equal(r.timers.size,0);
});
test('invalid or oversized envelopes and upgrade-before-hello cannot navigate',()=>{
  for(const send of [p=>p.event('upgrade',release('a')),p=>p.event('hello',{...release('a'),web_entry:'https://other.example/code'}),p=>p.event('hello',release('a'),2),p=>p.onmessage({data:' '.repeat(8193)})]){
    const r=rig();send(r.peers[0]);assert(r.peers[0].closed);assert.equal(r.seen.length,0);r.stop();
  }
  assert(!validRelease({id:'a'.repeat(64),web_entry:'/bad'}));
});
test('missing hello times out and stop prevents reconnect or late upgrade',()=>{
  const r=rig(),p=r.peers[0];const [id,t]=[...r.timers][0];assert.equal(t.ms,5000);r.timers.delete(id);t.fn();assert(p.closed);
  r.stop();assert.equal(r.timers.size,0);p.event('hello',release('b'));assert.equal(r.seen.length,0);
});

test('WebSocket construction failure retries without preventing app bootstrap',()=>{
  const timers=new Map();let attempts=0,serial=0;
  const stop=watchReleases(()=>{}, {base:'https://otrios.example',open:()=>{attempts++;throw Error('offline');},schedule:(fn,ms)=>{timers.set(++serial,{fn,ms});return serial;},cancel:id=>timers.delete(id)});
  assert.equal(attempts,1);const [id,t]=[...timers][0];assert.equal(t.ms,250);timers.delete(id);t.fn();assert.equal(attempts,2);stop();assert.equal(timers.size,0);
});
