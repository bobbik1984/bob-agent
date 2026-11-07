<script setup>
import {computed} from 'vue';
const props=defineProps({session:{type:Object,required:true},label:{type:String,default:''}});
const icon=computed(()=>{
 const s=props.session;
 const name=String(s.harness||s.toolLabel||s.tool_label||s.sourceKind||s.source_kind||s.id||'').toLowerCase();
 if(name.includes('claude'))return 'claude.png';
 if(name.includes('codex'))return 'codex.png';
 if(name.includes('antigravity')||name.includes('agy'))return 'antigravity.png';
 if(name==='pi'||name.startsWith('pi:')||name.startsWith('pi_'))return 'pi.svg';
 if(name.includes('dsh')||name.includes('deepseek'))return 'deepseek.png';
 return null;
});
</script>
<template><img v-if="icon" class="harness-icon" :src="`/logos/${icon}`" :alt="label" :title="label" width="16" height="16" decoding="async"><span v-else>{{label}}</span></template>
<style scoped>
.harness-icon{display:inline-block;width:16px;height:16px;object-fit:contain;vertical-align:middle;flex-shrink:0;margin-inline:3px;}
</style>
