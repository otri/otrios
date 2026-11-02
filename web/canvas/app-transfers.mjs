// SPDX-License-Identifier: Apache-2.0
// Trusted browser file/HTTP adapter. Authority decisions remain in the core/session.
const APP='application/vnd.otrios.app+zip',DATA='application/vnd.otrios.app-data+zip';
const ID=/^[a-f0-9]{32}$/,APP_ID=/^[a-z-]{1,64}$/;
export function transferPath(target){
  if(target?.type==='app'&&ID.test(target.space))return `/transfers/apps/${target.space}`;
  const d=target?.dataset;
  if(target?.type==='data'&&d&&ID.test(d.space)&&APP_ID.test(d.app)&&ID.test(d.collection)&&/^[a-f0-9]{64}$/.test(d.schema))return `/transfers/data/${d.space}/${d.app}/${d.collection}`;
  throw Error('The transfer destination is invalid.');
}
function responseError(status){return status===403?'Current Admin access is required.':status===409?'The dataset or review changed. Close and review current details.':status===429?'Finish or cancel the current transfer, then try again.':status===422?'The payload, publisher trust or app compatibility was rejected.':status===413?'The payload exceeds the transfer budget.':'The transfer is unavailable. Please try again.';}
export async function boundedBody(response,limit,signal){
  const length=response.headers.get('content-length');
  if(length!==null&&(!/^\d+$/.test(length)||Number(length)>limit))throw Error('The transfer exceeds its byte budget.');
  if(!response.body)throw Error('The transfer response was empty.');
  const reader=response.body.getReader(),chunks=[];let total=0,finished=false;
  try{
    for(;;){
      if(signal?.aborted)throw Error('Transfer cancelled.');
      const {done,value}=await reader.read();if(done){finished=true;break;}
      total+=value.byteLength;if(total>limit)throw Error('The transfer exceeds its byte budget.');
      chunks.push(value);
    }
    if(length!==null&&total!==Number(length))throw Error('The transfer was incomplete.');
    return chunks;
  }finally{if(!finished)await reader.cancel().catch(()=>{});reader.releaseLock();}
}
export class AppTransfers{
  constructor({origin,chooseFile,download,copy,reply,fetchImpl=globalThis.fetch}){
    this.origin=new URL(origin).origin;this.chooseFile=chooseFile;this.download=download;this.copy=copy;this.reply=reply;this.fetch=fetchImpl;this.active=new Map();this.generation=0;
  }
  reset(){this.generation++;for(const controller of this.active.values())controller.abort();this.active.clear();}
  async run(task,providedFile){
    const generation=this.generation,id=task?.id;
    if(!Number.isInteger(id)||id<1||id>0xffffffff)return;
    const send=value=>{if(generation===this.generation)this.reply(id,typeof value==='string'?value:JSON.stringify(value));};
    if(this.active.has(id)||this.active.size>=2){send({type:'failed',message:'The transfer host is busy.'});return;}
    const controller=new AbortController();this.active.set(id,controller);let timer,timedOut=false;
    try{
      const path=transferPath(task.target),kind=task.target.type,io=task.io;
      if(io?.type==='copy'){
        const url=new URL(io.text);
        if(io.text.length>512||url.origin!==this.origin||url.search||url.hash||!/^\/apps\/[a-z-]+\/[a-f0-9]{64}$/.test(url.pathname))throw Error('The app link is invalid.');
        try{await this.copy(io.text);}catch{throw Error('Copy is unavailable. Select the displayed URL instead.');}
        send({type:'copied'});return;
      }
      let body,media,method='POST',limit=kind==='app'?17*1024*1024:81*1024*1024;
      if(io?.type==='pick'){
        const file=providedFile||await this.chooseFile(kind,controller.signal);
        if(generation!==this.generation)return;
        if(!file){send({type:'cancelled'});return;}
        if(file.size===0||file.size>limit)throw Error('Choose a non-empty payload within the transfer budget.');
        body=file;media=kind==='app'?APP:DATA;
      }else if(io?.type==='url'&&kind==='app'){
        if(typeof io.url!=='string'||io.url.length>512)throw Error('The shared app URL is invalid.');
        body=io.url;media='text/plain; charset=utf-8';
      }else if(io?.type==='export'&&kind==='data'){method='GET';}
      else throw Error('This transfer action is unavailable in this browser.');
      timer=setTimeout(()=>{timedOut=true;controller.abort();},method==='GET'?120000:45000);
      const headers={'X-OTriOS-Request':'transfers'};if(media)headers['Content-Type']=media;
      const response=await this.fetch(new URL(path,this.origin),{method,body,headers,credentials:'same-origin',redirect:'error',cache:'no-store',signal:controller.signal});
      if(!response.ok){await response.body?.cancel();throw Error(responseError(response.status));}
      if(method==='GET'){
        if(response.headers.get('content-type')?.split(';')[0]!==DATA)throw Error('The server did not return a portable data payload.');
        const chunks=await boundedBody(response,limit,controller.signal);
        if(generation!==this.generation)return;
        const name=`${task.target.dataset.app}.otdata`;
        await this.download(new Blob(chunks,{type:DATA}),name);
        send({type:'exported',name});
      }else{
        const chunks=await boundedBody(response,16*1024,controller.signal);
        const text=await new Blob(chunks).text();
        // Preserve exact u64 review integers for serde in Wasm; don't round-trip
        // registry generations/revisions through JavaScript Number/JSON.parse.
        send(`{"type":"${kind}","review":${text}}`);
      }
    }catch(error){if(generation===this.generation)send({type:'failed',message:timedOut?'The transfer timed out. An unconfirmed upload will expire automatically.':String(error.message||'The transfer failed.')});}
    finally{clearTimeout(timer);this.active.delete(id);}
  }
}
