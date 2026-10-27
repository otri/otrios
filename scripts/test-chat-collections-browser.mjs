// SPDX-License-Identifier: Apache-2.0
// Real browser traversal/traffic/resource qualification against disposable state.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
const args=process.argv.slice(2),arg=(name,fallback)=>{const i=args.indexOf(name);return i<0?fallback:args[i+1];};
const origin=arg('--url'),records=Number(arg('--records'));
assert(origin&&Number.isInteger(records)&&records>=64&&records<=50000,'Supply disposable --url and --records');
const output=resolve(arg('--output','target/chat-collections-browser'));await mkdir(output,{recursive:true});
const profile=await mkdtemp(join(tmpdir(),'otrios-collections-'));
const browser=spawn(arg('--chromium','chromium'),['--headless=new','--no-sandbox','--disable-dev-shm-usage','--disable-background-timer-throttling','--disable-renderer-backgrounding','--enable-unsafe-swiftshader','--use-angle=swiftshader','--remote-debugging-port=0','--user-data-dir='+profile,'--no-first-run','about:blank'],{stdio:['ignore','ignore','pipe']});
let socket;
try {
  const endpoint=await new Promise((resolve,reject)=>{let log='';const timer=setTimeout(()=>reject(Error('Chromium startup: '+log)),30000);browser.stderr.on('data',b=>{log+=b;const m=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(m){clearTimeout(timer);resolve(m[1]);}});browser.on('error',reject);});
  socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
  let serial=0,canary;const pending=new Map(),received=new Map(),sent=new Map(),hellos=new Map(),contexts=new Map(),workers=new Map(),leaks=[];
  socket.addEventListener('message',({data})=>{
    const m=JSON.parse(data);
    if(m.id){const task=pending.get(m.id);if(task){pending.delete(m.id);clearTimeout(task.timer);m.error?task.reject(Error(JSON.stringify(m.error))):task.resolve(m.result);}return;}
    if(m.method==='Target.attachedToTarget'&&m.params.targetInfo.type==='worker')workers.set(m.sessionId,m.params.sessionId);
    if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===1){try{const h=JSON.parse(m.params.response.payloadData);if(h.type==='hello')hellos.set(m.sessionId,h);}catch{}}
    if(['Network.webSocketFrameReceived','Network.webSocketFrameSent'].includes(m.method)&&m.params.response.opcode===2){
      const b=Buffer.from(m.params.response.payloadData,'base64'),key=m.sessionId+':'+b[0],map=m.method.endsWith('Received')?received:sent,old=map.get(key)||{frames:0,bytes:0};map.set(key,{frames:old.frames+1,bytes:old.bytes+b.length});
      if(m.method.endsWith('Received')&&b[0]===3&&canary&&b.includes(Buffer.from(canary)))leaks.push(m.sessionId);
    }
  });
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(Error('CDP deadline: '+method));},30000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
  const evaluate=async(s,expression)=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},s);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  const capture=async(s,name)=>{await call('Page.bringToFront',{},s);const r=await call('Page.captureScreenshot',{format:'png'},s);await writeFile(join(output,name+'.png'),Buffer.from(r.data,'base64'));};
  const wait=async(s,expression)=>{for(let i=0;i<600;i++){if(await evaluate(s,expression))return;await new Promise(r=>setTimeout(r,100));}await capture(s,'timeout');throw Error('Browser condition: '+expression+'; '+await evaluate(s,"document.querySelector('#error')?.textContent"));};
  const settled="!JSON.parse(document.querySelector('#revision').dataset.chat||'{}').history_loading&&!JSON.parse(document.querySelector('#revision').dataset.runtimeResources||'{}').text_document?.loading";
  const info=s=>evaluate(s,"JSON.parse(document.querySelector('#revision').dataset.chat)");
  const metrics=(s,width,height)=>call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false},s);
  const page=async()=>{const {browserContextId}=await call('Target.createBrowserContext'),{targetId}=await call('Target.createTarget',{url:'about:blank',browserContextId}),{sessionId:s}=await call('Target.attachToTarget',{targetId,flatten:true});contexts.set(s,browserContextId);await call('Page.enable',{},s);await call('Runtime.enable',{},s);await call('Network.enable',{},s);await call('Target.setAutoAttach',{autoAttach:true,waitForDebuggerOnStart:false,flatten:true},s);await metrics(s,1280,800);await call('Page.navigate',{url:origin+'/brass-relay'},s);await wait(s,"document.querySelector('#revision')?.dataset.screen==='brass-relay'&&JSON.parse(document.querySelector('#revision').dataset.chat||'{}').online");return s;};
  const worker=async s=>{if(workers.has(s))return workers.get(s);const infos=(await call('Target.getTargets')).targetInfos.filter(t=>t.type==='worker'&&t.browserContextId===contexts.get(s)&&t.url.includes('/state-worker.js'));assert.equal(infos.length,1,'exactly one app worker');const {sessionId}=await call('Target.attachToTarget',{targetId:infos[0].targetId,flatten:true});workers.set(s,sessionId);return sessionId;};
  const snapshot=async s=>{
    await call('Page.bringToFront',{},s);await wait(s,settled);
    const w=await worker(s);await call('HeapProfiler.collectGarbage',{},s);await call('HeapProfiler.collectGarbage',{},w);
    const data=await evaluate(s,"({chat:JSON.parse(document.querySelector('#revision').dataset.chat),resources:JSON.parse(document.querySelector('#revision').dataset.runtimeResources),gpu:JSON.parse(document.querySelector('#revision').dataset.documentGpu),dom_records:document.querySelector('#chat-log').children.length})");
    data.main_heap=await call('Runtime.getHeapUsage',{},s);data.worker_heap=await call('Runtime.getHeapUsage',{},w);
    assert(data.worker_heap.usedSize>0&&data.worker_heap.backingStorageSize>0,'actual worker heap and backing storage metrics');
    assert(data.chat.messages<=64&&data.dom_records<=64);assert(data.chat.selected_bytes<=256*1024&&data.chat.replica_bytes<=512*1024);
    assert(data.resources.text_document.working_bytes<=8*1024*1024);assert(data.resources.text_document.source_index_bytes<=8*1024*1024);
    assert(data.gpu.budgeted_geometry_bytes<=8*1024*1024&&data.gpu.resident_versions<=128);
    return data;
  };
  const controls=s=>evaluate(s,"[...document.querySelectorAll('#controls button')].map(b=>{const r=b.getBoundingClientRect();return {label:b.getAttribute('aria-label'),x:r.x+r.width/2,y:r.y+r.height/2};})");
  const click=async(s,label)=>{await call('Page.bringToFront',{},s);const c=(await controls(s)).find(c=>c.label===label);assert(c,label);for(const type of ['mousePressed','mouseReleased'])await call('Input.dispatchMouseEvent',{type,button:'left',buttons:type==='mousePressed'?1:0,clickCount:1,x:c.x,y:c.y},s);};
  const started=performance.now(),a=await page();
  await wait(a,`JSON.parse(document.querySelector('#revision').dataset.chat).messages===64&&JSON.parse(document.querySelector('#revision').dataset.chat).last_sequence===${records}`);
  const initial=await snapshot(a),setupMs=performance.now()-started,initialTraffic={...received.get(a+':4')};
  assert.equal(initial.chat.first_sequence,records-63);await capture(a,'tail');
  for(let stroke=0;stroke<16;stroke++){
    await call('Page.bringToFront',{},a);
    await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:450,y:180},a);
    await new Promise(r=>setTimeout(r,80));
    await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,x:450,y:650},a);
    await new Promise(r=>setTimeout(r,220));
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:450,y:650},a);
    await wait(a,settled);
    if((await info(a)).first_sequence<initial.chat.first_sequence-16)break;
  }
  const older=await snapshot(a);assert(older.chat.first_sequence<initial.chat.first_sequence-16,'actual transcript dragging must acquire older coverage');assert(older.chat.last_sequence<records,'older view must be anchored away from the tail');await capture(a,'older');
  const b=await page();await wait(b,`JSON.parse(document.querySelector('#revision').dataset.chat).last_sequence===${records}`);await snapshot(b);
  await click(b,'Live drafts: compose privately');await click(b,'Message composer');
  canary='OUTSIDE_ANCHORED_VIEW_'+Date.now();const before=await snapshot(a),beforeBytes=received.get(a+':4').bytes;
  await call('Page.bringToFront',{},b);await call('Input.insertText',{text:canary},b);await click(b,'Send message');
  await wait(b,`JSON.parse(document.querySelector('#revision').dataset.chat).pending===0&&JSON.parse(document.querySelector('#revision').dataset.chat).last_sequence===${records+1}`);
  await new Promise(r=>setTimeout(r,300));const outside=await snapshot(a);
  assert.equal(received.get(a+':4').bytes,beforeBytes,'outside append transfers no body or collection event');
  for(const key of ['first_sequence','last_sequence','authors_revision'])assert.equal(outside.chat[key],before.chat[key]);
  assert.equal(outside.resources.text_document.packed_glyphs,before.resources.text_document.packed_glyphs,'outside append does not repack rows');
  const generation=outside.chat.lease_generation,anchor=outside.resources.text_document.anchor_identity;
  await metrics(a,800,1280);await wait(a,"document.querySelector('#view').dataset.logicalSize==='800x1280'");const resized=await snapshot(a);
  assert.equal(resized.chat.lease_generation,generation,'resize reuses data coverage');assert.deepEqual(resized.resources.text_document.anchor_identity,anchor);await capture(a,'older-portrait');
  await click(a,'Show new messages');await wait(a,`JSON.parse(document.querySelector('#revision').dataset.chat).last_sequence===${records+1}`);const latest=await snapshot(a);assert.equal(latest.chat.messages,64);
  const identity=latest.chat.participant;await call('Page.reload',{},a);await wait(a,`JSON.parse(document.querySelector('#revision').dataset.chat||'{}').online&&JSON.parse(document.querySelector('#revision').dataset.chat).last_sequence===${records+1}`);const reloaded=await snapshot(a);assert.equal(reloaded.chat.participant,identity);
  for(const s of [a,b]){assert.equal(hellos.get(s)?.version,1);assert.equal(hellos.get(s)?.state_protocol,'otrios-canvas-sync-v8');}
  assert.deepEqual(leaks,[]);
  const result={passed:true,records,setup_ms:setupMs,initial_collection_traffic:initialTraffic,initial,older,outside,resized,latest,reloaded,received:Object.fromEntries(received),sent:Object.fromEntries(sent),heap_scope:'CDP per-isolate JS heap and array-buffer/external-string backing storage; not RSS',private_body_leaks:leaks.length};
  await writeFile(join(output,'collection-windows.json'),JSON.stringify(result,null,2));console.log(JSON.stringify({passed:true,records,setup_ms:setupMs,initial_collection_traffic:initialTraffic,selected_bytes:initial.chat.selected_bytes,replica_bytes:initial.chat.replica_bytes,worker_heap:initial.worker_heap,gpu:initial.gpu,first_older:older.chat.first_sequence}));
} finally {
  socket?.close();if(browser.pid&&browser.exitCode===null&&browser.signalCode===null){browser.kill('SIGTERM');await new Promise(r=>browser.once('exit',r));}await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
