// SPDX-License-Identifier: Apache-2.0
// Sole owner of app replicas, input, admission and network leases. All state is RAM-only.
import init,{CanvasApp} from './pkg/otrios_dashboard.js';
import {awaitRelease,validRelease} from './release-gate.mjs';
import {MonitoringSession} from './monitoring-session.mjs';
let app,monitoring,socket,inkSocket,activeRelease,pendingRelease,releaseAbort,discoveredRelease,stopped=false;
let retry=250,inkRetry=250,inkTimer=null,inFlight=false,pendingFrame=null,started=false,authenticated=false,lastStall=0;
function error(e){postMessage({type:'error',message:String(e)});}
function sendFrame(){if(inFlight||!pendingFrame)return;const f=pendingFrame;pendingFrame=null;inFlight=true;postMessage({type:'frame',...f},f.vertices?[f.vertices.buffer,f.glyphVertices.buffer,f.draws.buffer]:[]);}
function publish(geometry=true){
  syncInk();
  const frame={screen:app.screen(),epoch:app.epoch(),ui:app.ui_ref(),state:app.state_ref(),keyboard:app.keyboard_visible(),editing:app.editing(),authRequired:app.auth_required(),presentation:app.presentation_kind(),intent:Array.from(app.motion_intent()),updates:app.stress_updates()};
  if(geometry&&frame.ui){frame.vertices=app.vertices();frame.glyphVertices=app.glyph_vertices();frame.draws=app.draws();frame.text=app.text();frame.title=app.title();frame.snapshot=frame.state;frame.layout=frame.ui;frame.controls=Array.from(app.controls(),id=>({id,label:app.label(id),bounds:Array.from(app.bounds(id)),disabled:app.auth_required(),value:app.control_value(id),role:Number.isFinite(app.control_value(id))?'slider':'button',checked:app.control_checked(id),draggable:app.control_draggable(id)}));}
  pendingFrame={...pendingFrame,...frame};sendFrame();
}
function clearPresentation(){syncInk();pendingFrame=null;postMessage({type:'clear-private'});}
function signOut(){dropPrivate();app.set_session_controls(false);app.navigate('home');monitoring.logout();}
function dropPrivate(){if(!app.private())return;app.clear_private();clearPresentation();}
function outgoing(){if(socket?.readyState===WebSocket.OPEN)for(let i=0;i<8;i++){const bytes=app.outgoing();if(!bytes)break;socket.send(bytes);}}
function release(message){
  const r=message.release;if(!validRelease(r))return;
  if(activeRelease&&activeRelease!==r.id){if(pendingRelease?.id===r.id)return;releaseAbort?.abort();releaseAbort=new AbortController();pendingRelease=r;const isCurrent=()=>pendingRelease===r&&!stopped;
    awaitRelease(r,{isCurrent,signal:releaseAbort.signal}).then(async ready=>{if(!ready||!isCurrent())return;while(isCurrent()&&!app.reload_ready())await new Promise(resolve=>setTimeout(resolve,100));if(isCurrent())postMessage({type:'release-candidate',entry:r.web_entry});}).catch(error);return;}
  pendingRelease=null;releaseAbort?.abort();
  activeRelease=r.id;postMessage({type:'release-id',id:r.id});
}
function connect(){
  if(stopped)return;app.reset_connection();const url=new URL('/apps/stream',location.href);url.protocol=url.protocol==='https:'?'wss:':'ws:';const peer=new WebSocket(url);socket=peer;peer.binaryType='arraybuffer';
  peer.onopen=()=>{retry=250;outgoing();};
  peer.onmessage=event=>{if(socket!==peer)return;try{
    // Code activation follows the code host pointer, never a state-stream hint.
    if(typeof event.data==='string')return;
    const changed=app.receive(new Uint8Array(event.data));if(changed&&!app.ui_ref())clearPresentation();
    if(app.auth_required()){dropPrivate();monitoring.refresh().catch(()=>{});publish(false);}else if(changed)publish();
    outgoing();postMessage({type:'online',online:true});
  }catch(e){
    if(app.rejected_mount_pending()){
      try{outgoing();publish();postMessage({type:'warning',message:'The app update was rejected. The current app is still available.'});}
      catch(recoveryError){error(recoveryError);peer.close();}
    }else{error(e);peer.close();}
  }};
  peer.onclose=()=>{if(socket!==peer)return;postMessage({type:'online',online:false});if(!stopped){setTimeout(connect,retry);retry=Math.min(5000,retry*2);}};peer.onerror=()=>peer.close();
}
function wantsInk(){return !stopped&&app?.ui_ref()&&app.presentation_kind()==='touch-pad';}
function syncInk(){
  if(!wantsInk()){clearTimeout(inkTimer);inkTimer=null;const peer=inkSocket;inkSocket=null;peer?.close();return;}
  if(inkSocket||inkTimer)return;
  const url=new URL('/demo/ink',location.href);url.protocol=url.protocol==='https:'?'wss:':'ws:';const peer=new WebSocket(url);inkSocket=peer;peer.binaryType='arraybuffer';
  peer.onopen=()=>{if(inkSocket===peer)inkRetry=250;};
  peer.onmessage=e=>{if(inkSocket!==peer||!wantsInk())return;try{if(typeof e.data!=='string'&&app.receive_ink(new Uint8Array(e.data),performance.now()))publish();}catch{peer.close();}};
  peer.onclose=()=>{if(inkSocket!==peer)return;inkSocket=null;if(wantsInk()){inkTimer=setTimeout(()=>{inkTimer=null;syncInk();},inkRetry);inkRetry=Math.min(5000,inkRetry*2);}};peer.onerror=()=>peer.close();
}
function route(hash){const parts=hash.replace(/^#\/?/,'').split('/'),aliases={controls:'component-playground',keyboard:'input-lab',animation:'animation-lab','3d':'three-d-lab',layout:'layout-lab'};if(app.auth_required())app.clear_private();app.navigate(aliases[parts[0]]||parts[0]||'home');}
onmessage=async({data:m})=>{try{
  if(m.type==='release-discovered'){if(validRelease(m.release)){discoveredRelease=m.release;if(app&&started)release(m);}return;}
  if(m.type==='start'){
    if(started)throw Error('App host is already initialized');started=true;await init();app=new CanvasApp();app.set_font(m.fontBytes,m.fontHash);app.admit_catalog(m.catalogBytes);activeRelease=m.release;
    for(const name of ['otrios-demo-v3','otrios-dashboard-v2'])try{indexedDB.deleteDatabase(name);}catch{}
    monitoring=new MonitoringSession({onStatus:state=>{
      if(!state.authenticated&&app.private()){dropPrivate();app.navigate('home');outgoing();}
      const signedIn=state.authenticated&&!authenticated;authenticated=state.authenticated;
      if(signedIn&&app.auth_required()){app.retry_mount();socket?.close();}
      const chromeChanged=app.set_session_controls(authenticated);
      postMessage({type:'monitoring-auth',...state});publish(chromeChanged);
    }});
    route(m.route||'#/home');connect();monitoring.refresh().catch(()=>postMessage({type:'monitoring-auth',authenticated:false,error:'Monitoring sign-in is temporarily unavailable.'}));
    if(discoveredRelease)release({release:discoveredRelease});
    setInterval(()=>{try{const now=performance.now(),before=app.stress_updates(),changed=app.tick(now,Date.now()/1000),stressed=app.stress_tick(now);if(changed||stressed)publish();else if(app.stress_updates()!==before)publish(false);const stall=app.stress_stall_ms();if(stall&&now-lastStall>=1000){lastStall=now;const until=performance.now()+stall;while(performance.now()<until){/* Deliberately stall only the app worker. */}}syncInk();if(wantsInk()){const ink=app.ink_outgoing(now);if(ink.length&&inkSocket?.readyState===WebSocket.OPEN)inkSocket.send(ink);}outgoing();}catch(e){error(e);}},8);return;
  }
  if(m.type==='frame-ack'){inFlight=false;sendFrame();return;}
  if(!app)return;
  if(m.type==='release-commit'){const r=pendingRelease;if(!r||r.web_entry!==m.entry){postMessage({type:'release-retry',entry:m.entry});return;}while(pendingRelease===r&&!app.reload_ready())await new Promise(resolve=>setTimeout(resolve,100));if(pendingRelease!==r){postMessage({type:'release-retry',entry:m.entry});return;}stopped=true;socket?.close();inkSocket?.close();postMessage({type:'release',entry:r.web_entry});return;}
  app.tick(performance.now(),Date.now()/1000);let changed=false;
  switch(m.type){
    case 'login':monitoring.login(m.username,m.password);break;
    case 'logout':signOut();changed=true;break;
    case 'pointer':if(!app.ui_ref())break;changed=app.pointer(m.epoch??app.epoch(),...m.args.slice(0,4));break;
    case 'activate':if(!app.ui_ref())break;changed=app.activate(m.epoch??app.epoch(),m.id);break;
    case 'adjust':if(!app.ui_ref())break;if(m.epoch===undefined||m.epoch===app.epoch())changed=app.adjust_control(m.id,m.dx,m.dy);break;
    case 'route':route(m.route);changed=true;break;
    case 'back':changed=app.back();break;
    case 'key':if(app.editing())changed=app.key(m.key,m.select);break;
    case 'text':if(app.editing())changed=app.type_text(m.text);break;
    case 'cancel':changed=app.cancel_input();break;
    case 'reload':app.open(app.ui_ref(),app.state_ref());changed=true;break;
  }
  if(app.take_sign_out()){signOut();changed=true;}
  if(changed)publish();outgoing();postMessage({type:'command-ack'});
}catch(e){error(e);postMessage({type:'command-ack'});}};
