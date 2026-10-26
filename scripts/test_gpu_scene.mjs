// SPDX-License-Identifier: Apache-2.0
// Host ordering/lifetime tests. Actual shader pixels require browser/native probes.
import test from 'node:test';
import assert from 'node:assert/strict';
import {SceneRenderer,linearToSrgb,srgbToLinear} from '../web/shared/gpu-scene.mjs';
function device(){
  const calls=[];let id=0,framebuffer=null,viewport=[0,0,1280,800];
  const methods={
    getExtension:name=>name==='OES_texture_float'||name==='OES_standard_derivatives'?{}:null,
    getShaderPrecisionFormat:()=>({precision:23}),isContextLost:()=>false,
    getShaderParameter:()=>true,getProgramParameter:()=>true,
    getUniformLocation:(_p,name)=>name,getAttribLocation:(_p,name)=>['position','color','renderCoord','bandTransform','glyph','uv'].indexOf(name),
    getParameter:name=>name==='MAX_TEXTURE_SIZE'?4096:name==='VIEWPORT'?viewport:name==='FRAMEBUFFER_BINDING'?framebuffer:undefined,
    checkFramebufferStatus:()=> 'FRAMEBUFFER_COMPLETE',getError:()=> 'NO_ERROR',
    bindFramebuffer:(_target,value)=>{framebuffer=value;},viewport:(...value)=>{viewport=value;},
  };
  const gl=new Proxy({}, {get:(_target,name)=>{
    if(name in methods)return methods[name];
    if(name.startsWith('create'))return ()=>({id:++id});
    if(name.toUpperCase()===name)return name;
    return (...args)=>calls.push([name,...args]);
  }});
  return {gl,calls};
}
function packet(){return {curveWidth:1,curveHeight:1,curves:new Float32Array(4),bandWidth:1,bandHeight:1,bands:new Uint8Array(4)};}
test('portrait logical projection changes independently of the physical glyph atlas',()=>{
  const {gl}=device(),renderer=new SceneRenderer(gl);
  renderer.cache.set('old',{});
  assert.equal(renderer.setLogicalSize(800,1280),true);
  assert.equal(renderer.cache.size,0);
  assert.equal(renderer.logicalWidth,800);assert.equal(renderer.logicalHeight,1280);
  assert(Math.abs(renderer.transform[0]-2/800)<1e-8);
  assert(Math.abs(renderer.transform[4]+2/1280)<1e-8);
  assert.equal(renderer.setLogicalSize(800,1280),false);
  assert.throws(()=>renderer.setLogicalSize(0,1280));
});
function frame(phase=0,color=[1,1,1,1]){
  const glyphVertices=new Float32Array(64);
  for(const [i,x,y] of [[0,10+phase,10],[1,20+phase,10],[2,10+phase,20],[3,20+phase,20]])glyphVertices.set([x,y,i%2,Math.floor(i/2),...color,8,8,0,0,0,0,7,7],i*16);
  return {vertices:new Float32Array(36),glyphVertices,draws:new Uint32Array([0,0,3,1,0,4,0,3,3])};
}
test('cache misses use analytic coverage once; warm glyph tint preserves draw ordering',()=>{
  const {gl,calls}=device(),renderer=new SceneRenderer(gl),layer=renderer.createLayer();renderer.setFont(packet());
  const uploads=calls.filter(call=>call[0]==='texImage2D').length;
  layer.setFrame(frame());renderer.prepare([layer]);assert.equal(renderer.cacheStats.misses,1);assert.equal(renderer.cacheStats.hits,0);
  calls.length=0;renderer.beginFrame();layer.draw();renderer.endFrame();
  assert.deepEqual(calls.filter(call=>call[0]==='drawArrays').map(call=>call.slice(2)),[[0,3],[0,6],[3,3]]);
  layer.setFrame(frame(0,[0.4,0.2,0.7,0.5]));renderer.prepare([layer]);assert.equal(renderer.cacheStats.misses,1);assert.equal(renderer.cacheStats.hits,1);
  assert.equal(calls.filter(call=>call[0]==='texImage2D').length,0);assert.equal(uploads,3);
  assert.equal(renderer.cacheStats.bytes,4*1024*1024);
  layer.setFrame(frame(0.25));renderer.prepare([layer]);assert.equal(renderer.cacheStats.misses,1);
  for(let phase=0;phase<4;phase+=0.03125){layer.setFrame(frame(phase));renderer.prepare([layer]);}
  assert.equal(renderer.cacheStats.misses,1,'fractional positions reuse one prefiltered glyph');
});
test('quad indices match TL TR BL BR and invalid plans cannot publish GPU buffers',()=>{
  const {gl,calls}=device(),renderer=new SceneRenderer(gl),layer=renderer.createLayer();
  layer.setFrame(frame());const indices=calls.find(call=>call[0]==='bufferData'&&call[2] instanceof Uint16Array)[2];assert.deepEqual([...indices],[0,1,2,2,1,3]);
  calls.length=0;assert.throws(()=>layer.setFrame({...frame(),draws:new Uint32Array([1,1,4])}),/Drawing plan/);
  assert.equal(calls.filter(call=>call[0]==='bufferData').length,0);
  const invalid=frame();invalid.glyphVertices[0]=NaN;assert.throws(()=>layer.setFrame(invalid),/Nonfinite/);
  renderer.contextLost();assert.throws(()=>layer.draw(),/context lost/);
});
test('linear coverage is composited before the single sRGB encoding pass',()=>{
  assert.equal(Math.round(linearToSrgb(0.5)*255),188);
  for(const value of [0,0.01,0.04045,0.2,0.5,1])assert.ok(Math.abs(linearToSrgb(srgbToLinear(value))-value)<1e-6);
});
test('physical viewport changes rebuild glyph coverage at the new device scale',()=>{
  const {gl}=device(),renderer=new SceneRenderer(gl),layer=renderer.createLayer();renderer.setFont(packet());layer.setFrame(frame());renderer.prepare([layer]);
  assert.equal([...renderer.cache.values()][0].width,28);
  const canvas={width:1280,height:800,dataset:{},getBoundingClientRect:()=>({width:1280,height:800})};
  assert.equal(renderer.fitCanvas(canvas,2),true);assert.equal(canvas.width,2560);assert.equal(canvas.height,1600);assert.equal(canvas.dataset.presentationPixels,'2560x1600');assert.equal(renderer.cache.size,0);
  renderer.prepare([layer]);assert.equal([...renderer.cache.values()][0].width,48);assert.equal(renderer.cacheStats.misses,2);
  assert.equal(renderer.fitCanvas(canvas,2),false);
});

