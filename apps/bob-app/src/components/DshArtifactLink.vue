<script setup>
import {imageThumbnail} from '../persistent-preview-cache.mjs';
import {ref,inject,onMounted,onUnmounted,nextTick} from 'vue';
import {captureReaderAnchor,restoreReaderAnchor} from '../reader-anchor.mjs';
import {useI18n} from 'vue-i18n';
const props=defineProps({path:String,label:String,messageSequence:{type:Number,default:null}});const {t}=useI18n();
const service=inject('dshArtifacts',null),open=inject('dshOpenFile');
const element=ref(null),state=ref('idle'),url=ref('');let observer,alive=true,visible=false;
let anchor=null;
async function imageReady(){await nextTick();restoreReaderAnchor(anchor);anchor=null}
const image=/\.(png|jpe?g|webp)(?::\d+(?::\d+)?)?$/i.test(props.path);
const supported=/\.(png|jpe?g|webp|svg|pdf|docx?|xlsx?|pptx?|html?|md|txt|json|ya?ml|csv|log|py|js|ts|rs|c|cpp|h|sh)(?::\d+(?::\d+)?)?$/i.test(props.path);
onMounted(()=>{if(!service||!supported)return;observer=new IntersectionObserver(entries=>{visible=entries[0].isIntersecting;if(visible&&state.value==='idle')service.enqueue({element:element.value,visible:()=>alive&&visible,run:async()=>{
 if(!alive||!visible)return;state.value='loading';try{if(image&&service.peek){const saved=await service.peek(props.path,props.messageSequence);if(saved&&alive){anchor=captureReaderAnchor(element.value?.closest('.dsh-reader'));url.value=await imageThumbnail(saved);}}const f=await service.get(props.path,props.messageSequence);if(!alive)return;
 if(image){anchor=captureReaderAnchor(element.value?.closest('.dsh-reader'));url.value=await imageThumbnail(f);}state.value='ready';
 }catch{if(alive)state.value='failed';}
 }});},{root:element.value.closest('.dsh-reader'),rootMargin:'120px'});observer.observe(element.value);});
onUnmounted(()=>{alive=false;observer?.disconnect();if(url.value.startsWith('blob:'))URL.revokeObjectURL(url.value)});
</script>
<template><button ref="element" type="button" class="artifact-link" :class="{image,loading:state==='loading'}" @click="open(path,messageSequence)"><img v-if="url" :src="url" :alt="label" @load="imageReady"/><span class="artifact-meta"><span class="artifact-label">{{ label }}</span><small v-if="supported">{{ t(state==='loading'?'artifactPreparing':state==='ready'?'artifactReady':state==='failed'?'artifactRetry':'artifactOpen') }}</small></span></button></template>
<style scoped>
.artifact-link{display:inline-flex;align-items:center;gap:8px;max-width:100%;text-align:left;background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--radius-sm);padding:3px 8px;color:var(--accent-primary);overflow-wrap:anywhere;position:relative;cursor:pointer;transition:border-color .15s ease}
.artifact-link:hover{border-color:var(--accent-primary)}
.artifact-meta{display:flex;flex-direction:column;gap:2px;min-width:0}
.artifact-label{color:var(--text-primary);font-size:var(--text-xs);font-weight:500;overflow-wrap:anywhere}
.artifact-link:not(.image) .artifact-meta{flex-direction:row;align-items:center;gap:6px}
.artifact-link:not(.image) .artifact-label{color:var(--accent-primary)}
.artifact-link.image{display:flex;flex-direction:column;align-items:stretch;width:min(100%,420px);padding:0;gap:0;border-radius:var(--radius-card);overflow:hidden;border:1px solid var(--border-color);background:var(--bg-secondary)}
.artifact-link.image .artifact-meta{padding:8px 12px}
.artifact-link img{display:block;width:100%;max-height:280px;object-fit:cover;border-radius:0}
.artifact-link small{color:var(--text-secondary);font-size:var(--text-2xs)}
.artifact-link.loading{background:var(--bg-tertiary)}
</style>
