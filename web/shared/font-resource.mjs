// SPDX-License-Identifier: Apache-2.0
// Immutable font bytes share the host artifact cache. Decode only after verification.
import {FONT_SHA256,FONT_PATH} from './font-reference.mjs';
import {EMOJI_SHA256,EMOJI_PATH} from './emoji-reference.mjs';
async function loadResource(moduleUrl,path,expected,limit,label){
  const response=await fetch(new URL(path,moduleUrl),{credentials:'same-origin'});
  if(!response.ok)throw Error(`${label} resource unavailable`);
  const reader=response.body.getReader(),chunks=[];let length=0;
  try{for(;;){const {done,value}=await reader.read();if(done)break;length+=value.byteLength;if(length>limit)throw Error(`${label} resource exceeds budget`);chunks.push(value);}}
  catch(error){await reader.cancel().catch(()=>{});throw error;}
  const bytes=new Uint8Array(length);let offset=0;for(const c of chunks){bytes.set(c,offset);offset+=c.length;}
  const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
  if(digest!==expected)throw Error(`${label} resource digest rejected`);
  return {bytes,hash:digest};
}
export function loadFontResource(moduleUrl){return loadResource(moduleUrl,FONT_PATH,FONT_SHA256,1024*1024,'Font');}
export function loadEmojiResource(moduleUrl){return loadResource(moduleUrl,EMOJI_PATH,EMOJI_SHA256,32*1024*1024,'Emoji');}
