// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {DocumentTransfer,DocumentLayers} from '../web/canvas/document-transfer.mjs';
const draw=(version,glyphs=1024)=>({version:String(version),glyphs,view:[0,glyphs*4,0,0,0,0,1200,800]});
const meta=(draws,epoch='1')=>({draws,epoch,em:24,loading:false});
const read=(id)=>({glyphVertices:new Float32Array(1024*64),vertices:new Float32Array()});
test('inline geometry and glyphs share retained resource admission, placement and clipping',()=>{
  const transfer=new DocumentTransfer(),d={...draw(1,1),geometry:3},payload={glyphVertices:new Float32Array(64),vertices:new Float32Array(18)};
  transfer.update(meta([d]),'chat');const packet=transfer.take(()=>payload);
  assert.equal(packet.upserts[0].vertices,payload.vertices);
  let frame,views,disposed=false;
  const layers=new DocumentLayers({createLayer(){return {setFrame(f){frame=f;},setTextViews(v){views=v;},dispose(){disposed=true;}};}});
  layers.accept(packet);
  assert.deepEqual([...frame.draws],[0,0,3,1,0,4]);
  assert.deepEqual([...views.slice(2,8)],[...views.slice(10,16)]);
  transfer.ack(packet.token);d.view[3]=17.25;d.view[5]=10;transfer.update(meta([d]),'chat');
  const warm=transfer.take(()=>assert.fail('scroll copied inline or text geometry'));layers.accept(warm);
  assert.equal(views[3],17.25);assert.equal(views[11],17.25);
  assert.equal(layers.stats().raw_uploaded_bytes,64*4+18*4);
  assert.equal(layers.stats().resource_uploads,1);
  assert.equal(layers.stats().budgeted_geometry_bytes,512+18*4+24);
  layers.clear();assert(disposed);
  assert.equal(layers.stats().budgeted_geometry_bytes,0);assert.equal(layers.stats().resource_evictions,1);
  transfer.reset();transfer.update(meta([{...d,geometry:5463}]),'chat');assert.throws(()=>transfer.take(()=>payload),/chunk size/);
});
test('coalesced viewports keep acknowledged resource dependencies and upload only incoming chunks',()=>{
  const transfer=new DocumentTransfer();
  transfer.update(meta([draw(1),draw(2),draw(3)]),'mount');
  const first=transfer.take(read);assert.equal(first.upserts.length,2);assert(first.loading);
  // A newer viewport arrives while the first payload is in flight. Its shared
  // chunk must survive ACK; the superseded frame has no independent upload list.
  transfer.update(meta([draw(2),draw(3),draw(4)]),'mount');
  transfer.ack(first.token);
  const next=transfer.take(read);assert.deepEqual(next.upserts.map(u=>u.version),['3','4']);
  assert.equal(next.draws.length,3);assert(!next.loading);
  transfer.ack(next.token);
  const warm=transfer.take(()=>assert.fail('warm scroll copied geometry'));
  assert.equal(warm.upserts.length,0);assert.equal(warm.draws.length,3);
});
test('bounded GPU residency retires offscreen versions and ignores stale mount acknowledgments',()=>{
  const transfer=new DocumentTransfer({limit:2*(1024*512+12)});
  transfer.update(meta([draw(1),draw(2)]),'old');let packet=transfer.take(read);transfer.ack(packet.token);
  transfer.update(meta([draw(2),draw(3)]),'old');packet=transfer.take(read);
  assert.deepEqual(packet.retire,['1']);assert.deepEqual(packet.upserts.map(u=>u.version),['3']);
  transfer.update(meta([draw(1)]),'new');transfer.ack(packet.token);
  const replay=transfer.take(read);assert.equal(replay.upserts.length,1);assert.notEqual(replay.key,packet.key);
  transfer.reset();transfer.update(meta([draw(1)]),'new');
  assert.equal(transfer.take(read).upserts.length,1,'context replay resends current resources');
});
test('display admission rejects absent dependencies and privacy clearing disposes every resource',()=>{
  const disposed=[],renderer={createLayer(){return {setFrame(){},setTextViews(){},dispose(){disposed.push(this);}};}};
  const layers=new DocumentLayers(renderer);
  assert.throws(()=>layers.accept({key:'x',draws:[draw(1)],upserts:[],retire:[]}),/unadmitted/);
  layers.accept({key:'x',em:24,draws:[draw(1)],upserts:[{version:'1',...read()}],retire:[],loading:false});
  assert.equal(layers.draws.length,1);layers.clear();assert.equal(layers.layers.size,0);assert.equal(disposed.length,1);
});
