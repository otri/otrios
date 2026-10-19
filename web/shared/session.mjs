// SPDX-License-Identifier: Apache-2.0
// Stable bootstrap/network owner. No dependency on Wasm or the CRDT decoder.
export const CONTROL_VERSION=1,STATE_PROTOCOL='otrios-canvas-sync-v2',STATE=1,INK=2;
export function validRelease(r){return r&&/^[0-9a-f]{64}$/.test(r.id)&&r.web_entry===`/releases/${r.id}/web/index.html`;}
export function connectSession({
  onRelease=()=>{},onState=()=>{},onInk=()=>{},onStatus=()=>{},onError=()=>{},
  open=url=>new WebSocket(url),schedule=setTimeout,cancel=clearTimeout,
  base=location.href,current=base.match(/\/releases\/([0-9a-f]{64})\//)?.[1]||null,
  stateProtocol=STATE_PROTOCOL,
}={}){
  let stopped=false,peer,timer,deadline,last,delay=250,ready=false,wantsInk=false,subscribed=false,generation=0;
  const url=new URL('/session',base);url.protocol=url.protocol==='https:'?'wss:':'ws:';
  function status(value){if(ready!==value){ready=value;onStatus(value,generation);}if(value)syncInk();}
  function syncInk(){if(ready&&subscribed!==wantsInk){try{peer.send(JSON.stringify({type:'ink',enabled:wantsInk}));subscribed=wantsInk;}catch(error){onError(error);status(false);peer.close();}}}
  function retry(){if(stopped)return;timer=schedule(connect,delay);delay=Math.min(10000,delay*2);}
  function connect(){
    if(stopped)return;let socket;
    try{socket=open(url);}catch(error){onError(error);retry();return;}
    const epoch=++generation;peer=socket;socket.binaryType='arraybuffer';let greeted=false,compatible=false;
    deadline=schedule(()=>{if(peer===socket)socket.close();},5000);
    socket.onopen=()=>{if(peer===socket&&!stopped)socket.send(JSON.stringify({type:'hello',version:CONTROL_VERSION,release:current,state_protocol:stateProtocol,state:true}));};
    socket.onmessage=({data})=>{
      if(stopped||peer!==socket)return;
      try{
        if(typeof data==='string'){
          if(data.length>8192)throw Error('Control event exceeds limit');const event=JSON.parse(data);
          if(event.version!==CONTROL_VERSION)throw Error('Unsupported bootstrap envelope');
          if(event.type==='state-status'){
            if(!greeted||typeof event.available!=='boolean')throw Error('Unexpected state status');
            status(compatible&&event.available);return;
          }
          if(!['hello','upgrade'].includes(event.type)||(!greeted&&event.type!=='hello')||(greeted&&event.type==='hello')||!(event.release===null||validRelease(event.release))||typeof event.compatible!=='boolean'||typeof event.state_available!=='boolean')throw Error('Invalid hello/upgrade');
          greeted=true;compatible=event.compatible;delay=250;cancel(deadline);
          // Mark the data path unavailable before an incompatible upgrade is offered.
          status(compatible&&event.state_available);
          if(event.release&&event.release.id!==last){last=event.release.id;onRelease(event.release);}
          return;
        }
        if(!greeted||!ready||!(data instanceof ArrayBuffer)||data.byteLength<2)throw Error('Binary data before compatible negotiation');
        const frame=new Uint8Array(data),payload=frame.subarray(1);
        if(frame[0]===STATE&&payload.length<=512*1024)onState(payload,epoch);
        else if(frame[0]===INK&&payload.length<=1025*21){if(wantsInk)onInk(payload,epoch);}
        else throw Error('Invalid session channel/size');
      }catch(error){onError(error);socket.close();}
    };
    socket.onclose=()=>{if(stopped||peer!==socket)return;cancel(deadline);peer=null;subscribed=false;status(false);retry();};
    socket.onerror=()=>socket.close();
  }
  function send(channel,bytes,epoch=generation){
    if(epoch!==generation||!ready||!peer||bytes.length===0)return false;
    if(bytes.length>(channel===STATE?512*1024:32*15))throw Error('Session payload exceeds limit');
    if(peer.bufferedAmount>1024*1024){peer.close();return false;}
    const frame=new Uint8Array(bytes.length+1);frame[0]=channel;frame.set(bytes,1);try{peer.send(frame);return true;}catch(error){onError(error);status(false);peer.close();return false;}
  }
  connect();
  return {
    get ready(){return ready;},get epoch(){return generation;},
    sendState:(bytes,epoch)=>send(STATE,bytes,epoch),sendInk:(bytes,epoch)=>wantsInk&&send(INK,bytes,epoch),
    subscribeInk(value){wantsInk=!!value;syncInk();},
    resume(){if(stopped){stopped=false;delay=250;connect();}},
    reconnect(){if(stopped)return;const old=peer;peer=null;cancel(timer);cancel(deadline);subscribed=false;status(false);old?.close();delay=250;connect();},
    stop(){stopped=true;cancel(timer);cancel(deadline);const old=peer;peer=null;subscribed=false;status(false);old?.close();},
  };
}

// Bounded network/app-worker handoff. State overflow reconnects for resync;
// ephemeral ink overflow drops samples. Epochs reject work from old sessions.
export function bridgeWorker(session,post){
  let epoch=0,state=0,ink=0;
  return {
    status(ready,nextEpoch){if(epoch!==nextEpoch){epoch=nextEpoch;state=0;ink=0;}post({type:'transport',ready,epoch});},
    receive(type,bytes,atEpoch){
      if(atEpoch!==session.epoch)return;
      if(type==='state'){if(state>=8){session.reconnect();return;}state++;}
      else{if(ink>=8)return;ink++;}
      post({type:type+'-frame',bytes,epoch:atEpoch},[bytes.buffer]);
    },
    handle(message){
      const m=message;
      switch(m.type){
        case 'transport-ready':this.status(session.ready,session.epoch);break;
        case 'transport-reconnect':session.reconnect();break;
        case 'state-send':session.sendState(m.bytes,m.epoch);post({type:'state-send-ack',epoch:m.epoch});break;
        case 'ink-send':session.sendInk(m.bytes,m.epoch);post({type:'ink-send-ack',epoch:m.epoch});break;
        case 'ink-subscribe':session.subscribeInk(m.active);break;
        case 'state-frame-ack':if(m.epoch===epoch)state=Math.max(0,state-1);break;
        case 'ink-frame-ack':if(m.epoch===epoch)ink=Math.max(0,ink-1);break;
        default:return false;
      }
      return true;
    },
  };
}
