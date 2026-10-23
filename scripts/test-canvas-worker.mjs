// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

test('app worker owns no sockets and scopes ink to the active app/transport epoch',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[];let received=0,active='',presentation='none',sockets=0;
  const fake={viewport_size:()=>[1280,800],ui_ref:()=>active,presentation_kind:()=>presentation,receive_ink:()=>{received++;return false;}};
  const context=vm.createContext({performance:{now:()=>0},postMessage:m=>messages.push(m),fake,WebSocket:class{constructor(){sockets++;}}});
  vm.runInContext(source,context);vm.runInContext('app=fake;transportReady=true;transportEpoch=1;connect();transportReady=true;syncInk()',context);
  active='app:component-playground/ui';presentation='touch-pad';vm.runInContext('syncInk();syncInk()',context);
  assert.equal(messages.filter(m=>m.type==='ink-subscribe'&&m.active).length,1);
  await context.onmessage({data:{type:'ink-frame',bytes:new Uint8Array([1]),epoch:1}});assert.equal(received,1);
  active='app:home/ui';presentation='none';vm.runInContext('syncInk()',context);
  await context.onmessage({data:{type:'ink-frame',bytes:new Uint8Array([1]),epoch:1}});assert.equal(received,1);
  active='app:component-playground/ui';presentation='touch-pad';vm.runInContext('transportEpoch=2;syncInk()',context);
  await context.onmessage({data:{type:'ink-frame',bytes:new Uint8Array([1]),epoch:1}});assert.equal(received,1);assert.equal(sockets,0);
});

test('a rejected candidate sends Cancel and leaves the current app usable; corrupt active sync is fatal',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[];let rejected=true,cancel=true,activations=0;
  const fake={viewport_size:()=>[1280,800],reset_connection(){},receive(){throw Error('candidate rejected');},rejected_mount_pending:()=>rejected,
    outgoing(){if(cancel){cancel=false;rejected=false;return new Uint8Array([42]);}return null;},
    screen:()=> 'layout-lab',epoch:()=>7,ui_ref:()=> 'app:layout-lab/ui',state_ref:()=> 'app:layout-lab/state',
    keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,presentation_kind:()=> 'none',motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}',
    vertices:()=>new Float32Array(),glyph_vertices:()=>new Float32Array(),draws:()=>new Uint32Array(),text:()=> 'Current app',title:()=> 'Layout Lab',controls:()=>[],
    tick:()=>false,take_sign_out:()=>false,activate:()=>{activations++;return true;}};
  const context=vm.createContext({URL,location:{href:'https://otrios.example/'},performance:{now:()=>0},postMessage:m=>messages.push(m),setTimeout(){},clearTimeout(){},fake});
  vm.runInContext(source,context);vm.runInContext('app=fake;transportReady=true;transportEpoch=1',context);
  await context.onmessage({data:{type:'state-frame',bytes:new Uint8Array([1]),epoch:1}});
  assert.deepEqual([...messages.find(m=>m.type==='state-send').bytes],[42]);assert(!messages.some(m=>m.type==='transport-reconnect'));assert(!messages.some(m=>m.type==='error'||m.type==='clear-private'));
  assert.equal(messages.find(m=>m.type==='frame').ui,'app:layout-lab/ui');assert(messages.some(m=>m.type==='warning'));
  await context.onmessage({data:{type:'activate',epoch:7,id:12}});assert.equal(activations,1);assert(messages.some(m=>m.type==='command-ack'));
  await context.onmessage({data:{type:'state-frame',bytes:new Uint8Array([1]),epoch:1}});assert(messages.some(m=>m.type==='transport-reconnect'));assert(messages.some(m=>m.type==='error'));
});

test('a Canvas sign-out action clears private pixels and revokes the browser session without syncing it',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[],calls=[];let requested=false,privateMount=true;
  const fake={viewport_size:()=>[1280,800],tick:()=>false,ui_ref:()=>privateMount?'app:monitoring/ui':'',presentation_kind:()=> 'none',
    private:()=>privateMount,clear_private(){calls.push('clear');privateMount=false;},
    set_session_controls(value){calls.push(['session-controls',value]);},navigate:screen=>calls.push(['navigate',screen]),
    activate(){requested=true;return true;},take_sign_out(){const value=requested;requested=false;return value;},
    screen:()=> 'home',epoch:()=>1,state_ref:()=>'',keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}'};
  const context=vm.createContext({URL,WebSocket:{OPEN:1},location:{href:'https://otrios.example/'},performance:{now:()=>0},postMessage:m=>messages.push(m),clearTimeout(){},fake,monitoringFake:{logout(){calls.push('logout');}}});
  vm.runInContext(source,context);vm.runInContext('app=fake;monitoring=monitoringFake',context);
  await context.onmessage({data:{type:'activate',id:12}});
  assert.deepEqual(calls,['clear',['session-controls',false],['navigate','home'],'logout']);
  assert(messages.some(m=>m.type==='clear-private'));assert(messages.some(m=>m.type==='command-ack'));assert(!messages.some(m=>m.type==='error'));
});

