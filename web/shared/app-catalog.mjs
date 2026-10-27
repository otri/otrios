// SPDX-License-Identifier: Apache-2.0
// The code replica verifies publisher signatures before exposing a release.
// Bind its admitted catalog to that immutable release's manifest and byte hashes.
import {boundedBody} from './artifact-cache.mjs';
const hash=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
export async function loadAppRelease(moduleUrl){
  const url=new URL(moduleUrl),release=url.pathname.match(/^\/releases\/([0-9a-f]{64})\/web\//)?.[1];
  const read=async(path,limit)=>{const response=await fetch(new URL(path,url),{credentials:'omit',redirect:'error'});if(!response.ok)throw Error('App catalog unavailable');return boundedBody(response,limit);};
  if(!release){
    if(!['127.0.0.1','localhost','[::1]'].includes(url.hostname))throw Error('An admitted app release is required');
    return {catalog:await read('/apps/catalog.flex',512*1024),manifest:null};
  }
  const manifestBytes=await read(`/releases/${release}/manifest.json`,65536);
  if(await hash(manifestBytes)!==release)throw Error('Release manifest digest rejected');
  const manifest=JSON.parse(new TextDecoder().decode(manifestBytes)),item=manifest.assets?.['apps/catalog.flex'];
  if(manifest.state_schema!=='otrios-canvas-merkle-state-v1'||manifest.sync_protocol!=='otrios-canvas-sync-v6'||!item||!Number.isSafeInteger(item.bytes)||item.bytes<1||item.bytes>512*1024||!/^[a-f0-9]{64}$/.test(item.sha256))throw Error('App catalog admission rejected');
  const bytes=await read(`/releases/${release}/apps/catalog.flex`,item.bytes);
  if(bytes.length!==item.bytes||await hash(bytes)!==item.sha256)throw Error('App catalog digest rejected');
  return {catalog:bytes,manifest:{id:release,manifest}};
}
export async function loadAppCatalog(moduleUrl){return (await loadAppRelease(moduleUrl)).catalog;}
