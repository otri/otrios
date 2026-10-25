// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';

test('modern Wasm host exports Canvas without the dashboard or motion adapters',async()=>{
  const root=fileURLToPath(new URL('../',import.meta.url));
  const target=resolve(root,process.env.CARGO_TARGET_DIR||'target');
  const bytes=await readFile(resolve(target,'wasm32-unknown-unknown/release/otrios_web_host.wasm'));
  const module=new WebAssembly.Module(bytes);
  const names=WebAssembly.Module.exports(module).map(e=>e.name);
  assert(names.some(n=>n.startsWith('canvasapp_')),'Canvas API must be present');
  assert(!names.some(n=>n.startsWith('dashboard_')),'legacy Dashboard API must be absent');
  assert(!names.some(n=>n.startsWith('motionpresenter_')),'bundled motion app adapter must be absent');
});

await test('a cloned compiled module initializes independent presentation and app memories',async()=>{
  const bytes=await readFile(new URL('../web/canvas/pkg/otrios_dashboard_bg.wasm',import.meta.url));
  const compiled=await WebAssembly.compile(bytes);
  const presentation=await import(new URL('../web/canvas/pkg/otrios_dashboard.js?presentation-test',import.meta.url));
  const worker=await import(new URL('../web/canvas/pkg/otrios_dashboard.js?worker-test',import.meta.url));
  const a=await presentation.default({module_or_path:compiled});
  const b=await worker.default({module_or_path:structuredClone(compiled)});
  assert.notEqual(a.memory,b.memory);
  const workerBytes=b.memory.buffer.byteLength;
  a.memory.grow(1);
  assert.equal(b.memory.buffer.byteLength,workerBytes);
  const presenter=new presentation.CanvasPresentation(),app=new worker.CanvasApp();
  presenter.free();app.free();
});
