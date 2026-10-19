// SPDX-License-Identifier: Apache-2.0
// Build the opt-in merkle-probe host, generate --target nodejs bindings into a
// temporary directory, then pass that directory here. V8 Wasm, not Safari/GPU.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {resolve} from 'node:path';
import {performance} from 'node:perf_hooks';
const require=createRequire(import.meta.url);
const {MerkleProbe}=require(resolve(process.argv[2]||'target/merkle-probe','otrios_merkle.cjs'));
for(const leaves of [64,512,4096]) {
  const probe=new MerkleProbe(leaves), samples=[];
  let report;
  for(let i=0;i<1100;i++) {
    const start=performance.now();report=probe.step(i);const us=(performance.now()-start)*1000;
    if(i>=100)samples.push(us);
    assert.equal(report[0],5,'one leaf plus four ancestors, independent of populated size');
  }
  assert.equal(probe.root_hash().length,32);
  samples.sort((a,b)=>a-b);
  console.log(JSON.stringify({engine:process.version,leaves,nodes:report[3],full_wire_bytes:report[4],hashed_nodes:report[0],hashed_bytes:report[1],delta_bytes:report[2],edit_offer_receive_ack_us:{p50:samples[500],p95:samples[949],p99:samples[989]}}));
  probe.free();
}
console.log('100 warmup + 1000 measured edits; 1 KiB registers, four ancestors, 8-way keys; includes Wasm ABI and root/hash validation; excludes network, durable I/O and rendering');
