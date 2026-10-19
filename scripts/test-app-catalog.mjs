// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {loadAppCatalog} from '../web/shared/app-catalog.mjs';
const hash=async bytes=>Buffer.from(await webcrypto.subtle.digest('SHA-256',bytes)).toString('hex');
globalThis.crypto??=webcrypto;
async function release(catalog){
  const manifest=new TextEncoder().encode(JSON.stringify({state_schema:'otrios-canvas-merkle-state-v1',sync_protocol:'otrios-canvas-sync-v2',assets:{'apps/catalog.flex':{bytes:catalog.length,sha256:await hash(catalog)}}}));
  return {manifest,id:await hash(manifest)};
}
test('admitted catalog bytes are bound to the immutable release manifest',async()=>{
  const bytes=new Uint8Array([4,8,15,16,23,42]),{manifest,id}=await release(bytes),original=globalThis.fetch;
  globalThis.fetch=async url=>new Response(String(url).endsWith('manifest.json')?manifest:bytes);
  try{assert.deepEqual(await loadAppCatalog(`https://demo.example/releases/${id}/web/app.js`),bytes);}
  finally{globalThis.fetch=original;}
});
test('catalog corruption, manifest substitution and unsigned public startup are rejected',async()=>{
  const bytes=new Uint8Array([4,8,15,16,23,42]),{manifest,id}=await release(bytes),original=globalThis.fetch;
  try{
    globalThis.fetch=async url=>new Response(String(url).endsWith('manifest.json')?manifest:new Uint8Array([4,8,15,16,23,0]));
    await assert.rejects(()=>loadAppCatalog(`https://demo.example/releases/${id}/web/app.js`),/catalog digest rejected/);
    globalThis.fetch=async()=>new Response(new Uint8Array([0]));
    await assert.rejects(()=>loadAppCatalog(`https://demo.example/releases/${id}/web/app.js`),/manifest digest rejected/);
    await assert.rejects(()=>loadAppCatalog('https://demo.example/app.js'),/admitted app release is required/);
  }finally{globalThis.fetch=original;}
});
