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
const associations=ref({links:[]}), associationError=ref(''), linking=ref(false), linksAvailable=ref(false);
const pinnedUids=ref((()=>{try{return typeof localStorage!=='undefined'?JSON.parse(localStorage.getItem('bob-pinned-sessions')||'[]'):[]}catch{return []}})());
function isPinned(uid){return pinnedUids.value.includes(uid);}
function togglePin(uid){const next=isPinned(uid)?pinnedUids.value.filter(x=>x!==uid):[uid,...pinnedUids.value];pinnedUids.value=next;try{if(typeof localStorage!=='undefined')localStorage.setItem('bob-pinned-sessions',JSON.stringify(next));}catch{}}
const cachedNodes=new Map();
const harness=s=>s.harness||(s.source_kind==='claude_native'?'claude':'dsh');
const association=s=>associations.value.links.find(l=>l.connection_id===s.node?.id&&l.harness===harness(s)&&l.session_id===s.id);
async function loadLinks(){try{associations.value=await kernelBridge.projectLinks();linksAvailable.value=true;}catch{linksAvailable.value=false;}}
async function setProject(s,event){linking.value=true;associationError.value='';try{
 associations.value=await kernelBridge.projectLinkSet({connectionId:s.node.id,harness:harness(s),sessionId:s.id,projectId:event.target.value||null,expectedVersion:association(s)?.version||0});
 }catch{associationError.value=t('projectLinkConflict');await loadLinks();}finally{linking.value=false;}}
loadLinks();
watch(()=>props.catalog?.projects?.length,()=>loadLinks());
const listKey=n=>readingKey(n.id,'directory',n.baseUrl||n.url||'');
const display=nodes=>{remote.value=nodes.flatMap(n=>(cachedNodes.get(listKey(n))||[]).map(s=>({...s,node:n,remote:true,uid:`${n.id}:${s.id}`})))};
async function refresh(){const g=++generation;loading.value=true;const nodes=(props.nodes||[]).filter(n=>n.enabled!==false);
 void loadLinks();
 display(nodes);
 await Promise.all(nodes.map(async n=>{const k=listKey(n);if(!cachedNodes.has(k)){const rows=await readingCache.get(k);if(Array.isArray(rows))cachedNodes.set(k,rows)}}));
 if(g!==generation)return;display(nodes);
 const results=await Promise.allSettled(nodes.map(async n=>{const revision=readingCache.revision();const rows=await singleFlight(listKey(n),()=>kernelBridge.dshList(n.id));if(!Array.isArray(rows))throw Error('DSH_INVALID_LIST');cachedNodes.set(listKey(n),rows);await readingCache.put(listKey(n),rows,undefined,revision);return rows}));
 if(g!==generation)return;display(nodes);failures.value=results.flatMap((x,i)=>x.status==='rejected'?[label(nodes[i])]:[]);loading.value=false;}
watch(()=>JSON.stringify((props.nodes||[]).map(n=>[n.id,n.enabled,n.baseUrl])),refresh,{immediate:true});
onUnmounted(()=>{generation++});
const stamp=s=>{const raw=s.updated_at;const n=Number(raw);return Number.isFinite(n)?(n<1e12?n*1000:n):(Date.parse(raw)||0)};
const rows=computed(()=>{
 const projects=props.catalog?.projects||[], imported=new Set(props.catalog?.imported_project_ids||[]);
 const local=(props.catalog?.conversations||[]).filter(s=>!s.id.startsWith('assistant:')).map(s=>{const p=projects.find(p=>p.id===s.project_id);return {...s,title:s.id.startsWith('assistant:')?t('defaultAssistant'):s.title,uid:s.id,project:p?.title,project_owner:associations.value.owner_instance,legacy:imported.has(s.project_id)||p?.source_kind==='legacy_import',device:(props.catalog?.device_groups||[]).find(g=>g.conversations?.some(c=>c.id===s.id))?.label||t('sidebarLocal')}});
 const q=(props.search||'').trim().toLocaleLowerCase();
 const linkedRemote=remote.value.map(s=>{const a=association(s);return a?.project_id?{...s,project_id:a.project_id,project:a.project_title,project_owner:a.owner_instance}:s;});
 return [...local,...linkedRemote].filter(s=>!q||[s.title,s.project,s.workspace,s.toolLabel,s.harness,s.remote?label(s.node):s.device].join(' ').toLocaleLowerCase().includes(q)).sort((a,b)=>stamp(b)-stamp(a)||a.uid.localeCompare(b.uid));});
