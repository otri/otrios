// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {MonitoringSession} from '../web/shared/monitoring-session.mjs';
const reply=(monitoring,status=200)=>({ok:status===200,status,json:async()=>({monitoring})});
function harness(fetcher){const states=[],calls=[];const session=new MonitoringSession({fetcher:(path,options)=>{calls.push({path,...options});return fetcher(path,options);},onStatus:s=>states.push(s)});return{session,states,calls};}

test('anonymous session checks use only the uncached cookie endpoint',async()=>{
  const h=harness(async()=>reply(false,401));await h.session.refresh();
  assert.equal(h.session.authenticated,false);assert.deepEqual(h.states,[{authenticated:false}]);
  assert.deepEqual(h.calls,[{path:'/auth/session',credentials:'same-origin',cache:'no-store'}]);
});
test('sign-in requires a successful mutation and an authoritative session check',async()=>{
  const h=harness(async(_,options)=>reply(options.method!=='POST'));await h.session.login('user','secret');
  assert.equal(h.session.authenticated,true);assert.deepEqual(h.calls.map(c=>c.method||'GET'),['POST','GET']);
  assert.deepEqual(JSON.parse(h.calls[0].body),{username:'user',password:'secret'});
  assert.equal(h.calls[0].headers['X-OTriOS-Request'],'session');assert.equal(h.calls[0].credentials,'same-origin');
  assert.deepEqual(h.states,[{authenticated:false,pending:true},{authenticated:true}]);
});
test('failed and rate-limited sign-in stay unauthorized and report the failure',async()=>{
  for(const status of [401,429]){const h=harness(async()=>reply(false,status));await h.session.login('user','wrong');assert.equal(h.session.authenticated,false);assert.equal(h.calls.length,1);assert.match(h.states.at(-1).error,status===429?/wait/i:/sign-in failed/i);}
});
test('logout revokes local permission immediately while the cookie mutation is pending',async()=>{
  let finish;const h=harness(async(_,options)=>options.method==='DELETE'?new Promise(resolve=>finish=resolve):reply(true));
  await h.session.refresh();const logout=h.session.logout();assert.equal(h.session.authenticated,false);assert.deepEqual(h.states.at(-1),{authenticated:false,pending:true});
  await Promise.resolve();finish(reply(false));await logout;assert.deepEqual(h.states.at(-1),{authenticated:false});
});
test('late login is followed by serialized cookie revocation and cannot restore permission',async()=>{
  let finish;const h=harness((_,options)=>options.method==='POST'?new Promise(r=>finish=r):Promise.resolve(reply(false)));
  const login=h.session.login('user','secret');await Promise.resolve();const logout=h.session.logout();finish(reply(true));await Promise.all([login,logout]);
  assert.deepEqual(h.calls.map(c=>c.method),['POST','DELETE']);assert.equal(h.session.authenticated,false);
});
test('an expired server session notifies its owner to revoke the private mount',async()=>{
  let authorized=true;const h=harness(async()=>reply(authorized,authorized?200:401));
  await h.session.refresh();authorized=false;await h.session.refresh();assert.equal(h.session.authenticated,false);assert.deepEqual(h.states.at(-1),{authenticated:false});
});
test('a delayed session body cannot restore access after logout',async()=>{
  let finish;const h=harness(async(_,options)=>options.method?reply(false):({ok:true,json:()=>new Promise(resolve=>finish=resolve)}));
  const refresh=h.session.refresh();await Promise.resolve();await h.session.logout();finish({monitoring:true});await refresh;assert.equal(h.session.authenticated,false);
});
test('an older refresh cannot replace a newer revocation response',async()=>{
  const pending=[];const h=harness(()=>new Promise(resolve=>pending.push(resolve)));
  const old=h.session.refresh(),recent=h.session.refresh();pending[1](reply(false,401));await recent;pending[0](reply(true));await old;
  assert.equal(h.session.authenticated,false);assert.deepEqual(h.states,[{authenticated:false}]);
});
test('failed server logout remains locally revoked and provides a retry error',async()=>{
  const h=harness(async(_,options)=>{if(options.method==='DELETE')throw Error('offline');return reply(true);});
  await h.session.refresh();await h.session.logout();assert.equal(h.session.authenticated,false);assert.match(h.states.at(-1).error,/offline/);
});
