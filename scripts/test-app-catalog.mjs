// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {loadAppRelease} from '../web/shared/app-catalog.mjs';
import {STATE_PROTOCOL} from '../web/shared/session.mjs';
const hash=async bytes=>Buffer.from(await webcrypto.subtle.digest('SHA-256',bytes)).toString('hex');
globalThis.crypto??=webcrypto;
async function release(catalog,protocol=STATE_PROTOCOL){
  const manifest=new TextEncoder().encode(JSON.stringify({state_schema:'otrios-canvas-merkle-state-v1',sync_protocol:protocol,assets:{'apps/catalog.flex':{bytes:catalog.length,sha256:await hash(catalog)}}}));
  return {manifest,id:await hash(manifest)};
}
test('admitted catalog bytes are bound to the immutable release manifest',async()=>{
  const bytes=new Uint8Array([4,8,15,16,23,42]),{manifest,id}=await release(bytes),original=globalThis.fetch;
  globalThis.fetch=async url=>new Response(String(url).endsWith('manifest.json')?manifest:bytes);
  try{const loaded=await loadAppRelease(`https://demo.example/releases/${id}/web/app.js`);assert.deepEqual(loaded.catalog,bytes);assert.equal(loaded.manifest.id,id);assert.equal(loaded.manifest.manifest.sync_protocol,STATE_PROTOCOL);}
  finally{globalThis.fetch=original;}
});
test('catalog corruption, manifest substitution and unsigned public startup are rejected',async()=>{
  const bytes=new Uint8Array([4,8,15,16,23,42]),{manifest,id}=await release(bytes),original=globalThis.fetch;
  try{
    globalThis.fetch=async url=>new Response(String(url).endsWith('manifest.json')?manifest:new Uint8Array([4,8,15,16,23,0]));
    await assert.rejects(()=>loadAppRelease(`https://demo.example/releases/${id}/web/app.js`),/catalog digest rejected/);
    globalThis.fetch=async()=>new Response(new Uint8Array([0]));
    await assert.rejects(()=>loadAppRelease(`https://demo.example/releases/${id}/web/app.js`),/manifest digest rejected/);
    await assert.rejects(()=>loadAppRelease('https://demo.example/app.js'),/admitted app release is required/);
  }finally{globalThis.fetch=original;}
});

test('release with a mismatched state protocol is rejected before catalog fetch',async()=>{
  const bytes=new Uint8Array([1,2,3]),{manifest,id}=await release(bytes,'incompatible-state'),original=globalThis.fetch;let calls=0;
  globalThis.fetch=async()=>{calls++;return new Response(manifest);};
  try{await assert.rejects(()=>loadAppRelease(`https://demo.example/releases/${id}/web/app.js`),/catalog admission rejected/);assert.equal(calls,1);}
  finally{globalThis.fetch=original;}
});
