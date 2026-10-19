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
  function syncInk(){if(ready&&subscribed!==wantsInk){peer.send(JSON.stringify({type:'ink',enabled:wantsInk}));subscribed=wantsInk;}}
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
    const frame=new Uint8Array(bytes.length+1);frame[0]=channel;frame.set(bytes,1);peer.send(frame);return true;
  }
  connect();
  return {
    get ready(){return ready;},get epoch(){return generation;},
    sendState:(bytes,epoch)=>send(STATE,bytes,epoch),sendInk:(bytes,epoch)=>wantsInk&&send(INK,bytes,epoch),
    subscribeInk(value){wantsInk=!!value;syncInk();},
    reconnect(){const old=peer;peer=null;cancel(timer);cancel(deadline);subscribed=false;status(false);old?.close();delay=250;connect();},
    stop(){stopped=true;cancel(timer);cancel(deadline);const old=peer;peer=null;subscribed=false;status(false);old?.close();},
  };
}
