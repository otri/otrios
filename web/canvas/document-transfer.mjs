// SPDX-License-Identifier: Apache-2.0
// Worker-side resource admission bookkeeping. There is one unacknowledged frame;
// viewport metadata can coalesce while exact resource versions survive its ACK.
export class DocumentTransfer {
  constructor({limit=8*1024*1024,uploadLimit=512*1024}={}){this.limit=limit;this.uploadLimit=uploadLimit;this.sequence=0;this.reset();}
  reset(){this.key=null;this.meta=null;this.resources=new Map();this.sent=null;this.needsMore=false;}
  update(meta,mount){
    const key=meta?`${mount}:${meta.epoch}`:null;
    if(key!==this.key){this.reset();this.key=key;}
    this.meta=meta;
  }
  take(read){
    if(!this.meta)return {key:null,draws:[],upserts:[],retire:[],loading:false,token:++this.sequence};
    const {draws,em}=this.meta,wanted=new Set(draws.map(d=>d.version)),retire=[],upserts=[];
    if(wanted.size!==draws.length||draws.length>128)throw Error('Invalid document draw list');
    const size=d=>{if(!Number.isInteger(d.glyphs)||d.glyphs<1||d.glyphs>2048)throw Error('Invalid document chunk size');return d.glyphs*512+12;};
    if(draws.reduce((n,d)=>n+size(d),0)>this.limit)throw Error('Visible document exceeds GPU budget');
    let bytes=0;
    for(const d of draws){
      if(this.resources.has(d.version))continue;
      const payloadBytes=d.glyphs*256;
      if(bytes+payloadBytes>this.uploadLimit)continue;
      while([...this.resources.values()].reduce((n,r)=>n+r.bytes,0)+size(d)>this.limit||this.resources.size>=128){
        const old=[...this.resources].find(([id,r])=>!wanted.has(id)&&r.state==='ack');
        if(!old)break;
        this.resources.delete(old[0]);retire.push(old[0]);
      }
      if([...this.resources.values()].reduce((n,r)=>n+r.bytes,0)+size(d)>this.limit)continue;
      const glyphVertices=read(d.version);
      if(!(glyphVertices instanceof Float32Array)||glyphVertices.byteLength!==payloadBytes)throw Error('Document snapshot/resource mismatch');
      this.resources.set(d.version,{bytes:size(d),state:'sent'});
      bytes+=payloadBytes;upserts.push({version:d.version,glyphVertices});
    }
    // Refresh insertion order so eviction preserves a reverse-direction reserve.
    for(const d of draws){const r=this.resources.get(d.version);if(r){this.resources.delete(d.version);this.resources.set(d.version,r);}}
    const ready=draws.filter(d=>this.resources.has(d.version));
    this.needsMore=ready.length!==draws.length;
    const token=++this.sequence;
    this.sent={token,key:this.key,versions:upserts.map(u=>u.version)};
    return {key:this.key,em,draws:ready,upserts,retire,loading:this.meta.loading||this.needsMore,token};
  }
  ack(token){
    if(!this.sent||this.sent.token!==token||this.sent.key!==this.key)return;
    for(const id of this.sent.versions){const r=this.resources.get(id);if(r)r.state='ack';}
    this.sent=null;
  }
}

// Display-owned buffers. Resources never contain source text or mutable layout.
export class DocumentLayers {
  constructor(renderer){this.renderer=renderer;this.key=null;this.layers=new Map();this.draws=[];this.loading=false;}
  clear(){for(const layer of this.layers.values())layer.dispose();this.layers.clear();this.draws=[];this.key=null;this.loading=false;}
  accept(packet){
    if(packet.key!==this.key){this.clear();this.key=packet.key;}
    for(const id of packet.retire){this.layers.get(id)?.dispose();this.layers.delete(id);}
    for(const {version,glyphVertices} of packet.upserts){
      if(this.layers.has(version))throw Error('Duplicate document resource version');
      const layer=this.renderer.createLayer();
      layer.setFrame({vertices:new Float32Array(),glyphVertices,draws:new Uint32Array([1,0,glyphVertices.length/16]),textStyles:new Float32Array([packet.em])});
      this.layers.set(version,layer);
    }
    this.draws=packet.draws.map(d=>{const layer=this.layers.get(d.version);if(!layer)throw Error('Document frame references an unadmitted version');layer.setTextViews(new Float32Array(d.view));return layer;});
    this.loading=packet.loading;
  }
}
