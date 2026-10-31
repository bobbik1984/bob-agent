<script setup>
import {computed,ref,watch,onUnmounted} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from '../native-bridge.mjs';
const props=defineProps({nodes:Array,catalog:Object,search:String,mode:String});
const emit=defineEmits(['select-local','select-remote']);
const {t}=useI18n();
const remote=ref([]), failures=ref([]), loading=ref(false);let generation=0;
const label=n=>n.label||n.name||n.baseUrl||n.id;
async function refresh(){const g=++generation;loading.value=true;const nodes=(props.nodes||[]).filter(n=>n.enabled!==false);
 const results=await Promise.allSettled(nodes.map(async n=>(await kernelBridge.dshList(n.id)).map(s=>({...s,node:n,remote:true,uid:`${n.id}:${s.id}`}))));
 if(g!==generation)return;remote.value=results.flatMap(x=>x.status==='fulfilled'?x.value:[]);failures.value=results.flatMap((x,i)=>x.status==='rejected'?[label(nodes[i])]:[]);loading.value=false;}
watch(()=>JSON.stringify((props.nodes||[]).map(n=>[n.id,n.enabled,n.baseUrl])),refresh,{immediate:true});
onUnmounted(()=>{generation++});
const stamp=s=>{const n=Number(s.updated_at)||0;return n<1e12?n*1000:n};
const rows=computed(()=>{
 const projects=props.catalog?.projects||[], imported=new Set(props.catalog?.imported_project_ids||[]);
 const local=(props.catalog?.conversations||[]).map(s=>{const p=projects.find(p=>p.id===s.project_id);return {...s,uid:s.id,project:p?.title,legacy:imported.has(s.project_id)||p?.source_kind==='legacy_import',device:(props.catalog?.device_groups||[]).find(g=>g.conversations?.some(c=>c.id===s.id))?.label||t('sidebarLocal')}});
 const q=(props.search||'').trim().toLocaleLowerCase();
 return [...local,...remote.value].filter(s=>!q||[s.title,s.project,s.workspace,s.remote?label(s.node):s.device].join(' ').toLocaleLowerCase().includes(q)).sort((a,b)=>stamp(b)-stamp(a)||a.uid.localeCompare(b.uid));});
const groups=computed(()=>{const map=new Map();for(const s of rows.value.filter(s=>!s.legacy)){
 const name=props.mode==='device'?(s.remote?label(s.node):s.device):(s.project||s.workspace||'');
 if(!map.has(name))map.set(name,[]);map.get(name).push(s);}return [...map].map(([name,items])=>({name,items}));});
const legacy=computed(()=>rows.value.filter(s=>s.legacy));
function select(s){emit(s.remote?'select-remote':'select-local',s.remote?{node:s.node,session:s}:s.id)}
const date=s=>stamp(s)?new Date(stamp(s)).toLocaleDateString():t('sidebarTimeUnknown');
</script>
<template>
 <section class="directory" :aria-label="t('conversations')">
  <div class="directory-status"><span v-if="loading" role="status">{{t('dshLoading')}}</span><button type="button" :disabled="loading" @click="refresh">{{t('dshRefresh')}}</button></div>
  <p v-for="name in failures" :key="name" role="status">{{t('directoryUnavailable',{name})}}</p>
  <template v-for="group in groups" :key="group.name">
   <details v-if="group.name" :open="!!search || mode==='device'"><summary>{{group.name}} <small>{{group.items.length}}</small></summary>
    <button v-for="s in group.items" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{s.remote?label(s.node):s.device}} · {{date(s)}}</small></button>
   </details>
   <template v-else><button v-for="s in group.items" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{s.remote?label(s.node):s.device}} · {{date(s)}}</small></button></template>
  </template>
  <details v-if="legacy.length" :open="!!search"><summary>{{t('sidebarLegacy',{count:legacy.length})}}</summary><button v-for="s in legacy" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{t('sidebarImported')}} · {{date(s)}}</small></button></details>
 </section>
</template>
<style scoped>
.directory{min-width:0;max-width:100%}.directory-status{display:flex;justify-content:flex-end;align-items:center;gap:8px}.directory details{margin-top:10px}.directory summary{cursor:pointer;padding:8px 0;overflow-wrap:anywhere;color:var(--text-secondary)}.directory-row{display:flex;flex-direction:column;align-items:flex-start;width:100%;min-width:0;gap:4px;padding:10px;margin-top:4px;text-align:left;background:var(--bg-primary);color:var(--text-primary)}.directory-row span{max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.directory small{font-size:11px;color:var(--text-secondary);overflow-wrap:anywhere}.directory p{overflow-wrap:anywhere}
</style>
