// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';
import {DocumentTransfer} from '../web/canvas/document-transfer.mjs';
function workerContext(fields){
  if(fields.fake){
    const app=fields.fake;
    let dirty=false,inFlight=null,serial=0n;
    app.request_frame??=()=>{dirty=true;};
    app.begin_frame??=()=>{if(!dirty||inFlight!==null)return;dirty=false;return inFlight=++serial;};
    app.frame_presented??=s=>{if(s!==inFlight)return false;inFlight=null;return true;};
    app.prepare_frame??=()=>JSON.stringify({revisions:{content:app.text_generation?.()||1,placement:1,semantics:app.text_generation?.()||1,effects:1},order:['base']});
    app.scroll_text_input??=()=>false;app.text_inputs??=()=> '[]';app.effect_vertices??=()=>new Float32Array();app.chat_info??=()=> 'null';app.chat_messages??=()=> '[]';app.chat_outgoing??=()=>null;app.chat_disconnected??=()=>{};app.collection_outgoing??=()=>null;app.take_collection_revoked??=()=>false;app.screen??=()=> 'home';app.document_frame??=()=> 'null';
    for(const key of ['overlay_vertices','overlay_glyph_vertices','overlay_styles','overlay_views'])app[key]??=()=>new Float32Array();app.overlay_draws??=()=>new Uint32Array();
  }
  return vm.createContext({DocumentTransfer,...fields});
}
test('context loss pauses transfers and replay resends complete current resource dependencies',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[],fake={viewport_size:()=>[1280,800],path:()=> 'layout-lab/document',screen:()=> 'layout-document',epoch:()=>7,ui_ref:()=> 'app:layout-document/ui',state_ref:()=> 'app:layout-document/state',
    keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,presentation_kind:()=> 'text-document',motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}',
    text_generation:()=>1,text_views:()=>new Float32Array(),text_styles:()=>new Float32Array(),reflow_frame:()=>new Uint8Array(),vertices:()=>new Float32Array(),glyph_vertices:()=>new Float32Array(),draws:()=>new Uint32Array(),text:()=> 'Document',title:()=> 'Document',controls:()=>[],
    document_frame:()=>JSON.stringify({epoch:'1',em:24,loading:false,draws:[{version:'1',glyphs:1,view:[0,4,0,0,0,0,100,100]}]}),document_chunk_vertices:()=>new Float32Array(64)};
  const context=workerContext({fake,postMessage:m=>messages.push(m)});
  vm.runInContext(source,context);vm.runInContext('app=fake;publish()',context);
  const first=messages.find(m=>m.type==='frame');assert.equal(first.document.upserts.length,1);
  await context.onmessage({data:{type:'graphics-lost'}});
  await context.onmessage({data:{type:'frame-ack',serial:first.serial,documentToken:first.document.token}});
  vm.runInContext('publish();publish()',context);assert.equal(messages.filter(m=>m.type==='frame').length,1);
  await context.onmessage({data:{type:'document-replay'}});
  const replay=messages.filter(m=>m.type==='frame').at(-1);
  assert.equal(replay.graphicsGeneration,1);assert.equal(first.graphicsGeneration,0);assert.equal(replay.document.upserts.length,1);assert(replay.vertices);assert(replay.document.token>first.document.token);
});

