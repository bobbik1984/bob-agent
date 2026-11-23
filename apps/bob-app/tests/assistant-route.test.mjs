import test from 'node:test';
import assert from 'node:assert/strict';
import {resolveAssistantRoute as route} from '../src/assistant-route.mjs';
const target={connectionId:'c',sessionId:'s',available:true};
test('discussion never dispatches',()=>assert.equal(route({intent:'discuss',proposal:target,authorized:true,candidates:[target]}).kind,'reply'));
test('observed identity and permission are required',()=>{const a={intent:'execute',proposal:target,candidates:[target]};assert.equal(route(a).kind,'approval_required');assert.equal(route({...a,authorized:true}).kind,'dispatch');assert.equal(route({...a,candidates:[]}).kind,'clarify');assert.equal(route({...a,candidates:[target,target]}).kind,'clarify');assert.equal(route({...a,candidates:[{...target,available:false}]}).kind,'unavailable');});
test('direct selection is never overwritten by proposal',()=>assert.deepEqual(route({mode:'direct',direct:target,proposal:{connectionId:'wrong',sessionId:'wrong'}}).target,target));
