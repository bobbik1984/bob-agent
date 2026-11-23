import test from 'node:test';
import assert from 'node:assert/strict';
import {createPreviewCache} from '../src/dsh-preview-cache.mjs';
const a={connectionId:'c',sessionId:'s',path:'a.md',cursor:1};
const file=a=>({connection_id:a.connectionId,session_id:a.sessionId,path:a.path,bytes:3,data:'YWJj'});
test('prefetch and click share one request; cursor and session isolate cache',async()=>{let count=0;const cache=createPreviewCache(async a=>{count++;return file(a)});await Promise.all([cache.get(a),cache.get(a)]);await cache.get(a);assert.equal(count,1);await cache.get({...a,cursor:2});await cache.get({...a,sessionId:'other'});assert.equal(count,3)});
test('cache evicts to byte and entry ceilings',async()=>{const c=createPreviewCache(async a=>file(a),{maxBytes:8,maxEntries:1});await c.get(a);await c.get({...a,path:'b.md'});assert.deepEqual(c.stats(),{bytes:8,entries:1,pending:0});c.clear();assert.equal(c.stats().bytes,0)});
test('failure and identity mismatch are not cached; retry is explicit',async()=>{let calls=0;const c=createPreviewCache(async a=>{calls++;if(calls===1)throw Error('offline');return {...file(a),session_id:'wrong'}});await assert.rejects(c.get(a));await assert.rejects(c.get(a),/IDENTITY/);assert.equal(c.stats().entries,0);assert.equal(c.stats().pending,0)});
test('clearing during fetch does not repopulate cache',async()=>{let release;const c=createPreviewCache(a=>new Promise(r=>{release=()=>r(file(a))}));const p=c.get(a);c.clear();release();await p;assert.equal(c.stats().entries,0)});
