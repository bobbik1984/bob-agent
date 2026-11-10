<script setup>
import {artifactPath} from '../artifact-path.mjs';
import {isNotAcceptedReceipt} from '../native-receipt.mjs';
import NativeActivity from './NativeActivity.vue';
import {unconfirmActivity,settleNativeActivity,orderedNativeActivities,waitingNativeApproval} from '../native-activity.mjs';
import { computed, ref, watch, onUnmounted, nextTick, provide } from 'vue';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from '../native-bridge.mjs';
import MessageContent from './MessageContent.vue';
import {createChatScroll} from '../chat-scroll.mjs';
import DshFilePreview from './DshFilePreview.vue';
import HarnessIcon from './HarnessIcon.vue';
import CustomSelect from './CustomSelect.vue';
import {readingCache,readingKey,connectionPrefix} from '../reading-cache.mjs';
import {isTestOrScratchSession} from '../conversation-directory.mjs';
import {createPersistentPreviewCache} from '../persistent-preview-cache.mjs';
const props=defineProps({nodes:{type:Array,default:()=>[]}, target:{type:Object,default:null}});
const {t}=useI18n();
const connection=ref(''), sessions=ref([]), selected=ref(null), history=ref(null), search=ref(''), busy=ref(false), sending=ref(false), error=ref(''), notice=ref(''), draft=ref(''), pending=ref(null);
const reader=ref(null), loadingOlder=ref(false);let lastTop=0;
const awayFromLatest=ref(false),chatContent=ref(null);
const chatScroll=createChatScroll(value=>{awayFromLatest.value=value;});
watch([reader,chatContent],()=>chatScroll.bind(reader.value),{flush:'post'});
const pullUpDistance=ref(0),attachments=ref([]),fileInput=ref(null);let touchStartY=null,wheelOverscroll=0,wheelResetTimer;
const approvalMode=ref('interactive'),takeoverPrompt=ref(null),terminalHolderOverride=ref(null);
const activeTerminalHolder=computed(()=>terminalHolderOverride.value||history.value?.terminal_holder||null);
const isClaudeRemoteControl=computed(()=>Boolean(activeTerminalHolder.value?.busy&&activeTerminalHolder.value?.remote_control));
const isClaudeTerminalBusy=computed(()=>Boolean(activeTerminalHolder.value?.busy&&!activeTerminalHolder.value?.remote_control));
const supportsApprovalMode=computed(()=>/^(claude-jsonl|agy):/.test(selected.value?.id||''));
const permissionModeOptions=computed(()=>[
  { value: 'interactive', label: t('permissionModeInteractive') },
  { value: 'read_only', label: t('permissionModeReadOnly') },
  { value: 'deny', label: t('permissionModePlan') },
]);
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
let syncPulseTimer;
function emitTopbarPhase(overridePhase){
 const fn=globalThis.__bobSetRemoteSyncPhase;if(typeof fn!=='function')return;
 if(overridePhase){fn(overridePhase);return;}
 if(!selected.value){fn('');return;}
 if(syncStatus.value==='offline_cached'){fn('offline');return;}
 if(syncStatus.value==='syncing'||sending.value||replying.value||nodeActivity.value?.running){fn('syncing');return;}
 if(syncStatus.value==='checking'||(busy.value&&!history.value)){fn('checking');return;}
 fn('synced');
}
watch([syncStatus,sending,replying,()=>nodeActivity.value?.running,busy],()=>emitTopbarPhase());
const dshLiveState=ref(null);let dshLiveTimer,dshReadCursor=-1;
const isNativeId=id=>/^(agy|claude-jsonl|codex|pi):/.test(id||'');
const dshQuestionView=ref(null);
let checkedDshQuestions=false;
const dshQuestions=computed(()=>dshQuestionView.value?.pending_questions||[]);
const dshQuestionBlocking=computed(()=>dshQuestions.value.some(q=>q.state==='open'));
const dshQuestionSource=computed(()=>{
 try{const url=new URL(dshQuestionView.value?.question_source);return url.protocol==='https:'&&!url.username&&!url.password&&!url.search&&!url.hash&&url.pathname==='/'?url.href:null;}
 catch{return null;}
});
function observeDshQuestions(page){
 if(!Array.isArray(page.pending_questions)||page.cursor<(dshQuestionView.value?.cursor??-1))return;
 dshQuestionView.value={cursor:page.cursor,pending_questions:page.pending_questions,question_source:page.question_source};
}
const dshLiveMessages=computed(()=> (dshLiveState.value?.messages||[]).filter(m=>m.text&&(m.active||!Number.isSafeInteger(m.sequence)||m.sequence>(history.value?.cursor??-1))));
async function refreshDshLive(){
 const g=generation,id=selected.value?.id,cid=connection.value;
 if(!id||isNativeId(id)||disposed||!kernelBridge.dshLive)return;
 try{
  const page=await kernelBridge.dshLive(cid,id);
  if(g!==generation||disposed)return;
  if(page.address?.session_id!==id||page.address?.connection_id!==cid||!Array.isArray(page.messages)||!Number.isSafeInteger(page.cursor))throw Error('DSH_IDENTITY_MISMATCH');
  const el=reader.value,follow=el&&el.scrollHeight-el.scrollTop-el.clientHeight<80;
  const changed=JSON.stringify(page.messages)!==JSON.stringify(dshLiveState.value?.messages||[]);
  if(page.status==='live')dshLiveState.value=page;
  else dshLiveState.value={...page,messages:(dshLiveState.value?.messages||[]).map(m=>({...m,active:false}))};
  replying.value=page.status==='live'&&page.active===true;
  const latest=Math.max(page.cursor,...page.messages.map(m=>Number.isSafeInteger(m.sequence)?m.sequence:-1));
  if(history.value&&latest>history.value.cursor&&latest>dshReadCursor&&!busy.value&&!loadingOlder.value&&!preview.value){dshReadCursor=latest;void refresh();}
  await nextTick();
  if(g!==generation||disposed)return;
  if(changed&&!preview.value){if(follow&&chatScroll.following)scrollToBottom();else hasNewMessages.value=true;}
 }catch(e){if(g===generation&&!disposed){dshLiveState.value={...(dshLiveState.value||{}),status:'reconnecting',messages:(dshLiveState.value?.messages||[]).map(m=>({...m,active:false}))};replying.value=false;}}
 finally{if(g===generation&&!disposed)dshLiveTimer=setTimeout(refreshDshLive,dshLiveState.value?.status==='live'?1000:3000);}
}
const cancelRequest=ref(null),cancelBusy=ref(false);
const dshStop=ref(null);
let dshStopTimer;
const canStopDsh=computed(()=>!!selected.value&&!isNativeId(selected.value.id)&&
 (dshStop.value!==null||(syncStatus.value!=='offline_cached'&&
 (dshQuestionBlocking.value||nodeActivity.value?.running===true||replying.value))));
