// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {parseRoute,readRoute,routePath,navigationUrl,moduleRelease,Navigation} from '../web/shared/navigation.mjs';
test('screen links have canonical paths and legacy hashes normalize once',()=>{
  for(const screen of ['home','component-playground','input-lab','monitoring'])assert.equal(parseRoute(routePath(screen)),screen);
  assert.equal(parseRoute('/'),'home');assert.equal(parseRoute('#/controls'),'component-playground');assert.equal(parseRoute('#/keyboard/input'),'input-lab');
  assert.equal(navigationUrl(new URL('https://demo.example/releases/'+ 'a'.repeat(64)+'/web/index.html?font-probe=1#/layout')),'/layout-lab?font-probe=1');
  for(const path of ['/input-lab/input','/monitoring/presentation','//evil.example','/../home','/%2fhome','/a?b','/a#b'])assert.equal(parseRoute(path),null);
});
test('module identity stays independent of the visible screen URL',()=>{
  const id='a'.repeat(64);assert.equal(moduleRelease(`https://demo.example/releases/${id}/web/app.js`),id);
  assert.equal(moduleRelease('http://localhost/app.js'),null);assert.equal(readRoute(new URL('https://demo.example/input-lab')),'input-lab');
});
function historyAt(url){
  const location=new URL(url),entries=[url];let index=0;
  const history={replaceState(_s,_t,path){entries[index]=new URL(path,location).href;location.href=entries[index];},pushState(_s,_t,path){entries.splice(++index);entries[index]=new URL(path,location).href;location.href=entries[index];}};
  return {location,history,entries,travel(delta){index+=delta;location.href=entries[index];}};
}
test('initial deep links wait for their screen; subsequent screen changes push and pop restores without another entry',()=>{
  const h=historyAt('https://demo.example/input-lab?font-probe=1'),n=new Navigation(h.location,h.history);
  n.present('home',false);n.present('home',true);assert.equal(h.location.pathname,'/input-lab');
  n.present('input-lab',true);n.present('input-lab',true);assert.equal(h.entries.length,1);
  n.present('home',true);n.present('layout-lab',true);assert.equal(h.entries.length,3);
  h.travel(-1);assert.equal(n.restore(),'/home');n.present('layout-lab',true);n.present('home',true);assert.equal(h.entries.length,3);
  h.travel(1);assert.equal(n.restore(),'/layout-lab');n.present('layout-lab',true);assert.equal(h.entries.length,3);assert.equal(h.location.search,'?font-probe=1');
});
test('legacy normalization and same-screen presentation preserve one history entry',()=>{
  const h=historyAt('https://demo.example/#/keyboard/input'),n=new Navigation(h.location,h.history);
  n.present('input-lab',true);assert.equal(h.location.href,'https://demo.example/input-lab');
  for(let i=0;i<10;i++)n.present('input-lab',true);assert.equal(h.entries.length,1);
});
