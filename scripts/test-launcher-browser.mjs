// SPDX-License-Identifier: Apache-2.0
// Real Chrome launcher access indicators; optional login uses loopback test fixtures only.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';
const args=process.argv.slice(2),arg=(name,fallback)=>{const i=args.indexOf(name);return i<0?fallback:args[i+1];};
const url=arg('--url'),expected=arg('--release'),next=arg('--wait-release');assert(url&&expected);if(args.includes('--test-login'))assert(['127.0.0.1','localhost'].includes(new URL(url).hostname),'Test login is loopback-only');
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
  const sample=()=>evaluate(`(()=>{const r=document.querySelector('#revision');return {release:r?.dataset.release,ui:r?.dataset.ui,text:document.querySelector('canvas')?.getAttribute('aria-label'),controls:[...document.querySelectorAll('#controls button')].map(b=>b.getAttribute('aria-label'))};})()`);
  const screenshot=async name=>{const shot=await call('Page.captureScreenshot',{format:'png'},sessionId);await writeFile(join(output,name+'.png'),Buffer.from(shot.data,'base64'));};
  await call('Page.enable',{},sessionId);await call('Runtime.enable',{},sessionId);await call('Network.enable',{},sessionId);
  await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},sessionId);
  await call('Page.navigate',{url:url.replace(/\/$/,'')+'/'},sessionId);
  const home="document.querySelector('#revision')?.dataset.ui==='app:home/ui'";
  const locked="document.querySelector('canvas')?.getAttribute('aria-label')?.includes('Sign-in required')";
  const unlocked="document.querySelector('canvas')?.getAttribute('aria-label')?.includes('Access available')";
  await wait(home+'&&'+locked);
  const initial=await sample();assert.equal(initial.release,expected);assert(!initial.text.includes('Installed app'));assert(!initial.text.includes('Access available'));
  assert(initial.controls.includes('Monitoring'));await screenshot('home-locked');
  let authenticated,revoked;
  if(args.includes('--test-login')){
    await evaluate("document.querySelector('#controls button[aria-label=\"Monitoring\"]').click()");
    await wait("document.querySelector('#monitoring-login')?.open");
    await evaluate("(()=>{const f=document.querySelector('#monitoring-form');f.elements.username.value='launcher@example.test';f.elements.password.value='test-only-password';f.requestSubmit();})()");
    await wait("document.querySelector('#revision')?.dataset.ui==='app:monitoring/ui'&&!document.querySelector('#monitoring-login')?.open");
    await evaluate("document.querySelector('#controls button[aria-label=\"OTriOS\"]').click()");
    await wait(home+'&&'+unlocked);authenticated=await sample();assert(!authenticated.text.includes('Sign-in required'));await screenshot('home-unlocked');
    await evaluate("document.querySelector('#controls button[aria-label=\"Monitoring\"]').click()");
    await wait("!!document.querySelector('#controls button[aria-label=\"Sign out\"]')");
    await evaluate("document.querySelector('#controls button[aria-label=\"Sign out\"]').click()");
    await wait(home+'&&'+locked);revoked=await sample();assert(!revoked.text.includes('Access available'));await screenshot('home-relocked');
  }
  assert.equal(errors.length,0,JSON.stringify(errors));
  const result={passed:true,initial,authenticated,revoked,hello,errors};await writeFile(join(output,'probe.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
}finally{
  socket?.close();if(browser.pid&&browser.exitCode===null&&browser.signalCode===null){browser.kill('SIGTERM');await new Promise(resolve=>browser.once('exit',resolve));}
  await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
