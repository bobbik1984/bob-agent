import {test} from 'node:test';
import assert from 'node:assert/strict';
import {compactQuota} from '../src/composables/quota-display.mjs';
test('quota shows the selected weekly window even when five-hour allowance is lower',()=>{
 const q=compactQuota({status:'available',windows:[{id:'gemini-weekly',remaining_percent:35,resets_at:1000+52*3600},{id:'gemini-5h',remaining_percent:2,resets_at:19000}]},1000,'gemini-weekly');
 assert.equal(q.id,'gemini-weekly');assert.equal(q.timeText,'2d 4h');assert.equal(q.timePercent,52/168*100);
});
test('missing data stays unknown; expiry never invents refreshed quota',()=>{
 assert.equal(compactQuota({status:'unavailable'},1000),null);
 const q=compactQuota({status:'stale',windows:[{id:'weekly',remaining_percent:50,resets_at:1000}]},1001);
 assert.equal(q.expired,true);assert.equal(q.timeText,'0m');assert.equal(q.timePercent,0);
});
test('unknown period does not fabricate a countdown fraction',()=>{
 const q=compactQuota({status:'available',windows:[{id:'primary',remaining_percent:0,resets_at:4600}]},1000,'primary');
 assert.equal(q.timePercent,null);assert.equal(q.timeText,'1h 0m');
});

test('weekly display never falls back to short or third-party windows',()=>{
 assert.equal(compactQuota({status:'available',windows:[{id:'gemini-5h',remaining_percent:2},{id:'3p-weekly',remaining_percent:90}]},1000,'gemini-weekly'),null);
});