function resetDshStop(){clearTimeout(dshStopTimer);dshStop.value=null;}
function settleDshIdle(){
 resetDshStop();replying.value=false;nodeActivity.value={running:false,at:Date.now()};
 notice.value=t('dshStopIdle');
}
async function stopDsh(){
 const id=selected.value?.id,cid=connection.value,g=generation;
 if(!id||!canStopDsh.value||dshStop.value||sending.value)return;
 dshStop.value={checking:false};notice.value='';
 try{
  const result=await kernelBridge.dshCancel(cid,id);
  if(g!==generation||disposed)return;
  if(result.address?.session_id!==id||result.address?.connection_id!==cid)throw Error('DSH_IDENTITY_MISMATCH');
  if(result.status==='idle'){settleDshIdle();return;}
  if(result.status!=='cancel_requested')throw Error('DSH_CANCEL_UNCONFIRMED');
  dshStop.value={checking:true};activityChecked=0;notice.value=t('dshStopChecking');
  dshStopTimer=setTimeout(()=>{if(g===generation&&dshStop.value){resetDshStop();notice.value=t('dshStopUnconfirmed');}},30000);
 }catch{if(g===generation&&!disposed){resetDshStop();notice.value=t('dshStopUnconfirmed');}}
 finally{if(g===generation&&!disposed)void refresh();}
}
const cancellableRequest=computed(()=>/^(codex|pi|agy|claude-jsonl):/.test(selected.value?.id||'')?Object.entries(nativeDrafts.value).find(([,v])=>v.active&&v.cancelSupported)?.[0]:null);
async function cancelNative(){
 const rid=cancellableRequest.value,g=generation;if(!rid||cancelBusy.value||cancelRequest.value===rid)return;
 cancelBusy.value=true;
 try{const result=await kernelBridge.nativeCancel(connection.value,selected.value.id,rid);
  if(g!==generation)return;
  if(result.request_id!==rid||result.session_id!==selected.value.id)throw Error('HARNESS_IDENTITY_UNCONFIRMED');
  cancelRequest.value=result.status==='cancel_requested'?rid:null;
  notice.value=t(result.status==='cancelled'?'nativeCancelled':result.status==='completed'?'dshSyncCompleted':'nativeCancelPending');
 }catch{if(g===generation)notice.value=t('nativeCancelUnconfirmed');}
 finally{cancelBusy.value=false;}
}
const nativeDrafts=ref({});
const nativeUnresolved=computed(()=>Object.entries(nativeDrafts.value).some(([k,v])=>!k.startsWith('external-codex-')&&v.status==='unknown')||(history.value?.request_results||[]).some(v=>v.status==='unknown'));const isExternalCodexWaitingApproval=computed(()=>Object.entries(nativeDrafts.value).some(([k,v])=>k.startsWith('external-codex-')&&(v.activity?.approvals||[]).some(a=>a.status==='pending')));const isExternalCodexRunning=computed(()=>Object.entries(nativeDrafts.value).some(([k,v])=>k.startsWith('external-codex-')&&v.active));let nativeCursor=0,nativeEpoch='',nativeTimer,nativeMore=false;
async function refreshNativeEvents(){
 const g=generation,id=selected.value?.id;
 nativeMore=false;
 if(!id||! /^(agy|claude-jsonl|codex|pi):/.test(id)||disposed)return;
 try{
  const page=await kernelBridge.nativeEvents(connection.value,id,nativeCursor,nativeEpoch);
  if(g!==generation||disposed||page.session_id!==id)return;
  const el=reader.value,follow=el&&el.scrollHeight-el.scrollTop-el.clientHeight<80;
  if(page.reset){
   if(!Number.isSafeInteger(page.cursor)||typeof page.epoch!=='string'||!Array.isArray(page.snapshots)||page.snapshots.some(v=>v.session_id!==id))throw Error('HARNESS_EVENTS_INVALID');
   const restored={};cancelRequest.value=null;
   for(const v of page.snapshots){
    restored[v.request_id]={status:v.status,activity:v.activity||null,text:v.text||'',unconfirmed:v.status==='unknown'&&v.code==='HARNESS_EXTERNAL_TURN_UNCONFIRMED',timestamp:v.observed_at,active:['accepted','running'].includes(v.status),completed:v.status==='completed',terminal:['completed','cancelled','error'].includes(v.status),cancelSupported:v.cancel_supported===true};
    if(v.cancel_pending)cancelRequest.value=v.request_id;
    if(v.request_id===awaitingReply.value&&['completed','cancelled','error'].includes(v.status))awaitingReply.value=null;
   }
   nativeDrafts.value=restored;nativeCursor=page.cursor;nativeEpoch=page.epoch;
   replying.value=Object.values(restored).some(v=>v.active);
   notice.value=t(page.snapshot_incomplete?'nativeRecoveryPartial':'nativeRecoveryRestored');
   if(page.history_required&&!busy.value&&!loadingOlder.value&&!preview.value)void refresh();
  }else if(page.epoch){
   if(nativeEpoch&&page.epoch!==nativeEpoch)throw Error('HARNESS_EVENTS_EPOCH_CHANGED');
   nativeEpoch=page.epoch;
  }
  for(const event of page.events||[]){
   if(event.sequence<=nativeCursor||event.session_id!==id)continue;
   nativeCursor=event.sequence;
   const rid=event.request_id;
   const settled=(history.value?.request_results||[]).find(r=>r.request_id===rid)?.status;
   if(settled==='completed'||(['error','not_accepted','cancelled'].includes(settled)&&rid!==awaitingReply.value)){
    const retained=settleNativeActivity(nativeDrafts.value[rid]||{timestamp:event.observed_at},settled,event.kind==='activity_snapshot'?event.payload.activity:undefined);
    if(retained)retained.status=settled;
    const next={...nativeDrafts.value};if(retained)next[rid]=retained;else delete next[rid];nativeDrafts.value=next;
    if(rid===awaitingReply.value){awaitingReply.value=null;replying.value=false;}
    continue;
   }
   const current=nativeDrafts.value[rid]||{text:'',timestamp:event.observed_at};
   if(['accepted','running','completed','cancelled','error','unknown'].includes(event.kind))current.status=event.kind;
   if(current.terminal&&!['message_snapshot','activity_snapshot'].includes(event.kind))continue;
   if(['accepted','running','completed'].includes(event.kind))current.unconfirmed=false;
   if(event.kind==='unknown'&&event.payload?.code==='HARNESS_EXTERNAL_TURN_UNCONFIRMED')current.unconfirmed=true;
   if(event.kind==='activity_snapshot')current.activity=current.terminal?unconfirmActivity(event.payload.activity):event.payload.activity||null;
   if(event.kind==='accepted')current.cancelSupported=event.payload.cancel_supported===true;
    if(event.kind==='text_delta')current.text+=event.payload.text||'';
   if(event.kind==='message_snapshot')current.text=event.payload.text||'';
    if(event.kind==='cancel_requested'){cancelRequest.value=rid;notice.value=t('nativeCancelPending');}
   if(['accepted','running','text_delta'].includes(event.kind)){current.active=true;replying.value=true;}
   if(['completed','error','cancelled','unknown'].includes(event.kind)){
    current.active=false;current.terminal=['completed','cancelled','error'].includes(event.kind);replying.value=false;
    current.activity=unconfirmActivity(current.activity);
     if(cancelRequest.value===rid){cancelRequest.value=null;notice.value=t(event.kind==='cancelled'?'nativeCancelled':event.kind==='completed'?'dshSyncCompleted':'nativeCancelUnconfirmed');}
    if(awaitingReply.value===rid)awaitingReply.value=null;
    if((event.kind==='error'||event.kind==='unknown')&&!rid.startsWith('external-codex-'))error.value=t('nativeExecutionFailed')+' · '+String(event.payload.code||'HARNESS_FAILED');
    if(event.kind==='completed'){current.completed=true;error.value='';}
   }
   nativeDrafts.value={...nativeDrafts.value,[rid]:current};
  }
   if(Number.isSafeInteger(page.cursor))nativeCursor=Math.max(nativeCursor,page.cursor);
   nativeMore=page.has_more===true;
   if(page.external_live && page.external_live!=='live'){
    let touched=false;const next={...nativeDrafts.value};
    for(const [key,value] of Object.entries(next))if(key.startsWith('external-codex-')&&value.active){next[key]={...value,active:false,activity:unconfirmActivity(value.activity)};touched=true;}
    if(touched){nativeDrafts.value=next;replying.value=Object.values(next).some(v=>v.active);}
   }
   // History can settle a request before its terminal event reaches this poll.
   // Derive the indicator from all retained executions, including empty replies.
   replying.value=Object.values(nativeDrafts.value).some(v=>v.active===true);
   if(pendingDecision.value){
    const record=nativeDrafts.value[pendingDecision.value.request];
    const approval=record?.activity?.approvals?.find(v=>v.permission_id===pendingDecision.value.permission);
    if(approval&& !['pending','queued'].includes(approval.status)){
     notice.value=t(approval.status==='denied'?'nativeApprovalDenied':approval.status==='approved'?'nativeApprovalApproved':'nativeApprovalUnconfirmed');
     pendingDecision.value=null;
    }
   }
   await nextTick();if(follow&&chatScroll.following&&g===generation)scrollToBottom();
 }catch(e){if(g===generation){syncStatus.value='offline_cached';nativeEpoch='';replying.value=false;nativeDrafts.value=Object.fromEntries(Object.entries(nativeDrafts.value).map(([k,v])=>[k,{...v,active:false,activity:unconfirmActivity(v.activity)}]));}}
 finally{if(g===generation&&!disposed)nativeTimer=setTimeout(refreshNativeEvents,nativeMore?0:replying.value||awaitingReply.value?400:5000);}
}
// A live turn can contain several assistant records separated by tool output.
// Compare ordered paragraphs within one user-bounded run, never across turns.
function nativeTextInHistory(text){
 const normalize=value=>String(value??'').replace(/\s+/g,' ').trim();
 const chunks=String(text??'').split(/\n\s*\n/).map(normalize).filter(Boolean);
 if(!chunks.length)return false;
 const runs=[];let run=[];
 for(const m of history.value?.messages||[]){if(m.role==='user'){if(run.length)runs.push(run.join(' '));run=[];}else if(m.role==='assistant')run.push(normalize(m.text));}
 if(run.length)runs.push(run.join(' '));
 return runs.some(body=>{let offset=0;for(const chunk of chunks){const at=body.indexOf(chunk,offset);if(at<0)return false;offset=at+chunk.length;}return true;});
}
const nativeActivities=computed(()=>orderedNativeActivities(nativeDrafts.value));
const busyPermission=ref(''),pendingDecision=ref(null),approvalErrors=ref({});
async function decidePermission(requestId,permissionId,optionId){
 if(busyPermission.value||!selected.value)return;
 const g=generation,address={connection:connection.value,session:selected.value.id};busyPermission.value=permissionId;
 try{
  const result=await kernelBridge.nativePermission(address.connection,address.session,requestId,permissionId,optionId);
  if(result?.error)throw Error(String(result.error?.code||result.error));
  if(result?.permission_id!==permissionId)throw Error('HARNESS_APPROVAL_UNCONFIRMED');
  if(g===generation){delete approvalErrors.value[permissionId];pendingDecision.value={request:requestId,permission:permissionId};notice.value=t('nativeApprovalQueued');await refreshNativeEvents();}
 }catch(e){if(g===generation)approvalErrors.value[permissionId]=String(e?.message||e).includes('HARNESS_TOOL_NOT_GRANTED')?'nativeApprovalRevoked':'nativeApprovalUnconfirmed';}
 finally{busyPermission.value='';}
}

