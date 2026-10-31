<script setup>
import { computed, ref, watch, onUnmounted, nextTick, provide } from 'vue';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from '../native-bridge.mjs';
import MessageContent from './MessageContent.vue';
import DshFilePreview from './DshFilePreview.vue';
import {readingCache,readingKey} from '../reading-cache.mjs';
import {createPersistentPreviewCache} from '../persistent-preview-cache.mjs';
const props=defineProps({nodes:{type:Array,default:()=>[]}, target:{type:Object,default:null}});
const {t}=useI18n();
const connection=ref(''), sessions=ref([]), selected=ref(null), history=ref(null), search=ref(''), busy=ref(false), sending=ref(false), error=ref(''), notice=ref(''), draft=ref(''), pending=ref(null);
const reader=ref(null), loadingOlder=ref(false);let lastTop=0;
const preview=ref(null);let previewScroll=0;
const fileCache=createPersistentPreviewCache((a,known)=>kernelBridge.dshPreview(a.connectionId,a.sessionId,a.path,known));
const fileAddress=path=>({connectionId:connection.value,sessionId:selected.value.id,endpoint:nodes.value.find(n=>n.id===connection.value)?.baseUrl||nodes.value.find(n=>n.id===connection.value)?.url||'',path:path.replace(/:\d+(?::\d+)?$/,''),cursor:history.value?.cursor});
let artifactQueue=[],artifactTimer,artifactRunning=false,artifactCount=0;
async function drainArtifacts(){if(artifactRunning||disposed)return;artifactQueue=artifactQueue.filter(x=>x.visible());artifactQueue.sort((a,b)=>b.element.getBoundingClientRect().top-a.element.getBoundingClientRect().top);const item=artifactQueue.shift();if(!item||artifactCount>=6)return;artifactCount++;artifactRunning=true;try{await item.run()}finally{artifactRunning=false;artifactTimer=setTimeout(drainArtifacts,150)}}
provide('dshArtifacts',{peek:path=>fileCache.peek(fileAddress(path)),get:path=>fileCache.get(fileAddress(path)),enqueue:item=>{if(!artifactQueue.some(x=>x.element===item.element))artifactQueue.push(item);clearTimeout(artifactTimer);artifactTimer=setTimeout(drainArtifacts,250)}});
provide('dshPreviewGet',async a=>{const cached=await fileCache.peek(a);return cached?{...cached,from_cache:true}:fileCache.get(a)});
async function closePreview(){preview.value=null;await nextTick();if(reader.value)reader.value.scrollTop=previewScroll;clearTimeout(timer);timer=setTimeout(refresh,5000)}
provide('dshOpenFile',path=>{previewScroll=reader.value?.scrollTop||0;clearTimeout(timer);preview.value=fileAddress(path)});
let generation=0, disposed=false, restoring=false, timer;
const nodes=computed(()=>props.nodes.filter(n=>n.enabled!==false));
const historyKey=()=>readingKey(connection.value,'history',nodes.value.find(n=>n.id===connection.value)?.baseUrl||nodes.value.find(n=>n.id===connection.value)?.url||'',selected.value?.id);
function saveHistory(){if(!history.value)return;const messages=history.value.messages.slice(-10);const h={...history.value,messages,before:messages[0]?.sequence??history.value.before,has_more:history.value.has_more||history.value.messages.length>10};void readingCache.put(historyKey(),h)}
const key=()=>`bob-dsh:${connection.value}:${selected.value?.id || ''}`;
function readState(){try{return JSON.parse(localStorage.getItem(key()) || '{}')}catch{return {}}}
function persist(){if(restoring || !selected.value)return false;try{localStorage.setItem(key(),JSON.stringify({draft:draft.value,pending:pending.value}));return true}catch{error.value=t('dshStorageError');return false}}
watch(draft,persist,{flush:'sync'});
const groups=computed(()=>{
 const map=new Map();
 for(const s of sessions.value){if(search.value.trim() && !`${s.title} ${s.workspace || ''}`.toLocaleLowerCase().includes(search.value.trim().toLocaleLowerCase()))continue;
 const k=typeof s.workspace==='string' && s.workspace ? s.workspace : t('dshNoWorkspace');
 if(!map.has(k))map.set(k,[]);map.get(k).push(s);}
 return [...map].map(([name,items])=>({name,items}));
});
function fault(e){const code=String(e?.message || e);return `${t(code.includes('DSH_RECEIPT_UNKNOWN')?'dshUncertain':code.includes('DSH_HISTORY_TOO_LARGE')?'dshHistoryTooLarge':code.includes('DSH_HISTORY_TIMEOUT')?'dshHistoryTimeout':code.includes('DSH_HISTORY_UNAVAILABLE')?'dshHistoryUnavailable':'dshError')} · ${code.match(/DSH_[A-Z_]+/)?.[0] || 'DSH_UNAVAILABLE'}`}
async function load(){const g=++generation;clearTimeout(timer);busy.value=true;error.value='';selected.value=null;history.value=null;sessions.value=[];try{const items=await kernelBridge.dshList(connection.value);if(g===generation&&!disposed)sessions.value=items;}catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation)busy.value=false}}
watch(connection,()=>{if(connection.value&&!props.target)load()});
function validatePage(h,id){if(h.address.session_id!==id||h.address.connection_id!==connection.value||!Number.isSafeInteger(h.cursor)||!Number.isSafeInteger(h.before)||!Array.isArray(h.messages))throw Error('DSH_IDENTITY_MISMATCH')}
function combine(old,h){const map=new Map((old?.messages||[]).map(m=>[m.sequence,m]));for(const m of h.messages)map.set(m.sequence,m);const replacements=[...(old?.replacements||[]),...(h.replacements||[])];for(const op of replacements)for(const seq of map.keys())if(seq>=op.startSeq&&seq<=op.endSeq)map.delete(seq);return {...h,messages:[...map.values()].sort((a,b)=>a.sequence-b.sequence),replacements,workspace:h.workspace||old?.workspace}}
let olderRequested=false;
async function refresh(){
 if(!selected.value||busy.value||loadingOlder.value||preview.value)return;
 clearTimeout(timer);const g=generation,id=selected.value.id;busy.value=true;
 try{
  const h=await kernelBridge.dshPage(connection.value,id,null,null,history.value?.cursor ?? null);validatePage(h,id);
  if(g!==generation||disposed||preview.value)return;
  if(h.not_modified){error.value='';return;}
  const first=history.value===null;let additions=h;
  if(!first&&notice.value===t('dshAccepted')&&h.messages.some(m=>m.role==='assistant'&&m.sequence>history.value.cursor))notice.value='';
  // Catch up a burst without losing messages between the old cursor and latest page.
  for(let n=0;!first&&additions.before>history.value.cursor+1&&additions.has_more;n++){
   if(n>=32)throw Error('DSH_PAGE_CATCHUP_REQUIRED');
   const older=await kernelBridge.dshPage(connection.value,id,additions.before,h.cursor);validatePage(older,id);
   if(g!==generation||disposed||preview.value)return;
   if(older.before>=additions.before)throw Error('DSH_PAGE_NO_PROGRESS');
   additions={...combine(older,additions),before:older.before,has_more:older.has_more};
  }
  const el=reader.value,follow=first||(el&&el.scrollHeight-el.scrollTop-el.clientHeight<80);
  history.value=first?additions:{...combine(history.value,additions),before:history.value.before,has_more:history.value.has_more};error.value='';
  await nextTick();if(g===generation&&!disposed&&follow&&reader.value)reader.value.scrollTop=reader.value.scrollHeight;
  lastTop=reader.value?.scrollTop||0;saveHistory();
 }catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation){busy.value=false;if(olderRequested&&reader.value?.scrollTop<=24){olderRequested=false;loadOlder()}else{olderRequested=false;timer=setTimeout(refresh,5000)}}}
}
async function loadOlder(){
 if(!history.value?.has_more||loadingOlder.value||busy.value||preview.value)return;
 const g=generation,id=selected.value.id,before=history.value.before;loadingOlder.value=true;clearTimeout(timer);
 try{
  const pageKey=readingKey(connection.value,'page',nodes.value.find(n=>n.id===connection.value)?.baseUrl||nodes.value.find(n=>n.id===connection.value)?.url||'',id,before,history.value.cursor);
  const cached=await readingCache.get(pageKey);if(g!==generation||disposed)return;
  const h=cached||await kernelBridge.dshPage(connection.value,id,before,history.value.cursor);validatePage(h,id);
  if(g!==generation||disposed||preview.value)return;
  if(h.has_more&&h.before>=before)throw Error('DSH_PAGE_NO_PROGRESS');
  void readingCache.put(pageKey,h);
  const el=reader.value, height=el?.scrollHeight||0,top=el?.scrollTop||0;
  history.value={...combine(h,history.value),before:h.before,has_more:h.has_more};error.value='';
  await nextTick();if(g===generation&&reader.value){reader.value.scrollTop=top+reader.value.scrollHeight-height;lastTop=reader.value.scrollTop;}
 }catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation){loadingOlder.value=false;timer=setTimeout(refresh,5000)}}
}
function onScroll(){const top=reader.value?.scrollTop||0;const upwards=top<lastTop;lastTop=top;if(upwards&&top<=24){if(busy.value)olderRequested=true;else loadOlder()}else if(top>24)olderRequested=false}
async function open(s){olderRequested=false;artifactQueue=[];artifactCount=0;fileCache.clear();persist();++generation;clearTimeout(timer);busy.value=false;loadingOlder.value=false;lastTop=0;selected.value=s;history.value=null;notice.value='';const saved=readState();restoring=true;draft.value=saved.draft||'';pending.value=saved.pending||null;restoring=false;if(pending.value)notice.value=t('dshUncertain');const g=generation;const cached=await readingCache.get(historyKey());if(g!==generation||disposed)return;if(cached){try{validatePage(cached,s.id);history.value=cached;await nextTick();if(reader.value)reader.value.scrollTop=reader.value.scrollHeight;lastTop=reader.value?.scrollTop||0;}catch{}}await refresh()}
function back(){persist();++generation;clearTimeout(timer);busy.value=false;selected.value=null;history.value=null;notice.value='';load()}
async function checkReceipt(){
 if(!pending.value || sending.value)return;
 const g=generation,p= pending.value; sending.value=true;
 try{const receipt=await kernelBridge.dshReceipt(connection.value,selected.value.id,p.text,p.id);
 if(g!==generation||disposed)return;
 if(receipt.status==='accepted' && receipt.session_id===selected.value.id && receipt.request_id===p.id){
 pending.value=null;if(draft.value.trim()===p.text)draft.value='';persist();error.value='';notice.value=t('dshAccepted');
 }else{notice.value=t('dshReceiptStillUnknown');}
 }catch(e){if(g===generation)error.value=fault(e)}finally{sending.value=false;if(g===generation&&!busy.value)refresh()}
}
async function send(){if(sending.value||pending.value||!draft.value.trim())return;const text=draft.value.trim(), g=generation, address={connection:connection.value,session:selected.value.id};pending.value={id:crypto.randomUUID(),text};if(!persist()){pending.value=null;return;}sending.value=true;error.value='';try{const requestId=pending.value.id;const receipt=await kernelBridge.dshPrompt(address.connection,address.session,text,requestId);if(receipt?.status!=='accepted'||receipt.session_id!==address.session||receipt.request_id!==requestId)throw Error('DSH_RECEIPT_UNKNOWN');if(g===generation){pending.value=null;if(draft.value.trim()===text)draft.value='';persist();notice.value=t('dshAccepted');}}catch(e){if(g===generation){error.value=fault(e);notice.value=t('dshUncertain')}}finally{sending.value=false;if(g===generation&&!busy.value)refresh()}}
watch(()=>props.target, async target=>{if(!target)return;connection.value=target.node.id;await open(target.session)},{immediate:true});
onUnmounted(()=>{clearTimeout(artifactTimer);artifactQueue=[];fileCache.clear();persist();disposed=true;++generation;clearTimeout(timer)});
</script>
<template>
 <DshFilePreview v-if="preview" :address="preview" @close="closePreview" />
 <section class="dsh-sessions" :class="{embedded:!!target}" :aria-label="t('dshTitle')">
  <div ref="reader" class="dsh-reader" tabindex="0" @scroll.passive="onScroll">
  <header v-if="!target"><h3>{{ t('dshTitle') }}</h3><select v-model="connection" :disabled="sending" :aria-label="t('dshChooseNode')"><option value="">{{ t('dshChooseNode') }}</option><option v-for="n in nodes" :key="n.id" :value="n.id">{{ n.label || n.name || n.baseUrl || n.url }}</option></select></header>
  <p v-if="!target" class="dsh-note">{{ t('dshSource') }}</p>
  <p v-if="error" role="alert">{{ error }}</p><p v-if="notice" role="status">{{ notice }}</p>
  <template v-if="connection && !selected">
   <div class="dsh-tools"><input v-model="search" type="search" :placeholder="t('dshSearch')" :aria-label="t('dshSearch')"><button :disabled="busy" @click="load">{{ t('dshRefresh') }}</button></div>
   <p v-if="busy" role="status">{{ t('dshLoading') }}</p>
   <details v-for="group in groups" :key="group.name" :open="!!search"><summary>{{ group.name }} · {{ group.items.length }}</summary><button v-for="s in group.items" :key="s.id" class="dsh-session" @click="open(s)"><strong>{{ s.title }}</strong><small>{{ s.updated_at ? new Date(s.updated_at).toLocaleString() : t('sidebarTimeUnknown') }} · {{ t(s.running?'dshRunning':'dshIdle') }}</small></button></details>
   <p v-if="!busy && !groups.length">{{ t('dshEmpty') }}</p>
  </template>
  <template v-if="selected">
   <div class="dsh-tools"><button v-if="!target" :disabled="sending" @click="back">{{ t('dshBack') }}</button><h4 v-if="!target">{{ selected.title }}</h4><small v-else>{{ target.node.label || target.node.name }} · {{ t('dshAgent') }}</small><button :disabled="busy" @click="refresh">{{ t('dshRefresh') }}</button></div>
   <p class="dsh-note">{{ history?.workspace || t('dshNoWorkspace') }}</p>
   <p v-if="history?.partial || history?.omitted" class="dsh-note">{{ t('dshPartial') }}</p>
   <p v-if="busy && !history" role="status">{{ t('dshLoading') }}</p>
   <button v-if="history?.has_more" class="dsh-older" :disabled="loadingOlder || busy" @click="loadOlder">{{ t(loadingOlder ? 'dshLoading' : 'dshLoadOlder') }}</button>
   <article v-for="m in history?.messages || []" :key="m.sequence" :data-sequence="m.sequence" class="dsh-message" :class="{user:m.role==='user'}"><small>{{ t(m.role==='user'?'dshYou':'dshAgent') }}</small><MessageContent :content="m.text" :plain="m.role==='user'" /></article>
  </template>
  </div>
   <form v-if="selected" class="dsh-compose" @submit.prevent="send"><textarea v-model="draft" :disabled="sending" :placeholder="t('dshPrompt')" :aria-label="t('dshPrompt')" rows="3"></textarea><button :disabled="sending || !!pending || !draft.trim() || !history">{{ t(sending?'dshSending':'send') }}</button><p v-if="pending && !sending" role="status">{{ t('dshUncertain') }} <button type="button" @click="checkReceipt">{{ t('dshCheckReceipt') }}</button></p></form>
 </section>