test('the byte lookup table exactly encodes all possible linear RGBA8 channels',()=>{
  const {gl,calls}=device();new SceneRenderer(gl,{linearLight:true});
  const upload=calls.find(call=>call[0]==='texImage2D'&&call[4]===256&&call[5]===1);
  const bytes=upload.at(-1);assert.equal(bytes.length,1024);
  for(let value=0;value<256;value++){assert.equal(bytes[value*4],Math.round(linearToSrgb(value/255)*255));assert.equal(bytes[value*4+3],255);}
  assert.equal(bytes[0],0);assert.equal(bytes[128*4],188);assert.equal(bytes[255*4],255);
});

test('direct default allocates only the glyph atlas and composites premultiplied colors',()=>{
  const {gl,calls}=device(),renderer=new SceneRenderer(gl);
  assert.equal(renderer.linearLight,false);assert.equal(renderer.linearTexture,undefined);assert.equal(renderer.encodeTexture,undefined);assert.equal(renderer.resolveProgram,undefined);
  const uploads=calls.filter(call=>call[0]==='texImage2D');assert.equal(uploads.length,1);assert.equal(uploads[0][4],1024);assert.equal(uploads[0][5],1024);
  const vertices=calls.filter(call=>call[0]==='shaderSource').map(call=>call[2]);assert.ok(vertices.some(source=>source.includes('tint=vec4(color.rgb*color.a,color.a)')));assert.ok(vertices.some(source=>source.includes('gl_FragColor=tint*texture2D(atlas,texcoord)')));
  calls.length=0;renderer.beginFrame();renderer.endFrame();assert.equal(calls.filter(call=>call[0]==='drawArrays').length,0);assert.ok(calls.filter(call=>call[0]==='blendFuncSeparate').every(call=>call[1]==='ONE'&&call[2]==='ONE_MINUS_SRC_ALPHA'));
  calls.length=0;renderer.resize(2560,1600);assert.equal(calls.filter(call=>call[0]==='texImage2D').length,0);
});

test('retained viewports change only uniforms and cull indexed glyph ranges',()=>{
  const {gl,calls}=device(),renderer=new SceneRenderer(gl),layer=renderer.createLayer();renderer.setFont(packet());
  const f=frame(),glyphs=new Float32Array(128);glyphs.set(f.glyphVertices);glyphs.set(f.glyphVertices,64);
  f.glyphVertices=glyphs;f.draws=new Uint32Array([0,0,3,1,0,8,0,3,3]);layer.setFrame(f);
  const views=new Float32Array(24);views.set([4,4,2.25,0,10,10,20,20],8);layer.setTextViews(views);renderer.prepare([layer]);
  calls.length=0;renderer.beginFrame();layer.draw();renderer.endFrame();
  assert(calls.some(c=>c[0]==='drawArrays'&&c[2]===6&&c[3]===6),'only the second cached glyph is drawn');
  assert(calls.some(c=>c[0]==='scissor'&&JSON.stringify(c.slice(1))==='[10,780,10,10]'));
  const misses=renderer.cacheStats.misses;calls.length=0;views[10]=10.5;views[9]=0;layer.setTextViews(views);renderer.prepare([layer]);layer.draw();
  assert.equal(renderer.cacheStats.misses,misses);assert.equal(calls.filter(c=>c[0]==='bufferData').length,0);
  assert.deepEqual(calls.filter(c=>c[0]==='drawArrays').map(c=>c.slice(2)),[[0,3],[3,3]],'a culled run sends no text draw');
  views[9]=100;assert.throws(()=>layer.setTextViews(views),/Culled text/);
});