const nativeMessages=computed(()=>{
 const time=value=>typeof value==='number'?value:Date.parse(value)||0;
 const latestUser=Math.max(0,...(history.value?.messages||[]).filter(m=>m.role==='user').map(m=>time(m.created_at||m.createdAt||m.timestamp)));
 // Historical checkpoints restore receipt state; they are not new replies to a later user turn.
 return Object.entries(nativeDrafts.value).filter(([id,v])=>v.text&&!v.unconfirmed&&!(v.terminal&&id!==awaitingReply.value&&time(v.timestamp)>0&&time(v.timestamp)<latestUser)&&!nativeTextInHistory(v.text)).map(([id,v])=>({...v,id})).sort((a,b)=>time(a.timestamp)-time(b.timestamp));
});
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
async function refreshActivity(g,id){
 const stop=dshStop.value;
 if(Date.now()-activityChecked<(stop?.checking?1800:10000))return;
 activityChecked=Date.now();
 try{
  const list=await kernelBridge.dshList(connection.value);if(g!==generation||disposed)return;
  const row=list.find(s=>s.id===id);nodeActivity.value=typeof row?.running==='boolean'?{running:row.running,at:Date.now()}:null;
  if(stop?.checking&&dshStop.value===stop&&row?.running===false)settleDshIdle();
 }catch{if(g===generation)nodeActivity.value=null}
}
const activityLabel=computed(()=>stripDots(dshQuestionBlocking.value?t('dshQuestionWaiting'):sending.value?t('dshSendingWaitReceipt'):waitingNativeApproval(nativeDrafts.value)?t('nativeApprovalPending'):isExternalCodexWaitingApproval.value?t('codexExternalWaitingApproval'):isExternalCodexRunning.value&&!awaitingReply.value?t('codexExternalActiveBadge'):replying.value?t('dshReplying'):nodeActivity.value?.running===true?t('dshNodeProcessing'):awaitingReply.value?t('dshAwaitingReply'):''));
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
 chatScroll.bind(reader.value);chatScroll.bottom();
 hasNewMessages.value=false;
 if(reader.value){reader.value.scrollTop=reader.value.scrollHeight;lastTop=reader.value.scrollTop;}
}
const preview=ref(null);let previewScroll=0;
const visualReelOpen = ref(false);
function openAssetPreview(path,sequence) {
  previewScroll = reader.value?.scrollTop || 0;
  clearTimeout(timer);
  preview.value = fileAddress(path,sequence);
}
const sessionVisualAssets = computed(() => {
  if (!selected.value || !displayMessages.value?.length) return [];
  const assetsMap = new Map();
  const fileRegex = /(?:!?\[([^\]]*)\]\(([^)]+)\)|`([^`]+)`|([a-zA-Z0-9_\-./\\]+\.(?:png|jpe?g|webp|svg|html?))|(\/[^\s`"'<>]+\.(?:png|jpe?g|webp|svg|html?)))/gi;
  for (const m of displayMessages.value) {
    const text = m.text || '';
    let match;
    while ((match = fileRegex.exec(text)) !== null) {
      const candidate = match[2] || match[3] || match[4] || match[5];
      if (!candidate) continue;
      const clean = candidate.trim().split('#')[0].split('?')[0];
      const extMatch = clean.match(/\.(png|jpe?g|webp|svg|html?)$/i);
      if (extMatch) {
        const ext = extMatch[1].toLowerCase();
        const isHtml = ext === 'html' || ext === 'htm';
        const name = clean.split('/').pop().split('\\').pop();
        if (!assetsMap.has(clean)) {
          assetsMap.set(clean, {
            path: clean,
            messageSequence: m.sequence,
            name: (match[1] && !match[1].startsWith('http')) ? match[1] : name,
            ext,
            isHtml,
            role: m.role
          });
        }
      }
    }
  }
  return Array.from(assetsMap.values());
});
const fileCache=createPersistentPreviewCache((a,known)=>kernelBridge.dshPreview(a.connectionId,a.sessionId,a.path,known,a.messageSequence));
const fileAddress=(path,messageSequence=null)=>({messageSequence,connectionId:connection.value,sessionId:selected.value.id,endpoint:nodes.value.find(n=>n.id===connection.value)?.baseUrl||nodes.value.find(n=>n.id===connection.value)?.url||'',path:artifactPath(path),cursor:history.value?.cursor});
let artifactQueue=[],artifactTimer,artifactRunning=false,artifactCount=0;
async function drainArtifacts(){if(artifactRunning||disposed)return;artifactQueue=artifactQueue.filter(x=>x.visible());artifactQueue.sort((a,b)=>b.element.getBoundingClientRect().top-a.element.getBoundingClientRect().top);const item=artifactQueue.shift();if(!item||artifactCount>=6)return;artifactCount++;artifactRunning=true;try{await item.run()}finally{artifactRunning=false;artifactTimer=setTimeout(drainArtifacts,150)}}
provide('dshArtifacts',{peek:(path,seq)=>fileCache.peek(fileAddress(path,seq)),get:(path,seq)=>fileCache.get(fileAddress(path,seq)),enqueue:item=>{if(!artifactQueue.some(x=>x.element===item.element))artifactQueue.push(item);clearTimeout(artifactTimer);artifactTimer=setTimeout(drainArtifacts,250)}});
provide('dshPreviewGet',async(a,onUpdate)=>{
 const cached=await fileCache.peek(a);
 if(!cached)return fileCache.get(a);
 // Return the saved view immediately, then verify its complete resource version.
 setTimeout(()=>fileCache.get(a,{force:true}).then(f=>onUpdate?.(f)).catch(()=>{}),0);
 return {...cached,from_cache:true};
});
async function closePreview(){preview.value=null;await nextTick();if(reader.value){reader.value.scrollTop=previewScroll;lastTop=previewScroll;}clearTimeout(timer);timer=setTimeout(refresh,60000)}
provide('dshOpenFile',(path,seq)=>{previewScroll=reader.value?.scrollTop||0;clearTimeout(timer);preview.value=fileAddress(path,seq)});
let generation=0, disposed=false, restoring=false, timer;
const nodes=computed(()=>props.nodes.filter(n=>n.enabled!==false));
const historyKey=()=>readingKey(connection.value,'history',nodes.value.find(n=>n.id===connection.value)?.baseUrl||nodes.value.find(n=>n.id===connection.value)?.url||'',selected.value?.id);
function saveHistory(){if(!history.value)return;const {pending_questions,question_source,...cacheable}=history.value;const messages=history.value.messages.slice(-10);const h={...cacheable,messages,before:messages[0]?.sequence??history.value.before,has_more:history.value.has_more||history.value.messages.length>10};void readingCache.put(historyKey(),h)}
const key=()=>`bob-dsh:${connection.value}:${selected.value?.id || ''}`;
function readState(){try{return JSON.parse(localStorage.getItem(key()) || '{}')}catch{return {}}}
function persist(){if(restoring || !selected.value)return false;try{localStorage.setItem(key(),JSON.stringify({draft:draft.value,pending:pending.value}));return true}catch{error.value=t('dshStorageError');return false}}
watch(draft,persist,{flush:'sync'});
const isListening=ref(false);
let speechRec=null;
function toggleVoiceInput(){
 if(isListening.value){stopVoiceInput();return;}
 const SR=globalThis.SpeechRecognition||globalThis.webkitSpeechRecognition;
 if(!SR){notice.value=t('voiceInputUnsupported');return;}
 try{
  const rec=new SR();
  rec.continuous=false;
  rec.interimResults=false;
  rec.lang=globalThis.navigator?.language||'zh-CN';
  rec.onstart=()=>{isListening.value=true;};
  rec.onresult=e=>{
   const text=Array.from(e.results).map(r=>r[0]?.transcript||'').join('');
   if(text){draft.value=draft.value?`${draft.value} ${text}`:text;persist();}
  };
  rec.onerror=e=>{isListening.value=false;if(e.error!=='no-speech')notice.value=t('voiceInputError');};
  rec.onend=()=>{isListening.value=false;speechRec=null;};
  speechRec=rec;
  rec.start();
 }catch{isListening.value=false;notice.value=t('voiceInputUnsupported');}
}
function stopVoiceInput(){if(speechRec){try{speechRec.stop();}catch{}speechRec=null;}isListening.value=false;}

const archivedSessionIds=ref(new Set());
try{
 const raw=localStorage?.getItem?.('bob-dsh-archived-sessions');
 if(raw){const arr=JSON.parse(raw);if(Array.isArray(arr))archivedSessionIds.value=new Set(arr);}
}catch{}
function toggleArchiveSession(id,e){
 e?.stopPropagation?.();
 const next=new Set(archivedSessionIds.value);
 if(next.has(id))next.delete(id);else next.add(id);
 archivedSessionIds.value=next;
 try{localStorage?.setItem?.('bob-dsh-archived-sessions',JSON.stringify([...next]));}catch{}
}
const searchMatches=ref(new Map());
let searchTimer=null;
watch([search,connection],async([q,conn])=>{
 clearTimeout(searchTimer);
 if(!q?.trim()||!conn){searchMatches.value=new Map();return;}
 searchTimer=setTimeout(async()=>{
  try{
   const matches=await readingCache.findMatches(connectionPrefix(conn),q);
   const map=new Map();
   for(const m of matches){if(m.sessionId)map.set(m.sessionId,m.snippet);}
   searchMatches.value=map;
  }catch{}
 },120);
});

const groups=computed(()=>{
 const map=new Map();
 for(const s of sessions.value){
  if(archivedSessionIds.value.has(s.id))continue;
  if(isTestOrScratchSession(s))continue;
  const q=search.value.trim().toLocaleLowerCase();
  const textMatch=!q || `${s.title} ${s.workspace || ''}`.toLocaleLowerCase().includes(q) || searchMatches.value.has(s.id);
  if(!textMatch)continue;
  const k=typeof s.workspace==='string' && s.workspace ? s.workspace : t('dshNoWorkspace');
  if(!map.has(k))map.set(k,[]);map.get(k).push(s);
 }
 return [...map].map(([name,items])=>({name,items}));
});
const testList=computed(()=>{
 const list=[];
 for(const s of sessions.value){
  if(archivedSessionIds.value.has(s.id))continue;
  if(!isTestOrScratchSession(s))continue;
  const q=search.value.trim().toLocaleLowerCase();
  const textMatch=!q || `${s.title} ${s.workspace || ''}`.toLocaleLowerCase().includes(q) || searchMatches.value.has(s.id);
  if(!textMatch)continue;
  list.push(s);
 }
 return list;
});
const archivedList=computed(()=>{
 const list=[];
 for(const s of sessions.value){
  if(!archivedSessionIds.value.has(s.id))continue;
  const q=search.value.trim().toLocaleLowerCase();
  const textMatch=!q || `${s.title} ${s.workspace || ''}`.toLocaleLowerCase().includes(q) || searchMatches.value.has(s.id);
  if(!textMatch)continue;
  list.push(s);
 }
 return list;
});

