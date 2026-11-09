// SPDX-License-Identifier: Apache-2.0
// Immutable UI resources cross renderer credit once; placement and paint remain
// frame metadata. Whole frames are admitted atomically, never partially drawn.
const LAYERS=['base','overlay'];
// Mirrors core MAX_TEXT_CHUNKS / MAX_TEXT_RESOURCE_BYTES; counts apply to both UI layers.
const MAX_CHUNKS=32,MAX_BYTES=16*1024*1024;
function version(id){return typeof id==='string'&&/^[1-9][0-9]{0,19}$/.test(id)&&BigInt(id)<=18446744073709551615n;}
const identity=(layer,id)=>`${layer}:${id}`;
function descriptor(d){
  if(!version(d.version)||!Number.isInteger(d.vertices)||d.vertices<0||d.vertices>65536||d.vertices%3||!Number.isInteger(d.glyphVertices)||d.glyphVertices<0||d.glyphVertices>65536||d.glyphVertices%4||!Number.isInteger(d.draws)||d.draws<1||d.draws>4096)throw Error('Invalid presentation chunk dimensions');
  if(!Array.isArray(d.views)||d.views.length!==d.draws*8||d.views.some(v=>!Number.isFinite(v)||Math.abs(v)>100000)||!Array.isArray(d.paints)||d.paints.length!==d.draws*4||d.paints.some(v=>!Number.isFinite(v)||v<0||v>1))throw Error('Invalid presentation metadata');
  return d.vertices*24+d.glyphVertices*128+d.draws*16;
}
function resource(payload,d){
  const {vertices,glyphVertices,draws,textStyles}=payload;
  if(!(vertices instanceof Float32Array)||vertices.length!==d.vertices*6||!(glyphVertices instanceof Float32Array)||glyphVertices.length!==d.glyphVertices*16||!(draws instanceof Uint32Array)||draws.length!==d.draws*3||!(textStyles instanceof Float32Array)||textStyles.length!==d.draws)throw Error('Presentation snapshot/resource mismatch');
}
export class PresentationTransfer {
  constructor(){this.sequence=0;this.reset();}
  reset(){this.key=null;this.meta=null;this.resources=new Set();this.sent=null;}
  update(meta,mount){
    if(meta&&!version(meta.epoch))throw Error('Invalid presentation epoch');
    const key=meta?`${mount}:${meta.epoch}`:null;
    if(key!==this.key){this.reset();this.key=key;}
    this.meta=meta;
  }
  take(read){
    const token=++this.sequence;
    if(!this.meta)return {key:null,enabled:false,base:[],overlay:[],upserts:[],retire:[],token};
    const wanted=new Set();let bytes=0;
    for(const layer of LAYERS){
      const list=this.meta[layer];if(!Array.isArray(list)||list.length>MAX_CHUNKS)throw Error('Invalid presentation chunk list');
      let vertices=0,glyphs=0,draws=0;
      for(const d of list){bytes+=descriptor(d);const key=identity(layer,d.version);if(wanted.has(key))throw Error('Duplicate presentation chunk');wanted.add(key);vertices+=d.vertices;glyphs+=d.glyphVertices;draws+=d.draws;}
      if(vertices>65536||glyphs>65536||draws>4096)throw Error('Presentation layer exceeds retained limits');
    }
    if(bytes>MAX_BYTES)throw Error('Presentation exceeds GPU budget');
    const upserts=[];
    for(const layer of LAYERS)for(const d of this.meta[layer]){
      if(this.resources.has(identity(layer,d.version)))continue;
      const payload=read(layer,d.version);resource(payload,d);upserts.push({layer,version:d.version,...payload});
    }
    const retire=[...this.resources].filter(id=>!wanted.has(id)).map(id=>{const [layer,version]=id.split(':');return {layer,version};});
    this.resources=wanted;this.sent={token,key:this.key};
    const metadata=layer=>this.meta[layer].map(d=>({version:d.version,views:new Float32Array(d.views),paints:new Float32Array(d.paints)}));
    return {key:this.key,enabled:true,base:metadata('base'),overlay:metadata('overlay'),upserts,retire,token};
  }
  ack(token){if(this.sent&&this.sent.token===token&&this.sent.key===this.key)this.sent=null;}
}

export class PresentationLayers {
  constructor(renderer){this.renderer=renderer;this.key=null;this.resources=new Map();this.base=[];this.overlay=[];this.enabled=false;this.uploads=0;this.uploadedBytes=0;}
  clear(){for(const layer of this.resources.values())layer.dispose();this.resources.clear();this.base=[];this.overlay=[];this.key=null;this.enabled=false;}
  accept(packet){
    if(packet.key!==this.key){this.clear();this.key=packet.key;}
    if(!packet.enabled){this.clear();return;}
    if(packet.base.length>MAX_CHUNKS||packet.overlay.length>MAX_CHUNKS)throw Error('Invalid presentation chunk list');
    for(const {layer,version} of packet.retire){const id=identity(layer,version);this.resources.get(id)?.dispose();this.resources.delete(id);}
    for(const {layer,version,...payload} of packet.upserts){
      const id=identity(layer,version);if(!LAYERS.includes(layer)||!version||this.resources.has(id))throw Error('Duplicate presentation resource');
      const next=this.renderer.createLayer();try{next.setFrame(payload);}catch(error){next.dispose();throw error;}
      this.resources.set(id,next);this.uploads++;this.uploadedBytes+=payload.vertices.byteLength+payload.glyphVertices.byteLength;
    }
    const ordered=layer=>packet[layer].map(d=>{const resource=this.resources.get(identity(layer,d.version));if(!resource)throw Error('Presentation frame references an unadmitted version');resource.setTextViews(d.views);resource.setPaints(d.paints);return resource;});
    this.base=ordered('base');this.overlay=ordered('overlay');this.enabled=true;
  }
}
