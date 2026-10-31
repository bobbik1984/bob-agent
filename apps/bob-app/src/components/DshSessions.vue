<script setup>
import { computed, ref, watch, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from '../native-bridge.mjs';
import MessageContent from './MessageContent.vue';
const props=defineProps({nodes:{type:Array,default:()=>[]}, target:{type:Object,default:null}});
const {t}=useI18n();
const connection=ref(''), sessions=ref([]), selected=ref(null), history=ref(null), search=ref(''), busy=ref(false), sending=ref(false), error=ref(''), notice=ref(''), draft=ref(''), pending=ref(null);
let generation=0, disposed=false, restoring=false, timer;
const nodes=computed(()=>props.nodes.filter(n=>n.enabled!==false));
const key=()=>`bob-dsh:${connection.value}:${selected.value?.id || ''}`;
function readState(){try{return JSON.parse(localStorage.getItem(key()) || '{}')}catch{return {}}}
function persist(){if(restoring || !selected.value)return;try{localStorage.setItem(key(),JSON.stringify({draft:draft.value,pending:pending.value}))}catch{error.value=t('dshStorageError')}}
watch(draft,persist,{flush:'sync'});
const groups=computed(()=>{
 const map=new Map();
 for(const s of sessions.value){if(search.value.trim() && !`${s.title} ${s.workspace || ''}`.toLocaleLowerCase().includes(search.value.trim().toLocaleLowerCase()))continue;
 const k=typeof s.workspace==='string' && s.workspace ? s.workspace : t('dshNoWorkspace');
 if(!map.has(k))map.set(k,[]);map.get(k).push(s);}
 return [...map].map(([name,items])=>({name,items}));
});
function fault(e){const code=String(e?.message || e);return `${t(code.includes('DSH_RECEIPT_UNKNOWN')?'dshUncertain':'dshError')} · ${code.match(/DSH_[A-Z_]+/)?.[0] || 'DSH_UNAVAILABLE'}`}
async function load(){const g=++generation;clearTimeout(timer);busy.value=true;error.value='';selected.value=null;history.value=null;sessions.value=[];try{const items=await kernelBridge.dshList(connection.value);if(g===generation&&!disposed)sessions.value=items;}catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation)busy.value=false}}
watch(connection,()=>{if(connection.value&&!props.target)load()});
async function refresh(){if(!selected.value||busy.value)return;clearTimeout(timer);const g=generation;const id=selected.value.id;busy.value=true;try{const h=await kernelBridge.dshHistory(connection.value,id);if(g!==generation||disposed)return;if(h.address.session_id!==id||h.address.connection_id!==connection.value)throw Error('DSH_IDENTITY_MISMATCH');history.value=h;error.value='';}catch(e){if(g===generation)error.value=fault(e)}finally{if(g===generation){busy.value=false;timer=setTimeout(refresh,5000)}}}
async function open(s){persist();++generation;clearTimeout(timer);busy.value=false;selected.value=s;history.value=null;notice.value='';const saved=readState();restoring=true;draft.value=saved.draft||'';pending.value=saved.pending||null;restoring=false;if(pending.value)notice.value=t('dshUncertain');await refresh()}
function back(){persist();++generation;clearTimeout(timer);busy.value=false;selected.value=null;history.value=null;notice.value='';load()}
async function send(){if(sending.value||pending.value||!draft.value.trim())return;const text=draft.value.trim(), g=generation, address={connection:connection.value,session:selected.value.id};pending.value={id:crypto.randomUUID(),text};persist();sending.value=true;error.value='';try{await kernelBridge.dshPrompt(address.connection,address.session,text,pending.value.id);if(g===generation){pending.value=null;if(draft.value.trim()===text)draft.value='';persist();notice.value=t('dshAccepted');}}catch(e){if(g===generation){error.value=fault(e);notice.value=t('dshUncertain')}}finally{sending.value=false;if(g===generation&&!busy.value)refresh()}}
watch(()=>props.target, async target=>{if(!target)return;connection.value=target.node.id;await open(target.session)},{immediate:true});
onUnmounted(()=>{persist();disposed=true;++generation;clearTimeout(timer)});
</script>
<template>
 <section class="dsh-sessions" :class="{embedded:!!target}" :aria-label="t('dshTitle')">
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
   <article v-for="m in history?.messages || []" :key="m.sequence" class="dsh-message" :class="{user:m.role==='user'}"><small>{{ t(m.role==='user'?'dshYou':'dshAgent') }}</small><MessageContent :content="m.text" :plain="m.role==='user'" /></article>
   <form class="dsh-compose" @submit.prevent="send"><textarea v-model="draft" :disabled="sending" :placeholder="t('dshPrompt')" :aria-label="t('dshPrompt')" rows="3"></textarea><button :disabled="sending || !!pending || !draft.trim() || !history">{{ t(sending?'dshSending':'send') }}</button><p v-if="pending && !sending" role="status">{{ t('dshUncertain') }}</p></form>
  </template>
 </section>
</template>
<style scoped>
.dsh-sessions{min-width:0;max-width:100%;padding:16px;border:1px solid var(--border-color);border-radius:12px;background:var(--bg-primary)}
.dsh-sessions.embedded{border:0;border-radius:0;padding:12px;overflow-y:auto;flex:1;min-height:0;scrollbar-gutter:stable}header,.dsh-tools{display:flex;align-items:center;gap:8px;flex-wrap:wrap}h3,h4{margin:0;overflow-wrap:anywhere}select,input,textarea{min-width:0;max-width:100%;color:var(--text-primary);background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:8px;padding:8px}select,input{flex:1}button{max-width:100%;white-space:normal}details{margin-top:10px}summary{overflow-wrap:anywhere;cursor:pointer;padding:8px 0}.dsh-note,small{color:var(--text-secondary);overflow-wrap:anywhere}.dsh-session{display:flex;flex-direction:column;text-align:left;width:100%;gap:5px;margin-top:5px;padding:10px}.dsh-session strong{overflow-wrap:anywhere}.dsh-message{min-width:0;margin-top:20px;overflow-wrap:anywhere}.dsh-message.user{background:var(--bg-secondary);border-radius:10px;padding:12px}.dsh-compose{margin-top:20px;display:flex;flex-wrap:wrap;gap:8px}.dsh-compose textarea{flex:1;min-width:65%}.dsh-compose p{width:100%}
</style>