const groups=computed(()=>directoryGroups(rows.value,props.mode,pinnedUids.value));
const legacy=computed(()=>rows.value.filter(s=>s.legacy));
function select(s){emit(s.remote?'select-remote':'select-local',s.remote?{node:s.node,session:s}:s.id)}
const date=s=>stamp(s)?new Date(stamp(s)).toLocaleDateString():t('sidebarTimeUnknown');
const toolLabel=s=>s.remote?(s.toolLabel||s.tool_label||(s.harness==='claude'||s.source_kind==='claude_native'?t('claudeAgent'):'')):'';
</script>
<template>
 <section class="directory" :aria-label="t('conversations')">
  <div class="directory-status"><span v-if="loading" role="status">{{t('dshLoading')}}</span><button type="button" :disabled="loading" @click="refresh">{{t('dshRefresh')}}</button></div>
  <p v-for="name in failures" :key="name" role="status">{{t('directoryUnavailable',{name})}}</p>
  <p v-if="associationError" role="alert">{{associationError}}</p>
  <template v-for="group in groups" :key="group.key">
   <details v-if="group.name" :open="!!search || mode==='device' || mode==='smart'"><summary :title="group.fullName"><span class="directory-heading"><svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></svg><span>{{group.name}}</span><small>{{group.items.length}}</small></span></summary>
    <div class="directory-children">
    <div v-for="s in group.items" :key="s.uid" class="directory-item-wrap"><div class="directory-row-bar"><button class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{s.remote?label(s.node):s.device}}<template v-if="toolLabel(s)"> · {{toolLabel(s)}}</template> · {{date(s)}}<template v-if="s.directoryStale"> · {{t('directoryCached')}}</template></small></button><button type="button" class="directory-pin-btn" :class="{pinned:isPinned(s.uid)}" :aria-label="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" :title="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" @click.stop="togglePin(s.uid)"><svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" :fill="isPinned(s.uid)?'currentColor':'none'" stroke="currentColor" stroke-width="1.8"/></svg></button></div>
    <select v-if="s.remote && linksAvailable" class="project-link-select" :aria-label="t('projectLinkLabel')" :disabled="linking" :value="association(s)?.project_id||''" @change="setProject(s,$event)"><option value="">{{t('projectLinkNone')}}</option><option v-for="p in catalog?.projects||[]" :key="p.id" :value="p.id">{{p.title}}</option></select></div>
    </div>
   </details>
   <template v-else><div v-for="s in group.items" :key="s.uid" class="directory-item-wrap"><div class="directory-row-bar"><button class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{s.remote?label(s.node):s.device}}<template v-if="toolLabel(s)"> · {{toolLabel(s)}}</template> · {{date(s)}}<template v-if="s.directoryStale"> · {{t('directoryCached')}}</template></small></button><button type="button" class="directory-pin-btn" :class="{pinned:isPinned(s.uid)}" :aria-label="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" :title="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" @click.stop="togglePin(s.uid)"><svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" :fill="isPinned(s.uid)?'currentColor':'none'" stroke="currentColor" stroke-width="1.8"/></svg></button></div>
    <select v-if="s.remote && linksAvailable" class="project-link-select" :aria-label="t('projectLinkLabel')" :disabled="linking" :value="association(s)?.project_id||''" @change="setProject(s,$event)"><option value="">{{t('projectLinkNone')}}</option><option v-for="p in catalog?.projects||[]" :key="p.id" :value="p.id">{{p.title}}</option></select></div></template>
  </template>
  <details v-if="legacy.length" :open="!!search"><summary>{{t('sidebarLegacy',{count:legacy.length})}}</summary><button v-for="s in legacy" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{t('sidebarImported')}} · {{date(s)}}<template v-if="s.directoryStale"> · {{t('directoryCached')}}</template></small></button></details>
 </section>
</template>
<style scoped>
.project-link-select{max-width:100%;width:100%;margin-top:2px;min-height:24px;padding:2px 6px;font-size:11px;color:var(--text-secondary);background:var(--bg-primary)}.directory{min-width:0;max-width:100%}.directory-status{display:flex;justify-content:flex-end;align-items:center;gap:8px}.directory details{margin-top:10px}.directory summary{cursor:pointer;padding:8px 0;color:var(--text-secondary)}.directory-heading{display:inline-flex;align-items:center;gap:6px;max-width:calc(100% - 20px);vertical-align:middle;min-width:0;font-weight:600;color:var(--text-primary)}.directory-heading>span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.directory-heading svg{width:16px;height:16px;flex-shrink:0;color:var(--text-secondary)}.directory-heading small{flex-shrink:0;font-weight:400}.directory-children{padding-inline-start:14px;padding-bottom:8px;min-width:0}.directory details+ .directory-row{margin-top:10px}.directory-item-wrap{margin-top:4px}.directory-row-bar{display:flex;align-items:stretch;gap:4px;min-width:0}.directory-row{display:flex;flex:1;flex-direction:column;align-items:flex-start;min-width:0;gap:3px;padding:8px 10px;text-align:left;background:var(--bg-primary);color:var(--text-primary)}.directory-row span{max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.directory-pin-btn{flex:0 0 28px;width:28px;min-height:28px;padding:0;display:inline-flex;align-items:center;justify-content:center;color:var(--text-secondary);background:var(--bg-primary)}.directory-pin-btn.pinned{color:var(--accent-primary);border-color:var(--accent-primary)}.directory small{font-size:11px;color:var(--text-secondary);overflow-wrap:anywhere}.directory p{overflow-wrap:anywhere}
</style>
