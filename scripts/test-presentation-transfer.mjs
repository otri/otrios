// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {PresentationTransfer,PresentationLayers} from '../web/canvas/presentation-transfer.mjs';
const chunk=(version,dx=0,color=[1,1,1,1])=>({version:String(version),vertices:3,glyphVertices:4,draws:2,views:[0,3,dx,0,0,0,800,600,0,4,dx,0,0,0,800,600],paints:[...color,...color]});
const meta=(base,overlay=[],epoch='1')=>({base,overlay,epoch});
const read=()=>({vertices:new Float32Array(18),glyphVertices:new Float32Array(64),draws:new Uint32Array([0,0,3,1,0,4]),textStyles:new Float32Array([0,24])});
test('one changed chunk uploads alone while painter order, pan and color metadata retain neighbors',()=>{
  const transfer=new PresentationTransfer();transfer.update(meta([chunk(1),chunk(2)],[chunk(1)]),'mount');
  const first=transfer.take(read);assert.equal(first.upserts.length,3);transfer.ack(first.token);
  transfer.update(meta([chunk(1,15,[.1,.2,.3,.4]),chunk(2,20)],[chunk(1)]),'mount');
  const warm=transfer.take(()=>assert.fail('paint/pan copied geometry'));assert.equal(warm.upserts.length,0);assert.equal(warm.base[0].views[2],15);assert(Math.abs(warm.base[0].paints[3]-.4)<1e-7);transfer.ack(warm.token);
  transfer.update(meta([chunk(3),chunk(2)],[chunk(1)]),'mount');
  const changed=transfer.take(read);assert.deepEqual(changed.upserts.map(u=>[u.layer,u.version]),[['base','3']]);assert.deepEqual(changed.retire,[{layer:'base',version:'1'}]);assert.deepEqual(changed.base.map(d=>d.version),['3','2']);
});
test('font/composer epoch, mount and graphics reset resend dependencies and ignore stale acknowledgements',()=>{
  const transfer=new PresentationTransfer();transfer.update(meta([chunk(1)]),'mount');const first=transfer.take(read);
  transfer.update(meta([chunk(1)],[],'2'),'mount');transfer.ack(first.token);const font=transfer.take(read);assert.notEqual(font.key,first.key);assert.equal(font.upserts.length,1);
  transfer.update(meta([chunk(1)],[],'2'),'other-mount');assert.equal(transfer.take(read).upserts.length,1);
  transfer.reset();transfer.update(meta([chunk(1)],[],'2'),'other-mount');assert.equal(transfer.take(read).upserts.length,1);
  transfer.update(null,'mount');const empty=transfer.take(()=>assert.fail());assert(!empty.enabled);assert.equal(empty.key,null);transfer.ack(undefined);
});
test('admission rejects aggregate limits and mismatched resources before recording a partial frame',()=>{
  const transfer=new PresentationTransfer();
  for(const bad of [meta([chunk(1),chunk(1)]),meta([{...chunk(1),views:[]}]),meta([{...chunk(1),version:'18446744073709551616'}]),meta(Array.from({length:33},(_,i)=>chunk(i+1))),meta([{...chunk(1),vertices:65535},{...chunk(2),vertices:3}])]){
    transfer.reset();transfer.update(bad,'mount');assert.throws(()=>transfer.take(read));assert.equal(transfer.resources.size,0);
  }
  const large={...chunk(1),vertices:65535,glyphVertices:65536,draws:1,views:[0,3,0,0,0,0,800,600],paints:[1,1,1,1]};
  transfer.reset();transfer.update(meta([large],[large]),'mount');assert.throws(()=>transfer.take(()=>assert.fail('over-budget frame copied geometry')),/GPU budget/);
  assert.throws(()=>transfer.update(meta([],[],'not-an-epoch'),'mount'),/epoch/);
  transfer.reset();transfer.update(meta([chunk(1),chunk(2)]),'mount');let calls=0;
  assert.throws(()=>transfer.take(()=>++calls===1?read():{...read(),glyphVertices:new Float32Array()}),/snapshot/);assert.equal(transfer.resources.size,0);
  assert.equal(transfer.take(read).upserts.length,2);
});
test('renderer resources preserve ordered layers and privacy/fallback clears every retained object',()=>{
  const disposed=[],frames=[];const renderer={createLayer(){return {setFrame(frame){frames.push(frame);},setTextViews(view){this.view=view;},setPaints(paint){this.paint=paint;},dispose(){disposed.push(this);}};}};
  const transfer=new PresentationTransfer(),layers=new PresentationLayers(renderer);transfer.update(meta([chunk(1),chunk(2)],[chunk(1)]),'mount');let p=transfer.take(read);layers.accept(p);transfer.ack(p.token);
  const neighbor=layers.base[1],overlay=layers.overlay[0];assert.equal(frames.length,3);
  transfer.update(meta([chunk(3),chunk(2,17,[0,1,0,.5])],[chunk(1)]),'mount');p=transfer.take(read);layers.accept(p);
  assert.equal(frames.length,4);assert.equal(layers.base[1],neighbor);assert.equal(layers.overlay[0],overlay);assert.equal(neighbor.view[2],17);assert.equal(neighbor.paint[3],.5);assert.equal(disposed.length,1);assert.equal(layers.resources.size,3);
  assert.equal(layers.uploadedBytes,4*(18+64)*4);
  transfer.update(null,'mount');layers.accept(transfer.take(read));assert(!layers.enabled);assert.equal(layers.resources.size,0);assert.equal(disposed.length,4);
  assert.throws(()=>layers.accept({enabled:true,key:'missing',base:[{version:'7'}],overlay:[],retire:[],upserts:[]}),/unadmitted/);
});