test('app worker owns no sockets and scopes ink to the active app/transport epoch',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[];let received=0,active='',presentation='none',sockets=0;
  const fake={viewport_size:()=>[1280,800],ui_ref:()=>active,presentation_kind:()=>presentation,receive_ink:()=>{received++;return false;}};
  const context=workerContext({prepareUiUpdate:async()=>null,performance:{now:()=>0},postMessage:m=>messages.push(m),fake,WebSocket:class{constructor(){sockets++;}}});
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
    path:()=> 'layout-lab',screen:()=> 'layout-lab',epoch:()=>7,ui_ref:()=> 'app:layout-lab/ui',state_ref:()=> 'app:layout-lab/state',
    keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,presentation_kind:()=> 'none',motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}',
    text_generation:()=>1,text_views:()=>new Float32Array(),text_styles:()=>new Float32Array(),reflow_frame:()=>new Uint8Array(),vertices:()=>new Float32Array(),glyph_vertices:()=>new Float32Array(),draws:()=>new Uint32Array(),text:()=> 'Current app',title:()=> 'Layout Lab',controls:()=>[],
    tick:()=>false,take_sign_out:()=>false,activate:()=>{activations++;return true;}};
  const context=workerContext({prepareUiUpdate:async()=>null,URL,location:{href:'https://otrios.example/'},performance:{now:()=>0},postMessage:m=>messages.push(m),setTimeout(){},clearTimeout(){},fake});
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
    path:()=> '',screen:()=> 'home',epoch:()=>1,state_ref:()=>'',keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}'};
  const context=workerContext({prepareUiUpdate:async()=>null,URL,WebSocket:{OPEN:1},location:{href:'https://otrios.example/'},performance:{now:()=>0},postMessage:m=>messages.push(m),clearTimeout(){},fake,monitoringFake:{logout(){calls.push('logout');}}});
  vm.runInContext(source,context);vm.runInContext('app=fake;monitoring=monitoringFake',context);
  await context.onmessage({data:{type:'activate',id:12}});
  assert.deepEqual(calls,['clear',['session-controls',false],['navigate',''],'logout']);
  assert(messages.some(m=>m.type==='clear-private'));assert(messages.some(m=>m.type==='command-ack'));assert(!messages.some(m=>m.type==='error'));
});

test('release control waits for connected edits/gestures and handles supersession',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const {validRelease}=await import('../web/shared/session.mjs');
  const descriptor=c=>({id:c.repeat(64),web_entry:`/releases/${c.repeat(64)}/web/index.html`});
  const messages=[],timers=[];let edits=true,gesture=false;
  const fake={viewport_size:()=>[1280,800],reload_ready:()=>!edits&&!gesture,input_busy:()=>gesture,reset_connection(){}};
  const context=workerContext({prepareUiUpdate:async()=>null,URL,AbortController,location:{href:'https://otrios.example/'},
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
  const context=workerContext({prepareUiUpdate:async()=>null,postMessage:m=>messages.push(m),setTimeout,clearTimeout,fake,validRelease});
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
 const context=workerContext({prepareUiUpdate:async()=>null,fake,postMessage:m=>messages.push(m)});vm.runInContext(source,context);vm.runInContext('app=fake;transportReady=true;transportEpoch=2;outgoing()',context);
 assert.equal(messages.filter(m=>m.type==='state-send').length,4);
 await context.onmessage({data:{type:'state-send-ack',epoch:1}});assert.equal(messages.filter(m=>m.type==='state-send').length,4);
 await context.onmessage({data:{type:'state-send-ack',epoch:2}});assert.equal(messages.filter(m=>m.type==='state-send').length,5);
});

test('collection input revokes presentation while renderer credit is held and bounds outgoing handoff',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[];let packets=3,received=0,revoked=false;
  const fake={viewport_size:()=>[1280,800],outgoing:()=>null,collection_outgoing:()=>packets-->0?new Uint8Array([4]):null,
    receive_collection(){received++;revoked=true;return true;},take_collection_revoked(){const value=revoked;revoked=false;return value;},
    receive(){revoked=true;return true;},
    ui_ref:()=>'',state_ref:()=>'',path:()=>'',epoch:()=>7,keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,
    presentation_kind:()=> 'none',motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}'};
  const context=workerContext({fake,postMessage:m=>messages.push(m)});
  vm.runInContext(source,context);vm.runInContext('app=fake;transportReady=true;transportEpoch=2;publish();outgoing();outgoing()',context);
  const first=messages.find(m=>m.type==='frame');
  assert.equal(messages.filter(m=>m.type==='collection-send').length,1);
  await context.onmessage({data:{type:'collection-send-ack',epoch:1}});
  assert.equal(messages.filter(m=>m.type==='collection-send').length,1);
  await context.onmessage({data:{type:'collection-frame',epoch:1,bytes:new Uint8Array([1])}});
  assert.equal(received,0);assert(!messages.some(m=>m.type==='clear-private'));
  await context.onmessage({data:{type:'collection-frame',epoch:2,bytes:new Uint8Array([1])}});
  assert.equal(received,1);assert(messages.some(m=>m.type==='clear-private'));
  assert.equal(messages.filter(m=>m.type==='frame').length,1,'revocation bypasses held renderer credit');
  assert(messages.some(m=>m.type==='collection-frame-ack'&&m.epoch===2));
  await context.onmessage({data:{type:'collection-send-ack',epoch:2}});
  assert.equal(messages.filter(m=>m.type==='collection-send').length,2);
  await context.onmessage({data:{type:'frame-ack',serial:first.serial,documentToken:first.document.token}});
  assert.equal(messages.filter(m=>m.type==='frame').length,2);
  await context.onmessage({data:{type:'state-frame',epoch:2,bytes:new Uint8Array([1])}});
  assert.equal(messages.filter(m=>m.type==='clear-private').length,2,'leaving a chat mount also clears its handed-off presentation');
  assert.equal(messages.filter(m=>m.type==='frame').length,2);
  assert(!messages.some(m=>m.type==='error'));
});