</template>
<style scoped>
.dsh-sessions{min-width:0;max-width:100%;padding:16px;border:1px solid var(--border-color);border-radius:12px;background:var(--bg-primary)}
.dsh-sessions.embedded{border:0;border-radius:0;padding:0;flex:1;min-height:0;overflow:hidden;display:flex;flex-direction:column}.dsh-reader{min-height:0;flex:1;overflow-y:auto;overflow-x:hidden;scrollbar-gutter:stable;overflow-anchor:none;padding:12px;overscroll-behavior:contain}.dsh-compose{flex-shrink:0;padding:12px;background:var(--bg-primary);border-top:1px solid var(--border-color)}.dsh-older{display:block;margin:8px auto;background:transparent;border:0;color:var(--text-secondary)}header,.dsh-tools{display:flex;align-items:center;gap:8px;flex-wrap:wrap}h3,h4{margin:0;overflow-wrap:anywhere}select,input,textarea{min-width:0;max-width:100%;color:var(--text-primary);background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:8px;padding:8px}select,input{flex:1}button{max-width:100%;white-space:normal}details{margin-top:10px}summary{overflow-wrap:anywhere;cursor:pointer;padding:8px 0}.dsh-note,small{color:var(--text-secondary);overflow-wrap:anywhere}.dsh-session{display:flex;flex-direction:column;text-align:left;width:100%;gap:5px;margin-top:5px;padding:10px}.dsh-session strong{overflow-wrap:anywhere}.dsh-message{min-width:0;margin-top:20px;overflow-wrap:anywhere}.dsh-message.user{background:var(--bg-secondary);border-radius:10px;padding:12px}.dsh-compose{margin-top:0;display:flex;flex-wrap:wrap;gap:8px}.dsh-compose textarea{flex:1;min-width:65%}.dsh-compose p{width:100%}
</style>
