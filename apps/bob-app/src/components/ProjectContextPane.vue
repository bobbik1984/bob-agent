<script setup>
import {ref,onMounted,onUnmounted} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from '../native-bridge.mjs';
const props=defineProps({active:Boolean});let timer;const cards=ref([]);const {t}=useI18n();const grants=ref([]),selected=ref(''),snapshot=ref(null),error=ref(''),busy=ref(false);
async function read(refresh=false){if(!selected.value||busy.value)return;busy.value=true;error.value='';try{snapshot.value=await kernelBridge.contextRead(selected.value,refresh);cards.value=await kernelBridge.contextTasks(selected.value)}catch(e){error.value=String(e?.message||e);snapshot.value=null}finally{busy.value=false}}
onUnmounted(()=>clearInterval(timer));
onMounted(async()=>{timer=setInterval(()=>{if(props.active&&!document.hidden&&selected.value)read()},5000);try{grants.value=await kernelBridge.contextList()}catch{error.value=t('contextHostOnly')}});
</script>
<template><section class="context-pane"><h3>{{t('contextTitle')}}</h3><p>{{t('contextScope')}}</p><p v-if="!grants.length">{{t('contextNoGrants')}}</p>
<select v-if="grants.length" v-model="selected" :aria-label="t('contextTitle')" @change="read()"><option value="">{{t('contextChoose')}}</option><option v-for="g in grants" :key="g.id" :value="g.id">{{g.project_id}} / {{g.workspace_id}}</option></select>
<button v-if="selected" :disabled="busy" @click="read(true)">{{t('contextRefresh')}}</button><p v-if="busy" role="status">{{t('contextLoading')}}</p><p v-if="error" role="alert">{{error}}</p>
<article v-if="snapshot"><strong>{{t(snapshot.current?'contextCurrent':'contextStale')}}</strong><p>{{snapshot.capsule.baseline.commit_sha}} · {{snapshot.capsule.baseline.owner_instance}}</p><small>{{snapshot.capsule.capsule_id}}</small><p>{{t('contextReadOnly')}}</p><details v-for="source in snapshot.capsule.source_manifest" :key="source.path"><summary>{{source.path}}</summary><small>{{source.sha256}}</small><pre>{{snapshot.capsule.excerpts[source.path]}}</pre></details><details v-if="cards.length"><summary>{{t('contextTaskCards')}}</summary><pre v-for="card in cards" :key="card.request_id">{{JSON.stringify(card.task_card,null,2)}}</pre></details></article>
</section></template>
<style scoped>.context-pane{padding:16px;border:1px solid var(--border-color);border-radius:8px;min-width:0}.context-pane p,.context-pane small{overflow-wrap:anywhere;color:var(--text-secondary)}.context-pane pre{white-space:pre-wrap;overflow-wrap:anywhere;background:var(--bg-secondary);padding:8px}.context-pane button,.context-pane select{margin:4px;max-width:100%}.context-pane details{margin-top:8px}</style>
