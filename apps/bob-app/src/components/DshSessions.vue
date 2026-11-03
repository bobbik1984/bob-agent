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
const pullUpDistance=ref(0),attachments=ref([]),fileInput=ref(null);let touchStartY=null,wheelOverscroll=0,wheelResetTimer;
const nodeActivity=ref(null),awaitingReply=ref(null),replying=ref(false);let activityChecked=0;
const syncStatus=ref(''),hasNewMessages=ref(false),copiedSeq=ref(null),streamSeq=ref(null),rodIndex=ref(0);let syncTimer,copyTimer,rodTimer;
const rodFrames=['|','/','-','\\'],stripDots=s=>String(s??'').replace(/\s*(?:…|\.{2,})\s*$/,'');
const rodFrame=computed(()=>`\u2060${rodFrames[rodIndex.value%rodFrames.length]}\u2060`);
const isSpinning=computed(()=>Boolean(busy.value||sending.value||replying.value||awaitingReply.value||nodeActivity.value?.running||syncStatus.value==='checking'||syncStatus.value==='syncing'));
function updateRodTicker(){
 if(typeof setInterval!=='function')return;
 if(isSpinning.value&&!rodTimer){rodTimer=setInterval(()=>{rodIndex.value=(rodIndex.value+1)%rodFrames.length;},120);}
 else if(!isSpinning.value&&rodTimer){if(typeof clearInterval==='function')clearInterval(rodTimer);rodTimer=null;}
}
watch(isSpinning,updateRodTicker);
const displayMessages=computed(()=>{
 const raw=history.value?.messages||[];
 if(!raw.length)return [];
 const out=[];let run=[];
 const flush=()=>{
  if(!run.length)return;
  if(run.length===1){out.push(run[0]);run=[];return;}
  const chunks=[];let sbuf=[];
  const flushSteps=()=>{if(sbuf.length){chunks.push('```thinking\n'+sbuf.map(s=>`- ${s}`).join('\n')+'\n```');sbuf=[];}};
  for(const m of run){
   const tx=String(m.text??'').trim();if(!tx)continue;
   if(/^(?:[-*]\s+)?(?:`?\[(?:Tool|Thinking)(?::\s*[^\]]*)?\]`?)/i.test(tx)&&!tx.includes('\n\n'))sbuf.push(tx.replace(/^\s*[-*]\s+/,'').trim());
   else{flushSteps();chunks.push(tx);}
  }
  flushSteps();
  const anchor=run[run.length-1];
  out.push({...anchor,text:chunks.join('\n\n')||anchor.text});run=[];
 };
 for(const m of raw){if(m.role==='user'){flush();out.push(m);}else run.push(m);}
 flush();
 return out;
});
const syncStatusLabel=computed(()=>stripDots(syncStatus.value==='checking'?t('dshSyncChecking'):syncStatus.value==='syncing'?t('dshSyncUpdating'):syncStatus.value==='synced'?t('dshSyncCompleted'):syncStatus.value==='offline_cached'?t('dshSyncOfflineCached'):''));
async function refreshActivity(g,id){if(Date.now()-activityChecked<10000)return;activityChecked=Date.now();try{const list=await kernelBridge.dshList(connection.value);if(g!==generation||disposed)return;const row=list.find(s=>s.id===id);nodeActivity.value=typeof row?.running==='boolean'?{running:row.running,at:Date.now()}:null}catch{if(g===generation)nodeActivity.value=null}}
const activityLabel=computed(()=>stripDots(sending.value?t('dshSendingWaitReceipt'):replying.value?t('dshReplying'):nodeActivity.value?.running===true?t('dshNodeProcessing'):awaitingReply.value?t('dshAwaitingReply'):''));
const agentLabel=computed(()=>{const h=selected.value?.harness||history.value?.harness||'';if(h==='agy'||selected.value?.id?.startsWith('agy:'))return t('agyAgent');if(h==='claude'||selected.value?.toolLabel==='Claude')return t('claudeAgent');if(h==='pi'||selected.value?.toolLabel==='Pi')return 'Pi';if(h==='codex'||selected.value?.toolLabel==='Codex')return 'Codex';return t('dshAgent');});
function formatMessageTime(ts){
 if(ts===null||ts===undefined||ts===''||typeof ts==='boolean')return '';
 const raw=typeof ts==='number'?ts:Number(ts);
 if(Number.isFinite(raw)&&raw<=0)return '';
 const ms=Number.isFinite(raw)?(raw<1e12&&raw>1e9?raw*1000:raw):Date.parse(String(ts));
 if(!Number.isFinite(ms)||ms<946684800000)return '';
 const d=new Date(ms),now=new Date(),p=n=>String(n).padStart(2,'0');
 const hm=`${p(d.getHours())}:${p(d.getMinutes())}`;
 if(d.getFullYear()===now.getFullYear()&&d.getMonth()===now.getMonth()&&d.getDate()===now.getDate())return hm;
 if(d.getFullYear()===now.getFullYear())return `${p(d.getMonth()+1)}-${p(d.getDate())} ${hm}`;
 return `${d.getFullYear()}-${p(d.getMonth()+1)}-${p(d.getDate())} ${hm}`;
}
async function copyMessage(m){
 const raw=String(m?.text??'');
 if(!raw)return false;
 try{
  if(globalThis.navigator?.clipboard?.writeText){await globalThis.navigator.clipboard.writeText(raw);}
  copiedSeq.value=m.sequence;
  clearTimeout(copyTimer);
  copyTimer=setTimeout(()=>{if(copiedSeq.value===m.sequence)copiedSeq.value=null;},1600);
  return true;
 }catch{return false;}
}
function scrollToBottom(){
 hasNewMessages.value=false;
 if(reader.value){reader.value.scrollTop=reader.value.scrollHeight;lastTop=reader.value.scrollTop;}
}
const preview=ref(null);let previewScroll=0;
const fileCache=createPersistentPreviewCache((a,known)=>kernelBridge.dshPreview(a.connectionId,a.sessionId,a.path,known));
const fileAddress=path=>({connectionId:connection.value,sessionId:selected.value.id,endpoint:nodes.value.find(n=>n.id===connection.value)?.baseUrl||nodes.value.find(n=>n.id===connection.value)?.url||'',path:path.replace(/:\d+(?::\d+)?$/,''),cursor:history.value?.cursor});
let artifactQueue=[],artifactTimer,artifactRunning=false,artifactCount=0;
async function drainArtifacts(){if(artifactRunning||disposed)return;artifactQueue=artifactQueue.filter(x=>x.visible());artifactQueue.sort((a,b)=>b.element.getBoundingClientRect().top-a.element.getBoundingClientRect().top);const item=artifactQueue.shift();if(!item||artifactCount>=6)return;artifactCount++;artifactRunning=true;try{await item.run()}finally{artifactRunning=false;artifactTimer=setTimeout(drainArtifacts,150)}}
provide('dshArtifacts',{peek:path=>fileCache.peek(fileAddress(path)),get:path=>fileCache.get(fileAddress(path)),enqueue:item=>{if(!artifactQueue.some(x=>x.element===item.element))artifactQueue.push(item);clearTimeout(artifactTimer);artifactTimer=setTimeout(drainArtifacts,250)}});
provide('dshPreviewGet',async a=>{const cached=await fileCache.peek(a);return cached?{...cached,from_cache:true}:fileCache.get(a)});
async function closePreview(){preview.value=null;await nextTick();if(reader.value){reader.value.scrollTop=previewScroll;lastTop=previewScroll;}clearTimeout(timer);timer=setTimeout(refresh,5000)}
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
function fault(e){const code=String(e?.message || e);if(code.includes('AGY_'))return t('agyHistoryPending');return `${t(code.includes('DSH_RECEIPT_UNKNOWN')?'dshUncertain':code.includes('DSH_HISTORY_TOO_LARGE')?'dshHistoryTooLarge':code.includes('DSH_HISTORY_TIMEOUT')?'dshHistoryTimeout':code.includes('DSH_HISTORY_UNAVAILABLE')?'dshHistoryUnavailable':'dshError')} · ${code.match(/DSH_[A-Z_]+/)?.[0] || 'DSH_UNAVAILABLE'}`}
async function load(){const g=++generation;clearTimeout(timer);busy.value=true;error.value='';selected.value=null;history.value=null;sessions.value=[];try{const items=await kernelBridge.dshList(connection.value);if(g===generation&&!disposed)sessions.value=items;}catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation)busy.value=false}}
watch(connection,()=>{if(connection.value&&!props.target)load()});
function validatePage(h,id){if(h.address.session_id!==id||h.address.connection_id!==connection.value||!Number.isSafeInteger(h.cursor)||!Number.isSafeInteger(h.before)||!Array.isArray(h.messages))throw Error('DSH_IDENTITY_MISMATCH')}
function combine(old,h){const map=new Map((old?.messages||[]).map(m=>[m.sequence,m]));for(const m of h.messages)map.set(m.sequence,m);const replacements=[...(old?.replacements||[]),...(h.replacements||[])];for(const op of replacements)for(const seq of map.keys())if(seq>=op.startSeq&&seq<=op.endSeq)map.delete(seq);return {...h,messages:[...map.values()].sort((a,b)=>a.sequence-b.sequence),replacements,workspace:h.workspace||old?.workspace}}
let olderRequested=false;
async function refresh(){if(selected.value?.historyReadable===false){error.value=t(selected.value?.historyReason==='AGY_HISTORY_NOT_LOCAL'?'agyHistoryNotLocal':'agyHistoryPending');return;}
 if(!selected.value||busy.value||loadingOlder.value||preview.value)return;
 clearTimeout(timer);const g=generation,id=selected.value.id;busy.value=true;
 if(history.value!==null){clearTimeout(syncTimer);syncStatus.value='checking';}
 try{
  const h=await kernelBridge.dshPage(connection.value,id,null,null,history.value?.cursor ?? null);validatePage(h,id);
  if(g!==generation||disposed||preview.value)return;
  await refreshActivity(g,id);if(g!==generation||disposed)return;
  if(h.request_results?.some(r=>r.request_id===awaitingReply.value&&['completed','error','blocked','cancelled'].includes(r.status))){awaitingReply.value=null;replying.value=false;}
  if(h.not_modified){
   error.value='';
   if(syncStatus.value==='checking'){syncStatus.value='synced';clearTimeout(syncTimer);syncTimer=setTimeout(()=>{if(syncStatus.value==='synced')syncStatus.value='';},1800);}
   return;
  }
  const first=history.value===null,prevCursor=history.value?.cursor??-1;let additions=h;
  if(!first&&(h.messages.length>0||h.cursor>prevCursor)){syncStatus.value='syncing';}
  if(!first&&awaitingReply.value&&h.messages.some(m=>m.role==='assistant'&&m.sequence>prevCursor)){replying.value=true;}
  if(!first&&notice.value===t('dshAccepted')&&h.messages.some(m=>m.role==='assistant'&&m.sequence>prevCursor))notice.value='';
  // Catch up a burst without losing messages between the old cursor and latest page.
  for(let n=0;!first&&additions.before>history.value.cursor+1&&additions.has_more;n++){
   if(n>=32)throw Error('DSH_PAGE_CATCHUP_REQUIRED');
   const older=await kernelBridge.dshPage(connection.value,id,additions.before,h.cursor);validatePage(older,id);
   if(g!==generation||disposed||preview.value)return;
   if(older.before>=additions.before)throw Error('DSH_PAGE_NO_PROGRESS');
   additions={...combine(older,additions),before:older.before,has_more:older.has_more};
  }
  const el=reader.value,follow=first||(el&&el.scrollHeight-el.scrollTop-el.clientHeight<80);
  const deltaAdded=!first&&additions.messages.some(m=>m.sequence>prevCursor);
  history.value=first?additions:{...combine(history.value,additions),before:history.value.before,has_more:history.value.has_more};error.value='';
  if(!first&&deltaAdded&&(replying.value||awaitingReply.value||nodeActivity.value?.running)){
   const lastAsst=[...(history.value?.messages||[])].reverse().find(m=>m.role!=='user'&&m.sequence>prevCursor);
   if(lastAsst)streamSeq.value=lastAsst.sequence;
  }
  if(!first){
   if(!follow&&deltaAdded)hasNewMessages.value=true;
   else if(follow)hasNewMessages.value=false;
   syncStatus.value='synced';clearTimeout(syncTimer);syncTimer=setTimeout(()=>{if(syncStatus.value==='synced')syncStatus.value='';},1800);
  }
  await nextTick();if(g===generation&&!disposed&&follow&&reader.value)reader.value.scrollTop=reader.value.scrollHeight;
  lastTop=reader.value?.scrollTop||0;saveHistory();
 }catch(e){if(g===generation){error.value=fault(e);nodeActivity.value=null;if(history.value!==null){clearTimeout(syncTimer);syncStatus.value='offline_cached';}}}finally{if(g===generation){busy.value=false;const pollMs=(awaitingReply.value||replying.value||nodeActivity.value?.running)?1800:5000;if(olderRequested&&reader.value?.scrollTop<=24){olderRequested=false;loadOlder()}else{olderRequested=false;timer=setTimeout(refresh,pollMs)}}}
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
function onScroll(){const el=reader.value,top=el?.scrollTop||0;const upwards=top<lastTop;lastTop=top;if(el&&el.scrollHeight-top-el.clientHeight<80)hasNewMessages.value=false;if(upwards&&top<=24){if(busy.value)olderRequested=true;else loadOlder()}else if(top>24)olderRequested=false}
async function forceRefresh(){if(!selected.value){if(connection.value)return load();return;}clearTimeout(timer);activityChecked=0;error.value='';if(pending.value&&!sending.value)return checkReceipt();await refresh();}
function onReaderTouchStart(e){if(!selected.value)return;const el=reader.value,atBottom=!el||(el.scrollHeight-el.scrollTop-el.clientHeight<=16);touchStartY=atBottom?(e?.touches?.[0]?.clientY??null):null;pullUpDistance.value=0;}
function onReaderTouchMove(e){if(touchStartY===null||!selected.value)return;const y=e?.touches?.[0]?.clientY;if(typeof y!=='number')return;const delta=touchStartY-y,el=reader.value,atBottom=!el||(el.scrollHeight-el.scrollTop-el.clientHeight<=20);if(delta>8&&atBottom)pullUpDistance.value=Math.min(72,Math.round((delta-8)*0.75));else if(delta<=0)pullUpDistance.value=0;}
function onReaderTouchEnd(){const dist=pullUpDistance.value;touchStartY=null;pullUpDistance.value=0;if(dist>=36)void forceRefresh();}
function onReaderWheel(e){if(!selected.value||!(e?.deltaY>24))return;const el=reader.value,atBottom=!el||(el.scrollHeight-el.scrollTop-el.clientHeight<=8);if(!atBottom){wheelOverscroll=0;return;}wheelOverscroll+=e.deltaY;clearTimeout(wheelResetTimer);wheelResetTimer=setTimeout(()=>{wheelOverscroll=0;},450);if(wheelOverscroll>=140){wheelOverscroll=0;void forceRefresh();}}
function openFilePicker(){fileInput.value?.click?.();}
async function onFilesSelected(e){const files=Array.from(e?.target?.files||[]);for(const f of files){let content='';try{if(typeof f.text==='function'&&(f.size||0)<=256*1024)content=await f.text();}catch{}attachments.value.push({id:crypto.randomUUID(),name:f.name||'file',size:f.size||0,type:f.type||'',content});}if(e?.target)e.target.value='';}
function removeAttachment(id){attachments.value=attachments.value.filter(a=>a.id!==id);}
function formatAttachmentBlock(list){if(!list?.length)return '';return list.map(a=>a.content?`[Attachment: ${a.name}]\n\`\`\`\n${String(a.content).slice(0,16384)}\n\`\`\``:`[Attachment: ${a.name} (${a.size||0} bytes)]`).join('\n\n');}
function onComposeKeydown(e){if(e?.key==='Enter'&&!e.shiftKey&&!e.isComposing){e.preventDefault();void send();}}
globalThis.__bobForceRefreshDsh=forceRefresh;
async function open(s){nodeActivity.value=null;awaitingReply.value=null;replying.value=false;hasNewMessages.value=false;streamSeq.value=null;syncStatus.value='';pullUpDistance.value=0;attachments.value=[];clearTimeout(syncTimer);activityChecked=0;olderRequested=false;artifactQueue=[];artifactCount=0;persist();++generation;clearTimeout(timer);busy.value=false;loadingOlder.value=false;lastTop=0;selected.value=s;history.value=null;notice.value='';const saved=readState();restoring=true;draft.value=saved.draft||'';pending.value=saved.pending||null;restoring=false;if(pending.value)notice.value=t('dshUncertain');const g=generation;const cached=await readingCache.get(historyKey());if(g!==generation||disposed)return;if(cached){try{validatePage(cached,s.id);history.value=cached;syncStatus.value='checking';await nextTick();if(reader.value)reader.value.scrollTop=reader.value.scrollHeight;lastTop=reader.value?.scrollTop||0;}catch{}}if(s.historyReadable===false){error.value=t(s.historyReason==='AGY_HISTORY_NOT_LOCAL'?'agyHistoryNotLocal':'agyHistoryPending');return;}await refresh()}
function back(){persist();++generation;clearTimeout(timer);clearTimeout(syncTimer);busy.value=false;selected.value=null;history.value=null;syncStatus.value='';hasNewMessages.value=false;notice.value='';load()}
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
function dismissPending(){if(!pending.value||sending.value)return;if(!draft.value.trim()&&pending.value.text)draft.value=pending.value.text;pending.value=null;notice.value='';error.value='';persist();}
async function send(){if(selected.value?.historyReadable===false){error.value=t('agyHistoryPending');return;}const attBlock=formatAttachmentBlock(attachments.value);const text=[draft.value.trim(),attBlock].filter(Boolean).join('\n\n');if(sending.value||pending.value||!text)return;const g=generation, address={connection:connection.value,session:selected.value.id};pending.value={id:crypto.randomUUID(),text};if(!persist()){pending.value=null;return;}sending.value=true;error.value='';try{const requestId=pending.value.id;const receipt=await kernelBridge.dshPrompt(address.connection,address.session,text,requestId);if(receipt?.status!=='accepted'||receipt.session_id!==address.session||receipt.request_id!==requestId)throw Error('DSH_RECEIPT_UNKNOWN');if(g===generation){awaitingReply.value=requestId;replying.value=false;activityChecked=0;pending.value=null;attachments.value=[];if(draft.value.trim()===text||attBlock)draft.value='';persist();notice.value=t('dshAccepted');}}catch(e){if(g===generation){error.value=fault(e);notice.value=t('dshUncertain')}}finally{sending.value=false;if(g===generation&&!busy.value)refresh()}}
watch(()=>props.target, async target=>{if(!target)return;connection.value=target.node.id;await open(target.session)},{immediate:true});
onUnmounted(()=>{if(globalThis.__bobForceRefreshDsh===forceRefresh)globalThis.__bobForceRefreshDsh=null;if(rodTimer&&typeof clearInterval==='function')clearInterval(rodTimer);clearTimeout(wheelResetTimer);clearTimeout(artifactTimer);clearTimeout(syncTimer);clearTimeout(copyTimer);artifactQueue=[];fileCache.clear();persist();disposed=true;++generation;clearTimeout(timer)});
</script>
<template>
 <DshFilePreview v-if="preview" :address="preview" @close="closePreview" />
 <section class="dsh-sessions" :class="{embedded:!!target}" :aria-label="t('dshTitle')">
  <div ref="reader" class="dsh-reader" tabindex="0" @scroll.passive="onScroll" @touchstart.passive="onReaderTouchStart" @touchmove.passive="onReaderTouchMove" @touchend.passive="onReaderTouchEnd" @touchcancel.passive="onReaderTouchEnd" @wheel.passive="onReaderWheel">
  <header v-if="!target"><h3>{{ t('dshTitle') }}</h3><select v-model="connection" :disabled="sending" :aria-label="t('dshChooseNode')"><option value="">{{ t('dshChooseNode') }}</option><option v-for="n in nodes" :key="n.id" :value="n.id">{{ n.label || n.name || n.baseUrl || n.url }}</option></select></header>
  <p v-if="!target" class="dsh-note">{{ t('dshSource') }}</p>
  <p v-if="error && syncStatus!=='offline_cached'" role="alert">{{ error }}</p><p v-if="notice" role="status">{{ notice }}</p>
  <template v-if="connection && !selected">
   <div class="dsh-tools"><input v-model="search" type="search" :placeholder="t('dshSearch')" :aria-label="t('dshSearch')"><button :disabled="busy" @click="load">{{ t('dshRefresh') }}</button></div>
   <p v-if="busy" class="terminal-status-line" role="status"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ stripDots(t('dshLoading')) }}</span><span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></p>
   <details v-for="group in groups" :key="group.name" :open="!!search"><summary>{{ group.name }} · {{ group.items.length }}</summary><button v-for="s in group.items" :key="s.id" class="dsh-session" @click="open(s)"><strong>{{ s.title }}</strong><small>{{ s.updated_at ? new Date(s.updated_at).toLocaleString() : t('sidebarTimeUnknown') }} · {{ t(s.running?'dshRunning':'dshIdle') }}</small></button></details>
   <p v-if="!busy && !groups.length">{{ t('dshEmpty') }}</p>
  </template>
  <template v-if="selected">
   <div class="dsh-tools"><button v-if="!target" :disabled="sending" @click="back">{{ t('dshBack') }}</button><h4 v-if="!target">{{ selected.title }}</h4><small v-else>{{ target.node.label || target.node.name }} · {{ agentLabel }}</small><span v-if="syncStatusLabel" class="dsh-sync-pill" :class="{active:syncStatus==='checking'||syncStatus==='syncing'}" role="status" aria-live="polite"><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ syncStatusLabel }}</span><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></span></div>
   <p class="dsh-note">{{ history?.workspace || t('dshNoWorkspace') }}</p>
   <p v-if="history?.partial || history?.omitted" class="dsh-note">{{ t('dshPartial') }}</p>
   <p v-if="busy && !history" class="terminal-status-line" role="status"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ stripDots(t('dshLoading')) }}</span><span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></p>
   <button v-if="history?.has_more" class="dsh-older" :disabled="loadingOlder || busy" @click="loadOlder"><span v-if="loadingOlder" class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span> {{ t(loadingOlder ? 'dshLoading' : 'dshLoadOlder') }}</button>
   <article v-for="(m, idx) in displayMessages" :key="m.sequence" :data-sequence="m.sequence" class="dsh-message" :class="{user:m.role==='user'}"><div class="dsh-message-meta"><small class="dsh-role">{{ m.role==='user'?t('dshYou'):agentLabel }}</small><time v-if="formatMessageTime(m.created_at || m.createdAt || m.timestamp)" class="dsh-time">{{ formatMessageTime(m.created_at || m.createdAt || m.timestamp) }}</time><button type="button" class="dsh-copy-btn" @click="copyMessage(m)">{{ t(copiedSeq===m.sequence?'copied':'copyMessage') }}</button></div><MessageContent :content="m.text" :thinking-steps="m.thinkingSteps || []" :plain="m.role==='user'" :stream="m.role!=='user' && idx===displayMessages.length-1 && m.sequence===streamSeq" /></article>
   <div v-if="pullUpDistance > 10" class="pull-up-refresh-hint" :class="{ready: pullUpDistance >= 36}" role="status" aria-live="polite"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ t(pullUpDistance >= 36 ? 'releaseToRefresh' : 'pullUpToRefresh') }}</span></div>
  </template>
  </div>
  <div v-if="selected && hasNewMessages" class="dsh-new-msg-bar"><button type="button" class="dsh-new-msg-btn" @click="scrollToBottom">{{ t('dshNewMessagesBtn') }}</button></div>
  <form v-if="selected" class="dsh-compose" @submit.prevent="send">
   <p v-if="activityLabel" class="dsh-activity" role="status" aria-live="polite"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span> <span>{{activityLabel}}</span><span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></p>
   <div class="dsh-compose-box">
    <textarea v-model="draft" :disabled="sending" :placeholder="t('dshPrompt')" :aria-label="t('dshPrompt')" rows="2" @keydown="onComposeKeydown"></textarea>
    <div v-if="attachments.length" class="composer-attachments"><span v-for="att in attachments" :key="att.id" class="composer-attachment-chip"><span class="chip-file-name">{{ att.name }}</span><button type="button" class="chip-remove-btn" :aria-label="t('removeAttachment')" @click="removeAttachment(att.id)">×</button></span></div>
    <div class="dsh-compose-toolbar">
     <div class="dsh-compose-left-tools">
      <input ref="fileInput" type="file" multiple class="visually-hidden" @change="onFilesSelected" />
      <button type="button" class="composer-tool-btn" :disabled="sending" :title="t('attachFileBtn')" :aria-label="t('attachFileBtn')" @click="openFilePicker"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg></button>
      <span v-if="syncStatusLabel" class="dsh-sync-pill" :class="{active:syncStatus==='checking'||syncStatus==='syncing'}"><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ syncStatusLabel }}</span><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></span>
     </div>
     <button type="submit" class="composer-submit-btn" :disabled="sending || !!pending || (!draft.trim() && !attachments.length) || !history">{{ t(sending?'dshSending':'send') }}</button>
    </div>
   </div>
   <p v-if="pending && !sending" role="status">{{ t('dshUncertain') }} <button type="button" @click="checkReceipt">{{ t('dshCheckReceipt') }}</button> <button type="button" @click="dismissPending">{{ t('dshUnlockEdit') }}</button></p>
  </form>
 </section>
