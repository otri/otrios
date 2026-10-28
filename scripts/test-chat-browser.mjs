// SPDX-License-Identifier: Apache-2.0
// Optional real Chromium/WebGL acceptance against a disposable local authority.
import assert from 'node:assert/strict';
import {chatViewport} from '../web/shared/viewport.mjs';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
const args=process.argv.slice(2),argument=(name,fallback)=>{const i=args.indexOf(name);return i<0?fallback:args[i+1];};
if(typeof WebSocket!=='function')throw Error('Node with built-in WebSocket support is required');
const origin=argument('--url');if(!origin)throw Error('Supply --url for a disposable authority');
const output=resolve(argument('--output','target/chat-browser')),profile=await mkdtemp(join(tmpdir(),'otrios-browser-'));
await mkdir(output,{recursive:true});
const browser=spawn(argument('--chromium','chromium'),['--headless=new','--no-sandbox','--disable-dev-shm-usage','--disable-background-timer-throttling','--disable-features=CalculateNativeWinOcclusion','--disable-renderer-backgrounding','--enable-unsafe-swiftshader','--use-angle=swiftshader','--remote-debugging-port=0',`--user-data-dir=${profile}`,'--no-first-run','about:blank'],{stdio:['ignore','ignore','pipe']});
let socket;
try{
  const endpoint=await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(Error('Chromium did not start')),30000);let log='';
    browser.stderr.on('data',bytes=>{log+=bytes;const match=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(match){clearTimeout(timer);resolve(match[1]);}});
    browser.once('error',error=>{clearTimeout(timer);reject(error);});browser.once('exit',code=>{clearTimeout(timer);reject(Error(`Chromium exited ${code}: ${log.slice(-1200)}`));});
  });
  socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
  let serial=0,privateSentBody;const pending=new Map(),inkReceived=new Map(),socketCount=new Map(),sentFrames=new Map(),sentBytes=new Map(),hellos=new Map(),receivedChannels=new Map(),bodyLeaks=[];
  socket.addEventListener('message',({data})=>{const m=JSON.parse(data);if(m.id){const task=pending.get(m.id);if(task){pending.delete(m.id);clearTimeout(task.timer);if(m.error)task.reject(Error(JSON.stringify(m.error)));else task.resolve(m.result);}}
    else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===1){try{const h=JSON.parse(m.params.response.payloadData);if(h.type==='hello')hellos.set(m.sessionId,h);}catch{}}
    else if(m.method==='Network.webSocketFrameSent'&&m.params.response.opcode===2){sentFrames.set(m.sessionId,(sentFrames.get(m.sessionId)||0)+1);sentBytes.set(m.sessionId,(sentBytes.get(m.sessionId)||0)+Buffer.from(m.params.response.payloadData,'base64').length);}
    else if(m.method==='Network.webSocketCreated')socketCount.set(m.sessionId,(socketCount.get(m.sessionId)||0)+1);
    else if(m.method==='Network.webSocketFrameReceived'&&m.params.response.opcode===2){const bytes=Buffer.from(m.params.response.payloadData,'base64'),channel=bytes[0],key=m.sessionId+':'+channel,old=receivedChannels.get(key)||{frames:0,bytes:0};receivedChannels.set(key,{frames:old.frames+1,bytes:old.bytes+bytes.length});if(channel===2)inkReceived.set(m.sessionId,(inkReceived.get(m.sessionId)||0)+1);if(channel===3&&privateSentBody&&bytes.includes(Buffer.from(privateSentBody)))bodyLeaks.push(m.sessionId);}
  });
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(Error(`CDP timeout: ${method}`));},30000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
  const evaluate=async(session,expression)=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},session);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  const wait=async(session,expression)=>{for(let i=0;i<200;i++){if(await evaluate(session,expression))return;await new Promise(r=>setTimeout(r,100));}const shot=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(join(output,'timeout.png'),Buffer.from(shot.data,'base64'));await writeFile(join(output,'timeout.json'),JSON.stringify(await evaluate(session,"({path:location.pathname,revision:{...document.querySelector('#revision')?.dataset},timings:document.querySelector('#timing-report')?.value,extent:document.querySelector('#view')?.dataset.logicalSize})"),null,2));throw Error('Browser condition timed out: '+expression+'; '+await evaluate(session,"document.querySelector('#error')?.textContent"));};
  const metrics=async(session,width,height)=>{await call('Page.bringToFront',{},session);await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false},session);};
  const testQuery=new URLSearchParams();if(args.includes('--timings'))testQuery.set('timings','1');if(args.includes('--stable-quads'))testQuery.set('stable-quads','1');
  const routes={'component-playground':'test-playground/components','layout-lab':'test-playground/layouts','layout-document':'test-playground/long-document','layout-text-scroll':'test-playground/text-panning','input-lab':'test-playground/text-input','animation-lab':'test-playground/animations-two-d','three-d-lab':'test-playground/animations-three-d'};
  const page=async(width,height,route,query=testQuery.size?'?'+testQuery:'')=>{const {browserContextId}=await call('Target.createBrowserContext');const {targetId}=await call('Target.createTarget',{url:'about:blank',browserContextId});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});await call('Page.enable',{},sessionId);await call('Runtime.enable',{},sessionId);await call('Network.enable',{},sessionId);await metrics(sessionId,width,height);await call('Page.navigate',{url:origin+'/'+(route==='home'?'':routes[route]||route)+query},sessionId);await ready(sessionId,route,route==='brass-relay'?chatViewport(width,height).join('x'):width<height?'800x1280':'1280x800');return sessionId;};
  const ready=async(session,screen,extent)=>{await wait(session,`document.querySelector('#revision')?.dataset.screen===${JSON.stringify(screen)}${extent?`&&document.querySelector('#view')?.dataset.logicalSize===${JSON.stringify(extent)}`:''}`);assert.equal(await evaluate(session,"document.querySelector('#error').hidden"),true);const name={home:'OTriOS','brass-relay':'The Brass Relay'}[screen];if(name)assert.deepEqual(await evaluate(session,"[document.title,document.querySelector('meta[property=\"og:title\"]').content,document.querySelector('meta[property=\"og:site_name\"]').content]"),[name==='OTriOS'?name:`OTriOS · ${name}`,name,'OTriOS']);};
  const controls=session=>evaluate(session,"[...document.querySelectorAll('#controls button')].map(b=>({id:b.dataset.sceneNode,label:b.getAttribute('aria-label'),value:b.getAttribute('aria-valuenow'),bounds:[b.getBoundingClientRect().x,b.getBoundingClientRect().y,b.getBoundingClientRect().width,b.getBoundingClientRect().height]}))");
  const click=async(session,label,screen,extent)=>{let control=(await controls(session)).find(c=>c.label===label);if(!control&&(await controls(session)).some(c=>c.label==='Test Playground')){await click(session,'Test Playground','test-playground',extent);control=(await controls(session)).find(c=>c.label===label);}assert(control,label);const [x,y,w,h]=control.bounds;
    await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:x+w/2,y:y+h/2},session);
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:x+w/2,y:y+h/2},session);
    await ready(session,screen,extent);};
  const screenshot=async(session,name)=>{await call('Page.bringToFront',{},session);await evaluate(session,'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');const {data}=await call('Page.captureScreenshot',{format:'png'},session);await writeFile(join(output,name+'.png'),Buffer.from(data,'base64'));};
  const drag=async(session,label,fraction,release=true)=>{const control=(await controls(session)).find(c=>c.label===label);assert(control);const [x,y,w,h]=control.bounds,point={x:x+w*fraction,y:y+h/2};await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...point},session);if(release)await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...point},session);};
  const info=s=>evaluate(s,"JSON.parse(document.querySelector('#revision').dataset.chat||'null')");
  const online=s=>wait(s,"JSON.parse(document.querySelector('#revision').dataset.chat||'{}').online");
  const press=async(s,key,modifiers=0)=>{const keyCode={Backspace:8,Tab:9,Enter:13,ArrowLeft:37,ArrowRight:39,a:65}[key];await call('Input.dispatchKeyEvent',{type:'keyDown',key,modifiers,windowsVirtualKeyCode:keyCode,...(key==='Enter'?{text:'\r'}:{}),...(key==='a'&&modifiers?{commands:['selectAll']}:{})},s);await call('Input.dispatchKeyEvent',{type:'keyUp',key,modifiers,windowsVirtualKeyCode:keyCode},s);};
  const paste=async(s,text)=>{await new Promise(r=>setTimeout(r,150));return call('Input.insertText',{text},s);};
  const control=async(s,label)=>{await click(s,label,'brass-relay');await new Promise(r=>setTimeout(r,100));};
  const dispatch=async(s,text)=>{await control(s,'Message composer');await paste(s,text);await press(s,'Enter');};
  if(args.includes('--phone-breadcrumb')){
    const results=[];
    for(const [width,height] of [[393,695],[390,844],[430,932],[1280,800]]){
      const a=await page(width,height,'brass-relay');await online(a);
      await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:5},a);
      const tap=async(label,screen,extent)=>{
        console.log('Touch',width,height,label);
        const c=(await controls(a)).find(c=>c.label===label);assert(c,label);const[x,y,w,h]=c.bounds;
        await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{id:1,x:x+w/2,y:y+h/2}]},a);
        await new Promise(r=>setTimeout(r,70));
        await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]},a);
        await ready(a,screen,extent);
      };
      await tap('Members','brass-relay');
      await wait(a,"!![...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Close dialog')");
      await tap('Close dialog','brass-relay');
      await wait(a,"!![...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Back to OTriOS')");
      await tap('Back to OTriOS','home',width<height?'800x1280':'1280x800');
      await wait(a,"location.pathname==='/'");
      await tap('The Brass Relay','brass-relay',chatViewport(width,height).join('x'));await online(a);
      await call('Page.reload',{},a);await ready(a,'brass-relay',chatViewport(width,height).join('x'));await online(a);
      await tap('Back to OTriOS','home',width<height?'800x1280':'1280x800');
      await wait(a,"location.pathname==='/'");await screenshot(a,'phone-back-'+width);
      if(width<600){
        await tap('The Brass Relay','brass-relay',chatViewport(width,height).join('x'));await online(a);
        await control(a,'Message composer');
        await evaluate(a,`(()=>{window.__nativeViewport=window.visualViewport;window.__shortViewport={width:${width},height:300,offsetTop:0,offsetLeft:0,scale:1};Object.defineProperty(window,'visualViewport',{configurable:true,get:()=>window.__shortViewport});window.__nativeViewport.dispatchEvent(new Event('resize'));})()`);
        await call('Page.captureScreenshot',{format:'png'},a);
        await ready(a,'brass-relay',chatViewport(width,300).join('x'));
        await tap('Back to OTriOS','home','800x1280');await wait(a,"location.pathname==='/'");
      }
      results.push({width,height,passed:true});
      const {targetInfo}=await call('Target.getTargetInfo',{},a);
      await call('Target.disposeBrowserContext',{browserContextId:targetInfo.browserContextId});
    }
    const result={passed:true,checks:['phone-width touch navigation after modal close','Home admission before viewport policy handoff','breadcrumb after full refresh','return with focused composer and short visual viewport','desktop touch navigation'],results};
    await writeFile(join(output,'phone-breadcrumb-result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }else{
  const draft='A pigeon '+Date.now()+' is composing',nickname='Captain '+Date.now().toString(36);
  for(const [path,name] of [['/','OTriOS'],['/brass-relay','The Brass Relay'],['/test-playground','Test Playground'],['/monitoring','Monitoring']]){
    const response=await fetch(origin+path);assert(response.ok);const html=await response.text();
    assert(html.includes(`<title>${name==='OTriOS'?name:`OTriOS · ${name}`}</title>`));
    assert(html.includes(`property="og:title" content="${name}"`));
    assert(html.includes('property="og:site_name" content="OTriOS"'));
  }
  const a=await page(1280,800,'home');
  assert.deepEqual((await controls(a)).map(c=>c.label),['The Brass Relay','Test Playground','Monitoring']);
  await screenshot(a,'home-landscape');await click(a,'The Brass Relay','brass-relay');await online(a);
  const b=await page(800,1280,'brass-relay');await online(b);console.log('guests online');
  await control(a,'Message composer');
  assert.equal(await evaluate(a,"document.activeElement?.tagName"),'TEXTAREA');
  await paste(a,'teh engine');
  await evaluate(a,"(()=>{const e=document.activeElement;e.setSelectionRange(0,3);return true;})()");
  await call('Input.insertText',{text:'the'},a);
  await wait(a,"document.querySelector('.canvas-editor').value==='the engine'");
  // Genuine composition protocol events; no intermediate text enters chat drafts.
  await press(a,'a',4);await call('Input.imeSetComposition',{text:'日本',selectionStart:2,selectionEnd:2},a);
  await new Promise(r=>setTimeout(r,300));
  assert.equal(await evaluate(a,"document.querySelector('.canvas-editor').value"),'日本');
  await call('Input.insertText',{text:'日本語'},a);
  await wait(a,"document.querySelector('.canvas-editor').value==='日本語'");
  await press(a,'a',4);await press(a,'Backspace');

  assert.notEqual((await info(a)).participant,(await info(b)).participant);
  await control(a,'Message composer');assert(!(await evaluate(a,"document.querySelector('#view').getAttribute('aria-label')")).includes('BKSP'),'browser input must not show the app keyboard');await paste(a,draft);
  await wait(b,"JSON.parse(document.querySelector('#revision').dataset.chat).drafts===1");
  assert.equal(await evaluate(b,`document.querySelector('#chat-log').textContent.includes(${JSON.stringify(draft)})`),false,'draft is not sent history');
  await screenshot(b,'live-draft-portrait');console.log('draft visible');
  await control(a,'Live drafts: compose privately');
  await wait(b,"JSON.parse(document.querySelector('#revision').dataset.chat).drafts===0");
  await press(a,'Enter');
  await wait(b,`document.querySelector('#chat-log').textContent.includes(${JSON.stringify(draft)})`);
  await wait(a,"JSON.parse(document.querySelector('#revision').dataset.chat).pending===0");
  await dispatch(b,'A dispatch from the other station.');
  await wait(a,"document.querySelector('#chat-log').textContent.includes('other station')");
  const sentSnapshot=await evaluate(a,`(()=>{const row=[...document.querySelector('#chat-log').children].find(row=>row.textContent.includes(${JSON.stringify(draft)}));return {sequence:row.dataset.sequence,text:row.textContent};})()`);
  console.log('messages exchanged');await control(a,'Edit profile');await screenshot(a,'profile-icons');
  await control(a,'Next avatars');await control(a,'Next avatars');await control(a,'Next avatars');
  const avatar=(await controls(a)).find(c=>c.label.includes('Pocket watch'));assert(avatar);await control(a,avatar.label);
  await control(a,'Edit room name');await press(a,'a',4);await paste(a,nickname);await control(a,'Save profile');
  await wait(a,`JSON.parse(document.querySelector('#revision').dataset.chat).name===${JSON.stringify(nickname)}`);
  for(const session of [a,b])assert.equal(await evaluate(session,`[...document.querySelector('#chat-log').children].find(row=>row.dataset.sequence===${JSON.stringify(sentSnapshot.sequence)}).textContent`),sentSnapshot.text,'profile changes preserve the immutable sender snapshot on a sent record');
  privateSentBody='A dispatch with the new sender snapshot '+Date.now();
  await dispatch(a,privateSentBody);
  for(const session of [a,b])await wait(session,`(()=>{const log=document.querySelector('#chat-log').textContent;return log.includes(${JSON.stringify(nickname)})&&log.includes(${JSON.stringify(draft)});})()`);
  assert((await info(a)).avatar>=56);
  const shortComposerHeight=(await controls(a)).find(c=>c.label==='Message composer').bounds[3];
  const tail='TAIL'+Date.now();
  await control(a,'Message composer');await paste(a,'Clockwork text across the field. '.repeat(20)+tail);
  await wait(a,`document.querySelector('#view').getAttribute('aria-label').includes(${JSON.stringify(tail)})`);
  assert((await controls(a)).find(c=>c.label==='Message composer').bounds[3]>shortComposerHeight,'long composition grows into wrapped lines');
  assert(!(await evaluate(a,"document.querySelector('#view').getAttribute('aria-label')")).includes('BKSP'));
  await screenshot(a,'wrapped-composer');
  await press(a,'ArrowLeft');await press(a,'ArrowRight');await press(a,'Enter');
  await wait(b,`document.querySelector('#chat-log').textContent.includes(${JSON.stringify(tail)})`);
  await dispatch(a,'A multiline dispatch.\nSecond line preserved.');
  await wait(b,"document.querySelector('#chat-log').textContent.includes('Second line preserved.')");
  await control(a,'Message composer');await paste(a,'Shift Enter first');await press(a,'Enter',8);await paste(a,'second line');await press(a,'Enter');
  await call('Page.bringToFront',{},b);
  await wait(b,`document.querySelector('#chat-log').textContent.includes(${JSON.stringify('Shift Enter first\nsecond line')})`);
  const identity=(await info(a)).participant;
  await call('Page.reload',{},a);await ready(a,'brass-relay');await online(a);assert.equal((await info(a)).participant,identity);assert.equal((await info(a)).name,nickname);
  await dispatch(a,'/roll 2d6');await wait(b,"document.querySelector('#chat-log').textContent.includes('rolls 2d6')");
  await dispatch(a,'/steam');
  await screenshot(a,'lounge-landscape');await screenshot(b,'lounge-portrait');
  const documentStats=s=>evaluate(s,"JSON.parse(document.querySelector('#revision').dataset.runtimeResources).text_document");
  const beforeScroll=await documentStats(a),beforeAuthors=(await info(a)).authors_revision;
  await call('Page.bringToFront',{},a);
  await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:450,y:250},a);
  await call('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,x:450,y:420},a);
  await wait(a,`JSON.parse(document.querySelector('#revision').dataset.runtimeResources).text_document.offset<${beforeScroll.offset-20}`);
  await new Promise(r=>setTimeout(r,200));
  await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:450,y:420},a);
  await wait(a,"!JSON.parse(document.querySelector('#revision').dataset.chat).history_loading");
  const afterScroll=await documentStats(a);
  assert.equal(afterScroll.source_bytes,beforeScroll.source_bytes,'panning does not reconstruct message source');
  assert.equal((await info(a)).authors_revision,beforeAuthors);
  await screenshot(a,'transcript-drag-landscape');
  await writeFile(join(output,'message-block-scroll.json'),JSON.stringify({passed:true,before:beforeScroll,after:afterScroll},null,2));
  await control(b,'Members');await screenshot(b,'members-portrait');
  assert((await controls(b)).some(c=>c.label==='Close dialog'));
  assert(!(await evaluate(b,"document.querySelector('#view').getAttribute('aria-label')")).includes('questionable ideas'));
  await control(b,'Close dialog');
  await metrics(a,800,1280);await ready(a,'brass-relay','800x1280');await screenshot(a,'lounge-resized');
  await evaluate(a,"window.__graphicsRecovery=document.querySelector('#view').getContext('webgl').getExtension('WEBGL_lose_context');window.__graphicsRecovery.loseContext();true");
  await wait(a,"document.querySelector('#revision').dataset.graphics==='lost'");
  await evaluate(a,"window.__graphicsRecovery.restoreContext();true");
  await wait(a,"document.querySelector('#revision').dataset.graphics==='ready'&&document.querySelector('#revision').dataset.documentResources!=='0'");
  await ready(a,'brass-relay','800x1280');await online(a);
  assert.equal((await info(a)).participant,identity);
  await dispatch(a,'Dispatch after graphics recovery '+nickname);
  await wait(b,`document.querySelector('#chat-log').textContent.includes(${JSON.stringify('Dispatch after graphics recovery '+nickname)})`);
  await control(a,'Back to OTriOS');await ready(a,'home','800x1280');await screenshot(a,'home-portrait');
  assert.deepEqual((await controls(a)).map(c=>c.label),['The Brass Relay','Test Playground','Monitoring']);
  const homeBeforeLogin=await evaluate(a,"document.querySelector('#revision').dataset.epoch");
  await evaluate(a,"[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')==='Monitoring').click();true");
  await wait(a,"document.querySelector('#monitoring-login').open&&document.title==='OTriOS · Monitoring'");
  assert.equal(await evaluate(a,"document.querySelector('meta[property=\"og:title\"]').content"),'Monitoring');
  await evaluate(a,"document.querySelector('#login-back').click();true");await wait(a,"document.title==='OTriOS'");await ready(a,'home','800x1280');
  await wait(a,`document.querySelector('#revision').dataset.epoch!==${JSON.stringify(homeBeforeLogin)}&&document.querySelector('#revision').dataset.screen==='home'`);
  const phone=a;await metrics(phone,390,844);await ready(phone,'home','800x1280');
  await click(phone,'The Brass Relay','brass-relay',chatViewport(390,844).join('x'));await online(phone);
  await control(phone,'Message composer');await paste(phone,'The viewport keeps its width.');
  const phoneFont=await evaluate(phone,"(()=>{const e=document.activeElement,s=getComputedStyle(e);return parseFloat(s.fontSize)*new DOMMatrix(s.transform).a;})()");
  assert(phoneFont>=16&&phoneFont<=19,`phone body text has a readable CSS size: ${phoneFont}`);
  await evaluate(phone,`(()=>{const v=window.visualViewport;window.__nativeViewport=v;window.__availableViewport={width:390,height:844,offsetTop:0,offsetLeft:0,scale:1};Object.defineProperty(window,'visualViewport',{configurable:true,get:()=>window.__availableViewport});window.__editor=document.activeElement;window.__editor.setSelectionRange(4,8);window.__editor.dispatchEvent(new Event('select'));return true;})()`);
  await new Promise(r=>setTimeout(r,150));
  const phoneBefore=await evaluate(phone,"({width:document.querySelector('#view').getBoundingClientRect().width,value:window.__editor.value,selection:[window.__editor.selectionStart,window.__editor.selectionEnd]})");
  const immediateWidth=await evaluate(phone,"Object.assign(window.__availableViewport,{height:420,offsetTop:50});window.__nativeViewport.dispatchEvent(new Event('resize'));document.querySelector('#view').getBoundingClientRect().width");
  assert(Math.abs(immediateWidth-390)<1,'horizontal scale remains steady before the worker acknowledges reflow');
  // A mocked VisualViewport has no native compositor resize. Present once to
  // return the held renderer credit before waiting for the reflowed scene.
  await call('Page.bringToFront',{},phone);await call('Page.captureScreenshot',{format:'png'},phone);
  await ready(phone,'brass-relay',chatViewport(390,420).join('x'));
  const phoneAfter=await evaluate(phone,"(()=>{const r=document.querySelector('#view').getBoundingClientRect();return {width:r.width,top:r.top,bottom:r.bottom,value:window.__editor.value,selection:[window.__editor.selectionStart,window.__editor.selectionEnd],focused:document.activeElement===window.__editor,layoutHeight:innerHeight};})()");
  assert(Math.abs(phoneAfter.width-390)<1);assert(Math.abs(phoneAfter.width-phoneBefore.width)<1);
  assert(phoneAfter.top>=49&&phoneAfter.bottom<=471);assert.equal(phoneAfter.layoutHeight,844,'only the visual viewport changed');
  assert.equal(phoneAfter.value,phoneBefore.value);assert.deepEqual(phoneAfter.selection,phoneBefore.selection);assert(phoneAfter.focused);
  await screenshot(phone,'phone-keyboard-reflow');
  await evaluate(phone,"Object.assign(window.__availableViewport,{height:844,offsetTop:0});window.__nativeViewport.dispatchEvent(new Event('resize'));true");
  await call('Page.captureScreenshot',{format:'png'},phone);
  await ready(phone,'brass-relay',chatViewport(390,844).join('x'));
  await screenshot(phone,'phone-full-height');
  await writeFile(join(output,'mobile-reflow.json'),JSON.stringify({passed:true,fontSizeCss:phoneFont,immediateWidth,before:phoneBefore,after:phoneAfter},null,2));
  const windows=await Promise.all([info(a),info(b)]);
  for(const window of windows){assert(window.messages<=64);assert(window.selected_bytes<=256*1024);assert(window.replica_bytes<=512*1024);}
  for(const session of [a,b]){assert(receivedChannels.get(session+':4')?.frames>0,'both browsers consume the collection channel');assert.equal(hellos.get(session)?.version,1);assert.equal(hellos.get(session)?.state_protocol,'otrios-canvas-sync-v8');}
  assert.deepEqual(bodyLeaks,[],'a private sent body must not be broadcast over the ephemeral room lane');
  await writeFile(join(output,'collection-transport.json'),JSON.stringify({passed:true,windows,channels:Object.fromEntries(receivedChannels),hellos:Object.fromEntries(hellos),privateBodyLeaks:bodyLeaks.length},null,2));
  const result={passed:true,checks:['unified Home in both orientations','independent persistent guest cookies','live draft visibility and private clearing','bidirectional durable messages','64-avatar paging and profile rename','wrapped composer grows and scrolls its caret into view','native textarea focus, selection replacement and IME composition','no app keyboard in browsers','multiline paste','identity and history survive reload','commands and particles','member dialog close control','responsive retained transcript','visual viewport-only keyboard reflow preserves width, focus, text and selection','WebGL loss/recovery replays resources and preserves identity'],screenshots:output};
  await writeFile(join(output,'acceptance.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
  }
}finally{
  socket?.close();if(browser.pid&&browser.exitCode===null&&browser.signalCode===null){browser.kill('SIGTERM');await new Promise(resolve=>browser.once('exit',resolve));}await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
