// SPDX-License-Identifier: Apache-2.0
// Presentation owner: monotonic clock, cached GPU geometry, input forwarding.
import init,{CanvasPresentation} from './pkg/otrios_dashboard.js';
import {SceneRenderer} from './gpu-scene.mjs';
import {DocumentLayers} from './document-transfer.mjs';
import {layoutViewport,pointerPosition,semanticBounds} from './viewport.mjs';
import {loadFontResource} from './font-resource.mjs';
import {loadAppRelease} from './app-catalog.mjs';
import {connectSession,bridgeWorker} from './session.mjs';
const timingToolsReady=new URL(location.href).searchParams.get('timings')==='1'?import('./navigation-timings.mjs'):null;
let timingTools=null,timings=null;
const canvas=document.querySelector('#view'),controlsLayer=document.querySelector('#controls');
const login=document.querySelector('#monitoring-login'),form=document.querySelector('#monitoring-form'),loginError=document.querySelector('#login-error');
let monitoringAuth={authenticated:false},modelAuthRequired=false;
function showAuth(){const needed=modelAuthRequired&&!monitoringAuth.authenticated;if(needed&&!login.open)login.showModal();else if(!needed&&login.open)login.close();document.querySelector('#login-submit').disabled=!!monitoringAuth.pending;loginError.textContent=monitoringAuth.error||'';}
const connection=document.querySelector('#connection'),dot=document.querySelector('#connection-dot'),revision=document.querySelector('#revision'),error=document.querySelector('#error');
function fatal(e){error.hidden=false;error.textContent=String(e);connection.textContent='Unable to open this space';}
const initialRelease=new URL(import.meta.url).pathname.match(/^\/releases\/([a-f0-9]{64})\/web\//)?.[1]||null;
const screenFromPath=()=>location.pathname.replace(/\/$/,'')||'/';
const screenUrl=()=>screenFromPath()+location.search;
let followingHistory=true,requestedScreen=screenFromPath(),routedScreen=null,routedUi=null;
function restoreScreen(){followingHistory=true;requestedScreen=screenFromPath();return requestedScreen;}
function reflectScreen(screen,ready,ui){
  if(!ready||!screen)return;
  const url=`${screen}${location.search}`;
  if(followingHistory){
    if(requestedScreen&&screen!==requestedScreen)return;
    routedScreen=screen;followingHistory=false;history.replaceState(null,'',url);
  }else if(screen!==routedScreen){routedScreen=screen;history[ui===routedUi?'replaceState':'pushState'](null,'',url);}
  routedUi=ui;
}
// Start before Wasm initialization. An unopened app has no user edits to lose.
let offerRelease=r=>{if(initialRelease&&r.id!==initialRelease)location.replace(screenUrl());};
let session,transportBridge=null,observedRelease;
function startDiscovery(){if(session){session.resume();return;}session=connectSession({current:initialRelease,
  onRelease:r=>{observedRelease=r;offerRelease(r);},
  onState:(bytes,epoch)=>transportBridge?.receive('state',bytes,epoch),
  onInk:(bytes,epoch)=>transportBridge?.receive('ink',bytes,epoch),
  onStatus:(ready,epoch)=>transportBridge?.status(ready,epoch),
  onError:error=>console.warn('Session:',error),
});}
startDiscovery();
window.addEventListener('pagehide',()=>session.stop());
window.addEventListener('pageshow',e=>{if(e.persisted)startDiscovery();});
try{
  if(timingToolsReady){timingTools=await timingToolsReady;timings=timingTools.createTimingDiagnostics({location,performance,document,navigator});}
  if('serviceWorker' in navigator){
    revision.dataset.cacheState='starting';
    navigator.serviceWorker.addEventListener('controllerchange',()=>navigator.serviceWorker.controller?.postMessage({type:'artifact-cache-status'}));
    navigator.serviceWorker.addEventListener('message',({data})=>{if(data?.type==='artifact-cache-status'){revision.dataset.cacheState=data.enabled?'ready':'unavailable';if(data.enabled){revision.dataset.cacheBytes=String(data.bytes);revision.dataset.cacheLimit=String(data.limit);revision.dataset.cacheEntries=String(data.count);}}});
    navigator.serviceWorker.register(new URL('./artifact-cache-worker.js',import.meta.url),{type:'module',scope:'/'}).then(()=>navigator.serviceWorker.ready).then(reg=>reg.active?.postMessage({type:'artifact-cache-status'})).catch(e=>{revision.dataset.cacheState='unavailable';console.warn('Artifact cache unavailable',e);});
  }else revision.dataset.cacheState='unsupported';
  const [wasmModule,fontResource,appRelease]=await Promise.all([
    WebAssembly.compileStreaming(fetch(new URL('./pkg/otrios_dashboard_bg.wasm',import.meta.url))).then(module=>{timings?.mark('wasmCompiled');return module;}),
    loadFontResource(import.meta.url).then(font=>{timings?.mark('fontLoaded');return font;}),loadAppRelease(import.meta.url).then(release=>{timings?.mark('catalogAdmitted');return release;}),
  ]);
  await init({module_or_path:wasmModule});const presenter=new CanvasPresentation();presenter.set_font(fontResource.bytes,fontResource.hash);timings?.mark('presenterReady');
  const worker=new Worker(new URL('./state-worker.js',import.meta.url),{type:'module'});
  let desiredViewport=layoutViewport(innerWidth,innerHeight);
  let model=null,queue=[],busy=false,releaseCandidate=null,reloading=false,workerFailed=false;
  function command(m,event,downAt){if(timings&&(['activate','navigate','back'].includes(m.type)||(m.type==='pointer'&&m.args[0]===2)))m.timing=timings.input(event,downAt);if(reloading)return;if(queue.length>=128){fatal('Input queue full. Please wait for state processing.');return;}queue.push(m);pump();}
  function pump(){if(!busy&&model&&queue.length){busy=true;const message=queue.shift();timings?.dispatched(message.timing);worker.postMessage(message);}else if(!busy&&releaseCandidate&&(!model||!queue.length)&&!pointers.size&&!moves.size&&!reloading){reloading=true;worker.postMessage({type:'release-commit',entry:releaseCandidate});}}
  window.addEventListener('popstate',()=>command({type:'navigate',screen:restoreScreen()}));
  const gl=canvas.getContext('webgl',{alpha:false,antialias:false,depth:false,stencil:false});if(!gl)throw new Error('This dashboard requires WebGL.');
  const query=new URL(location.href).searchParams;
  let renderer,staticLayer,motionLayer,hudLayer,documentLayers,graphicsLost=false;
  function createGraphics(){
    renderer=new SceneRenderer(gl,{stableTextQuads:query.get('stable-quads')==='1',atlasScale:query.has('font-probe')?0:2,linearLight:query.get('linear-light')==='1'||query.get('font-probe')==='1'});
    staticLayer=renderer.createLayer();motionLayer=renderer.createLayer();hudLayer=renderer.createLayer();documentLayers=new DocumentLayers(renderer);
    renderer.setFont({curveWidth,curveHeight,curves:presenter.font_curves(),bandWidth,bandHeight,bands:presenter.font_bands()});
  }
  const [curveWidth,curveHeight,bandWidth,bandHeight]=presenter.font_dimensions();
  createGraphics();revision.dataset.fontRenderer='analytic-quadratic-cached';revision.dataset.gamma=renderer.linearLight?'linear-light-srgb':'authored-srgb';revision.dataset.alpha='premultiplied';
  let hudRevision=-1,dirty=true,lastFontCache='',fontProbed=false;const fontProbeRequested=['1','direct'].includes(query.get('font-probe'));const controls=new Map();
  function resize(){if(graphicsLost||gl.isContextLost())return;try{dirty=renderer.fitCanvas(canvas)||dirty;}catch(error){fatal(error);}}
  resize();new ResizeObserver(resize).observe(canvas);window.addEventListener('resize',()=>{
    resize();const next=layoutViewport(innerWidth,innerHeight);
    if(next.some((n,i)=>n!==desiredViewport[i])){
      desiredViewport=next;moves.clear();
      for(const id of pointers){if(canvas.hasPointerCapture(id))canvas.releasePointerCapture(id);}pointers.clear();pointerStarts.clear();
      command({type:'viewport',size:next});
    }
  });
  function accept(f){
    if(graphicsLost||gl.isContextLost()){worker.postMessage({type:'frame-ack',documentToken:f.document?.token});return;}
    const wasReflow=!!model?.reflow;if(model?.screen!==f.screen)presenter.suspend();timings?.accept(f);
    const focused=document.activeElement?.closest('#controls button'),focusKey=focused?{id:focused.dataset.sceneNode,label:focused.getAttribute('aria-label')}:null;
    if(model&&(model.ui!==f.ui||model.epoch!==f.epoch)){for(const b of controls.values())b.remove();controls.clear();}
    if(f.document){documentLayers.accept(f.document);revision.dataset.documentLoading=String(documentLayers.loading);revision.dataset.documentResources=String(documentLayers.layers.size);}
    model={...model,...f};revision.dataset.epoch=String(f.epoch);modelAuthRequired=!!f.authRequired;showAuth();
    if(f.controls){
      renderer.setLogicalSize(...f.extent);document.querySelector('.screen').style.setProperty('--aspect',String(f.extent[0]/f.extent[1]));
      canvas.dataset.logicalSize=f.extent.join('x');resize();
      presenter.set_reflow(f.reflow||new Uint8Array(),performance.now());
      model.reflow=!!f.reflow?.length;if(f.vertices||(wasReflow&&!model.reflow))staticLayer.setFrame(model);if(!model.reflow)staticLayer.setTextViews(f.textViews||new Float32Array());if(f.overlayVertices)motionLayer.setFrame({vertices:f.overlayVertices,glyphVertices:f.overlayGlyphVertices,draws:f.overlayDraws,textStyles:f.overlayStyles});if(f.overlayViews)motionLayer.setTextViews(f.overlayViews);
      const ids=new Set(f.controls.map(c=>c.id));for(const [id,b] of controls)if(!ids.has(id)){b.remove();controls.delete(id);}
      for(const c of f.controls){let b=controls.get(c.id);if(!b){b=document.createElement('button');b.type='button';b.dataset.sceneNode=String(c.id);b.addEventListener('click',event=>command({type:'activate',epoch:Number(b.dataset.epoch),viewportRevision:model.viewportRevision,id:c.id},event));b.addEventListener('keydown',event=>{const delta={ArrowLeft:[-50,0],ArrowRight:[50,0],ArrowUp:[0,50],ArrowDown:[0,-50]}[event.key];if(delta&&b.dataset.draggable==='true'){event.preventDefault();event.stopPropagation();command({type:'adjust',epoch:Number(b.dataset.epoch),viewportRevision:model.viewportRevision,id:c.id,dx:delta[0],dy:delta[1]});}});controlsLayer.append(b);controls.set(c.id,b);}
        b.dataset.epoch=String(f.epoch);b.dataset.draggable=String(c.draggable);if(Number.isFinite(c.value)){b.setAttribute('aria-valuemin','0');b.setAttribute('aria-valuemax','100');b.setAttribute('aria-valuenow',String(c.value));}else{for(const name of ['aria-valuemin','aria-valuemax','aria-valuenow'])b.removeAttribute(name);}b.setAttribute('aria-label',c.label);b.disabled=c.disabled;b.setAttribute('role',c.role);if(c.role==='switch')b.setAttribute('aria-checked',String(c.checked));else b.removeAttribute('aria-checked');Object.assign(b.style,semanticBounds(c.bounds,f.extent));}
      if(focusKey){const b=controls.get(Number(focusKey.id));if(b?.getAttribute('aria-label')===focusKey.label)b.focus({preventScroll:true});}
      canvas.setAttribute('aria-label',f.text);revision.textContent=`VIEW ${f.snapshot.slice(0,12)}`;revision.dataset.snapshot=f.snapshot;revision.dataset.layout=f.layout;
    }
    if(f.resources)revision.dataset.runtimeResources=JSON.stringify(f.resources);
    revision.dataset.screen=f.screen;revision.dataset.ui=f.ui||'';revision.dataset.state=f.state||'';revision.dataset.reordering=String(!!f.controls?.some(c=>c.draggable)&&!!model.reflow);dirty=true;
    if(f.resources?.text_scroll||f.resources?.text_document)revision.dataset.textScroll=JSON.stringify(f.resources.text_scroll||f.resources.text_document);else{delete revision.dataset.textScroll;scrollLabel='';scrollSamples=[];scrollLast=null;}
    reflectScreen(f.path,!!f.ui||f.authRequired,f.authRequired?f.path:f.ui);
    pump();worker.postMessage({type:'frame-ack',documentToken:f.document?.token});
  }
  transportBridge=bridgeWorker(session,(message,transfer=[])=>{if(timings&&message.type==='state-frame')message.receivedAt=timingTools.timestamp(performance);worker.postMessage(message,transfer);});
  worker.onmessage=event=>{try{const m=event.data;if(m.type==='state-send'&&m.timing)timings?.sent(m.timing);if(transportBridge.handle(m))return;switch(m.type){case 'clear-private':documentLayers.clear();presenter.set_reflow(new Uint8Array(),performance.now());model={epoch:model?.epoch,extent:model?.extent||[1280,800],reflow:false,presentation:'none',intent:[],ui:'',state:'',editing:false,screen:''};if(!graphicsLost&&!gl.isContextLost()){staticLayer.setFrame({vertices:new Float32Array()});motionLayer.setFrame({vertices:new Float32Array()});hudLayer.setFrame({vertices:new Float32Array()});}else{staticLayer.dispose();motionLayer.dispose();hudLayer.dispose();}controlsLayer.replaceChildren();controls.clear();canvas.setAttribute('aria-label','Opening app');revision.dataset.ui='';revision.dataset.state='';dirty=true;break;case 'frame':accept(m);break;case 'command-ack':busy=false;pump();break;case 'release-candidate':releaseCandidate=m.entry;pump();break;case 'release-retry':if(releaseCandidate===m.entry)releaseCandidate=null;reloading=false;pump();break;case 'release':location.replace(screenUrl());break;case 'release-id':revision.dataset.release=m.id;session.setRelease(m.id);if(m.uiOnly){revision.dataset.uiUpdates=String(Number(revision.dataset.uiUpdates||0)+1);}break;case 'monitoring-auth':monitoringAuth=m;showAuth();break;case 'online':dot.classList.toggle('online',m.online);connection.textContent=m.online?'Live shared space':'Offline · unconfirmed changes held in memory';break;case 'warning':connection.textContent=m.message;break;case 'error':fatal(m.message);break;}}catch(error){fatal(error);}};
  worker.onerror=e=>{workerFailed=true;fatal(e.message);session.reconnect();if(observedRelease)offerRelease(observedRelease);};
  offerRelease=r=>{if(initialRelease&&r.id!==initialRelease&&(workerFailed||!model?.ui)){location.replace(screenUrl());return;}worker.postMessage({type:'release-discovered',release:r});};
  if(observedRelease)offerRelease(observedRelease);
  form.addEventListener('submit',event=>{event.preventDefault();const data=new FormData(form);command({type:'login',username:String(data.get('username')),password:String(data.get('password'))});form.elements.password.value='';});
  function leaveLogin(){command({type:'navigate',screen:null});}
  document.querySelector('#login-back').addEventListener('click',leaveLogin);login.addEventListener('cancel',event=>{event.preventDefault();leaveLogin();});
  try{sessionStorage.removeItem('otrios-dashboard-v1');sessionStorage.removeItem('otrios-dashboard-presentation-v1');}catch{}
  const start={type:'start',timings:!!timings,wasmModule,fontBytes:fontResource.bytes,fontHash:fontResource.hash,catalogBytes:appRelease.catalog,manifest:appRelease.manifest,moduleUrl:import.meta.url,viewport:desiredViewport,release:initialRelease,screen:restoreScreen()};
  try{worker.postMessage(start);}catch(error){
    // Older engines may instantiate Wasm but reject cloning compiled modules.
    if(error.name!=='DataCloneError')throw error;
    delete start.wasmModule;worker.postMessage(start);
  }
  const pointers=new Set(),moves=new Map(),pointerStarts=new Map();
  function pointer(e,phase){const p=pointerPosition(e.clientX,e.clientY,canvas.getBoundingClientRect(),model?.extent||[1280,800]);return {type:'pointer',epoch:model?.epoch,viewportRevision:model?.viewportRevision,args:[phase,e.pointerId,...p,performance.now()]};}
  canvas.addEventListener('pointerdown',e=>{if(graphicsLost)return;e.preventDefault();canvas.focus({preventScroll:true});pointers.add(e.pointerId);if(timings)pointerStarts.set(e.pointerId,timingTools.eventTimestamp(e,performance));canvas.setPointerCapture(e.pointerId);command(pointer(e,0));});
  canvas.addEventListener('pointermove',e=>{if(pointers.has(e.pointerId))moves.set(e.pointerId,pointer(e,1));});
  function finish(e,phase){if(!pointers.has(e.pointerId))return;if(moves.has(e.pointerId)){command(moves.get(e.pointerId));moves.delete(e.pointerId);}command(pointer(e,phase),e,pointerStarts.get(e.pointerId));pointerStarts.delete(e.pointerId);pointers.delete(e.pointerId);}
  canvas.addEventListener('pointerup',e=>finish(e,2));canvas.addEventListener('pointercancel',e=>finish(e,3));canvas.addEventListener('lostpointercapture',e=>finish(e,3));
  window.addEventListener('blur',()=>{moves.clear();for(const id of pointers){command({type:'pointer',args:[3,id,0,0,performance.now()]});if(canvas.hasPointerCapture(id))canvas.releasePointerCapture(id);}pointers.clear();pointerStarts.clear();command({type:'cancel'});});
  window.addEventListener('keydown',e=>{if(graphicsLost||e.isComposing||e.target.closest?.('input,textarea,dialog,[contenteditable="true"]'))return;if(e.key==='Escape'){e.preventDefault();command({type:'back'},e);return;}if(!model?.editing||e.target.closest?.('#controls button'))return;const shortcut=e.ctrlKey||e.metaKey,key=shortcut&&e.key.toLowerCase()==='a'?'SelectAll':e.key;if(['Backspace','ArrowLeft','ArrowRight','Tab','Enter','SelectAll'].includes(key)){e.preventDefault();command({type:'key',key,select:e.shiftKey});}else if(!shortcut&&!e.altKey&&e.key.length===1){e.preventDefault();command({type:'text',text:e.key});}});
  window.addEventListener('paste',e=>{if(graphicsLost)return;if(e.target.closest?.('input,textarea,dialog,[contenteditable="true"]'))return;if(model?.editing){e.preventDefault();command({type:'text',text:e.clipboardData.getData('text/plain')});}});
  let lastMetric=0,scrollSamples=[],scrollLast=null,scrollLabel='',scrollHudAt=0,scrollWasActive=false,scrollHudExtent='';
  function textScrollHud(now,active){
    if(active){
      const dt=scrollLast===null?0:now-scrollLast;
      if(dt>0&&dt<500)scrollSamples.push([now,dt]);else scrollSamples=[];
      scrollLast=now;while(scrollSamples.length>240||scrollSamples[0]?.[0]<now-1000)scrollSamples.shift();
    }else{scrollLast=null;}
    if(now-scrollHudAt>=500||active!==scrollWasActive||!scrollLabel||scrollHudExtent!==model.extent.join(',')){
      const elapsed=scrollSamples.reduce((sum,p)=>sum+p[1],0);
      scrollLabel=documentLayers.loading?'Preparing document...':active?(elapsed?`${(scrollSamples.length*1000/elapsed).toFixed(1)} fps - retained text / momentum scroll`:'Measuring scroll frames...'):'Idle - drag in any direction; release to coast';
      const [w,h]=model.extent;
      hudLayer.setFrame({vertices:presenter.scroll_hud(scrollLabel,w,h),glyphVertices:presenter.scroll_hud_glyphs(),draws:presenter.scroll_hud_draws()});
      scrollHudAt=now;scrollWasActive=active;scrollHudExtent=model.extent.join(',');
      revision.dataset.scrollStats=JSON.stringify({label:scrollLabel,active,samples:scrollSamples.length});
    }
  }
  function frame(now){
    // Loss can precede delivery of webglcontextlost by one animation frame.
    if(gl.isContextLost()){requestAnimationFrame(frame);return;}
    try{
      if(!busy)for(const [id,m]of moves){command(m);moves.delete(id);break;}
      if(releaseCandidate)pump();
      const animated=model&&model.presentation==='motion';
      const reflow=model?.reflow&&presenter.reflow_active(now);
      if(!graphicsLost&&model&&(dirty||animated||reflow)){
        if(model.reflow)staticLayer.setFrame({vertices:presenter.reflow_vertices(now),glyphVertices:presenter.reflow_glyphs(),draws:presenter.reflow_draws()});
        if(animated){const [playing,speed,count]=model.intent;motionLayer.setFrame({vertices:presenter.vertices(model.intent[3]!==0,playing!==0,speed,count,now,model.updates)});if(hudRevision!==presenter.hud_revision()){hudRevision=presenter.hud_revision();hudLayer.setFrame({vertices:presenter.hud(),glyphVertices:presenter.hud_glyph_vertices(),draws:presenter.hud_draws()});}}
        const scrolling=['text-scroll','text-document'].includes(model.presentation);
        if(scrolling)textScrollHud(now,!!(model.resources?.text_scroll||model.resources?.text_document)?.active);
        renderer.prepare([staticLayer,...documentLayers.draws,...(model.presentation==='text-document'?[motionLayer]:[]),...(animated||scrolling?[hudLayer]:[])]);
        renderer.beginFrame();staticLayer.draw();for(const layer of documentLayers.draws)layer.draw();
        if(animated||model.presentation==='text-document')motionLayer.draw();if(animated||scrolling)hudLayer.draw();renderer.endFrame();if(fontProbeRequested&&!fontProbed&&staticLayer.glyphVertices.length){fontProbed=true;const result=renderer.probe([staticLayer]);canvas.dataset.fontProbe=JSON.stringify(result);if(!result.passed)throw Error('GPU font probe did not pass: '+JSON.stringify(result));}
        const fontCache=JSON.stringify(renderer.cacheStats);if(fontCache!==lastFontCache){revision.dataset.fontCache=fontCache;lastFontCache=fontCache;}
        const gpuError=gl.getError();if(gpuError!==gl.NO_ERROR&&gpuError!==gl.CONTEXT_LOST_WEBGL)throw new Error('GPU rendering failed.');dirty=false;
        const sample=timings?.drawn(model.epoch);if(sample)requestAnimationFrame(()=>timings.presented(sample,model?.epoch));
        if(animated&&now-lastMetric>=1000){lastMetric=now;revision.dataset.frameStats=presenter.stats();}
      }
    }catch(e){if(!gl.isContextLost()){fatal(e);return;}}
    requestAnimationFrame(frame);
  }
  document.addEventListener('visibilitychange',()=>{presenter.suspend();dirty=true;});canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();graphicsLost=true;renderer.contextLost();documentLayers.clear();staticLayer.dispose();motionLayer.dispose();hudLayer.dispose();if(model)model.document=null;revision.dataset.graphics='lost';controlsLayer.inert=true;moves.clear();for(const id of pointers){if(canvas.hasPointerCapture(id))canvas.releasePointerCapture(id);}pointers.clear();pointerStarts.clear();command({type:'cancel'});worker.postMessage({type:'graphics-lost'});connection.textContent='Restoring graphics...';});
  canvas.addEventListener('webglcontextrestored',()=>{try{documentLayers.clear();createGraphics();graphicsLost=false;if(model?.extent)renderer.setLogicalSize(...model.extent);resize();if(model?.vertices&&!model.reflow)staticLayer.setFrame(model);scrollLabel='';hudRevision=-1;dirty=true;revision.dataset.graphics='ready';controlsLayer.inert=false;connection.textContent=dot.classList.contains('online')?'Live shared space':'Offline · unconfirmed changes held in memory';worker.postMessage({type:'document-replay'});}catch(e){fatal(e);}});requestAnimationFrame(frame);
}catch(e){fatal(e);}