const inSessionSearchOpen=ref(false);
const inSessionSearchQuery=ref('');
function toggleInSessionSearch(){
 inSessionSearchOpen.value=!inSessionSearchOpen.value;
 if(!inSessionSearchOpen.value)inSessionSearchQuery.value='';
}
const filteredDisplayMessages=computed(()=>{
 const q=inSessionSearchQuery.value.trim().toLocaleLowerCase();
 if(!q)return displayMessages.value;
 return displayMessages.value.filter(m=>String(m.text||'').toLocaleLowerCase().includes(q));
});
function fault(e){const code=String(e?.message || e);if(code.includes('AGY_'))return t('agyHistoryPending');return `${t(code.includes('DSH_RECEIPT_UNKNOWN')?'dshUncertain':code.includes('DSH_HISTORY_TOO_LARGE')?'dshHistoryTooLarge':code.includes('DSH_HISTORY_TIMEOUT')?'dshHistoryTimeout':code.includes('DSH_HISTORY_UNAVAILABLE')?'dshHistoryUnavailable':'dshError')} · ${code.match(/DSH_[A-Z_]+/)?.[0] || 'DSH_UNAVAILABLE'}`}
async function load(){resetDshStop();dshQuestionView.value=null;const g=++generation;clearTimeout(timer);busy.value=true;error.value='';selected.value=null;history.value=null;sessions.value=[];try{const items=await kernelBridge.dshList(connection.value);if(g===generation&&!disposed)sessions.value=items;}catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation)busy.value=false}}
watch(connection,()=>{if(connection.value&&!props.target)load()});
function validatePage(h,id){if(h.address.session_id!==id||h.address.connection_id!==connection.value||!Number.isSafeInteger(h.cursor)||!Number.isSafeInteger(h.before)||!Array.isArray(h.messages))throw Error('DSH_IDENTITY_MISMATCH')}
function combine(old,h){if(h.history_snapshot)return h;const map=new Map((old?.messages||[]).map(m=>[m.sequence,m]));for(const m of h.messages)map.set(m.sequence,m);const replacements=[...(old?.replacements||[]),...(h.replacements||[])];for(const op of replacements)for(const seq of map.keys())if(seq>=op.startSeq&&seq<=op.endSeq)map.delete(seq);return {...h,messages:[...map.values()].sort((a,b)=>a.sequence-b.sequence),replacements,workspace:h.workspace||old?.workspace,terminal_holder:h.terminal_holder!==undefined?h.terminal_holder:old?.terminal_holder}}
let olderRequested=false;
async function refresh(){if(selected.value?.historyReadable===false){error.value=t(selected.value?.historyReason==='AGY_HISTORY_NOT_LOCAL'?'agyHistoryNotLocal':'agyHistoryPending');return;}
 if(!selected.value||busy.value||loadingOlder.value||preview.value)return;
 clearTimeout(timer);const g=generation,id=selected.value.id;busy.value=true;
 clearTimeout(syncTimer);syncStatus.value='checking';emitTopbarPhase('checking');
 try{
  const native=isNativeId(id);
  const h=await kernelBridge.dshPage(connection.value,id,null,null,(native||checkedDshQuestions)?history.value?.cursor??null:null);validatePage(h,id);
  if(g!==generation||disposed||preview.value)return;
  if(!native){observeDshQuestions(h);checkedDshQuestions=true;}
  await refreshActivity(g,id);if(g!==generation||disposed)return;
  if(h.request_results?.some(r=>r.request_id===awaitingReply.value&&['completed','error','blocked','cancelled'].includes(r.status))){awaitingReply.value=null;replying.value=false;}
  if(h.terminal_holder!==undefined){terminalHolderOverride.value=null;if(history.value)history.value={...history.value,terminal_holder:h.terminal_holder};}
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
  const deltaAdded=!first&&(additions.history_snapshot?JSON.stringify(additions.messages)!==JSON.stringify(history.value.messages):additions.messages.some(m=>m.sequence>prevCursor));
  history.value=first||additions.history_snapshot?additions:{...combine(history.value,additions),before:history.value.before,has_more:history.value.has_more};error.value='';
  if(!first&&deltaAdded&&(replying.value||awaitingReply.value||nodeActivity.value?.running)){
   const lastAsst=[...(history.value?.messages||[])].reverse().find(m=>m.role!=='user'&&m.sequence>prevCursor);
   if(lastAsst)streamSeq.value=lastAsst.sequence;
  }
  if(!first){
   if(!follow&&deltaAdded)hasNewMessages.value=true;
   else if(follow)hasNewMessages.value=false;
   syncStatus.value='synced';clearTimeout(syncTimer);syncTimer=setTimeout(()=>{if(syncStatus.value==='synced')syncStatus.value='';},1800);
  }
  if(first||deltaAdded){emitTopbarPhase('syncing');clearTimeout(syncPulseTimer);syncPulseTimer=setTimeout(()=>emitTopbarPhase(),700);}else{emitTopbarPhase();}
  await nextTick();if(g===generation&&!disposed&&follow&&chatScroll.following&&reader.value)scrollToBottom();
  lastTop=reader.value?.scrollTop||0;saveHistory();
 }catch(e){if(g===generation){error.value=fault(e);nodeActivity.value=null;if(history.value!==null){clearTimeout(syncTimer);syncStatus.value='offline_cached';}}}finally{if(g===generation){busy.value=false;const pollMs=(dshStop.value||dshQuestionBlocking.value||awaitingReply.value||replying.value||nodeActivity.value?.running)?1800:/^(agy|claude-jsonl|codex|pi):/.test(id)?5000:60000;if(olderRequested&&reader.value?.scrollTop<=24){olderRequested=false;loadOlder()}else{olderRequested=false;timer=setTimeout(refresh,pollMs)}}}
}
async function loadOlder(){
 if(!history.value?.has_more||loadingOlder.value||busy.value||preview.value)return;
 chatScroll.hold();const g=generation,id=selected.value.id,before=history.value.before;loadingOlder.value=true;clearTimeout(timer);
 try{
  const pageKey=readingKey(connection.value,'page',nodes.value.find(n=>n.id===connection.value)?.baseUrl||nodes.value.find(n=>n.id===connection.value)?.url||'',id,before,history.value.cursor);
  const cached=await readingCache.get(pageKey);if(g!==generation||disposed)return;
  const h=cached||await kernelBridge.dshPage(connection.value,id,before,history.value.cursor);validatePage(h,id);
  if(g!==generation||disposed||preview.value)return;
  if(h.has_more&&h.before>=before)throw Error('DSH_PAGE_NO_PROGRESS');
  void readingCache.put(pageKey,h);
  const el=reader.value, height=el?.scrollHeight||0,top=el?.scrollTop||0;
  history.value={...combine(h,history.value),before:h.before,has_more:h.has_more};error.value='';
  await nextTick();if(g===generation&&reader.value){reader.value.scrollTop=top+reader.value.scrollHeight-height;lastTop=reader.value.scrollTop;chatScroll.measure();}
 }catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation){loadingOlder.value=false;timer=setTimeout(refresh,60000)}}
}
function onScroll(){chatScroll.bind(reader.value);chatScroll.scroll();const el=reader.value,top=el?.scrollTop||0;const upwards=top<lastTop;lastTop=top;if(el&&el.scrollHeight-top-el.clientHeight<80)hasNewMessages.value=false;if(upwards&&top<=24){if(busy.value)olderRequested=true;else loadOlder()}else if(top>24)olderRequested=false}
async function forceRefresh(){if(!selected.value){if(connection.value)return load();return;}clearTimeout(timer);activityChecked=0;error.value='';if(pending.value&&!sending.value)return checkReceipt();await refresh();}
function onReaderTouchStart(e){if(!selected.value)return;const el=reader.value,atBottom=!el||(el.scrollHeight-el.scrollTop-el.clientHeight<=16);touchStartY=atBottom?(e?.touches?.[0]?.clientY??null):null;pullUpDistance.value=0;}
function onReaderTouchMove(e){if(touchStartY===null||!selected.value)return;const y=e?.touches?.[0]?.clientY;if(typeof y!=='number')return;const delta=touchStartY-y,el=reader.value,atBottom=!el||(el.scrollHeight-el.scrollTop-el.clientHeight<=20);if(delta>8&&atBottom)pullUpDistance.value=Math.min(72,Math.round((delta-8)*0.75));else if(delta<=0)pullUpDistance.value=0;}
function onReaderTouchEnd(){const dist=pullUpDistance.value;touchStartY=null;pullUpDistance.value=0;if(dist>=36)void forceRefresh();}
function onReaderWheel(e){if(!selected.value||!(e?.deltaY>24))return;const el=reader.value,atBottom=!el||(el.scrollHeight-el.scrollTop-el.clientHeight<=8);if(!atBottom){wheelOverscroll=0;return;}wheelOverscroll+=e.deltaY;clearTimeout(wheelResetTimer);wheelResetTimer=setTimeout(()=>{wheelOverscroll=0;},450);if(wheelOverscroll>=140){wheelOverscroll=0;void forceRefresh();}}
function openFilePicker(){fileInput.value?.click?.();}
function isTextAttachment(f){const m=String(f?.type||'').toLowerCase(),n=String(f?.name||'').toLowerCase();if(m.startsWith('image/')||m.startsWith('audio/')||m.startsWith('video/')||m==='application/pdf'||m==='application/zip')return false;if(m.startsWith('text/')||m.includes('json')||m.includes('yaml')||m.includes('xml')||m.includes('javascript'))return true;return /\.(txt|md|json|ya?ml|csv|tsv|py|rs|js|mjs|cjs|ts|tsx|jsx|vue|html|css|scss|sh|bat|ps1|toml|ini|cfg|log|sql|gd|tscn)$/i.test(n);}
function bytesToBase64(bytes){let bin='';const chunk=0x8000;for(let i=0;i<bytes.length;i+=chunk){bin+=String.fromCharCode.apply(null,bytes.subarray(i,i+chunk));}return btoa(bin);}
async function readAttachmentBase64(f){if(!f||(f.size||0)>8*1024*1024)return '';try{if(String(f.type||'').startsWith('image/')&&(f.size||0)>1024*1024&&typeof createImageBitmap==='function'&&typeof document!=='undefined'){const bmp=await createImageBitmap(f);const maxSide=Math.max(bmp.width||1,bmp.height||1);const scale=maxSide>1920?1920/maxSide:1;const w=Math.max(1,Math.round(bmp.width*scale)),h=Math.max(1,Math.round(bmp.height*scale));const c=document.createElement('canvas');c.width=w;c.height=h;const ctx2d=c.getContext('2d');if(ctx2d){ctx2d.drawImage(bmp,0,0,w,h);const outMime=f.type==='image/png'?'image/png':'image/jpeg';const dataUrl=c.toDataURL(outMime,0.86);const idx=dataUrl.indexOf(',');if(idx>0)return dataUrl.slice(idx+1);}}if(typeof f.arrayBuffer==='function'){const ab=await f.arrayBuffer();return bytesToBase64(new Uint8Array(ab));}}catch{}return '';}
async function appendFilesToAttachments(fileList){
  const files=Array.from(fileList||[]);
  for(const f of files){
    let content='',base64='';
    try{
      if(isTextAttachment(f)&&typeof f.text==='function'&&(f.size||0)<=256*1024)content=await f.text();
      base64=await readAttachmentBase64(f);
    }catch{}
    const name=f.name||`attachment_${Date.now()}`;
    attachments.value.push({id:crypto.randomUUID(),name,size:f.size||0,type:f.type||'',content,base64});
  }
}
async function onFilesSelected(e){await appendFilesToAttachments(e?.target?.files);if(e?.target)e.target.value='';}
async function onComposerPaste(e){const items=Array.from(e?.clipboardData?.items||[]);const fileItems=items.filter(it=>it.kind==='file').map(it=>it.getAsFile()).filter(Boolean);if(fileItems.length>0){e.preventDefault();await appendFilesToAttachments(fileItems);}}
async function onComposerDrop(e){const files=Array.from(e?.dataTransfer?.files||[]);if(files.length>0){e.preventDefault();await appendFilesToAttachments(files);}}
function onQuoteToPrompt({path, name}){
  preview.value=null;
  const cleanName=name||path?.split('/')?.pop()||'image';
  const quoteText=`请基于此图 [Attachment: ${cleanName} -> ${path}] 调整：`;
  draft.value=draft.value?`${draft.value}\n\n${quoteText}`:quoteText;
  nextTick(()=>{
    if(typeof document!=='undefined'){
      const el=document.querySelector('.dsh-compose-box textarea');
      if(el){el.focus();el.setSelectionRange(el.value.length,el.value.length);}
    }
  });
}
function removeAttachment(id){attachments.value=attachments.value.filter(a=>a.id!==id);}
function formatAttachmentBlock(list){if(!list?.length)return '';return list.map(a=>a.content?`[Attachment: ${a.name}]\n\`\`\`\n${String(a.content).slice(0,16384)}\n\`\`\``:`[Attachment: ${a.name} (${a.size||0} bytes)]`).join('\n\n');}
function onComposeKeydown(e){if(e?.key==='Enter'&&!e.shiftKey&&!e.isComposing){e.preventDefault();void send();}}
globalThis.__bobForceRefreshDsh=forceRefresh;
async function open(s){resetDshStop();checkedDshQuestions=false;dshQuestionView.value=null;inSessionSearchOpen.value=false;inSessionSearchQuery.value='';approvalErrors.value={};pendingDecision.value=null;cancelRequest.value=null;cancelBusy.value=false;takeoverPrompt.value=null;terminalHolderOverride.value=null;chatScroll.reset();clearTimeout(dshLiveTimer);dshLiveState.value=null;dshReadCursor=-1;clearTimeout(nativeTimer);nativeCursor=0;nativeEpoch='';nativeDrafts.value={};nodeActivity.value=null;awaitingReply.value=null;replying.value=false;hasNewMessages.value=false;streamSeq.value=null;syncStatus.value='';pullUpDistance.value=0;attachments.value=[];clearTimeout(syncTimer);activityChecked=0;olderRequested=false;artifactQueue=[];artifactCount=0;persist();++generation;clearTimeout(timer);busy.value=false;loadingOlder.value=false;lastTop=0;selected.value=s;history.value=null;notice.value='';const saved=readState();restoring=true;draft.value=saved.draft||'';pending.value=saved.pending||null;restoring=false;if(pending.value)notice.value=t('dshUncertain');const g=generation;const cached=await readingCache.get(historyKey());if(g!==generation||disposed)return;if(cached){try{validatePage(cached,s.id);history.value=cached;syncStatus.value='checking';await nextTick();if(reader.value)reader.value.scrollTop=reader.value.scrollHeight;lastTop=reader.value?.scrollTop||0;}catch{}}if(s.historyReadable===false){error.value=t(s.historyReason==='AGY_HISTORY_NOT_LOCAL'?'agyHistoryNotLocal':'agyHistoryPending');return;}await refresh();void refreshNativeEvents();void refreshDshLive()}
function back(){resetDshStop();dshQuestionView.value=null;inSessionSearchOpen.value=false;inSessionSearchQuery.value='';clearTimeout(dshLiveTimer);dshLiveState.value=null;clearTimeout(nativeTimer);persist();++generation;clearTimeout(timer);clearTimeout(syncTimer);busy.value=false;selected.value=null;history.value=null;syncStatus.value='';hasNewMessages.value=false;notice.value='';load()}
async function checkReceipt(){
 if(!pending.value || sending.value)return;
 const g=generation,p= pending.value; sending.value=true;
 try{const receipt=await kernelBridge.dshReceipt(connection.value,selected.value.id,p.text,p.id);
 if(g!==generation||disposed)return;
 if(isNotAcceptedReceipt(receipt,selected.value.id,p.id)){pending.value=null;persist();error.value='';notice.value=t('dshNotDispatched');}
 else if(receipt.status==='accepted' && receipt.session_id===selected.value.id && receipt.request_id===p.id){
 pending.value=null;if(draft.value.trim()===p.text)draft.value='';persist();error.value='';notice.value=t('dshAccepted');
 }else{notice.value=t('dshReceiptStillUnknown');}
 }catch(e){if(g===generation)error.value=fault(e)}finally{sending.value=false;if(g===generation&&!busy.value)refresh()}
}
function dismissPending(){if(!pending.value||sending.value)return;if(!draft.value.trim()&&pending.value.text)draft.value=pending.value.text;pending.value=null;notice.value='';error.value='';persist();}
async function send(opts={}){const takeover=opts?.takeover===true;if(selected.value?.historyReadable===false){error.value=t('agyHistoryPending');return;}if(isClaudeRemoteControl.value){notice.value=t('claudeRemoteControlTitle');return;}const attBlock=formatAttachmentBlock(attachments.value);const text=[draft.value.trim(),attBlock].filter(Boolean).join('\n\n');if(sending.value||pending.value||nativeUnresolved.value||dshQuestionBlocking.value||dshStop.value||!text)return;if(!takeover&&isClaudeTerminalBusy.value){takeoverPrompt.value=activeTerminalHolder.value||{busy:true,remote_control:false};return;}const g=generation, address={connection:connection.value,session:selected.value.id};const attPayload=attachments.value.filter(a=>a.base64).map(a=>({name:a.name,type:a.type,size:a.size,base64:a.base64}));pending.value={id:crypto.randomUUID(),text};if(!persist()){pending.value=null;return;}sending.value=true;error.value='';try{const requestId=pending.value.id;const receipt=await kernelBridge.dshPrompt(address.connection,address.session,text,requestId,attPayload,{takeover,approvalMode:supportsApprovalMode.value?approvalMode.value:null,workspace:selected.value?.workspace||history.value?.workspace||null});if(isNotAcceptedReceipt(receipt,address.session,requestId)){if(g===generation){pending.value=null;persist();if(receipt?.reason==='HARNESS_CLAUDE_REMOTE_CONTROL'){terminalHolderOverride.value=receipt.terminal_holder||{busy:true,remote_control:true};takeoverPrompt.value=null;notice.value=t('claudeRemoteControlTitle');}else if(receipt?.reason==='HARNESS_SESSION_BUSY'&&String(address.session||'').startsWith('claude-jsonl:')){const holder=receipt.terminal_holder||{busy:true,remote_control:false};terminalHolderOverride.value=holder;takeoverPrompt.value=holder;notice.value='';}else{notice.value=t('dshNotDispatched');}}return;}if(receipt?.status!=='accepted'||receipt.session_id!==address.session||receipt.request_id!==requestId)throw Error('DSH_RECEIPT_UNKNOWN');if(g===generation){takeoverPrompt.value=null;terminalHolderOverride.value={busy:false,remote_control:false};awaitingReply.value=requestId;replying.value=false;activityChecked=0;pending.value=null;attachments.value=[];if(draft.value.trim()===text||attBlock)draft.value='';persist();notice.value=t('dshAccepted');}}catch(e){if(g===generation){error.value=fault(e);notice.value=t('dshUncertain')}}finally{sending.value=false;if(g===generation&&!busy.value)refresh()}}
watch(()=>props.target, async target=>{if(!target)return;connection.value=target.node.id;await open(target.session)},{immediate:true});
onUnmounted(()=>{resetDshStop();stopVoiceInput();chatScroll.dispose();clearTimeout(dshLiveTimer);clearTimeout(nativeTimer);if(globalThis.__bobForceRefreshDsh===forceRefresh)globalThis.__bobForceRefreshDsh=null;clearTimeout(syncPulseTimer);if(typeof globalThis.__bobSetRemoteSyncPhase==='function')globalThis.__bobSetRemoteSyncPhase('');if(rodTimer&&typeof clearInterval==='function')clearInterval(rodTimer);clearTimeout(wheelResetTimer);clearTimeout(artifactTimer);clearTimeout(syncTimer);clearTimeout(copyTimer);artifactQueue=[];fileCache.clear();persist();disposed=true;++generation;clearTimeout(timer)});
</script>
<template>
 <DshFilePreview v-if="preview" :address="preview" @close="closePreview" @quote="onQuoteToPrompt" />
 <section class="dsh-sessions" :class="{embedded:!!target}" :aria-label="t('dshTitle')">
  <div ref="reader" class="dsh-reader" tabindex="0" @scroll.passive="onScroll" @touchstart.passive="onReaderTouchStart" @touchmove.passive="onReaderTouchMove" @touchend.passive="onReaderTouchEnd" @touchcancel.passive="onReaderTouchEnd" @wheel.passive="onReaderWheel">
  <div ref="chatContent" data-chat-content>
  <header v-if="!target"><h3>{{ t('dshTitle') }}</h3><select v-model="connection" :disabled="sending" :aria-label="t('dshChooseNode')"><option value="">{{ t('dshChooseNode') }}</option><option v-for="n in nodes" :key="n.id" :value="n.id">{{ n.label || n.name || n.baseUrl || n.url }}</option></select></header>
  <p v-if="!target" class="dsh-note">{{ t('dshSource') }}</p>
  <p v-if="error && syncStatus!=='offline_cached'" role="alert">{{ error }}</p><p v-if="nativeUnresolved" role="status">{{ t('nativeExecutionUnconfirmed') }}</p><p v-else-if="notice" role="status">{{ notice }}</p>
  <template v-if="connection && !selected">
   <div class="dsh-tools"><input v-model="search" type="search" :placeholder="t('dshSearch')" :aria-label="t('dshSearch')"><button :disabled="busy" @click="load">{{ t('dshRefresh') }}</button></div>
   <p v-if="busy" class="terminal-status-line" role="status"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ stripDots(t('dshLoading')) }}</span><span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></p>
   <details v-for="group in groups" :key="group.name" :open="!!search"><summary>{{ group.name }} · {{ group.items.length }}</summary><div v-for="s in group.items" :key="s.id" class="dsh-session-item"><button class="dsh-session" @click="open(s)"><strong><HarnessIcon :session="s" class="session-card-icon" />{{ s.title }}</strong><small>{{ s.updated_at ? new Date(s.updated_at).toLocaleString() : t('sidebarTimeUnknown') }} · {{ t(s.running?'dshRunning':'dshIdle') }}</small><small v-if="searchMatches.has(s.id)" class="dsh-search-snippet">“{{ searchMatches.get(s.id) }}”</small></button><button type="button" class="dsh-session-archive-btn" :title="t('dshArchiveSession')" :aria-label="t('dshArchiveSession')" @click.stop="toggleArchiveSession(s.id, $event)"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="4" rx="1"/><path d="M5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/></svg></button></div></details>
   <p v-if="!busy && !groups.length && !archivedList.length && !testList.length">{{ t('dshEmpty') }}</p>
   <details v-if="testList.length" class="dsh-test-group" :open="!!search"><summary><svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 2v7.24a4 4 0 0 0 1.25 2.9l5.5 5.5a2.83 2.83 0 0 1-4 4H7"/><path d="M14 2v4"/><path d="M8.5 2h7"/></svg> {{ t('testAndScratchSessions') }} · {{ testList.length }}</summary><div v-for="s in testList" :key="s.id" class="dsh-session-item test-item"><button class="dsh-session" @click="open(s)"><strong><HarnessIcon :session="s" class="session-card-icon" />{{ s.title }}</strong><small>{{ s.updated_at ? new Date(s.updated_at).toLocaleString() : t('sidebarTimeUnknown') }} · {{ t(s.running?'dshRunning':'dshIdle') }}</small><small v-if="searchMatches.has(s.id)" class="dsh-search-snippet">“{{ searchMatches.get(s.id) }}”</small></button><button type="button" class="dsh-session-archive-btn" :title="t('dshArchiveSession')" :aria-label="t('dshArchiveSession')" @click.stop="toggleArchiveSession(s.id, $event)"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="4" rx="1"/><path d="M5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/></svg></button></div></details>
   <details v-if="archivedList.length" class="dsh-archived-group" :open="!!search"><summary>{{ t('dshArchivedSessions') }} · {{ archivedList.length }}</summary><div v-for="s in archivedList" :key="s.id" class="dsh-session-item archived"><button class="dsh-session" @click="open(s)"><strong><HarnessIcon :session="s" class="session-card-icon" />{{ s.title }}</strong><small>{{ s.updated_at ? new Date(s.updated_at).toLocaleString() : t('sidebarTimeUnknown') }} · {{ t(s.running?'dshRunning':'dshIdle') }}</small><small v-if="searchMatches.has(s.id)" class="dsh-search-snippet">“{{ searchMatches.get(s.id) }}”</small></button><button type="button" class="dsh-session-unarchive-btn" :title="t('dshUnarchiveSession')" :aria-label="t('dshUnarchiveSession')" @click.stop="toggleArchiveSession(s.id, $event)">{{ t('dshUnarchiveSession') }}</button></div></details>
  </template>
  <template v-if="selected">
   <div class="dsh-tools"><button v-if="!target" :disabled="sending" @click="back">{{ t('dshBack') }}</button><h4 v-if="!target"><HarnessIcon :session="selected" class="topbar-harness-icon" />{{ selected.title }}</h4><small v-else><HarnessIcon :session="selected" class="topbar-harness-icon" />{{ target.node.label || target.node.name }} · {{ agentLabel }}</small><span v-if="isClaudeTerminalBusy" class="dsh-holder-pill" role="status"><span>{{ t('claudeTerminalBusyBadge', {pid: activeTerminalHolder?.pid || '?'}) }}</span><button type="button" class="dsh-holder-takeover-btn" @click="takeoverPrompt = activeTerminalHolder">{{ t('claudeTakeoverBtn') }}</button></span><button type="button" class="dsh-search-toggle-btn" :class="{active:inSessionSearchOpen}" :title="t('dshSearchInSession')" :aria-label="t('dshSearchInSession')" @click="toggleInSessionSearch"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg></button><button v-if="sessionVisualAssets.length" type="button" class="dsh-visual-reel-toggle-btn" :class="{active: visualReelOpen}" :title="t('visualAssetsCount', {count: sessionVisualAssets.length})" :aria-label="t('visualAssets')" @click="visualReelOpen = !visualReelOpen"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/></svg><span>{{ sessionVisualAssets.length }}</span></button>
    <span v-if="syncStatus==='offline_cached' && syncStatusLabel" class="dsh-sync-pill" :class="{active:syncStatus==='checking'||syncStatus==='syncing'}" role="status" aria-live="polite"><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ syncStatusLabel }}</span><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></span></div>
   <div v-if="visualReelOpen && sessionVisualAssets.length" class="dsh-visual-reel-strip" role="region" :aria-label="t('visualAssets')">
     <div class="visual-reel-header">
       <span class="visual-reel-title"><svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/></svg>{{ t('visualAssetsCount', {count: sessionVisualAssets.length}) }}</span>
       <button type="button" class="visual-reel-close" :aria-label="t('filePreviewClose')" @click="visualReelOpen = false">✕</button>
     </div>
     <div class="visual-reel-scroll">
       <div
         v-for="asset in sessionVisualAssets"
         :key="asset.path"
         class="visual-reel-card-wrap"
       >
         <button
           type="button"
           class="visual-reel-card"
           :title="asset.path"
           @click="openAssetPreview(asset.path,asset.messageSequence)"
         >
           <div class="visual-reel-thumb-box">
             <span v-if="asset.isHtml" class="reel-type-badge html-badge">HTML</span>
             <span v-else class="reel-type-badge img-badge">{{ asset.ext.toUpperCase() }}</span>
             <svg v-if="asset.isHtml" viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="reel-icon" aria-hidden="true"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
             <svg v-else viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="reel-icon" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/></svg>
           </div>
           <span class="visual-reel-name">{{ asset.name }}</span>
         </button>
         <button
           type="button"
           class="reel-card-quote-btn"
           :title="t('quoteFeedbackBtn')"
           :aria-label="t('quoteFeedbackBtn')"
           @click.stop="onQuoteToPrompt({path: asset.path, name: asset.name})"
         >
           <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
         </button>
       </div>
     </div>
   </div>
   <div v-if="inSessionSearchOpen" class="dsh-session-search-bar"><input v-model="inSessionSearchQuery" type="search" :placeholder="t('dshSearchMessagesPlaceholder')" :aria-label="t('dshSearchMessagesPlaceholder')"><span v-if="inSessionSearchQuery.trim()" class="dsh-search-count">{{ t('dshSearchMatchCount', {count: filteredDisplayMessages.length}) }}</span><button v-if="inSessionSearchQuery" type="button" class="dsh-search-clear" :aria-label="t('dshSearchClear')" @click="inSessionSearchQuery=''">✕</button></div>
    <div v-if="isClaudeRemoteControl" class="dsh-remote-control-banner" role="status"><strong>{{ t('claudeRemoteControlTitle') }}</strong><p>{{ t('claudeRemoteControlDesc', {pid: activeTerminalHolder?.pid || '?'}) }}</p></div>
   <p class="dsh-note">{{ history?.workspace || t('dshNoWorkspace') }}</p>
   <p v-if="history?.partial || history?.omitted" class="dsh-note">{{ t('dshPartial') }}</p>
   <p v-if="busy && !history" class="terminal-status-line" role="status"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ stripDots(t('dshLoading')) }}</span><span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></p>
   <button v-if="history?.has_more" class="dsh-older" :disabled="loadingOlder || busy" @click="loadOlder"><span v-if="loadingOlder" class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span> {{ t(loadingOlder ? 'dshLoading' : 'dshLoadOlder') }}</button>
   <article v-for="(m, idx) in filteredDisplayMessages" :key="m.sequence" :data-sequence="m.sequence" :data-chat-anchor="String(m.sequence)" class="dsh-message" :class="{user:m.role==='user'}"><div class="dsh-message-meta"><small class="dsh-role"><HarnessIcon v-if="m.role!=='user'" :session="selected" class="role-harness-icon" />{{ m.role==='user'?t('dshYou'):agentLabel }}</small><time v-if="formatMessageTime(m.created_at || m.createdAt || m.timestamp)" class="dsh-time">{{ formatMessageTime(m.created_at || m.createdAt || m.timestamp) }}</time><button type="button" class="dsh-copy-btn" @click="copyMessage(m)">{{ t(copiedSeq===m.sequence?'copied':'copyMessage') }}</button></div><MessageContent :message-sequence="m.sequence" :content="m.text" :thinking-steps="m.thinkingSteps || []" :plain="m.role==='user'" :stream="m.role!=='user' && idx===displayMessages.length-1 && m.sequence===streamSeq" /></article>
   <p v-if="dshLiveState && dshLiveState.status!=='live'" class="dsh-notice" role="status">{{ t(dshLiveState.status==='connecting'?'dshLiveConnecting':'dshLiveReconnecting') }}</p>
   <article v-for="m in dshLiveMessages" :key="'dsh-live-'+m.id" :data-chat-anchor="'live-'+m.id" class="dsh-message"><small><HarnessIcon :session="selected" class="role-harness-icon" />{{ agentLabel }}</small><MessageContent :message-sequence="m.sequence" :content="m.text" :stream="m.active" /></article>
   <article v-for="m in nativeMessages" :key="m.id" :data-chat-anchor="m.id" class="dsh-message"><small><HarnessIcon :session="selected" class="role-harness-icon" />{{ agentLabel }}</small><MessageContent :message-sequence="m.sequence" :content="m.text" :stream="m.active" /></article>
   <p v-if="inSessionSearchQuery.trim() && !filteredDisplayMessages.length" class="dsh-empty-search" role="status">{{ t('dshSearchNoMatches') }}</p>
    <NativeActivity v-for="record in nativeActivities" :key="'activity-'+record.id" :activity="record.activity" :busy-permission="busyPermission" :approval-errors="approvalErrors" @decide="(permission,option)=>decidePermission(record.id,permission,option)"/>
   <div v-if="pullUpDistance > 10" class="pull-up-refresh-hint" :class="{ready: pullUpDistance >= 36}" role="status" aria-live="polite"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ t(pullUpDistance >= 36 ? 'releaseToRefresh' : 'pullUpToRefresh') }}</span></div>
  </template>
  </div>
  </div>
  <div v-if="selected && (awayFromLatest || hasNewMessages)" class="dsh-new-msg-bar"><button type="button" class="dsh-new-msg-btn" @click="scrollToBottom" :aria-label="t('chatJumpLatest')" :title="t('chatJumpLatest')"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M12 4v16m-6-6 6 6 6-6"/></svg></button></div>
  <form v-if="selected" class="dsh-compose" @submit.prevent="send()">
   <div v-if="takeoverPrompt && !isClaudeRemoteControl" class="dsh-takeover-card" role="alertdialog" :aria-label="t('claudeTakeoverConfirmTitle')">
    <div class="dsh-takeover-body">
     <strong>{{ t('claudeTakeoverConfirmTitle') }}</strong>
     <p>{{ t('claudeTakeoverConfirmDesc', {pid: takeoverPrompt?.pid || '?'}) }}</p>
    </div>
    <div class="dsh-takeover-actions">
     <button type="button" class="dsh-takeover-primary" :disabled="sending" @click="send({takeover: true})">{{ t('claudeTakeoverSendBtn') }}</button>
     <button type="button" class="dsh-takeover-secondary" :disabled="sending" @click="takeoverPrompt = null">{{ t('claudeKeepBoardBtn') }}</button>
    </div>
   </div>
   <section v-if="dshQuestions.length" class="dsh-question-notice" aria-live="polite">
    <strong>{{ t(dshQuestionBlocking ? 'dshQuestionWaiting' : 'dshQuestionOpen') }}</strong>
    <p>{{ t('dshQuestionReplyInSource') }}</p>
    <a v-if="dshQuestionSource" :href="dshQuestionSource" target="_blank" rel="noopener noreferrer">{{ t('dshQuestionOpenSource') }}</a>
    <details open>
     <summary>{{ t('dshQuestionDetails') }}</summary>
     <div v-for="request in dshQuestions" :key="request.call_id">
      <div v-for="(item,index) in request.questions" :key="index">
       <p>{{ item.question }}</p>
       <ul v-if="item.options.length"><li v-for="(option,index) in item.options" :key="index">{{ option.label }} <small v-if="option.description">{{ option.description }}</small></li></ul>
      </div>
     </div>
    </details>
   </section>
   <p v-if="activityLabel && !dshQuestionBlocking && !dshStop" class="dsh-activity" role="status" aria-live="polite"><span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span> <span>{{activityLabel}}</span><span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></p>
   <div class="dsh-compose-box">
    <textarea v-model="draft" :disabled="sending || isClaudeRemoteControl" :placeholder="t(isClaudeRemoteControl ? 'claudeRemoteControlTitle' : 'dshPrompt')" :aria-label="t('dshPrompt')" rows="2" @keydown="onComposeKeydown" @paste="onComposerPaste" @dragover.prevent @drop.prevent="onComposerDrop"></textarea>
    <div v-if="attachments.length" class="composer-attachments"><span v-for="att in attachments" :key="att.id" class="composer-attachment-chip"><img v-if="att.base64 && att.type?.startsWith('image/')" :src="'data:' + att.type + ';base64,' + att.base64" alt="" class="chip-thumb" /><span class="chip-file-name">{{ att.name }}</span><button type="button" class="chip-remove-btn" :aria-label="t('removeAttachment')" @click="removeAttachment(att.id)">×</button></span></div>
    <div class="dsh-compose-toolbar">
     <div class="dsh-compose-left-tools">
      <input ref="fileInput" type="file" multiple class="visually-hidden" @change="onFilesSelected" />
      <button type="button" class="composer-tool-btn" :disabled="sending || isClaudeRemoteControl" :title="t('attachFileBtn')" :aria-label="t('attachFileBtn')" @click="openFilePicker"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg></button>
      <button type="button" class="composer-tool-btn voice-btn" :class="{recording:isListening}" :disabled="sending || isClaudeRemoteControl" :title="t(isListening ? 'voiceInputListening' : 'voiceInputBtn')" :aria-label="t(isListening ? 'voiceInputListening' : 'voiceInputBtn')" @click="toggleVoiceInput"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/></svg></button>
      <CustomSelect
        v-if="supportsApprovalMode"
        v-model="approvalMode"
        class="dsh-permission-mode-select"
        :options="permissionModeOptions"
        :disabled="sending || isClaudeRemoteControl"
        :aria-label="t('permissionModeLabel')"
        :title="t('permissionModeLabel')"
      />
      <span v-if="syncStatus==='offline_cached' && syncStatusLabel" class="dsh-sync-pill" :class="{active:syncStatus==='checking'||syncStatus==='syncing'}"><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span><span>{{ syncStatusLabel }}</span><span v-if="syncStatus==='checking'||syncStatus==='syncing'" class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></span>
     </div>
     <button v-if="canStopDsh" type="button" class="composer-submit-btn" :disabled="!!dshStop || sending" @click="stopDsh">{{ t(dshStop ? 'dshStopChecking' : 'dshStop') }}</button>
     <button v-else-if="cancellableRequest" type="button" class="composer-submit-btn" :disabled="cancelBusy || cancelRequest===cancellableRequest" @click="cancelNative">{{ t(cancelRequest===cancellableRequest?'nativeCancelPending':'nativeCancel') }}</button>
      <button v-else type="submit" class="composer-submit-btn" :disabled="sending || isClaudeRemoteControl || !!pending || nativeUnresolved || dshQuestionBlocking || (!draft.trim() && !attachments.length) || !history">{{ t(sending?'dshSending':'send') }}</button>
    </div>
   </div>
   <p v-if="pending && !sending" role="status">{{ t('dshUncertain') }} <button type="button" @click="checkReceipt">{{ t('dshCheckReceipt') }}</button> <button type="button" @click="dismissPending">{{ t('dshUnlockEdit') }}</button></p>
  </form>
 </section>
