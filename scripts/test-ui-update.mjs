// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {digest,uiOnlyChange,prepareUiUpdate} from '../web/shared/ui-update.mjs';
globalThis.crypto??=webcrypto;
const asset=n=>({bytes:n,sha256:String(n).repeat(64)});
const baseline=()=>({ui_abi:'otrios-app-ui-v1',sync_protocol:'otrios-canvas-sync-v7',assets:{'apps/catalog.flex':asset(1),'web/app.js':asset(2),'native/armv7/dashboard-native':asset(3)},authority:{'authority/apps/home/ui.flex':asset(4),'authority/apps/home/behavior.wasm':asset(5),'authority/apps/other/ui.flex':asset(6)}});
test('App UI replacement requires the identical executable and other-app closure',()=>{
 const a=baseline(),b=structuredClone(a);b.assets['apps/catalog.flex']=asset(7);b.authority['authority/apps/home/ui.flex']=asset(8);assert(uiOnlyChange(a,b));
 for(const mutate of [m=>m.assets['web/app.js']=asset(9),m=>m.authority['authority/apps/home/behavior.wasm']=asset(9),m=>m.sync_protocol='future',m=>delete m.ui_abi,m=>m.assets['extra']=asset(1)]){const bad=structuredClone(b);mutate(bad);assert(!uiOnlyChange(a,bad));}
});
test('preparation binds the catalog and both manifests to content hashes before admission',async()=>{
 const bytes=new Uint8Array([1,2,3]),a=baseline(),b=baseline();b.assets['apps/catalog.flex']={bytes:bytes.length,sha256:await digest(bytes)};
 const encode=m=>new TextEncoder().encode(JSON.stringify(m)),ar=encode(a),br=encode(b),ai=await digest(ar),bi=await digest(br);const fetch=globalThis.fetch;
 globalThis.fetch=async url=>new Response(String(url).endsWith('catalog.flex')?bytes:String(url).includes(ai)?ar:br);
 try{const prepared=await prepareUiUpdate('https://demo.example',ai,bi);assert.deepEqual(prepared.bytes,bytes);
 globalThis.fetch=async url=>{assert(!String(url).includes(ai),'old manifest may already be pruned');return new Response(String(url).endsWith('catalog.flex')?bytes:br);};
 assert.deepEqual((await prepareUiUpdate('https://demo.example',ai,bi,{id:ai,manifest:a})).bytes,bytes);
 globalThis.fetch=async url=>new Response(String(url).endsWith('catalog.flex')?new Uint8Array([9,2,3]):String(url).includes(ai)?ar:br);
 await assert.rejects(()=>prepareUiUpdate('https://demo.example',ai,bi),/catalog digest/);
 globalThis.fetch=async()=>new Response(bytes);await assert.rejects(()=>prepareUiUpdate('https://demo.example',ai,bi),/release digest/);
 }finally{globalThis.fetch=fetch;}
});

test('guest replacement requires an advertised ABI and cannot replace host code or policy',async()=>{
 const {appOnlyChange}=await import('../web/shared/ui-update.mjs');
 const a=baseline(),b=structuredClone(a);a.behavior_abi=b.behavior_abi='otrios-app-behavior-v1';b.authority['authority/apps/home/behavior.wasm']=asset(7);b.authority['authority/apps/home/ui.flex']=asset(8);b.assets['apps/catalog.flex']=asset(9);
 assert(!uiOnlyChange(a,b));assert(appOnlyChange(a,b));
 const legacy=structuredClone(a);delete legacy.behavior_abi;assert(!appOnlyChange(legacy,b));
 b.assets['web/app.js']=asset(8);assert(!appOnlyChange(a,b));
});

test('nested screen UI changes retain the executable closure',()=>{
 const a=baseline();a.authority['authority/apps/fixture/detail/ui.flex']=asset(1);
 const b=structuredClone(a);b.authority['authority/apps/fixture/detail/ui.flex']=asset(2);
 assert(uiOnlyChange(a,b));
 b.authority['authority/apps/layout-lab/../ui.flex']=asset(3);
 assert(!uiOnlyChange(a,b));
});
