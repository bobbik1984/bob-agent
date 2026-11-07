<script setup>
import {ref,onMounted,onUnmounted,provide,inject} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from '../native-bridge.mjs';
import MessageContent from './MessageContent.vue';
import {artifactHtmlDocument,artifactHtmlSandbox} from '../artifact-html.mjs';
const props=defineProps({address:Object});const emit=defineEmits(['close']);const {t}=useI18n();
provide('dshOpenFile',null);
const getFile=inject('dshPreviewGet',a=>kernelBridge.dshPreview(a.connectionId,a.sessionId,a.path));
const zoomed=ref(false);
const dialog=ref(null),busy=ref(true),error=ref(''),file=ref(null),url=ref(''),text=ref('');
let alive=true,pushedStates=0,ignoringPop=false,closingFromPop=false;
const origin=globalThis.document?.activeElement||null;
const documentTypes={pdf:'application/pdf',docx:'application/vnd.openxmlformats-officedocument.wordprocessingml.document',xlsx:'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',pptx:'application/vnd.openxmlformats-officedocument.presentationml.presentation',doc:'application/msword',xls:'application/vnd.ms-excel',ppt:'application/vnd.ms-powerpoint',odt:'application/vnd.oasis.opendocument.text',ods:'application/vnd.oasis.opendocument.spreadsheet',odp:'application/vnd.oasis.opendocument.presentation'};
const imageTypes={png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',webp:'image/webp'};
function pushPreviewState(layer){
 try{if(globalThis.history?.pushState){globalThis.history.pushState({bobPreview:layer,path:props.address?.path||''},'');pushedStates++;}}catch{}
}
function toggleZoom(){
 if(!zoomed.value){zoomed.value=true;pushPreviewState('zoom');}
 else{handleBack();}
}
function handleBack(){
 if(zoomed.value){
  zoomed.value=false;
  if(pushedStates>1&&globalThis.history?.back){pushedStates--;ignoringPop=true;try{globalThis.history.back();}catch{ignoringPop=false;}}
  return;
 }
 closeAll();
}
function closeAll(){
 zoomed.value=false;
 if(!closingFromPop&&pushedStates>0&&globalThis.history?.go){
  const count=pushedStates;pushedStates=0;ignoringPop=true;
  try{globalThis.history.go(-count);}catch{ignoringPop=false;}
 }
 emit('close');
}
function onPopState(){
 if(ignoringPop){ignoringPop=false;return;}
 if(pushedStates>0)pushedStates--;
 if(zoomed.value){zoomed.value=false;return;}
 closingFromPop=true;emit('close');
}
function onBackdropClick(e){
 if(e.target===dialog.value){
  const rect=dialog.value.getBoundingClientRect();
  const inside=rect.top<=e.clientY&&e.clientY<=rect.top+rect.height&&rect.left<=e.clientX&&e.clientX<=rect.left+rect.width;
  if(!inside)closeAll();
 }
}
onMounted(async()=>{
 pushPreviewState('file');
 globalThis.window?.addEventListener?.('popstate',onPopState);
 dialog.value?.showModal?.();
 try{
  const a=props.address;
  const applyFile=f=>{
   if(!alive)return;
  if(f.path!==a.path||f.session_id!==a.sessionId||f.connection_id!==a.connectionId)throw Error('DSH_IDENTITY_MISMATCH');
  const bytes=Uint8Array.from(atob(f.data),c=>c.charCodeAt(0));
  if(bytes.length!==f.bytes)throw Error('DSH_FILE_CHANGED');
  if(url.value)URL.revokeObjectURL(url.value);
  file.value=f;
  url.value=URL.createObjectURL(new Blob([bytes],{type:imageTypes[f.extension]||documentTypes[f.extension]||'text/plain;charset=utf-8'}));
  if(!imageTypes[f.extension]&&!documentTypes[f.extension])text.value=new TextDecoder('utf-8',{fatal:true}).decode(bytes);
  };
  const f=await getFile(a,updated=>{try{applyFile(updated);}catch{/* Keep the previously verified preview on refresh failure. */}});
  applyFile(f);
 }catch(e){
  error.value=String(e.message||e).match(/DSH_[A-Z_]+/)?.[0]||'DSH_PREVIEW_UNAVAILABLE';
 }finally{
  busy.value=false;
 }
});
onUnmounted(()=>{
 alive=false;
 globalThis.window?.removeEventListener?.('popstate',onPopState);
 if(url.value)URL.revokeObjectURL(url.value);
 origin?.focus?.({preventScroll:true});
});
</script>
<template>
<dialog ref="dialog" class="dsh-preview" @cancel.prevent="handleBack" @click="onBackdropClick">
 <header class="dsh-preview-header">
  <button type="button" class="dsh-preview-back" @click="handleBack">← {{ t(zoomed ? 'filePreviewExitZoom' : 'filePreviewBack') }}</button>
  <strong class="dsh-preview-title">{{ address.path }}</strong>
  <button type="button" class="dsh-preview-close" @click="closeAll">{{ t('filePreviewClose') }}</button>
 </header>
 <p>{{ t(file?.from_cache?'filePreviewCached':'filePreviewCurrent') }}</p>
 <p v-if="file?.resources_unavailable?.length" role="status">{{ t('filePreviewResourcesPartial', {count:file.resources_unavailable.length}) }}</p>
 <p v-if="busy" role="status">{{ t('filePreviewLoading') }}</p>
 <p v-if="error" role="alert">{{ t('filePreviewFailed') }} · {{ error }}</p>
 <template v-if="file && !error"><button v-if="imageTypes[file.extension]" type="button" @click="toggleZoom">{{ t(zoomed ? 'filePreviewFit' : 'filePreviewActual') }}</button><div class="dsh-preview-body" :class="{zoomed}"><img v-if="imageTypes[file.extension]" :src="url" :alt="address.path"/><section v-else-if="file.document" class="document-pages"><figure v-for="page in file.document.pages" :key="page.number"><figcaption>{{ t('filePreviewPage', {page:page.number,total:file.document.page_count}) }}</figcaption><img :src="'data:image/jpeg;base64,'+page.data" :alt="t('filePreviewPage', {page:page.number,total:file.document.page_count})" loading="lazy"/></figure></section><MessageContent v-else-if="file.extension==='md'" :content="text"/><iframe v-else-if="file.extension==='html'" class="dsh-html-preview" :title="address.path" :sandbox="artifactHtmlSandbox" referrerpolicy="no-referrer" :srcdoc="artifactHtmlDocument(text, file.resources || [])"/><pre v-else>{{ text }}</pre></div><footer><a :href="url" :download="address.path.split('/').pop()">{{ t('filePreviewDownload') }}</a><small :title="file.version">{{ Math.ceil(file.bytes / 1024) }} KB</small></footer></template>
</dialog>
</template>
<style scoped>
.document-pages figure{margin:0 0 16px}.document-pages figcaption{color:var(--text-secondary);padding:8px 0}.document-pages img{display:block;width:100%;height:auto}
.dsh-html-preview{display:block;width:100%;height:65dvh;border:1px solid var(--border-color);background:var(--bg-primary)}
.dsh-preview::backdrop{background:color-mix(in srgb,var(--bg-root,#0b0f17) 78%,transparent);backdrop-filter:blur(16px);-webkit-backdrop-filter:blur(16px)}
.dsh-preview{width:min(1100px,94vw);height:90dvh;max-height:94dvh;box-sizing:border-box;padding:max(16px,calc(env(safe-area-inset-top,0px) + 12px)) 16px max(16px,calc(env(safe-area-inset-bottom,0px) + 12px));border:1px solid var(--border-color);border-radius:12px;background:var(--bg-primary);color:var(--text-primary);box-shadow:0 24px 64px rgba(0,0,0,0.65), 0 0 0 1px var(--border-color);overflow:auto;scrollbar-gutter:stable}
footer a{color:var(--accent-primary);white-space:nowrap}.dsh-preview-body{overflow:auto;margin-top:12px}.dsh-preview-body.zoomed{max-height:70dvh}.dsh-preview-body.zoomed img{max-width:none}header,footer{display:flex;align-items:center;justify-content:space-between;gap:12px}header{position:sticky;top:0;z-index:1;background:var(--bg-primary);padding-block:8px}header strong{min-width:0;flex:1;text-align:center;overflow-wrap:anywhere}header button{flex-shrink:0;padding:6px 12px;border-radius:8px;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--text-primary);cursor:pointer}p,small{color:var(--text-secondary)}small{overflow-wrap:anywhere}img{max-width:100%;height:auto}pre{white-space:pre-wrap;overflow-wrap:anywhere}footer{margin-top:16px}
@media(max-width:600px){.dsh-preview{width:100%;height:100dvh;max-width:100%;max-height:100dvh;margin:0;border-radius:0}}
</style>
