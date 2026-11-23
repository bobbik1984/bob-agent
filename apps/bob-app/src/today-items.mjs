import { calendarEntries, entryDay } from './todo-calendar.mjs';

export function todayItems(items, now = Date.now()) {
  const date = new Date(now);
  const day = entryDay({ at: now });
  const start = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const end = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1).getTime();
  return items.filter(item => {
    if (['completed', 'cancelled', 'canceled', 'done', 'dismissed'].includes(item.status)) return false;
    if (item.kind === 'event' && Number(item.start_at) < end && Number(item.end_at) > start) return true;
    return calendarEntries(item).some(entry => entryDay(entry) === day);
  });
}
