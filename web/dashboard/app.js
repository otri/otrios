// SPDX-License-Identifier: Apache-2.0
import init, { Dashboard } from './pkg/otrios_dashboard.js';
const canvas=document.querySelector('#dashboard');
const controlsLayer=document.querySelector('#controls');
const controls=new Map();
const connection=document.querySelector('#connection');
const dot=document.querySelector('#connection-dot');
const revision=document.querySelector('#revision');
const error=document.querySelector('#error');
function fatal(e){error.hidden=false;error.textContent=String(e);connection.textContent='Unable to open this space';}
try {
  await init();
  let dashboard;
  const storageKey='otrios-dashboard-v1';
  const presentationKey='otrios-dashboard-presentation-v1';
  try {
    const saved=sessionStorage.getItem(storageKey);
    dashboard=saved?Dashboard.load(Uint8Array.from(atob(saved),c=>c.charCodeAt(0))):new Dashboard();
  } catch(e) {throw new Error(`Saved dashboard could not be loaded; it has been kept intact. ${e}`);}
  const cachedPresentation=sessionStorage.getItem(presentationKey);
  if(cachedPresentation)dashboard.present(Uint8Array.from(atob(cachedPresentation),c=>c.charCodeAt(0)));
  const gl=canvas.getContext('webgl',{alpha:false,antialias:false,depth:false,stencil:false});
  if(!gl)throw new Error('This dashboard requires WebGL.');
  gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
  function shader(type,source){const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw new Error(gl.getShaderInfoLog(s));return s;}
  const vs=shader(gl.VERTEX_SHADER,'attribute vec2 pos; attribute vec4 color; varying lowp vec4 tint; void main(){gl_Position=vec4(pos.x/640.0-1.0,1.0-pos.y/400.0,0.0,1.0);tint=color;}');
  const fs=shader(gl.FRAGMENT_SHADER,'varying lowp vec4 tint; void main(){gl_FragColor=tint;}');
  const program=gl.createProgram();gl.attachShader(program,vs);gl.attachShader(program,fs);gl.linkProgram(program);
  if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(program));
  gl.useProgram(program);gl.deleteShader(vs);gl.deleteShader(fs);
  const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
  const pos=gl.getAttribLocation(program,'pos'),color=gl.getAttribLocation(program,'color');
  gl.enableVertexAttribArray(pos);gl.vertexAttribPointer(pos,2,gl.FLOAT,false,24,0);
  gl.enableVertexAttribArray(color);gl.vertexAttribPointer(color,4,gl.FLOAT,false,24,8);
  function persist(){
    try {const bytes=dashboard.document();let binary='';for(let i=0;i<bytes.length;i+=8192)binary+=String.fromCharCode(...bytes.subarray(i,i+8192));sessionStorage.setItem(storageKey,btoa(binary));return true;}
    catch(e){connection.textContent='Changes live in this tab; storage unavailable';console.warn(e);return false;}
  }
  function draw(){
    const vertices=dashboard.vertices();gl.viewport(0,0,1280,800);gl.clearColor(16/255,28/255,41/255,1);gl.clear(gl.COLOR_BUFFER_BIT);gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STATIC_DRAW);gl.drawArrays(gl.TRIANGLES,0,vertices.length/6);
    if(gl.getError()!==gl.NO_ERROR)throw new Error('GPU rendering failed.');
    const ids=new Set(dashboard.control_ids());
    for(const [id,button] of controls)if(!ids.has(id)){button.remove();controls.delete(id);}
    for(const id of ids){
      let button=controls.get(id);
      if(!button){
        button=document.createElement('button');button.type='button';button.setAttribute('role','switch');button.dataset.sceneNode=String(id);
        button.addEventListener('click',()=>{try{if(dashboard.activate(id)){draw();sendChanges();}}catch(e){fatal(e);}});
        controlsLayer.append(button);controls.set(id,button);
      }
      button.setAttribute('aria-label',dashboard.control_label(id));button.setAttribute('aria-checked',String(dashboard.control_checked(id)));
      const [x,y,w,h]=dashboard.control_bounds(id);
      Object.assign(button.style,{left:`${x/12.8}%`,top:`${y/8}%`,width:`${w/12.8}%`,height:`${h/8}%`});
    }
    canvas.setAttribute('aria-label',dashboard.scene_text());
    revision.textContent=`VIEW ${dashboard.snapshot_id().slice(0,12)}`;
    revision.dataset.snapshot=dashboard.snapshot_id();
    revision.dataset.layout=dashboard.layout_id();
    persist();
  }
  let socket=null,retry=250,stopped=false;
  let activeRelease=location.pathname.match(/^\/releases\/([0-9a-f]{64})\//)?.[1]||null;
  function sendChanges(){
    if(socket?.readyState!==WebSocket.OPEN)return;
    const bytes=dashboard.sync_message();if(bytes)socket.send(bytes);
  }
  function connect(){
    dashboard.reset_sync();
    const url=new URL('/stream',location.href);url.protocol=url.protocol==='https:'?'wss:':'ws:';
    socket=new WebSocket(url);socket.binaryType='arraybuffer';
    socket.onopen=()=>{retry=250;sendChanges();};
    socket.onmessage=event=>{
      try {
        if(typeof event.data==='string'){
          const message=JSON.parse(event.data);
          if(message.type==='release'){
            const release=message.release;
            if(activeRelease&&release.id!==activeRelease&&release.web_entry){
              if(!persist()){error.hidden=false;error.textContent='A code update is ready. Automatic reload is paused because this tab cannot save its state.';return;}
              stopped=true;socket.close();location.replace(release.web_entry);return;
            }
            activeRelease=release.id;
            revision.dataset.release=release.id;
          }
        }else{
          const bytes=new Uint8Array(event.data);
          if(bytes.length>=5&&String.fromCharCode(...bytes.subarray(0,5))==='OTML1'){
            const view=bytes.subarray(5);
            if(dashboard.present(view))draw();
            try{let encoded='';for(let i=0;i<view.length;i+=8192)encoded+=String.fromCharCode(...view.subarray(i,i+8192));if(view.length)sessionStorage.setItem(presentationKey,btoa(encoded));else sessionStorage.removeItem(presentationKey);}catch(e){console.warn('Monitoring view cache unavailable',e);}
          }else if(dashboard.receive_sync(bytes))draw();
          sendChanges();
          dot.classList.add('online');connection.textContent='Live shared space';
        }
      }catch(e){console.error(e);socket.close();}
    };
    socket.onclose=()=>{
      dot.classList.remove('online');connection.textContent='Offline · changes saved in this tab';
      if(!stopped){setTimeout(connect,retry);retry=Math.min(retry*2,5000);}
    };
    socket.onerror=()=>socket.close();
  }
  const pointers=new Set();
  function pointer(event,phase){
    try{
      const r=canvas.getBoundingClientRect();
      const changed=dashboard.pointer(phase,event.pointerId,(event.clientX-r.left)*1280/r.width,(event.clientY-r.top)*800/r.height);
      if(changed){draw();sendChanges();}
    }catch(e){fatal(e);}
  }
  canvas.addEventListener('pointerdown',event=>{event.preventDefault();pointer(event,0);pointers.add(event.pointerId);canvas.setPointerCapture(event.pointerId);});
  canvas.addEventListener('pointermove',event=>{if(pointers.has(event.pointerId))pointer(event,1);});
  canvas.addEventListener('pointerup',event=>{pointer(event,2);pointers.delete(event.pointerId);});
  canvas.addEventListener('pointercancel',event=>{pointer(event,3);pointers.delete(event.pointerId);});
  canvas.addEventListener('lostpointercapture',event=>{pointer(event,3);pointers.delete(event.pointerId);});
  window.addEventListener('blur',()=>{for(const id of pointers){dashboard.pointer(3,id,0,0);if(canvas.hasPointerCapture(id))canvas.releasePointerCapture(id);}pointers.clear();});
  canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();fatal('Graphics context lost. Reload to resume your saved dashboard.');});
  draw();connect();
}catch(e){fatal(e);}