</template>
<style scoped>
.dsh-sessions{min-width:0;max-width:100%;padding:16px;border:1px solid var(--border-color);border-radius:12px;background:var(--bg-primary)}
.dsh-sessions.embedded{border:0;border-radius:0;padding:0;flex:1;min-height:0;overflow:hidden;display:flex;flex-direction:column}.dsh-reader{min-height:0;flex:1;overflow-y:auto;overflow-x:hidden;scrollbar-gutter:stable;overflow-anchor:none;padding:12px;overscroll-behavior:contain}.dsh-compose{flex-shrink:0;padding:8px 12px 10px;background:var(--bg-primary);border-top:1px solid var(--border-color);display:flex;flex-direction:column;gap:6px}.dsh-compose-box{border:1px solid var(--border-color);border-radius:12px;padding:7px 10px;background:var(--bg-secondary);display:flex;flex-direction:column;gap:6px;transition:border-color .15s ease}.dsh-compose-box:focus-within{border-color:var(--accent-primary)}.dsh-compose-box textarea{width:100%;border:0;outline:none;background:transparent;padding:2px;resize:vertical;min-height:2.6rem;max-height:7.5rem}.dsh-compose-toolbar{display:flex;align-items:center;justify-content:space-between;gap:8px}.dsh-compose-left-tools{display:flex;align-items:center;gap:6px;min-width:0;flex:1;flex-wrap:wrap}.dsh-older{display:block;margin:8px auto;background:transparent;border:0;color:var(--text-secondary)}header,.dsh-tools{display:flex;align-items:center;gap:8px;flex-wrap:wrap}h3,h4{margin:0;overflow-wrap:anywhere}select,input,textarea{min-width:0;max-width:100%;color:var(--text-primary);background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:8px;padding:8px}select,input{flex:1}button{max-width:100%;white-space:normal}details{margin-top:10px}summary{overflow-wrap:anywhere;cursor:pointer;padding:8px 0}.dsh-note,small{color:var(--text-secondary);overflow-wrap:anywhere}.dsh-sync-pill{display:inline-flex;align-items:center;gap:5px;font-size:.75rem;padding:2px 8px;border-radius:999px;background:var(--bg-primary);color:var(--text-secondary);border:1px solid var(--border-color)}.dsh-tools .dsh-sync-pill{margin-left:auto}.dsh-sync-pill.active{color:var(--accent-primary);border-color:var(--accent-primary)}.dsh-session{display:flex;flex-direction:column;text-align:left;width:100%;gap:5px;margin-top:5px;padding:10px}.dsh-session strong{overflow-wrap:anywhere}.dsh-message{min-width:0;margin-top:20px;overflow-wrap:anywhere}.dsh-message-meta{display:flex;align-items:center;gap:8px;margin-bottom:6px;flex-wrap:wrap}.dsh-message.user{background:var(--bg-secondary);border-radius:12px;padding:14px 16px;width:fit-content;max-width:88%;margin-left:auto}.dsh-message.user .dsh-message-meta{justify-content:flex-end}.dsh-message:not(.user){padding:4px 2px}.dsh-time{font-size:.75rem;color:var(--text-secondary)}.dsh-copy-btn{padding:2px 8px;font-size:.75rem;border-radius:6px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--text-secondary);cursor:pointer}@media(hover:hover) and (pointer:fine){.dsh-copy-btn{opacity:0;transition:opacity .15s ease}.dsh-message:hover .dsh-copy-btn,.dsh-copy-btn:focus-visible{opacity:1}}.dsh-new-msg-bar{display:flex;justify-content:center;padding:4px 12px;background:transparent}.dsh-new-msg-btn{padding:4px 14px;border-radius:999px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--accent-primary);font-size:.82rem;cursor:pointer}.dsh-activity{margin:0;display:flex;align-items:center;gap:6px;color:var(--text-secondary);font-size:.82rem}.dsh-compose p{width:100%;margin:0}
</style>
