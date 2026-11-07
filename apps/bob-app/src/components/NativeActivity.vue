<script setup>
import {actionableApproval} from '../native-activity.mjs';
import {useI18n} from 'vue-i18n';
defineProps({activity:{type:Object,required:true},busyPermission:{type:String,default:''},approvalErrors:{type:Object,default:()=>({})}});
const emit=defineEmits(['decide']);
const {t}=useI18n();
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
  } catch {}
  return raw;
}
function approvalTitleText(approval) {
  if (approval?.title === 'AskUserQuestion') return t('nativeQuestionTitle');
  if (approval?.title) return approval.title;
  return t(['denied','approved','queued'].includes(approval?.status)?'nativeApprovalLocal':'nativeApprovalSource');
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
 <details class="native-activity-record" :open="(activity.approvals||[]).some(a=>actionableApproval(a)||a.status==='pending')">
  <summary>{{ t('nativeActivityRecords', {count:(activity.items?.length||0)+(activity.approvals?.length||0)}) }}</summary>
  <p v-if="activity.partial" class="activity-note">{{ t('nativeActivityPartial') }}</p>
  <ul>
   <li v-for="item in activity.items||[]" :key="'item-'+item.id"><span>{{ item.label }}</span><small>{{ t(stateKey(item.status)) }}</small></li>
   <li v-for="approval in activity.approvals||[]" :key="'approval-'+approval.id" class="approval-record" :class="{'is-actionable': actionableApproval(approval)}">
    <div><span class="approval-title">{{ approvalTitleText(approval) }}</span><small>{{ t(stateKey(approval.status)) }}</small></div>
    <pre v-if="approval.detail">{{ formatApprovalDetail(approval) }}</pre>
    <p v-if="actionableApproval(approval) && approvalErrors[approval.permission_id]" class="activity-note" role="alert">{{ t(approvalErrors[approval.permission_id]) }}</p>
    <div v-if="actionableApproval(approval)" class="approval-actions">
     <button v-for="option in (approval.options||[]).filter(o=>o.kind==='allow_once')" :key="option.id" class="approval-btn-allow" :disabled="!!busyPermission || approvalErrors[approval.permission_id]==='nativeApprovalRevoked'" @click="emit('decide',approval.permission_id,option.id)">{{ optionButtonLabel(option) }}</button>
     <button class="approval-btn-reject" :disabled="!!busyPermission" @click="emit('decide',approval.permission_id,null)">{{ t('nativeRejectOnce') }}</button>
    </div>
   </li>
  </ul>
 </details>
</template>
<style scoped>
.native-activity-record{margin-block:12px;border:1px solid var(--border-color);border-radius:10px;padding:10px 14px;background:var(--bg-secondary)}
summary{cursor:pointer;color:var(--text-secondary);font-size:.86rem;font-weight:500}ul{list-style:none;padding:0;margin:10px 0 0}li{display:flex;gap:12px;justify-content:space-between;padding:6px 0}li span{min-width:0;overflow-wrap:anywhere}small,.activity-note{color:var(--text-secondary)}small{flex-shrink:0}.activity-note{margin:8px 0 0}
.approval-record{display:block;border-top:1px solid var(--border-color);padding-top:10px;margin-top:6px}.approval-record.is-actionable{background:var(--bg-primary);border:1px solid var(--accent-primary);border-radius:8px;padding:10px 12px}.approval-record>div{display:flex;gap:12px;justify-content:space-between;align-items:center}.approval-title{font-weight:600;color:var(--text-primary)}.approval-record pre{white-space:pre-wrap;overflow-wrap:anywhere;font-size:.82rem;color:var(--text-secondary);margin:8px 0;padding:8px 10px;background:var(--bg-secondary);border-radius:6px}
.approval-actions{margin-top:10px;display:flex;flex-wrap:wrap;gap:8px}
.approval-btn-allow,.approval-btn-reject{padding:6px 12px;border-radius:999px;font-size:.8rem;font-weight:600;cursor:pointer;border:1px solid var(--border-color);transition:opacity .15s}
.approval-btn-allow{background:var(--accent-primary);color:var(--bg-primary);border-color:var(--accent-primary)}
.approval-btn-reject{background:var(--bg-secondary);color:var(--text-secondary)}
.approval-btn-allow:disabled,.approval-btn-reject:disabled{opacity:.5;cursor:not-allowed}
</style>
