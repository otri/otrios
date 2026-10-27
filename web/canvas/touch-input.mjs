// SPDX-License-Identifier: Apache-2.0
// Keep source timestamps and coalesced samples while batching worker messages.
export function sampleTime(event,clock){
  const now=clock.timeOrigin+clock.now(),t=event.timeStamp;
  if(!Number.isFinite(t)||t<0)return now;
  return Math.min(now,t>1e12?t:clock.timeOrigin+t);
}
export class PointerSamples {
  constructor(){this.pending=new Map();}
  get size(){return this.pending.size;}
  has(id){return this.pending.has(id);}
  clear(){this.pending.clear();}
  delete(id){this.pending.delete(id);}
  add(event,convert){
    const coalesced=event.getCoalescedEvents?.()||[];
    for(const e of coalesced.length?coalesced:[event]){
      const packet=convert(e),id=packet.args[1],at=packet.args[4];
      const batch=this.pending.get(id)||{type:'pointer-batch',epoch:packet.epoch,viewportRevision:packet.viewportRevision,samples:[]};
      const samples=batch.samples,last=samples.at(-1);
      if(last&&at<last[4])continue;
      if(last&&Math.floor(last[4]/4)===Math.floor(at/4))samples.pop();
      samples.push(packet.args);
      while(samples.length>64||at-samples[0][4]>200)samples.shift();
      this.pending.set(id,batch);
    }
  }
  take(id){const batch=this.pending.get(id);this.pending.delete(id);return batch;}
  [Symbol.iterator](){return this.pending[Symbol.iterator]();}
}
