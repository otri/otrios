// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {TextInputs,selection} from '../web/shared/text-input.mjs';
class Field extends EventTarget {
  constructor(){super();this.value='';this.style={};this.dataset={};this.selectionStart=this.selectionEnd=0;this.selectionDirection='none';}
  setAttribute(name,value){(this.attributes??={})[name]=value;}
  remove(){this.removed=true;if(document.activeElement===this)this.blur();}
  blur(){if(document.activeElement!==this)return;document.activeElement=null;this.dispatchEvent(new Event('blur'));}
  focus(){if(document.activeElement===this)return;document.activeElement?.blur();document.activeElement=this;this.dispatchEvent(new Event('focus'));}
  setSelectionRange(start,end,direction='none'){this.selectionStart=start;this.selectionEnd=end;this.selectionDirection=direction;}
}
function fixture(options={}){
  globalThis.document=Object.assign(new EventTarget(),{createElement:()=>new Field(),activeElement:null});
  globalThis.ResizeObserver=class {observe(){}};
  const sent=[],layer={append(){},getBoundingClientRect:()=>({width:390})};
  const bridge=new TextInputs(layer,m=>sent.push(m));
  const d={id:42,value:'hello',cursor:5,anchor:5,multiline:true,max_bytes:2048,bounds:[0,0,700,60],inset:[12,12,12,12],font_size:22.5,line_height:26.4,focused:true,...options};
  const frame=(value=d.value,revision=0)=>({epoch:7,extent:[800,1280],controls:[{id:42,label:'Composer'}],inputs:[{...d,value,cursor:value.length,anchor:value.length}],inputRevision:revision});
  bridge.accept(frame());const field=bridge.fields.get('7:42').input;assert.equal(document.activeElement,null,'initial core focus never autofocuses');field.focus();
  return {bridge,field,sent,frame};
}
test('retained field refreshes its accessible name without replacing focused raw input',()=>{
  const {bridge,field,sent,frame}=fixture({multiline:false});
  field.value='uncommitted caption';field.setSelectionRange(3,8,'backward');
  field.dispatchEvent(new Event('compositionstart'));
  const updated={...frame('old caption',100),controls:[{id:42,label:'Property name: caption'}]};
  bridge.accept(updated);
  assert.equal(bridge.fields.get('7:42').input,field);
  assert.equal(field.attributes['aria-label'],'Property name: caption');
  assert.equal(document.activeElement,field);
  assert.equal(field.value,'uncommitted caption');
  assert.deepEqual(selection(field),{cursor:3,anchor:8});
  assert(!sent.some(m=>m.type==='text-input'));
});
test('native selection preserves backward direction and UTF-16 offsets',()=>{
  const {bridge,field,sent}=fixture();field.value='a😀é';field.setSelectionRange(1,4,'backward');field.dispatchEvent(new Event('select'));
  assert.deepEqual(selection(field),{cursor:1,anchor:4});assert.equal(sent.at(-1).text,'a😀é');
  assert.equal(field.style.fontSize,'22.5px');assert.equal(field.style.transform,'scale(0.4875)');assert.equal(bridge.composing,false);
});
test('IME stays local through stale frames and commits exactly once',()=>{
  const {bridge,field,sent,frame}=fixture();field.dispatchEvent(new Event('compositionstart'));
  field.value='日本語';field.setSelectionRange(3,3);field.dispatchEvent(Object.assign(new Event('input'),{isComposing:true}));
  bridge.accept(frame('old server value',100));assert.equal(field.value,'日本語');assert(!sent.some(m=>m.type==='text-input'));
  field.dispatchEvent(new Event('compositionend'));field.dispatchEvent(new Event('input'));
  const edits=sent.filter(m=>m.type==='text-input');assert.equal(edits.length,1);assert.equal(edits[0].text,'日本語');assert.equal(bridge.composing,false);
});
test('delayed frames never erase fast typing; acknowledged rejection restores bounded state',()=>{
  const {bridge,field,sent,frame}=fixture();field.value='new value';field.setSelectionRange(9,9);field.dispatchEvent(new Event('input'));
  const revision=sent.at(-1).inputRevision;bridge.accept(frame('hello',revision-1));assert.equal(field.value,'new value');
  bridge.accept(frame('hello',revision));assert.equal(field.value,'hello');assert.equal(field.selectionStart,5);
});
test('unmount clears private text and a late composition end cannot edit another field',()=>{
  const {bridge,field,sent}=fixture();field.dispatchEvent(new Event('compositionstart'));field.value='secret';bridge.clear();field.dispatchEvent(new Event('compositionend'));
  assert.equal(field.value,'');assert.equal(bridge.composing,false);assert(!sent.some(m=>m.type==='text-input'));
});
test('an old frame cannot reset native selection before its queued select event',()=>{
  const {bridge,field,sent,frame}=fixture();bridge.accept(frame('hello',sent.at(-1).inputRevision));
  field.setSelectionRange(0,2,'forward');bridge.accept(frame('hello',sent.at(-1).inputRevision));
  assert.equal(field.selectionStart,0);assert.equal(field.selectionEnd,2);assert.equal(sent.at(-1).type,'text-input');
});
test('selection messages are distinguished from actual replacement text',()=>{
  const {field,sent}=fixture();field.setSelectionRange(0,2);field.dispatchEvent(new Event('select'));
  assert.equal(sent.at(-1).selectionOnly,true);
  field.value='changed';field.setSelectionRange(7,7);field.dispatchEvent(new Event('input'));
  assert.equal(sent.at(-1).selectionOnly,false);
  field.setSelectionRange(0,7);field.dispatchEvent(new Event('select'));
  assert.equal(sent.at(-1).selectionOnly,true);
});

