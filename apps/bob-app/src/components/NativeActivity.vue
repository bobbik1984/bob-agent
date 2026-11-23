<script setup>
import {computed,onMounted,onUnmounted,ref} from 'vue';
import {partitionNativeActivity} from '../native-activity.mjs';
import {useI18n} from 'vue-i18n';
const props=defineProps({activity:{type:Object,required:true},busyPermission:{type:String,default:''},approvalErrors:{type:Object,default:()=>({})}});
const emit=defineEmits(['decide']);
const {t}=useI18n();
const nowMs=ref(Date.now());
let tickTimer=null;
onMounted(()=>{tickTimer=setInterval(()=>{nowMs.value=Date.now();},15000);});
onUnmounted(()=>{if(tickTimer)clearInterval(tickTimer);});
const partitioned=computed(()=>partitionNativeActivity(props.activity,nowMs.value));
const stateKey=s=>({running:'nativeToolRunning',completed:'nativeToolCompleted',error:'nativeToolFailed',queued:'nativeApprovalQueued',approved:'nativeApprovalApproved',denied:'nativeApprovalDenied',pending:'nativeApprovalPending',cleared:'nativeApprovalCleared',unconfirmed:'nativeStateUnconfirmed'}[s]||'nativeStateUnconfirmed');
function formatApprovalDetail(approval) {
  const raw = String(approval?.detail || '').trim();
  if (!raw) return '';
  try {
    const parsed = JSON.parse(raw);
    if (approval?.title === 'AskUserQuestion' && Array.isArray(parsed?.questions) && parsed.questions.length > 0) {
      return parsed.questions.map(q => String(q?.question || q?.header || '').trim()).filter(Boolean).join('\n');
    }
    if (typeof parsed?.command === 'string' && parsed.command.trim()) {
      return parsed.command.trim();
    }
    if (typeof parsed?.file_path === 'string' && parsed.file_path.trim()) {
      return parsed.file_path.trim();
    }
    if (typeof parsed?.filePath === 'string' && parsed.filePath.trim()) {
      return parsed.filePath.trim();
    }
  } catch {}
  return raw;
}
function approvalTitleText(approval) {
  if (approval?.title === 'AskUserQuestion') return t('nativeQuestionTitle');
  if (approval?.title) return approval.title;
  return t(['denied','approved','queued'].includes(approval?.status)?'nativeApprovalLocal':'nativeApprovalSource');
}
function remainingMinutes(approval) {
  const leftMs = Number(approval?.expires_at || 0) - nowMs.value;
  return Math.max(1, Math.ceil(leftMs / 60000));
}
function optionButtonLabel(option) {
  const label = String(option?.label || '').trim();
  if (label && label !== 'allow_once' && label !== 'reject_once') {
    return label;
  }
  return t('nativeApproveOnce');
}
</script>
<template>
 <div v-if="partitioned.logCount > 0 || partitioned.actionableApprovals.length > 0" class="native-activity-bundle">
  <details v-if="partitioned.logCount > 0" class="native-activity-record">
   <summary>{{ t('nativeActivityRecords', {count: partitioned.logCount}) }}</summary>
   <p v-if="activity.partial" class="activity-note">{{ t('nativeActivityPartial') }}</p>
   <ul>
    <li v-for="item in partitioned.logItems" :key="'item-'+item.id"><span>{{ item.label }}</span><small>{{ t(stateKey(item.status)) }}</small></li>
    <li v-for="approval in partitioned.historyApprovals" :key="'history-approval-'+approval.id" class="approval-history-row">
     <div class="approval-history-head"><span class="approval-title">{{ approvalTitleText(approval) }}</span><small>{{ t(stateKey(approval.status)) }}</small></div>
     <pre v-if="approval.detail">{{ formatApprovalDetail(approval) }}</pre>
    </li>
   </ul>
  </details>
  <div v-for="approval in partitioned.actionableApprovals" :key="'pending-approval-'+approval.id" :data-native-approval-id="approval.permission_id" class="native-approval-standalone-card" role="region" aria-live="polite">
   <div class="approval-standalone-head">
    <span class="approval-standalone-title-wrap">
     <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
     <span class="approval-title">{{ approvalTitleText(approval) }}</span>
    </span>
    <span class="approval-standalone-badges">
     <small class="approval-ttl-pill">{{ t('nativeApprovalRemainingMin', {minutes: remainingMinutes(approval)}) }}</small>
     <small>{{ t(stateKey(approval.status)) }}</small>
    </span>
   </div>
   <pre v-if="approval.detail">{{ formatApprovalDetail(approval) }}</pre>
   <p v-if="approvalErrors[approval.permission_id]" class="activity-note" role="alert">{{ t(approvalErrors[approval.permission_id]) }}</p>
   <div class="approval-actions">
    <button v-for="option in (approval.options||[]).filter(o=>o.kind==='allow_once')" :key="option.id" type="button" class="approval-btn-allow" :disabled="!!busyPermission || approvalErrors[approval.permission_id]==='nativeApprovalRevoked'" @click="emit('decide',approval.permission_id,option.id)">{{ optionButtonLabel(option) }}</button>
    <button type="button" class="approval-btn-reject" :disabled="!!busyPermission" @click="emit('decide',approval.permission_id,null)">{{ t('nativeRejectOnce') }}</button>
   </div>
  </div>
 </div>
