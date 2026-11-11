// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {layoutViewport,chatViewport,pointerPosition,semanticBounds,viewportRequestNeeded} from '../web/shared/viewport.mjs';
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

test('chat preserves full width and UI scale as the visual keyboard changes height',()=>{
  assert.deepEqual(chatViewport(390,844),[534,1155]);
  assert.deepEqual(chatViewport(390,420),[534,575]);
  assert.deepEqual(chatViewport(390,300),[534,410]);
  assert.deepEqual(chatViewport(320,568),[534,947]);
  assert.deepEqual(chatViewport(599,900),[534,802]);
  assert.deepEqual(chatViewport(600,900),[800,1200]);
  assert.deepEqual(chatViewport(844,390),[800,369]);
  assert.deepEqual(chatViewport(1280,800),[1280,800]);
  assert.deepEqual(chatViewport(800,1280),[800,1280]);
  assert.throws(()=>chatViewport(NaN,420));
});

const responsive={kind:'responsive',minimum:[800,480]};
test('signed responsive viewport preserves actual desktop size, aspect and readable minima',()=>{
  assert.deepEqual(layoutViewport(1920,1080,responsive),[1920,1080]);
  assert.deepEqual(layoutViewport(1600,900,responsive),[1600,900]);
  assert.deepEqual(layoutViewport(1280,800,responsive),[1280,800]);
  assert.deepEqual(layoutViewport(640,480,responsive),[800,600]);
  assert.deepEqual(layoutViewport(800,400,responsive),[960,480]);
  const wide=layoutViewport(10000,2000,responsive);
  assert(Math.abs(wide[0]/wide[1]-5)<1e-9);assert(wide.every(n=>n<=8192));assert(wide[1]>=480);
  assert.throws(()=>layoutViewport(10000,1,responsive));
  assert.throws(()=>layoutViewport(1920,1080,{kind:'responsive',minimum:[0,480]}));
  assert.throws(()=>layoutViewport(1920,1080,{kind:'unknown'}));
  assert.deepEqual(layoutViewport(1920,1080),[1280,800],'unannotated applications preserve their original Fit policy');
});
test('responsive pointer and semantic bounds use the same published logical dimensions',()=>{
  const extent=layoutViewport(1920,1080,responsive),bounds={left:10,top:20,width:1920,height:1080};
  assert.deepEqual(pointerPosition(970,560,bounds,extent),[960,540]);
  assert.deepEqual(semanticBounds([480,270,960,540],extent),{left:'25%',top:'25%',width:'50%',height:'50%'});
});
test('policy and mount entry trigger sizing even at identical current dimensions',()=>{
  const extent=[1280,800];
  assert(viewportRequestNeeded(extent,extent,'fit:epoch1','responsive:epoch2'));
  assert(viewportRequestNeeded(extent,extent,'responsive:epoch2','responsive:epoch3'));
  assert(!viewportRequestNeeded(extent,extent,'responsive:epoch3','responsive:epoch3'));
  assert(viewportRequestNeeded(extent,[1920,1080],'responsive:epoch3','responsive:epoch3'));
});
