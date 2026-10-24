// SPDX-License-Identifier: Apache-2.0
// Sole owner of app replicas, input, admission and network leases. All state is RAM-only.
import init,{CanvasApp} from './pkg/otrios_dashboard.js';
import {validRelease} from './session.mjs';
import {prepareUiUpdate} from './ui-update.mjs';
import {MonitoringSession} from './monitoring-session.mjs';
import {parseRoute} from './navigation.mjs';
let viewportRevision=0;
let hostUrl,admittedManifest,updateBusy=false;
let app,monitoring,activeRelease,pendingRelease,discoveredRelease,stopped=false,stateOnline=false;
let transportReady=false,transportEpoch=0,stateSends=0,inkSending=false,inkActive=false,inFlight=false,pendingFrame=null,started=false,authenticated=false,lastStall=0;
function error(e){postMessage({type:'error',message:String(e)});}
function sendFrame(){if(inFlight||!pendingFrame)return;const f=pendingFrame;pendingFrame=null;inFlight=true;postMessage({type:'frame',...f},f.vertices?[f.vertices.buffer,f.glyphVertices.buffer,f.draws.buffer]:[]);}
function publish(geometry=true){
  syncInk();
  const frame={extent:Array.from(app.viewport_size()),viewportRevision,screen:app.screen(),epoch:app.epoch(),ui:app.ui_ref(),state:app.state_ref(),keyboard:app.keyboard_visible(),editing:app.editing(),authRequired:app.auth_required(),presentation:app.presentation_kind(),intent:Array.from(app.motion_intent()),updates:app.stress_updates(),resources:JSON.parse(app.resource_stats())};
  if(frame.authRequired)frame.routeScreen=app.pending_ui_ref().match(/^app:([a-z-]+)\/ui$/)?.[1]||frame.screen;
  if(geometry&&frame.ui){frame.reflow=app.reflow_frame();frame.vertices=app.vertices();frame.glyphVertices=app.glyph_vertices();frame.draws=app.draws();frame.text=app.text();frame.title=app.title();frame.snapshot=frame.state;frame.layout=frame.ui;frame.controls=Array.from(app.controls(),id=>({id,label:app.label(id),bounds:Array.from(app.bounds(id)),disabled:app.auth_required(),value:app.control_value(id),role:Number.isFinite(app.control_value(id))?'slider':'button',checked:app.control_checked(id),draggable:app.control_draggable(id)}));}
  pendingFrame={...pendingFrame,...frame};sendFrame();
}
function clearPresentation(){syncInk();pendingFrame=null;postMessage({type:'clear-private'});}
function signOut(){dropPrivate();app.set_session_controls(false);app.navigate('home');monitoring.logout();}
function dropPrivate(){if(!app.private())return;app.clear_private();clearPresentation();}
function outgoing(){if(transportReady)while(stateSends<4){const bytes=app.outgoing();if(!bytes)break;stateSends++;postMessage({type:'state-send',bytes,epoch:transportEpoch},[bytes.buffer]);}}
function reloadReady(){return !app.input_busy()&&(!stateOnline||app.reload_ready());}
function release(message){
  const r=message.release;if(!validRelease(r))return;
  if(activeRelease&&activeRelease!==r.id){if(pendingRelease?.id===r.id)return;pendingRelease=r;const isCurrent=()=>pendingRelease===r&&!stopped;
    Promise.resolve().then(async()=>{
      updateBusy=true;
      try{
        const update=await prepareUiUpdate(hostUrl,activeRelease,r.id,admittedManifest);
        while(isCurrent()&&(!reloadReady()||(update&&(!stateOnline||!app.can_update_catalog()))))await new Promise(resolve=>setTimeout(resolve,100));
        if(!isCurrent())return;
        if(update){
          const changesView=app.update_catalog(update.bytes);outgoing();
          while(isCurrent()&&app.catalog_update_pending())await new Promise(resolve=>setTimeout(resolve,25));
          if(!isCurrent())return;
          if(app.catalog_hash()!==update.hash)throw Error('UI candidate rejected; retaining current view');
          activeRelease=r.id;admittedManifest=update.manifest;pendingRelease=null;publish(changesView);postMessage({type:'release-id',id:r.id,uiOnly:true});return;
        }
        postMessage({type:'release-candidate',entry:r.web_entry});
      }catch(e){postMessage({type:'warning',message:String(e)});if(isCurrent())pendingRelease=null;}
      finally{updateBusy=false;}
    });return;}
  pendingRelease=null;
  activeRelease=r.id;postMessage({type:'release-id',id:r.id});
}
function connect(){stateOnline=false;transportReady=false;postMessage({type:'transport-ready'});}
function receiveState(bytes){try{
  const changed=app.receive(bytes);if(changed&&!app.ui_ref())clearPresentation();
  if(app.auth_required()){dropPrivate();monitoring.refresh().catch(()=>{});publish(false);}else publish(changed);
  stateOnline=true;outgoing();postMessage({type:'online',online:true});
}catch(e){
  if(app.rejected_mount_pending()){
    try{outgoing();publish();postMessage({type:'warning',message:'The app update was rejected. The current app is still available.'});}catch(recoveryError){error(recoveryError);postMessage({type:'transport-reconnect'});}
  }else{error(e);postMessage({type:'transport-reconnect'});}
}}
function wantsInk(){return !stopped&&app?.ui_ref()&&app.presentation_kind()==='touch-pad';}
function syncInk(){const active=!!wantsInk();if(active!==inkActive){inkActive=active;postMessage({type:'ink-subscribe',active});}}
function route(path){
  const screen=parseRoute(path);if(!screen)throw Error('Unknown screen path');
  if(app.auth_required())app.clear_private();
  if(app.ui_ref()&&app.screen()===screen&&!app.auth_required())return;
  app.navigate(screen);
}
onmessage=async({data:m})=>{try{
  if(m.type==='release-discovered'){if(validRelease(m.release)){discoveredRelease=m.release;if(app&&started)release(m);}return;}
  if(m.type==='start'){
    if(started)throw Error('App host is already initialized');started=true;await init();hostUrl=m.moduleUrl||location.href;admittedManifest=m.manifest;app=new CanvasApp();app.set_viewport(...(m.viewport||[1280,800]));app.set_font(m.fontBytes,m.fontHash);app.admit_catalog(m.catalogBytes);activeRelease=m.release;
    for(const name of ['otrios-demo-v3','otrios-dashboard-v2'])try{indexedDB.deleteDatabase(name);}catch{}
    monitoring=new MonitoringSession({onStatus:state=>{
      if(!state.authenticated&&app.private()){dropPrivate();app.navigate('home');outgoing();}
      const signedIn=state.authenticated&&!authenticated;authenticated=state.authenticated;
      if(signedIn&&app.auth_required()){app.retry_mount();postMessage({type:'transport-reconnect'});}
      const chromeChanged=app.set_session_controls(authenticated);
      postMessage({type:'monitoring-auth',...state});publish(chromeChanged);
    }});
    route(m.route||'/home');connect();monitoring.refresh().catch(()=>postMessage({type:'monitoring-auth',authenticated:false,error:'Monitoring sign-in is temporarily unavailable.'}));
    if(discoveredRelease)release({release:discoveredRelease});
    setInterval(()=>{try{const now=performance.now(),before=app.stress_updates(),changed=app.tick(now,Date.now()/1000),stressed=app.stress_tick(now);if(changed||stressed)publish();else if(app.stress_updates()!==before)publish(false);const stall=app.stress_stall_ms();if(stall&&now-lastStall>=1000){lastStall=now;const until=performance.now()+stall;while(performance.now()<until){/* Deliberately stall only the app worker. */}}syncInk();if(wantsInk()&&transportReady&&!inkSending){const ink=app.ink_outgoing(now);if(ink.length){inkSending=true;postMessage({type:'ink-send',bytes:ink,epoch:transportEpoch},[ink.buffer]);}}outgoing();}catch(e){error(e);}},8);return;
  }
  if(m.type==='frame-ack'){inFlight=false;sendFrame();return;}
  if(!app)return;
  if(m.type==='transport'){
    if(stopped)return;const was=transportReady,changed=m.epoch!==transportEpoch;transportReady=m.ready;transportEpoch=m.epoch;if(changed){stateSends=0;inkSending=false;}
    if(transportReady&&(!was||changed)){app.reset_connection();outgoing();syncInk();}
    if(!transportReady){stateOnline=false;postMessage({type:'online',online:false});}return;
  }
  if(m.type==='state-frame'){try{if(!stopped&&transportReady&&m.epoch===transportEpoch)receiveState(m.bytes);}finally{postMessage({type:'state-frame-ack',epoch:m.epoch});}return;}
  if(m.type==='ink-frame'){try{if(transportReady&&m.epoch===transportEpoch&&wantsInk()&&app.receive_ink(m.bytes,performance.now()))publish();}finally{postMessage({type:'ink-frame-ack',epoch:m.epoch});}return;}
  if(m.type==='state-send-ack'){if(m.epoch===transportEpoch){stateSends=Math.max(0,stateSends-1);outgoing();}return;}
  if(m.type==='ink-send-ack'){if(m.epoch===transportEpoch)inkSending=false;return;}

  if(m.type==='release-commit'){const r=pendingRelease;if(!r||r.web_entry!==m.entry){postMessage({type:'release-retry',entry:m.entry});return;}while(pendingRelease===r&&!reloadReady())await new Promise(resolve=>setTimeout(resolve,100));if(pendingRelease!==r){postMessage({type:'release-retry',entry:m.entry});return;}stopped=true;transportReady=false;syncInk();postMessage({type:'release',entry:r.web_entry});return;}
  app.tick(performance.now(),Date.now()/1000);let changed=false;
  if(['pointer','activate','adjust'].includes(m.type)&&m.viewportRevision!==undefined&&m.viewportRevision!==viewportRevision){postMessage({type:'command-ack'});return;}
  switch(m.type){
    case 'viewport':app.cancel_input();changed=app.set_viewport(...m.size);viewportRevision++;changed=!!app.ui_ref()||changed;break;
    case 'login':monitoring.login(m.username,m.password);break;
    case 'logout':signOut();changed=true;break;
    case 'pointer':if(!app.ui_ref())break;changed=app.pointer(m.epoch??app.epoch(),...m.args.slice(0,4));break;
    case 'activate':if(!app.ui_ref())break;changed=app.activate(m.epoch??app.epoch(),m.id);break;
    case 'adjust':if(!app.ui_ref())break;if(m.epoch===undefined||m.epoch===app.epoch())changed=app.adjust_control(m.id,m.dx,m.dy);break;
    case 'route':if(updateBusy&&app.catalog_update_pending())break;route(m.route);changed=true;break;
    case 'back':changed=app.back();break;
    case 'key':if(app.editing())changed=app.key(m.key,m.select);break;
    case 'text':if(app.editing())changed=app.type_text(m.text);break;
    case 'cancel':changed=app.cancel_input();break;
    case 'reload':app.open(app.ui_ref(),app.state_ref());changed=true;break;
  }
  if(app.take_sign_out()){signOut();changed=true;}
  if(changed)publish();outgoing();postMessage({type:'command-ack'});
}catch(e){error(e);postMessage({type:'command-ack'});}};
