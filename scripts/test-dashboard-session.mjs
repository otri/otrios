// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';import assert from 'node:assert/strict';
import {connectSession,validRelease} from '../web/shared/session.mjs';
const release=c=>({id:c.repeat(64),web_entry:`/releases/${c.repeat(64)}/web/index.html`,sync_protocol:'otrios-canvas-sync-v6'});
function rig({failOpen=false}={}){
 const peers=[],timers=new Map(),seen=[],state=[],ink=[],status=[],errors=[];let serial=0;
 class Peer {constructor(url){this.url=String(url);this.sent=[];this.bufferedAmount=0;peers.push(this);}send(data){this.sent.push(typeof data==='string'?JSON.parse(data):[...data]);}close(){this.closed=true;this.onclose?.();}event(type='hello',r=release('a'),extra={}){this.onmessage({data:JSON.stringify({type,version:1,release:r,compatible:true,state_available:true,...extra})});}binary(bytes){this.onmessage({data:new Uint8Array(bytes).buffer});}}
 const session=connectSession({open:url=>{if(failOpen)throw Error('offline');return new Peer(url);},base:'https://otrios.example/releases/'+ 'a'.repeat(64)+'/web/index.html',onRelease:r=>seen.push(r),onState:(b,e)=>state.push([[...b],e]),onInk:(b,e)=>ink.push([[...b],e]),onStatus:(v,e)=>status.push([v,e]),onError:e=>errors.push(String(e)),schedule:(fn,ms)=>{timers.set(++serial,{fn,ms});return serial;},cancel:id=>timers.delete(id)});
 return {peers,timers,seen,state,ink,status,errors,session};
}
test('one socket negotiates before binary traffic and multiplexes subscribed ink',()=>{
 const r=rig(),p=r.peers[0];p.onopen();assert.equal(p.url,'wss://otrios.example/session');
 assert.deepEqual(p.sent,[{type:'hello',version:1,release:'a'.repeat(64),state_protocol:'otrios-canvas-sync-v6',state:true}]);
 assert(!r.session.sendState(new Uint8Array([9])));p.event();assert(r.session.ready);
 assert(r.session.sendState(new Uint8Array([8,9])));assert.deepEqual(p.sent[1],[1,8,9]);p.binary([1,7]);assert.deepEqual(r.state,[[[7],1]]);
 r.session.subscribeInk(true);assert.deepEqual(p.sent[2],{type:'ink',enabled:true});r.session.sendInk(new Uint8Array([6]));assert.deepEqual(p.sent[3],[2,6]);p.binary([2,5]);assert.deepEqual(r.ink,[[[5],1]]);
 r.session.subscribeInk(false);p.binary([2,4]);assert.equal(r.ink.length,1);assert.equal(r.peers.length,1);assert.equal(r.timers.size,0,'healthy sessions never poll releases');r.session.stop();
});
test('unknown state protocol still offers code and never sends state',()=>{
 const r=rig(),p=r.peers[0];p.event('hello',release('b'),{compatible:false});assert.deepEqual(r.seen,[release('b')]);assert(!r.session.ready);assert(!r.session.sendState(new Uint8Array([9])));
 p.event('upgrade',release('c'),{compatible:false});assert.equal(r.seen.at(-1).id,release('c').id);r.session.stop();
});
test('state unavailable during bootstrap does not prevent an upgrade',()=>{
 const r=rig(),p=r.peers[0];p.event('hello',release('a'),{state_available:false});assert(!r.session.ready);
 p.event('upgrade',release('b'),{state_available:false});assert.deepEqual(r.seen,[release('a'),release('b')]);
 p.onmessage({data:JSON.stringify({type:'state-status',version:1,available:true})});assert(r.session.ready);assert.deepEqual(r.status,[[true,1]]);r.session.stop();
});
test('reconnect hello recovers missed release; stale peers and queued writes cannot cross epochs',()=>{
 const r=rig(),old=r.peers[0];old.event();const epoch=r.session.epoch;old.close();
 const [id,t]=[...r.timers][0];assert.equal(t.ms,250);r.timers.delete(id);t.fn();const next=r.peers[1];next.onopen();next.event('hello',release('b'));old.event('upgrade',release('c'));old.binary([1,3]);
 assert.deepEqual(r.seen,[release('a'),release('b')]);assert.equal(r.state.length,0);assert(!r.session.sendState(new Uint8Array([9]),epoch));assert(r.session.sendState(new Uint8Array([9]),r.session.epoch));r.session.stop();
});
test('invalid control/channel frames are rejected before CRDT delivery',()=>{
 for(const trigger of [p=>p.binary([1,0]),p=>p.event('upgrade'),p=>p.event('hello',{...release('a'),web_entry:'https://other.example/code'}),p=>p.event('hello',release('a'),{version:9}),p=>p.onmessage({data:' '.repeat(8193)}),p=>{p.event();p.binary([4,0]);},p=>{p.event('hello',release('a'),{compatible:false});p.binary([1,0]);}]){
  const r=rig();trigger(r.peers[0]);assert(r.peers[0].closed);assert.equal(r.state.length,0);r.session.stop();
 }
 assert(!validRelease({id:'a'.repeat(64),web_entry:'/bad'}));
});
test('timeout, constructor failure and stop have bounded reconnect lifecycle',()=>{
 const r=rig(),p=r.peers[0];const [id,t]=[...r.timers][0];assert.equal(t.ms,5000);r.timers.delete(id);t.fn();assert(p.closed);r.session.stop();assert.equal(r.timers.size,0);p.event('hello',release('b'));assert.equal(r.seen.length,0);
 const f=rig({failOpen:true});assert.equal(f.peers.length,0);assert.equal(f.timers.size,1);f.session.stop();assert.equal(f.timers.size,0);
});
test('dev hello may have no release; cookie refresh reconnects the same session owner',()=>{
 const r=rig();r.peers[0].event('hello',null);assert(r.session.ready);r.session.reconnect();assert(r.peers[0].closed);assert.equal(r.peers.length,2);r.peers[1].event();assert.equal(r.session.epoch,2);r.session.stop();
});
test('worker mailboxes bound queued data and reject acknowledgements from an old connection',async()=>{
 const {bridgeWorker}=await import('../web/shared/session.mjs');const messages=[];let resets=0;
 const session={epoch:1,ready:true,reconnect(){resets++;this.epoch++;},sendState(){},sendInk(){},subscribeInk(){}};
 const bridge=bridgeWorker(session,m=>messages.push(m));bridge.status(true,1);
 for(let i=0;i<9;i++)bridge.receive('state',new Uint8Array([i]),1);assert.equal(resets,1);assert.equal(messages.filter(m=>m.type==='state-frame').length,8);
 bridge.status(true,2);for(let i=0;i<20;i++)bridge.receive('ink',new Uint8Array([i]),2);assert.equal(messages.filter(m=>m.type==='ink-frame').length,8);
 bridge.handle({type:'ink-frame-ack',epoch:1});bridge.receive('ink',new Uint8Array([0]),2);assert.equal(messages.filter(m=>m.type==='ink-frame').length,8);
 bridge.handle({type:'ink-frame-ack',epoch:2});bridge.receive('ink',new Uint8Array([0]),2);assert.equal(messages.filter(m=>m.type==='ink-frame').length,9);
});
test('back-forward cache resumes one owner without reusing connection epochs',()=>{
 const r=rig();r.peers[0].event();const epoch=r.session.epoch;r.session.stop();r.session.reconnect();assert.equal(r.peers.length,1);
 r.session.resume();r.peers[1].event();assert.equal(r.session.epoch,epoch+1);assert(!r.session.sendState(new Uint8Array([3]),epoch));r.session.stop();
});
test('browser and native/server share the same declared codec version',async()=>{
 const {readFile}=await import('node:fs/promises');const {STATE_PROTOCOL,CONTROL_VERSION}=await import('../web/shared/session.mjs');
 const source=await readFile(new URL('../crates/otrios-interfaces/src/session.rs',import.meta.url),'utf8');
 assert.equal(CONTROL_VERSION,1);assert.match(source,/VERSION: u32 = 1/);
 assert.equal(STATE_PROTOCOL,source.match(/STATE_PROTOCOL: &str = "([^"]+)"/)[1]);
});

test('chat has its own bounded channel and mailbox credit',async()=>{
 const {bridgeWorker}=await import('../web/shared/session.mjs');
 const r=rig();r.peers[0].event();assert(r.session.sendChat(new Uint8Array([8,9])));assert.deepEqual(r.peers[0].sent[0],[3,8,9]);
 assert.throws(()=>r.session.sendChat(new Uint8Array(8193)),/exceeds limit/);r.session.stop();
 let resets=0;const messages=[],session={epoch:1,ready:true,reconnect(){resets++;},sendChat(){}};
 const bridge=bridgeWorker(session,m=>messages.push(m));bridge.status(true,1);
 for(let i=0;i<8;i++)bridge.receive('chat',new Uint8Array([i]),1);
 bridge.handle({type:'chat-frame-ack',epoch:0});bridge.receive('chat',new Uint8Array([9]),1);assert.equal(resets,1);
 bridge.handle({type:'chat-frame-ack',epoch:1});bridge.receive('chat',new Uint8Array([10]),1);assert.equal(messages.filter(m=>m.type==='chat-frame').length,9);
});
