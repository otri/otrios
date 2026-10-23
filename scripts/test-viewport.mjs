// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {layoutViewport,pointerPosition,semanticBounds} from '../web/shared/viewport.mjs';
test('available orientation selects a supported logical viewport independently of DPR',()=>{
  assert.deepEqual(layoutViewport(1440,900),[1280,800]);
  assert.deepEqual(layoutViewport(768,1024),[800,1280]);
  assert.deepEqual(layoutViewport(1536,2048),[800,1280]);
  assert.throws(()=>layoutViewport(0,1024));
});
test('portrait pointer and accessibility bounds describe the same rendered rectangle',()=>{
  const extent=[800,1280],screen={left:20,top:10,width:400,height:640};
  assert.deepEqual(pointerPosition(220,330,screen,extent),[400,640]);
  assert.deepEqual(semanticBounds([200,320,400,640],extent),{left:'25%',top:'25%',width:'50%',height:'50%'});
  assert.deepEqual(pointerPosition(10,660,screen,extent),[-20,1300], 'captured pointers preserve positions outside the canvas');
  assert.deepEqual(pointerPosition(220,330,screen,[1280,800]),[640,400], 'legacy scene extent remains authoritative');
});
