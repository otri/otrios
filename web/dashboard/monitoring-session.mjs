// SPDX-License-Identifier: Apache-2.0
// Private views live only in this session's memory, never in the demo cache.
export class MonitoringSession {
  constructor({fetcher=globalThis.fetch.bind(globalThis),socket=url=>new WebSocket(url),origin=globalThis.location?.href,onStatus=()=>{},onView=()=>{},schedule=setTimeout,cancel=clearTimeout}={}){
    Object.assign(this,{fetcher,socketFactory:socket,origin,onStatus,onView,schedule,cancel});
    this.authenticated=false;this.active=false;this.epoch=0;this.mutations=Promise.resolve();
  }
  status(extra={}){this.onStatus({authenticated:this.authenticated,...extra});}
  stop(){this.cancel(this.timer);this.timer=null;const socket=this.socket;this.socket=null;socket?.close();this.onView(new Uint8Array());}
  setActive(active){if(active===this.active)return;this.active=active;if(active)this.open();else this.stop();}
  async refresh(){
    const epoch=this.epoch;
    const response=await this.fetcher('/auth/session',{credentials:'same-origin',cache:'no-store'});
    if(epoch!==this.epoch)return;
    const authenticated=response.ok&&(await response.json()).monitoring===true;
    if(epoch!==this.epoch)return;this.authenticated=authenticated;
    if(!this.authenticated)this.stop();this.status();this.open();
  }
  mutate(method,body){
    const operation=()=>this.fetcher('/auth/session',{method,credentials:'same-origin',cache:'no-store',headers:{'Content-Type':'application/json','X-OTriOS-Request':'session'},...(body?{body:JSON.stringify(body)}:{})});
    const result=this.mutations.then(operation,operation);this.mutations=result.catch(()=>{});return result;
  }
  async login(username,password){
    const epoch=++this.epoch;this.authenticated=false;this.stop();this.status({pending:true});
    try{
      const response=await this.mutate('POST',{username,password});
      if(epoch!==this.epoch)return;
      if(!response.ok)throw Error(response.status===429?'Please wait before trying again.':'Sign-in failed. Check your details.');
      await this.refresh();
    }catch(error){if(epoch===this.epoch)this.status({error:error.message});}
  }
  async logout(){
    ++this.epoch;this.authenticated=false;this.stop();this.status({pending:true});
    // Serialize after a pending login so its late Set-Cookie is revoked too.
    try{const response=await this.mutate('DELETE');if(!response.ok)throw Error('Could not finish signing out. Please retry.');this.status();}
    catch(error){this.status({error:error.message});}
  }
  open(){
    if(!this.active||!this.authenticated||this.socket)return;
    const url=new URL('/monitoring/stream',this.origin);url.protocol=url.protocol==='https:'?'wss:':'ws:';
    const socket=this.socketFactory(url);this.socket=socket;socket.binaryType='arraybuffer';
    socket.onmessage=event=>{
      if(this.socket!==socket||!this.authenticated||!this.active)return;
      const bytes=new Uint8Array(event.data);
      if(bytes.length<5||String.fromCharCode(...bytes.subarray(0,5))!=='OTML1'){socket.close();return;}
      this.onView(bytes.slice(5));
    };
    socket.onclose=()=>{
      if(this.socket!==socket)return;this.socket=null;this.onView(new Uint8Array());
      const retry=()=>{if(!this.active||!this.authenticated)return;return this.refresh().catch(()=>{this.timer=this.schedule(retry,2000);});};
      this.timer=this.schedule(retry,500);
    };
    socket.onerror=()=>socket.close();
  }
}
