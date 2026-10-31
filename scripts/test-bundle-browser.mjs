// SPDX-License-Identifier: Apache-2.0
// Real temporary-profile Chrome qualification of installed app discovery/guest execution.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';
const args=process.argv.slice(2),arg=(name,fallback)=>{const i=args.indexOf(name);return i<0?fallback:args[i+1];};
const url=arg('--url'),expected=arg('--release'),next=arg('--wait-release');assert(url&&expected);
const output=resolve(arg('--output','target/bundle-browser'));await mkdir(output,{recursive:true});
const profile=await mkdtemp(join(tmpdir(),'otrios-bundle-browser-'));
const browser=spawn(arg('--chromium','chromium'),['--headless=new','--no-sandbox','--enable-unsafe-swiftshader','--use-angle=swiftshader','--remote-debugging-port=0','--user-data-dir='+profile,'--no-first-run','about:blank'],{stdio:['ignore','ignore','pipe']});
let socket;
try{
  const endpoint=await new Promise((resolve,reject)=>{let log='';const timer=setTimeout(()=>reject(Error(log)),30000);browser.stderr.on('data',bytes=>{log+=bytes;const m=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(m){clearTimeout(timer);resolve(m[1]);}});browser.on('error',reject);});
  socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
  let serial=0,hello;const pending=new Map(),errors=[];
  socket.addEventListener('message',({data})=>{const m=JSON.parse(data);if(m.id){const p=pending.get(m.id);if(p){pending.delete(m.id);clearTimeout(p.timer);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}}
    else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===1){try{const h=JSON.parse(m.params.response.payloadData);if(h.type==='hello')hello=h;}catch{}}
    else if(m.method==='Runtime.exceptionThrown')errors.push(m.params.exceptionDetails);});
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(Error(method));},30000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
  const {targetId}=await call('Target.createTarget',{url:'about:blank'}),{sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
  const evaluate=async expression=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},sessionId);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  const wait=async(expression,seconds=90)=>{for(let i=0;i<seconds*10;i++){const value=await evaluate(expression);if(value)return value;await new Promise(resolve=>setTimeout(resolve,100));}throw Error('Browser app did not settle: '+expression);};
  const sample=()=>evaluate(`(()=>{const r=document.querySelector('#revision'),canvas=document.querySelector('#scene'),text=canvas?.getAttribute('aria-label')||document.querySelector('canvas')?.getAttribute('aria-label')||'',m=text.match(/A small shared counter\\D*(\\d+)/);return {release:r?.dataset.release,screen:r?.dataset.screen,ui:r?.dataset.ui,text,value:m?Number(m[1]):null,online:document.querySelector('#connection')?.textContent||document.body.textContent,controls:[...document.querySelectorAll('#controls button')].map(b=>b.getAttribute('aria-label'))};})()`);
  const screenshot=async name=>{const shot=await call('Page.captureScreenshot',{format:'png'},sessionId);await writeFile(join(output,name+'.png'),Buffer.from(shot.data,'base64'));};
  await call('Page.enable',{},sessionId);await call('Runtime.enable',{},sessionId);await call('Network.enable',{},sessionId);
  await call('Emulation.setDeviceMetricsOverride',{width:Number(arg('--width',1280)),height:Number(arg('--height',800)),deviceScaleFactor:1,mobile:false},sessionId);
  await call('Page.navigate',{url:url.replace(/\/$/,'')+'/'},sessionId);
  await wait("document.querySelector('#revision')?.dataset.ui==='app:home/ui'&&!!document.querySelector('#controls button[aria-label=\"Bundle Counter\"]')");
  await screenshot('home');
  await evaluate("document.querySelector('#controls button[aria-label=\"Bundle Counter\"]').click()");
  await wait("document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'&&!!document.querySelector('#controls button[aria-label=\"Increase counter\"]')");
  const before=await sample();assert.equal(before.release,expected);assert.notEqual(before.value,null);assert.equal(hello?.version,1);assert.equal(hello?.state_protocol,'otrios-canvas-sync-v8');assert.equal(hello?.release,expected);
  let after=before;
  if(args.includes('--click')){
    await evaluate("document.querySelector('#controls button[aria-label=\"Increase counter\"]').click()");
    for(let i=0;i<100;i++){after=await sample();if(after.value===before.value+Number(arg('--step',1)))break;await new Promise(resolve=>setTimeout(resolve,100));}
    assert.equal(after.value,before.value+Number(arg('--step',1)));
  }
  await screenshot('counter');await writeFile(join(output,'ready.json'),JSON.stringify({before,after,hello},null,2)+'\n');
  let upgraded;
  if(next){
    await wait(`document.querySelector('#revision')?.dataset.release===${JSON.stringify(next)}&&document.querySelector('#revision')?.dataset.ui==='app:bundle-counter/ui'`,180);
    upgraded=await sample();assert.equal(upgraded.value,after.value);assert.equal(hello?.release,next);
    if(args.includes('--upgrade-click')){
      const value=upgraded.value;await evaluate("document.querySelector('#controls button[aria-label=\"Increase counter\"]').click()");
      for(let i=0;i<100;i++){upgraded=await sample();if(upgraded.value===value+Number(arg('--upgrade-step',2)))break;await new Promise(resolve=>setTimeout(resolve,100));}
      assert.equal(upgraded.value,value+Number(arg('--upgrade-step',2)));
    }
    await screenshot('upgraded-counter');
  }
  assert.equal(errors.length,0,JSON.stringify(errors));
  const result={passed:true,before,after,upgraded,hello,errors};await writeFile(join(output,'probe.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
}finally{
  socket?.close();if(browser.pid&&browser.exitCode===null&&browser.signalCode===null){browser.kill('SIGTERM');await new Promise(resolve=>browser.once('exit',resolve));}
  await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
