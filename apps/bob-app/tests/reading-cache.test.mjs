import test from 'node:test';
import assert from 'node:assert/strict';
import {createReadingCache,readingKey,singleFlight} from '../src/reading-cache.mjs';
import {createPersistentPreviewCache} from '../src/persistent-preview-cache.mjs';
test('bounded LRU, expiration, scoped removal, oversized rejection',async()=>{
 let time=1;const c=createReadingCache({storage:null,now:()=>time,limits:{bytes:100,entries:2,age:100}});
 await c.put('a','first');time++;await c.put('b','second');time++;await c.get('a');time++;await c.put('c','third');
 assert.equal(await c.get('b'),null);assert.equal(await c.get('a'),'first');
 await c.put('huge','x'.repeat(200));assert.equal(await c.get('huge'),null);assert.ok((await c.stats()).bytes<=100);
 await c.removePrefix('a');assert.equal(await c.get('a'),null);time=200;assert.equal(await c.get('c'),null);
});
test('single flight joins simultaneous reads and releases errors',async()=>{let calls=0;const load=async()=>{calls++;return 1};assert.deepEqual(await Promise.all([singleFlight('same',load),singleFlight('same',load)]),[1,1]);assert.equal(calls,1);await assert.rejects(singleFlight('bad',async()=>{throw Error('offline')}));assert.equal(await singleFlight('bad',load),1)});
test('persisted files survive view remount, validate versions without another download',async()=>{
 let time=0,calls=0,downloads=0;const store=createReadingCache({storage:null});const a={connectionId:'c',sessionId:'s',path:'a.txt',cursor:1,endpoint:'https://example.invalid'};
 const fetch=async(a,known)=>{calls++;if(known)return {not_modified:true,connection_id:'c',session_id:'s',path:'a.txt',bytes:3,version:'v1'};downloads++;return {connection_id:'c',session_id:'s',path:'a.txt',bytes:3,version:'v1',data:'YWJj'}};
 const one=createPersistentPreviewCache(fetch,{store,now:()=>time});await one.get(a);one.clear();
 const two=createPersistentPreviewCache(fetch,{store,now:()=>time});assert.equal((await two.get(a)).data,'YWJj');assert.equal(calls,1);
 time=61000;await two.get(a);assert.equal(calls,2);assert.equal(downloads,1);
 await assert.rejects(createPersistentPreviewCache(async()=>{throw Error('offline')},{store,now:()=>122000}).get(a),/offline/);
});
test('changed versions replace bytes; endpoint and session keys do not collide',()=>{assert.notEqual(readingKey('a','file','one','s','x'),readingKey('a','file','two','s','x'));assert.notEqual(readingKey('a','file','one','s','x'),readingKey('b','file','one','s','x'))});

test('reactive proxy values are stored as detached serializable snapshots',async()=>{const c=createReadingCache({storage:null});const data={messages:[{text:'saved'}]};await c.put('reactive',new Proxy(data,{}));data.messages[0].text='changed';assert.equal((await c.get('reactive')).messages[0].text,'saved')});

test('opening a cached preview can revalidate its resource bundle without losing offline copy',async()=>{
 const store=createReadingCache({storage:null});let version='old',calls=0;
 const a={connectionId:'bundle',sessionId:'s',path:'index.html',cursor:1};
 const cache=createPersistentPreviewCache(async(a,known)=>{calls++;if(version==='offline')throw Error('offline');return {connection_id:a.connectionId,session_id:a.sessionId,path:a.path,bytes:3,data:'YWJj',version,resources:version==='new'?[{url:'image.png'}]:[]}}, {store,now:()=>1});
 await cache.get(a);version='new';assert.equal((await cache.peek(a)).version,'old');
 assert.equal((await cache.get(a)).version,'old');assert.equal(calls,1);
 assert.equal((await cache.get(a,{force:true})).resources.length,1);assert.equal(calls,2);
 version='offline';await assert.rejects(cache.get(a,{force:true}),/offline/);
 assert.equal((await cache.peek(a)).version,'new');
});

test('findMatches finds query in cached session messages and extracts snippet', async () => {
 const c = createReadingCache({ storage: null });
 const key = readingKey('node-1', 'history', 'http://127.0.0.1', 'session-123');
 await c.put(key, {
  address: { connection_id: 'node-1', session_id: 'session-123' },
  messages: [
   { sequence: 1, role: 'user', text: 'Please build london port model in blender' },
   { sequence: 2, role: 'assistant', text: 'Exported london_port_model.png to docs' },
  ],
 });
 const matches = await c.findMatches('dsh:node-1:', 'blender');
 assert.equal(matches.length, 1);
 assert.equal(matches[0].sessionId, 'session-123');
 assert.match(matches[0].snippet, /blender/);
});
