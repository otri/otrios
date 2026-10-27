// SPDX-License-Identifier: Apache-2.0
// Read-only sent-history/release probe; guest admission may persist identity metadata.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';
const args=process.argv.slice(2),arg=(name,fallback)=>{const i=args.indexOf(name);return i<0?fallback:args[i+1];};
const url=arg('--url'),expectedProtocol=arg('--protocol','otrios-canvas-sync-v8'),expectedRelease=arg('--release');assert(url);
const output=resolve(arg('--output','target/chat-release-probe'));await mkdir(output,{recursive:true});
const profile=await mkdtemp(join(tmpdir(),'otrios-release-probe-'));
const browser=spawn(arg('--chromium','chromium'),['--headless=new','--no-sandbox','--enable-unsafe-swiftshader','--use-angle=swiftshader','--remote-debugging-port=0','--user-data-dir='+profile,'--no-first-run','about:blank'],{stdio:['ignore','ignore','pipe']});let socket;
try{
  const endpoint=await new Promise((resolve,reject)=>{let log='';const timer=setTimeout(()=>reject(Error(log)),30000);browser.stderr.on('data',b=>{log+=b;const m=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(m){clearTimeout(timer);resolve(m[1]);}});browser.on('error',reject);});
  socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
  let serial=0,hello;const pending=new Map();
  socket.addEventListener('message',({data})=>{const m=JSON.parse(data);if(m.id){const q=pending.get(m.id);if(q){pending.delete(m.id);clearTimeout(q.timer);m.error?q.reject(Error(JSON.stringify(m.error))):q.resolve(m.result);}}else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===1){try{const h=JSON.parse(m.params.response.payloadData);if(h.type==='hello')hello=h;}catch{}}});
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(Error(method));},30000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
  const {targetId}=await call('Target.createTarget',{url:'about:blank'}),{sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
  const evaluate=async expression=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},sessionId);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  await call('Page.enable',{},sessionId);await call('Network.enable',{},sessionId);await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},sessionId);await call('Page.navigate',{url:url.replace(/\/$/,'')+'/brass-relay'},sessionId);
  let ready=false;
  for(let i=0;i<600;i++){
    if(await evaluate("(()=>{const r=document.querySelector('#revision');if(!r)return false;const c=JSON.parse(r.dataset.chat||'{}'),d=JSON.parse(r.dataset.runtimeResources||'{}');return r.dataset.screen==='brass-relay'&&c.online&&c.messages>0&&!c.history_loading&&!d.text_document?.loading;})()")){ready=true;break;}
    await new Promise(r=>setTimeout(r,100));
  }
  assert(ready,'online recovered chat with a settled document');assert.equal(hello?.version,1);assert.equal(hello?.state_protocol,expectedProtocol);if(expectedRelease)assert.equal(hello.release,expectedRelease);
  const sample=await evaluate("(()=>{const r=document.querySelector('#revision'),c=JSON.parse(r.dataset.chat);return {release:r.dataset.release,rows:[...document.querySelector('#chat-log').children].map(n=>({sequence:Number(n.dataset.sequence),text:n.textContent})).sort((a,b)=>a.sequence-b.sequence),chat:{messages:c.messages,first_sequence:c.first_sequence,last_sequence:c.last_sequence,selected_bytes:c.selected_bytes,replica_bytes:c.replica_bytes,pending:c.pending},resources:JSON.parse(r.dataset.runtimeResources)};})()");
  assert.equal(sample.chat.pending,0);assert(sample.rows.length>0);if(expectedProtocol==='otrios-canvas-sync-v8')assert(sample.rows.length<=64);
  const tail=sample.rows.slice(-10),hash=value=>createHash('sha256').update(value).digest('hex');
  const result={passed:true,hello,release:sample.release,chat:sample.chat,resources:sample.resources,dom_records:sample.rows.length,tail_sequences:tail.map(r=>r.sequence),tail_text_sha256:tail.map(r=>hash(r.text)),tail_sha256:hash(JSON.stringify(tail)),sent_message_commands:0};
  const shot=await call('Page.captureScreenshot',{format:'png'},sessionId);await writeFile(join(output,'chat.png'),Buffer.from(shot.data,'base64'));await writeFile(join(output,'probe.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({passed:true,release:hello.release,protocol:hello.state_protocol,dom_records:sample.rows.length,tail_sequences:result.tail_sequences,tail_sha256:result.tail_sha256}));
}finally{
  socket?.close();if(browser.pid&&browser.exitCode===null&&browser.signalCode===null){browser.kill('SIGTERM');await new Promise(r=>browser.once('exit',r));}await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
