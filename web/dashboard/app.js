// SPDX-License-Identifier: Apache-2.0
import init, { Dashboard } from './pkg/otrios_dashboard.js';
const canvas=document.querySelector('#dashboard');
const toggle=document.querySelector('#toggle');
const connection=document.querySelector('#connection');
const dot=document.querySelector('#connection-dot');
const revision=document.querySelector('#revision');
const error=document.querySelector('#error');
function fatal(e){error.hidden=false;error.textContent=String(e);connection.textContent='Unable to open this space';}
try {
  await init();
  let dashboard;
  const storageKey='otrios-dashboard-v1';
  try {
    const saved=sessionStorage.getItem(storageKey);
    dashboard=saved?Dashboard.load(Uint8Array.from(atob(saved),c=>c.charCodeAt(0))):new Dashboard();
  } catch(e) {throw new Error(`Saved dashboard could not be loaded; it has been kept intact. ${e}`);}
  const gl=canvas.getContext('webgl',{alpha:false,antialias:false,depth:false,stencil:false});
  if(!gl)throw new Error('This dashboard requires WebGL.');
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
    const vertices=dashboard.vertices();gl.viewport(0,0,1280,800);gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STATIC_DRAW);gl.drawArrays(gl.TRIANGLES,0,vertices.length/6);
    if(gl.getError()!==gl.NO_ERROR)throw new Error('GPU rendering failed.');
    toggle.disabled=false;toggle.setAttribute('aria-checked',String(dashboard.enabled()));
    const [x,y,w,h]=dashboard.switch_bounds();
    Object.assign(toggle.style,{left:`${x/12.8}%`,top:`${y/8}%`,width:`${w/12.8}%`,height:`${h/8}%`});
    revision.textContent=`VIEW ${dashboard.snapshot_id().slice(0,12)}`;
    revision.dataset.snapshot=dashboard.snapshot_id();
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
          if(dashboard.receive_sync(new Uint8Array(event.data)))draw();
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
  toggle.addEventListener('click',()=>{try {dashboard.set_enabled(!dashboard.enabled());draw();sendChanges();}catch(e){fatal(e);}});
  canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();fatal('Graphics context lost. Reload to resume your saved dashboard.');});
  draw();connect();
}catch(e){fatal(e);}
