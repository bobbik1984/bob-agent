// Bounded, volatile cache. A new history cursor invalidates current-file reuse.
export function createPreviewCache(fetchFile,{maxBytes=16*1024*1024,maxEntries=12}={}) {
 const ready=new Map(),pending=new Map(),times=new Map();let size=0,generation=0;
 const key=a=>JSON.stringify([a.connectionId,a.sessionId,a.path,a.cursor??null]);
 return {async get(a){const k=key(a);if(ready.has(k)&&Date.now()-times.get(k)>60000){size-=ready.get(k).data.length*2;ready.delete(k);times.delete(k);}
  if(ready.has(k)){const f=ready.get(k);ready.delete(k);ready.set(k,f);return f;}
  if(pending.has(k))return pending.get(k);
  const g=generation;const job=(async()=>{const f=await fetchFile(a);
   if(f.connection_id!==a.connectionId||f.session_id!==a.sessionId||f.path!==a.path)throw Error('DSH_IDENTITY_MISMATCH');
   if(!Number.isSafeInteger(f.bytes)||f.bytes<0||f.bytes>2*1024*1024||typeof f.data!=='string')throw Error('DSH_FILE_TOO_LARGE');
   const cost=f.data.length*2;
   while(ready.size&&(size+cost>maxBytes||ready.size>=maxEntries)){const first=ready.keys().next().value;size-=ready.get(first).data.length*2;ready.delete(first);}
   if(cost<=maxBytes&&g===generation){ready.set(k,f);times.set(k,Date.now());size+=cost;}return f;
  })();pending.set(k,job);try{return await job;}finally{pending.delete(k);}
 },clear(){generation++;ready.clear();times.clear();size=0;},stats(){return {bytes:size,entries:ready.size,pending:pending.size};}};
}
