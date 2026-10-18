// SPDX-License-Identifier: Apache-2.0
// Presentation only: state and executable behavior belong to the worker.
import {SceneRenderer} from './gpu-scene.mjs';
import {loadFontResource} from './font-resource.mjs';
const canvas=document.querySelector('#view'),layer=document.querySelector('#controls'),status=document.querySelector('#status');
const worker=new Worker(new URL('./state-worker.js',import.meta.url),{type:'module'});
let epoch=0,dirty=true,fontProbed=false;const query=new URL(location.href).searchParams,fontProbeRequested=['1','direct'].includes(query.get('font-probe'));
function fatal(e){const box=document.querySelector('#error');box.hidden=false;box.textContent=String(e);}
try{
 const gl=canvas.getContext('webgl',{alpha:false,antialias:false});if(!gl)throw Error('WebGL is required');
 const renderer=new SceneRenderer(gl,{linearLight:query.get('linear-light')==='1'||query.get('font-probe')==='1'}),scene=renderer.createLayer();
 function resize(){try{dirty=renderer.fitCanvas(canvas)||dirty;}catch(error){fatal(error);}}
 resize();new ResizeObserver(resize).observe(canvas);window.addEventListener('resize',resize);
 worker.onmessage=({data:m})=>{try{if(m.type==='font'){renderer.setFont(m.font);canvas.dataset.fontRenderer='analytic-quadratic-cached';canvas.dataset.gamma=renderer.linearLight?'linear-light-srgb':'authored-srgb';canvas.dataset.alpha='premultiplied';dirty=true;return;}if(m.type==='error'){fatal(m.message);return;}if(m.type==='status'){status.textContent=m.value;return;}if(m.type==='online'){status.textContent=m.value?'Connected':'Reconnecting · unconfirmed edits are volatile';return;}if(m.type!=='frame')return;
  epoch=m.epoch;scene.setFrame(m);dirty=true;canvas.setAttribute('aria-label',m.text);document.title=`${m.title} · OTriOS`;document.querySelector('#scope').textContent=`UI ${m.ui}   ·   State ${m.state}`;
  layer.replaceChildren();for(const c of m.controls){const b=document.createElement('button');b.setAttribute('aria-label',c.label);b.type='button';b.dataset.node=c.id;const [x,y,w,h]=c.bounds;Object.assign(b.style,{left:`${x/12.8}%`,top:`${y/8}%`,width:`${w/12.8}%`,height:`${h/8}%`});const generation=m.epoch;b.onclick=()=>worker.postMessage({type:'activate',epoch:generation,id:c.id});layer.append(b);}
 }catch(error){fatal(error);}};
 worker.onerror=e=>fatal(e.message);
 const pointers=new Set();function send(e,phase){const r=canvas.getBoundingClientRect();worker.postMessage({type:'pointer',epoch,args:[phase,e.pointerId,(e.clientX-r.left)*1280/r.width,(e.clientY-r.top)*800/r.height]});}
 canvas.onpointerdown=e=>{e.preventDefault();pointers.add(e.pointerId);canvas.setPointerCapture(e.pointerId);send(e,0);};canvas.onpointermove=e=>{if(pointers.has(e.pointerId))send(e,1);};canvas.onpointerup=e=>{send(e,2);pointers.delete(e.pointerId);};canvas.onpointercancel=e=>{send(e,3);pointers.delete(e.pointerId);};window.onblur=()=>{for(const id of pointers){worker.postMessage({type:'pointer',epoch,args:[3,id,0,0]});}pointers.clear();};
 document.querySelector('#reload').onclick=()=>worker.postMessage({type:'reload'});
 function frame(){try{if(dirty){renderer.prepare([scene]);canvas.dataset.fontCache=JSON.stringify(renderer.cacheStats);renderer.beginFrame();scene.draw();renderer.endFrame();if(fontProbeRequested&&!fontProbed&&scene.glyphVertices.length){fontProbed=true;const result=renderer.probe([scene]);canvas.dataset.fontProbe=JSON.stringify(result);if(!result.passed)throw Error('GPU font probe did not pass: '+JSON.stringify(result));}if(gl.getError()!==gl.NO_ERROR)throw Error('GPU rendering failed');dirty=false;}}catch(error){fatal(error);return;}requestAnimationFrame(frame);}requestAnimationFrame(frame);
 const fontResource=await loadFontResource(import.meta.url);worker.postMessage({type:'start',fontBytes:fontResource.bytes,fontHash:fontResource.hash});
 canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();renderer.contextLost();fatal('Graphics context lost. Reload to reconnect.');});
}catch(e){fatal(e);}
