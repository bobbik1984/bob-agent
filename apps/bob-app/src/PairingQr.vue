<script setup>
import { ref, watch, onUnmounted, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import QRCode from 'qrcode';
const props = defineProps({ code:String, expiresAt:Number });
const emit = defineEmits(['expired','error']);
const { t }=useI18n(); const canvas=ref(null), seconds=ref(0);
let timer=null, generation=0;
function tick() { seconds.value=Math.max(0,Math.ceil((props.expiresAt-Date.now())/1000)); if(!seconds.value) { if(timer)clearInterval(timer);timer=null;emit('expired'); } }
watch(()=>[props.code,props.expiresAt],async()=>{
 const current=++generation;if(timer)clearInterval(timer);timer=null;
 if(!props.code)return;tick();if(!seconds.value)return;
 await nextTick();if(current!==generation||!canvas.value)return;
 try { // QR's default dark/light pixels are machine-readable encoding, independent of theme.
  await QRCode.toCanvas(canvas.value,props.code,{width:420,margin:4,errorCorrectionLevel:'M'});
 }catch{emit('error');return;}
 timer=setInterval(tick,1000);
},{immediate:true});
onUnmounted(()=>{generation++;if(timer)clearInterval(timer);});
</script>
<template><figure v-if="code && seconds" class="pair-qr"><canvas ref="canvas" role="img" :aria-label="t('qrScanImage')"/><figcaption>{{t('qrExpires',{seconds})}}</figcaption></figure></template>
<style scoped>.pair-qr{margin:0;display:grid;justify-items:center;gap:.6rem}.pair-qr canvas{display:block;width:min(420px,calc(100dvh - 240px));max-width:100%;height:auto;border-radius:.6rem}.pair-qr figcaption{font-size:.8rem;color:var(--text-secondary)}</style>
