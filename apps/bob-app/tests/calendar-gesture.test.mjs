import test from 'node:test';
import assert from 'node:assert/strict';
import {calendarPatch,timedDayLayout} from '../src/calendar-gesture.mjs';
const start=new Date(2026,10,12,10).getTime();
const item={id:'one',version:7,kind:'event',title:'Meeting',start_at:start,end_at:start+3600000,notes:'preserve'};
const entry={...item,original:item,boundary:'event',at:start,until:item.end_at,key:'one'};
test('moving interval preserves duration and identity/version',()=>{const p=calendarPatch(entry,'move',1,30);assert.equal(p.endAt-p.startAt,3600000);assert.equal(new Date(p.startAt).getDate(),13);assert.equal(new Date(p.startAt).getMinutes(),30);assert.equal(p.expectedVersion,7);assert.equal(p.notes,'preserve');});
test('resize start and end independently, invalid inversion refused',()=>{assert.equal(calendarPatch(entry,'start',0,-30).endAt,item.end_at);assert.equal(calendarPatch(entry,'end',0,30).startAt,start);assert.throws(()=>calendarPatch(entry,'start',0,60));});
test('deadline point does not invent a start time',()=>{const e={...entry,boundary:'due',original:{...item,kind:'todo',start_at:0}};const p=calendarPatch(e,'move',0,15);assert.equal(p.startAt,null);assert.equal(p.endAt,item.end_at+900000);});
test('date-only remains date-only across month boundary',()=>{const e={date:'2026-11-30',boundary:'due',original:{...item,kind:'todo',start_at:0,end_at:null,timing:{due_date:'2026-11-30',source_text:'original'}}};const p=calendarPatch(e,'move',1,0);assert.equal(p.timing.due_date,'2026-12-01');assert.equal(p.endAt,null);assert.equal(p.timing.source_text,'original');});
test('cross-day segments and overlap lanes keep titles accessible',()=>{const midnight=new Date(2026,10,12).getTime();const a=timedDayLayout([entry,{...entry,key:'two'}],midnight,midnight+86400000);assert.equal(a.length,2);assert.equal(a[0].lanes,2);assert.notEqual(a[0].lane,a[1].lane);const b=timedDayLayout([{...entry,at:midnight-3600000,until:midnight+3600000}],midnight,midnight+86400000);assert.equal(b[0].top,0);assert.equal(b[0].clipStart,true);});
test('date-only cannot be moved before another start date',()=>{const e={date:'2026-11-13',boundary:'due',original:{...item,kind:'todo',start_at:0,end_at:null,timing:{start_date:'2026-11-12',due_date:'2026-11-13'}}};assert.throws(()=>calendarPatch(e,'move',-2,0));});

test('adjacent hours do not retain overlap lanes from floating point rounding',()=>{const day=new Date(2026,10,12).getTime();const a=day+17*3600000;const b=timedDayLayout([{...entry,at:a,until:a+3600000},{...entry,key:'two',at:a+3600000,until:a+7200000}],day,day+86400000);assert.deepEqual(b.map(x=>x.lanes),[1,1]);});
