<script setup>
import {computed,ref,watch,onUnmounted} from 'vue';
import HarnessIcon from './HarnessIcon.vue';
import {useI18n} from 'vue-i18n';
import {readingCache,readingKey,singleFlight} from '../reading-cache.mjs';
import {kernelBridge} from '../native-bridge.mjs';
import {nodeLabel as label,directoryGroups,extractFilterOptions,groupIconKind} from '../conversation-directory.mjs';
import {checkPath,pathState} from '../path-health.mjs';
import {deviceGroups,deviceFilterKeys} from '../device-paths.mjs';
const props=defineProps({nodes:Array,catalog:Object,search:String,mode:String});
const emit=defineEmits(['select-local','select-remote']);
const {t}=useI18n();
const remote=ref([]), failures=ref([]), loading=ref(false), rodIndex=ref(0);let generation=0, rodTimer=null;
const rodFrames=['|','/','─','\\'];
const rodFrame=computed(()=>rodFrames[rodIndex.value%rodFrames.length]);
watch(loading,val=>{if(typeof setInterval!=='function')return;if(val&&!rodTimer){rodTimer=setInterval(()=>{rodIndex.value=(rodIndex.value+1)%rodFrames.length;},120);}else if(!val&&rodTimer){clearInterval(rodTimer);rodTimer=null;}},{immediate:true});
const associations=ref({links:[]}), associationError=ref(''), linking=ref(false), linksAvailable=ref(false);
const pinnedUids=ref((()=>{try{return typeof localStorage!=='undefined'?JSON.parse(localStorage.getItem('bob-pinned-sessions')||'[]'):[]}catch{return []}})());
function isPinned(uid){return pinnedUids.value.includes(uid);}
function togglePin(uid){const next=isPinned(uid)?pinnedUids.value.filter(x=>x!==uid):[uid,...pinnedUids.value];pinnedUids.value=next;try{if(typeof localStorage!=='undefined')localStorage.setItem('bob-pinned-sessions',JSON.stringify(next));}catch{}}
const savedFilters=(()=>{try{return typeof localStorage!=='undefined'?JSON.parse(localStorage.getItem('bob-directory-filters')||'{}'):{}}catch{return {}}})();
const filterOpen=ref(Boolean(savedFilters.filterOpen));
const selectedNodes=ref(Array.isArray(savedFilters.selectedNodes)?savedFilters.selectedNodes:[]);
const selectedHarnesses=ref(Array.isArray(savedFilters.selectedHarnesses)?savedFilters.selectedHarnesses:[]);
const selectedTopic=ref(typeof savedFilters.selectedTopic==='string'?savedFilters.selectedTopic:'all');
const sortBy=ref(typeof savedFilters.sortBy==='string'?savedFilters.sortBy:'time_desc');
function saveFilters(){try{if(typeof localStorage!=='undefined')localStorage.setItem('bob-directory-filters',JSON.stringify({filterOpen:filterOpen.value,selectedNodes:selectedNodes.value,selectedHarnesses:selectedHarnesses.value,selectedTopic:selectedTopic.value,sortBy:sortBy.value}));}catch{}}
watch([filterOpen,selectedNodes,selectedHarnesses,selectedTopic,sortBy],saveFilters,{deep:true});
function toggleNodeFilter(key){selectedNodes.value=selectedNodes.value.includes(key)?selectedNodes.value.filter(x=>x!==key):[...selectedNodes.value,key];}
function toggleHarnessFilter(key){selectedHarnesses.value=selectedHarnesses.value.includes(key)?selectedHarnesses.value.filter(x=>x!==key):[...selectedHarnesses.value,key];}
function resetFilters(){selectedNodes.value=[];selectedHarnesses.value=[];selectedTopic.value='all';sortBy.value='time_desc';}
const activeFilterCount=computed(()=>(selectedNodes.value.length>0?1:0)+(selectedHarnesses.value.length>0?1:0)+(selectedTopic.value&&selectedTopic.value!=='all'?1:0)+(sortBy.value&&sortBy.value!=='time_desc'?1:0));
const cachedNodes=new Map();
const harness=s=>s.harness||(s.source_kind==='claude_native'?'claude':'dsh');
const association=s=>associations.value.links.find(l=>l.connection_id===s.node?.id&&l.harness===harness(s)&&l.session_id===s.id);
async function loadLinks(){try{associations.value=await kernelBridge.projectLinks();linksAvailable.value=true;}catch{linksAvailable.value=false;}}
async function setProject(s,event){linking.value=true;associationError.value='';try{
 associations.value=await kernelBridge.projectLinkSet({connectionId:s.node.id,harness:harness(s),sessionId:s.id,projectId:event.target.value||null,expectedVersion:association(s)?.version||0});
 }catch{associationError.value=t('projectLinkConflict');await loadLinks();}finally{linking.value=false;}}
