// SPDX-License-Identifier: Apache-2.0
// Presentation only: state and executable behavior belong to the worker.
const canvas=document.querySelector('#view'),layer=document.querySelector('#controls'),status=document.querySelector('#status');
const worker=new Worker(new URL('./state-worker.js',import.meta.url),{type:'module'});
let epoch=0,vertices=new Float32Array(),dirty=true;
function fatal(e){const box=document.querySelector('#error');box.hidden=false;box.textContent=String(e);}
try{
 const gl=canvas.getContext('webgl',{alpha:false,antialias:false});if(!gl)throw Error('WebGL is required');
 function shader(type,source){const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;}
 const program=gl.createProgram();gl.attachShader(program,shader(gl.VERTEX_SHADER,'attribute vec2 pos;attribute vec4 color;varying lowp vec4 tint;void main(){gl_Position=vec4(pos.x/640.0-1.0,1.0-pos.y/400.0,0.,1.);tint=color;}'));gl.attachShader(program,shader(gl.FRAGMENT_SHADER,'varying lowp vec4 tint;void main(){gl_FragColor=tint;}'));gl.linkProgram(program);if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(program));gl.useProgram(program);
 const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);const pos=gl.getAttribLocation(program,'pos'),color=gl.getAttribLocation(program,'color');gl.enableVertexAttribArray(pos);gl.enableVertexAttribArray(color);gl.vertexAttribPointer(pos,2,gl.FLOAT,false,24,0);gl.vertexAttribPointer(color,4,gl.FLOAT,false,24,8);gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
 worker.onmessage=({data:m})=>{if(m.type==='error'){fatal(m.message);return;}if(m.type==='status'){status.textContent=m.value;return;}if(m.type==='online'){status.textContent=m.value?'Connected':'Reconnecting · unconfirmed edits are volatile';return;}if(m.type!=='frame')return;
  epoch=m.epoch;vertices=m.vertices;dirty=true;canvas.setAttribute('aria-label',m.text);document.title=`${m.title} · OTriOS`;document.querySelector('#scope').textContent=`UI ${m.ui}   ·   State ${m.state}`;
  layer.replaceChildren();for(const c of m.controls){const b=document.createElement('button');b.setAttribute('aria-label',c.label);b.type='button';b.dataset.node=c.id;const [x,y,w,h]=c.bounds;Object.assign(b.style,{left:`${x/12.8}%`,top:`${y/8}%`,width:`${w/12.8}%`,height:`${h/8}%`});const generation=m.epoch;b.onclick=()=>worker.postMessage({type:'activate',epoch:generation,id:c.id});layer.append(b);}
 };
 worker.onerror=e=>fatal(e.message);
 const pointers=new Set();function send(e,phase){const r=canvas.getBoundingClientRect();worker.postMessage({type:'pointer',epoch,args:[phase,e.pointerId,(e.clientX-r.left)*1280/r.width,(e.clientY-r.top)*800/r.height]});}
 canvas.onpointerdown=e=>{e.preventDefault();pointers.add(e.pointerId);canvas.setPointerCapture(e.pointerId);send(e,0);};canvas.onpointermove=e=>{if(pointers.has(e.pointerId))send(e,1);};canvas.onpointerup=e=>{send(e,2);pointers.delete(e.pointerId);};canvas.onpointercancel=e=>{send(e,3);pointers.delete(e.pointerId);};window.onblur=()=>{for(const id of pointers){worker.postMessage({type:'pointer',epoch,args:[3,id,0,0]});}pointers.clear();};
 document.querySelector('#reload').onclick=()=>worker.postMessage({type:'reload'});
 function frame(){if(dirty){gl.viewport(0,0,1280,800);gl.clearColor(16/255,28/255,41/255,1);gl.clear(gl.COLOR_BUFFER_BIT);gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STATIC_DRAW);gl.drawArrays(gl.TRIANGLES,0,vertices.length/6);dirty=false;}requestAnimationFrame(frame);}requestAnimationFrame(frame);
}catch(e){fatal(e);}
