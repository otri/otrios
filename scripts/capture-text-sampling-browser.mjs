// SPDX-License-Identifier: Apache-2.0
// Record real WebGL sampling comparisons; reported differences are not pixel-identity acceptance.
import {createServer} from 'node:http';
import {readFile,mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
const args=process.argv.slice(2),arg=(name,fallback)=>{const i=args.indexOf(name);return i<0?fallback:args[i+1];};
const repo=fileURLToPath(new URL('..',import.meta.url)),out=resolve(arg('--output',repo+'/target/text-rendering'));
const fixture=arg('--fixture');if(!fixture)throw Error('Supply --fixture from text_scroll_frames --dense --retained');await mkdir(out,{recursive:true});
const font=await readFile(fixture+'.font'),stream=await readFile(fixture),magic=stream.subarray(0,8).toString();assert(['OTRTEXT2','OTRTEXT3'].includes(magic),'Supply a retained fixture');let at=16;
const n=Array.from({length:magic==='OTRTEXT3'?5:4},()=>{const x=stream.readUInt32LE(at);at+=4;return x;});
const arr=(count,kind)=>{const x=Array.from({length:count},(_,i)=>kind==='u'?stream.readUInt32LE(at+i*4):stream.readFloatLE(at+i*4));at+=count*4;return x;};
const v=arr(n[0]),g=arr(n[1]),d=arr(n[2],'u'),views=arr(n[3]),styles=arr(n[4]||0),i=d.findIndex((x,i)=>i%3===0&&x===1),first=d[i+1],count=d[i+2];
const frame={vertices:v,glyphVertices:g.slice(first*16,(first+count)*16),draws:[0,0,v.length/6,1,0,count],textStyles:styles.length?[0,styles[i/3]]:[],textViews:[...Array(8).fill(0),...views.slice(i/3*8,i/3*8+8)]};frame.textViews[8]-=first;
const html=`<body style="margin:0;background:#101c29"><canvas id="view" width="1280" height="800"></canvas><script type="module">
import {SceneRenderer} from '/gpu-scene.mjs';
window.result=null;
try{
const data=await(await fetch('/frame')).json(),b=await(await fetch('/font')).arrayBuffer(),h=new DataView(b),cw=h.getUint32(8,true),ch=h.getUint32(12,true),bw=h.getUint32(16,true),bh=h.getUint32(20,true),packet={curveWidth:cw,curveHeight:ch,bandWidth:bw,bandHeight:bh,curves:new Float32Array(b,24,cw*ch*4),bands:new Uint8Array(b,24+cw*ch*16,bw*bh*4)};
const results=[],captures=[];const canvas=document.querySelector('#view'),gl=canvas.getContext('webgl',{alpha:false,antialias:false,preserveDrawingBuffer:true});
for(const sampling of [2,3,0]){const r=new SceneRenderer(gl,{atlasScale:sampling,stableTextQuads:${args.includes("--stable-quads")}}),l=r.createLayer();r.setFont(packet);l.setFrame({vertices:new Float32Array(data.vertices),glyphVertices:new Float32Array(data.glyphVertices),draws:new Uint32Array(data.draws),textStyles:new Float32Array(data.textStyles)});l.setTextViews(new Float32Array(data.textViews));r.prepare([l]);const misses=r.cacheStats.misses;
for(const phase of [0,.25,.5,.75]){const views=new Float32Array(data.textViews);views[10]-=phase;l.setTextViews(views);const report=r.probe([l]);results.push({sampling,phase,...report});
if(sampling!==0&&phase===.25){
 const count=1280*800*4,a=new Uint8Array(count),b=new Uint8Array(count);gl.readPixels(0,0,1280,800,gl.RGBA,gl.UNSIGNED_BYTE,a);const generation=l.preparedGeneration;l.preparedGeneration=0;r.beginFrame();l.draw();r.endFrame();gl.readPixels(0,0,1280,800,gl.RGBA,gl.UNSIGNED_BYTE,b);l.preparedGeneration=generation;
 const c=document.createElement('canvas');c.width=1280;c.height=210;const ctx=c.getContext('2d'),image=ctx.createImageData(1280,210);
 for(let y=0;y<70;y++)for(let x=0;x<1280;x++){const p=((799-y)*1280+x)*4,q=(y*1280+x)*4;for(let j=0;j<4;j++){image.data[q+j]=a[p+j];image.data[q+1280*70*4+j]=b[p+j];image.data[q+1280*140*4+j]=j===3?255:Math.min(255,Math.abs(a[p+j]-b[p+j])*8);}}
 ctx.putImageData(image,0,0);captures.push({name:'sampling-'+sampling+'x-phase-025',png:c.toDataURL('image/png')});
}
if(r.cacheStats.misses!==misses)throw Error('Viewport motion rasterized glyphs');}
r.dispose();}
window.result={results,captures};
}catch(error){window.result={error:String(error),stack:error.stack};}
</script>`;
const server=createServer(async(req,res)=>{try{if(req.url==='/'){res.setHeader('content-type','text/html');res.end(html);}else if(req.url==='/font')res.end(font);else if(req.url==='/frame'){res.setHeader('content-type','application/json');res.end(JSON.stringify(frame));}else if(['/gpu-scene.mjs','/slug-shaders.mjs'].includes(req.url)){res.setHeader('content-type','text/javascript');res.end(await readFile(repo+'/web/shared'+req.url));}else{res.statusCode=404;res.end();}}catch(e){res.statusCode=500;res.end(String(e));}});await new Promise(r=>server.listen(0,'127.0.0.1',r));
const profile=await mkdtemp('/tmp/otrios-text-chrome-');const browser=spawn(arg('--chromium','chromium'),['--headless=new','--no-sandbox','--enable-unsafe-swiftshader','--use-angle=swiftshader','--remote-debugging-port=0','--user-data-dir='+profile,'about:blank'],{stdio:['ignore','ignore','pipe']});let socket;
try{const endpoint=await new Promise((resolve,reject)=>{let log='';const timer=setTimeout(()=>reject(Error(log)),30000);browser.stderr.on('data',b=>{log+=b;const m=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(m){clearTimeout(timer);resolve(m[1]);}});browser.on('error',reject);});socket=new WebSocket(endpoint);await new Promise(r=>socket.addEventListener('open',r,{once:true}));let serial=0;const pending=new Map();socket.addEventListener('message',({data})=>{const m=JSON.parse(data);if(m.id&&pending.has(m.id)){const {resolve,reject}=pending.get(m.id);pending.delete(m.id);m.error?reject(Error(JSON.stringify(m.error))):resolve(m.result);}});const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++serial;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params,sessionId}));});const {targetId}=await call('Target.createTarget',{url:'about:blank'}),{sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});await call('Page.enable',{},sessionId);await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},sessionId);await call('Page.navigate',{url:'http://127.0.0.1:'+server.address().port},sessionId);let result;for(let i=0;i<600;i++){const q=await call('Runtime.evaluate',{expression:'window.result',returnByValue:true},sessionId);result=q.result.value;if(result)break;await new Promise(r=>setTimeout(r,100));}assert(result);for(const capture of result.captures||[])await writeFile(out+'/'+capture.name+'.png',Buffer.from(capture.png.split(',')[1],'base64'));delete result.captures;await writeFile(out+'/browser-sampling.json',JSON.stringify(result,null,2));console.log(JSON.stringify(result));assert(!result.error,result.error);const shot=await call('Page.captureScreenshot',{format:'png'},sessionId);await writeFile(out+'/browser-sampling.png',Buffer.from(shot.data,'base64'));
}finally{socket?.close();browser.kill('SIGTERM');await new Promise(r=>browser.once('exit',r));server.close();await rm(profile,{recursive:true,force:true});}
