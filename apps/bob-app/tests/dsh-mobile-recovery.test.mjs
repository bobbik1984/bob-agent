import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {createPreviewCache} from '../src/dsh-preview-cache.mjs';
// Exercise the shipped component's state/actions; no duplicate send controller.
const source=readFileSync(new URL('../src/components/DshSessions.vue',import.meta.url),'utf8').split('<script setup>')[1].split('</script>')[0].replace(/^import .*;\r?\n/gm,'');
function mount(storage=new Map(), overrides={}) {
 const calls=[];
 const bridge={dshPage:async(c,s)=>({address:{connection_id:c,session_id:s},messages:[],cursor:20,before:0,has_more:false}),dshPrompt:async(...a)=>{calls.push(a);throw Error('DSH_RECEIPT_UNKNOWN')},dshReceipt:async()=>({status:'unknown'}),...overrides};
 const context={createPreviewCache,provide:()=>{},nextTick:async()=>{},defineProps:()=>({nodes:[],target:null}),ref:value=>({value}),computed:f=>({get value(){return f()}}),watch:()=>{},onUnmounted:()=>{},useI18n:()=>({t:x=>x}),kernelBridge:bridge,localStorage:{getItem:k=>storage.get(k)||null,setItem:(k,v)=>storage.set(k,v)},crypto:{randomUUID:()=> 'request-1'},setTimeout:()=>0,clearTimeout:()=>{}};
 vm.createContext(context);vm.runInContext(source+'\nglobalThis.actions={open,send,checkReceipt,connection,draft,pending,notice,error,reader,refresh,loadOlder,history,loadingOlder};',context);
 context.actions.connection.value='device';return {a:context.actions,calls,context,storage};
}
test('mobile session: lost reply survives reopening, cannot resend, exact accepted receipt recovers',async()=>{
 const one=mount();await one.a.open({id:'original'});one.a.draft.value='progress only';await one.a.send();assert.equal(one.calls.length,1);assert.ok(one.a.pending.value);
 const two=mount(one.storage,{dshReceipt:async()=>({status:'accepted',session_id:'original',request_id:'request-1'})});await two.a.open({id:'original'});assert.equal(two.a.draft.value,'progress only');await two.a.send();assert.equal(two.calls.length,0);await two.a.checkReceipt();assert.equal(two.a.pending.value,null);assert.equal(two.a.draft.value,'');assert.equal(two.calls.length,0);
});
test('unknown or mismatched receipt never unlocks duplicate send',async()=>{
 for(const receipt of [{status:'unknown'},{status:'accepted',session_id:'other',request_id:'request-1'},{status:'accepted',session_id:'original',request_id:'other'}]){
 const m=mount(new Map(),{dshReceipt:async()=>receipt});await m.a.open({id:'original'});m.a.draft.value='progress';await m.a.send();await m.a.checkReceipt();await m.a.send();assert.ok(m.a.pending.value);assert.equal(m.calls.length,1);
 }
});
test('storage failure preserves draft and prevents network send',async()=>{
 const m=mount();await m.a.open({id:'original'});m.context.localStorage.setItem=()=>{throw Error('quota')};m.a.draft.value='keep me';await m.a.send();assert.equal(m.calls.length,0);assert.equal(m.a.draft.value,'keep me');assert.equal(m.a.error.value,'dshStorageError');
});
test('pending recovery is isolated from another session and retains edited draft',async()=>{
 const m=mount(new Map(),{dshReceipt:async()=>({status:'accepted',session_id:'original',request_id:'request-1'})});await m.a.open({id:'original'});m.a.draft.value='original text';await m.a.send();await m.a.open({id:'second'});assert.equal(m.a.pending.value,null);assert.equal(m.a.draft.value,'');await m.a.open({id:'original'});m.a.draft.value='next draft';await m.a.checkReceipt();assert.equal(m.a.draft.value,'next draft');assert.equal(m.calls.length,1);
});

test('send must retain pending when a response belongs to another session',async()=>{
 const m=mount(new Map(),{dshPrompt:async()=>({status:'accepted',session_id:'wrong',request_id:'request-1'})});await m.a.open({id:'original'});m.a.draft.value='preserve';await m.a.send();assert.ok(m.a.pending.value);assert.equal(m.a.draft.value,'preserve');
});