test('navigation delegates destination paths to the shared runtime',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const calls=[],fake={auth_required:()=>false,ui_ref:()=> 'app:launcher/ui',screen:()=> 'launcher',navigate:id=>calls.push(id)};
  const context=workerContext({fake});vm.runInContext(source,context);vm.runInContext('app=fake;navigate("launcher");navigate("example-app");navigate(null)',context);
  assert.deepEqual(calls,['launcher','example-app','']);
});


test('timing traces follow the mount generation and ignore earlier control packets',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const {timestamp}=await import('../web/canvas/navigation-timings.mjs');
  const messages=[];let pending=false,epoch=1,packets=0;
  const fake={viewport_size:()=>[1280,800],tick:()=>false,activate(){pending=true;packets=2;return true;},take_sign_out:()=>false,
    pending_mount_epoch:()=>pending?2:0,outgoing(){if(!packets)return null;if(--packets===0)pending=false;return new Uint8Array([packets]);},
    receive(){epoch=2;return true;},rejected_mount_pending:()=>false,
    path:()=> 'layout-lab',screen:()=> 'layout-lab',epoch:()=>epoch,ui_ref:()=> 'app:layout-lab/ui',state_ref:()=> 'state',
    keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,presentation_kind:()=> 'none',motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}',
    text_generation:()=>1,text_views:()=>new Float32Array(),text_styles:()=>new Float32Array(),reflow_frame:()=>new Uint8Array(),vertices:()=>new Float32Array(),glyph_vertices:()=>new Float32Array(),draws:()=>new Uint32Array(),text:()=> 'App',title:()=> 'App',controls:()=>[]};
  let time=100;const context=workerContext({fake,timestamp,performance:{timeOrigin:1700000000000,now:()=>time++},Date,postMessage:m=>messages.push(m)});
  vm.runInContext(source,context);vm.runInContext('app=fake;timings=true;transportReady=true;transportEpoch=1',context);
  await context.onmessage({data:{type:'activate',id:12,timing:{id:7}}});
  const sent=messages.filter(m=>m.type==='state-send');assert.equal(sent.length,2);assert.equal(sent[0].timing,undefined);assert.equal(sent[1].timing.id,7);assert.equal(sent[1].timing.epoch,2);
  await context.onmessage({data:{type:'frame-ack',serial:messages.filter(m=>m.type==='frame').at(-1).serial}});
  await context.onmessage({data:{type:'state-frame',epoch:1,bytes:new Uint8Array([1,2,3]),receivedAt:1700000000110}});
  const frame=messages.filter(m=>m.type==='frame').at(-1);assert.equal(frame.epoch,2);assert.equal(frame.timing.id,7);assert.equal(frame.timing.responseBytes,3);assert(!messages.some(m=>m.type==='error'));
});

test('pointer batches preserve sample spacing across worker clocks and reject stale viewport batches',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const seen=[],messages=[],fake={ui_ref:()=> 'app:input/ui',epoch:()=>7,tick:()=>false,take_sign_out:()=>false,pointer:(...args)=>{seen.push(args);return false;}};
  const context=workerContext({fake,performance:{timeOrigin:1000,now:()=>500},postMessage:m=>messages.push(m)});
  vm.runInContext(source,context);vm.runInContext('app=fake',context);
  await context.onmessage({data:{type:'pointer-batch',epoch:7,viewportRevision:0,samples:[[1,9,10,20,1100],[1,9,12,23,1120]]}});
  assert.deepEqual(seen.map(p=>p.at(-1)),[100,120]);
  await context.onmessage({data:{type:'pointer-batch',epoch:7,viewportRevision:3,samples:[[1,9,15,25,1140]]}});
  assert.equal(seen.length,2);assert(!messages.some(m=>m.type==='error'));
});

