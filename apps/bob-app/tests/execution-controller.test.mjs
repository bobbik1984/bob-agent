import test from 'node:test';
import assert from 'node:assert/strict';
import { createExecutionController, canCancelExecution, executionStatusKey, singleFlight, blocksNewExecution } from '../src/execution-controller.mjs';
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
test('new durable receipt dispatches once; uncertain acknowledgement never auto retries',async()=>{
 let calls=0;const c=createExecutionController({execute:async()=>{calls++;throw Error('lost IPC');}});
 const a=c.dispatch({request_id:'r'}),b=c.dispatch({request_id:'r'});
 assert.equal(a,b);await assert.rejects(a);await assert.rejects(c.dispatch({request_id:'r'}));assert.equal(calls,1);
});
test('successful dispatch joins duplicate clicks and does not accept another request result',async()=>{
 let calls=0;const gate=deferred();const c=createExecutionController({execute:()=>{calls++;return gate.promise;}});
 const a=c.dispatch({request_id:'r'}),b=c.dispatch({request_id:'r'});assert.equal(a,b);
 gate.resolve({id:'e',request_id:'r',status:'running'});assert.equal((await a).status,'running');assert.equal(calls,1);
 const wrong=createExecutionController({execute:async()=>({id:'e',request_id:'other'})});await assert.rejects(wrong.dispatch({request_id:'r'}),/MISMATCH/);
});
test('read refresh has one flight, including repeated timer ticks, and never invokes execution',async()=>{
 const gate=deferred();let calls=0;const refresh=singleFlight(()=>{calls++;return gate.promise;});
 const a=refresh();for(let i=0;i<20;i++)assert.equal(refresh(),a);
 await Promise.resolve();assert.equal(calls,1);gate.resolve([]);await a;
 await refresh();assert.equal(calls,2);
});
test('stop acknowledgement is not stop completion; terminal and unknown cannot be cancelled again',async()=>{
 const gate=deferred();let calls=0;const c=createExecutionController({cancel:()=>{calls++;return gate.promise;}});
 const e={id:'e',status:'running',stop_requested:false};const a=c.cancel(e),b=c.cancel(e);assert.equal(a,b);
 gate.resolve({...e,stop_requested:true});const result=await a;assert.equal(calls,1);
 assert.equal(executionStatusKey(result),'executionStopRequested');assert.equal(canCancelExecution(result),false);
 assert.equal(executionStatusKey({...e,status:'cancelled',stop_requested:true}),'executionCancelled');
 assert.equal(executionStatusKey({...e,status:'unknown',stop_requested:true}),'executionUnknown');
 assert.equal(executionStatusKey({...e,status:'failed',error:'timeout'}),'executionTimeout');
 await assert.rejects(c.cancel({...e,status:'unknown'}),/NOT_CANCELLABLE/);
});
test('failed polling is released for a later read without any execution retry',async()=>{
 let calls=0;const refresh=singleFlight(async()=>{if(++calls===1)throw Error('read failed');return ['persisted'];});
 await assert.rejects(refresh());assert.deepEqual(await refresh(),['persisted']);assert.equal(calls,2);
});

test('live or unresolved execution blocks a second send, terminal records do not',()=>{
 for(const status of ['pending','dispatching','running','paused','unknown'])assert.equal(blocksNewExecution([{status}]),true);
 assert.equal(blocksNewExecution([{status:'completed'},{status:'failed'},{status:'cancelled'}]),false);
 assert.equal(blocksNewExecution([]),false);
});