test('first open scrolls to newest; polling does not pull a reader away from older messages',async()=>{
 const m=mount();const el={scrollHeight:2000,scrollTop:0,clientHeight:600};m.a.reader.value=el;await m.a.open({id:'original'});assert.equal(el.scrollTop,2000);el.scrollTop=150;await m.a.refresh();assert.equal(el.scrollTop,150);el.scrollTop=1380;await m.a.refresh();assert.equal(el.scrollTop,2000);
});

test('older page prepends ten messages without changing viewport anchor or draft',async()=>{
 let n=0;const page=(c,s,start)=>({address:{connection_id:c,session_id:s},cursor:30,before:start,has_more:start>0,messages:Array.from({length:10},(_,i)=>({sequence:start+i,role:'assistant',text:'message '+(start+i)}))});
 const m=mount(new Map(),{dshPage:async(c,s,b)=>{n++;return page(c,s,b===null||b===undefined?20:10)}});
 const el={scrollHeight:2000,scrollTop:0,clientHeight:600};m.a.reader.value=el;await m.a.open({id:'original'});m.a.draft.value='draft';el.scrollTop=0;
 m.context.nextTick=async()=>{el.scrollHeight=4000};await m.a.loadOlder();assert.equal(el.scrollTop,2000);assert.equal(m.a.history.value.messages.length,20);assert.equal(m.a.history.value.messages[0].sequence,10);assert.equal(m.a.draft.value,'draft');assert.equal(n,2);
});
test('older page identity failure preserves history and permits explicit retry',async()=>{
 const m=mount();await m.a.open({id:'original'});m.a.history.value.has_more=true;m.a.history.value.before=10;m.context.kernelBridge.dshPage=async()=>({address:{connection_id:'wrong',session_id:'original'},messages:[],cursor:20,before:0});await m.a.loadOlder();assert.equal(m.a.history.value.before,10);assert.equal(m.a.loadingOlder.value,false);assert.match(m.a.error.value,/IDENTITY/);
});
test('late older page cannot contaminate a switched session',async()=>{
 const m=mount();await m.a.open({id:'original'});m.a.history.value.has_more=true;m.a.history.value.before=10;let release;
 m.context.kernelBridge.dshPage=(c,s,b)=>b?new Promise(resolve=>release=()=>resolve({address:{connection_id:c,session_id:s},messages:[{sequence:1,text:'old'}],cursor:20,before:0,has_more:false})):Promise.resolve({address:{connection_id:c,session_id:s},messages:[],cursor:20,before:0,has_more:false});
 const pending=m.a.loadOlder();await m.a.open({id:'second'});release();await pending;assert.equal(m.a.history.value.address.session_id,'second');assert.equal(m.a.history.value.messages.length,0);
});

test('unchanged native cursor keeps loaded history and reader position',async()=>{
 const m=mount();await m.a.open({id:'original'});m.a.history.value.messages=[{sequence:3,text:'kept'}];m.a.reader.value={scrollTop:120,scrollHeight:2000,clientHeight:500};
 m.context.kernelBridge.dshPage=async(c,s,b,t,after)=>{assert.equal(after,20);return {address:{connection_id:c,session_id:s},cursor:20,before:0,messages:[],not_modified:true}};
 await m.a.refresh();assert.equal(m.a.history.value.messages[0].text,'kept');assert.equal(m.a.reader.value.scrollTop,120);
});
test('burst catchup fills the gap before advancing the cursor',async()=>{
 const m=mount();await m.a.open({id:'original'});m.a.history.value.messages=[{sequence:20,text:'old'}];let calls=0;
 m.context.kernelBridge.dshPage=async(c,s,b)=>{calls++;return {address:{connection_id:c,session_id:s},cursor:50,before:b?20:40,has_more:true,messages:[{sequence:b?30:50,text:'new'}]}};
 await m.a.refresh();assert.equal(calls,2);assert.deepEqual(Array.from(m.a.history.value.messages,m=>m.sequence),[20,30,50]);assert.equal(m.a.history.value.cursor,50);
});
test('surface replacement stays applied when older records are prepended',async()=>{
 const m=mount();await m.a.open({id:'original'});m.a.history.value={...m.a.history.value,before:10,has_more:true,replacements:[{op:'replace',startSeq:2,endSeq:4}],messages:[{sequence:12,text:'replacement'}]};
 m.context.kernelBridge.dshPage=async(c,s)=>({address:{connection_id:c,session_id:s},cursor:20,before:0,has_more:false,messages:[{sequence:3,text:'removed'},{sequence:1,text:'retain'}]});await m.a.loadOlder();assert.deepEqual(Array.from(m.a.history.value.messages,m=>m.sequence),[1,12]);
});