loadLinks();
watch(()=>[props.catalog?.projects?.length,props.catalog?.conversations?.[0]?.updated_at],()=>loadLinks());
const listKey=n=>readingKey(n.id,'directory',n.baseUrl||n.url||'');
const display=nodes=>{const labels=new Map(deviceGroups(nodes).map(g=>[g.id,g.label]));remote.value=nodes.flatMap(n=>(cachedNodes.get(listKey(n))||[]).map(s=>({...s,node:{...n,label:labels.get(n.deviceId||n.id)},remote:true,directoryStale:s.directoryStale||pathState(n)!=='readable',uid:`${n.id}:${s.id}`})))};
async function refresh(){const g=++generation;loading.value=true;const nodes=(props.nodes||[]).filter(n=>n.enabled!==false);
 failures.value=failures.value.filter(f=>nodes.some(n=>n.id===f.id&&(n.baseUrl||n.url)===f.url));
 void loadLinks();
 display(nodes);
 await Promise.all(nodes.map(async n=>{const k=listKey(n);if(!cachedNodes.has(k)){const rows=await readingCache.get(k);if(Array.isArray(rows))cachedNodes.set(k,rows)}}));
 if(g!==generation)return;display(nodes);
 const results=await Promise.allSettled(nodes.map(async n=>{const revision=readingCache.revision();const rows=await singleFlight(listKey(n),()=>checkPath(n,kernelBridge));if(!Array.isArray(rows))throw Error('DSH_INVALID_LIST');cachedNodes.set(listKey(n),rows);await readingCache.put(listKey(n),rows,undefined,revision);return rows}));
 if(g!==generation)return;display(nodes);failures.value=results.flatMap((x,i)=>x.status==='rejected'?[{id:nodes[i].id,name:label(nodes[i]),url:nodes[i].baseUrl||nodes[i].url,cached:Boolean(cachedNodes.get(listKey(nodes[i]))?.length)}]:[]);loading.value=false;}
watch(()=>JSON.stringify((props.nodes||[]).map(n=>[n.id,n.deviceId,n.enabled,n.baseUrl])),()=>{selectedNodes.value=deviceFilterKeys(selectedNodes.value,props.nodes||[]);return refresh()},{immediate:true});
onUnmounted(()=>{if(rodTimer&&typeof clearInterval==='function')clearInterval(rodTimer);generation++});
const stamp=s=>{const raw=s.updated_at;const n=Number(raw);return Number.isFinite(n)?(n<1e12?n*1000:n):(Date.parse(raw)||0)};
const rows=computed(()=>{
 const projects=props.catalog?.projects||[], imported=new Set(props.catalog?.imported_project_ids||[]);
 const local=(props.catalog?.conversations||[]).filter(s=>!s.id.startsWith('assistant:')).map(s=>{const p=projects.find(p=>p.id===s.project_id);return {...s,title:s.id.startsWith('assistant:')?t('defaultAssistant'):s.title,uid:s.id,project:p?.title,project_owner:associations.value.owner_instance,legacy:imported.has(s.project_id)||p?.source_kind==='legacy_import',device:(props.catalog?.device_groups||[]).find(g=>g.conversations?.some(c=>c.id===s.id))?.label||t('sidebarLocal')}});
 const q=(props.search||'').trim().toLocaleLowerCase();
 const linkedRemote=remote.value.map(s=>{const a=association(s);return a?.project_id?{...s,project_id:a.project_id,project:a.project_title,project_owner:a.owner_instance}:s;});
 return [...local,...linkedRemote].filter(s=>!q||[s.title,s.project,s.workspace,s.toolLabel,s.harness,s.remote?label(s.node):s.device].join(' ').toLocaleLowerCase().includes(q)).sort((a,b)=>stamp(b)-stamp(a)||a.uid.localeCompare(b.uid));});
