import test from 'node:test';
import assert from 'node:assert/strict';
import {calendarEntries,entryDay} from '../src/todo-calendar.mjs';
test('date-only remains a civil day and points to original todo',()=>{
 const item={id:'todo',kind:'todo',start_at:0,end_at:null,timing:{due_date:'2026-11-15'}};
 const [entry]=calendarEntries(item);assert.equal(entryDay(entry),'2026-11-15');assert.equal(entry.at,null);assert.equal(entry.original,item);
});
test('two boundaries are projections, not duplicate objects',()=>{
 const item={id:'todo',kind:'todo',start_at:0,end_at:1794888000000,timing:{start_date:'2026-11-12'}};
 const entries=calendarEntries(item);assert.equal(entries.length,2);assert.equal(entries[1].at,item.end_at);assert.equal(entries[0].id,entries[1].id);
});
test('undated todos stay off calendar; events remain timed',()=>{
 assert.equal(calendarEntries({kind:'todo',start_at:0}).length,0);
 assert.equal(calendarEntries({kind:'event',start_at:1000})[0].at,1000);
});
