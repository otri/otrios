// SPDX-License-Identifier: Apache-2.0
// Real isolated Chromium contexts against the disposable Space authority fixture.
import assert from 'node:assert/strict';
import {pngPixels} from './browser-pixels.mjs';
import {createHash} from 'node:crypto';
import {spawn} from 'node:child_process';
import {readFile,mkdtemp,mkdir,writeFile,rm,stat} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const fixture=JSON.parse(await readFile(process.argv[2],'utf8'));
const output=fixture.output;await mkdir(output,{recursive:true});
const profile=await mkdtemp(join(tmpdir(),'otrios-spaces-chrome-'));
const chrome=spawn(process.env.OTRIOS_CHROMIUM||(process.platform==='darwin'?'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome':'chromium'),['--headless=new','--no-sandbox','--disable-dev-shm-usage','--disable-background-timer-throttling','--disable-renderer-backgrounding','--use-angle=swiftshader','--enable-unsafe-swiftshader','--remote-debugging-port=0','--user-data-dir='+profile,'--no-first-run','about:blank'],{stdio:['ignore','ignore','pipe']});
let socket;
try {
 const endpoint=await new Promise((resolve,reject)=>{let log='';const timer=setTimeout(()=>reject(Error(log)),20000);chrome.stderr.on('data',data=>{log+=data;const match=log.match(/DevTools listening on (ws:\/\/\S+)/);if(match){clearTimeout(timer);resolve(match[1]);}});chrome.on('error',reject);});
 socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
 let serial=0;const pending=new Map(),errors=[],denied=new Set(),hellos=[],ink=new Map(),inkSent=new Map(),welcomes=new Map(),revocations=new Map(),disposals=new Map(),contexts=new Map(),choosers=new Set(),downloadEvents=[],downloadStates=new Map();
 socket.addEventListener('message',({data})=>{const m=JSON.parse(data);if(m.id){const p=pending.get(m.id);if(p){pending.delete(m.id);clearTimeout(p.timer);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}}
 else if(m.method==='Page.fileChooserOpened')choosers.add(m.sessionId);
 else if(m.method==='Browser.downloadWillBegin')downloadEvents.push(m.params);
 else if(m.method==='Browser.downloadProgress')downloadStates.set(m.params.guid,m.params.state);
 else if(m.method==='Runtime.exceptionThrown')errors.push({session:m.sessionId,description:m.params.exceptionDetails.text});
 else if(m.method==='Network.webSocketFrameReceived'&&m.params.response.opcode===2){const b=Buffer.from(m.params.response.payloadData,'base64');if(b[0]===1&&b.length>=16&&b.subarray(1,5).toString()==='OTC5'&&b[5]===0&&b[11]===20){denied.add(m.sessionId);const events=revocations.get(m.sessionId)||[];events.push(b.readUInt32LE(12));revocations.set(m.sessionId,events);}if(b[0]===2){if(b.length<5||(b.length-5)%21){errors.push({description:'Malformed scoped ink'});return;}const epoch=b.readUInt32LE(1);for(let at=5;at+21<=b.length;at+=21){if(b[at+12]===4)welcomes.set(m.sessionId,(welcomes.get(m.sessionId)||0)+1);else{const points=ink.get(m.sessionId)||[];points.push({epoch,phase:b[at+12],peer:b.readUInt32LE(at+6),x:b.readUInt16LE(at+17),y:b.readUInt16LE(at+19)});ink.set(m.sessionId,points);}}}}
 else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===2){const b=Buffer.from(m.params.response.payloadData,'base64');if(b[0]===1&&b.length>=16&&b.subarray(1,5).toString()==='OTC5'&&b[5]===0&&b[11]===5){const events=disposals.get(m.sessionId)||[];events.push(b.readUInt32LE(12));disposals.set(m.sessionId,events);}if(b[0]===2&&b.length>=20){const points=inkSent.get(m.sessionId)||[];for(let at=5;at+15<=b.length;at+=15)points.push({epoch:b.readUInt32LE(1),phase:b[at+6],stroke:b.readUInt32LE(at+7),x:b.readUInt16LE(at+11),y:b.readUInt16LE(at+13)});inkSent.set(m.sessionId,points);}}
 else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===1){try{const h=JSON.parse(m.params.response.payloadData);if(h.type==='hello')hellos.push({version:h.version,state_protocol:h.state_protocol});}catch{}}
 });
 const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(Error(method));},20000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
 const evaluate=async(session,expression)=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},session);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
 const wait=async(session,expression)=>{for(let i=0;i<600;i++){const value=await evaluate(session,expression);if(value)return value;await new Promise(r=>setTimeout(r,100));}const png=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(output+'/failure.png',Buffer.from(png.data,'base64'));throw Error('Browser timeout: '+expression+' '+await evaluate(session,"JSON.stringify({body:document.body.innerText.slice(-1000),view:document.querySelector('#view')?.getAttribute('aria-label'),controls:[...document.querySelectorAll('#controls button')].map(e=>e.getAttribute('aria-label')),ui:document.querySelector('#revision')?.dataset.ui,error:document.querySelector('#error')?.textContent})"));};
 const inputProbe="(()=>{window.__spaceProbe=[];const save=x=>{window.__spaceProbe.push({at:performance.now(),...x});if(window.__spaceProbe.length>128)window.__spaceProbe.shift();};for(const type of ['focus','blur','pointerdown','pointermove','pointerup','pointercancel','gotpointercapture','lostpointercapture'])window.addEventListener(type,e=>save({kind:'dom',type,target:e.target===window?'window':e.target.tagName,pointer:e.pointerId,buttons:e.buttons,eventTime:e.timeStamp,capture:e.pointerId!==undefined&&document.querySelector('#view')?.hasPointerCapture(e.pointerId),coalesced:e.getCoalescedEvents?.().map(s=>s.timeStamp)}),true);})()";
 // macOS headless windows can stay hidden after bringToFront; keep every test view visible.
 const page=async(token,space,path)=>{const {browserContextId}=await call('Target.createBrowserContext');const {targetId}=await call('Target.createTarget',{url:'about:blank',browserContextId});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});contexts.set(sessionId,browserContextId);for(const type of ['Page','Runtime','Network'])await call(type+'.enable',{},sessionId);await call('Emulation.setFocusEmulationEnabled',{enabled:true},sessionId);await call('Page.addScriptToEvaluateOnNewDocument',{source:inputProbe},sessionId);await call('Page.navigate',{url:fixture.url+(path||'/bundle-counter?space='+space)},sessionId);await call('Page.bringToFront',{},sessionId);if(token){await wait(sessionId,"document.querySelector('#monitoring-login')?.open");await evaluate(sessionId,`(()=>{const f=document.querySelector('#monitoring-form');f.elements.username.value=${JSON.stringify(token)};f.elements.password.value='password';f.requestSubmit();})()`);}return sessionId;};
 const count=async(session)=>evaluate(session,"Number((document.querySelector('#view')?.getAttribute('aria-label')||'').match(/A small shared counter\\D*(\\d+)/)?.[1])");
 const adminToken=(await readFile(fixture.root+'/admin.identity','utf8')).trim();

 const headers={Origin:fixture.url,Authorization:'Bearer '+adminToken,'Content-Type':'application/json','X-OTriOS-Request':'spaces'};
 const get=async()=>{const r=await fetch(fixture.url+'/spaces',{headers});assert.equal(r.status,200);return r.json();};
 const manage=async(change,generation)=>{const before=await get();const r=await fetch(fixture.url+'/spaces',{method:'POST',headers,body:JSON.stringify({version:1,expected_generation:generation??before.generation,change})});assert.equal(r.status,200,await r.text());};
 const view="(document.querySelector('#view')?.getAttribute('aria-label')||'')";
 const ui="document.querySelector('#revision')?.dataset.ui";
 const selector=label=>'#controls button[aria-label='+JSON.stringify(label)+']';
 let clickSequence=0;
 const click=async(session,label)=>{const key=++clickSequence;await wait(session,`(()=>{const button=document.querySelector(${JSON.stringify(selector(label))}),previous=window.__spaceClick;if(!button||previous?.key!==${key}||previous.epoch!==button.dataset.epoch){window.__spaceClick={key:${key},epoch:button?.dataset.epoch,at:performance.now()};return false;}if(performance.now()-previous.at<120)return false;button.click();delete window.__spaceClick;return true;})()`);};
 const press=async(session,label)=>{await wait(session,`!!document.querySelector(${JSON.stringify(selector(label))})`);const p=await evaluate(session,`(()=>{const r=document.querySelector(${JSON.stringify(selector(label))}).getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...p},session);await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...p},session);};
 const shot=async(session,name)=>{assert(!await evaluate(session,"(()=>{const e=document.querySelector('#error');return e&&!e.hidden&&e.textContent})()"),'visible host error');await evaluate(session,'new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');const png=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(output+'/'+name+'.png',Buffer.from(png.data,'base64'));};
 if(fixture.populated){
  // Match the deployed home density using only synthetic documents/identities.
  await manage({type:'audience',space:fixture.space_b,audience:'public-comment'});
  const a=await page(null,fixture.space_b,'/');
  await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},a);
  await wait(a,ui+"==='app:spaces-host/ui'");
  await click(a,'Sign in');await wait(a,"document.querySelector('#monitoring-login').open");
  await evaluate(a,"(()=>{const f=document.querySelector('#monitoring-form');f.elements.username.value='alice@spaces.test';f.elements.password.value='password';})()");
  // Keyboard/viewport changes must not cancel the common-entry sign-in request.
  await call('Emulation.setDeviceMetricsOverride',{width:1278,height:798,deviceScaleFactor:1,mobile:false},a);
  await evaluate(a,'new Promise(r=>setTimeout(r,150))');
  assert(await evaluate(a,"document.querySelector('#monitoring-login').open"));
  assert.equal(await evaluate(a,"document.querySelector('#monitoring-form').elements.password.value"),'password');
  const signInAt=Date.now();
  await evaluate(a,"document.querySelector('#monitoring-form').requestSubmit()");
  await wait(a,`!!document.querySelector(${JSON.stringify(selector('Profile'))})&&!document.querySelector('#monitoring-login').open`);
  const signInMs=Date.now()-signInAt;
  await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},a);
  const all=await get();assert.equal(all.spaces.length,4);assert.equal(all.spaces.reduce((n,s)=>n+s.apps.length,0),14);
  for(const [i,space]of all.spaces.entries())await manage({type:'configure-space',space:space.id,name:space.name,layout:{width:[3,1,1,3][i],height:[2,1,1,1][i],background:['sage','sand','stone','lilac'][i]}});
  const scroll="JSON.parse(document.querySelector('#revision')?.dataset.mapScroll||'null')";
  await wait(a,scroll+"?.limit[1]>0");
  const bounds=label=>evaluate(a,`document.querySelector(${JSON.stringify(selector(label))})?.getBoundingClientRect().toJSON()`);
  const profile=await bounds('Profile'),edit=await bounds('Edit');
  const samples=[];
  for(const delta of [96,96,100000,-96,-100000,100000,-100000]){
   const before=await evaluate(a,scroll);
   await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:20,y:600,deltaX:0,deltaY:delta},a);
   await wait(a,scroll+"?.offset[1]==="+Math.max(0,Math.min(before.limit[1],before.offset[1]+delta)));
   assert.deepEqual(await bounds('Profile'),profile,'profile remains fixed');assert.deepEqual(await bounds('Edit'),edit,'footer remains fixed');
   const png=await call('Page.captureScreenshot',{format:'png'},a);const bytes=Buffer.from(png.data,'base64'),im=pngPixels(bytes);
   const pixel=(x,y)=>[...im.pixels.subarray((y*im.width+x)*im.channels,(y*im.width+x)*im.channels+3)];
   for(const [x,y]of [[10,790],[640,790],[10,750],[50,719]]){
    const actual=pixel(x,y);assert(actual.every((v,c)=>Math.abs(v-[247,246,240][c])<=2),`page/fade edge ${x},${y}: ${actual}`);
   }
   samples.push((await evaluate(a,scroll)).offset[1]);await writeFile(output+'/populated-'+samples.length+'.png',bytes);
  }
  // Drag the second Space over a full-screen first Space without releasing.
  // Exercise a narrow touch viewport and desktop pointer frames with one epoch.
  const reorderChecks=[];
  await click(a,'Edit');
  for(const [width,touch]of [[390,true],[1280,false]]){
   await manage({type:'configure-space',space:fixture.space_a,name:'Browser A',layout:{width:6,height:6,background:'sage'}});
   if((await get()).spaces[0].id!==fixture.space_a)await manage({type:'place-space',space:fixture.space_a,before:fixture.space_b});
   await call('Emulation.setDeviceMetricsOverride',{width,height:700,deviceScaleFactor:1,mobile:false},a);
   await call('Emulation.setTouchEmulationEnabled',{enabled:touch,maxTouchPoints:1},a);
   await wait(a,'innerWidth==='+width);
   await wait(a,scroll+'?.limit[1]>500');
   await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:20,y:400,deltaX:0,deltaY:100000},a);
   await wait(a,scroll+'?.offset[1]==='+scroll+'?.limit[1]');
   for(let n=0;n<30&&!await bounds('Space: Browser B');n++){
    const old=(await evaluate(a,scroll)).offset[1];
    await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:20,y:400,deltaX:0,deltaY:-80},a);
    await wait(a,scroll+'?.offset[1]<'+old);
   }
   const rect=await bounds('Space: Browser B');assert(rect,'source title becomes visible');
   assert(!await bounds('Space: Browser A'),'destination title starts above the viewport');
   const epoch=await evaluate(a,`document.querySelector(${JSON.stringify(selector('Done'))}).dataset.epoch`);
   const start={x:rect.x+4,y:rect.y+4},edge={x:start.x,y:108};
   const input=async(phase,p)=>touch
    ?call('Input.dispatchTouchEvent',{type:phase,touchPoints:phase==='touchEnd'?[]:[{...p,id:0,radiusX:1,radiusY:1,force:1}]},a)
    :call('Input.dispatchMouseEvent',{type:phase,button:'left',buttons:phase==='mouseReleased'?0:1,clickCount:1,...p},a);
   await input(touch?'touchStart':'mousePressed',start);
   await input(touch?'touchMove':'mouseMoved',edge);
   await wait(a,scroll+'?.offset[1]===0');
   assert.equal(await evaluate(a,`document.querySelector(${JSON.stringify(selector('Done'))}).dataset.epoch`),epoch,'preview and autoscroll retain native capture epoch');
   await shot(a,'reorder-edge-'+width);
   await input(touch?'touchEnd':'mouseReleased',edge);
   for(let n=0;n<100&&(await get()).spaces[0].id!==fixture.space_b;n++)await new Promise(r=>setTimeout(r,50));
   assert.equal((await get()).spaces[0].id,fixture.space_b);
   reorderChecks.push({width,input:touch?'touch':'mouse',offscreen_destination:true,stationary_edge_scroll:true,stable_epoch:true});
  }
  assert.equal(errors.length,0,JSON.stringify(errors));
  await writeFile(output+'/populated-acceptance.json',JSON.stringify({passed:true,spaces:4,apps:14,first_attempt_sign_in:true,sign_in_ms:signInMs,viewport_resize_preserves_sign_in:true,fixed_header_footer:true,fade_and_footer_pixels:true,scroll_samples:samples,reorder_checks:reorderChecks,source:fixture.source,source_dirty:fixture.source_dirty,wasm_sha256:fixture.wasm_sha256,runtime_errors:errors},null,2));
  console.log('Populated map pixels, scrolling and first-attempt sign-in passed');
 }else{
 const a=await page('alice@spaces.test',fixture.space_a);
 await wait(a,ui+"==='app:bundle-counter/ui'");await click(a,'Back to OTriOS');await wait(a,ui+"==='app:spaces-host/ui'");
 await wait(a,"document.querySelector('#connection')?.textContent==='Live shared space'");
 assert(await evaluate(a,`!!document.querySelector(${JSON.stringify(selector('Edit'))})`));
 assert(!await evaluate(a,`!!document.querySelector(${JSON.stringify(selector('Spaces'))})`));
 assert(await evaluate(a,`!!document.querySelector(${JSON.stringify(selector('Profile'))})`));
 const b=await page('bob@spaces.test',fixture.space_a);await wait(b,ui+"==='app:bundle-counter/ui'");const heldValue=await count(b);
 const initial=await get();const ids=initial.spaces.flatMap(s=>s.apps.map(a=>a.collection));
 await manage({type:'configure-space',space:fixture.space_a,name:'Browser A',layout:{width:3,height:2,background:'sage'}});
 await manage({type:'configure-space',space:fixture.space_b,name:'Browser B',layout:{width:1,height:1,background:'sand'}});
 await manage({type:'create',name:'Empty Space'});
 await wait(a,view+".includes('Browser A')");
 assert(!await evaluate(a,view+".includes('Empty Space')"));
 await shot(a,'map');
 await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},a);
 await wait(a,"innerWidth===1280");
 // Regression: wheel input and repeated bound-to-bound pointer reversals retain
 // the mount, restore clipped controls, and leave no captured pointer behind.
 await manage({type:'configure-space',space:fixture.space_a,name:'Browser A',layout:{width:3,height:6,background:'sage'}});
 const scroll="JSON.parse(document.querySelector('#revision')?.dataset.mapScroll||'null')";
 await wait(a,scroll+"?.limit[1]>300");
 const mapEpoch=await evaluate(a,`document.querySelector(${JSON.stringify(selector('Profile'))}).dataset.epoch`);
 assert(!await evaluate(a,`!!document.querySelector(${JSON.stringify(selector('Scroll down'))})`));
 for(let round=0;round<2;round++){
  await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:20,y:600,deltaX:0,deltaY:10000},a);
  await wait(a,scroll+"?.offset[1]==="+scroll+"?.limit[1]");
  await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:20,y:600,deltaX:0,deltaY:-10000},a);
  await wait(a,scroll+"?.offset[1]===0");
  await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:20,y:650},a);
  await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,x:20,y:-10000},a);
  await wait(a,scroll+"?.offset[1]==="+scroll+"?.limit[1]");
  await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,x:20,y:20000},a);
  await wait(a,scroll+"?.offset[1]===0");
  await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:20,y:20000},a);
  assert.equal(await evaluate(a,`document.querySelector(${JSON.stringify(selector('Profile'))}).dataset.epoch`),mapEpoch);
 }
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Bundle Counter in Browser A'))})`);
 await shot(a,'scroll-returned-top');
 await manage({type:'configure-space',space:fixture.space_a,name:'Browser A',layout:{width:3,height:2,background:'sage'}});
 await wait(a,scroll+"?.limit[1]<300");
 await click(a,'Edit');await wait(a,`!!document.querySelector(${JSON.stringify(selector('Space settings: Empty Space'))})`);
 await shot(a,'map-edit');
 await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:20,y:600,deltaX:0,deltaY:10000},a);
 await click(a,'Add app: Empty Space');
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Bundle Counter'))})`);
 await shot(a,'add-app');await click(a,'Close');
 await wait(a,scroll+"!==null");
 await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:20,y:600,deltaX:0,deltaY:-10000},a);
 await click(a,'Space settings: Browser A');await wait(a,`!!document.querySelector(${JSON.stringify(selector('Name'))})`);
 await press(a,'Name');await wait(a,"document.activeElement?.classList.contains('canvas-editor')");
 await call('Input.dispatchKeyEvent',{type:'keyDown',key:'a',code:'KeyA',modifiers:4},a);await call('Input.dispatchKeyEvent',{type:'keyUp',key:'a',code:'KeyA',modifiers:4},a);
 // Supported browser atomic input replacement, followed by a real input event.
 await evaluate(a,"(()=>{const e=document.activeElement;e.value='Workshop';e.setSelectionRange(e.value.length,e.value.length);e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertReplacementText'}));})()");
 await wait(a,"document.querySelector('.canvas-editor')?.value==='Workshop'");
 await click(a,'Background: Lilac');
 for(let i=0;i<100&&(await get()).spaces.find(s=>s.id===fixture.space_a).name!=='Workshop';i++)await new Promise(r=>setTimeout(r,50));

 let changed=await get();assert.equal(changed.spaces.find(s=>s.id===fixture.space_a).name,'Workshop');assert.equal(changed.spaces.find(s=>s.id===fixture.space_a).layout.background,'lilac');
 await shot(a,'space-settings');await click(a,'Close');
 await click(a,'Bundle Counter in Workshop');await wait(a,`!!document.querySelector(${JSON.stringify(selector('Name'))})`);
 assert(await evaluate(a,view+".includes('bundle-counter')"));
 assert(!await evaluate(a,`!!document.querySelector(${JSON.stringify(selector('Rename'))})`));
 await press(a,'Name');await wait(a,"document.activeElement?.classList.contains('canvas-editor')");
 await evaluate(a,"(()=>{const e=document.activeElement;e.value='Tally';e.setSelectionRange(5,5);e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertReplacementText'}));})()");
 await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter'},a);await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter'},a);
 for(let i=0;i<100&&(await get()).spaces.find(s=>s.id===fixture.space_a).apps[0].label!=='Tally';i++)await new Promise(r=>setTimeout(r,50));
 changed=await get();assert.equal(changed.spaces.find(s=>s.id===fixture.space_a).apps[0].label,'Tally');
 const transferDownloads=join(output,'downloads',contexts.get(a));await mkdir(transferDownloads,{recursive:true});
 await call('Browser.setDownloadBehavior',{behavior:'allow',browserContextId:contexts.get(a),downloadPath:transferDownloads,eventsEnabled:true});
 const exportStart=downloadEvents.length;
 await click(a,'Export Data');await wait(a,view+".includes('bundle-counter.otdata')");await shot(a,'data-export');
 let portableDownload;
 for(let n=0;n<100;n++){
  portableDownload=downloadEvents.slice(exportStart).find(event=>event.suggestedFilename==='bundle-counter.otdata');
  if(portableDownload&&downloadStates.get(portableDownload.guid)==='completed')break;
  await new Promise(resolve=>setTimeout(resolve,100));
 }
 assert(portableDownload&&downloadStates.get(portableDownload.guid)==='completed','the browser must complete the actual portable download');
 const portableFile=join(transferDownloads,portableDownload.suggestedFilename),portableBytes=await readFile(portableFile);
 assert.equal(portableBytes.subarray(0,4).toString('hex'),'504b0304');assert(portableBytes.length>128);
 await click(a,'Close');await shot(a,'app-settings');await click(a,'Close');
 // Keyboard resize produces the same shared metadata transition as the corner.
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Resize: Workshop'))})`);
 const size=changed.spaces.find(s=>s.id===fixture.space_a).layout;
 const oldCorner=await evaluate(a,`document.querySelector(${JSON.stringify(selector('Resize: Workshop'))}).getBoundingClientRect().right`);
 await evaluate(a,`document.querySelector(${JSON.stringify(selector('Resize: Workshop'))}).focus()`);
 await call('Input.dispatchKeyEvent',{type:'keyDown',key:'ArrowLeft',code:'ArrowLeft'},a);await call('Input.dispatchKeyEvent',{type:'keyUp',key:'ArrowLeft',code:'ArrowLeft'},a);
 for(let i=0;i<100&&(await get()).spaces.find(s=>s.id===fixture.space_a).layout.width===size.width;i++)await new Promise(r=>setTimeout(r,50));
 assert.equal((await get()).spaces.find(s=>s.id===fixture.space_a).layout.width,size.width-1);
 await wait(a,`document.querySelector(${JSON.stringify(selector('Resize: Workshop'))})?.getBoundingClientRect().right<${oldCorner}`);

 const drag=async(from,to)=>{
  const point=async label=>wait(a,`(()=>{const r=document.querySelector(${JSON.stringify(selector(label))})?.getBoundingClientRect();return r?{x:r.x+r.width/2,y:r.y+Math.min(32,r.height/2)}:null})()`);
  const start=await point(from),end=await point(to);
  await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...start},a);
  await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,...end},a);
  await evaluate(a,'new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');
  await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...end},a);
 };
 await drag('Space: Browser B','Space: Workshop');
 for(let i=0;i<100&&(await get()).spaces[0].id!==fixture.space_b;i++)await new Promise(r=>setTimeout(r,50));
 await shot(a,'pointer-space-order');
 assert.equal((await get()).spaces[0].id,fixture.space_b);
 await wait(a,`document.querySelector(${JSON.stringify(selector('Space: Browser B'))})?.getBoundingClientRect().left<document.querySelector(${JSON.stringify(selector('Space: Workshop'))})?.getBoundingClientRect().left`);
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Tally'))})`);
 await drag('Tally','Space ink probe in Workshop');
 for(let i=0;i<100&&(await get()).spaces.find(s=>s.id===fixture.space_a).apps[0].policy.app==='bundle-counter';i++)await new Promise(r=>setTimeout(r,50));
 assert.equal((await get()).spaces.find(s=>s.id===fixture.space_a).apps[0].policy.app,'space-ink-probe');
 assert.equal(await evaluate(b,ui),'app:bundle-counter/ui');assert.equal(await count(b),heldValue);
 await click(a,'Done');
 for(const width of [1280,390]){
  await call('Emulation.setDeviceMetricsOverride',{width,height:844,deviceScaleFactor:1,mobile:false},a);await wait(a,`innerWidth===${width}`);await evaluate(a,'new Promise(r=>setTimeout(r,250))');
  const bounds=await evaluate(a,"[...document.querySelectorAll('#controls button')].map(b=>({label:b.getAttribute('aria-label'),r:b.getBoundingClientRect().toJSON()}))");
  for(const {label,r}of bounds){assert(r.x>=-.1&&r.y>=-.1&&r.right<=width+.1&&r.bottom<=844.1,label+' '+JSON.stringify(r));}
  await shot(a,'map-'+width);
 }
 await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},a);
 // Retained-data UX uses a disposable new app so original fixture data is preserved.
 const empty=(await get()).spaces.find(s=>s.name==='Empty Space');
 await click(a,'Edit');await click(a,'Add app: Empty Space');
 await click(a,'Bundle Counter');
 for(let n=0;n<100&&!(await get()).spaces.find(s=>s.id===empty.id).apps.some(a=>a.attached);n++)await new Promise(r=>setTimeout(r,50));
 const retainedCID=(await get()).spaces.find(s=>s.id===empty.id).apps[0].collection;
 await click(a,'Close');await click(a,'Delete App: Bundle Counter in Empty Space');
 // Duplicate canonical names are disambiguated by the containing Space on map tiles;
 // the minus button must also carry its Space scope.
 const checked=()=>evaluate(a,`document.querySelector(${JSON.stringify(selector('Remove Data'))})?.getAttribute('aria-checked')`);
 assert.equal(await checked(),'false');
 await press(a,'Remove Data');await wait(a,`document.querySelector(${JSON.stringify(selector('Remove Data'))})?.getAttribute('aria-checked')==='true'`);
 assert.equal(await evaluate(a,`document.querySelector(${JSON.stringify(selector('Remove Data'))})?.getAttribute('role')`),'checkbox');
 await shot(a,'remove-data-checked');
 await evaluate(a,`document.querySelector(${JSON.stringify(selector('Remove Data'))}).focus()`);
 await call('Input.dispatchKeyEvent',{type:'keyDown',key:' ',code:'Space',windowsVirtualKeyCode:32},a);
 await call('Input.dispatchKeyEvent',{type:'keyUp',key:' ',code:'Space',windowsVirtualKeyCode:32},a);
 await wait(a,`document.querySelector(${JSON.stringify(selector('Remove Data'))})?.getAttribute('aria-checked')==='false'`);
 await shot(a,'remove-data-unchecked');
 await click(a,'Remove Data');await click(a,'Cancel');
 await click(a,'Delete App: Bundle Counter in Empty Space');await click(a,'Delete');
 for(let n=0;n<100&&(await get()).spaces.find(s=>s.id===empty.id).apps[0].attached;n++)await new Promise(r=>setTimeout(r,50));
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Bundle Counter (data) in Empty Space'))})`);
 await shot(a,'retained-ghost');
 assert(!await evaluate(a,view+".includes('Saved.')"));
 await click(a,'Bundle Counter (data) in Empty Space');await click(a,'Restore App');await shot(a,'restore-data-review');await click(a,'Restore');
 for(let n=0;n<100&&!(await get()).spaces.find(s=>s.id===empty.id).apps[0].attached;n++)await new Promise(r=>setTimeout(r,50));
 assert.equal((await get()).spaces.find(s=>s.id===empty.id).apps[0].collection,retainedCID);
 await click(a,'Delete App: Bundle Counter in Empty Space');await click(a,'Remove Data');await click(a,'Delete');
 for(let n=0;n<100&&(await get()).spaces.find(s=>s.id===empty.id).apps.length;n++)await new Promise(r=>setTimeout(r,50));
 assert.equal((await get()).spaces.find(s=>s.id===empty.id).apps.length,0);
 await click(a,'Space settings: Empty Space');await click(a,'Delete Space');await click(a,'Delete');
 await click(a,'Profile');await click(a,'Deleted Spaces');await click(a,'Restore Space: Empty Space');
 for(let n=0;n<100&&(await get()).spaces.find(s=>s.id===empty.id).archived;n++)await new Promise(r=>setTimeout(r,50));
 assert.equal((await get()).spaces.find(s=>s.id===empty.id).archived,false);
 await click(a,'Close');await click(a,'Done');
 await writeFile(output+'/retained-acceptance.json',JSON.stringify({passed:true,source:fixture.source,source_dirty:fixture.source_dirty,release:fixture.release,authority_sha256:fixture.authority_sha256,wasm_sha256:fixture.wasm_sha256,default_keep_data:true,cancel_clears_remove_data:true,visible_checkbox_and_accessible_checked_state:true,exact_identity_restore:true,checked_remove_data:true,deleted_space_recovery:true,silent_success:true,runtime_errors:errors},null,2));
 // One transfer path covers the browser keyboard pickup, shared review and fresh binding.
 await manage({type:'create',name:'Transfer target'});
 const transferTarget=(await get()).spaces.find(s=>s.name==='Transfer target');
 await manage({type:'audience',space:transferTarget.id,audience:'public-view'});
 await manage({type:'attach',space:empty.id,app:'bundle-counter'});
 const movingCID=(await get()).spaces.find(s=>s.id===empty.id).apps[0].collection;
 await click(a,'Edit');
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Bundle Counter in Empty Space'))})`);
 await wait(a,`document.querySelector(${JSON.stringify(selector('Bundle Counter in Empty Space'))})?.dataset.draggable==='true'`);
 await evaluate(a,`document.querySelector(${JSON.stringify(selector('Bundle Counter in Empty Space'))}).focus()`);
 await call('Input.dispatchKeyEvent',{type:'keyDown',key:' ',code:'Space',windowsVirtualKeyCode:32},a);
 await call('Input.dispatchKeyEvent',{type:'keyUp',key:' ',code:'Space',windowsVirtualKeyCode:32},a);
 await click(a,'Move to Transfer target');await wait(a,view+".includes('Public · read only')");
 await shot(a,'cross-space-public-review');await click(a,'Move and publish data');
 for(let n=0;n<100&&!(await get()).spaces.find(s=>s.id===transferTarget.id).apps.length;n++)await new Promise(r=>setTimeout(r,50));
 const moved=(await get()).spaces.find(s=>s.id===transferTarget.id).apps;
 assert.equal(moved.length,1);assert.notEqual(moved[0].collection,movingCID);
 assert.equal((await get()).spaces.find(s=>s.id===empty.id).apps.length,0);
 assert(!await evaluate(a,view+".includes('Saved.')"));
 await manage({type:'delete-space',space:transferTarget.id,remove_data:true});
 await click(a,'Done');
 await writeFile(output+'/moves-acceptance.json',JSON.stringify({passed:true,source:fixture.source,source_dirty:fixture.source_dirty,release:fixture.release,authority_sha256:fixture.authority_sha256,wasm_sha256:fixture.wasm_sha256,keyboard_pickup:true,public_exposure_review:true,fresh_destination_binding:true,source_retired:true,silent_success:true,runtime_errors:errors},null,2));
 // Exercise real file picking and reviewed replacement in a disposable Space.
 // Existing fixture CIDs and the other client's replica remain unchanged.
 await manage({type:'create',name:'Portable fixture'});
 const portableSpace=(await get()).spaces.find(space=>space.name==='Portable fixture');
 await manage({type:'attach',space:portableSpace.id,app:'bundle-counter'});
 const outgoingCID=(await get()).spaces.find(space=>space.id===portableSpace.id).apps[0].collection;
 await click(a,'Space: Portable fixture');await wait(a,ui+"==='app:bundle-counter/ui'");
 await click(a,'Back to OTriOS');await click(a,'Edit');await click(a,'Bundle Counter in Portable fixture');
 await click(a,'Replace Data');
 await call('Page.setInterceptFileChooserDialog',{enabled:true},a);choosers.delete(a);
 await click(a,'Choose File');
 for(let n=0;n<100&&!choosers.has(a);n++)await new Promise(resolve=>setTimeout(resolve,50));
 assert(choosers.has(a),'host must invoke its real file picker');
 const document=await call('DOM.getDocument',{},a);
 const {nodeId}=await call('DOM.querySelector',{nodeId:document.root.nodeId,selector:'#app-transfer-file'},a);
 await call('DOM.setFileInputFiles',{nodeId,files:[portableFile]},a);
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Confirm Replacement'))})`);
 assert.equal((await get()).spaces.find(space=>space.id===portableSpace.id).apps[0].collection,outgoingCID,'upload and review must not activate data');
 await shot(a,'data-replacement-review');await click(a,'Confirm Replacement');
 let replacementCID=outgoingCID;
 for(let n=0;n<100;n++){
  replacementCID=(await get()).spaces.find(space=>space.id===portableSpace.id).apps[0].collection;
  if(replacementCID!==outgoingCID)break;
  await new Promise(resolve=>setTimeout(resolve,50));
 }
 assert.notEqual(replacementCID,outgoingCID);assert(!await evaluate(a,view+".includes('Saved.')"));
 assert((await get()).spaces.find(space=>space.id===portableSpace.id).retained.some(dataset=>dataset.collection===outgoingCID&&dataset.origin==='before-replacement'),'the exact outgoing collection must remain sealed');
 await wait(a,`[...document.querySelectorAll('#controls button')].some(button=>{const label=button.getAttribute('aria-label');return label.startsWith('Bundle Counter before replacement')&&label.endsWith(' (data) in Portable fixture');})`);
 await shot(a,'data-replacement-backup');await click(a,'Done');await click(a,'Space: Portable fixture');
 await wait(a,ui+"==='app:bundle-counter/ui'");assert.equal(await count(a),heldValue,'all exported confirmed fields survive replacement');
 await click(a,'Back to OTriOS');await manage({type:'delete-space',space:portableSpace.id,remove_data:true});
 await writeFile(output+'/portable-data-acceptance.json',JSON.stringify({passed:true,source:fixture.source,source_dirty:fixture.source_dirty,release:fixture.release,authority_sha256:fixture.authority_sha256,wasm_sha256:fixture.wasm_sha256,archive_sha256:createHash('sha256').update(portableBytes).digest('hex'),actual_browser_download:true,actual_file_picker:true,review_has_no_mutation:true,fresh_binding:true,retained_outgoing_data:true,confirmed_value:heldValue,runtime_errors:errors},null,2));
 // Switching to another Space's app stays within one browser session and opens it.
 await click(a,'Bundle Counter');await wait(a,"location.search.includes("+JSON.stringify(fixture.space_b)+")&&"+ui+"==='app:bundle-counter/ui'");
 await click(a,'Back to OTriOS');await click(a,'Profile');await click(a,'Sign out');
 await manage({type:'audience',space:fixture.space_b,audience:'public-comment'});
 const anonymous=await page(null,fixture.space_b);await wait(anonymous,ui+"==='app:bundle-counter/ui'");await click(anonymous,'Back to OTriOS');
 assert(!await evaluate(anonymous,`!!document.querySelector(${JSON.stringify(selector('Edit'))})`));
 assert(!await evaluate(anonymous,view+".includes('Workshop')"));
 await shot(anonymous,'public-map');
 // A Space's title directly enters its sole attached app.
 for(const app of ['space-ink-probe','brass-relay'])await manage({type:'remove',space:fixture.space_b,app});
 await click(anonymous,'Space: Browser B');await wait(anonymous,ui+"==='app:bundle-counter/ui'");
 const final=await get();assert.deepEqual(final.spaces.flatMap(s=>s.apps.map(a=>a.collection)).sort(),ids.sort());
 assert.equal(errors.length,0,JSON.stringify(errors));
 await writeFile(output+'/map-acceptance.json',JSON.stringify({source:fixture.source,source_dirty:fixture.source_dirty,release:fixture.release,authority_sha256:fixture.authority_sha256,wasm_sha256:fixture.wasm_sha256,protocols:hellos,collections_preserved:true,checks:['wheel and repeated scroll reversal','stable map epoch','restored clipped controls','common map','empty edit-only','Space name/color','implicit app name','stable bundle ID','keyboard resize','pointer Space/app reorder','mounted peer preserved','responsive bounds','same-session Space switch','anonymous filtering','single-app direct launch'],exceptions:errors},null,2));
 console.log('Spaces map browser acceptance passed');
 }
} finally {socket?.close();if(chrome.exitCode===null&&chrome.signalCode===null){const exited=new Promise(r=>chrome.once('exit',r));chrome.kill('SIGTERM');await exited;}await rm(profile,{recursive:true,force:true});}