const filterOptions=computed(()=>extractFilterOptions(rows.value));
const groups=computed(()=>directoryGroups(rows.value,props.mode,pinnedUids.value,{selectedNodes:selectedNodes.value,selectedHarnesses:selectedHarnesses.value,selectedTopic:selectedTopic.value,sortBy:sortBy.value}));
const legacy=computed(()=>rows.value.filter(s=>s.legacy));
function select(s){emit(s.remote?'select-remote':'select-local',s.remote?{node:s.node,session:s}:s.id)}
const date=s=>stamp(s)?new Date(stamp(s)).toLocaleDateString():t('sidebarTimeUnknown');
const toolLabel=s=>s.remote?(s.toolLabel||s.tool_label||(s.harness==='claude'||s.source_kind==='claude_native'?t('claudeAgent'):'')):'';
</script>
<template>
 <section class="directory" :aria-label="t('conversations')">
  <div class="directory-status">
   <div class="directory-filter-bar">
    <button type="button" class="directory-filter-toggle" :class="{active:filterOpen||activeFilterCount>0}" :aria-expanded="filterOpen" @click="filterOpen=!filterOpen">
     <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6h16M7 12h10M10 18h4"/></svg>
     <span>{{t('filterToggleBtn')}}</span>
     <small v-if="activeFilterCount>0" class="filter-count-badge">{{activeFilterCount}}</small>
    </button>
    <button v-if="activeFilterCount>0" type="button" class="directory-filter-reset" @click="resetFilters">{{t('filterResetBtn')}}</button>
   </div>
   <div class="directory-refresh-bar">
    <span v-if="loading" class="terminal-status-line" role="status"><span class="terminal-rod" aria-hidden="true">[{{rodFrame}}]</span><span>{{t('dshLoading')}}</span><span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></span>
    <button type="button" :disabled="loading" @click="refresh">{{t('dshRefresh')}}</button>
   </div>
  </div>
  <div v-if="filterOpen" class="directory-filter-panel">
   <div v-if="filterOptions.nodes.length>0" class="filter-group">
    <span class="filter-label">{{t('filterNodesLabel')}}</span>
    <div class="filter-chips">
     <button type="button" class="filter-chip" :class="{active:selectedNodes.length===0}" @click="selectedNodes=[]">{{t('filterAllOption')}}</button>
     <button v-for="n in filterOptions.nodes" :key="n.key" type="button" class="filter-chip" :class="{active:selectedNodes.includes(n.key)}" @click="toggleNodeFilter(n.key)">{{n.label}} <small>{{n.count}}</small></button>
    </div>
   </div>
   <div v-if="filterOptions.harnesses.length>1" class="filter-group">
    <span class="filter-label">{{t('filterHarnessesLabel')}}</span>
    <div class="filter-chips">
     <button type="button" class="filter-chip" :class="{active:selectedHarnesses.length===0}" @click="selectedHarnesses=[]">{{t('filterAllOption')}}</button>
     <button v-for="h in filterOptions.harnesses" :key="h.key" type="button" class="filter-chip" :class="{active:selectedHarnesses.includes(h.key)}" @click="toggleHarnessFilter(h.key)">{{h.label}} <small>{{h.count}}</small></button>
    </div>
   </div>
   <div class="filter-select-grid">
    <label class="filter-select-field">
     <span>{{t('filterTopicLabel')}}</span>
     <select v-model="selectedTopic" :aria-label="t('filterTopicLabel')">
      <option value="all">{{t('filterTopicAll')}}</option>
      <option value="pinned">{{t('filterTopicPinned')}}</option>
      <option value="unlinked">{{t('filterTopicUnlinked')}}</option>
      <option v-for="tp in filterOptions.topics" :key="tp.key" :value="tp.key">{{tp.label}} ({{tp.count}})</option>
     </select>
    </label>
    <label class="filter-select-field">
     <span>{{t('filterSortLabel')}}</span>
     <select v-model="sortBy" :aria-label="t('filterSortLabel')">
      <option value="time_desc">{{t('filterSortTimeDesc')}}</option>
      <option value="time_asc">{{t('filterSortTimeAsc')}}</option>
      <option value="title_asc">{{t('filterSortTitleAsc')}}</option>
      <option value="node_asc">{{t('filterSortNodeAsc')}}</option>
      <option value="topic_asc">{{t('filterSortTopicAsc')}}</option>
     </select>
    </label>
   </div>
  </div>
  <p v-for="failure in failures" :key="failure.id" role="status">{{t('directoryPathFailed',{name:failure.name,url:failure.url})}} <span v-if="failure.cached">{{t('directoryCacheRetained')}}</span></p>
  <p v-if="associationError" role="alert">{{associationError}}</p>
  <template v-for="group in groups" :key="group.key">
   <details v-if="group.isTestGroup" class="directory-test-group" :open="!!search"><summary :title="group.fullName"><span class="directory-heading test-heading"><svg aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="4" rx="1"/><path d="M5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/></svg><span>{{t('testAndScratchSessions')}}</span><small>{{group.items.length}}</small></span></summary>
    <div class="directory-children">
    <div v-for="s in group.items" :key="s.uid" class="directory-item-wrap"><div class="directory-row-bar"><button class="directory-row" @click="select(s)"><span>{{s.title}}</span><small><span class="node-tag-pill">{{s.remote?label(s.node):s.device}}</span><span v-if="s.remote && (props.nodes||[]).filter(n=>(n.deviceId||n.id)===(s.node.deviceId||s.node.id)).length>1"> · {{s.node.baseUrl||s.node.url}}</span><template v-if="toolLabel(s)"> · <HarnessIcon :session="s" :label="toolLabel(s)" /></template> · {{date(s)}}<template v-if="s.directoryStale"> · {{t('directoryCached')}}</template></small></button><button type="button" class="directory-pin-btn" :class="{pinned:isPinned(s.uid)}" :aria-label="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" :title="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" @click.stop="togglePin(s.uid)"><svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" :fill="isPinned(s.uid)?'currentColor':'none'" stroke="currentColor" stroke-width="1.8"/></svg></button></div>
    <select v-if="s.remote && linksAvailable" class="project-link-select" :aria-label="t('projectLinkLabel')" :disabled="linking" :value="association(s)?.project_id||''" @change="setProject(s,$event)"><option value="">{{t('projectLinkNone')}}</option><option v-for="p in catalog?.projects||[]" :key="p.id" :value="p.id">{{p.title}}</option></select></div>
    </div>
   </details>
   <details v-else-if="group.name" :open="!!search || mode==='device' || mode==='smart' || activeFilterCount>0"><summary :title="group.fullName"><span class="directory-heading"><svg v-if="groupIconKind(group, mode)==='project'" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z"/></svg><svg v-else-if="groupIconKind(group, mode)==='phone'" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="2" width="12" height="20" rx="2.5"/><path d="M10 18h4"/></svg><svg v-else-if="groupIconKind(group, mode)==='cloud'" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 19h11a4.5 4.5 0 0 0 .7-8.95A5.5 5.5 0 0 0 7.4 8.5 4 4 0 0 0 6.5 19Z"/></svg><svg v-else aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></svg><span>{{group.name}}</span><small>{{group.items.length}}</small></span></summary>
    <div class="directory-children">
    <div v-for="s in group.items" :key="s.uid" class="directory-item-wrap"><div class="directory-row-bar"><button class="directory-row" @click="select(s)"><span>{{s.title}}</span><small><span class="node-tag-pill">{{s.remote?label(s.node):s.device}}</span><span v-if="s.remote && (props.nodes||[]).filter(n=>(n.deviceId||n.id)===(s.node.deviceId||s.node.id)).length>1"> · {{s.node.baseUrl||s.node.url}}</span><template v-if="toolLabel(s)"> · <HarnessIcon :session="s" :label="toolLabel(s)" /></template> · {{date(s)}}<template v-if="s.directoryStale"> · {{t('directoryCached')}}</template></small></button><button type="button" class="directory-pin-btn" :class="{pinned:isPinned(s.uid)}" :aria-label="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" :title="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" @click.stop="togglePin(s.uid)"><svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" :fill="isPinned(s.uid)?'currentColor':'none'" stroke="currentColor" stroke-width="1.8"/></svg></button></div>
    <select v-if="s.remote && linksAvailable" class="project-link-select" :aria-label="t('projectLinkLabel')" :disabled="linking" :value="association(s)?.project_id||''" @change="setProject(s,$event)"><option value="">{{t('projectLinkNone')}}</option><option v-for="p in catalog?.projects||[]" :key="p.id" :value="p.id">{{p.title}}</option></select></div>
    </div>
   </details>
   <template v-else><div v-for="s in group.items" :key="s.uid" class="directory-item-wrap"><div class="directory-row-bar"><button class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{s.remote?label(s.node):s.device}}<span v-if="s.remote && (props.nodes||[]).filter(n=>(n.deviceId||n.id)===(s.node.deviceId||s.node.id)).length>1"> · {{s.node.baseUrl||s.node.url}}</span><template v-if="toolLabel(s)"> · <HarnessIcon :session="s" :label="toolLabel(s)" /></template> · {{date(s)}}<template v-if="s.directoryStale"> · {{t('directoryCached')}}</template></small></button><button type="button" class="directory-pin-btn" :class="{pinned:isPinned(s.uid)}" :aria-label="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" :title="t(isPinned(s.uid)?'unpinSessionLabel':'pinSessionLabel')" @click.stop="togglePin(s.uid)"><svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" :fill="isPinned(s.uid)?'currentColor':'none'" stroke="currentColor" stroke-width="1.8"/></svg></button></div>
    <select v-if="s.remote && linksAvailable" class="project-link-select" :aria-label="t('projectLinkLabel')" :disabled="linking" :value="association(s)?.project_id||''" @change="setProject(s,$event)"><option value="">{{t('projectLinkNone')}}</option><option v-for="p in catalog?.projects||[]" :key="p.id" :value="p.id">{{p.title}}</option></select></div></template>
  </template>
  <details v-if="legacy.length" :open="!!search"><summary>{{t('sidebarLegacy',{count:legacy.length})}}</summary><button v-for="s in legacy" :key="s.uid" class="directory-row" @click="select(s)"><span>{{s.title}}</span><small>{{t('sidebarImported')}} · {{date(s)}}<template v-if="s.directoryStale"> · {{t('directoryCached')}}</template></small></button></details>
 </section>
