// Existing records have no completion timestamp; updated_at is the legacy fallback.
export function todoSections(items) {
  const active = [], years = new Map();
  for (const item of items) {
    if (item.status !== 'completed') { active.push(item); continue; }
    const value = item.completed_at ?? item.updated_at;
    const ms = Number(value);
    const date = new Date(Number.isFinite(ms) && ms > 0 ? ms : NaN);
    const year = Number.isNaN(date.getTime()) ? 'unknown' : String(date.getFullYear());
    if (!years.has(year)) years.set(year, []);
    years.get(year).push(item);
  }
  const groups = [...years].sort(([a], [b]) => a === 'unknown' ? 1 : b === 'unknown' ? -1 : Number(b) - Number(a))
    .map(([key, items]) => ({ key, items: [...items].sort((a,b) => Number(b.completed_at ?? b.updated_at ?? 0) - Number(a.completed_at ?? a.updated_at ?? 0)) }));
  return [
    { key: 'active', archived: false, count: active.length, groups: [{ key: 'active', items: active }] },
    { key: 'completed', archived: true, count: items.length - active.length, groups },
  ];
}
