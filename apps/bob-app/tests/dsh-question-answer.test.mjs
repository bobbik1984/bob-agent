import {createChatScroll} from '../src/chat-scroll.mjs';
import {artifactPath} from '../src/artifact-path.mjs';
import {isNotAcceptedReceipt} from '../src/native-receipt.mjs';
import {unconfirmActivity,settleNativeActivity,orderedNativeActivities,waitingNativeApproval} from '../src/native-activity.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {createPersistentPreviewCache} from '../src/persistent-preview-cache.mjs';
import {createReadingCache} from '../src/reading-cache.mjs';

const source=readFileSync(new URL('../src/components/DshSessions.vue',import.meta.url),'utf8').split('<script setup>')[1].split('</script>')[0].replace(/^import .*;\r?\n/gm,'');

function mount(overrides={}){
 const calls=[];
 const storage=new Map();
 const bridge={
  dshPage:async(c,s)=>({address:{connection_id:c,session_id:s},messages:[],cursor:20,before:0,has_more:false,pending_questions:[]}),
  dshList:async()=>[{id:'s1',running:true}],
  dshAnswer:async(...a)=>{calls.push(a);return {ok:true};},
  ...overrides,
 };
 const context={
  createChatScroll,artifactPath,isNotAcceptedReceipt,unconfirmActivity,settleNativeActivity,orderedNativeActivities,waitingNativeApproval,
  createPersistentPreviewCache,readingCache:createReadingCache({storage:null}),readingKey:(...parts)=>parts.join(':'),
  provide:()=>{},nextTick:async()=>{},defineProps:()=>({nodes:[],target:null}),
  ref:value=>({value}),computed:f=>({get value(){return f()}}),watch:()=>{},onUnmounted:()=>{},
  useI18n:()=>({t:x=>x}),kernelBridge:bridge,
  localStorage:{getItem:k=>storage.get(k)||null,setItem:(k,v)=>storage.set(k,v)},
  crypto:{randomUUID:()=> 'req-1'},setTimeout:()=>0,clearTimeout:()=>{},
 };
 vm.createContext(context);
 vm.runInContext(
  source+'\nglobalThis.actions={open,back,refresh,connection,notice,error,dshQuestions,dshQuestionBlocking,answeredDshCallIds,dshQuestionBusy,dshQuestionErrors,canAnswerDshRequest,isDshOptionSelected,toggleDshOption,setDshCustomAnswer,canSubmitDshQuestion,buildDshAnswersPayload,submitDshQuestion};',
  context,
 );
 context.actions.connection.value='device';
 return {a:context.actions,calls,context};
}

const plain=v=>JSON.parse(JSON.stringify(v));
const dshPage=(cursor,pending_questions)=>({address:{connection_id:'device',session_id:'s1'},messages:[],cursor,before:0,has_more:false,pending_questions});

test('single-select submits clean payload without empty custom and suppresses question optimistically',async()=>{
 const q={
  call_id:'call-single',
  state:'open',
  questions:[{
   id:'deploy_mode',
   header:'部署',
   question:'要如何处理未提交的 WebGL 改动？',
   multi_select:false,
   options:[{label:'暂存 WebGL（推荐）'},{label:'一起提交'}],
  }],
 };
 let pageQuestions=[q];
 const m=mount({dshPage:async(_c,s)=>({address:{connection_id:'device',session_id:s},messages:[],cursor:20,before:0,has_more:false,pending_questions:pageQuestions})});
 await m.a.open({id:'s1'});
 assert.equal(m.a.dshQuestions.value.length,1);
 assert.equal(m.a.dshQuestionBlocking.value,true);
 assert.equal(m.a.canAnswerDshRequest(q),true);
 assert.equal(m.a.canSubmitDshQuestion(q),false);

 m.a.toggleDshOption(q,q.questions[0],'一起提交');
 m.a.toggleDshOption(q,q.questions[0],'暂存 WebGL（推荐）');
 assert.equal(m.a.isDshOptionSelected('call-single','deploy_mode','暂存 WebGL（推荐）'),true);
 assert.equal(m.a.isDshOptionSelected('call-single','deploy_mode','一起提交'),false);
 assert.equal(m.a.canSubmitDshQuestion(q),true);

 const submitPromise=m.a.submitDshQuestion(q);
 assert.equal(m.a.dshQuestionBusy.value['call-single'],true);
 await submitPromise;

 assert.equal(m.calls.length,1);
 assert.deepEqual(plain(m.calls[0]),['device','s1','call-single',[{id:'deploy_mode',selected:['暂存 WebGL（推荐）']}]]);
 assert.equal('custom' in m.calls[0][3][0],false);
 assert.equal(m.a.notice.value,'dshQuestionSubmitted');
 assert.equal(m.a.dshQuestions.value.length,0);
 assert.equal(m.a.dshQuestionBlocking.value,false);

 // Stale poll returning same call_id keeps it suppressed; once cleared upstream, map prunes.
 await m.a.refresh();
 assert.equal(m.a.dshQuestions.value.length,0);
 assert.equal(m.a.answeredDshCallIds.value['call-single'],true);
 pageQuestions=[];
 await m.a.refresh();
 assert.equal(m.a.answeredDshCallIds.value['call-single'],undefined);
});

