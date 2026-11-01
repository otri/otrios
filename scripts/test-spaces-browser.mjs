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
 const page=async(token,space)=>{const {browserContextId}=await call('Target.createBrowserContext');const {targetId}=await call('Target.createTarget',{url:'about:blank',browserContextId});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});contexts.set(sessionId,browserContextId);for(const type of ['Page','Runtime','Network'])await call(type+'.enable',{},sessionId);await call('Page.addScriptToEvaluateOnNewDocument',{source:inputProbe},sessionId);await call('Page.navigate',{url:fixture.url+'/bundle-counter?space='+space},sessionId);if(token){await wait(sessionId,"document.querySelector('#monitoring-login')?.open");await evaluate(sessionId,`(()=>{const f=document.querySelector('#monitoring-form');f.elements.username.value=${JSON.stringify(token)};f.elements.password.value='password';f.requestSubmit();})()`);}return sessionId;};
 const count=async(session)=>evaluate(session,"Number((document.querySelector('#view')?.getAttribute('aria-label')||'').match(/A small shared counter\\D*(\\d+)/)?.[1])");
 const adminToken=(await readFile(fixture.root+'/admin.identity','utf8')).trim();
 const a=await page('alice@spaces.test',fixture.space_a);await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'&&!!document.querySelector('#controls button[aria-label=\"Increase counter\"]')");
 const b=await page('bob@spaces.test',fixture.space_a);await wait(b,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");
 const other=await page('alice@spaces.test',fixture.space_b);await wait(other,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");
 await call('Page.bringToFront',{},a);const initial=await count(a);await call('Page.bringToFront',{},other);const isolated=await count(other);assert(Number.isFinite(initial)&&Number.isFinite(isolated));const expected=initial+1;await call('Page.bringToFront',{},a);
 await evaluate(a,"document.querySelector('#controls button[aria-label=\"Increase counter\"]').click()");for(let i=0;i<100&&await count(a)!==expected;i++)await new Promise(r=>setTimeout(r,50));await call('Page.bringToFront',{},b);
 for(let i=0;i<100&&await count(b)!==expected;i++)await new Promise(r=>setTimeout(r,50));assert.equal(await count(a),expected);assert.equal(await count(b),expected);await call('Page.bringToFront',{},other);assert.equal(await count(other),isolated);
 const anonymous=await page(null,fixture.space_a),wrong=await page('bob@spaces.test',fixture.space_b);
 for(let i=0;i<300&&(!denied.has(anonymous)||!denied.has(wrong));i++)await new Promise(r=>setTimeout(r,100));assert(denied.has(anonymous)&&denied.has(wrong));
 assert.notEqual(await evaluate(anonymous,"document.querySelector('#revision')?.dataset.ui"),'app:bundle-counter/ui');assert.notEqual(await evaluate(wrong,"document.querySelector('#revision')?.dataset.ui"),'app:bundle-counter/ui');
 const headers={Origin:fixture.url,Authorization:'Bearer '+adminToken,'Content-Type':'application/json','X-OTriOS-Request':'spaces'};
 const state=await(await fetch(fixture.url+'/spaces',{headers})).json();
 const response=await fetch(fixture.url+'/spaces',{method:'POST',headers,body:JSON.stringify({version:1,expected_generation:state.generation,change:{type:'membership',space:fixture.space_a,identity:'bob',role:null}})});assert.equal(response.status,200);
 await wait(b,"document.querySelector('#revision')?.dataset.ui!=='app:bundle-counter/ui'");assert(!await evaluate(b,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('A small shared counter')"));await call('Page.bringToFront',{},a);await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");assert.equal(await count(a),expected);await evaluate(a,"document.querySelector('#controls button[aria-label=\"Increase counter\"]').click()");for(let i=0;i<100&&await count(a)!==expected+1;i++)await new Promise(r=>setTimeout(r,50));assert.equal(await count(a),expected+1);
 const click=async(session,label)=>{await call('Page.bringToFront',{},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');const selector='#controls button[aria-label='+JSON.stringify(label)+']';await wait(session,`!!document.querySelector(${JSON.stringify(selector)})`);await evaluate(session,`document.querySelector(${JSON.stringify(selector)}).click()`);};
 await click(a,'Quit');await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:spaces-host/ui'");
 assert.equal(await count(other),isolated);
 await click(a,'Apps & Data');await click(a,'Bundle Counter');
 const beforeRemoval=await(await fetch(fixture.url+'/spaces',{headers})).json();
 const collection=beforeRemoval.spaces.find(s=>s.id===fixture.space_a).apps[0].collection;
 const readmit=await fetch(fixture.url+'/spaces',{method:'POST',headers,body:JSON.stringify({version:1,expected_generation:beforeRemoval.generation,change:{type:'membership',space:fixture.space_a,identity:'bob',role:'member'}})});assert.equal(readmit.status,200);
 const rejoined=b;await call('Page.navigate',{url:fixture.url+'/bundle-counter?space='+fixture.space_a},rejoined);await wait(rejoined,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");assert.equal(await count(rejoined),expected+1);
 await click(a,'Home');await click(a,'Bundle Counter');await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");await click(a,'Quit');await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:spaces-host/ui'");
 await click(rejoined,'Increase counter');await wait(rejoined,"Number((document.querySelector('#view')?.getAttribute('aria-label')||'').match(/A small shared counter\\D*(\\d+)/)?.[1])==="+(expected+2));
 const retainedValue=expected+2;await click(a,'Apps & Data');await click(a,'Bundle Counter');
 await click(a,'Remove app');
 await wait(a,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('retains its data')&&!!document.querySelector('#controls button[aria-label=\"Add app\"]')");
 await wait(rejoined,"document.querySelector('#revision')?.dataset.ui!=='app:bundle-counter/ui'");
 const removed=await(await fetch(fixture.url+'/spaces',{headers})).json();const retained=removed.spaces.find(s=>s.id===fixture.space_a).apps[0];assert.equal(retained.collection,collection);assert.equal(retained.attached,false);
 await click(a,'Home');await click(a,'Apps & Data');await wait(a,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('Data from removed apps')");
 const retainedShot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/retained-data.png',Buffer.from(retainedShot.data,'base64'));
 await click(a,'Bundle Counter');await click(a,'Add app');await click(a,'Open');await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");assert.equal(await count(a),retainedValue);
 const reattached=await(await fetch(fixture.url+'/spaces',{headers})).json();assert.equal(reattached.spaces.find(s=>s.id===fixture.space_a).apps[0].collection,collection);
 const shot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/space-a.png',Buffer.from(shot.data,'base64'));
 await click(a,'Quit');await click(a,'Spaces');await click(a,'Browser B');await wait(a,"location.search.includes("+JSON.stringify(fixture.space_b)+")&&document.querySelector('#revision')?.dataset.ui==='app:spaces-host/ui'");await click(a,'Bundle Counter');await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");assert.equal(await count(a),isolated);
 const inkWait=async(predicate)=>{for(let attempt=0;attempt<200;attempt++){if(predicate())return;await new Promise(resolve=>setTimeout(resolve,50));}throw Error('Ink observation timed out');};
 for(const session of [a,other]){await click(session,'Quit');await click(session,'Space ink probe');await wait(session,"document.querySelector('#revision')?.dataset.ui==='app:space-ink-probe/ui'");}
 await call('Page.bringToFront',{},b);await wait(b,"['app:spaces-host/ui','app:bundle-counter/ui'].includes(document.querySelector('#revision')?.dataset.ui)");if(await evaluate(b,"!!document.querySelector('#controls button[aria-label=\"Quit\"]')"))await click(b,'Quit');else await click(b,'Home');await click(b,'Space ink probe');await wait(b,"document.querySelector('#revision')?.dataset.ui==='app:space-ink-probe/ui'");
 await inkWait(()=>[a,b,other].every(session=>(welcomes.get(session)||0)>0));
 // CDP drag moves need the pressed button as well as its bitmask; otherwise
 // Chromium may lose capture on the next stroke's first move.
 const draw=async session=>{await call('Page.bringToFront',{},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');const bounds=await evaluate(session,"(()=>{const r=document.querySelector('#controls button[aria-label=\"Touch pad\"]').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}})()");const x=bounds.x+bounds.width*.25,y=bounds.y+bounds.height*.25;await call('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',buttons:1,clickCount:1},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',x:bounds.x+bounds.width*.7,y:bounds.y+bounds.height*.6,buttons:1},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');await call('Input.dispatchMouseEvent',{type:'mouseReleased',x:bounds.x+bounds.width*.7,y:bounds.y+bounds.height*.6,button:'left',buttons:0,clickCount:1},session);};
 await draw(a);await inkWait(()=>ink.get(a)?.some(point=>point.phase===2)&&ink.get(other)?.some(point=>point.phase===2));
 assert(ink.get(a).every(point=>point.epoch>0));assert.deepEqual((({phase,x,y,peer})=>({phase,x,y,peer}))(ink.get(a).at(-1)),(({phase,x,y,peer})=>({phase,x,y,peer}))(ink.get(other).at(-1)));
 const bWelcome=welcomes.get(b);await click(b,'Quit');await click(b,'Space ink probe');await inkWait(()=>welcomes.get(b)>bWelcome);assert.equal((ink.get(b)||[]).length,0,'another Space must receive neither live nor retained trails');
 await click(other,'Quit');await wait(other,"document.querySelector('#revision')?.dataset.ui==='app:spaces-host/ui'");const pointsBefore=ink.get(a).length;await draw(a);try{await inkWait(()=>ink.get(a).length>pointsBefore&&ink.get(a).at(-1).phase===2);}catch(error){await writeFile(output+'/ink-failure.json',JSON.stringify({pointsBefore,received:ink.get(a),sent:inkSent.get(a),welcomes:welcomes.get(a),page:await evaluate(a,"({trace:window.__spaceProbe,focus:document.hasFocus(),visibility:document.visibilityState,ui:document.querySelector('#revision')?.dataset.ui,epoch:document.querySelector('#revision')?.dataset.epoch,inputs:[...document.querySelectorAll('#controls button')].map(b=>({label:b.getAttribute('aria-label'),bounds:{x:b.getBoundingClientRect().x,y:b.getBoundingClientRect().y,width:b.getBoundingClientRect().width,height:b.getBoundingClientRect().height}}))})")},null,2));throw error;}assert.equal(await evaluate(other,"document.querySelector('#revision')?.dataset.ui"),'app:spaces-host/ui');
 const captureTrace=await evaluate(a,'window.__spaceProbe');assert.equal(captureTrace.filter(e=>e.type==='gotpointercapture').length,2);assert.equal(captureTrace.filter(e=>e.type==='lostpointercapture'&&e.buttons===0).length,2);assert(!captureTrace.some(e=>e.type==='lostpointercapture'&&e.buttons!==0));
 await writeFile(output+'/ink-trace.json',JSON.stringify({received:ink.get(a),sent:inkSent.get(a),trace:await evaluate(a,'window.__spaceProbe')},null,2));
 const inkShot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/space-ink.png',Buffer.from(inkShot.data,'base64'));
 await click(a,'Quit');await click(a,'Spaces');await click(a,'Browser A');await wait(a,"location.search.includes("+JSON.stringify(fixture.space_a)+")&&document.querySelector('#revision')?.dataset.ui==='app:spaces-host/ui'");
 await click(b,'Quit');
 for(const session of [a,b,other]){await click(session,'The Brass Relay');await wait(session,"document.querySelector('#revision')?.dataset.ui==='app:brass-relay/ui'&&JSON.parse(document.querySelector('#revision').dataset.chat||'null')?.online");}
 for(const session of [a,b])await wait(session,"document.querySelector('#chat-log').textContent.includes('Legacy Engineer: Legacy household dispatch')");assert(!await evaluate(other,"document.querySelector('#chat-log').textContent.includes('Legacy household dispatch')"));
 const chatProfile=await evaluate(a,"JSON.parse(document.querySelector('#revision').dataset.chat).participant");
 const dispatch=async(session,text)=>{await call('Page.bringToFront',{},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');const r=await evaluate(session,"(()=>{const r=document.querySelector('#controls button[aria-label=\"Message composer\"]').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()");await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...r},session);await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...r},session);await wait(session,"document.activeElement?.classList.contains('canvas-editor')");await call('Input.insertText',{text},session);await wait(session,"document.querySelector('.canvas-editor')?.value==="+JSON.stringify(text));await click(session,'Send message');await wait(session,"JSON.parse(document.querySelector('#revision').dataset.chat).pending===0&&document.querySelector('#chat-log').textContent.includes("+JSON.stringify(text)+")");};
 const hasMessage=text=>"document.querySelector('#chat-log')?.textContent.includes("+JSON.stringify(text)+")";
 await dispatch(a,'Household transcript shared in this Space');await wait(b,hasMessage('Household transcript shared in this Space'));assert(!await evaluate(other,hasMessage('Household transcript shared in this Space')));
 await click(a,'Quit');await dispatch(b,'Another participant continues after Quit');await click(a,'The Brass Relay');await wait(a,hasMessage('Another participant continues after Quit'));await wait(a,"JSON.parse(document.querySelector('#revision').dataset.chat||'null')?.online");assert.equal(await evaluate(a,"JSON.parse(document.querySelector('#revision').dataset.chat).participant"),chatProfile);
 await click(a,'Members');assert(!await evaluate(a,"!!document.querySelector('#controls button[aria-label=\"Monitoring admin sign-in\"]')"));await click(a,'Close dialog');
 await click(a,'Quit');await click(a,'Apps & Data');await click(a,'The Brass Relay');await click(a,'Remove app');await wait(b,"document.querySelector('#revision')?.dataset.ui!=='app:brass-relay/ui'");assert.equal(await evaluate(b,"document.querySelector('#chat-log').children.length"),0);
 await click(a,'Add app');await click(a,'Open');await wait(a,hasMessage('Household transcript shared in this Space'));await wait(a,hasMessage('Another participant continues after Quit'));
 const chatShot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/space-chat.png',Buffer.from(chatShot.data,'base64'));

 // Delete a live, migrated transcript through the trusted host confirmation.
 // CDP observes real transport traffic; no Worker/DOM prototypes are replaced.
 await click(b,'The Brass Relay');await wait(b,hasMessage('Household transcript shared in this Space'));
 const beforeDelete=await(await fetch(fixture.url+'/spaces',{headers})).json();
 const deleting=beforeDelete.spaces.find(space=>space.id===fixture.space_a).apps.find(app=>app.policy.app==='brass-relay');
 const deleteRequest={version:1,expected_generation:beforeDelete.generation,change:{type:'delete-data',space:fixture.space_a,app:'brass-relay',collection:deleting.collection,remove:false}};
 const memberDelete=await evaluate(b,`fetch('/spaces',{method:'POST',headers:{'Content-Type':'application/json','X-OTriOS-Request':'spaces'},body:JSON.stringify(${JSON.stringify(deleteRequest)})}).then(response=>response.status)`);
 assert.equal(memberDelete,403,'a signed-in Member cannot delete the collection');
 const revokedBefore=(revocations.get(b)||[]).length,disposedBefore=(disposals.get(b)||[]).length;
 await click(a,'Quit');await click(a,'Apps & Data');await click(a,'The Brass Relay');await click(a,'Delete data');
 await wait(a,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('Space: Browser A')");
 const confirmation=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/delete-confirmation.png',Buffer.from(confirmation.data,'base64'));
 await click(a,'Permanently delete');
 await inkWait(()=>(revocations.get(b)||[]).length>revokedBefore);
 await wait(b,"document.querySelector('#chat-log').children.length===0");
 let deletedSpace;
 for(let attempt=0;attempt<100;attempt++){
  const state=await(await fetch(fixture.url+'/spaces',{headers})).json();
  deletedSpace=state.spaces.find(space=>space.id===fixture.space_a);
  if(deletedSpace.cleanup.local_pending===0&&deletedSpace.apps.find(app=>app.policy.app==='brass-relay').collection!==deleting.collection)break;
  await new Promise(resolve=>setTimeout(resolve,50));
 }
 const freshCollection=deletedSpace.apps.find(app=>app.policy.app==='brass-relay').collection;
 assert.notEqual(freshCollection,deleting.collection);assert.equal(deletedSpace.cleanup.local_pending,0);
 assert.equal(deletedSpace.cleanup.legacy_qualification_required,true,'unqualified migration copies stay pending');
 const revokedEpoch=revocations.get(b)[revokedBefore];
 assert(disposals.get(b).slice(disposedBefore).includes(revokedEpoch),'the real browser host must acknowledge disposal of the revoked epoch');
 await assert.rejects(stat(fixture.root+'/state/collection-'+deleting.collection+'.data'),{code:'ENOENT'});
 await wait(a,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('Imported originals await migration qualification')");
 const cleanup=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/deletion-cleanup.png',Buffer.from(cleanup.data,'base64'));
 await click(a,'Apps & Data');await click(a,'The Brass Relay');await click(a,'Open');
 await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:brass-relay/ui'&&JSON.parse(document.querySelector('#revision').dataset.chat||'null')?.online");
 assert.equal(await evaluate(a,"document.querySelector('#chat-log').children.length"),0);
 assert.notEqual(await evaluate(a,"JSON.parse(document.querySelector('#revision').dataset.chat).participant"),chatProfile);
 await call('Page.navigate',{url:fixture.url+'/brass-relay?space='+fixture.space_a},b);
 await wait(b,"document.querySelector('#revision')?.dataset.ui==='app:brass-relay/ui'&&JSON.parse(document.querySelector('#revision').dataset.chat||'null')?.online");
 assert.equal(await evaluate(b,"document.querySelector('#chat-log').children.length"),0);
 await dispatch(a,'Fresh transcript after permanent deletion');await wait(b,hasMessage('Fresh transcript after permanent deletion'));
 await dispatch(other,'Independent second Space continues');
 assert(!await evaluate(other,hasMessage('Fresh transcript after permanent deletion')));
 assert(!await evaluate(a,hasMessage('Independent second Space continues')));
 assert(!await evaluate(a,hasMessage('Legacy household dispatch')));
 const freshShot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/fresh-chat-collection.png',Buffer.from(freshShot.data,'base64'));
 // Public audience controls apply across isolated anonymous and signed-in contexts.
 await click(other,'Quit');await click(other,'Apps & Data');await click(other,'Sharing');
 await click(other,'Public view');await wait(other,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('Anyone will be able to view')");
 const sharing=await call('Page.captureScreenshot',{format:'png'},other);await writeFile(output+'/public-confirmation.png',Buffer.from(sharing.data,'base64'));
 await click(other,'Make public');
 const publicReader=await page(null,fixture.space_b);
 await call('Page.navigate',{url:fixture.url+'/bundle-counter?space='+fixture.space_b},wrong);
 for(const session of [publicReader,wrong]){
  await wait(session,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");
  assert.equal(await count(session),isolated);
  assert(!await evaluate(session,"!!document.querySelector('#controls button[aria-label=\"Increase counter\"]')"),'public viewers cannot invoke document edits');
  await click(session,'Quit');await click(session,'The Brass Relay');
  await wait(session,"document.querySelector('#revision')?.dataset.ui==='app:brass-relay/ui'&&JSON.parse(document.querySelector('#revision').dataset.chat||'null')?.online");
  await wait(session,hasMessage('Independent second Space continues'));
  assert.equal(await evaluate(session,"JSON.parse(document.querySelector('#revision').dataset.chat).participant"),'');
  assert(!await evaluate(session,"!!document.querySelector('#controls button[aria-label=\"Message composer\"]')"));
 }
 const readOnlyShot=await call('Page.captureScreenshot',{format:'png'},publicReader);await writeFile(output+'/public-read-only.png',Buffer.from(readOnlyShot.data,'base64'));
 await click(other,'Public view & comment');await click(other,'Make public');
 for(const session of [publicReader,wrong])await call('Page.navigate',{url:fixture.url+'/brass-relay?space='+fixture.space_b},session);
 await wait(wrong,"JSON.parse(document.querySelector('#revision')?.dataset.chat||'null')?.participant");
 await wait(publicReader,"JSON.parse(document.querySelector('#revision')?.dataset.chat||'null')?.online");
 assert(!await evaluate(publicReader,"!!document.querySelector('#controls button[aria-label=\"Message composer\"]')"),'anonymous viewers cannot comment');
 await dispatch(wrong,'Signed-in visitor comment in public Space');await wait(publicReader,hasMessage('Signed-in visitor comment in public Space'));
 await click(wrong,'Members');assert(!await evaluate(wrong,"[...document.querySelectorAll('#controls button')].some(button=>button.getAttribute('aria-label')?.startsWith('Kick '))"));await click(wrong,'Close dialog');
 const commentShot=await call('Page.captureScreenshot',{format:'png'},wrong);await writeFile(output+'/public-comment.png',Buffer.from(commentShot.data,'base64'));
 const commentRevoked=(revocations.get(wrong)||[]).length;
 await click(other,'Public view');await click(other,'Make public');
 await inkWait(()=>(revocations.get(wrong)||[]).length>commentRevoked);
 await wait(wrong,"document.querySelector('#chat-log').children.length===0");
 await call('Page.navigate',{url:fixture.url+'/brass-relay?space='+fixture.space_b},wrong);
 await wait(wrong,hasMessage('Signed-in visitor comment in public Space'));
 assert(!await evaluate(wrong,"!!document.querySelector('#controls button[aria-label=\"Message composer\"]')"));
 await call('Page.navigate',{url:fixture.url+'/brass-relay?space='+fixture.space_b},publicReader);await wait(publicReader,hasMessage('Signed-in visitor comment in public Space'));
 await click(other,'Keep closed');
 for(const session of [wrong,publicReader])await wait(session,"document.querySelector('#chat-log').children.length===0&&document.querySelector('#revision')?.dataset.ui!=='app:brass-relay/ui'");
 assert(await evaluate(a,hasMessage('Fresh transcript after permanent deletion')),'a closed unrelated Space remains available');
 // Publish one selected private dispatch with real owned bytes through the host.
 await click(other,'Public view');await click(other,'Make public');
 await dispatch(a,'Unselected private dispatch stays in its source Space');
 const caption='Selected private attachment publication';
 const fillComposer=async(session,text)=>{await call('Page.bringToFront',{},session);const point=await evaluate(session,"(()=>{const r=document.querySelector('#controls button[aria-label=\"Message composer\"]').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()");await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...point},session);await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...point},session);await wait(session,"document.activeElement?.classList.contains('canvas-editor')");await call('Input.insertText',{text},session);await wait(session,"document.querySelector('.canvas-editor')?.value==="+JSON.stringify(text));};
 const press=async(session,label)=>{await call('Page.bringToFront',{},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');const selector='#controls button[aria-label='+JSON.stringify(label)+']';await wait(session,`!!document.querySelector(${JSON.stringify(selector)})`);const point=await evaluate(session,`(()=>{const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`);await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...point},session);await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...point},session);};
 await fillComposer(a,caption);await click(a,'Commands');
 await call('Page.setInterceptFileChooserDialog',{enabled:true},a);
 await press(a,'Send file');try{await inkWait(()=>choosers.has(a));}catch(error){await writeFile(output+'/file-chooser-failure.json',JSON.stringify({errors,page:await evaluate(a,"({url:location.href,ui:document.querySelector('#revision')?.dataset.ui,epoch:document.querySelector('#revision')?.dataset.epoch,view:document.querySelector('#view')?.getAttribute('aria-label'),buttons:[...document.querySelectorAll('#controls button')].map(b=>({label:b.getAttribute('aria-label'),bounds:b.getBoundingClientRect().toJSON()})),trace:window.__spaceProbe,activation:navigator.userActivation.isActive})")},null,2));const shot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/file-chooser-failure.png',Buffer.from(shot.data,'base64'));throw error;}
 const document=await call('DOM.getDocument',{},a);const fileInput=await call('DOM.querySelector',{nodeId:document.root.nodeId,selector:'#chat-attachment'},a);
 await call('DOM.setFileInputFiles',{nodeId:fileInput.nodeId,files:[fixture.attachment]},a);
 await wait(a,"JSON.parse(document.querySelector('#revision').dataset.chat).pending===0&&document.querySelector('#chat-log').textContent.includes("+JSON.stringify(caption)+")&&document.querySelector('#chat-log').textContent.includes('selected-attachment.bin')");
 await wait(b,hasMessage(caption));
 const selectedSequence=await evaluate(a,"Number([...document.querySelector('#chat-log').children].find(row=>row.textContent.includes("+JSON.stringify(caption)+")).dataset.sequence)");
 assert(Number.isSafeInteger(selectedSequence));
 // Removal retains the attachment namespace as well as the transcript.
 await click(a,'Quit');await click(a,'Apps & Data');await click(a,'The Brass Relay');await click(a,'Remove app');
 await wait(b,"document.querySelector('#chat-log').children.length===0");
 await click(a,'Add app');await click(a,'Open');await wait(a,hasMessage(caption));
 assert(await evaluate(a,"document.querySelector('#chat-log').textContent.includes('selected-attachment.bin')"));
 await call('Page.navigate',{url:fixture.url+'/brass-relay?space='+fixture.space_a},b);await wait(b,hasMessage(caption));
 const review=async(session)=>{await click(session,'Commands');await click(session,'Browse dispatches');await click(session,'Select dispatch '+selectedSequence);};
 const sourceEpoch=Number(await evaluate(a,"document.querySelector('#revision').dataset.epoch")),leavesBefore=(disposals.get(a)||[]).length;
 await review(a);await click(a,'Publish to another Space');await click(a,'Publish in Browser B');
 await wait(a,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('Copies the full selected dispatch')");
 assert(!(disposals.get(a)||[]).slice(leavesBefore).includes(sourceEpoch),'publication review must keep its revocable source mount');
 const publicationShot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/publication-confirmation.png',Buffer.from(publicationShot.data,'base64'));
 await call('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true},a);
 await wait(a,"innerWidth===390&&document.querySelector('#controls button[aria-label=\"Publish copy\"]')?.getBoundingClientRect().right<=390");
 await evaluate(a,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');
 const portraitShot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/publication-portrait.png',Buffer.from(portraitShot.data,'base64'));
 await press(a,'Publish copy');await wait(a,"JSON.parse(document.querySelector('#revision').dataset.chat).pending===0");
 await call('Emulation.clearDeviceMetricsOverride',{},a);
 const gallery=await page(null,fixture.space_b);await wait(gallery,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");await click(gallery,'Quit');await click(gallery,'The Brass Relay');await wait(gallery,hasMessage(caption));
 assert(!await evaluate(gallery,hasMessage('Unselected private dispatch stays in its source Space')));
 assert(!await evaluate(gallery,hasMessage('Fresh transcript after permanent deletion')));
 const publicMetadata=await evaluate(gallery,"fetch('/spaces').then(response=>response.json())");
 assert(!JSON.stringify(publicMetadata).includes(fixture.space_a));
 const publishedSequence=await evaluate(gallery,"Number([...document.querySelector('#chat-log').children].find(row=>row.textContent.includes("+JSON.stringify(caption)+")).dataset.sequence)");
 await mkdir(output+'/downloads',{recursive:true});await call('Browser.setDownloadBehavior',{behavior:'allowAndName',downloadPath:output+'/downloads',browserContextId:contexts.get(gallery),eventsEnabled:true});
 const download=async()=>{const before=downloadEvents.length;await press(gallery,'Download selected-attachment.bin');await inkWait(()=>downloadEvents.length>before);const entry=downloadEvents[before];assert.equal(entry.suggestedFilename,'selected-attachment.bin');await inkWait(()=>downloadStates.get(entry.guid)==='completed');const bytes=await readFile(output+'/downloads/'+entry.guid);assert.equal(bytes.length,200000);assert.equal(createHash('sha256').update(bytes).digest('hex'),fixture.attachment_sha256);return entry.guid;};
 await click(gallery,'Commands');await click(gallery,'Browse dispatches');await click(gallery,'Select dispatch '+publishedSequence);await click(gallery,'Download attachments');
 const firstDownload=await download();
 await dispatch(a,'Later private changes do not update the published copy');
 assert(!await evaluate(gallery,hasMessage('Later private changes do not update the published copy')));
 // Leave a second publication review and another private preview open while
 // a separate Admin context deletes their source. Both hosts must redact before ACK.
 await review(a);await click(a,'Publish to another Space');await click(a,'Publish in Browser B');
 await review(b);
 const previewShot=await call('Page.captureScreenshot',{format:'png'},b);await writeFile(output+'/private-dispatch-preview.png',Buffer.from(previewShot.data,'base64'));
 const revokeStart=new Map([a,b].map(session=>[session,(revocations.get(session)||[]).length]));
 const disposalStart=new Map([a,b].map(session=>[session,(disposals.get(session)||[]).length]));
 await click(other,'Home');await click(other,'Spaces');await click(other,'Browser A');await wait(other,"location.search.includes("+JSON.stringify(fixture.space_a)+")&&document.querySelector('#revision')?.dataset.ui==='app:spaces-host/ui'");
 await click(other,'Apps & Data');await click(other,'The Brass Relay');await click(other,'Delete data');await click(other,'Permanently delete');
 for(const session of [a,b]){
  await inkWait(()=>(revocations.get(session)||[]).length>revokeStart.get(session));
  await wait(session,"document.querySelector('#chat-log').children.length===0&&!(document.querySelector('#view')?.getAttribute('aria-label')||'').includes("+JSON.stringify(caption)+")");
  const revoked=revocations.get(session)[revokeStart.get(session)];await inkWait(()=>(disposals.get(session)||[]).slice(disposalStart.get(session)).includes(revoked));
 }
 for(let attempt=0;attempt<200;attempt++){try{await stat(fixture.root+'/state/collection-'+freshCollection+'.data');}catch(error){if(error.code==='ENOENT')break;throw error;}if(attempt===199)throw Error('Published source namespace was not reclaimed');await new Promise(resolve=>setTimeout(resolve,25));}
 const afterSourceDelete=await download();assert.notEqual(afterSourceDelete,firstDownload);
 assert(await evaluate(gallery,hasMessage(caption)));
 assert.equal(await evaluate(gallery,"JSON.parse(document.querySelector('#revision').dataset.chat).participant"),'');
 const galleryShot=await call('Page.captureScreenshot',{format:'png'},gallery);await writeFile(output+'/published-attachment.png',Buffer.from(galleryShot.data,'base64'));
 assert.equal(errors.length,0,JSON.stringify(errors));assert(hellos.every(h=>h.version===1));
 const result={passed:true,trustedPublicationReview:true,portraitPublicationPointer:true,sourceLeaseHeldDuringReview:true,selectedConfirmedContentOnly:true,ownedAttachmentUploaded:true,attachmentSurvivesRemovalAndReattachment:true,publicAttachmentDownloadVerified:true,privateUpdatesDoNotMirror:true,sourcePreviewRevocationAcknowledged:true,publishedAttachmentSurvivesSourceDeletion:true,anonymousAttachmentReadWithoutProfile:true,attachmentSha256:fixture.attachment_sha256,publicViewReadable:true,publicDocumentEditsDisabled:true,publicChatWithoutProfile:true,anonymousCommentsDisabled:true,signedVisitorComments:true,visitorModerationDisabled:true,commentRestrictionRevokes:true,closingSpaceRevokesPublicReaders:true,initial,value:retainedValue,shared:expected,isolated,anonymousDenied:true,nonMemberDenied:true,activeRevoked:true,remainingMemberContinues:true,quitPreservesData:true,quitLeavesOtherParticipantEditing:true,removeRevokesAnotherClient:true,retainedDataVisible:true,reattachPreservesCollectionAndValue:true,spacePickerSwitchesScope:true,inkSharedAcrossClients:true,inkSpaceIsolation:true,inkParticipantContinuesAfterQuit:true,inkPointerCaptureLifecycle:true,chatSharedAcrossClients:true,chatSpaceIsolation:true,legacyChatHistoryMigratedAndIsolated:true,chatQuitLeavesOtherParticipantSending:true,chatProfileRestoredAfterQuit:true,chatRemovalRevokesAndRedacts:true,chatReattachmentRestoresTranscript:true,memberDeletionDenied:true,trustedDeletionConfirmation:true,connectedDisposalAckObserved:true,deletedNamespaceReclaimed:true,legacyCleanupPendingIsExplicit:true,freshCollectionAfterDelete:true,deletedTranscriptRedacted:true,otherSpaceContinuesAfterDelete:true,freshCollection,deletedCollection:deleting.collection,source:fixture.source,sourceDirty:fixture.source_dirty,authoritySha256:fixture.authority_sha256,wasmSha256:fixture.wasm_sha256,release:fixture.release,browserIdentities:'cookie sessions in isolated browser contexts',hello:hellos[0],runtimeErrors:errors};await writeFile(output+'/qualification.json',JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
}finally{socket?.close();if(chrome.pid&&chrome.exitCode===null){chrome.kill();await new Promise(r=>chrome.once('exit',r));}await rm(profile,{recursive:true,force:true,maxRetries:3,retryDelay:100});}