</template>
<style scoped>
.native-activity-bundle{margin-block:10px;display:flex;flex-direction:column;gap:10px}
.native-activity-record{border:1px solid var(--border-color);border-radius:10px;padding:10px 14px;background:var(--bg-secondary)}
summary{cursor:pointer;color:var(--text-secondary);font-size:.86rem;font-weight:500}
ul{list-style:none;padding:0;margin:10px 0 0}
li{display:flex;gap:12px;justify-content:space-between;padding:6px 0}
li span{min-width:0;overflow-wrap:anywhere}
small,.activity-note{color:var(--text-secondary)}
small{flex-shrink:0}
.activity-note{margin:8px 0 0}
.approval-history-row{display:block;border-top:1px solid var(--border-color);padding-top:10px;margin-top:6px}
.approval-history-head,.approval-standalone-head{display:flex;gap:10px;justify-content:space-between;align-items:center;flex-wrap:wrap}
.approval-history-row pre,.native-approval-standalone-card pre{white-space:pre-wrap;overflow-wrap:anywhere;font-size:.82rem;color:var(--text-primary);margin:8px 0 0;padding:8px 10px;background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:6px;max-height:12rem;overflow:auto}
.native-approval-standalone-card{border:1px solid var(--accent-primary);border-radius:10px;padding:12px 14px;background:var(--bg-primary);display:flex;flex-direction:column}
.approval-standalone-title-wrap{display:inline-flex;align-items:center;gap:6px;min-width:0}
.approval-standalone-title-wrap svg{color:var(--accent-primary);flex-shrink:0}
.approval-title{font-weight:600;color:var(--text-primary);overflow-wrap:anywhere}
.approval-standalone-badges{display:inline-flex;align-items:center;gap:8px}
.approval-ttl-pill{padding:2px 8px;border-radius:999px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--text-secondary);font-size:.74rem}
.approval-actions{margin-top:10px;display:flex;flex-wrap:wrap;gap:8px}
.approval-btn-allow,.approval-btn-reject{padding:6px 12px;border-radius:999px;font-size:.8rem;font-weight:600;cursor:pointer;border:1px solid var(--border-color);transition:opacity .15s}
.approval-btn-allow{background:var(--accent-primary);color:var(--bg-primary);border-color:var(--accent-primary)}
.approval-btn-reject{background:var(--bg-secondary);color:var(--text-secondary)}
.approval-btn-allow:disabled,.approval-btn-reject:disabled{opacity:.5;cursor:not-allowed}
</style>
