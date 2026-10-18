// SPDX-License-Identifier: Apache-2.0
import init,{CanvasApp} from './pkg/host.js';
const ready=init();
let app,ws,dirty=false,fontBytes,fontHash;
function error(e){postMessage({type:'error',message:String(e)});}
function start(){app?.free();app=new CanvasApp();app.set_font(fontBytes,fontHash);app.open('app:home/ui','app:home/state');ws=new WebSocket(`${location.protocol==='https:'?'wss:':'ws:'}//${location.host}/stream`);ws.binaryType='arraybuffer';ws.onopen=()=>postMessage({type:'online',value:true});ws.onmessage=e=>{try{dirty=app.receive(new Uint8Array(e.data))||dirty;}catch(e){error(e);}};ws.onclose=()=>{postMessage({type:'online',value:false});setTimeout(start,1200);};}
onmessage=async({data:m})=>{try{await ready;if(m.type==='start'){fontBytes=m.fontBytes;fontHash=m.fontHash;start();const [curveWidth,curveHeight,bandWidth,bandHeight]=app.font_dimensions();postMessage({type:'font',font:{curveWidth,curveHeight,curves:app.font_curves(),bandWidth,bandHeight,bands:app.font_bands()}});return;}if(m.type==='pointer')dirty=app.pointer(m.epoch,...m.args)||dirty;else if(m.type==='activate')dirty=app.activate(m.epoch,m.id)||dirty;else if(m.type==='reload'){app.open(app.ui_ref(),app.state_ref());}}catch(e){error(e);}};
let previous='';
setInterval(()=>{try{
  if(!app||!ws)return;
  if(ws.readyState===WebSocket.OPEN){for(let i=0;i<8;i++){const b=app.outgoing();if(!b)break;ws.send(b);}}
  if(dirty&&app.ui_ref()){
    const vertices=app.vertices(),glyphVertices=app.glyph_vertices(),draws=app.draws();const controls=Array.from(app.controls(),id=>({id,label:app.label(id),bounds:Array.from(app.bounds(id))}));
    postMessage({type:'frame',vertices,glyphVertices,draws,controls,text:app.text(),title:app.title(),epoch:app.epoch(),ui:app.ui_ref(),state:app.state_ref()},[vertices.buffer,glyphVertices.buffer,draws.buffer]);dirty=false;
  }
  const status=app.confirmed()?'Shared state acknowledged':'Synchronizing';if(status!==previous){postMessage({type:'status',value:status});previous=status;}
}catch(e){error(e);}},20);
