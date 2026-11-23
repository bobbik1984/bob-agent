import test from 'node:test';
import assert from 'node:assert/strict';
import {checkPath,pathHealth,pathState} from '../src/path-health.mjs';
test('reading success is bounded to the actual path and expires',async()=>{
 const node={id:'one',baseUrl:'https://one.example'};
 assert.equal(pathState(node),'unverified');
 await checkPath(node,{dshList:async id=>{assert.equal(id,node.id);return [];}});
 assert.equal(pathState(node),'readable');
 assert.equal(pathState({...node,baseUrl:'https://other.example'}),'unverified');
 assert.equal(pathState(node,pathHealth.one.checkedAt+60_001),'unverified');
});
test('authentication failure clears success without changing sibling path state or sending messages',async()=>{
 const first={id:'first',baseUrl:'https://one.example'},second={id:'second',baseUrl:'https://two.example'};
 const bridge={dshList:async()=>[],dshPrompt:()=>assert.fail('must not send')};
 await checkPath(first,bridge);await checkPath(second,bridge);
 await assert.rejects(checkPath(first,{...bridge,dshList:async()=>{throw Error('DSH_AUTH_REJECTED')}}));
 assert.equal(pathState(first),'failed');assert.equal(pathState(second),'readable');
});
test('an older response cannot override a newer probe',async()=>{
 const node={id:'race',baseUrl:'https://one.example'};let fail;
 const old=checkPath(node,{dshList:()=>new Promise((_,reject)=>{fail=reject})});
 const rejected=assert.rejects(old);
 await checkPath(node,{dshList:async()=>[]});fail(Error('offline'));await rejected;
 assert.equal(pathState(node),'readable');
});
