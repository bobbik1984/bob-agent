import test from 'node:test';
import assert from 'node:assert/strict';
import {nodeAddress,nodeClient,invitationUri} from '../src/node-connection.mjs';
test('address defaults to TLS and refuses embedded secrets or paths',()=>{
 assert.equal(nodeAddress('worker.example'),'https://worker.example');
 for(const value of ['https://user:secret@worker.example','https://worker.example/path','https://worker.example?token=x','http://worker.example']) assert.throws(()=>nodeAddress(value));
 assert.equal(nodeAddress('http://127.0.0.1:9527'),'http://127.0.0.1:9527');
});
test('credentials never follow redirects or enter request URL',async()=>{
 let observed;
 const api=nodeClient('worker.example',async(url,options)=>{observed={url,options};return {ok:true,json:async()=>({protocol:'bob-node-v1'})};});
 await api('login',{password:'test-only-password'});
 assert.equal(observed.options.redirect,'error');assert.equal(observed.options.cache,'no-store');
 assert.equal(observed.url,'https://worker.example/api/node/v1/login');
 assert.equal(JSON.parse(observed.options.body).password,'test-only-password');
});
test('rate limit is distinguished; arbitrary server body is not shown',async()=>{
 const api=nodeClient('worker.example',async()=>({ok:false,status:429}));
 await assert.rejects(api('login',{}),/NODE_RATE_LIMITED/);
});
test('QR contains a one-use invitation, never the owner password or device bearer',()=>{
 const code=invitationUri('worker.example',{pairing_id:'invitation',user_code:'one-use',node_id:'stable-node',expires_at:123});
 const params=new URLSearchParams(code.split('?')[1]);assert.equal(params.get('id'),'stable-node');assert.equal(params.get('c'),'one-use');assert.equal(params.get('u'),'https://worker.example');
});
