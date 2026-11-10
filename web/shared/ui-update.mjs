// SPDX-License-Identifier: Apache-2.0
// Classify an admitted release for catalog replacement. Package signatures,
// feature admission and the whole candidate still validate before activation.
import {boundedBody} from './artifact-cache.mjs';
export const digest=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
const canonical=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
const record=value=>value!==null&&typeof value==='object'&&!Array.isArray(value);
const bundleRoot=/^authority\/bundles\/([a-z-]+\.otapp)\//;
const dataFiles=new Set(['Info.flex','UI/main.flex','Integrity/files.flex','Integrity/publisher.flex']);
const libraryDataFiles=new Set(['Info.flex','OIB/library.flex','Integrity/files.flex','Integrity/publisher.flex']);
function compatibleBundles(previous,next){
  if(previous.bundle_abi!=='otrios-app-bundle-v1'||next.bundle_abi!==previous.bundle_abi)return false;
  const roots=m=>Array.from(new Set(Object.keys(m.authority||{}).map(name=>bundleRoot.exec(name)?.[1]).filter(Boolean))).sort();
  const a=roots(previous),b=roots(next),contracts=previous.app_contracts;
  if(!a.length||canonical(a)!==canonical(b)||!record(contracts)||!record(next.app_contracts)||canonical(contracts)!==canonical(next.app_contracts)||canonical(Object.keys(contracts).sort())!==canonical(a))return false;
  if(a.some(root=>typeof contracts[root]!=='string'||!/^[a-f0-9]{64}$/.test(contracts[root])))return false;
  return a.every(root=>{
    const path=`authority/bundles/${root}/Schemas/state.flex`;
    return Object.hasOwn(previous.authority,path)&&Object.hasOwn(next.authority,path)&&canonical(previous.authority[path])===canonical(next.authority[path]);
  });
}
function packageChange(name,behavior,component){
  const root=bundleRoot.exec(name);if(!root)return false;
  const relative=name.slice(root[0].length);
  if(!relative.split('/').every(part=>/^[A-Za-z0-9_.-]+$/.test(part)&&part!=='.'&&part!=='..'))return false;
  if(dataFiles.has(relative)||relative.startsWith('Notices/'))return true;
  if(relative==='Code/behavior.wasm')return behavior;
  const dependency=/^Dependencies\/[a-f0-9]{64}\.otlib\/(.+)$/.exec(relative);if(!dependency)return false;
  return libraryDataFiles.has(dependency[1])||dependency[1].startsWith('Notices/')||(component&&dependency[1]==='Code/component.wasm');
}
function catalogChange(previous,next,behavior,component){
  if(previous.ui_abi!=='otrios-app-ui-v1'||next.ui_abi!==previous.ui_abi)return false;
  const metadata=m=>Object.fromEntries(Object.entries(m).filter(([k])=>!['assets','authority'].includes(k)));
  if(canonical(metadata(previous))!==canonical(metadata(next)))return false;
  const a=previous.assets,b=next.assets;
  if(!record(a)||!record(b)||!Object.keys(a).length||canonical(Object.keys(a).sort())!==canonical(Object.keys(b).sort())||Object.keys(a).some(name=>name!=='apps/catalog.flex'&&canonical(a[name])!==canonical(b[name])))return false;
  const old=previous.authority,current=next.authority;
  if(!record(old)||!record(current)||!Object.keys(old).length||!Object.keys(current).length)return false;
  const packages=compatibleBundles(previous,next);
  for(const name of new Set([...Object.keys(old),...Object.keys(current)])){
    if(canonical(old[name])===canonical(current[name]))continue;
    if(/^authority\/apps\/[a-z-]+(?:\/[a-z-]+)*\/ui\.flex$/.test(name)){
      if(!Object.hasOwn(old,name)||!Object.hasOwn(current,name))return false;
      continue;
    }
    if(behavior&&/^authority\/apps\/[a-z-]+(?:\/[a-z-]+)*\/behavior\.wasm$/.test(name)){
      if(!Object.hasOwn(old,name)||!Object.hasOwn(current,name))return false;
      continue;
    }
    if(!packages||!packageChange(name,behavior,component))return false;
  }
  return true;
}
// A default UI-only classification never permits executable guest replacement.
export function uiOnlyChange(previous,next,behavior=false){return catalogChange(previous,next,behavior,false);}
export function appOnlyChange(previous,next){
  const behavior=previous.behavior_abi==='otrios-app-behavior-v1'&&next.behavior_abi===previous.behavior_abi;
  const component=previous.component_behavior_abi==='otrios-component-behavior-v1'&&next.component_behavior_abi===previous.component_behavior_abi;
  return catalogChange(previous,next,behavior,component);
}
async function read(url,limit){const r=await fetch(url,{credentials:'omit',redirect:'error'});if(!r.ok)throw Error('UI update unavailable');return boundedBody(r,limit);}
export async function prepareUiUpdate(base,previous,next,admitted){
  const manifest=async id=>{if(!/^[a-f0-9]{64}$/.test(id))throw Error('Invalid release');const raw=await read(new URL(`/releases/${id}/manifest.json`,base),65536);if(await digest(raw)!==id)throw Error('UI release digest rejected');return JSON.parse(new TextDecoder().decode(raw));};
  const [a,b]=await Promise.all([admitted?.id===previous?admitted.manifest:manifest(previous),manifest(next)]);
  if(!appOnlyChange(a,b))return null;
  const item=b.assets['apps/catalog.flex'];if(!item||!Number.isSafeInteger(item.bytes)||item.bytes<1||item.bytes>512*1024)throw Error('Invalid UI catalog budget');
  const bytes=await read(new URL(`/releases/${next}/apps/catalog.flex`,base),item.bytes);
  if(bytes.length!==item.bytes||await digest(bytes)!==item.sha256)throw Error('UI catalog digest rejected');
  return {bytes,hash:item.sha256,manifest:{id:next,manifest:b}};
}