test('clipped glyph edges reuse the complete prefiltered coverage',()=>{
  const {gl}=device(),renderer=new SceneRenderer(gl),layer=renderer.createLayer();renderer.setFont(packet());
  layer.setFrame(frame());renderer.prepare([layer]);const f=frame();
  for(const i of [0,2]){f.glyphVertices[i*16]=13;f.glyphVertices[i*16+2]=0.3;}
  layer.setFrame(f);renderer.prepare([layer]);assert.equal(renderer.cacheStats.misses,1);
});

test('stable retained quads correct UVs without multiplying long-pen cache sizes',()=>{
  const {gl,calls}=device(),renderer=new SceneRenderer(gl,{stableTextQuads:true}),layer=renderer.createLayer();renderer.setFont(packet());
  const f=frame(.25);f.draws=new Uint32Array([1,0,4]);layer.setFrame(f);layer.setTextViews(new Float32Array([0,4,0,0,0,0,1280,800]));renderer.prepare([layer]);
  const cached=calls.filter(c=>c[0]==='bufferData'&&c[2] instanceof Float32Array).at(-1)[2];
  assert.equal(cached[0],10.25);assert.equal(cached[8],20.25);assert.notEqual(cached[2],cached[10]);assert.equal(renderer.cacheStats.misses,1);
  const next=frame(.25);for(let i=0;i<4;i++)next.glyphVertices[i*16]+=10000+(i%2?.004:0);next.draws=f.draws;layer.setFrame(next);layer.setTextViews(new Float32Array([0,4,0,0,0,0,1280,800]));renderer.prepare([layer]);
  assert.equal(renderer.cacheStats.misses,1,'retained font size comes from the local Y span');
  const degenerate=frame();degenerate.draws=f.draws;for(const i of [1,3])degenerate.glyphVertices[i*16]=degenerate.glyphVertices[0];layer.setFrame(degenerate);layer.setTextViews(new Float32Array([0,4,0,0,0,0,1280,800]));calls.length=0;renderer.prepare([layer]);
  assert.equal(renderer.cacheStats.fallbacks,1,'a degenerate retained quad uses the original draw path');
  assert(calls.filter(c=>c[0]==='bufferData'&&c[2] instanceof Float32Array).every(c=>c[2].every(Number.isFinite)));
  const cropped=frame(.25);cropped.draws=f.draws;layer.setFrame(cropped);calls.length=0;renderer.prepare([layer]);
  const rawBounds=calls.filter(c=>c[0]==='bufferData'&&c[2] instanceof Float32Array).at(-1)[2];assert.equal(rawBounds[0],10.25,'CPU-clipped quads without a viewport do not expand');
});

test('declared text scales are validated before publication and stay independent of quad rounding',()=>{
  const {gl,calls}=device(),renderer=new SceneRenderer(gl,{width:4096,height:2560,stableTextQuads:true}),layer=renderer.createLayer();renderer.setLogicalSize(1280,800);renderer.setFont(packet());
  const f=frame();f.draws=new Uint32Array([1,0,4]);f.textStyles=new Float32Array([10]);layer.setFrame(f);layer.setTextViews(new Float32Array([0,4,0,0,0,0,1280,800]));renderer.prepare([layer]);assert.equal(renderer.cacheStats.misses,1);
  const changed=frame();changed.draws=f.draws;changed.textStyles=f.textStyles;for(const i of [2,3])changed.glyphVertices[i*16+1]+=.004;
  layer.setFrame(changed);layer.setTextViews(new Float32Array([0,4,0,0,0,0,1280,800]));renderer.prepare([layer]);assert.equal(renderer.cacheStats.misses,1,'the declared font scale controls coverage identity');
  const outside=frame();outside.draws=f.draws;outside.textStyles=f.textStyles;for(let i=0;i<4;i++)outside.glyphVertices[i*16+2]-=10;
  layer.setFrame(outside);layer.setTextViews(new Float32Array([0,4,0,0,0,0,1280,800]));renderer.prepare([layer]);assert.equal(renderer.cacheStats.fallbacks,1,'quads outside the clear sampling domain use analytic fallback');
  calls.length=0;
  assert.throws(()=>layer.setFrame({...f,textStyles:new Float32Array([NaN])}),/Nonfinite/);
  assert.throws(()=>layer.setFrame({...f,textStyles:new Float32Array([-1])}),/range/);
  assert.throws(()=>layer.setFrame({...f,textStyles:new Float32Array([1,2])}),/dimensions/);
  assert.throws(()=>layer.setFrame({...f,draws:new Uint32Array([1,0,4,1,0,4]),textStyles:new Float32Array([1,2])}),/Conflicting/);
  assert.equal(calls.filter(c=>c[0]==='bufferData').length,0,'invalid metadata cannot replace GPU buffers');
});
