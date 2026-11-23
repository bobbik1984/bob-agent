import test from 'node:test';
import assert from 'node:assert/strict';
import { todayItems } from '../src/today-items.mjs';
test('today includes date-only start and deadline, timed and spanning events once', () => {
  const now = new Date(2026, 10, 13, 12).getTime();
  const rows = [
    { id:'a', kind:'todo', timing:{start_date:'2026-11-13',due_date:'2026-11-13'} },
    { id:'b', kind:'todo', timing:{due_date:'2026-11-14'} },
    { id:'c', kind:'event',start_at:now-86400000,end_at:now+86400000 },
    { id:'d', kind:'todo',end_at:now },
    { id:'e', kind:'todo',timing:{due_date:'2026-11-13'},status:'completed' },
    { id:'f', kind:'todo' },
    { id:'g', kind:'event',start_at:now,status:'cancelled' },
  ];
  assert.deepEqual(todayItems(rows, now).map(x=>x.id), ['a','c','d']);
});
test('empty and invalid dates do not invent today tasks', () => {
  assert.deepEqual(todayItems([]), []);
  assert.deepEqual(todayItems([{kind:'todo',start_at:'invalid'}]), []);
});
