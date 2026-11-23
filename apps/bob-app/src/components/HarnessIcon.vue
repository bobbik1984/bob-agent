<script setup>
import {computed} from 'vue';
const props=defineProps({session:{type:Object,default:()=>({})},label:{type:String,default:''}});
const icon=computed(()=>{
 const s=props.session||{};
 const name=String(s.harness||s.toolLabel||s.tool_label||s.sourceKind||s.source_kind||s.id||'').toLowerCase();
 if(name.includes('claude'))return 'claude.png';
 if(name.includes('codex'))return 'codex.png';
 if(name.includes('antigravity')||name.includes('agy'))return 'antigravity.png';
 if(name.includes('copilot'))return 'Copilot.png';
 if(name.includes('kimi'))return 'kimi.png';
 if(name.includes('workbuddy'))return 'workbuddy.png';
 if(name.includes('doubao')||name.includes('arkcli'))return 'doubao.png';
 if(name.includes('hermes'))return 'hermes%20agent.png';
 if(name.includes('openclaw'))return 'openclaw.png';
 if(name.includes('qoder')||name.includes('zcode')||name.includes('goose'))return '';
 if(name==='pi'||name.startsWith('pi:')||name.startsWith('pi_'))return 'pi.svg';
 return 'deepseek.png';
});
</script>
<template><img v-if="icon" class="harness-icon" :src="`/logos/${icon}`" :alt="label||'harness'" :title="label||'harness'" width="16" height="16" decoding="async"><span v-else>{{label}}</span></template>
<style scoped>
.harness-icon{display:inline-block;width:16px;height:16px;object-fit:contain;vertical-align:middle;flex-shrink:0;margin-inline:3px;}
</style>
