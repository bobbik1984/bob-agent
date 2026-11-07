<script setup>
import {actionableApproval} from '../native-activity.mjs';
import {useI18n} from 'vue-i18n';
defineProps({activity:{type:Object,required:true},busyPermission:{type:String,default:''},approvalErrors:{type:Object,default:()=>({})}});
const emit=defineEmits(['decide']);
const {t}=useI18n();
const stateKey=s=>({running:'nativeToolRunning',completed:'nativeToolCompleted',error:'nativeToolFailed',queued:'nativeApprovalQueued',approved:'nativeApprovalApproved',denied:'nativeApprovalDenied',pending:'nativeApprovalPending',cleared:'nativeApprovalCleared',unconfirmed:'nativeStateUnconfirmed'}[s]||'nativeStateUnconfirmed');
</script>
<template>
 <details class="native-activity-record" :open="(activity.approvals||[]).some(a=>actionableApproval(a))">
  <summary>{{ t('nativeActivityRecords', {count:(activity.items?.length||0)+(activity.approvals?.length||0)}) }}</summary>
  <p v-if="activity.partial" class="activity-note">{{ t('nativeActivityPartial') }}</p>
  <ul>
   <li v-for="item in activity.items||[]" :key="'item-'+item.id"><span>{{ item.label }}</span><small>{{ t(stateKey(item.status)) }}</small></li>
   <li v-for="approval in activity.approvals||[]" :key="'approval-'+approval.id" class="approval-record">
    <div><span>{{ approval.title || t(['denied','approved','queued'].includes(approval.status)?'nativeApprovalLocal':'nativeApprovalSource') }}</span><small>{{ t(stateKey(approval.status)) }}</small></div>
    <pre v-if="approval.detail">{{ approval.detail }}</pre>
    <p v-if="actionableApproval(approval) && approvalErrors[approval.permission_id]" class="activity-note" role="alert">{{ t(approvalErrors[approval.permission_id]) }}</p>
    <div v-if="actionableApproval(approval)" class="approval-actions">
     <button v-for="option in (approval.options||[]).filter(o=>o.kind==='allow_once')" :key="option.id" :disabled="!!busyPermission || approvalErrors[approval.permission_id]==='nativeApprovalRevoked'" @click="emit('decide',approval.permission_id,option.id)">{{ t('nativeApproveOnce') }}</button>
     <button :disabled="!!busyPermission" @click="emit('decide',approval.permission_id,null)">{{ t('nativeRejectOnce') }}</button>
    </div>
   </li>
  </ul>
 </details>
</template>
<style scoped>
.native-activity-record{margin-block:12px;border:1px solid var(--border-color);border-radius:8px;padding:8px 12px;background:var(--bg-secondary)}
summary{cursor:pointer;color:var(--text-secondary)}ul{list-style:none;padding:0;margin:10px 0 0}li{display:flex;gap:12px;justify-content:space-between;padding:6px 0}li span{min-width:0;overflow-wrap:anywhere}small,.activity-note{color:var(--text-secondary)}small{flex-shrink:0}.activity-note{margin:8px 0 0}
.approval-record{display:block}.approval-record>div{display:flex;gap:12px;justify-content:space-between}.approval-record pre{white-space:pre-wrap;overflow-wrap:anywhere;font-size:.8rem;color:var(--text-secondary)}.approval-actions{margin-top:8px}
</style>
