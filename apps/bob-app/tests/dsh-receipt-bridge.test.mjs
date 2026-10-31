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
