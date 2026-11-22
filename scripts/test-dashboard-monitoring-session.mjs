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
test('a revoked server session notifies its owner to revoke the private mount',async()=>{
  let authorized=true;const h=harness(async()=>reply(authorized,authorized?200:401));
  await h.session.refresh();authorized=false;await h.session.refresh();assert.equal(h.session.authenticated,false);assert.deepEqual(h.states.at(-1),{authenticated:false});
});
test('temporary status failures cannot turn a confirmed login into a sign-out',async()=>{
  for(const status of [429,500,502,503,504]){
    let failure=false;const h=harness(async()=>failure?reply(false,status):reply(true));
    await h.session.refresh();failure=true;
    await assert.rejects(h.session.refresh(),/temporarily unavailable/);
    assert.equal(h.session.authenticated,true);assert.deepEqual(h.states,[{authenticated:true}]);
  }
});
test('unavailable status does not grant an anonymous session access',async()=>{
  const h=harness(async()=>reply(false,503));
  await assert.rejects(h.session.refresh(),/temporarily unavailable/);
  assert.equal(h.session.authenticated,false);assert.deepEqual(h.states,[]);
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

test('discovery refresh cannot reset a pending first sign-in',async()=>{
  let finish;const h=harness(async(_,options)=>options.method==='POST'?new Promise(resolve=>finish=resolve):reply(true));
  const login=h.session.login('user','secret');await Promise.resolve();
  await h.session.refresh();assert.equal(h.calls.length,1);assert.deepEqual(h.states.at(-1),{authenticated:false,pending:true});
  finish(reply(true));await login;assert.equal(h.session.authenticated,true);assert.deepEqual(h.calls.map(c=>c.method||'GET'),['POST','GET']);
});

test('persistent browser identity readiness is reported without granting account access',async()=>{
  const h=harness(async()=>({ok:false,status:401,json:async()=>({monitoring:false,browser_identity:true})}));
  await h.session.refresh();
  assert.equal(h.session.authenticated,false);assert.equal(h.session.browserIdentity,true);
  assert.deepEqual(h.states,[{authenticated:false,browserIdentity:true}]);
});

test('worker retries older-authority identity discovery and outages before allowing a managed guest join',async()=>{
  const {readFileSync}=await import('node:fs'),{runInNewContext}=await import('node:vm');
  const source=readFileSync(new URL('../web/canvas/state-worker.js',import.meta.url),'utf8');
  const functionSource=name=>{const start=source.indexOf(`function ${name}(`);assert.ok(start>=0);return source.slice(start,source.indexOf('\nfunction ',start+1));};
  const timers=[],sent=[];let attempts=0,joins=0;
  const context={stopped:false,browserIdentityReady:false,browserIdentityRefreshPending:false,browserIdentityRetry:null,browserIdentityDelay:1000,
    transportReady:true,transportEpoch:1,chatSending:false,chatIdentityReady:false,chatIdentityRequested:false,
    app:{space_managed:()=>true,screen:()=> 'brass-relay',chat_outgoing:()=>{joins++;return new Uint8Array([1]);}},
    postMessage:message=>sent.push(message),setTimeout:(callback,delay)=>{timers.push({callback,delay});return timers.length;},Math};
  context.monitoring=new MonitoringSession({fetcher:async()=>{
    attempts++;if(attempts===1)throw Error('authority restarting');
    return{ok:false,status:401,json:async()=>({monitoring:false,...(attempts>=10?{browser_identity:true}:{})})};
  },onStatus:state=>{context.browserIdentityReady=!!state.browserIdentity;}});
  runInNewContext(`${functionSource('refreshBrowserIdentity')}\n${functionSource('pumpChat')}`,context);
  const flush=async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
  context.refreshBrowserIdentity();context.refreshBrowserIdentity();
  assert.equal(attempts,1,'only one discovery request may be in flight');
  await flush();
  while(!context.browserIdentityReady){
    context.pumpChat();assert.equal(joins,0,'no guest is minted before its cookie is confirmed');
    assert.equal(timers.length,1,'one paced retry is queued');
    const timer=timers.shift();assert.ok(timer.delay>=1000&&timer.delay<=30000);
    timer.callback();context.refreshBrowserIdentity();await flush();
  }
  assert.equal(attempts,10);assert.equal(timers.length,0);
  context.refreshBrowserIdentity();await flush();assert.equal(attempts,10,'readiness stops discovery retries');
  context.pumpChat();assert.equal(joins,1);assert.equal(sent.at(-1).type,'chat-send');
  assert.equal(context.monitoring.authenticated,false,'browser readiness never grants account access');
});
