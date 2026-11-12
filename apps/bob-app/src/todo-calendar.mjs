// Calendar entries are projections of one todo; clicking edits its original ID.
export function calendarEntries(item) {
  if (item.kind === 'event') return item.start_at ? [{...item, key:item.id, at:Number(item.start_at), boundary:'event', original:item}] : [];
  const result=[];
  for (const boundary of ['start','due']) {
    const date=item.timing?.[`${boundary}_date`];
    const at=boundary==='start' ? item.start_at : item.end_at;
    if (date || at) result.push({...item,key:`${item.id}-${boundary}`,date:date || null,at:date ? null : Number(at),boundary,original:item});
  }
  return result;
}
export function entryDay(entry) {
  if (entry.date) return entry.date;
  const d=new Date(entry.at);
  return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
}
