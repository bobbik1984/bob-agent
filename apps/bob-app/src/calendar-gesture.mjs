// Calendar gestures update the original object with its captured CAS version.
export function calendarPatch(entry, mode, dayDelta, minuteDelta) {
  const item=entry.original || entry;
  const patch={id:item.id,expectedVersion:item.version,title:item.title,kind:item.kind,projectId:item.project_id || null,
    startAt:item.start_at || null,endAt:item.end_at || null,notes:item.notes || '',timing:{...(item.timing || {})}};
  const shift=ms=>{const d=new Date(ms);d.setDate(d.getDate()+dayDelta);d.setMinutes(d.getMinutes()+minuteDelta);return d.getTime();};
  if(entry.date) {
    const d=new Date(`${entry.date}T12:00:00`);d.setDate(d.getDate()+dayDelta);
    patch.timing[`${entry.boundary}_date`]=`${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
  } else if(entry.boundary==='event' || entry.boundary==='range') {
    if(mode==='move') {patch.startAt=shift(patch.startAt);if(patch.endAt)patch.endAt=shift(patch.endAt);}
    else if(mode==='start') patch.startAt=shift(patch.startAt);
    else patch.endAt=shift(patch.endAt || patch.startAt+3600000);
  } else {
    const key=entry.boundary==='due'?'endAt':'startAt';patch[key]=shift(patch[key]);
  }
  if(patch.startAt && patch.endAt && patch.endAt < patch.startAt+((entry.boundary==='event'||entry.boundary==='range')?900000:0))throw new Error('INVALID_INTERVAL');
  const start=patch.timing.start_date || (patch.startAt?localDay(patch.startAt):null);
  const end=patch.timing.due_date || (patch.endAt?localDay(patch.endAt):null);
  if(start && end && end<start)throw new Error('INVALID_INTERVAL');
  return patch;
}
export function localDay(ms){const d=new Date(ms);return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;}
export function timedDayLayout(entries, dayStart, dayEnd) {
  const duration=dayEnd-dayStart;
  const blocks=entries.filter(e=>!e.date && e.at<dayEnd && (e.until || e.at+1)>dayStart).map(e=>{
    const start=Math.max(dayStart,e.at),end=Math.min(dayEnd,e.until || e.at+3600000);
    const top=(start-dayStart)/duration*100;
    return {...e,top,height:Math.max(45/1440*100,(end-start)/duration*100),clipStart:e.at<dayStart,clipEnd:(e.until||e.at)>dayEnd};
  }).sort((a,b)=>a.top-b.top || a.key.localeCompare(b.key));
  let group=[],ends=[];
  const finish=()=>{for(const b of group)b.lanes=ends.length;group=[];ends=[];};
  for(const b of blocks){if(group.length && b.top+1e-8>=Math.max(...ends))finish();let lane=ends.findIndex(end=>end<=b.top+1e-8);if(lane<0)lane=ends.length;ends[lane]=b.top+b.height;b.lane=lane;group.push(b);}
  finish();return blocks;
}
