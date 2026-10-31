// Client reading cache only. Never a task ledger, credential store or immutable artifact.
export const CACHE_LIMITS={bytes:64*1024*1024,entries:256,age:7*86400000};
export function evictCache(rows,limits=CACHE_LIMITS,now=Date.now()){
 const valid=rows.filter(r=>r.expires>now&&r.bytes<=limits.bytes).sort((a,b)=>b.used-a.used);
 let bytes=0,count=0;return valid.filter(r=>{if(count>=limits.entries||bytes+r.bytes>limits.bytes)return false;bytes+=r.bytes;count++;return true});
}
export function createReadingCache({storage=globalThis.indexedDB,limits=CACHE_LIMITS,now=()=>Date.now()}={}){
 let database,chain=Promise.resolve();const memory=new Map();let epoch=0;
 const serial=fn=>{const task=chain.then(fn,fn);chain=task.catch(()=>{});return task};
 async function db(){if(!storage)return null;if(!database)database=new Promise((resolve,reject)=>{const q=storage.open('bob-reading-cache-v1',1);q.onupgradeneeded=()=>q.result.createObjectStore('entries',{keyPath:'key'});q.onsuccess=()=>resolve(q.result);q.onerror=()=>reject(q.error)});return database}
 async function transaction(mode,fn){const d=await db();if(!d)return fn(null);return new Promise((resolve,reject)=>{const tx=d.transaction('entries',mode),store=tx.objectStore('entries');let value;fn(store,x=>{value=x});tx.oncomplete=()=>resolve(value);tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error)})}
 async function all(){return transaction('readonly',(s,done)=>{if(!s)return [...memory.values()];const q=s.getAll();q.onsuccess=()=>done(q.result)})}
 return {
  revision:()=>epoch,
  async get(key){return serial(async()=>{try{const row=await transaction('readonly',(s,done)=>{if(!s)return memory.get(key);const q=s.get(key);q.onsuccess=()=>done(q.result)});if(!row||row.expires<=now())return null;row.used=now();await transaction('readwrite',s=>s?s.put(row):memory.set(key,row));return structuredClone(row.value)}catch{return null}})},
  async put(key,value,age=limits.age,generation=epoch){return serial(async()=>{if(generation!==epoch)return;try{const bytes=new TextEncoder().encode(JSON.stringify(value)).length;if(bytes>limits.bytes)return;const row={key,value,bytes,used:now(),expires:now()+Math.min(age,limits.age)};const rows=(await all()).filter(x=>x.key!==key);rows.push(row);const keep=evictCache(rows,limits,now()),keys=new Set(keep.map(x=>x.key));await transaction('readwrite',s=>{for(const r of rows){if(!keys.has(r.key)){if(s)s.delete(r.key);else memory.delete(r.key)}}if(keys.has(key)){if(s)s.put(row);else memory.set(key,row)}})}catch{/* Storage full/unavailable must not break reading or sending. */}})},
  async removePrefix(prefix){epoch++;return serial(async()=>{try{const rows=await all();await transaction('readwrite',s=>{for(const r of rows)if(r.key.startsWith(prefix)){if(s)s.delete(r.key);else memory.delete(r.key)}})}catch{}})},
  async stats(){return serial(async()=>{try{const rows=await all();return {entries:rows.length,bytes:rows.reduce((n,r)=>n+r.bytes,0),limit:limits.bytes}}catch{return {entries:0,bytes:0,limit:limits.bytes}}})}
 };
}
export const readingCache=createReadingCache();
export const connectionPrefix=id=>`dsh:${encodeURIComponent(id)}:`;
export const readingKey=(id,kind,...parts)=>connectionPrefix(id)+kind+':'+JSON.stringify(parts);
const flights=new Map();
export async function singleFlight(key,load){if(flights.has(key))return flights.get(key);const job=Promise.resolve().then(load);flights.set(key,job);try{return await job}finally{if(flights.get(key)===job)flights.delete(key)}}