function key(field,key,extra={}){
  const event=Object.assign(new Event('keydown',{cancelable:true}),{key,...extra});
  let stopped=false;event.stopPropagation=()=>{stopped=true;};field.dispatchEvent(event);
  return {prevented:event.defaultPrevented,stopped};
}
function types(sent){return sent.map(m=>m.type);}
test('Escape cancels without flushing queued input, selection or blur',()=>{
  const {bridge,field,sent,frame}=fixture({multiline:false});
  field.value='unreported draft';field.setSelectionRange(16,16);sent.length=0;
  assert.deepEqual(key(field,'Escape'),{prevented:true,stopped:true});
  field.dispatchEvent(new Event('input'));field.dispatchEvent(new Event('select'));
  document.dispatchEvent(new Event('selectionchange'));field.dispatchEvent(new Event('blur'));field.focus();
  assert.equal(document.activeElement,null,'cancelled surface cannot reactivate before acknowledgement');
  assert.deepEqual(types(sent),['text-cancel']);assert.equal(sent[0].epoch,7);assert.equal(sent[0].id,42);
  const revision=sent[0].inputRevision;
  bridge.accept(frame('stale draft',revision-1));assert.equal(field.value,'unreported draft');assert.equal(document.activeElement,null);
  bridge.accept({...frame('hello',revision),inputs:[{...frame().inputs[0],focused:false}]});
  assert.equal(field.value,'hello');assert.equal(document.activeElement,null);assert.deepEqual(types(sent),['text-cancel']);
});
test('IME Escape cannot cancel during composition, keyCode 229, or the composition-end guard',()=>{
  for(const mode of ['composing','229','just-ended']){
    const {field,sent}=fixture();
    if(mode!=='229'){field.dispatchEvent(new Event('compositionstart'));field.value='日本語';field.setSelectionRange(3,3);}
    if(mode==='just-ended')field.dispatchEvent(new Event('compositionend'));
    sent.length=0;
    const result=key(field,'Escape',mode==='229'?{keyCode:229}:mode==='composing'?{isComposing:true}:{});
    assert.equal(result.stopped,true);assert.equal(document.activeElement,field);assert.deepEqual(types(sent),[]);
  }
});
test('ordinary blur flushes the raw snapshot before blur',()=>{
  const {field,sent}=fixture({multiline:false});sent.length=0;field.value='377.5';field.setSelectionRange(5,5);field.blur();
  assert.deepEqual(types(sent),['text-input','text-blur']);assert.equal(sent[0].text,'377.5');assert.equal(sent[0].selectionOnly,false);
});
test('Enter transports one raw snapshot and one explicit Enter without publication through blur',()=>{
  for(const multiline of [false,true]){
    const {field,sent}=fixture({multiline});sent.length=0;field.value='Sound';field.setSelectionRange(5,5);
    key(field,'Enter');field.dispatchEvent(Object.assign(new Event('beforeinput',{cancelable:true}),{inputType:'insertParagraph'}));
    assert.deepEqual(types(sent),['text-input','text-enter']);
    assert.equal(document.activeElement,multiline?field:null);
  }
});
function twoFields(){
  const f=fixture({multiline:false});
  const make=(revision,focused=42)=>{
    const frame=f.frame('hello',revision),d=frame.inputs[0];
    frame.inputs=[{...d,focused:focused===42},{...d,id:17,value:'width',cursor:2,anchor:4,focused:focused===17}];
    return frame;
  };
  // Flush the native focus snapshot so the next projection is acknowledged.
  f.field.dispatchEvent(new Event('input'));f.bridge.accept(make(f.sent.at(-1).inputRevision));
  return {...f,make,next:f.bridge.fields.get('7:17').input};
}
test('Tab and ShiftTab flush raw text then direction; acknowledged core focus overrides DOM order',()=>{
  for(const backward of [false,true]){
    const {bridge,field,next,sent,make}=twoFields();sent.length=0;
    field.value='Sound';field.setSelectionRange(5,5);
    assert.deepEqual(key(field,'Tab',{shiftKey:backward}),{prevented:true,stopped:true});
    assert.deepEqual(types(sent),['text-input','text-tab']);assert.equal(sent[1].backward,backward);
    assert.equal(document.activeElement,field);
    const revision=sent[1].inputRevision;
    bridge.accept(make(revision-1,17));assert.equal(document.activeElement,field);assert.equal(field.value,'Sound');
    bridge.accept(make(revision,17));assert.equal(document.activeElement,next);
    assert.deepEqual(types(sent),['text-input','text-tab'],'core focus synchronization emits no redundant focus/blur/Replace');
    assert.equal(next.selectionStart,2);assert.equal(next.selectionEnd,4);
    bridge.accept(make(revision-1,42));assert.equal(document.activeElement,next);
  }
});
test('focus reconciliation preserves composition, queued native selection, and external focus',()=>{
  const {bridge,field,next,sent,make}=twoFields();field.dispatchEvent(new Event('compositionstart'));
  bridge.accept(make(sent.at(-1).inputRevision,17));assert.equal(document.activeElement,field);
  field.dispatchEvent(new Event('compositionend'));bridge.accept(make(sent.at(-1).inputRevision,17));assert.equal(document.activeElement,next);
  next.setSelectionRange(0,1);const old=sent.at(-1).inputRevision;
  bridge.accept(make(old,42));assert.equal(document.activeElement,next);assert.equal(next.selectionEnd,1);
  next.blur();const external=new Field();external.focus();bridge.accept(make(sent.at(-1).inputRevision,42));assert.equal(document.activeElement,external);
});
test('cancelled and retired fields cannot be restored by old projections or retarget late DOM events',()=>{
  for(const epoch of [7,8]){
    const {bridge,field,sent,frame}=fixture({multiline:false});key(field,'Escape');
    const revision=sent.at(-1).inputRevision;
    const removed={...frame('hello',revision),epoch,inputs:epoch===7?[]:[{...frame().inputs[0],value:'new epoch',focused:true}],serial:20n};
    bridge.accept(removed);assert.equal(field.removed,true);
    const count=sent.length;
    field.value='late';field.dispatchEvent(new Event('input'));field.dispatchEvent(new Event('compositionend'));field.focus();key(field,'Tab');key(field,'Escape');
    assert.equal(sent.length,count);
    bridge.accept({...frame('old',revision),serial:19n});
    assert.equal(bridge.fields.has('7:42'),false);
    if(epoch===8)assert.equal(bridge.fields.get('8:42').input.value,'new epoch');
  }
});
test('same-revision stale frame cannot reverse acknowledged core focus',()=>{
  const {bridge,field,next,sent,make}=twoFields();key(field,'Tab');const revision=sent.at(-1).inputRevision;
  bridge.accept({...make(revision,17),serial:12n});assert.equal(document.activeElement,next);
  bridge.accept({...make(revision,42),serial:11n});assert.equal(document.activeElement,next);
});
test('secret single-line controls retain browser password and iOS font/scale attributes',()=>{
  const {field}=fixture({secret:true,multiline:false});assert.equal(field.type,'password');assert.equal(field.autocomplete,'current-password');assert.equal(field.spellcheck,false);
  assert.equal(field.attributes.autocapitalize,'none');assert.equal(field.style.fontSize,'22.5px');assert.equal(field.style.transform,'scale(0.4875)');
});

