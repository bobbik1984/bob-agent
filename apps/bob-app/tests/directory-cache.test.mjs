import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import {createReadingCache,readingKey,singleFlight} from '../src/reading-cache.mjs';
const source=readFileSync(new URL('../src/components/ConversationDirectory.vue',import.meta.url),'utf8').split('<script setup>')[1].split('</script>')[0].replace(/^import .*;\r?\n/gm,'');
test('real directory component shows persisted list while refreshing and preserves it on failure',async()=>{
 const cache=createReadingCache({storage:null}),node={id:'node',baseUrl:'https://example.invalid',enabled:true};
 await cache.put(readingKey(node.id,'directory',node.baseUrl),[{id:'old',title:'cached',updated_at:1}]);
 let resolve,reject;const ctx={readingCache:cache,readingKey,singleFlight,defineProps:()=>({nodes:[node],catalog:{},search:'',mode:'project'}),defineEmits:()=>()=>{},useI18n:()=>({t:x=>x}),ref:value=>({value}),watch:()=>{},onUnmounted:()=>{},computed:fn=>({get value(){return fn()}}),label:n=>n.id,directoryGroups:()=>[],kernelBridge:{dshList:()=>new Promise((r,j)=>{resolve=r;reject=j})}};
 vm.createContext(ctx);vm.runInContext(source+';globalThis.actions={refresh,remote,failures,loading};',ctx);
 const pending=ctx.actions.refresh();await new Promise(r=>setImmediate(r));assert.equal(ctx.actions.remote.value[0].title,'cached');assert.equal(ctx.actions.loading.value,true);
 reject(Error('offline'));await pending;assert.equal(ctx.actions.remote.value[0].title,'cached');assert.equal(ctx.actions.failures.value.length,1);
 const update=ctx.actions.refresh();await new Promise(r=>setImmediate(r));resolve([{id:'new',title:'fresh',updated_at:2}]);await update;assert.equal(ctx.actions.remote.value[0].title,'fresh');assert.equal(ctx.actions.failures.value.length,0);
});
