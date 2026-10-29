import test from 'node:test';
import assert from 'node:assert/strict';
import { createSendController } from '../src/send-controller.mjs';
const deferred = () => { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return {promise,resolve,reject}; };
function setup(acceptMessage, prewarm = async () => {}) {
  let ids=0; const shown=[];
  const ui=createSendController({ bridge:{acceptMessage,prewarm},
    createIntent: content => ({ request_id:`r${++ids}`,message_id:`m${ids}`,content }),
    onAccepted: receipt => shown.push(receipt) });
  return {ui,shown};
}
test('local confirmation does not wait for network prewarm', async () => {
  const never=deferred(); const {ui,shown}=setup(async x=>x,()=>never.promise);
  ui.prewarm(); ui.setDraft('hello'); await ui.send();
  assert.equal(ui.snapshot().draft,''); assert.equal(shown.length,1);
});
test('double click joins one save; later typing is preserved', async () => {
  const gate=deferred(); let calls=0, intent;
  const {ui}=setup(x=>{calls++;intent=x;return gate.promise;});
  ui.setDraft('first'); const a=ui.send(), b=ui.send(); assert.equal(a,b);
  await Promise.resolve(); ui.setDraft('second'); gate.resolve(intent); await a;
  assert.equal(calls,1); assert.equal(ui.snapshot().draft,'second');
});
test('unconfirmed IPC response keeps draft and retries same identity', async () => {
  const seen=[]; const {ui}=setup(async x=>{seen.push(x);if(seen.length===1) throw Error('IPC');return x;});
  ui.setDraft('hello'); await assert.rejects(ui.send());
  assert.equal(ui.snapshot().draft,'hello'); await ui.send();
  assert.equal(seen[0].request_id,seen[1].request_id);
});
test('editing after failure generates a new identity', async () => {
  const seen=[]; const {ui}=setup(async x=>{seen.push(x);if(seen.length===1)throw Error('disk');return x;});
  ui.setDraft('first'); await assert.rejects(ui.send()); ui.setDraft('changed'); await ui.send();
  assert.notEqual(seen[0].request_id,seen[1].request_id);
});
test('wrong local receipt does not clear draft or append to view', async () => {
  const {ui,shown}=setup(async ()=>({request_id:'wrong',message_id:'wrong'}));
  ui.setDraft('hello'); await assert.rejects(ui.send(), /LOCAL_RECEIPT_MISMATCH/);
  assert.equal(ui.snapshot().draft,'hello'); assert.equal(shown.length,0);
});
test('prewarm failure is silent; empty send creates nothing', async () => {
  let calls=0; const {ui}=setup(async x=>{calls++;return x;},async()=>{throw Error('offline');});
  await ui.prewarm(); assert.equal(ui.snapshot().errorKey,null);
  assert.equal(await ui.send(),null); assert.equal(calls,0);
});