</template>
<style scoped>
.dsh-question-notice{max-height:34vh;overflow:auto;padding:10px 12px;border:1px solid var(--accent-primary);border-radius:var(--radius-card);background:var(--bg-secondary);overflow-wrap:anywhere}
.dsh-question-notice p{margin:4px 0}.dsh-question-notice details{margin-top:0}.dsh-question-notice ul{margin:4px 0;padding-inline-start:1.5rem}

.dsh-sessions{min-width:0;max-width:100%;padding:16px;border:1px solid var(--border-color);border-radius:var(--radius-modal);background:var(--bg-primary)}
.dsh-sessions.embedded{border:0;border-radius:0;padding:0;flex:1;min-height:0;overflow:hidden;display:flex;flex-direction:column;width:100%}
.dsh-sessions.embedded .dsh-reader{min-height:0;flex:1;overflow-y:auto;overflow-x:hidden;scrollbar-gutter:stable;overflow-anchor:none;padding:12px 8px;overscroll-behavior:contain}
.dsh-sessions.embedded .dsh-reader [data-chat-content]{width:min(100%, 46rem);margin:0 auto;padding:0 8px;box-sizing:border-box}
.dsh-sessions.embedded .dsh-compose{width:100%;align-items:center;background:var(--bg-primary);border-top:1px solid var(--border-color)}
.dsh-sessions.embedded .dsh-compose-box{width:min(100%, 46rem);margin:0 auto;box-sizing:border-box}
.dsh-sessions.embedded .dsh-new-msg-bar{max-width:46rem;margin:0 auto}.dsh-reader{min-height:0;flex:1;overflow-y:auto;overflow-x:hidden;scrollbar-gutter:stable;overflow-anchor:none;padding:12px;overscroll-behavior:contain}.dsh-compose{flex-shrink:0;padding:8px 12px 10px;background:var(--bg-primary);border-top:1px solid var(--border-color);display:flex;flex-direction:column;gap:6px}.dsh-compose-box{border:1px solid var(--border-color);border-radius:var(--radius-card);padding:7px 10px;background:var(--bg-secondary);display:flex;flex-direction:column;gap:6px;transition:border-color .15s ease}.dsh-compose-box:focus-within{border-color:var(--accent-primary)}.dsh-compose-box textarea{width:100%;border:0;outline:none;background:transparent;padding:2px;resize:vertical;min-height:2.6rem;max-height:7.5rem}.dsh-compose-toolbar{display:flex;align-items:center;justify-content:space-between;gap:8px}.dsh-compose-left-tools{display:flex;align-items:center;gap:6px;min-width:0;flex:1;flex-wrap:wrap}.dsh-older{display:block;margin:8px auto;background:transparent;border:0;color:var(--text-secondary)}header,.dsh-tools{display:flex;align-items:center;gap:8px;flex-wrap:wrap}h3,h4{margin:0;overflow-wrap:anywhere}select,input,textarea{min-width:0;max-width:100%;color:var(--text-primary);background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:8px;padding:8px}select,input{flex:1}button{max-width:100%;white-space:normal}details{margin-top:10px}summary{overflow-wrap:anywhere;cursor:pointer;padding:8px 0}.dsh-note,small{color:var(--text-secondary);overflow-wrap:anywhere}.dsh-sync-pill{display:inline-flex;align-items:center;gap:5px;font-size:.75rem;padding:2px 8px;border-radius:999px;background:var(--bg-primary);color:var(--text-secondary);border:1px solid var(--border-color)}.dsh-tools .dsh-sync-pill{margin-left:auto}.dsh-sync-pill.active{color:var(--accent-primary);border-color:var(--accent-primary)}.dsh-session{display:flex;flex-direction:column;text-align:left;width:100%;gap:5px;margin-top:5px;padding:10px}.dsh-session strong{overflow-wrap:anywhere}.dsh-message{min-width:0;margin-top:20px;overflow-wrap:anywhere}.dsh-message-meta{display:flex;align-items:center;gap:8px;margin-bottom:6px;flex-wrap:wrap}.dsh-message.user{background:var(--bg-secondary);border-radius:var(--radius-card);padding:14px 16px;width:fit-content;max-width:88%;margin-left:auto}.dsh-message.user .dsh-message-meta{justify-content:flex-end}.dsh-message:not(.user){padding:4px 2px}.dsh-time{font-size:.75rem;color:var(--text-secondary)}.dsh-copy-btn{padding:2px 8px;font-size:.75rem;border-radius:6px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--text-secondary);cursor:pointer}@media(hover:hover) and (pointer:fine){.dsh-copy-btn{opacity:0;transition:opacity .15s ease}.dsh-message:hover .dsh-copy-btn,.dsh-copy-btn:focus-visible{opacity:1}}.dsh-new-msg-bar{height:0;position:relative;z-index:2;display:flex;justify-content:flex-end;padding:0 20px;background:transparent;transform:translateY(-46px)}.dsh-new-msg-btn{display:grid;place-items:center;width:36px;height:36px;padding:6px;border-radius:999px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--accent-primary);font-size:.82rem;cursor:pointer}.dsh-activity{margin:0;display:flex;align-items:center;gap:6px;color:var(--text-secondary);font-size:.82rem}.dsh-compose p{width:100%;margin:0}
.dsh-holder-pill{display:inline-flex;align-items:center;gap:6px;font-size:.75rem;padding:2px 8px;border-radius:999px;background:var(--bg-secondary);color:var(--text-primary);border:1px solid var(--accent-primary)}
.dsh-holder-takeover-btn{padding:1px 7px;border-radius:999px;border:0;background:var(--accent-primary);color:var(--bg-primary);font-size:.72rem;font-weight:600;cursor:pointer}
.dsh-remote-control-banner{margin:8px 0;padding:10px 12px;border-radius:var(--radius-card);border:1px solid var(--accent-primary);background:var(--bg-secondary);display:flex;flex-direction:column;gap:4px}
.dsh-remote-control-banner strong{font-size:.84rem;color:var(--accent-primary)}.dsh-remote-control-banner p{margin:0;font-size:.8rem;color:var(--text-secondary)}
.dsh-takeover-card{padding:10px 12px;border-radius:var(--radius-card);border:1px solid var(--accent-primary);background:var(--bg-secondary);display:flex;flex-direction:column;gap:8px}
.dsh-takeover-body strong{display:block;font-size:.85rem;color:var(--text-primary);margin-bottom:2px}.dsh-takeover-body p{margin:0;font-size:.8rem;color:var(--text-secondary)}
.dsh-takeover-actions{display:flex;gap:8px;flex-wrap:wrap}
.dsh-takeover-primary,.dsh-takeover-secondary{padding:5px 12px;border-radius:var(--radius-btn);font-size:.78rem;font-weight:600;cursor:pointer;border:1px solid var(--border-color)}
.dsh-takeover-primary{background:var(--accent-primary);color:var(--bg-primary);border-color:var(--accent-primary)}
.dsh-takeover-secondary{background:var(--bg-primary);color:var(--text-secondary)}
.dsh-permission-mode-select.custom-select-wrapper{flex:0 0 auto;width:auto;min-width:0}
.dsh-permission-mode-select :deep(.custom-select-trigger){min-height:1.8rem;height:1.8rem;padding:0 8px;font-size:.76rem;border-radius:var(--radius-btn);background:var(--bg-primary);color:var(--text-secondary);border:1px solid var(--border-color);gap:4px}
.dsh-permission-mode-select :deep(.custom-select-trigger:hover:not(.disabled)){color:var(--text-primary);border-color:var(--accent-primary)}
.dsh-permission-mode-select :deep(.custom-select-chevron){width:.75rem;height:.75rem}
.dsh-session-item{display:flex;align-items:center;gap:6px;width:100%;margin-top:5px;position:relative}.dsh-session-item .dsh-session{flex:1;min-width:0;margin-top:0}.dsh-session-archive-btn{flex-shrink:0;padding:6px;border-radius:6px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--text-secondary);cursor:pointer;opacity:0.6;transition:opacity .15s ease}.dsh-session-item:hover .dsh-session-archive-btn,.dsh-session-archive-btn:focus-visible{opacity:1}.dsh-session-unarchive-btn{flex-shrink:0;padding:4px 8px;font-size:.75rem;border-radius:6px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--text-secondary);cursor:pointer}.dsh-test-group{margin-top:14px;border-top:1px dashed var(--border-color);padding-top:8px}.dsh-test-group summary{color:var(--text-secondary);cursor:pointer;display:inline-flex;align-items:center;gap:6px}.dsh-test-group summary svg{color:var(--accent-primary)}.dsh-archived-group{margin-top:16px;border-top:1px dashed var(--border-color);padding-top:8px}.dsh-archived-group summary{color:var(--text-secondary)}.composer-tool-btn.voice-btn.recording{color:var(--color-error);animation:pulse-mic 1.2s infinite ease-in-out}@keyframes pulse-mic{0%,100%{opacity:1;transform:scale(1)}50%{opacity:0.5;transform:scale(1.15)}}.dsh-search-snippet{display:block;margin-top:2px;font-style:italic;color:var(--accent-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.dsh-search-toggle-btn{padding:4px 8px;border-radius:6px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--text-secondary);cursor:pointer;display:inline-flex;align-items:center}.dsh-search-toggle-btn.active{color:var(--accent-primary);border-color:var(--accent-primary)}.dsh-session-search-bar{display:flex;align-items:center;gap:8px;padding:6px 0;width:100%}.dsh-session-search-bar input{flex:1;padding:6px 10px;font-size:.85rem}.dsh-search-count{font-size:.78rem;color:var(--accent-primary);white-space:nowrap}.dsh-search-clear{background:transparent;border:0;color:var(--text-secondary);cursor:pointer;padding:4px 8px;font-size:1rem}.dsh-empty-search{padding:16px;text-align:center;color:var(--text-secondary);font-size:.85rem}.composer-attachment-chip .chip-thumb{width:22px;height:22px;object-fit:cover;border-radius:4px;flex-shrink:0}</style>