test('multi-select and multi-question require all questions answered and forward trimmed custom text',async()=>{
 const q={
  call_id:'call-multi',
  state:'continued',
  questions:[
   {id:'targets',question:'选择部署目标',multi_select:true,options:[{label:'Hostinger'},{label:'Contabo'},{label:'X1T'}]},
   {id:'notes',question:'补充备注',multi_select:false,options:[]},
  ],
 };
 const m=mount({dshPage:async()=>dshPage(30,[q])});
 await m.a.open({id:'s1'});
 m.a.toggleDshOption(q,q.questions[0],'Hostinger');
 m.a.toggleDshOption(q,q.questions[0],'Contabo');
 m.a.toggleDshOption(q,q.questions[0],'X1T');
 m.a.toggleDshOption(q,q.questions[0],'Contabo'); // unselect Contabo
 assert.equal(m.a.canSubmitDshQuestion(q),false); // second question still empty

 m.a.setDshCustomAnswer(q,q.questions[1],'  先部署云端节点  ');
 assert.equal(m.a.canSubmitDshQuestion(q),true);
 await m.a.submitDshQuestion(q);

 assert.deepEqual(plain(m.calls[0]),['device','s1','call-multi',[
  {id:'targets',selected:['Hostinger','X1T']},
  {id:'notes',selected:[],custom:'先部署云端节点'},
 ]]);
});

test('custom-only answer works after untoggling a single-select option',async()=>{
 const q={
  call_id:'call-custom',
  state:'open',
  questions:[{id:'choice',question:'选择策略',multi_select:false,options:[{label:'A'},{label:'B'}]}],
 };
 const m=mount({dshPage:async()=>dshPage(10,[q])});
 await m.a.open({id:'s1'});
 m.a.toggleDshOption(q,q.questions[0],'A');
 m.a.toggleDshOption(q,q.questions[0],'A'); // untoggle -> selected: []
 m.a.setDshCustomAnswer(q,q.questions[0],'自定义处理');
 assert.equal(m.a.canSubmitDshQuestion(q),true);
 await m.a.submitDshQuestion(q);
 assert.deepEqual(plain(m.calls[0]),['device','s1','call-custom',[{id:'choice',selected:[],custom:'自定义处理'}]]);
});

test('DSH_QUESTION_EXPIRED shows localized notice, keeps question until refresh clears it',async()=>{
 const q={
  call_id:'call-exp',
  state:'open',
  questions:[{id:'q1',question:'确认继续？',multi_select:false,options:[{label:'确认'}]}],
 };
 let pending=[q];
 let refreshCount=0;
 const m=mount({
  dshPage:async()=>{refreshCount++;return dshPage(40+refreshCount,pending);},
  dshAnswer:async()=>{throw Error('DSH_QUESTION_EXPIRED');},
 });
 await m.a.open({id:'s1'});
 m.a.toggleDshOption(q,q.questions[0],'确认');
 await m.a.submitDshQuestion(q);
 assert.equal(m.a.dshQuestionErrors.value['call-exp'],'dshQuestionExpired');
 assert.equal(m.a.answeredDshCallIds.value['call-exp'],undefined);

 pending=[];
 await m.a.refresh();
 assert.equal(m.a.dshQuestions.value.length,0);
});

test('questions missing item.id fall back to non-answerable read-only state cleanly',async()=>{
 const legacy={call_id:'legacy',state:'open',questions:[{question:'Legacy question',options:[{label:'A'}]}]};
 const m=mount({dshPage:async()=>dshPage(5,[legacy])});
 await m.a.open({id:'s1'});
 assert.equal(m.a.dshQuestions.value.length,1);
 assert.equal(m.a.canAnswerDshRequest(legacy),false);
});
