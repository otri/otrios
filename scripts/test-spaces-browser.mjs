// SPDX-License-Identifier: Apache-2.0
// Real isolated Chromium contexts against the disposable Space authority fixture.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {readFile,mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
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
 let serial=0;const pending=new Map(),errors=[],denied=new Set(),hellos=[];
 socket.addEventListener('message',({data})=>{const m=JSON.parse(data);if(m.id){const p=pending.get(m.id);if(p){pending.delete(m.id);clearTimeout(p.timer);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}}
 else if(m.method==='Runtime.exceptionThrown')errors.push({session:m.sessionId,description:m.params.exceptionDetails.text});
 else if(m.method==='Network.webSocketFrameReceived'&&m.params.response.opcode===2){const b=Buffer.from(m.params.response.payloadData,'base64');if(b[0]===1&&b.subarray(1,5).toString()==='OTC5'&&b[5]===0&&b[11]===20)denied.add(m.sessionId);}
 else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===1){try{const h=JSON.parse(m.params.response.payloadData);if(h.type==='hello')hellos.push({version:h.version,state_protocol:h.state_protocol});}catch{}}
 });
 const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(Error(method));},20000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
 const evaluate=async(session,expression)=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},session);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
 const wait=async(session,expression)=>{for(let i=0;i<600;i++){const value=await evaluate(session,expression);if(value)return value;await new Promise(r=>setTimeout(r,100));}throw Error('Browser timeout: '+expression+' '+await evaluate(session,"JSON.stringify({body:document.body.innerText.slice(-1000),ui:document.querySelector('#revision')?.dataset.ui,error:document.querySelector('#error')?.textContent})"));};
 const page=async(token,space)=>{const {browserContextId}=await call('Target.createBrowserContext');const {targetId}=await call('Target.createTarget',{url:'about:blank',browserContextId});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});for(const type of ['Page','Runtime','Network'])await call(type+'.enable',{},sessionId);await call('Page.navigate',{url:fixture.url+'/bundle-counter?space='+space},sessionId);if(token){await wait(sessionId,"document.querySelector('#monitoring-login')?.open");await evaluate(sessionId,`(()=>{const f=document.querySelector('#monitoring-form');f.elements.username.value=${JSON.stringify(token)};f.elements.password.value='password';f.requestSubmit();})()`);}return sessionId;};
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
 await wait(b,"document.querySelector('#revision')?.dataset.ui===''");assert(!await evaluate(b,"(document.querySelector('#view')?.getAttribute('aria-label')||'').includes('A small shared counter')"));await call('Page.bringToFront',{},a);await wait(a,"document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'");assert.equal(await count(a),expected);await evaluate(a,"document.querySelector('#controls button[aria-label=\"Increase counter\"]').click()");for(let i=0;i<100&&await count(a)!==expected+1;i++)await new Promise(r=>setTimeout(r,50));assert.equal(await count(a),expected+1);
 const shot=await call('Page.captureScreenshot',{format:'png'},a);await writeFile(output+'/space-a.png',Buffer.from(shot.data,'base64'));
 assert.equal(errors.length,0,JSON.stringify(errors));assert(hellos.every(h=>h.version===1));
 const result={passed:true,initial,value:await count(a),shared:expected,isolated:await count(other),anonymousDenied:true,nonMemberDenied:true,activeRevoked:true,remainingMemberContinues:true,source:fixture.source,sourceDirty:fixture.source_dirty,authoritySha256:fixture.authority_sha256,wasmSha256:fixture.wasm_sha256,release:fixture.release,browserIdentities:'cookie sessions in isolated browser contexts',hello:hellos[0],runtimeErrors:errors};await writeFile(output+'/qualification.json',JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
}finally{socket?.close();if(chrome.pid&&chrome.exitCode===null){chrome.kill();await new Promise(r=>chrome.once('exit',r));}await rm(profile,{recursive:true,force:true,maxRetries:3,retryDelay:100});}
