// SPDX-License-Identifier: Apache-2.0
// Presentation owner: monotonic clock, cached GPU geometry, input forwarding.
import init,{MotionPresenter} from './pkg/otrios_dashboard.js';
import {SceneRenderer} from './gpu-scene.mjs';
import {loadFontResource} from './font-resource.mjs';
const canvas=document.querySelector('#dashboard'),controlsLayer=document.querySelector('#controls');
const login=document.querySelector('#monitoring-login'),form=document.querySelector('#monitoring-form'),signout=document.querySelector('#monitoring-signout'),loginError=document.querySelector('#login-error');
let monitoringAuth={authenticated:false},currentScreen='home';
function showAuth(){const needed=currentScreen==='monitoring'&&!monitoringAuth.authenticated;signout.hidden=currentScreen!=='monitoring'||!monitoringAuth.authenticated;if(needed&&!login.open)login.showModal();else if(!needed&&login.open)login.close();document.querySelector('#login-submit').disabled=!!monitoringAuth.pending;loginError.textContent=monitoringAuth.error||'';}
const connection=document.querySelector('#connection'),dot=document.querySelector('#connection-dot'),revision=document.querySelector('#revision'),error=document.querySelector('#error');
function fatal(e){error.hidden=false;error.textContent=String(e);connection.textContent='Unable to open this space';}
try{
  if('serviceWorker' in navigator){
    revision.dataset.cacheState='starting';
    navigator.serviceWorker.addEventListener('controllerchange',()=>navigator.serviceWorker.controller?.postMessage({type:'artifact-cache-status'}));
    navigator.serviceWorker.addEventListener('message',({data})=>{if(data?.type==='artifact-cache-status'){revision.dataset.cacheState=data.enabled?'ready':'unavailable';if(data.enabled){revision.dataset.cacheBytes=String(data.bytes);revision.dataset.cacheLimit=String(data.limit);revision.dataset.cacheEntries=String(data.count);}}});
    navigator.serviceWorker.register(new URL('./artifact-cache-worker.js',import.meta.url),{type:'module',scope:'/'}).then(()=>navigator.serviceWorker.ready).then(reg=>reg.active?.postMessage({type:'artifact-cache-status'})).catch(e=>{revision.dataset.cacheState='unavailable';console.warn('Artifact cache unavailable',e);});
  }else revision.dataset.cacheState='unsupported';
  await init();const presenter=new MotionPresenter();const fontResource=await loadFontResource(import.meta.url);presenter.set_font(fontResource.bytes,fontResource.hash);
  const worker=new Worker(new URL('./state-worker.js',import.meta.url),{type:'module'});
  let model=null,lastRoute='',applyingRoute=true,routeTarget=null,queue=[],busy=false,releaseCandidate=null,reloading=false;
  function command(m){if(reloading)return;if(queue.length>=128){fatal('Input queue full. Please wait for state processing.');return;}queue.push(m);pump();}
  function pump(){if(!busy&&model&&queue.length){busy=true;worker.postMessage(queue.shift());}else if(!busy&&model&&releaseCandidate&&!pointers.size&&!moves.size&&!reloading){reloading=true;worker.postMessage({type:'release-commit',entry:releaseCandidate});}}
  function route(){return `/${model.screen}${model.keyboard?'/input':''}`;}
  window.addEventListener('popstate',()=>{applyingRoute=true;routeTarget=location.hash.replace(/^#/, '')||'/home';command({type:'route',route:location.hash});});
  const gl=canvas.getContext('webgl',{alpha:false,antialias:false,depth:false,stencil:false});if(!gl)throw new Error('This dashboard requires WebGL.');
  const renderer=new SceneRenderer(gl),staticLayer=renderer.createLayer(),motionLayer=renderer.createLayer(),hudLayer=renderer.createLayer();
  const [curveWidth,curveHeight,bandWidth,bandHeight]=presenter.font_dimensions();
  renderer.setFont({curveWidth,curveHeight,curves:presenter.font_curves(),bandWidth,bandHeight,bands:presenter.font_bands()});revision.dataset.fontRenderer='analytic-quadratic-cached';revision.dataset.gamma='linear-light-srgb';
  let hudRevision=-1,dirty=true,lastFontCache='';const controls=new Map();
  function resize(){try{dirty=renderer.fitCanvas(canvas)||dirty;}catch(error){fatal(error);}}
  resize();new ResizeObserver(resize).observe(canvas);window.addEventListener('resize',resize);
  function accept(f){
    if(model?.screen!==f.screen)presenter.suspend();
    model={...model,...f};currentScreen=f.screen;showAuth();
    if(f.vertices){staticLayer.setFrame(f);
      const ids=new Set(f.controls.map(c=>c.id));for(const [id,b] of controls)if(!ids.has(id)){b.remove();controls.delete(id);}
      for(const c of f.controls){let b=controls.get(c.id);if(!b){b=document.createElement('button');b.type='button';b.dataset.sceneNode=String(c.id);b.addEventListener('click',()=>command({type:'activate',id:c.id}));b.addEventListener('keydown',event=>{const delta={ArrowLeft:[-50,0],ArrowRight:[50,0],ArrowUp:[0,50],ArrowDown:[0,-50]}[event.key];if(delta&&b.dataset.draggable==='true'){event.preventDefault();event.stopPropagation();command({type:'adjust',id:c.id,dx:delta[0],dy:delta[1]});}});controlsLayer.append(b);controls.set(c.id,b);}
        b.dataset.draggable=String(c.draggable);if(Number.isFinite(c.value)){b.setAttribute('aria-valuemin','0');b.setAttribute('aria-valuemax','100');b.setAttribute('aria-valuenow',String(c.value));}else{for(const name of ['aria-valuemin','aria-valuemax','aria-valuenow'])b.removeAttribute(name);}b.setAttribute('aria-label',c.label);b.disabled=c.disabled;b.setAttribute('role',c.role);if(c.role==='switch')b.setAttribute('aria-checked',String(c.checked));else b.removeAttribute('aria-checked');const [x,y,w,h]=c.bounds;Object.assign(b.style,{left:`${x/12.8}%`,top:`${y/8}%`,width:`${w/12.8}%`,height:`${h/8}%`});}
      canvas.setAttribute('aria-label',f.text);revision.textContent=`VIEW ${f.snapshot.slice(0,12)}`;revision.dataset.snapshot=f.snapshot;revision.dataset.layout=f.layout;
    }
    revision.dataset.screen=f.screen;dirty=true;
    const next=route();if(applyingRoute&&(!routeTarget||next===routeTarget)){lastRoute=next;routeTarget=null;history.replaceState({otrios:true,previous:history.state?.previous},'',`#${next}`);applyingRoute=false;}
    else if(!applyingRoute&&next!==lastRoute){if(next===history.state?.previous){lastRoute=next;history.back();}else{const previous=lastRoute;lastRoute=next;history.pushState({otrios:true,previous},'',`#${next}`);}}
    pump();worker.postMessage({type:'frame-ack'});
  }
  worker.onmessage=event=>{try{const m=event.data;switch(m.type){case 'frame':accept(m);break;case 'command-ack':busy=false;pump();break;case 'release-candidate':releaseCandidate=m.entry;pump();break;case 'release-retry':releaseCandidate=null;reloading=false;break;case 'release':location.replace(m.entry+location.hash);break;case 'release-id':revision.dataset.release=m.id;break;case 'monitoring-auth':monitoringAuth=m;showAuth();break;case 'online':dot.classList.toggle('online',m.online);connection.textContent=m.online?'Live shared space':'Offline · unconfirmed changes held in memory';break;case 'warning':connection.textContent=m.message;break;case 'error':fatal(m.message);break;}}catch(error){fatal(error);}};
  worker.onerror=e=>fatal(e.message);
  form.addEventListener('submit',event=>{event.preventDefault();const data=new FormData(form);command({type:'login',username:String(data.get('username')),password:String(data.get('password'))});form.elements.password.value='';});
  function leaveLogin(){command({type:'route',route:'#/home'});}
  document.querySelector('#login-back').addEventListener('click',leaveLogin);login.addEventListener('cancel',event=>{event.preventDefault();leaveLogin();});signout.addEventListener('click',()=>command({type:'logout'}));
  try{sessionStorage.removeItem('otrios-dashboard-v1');sessionStorage.removeItem('otrios-dashboard-presentation-v1');}catch{}
  worker.postMessage({type:'start',fontBytes:fontResource.bytes,fontHash:fontResource.hash,release:location.pathname.match(/^\/releases\/([0-9a-f]{64})\//)?.[1]||null,route:location.hash});
  const pointers=new Set(),moves=new Map();
  function pointer(e,phase){const r=canvas.getBoundingClientRect();return {type:'pointer',args:[phase,e.pointerId,(e.clientX-r.left)*1280/r.width,(e.clientY-r.top)*800/r.height,performance.now()]};}
  canvas.addEventListener('pointerdown',e=>{e.preventDefault();canvas.focus({preventScroll:true});pointers.add(e.pointerId);canvas.setPointerCapture(e.pointerId);command(pointer(e,0));});
  canvas.addEventListener('pointermove',e=>{if(pointers.has(e.pointerId))moves.set(e.pointerId,pointer(e,1));});
  function finish(e,phase){if(!pointers.has(e.pointerId))return;if(moves.has(e.pointerId)){command(moves.get(e.pointerId));moves.delete(e.pointerId);}command(pointer(e,phase));pointers.delete(e.pointerId);}
  canvas.addEventListener('pointerup',e=>finish(e,2));canvas.addEventListener('pointercancel',e=>finish(e,3));canvas.addEventListener('lostpointercapture',e=>finish(e,3));
  window.addEventListener('blur',()=>{moves.clear();for(const id of pointers){command({type:'pointer',args:[3,id,0,0,performance.now()]});if(canvas.hasPointerCapture(id))canvas.releasePointerCapture(id);}pointers.clear();command({type:'cancel'});});
  window.addEventListener('keydown',e=>{if(e.isComposing)return;if(e.key==='Escape'){e.preventDefault();command({type:'back'});return;}if(!model?.editing||e.target.closest?.('#controls button'))return;const shortcut=e.ctrlKey||e.metaKey,key=shortcut&&e.key.toLowerCase()==='a'?'SelectAll':e.key;if(['Backspace','ArrowLeft','ArrowRight','Tab','Enter','SelectAll'].includes(key)){e.preventDefault();command({type:'key',key,select:e.shiftKey});}else if(!shortcut&&!e.altKey&&e.key.length===1){e.preventDefault();command({type:'text',text:e.key});}});
  window.addEventListener('paste',e=>{if(model?.editing){e.preventDefault();command({type:'text',text:e.clipboardData.getData('text/plain')});}});
  let lastMetric=0;
  function frame(now){
    try{
      if(!busy)for(const [id,m]of moves){command(m);moves.delete(id);break;}
      if(releaseCandidate)pump();
      const animated=model&&(model.screen==='animation'||model.screen==='3d');
      if(model&&(dirty||animated)){
        if(animated){const [playing,speed,count]=model.intent;motionLayer.setFrame({vertices:presenter.vertices(model.screen==='3d',playing!==0,speed,count,now,model.updates)});if(hudRevision!==presenter.hud_revision()){hudRevision=presenter.hud_revision();hudLayer.setFrame({vertices:presenter.hud(),glyphVertices:presenter.hud_glyph_vertices(),draws:presenter.hud_draws()});}}
        renderer.prepare(animated?[staticLayer,hudLayer]:[staticLayer]);
        renderer.beginFrame();staticLayer.draw();
        if(animated){motionLayer.draw();hudLayer.draw();}renderer.endFrame();
        const fontCache=JSON.stringify(renderer.cacheStats);if(fontCache!==lastFontCache){revision.dataset.fontCache=fontCache;lastFontCache=fontCache;}
        if(gl.getError()!==gl.NO_ERROR)throw new Error('GPU rendering failed.');dirty=false;
        if(animated&&now-lastMetric>=1000){lastMetric=now;revision.dataset.frameStats=presenter.stats();}
      }
    }catch(e){fatal(e);return;}
    requestAnimationFrame(frame);
  }
  document.addEventListener('visibilitychange',()=>{presenter.suspend();dirty=true;});canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();renderer.contextLost();fatal('Graphics context lost. Reload to reconnect to the shared dashboard.');});requestAnimationFrame(frame);
}catch(e){fatal(e);}
