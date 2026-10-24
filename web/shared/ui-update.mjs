// SPDX-License-Identifier: Apache-2.0
// A data-only update retains the exact executable/public asset closure.
import {boundedBody} from './artifact-cache.mjs';
export const digest=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
const canonical=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
export function uiOnlyChange(previous,next){
  if(previous.ui_abi!=='otrios-app-ui-v1'||next.ui_abi!==previous.ui_abi)return false;
  const metadata=m=>Object.fromEntries(Object.entries(m).filter(([k])=>!['assets','authority'].includes(k)));
  if(canonical(metadata(previous))!==canonical(metadata(next)))return false;
  for(const [section,allowed]of [['assets','apps/catalog.flex'],['authority',null]]){
    const a=previous[section],b=next[section];if(!a||!b||canonical(Object.keys(a).sort())!==canonical(Object.keys(b).sort()))return false;
    for(const name of Object.keys(a))if(!(name===allowed||(section==='authority'&&/^authority\/apps\/[a-z-]+\/ui\.flex$/.test(name)))&&canonical(a[name])!==canonical(b[name]))return false;
  }
  return true;
}
async function read(url,limit){const r=await fetch(url,{credentials:'omit',redirect:'error'});if(!r.ok)throw Error('UI update unavailable');return boundedBody(r,limit);}
export async function prepareUiUpdate(base,previous,next,admitted){
  const manifest=async id=>{if(!/^[a-f0-9]{64}$/.test(id))throw Error('Invalid release');const raw=await read(new URL(`/releases/${id}/manifest.json`,base),65536);if(await digest(raw)!==id)throw Error('UI release digest rejected');return JSON.parse(new TextDecoder().decode(raw));};
  const [a,b]=await Promise.all([admitted?.id===previous?admitted.manifest:manifest(previous),manifest(next)]);
  if(!uiOnlyChange(a,b))return null;
  const item=b.assets['apps/catalog.flex'];if(!item||!Number.isSafeInteger(item.bytes)||item.bytes<1||item.bytes>512*1024)throw Error('Invalid UI catalog budget');
  const bytes=await read(new URL(`/releases/${next}/apps/catalog.flex`,base),item.bytes);
  if(bytes.length!==item.bytes||await digest(bytes)!==item.sha256)throw Error('UI catalog digest rejected');
  return {bytes,hash:item.sha256,manifest:{id:next,manifest:b}};
}
