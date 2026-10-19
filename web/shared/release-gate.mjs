// SPDX-License-Identifier: Apache-2.0
// Stable v1 bootstrap. App/CRDT protocol changes must not retire this envelope.
export const CONTROL_VERSION=1,STATE_PROTOCOL='otrios-canvas-sync-v2';
export function validRelease(r) {
  return r && /^[0-9a-f]{64}$/.test(r.id) && r.web_entry===`/releases/${r.id}/web/index.html`;
}
export function watchReleases(onRelease,{
  open=url=>new WebSocket(url),schedule=setTimeout,cancel=clearTimeout,
  base=location.href,current=base.match(/\/releases\/([0-9a-f]{64})\//)?.[1]||null,
  stateProtocol=STATE_PROTOCOL,
}={}) {
  let stopped=false,peer,timer,deadline,last,delay=250;
  const url=new URL('/release/events',base);url.protocol=url.protocol==='https:'?'wss:':'ws:';
  function connect(){
    if(stopped)return;
    let socket;try{socket=open(url);}catch{timer=schedule(connect,delay);delay=Math.min(10000,delay*2);return;}
    peer=socket;let greeted=false;
    deadline=schedule(()=>{if(peer===socket)socket.close();},5000);
    socket.onopen=()=>{if(peer===socket&&!stopped)socket.send(JSON.stringify({type:'hello',version:CONTROL_VERSION,release:current,state_protocol:stateProtocol}));};
    socket.onmessage=({data})=>{
      if(stopped||peer!==socket)return;
      try{
        if(typeof data!=='string'||data.length>8192)throw Error('Oversized control event');
        const event=JSON.parse(data);
        if(event.version!==CONTROL_VERSION||!['hello','upgrade'].includes(event.type)||(!greeted&&event.type!=='hello')||!validRelease(event.release))throw Error('Invalid control hello');
        greeted=true;delay=250;cancel(deadline);
        if(event.release.id!==last){last=event.release.id;onRelease(event.release);}
      }catch{socket.close();}
    };
    socket.onclose=()=>{if(stopped||peer!==socket)return;cancel(deadline);peer=null;timer=schedule(connect,delay);delay=Math.min(10000,delay*2);};
    socket.onerror=()=>socket.close();
  }
  connect();
  return ()=>{stopped=true;cancel(timer);cancel(deadline);peer?.close();peer=null;};
}
