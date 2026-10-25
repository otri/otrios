// SPDX-License-Identifier: Apache-2.0
// Direct Node acceptance: instantiate the actual generated host twice from one module.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
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
console.log('PASS compiled module cloning, separate memories and presentation/app construction');
