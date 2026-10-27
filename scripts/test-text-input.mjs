// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {TextInputs,selection} from '../web/shared/text-input.mjs';
class Field extends EventTarget {
  constructor(){super();this.value='';this.style={};this.dataset={};this.selectionStart=this.selectionEnd=0;this.selectionDirection='none';}
  setAttribute(){} remove(){this.removed=true;} blur(){globalThis.document.activeElement=null;this.dispatchEvent(new Event('blur'));}
  focus(){globalThis.document.activeElement=this;this.dispatchEvent(new Event('focus'));}
  setSelectionRange(start,end,direction='none'){this.selectionStart=start;this.selectionEnd=end;this.selectionDirection=direction;}
}
function fixture(){
  globalThis.document=Object.assign(new EventTarget(),{createElement:()=>new Field(),activeElement:null});
  globalThis.ResizeObserver=class {observe(){}};
  const sent=[],layer={append(){},getBoundingClientRect:()=>({width:390})};
  const bridge=new TextInputs(layer,m=>sent.push(m));
  const d={id:42,value:'hello',cursor:5,anchor:5,multiline:true,max_bytes:2048,bounds:[0,0,700,60],inset:[12,12,12,12],font_size:22.5,line_height:26.4};
  const frame=(value=d.value,revision=0)=>({epoch:7,extent:[800,1280],controls:[{id:42,label:'Composer'}],inputs:[{...d,value,cursor:value.length,anchor:value.length}],inputRevision:revision});
  bridge.accept(frame());const field=bridge.fields.get('7:42').input;field.focus();
  return {bridge,field,sent,frame};
}
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
