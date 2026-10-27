// SPDX-License-Identifier: Apache-2.0
// Optional real Chromium/WebGL acceptance against a disposable local authority.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,readFile,writeFile,rm} from 'node:fs/promises';
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
  const wait=async(session,expression)=>{for(let i=0;i<200;i++){if(await evaluate(session,expression))return;await new Promise(r=>setTimeout(r,100));}const shot=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(join(output,'timeout.png'),Buffer.from(shot.data,'base64'));await writeFile(join(output,'timeout.json'),JSON.stringify(await evaluate(session,"({path:location.pathname,revision:{...document.querySelector('#revision')?.dataset},timings:document.querySelector('#timing-report')?.value,inputs:[...document.querySelectorAll('.canvas-editor')].filter(e=>e.type!=='password').map(e=>({label:e.getAttribute('aria-label'),value:e.value,id:e.dataset.sceneNode}))})"),null,2));throw Error('Browser condition timed out: '+expression+'; '+await evaluate(session,"document.querySelector('#error')?.textContent"));};
  const metrics=async(session,width,height)=>{await call('Page.bringToFront',{},session);await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false},session);};
  const testQuery=new URLSearchParams();if(args.includes('--timings'))testQuery.set('timings','1');if(args.includes('--stable-quads'))testQuery.set('stable-quads','1');
  const routes={'component-playground':'test-playground/components','layout-lab':'test-playground/layouts','layout-document':'test-playground/long-document','layout-text-scroll':'test-playground/text-panning','input-lab':'test-playground/text-input','animation-lab':'test-playground/animations-two-d','three-d-lab':'test-playground/animations-three-d'};
  const page=async(width,height,route,query=testQuery.size?'?'+testQuery:'')=>{const {targetId}=await call('Target.createTarget',{url:'about:blank'});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});await call('Page.enable',{},sessionId);await call('Runtime.enable',{},sessionId);await call('Network.enable',{},sessionId);await metrics(sessionId,width,height);await call('Page.navigate',{url:origin+'/'+(route==='home'?'':routes[route]||route)+query},sessionId);await ready(sessionId,route,['input-lab','animation-lab','three-d-lab'].includes(route)?'1280x800':width<height?'800x1280':'1280x800');return sessionId;};
  const ready=async(session,screen,extent)=>{await wait(session,`document.querySelector('#revision')?.dataset.screen===${JSON.stringify(screen)}${extent?`&&document.querySelector('#view')?.dataset.logicalSize===${JSON.stringify(extent)}`:''}`);assert.equal(await evaluate(session,"document.querySelector('#error').hidden"),true);};
  const controls=session=>evaluate(session,"[...document.querySelectorAll('#controls button')].map(b=>({id:b.dataset.sceneNode,label:b.getAttribute('aria-label'),value:b.getAttribute('aria-valuenow'),bounds:[b.getBoundingClientRect().x,b.getBoundingClientRect().y,b.getBoundingClientRect().width,b.getBoundingClientRect().height]}))");
  const click=async(session,label,screen,extent)=>{let control=(await controls(session)).find(c=>c.label===label);if(!control&&(await controls(session)).some(c=>c.label==='Test Playground')){await click(session,'Test Playground','test-playground',extent);control=(await controls(session)).find(c=>c.label===label);}assert(control,label);const [x,y,w,h]=control.bounds;
    await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:x+w/2,y:y+h/2},session);
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:x+w/2,y:y+h/2},session);
    await ready(session,screen,['input-lab','animation-lab','three-d-lab'].includes(screen)?'1280x800':extent);};
  const screenshot=async(session,name)=>{await call('Page.bringToFront',{},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');const {data}=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(join(output,name+'.png'),Buffer.from(data,'base64'));};
  const drag=async(session,label,fraction,release=true)=>{const control=(await controls(session)).find(c=>c.label===label);assert(control);const [x,y,w,h]=control.bounds,point={x:x+w*fraction,y:y+h/2};await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...point},session);if(release)await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...point},session);};
  if(args.includes('--playground')){
    const expected=[['Components','component-playground'],['Layouts','layout-lab'],['Long Document','layout-document'],['Text Panning','layout-text-scroll'],['Text Input','input-lab'],['2D Animations','animation-lab'],['3D Animations','three-d-lab']];
    const a=await page(1280,800,'home');
    for(const [width,height]of [[1280,800],[800,1280]]){
      await metrics(a,width,height);await ready(a,'home',width+'x'+height);
      assert.deepEqual((await controls(a)).map(c=>c.label),['The Brass Relay','Test Playground','Monitoring']);
      assert(!await evaluate(a,"document.querySelector('#view').getAttribute('aria-label').includes('OPEN')"));
      await screenshot(a,'home-'+width);
      await click(a,'Test Playground','test-playground',width+'x'+height);
      assert.deepEqual((await controls(a)).filter(c=>!['Edit cards','Home'].includes(c.label)).map(c=>c.label),expected.map(x=>x[0]));
      const bounds=await evaluate(a,"({w:innerWidth,h:innerHeight})");
      for(const c of await controls(a)){const[x,y,w,h]=c.bounds;assert(x>=0&&y>=0&&x+w<=bounds.w+1&&y+h<=bounds.h+1,JSON.stringify(c));}
      await screenshot(a,'test-playground-'+width);
      for(const[label,screen]of expected){
        await click(a,label,screen,width+'x'+height);
        assert.equal(await evaluate(a,"document.querySelector('#revision').dataset.screen"),screen);
        if(screen==='layout-lab')assert(!(await controls(a)).some(c=>['Long Document','Text Panning','Long Document','Text Panning'].includes(c.label)));
        await click(a,'Back','test-playground',width+'x'+height);
      }
      await click(a,'Home','home',width+'x'+height);
    }
    const result={passed:true,checks:['Home has Brass Relay, Test Playground and Monitoring','seven demo cards with compact content','all cards fit portrait and landscape','all demo navigation and parent Back','Layouts has no extra demo links','Edit belongs to Test Playground']};
    await writeFile(join(output,'playground-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--touch-replay')){
    const a=await page(1280,800,'component-playground'),results=[];
    const replay=async(session,name)=>{
      const fixture=JSON.parse(await readFile(new URL('../crates/otrios-core/tests/fixtures/touch/'+name+'.json',import.meta.url),'utf8'));
      const rect=await evaluate(session,"(()=>{const r=document.querySelector('#view').getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};})()");
      await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:5},session);
      const start=Date.now()+20;
      for(const p of fixture.points){
        const delay=start+p.at_ms-Date.now();if(delay>0)await new Promise(r=>setTimeout(r,delay));
        const type=['touchStart','touchMove','touchEnd','touchCancel'][p.phase];
        const touchPoints=p.phase<2?[{id:p.id,x:rect.x+p.position[0]*rect.w/1280,y:rect.y+p.position[1]*rect.h/800}]:[];
        await call('Input.dispatchTouchEvent',{type,touchPoints,timestamp:(start+p.at_ms)/1000},session);
      }
    };
    for(const [name,label,target]of [['slider-lift','Linear slider',70],['pad-lift','Touch pad',70],['dial-lift','Rotary dial',37.5]]){
      await replay(a,name);
      await wait(a,`[...document.querySelectorAll('#controls button')].some(b=>b.getAttribute('aria-label')===${JSON.stringify(label)}&&Math.abs(Number(b.getAttribute('aria-valuenow'))-${target})<2.5)`);
      const result=(await controls(a)).find(c=>c.label===label);results.push({fixture:name,value:Number(result.value)});
    }
    await screenshot(a,'touch-controls');
    const b=await page(1280,800,'layout-text-scroll');await replay(b,'scroll-slow-lift');
    await wait(b,"JSON.parse(document.querySelector('#revision').dataset.textScroll||'{}').offset?.[0]>=16&&!JSON.parse(document.querySelector('#revision').dataset.textScroll).active");
    const slow=await evaluate(b,"JSON.parse(document.querySelector('#revision').dataset.textScroll)");assert(slow.offset[0]<45);results.push({fixture:'scroll-slow-lift',...slow});
    const c=await page(1280,800,'layout-document');await wait(c,"document.querySelector('#revision').dataset.documentLoading==='false'");await replay(c,'document-repeat');
    // CDP itself can deliver after the recorded release, so the published
    // instantaneous speed may already have decayed. Total travel distinguishes
    // boost (>3300 px) from five unboosted strokes (<2700 px).
    await wait(c,"JSON.parse(document.querySelector('#revision').dataset.textScroll||'{}').offset>1600&&!JSON.parse(document.querySelector('#revision').dataset.textScroll).active");
    const repeated=await evaluate(c,"JSON.parse(document.querySelector('#revision').dataset.textScroll)");assert(repeated.offset>3300&&repeated.offset<3650&&!repeated.loading);results.push({fixture:'document-repeat',...repeated});await screenshot(c,'touch-repeated-strokes');
    const result={passed:true,checks:['real timed touch replay','slider filtered lift','pad filtered lift','dial filtered lift','slow scroll without release kick','repeated strokes exceed single-stroke velocity'],results};
    await writeFile(join(output,'acceptance.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--document')){
    const a=await page(1280,800,'test-playground');
    await click(a,'Long Document','layout-document','1280x800');
    const info=()=>evaluate(a,"JSON.parse(document.querySelector('#revision').dataset.textScroll||'null')");
    await wait(a,"!!JSON.parse(document.querySelector('#revision').dataset.textScroll||'null')?.anchor&&document.querySelector('#revision').dataset.documentLoading==='false'");
    const initial=await info();assert(initial.source_bytes>=1024*1024);assert.equal(initial.offset,0);assert(initial.working_bytes<=8*1024*1024);
    assert.equal(await evaluate(a,'location.pathname'),'/test-playground/long-document');await screenshot(a,'document-start');
    const rect=await evaluate(a,"(()=>{const r=document.querySelector('#view').getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};})()");
    const point=(x,y)=>({id:1,x:rect.x+x*rect.w/1280,y:rect.y+y*rect.h/800});
    await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:5},a);
    await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[point(900,600)]},a);
    for(let i=1;i<=6;i++){await call('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[point(900,600-i*40)]},a);await new Promise(r=>setTimeout(r,30));}
    const dragged=await info();assert(dragged.offset>100);
    // Keep inspection round trips out of the lift-off interval. A deliberately
    // paused release should stop; this case qualifies a fresh moving release.
    await call('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[point(900,320)]},a);
    await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]},a);
    await wait(a,`JSON.parse(document.querySelector('#revision').dataset.textScroll).offset>${dragged.offset+60}`);
    await wait(a,"!JSON.parse(document.querySelector('#revision').dataset.textScroll).active");
    const coast=await info();assert(coast.working_bytes<=8*1024*1024);assert.equal(coast.loading,false);
    await screenshot(a,'document-coasted');
    const b=await page(1280,800,'layout-document');await wait(b,"document.querySelector('#revision').dataset.documentLoading==='false'");
    assert.equal((await evaluate(b,"JSON.parse(document.querySelector('#revision').dataset.textScroll)")).offset,0,'document viewport is local to its client');
    await metrics(a,800,1280);await ready(a,'layout-document','800x1280');
    await wait(a,"document.querySelector('#revision').dataset.documentLoading==='false'&&!JSON.parse(document.querySelector('#revision').dataset.textScroll).loading");
    const resized=await info();assert(Math.abs(resized.anchor.viewport_y-coast.anchor.viewport_y)<0.01,'reflow preserves the anchor’s viewport position');assert.equal(resized.anchor.block,coast.anchor.block);assert(resized.anchor.source_byte<=coast.anchor.source_byte);assert(coast.anchor.source_byte-resized.anchor.source_byte<400);
    await screenshot(a,'document-portrait');
    // Lose and restore the real WebGL context without replacing the authority,
    // source or app state. The resource ACK map must request a complete replay.
    for(let restore=0;restore<3;restore++){
    await evaluate(a,"window.documentLoss=document.querySelector('#view').getContext('webgl').getExtension('WEBGL_lose_context');documentLoss.loseContext()");
    await wait(a,"document.querySelector('#revision').dataset.graphics==='lost'");
    await evaluate(a,'documentLoss.restoreContext()');
    await wait(a,"document.querySelector('#revision').dataset.graphics==='ready'&&document.querySelector('#revision').dataset.documentLoading==='false'");
    await screenshot(a,'document-context-restored');assert.equal(await evaluate(a,"document.querySelector('#error').hidden"),true,await evaluate(a,"document.querySelector('#error').textContent"));}
    await click(a,'Back','test-playground','800x1280');
    await wait(a,"document.querySelector('#revision').dataset.documentResources==='0'");
    const result={passed:true,checks:['1 MiB source admission','touch drag and momentum','bounded working memory','independent client viewports','source anchor across reflow','context-loss resource replay','navigation retires document resources'],initial,dragged,coast,resized,screenshots:output};
    await writeFile(join(output,'acceptance.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));process.exitCode=0;
  }else if(args.includes('--text-scroll')){
    const a=await page(1280,800,'test-playground');
    await click(a,'Text Panning','layout-text-scroll','1280x800');
    await wait(a,"JSON.parse(document.querySelector('#revision').dataset.textScroll||'{}').limit?.every(v=>v>0)");
    assert.equal(await evaluate(a,'location.pathname'),'/test-playground/text-panning');
    const info=()=>evaluate(a,"JSON.parse(document.querySelector('#revision').dataset.textScroll)");
    const initial=await info();assert(initial.limit.every(v=>v>0));assert.deepEqual(initial.offset,[0,0]);
    await screenshot(a,'text-scroll-start');
    const rect=await evaluate(a,"(()=>{const r=document.querySelector('#view').getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};})()");
    const point=(x,y)=>({id:1,x:rect.x+x*rect.w/1280,y:rect.y+y*rect.h/800});
    await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:5},a);
    await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[point(1000,550)]},a);
    for(let i=1;i<=4;i++){
      await call('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[point(1000-i*45,550-i*45)]},a);
      await new Promise(r=>setTimeout(r,40));
    }
    await wait(a,"JSON.parse(document.querySelector('#revision').dataset.textScroll).offset.every(v=>v>100)");
    const dragged=await info();
    await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]},a);
    await new Promise(r=>setTimeout(r,120));const coast=await info();
    assert(coast.offset.every((v,i)=>v>dragged.offset[i]),'both axes coast after release');
    await wait(a,"/\\d[.]\\d fps/.test(JSON.parse(document.querySelector('#revision').dataset.scrollStats||'{}').label||'')");
    const fps=await evaluate(a,"JSON.parse(document.querySelector('#revision').dataset.scrollStats)");
    await screenshot(a,'text-scroll-coasting');
    await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[point(900,500)]},a);
    await call('Input.dispatchTouchEvent',{type:'touchCancel',touchPoints:[]},a);
    await wait(a,"!JSON.parse(document.querySelector('#revision').dataset.textScroll).active");
    const stopped=await info();await new Promise(r=>setTimeout(r,250));assert.deepEqual((await info()).offset,stopped.offset,'cancel stops momentum');
    const b=await page(1280,800,'test-playground');await click(b,'Text Panning','layout-text-scroll','1280x800');
    await wait(b,"!!document.querySelector('#revision').dataset.textScroll");
    assert.deepEqual(await evaluate(b,"JSON.parse(document.querySelector('#revision').dataset.textScroll).offset"),[0,0],'panning is client-local');
    await metrics(a,800,1280);await ready(a,'layout-text-scroll','800x1280');
    const portrait=await info();assert(portrait.offset.every((v,i)=>v>=0&&v<=portrait.limit[i]));
    await screenshot(a,'text-scroll-portrait');
    const cache=await evaluate(a,"JSON.parse(document.querySelector('#revision').dataset.fontCache)");assert(cache.bytes<=4194304,'bounded glyph cache');
    await click(a,'Back','test-playground','800x1280');await click(a,'Home','home','800x1280');
    const result={passed:true,checks:['Test Playground owns demo routes','full text page uses real WebGL','two-axis touch drag and release momentum','cancel stops coasting','panning is client-local','portrait reflow and bounds','Back and Home navigation'],initial,dragged,coast,portrait,fps,cache};
    await writeFile(join(output,'text-scroll-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--input-viewport')){
    const a=await page(1280,800,'input-lab'),b=await page(800,1280,'input-lab');
    const visibleWait=async(s,expression)=>{await call('Page.bringToFront',{},s);return wait(s,expression);};
    const input="document.querySelector('.canvas-editor[aria-label=\"First text field\"]')";
    await call('Page.bringToFront',{},a);
    await click(a,'First text field','input-lab','1280x800');
    await visibleWait(a,"document.activeElement?.classList.contains('canvas-editor')");
    assert(!await evaluate(a,"[...document.querySelectorAll('#controls button')].some(b=>b.getAttribute('aria-label')==='SPACE')"));
    const key=async(key,modifiers=0)=>{await call('Page.bringToFront',{},a);await call('Input.dispatchKeyEvent',{type:'keyDown',key,modifiers,windowsVirtualKeyCode:{Backspace:8,Tab:9,a:65}[key],...(key==='a'&&modifiers?{commands:['selectAll']}:{})},a);await call('Input.dispatchKeyEvent',{type:'keyUp',key,modifiers},a);};
    await key('a',4);await key('Backspace');
    const source=Array.from({length:60},(_,i)=>`W${String(i).padStart(2,'0')} `).join('');
    await call('Input.insertText',{text:source},a);
    await visibleWait(a,`${input}.selectionStart===${source.length}&&${input}.scrollLeft>0`);
    await visibleWait(b,`${input}.value===${JSON.stringify(source)}`);
    await screenshot(a,'input-caret-end');
    const geometry=await evaluate(a,`(()=>{const e=${input},r=e.getBoundingClientRect(),b=document.querySelector('#controls button[aria-label="First text field"]').getBoundingClientRect();return {delta:Math.abs(r.width-b.width),font:parseFloat(getComputedStyle(e).fontSize)};})()`);
    assert(geometry.delta<1);assert(geometry.font>=16,'computed font avoids iOS focus auto-zoom');
    await call('Page.bringToFront',{},a);
    await evaluate(a,`(()=>{const e=${input};e.setSelectionRange(4,7);e.dispatchEvent(new Event('select'));})()`);
    await call('Input.insertText',{text:'ABC'},a);
    const expected=source.slice(0,4)+'ABC'+source.slice(7);
    await visibleWait(b,`${input}.value===${JSON.stringify(expected)}`);
    await key('a',4);await call('Input.insertText',{text:'é'.repeat(200)},a);
    await visibleWait(a,`${input}.value===${JSON.stringify(expected)}`);
    await key('Tab');await visibleWait(a,"document.activeElement?.getAttribute('aria-label')==='Second text field'");
    await call('Input.insertText',{text:'Second field'},a);
    await visibleWait(b,"document.querySelector('.canvas-editor[aria-label=\"Second text field\"]').value.includes('Second field')");
    await screenshot(a,'input-native-selection');
    await metrics(a,390,844);await ready(a,'input-lab','1280x800');
    await click(a,'First text field','input-lab','1280x800');await metrics(a,390,420);
    await visibleWait(a,`(()=>{const r=${input}.getBoundingClientRect();return r.top>=0&&r.bottom<=innerHeight+1;})()`);
    await screenshot(a,'input-mobile-keyboard-space');
    const result={passed:true,checks:['real native input focus and selection replacement','long text uses native horizontal viewport','field fills its authored bounds','bounded UTF-8 rejection reconciles the DOM','two fields sync committed text between clients','native Tab focus','no app keyboard','field remains inside reduced mobile viewport'],geometry};
    await writeFile(join(output,'input-viewport-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--timings')){
    const a=await page(800,1280,'home');
    const report=()=>evaluate(a,"JSON.parse(document.querySelector('#timing-report').value)");
    await wait(a,"document.querySelector('#timing-report')?.value.includes('startup')");
    assert.equal((await report()).samples[0].kind,'startup');
    const results=[];
    for(let pass=0;pass<2;pass++)for(const [label,screen] of [['Components','component-playground'],['Layouts','layout-lab'],['Text Input','input-lab'],['2D Animations','animation-lab'],['3D Animations','three-d-lab']]){
      const before=(await report()).samples.length;
      await click(a,label,screen);
      await wait(a,`JSON.parse(document.querySelector('#timing-report').value).samples.length>${before}`);
      const sample=(await report()).samples.at(-1);assert.equal(sample.screen,screen);assert(sample.totalMs>0);assert(Number.isFinite(sample.ms.queue));assert(Number.isFinite(sample.ms.responseWait));results.push({pass,...sample});
      const count=(await report()).samples.length;
      if(screen==='input-lab'){await click(a,'First text field',screen);await evaluate(a,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');assert.equal((await report()).samples.length,count,'editing local state is not navigation');}
      await click(a,'Home','home','800x1280');
      await wait(a,`JSON.parse(document.querySelector('#timing-report').value).samples.length>${count}`);
    }
    for(const sample of results.filter(s=>s.pass===1)){const first=results.find(s=>s.pass===0&&s.screen===sample.screen);assert(sample.responseBytes<first.responseBytes,`${sample.screen}: cached mount must save bytes`);assert(sample.uiCacheHits>first.uiCacheHits);}
    assert.equal(await evaluate(a,"location.search"),'?timings=1');
    await screenshot(a,'portrait-timings');
    const clean=await page(1280,800,'home','');assert.equal(await evaluate(clean,"document.querySelector('#navigation-timings')"),null,'default page has no diagnostic panel');assert.equal(await evaluate(clean,"performance.getEntriesByType('resource').some(e=>e.name.includes('navigation-timings.mjs'))"),false,'default page does not fetch timing code');
    const result={passed:true,results,report:await report(),checks:['startup and tap through draw plus next RAF','phase timings across real worker and WebSocket','all five public apps have smaller repeat mount responses','local edits do not record navigation','query survives navigation','no panel by default']};
    await writeFile(join(output,'timings-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--wait-route-rename')){
    const a=await page(800,1280,'layout-lab'),owner=await page(1280,800,'home');
    await call('Page.bringToFront',{},a);
    const snapshot=()=>evaluate(a,"({path:location.pathname,time:performance.timeOrigin,release:document.querySelector('#revision').dataset.release,length:history.length,epoch:document.querySelector('#revision').dataset.epoch,state:document.querySelector('#revision').dataset.state,ui:document.querySelector('#revision').dataset.ui})");
    const before=await snapshot();const sockets=socketCount.get(a);
    await writeFile(join(output,'route-rename-ready.json'),JSON.stringify(before));console.log('READY for an app-owned parent rename');
    await wait(a,"location.pathname==='/playground/layouts'",90000);const after=await snapshot();
    assert.equal(after.time,before.time);assert.equal(after.length,before.length);assert.equal(after.epoch,before.epoch);assert.equal(after.state,before.state);assert.equal(after.ui,before.ui);assert.equal(socketCount.get(a),sockets);
    await click(a,'Home','home','800x1280');await click(a,'Layouts','layout-lab','800x1280');await wait(a,"location.pathname==='/playground/layouts'");
    await call('Page.reload',{},a);await ready(a,'layout-lab','800x1280');assert.equal(await evaluate(a,'location.pathname'),'/playground/layouts');
    await wait(owner,`document.querySelector('#revision').dataset.release!==${JSON.stringify(before.release)}`);await call('Page.bringToFront',{},owner);
    await click(owner,'Layouts','layout-lab','1280x800');await wait(owner,"location.pathname==='/playground/layouts'");
    const result={passed:true,before,after,checks:['owner UI hot-swaps and launches its renamed route','owner rename rebases an active app URL','same UI, state, epoch, page and WebSocket','rename replaces the current history entry','parent Back and refresh use the new owner path']};
    await writeFile(join(output,'route-rename-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--wait-route-update')){
    const a=await page(1280,800,'layout-lab');
    const before=await evaluate(a,"({path:location.pathname,time:performance.timeOrigin,release:document.querySelector('#revision').dataset.release,length:history.length})");
    await writeFile(join(output,'route-update-ready.json'),JSON.stringify(before));console.log('READY for runtime update at a screen URL');
    let updated=false;for(let i=0;i<900;i++){if(await evaluate(a,`document.querySelector('#revision')?.dataset.screen==='layout-lab'&&document.querySelector('#revision').dataset.release!==${JSON.stringify(before.release)}`)){updated=true;break;}await new Promise(r=>setTimeout(r,100));}
    assert(updated,'screen reopens after an executable release');const after=await evaluate(a,"({path:location.pathname,time:performance.timeOrigin,release:document.querySelector('#revision').dataset.release,length:history.length,hash:location.hash})");
    assert.equal(after.path,before.path);assert.equal(after.hash,'');assert.equal(after.length,before.length);assert.notEqual(after.time,before.time);assert.equal(hellos.get(a).release,after.release);
    const result={passed:true,before,after,checks:['full upgrades reload the same screen URL','history length stays fixed','hello carries the new module release']};await writeFile(join(output,'route-update-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--navigation')){
    const a=await page(1280,800,'input-lab'),snapshot=()=>evaluate(a,"({path:location.pathname,hash:location.hash,time:performance.timeOrigin,release:document.querySelector('#revision').dataset.release,length:history.length})");
    const initial=await snapshot();assert.equal(initial.path,'/test-playground/text-input');assert.equal(initial.hash,'');assert.equal(hellos.get(a).release,initial.release,'hello identifies the module release at a clean URL');
    await click(a,'First text field','input-lab','1280x800');assert.equal((await snapshot()).path,initial.path);assert.equal((await snapshot()).length,initial.length,'local keyboard creates no navigation entry');
    await click(a,'Home','home','1280x800');assert.equal((await snapshot()).path,'/');
    await click(a,'Components','component-playground','1280x800');assert.equal((await snapshot()).path,'/test-playground/components');
    const sockets=socketCount.get(a);
    for(const [action,screen]of [['back','test-playground'],['back','home'],['back','input-lab'],['forward','home'],['forward','test-playground'],['forward','component-playground']]){
      await evaluate(a,`history.${action}()`);await ready(a,screen,'1280x800');await wait(a,`location.pathname===${JSON.stringify(screen==='home'?'/':'/'+(routes[screen]||screen))}`);
    }
    assert.equal((await snapshot()).time,initial.time);assert.equal(socketCount.get(a),sockets,'screen navigation keeps the same page and session');
    await call('Page.reload',{},a);await ready(a,'component-playground','1280x800');assert.equal((await snapshot()).path,'/test-playground/components');assert.equal(hellos.get(a).release,(await snapshot()).release);
    await call('Page.navigate',{url:origin+'/monitoring'},a);await wait(a,"document.querySelector('#monitoring-login').open");assert.equal((await snapshot()).path,'/monitoring');assert.equal(await evaluate(a,"(async()=> (await fetch('/monitoring/presentation')).status)()"),401);
    await evaluate(a,"document.querySelector('#login-back').click()");await ready(a,'home','1280x800');await wait(a,"location.pathname==='/'");assert.equal(await evaluate(a,"document.querySelector('#monitoring-login').open"),false);
    await click(a,'Layouts','layout-lab','1280x800');
    assert(!(await controls(a)).some(c=>['Signal screen','Cards screen'].includes(c.label)));
    await call('Page.reload',{},a);await ready(a,'layout-lab','1280x800');assert.equal((await snapshot()).path,'/test-playground/layouts');
    const portrait=await page(800,1280,'layout-lab');
    await click(portrait,'Home','home','800x1280');
    assert.equal(await evaluate(portrait,'location.pathname'),'/');
    for(const retired of ['/test-playground/layouts/signal','/test-playground/layouts/cards'])assert.equal(await evaluate(a,`(async()=> (await fetch(${JSON.stringify(retired)})).status)()`),404);
    await call('Page.bringToFront',{},a);
    const resources=await evaluate(a,"performance.getEntriesByType('resource').filter(r=>r.name.endsWith('/app.js')).map(r=>r.name)");assert(resources.every(u=>/\/releases\/[a-f0-9]{64}\/web\/app.js$/.test(u)));
    assert.equal(await evaluate(a,"(async()=> (await fetch('/not-an-app')).status)()"),404);assert.equal(await evaluate(a,"document.querySelector('#error').hidden"),true);
    const result={passed:true,checks:['direct app links with refresh','retired Layouts subroutes stay unavailable','portrait Layouts and Home navigation','Back/Forward without page/session replacement','local keyboard leaves URL/history unchanged','release hello uses the pinned module identity','Monitoring URL preserves sign-in boundary','executable modules retain immutable URLs']};
    await writeFile(join(output,'navigation-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else if(args.includes('--home-reorder')||args.includes('--playground-reorder')){
    const a=await page(800,1280,'test-playground'),b=await page(1280,800,'test-playground');
    await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:5},a);
    const cards=async session=>(await controls(session)).filter(c=>!['Edit cards','Done editing cards','Home'].includes(c.label)).sort((a,b)=>a.bounds[1]-b.bounds[1]||a.bounds[0]-b.bounds[0]);
    const order=async session=>(await cards(session)).map(c=>c.label);
    const orderExpression=`[...document.querySelectorAll('#controls button')].filter(b=>!['Edit cards','Done editing cards','Home'].includes(b.getAttribute('aria-label'))).sort((a,b)=>a.getBoundingClientRect().y-b.getBoundingClientRect().y||a.getBoundingClientRect().x-b.getBoundingClientRect().x).map(b=>b.getAttribute('aria-label'))`;
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
    assert.deepEqual(await order(a),await order(b));assert.equal(await evaluate(a,"document.querySelector('#revision').dataset.screen"),'test-playground');
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
    await metrics(a,1280,800);await ready(a,'test-playground','1280x800');console.log('Ending rotated touch');await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]},a);
    await evaluate(a,"[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Done editing cards').click()");
    await wait(a,"!![...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Edit cards')");
    await new Promise(r=>setTimeout(r,350));assert.equal(await evaluate(a,"document.querySelector('#revision').dataset.screen"),'test-playground');assert.deepEqual(await order(a),await order(b));
    await click(a,'Components','component-playground','1280x800');
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
    await screenshot(active,'behavior-after');await click(other,'Components','component-playground','1280x800');
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
    assert.equal(await evaluate(home,'location.pathname'),'/');assert.equal(await evaluate(other,'location.pathname'),'/test-playground/components');
    assert.equal(await evaluate(home,'document.activeElement.dataset.sceneNode'),before.home.focus);
    assert.equal(await evaluate(other,"document.querySelector('#revision').dataset.epoch"),before.other.epoch);
    assert.deepEqual([socketCount.get(home),socketCount.get(other)],before.sockets,'same sessions remain connected');
    assert(Number((await controls(other)).find(c=>c.label==='Linear slider').value)>75,'other app retains its confirmed value');
    console.log('Capturing updated Home');await screenshot(home,'home-after-update');
    console.log('Opening Home on other client');await call('Page.bringToFront',{},other);
    await click(other,'Home','home','1280x800');
    await wait(other,"document.querySelector('#view').getAttribute('aria-label').includes('Explore the demos')");
    console.log('Refreshing Home');await call('Page.bringToFront',{},home);await call('Page.reload',{},home);await ready(home,'home','800x1280');
    await wait(home,"document.querySelector('#view').getAttribute('aria-label').includes('Explore the demos')");
    const result={passed:true,checks:['explicit refresh opens the current release','Home presentation replaced without page reload','other app retains its mount and confirmed state','same WebSocket sessions','compatible keyboard focus retained','later Home navigation uses updated catalog'],release:await evaluate(home,"document.querySelector('#revision').dataset.release")};
    await writeFile(join(output,'home-update-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else{
  const a=await page(1280,800,'home');await screenshot(a,'home-landscape');
  const homeWide=(await controls(a)).filter(c=>c.label!=='Edit cards');assert.deepEqual(homeWide.map(c=>c.label),['The Brass Relay','Test Playground','Monitoring']);
  await click(a,'Components','component-playground','1280x800');
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
  await click(b,'Home','home','800x1280');const homeTall=(await controls(b)).filter(c=>c.label!=='Edit cards');assert(Math.abs(homeTall[0].bounds[1]-homeTall[1].bounds[1])<1);assert.equal(homeTall.length,3);await screenshot(b,'home-portrait');
  await click(b,'Layouts','layout-lab','800x1280');await screenshot(b,'layout-lab-portrait');
  await click(a,'Home','home','1280x800');await click(a,'Layouts','layout-lab','1280x800');await screenshot(a,'layout-lab-landscape');
  const result={passed:true,checks:['landscape and portrait WebGL rendering','three-card Home composition','same controls and behavior identities','two clients share edits and ink across orientations','resize during pointer capture and subsequent input','Layouts in both orientations'],screenshots:output,native_hardware:'not tested'};
  await writeFile(join(output,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));
  }
}finally{
  socket?.close();if(browser.pid&&browser.exitCode===null&&browser.signalCode===null){browser.kill('SIGTERM');await new Promise(resolve=>browser.once('exit',resolve));}await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