test('IME Tab remains native and ShiftEnter retains multiline behavior',()=>{
  const {field,sent}=fixture();sent.length=0;field.dispatchEvent(new Event('compositionstart'));
  assert.deepEqual(key(field,'Tab',{isComposing:true}),{prevented:false,stopped:false});assert(!sent.some(m=>m.type==='text-tab'));
  field.dispatchEvent(new Event('compositionend'));sent.length=0;
  assert.equal(key(field,'Enter',{shiftKey:true}).prevented,false);
  const event=Object.assign(new Event('beforeinput',{cancelable:true}),{inputType:'insertLineBreak'});field.dispatchEvent(event);
  assert.equal(event.defaultPrevented,false);assert.deepEqual(types(sent),[]);
});
test('desired focus never crosses epochs or focuses a disabled or missing surface',()=>{
  for(const mode of ['epoch','disabled','missing']){
    const {bridge,field,sent,make}=twoFields();key(field,'Tab');const revision=sent.at(-1).inputRevision;
    const frame=make(revision,17);
    if(mode==='epoch')frame.epoch=8;
    if(mode==='disabled')frame.authRequired=true;
    if(mode==='missing')frame.inputs=frame.inputs.filter(d=>d.id!==17);
    bridge.accept(frame);
    assert.notEqual(document.activeElement,bridge.fields.get(`${frame.epoch}:17`)?.input);
  }
});
