// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {digest,uiOnlyChange,prepareUiUpdate} from '../web/shared/ui-update.mjs';
globalThis.crypto??=webcrypto;
const asset=n=>({bytes:n,sha256:String(n).repeat(64)});
const baseline=()=>({ui_abi:'otrios-app-ui-v1',sync_protocol:'otrios-canvas-sync-v35',assets:{'apps/catalog.flex':asset(1),'web/app.js':asset(2),'native/armv7/dashboard-native':asset(3)},authority:{'authority/apps/home/ui.flex':asset(4),'authority/apps/home/behavior.wasm':asset(5),'authority/apps/other/ui.flex':asset(6)}});
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

test('compatible signed bundle library revisions rotate exact dependencies through the catalog path',async()=>{
 const {readFile}=await import('node:fs/promises');
 const {appOnlyChange}=await import('../web/shared/ui-update.mjs');
 const pair=JSON.parse(await readFile(new URL('./fixtures/app-update-classification.json',import.meta.url)));
 const a=pair.previous,b=pair.next;
 assert(!uiOnlyChange(a,b),'UI-only cannot authorize executable dependency replacement');
 assert(appOnlyChange(a,b));assert(appOnlyChange(b,a),'compatible signed rollback uses the same host path');
 const ui=structuredClone(a);ui.assets['apps/catalog.flex']=asset(7);ui.authority['authority/bundles/designer.otapp/UI/main.flex']=asset(8);
 assert(uiOnlyChange(a,ui),'ordinary package UI changes retain the exact executable closure');
 const dataA=structuredClone(a),dataB=structuredClone(b);
 delete dataA.component_behavior_abi;delete dataB.component_behavior_abi;
 for(const m of [dataA,dataB])for(const path of Object.keys(m.authority))if(path.endsWith('/Code/component.wasm'))delete m.authority[path];
 assert(uiOnlyChange(dataA,dataB),'data-only libraries do not require a component execution ABI');
});

test('bundle catalog replacement rejects changed contracts, schema, host policy and unknown private paths',async()=>{
 const {readFile}=await import('node:fs/promises');
 const {appOnlyChange}=await import('../web/shared/ui-update.mjs');
 const pair=JSON.parse(await readFile(new URL('./fixtures/app-update-classification.json',import.meta.url)));
 const root='authority/bundles/designer.otapp/';
 const cases=[
  ['state schema',m=>m.authority[root+'Schemas/state.flex']=asset(8)],
  ['app contract',m=>m.app_contracts['designer.otapp']='f'.repeat(64)],
  ['host executable',m=>m.assets['native/armv7/dashboard-native']=asset(8)],
  ['host policy',m=>m.host_session=2],
  ['state protocol',m=>m.sync_protocol='future-v99'],
  ['unknown package payload',m=>m.authority[root+'Host/runtime.so']=asset(8)],
  ['path traversal',m=>m.authority[root+'Dependencies/'+'c'.repeat(64)+'.otlib/../../policy.flex']=asset(8)],
  ['floating dependency',m=>m.authority[root+'Dependencies/latest.otlib/Code/component.wasm']=asset(8)],
  ['unknown library executable',m=>m.authority[root+'Dependencies/'+'c'.repeat(64)+'.otlib/Code/native.so']=asset(8)],
  ['non-app authority addition',m=>m.authority['authority/policy.flex']=asset(8)],
 ];
 for(const [label,mutate]of cases){const b=structuredClone(pair.next);mutate(b);assert(!appOnlyChange(pair.previous,b),label);}
 for(const field of ['component_behavior_abi','bundle_abi','app_contracts']){
  const a=structuredClone(pair.previous),b=structuredClone(pair.next);delete a[field];delete b[field];assert(!appOnlyChange(a,b),`missing ${field}`);
 }
 const malformedA=structuredClone(pair.previous),malformedB=structuredClone(pair.next);malformedA.app_contracts['designer.otapp']=malformedB.app_contracts['designer.otapp']=['f'.repeat(64)];assert(!appOnlyChange(malformedA,malformedB),'contract type must be a string');
 const a=structuredClone(pair.previous),b=structuredClone(pair.next);a.authority['authority/policy.flex']=b.authority['authority/policy.flex']=asset(8);assert(appOnlyChange(a,b));
 b.authority['authority/policy.flex']=asset(7);assert(!appOnlyChange(a,b),'non-app authority changed');delete b.authority['authority/policy.flex'];assert(!appOnlyChange(a,b),'non-app authority removed');
 const add=structuredClone(pair.next);add.app_contracts['new.otapp']='e'.repeat(64);add.authority['authority/bundles/new.otapp/Schemas/state.flex']=asset(1);assert(!appOnlyChange(pair.previous,add),'registry install is a distinct path');
});

test('preparation admits a compatible package catalog without choosing a full reload',async()=>{
 const {readFile}=await import('node:fs/promises');
 const {previous:a,next:b}=JSON.parse(await readFile(new URL('./fixtures/app-update-classification.json',import.meta.url)));
 const bytes=new Uint8Array([4,5,6]);b.assets['apps/catalog.flex']={bytes:bytes.length,sha256:await digest(bytes)};
 const raw=new TextEncoder().encode(JSON.stringify(b)),id=await digest(raw),fetch=globalThis.fetch;
 globalThis.fetch=async url=>new Response(String(url).endsWith('catalog.flex')?bytes:raw);
 try{const prepared=await prepareUiUpdate('https://demo.example','a'.repeat(64),id,{id:'a'.repeat(64),manifest:a});assert.deepEqual(prepared.bytes,bytes);assert.equal(prepared.manifest.id,id);}
 finally{globalThis.fetch=fetch;}
});
