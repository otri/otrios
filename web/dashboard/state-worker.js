// SPDX-License-Identifier: Apache-2.0
// Sole owner of CRDT, Shell, network protocol, projection and RAM-only working state.
import init,{Dashboard} from './pkg/otrios_dashboard.js';
import {awaitRelease,validRelease} from './release-gate.mjs';
import {MonitoringSession} from './monitoring-session.mjs';
let monitoring,inkSocket,inkRetry=250;
let pendingRelease,releaseAbort;
let dashboard,socket,retry=250,stopped=false,activeRelease;
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
    frame.controls=Array.from(dashboard.control_ids(),id=>({id,label:dashboard.control_label(id),disabled:dashboard.control_disabled(id),role:dashboard.control_role(id),checked:dashboard.control_checked(id),bounds:Array.from(dashboard.control_bounds(id)),value:dashboard.control_value(id),draggable:dashboard.control_draggable(id)}));
  }
  // Preserve unsent geometry when replacing only the counters/intent.
  pendingFrame={...pendingFrame,...frame};sendFrame();
}
// Retire old document caches. Artifact caching has its own independent budget.
function retireDocumentCaches(){
  for(const name of ['otrios-demo-v3','otrios-dashboard-v2']){
    try{indexedDB.deleteDatabase(name);}catch{/* Storage can be unavailable. */}
  }
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
            if(pendingRelease?.id===r.id)return;
            releaseAbort?.abort();releaseAbort=new AbortController();pendingRelease=r;
            const isCurrent=()=>pendingRelease===r&&!stopped;
            awaitRelease(r,{isCurrent,signal:releaseAbort.signal}).then(async ready=>{
              if(!ready||!isCurrent())return;
              // The server has persisted every acknowledged head before it replies.
              while(isCurrent()&&(!dashboard.state_confirmed()||dashboard.input_busy()))await new Promise(resolve=>setTimeout(resolve,100));
              if(!isCurrent())return;
              postMessage({type:'release-candidate',entry:r.web_entry});
            }).catch(e=>postMessage({type:'warning',message:String(e)}));return;
          }
          pendingRelease=null;releaseAbort?.abort();activeRelease=r.id;postMessage({type:'release-id',id:r.id});
        }
      }else{
        const bytes=new Uint8Array(event.data);let changed=false;
        if(bytes.length>=5&&String.fromCharCode(...bytes.subarray(0,5))==='OTML1'){
          throw Error('Private data received on the public demo stream');
        }else changed=dashboard.receive_sync(bytes);
        if(changed)publish();sendChanges();postMessage({type:'online',online:true});
      }
    }catch(e){console.error(e);socket.close();}
  };
  socket.onclose=()=>{postMessage({type:'online',online:false});if(!stopped){setTimeout(connect,retry);retry=Math.min(retry*2,5000);}};
  socket.onerror=()=>socket.close();
}
function connectInk(){
  const url=new URL('/demo/ink',location.href);url.protocol=url.protocol==='https:'?'wss:':'ws:';
  inkSocket=new WebSocket(url);inkSocket.binaryType='arraybuffer';
  inkSocket.onopen=()=>{inkRetry=250;};
  inkSocket.onmessage=event=>{try{if(typeof event.data!=='string'&&dashboard.receive_ink(new Uint8Array(event.data),performance.now()))publish();}catch{inkSocket.close();}};
  inkSocket.onclose=()=>{if(!stopped){setTimeout(connectInk,inkRetry);inkRetry=Math.min(5000,inkRetry*2);}};
  inkSocket.onerror=()=>inkSocket.close();
}
function fatal(e){postMessage({type:'error',message:String(e)});}
onmessage=async event=>{
  const m=event.data;
  try{
    if(m.type==='start'){
      await init();retireDocumentCaches();dashboard=new Dashboard();dashboard.set_wall_time(Date.now()/1000);
      monitoring=new MonitoringSession({onStatus:state=>{
        dashboard.set_monitoring_access(state.authenticated);publish();postMessage({type:'monitoring-auth',...state});
      },onView:bytes=>{if(dashboard.present(bytes))publish();}});
      activeRelease=m.release;applyRoute(m.route);publish();connect();connectInk();monitoring.refresh().catch(()=>postMessage({type:'monitoring-auth',authenticated:false,error:'Monitoring sign-in is temporarily unavailable.'}));
      setInterval(()=>{try{
        const now=performance.now();
        const ink=dashboard.ink_outgoing(now);
        if(ink.length&&inkSocket?.readyState===WebSocket.OPEN)inkSocket.send(ink);
        const clockChanged=dashboard.set_wall_time(Date.now()/1000);
        const changed=dashboard.tick(now)||clockChanged;const stressed=dashboard.stress_tick(now);
        const stall=dashboard.stress_stall_ms();
        if(stall&&now-lastStall>=1000){lastStall=now;const until=performance.now()+stall;while(performance.now()<until){/* Deliberately stall only the state worker. */}}
        if(changed){publish();sendChanges();}else if(stressed)publish(false);
      }catch(e){fatal(e);}},8);
      return;
    }
    if(m.type==='release-commit'){
      const r=pendingRelease;
      if(!r||r.web_entry!==m.entry){postMessage({type:'release-retry'});return;}
      while(pendingRelease===r&&(!dashboard.state_confirmed()||dashboard.input_busy()))await new Promise(resolve=>setTimeout(resolve,100));
      if(pendingRelease!==r){postMessage({type:'release-retry'});return;}
      stopped=true;socket?.close();inkSocket?.close();postMessage({type:'release',entry:r.web_entry});return;
    }
    if(m.type==='frame-ack'){inFlight=false;sendFrame();return;}
    if(!dashboard)return;
    const clockChanged=dashboard.set_wall_time(Date.now()/1000);
    let changed=false;
    switch(m.type){
      case 'login':monitoring.login(m.username,m.password);break;
      case 'logout':monitoring.logout();break;
      case 'pointer':changed=dashboard.pointer(...m.args.slice(0,4),performance.now());break;
      case 'adjust':changed=dashboard.adjust_control(m.id,m.dx,m.dy);break;
      case 'activate':changed=dashboard.activate(m.id);break;
      case 'route':applyRoute(m.route);changed=true;break;
      case 'back':changed=dashboard.back();break;
      case 'key':changed=dashboard.key(m.key,m.select);break;
      case 'text':changed=dashboard.type_text(m.text);break;
      case 'cancel':dashboard.cancel_input();changed=true;break;
    }
    monitoring.setActive(dashboard.screen()==='monitoring');
    if(changed||clockChanged){publish();if(changed)sendChanges();}
    postMessage({type:'command-ack'});
  }catch(e){fatal(e);postMessage({type:'command-ack'});}
};
function applyRoute(route){const p=route.replace(/^#\/?/,'').split('/');dashboard.navigate(p[0]);if(p[0]==='keyboard'&&p[1]==='input'&&!dashboard.keyboard_visible())dashboard.show_keyboard();else if(dashboard.keyboard_visible()&&p[1]!=='input')dashboard.back();monitoring?.setActive(dashboard.screen()==='monitoring');}
