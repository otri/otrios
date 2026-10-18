// SPDX-License-Identifier: Apache-2.0
// Immutable font bytes share the host artifact cache. Decode only after verification.
import {FONT_SHA256,FONT_PATH} from './font-reference.mjs';
export async function loadFontResource(moduleUrl){
  const response=await fetch(new URL(FONT_PATH,moduleUrl),{credentials:'same-origin'});
  if(!response.ok)throw Error('Font resource unavailable');
  const limit=1024*1024,reader=response.body.getReader(),chunks=[];let length=0;
  try{for(;;){const {done,value}=await reader.read();if(done)break;length+=value.byteLength;if(length>limit)throw Error('Font resource exceeds budget');chunks.push(value);}}
  catch(error){await reader.cancel().catch(()=>{});throw error;}
  const bytes=new Uint8Array(length);let offset=0;for(const c of chunks){bytes.set(c,offset);offset+=c.length;}
  const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
  if(digest!==FONT_SHA256)throw Error('Font resource digest rejected');
  return {bytes,hash:digest};
}
