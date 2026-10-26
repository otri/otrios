// SPDX-License-Identifier: Apache-2.0
// Shared WebGL1 host for retained geometry and analytic quadratic glyph quads.
import {slugVertex,slugFragment} from './slug-shaders.mjs';
const MAX_FLOATS=8*1024*1024,MAX_GLYPH_VERTICES=65536;
const DECODE_SRGB='vec3 decodeSrgb(vec3 c){vec3 lo=c/12.92;vec3 hi=pow((c+0.055)/1.055,vec3(2.4));return vec3(c.r<=0.04045?lo.r:hi.r,c.g<=0.04045?lo.g:hi.g,c.b<=0.04045?lo.b:hi.b);}';

export function srgbToLinear(value){return value<=0.04045?value/12.92:((value+0.055)/1.055)**2.4;}
export function linearToSrgb(value){return value<=0.0031308?12.92*value:1.055*value**(1/2.4)-0.055;}
function floats(value,stride,label){
  if(!(value instanceof Float32Array)||value.length%stride||value.length>MAX_FLOATS)throw Error(`Invalid ${label} buffer`);
  for(const number of value)if(!Number.isFinite(number))throw Error(`Nonfinite ${label} value`);
  return value;
}
function halfFloats(values){
  const scratch=new Float32Array(1),bits=new Uint32Array(scratch.buffer),result=new Uint16Array(values.length);
  for(let i=0;i<values.length;i++){
    if(Math.abs(values[i])>65504)throw Error('Font curve exceeds half-float range');
    scratch[0]=values[i];const b=bits[0],sign=(b>>>16)&0x8000,exponent=(b>>>23)&255,mantissa=b&0x7fffff;
    if(exponent<102){result[i]=sign;continue;}
    const shift=exponent<113?126-exponent:13,m=exponent<113?mantissa|0x800000:mantissa;
    let base=m>>>shift;const remainder=m&((1<<shift)-1),midpoint=1<<(shift-1);
    if(remainder>midpoint||(remainder===midpoint&&(base&1)))base++;
    result[i]=sign+(exponent<113?base:((exponent-112)<<10)+base);
  }
  return result;
}
function compile(gl,type,source){
  const shader=gl.createShader(type);if(!shader)throw Error('Unable to allocate a GPU shader');
  gl.shaderSource(shader,source);gl.compileShader(shader);
  if(!gl.getShaderParameter(shader,gl.COMPILE_STATUS)){const reason=gl.getShaderInfoLog(shader);gl.deleteShader(shader);throw Error(`GPU shader compilation failed: ${reason}`);}
  return shader;
}
function program(gl,vertex,fragment){
  const v=compile(gl,gl.VERTEX_SHADER,vertex);let f,p;
  try{f=compile(gl,gl.FRAGMENT_SHADER,fragment);p=gl.createProgram();if(!p)throw Error('Unable to allocate a GPU program');gl.attachShader(p,v);gl.attachShader(p,f);gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(`GPU program linking failed: ${gl.getProgramInfoLog(p)}`);return p;}
  catch(error){if(p)gl.deleteProgram(p);throw error;}
  finally{gl.deleteShader(v);if(f)gl.deleteShader(f);}
}
function required(gl,p,name){const value=gl.getUniformLocation(p,name);if(value===null)throw Error(`Missing GPU uniform ${name}`);return value;}
function attribute(gl,p,name){const value=gl.getAttribLocation(p,name);if(value<0)throw Error(`Missing GPU attribute ${name}`);return value;}
const ATLAS_SIZE=1024,MAX_CACHE_ENTRIES=4096;
function cacheGlyphExact(vertices,offset,scaleX=1,scaleY=1){
  let left=Infinity,top=Infinity,right=-Infinity,bottom=-Infinity;
  for(let i=0;i<4;i++){left=Math.min(left,vertices[offset+i*16]*scaleX);right=Math.max(right,vertices[offset+i*16]*scaleX);top=Math.min(top,vertices[offset+i*16+1]*scaleY);bottom=Math.max(bottom,vertices[offset+i*16+1]*scaleY);}
  const corners=new Set();for(let i=0;i<4;i++){const x=vertices[offset+i*16]*scaleX,y=vertices[offset+i*16+1]*scaleY;if((x!==left&&x!==right)||(y!==top&&y!==bottom))return null;corners.add(`${x},${y}`);}if(corners.size!==4)return null;
  const x=Math.floor(left)-1,y=Math.floor(top)-1,width=Math.ceil(right)-x+1,height=Math.ceil(bottom)-y+1;
  if(width>ATLAS_SIZE||height>ATLAS_SIZE||width<=0||height<=0)return null;
  const key=[];for(let i=0;i<4;i++){const v=offset+i*16;key.push(vertices[v]*scaleX-x,vertices[v+1]*scaleY-y,vertices[v+2],vertices[v+3],...vertices.subarray(v+8,v+16));}
  return {key:key.join(','),x,y,width,height,offset};
}
function cacheGlyph(vertices,offset,scaleX=1,scaleY=1,sampling=2){
  if(!sampling)return cacheGlyphExact(vertices,offset,scaleX,scaleY);
  const q=vertices.subarray(offset,offset+64),du=q[18]-q[2],dv=q[35]-q[3];
  for(let i=1;i<4;i++)for(let j=4;j<16;j++)if(q[i*16+j]!==q[j])return null;
  if(!Number.isFinite(du)||!Number.isFinite(dv)||Math.abs(du)<1e-8||Math.abs(dv)<1e-8||q[8]<=0||q[9]<=0)return null;
  const sx=Math.round(Math.abs((q[16]-q[0])*scaleX/du)*256)/256,sy=Math.round(Math.abs((q[33]-q[1])*scaleY/dv)*256)/256;
  if(sx<=0||sy<=0||q[17]!==q[1]||q[32]!==q[0])return null;
  const x0=-q[10]/q[8],y1=(q[15]+1-q[11])/q[9],pad=2*sampling;
  const width=Math.ceil((q[14]+1)/q[8]*sx*sampling+2*pad),height=Math.ceil((q[15]+1)/q[9]*sy*sampling+2*pad);
  if(!Number.isFinite(width)||!Number.isFinite(height)||width>ATLAS_SIZE||height>ATLAS_SIZE||width<=0||height<=0)return null;
  return {key:[sx,sy,...q.subarray(8,16)].join(','),width,height,offset,sx,sy,x0,y1,pad,sampling};
}
function appendRun(runs,kind,first,count,groups,view,source){const n=runs.length;if(n&&runs[n-3]===kind&&runs[n-2]+runs[n-1]===first&&groups.at(-2)===view&&groups.at(-1)+(kind===2?runs[n-1]/6*4:runs[n-1])===source)runs[n-1]+=count;else{runs.push(kind,first,count);groups.push(view,source);}}
export class SceneRenderer {
  constructor(gl,{width=1280,height=800,linearLight=false,atlasScale=2}={}){
    if(!Number.isFinite(width)||!Number.isFinite(height)||width<=0||height<=0)throw Error('Invalid presentation dimensions');
    const floatTexture=gl.getExtension('OES_texture_float'),halfTexture=floatTexture?null:gl.getExtension('OES_texture_half_float');
    if(!floatTexture&&!halfTexture)throw Error('Analytic font rendering requires floating point textures');
    this.curveTextureType=floatTexture?gl.FLOAT:halfTexture.HALF_FLOAT_OES;
    if(!gl.getExtension('OES_standard_derivatives'))throw Error('Analytic font rendering requires shader derivatives');
    if(!gl.getShaderPrecisionFormat(gl.FRAGMENT_SHADER,gl.HIGH_FLOAT)?.precision)throw Error('Analytic font rendering requires high precision fragment math');
    if(![0,2,3].includes(atlasScale))throw Error('Choose exact reference (0), 2x or 3x text sampling');this.atlasScale=atlasScale;
    this.linearLight=!!linearLight;this.gl=gl;this.layers=new Set();this.lost=false;this.font=null;this.activeAttributes=new Set();
    this.transform=new Float32Array([2/width,0,0,0,-2/height,0,-1,1,1]);
    const tintExpression=this.linearLight?'decodeSrgb(color.rgb)':'color.rgb',decode=this.linearLight?DECODE_SRGB:'';
    this.geometry=program(gl,'attribute vec2 position;attribute vec4 color;uniform mat3 transform;varying mediump vec4 tint;'+decode+'void main(){vec3 p=transform*vec3(position,1.0);gl_Position=vec4(p.xy,0.0,1.0);tint=vec4('+tintExpression+'*color.a,color.a);}','precision mediump float;varying mediump vec4 tint;void main(){gl_FragColor=tint;}');
    this.text=program(gl,slugVertex,slugFragment);
    this.geometryAttributes=[attribute(gl,this.geometry,'position'),attribute(gl,this.geometry,'color')];
    this.textAttributes=['position','renderCoord','color','bandTransform','glyph'].map(name=>attribute(gl,this.text,name));
    this.geometryTransform=required(gl,this.geometry,'transform');
    this.textUniforms=Object.fromEntries(['transform','curveTexture','bandTexture','curveSize','bandSize','linearTint','coverageScale'].map(name=>[name,required(gl,this.text,name)]));
    this.indices=gl.createBuffer();if(!this.indices)throw Error('Unable to allocate glyph indices');this.indexGlyphs=0;
    this.cacheProgram=program(gl,'attribute vec2 position;attribute vec2 uv;attribute vec4 color;uniform mat3 transform;varying highp vec2 texcoord;varying mediump vec4 tint;'+decode+'void main(){vec3 p=transform*vec3(position,1.0);gl_Position=vec4(p.xy,0.0,1.0);texcoord=uv;tint=vec4('+tintExpression+'*color.a,color.a);}','precision mediump float;uniform sampler2D atlas;varying highp vec2 texcoord;varying mediump vec4 tint;void main(){gl_FragColor=tint*texture2D(atlas,texcoord);}');
    this.cacheAttributes=['position','uv','color'].map(name=>attribute(gl,this.cacheProgram,name));this.cacheTransform=required(gl,this.cacheProgram,'transform');this.cacheSampler=required(gl,this.cacheProgram,'atlas');
    this.atlas=gl.createTexture();this.atlasFramebuffer=gl.createFramebuffer();this.scratch=gl.createBuffer();if(!this.atlas||!this.atlasFramebuffer||!this.scratch)throw Error('Unable to allocate the bounded glyph cache');
    gl.bindTexture(gl.TEXTURE_2D,this.atlas);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,atlasScale?gl.LINEAR:gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,atlasScale?gl.LINEAR:gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,ATLAS_SIZE,ATLAS_SIZE,0,gl.RGBA,gl.UNSIGNED_BYTE,null);gl.bindFramebuffer(gl.FRAMEBUFFER,this.atlasFramebuffer);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,this.atlas,0);if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw Error('Glyph cache framebuffer is unavailable');gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    this.width=width;this.height=height;this.logicalWidth=width;this.logicalHeight=height;this.scaleX=1;this.scaleY=1;
    if(this.linearLight){
    this.linearTexture=gl.createTexture();this.linearFramebuffer=gl.createFramebuffer();this.resolveBuffer=gl.createBuffer();if(!this.linearTexture||!this.linearFramebuffer||!this.resolveBuffer)throw Error('Unable to allocate the linear presentation target');
    gl.bindTexture(gl.TEXTURE_2D,this.linearTexture);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,width,height,0,gl.RGBA,gl.UNSIGNED_BYTE,null);gl.bindFramebuffer(gl.FRAMEBUFFER,this.linearFramebuffer);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,this.linearTexture,0);if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw Error('Linear presentation framebuffer is unavailable');gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    // The linear render target is RGBA8, so a 256-entry table maps every
    // possible stored channel exactly to its piecewise sRGB-encoded output byte.
    this.encodeTexture=gl.createTexture();if(!this.encodeTexture)throw Error('Unable to allocate the presentation encoding table');
    const encode=new Uint8Array(256*4);for(let i=0;i<256;i++){const value=Math.round(linearToSrgb(i/255)*255);encode.set([value,value,value,255],i*4);}
    gl.activeTexture(gl.TEXTURE4);gl.bindTexture(gl.TEXTURE_2D,this.encodeTexture);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,256,1,0,gl.RGBA,gl.UNSIGNED_BYTE,encode);
    this.resolveProgram=program(gl,'attribute vec2 position;varying highp vec2 uv;void main(){gl_Position=vec4(position,0.0,1.0);uv=position*0.5+0.5;}','precision highp float;uniform sampler2D scene;uniform sampler2D encodeTexture;varying highp vec2 uv;float encodeChannel(float c){return texture2D(encodeTexture,vec2((c*255.0+0.5)/256.0,0.5)).r;}void main(){vec4 c=texture2D(scene,uv);gl_FragColor=vec4(encodeChannel(c.r),encodeChannel(c.g),encodeChannel(c.b),c.a);}');this.resolveAttribute=attribute(gl,this.resolveProgram,'position');this.resolveSampler=required(gl,this.resolveProgram,'scene');this.encodeSampler=required(gl,this.resolveProgram,'encodeTexture');gl.bindBuffer(gl.ARRAY_BUFFER,this.resolveBuffer);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,3,-1,-1,3]),gl.STATIC_DRAW);
    }
    this.cache=new Map();this.cacheX=0;this.cacheY=0;this.cacheRow=0;this.cacheGeneration=1;this.cacheStats={hits:0,misses:0,fallbacks:0,resets:0,bytes:ATLAS_SIZE*ATLAS_SIZE*4};
    gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.ONE,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  }
  assertReady(){if(this.lost||this.gl.isContextLost())throw Error('Graphics context lost. Reload to reconnect.');}
  createLayer(){this.assertReady();const layer=new SceneLayer(this);this.layers.add(layer);return layer;}
  setFont(packet){
    this.assertReady();if(this.font)throw Error('A presentation context already owns its font resource');
    const gl=this.gl,limit=gl.getParameter(gl.MAX_TEXTURE_SIZE);
    const dimensions=(w,h)=>Number.isInteger(w)&&Number.isInteger(h)&&w>0&&h>0&&w<=limit&&h<=limit;
    if(!packet||!dimensions(packet.curveWidth,packet.curveHeight)||!dimensions(packet.bandWidth,packet.bandHeight))throw Error('Invalid font texture dimensions');
    floats(packet.curves,4,'font curves');
    if(packet.curves.length!==packet.curveWidth*packet.curveHeight*4||!(packet.bands instanceof Uint8Array)||packet.bands.length!==packet.bandWidth*packet.bandHeight*4)throw Error('Invalid font texture payload');
    const texture=(unit,width,height,type,data)=>{const t=gl.createTexture();if(!t)throw Error('Unable to allocate a font texture');gl.activeTexture(gl.TEXTURE0+unit);gl.bindTexture(gl.TEXTURE_2D,t);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,width,height,0,gl.RGBA,type,data);return t;};
    let curves,bands;
    try{curves=texture(0,packet.curveWidth,packet.curveHeight,this.curveTextureType,this.curveTextureType===gl.FLOAT?packet.curves:halfFloats(packet.curves));bands=texture(1,packet.bandWidth,packet.bandHeight,gl.UNSIGNED_BYTE,packet.bands);if(gl.getError()!==gl.NO_ERROR)throw Error('GPU rejected the analytic font resource');this.font={curves,bands,curveSize:[packet.curveWidth,packet.curveHeight],bandSize:[packet.bandWidth,packet.bandHeight]};}
    catch(error){if(curves)gl.deleteTexture(curves);if(bands)gl.deleteTexture(bands);throw error;}
  }
  useAttributes(values){const wanted=new Set(values);for(const old of this.activeAttributes)if(!wanted.has(old))this.gl.disableVertexAttribArray(old);for(const value of wanted)if(!this.activeAttributes.has(value))this.gl.enableVertexAttribArray(value);this.activeAttributes=wanted;}
  ensureIndices(glyphs){
    if(glyphs<=this.indexGlyphs)return;
    const indices=new Uint16Array(glyphs*6);for(let i=0;i<glyphs;i++){const v=i*4;indices.set([v,v+1,v+2,v+2,v+1,v+3],i*6);}
    this.gl.bindBuffer(this.gl.ELEMENT_ARRAY_BUFFER,this.indices);this.gl.bufferData(this.gl.ELEMENT_ARRAY_BUFFER,indices,this.gl.STATIC_DRAW);this.indexGlyphs=glyphs;
  }
  allocateGlyph(glyph){
    if(this.cache.size>=MAX_CACHE_ENTRIES)return null;
    if(this.cacheX+glyph.width>ATLAS_SIZE){this.cacheX=0;this.cacheY+=this.cacheRow;this.cacheRow=0;}
    if(this.cacheY+glyph.height>ATLAS_SIZE)return null;
    const tile={x:this.cacheX,y:this.cacheY,width:glyph.width,height:glyph.height};this.cacheX+=glyph.width;this.cacheRow=Math.max(this.cacheRow,glyph.height);return tile;
  }
  bindText(buffer,transform=this.transform,coverageScale=1){
    const gl=this.gl,u=this.textUniforms,f=this.font;if(!f)throw Error('The verified font resource has not arrived');
    gl.useProgram(this.text);gl.uniformMatrix3fv(u.transform,false,transform);gl.uniform1f(u.linearTint,this.linearLight?1:0);gl.uniform1f(u.coverageScale,coverageScale);gl.uniform1i(u.curveTexture,0);gl.uniform1i(u.bandTexture,1);gl.uniform2fv(u.curveSize,f.curveSize);gl.uniform2fv(u.bandSize,f.bandSize);gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,f.curves);gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,f.bands);this.useAttributes(this.textAttributes);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);for(const [j,size,offset] of [[0,2,0],[1,2,8],[2,4,16],[3,4,32],[4,4,48]])gl.vertexAttribPointer(this.textAttributes[j],size,gl.FLOAT,false,64,offset);gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,this.indices);
  }
  rasterGlyph(layer,glyph,tile){
    const gl=this.gl,vertices=layer.glyphVertices.slice(glyph.offset,glyph.offset+64);for(let i=0;i<4;i++){
      vertices.fill(1,i*16+4,i*16+8);
      if(glyph.sampling){const x=(i%2)*tile.width,y=Math.floor(i/2)*tile.height;vertices[i*16]=tile.x+x;vertices[i*16+1]=tile.y+y;vertices[i*16+2]=glyph.x0+(x-glyph.pad)/(glyph.sx*glyph.sampling);vertices[i*16+3]=glyph.y1-(y-glyph.pad)/(glyph.sy*glyph.sampling);}
    }
    gl.bindBuffer(gl.ARRAY_BUFFER,this.scratch);gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STREAM_DRAW);
    const transform=glyph.sampling?new Float32Array([2/ATLAS_SIZE,0,0,0,-2/ATLAS_SIZE,0,-1,1,1]):new Float32Array([2*this.scaleX/ATLAS_SIZE,0,0,0,-2*this.scaleY/ATLAS_SIZE,0,2*(tile.x-glyph.x)/ATLAS_SIZE-1,1-2*(tile.y-glyph.y)/ATLAS_SIZE,1]);
    this.bindText(this.scratch,transform,glyph.sampling||1);gl.scissor(tile.x,ATLAS_SIZE-tile.y-tile.height,tile.width,tile.height);gl.clear(gl.COLOR_BUFFER_BIT);gl.disable(gl.BLEND);gl.drawElements(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,0);gl.enable(gl.BLEND);
  }
  prepare(layers){
    this.assertReady();if(layers.every(layer=>layer.preparedGeneration===this.cacheGeneration))return;
    const gl=this.gl,requests=[];for(const layer of layers){for(let offset=0;offset<layer.glyphVertices.length;offset+=64){const glyph=this.atlasScale===0&&layer.textViews.length?null:cacheGlyph(layer.glyphVertices,offset,this.scaleX,this.scaleY,this.atlasScale);if(glyph)requests.push(glyph);}}
    // Predict shelf use before invalidating any UVs. Reset at most once, before drawing.
    let x=this.cacheX,y=this.cacheY,row=this.cacheRow,full=false;const unseen=new Set();for(const glyph of requests){if(this.cache.has(glyph.key)||unseen.has(glyph.key))continue;unseen.add(glyph.key);if(this.cache.size+unseen.size>MAX_CACHE_ENTRIES){full=true;break;}if(x+glyph.width>ATLAS_SIZE){x=0;y+=row;row=0;}if(y+glyph.height>ATLAS_SIZE){full=true;break;}x+=glyph.width;row=Math.max(row,glyph.height);}
    if(full){this.cache.clear();this.cacheX=0;this.cacheY=0;this.cacheRow=0;this.cacheGeneration++;this.cacheStats.resets++;}
    let bound=false;const previousViewport=gl.getParameter(gl.VIEWPORT),previousFramebuffer=gl.getParameter(gl.FRAMEBUFFER_BINDING);
    try{for(const layer of layers){const runs=[],cached=[],groups=[];
      for(let i=0;i<layer.draws.length;i+=3){const kind=layer.draws[i],first=layer.draws[i+1],count=layer.draws[i+2];if(kind===0){appendRun(runs,kind,first,count,groups,i/3,first);continue;}
        for(let vertex=first;vertex<first+count;vertex+=4){const glyph=this.atlasScale===0&&layer.textViews.length?null:cacheGlyph(layer.glyphVertices,vertex*16,this.scaleX,this.scaleY,this.atlasScale);let tile=glyph&&this.cache.get(glyph.key);
          if(tile)this.cacheStats.hits++;
          else if(glyph){tile=this.allocateGlyph(glyph);if(tile){if(!bound){bound=true;gl.bindFramebuffer(gl.FRAMEBUFFER,this.atlasFramebuffer);gl.viewport(0,0,ATLAS_SIZE,ATLAS_SIZE);gl.enable(gl.SCISSOR_TEST);gl.clearColor(0,0,0,0);}this.rasterGlyph(layer,glyph,tile);this.cache.set(glyph.key,tile);this.cacheStats.misses++;}}
          if(!tile){appendRun(runs,1,vertex,4,groups,i/3,vertex);this.cacheStats.fallbacks++;continue;}
          const color=layer.glyphVertices.subarray(vertex*16+4,vertex*16+8),start=cached.length/8;
          const points=glyph.sampling?[0,1,3,2].map(i=>{const q=vertex*16+i*16;return [layer.glyphVertices[q]*this.scaleX,layer.glyphVertices[q+1]*this.scaleY,(tile.x+glyph.pad+(layer.glyphVertices[q+2]-glyph.x0)*glyph.sx*glyph.sampling)/ATLAS_SIZE,1-(tile.y+glyph.pad+(glyph.y1-layer.glyphVertices[q+3])*glyph.sy*glyph.sampling)/ATLAS_SIZE];}):[[glyph.x,glyph.y,tile.x/ATLAS_SIZE,1-tile.y/ATLAS_SIZE],[glyph.x+tile.width,glyph.y,(tile.x+tile.width)/ATLAS_SIZE,1-tile.y/ATLAS_SIZE],[glyph.x+tile.width,glyph.y+tile.height,(tile.x+tile.width)/ATLAS_SIZE,1-(tile.y+tile.height)/ATLAS_SIZE],[glyph.x,glyph.y+tile.height,tile.x/ATLAS_SIZE,1-(tile.y+tile.height)/ATLAS_SIZE]];
          for(const corner of [0,1,2,0,2,3]){const [x,y,u,v]=points[corner];cached.push(x/this.scaleX,y/this.scaleY,u,v,...color);}appendRun(runs,2,start,6,groups,i/3,vertex);
        }
      }
      gl.bindBuffer(gl.ARRAY_BUFFER,layer.cached);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(cached),gl.STATIC_DRAW);layer.preparedDraws=new Uint32Array(runs);layer.preparedGroups=groups;layer.preparedGeneration=this.cacheGeneration;
    }}finally{if(bound){gl.disable(gl.SCISSOR_TEST);gl.bindFramebuffer(gl.FRAMEBUFFER,previousFramebuffer);gl.viewport(...previousViewport);gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.ONE,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);}}
  }
  setLogicalSize(width,height){
    this.assertReady();
    if(!Number.isFinite(width)||!Number.isFinite(height)||width<1||height<1||width>8192||height>8192)throw Error('Invalid logical presentation size');
    if(this.logicalWidth===width&&this.logicalHeight===height)return false;
    this.logicalWidth=width;this.logicalHeight=height;this.scaleX=this.width/width;this.scaleY=this.height/height;
    this.transform.set([2/width,0,0,0,-2/height,0,-1,1,1]);
    this.cache.clear();this.cacheX=0;this.cacheY=0;this.cacheRow=0;this.cacheGeneration++;this.cacheStats.resets++;return true;
  }
  resize(width,height){
    this.assertReady();const gl=this.gl,limit=gl.getParameter(gl.MAX_TEXTURE_SIZE);
    if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||width>limit||height>limit)throw Error('Invalid physical presentation size');
    if(this.width===width&&this.height===height)return false;
    if(this.linearLight){gl.activeTexture(gl.TEXTURE3);gl.bindTexture(gl.TEXTURE_2D,this.linearTexture);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,width,height,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
    if(gl.getError()!==gl.NO_ERROR)throw Error('Unable to resize the linear presentation target');}
    this.width=width;this.height=height;this.scaleX=width/this.logicalWidth;this.scaleY=height/this.logicalHeight;
    this.cache.clear();this.cacheX=0;this.cacheY=0;this.cacheRow=0;this.cacheGeneration++;this.cacheStats.resets++;return true;
  }
  fitCanvas(canvas,dpr=globalThis.devicePixelRatio||1){
    const bounds=canvas.getBoundingClientRect(),limit=Math.min(4096,this.gl.getParameter(this.gl.MAX_TEXTURE_SIZE));
    if(!bounds.width||!bounds.height)return false;
    const ratio=Math.min(Number.isFinite(dpr)&&dpr>0?dpr:1,limit/bounds.width,limit/bounds.height),width=Math.max(1,Math.round(bounds.width*ratio)),height=Math.max(1,Math.round(bounds.height*ratio));
    if(canvas.width!==width)canvas.width=width;if(canvas.height!==height)canvas.height=height;
    canvas.dataset.presentationPixels=`${width}x${height}`;canvas.dataset.presentationDpr=String(ratio);
    return this.resize(width,height);
  }
  beginFrame(background=[16/255,28/255,41/255,1]){
    this.assertReady();const gl=this.gl;gl.bindFramebuffer(gl.FRAMEBUFFER,this.linearLight?this.linearFramebuffer:null);gl.viewport(0,0,this.width,this.height);const color=this.linearLight?background.slice(0,3).map(srgbToLinear):background;gl.clearColor(color[0]*background[3],color[1]*background[3],color[2]*background[3],background[3]);gl.clear(gl.COLOR_BUFFER_BIT);gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.ONE,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  }
  endFrame(){
    this.assertReady();if(!this.linearLight)return;const gl=this.gl;gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.viewport(0,0,this.width,this.height);gl.disable(gl.BLEND);gl.useProgram(this.resolveProgram);gl.uniform1i(this.resolveSampler,3);gl.uniform1i(this.encodeSampler,4);gl.activeTexture(gl.TEXTURE4);gl.bindTexture(gl.TEXTURE_2D,this.encodeTexture);gl.activeTexture(gl.TEXTURE3);gl.bindTexture(gl.TEXTURE_2D,this.linearTexture);this.useAttributes([this.resolveAttribute]);gl.bindBuffer(gl.ARRAY_BUFFER,this.resolveBuffer);gl.vertexAttribPointer(this.resolveAttribute,2,gl.FLOAT,false,8,0);gl.drawArrays(gl.TRIANGLES,0,3);gl.enable(gl.BLEND);
  }
  probe(layers){
    this.assertReady();this.prepare(layers);const gl=this.gl,count=this.width*this.height*4;
    const cachedLinear=new Uint8Array(count),cachedEncoded=new Uint8Array(count),scratch=new Uint8Array(count);
    const read=buffer=>gl.readPixels(0,0,this.width,this.height,gl.RGBA,gl.UNSIGNED_BYTE,buffer);
    const compare=(a,b)=>{let maxDifference=0,changedPixels=0,aboveFivePixels=0,totalDifference=0;for(let i=0;i<a.length;i+=4){let changed=false,aboveFive=false;for(let j=0;j<3;j++){const difference=Math.abs(a[i+j]-b[i+j]);maxDifference=Math.max(maxDifference,difference);totalDifference+=difference;changed||=difference!==0;aboveFive||=difference>5;}changedPixels+=Number(changed);aboveFivePixels+=Number(aboveFive);}return {maxDifference,changedPixels,aboveFivePixels,meanDifference:totalDifference/(a.length/4*3)};};
    const generations=layers.map(layer=>layer.preparedGeneration);let gammaLayer;
    try{
      this.beginFrame();for(const layer of layers)layer.draw();read(cachedLinear);this.endFrame();read(cachedEncoded);
      for(const layer of layers)layer.preparedGeneration=0;
      this.beginFrame();for(const layer of layers)layer.draw();read(scratch);const linear=compare(cachedLinear,scratch);this.endFrame();read(scratch);const encoded=compare(cachedEncoded,scratch);
      for(let i=0;i<layers.length;i++)layers[i].preparedGeneration=generations[i];
      gammaLayer=this.createLayer();const vertices=[];for(const [x,y] of [[0,0],[this.logicalWidth,0],[0,this.logicalHeight],[0,this.logicalHeight],[this.logicalWidth,0],[this.logicalWidth,this.logicalHeight]])vertices.push(x,y,1,1,1,0.5);
      gammaLayer.setFrame({vertices:new Float32Array(vertices)});this.beginFrame([0,0,0,1]);gammaLayer.draw();this.endFrame();const gammaPixel=new Uint8Array(4);gl.readPixels(Math.floor(this.width/2),Math.floor(this.height/2),1,1,gl.RGBA,gl.UNSIGNED_BYTE,gammaPixel);
      const expected=this.linearLight?188:128,gamma={expected,actual:[...gammaPixel],passed:[...gammaPixel.subarray(0,3)].every(value=>Math.abs(value-expected)<=2)};
      if(gl.getError()!==gl.NO_ERROR)throw Error('GPU font probe failed');
      return {mode:this.linearLight?'linear-light-srgb':'authored-srgb',alpha:'premultiplied',pixels:this.width*this.height,glyphs:layers.reduce((n,layer)=>n+layer.glyphVertices.length/64,0),linear,encoded,gamma,cache:{...this.cacheStats},passed:linear.maxDifference<=2&&gamma.passed};
    }finally{
      for(let i=0;i<layers.length;i++)layers[i].preparedGeneration=generations[i];gammaLayer?.dispose();this.beginFrame();for(const layer of layers)layer.draw();this.endFrame();
    }
  }
  contextLost(){this.lost=true;this.font=null;this.activeAttributes.clear();for(const layer of this.layers)layer.invalid=true;}
  dispose(){
    const gl=this.gl;for(const layer of [...this.layers])layer.dispose();
    if(this.font){gl.deleteTexture(this.font.curves);gl.deleteTexture(this.font.bands);}gl.deleteBuffer(this.indices);gl.deleteBuffer(this.scratch);gl.deleteTexture(this.atlas);gl.deleteFramebuffer(this.atlasFramebuffer);if(this.linearLight){gl.deleteTexture(this.encodeTexture);gl.deleteTexture(this.linearTexture);gl.deleteFramebuffer(this.linearFramebuffer);gl.deleteBuffer(this.resolveBuffer);gl.deleteProgram(this.resolveProgram);}gl.deleteProgram(this.cacheProgram);gl.deleteProgram(this.geometry);gl.deleteProgram(this.text);this.contextLost();
  }
}
class SceneLayer {
  constructor(owner){this.owner=owner;const gl=owner.gl;this.geometry=gl.createBuffer();this.text=gl.createBuffer();this.cached=gl.createBuffer();this.glyphVertices=new Float32Array();this.textViews=new Float32Array();this.preparedGroups=[];this.preparedGeneration=0;this.preparedDraws=null;if(!this.geometry||!this.text||!this.cached)throw Error('Unable to allocate presentation buffers');this.draws=new Uint32Array();this.invalid=false;}
  setFrame({vertices,glyphVertices=new Float32Array(),draws}){
    const owner=this.owner;owner.assertReady();if(this.invalid)throw Error('Presentation layer is unavailable');
    floats(vertices,6,'geometry');floats(glyphVertices,16,'glyph vertices');
    if(glyphVertices.length/16>MAX_GLYPH_VERTICES||glyphVertices.length%64)throw Error('Invalid glyph quad count');
    if(draws===undefined){if(glyphVertices.length)throw Error('Text requires an ordered drawing plan');draws=vertices.length?new Uint32Array([0,0,vertices.length/6]):new Uint32Array();}
    if(!(draws instanceof Uint32Array)||draws.length%3||draws.length>65536)throw Error('Invalid drawing plan');
    for(let i=0;i<draws.length;i+=3){const kind=draws[i],first=draws[i+1],count=draws[i+2],stride=kind===0?3:4,limit=kind===0?vertices.length/6:glyphVertices.length/16;if(kind>1||first%stride||count%stride||first+count>limit)throw Error('Drawing plan exceeds its presentation buffer');}
    const gl=owner.gl;gl.bindBuffer(gl.ARRAY_BUFFER,this.geometry);gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STATIC_DRAW);gl.bindBuffer(gl.ARRAY_BUFFER,this.text);gl.bufferData(gl.ARRAY_BUFFER,glyphVertices,gl.STATIC_DRAW);owner.ensureIndices(glyphVertices.length/64);this.draws=draws;this.textViews=new Float32Array();this.glyphVertices=glyphVertices;this.preparedGeneration=0;this.preparedDraws=null;
  }
  setTextViews(views){
    floats(views,8,'text viewport');
    if(views.length&&views.length!==this.draws.length/3*8)throw Error('Text viewport dimensions');
    for(let i=0;i<this.draws.length/3&&views.length;i++)if(this.draws[i*3]===1){const v=views.subarray(i*8,i*8+8),start=this.draws[i*3+1],end=start+this.draws[i*3+2];if(!Number.isInteger(v[0])||!Number.isInteger(v[1])||v[0]%4||v[1]%4||v[0]<start||v[1]<0||v[0]>end||v[1]>end-v[0])throw Error('Culled text range');}
    if(this.owner.atlasScale===0&&!!views.length!==!!this.textViews.length)this.preparedGeneration=0;
    this.textViews=views;
  }
  draw(){
    const owner=this.owner;owner.assertReady();if(this.invalid)throw Error('Presentation layer is unavailable');const gl=owner.gl;
    let active=-1;
    const draws=this.preparedGeneration===owner.cacheGeneration?this.preparedDraws:this.draws;
    for(let i=0;i<draws.length;i+=3){const kind=draws[i];let first=draws[i+1],count=draws[i+2];if(!count)continue;
      let view;
      if(kind!==0&&this.textViews.length){const prepared=this.preparedGeneration===owner.cacheGeneration,group=prepared?this.preparedGroups[i/3*2]:i/3,source=prepared?this.preparedGroups[i/3*2+1]:first;view=this.textViews.subarray(group*8,group*8+8);const ratio=kind===2?1.5:1,end=Math.min(source+count/ratio,view[0]+view[1]),start=Math.max(source,view[0]);if(end<=start||view[6]<=view[4]||view[7]<=view[5])continue;first+=(start-source)*ratio;count=(end-start)*ratio;}

      if(active!==kind){
        active=kind;
        if(kind===0){gl.useProgram(owner.geometry);gl.uniformMatrix3fv(owner.geometryTransform,false,owner.transform);owner.useAttributes(owner.geometryAttributes);gl.bindBuffer(gl.ARRAY_BUFFER,this.geometry);gl.vertexAttribPointer(owner.geometryAttributes[0],2,gl.FLOAT,false,24,0);gl.vertexAttribPointer(owner.geometryAttributes[1],4,gl.FLOAT,false,24,8);}
        else if(kind===1)owner.bindText(this.text);
        else{gl.useProgram(owner.cacheProgram);gl.uniformMatrix3fv(owner.cacheTransform,false,owner.transform);gl.uniform1i(owner.cacheSampler,2);gl.activeTexture(gl.TEXTURE2);gl.bindTexture(gl.TEXTURE_2D,owner.atlas);owner.useAttributes(owner.cacheAttributes);gl.bindBuffer(gl.ARRAY_BUFFER,this.cached);gl.vertexAttribPointer(owner.cacheAttributes[0],2,gl.FLOAT,false,32,0);gl.vertexAttribPointer(owner.cacheAttributes[1],2,gl.FLOAT,false,32,8);gl.vertexAttribPointer(owner.cacheAttributes[2],4,gl.FLOAT,false,32,16);}
      }
      if(kind!==0){
        const transform=owner.transform.slice();
        if(view){transform[6]+=owner.transform[0]*view[2]+owner.transform[3]*view[3];transform[7]+=owner.transform[1]*view[2]+owner.transform[4]*view[3];const edge=(v,max)=>Math.ceil(Math.max(0,Math.min(max,v))-.5),x0=edge(view[4]*owner.scaleX,owner.width),x1=edge(view[6]*owner.scaleX,owner.width),y0=edge(owner.height-view[7]*owner.scaleY,owner.height),y1=edge(owner.height-view[5]*owner.scaleY,owner.height);if(x1<=x0||y1<=y0)continue;gl.enable(gl.SCISSOR_TEST);gl.scissor(x0,y0,x1-x0,y1-y0);}
        gl.uniformMatrix3fv(kind===1?owner.textUniforms.transform:owner.cacheTransform,false,transform);
      }
      if(kind!==1)gl.drawArrays(gl.TRIANGLES,first,count);else gl.drawElements(gl.TRIANGLES,count/4*6,gl.UNSIGNED_SHORT,first/4*12);
      if(view)gl.disable(gl.SCISSOR_TEST);
    }
  }
  dispose(){if(this.invalid)return;this.owner.gl.deleteBuffer(this.geometry);this.owner.gl.deleteBuffer(this.text);this.owner.gl.deleteBuffer(this.cached);this.owner.layers.delete(this);this.invalid=true;this.draws=new Uint32Array();}
}
