// SPDX-License-Identifier: Apache-2.0
// Cookie/session policy only. Canvas owns private mounts and their payload lifetime.
export class MonitoringSession {
  constructor({fetcher=globalThis.fetch.bind(globalThis),onStatus=()=>{}}={}){
    Object.assign(this,{fetcher,onStatus});
    this.authenticated=false;this.epoch=0;this.refreshVersion=0;this.mutations=Promise.resolve();
  }
  status(extra={}){this.onStatus({authenticated:this.authenticated,...extra});}
  async refresh(){
    const epoch=this.epoch,version=++this.refreshVersion;
    const response=await this.fetcher('/auth/session',{credentials:'same-origin',cache:'no-store'});
    if(epoch!==this.epoch||version!==this.refreshVersion)return;
    const authenticated=response.ok&&(await response.json()).monitoring===true;
    if(epoch!==this.epoch||version!==this.refreshVersion)return;
    this.authenticated=authenticated;this.status();
  }
  mutate(method,body){
    const operation=()=>this.fetcher('/auth/session',{method,credentials:'same-origin',cache:'no-store',headers:{'Content-Type':'application/json','X-OTriOS-Request':'session'},...(body?{body:JSON.stringify(body)}:{})});
    const result=this.mutations.then(operation,operation);this.mutations=result.catch(()=>{});return result;
  }
  async login(username,password){
    const epoch=++this.epoch;this.authenticated=false;this.status({pending:true});
    try{
      const response=await this.mutate('POST',{username,password});
      if(epoch!==this.epoch)return;
      if(!response.ok)throw Error(response.status===429?'Please wait before trying again.':'Sign-in failed. Check your details.');
      await this.refresh();
    }catch(error){if(epoch===this.epoch)this.status({error:error.message});}
  }
  async logout(){
    const epoch=++this.epoch;this.authenticated=false;this.status({pending:true});
    // Serialize after a pending login so its late Set-Cookie is revoked too.
    try{const response=await this.mutate('DELETE');if(!response.ok)throw Error('Could not finish signing out. Please retry.');if(epoch===this.epoch)this.status();}
    catch(error){if(epoch===this.epoch)this.status({error:error.message});}
  }
}
