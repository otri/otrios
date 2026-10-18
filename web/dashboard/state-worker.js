// SPDX-License-Identifier: Apache-2.0
// Sole owner of CRDT, Shell, network protocol, projection and durable storage.
import init,{Dashboard} from './pkg/otrios_dashboard.js';
import {awaitRelease,validRelease} from './release-gate.mjs';
import {MonitoringSession} from './monitoring-session.mjs';
let monitoring;
let pendingRelease,releaseAbort;
let dashboard,db,socket,retry=250,stopped=false,activeRelease,lastSnapshot;
let durable=true;
function storageError(e){durable=false;postMessage({type:'warning',message:`Changes live in this tab; durable cache unavailable: ${e}`});}
let inFlight=false,pendingFrame=null,lastStall=0;
function sendFrame(){
  if(inFlight||!pendingFrame)return;
  const f=pendingFrame;pendingFrame=null;inFlight=true;
  postMessage({type:'frame',...f},f.vertices?[f.vertices.buffer]:[]);
}
function publish(geometry=true){
  const frame={screen:dashboard.screen(),keyboard:dashboard.keyboard_visible(),editing:dashboard.editing(),intent:Array.from(dashboard.motion_intent()),updates:dashboard.stress_updates()};
  if(geometry){
    frame.vertices=dashboard.vertices();frame.text=dashboard.scene_text();frame.snapshot=dashboard.snapshot_id();frame.layout=dashboard.layout_id();
    frame.controls=Array.from(dashboard.control_ids(),id=>({id,label:dashboard.control_label(id),disabled:dashboard.control_disabled(id),role:dashboard.control_role(id),checked:dashboard.control_checked(id),bounds:Array.from(dashboard.control_bounds(id))}));
  }
  // Preserve unsent geometry when replacing only the counters/intent.
  pendingFrame={...pendingFrame,...frame};sendFrame();
}
function transaction(mode,operation){return new Promise((resolve,reject)=>{const tx=db.transaction('cache',mode);operation(tx.objectStore('cache'));tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);});}
async function load(){
  db=await new Promise((resolve,reject)=>{const r=indexedDB.open('otrios-demo-v3',1);r.onupgradeneeded=()=>r.result.createObjectStore('cache');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
  const cached=await new Promise((resolve,reject)=>{const tx=db.transaction('cache');const r=tx.objectStore('cache').get('state');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
  // Previous builds mixed Monitoring into their cache. Migrate only the public
  // document, close the handle, then remove that combined database.
  let legacy;
  try{const old=await new Promise((resolve,reject)=>{const r=indexedDB.open('otrios-dashboard-v2',1);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
    if(old.objectStoreNames.contains('cache'))legacy=await new Promise(resolve=>{const r=old.transaction('cache').objectStore('cache').get('state');r.onsuccess=()=>resolve(r.result);r.onerror=()=>resolve(null);});
    old.close();indexedDB.deleteDatabase('otrios-dashboard-v2');
  }catch{/* The public authority can restore the shared document. */}
  return cached||{document:legacy?.document};
}
async function persist(){
  const id=dashboard.snapshot_id();if(id===lastSnapshot)return;
  if(!db){storageError('storage unavailable');return;}
  const document=dashboard.document();
  await transaction('readwrite',store=>store.put({document},'state'));lastSnapshot=id;
}
function sendChanges(){if(socket?.readyState===WebSocket.OPEN){const b=dashboard.sync_message();if(b)socket.send(b);}}
function connect(){
  dashboard.reset_sync();const url=new URL('/demo/stream',location.href);url.protocol=url.protocol==='https:'?'wss:':'ws:';
  socket=new WebSocket(url);socket.binaryType='arraybuffer';
  socket.onopen=()=>{retry=250;sendChanges();};
  socket.onmessage=event=>{
    try{
      if(typeof event.data==='string'){
        const message=JSON.parse(event.data);
        if(message.type==='release'){
          const r=message.release;
          if(!validRelease(r))return;
          if(activeRelease&&r.id!==activeRelease){
            if(!db||!durable){postMessage({type:'warning',message:'A code update is ready; reload paused because durable storage is unavailable'});return;}
            if(pendingRelease?.id===r.id)return;
            releaseAbort?.abort();releaseAbort=new AbortController();pendingRelease=r;
            const isCurrent=()=>pendingRelease===r&&!stopped&&durable;
            awaitRelease(r,{isCurrent,signal:releaseAbort.signal}).then(async ready=>{
              if(!ready||!isCurrent())return;await persist();if(!isCurrent())return;
              stopped=true;socket?.close();postMessage({type:'release',entry:r.web_entry});
            }).catch(storageError);return;
          }
          pendingRelease=null;releaseAbort?.abort();activeRelease=r.id;postMessage({type:'release-id',id:r.id});
        }
      }else{
        const bytes=new Uint8Array(event.data);let changed=false;
        if(bytes.length>=5&&String.fromCharCode(...bytes.subarray(0,5))==='OTML1'){
          throw Error('Private data received on the public demo stream');
        }else changed=dashboard.receive_sync(bytes);
        if(changed){publish();persist().catch(storageError);}sendChanges();postMessage({type:'online',online:true});
      }
    }catch(e){console.error(e);socket.close();}
  };
  socket.onclose=()=>{postMessage({type:'online',online:false});if(!stopped){setTimeout(connect,retry);retry=Math.min(retry*2,5000);}};
  socket.onerror=()=>socket.close();
}
function fatal(e){postMessage({type:'error',message:String(e)});}
onmessage=async event=>{
  const m=event.data;
  try{
    if(m.type==='start'){
      await init();let cached;
      try{cached=await load();}catch(e){postMessage({type:'warning',message:`Durable cache unavailable: ${e}`});}
      const saved=cached?.document||m.saved;dashboard=saved?.length?Dashboard.load(saved):new Dashboard();
      monitoring=new MonitoringSession({onStatus:state=>{
        dashboard.set_monitoring_access(state.authenticated);publish();postMessage({type:'monitoring-auth',...state});
      },onView:bytes=>{if(dashboard.present(bytes))publish();}});
      activeRelease=m.release;applyRoute(m.route);publish();await persist().catch(storageError);connect();monitoring.refresh().catch(()=>postMessage({type:'monitoring-auth',authenticated:false,error:'Monitoring sign-in is temporarily unavailable.'}));
      setInterval(()=>{try{
        const now=performance.now();const changed=dashboard.tick(now);const stressed=dashboard.stress_tick(now);
        const stall=dashboard.stress_stall_ms();
        if(stall&&now-lastStall>=1000){lastStall=now;const until=performance.now()+stall;while(performance.now()<until){/* Deliberately stall only the state worker. */}}
        if(changed)publish();else if(stressed)publish(false);
      }catch(e){fatal(e);}},8);
      return;
    }
    if(m.type==='frame-ack'){inFlight=false;sendFrame();return;}
    if(!dashboard)return;
    let changed=false;
    switch(m.type){
      case 'login':monitoring.login(m.username,m.password);break;
      case 'logout':monitoring.logout();break;
      case 'pointer':changed=dashboard.pointer(...m.args.slice(0,4),performance.now());break;
      case 'activate':changed=dashboard.activate(m.id);break;
      case 'route':applyRoute(m.route);changed=true;break;
      case 'back':changed=dashboard.back();break;
      case 'key':changed=dashboard.key(m.key,m.select);break;
      case 'text':changed=dashboard.type_text(m.text);break;
      case 'cancel':dashboard.cancel_input();changed=true;break;
    }
    monitoring.setActive(dashboard.screen()==='monitoring');
    if(changed){publish();sendChanges();persist().catch(storageError);}
    postMessage({type:'command-ack'});
  }catch(e){fatal(e);postMessage({type:'command-ack'});}
};
function applyRoute(route){const p=route.replace(/^#\/?/,'').split('/');dashboard.navigate(p[0]);if(p[0]==='keyboard'&&p[1]==='input'&&!dashboard.keyboard_visible())dashboard.show_keyboard();else if(dashboard.keyboard_visible()&&p[1]!=='input')dashboard.back();monitoring?.setActive(dashboard.screen()==='monitoring');}
