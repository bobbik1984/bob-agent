<script setup>
import {ref} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from './native-bridge.mjs';
const props=defineProps({paths:{type:Array,default:()=>[]},global:{type:Boolean,default:false}});
const {t}=useI18n();
const rows=ref([]),busy=ref(false),message=ref('');
const names={dsh:'DSH',codex:'Codex',agy:'Antigravity',claude:'Claude Code',pi:'Pi',copilot:'Copilot'};
async function load(){
 busy.value=true;message.value='';
 try { const snapshot=await kernelBridge.dshInventory(props.global?'local':props.paths[0]?.id||'local');
 rows.value=(snapshot.tools||[]).map(row=>props.global?{...row,enabled:row.global_enabled!==false}:row);
 if(rows.value.some(row=>row.policy_synced===false))message.value='toolPolicyPending'; } catch {message.value='toolPolicyReadFailed';}
 finally {busy.value=false;}
}
async function change(row,event){
 const enabled=event.target.checked;
 event.target.checked=row.enabled!==false;busy.value=true;message.value='';
 try{
 const targets=props.global?['local']:props.paths.map(p=>p.id);
 let pending=false;
 for(const connectionId of targets){
 const result=await kernelBridge.dshToolPolicySet({connectionId,tool:row.id,enabled,all:props.global});
 pending ||= (result.nodes||[]).some(n=>!n.synced);
 }
 await load();message.value=pending?'toolPolicyPending':'toolPolicySaved';
 }catch {message.value='toolPolicySaveFailed';}
 finally{busy.value=false;}
}
</script>
<template>
 <details class="tool-policy" @toggle="event=>{if(event.target.open)load()}">
  <summary>{{t(global?'toolPolicyGlobal':'toolPolicyTitle')}}</summary>
  <p>{{t('toolPolicyHint')}}</p>
  <label v-for="row in rows" :key="row.id" class="tool-policy-row">
   <span>{{names[row.id]||row.id}}</span>
   <span>{{t(row.enabled===false?'toolPolicyDisabled':'toolPolicyEnabled')}}</span>
   <input type="checkbox" role="switch" :aria-label="names[row.id]" :checked="row.enabled!==false" :disabled="busy" @change="change(row,$event)" />
  </label>
  <p v-if="message" role="status">{{t(message)}}</p>
 </details>
</template>
<style scoped>
.tool-policy{margin-block:12px;color:var(--text-secondary)}
.tool-policy summary{cursor:pointer}
.tool-policy-row{display:flex;align-items:center;gap:12px;padding:8px 0}
.tool-policy-row span:first-child{flex:1;color:var(--text-primary)}
.tool-policy-row input{width:32px;accent-color:var(--user-accent)}
</style>
