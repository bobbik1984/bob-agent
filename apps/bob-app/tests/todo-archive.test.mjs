import { test } from 'node:test';
import assert from 'node:assert/strict';
import { todoSections } from '../src/todo-archive.mjs';
const time = year => new Date(year, 5, 1).getTime();
test('completed records move to descending year groups without modifying records', () => {
  const items = [{id:'a',status:'scheduled'}, {id:'b',status:'completed',updated_at:time(2025)}, {id:'c',status:'completed',updated_at:time(2026)}, {id:'d',status:'cancelled'}];
  const before = JSON.stringify(items), [active, archive] = todoSections(items);
  assert.deepEqual(active.groups[0].items.map(x=>x.id), ['a','d']);
  assert.deepEqual(archive.groups.map(x=>x.key), ['2026','2025']);
  assert.equal(JSON.stringify(items), before);
  items[2].status='scheduled';
  assert.equal(todoSections(items)[0].count,3);
  assert.equal(todoSections(items)[1].count,1);
});
test('empty input and missing or invalid dates do not invent years', () => {
  assert.equal(todoSections([])[1].count,0);
  assert.deepEqual(todoSections([{status:'completed'}, {status:'completed',updated_at:'bad'}])[1].groups.map(x=>[x.key,x.items.length]), [['unknown',2]]);
});
test('explicit completion timestamp takes precedence over last edit', () => {
  assert.equal(todoSections([{status:'completed',completed_at:time(2024),updated_at:time(2026)}])[1].groups[0].key,'2024');
});