test('release control waits for connected edits/gestures and handles supersession',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const {validRelease}=await import('../web/shared/session.mjs');
  const descriptor=c=>({id:c.repeat(64),web_entry:`/releases/${c.repeat(64)}/web/index.html`});
  const messages=[],timers=[];let edits=true,gesture=false;
  const fake={viewport_size:()=>[1280,800],reload_ready:()=>!edits&&!gesture,input_busy:()=>gesture,reset_connection(){}};
  const context=vm.createContext({URL,AbortController,location:{href:'https://otrios.example/'},
    postMessage:m=>messages.push(m),setTimeout:fn=>timers.push(fn),clearTimeout(){},fake,validRelease});
  vm.runInContext(source,context);vm.runInContext(`app=fake;started=true;activeRelease='${'a'.repeat(64)}';transportReady=true;stateOnline=true`,context);
  await context.onmessage({data:{type:'release-discovered',release:descriptor('b')}});
  const settle=async()=>{for(let i=0;i<6;i++)await Promise.resolve();};await settle();
  assert(!messages.some(m=>m.type==='release-candidate'));
  edits=false;gesture=true;timers.shift()();await settle();assert(!messages.some(m=>m.type==='release-candidate'));
  gesture=false;timers.shift()();await settle();assert.equal(messages.find(m=>m.type==='release-candidate').entry,descriptor('b').web_entry);
  // Input can become busy between the candidate and the final commit.
  edits=true;const commit=context.onmessage({data:{type:'release-commit',entry:descriptor('b').web_entry}});
  await settle();assert(!messages.some(m=>m.type==='release'));
  await context.onmessage({data:{type:'release-discovered',release:descriptor('a')}});
  timers.shift()();await commit;assert(messages.some(m=>m.type==='release-retry'));assert(!messages.some(m=>m.type==='release'));
  edits=false;await context.onmessage({data:{type:'release-discovered',release:descriptor('b')}});await settle();
  await context.onmessage({data:{type:'release-commit',entry:descriptor('b').web_entry}});
  assert.equal(messages.filter(m=>m.type==='release').length,1);assert.equal(vm.runInContext('transportReady',context),false);
});


test('disconnected RAM edits do not strand a release upgrade',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const {validRelease}=await import('../web/shared/session.mjs');const messages=[];
  const fake={viewport_size:()=>[1280,800],reload_ready:()=>false,input_busy:()=>false};
  const context=vm.createContext({postMessage:m=>messages.push(m),setTimeout,clearTimeout,fake,validRelease});
  vm.runInContext(source,context);vm.runInContext(`app=fake;started=true;activeRelease='${'a'.repeat(64)}';stateOnline=false`,context);
  const entry=`/releases/${'b'.repeat(64)}/web/index.html`;
  await context.onmessage({data:{type:'release-discovered',release:{id:'b'.repeat(64),web_entry:entry}}});
  for(let i=0;i<6;i++)await Promise.resolve();
  assert(messages.some(m=>m.type==='release-candidate'));
  await context.onmessage({data:{type:'release-commit',entry}});
  assert(messages.some(m=>m.type==='release'&&m.entry===entry));
});

test('worker state output uses a bounded credit window and ignores stale acknowledgements',async()=>{
 const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');const messages=[];let packets=10;
 const fake={viewport_size:()=>[1280,800],outgoing:()=>packets-->0?new Uint8Array([4]):null};
 const context=vm.createContext({fake,postMessage:m=>messages.push(m)});vm.runInContext(source,context);vm.runInContext('app=fake;transportReady=true;transportEpoch=2;outgoing()',context);
 assert.equal(messages.filter(m=>m.type==='state-send').length,4);
 await context.onmessage({data:{type:'state-send-ack',epoch:1}});assert.equal(messages.filter(m=>m.type==='state-send').length,4);
 await context.onmessage({data:{type:'state-send-ack',epoch:2}});assert.equal(messages.filter(m=>m.type==='state-send').length,5);
});
