// SPDX-License-Identifier: Apache-2.0
// Sole owner of app replicas, input, admission and network leases. All state is RAM-only.
import init,{CanvasApp} from './pkg/otrios_dashboard.js';
import {validRelease} from './session.mjs';
import {prepareUiUpdate} from './ui-update.mjs';
import {MonitoringSession} from './monitoring-session.mjs';
import {DocumentTransfer} from './document-transfer.mjs';
const documents=new DocumentTransfer();
let timings=false,navigationTiming=null,graphicsPaused=false,graphicsGeneration=0;
const timingNow=()=>performance.timeOrigin+performance.now();
let inputRevision=0,lastInputRevision=-1,composition=null;
let viewportRevision=0,lastTextGeneration=0,lastPlacement=0,lastSemantics=0,lastEffects=0,pendingGeometry=false;
let hostUrl,admittedManifest,updateBusy=false;
let app,monitoring,activeRelease,pendingRelease,discoveredRelease,stopped=false,stateOnline=false;
let transportReady=false,transportEpoch=0,stateSends=0,inkSending=false,inkActive=false,inFlight=null,started=false,authenticated=false,lastStall=0;
let chatIdentityReady=false,chatIdentityRequested=false,chatSending=false,collectionSending=false;
let chatLogKey='';
function pumpChat(){
  if(app?.screen()==='brass-relay'&&!chatIdentityReady){if(!chatIdentityRequested){chatIdentityRequested=true;postMessage({type:'chat-identity'});}return;}
  if(transportReady&&!chatSending){const bytes=app?.chat_outgoing();if(bytes){chatSending=true;postMessage({type:'chat-send',bytes,epoch:transportEpoch},[bytes.buffer]);}}
}
function pumpCollection(){if(transportReady&&!collectionSending){const bytes=app?.collection_outgoing();if(bytes){collectionSending=true;postMessage({type:'collection-send',bytes,epoch:transportEpoch},[bytes.buffer]);}}}
function error(e){postMessage({type:'error',message:String(e)});}
// Input and network work continue while the renderer holds its single credit.
// Coalesce dirty requests before crossing Wasm or assembling transferable arrays.
function publish(geometry=true){pendingGeometry ||= geometry;app.request_frame();sendFrame();}
function sendFrame(){
  if(graphicsPaused||inFlight!==null)return;
  const serial=app.begin_frame();if(serial===undefined||serial===null)return;
  inFlight=serial;
  const geometry=pendingGeometry;pendingGeometry=false;
  syncInk();
  const chat=JSON.parse(app.chat_info());const logKey=chat?`${chat.first_sequence}:${chat.last_sequence}:${chat.messages}:${chat.authors_revision}`:'';if(logKey!==chatLogKey){chatLogKey=logKey;postMessage({type:'chat-log',messages:chat?JSON.parse(app.chat_messages()):[]});}
  const ui=app.ui_ref();
  const plan=ui?JSON.parse(app.prepare_frame()):{revisions:{content:0,placement:0,semantics:0,effects:0},order:[]};
  const revisions=plan.revisions;
  const frame={inputRevision,serial,order:plan.order,extent:Array.from(app.viewport_size()),viewportRevision,screen:app.screen(),path:"/"+app.path(),epoch:app.epoch(),ui,state:app.state_ref(),keyboard:app.keyboard_visible(),editing:app.editing(),authRequired:app.auth_required(),presentation:app.presentation_kind(),intent:Array.from(app.motion_intent()),updates:app.stress_updates(),resources:JSON.parse(app.resource_stats())};
  if(ui&&revisions.effects!==lastEffects){frame.effects=app.effect_vertices();lastEffects=revisions.effects;}
  documents.update(ui?JSON.parse(app.document_frame()):null,`${frame.ui}:${frame.epoch}`);
  if(frame.authRequired){frame.path="/"+app.pending_path();frame.title=app.title();}
  if(geometry&&ui){
    frame.textGeneration=revisions.content;
    if(revisions.content!==lastTextGeneration){frame.vertices=app.vertices();frame.glyphVertices=app.glyph_vertices();frame.draws=app.draws();frame.textStyles=app.text_styles();frame.overlayVertices=app.overlay_vertices();frame.overlayGlyphVertices=app.overlay_glyph_vertices();frame.overlayDraws=app.overlay_draws();frame.overlayStyles=app.overlay_styles();}
    if(revisions.placement!==lastPlacement||revisions.content!==lastTextGeneration){frame.textViews=app.text_views();frame.overlayViews=app.overlay_views();}
    if(revisions.semantics!==lastSemantics||inputRevision!==lastInputRevision){frame.reflow=app.reflow_frame();frame.text=app.text();frame.title=app.title();frame.snapshot=frame.state;frame.layout=frame.ui;frame.inputs=JSON.parse(app.text_inputs());frame.controls=Array.from(app.controls(),id=>({id,label:app.label(id),bounds:Array.from(app.bounds(id)),disabled:app.auth_required(),value:app.control_value(id),role:Number.isFinite(app.control_value(id))?'slider':'button',checked:app.control_checked(id),draggable:app.control_draggable(id)}));}
    lastTextGeneration=revisions.content;lastPlacement=revisions.placement;lastSemantics=revisions.semantics;lastInputRevision=inputRevision;
  }

  if(timings){frame.timing=geometry&&frame.ui&&navigationTiming?.epoch===frame.epoch?{...navigationTiming,frameReady:timingNow()}:null;}
  if(frame.timing&&navigationTiming?.id===frame.timing.id)navigationTiming=null;
  frame.graphicsGeneration=graphicsGeneration;frame.document=documents.take(id=>({glyphVertices:app.document_chunk_vertices(id),vertices:app.document_chunk_geometry(id)}));
  const transfer=[...(frame.effects?[frame.effects.buffer]:[]),...(frame.vertices?[frame.vertices.buffer,frame.glyphVertices.buffer,frame.draws.buffer,frame.textStyles.buffer,frame.overlayVertices.buffer,frame.overlayGlyphVertices.buffer,frame.overlayDraws.buffer,frame.overlayStyles.buffer]:[]),...(frame.textViews?[frame.textViews.buffer,frame.overlayViews.buffer]:[]),...frame.document.upserts.flatMap(u=>[u.glyphVertices.buffer,u.vertices.buffer])];
  postMessage({type:'frame',...frame},transfer);
}
function clearPresentation(){chatLogKey='';composition=null;documents.reset();syncInk();lastTextGeneration=lastPlacement=lastSemantics=lastEffects=0;postMessage({type:'clear-private'});}
function signOut(){dropPrivate();app.set_session_controls(false);app.navigate('');monitoring.logout();}
function dropPrivate(){if(!app.private())return;app.clear_private();clearPresentation();}
function outgoing(){if(transportReady)while(stateSends<4){const mountEpoch=timings?app.pending_mount_epoch():0,bytes=app.outgoing();if(!bytes)break;let timing;if(navigationTiming&&mountEpoch&&app.pending_mount_epoch()===0){navigationTiming.epoch=mountEpoch;navigationTiming.mountQueued=timingNow();timing={...navigationTiming};}stateSends++;postMessage({type:'state-send',bytes,epoch:transportEpoch,...(timing?{timing}:{})},[bytes.buffer]);}pumpChat();pumpCollection();}
function reloadReady(){return !composition&&!app.input_busy()&&(!stateOnline||app.reload_ready());}
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
function receiveState(bytes,receivedAt){try{
  const workerReceived=timings?timingNow():0;
  const changed=app.receive(bytes);if(navigationTiming?.epoch===app.epoch()&&!navigationTiming.qualified){Object.assign(navigationTiming,{receivedAt,workerReceived,qualified:timingNow(),responseBytes:bytes.length});}if(app.take_collection_revoked()||changed&&!app.ui_ref())clearPresentation();
  if(app.auth_required()){dropPrivate();monitoring.refresh().catch(()=>{});publish(false);}else publish(changed);
  stateOnline=true;outgoing();postMessage({type:'online',online:true});
}catch(e){
  if(app.rejected_mount_pending()){
    try{outgoing();publish();postMessage({type:'warning',message:'The app update was rejected. The current app is still available.'});}catch(recoveryError){error(recoveryError);postMessage({type:'transport-reconnect'});}
  }else{error(e);postMessage({type:'transport-reconnect'});}
}}
function wantsInk(){return !stopped&&app?.ui_ref()&&app.presentation_kind()==='touch-pad';}
function syncInk(){const active=!!wantsInk();if(active!==inkActive){inkActive=active;postMessage({type:'ink-subscribe',active});}}
function navigate(screen){
  if(app.auth_required())app.clear_private();
  app.navigate(screen||'');
}
onmessage=async({data:m})=>{try{
  if(m.type==='release-discovered'){if(validRelease(m.release)){discoveredRelease=m.release;if(app&&started)release(m);}return;}
  if(m.type==='start'){
    if(started)throw Error('App host is already initialized');started=true;timings=!!m.timings;if(timings)navigationTiming={id:0,workerStarted:timingNow()};await init({module_or_path:m.wasmModule});hostUrl=m.moduleUrl||location.href;admittedManifest=m.manifest;app=new CanvasApp();app.set_viewport(...(m.viewport||[1280,800]));app.set_font(m.fontBytes,m.fontHash);app.admit_catalog(m.catalogBytes);activeRelease=m.release;
    app.set_reduced_motion(!!m.reducedMotion);
    for(const name of ['otrios-demo-v3','otrios-dashboard-v2'])try{indexedDB.deleteDatabase(name);}catch{}
    monitoring=new MonitoringSession({onStatus:state=>{
      if(!state.authenticated&&app.private()){dropPrivate();app.navigate('');outgoing();}
      const signedIn=state.authenticated&&!authenticated;authenticated=state.authenticated;
      if(signedIn){if(app.auth_required())app.retry_mount();postMessage({type:'transport-reconnect'});}
      const chromeChanged=app.set_session_controls(authenticated);
      postMessage({type:'monitoring-auth',...state});publish(chromeChanged);
    }});
    navigate(m.screen);connect();monitoring.refresh().catch(()=>postMessage({type:'monitoring-auth',authenticated:false,error:'Monitoring sign-in is temporarily unavailable.'}));
    if(discoveredRelease)release({release:discoveredRelease});
    setInterval(()=>{try{const now=performance.now(),before=app.stress_updates(),changed=app.tick(now,Date.now()/1000),stressed=app.stress_tick(now);if(changed||stressed||documents.needsMore)publish();else if(app.stress_updates()!==before)publish(false);const stall=app.stress_stall_ms();if(stall&&now-lastStall>=1000){lastStall=now;const until=performance.now()+stall;while(performance.now()<until){/* Deliberately stall only the app worker. */}}syncInk();if(wantsInk()&&transportReady&&!inkSending){const ink=app.ink_outgoing(now);if(ink.length){inkSending=true;postMessage({type:'ink-send',bytes:ink,epoch:transportEpoch},[ink.buffer]);}}outgoing();}catch(e){error(e);}},8);return;
  }
  if(m.type==='frame-ack'){if(m.serial!==inFlight||!app.frame_presented(m.serial))return;documents.ack(m.documentToken);inFlight=null;sendFrame();return;}
  if(m.type==='graphics-lost'){graphicsGeneration=m.generation??graphicsGeneration+1;graphicsPaused=true;if(inFlight!==null)app.frame_presented(inFlight);inFlight=null;documents.reset();lastTextGeneration=lastPlacement=lastSemantics=lastEffects=0;return;}
  if(m.type==='document-replay'){if(m.generation!==undefined&&m.generation!==graphicsGeneration)return;graphicsPaused=false;documents.reset();lastTextGeneration=lastPlacement=lastSemantics=lastEffects=0;if(app)publish();return;}
  if(!app)return;
  if(m.type==='transport'){
    if(stopped)return;const was=transportReady,changed=m.epoch!==transportEpoch;transportReady=m.ready;transportEpoch=m.epoch;if(changed){stateSends=0;inkSending=false;chatSending=false;collectionSending=false;}
    if(transportReady&&(!was||changed)){app.reset_connection();outgoing();syncInk();}
    if(!transportReady){app.chat_disconnected();chatSending=false;stateOnline=false;postMessage({type:'online',online:false});}return;
  }
  if(m.type==='state-frame'){try{if(!stopped&&transportReady&&m.epoch===transportEpoch)receiveState(m.bytes,m.receivedAt);}finally{postMessage({type:'state-frame-ack',epoch:m.epoch});}return;}
  if(m.type==='ink-frame'){try{if(transportReady&&m.epoch===transportEpoch&&wantsInk()&&app.receive_ink(m.bytes,performance.now()))publish();}finally{postMessage({type:'ink-frame-ack',epoch:m.epoch});}return;}
  if(m.type==='chat-frame'){try{if(transportReady&&m.epoch===transportEpoch){const changed=app.receive_chat(m.bytes);if(app.take_collection_revoked())clearPresentation();if(changed)publish();}}finally{postMessage({type:'chat-frame-ack',epoch:m.epoch});}outgoing();return;}
  if(m.type==='collection-frame'){try{if(transportReady&&m.epoch===transportEpoch){const changed=app.receive_collection(m.bytes);if(app.take_collection_revoked())clearPresentation();if(changed)publish();}}finally{postMessage({type:'collection-frame-ack',epoch:m.epoch});}outgoing();return;}
  if(m.type==='collection-send-ack'){if(m.epoch===transportEpoch){collectionSending=false;pumpCollection();}return;}
  if(m.type==='chat-send-ack'){if(m.epoch===transportEpoch){chatSending=false;pumpChat();}return;}
  if(m.type==='chat-identity-ready'){chatIdentityReady=true;outgoing();return;}
  if(m.type==='state-send-ack'){if(m.epoch===transportEpoch){stateSends=Math.max(0,stateSends-1);outgoing();}return;}
  if(m.type==='ink-send-ack'){if(m.epoch===transportEpoch)inkSending=false;return;}

  if(m.type==='release-commit'){const r=pendingRelease;if(!r||r.web_entry!==m.entry){postMessage({type:'release-retry',entry:m.entry});return;}while(pendingRelease===r&&!reloadReady())await new Promise(resolve=>setTimeout(resolve,100));if(pendingRelease!==r){postMessage({type:'release-retry',entry:m.entry});return;}stopped=true;transportReady=false;syncInk();postMessage({type:'release',entry:r.web_entry});return;}
  const workerStarted=timings?timingNow():0;
  const pointerCommand=['pointer','pointer-batch'].includes(m.type);
  if(!pointerCommand)app.tick(performance.now(),Date.now()/1000);let changed=false;
  const applyPointer=args=>app.pointer(m.epoch??app.epoch(),...args.slice(0,4),Number.isFinite(args[4])?Math.max(0,args[4]-(performance.timeOrigin||0)):performance.now());
  if(['pointer','pointer-batch','activate','adjust'].includes(m.type)&&m.viewportRevision!==undefined&&m.viewportRevision!==viewportRevision){postMessage({type:'command-ack'});return;}
  switch(m.type){
    case 'viewport':app.cancel_input();changed=app.set_viewport(...m.size);viewportRevision++;changed=!!app.ui_ref()||changed;break;
    case 'reduced-motion':app.set_reduced_motion(!!m.reduced);changed=true;break;
    case 'login':monitoring.login(m.username,m.password);break;
    case 'logout':signOut();changed=true;break;
    case 'pointer':if(!app.ui_ref())break;changed=applyPointer(m.args);break;
    case 'pointer-batch':if(!app.ui_ref())break;if(!Array.isArray(m.samples)||m.samples.length>64)throw Error('Pointer batch limit');for(const args of m.samples)changed=applyPointer(args)||changed;break;
    case 'activate':if(!app.ui_ref())break;changed=app.activate(m.epoch??app.epoch(),m.id);break;
    case 'adjust':if(!app.ui_ref())break;if(m.epoch===undefined||m.epoch===app.epoch())changed=app.adjust_control(m.id,m.dx,m.dy);break;
    case 'navigate':if(updateBusy&&app.catalog_update_pending())break;navigate(m.screen);changed=true;break;
    case 'back':changed=app.back();break;
    case 'text-focus':changed=app.activate(m.epoch,m.id);break;
    case 'text-input':changed=app.replace_text_input(m.epoch,m.id,m.text,m.cursor,m.anchor,!!m.selectionOnly);changed=app.scroll_text_input(m.epoch,m.id,m.scroll||0)||changed;break;
    case 'text-scroll':changed=app.scroll_text_input(m.epoch,m.id,m.scroll);break;
    case 'text-enter':changed=app.text_input_key(m.epoch,m.id,'Enter');break;
    case 'text-blur':changed=app.blur_text_input(m.epoch,m.id);break;
    case 'text-composition':if(m.epoch===app.epoch())composition=m.active?{epoch:m.epoch,id:m.id}:null;break;
    case 'key':if(app.editing())changed=app.key(m.key,m.select);break;
    case 'text':if(app.editing())changed=app.type_text(m.text);break;
    case 'cancel':changed=app.cancel_input();break;
    case 'reload':app.open(app.ui_ref(),app.state_ref());changed=true;break;
  }
  if(m.inputRevision){inputRevision=Math.max(inputRevision,m.inputRevision);changed=true;}
  if(composition&&composition.epoch!==app.epoch())composition=null;
  if(pointerCommand)changed=app.tick(performance.now(),Date.now()/1000)||changed;
  if(app.take_sign_out()){signOut();changed=true;}
  if(timings&&m.timing&&app.pending_mount_epoch())navigationTiming={...m.timing,workerStarted};
  if(changed)publish();outgoing();postMessage({type:'command-ack'});
}catch(e){error(e);postMessage({type:'command-ack'});}};
