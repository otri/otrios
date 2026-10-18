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
