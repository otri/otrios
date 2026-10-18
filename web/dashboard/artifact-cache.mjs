// SPDX-License-Identifier: Apache-2.0
// Cache only immutable public payloads. Application documents never enter here.
export const CACHE='otrios-artifacts-v1',MAX_BYTES=512*1024*1024,MAX_ITEM=32*1024*1024;
export function eligible(request,origin){
  const u=new URL(request.url);
  return request.method==='GET'&&u.origin===origin&&!u.search&&!request.headers.has('range')&&!request.headers.has('authorization')&&(
    /^\/releases\/[a-f0-9]{64}\/web\/(?:[a-z0-9-]+\.(?:html|css|js|mjs)|pkg\/otrios_dashboard(?:_bg\.wasm|\.js)|fonts\/Roboto-Regular\.otq\.zst)$/.test(u.pathname)||
    /^\/objects\/[a-f0-9]{64}$/.test(u.pathname));
}
export function budget(estimate={},owned=0){
  if(!Number.isFinite(estimate.quota))return MAX_BYTES;
  return Math.max(0,Math.floor(Math.min(MAX_BYTES,estimate.quota/4,estimate.quota*.9-(estimate.usage||0)+owned)));
}
export function victims(entries,limit,extra=0,keep=''){
  let total=entries.reduce((n,e)=>n+e.bytes,0)+extra;const result=[];
  for(const e of entries.filter(e=>e.url!==keep).sort((a,b)=>a.used-b.used)){
    if(total<=limit)break;result.push(e.url);total-=e.bytes;
  }
  return {remove:result,fits:total<=limit};
}
export async function boundedBody(response,max=MAX_ITEM){
  if(Number(response.headers.get('content-length'))>max)throw Error('Artifact exceeds per-item cache limit');
  const reader=response.body.getReader(),chunks=[];let size=0;
  try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;if(size>max)throw Error('Artifact exceeds per-item cache limit');chunks.push(value);}}
  catch(e){await reader.cancel().catch(()=>{});throw e;}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}return bytes;
}
export class ArtifactCache {
  constructor({cache,index,estimate=async()=>({}),now=()=>Date.now(),lock=work=>work()}){Object.assign(this,{cache,index,estimate,now,lock});this.queue=Promise.resolve();}
  serial(work){const next=this.queue.then(()=>this.lock(work));this.queue=next.catch(()=>{});return next;}
  async trim(extra=0,keep=''){
    const entries=await this.index.all(),owned=entries.reduce((n,e)=>n+e.bytes,0);
    const plan=victims(entries,budget(await this.estimate(),owned),extra,keep);
    for(const url of plan.remove){await this.cache.delete(url);await this.index.delete(url);}return plan.fits;
  }
  match(url){return this.serial(async()=>{
    await this.trim();const hit=await this.cache.match(url);
    if(hit){const item=(await this.index.all()).find(e=>e.url===url);if(item)await this.index.put({...item,used:this.now()});else{await this.cache.delete(url);return;}}
    return hit;
  });}
  put(url,response,bytes){return this.serial(async()=>{
    if(bytes>MAX_ITEM)return false;
    // Delete a prior copy first so reservation cannot double-count it.
    await this.cache.delete(url);await this.index.delete(url);
    if(!await this.trim(bytes,url))return false;
    // Reserve metadata first: a terminated write may over-count, never under-count.
    await this.index.put({url,bytes,used:this.now()});
    try{await this.cache.put(url,response);return true;}
    catch(e){await this.cache.delete(url);await this.index.delete(url);await this.trim();return false;}
  });}
}