test('renderer credit coalesces before Wasm assembly and effects do not recopy UI or semantics',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[];let prepared=0,meshes=0,semantics=0,effect=1;
  const fake={ui_ref:()=> 'app:chat/ui',screen:()=> 'chat',path:()=> 'chat',epoch:()=>1,state_ref:()=> 'state',viewport_size:()=>[1280,800],
    keyboard_visible:()=>false,editing:()=>false,auth_required:()=>false,presentation_kind:()=> 'chat',motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}',
    prepare_frame(){prepared++;return JSON.stringify({revisions:{content:1,placement:1,semantics:1,effects:effect},order:['base','effects']});},
    text_views:()=>new Float32Array(),text_styles:()=>new Float32Array(),reflow_frame:()=>new Uint8Array(),
    vertices(){meshes++;return new Float32Array();},glyph_vertices:()=>new Float32Array(),draws:()=>new Uint32Array(),text:()=> 'Chat',title:()=> 'Chat',controls(){semantics++;return [];}};
  const context=workerContext({fake,postMessage:m=>messages.push(m)});vm.runInContext(source,context);vm.runInContext('app=fake;publish()',context);
  const first=messages.find(m=>m.type==='frame');
  for(let i=0;i<100;i++){effect++;vm.runInContext('publish()',context);}
  assert.equal(prepared,1);assert.equal(meshes,1);assert.equal(semantics,1);
  await context.onmessage({data:{type:'frame-ack',serial:first.serial+1n,documentToken:first.document.token}});
  assert.equal(prepared,1);
  await context.onmessage({data:{type:'frame-ack',serial:first.serial,documentToken:first.document.token}});
  const latest=messages.filter(m=>m.type==='frame').at(-1);
  assert.equal(prepared,2);assert.equal(meshes,1);assert.equal(semantics,1);
  assert(latest.effects);assert.equal(latest.vertices,undefined);assert.equal(latest.controls,undefined);assert.equal(latest.textViews,undefined);
  assert.equal(messages.filter(m=>m.type==='frame').length,2);
  await context.onmessage({data:{type:'frame-ack',serial:first.serial,documentToken:first.document.token}});
  assert.equal(prepared,2);
});

test('browser input acknowledgement reconciles rejected edits even when scene semantics stay unchanged',async()=>{
  const source=(await readFile(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
  const messages=[],fake={ui_ref:()=> 'app:chat/ui',screen:()=> 'chat',path:()=> 'chat',epoch:()=>1,state_ref:()=> 'state',viewport_size:()=>[1280,800],
    keyboard_visible:()=>false,editing:()=>true,auth_required:()=>false,presentation_kind:()=> 'chat',motion_intent:()=>[],stress_updates:()=>0,resource_stats:()=>'{}',
    text_views:()=>new Float32Array(),text_styles:()=>new Float32Array(),reflow_frame:()=>new Uint8Array(),vertices:()=>new Float32Array(),glyph_vertices:()=>new Float32Array(),draws:()=>new Uint32Array(),text:()=> 'Chat',title:()=> 'Chat',controls:()=>[],tick:()=>false,take_sign_out:()=>false,
    text_inputs:()=>JSON.stringify([{id:42,value:'bounded'}]),replace_text_input:()=>false,input_busy:()=>false,reload_ready:()=>true};
  const context=workerContext({fake,performance:{now:()=>100},postMessage:m=>messages.push(m)});vm.runInContext(source,context);vm.runInContext('app=fake;publish()',context);
  const first=messages.find(m=>m.type==='frame');
  await context.onmessage({data:{type:'text-input',epoch:1,id:42,text:'invalid',cursor:7,anchor:7,inputRevision:9}});
  await context.onmessage({data:{type:'frame-ack',serial:first.serial,documentToken:first.document.token}});
  const latest=messages.filter(m=>m.type==='frame').at(-1);assert.equal(latest.inputRevision,9);assert.equal(latest.inputs[0].value,'bounded');assert.equal(latest.vertices,undefined);
  await context.onmessage({data:{type:'text-composition',epoch:1,id:42,active:true,inputRevision:10}});
  assert.equal(vm.runInContext('reloadReady()',context),false,'unfinished native IME blocks hot replacement');
  await context.onmessage({data:{type:'text-composition',epoch:1,id:42,active:false,inputRevision:11}});
  assert.equal(vm.runInContext('reloadReady()',context),true);assert(!messages.some(m=>m.type==='error'));
});