</template>
<style scoped>
.project-link-select{max-width:100%;width:100%;margin-top:2px;min-height:24px;padding:2px 6px;font-size:11px;color:var(--text-secondary);background:var(--bg-primary)}.directory{min-width:0;max-width:100%}.directory-status{display:flex;justify-content:space-between;align-items:center;gap:8px;flex-wrap:wrap}.directory-filter-bar,.directory-refresh-bar{display:inline-flex;align-items:center;gap:6px}.directory-filter-toggle{display:inline-flex;align-items:center;gap:5px;padding:4px 9px;font-size:12px;color:var(--text-secondary);background:var(--bg-primary)}.directory-filter-toggle.active{color:var(--text-primary);border-color:var(--accent-primary)}.filter-count-badge{display:inline-flex;align-items:center;justify-content:center;min-width:16px;height:16px;padding:0 4px;border-radius:999px;font-size:10px;font-weight:600;color:var(--bg-primary);background:var(--accent-primary)}.directory-filter-reset{padding:4px 8px;font-size:11px;color:var(--text-secondary);background:transparent}.directory-filter-panel{margin-top:8px;padding:10px;border-radius:10px;background:var(--bg-secondary);display:flex;flex-direction:column;gap:8px}.filter-group{display:flex;flex-direction:column;gap:4px}.filter-label{font-size:11px;font-weight:600;color:var(--text-secondary)}.filter-chips{display:flex;flex-wrap:wrap;gap:5px}.filter-chip{padding:3px 8px;min-height:24px;border-radius:999px;font-size:11px;color:var(--text-secondary);background:var(--bg-primary)}.filter-chip.active{color:var(--text-primary);border-color:var(--accent-primary);font-weight:600}.filter-select-grid{display:grid;grid-template-columns:1fr 1fr;gap:6px}.filter-select-field{display:flex;flex-direction:column;gap:3px;min-width:0}.filter-select-field span{font-size:11px;font-weight:600;color:var(--text-secondary)}.filter-select-field select{width:100%;min-height:28px;padding:3px 6px;font-size:11px;color:var(--text-primary);background:var(--bg-primary)}.directory details{margin-top:10px}.directory summary{cursor:pointer;padding:8px 0;color:var(--text-secondary)}.directory-heading{display:inline-flex;align-items:center;gap:6px;max-width:calc(100% - 20px);vertical-align:middle;min-width:0;font-weight:600;color:var(--text-primary)}.directory-heading>span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.directory-heading svg{width:16px;height:16px;flex-shrink:0;color:var(--text-secondary)}.directory-heading small{flex-shrink:0;font-weight:400}.directory-children{padding-inline-start:14px;padding-bottom:8px;min-width:0}.directory details+ .directory-row{margin-top:10px}.directory-item-wrap{margin-top:4px}.directory-row-bar{display:flex;align-items:stretch;gap:4px;min-width:0}.directory-row{display:flex;flex:1;flex-direction:column;align-items:flex-start;min-width:0;gap:3px;padding:8px 10px;text-align:left;background:var(--bg-primary);color:var(--text-primary)}.directory-row span{max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.node-tag-pill{display:inline-block;padding:1px 5px;border-radius:4px;font-size:10px;font-weight:600;background:var(--bg-secondary);color:var(--text-secondary);margin-right:2px}.directory-test-group summary{opacity:0.8}.directory-test-group summary:hover{opacity:1}.directory-pin-btn{flex:0 0 28px;width:28px;min-height:28px;padding:0;display:inline-flex;align-items:center;justify-content:center;color:var(--text-secondary);background:var(--bg-primary)}.directory-pin-btn.pinned{color:var(--accent-primary);border-color:var(--accent-primary)}.directory small{font-size:11px;color:var(--text-secondary);overflow-wrap:anywhere}.directory p{overflow-wrap:anywhere}
</style>
