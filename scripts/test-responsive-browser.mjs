// SPDX-License-Identifier: Apache-2.0
// Optional real Chromium/WebGL acceptance against a disposable local authority.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
const args=process.argv.slice(2),argument=(name,fallback)=>{const i=args.indexOf(name);return i<0?fallback:args[i+1];};
if(typeof WebSocket!=='function')throw Error('Node with built-in WebSocket support is required');
const origin=argument('--url');if(!origin)throw Error('Supply --url for a disposable authority');
const output=resolve(argument('--output','target/responsive-browser')),profile=await mkdtemp(join(tmpdir(),'otrios-browser-'));
await mkdir(output,{recursive:true});
const browser=spawn(argument('--chromium','chromium'),['--headless=new','--no-sandbox','--disable-dev-shm-usage','--disable-background-timer-throttling','--disable-renderer-backgrounding','--enable-unsafe-swiftshader','--use-angle=swiftshader','--remote-debugging-port=0',`--user-data-dir=${profile}`,'--no-first-run','about:blank'],{stdio:['ignore','ignore','pipe']});
let socket;
try{
  const endpoint=await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(Error('Chromium did not start')),30000);let log='';
    browser.stderr.on('data',bytes=>{log+=bytes;const match=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(match){clearTimeout(timer);resolve(match[1]);}});
    browser.once('error',error=>{clearTimeout(timer);reject(error);});browser.once('exit',code=>{clearTimeout(timer);reject(Error(`Chromium exited ${code}: ${log.slice(-1200)}`));});
  });
  socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
  let serial=0;const pending=new Map(),inkReceived=new Map(),socketCount=new Map(),sentFrames=new Map(),sentBytes=new Map(),hellos=new Map();
  socket.addEventListener('message',({data})=>{const m=JSON.parse(data);if(m.id){const task=pending.get(m.id);if(task){pending.delete(m.id);clearTimeout(task.timer);if(m.error)task.reject(Error(JSON.stringify(m.error)));else task.resolve(m.result);}}
    else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===1){try{const h=JSON.parse(m.params.response.payloadData);if(h.type==='hello')hellos.set(m.sessionId,h);}catch{}}
    else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===2){sentFrames.set(m.sessionId,(sentFrames.get(m.sessionId)||0)+1);sentBytes.set(m.sessionId,(sentBytes.get(m.sessionId)||0)+Buffer.from(m.params.response.payloadData,'base64').length);}
    else if(m.method==='Network.webSocketCreated')socketCount.set(m.sessionId,(socketCount.get(m.sessionId)||0)+1);
    else if(m.method==='Network.webSocketFrameReceived'&&m.params.response.opcode===2&&Buffer.from(m.params.response.payloadData,'base64')[0]===2)inkReceived.set(m.sessionId,(inkReceived.get(m.sessionId)||0)+1);
  });
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(Error(`CDP timeout: ${method}`));},30000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
  const evaluate=async(session,expression)=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},session);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  const wait=async(session,expression)=>{for(let i=0;i<200;i++){if(await evaluate(session,expression))return;await new Promise(r=>setTimeout(r,100));}throw Error('Browser condition timed out: '+expression+'; '+await evaluate(session,"document.querySelector('#error')?.textContent"));};
  const metrics=async(session,width,height)=>{await call('Page.bringToFront',{},session);await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false},session);};
  const page=async(width,height,route)=>{const {targetId}=await call('Target.createTarget',{url:'about:blank'});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});await call('Page.enable',{},sessionId);await call('Runtime.enable',{},sessionId);await call('Network.enable',{},sessionId);await metrics(sessionId,width,height);await call('Page.navigate',{url:origin+'/'+route},sessionId);await ready(sessionId,route,width<height?'800x1280':'1280x800');return sessionId;};
  const ready=async(session,screen,extent)=>{await wait(session,`document.querySelector('#revision')?.dataset.screen===${JSON.stringify(screen)}&&document.querySelector('#view')?.dataset.logicalSize===${JSON.stringify(extent)}`);assert.equal(await evaluate(session,"document.querySelector('#error').hidden"),true);};
  const controls=session=>evaluate(session,"[...document.querySelectorAll('#controls button')].map(b=>({id:b.dataset.sceneNode,label:b.getAttribute('aria-label'),value:b.getAttribute('aria-valuenow'),bounds:[b.getBoundingClientRect().x,b.getBoundingClientRect().y,b.getBoundingClientRect().width,b.getBoundingClientRect().height]}))");
  const click=async(session,label,screen,extent)=>{const control=(await controls(session)).find(c=>c.label===label);assert(control);const [x,y,w,h]=control.bounds;
    await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:x+w/2,y:y+h/2},session);
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:x+w/2,y:y+h/2},session);
    await ready(session,screen,extent);};
  const screenshot=async(session,name)=>{await call('Page.bringToFront',{},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');const {data}=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(join(output,name+'.png'),Buffer.from(data,'base64'));};
  const drag=async(session,label,fraction,release=true)=>{const control=(await controls(session)).find(c=>c.label===label);assert(control);const [x,y,w,h]=control.bounds,point={x:x+w*fraction,y:y+h/2};await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...point},session);if(release)await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...point},session);};
  if(args.includes('--wait-route-update')){
    const a=await page(1280,800,'layout-lab');
    const before=await evaluate(a,"({path:location.pathname,time:performance.timeOrigin,release:document.querySelector('#revision').dataset.release,length:history.length})");
    await writeFile(join(output,'route-update-ready.json'),JSON.stringify(before));console.log('READY for runtime update at a screen URL');
    let updated=false;for(let i=0;i<900;i++){if(await evaluate(a,`document.querySelector('#revision')?.dataset.screen==='layout-lab'&&document.querySelector('#revision').dataset.release!==${JSON.stringify(before.release)}`)){updated=true;break;}await new Promise(r=>setTimeout(r,100));}
    assert(updated,'screen reopens after an executable release');const after=await evaluate(a,"({path:location.pathname,time:performance.timeOrigin,release:document.querySelector('#revision').dataset.release,length:history.length,hash:location.hash})");
    assert.equal(after.path,before.path);assert.equal(after.hash,'');assert.equal(after.length,before.length);assert.notEqual(after.time,before.time);assert.equal(hellos.get(a).release,after.release);
    const result={passed:true,before,after,checks:['full upgrades reload the same screen URL','history length stays fixed','hello carries the new module release']};await writeFile(join(output,'route-update-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--navigation')){
    const a=await page(1280,800,'input-lab'),snapshot=()=>evaluate(a,"({path:location.pathname,hash:location.hash,time:performance.timeOrigin,release:document.querySelector('#revision').dataset.release,length:history.length})");
    const initial=await snapshot();assert.equal(initial.path,'/input-lab');assert.equal(initial.hash,'');assert.equal(hellos.get(a).release,initial.release,'hello identifies the module release at a clean URL');
    await click(a,'First text field','input-lab','1280x800');assert.equal((await snapshot()).path,initial.path);assert.equal((await snapshot()).length,initial.length,'local keyboard creates no navigation entry');
    await click(a,'Home','home','1280x800');assert.equal((await snapshot()).path,'/home');
    await click(a,'Component Playground','component-playground','1280x800');assert.equal((await snapshot()).path,'/component-playground');
    const sockets=socketCount.get(a);
    for(const [action,screen]of [['back','home'],['back','input-lab'],['forward','home'],['forward','component-playground']]){
      await evaluate(a,`history.${action}()`);await ready(a,screen,'1280x800');await wait(a,`location.pathname===${JSON.stringify('/'+screen)}`);
    }
    assert.equal((await snapshot()).time,initial.time);assert.equal(socketCount.get(a),sockets,'screen navigation keeps the same page and session');
    await call('Page.reload',{},a);await ready(a,'component-playground','1280x800');assert.equal((await snapshot()).path,'/component-playground');assert.equal(hellos.get(a).release,(await snapshot()).release);
    await call('Page.navigate',{url:origin+'/#/keyboard/input'},a);await ready(a,'input-lab','1280x800');await wait(a,"location.pathname==='/input-lab'&&location.hash===''");
    await call('Page.navigate',{url:origin+'/releases/'+'0'.repeat(64)+'/web/index.html?view=demo#/layout'},a);await ready(a,'layout-lab','1280x800');await wait(a,"location.pathname==='/layout-lab'&&location.hash===''");assert.equal(await evaluate(a,'location.search'),'?view=demo');
    await call('Page.navigate',{url:origin+'/monitoring'},a);await wait(a,"document.querySelector('#monitoring-login').open");assert.equal((await snapshot()).path,'/monitoring');assert.equal(await evaluate(a,"(async()=> (await fetch('/monitoring/presentation')).status)()"),401);
    await evaluate(a,"document.querySelector('#login-back').click()");await ready(a,'home','1280x800');await wait(a,"location.pathname==='/home'");assert.equal(await evaluate(a,"document.querySelector('#monitoring-login').open"),false);
    const resources=await evaluate(a,"performance.getEntriesByType('resource').filter(r=>r.name.endsWith('/app.js')).map(r=>r.name)");assert(resources.every(u=>/\/releases\/[a-f0-9]{64}\/web\/app.js$/.test(u)));
    assert.equal(await evaluate(a,"(async()=> (await fetch('/not-an-app')).status)()"),404);assert.equal(await evaluate(a,"document.querySelector('#error').hidden"),true);
    const result={passed:true,checks:['direct screen links and refresh','Back/Forward without page/session replacement','local keyboard leaves URL/history unchanged','release hello uses the pinned module identity','legacy hashes and expired release bookmarks normalize','Monitoring URL preserves sign-in boundary','executable modules retain immutable URLs']};
    await writeFile(join(output,'navigation-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--home-reorder')){
    const a=await page(800,1280,'home'),b=await page(1280,800,'home');
    await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:5},a);
    const cards=async session=>(await controls(session)).filter(c=>!['Edit cards','Done editing cards'].includes(c.label)).sort((a,b)=>a.bounds[1]-b.bounds[1]||a.bounds[0]-b.bounds[0]);
    const order=async session=>(await cards(session)).map(c=>c.label);
    const orderExpression=`[...document.querySelectorAll('#controls button')].filter(b=>!['Edit cards','Done editing cards'].includes(b.getAttribute('aria-label'))).sort((a,b)=>a.getBoundingClientRect().y-b.getBoundingClientRect().y||a.getBoundingClientRect().x-b.getBoundingClientRect().x).map(b=>b.getAttribute('aria-label'))`;
    await new Promise(r=>setTimeout(r,500)); const baseline=sentFrames.get(a)||0,baselineBytes=sentBytes.get(a)||0;
    await evaluate(a,"[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Edit cards').click()");
    await wait(a,"!![...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Done editing cards')");
    await new Promise(r=>setTimeout(r,350));assert.equal(sentFrames.get(a)||0,baseline,'editing and wiggle send no shared state');
    assert((await controls(b)).some(c=>c.label==='Edit cards'),'remote client stays outside editing');
    const initial=await cards(a),start=initial[5].bounds,target=initial[1].bounds,moved=initial[5].label;
    const center=([x,y,w,h])=>({x:x+w/2,y:y+h/2});
    await call('Page.bringToFront',{},a);
    await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{id:1,...center(start)}]},a);
    await call('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{id:1,...center(target)}]},a);
    await wait(b,`(${orderExpression})[1]===${JSON.stringify(moved)}`);
    await new Promise(r=>setTimeout(r,350));const settled=sentFrames.get(a)||0,settledBytes=sentBytes.get(a)||0;
    for(let i=0;i<12;i++){await call('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{id:1,...center(target)}]},a);await new Promise(r=>setTimeout(r,20));}
    await new Promise(r=>setTimeout(r,350));assert.equal(sentFrames.get(a)||0,settled,'same-slot pointer samples send no state');
    assert((await controls(a)).some(c=>c.label==='Done editing cards'));assert((await controls(b)).some(c=>c.label==='Edit cards'));
    await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]},a);await new Promise(r=>setTimeout(r,300));
    assert.deepEqual(await order(a),await order(b));assert.equal(await evaluate(a,"document.querySelector('#revision').dataset.screen"),'home');
    const ids=(await cards(a)).map(c=>[c.label,c.id]).sort();assert.deepEqual(ids,initial.map(c=>[c.label,c.id]).sort());
    await screenshot(a,'home-edit-portrait');await screenshot(b,'home-reordered-landscape');
    // Arrow movement retains focus and updates the other client's target order.
    await call('Page.bringToFront',{},a);
    const focused=(await cards(a))[1];await evaluate(a,`document.querySelector('[data-scene-node="${focused.id}"]').focus()`);
    await call('Input.dispatchKeyEvent',{type:'keyDown',key:'ArrowRight',code:'ArrowRight'},a);await call('Input.dispatchKeyEvent',{type:'keyUp',key:'ArrowRight',code:'ArrowRight'},a);
    await wait(b,`(${orderExpression})[2]===${JSON.stringify(moved)}`);assert.equal(await evaluate(a,'document.activeElement.dataset.sceneNode'),focused.id);
    // Rotation during capture cancels the drag and cannot launch the captured card.
    console.log('Checking rotation during touch capture');
    const held=center((await cards(a))[0].bounds);await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{id:2,...held}]},a);
    console.log('Rotating held touch');
    await metrics(a,1280,800);await ready(a,'home','1280x800');console.log('Ending rotated touch');await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]},a);
    await evaluate(a,"[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Done editing cards').click()");
    await wait(a,"!![...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Edit cards')");
    await new Promise(r=>setTimeout(r,350));assert.equal(await evaluate(a,"document.querySelector('#revision').dataset.screen"),'home');assert.deepEqual(await order(a),await order(b));
    await click(a,'Component Playground','component-playground','1280x800');
    const result={passed:true,checks:['touch drag streams card order before release','local Edit/Done and wiggle send no state','same-slot motion sends no state','portrait and landscape agree on order','stable component IDs and keyboard focus','keyboard reorder shares state','rotation cancels touch capture without navigation','Done restores launching'],binaryFramesDuringOneMove:settled-baseline,binaryBytesDuringOneMove:settledBytes-baselineBytes};
    await writeFile(join(output,'home-reorder-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--wait-behavior-update')){
    const active=await page(800,1280,'component-playground'),other=await page(1280,800,'home');
    await call('Page.bringToFront',{},active);await drag(active,'Linear slider',0.81);
    await wait(active,"Number([...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow'))>80");
    const snapshot=session=>evaluate(session,"({time:performance.timeOrigin,epoch:document.querySelector('#revision').dataset.epoch,behavior:JSON.parse(document.querySelector('#revision').dataset.runtimeResources).behavior_hash,release:document.querySelector('#revision').dataset.release})");
    await evaluate(active,"[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').focus();window.__guestUpdateNonce='same-page'");
    const before={active:await snapshot(active),other:await snapshot(other),value:(await controls(active)).find(c=>c.label==='Linear slider').value,controls:(await controls(active)).map(c=>[c.id,c.label]),focus:await evaluate(active,'document.activeElement.dataset.sceneNode'),sockets:[socketCount.get(active),socketCount.get(other)]};
    await screenshot(active,'behavior-before');await writeFile(join(output,'behavior-update-ready.json'),JSON.stringify(before));console.log('READY for behavior activation');
    let updated=false;for(let i=0;i<1800;i++){const a=await snapshot(active),b=await snapshot(other);if(a.behavior!==before.active.behavior&&a.release!==before.active.release&&b.release===a.release){updated=true;break;}await new Promise(r=>setTimeout(r,100));}
    assert(updated,'new module is admitted on both clients');
    const after={active:await snapshot(active),other:await snapshot(other)};
    assert.equal(after.active.time,before.active.time);assert.equal(after.other.time,before.other.time);assert.equal(after.other.epoch,before.other.epoch);
    assert.equal(await evaluate(active,'window.__guestUpdateNonce'),'same-page');assert.equal(await evaluate(active,'document.activeElement.dataset.sceneNode'),before.focus);
    assert.deepEqual((await controls(active)).map(c=>[c.id,c.label]),before.controls);
    assert.equal((await controls(active)).find(c=>c.label==='Linear slider').value,before.value,'replacement retains the confirmed value');
    assert.deepEqual([socketCount.get(active),socketCount.get(other)],before.sockets);
    await call('Page.bringToFront',{},active);await drag(active,'Linear slider',0.43);
    await wait(active,"Number([...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow'))===40");
    await screenshot(active,'behavior-after');await click(other,'Component Playground','component-playground','1280x800');
    await wait(other,"Number([...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow'))===40");
    assert.equal((await snapshot(other)).behavior,after.active.behavior);
    const result={passed:true,before,after,checks:['actual loaded guest hash changed','state and UI controls retained across code replacement','same pages, workers and WebSocket sessions','inactive Home mount retained','new slider guest snaps input to 10-percent steps','other client shares the confirmed result']};
    await writeFile(join(output,'behavior-update-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else  if(args.includes('--wait-home-update')){
    const home=await page(800,1280,'home'),other=await page(1280,800,'component-playground');
    await drag(other,'Linear slider',0.81);
    await wait(other,"Number([...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow'))>75");
    await call('Page.bringToFront',{},home);await evaluate(home,"document.querySelector('#controls button').focus();window.__homeUpdateNonce='retained-worker-test'");
    const before={home:await evaluate(home,"({path:location.pathname,time:performance.timeOrigin,epoch:document.querySelector('#revision').dataset.epoch,focus:document.activeElement.dataset.sceneNode})"),other:await evaluate(other,"({path:location.pathname,time:performance.timeOrigin,epoch:document.querySelector('#revision').dataset.epoch})"),sockets:[socketCount.get(home),socketCount.get(other)]};
    await screenshot(home,'home-before-update');await writeFile(join(output,'home-update-ready.json'),JSON.stringify(before));console.log('READY for Home UI activation');
    let updated=false;for(let i=0;i<1800;i++){if(await evaluate(home,"Number(document.querySelector('#revision')?.dataset.uiUpdates)>0")&&await evaluate(other,"Number(document.querySelector('#revision')?.dataset.uiUpdates)>0")){updated=true;break;}await new Promise(r=>setTimeout(r,100));}
    assert(updated,'both clients accept the new UI release');console.log('Both clients accepted UI update');
    assert.equal(await evaluate(home,'window.__homeUpdateNonce'),'retained-worker-test');
    assert.equal(await evaluate(home,'performance.timeOrigin'),before.home.time);assert.equal(await evaluate(other,'performance.timeOrigin'),before.other.time);
    const applied=await evaluate(home,"document.querySelector('#revision').dataset.release");
    assert.equal(await evaluate(home,'location.pathname'),'/home');assert.equal(await evaluate(other,'location.pathname'),'/component-playground');
    assert.equal(await evaluate(home,'document.activeElement.dataset.sceneNode'),before.home.focus);
    assert.equal(await evaluate(other,"document.querySelector('#revision').dataset.epoch"),before.other.epoch);
    assert.deepEqual([socketCount.get(home),socketCount.get(other)],before.sockets,'same sessions remain connected');
    assert(Number((await controls(other)).find(c=>c.label==='Linear slider').value)>75,'other app retains its confirmed value');
    console.log('Capturing updated Home');await screenshot(home,'home-after-update');
    console.log('Opening Home on other client');await call('Page.bringToFront',{},other);
    await click(other,'Home','home','1280x800');
    await wait(other,"document.querySelector('#view').getAttribute('aria-label').includes('Ready to explore')");
    console.log('Refreshing Home');await call('Page.bringToFront',{},home);await call('Page.reload',{},home);await ready(home,'home','800x1280');
    await wait(home,"document.querySelector('#view').getAttribute('aria-label').includes('Ready to explore')");
    const result={passed:true,checks:['explicit refresh opens the current release','Home presentation replaced without page reload','other app retains its mount and confirmed state','same WebSocket sessions','compatible keyboard focus retained','later Home navigation uses updated catalog'],release:await evaluate(home,"document.querySelector('#revision').dataset.release")};
    await writeFile(join(output,'home-update-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else{
  const a=await page(1280,800,'home');await screenshot(a,'home-landscape');
  const homeWide=(await controls(a)).filter(c=>c.label!=='Edit cards');assert.equal(homeWide.length,6);assert(Math.abs(homeWide[0].bounds[1]-homeWide[2].bounds[1])<1);
  await click(a,'Component Playground','component-playground','1280x800');
  const wide=await controls(a);await drag(a,'Linear slider',0.81);
  await wait(a,"Number([...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow'))>75");
  const b=await page(800,1280,'component-playground');
  const tall=await controls(b);assert.deepEqual(tall.map(c=>[c.id,c.label]),wide.map(c=>[c.id,c.label]));
  const pad=tall.find(c=>c.label==='Touch pad'),dial=tall.find(c=>c.label==='Rotary dial');assert(pad.bounds[1]>dial.bounds[1]+dial.bounds[3]);
  const expected=(await controls(a)).find(c=>c.label==='Linear slider').value;
  await wait(b,`[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow')===${JSON.stringify(expected)}`);
  const baselineInk=inkReceived.get(a)||0,[px,py,pw,ph]=pad.bounds;
  await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:px+pw*.2,y:py+ph*.7},b);
  await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,x:px+pw*.7,y:py+ph*.3},b);
  await new Promise(r=>setTimeout(r,200));
  await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:px+pw*.7,y:py+ph*.3},b);
  for(let i=0;i<100&&(inkReceived.get(a)||0)<=baselineInk;i++)await new Promise(r=>setTimeout(r,50));
  assert((inkReceived.get(a)||0)>baselineInk,'portrait ink reaches the landscape client over the shared session');
  await screenshot(a,'playground-landscape');await screenshot(b,'playground-portrait');
  await evaluate(b,"[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').focus()");
  const focused=await evaluate(b,'document.activeElement.dataset.sceneNode');
  await metrics(b,1280,800);await ready(b,'component-playground','1280x800');
  assert.equal(await evaluate(b,'document.activeElement.dataset.sceneNode'),focused,'orientation preserves semantic keyboard focus');
  await metrics(b,800,1280);await ready(b,'component-playground','800x1280');
  await drag(b,'Rotary dial',0.5,false);await metrics(b,1280,800);await ready(b,'component-playground','1280x800');
  await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:200,y:200},b);
  assert.deepEqual((await controls(b)).map(c=>[c.id,c.label]),wide.map(c=>[c.id,c.label]));
  await metrics(b,800,1280);await ready(b,'component-playground','800x1280');
  await drag(b,'Linear slider',0.43);await wait(b,"Number([...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow'))<50");
  await wait(a,"Number([...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Linear slider').getAttribute('aria-valuenow'))<50");
  await click(b,'Home','home','800x1280');const homeTall=(await controls(b)).filter(c=>c.label!=='Edit cards');assert(Math.abs(homeTall[0].bounds[1]-homeTall[1].bounds[1])<1);assert(homeTall[2].bounds[1]>homeTall[1].bounds[1]);await screenshot(b,'home-portrait');
  await click(b,'Layout Lab','layout-lab','800x1280');await screenshot(b,'layout-lab-portrait');
  await click(a,'Home','home','1280x800');await click(a,'Layout Lab','layout-lab','1280x800');await screenshot(a,'layout-lab-landscape');
  const result={passed:true,checks:['landscape and portrait WebGL rendering','Home three/two-column composition','same controls and behavior identities','two clients share edits and ink across orientations','resize during pointer capture and subsequent input','Layout Lab in both orientations'],screenshots:output,native_hardware:'not tested'};
  await writeFile(join(output,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));
  }
}finally{
  socket?.close();if(browser.pid&&browser.exitCode===null&&browser.signalCode===null){browser.kill('SIGTERM');await new Promise(resolve=>browser.once('exit',resolve));}await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
