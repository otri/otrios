// SPDX-License-Identifier: Apache-2.0
// Real isolated Chromium contexts against the disposable Space authority fixture.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawn} from 'node:child_process';
import {readFile,mkdtemp,mkdir,writeFile,rm,stat} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const fixture=JSON.parse(await readFile(process.argv[2],'utf8'));
const output=fixture.output;await mkdir(output,{recursive:true});
const profile=await mkdtemp(join(tmpdir(),'otrios-spaces-chrome-'));
const chrome=spawn('chromium',['--headless=new','--no-sandbox','--disable-dev-shm-usage','--disable-background-timer-throttling','--disable-renderer-backgrounding','--use-angle=swiftshader','--enable-unsafe-swiftshader','--remote-debugging-port=0','--user-data-dir='+profile,'--no-first-run','about:blank'],{stdio:['ignore','ignore','pipe']});
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
 const wait=async(session,expression)=>{for(let i=0;i<600;i++){const value=await evaluate(session,expression);if(value)return value;await new Promise(r=>setTimeout(r,100));}throw Error('Browser timeout: '+expression+' '+await evaluate(session,"JSON.stringify({body:document.body.innerText.slice(-1000),ui:document.querySelector('#revision')?.dataset.ui,error:document.querySelector('#error')?.textContent})"));};
 const inputProbe="(()=>{window.__spaceProbe=[];const save=x=>{window.__spaceProbe.push({at:performance.now(),...x});if(window.__spaceProbe.length>128)window.__spaceProbe.shift();};for(const type of ['focus','blur','pointerdown','pointermove','pointerup','pointercancel','gotpointercapture','lostpointercapture'])window.addEventListener(type,e=>save({kind:'dom',type,target:e.target===window?'window':e.target.tagName,pointer:e.pointerId,buttons:e.buttons,eventTime:e.timeStamp,capture:e.pointerId!==undefined&&document.querySelector('#view')?.hasPointerCapture(e.pointerId),coalesced:e.getCoalescedEvents?.().map(s=>s.timeStamp)}),true);})()";
 // macOS headless windows can stay hidden after bringToFront; keep every test view visible.
 const page=async(token,space)=>{const {browserContextId}=await call('Target.createBrowserContext');const {targetId}=await call('Target.createTarget',{url:'about:blank',browserContextId});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});contexts.set(sessionId,browserContextId);for(const type of ['Page','Runtime','Network'])await call(type+'.enable',{},sessionId);await call('Emulation.setFocusEmulationEnabled',{enabled:true},sessionId);await call('Page.addScriptToEvaluateOnNewDocument',{source:inputProbe},sessionId);await call('Page.navigate',{url:fixture.url+'/bundle-counter?space='+space},sessionId);await call('Page.bringToFront',{},sessionId);if(token){await wait(sessionId,"document.querySelector('#monitoring-login')?.open");await evaluate(sessionId,`(()=>{const f=document.querySelector('#monitoring-form');f.elements.username.value=${JSON.stringify(token)};f.elements.password.value='password';f.requestSubmit();})()`);}return sessionId;};
 const count=async(session)=>evaluate(session,"Number((document.querySelector('#view')?.getAttribute('aria-label')||'').match(/A small shared counter\\D*(\\d+)/)?.[1])");
 const adminToken=(await readFile(fixture.root+'/admin.identity','utf8')).trim();

 const headers={Origin:fixture.url,Authorization:'Bearer '+adminToken,'Content-Type':'application/json','X-OTriOS-Request':'spaces'};
 const get=async()=>{const r=await fetch(fixture.url+'/spaces',{headers});assert.equal(r.status,200);return r.json();};
 const manage=async(change,generation)=>{const before=await get();const r=await fetch(fixture.url+'/spaces',{method:'POST',headers,body:JSON.stringify({version:1,expected_generation:generation??before.generation,change})});assert.equal(r.status,200,await r.text());};
 const view="(document.querySelector('#view')?.getAttribute('aria-label')||'')";
 const ui="document.querySelector('#revision')?.dataset.ui";
 const selector=label=>'#controls button[aria-label='+JSON.stringify(label)+']';
 const click=async(session,label)=>{await evaluate(session,'new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');await wait(session,`!!document.querySelector(${JSON.stringify(selector(label))})`);await evaluate(session,`document.querySelector(${JSON.stringify(selector(label))}).click()`);};
 const press=async(session,label)=>{await wait(session,`!!document.querySelector(${JSON.stringify(selector(label))})`);const p=await evaluate(session,`(()=>{const r=document.querySelector(${JSON.stringify(selector(label))}).getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...p},session);await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...p},session);};
 const shot=async(session,name)=>{assert(!await evaluate(session,"(()=>{const e=document.querySelector('#error');return e&&!e.hidden&&e.textContent})()"),'visible host error');await evaluate(session,'new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');const png=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(output+'/'+name+'.png',Buffer.from(png.data,'base64'));};
 const a=await page('alice@spaces.test',fixture.space_a);
 await wait(a,ui+"==='app:bundle-counter/ui'");await click(a,'Quit');await wait(a,ui+"==='app:spaces-host/ui'");
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
 await click(a,'Edit');await wait(a,`!!document.querySelector(${JSON.stringify(selector('Space settings: Empty Space'))})`);
 await shot(a,'map-edit');
 await click(a,'Space settings: Browser A');await wait(a,`!!document.querySelector(${JSON.stringify(selector('Name'))})`);
 await press(a,'Name');await wait(a,"document.activeElement?.classList.contains('canvas-editor')");
 await call('Input.dispatchKeyEvent',{type:'keyDown',key:'a',code:'KeyA',modifiers:4},a);await call('Input.dispatchKeyEvent',{type:'keyUp',key:'a',code:'KeyA',modifiers:4},a);
 // Supported browser atomic input replacement, followed by a real input event.
 await evaluate(a,"(()=>{const e=document.activeElement;e.value='Workshop';e.setSelectionRange(e.value.length,e.value.length);e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertReplacementText'}));})()");
 await wait(a,"document.querySelector('.canvas-editor')?.value==='Workshop'");
 await click(a,'Background: Lilac');await click(a,'Save');
 for(let i=0;i<100&&(await get()).spaces.find(s=>s.id===fixture.space_a).name!=='Workshop';i++)await new Promise(r=>setTimeout(r,50));
 await shot(a,'space-settings-at-save');await writeFile(output+'/settings-debug.json',JSON.stringify(await evaluate(a,"({view:document.querySelector('#view')?.getAttribute('aria-label'),inputs:[...document.querySelectorAll('.canvas-editor')].map(e=>({value:e.value,focused:e===document.activeElement}))})")));
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
 await click(a,'Export Data');await wait(a,view+".includes('not available')");await shot(a,'app-settings');await click(a,'Close');
 // Keyboard resize produces the same shared metadata transition as the corner.
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Resize: Workshop'))})`);
 const size=changed.spaces.find(s=>s.id===fixture.space_a).layout;
 const oldCorner=await evaluate(a,`document.querySelector(${JSON.stringify(selector('Resize: Workshop'))}).getBoundingClientRect().right`);
 await evaluate(a,`document.querySelector(${JSON.stringify(selector('Resize: Workshop'))}).focus()`);
 await call('Input.dispatchKeyEvent',{type:'keyDown',key:'ArrowLeft',code:'ArrowLeft'},a);await call('Input.dispatchKeyEvent',{type:'keyUp',key:'ArrowLeft',code:'ArrowLeft'},a);
 for(let i=0;i<100&&(await get()).spaces.find(s=>s.id===fixture.space_a).layout.width===size.width;i++)await new Promise(r=>setTimeout(r,50));
 assert.equal((await get()).spaces.find(s=>s.id===fixture.space_a).layout.width,size.width-1);
 await wait(a,`document.querySelector(${JSON.stringify(selector('Resize: Workshop'))}).getBoundingClientRect().right<${oldCorner}`);

 const drag=async(from,to)=>{
  const point=async label=>evaluate(a,`(()=>{const r=document.querySelector(${JSON.stringify(selector(label))}).getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+Math.min(32,r.height/2)}})()`);
  const start=await point(from),end=await point(to);
  await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...start},a);
  await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,...end},a);
  await evaluate(a,'new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');
  await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...end},a);
 };
 await drag('Space: Browser B','Space: Workshop');
 for(let i=0;i<100&&(await get()).spaces[0].id!==fixture.space_b;i++)await new Promise(r=>setTimeout(r,50));
 await writeFile(output+'/pointer-order-debug.json',JSON.stringify({state:await get(),view:await evaluate(a,view)}));await shot(a,'pointer-space-order');
 assert.equal((await get()).spaces[0].id,fixture.space_b);
 await wait(a,`document.querySelector(${JSON.stringify(selector('Space: Browser B'))}).getBoundingClientRect().left<document.querySelector(${JSON.stringify(selector('Space: Workshop'))}).getBoundingClientRect().left`);
 await wait(a,`!!document.querySelector(${JSON.stringify(selector('Tally'))})`);
 await drag('Tally','Space ink probe in Workshop');
 for(let i=0;i<100&&(await get()).spaces.find(s=>s.id===fixture.space_a).apps[0].policy.app==='bundle-counter';i++)await new Promise(r=>setTimeout(r,50));
 assert.equal((await get()).spaces.find(s=>s.id===fixture.space_a).apps[0].policy.app,'space-ink-probe');
 assert.equal(await evaluate(b,ui),'app:bundle-counter/ui');assert.equal(await count(b),heldValue);
 await click(a,'Done');
 for(const width of [1024,736,390,320]){
  await call('Emulation.setDeviceMetricsOverride',{width,height:844,deviceScaleFactor:1,mobile:false},a);await wait(a,`innerWidth===${width}`);await evaluate(a,'new Promise(r=>setTimeout(r,250))');
  const bounds=await evaluate(a,"[...document.querySelectorAll('#controls button')].map(b=>({label:b.getAttribute('aria-label'),r:b.getBoundingClientRect().toJSON()}))");
  for(const {label,r}of bounds){assert(r.x>=-.1&&r.y>=-.1&&r.right<=width+.1&&r.bottom<=844.1,label+' '+JSON.stringify(r));}
  await shot(a,'map-'+width);
 }
 await call('Emulation.clearDeviceMetricsOverride',{},a);
 // Switching to another Space's app stays within one browser session and opens it.
 await click(a,'Bundle Counter');await wait(a,"location.search.includes("+JSON.stringify(fixture.space_b)+")&&"+ui+"==='app:bundle-counter/ui'");
 await click(a,'Quit');await click(a,'Profile');await click(a,'Sign out');
 await manage({type:'audience',space:fixture.space_b,audience:'public-comment'});
 const anonymous=await page(null,fixture.space_b);await wait(anonymous,ui+"==='app:bundle-counter/ui'");await click(anonymous,'Quit');
 assert(!await evaluate(anonymous,`!!document.querySelector(${JSON.stringify(selector('Edit'))})`));
 assert(!await evaluate(anonymous,view+".includes('Workshop')"));
 await shot(anonymous,'public-map');
 // A Space's title directly enters its sole attached app.
 for(const app of ['space-ink-probe','brass-relay'])await manage({type:'remove',space:fixture.space_b,app});
 await click(anonymous,'Space: Browser B');await wait(anonymous,ui+"==='app:bundle-counter/ui'");
 const final=await get();assert.deepEqual(final.spaces.flatMap(s=>s.apps.map(a=>a.collection)).sort(),ids.sort());
 assert.equal(errors.length,0,JSON.stringify(errors));
 await writeFile(output+'/map-acceptance.json',JSON.stringify({source:fixture.source,source_dirty:fixture.source_dirty,release:fixture.release,authority_sha256:fixture.authority_sha256,wasm_sha256:fixture.wasm_sha256,protocols:hellos,collections_preserved:true,checks:['common map','empty edit-only','Space name/color','implicit app name','stable bundle ID','keyboard resize','pointer Space/app reorder','mounted peer preserved','responsive bounds','same-session Space switch','anonymous filtering','single-app direct launch'],exceptions:errors},null,2));
 console.log('Spaces map browser acceptance passed');
} finally {socket?.close();chrome.kill('SIGTERM');await new Promise(r=>chrome.once('exit',r));await rm(profile,{recursive:true,force:true});}
