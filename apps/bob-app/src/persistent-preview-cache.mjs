import {readingCache,readingKey,singleFlight} from './reading-cache.mjs';
export function createPersistentPreviewCache(fetchFile,{store=readingCache,now=()=>Date.now()}={}){
 const key=a=>readingKey(a.connectionId,'file',a.endpoint||'',a.sessionId,a.path);
 function validate(f,a){if(f.connection_id!==a.connectionId||f.session_id!==a.sessionId||f.path!==a.path)throw Error('DSH_IDENTITY_MISMATCH');if(!Number.isSafeInteger(f.bytes)||f.bytes<0||f.bytes>2*1024*1024)throw Error('DSH_FILE_TOO_LARGE')}
 return {
  async peek(a){return (await store.get(key(a)))?.file||null},
  async get(a){return singleFlight(key(a),async()=>{const revision=store.revision();const saved=await store.get(key(a));
   if(saved&&saved.cursor===a.cursor&&now()-saved.checked<60000)return saved.file;
   const reply=await fetchFile(a,saved?{version:saved.file.version,bytes:saved.file.bytes}:null);validate(reply,a);
   if(reply.not_modified&&!saved)throw Error('DSH_CACHE_MISSING');
   const file=reply.not_modified?saved.file:reply;
   if(typeof file.data!=='string'||atob(file.data).length!==file.bytes)throw Error('DSH_FILE_CHANGED');
   await store.put(key(a),{file,cursor:a.cursor,checked:now()},undefined,revision);return file;
  })},
  // Leaving a view releases its resources, not the bounded persisted downloads.
  clear(){}
 };
}
export async function imageThumbnail(file,{store=readingCache}={}){
 const key=readingKey(file.connection_id,'thumbnail',file.session_id,file.path,file.sha256);
 const cached=await store.get(key);if(cached)return cached;
 const bytes=Uint8Array.from(atob(file.data),c=>c.charCodeAt(0));
 const bitmap=await createImageBitmap(new Blob([bytes]));
 try{const ratio=Math.min(1,640/Math.max(bitmap.width,bitmap.height));const canvas=document.createElement('canvas');canvas.width=Math.max(1,Math.round(bitmap.width*ratio));canvas.height=Math.max(1,Math.round(bitmap.height*ratio));canvas.getContext('2d').drawImage(bitmap,0,0,canvas.width,canvas.height);const data=canvas.toDataURL('image/webp',0.78);await store.put(key,data);return data}finally{bitmap.close()}
}
