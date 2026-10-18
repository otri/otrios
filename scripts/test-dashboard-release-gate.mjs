// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {awaitRelease,validRelease} from '../web/dashboard/release-gate.mjs';
const release=c=>({id:c.repeat(64),web_entry:`/releases/${c.repeat(64)}/web/index.html`});
test('lagging code host does not switch a running runtime/state session',async()=>{
  let reads=0,liveEdits=0,switched=false;
  const ready=awaitRelease(release('b'),{read:async()=>++reads<3?release('a'):release('b'),wait:async()=>{assert.equal(switched,false);liveEdits++;}}).then(v=>{switched=v;return v;});
  assert.equal(await ready,true);assert.equal(liveEdits,2);assert.equal(reads,3);
});
test('superseded release never switches after a late network response',async()=>{
  let current=true;
  assert.equal(await awaitRelease(release('a'),{isCurrent:()=>current,read:async()=>{current=false;return release('a');},wait:async()=>assert.fail('obsolete release must not retry')}),false);
});
test('temporary host failure preserves the session and retries',async()=>{
  let attempts=0;
  assert.equal(await awaitRelease(release('b'),{read:async()=>{if(++attempts===1)throw Error('offline');return release('b');},wait:async()=>{}}),true);
  assert.equal(attempts,2);
});
test('untrusted entry URLs and aborted requests cannot trigger navigation',async()=>{
  const invalid={...release('a'),web_entry:'https://unrelated.example/code'};
  assert.equal(validRelease(invalid),false);await assert.rejects(awaitRelease(invalid));
  const controller=new AbortController();controller.abort();
  assert.equal(await awaitRelease(release('a'),{signal:controller.signal,read:async()=>assert.fail('aborted request')}),false);
});
