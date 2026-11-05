import {test} from 'node:test';
import assert from 'node:assert/strict';
import {compactQuota} from '../src/composables/quota-display.mjs';
test('quota uses most constrained actual window, not sum or average',()=>{
 const q=compactQuota({status:'available',windows:[{id:'gemini-weekly',remaining_percent:35,resets_at:1000+52*3600},{id:'gemini-5h',remaining_percent:100,resets_at:19000}]},1000);
 assert.equal(q.id,'gemini-weekly');assert.equal(q.timeText,'2d 4h');assert.equal(q.timePercent,52/168*100);
});
test('missing data stays unknown; expiry never invents refreshed quota',()=>{
 assert.equal(compactQuota({status:'unavailable'},1000),null);
 const q=compactQuota({status:'stale',windows:[{id:'weekly',remaining_percent:50,resets_at:1000}]},1001);
 assert.equal(q.expired,true);assert.equal(q.timeText,'0m');assert.equal(q.timePercent,0);
});
test('unknown period does not fabricate a countdown fraction',()=>{
 const q=compactQuota({status:'available',windows:[{id:'primary',remaining_percent:0,resets_at:4600}]},1000);
 assert.equal(q.timePercent,null);assert.equal(q.timeText,'1h 0m');
});
