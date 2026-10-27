// SPDX-License-Identifier: Apache-2.0
// Real HTML editing surfaces; the worker remains the owner of committed text.
import {semanticBounds} from './viewport.mjs';
export function selection(input){
  const start=input.selectionStart??0,end=input.selectionEnd??start;
  return input.selectionDirection==='backward'?{cursor:start,anchor:end}:{cursor:end,anchor:start};
}
export function canReconcile(localRevision,frameRevision,composing){return !composing&&frameRevision>=localRevision;}
export class TextInputs {
  constructor(layer,send){this.layer=layer;this.send=send;this.fields=new Map();this.revision=0;new ResizeObserver(()=>this.resize()).observe(layer);document.addEventListener('selectionchange',()=>{for(const s of this.fields.values())if(s.input===document.activeElement)this.commit(s);});}
  resize(){for(const s of this.fields.values())s.input.style.transform=`scale(${this.layer.getBoundingClientRect().width/s.extent[0]})`;}
  get composing(){return [...this.fields.values()].some(s=>s.composing);}
  clear(){for(const s of this.fields.values()){if(s.composing)this.emit(s,'text-composition',{active:false});s.composing=false;s.removed=true;s.input.value='';s.input.remove();}this.fields.clear();}
  emit(s,type,extra={}){s.revision=++this.revision;this.send({type,epoch:s.epoch,id:s.id,inputRevision:s.revision,...extra});}
  commit(s){
    if(s.removed||s.composing)return;
    const value=s.input.value,range=selection(s.input),snapshot=JSON.stringify([value,range.cursor,range.anchor]);
    if(snapshot===s.sent)return;
    const selectionOnly=value===s.value;s.value=value;s.sent=snapshot;this.emit(s,'text-input',{text:value,selectionOnly,scroll:s.input.scrollTop,...range});
  }
  finishComposition(s){if(!s.composing)return;s.composing=false;s.ended=performance.now();this.commit(s);this.emit(s,'text-composition',{active:false});}
  create(d,epoch,label){
    const input=document.createElement(d.multiline?'textarea':'input');
    const s={input,epoch,id:d.id,revision:0,composing:false,removed:false,sent:null,ended:-Infinity,allowLineBreak:false};
    input.className='canvas-editor';input.dataset.sceneNode=String(d.id);input.dataset.epoch=String(epoch);
    if(!d.multiline)input.type=d.secret?'password':'text';
    input.setAttribute('aria-label',label);input.autocomplete=d.secret?'current-password':'off';input.spellcheck=!d.secret;
    input.maxLength=d.max_bytes;
    if(d.multiline)input.placeholder='Message...';
    input.enterKeyHint=d.multiline?'send':'done';input.setAttribute('autocapitalize',d.secret?'none':'sentences');
    input.addEventListener('focus',()=>{this.emit(s,'text-focus');s.sent=null;});
    input.addEventListener('input',e=>{if(!e.isComposing)this.commit(s);});
    input.addEventListener('scroll',()=>{if(!s.removed&&!s.composing&&document.activeElement===input)this.emit(s,'text-scroll',{scroll:input.scrollTop});});
    input.addEventListener('select',()=>{if(document.activeElement===input)this.commit(s);});
    input.addEventListener('compositionstart',()=>{s.composing=true;this.emit(s,'text-composition',{active:true});});
    input.addEventListener('compositionend',()=>this.finishComposition(s));
    input.addEventListener('blur',()=>{if(s.removed)return;this.finishComposition(s);this.commit(s);this.emit(s,'text-blur');});
    const enter=()=>{this.commit(s);this.emit(s,'text-enter');if(!d.multiline)input.blur();};
    input.addEventListener('keydown',e=>{
      if(e.isComposing||s.composing||e.keyCode===229)return;
      if(e.key==='Escape'){e.preventDefault();input.blur();return;}
      s.allowLineBreak=e.key==='Enter'&&e.shiftKey;
      if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();if(performance.now()-s.ended>50)enter();}
    });
    input.addEventListener('beforeinput',e=>{
      if(['insertParagraph','insertLineBreak'].includes(e.inputType)&&!e.isComposing&&!s.composing&&!s.allowLineBreak){e.preventDefault();if(performance.now()-s.ended>50)enter();}
    });
    input.addEventListener('keyup',()=>{s.allowLineBreak=false;});
    this.layer.append(input);return s;
  }
  accept(frame){
    if(!frame.inputs)return;
    const wanted=new Set(frame.inputs.map(d=>`${frame.epoch}:${d.id}`));
    for(const [key,s] of this.fields)if(!wanted.has(key)){if(s.composing)this.emit(s,'text-composition',{active:false});s.composing=false;s.removed=true;s.input.value='';s.input.remove();this.fields.delete(key);}
    const labels=new Map(frame.controls.map(c=>[c.id,c.label]));
    for(const d of frame.inputs){
      const key=`${frame.epoch}:${d.id}`;let s=this.fields.get(key);
      if(!s){s=this.create(d,frame.epoch,labels.get(d.id)||'Text');this.fields.set(key,s);}
      const input=s.input;Object.assign(input.style,semanticBounds(d.bounds,frame.extent));
      // Keep the computed font at its logical size (>=16px), then scale the
      // complete editor with the scene. This avoids iOS focus auto-zoom without
      // disabling the user's pinch zoom or changing shared font metrics.
      s.extent=frame.extent;input.style.width=`${d.bounds[2]}px`;input.style.height=`${d.bounds[3]}px`;
      input.style.fontSize=`${d.font_size}px`;input.style.lineHeight=`${d.line_height}px`;
      input.style.padding=[d.inset[1],d.inset[2],d.inset[3],d.inset[0]].map(v=>`${v}px`).join(' ');
      input.style.transform=`scale(${this.layer.getBoundingClientRect().width/frame.extent[0]})`;
      input.disabled=!!frame.authRequired;
      // Native selection/default edits can precede their queued select/input
      // event. Capture them before an older worker frame can move the caret.
      if(document.activeElement===input)this.commit(s);
      if(canReconcile(s.revision,frame.inputRevision||0,s.composing)){
        if(input.value!==d.value)input.value=d.value;
        const start=Math.min(d.cursor,d.anchor),end=Math.max(d.cursor,d.anchor),direction=d.cursor<d.anchor?'backward':'forward';
        if(input.selectionStart!==start||input.selectionEnd!==end||input.selectionDirection!==direction)input.setSelectionRange(start,end,direction);
        s.value=input.value;s.sent=JSON.stringify([input.value,d.cursor,d.anchor]);
      }
    }
  }
}
