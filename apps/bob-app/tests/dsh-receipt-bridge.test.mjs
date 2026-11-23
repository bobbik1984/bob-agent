import test from 'node:test';
import assert from 'node:assert/strict';
import {createKernelBridge} from '../src/kernel-bridge.mjs';
test('receipt check preserves original address and never sends a prompt', async()=>{
 const calls=[];const bridge=createKernelBridge(async(c,a)=>{calls.push({c,a});return {status:'unknown'}},()=>true);
 assert.equal((await bridge.dshReceipt('device','original','text','request')).status,'unknown');
 assert.deepEqual(calls,[{c:'app_dsh_receipt',a:{connectionId:'device',sessionId:'original',text:'text',requestId:'request'}}]);
});
test('receipt check cannot silently use a browser substitute', async()=>{
 const bridge=createKernelBridge(()=>{throw Error('must not call')},()=>false);
 await assert.rejects(bridge.dshReceipt('c','s','t','r'),/NATIVE_HOST_REQUIRED/);
});
test('dshPrompt forwards takeover and approvalMode options cleanly', async()=>{
 const calls=[];const bridge=createKernelBridge(async(c,a)=>{calls.push({c,a});return {status:'accepted'}},()=>true);
 await bridge.dshPrompt('conn-1','claude-jsonl:s1','hello','req-1',null,{takeover:true,approvalMode:'interactive'});
 assert.deepEqual(calls,[{c:'app_dsh_prompt',a:{connectionId:'conn-1',sessionId:'claude-jsonl:s1',text:'hello',requestId:'req-1',takeover:true,approvalMode:'interactive'}}]);
});
test('dshAnswer forwards callId and answers cleanly', async()=>{
 const calls=[];const bridge=createKernelBridge(async(c,a)=>{calls.push({c,a});return {ok:true}},()=>true);
 const answers=[{id:'q1',selected:['opt-a']}];
 await bridge.dshAnswer('conn-1','dsh-s1','call-1',answers);
 assert.deepEqual(calls,[{c:'app_dsh_answer',a:{connectionId:'conn-1',sessionId:'dsh-s1',callId:'call-1',answers}}]);
});
