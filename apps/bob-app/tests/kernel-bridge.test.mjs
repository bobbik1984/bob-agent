import test from 'node:test';
import assert from 'node:assert/strict';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { createSendController } from '../src/send-controller.mjs';
test('IPC maps stable identities and payload without inventing remote acceptance',async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {request_id:args?.requestId,message_id:args?.messageId};},()=>true);
 await bridge.bootstrap();await bridge.history('session');await bridge.prewarm();const receipt=await bridge.acceptMessage({request_id:'request',message_id:'message',session_id:'session',content:'text'});
 assert.deepEqual(calls.map(x=>x.command),['app_bootstrap','app_history','app_prewarm','app_accept']);assert.deepEqual(calls[3].args,{requestId:'request',messageId:'message',sessionId:'session',content:'text'});assert.equal(receipt.request_id,'request');assert.equal(receipt.accepted,undefined);
});
test('browser without native host cannot silently save in another store',async()=>{
 let invoked=0;const bridge=createKernelBridge(()=>{invoked++;},()=>false);
 for(const action of [()=>bridge.bootstrap(),()=>bridge.history('s'),()=>bridge.acceptMessage({}),()=>bridge.prewarm()])await assert.rejects(action(),/NATIVE_HOST_REQUIRED/);
 assert.equal(invoked,0);
});
test('lost native response retains draft and same request identity through bridge',async()=>{
 const ids=[];let generated=0;const bridge=createKernelBridge(async(command,args)=>{assert.equal(command,'app_accept');ids.push(args.requestId);if(ids.length===1)throw Error('IPC_LOST');return {request_id:args.requestId,message_id:args.messageId};},()=>true);
 const ui=createSendController({bridge,createIntent:content=>({request_id:`r${++generated}`,message_id:'m',session_id:'s',content}),onAccepted:()=>{}});
 ui.setDraft('hello');await assert.rejects(ui.send());assert.equal(ui.snapshot().draft,'hello');await ui.send();assert.deepEqual(ids,['r1','r1']);assert.equal(ui.snapshot().draft,'');
});

test('execution IPC passes identities without a renderer-supplied command or result', async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {};},()=>true);
 await bridge.execute('request');await bridge.executions('session');await bridge.cancel('execution');
 assert.deepEqual(calls,[{command:'app_execute',args:{requestId:'request'}},{command:'app_executions',args:{sessionId:'session'}},{command:'app_cancel',args:{executionId:'execution'}}]);
});
test('execution commands require the native host too',async()=>{
 const bridge=createKernelBridge(()=>assert.fail('must not invoke'),()=>false);
 for(const action of [()=>bridge.execute('r'),()=>bridge.executions('s'),()=>bridge.cancel('e')])await assert.rejects(action(),/NATIVE_HOST_REQUIRED/);
});
