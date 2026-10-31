<script setup>
import {ref,inject,onMounted,onUnmounted} from 'vue';
import {useI18n} from 'vue-i18n';
const props=defineProps({path:String,label:String});const {t}=useI18n();
const service=inject('dshArtifacts',null),open=inject('dshOpenFile');
const element=ref(null),state=ref('idle'),url=ref('');let observer,alive=true,visible=false;
const image=/\.(png|jpe?g|webp)(?::\d+(?::\d+)?)?$/i.test(props.path);
const supported=/\.(png|jpe?g|webp|md|txt|json|yaml|yml|csv)(?::\d+(?::\d+)?)?$/i.test(props.path);
onMounted(()=>{if(!service||!supported)return;observer=new IntersectionObserver(entries=>{visible=entries[0].isIntersecting;if(visible&&state.value==='idle')service.enqueue({element:element.value,visible:()=>alive&&visible,run:async()=>{
 if(!alive||!visible)return;state.value='loading';try{const f=await service.get(props.path);if(!alive)return;
 if(image){const bytes=Uint8Array.from(atob(f.data),c=>c.charCodeAt(0));url.value=URL.createObjectURL(new Blob([bytes],{type:`image/${f.extension==='jpg'?'jpeg':f.extension}`}));}state.value='ready';
 }catch{if(alive)state.value='failed';}
 }});},{root:element.value.closest('.dsh-reader'),rootMargin:'120px'});observer.observe(element.value);});
onUnmounted(()=>{alive=false;observer?.disconnect();if(url.value)URL.revokeObjectURL(url.value)});
</script>
<template><button ref="element" type="button" class="artifact-link" :class="{image,loading:state==='loading'}" @click="open(path)"><img v-if="url" :src="url" :alt="label"/><span>{{ label }}</span><small v-if="supported">{{ t(state==='loading'?'artifactPreparing':state==='ready'?'artifactReady':state==='failed'?'artifactRetry':'artifactOpen') }}</small></button></template>
<style scoped>
.artifact-link{display:inline-flex;align-items:center;gap:8px;max-width:100%;text-align:left;background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:10px;padding:10px;color:var(--text-primary);overflow-wrap:anywhere;position:relative}.artifact-link.image{display:flex;flex-direction:column;align-items:flex-start;width:min(100%,420px);min-height:80px}.artifact-link img{width:100%;max-height:260px;object-fit:contain;border-radius:6px}.artifact-link small{color:var(--text-secondary);font-size:.75rem}.artifact-link.loading{background:var(--bg-tertiary)}
</style>
