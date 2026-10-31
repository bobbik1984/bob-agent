<script setup>
import {computed,ref,watch,onUnmounted} from 'vue';
import {useI18n} from 'vue-i18n';
import {readingCache,readingKey,singleFlight} from '../reading-cache.mjs';
import {kernelBridge} from '../native-bridge.mjs';
import {nodeLabel as label,directoryGroups} from '../conversation-directory.mjs';
const props=defineProps({nodes:Array,catalog:Object,search:String,mode:String});
const emit=defineEmits(['select-local','select-remote']);
const {t}=useI18n();
const remote=ref([]), failures=ref([]), loading=ref(false);let generation=0;
const cachedNodes=new Map();
const listKey=n=>readingKey(n.id,'directory',n.baseUrl||n.url||'');
const display=nodes=>{remote.value=nodes.flatMap(n=>(cachedNodes.get(listKey(n))||[]).map(s=>({...s,node:n,remote:true,uid:`${n.id}:${s.id}`})))};
async function refresh(){const g=++generation;loading.value=true;const nodes=(props.nodes||[]).filter(n=>n.enabled!==false);
 display(nodes);
 await Promise.all(nodes.map(async n=>{const k=listKey(n);if(!cachedNodes.has(k)){const rows=await readingCache.get(k);if(Array.isArray(rows))cachedNodes.set(k,rows)}}));
 if(g!==generation)return;display(nodes);
 const results=await Promise.allSettled(nodes.map(async n=>{const revision=readingCache.revision();const rows=await singleFlight(listKey(n),()=>kernelBridge.dshList(n.id));if(!Array.isArray(rows))throw Error('DSH_INVALID_LIST');cachedNodes.set(listKey(n),rows);await readingCache.put(listKey(n),rows,undefined,revision);return rows}));
 if(g!==generation)return;display(nodes);failures.value=results.flatMap((x,i)=>x.status==='rejected'?[label(nodes[i])]:[]);loading.value=false;}
watch(()=>JSON.stringify((props.nodes||[]).map(n=>[n.id,n.enabled,n.baseUrl])),refresh,{immediate:true});
onUnmounted(()=>{generation++});
const stamp=s=>{const n=Number(s.updated_at)||0;return n<1e12?n*1000:n};
const rows=computed(()=>{
 const projects=props.catalog?.projects||[], imported=new Set(props.catalog?.imported_project_ids||[]);
 const local=(props.catalog?.conversations||[]).filter(s=>!s.id.startsWith('assistant:')).map(s=>{const p=projects.find(p=>p.id===s.project_id);return {...s,title:s.id.startsWith('assistant:')?t('defaultAssistant'):s.title,uid:s.id,project:p?.title,legacy:imported.has(s.project_id)||p?.source_kind==='legacy_import',device:(props.catalog?.device_groups||[]).find(g=>g.conversations?.some(c=>c.id===s.id))?.label||t('sidebarLocal')}});
 const q=(props.search||'').trim().toLocaleLowerCase();
 return [...local,...remote.value].filter(s=>!q||[s.title,s.project,s.workspace,s.remote?label(s.node):s.device].join(' ').toLocaleLowerCase().includes(q)).sort((a,b)=>stamp(b)-stamp(a)||a.uid.localeCompare(b.uid));});
const groups=computed(()=>directoryGroups(rows.value,props.mode));
const legacy=computed(()=>rows.value.filter(s=>s.legacy));
function select(s){emit(s.remote?'select-remote':'select-local',s.remote?{node:s.node,session:s}:s.id)}
const date=s=>stamp(s)?new Date(stamp(s)).toLocaleDateString():t('sidebarTimeUnknown');
</script>
<template>
 <section class="directory" :aria-label="t('conversations')">
  <div class="directory-status"><span v-if="loading" role="status">{{t('dshLoading')}}</span><button type="button" :disabled="loading" @click="refresh">{{t('dshRefresh')}}</button></div>
  <p v-for="name in failures" :key="name" role="status">{{t('directoryUnavailable',{name})}}</p>
  <template v-for="group in groups" :key="group.key">
   <details v-if="group.name" :open="!!search || mode==='device'"><summary :title="group.fullName"><span class="directory-heading"><svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></svg><span>{{group.name}}</span><small>{{group.items.length}}</small></span></summary>
    <div class="directory-children">
    <button v-for="s in group.items" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{s.remote?label(s.node):s.device}} · {{date(s)}}</small></button>
    </div>
   </details>
   <template v-else><button v-for="s in group.items" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{s.remote?label(s.node):s.device}} · {{date(s)}}</small></button></template>
  </template>
  <details v-if="legacy.length" :open="!!search"><summary>{{t('sidebarLegacy',{count:legacy.length})}}</summary><button v-for="s in legacy" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{t('sidebarImported')}} · {{date(s)}}</small></button></details>
 </section>
</template>
<style scoped>
.directory{min-width:0;max-width:100%}.directory-status{display:flex;justify-content:flex-end;align-items:center;gap:8px}.directory details{margin-top:10px}.directory summary{cursor:pointer;padding:8px 0;color:var(--text-secondary)}.directory-heading{display:inline-flex;align-items:center;gap:6px;max-width:calc(100% - 20px);vertical-align:middle;min-width:0;font-weight:600;color:var(--text-primary)}.directory-heading>span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.directory-heading svg{width:16px;height:16px;flex-shrink:0;color:var(--text-secondary)}.directory-heading small{flex-shrink:0;font-weight:400}.directory-children{padding-inline-start:18px;padding-bottom:8px;min-width:0}.directory details+ .directory-row{margin-top:10px}.directory-row{display:flex;flex-direction:column;align-items:flex-start;width:100%;min-width:0;gap:4px;padding:10px;margin-top:4px;text-align:left;background:var(--bg-primary);color:var(--text-primary)}.directory-row span{max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.directory small{font-size:11px;color:var(--text-secondary);overflow-wrap:anywhere}.directory p{overflow-wrap:anywhere}
</style>
