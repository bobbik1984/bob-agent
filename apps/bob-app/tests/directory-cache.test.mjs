import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import {deviceGroups} from '../src/device-paths.mjs';
import {createReadingCache,readingKey,singleFlight} from '../src/reading-cache.mjs';
const source=readFileSync(new URL('../src/components/ConversationDirectory.vue',import.meta.url),'utf8').split('<script setup>')[1].split('</script>')[0].replace(/^import .*;\r?\n/gm,'');
test('real directory component shows persisted list while refreshing and preserves it on failure',async()=>{
 const cache=createReadingCache({storage:null}),node={id:'node',baseUrl:'https://example.invalid',enabled:true};
 await cache.put(readingKey(node.id,'directory',node.baseUrl),[{id:'old',title:'cached',updated_at:1}]);
 let resolve,reject,resolveLinks;let firstLinks=true;const ctx={setInterval:()=>0,clearInterval:()=>{},deviceGroups,pathState:()=>'unverified',checkPath:(n,bridge)=>bridge.dshList(n.id),readingCache:cache,readingKey,singleFlight,defineProps:()=>({nodes:[node],catalog:{},search:'',mode:'project'}),defineEmits:()=>()=>{},useI18n:()=>({t:x=>x}),ref:value=>({value}),watch:()=>{},onUnmounted:()=>{},computed:fn=>({get value(){return fn()}}),label:n=>n.id,directoryGroups:()=>[],kernelBridge:{projectLinks:()=>{if(!firstLinks)return Promise.resolve({links:[]});firstLinks=false;return new Promise(r=>{resolveLinks=r})},dshList:()=>new Promise((r,j)=>{resolve=r;reject=j})}};
 vm.createContext(ctx);vm.runInContext(source+';globalThis.actions={refresh,remote,failures,loading};',ctx);
 const pending=ctx.actions.refresh();await new Promise(r=>setImmediate(r));assert.equal(ctx.actions.remote.value.length,0);
 resolveLinks({links:[]});await new Promise(r=>setImmediate(r));
 assert.equal(ctx.actions.remote.value[0].title,'cached');assert.equal(ctx.actions.loading.value,true);
 reject(Error('offline'));await pending;assert.equal(ctx.actions.remote.value[0].title,'cached');assert.equal(ctx.actions.failures.value.length,1);assert.equal(ctx.actions.failures.value[0].url,node.baseUrl);assert.equal(ctx.actions.failures.value[0].cached,true);assert.equal(ctx.actions.remote.value[0].directoryStale,true);
 const update=ctx.actions.refresh();await new Promise(r=>setImmediate(r));resolve([{id:'new',title:'fresh',updated_at:2}]);await update;assert.equal(ctx.actions.remote.value[0].title,'fresh');assert.equal(ctx.actions.failures.value.length,0);
 const groupedInput=ctx.actions.remote.value;
 const unchanged=ctx.actions.refresh();await new Promise(r=>setImmediate(r));
 resolve([{id:'new',title:'fresh',updated_at:2}]);await unchanged;
 assert.equal(ctx.actions.remote.value,groupedInput);
});

test('progressive directory refresh renders fast node immediately without waiting for slow or offline nodes',async()=>{
 const cache=createReadingCache({storage:null});
 const fastNode={id:'fast',baseUrl:'https://fast.example.invalid',enabled:true};
 const slowNode={id:'slow',baseUrl:'https://slow.example.invalid',enabled:true};
 const resolvers=new Map(), rejectors=new Map(), readable=new Set();
 const ctx={
  setInterval:()=>0,clearInterval:()=>{},deviceGroups,
  pathState:n=>readable.has(n.id)?'readable':'unverified',
  checkPath:async(n,bridge)=>{const rows=await bridge.dshList(n.id);readable.add(n.id);return rows;},
  readingCache:cache,readingKey,singleFlight,
  defineProps:()=>({nodes:[fastNode,slowNode],catalog:{},search:'',mode:'device'}),
  defineEmits:()=>()=>{},useI18n:()=>({t:x=>x}),
  ref:value=>({value}),watch:()=>{},onUnmounted:()=>{},
  computed:fn=>({get value(){return fn()}}),
  label:n=>n.id,directoryGroups:()=>[],
  kernelBridge:{
   projectLinks:()=>Promise.resolve({links:[]}),
   dshList:id=>new Promise((r,j)=>{resolvers.set(id,r);rejectors.set(id,j);}),
  },
 };
 vm.createContext(ctx);
 vm.runInContext(source+';globalThis.actions={refresh,remote,failures,loading,settledCount,totalCount};',ctx);
 const pending=ctx.actions.refresh();
 await new Promise(r=>setImmediate(r));
 assert.equal(ctx.actions.loading.value,true);
 assert.equal(ctx.actions.totalCount.value,2);
 assert.equal(ctx.actions.settledCount.value,0);
 assert.equal(ctx.actions.remote.value.length,0);

 // Fast node connects first: its sessions render immediately while slow node is still pending.
 resolvers.get('fast')([{id:'f1',title:'fast session',updated_at:10}]);
 await new Promise(r=>setImmediate(r));
 await new Promise(r=>setImmediate(r));
 assert.equal(ctx.actions.loading.value,true);
 assert.equal(ctx.actions.settledCount.value,1);
 assert.equal(ctx.actions.remote.value.length,1);
 assert.equal(ctx.actions.remote.value[0].title,'fast session');
 assert.equal(ctx.actions.remote.value[0].directoryStale,false);
 assert.equal(ctx.actions.failures.value.length,0);

 // Slow node later times out: fast node remains rendered and only slow node reports failure.
 rejectors.get('slow')(Error('timeout'));
 await pending;
 assert.equal(ctx.actions.loading.value,false);
 assert.equal(ctx.actions.settledCount.value,2);
 assert.equal(ctx.actions.remote.value.length,1);
 assert.equal(ctx.actions.remote.value[0].title,'fast session');
 assert.equal(ctx.actions.failures.value.length,1);
 assert.equal(ctx.actions.failures.value[0].id,'slow');
});

test('status indicator does not get stuck yellow while checking or viewing a synced remote conversation',()=>{
 const appVue=readFileSync(new URL('../src/App.vue',import.meta.url),'utf8');
 const chatSession=readFileSync(new URL('../src/composables/useChatSession.mjs',import.meta.url),'utf8');
 assert.match(appVue,/unconfirmed:\s*deviceSyncPhase === 'offline' \|\| \(!remoteConversation && \(homeFault \|\| \(devicePaired && !deviceOnline && deviceSyncPhase !== 'checking'\)\)\)/);
 assert.match(chatSession,/const homeSyncPending = ref\(false\);/);
 assert.match(chatSession,/status === 'sync_checked' \|\| isReachablePayload/);
 assert.match(chatSession,/Promise\.all\(\[\s*kernelBridge\.catalog\(\),\s*kernelBridge\.assistantConversation\(\)/);
});
