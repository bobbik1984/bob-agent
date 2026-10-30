import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { createSendController } from '../src/send-controller.mjs';
import { createLayoutState, getProviderLogo } from '../src/layout-mode.mjs';
import { createOnlineWorkClient } from '../src/online-client.mjs';

test('IPC maps stable identities and payload without inventing remote acceptance',async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {request_id:args?.requestId,message_id:args?.messageId};},()=>true);
 await bridge.bootstrap();await bridge.history('session');await bridge.prewarm();const receipt=await bridge.acceptMessage({request_id:'request',message_id:'message',session_id:'session',content:'text'});
 assert.deepEqual(calls.map(x=>x.command),['app_bootstrap','app_history','app_prewarm','app_accept']);assert.deepEqual(calls[3].args,{requestId:'request',messageId:'message',sessionId:'session',content:'text'});assert.equal(receipt.request_id,'request');assert.equal(receipt.accepted,undefined);
});
test('browser without native host cannot silently save in another store',async()=>{
 let invoked=0;const bridge=createKernelBridge(()=>{invoked++;},()=>false);
 for(const action of [()=>bridge.bootstrap(),()=>bridge.history('s'),()=>bridge.acceptMessage({}),()=>bridge.prewarm()])await assert.rejects(action(),/NATIVE_HOST_REQUIRED/);
 assert.equal(invoked,0);
});
test('lost native response retains draft and same request identity through bridge',async()=>{
 const ids=[];let generated=0;const bridge=createKernelBridge(async(command,args)=>{assert.equal(command,'app_accept');ids.push(args.requestId);if(ids.length===1)throw Error('IPC_LOST');return {request_id:args.requestId,message_id:args.messageId};},()=>true);
 const ui=createSendController({bridge,createIntent:content=>({request_id:`r${++generated}`,message_id:'m',session_id:'s',content}),onAccepted:()=>{}});
 ui.setDraft('hello');await assert.rejects(ui.send());assert.equal(ui.snapshot().draft,'hello');await ui.send();assert.deepEqual(ids,['r1','r1']);assert.equal(ui.snapshot().draft,'');
});

test('execution IPC passes identities without a renderer-supplied command or result', async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {};},()=>true);
 await bridge.execute('request');await bridge.executions('session');await bridge.cancel('execution');
 assert.deepEqual(calls,[{command:'app_execute',args:{requestId:'request'}},{command:'app_executions',args:{sessionId:'session'}},{command:'app_cancel',args:{executionId:'execution'}}]);
});
test('execution commands require the native host too',async()=>{
 const bridge=createKernelBridge(()=>assert.fail('must not invoke'),()=>false);
 for(const action of [()=>bridge.execute('r'),()=>bridge.executions('s'),()=>bridge.cancel('e')])await assert.rejects(action(),/NATIVE_HOST_REQUIRED/);
});

test('Home sync carries no renderer credential or fabricated result',async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return 'provider_unconfigured';},()=>true);
 assert.equal(await bridge.syncHome(),'provider_unconfigured');assert.deepEqual(calls,[{command:'app_home_sync',args:undefined}]);
 const unavailable=createKernelBridge(()=>assert.fail('no native'),()=>false);await assert.rejects(()=>unavailable.syncHome(),/NATIVE_HOST_REQUIRED/);
});

test('catalog, view-state and preferences IPC map multi-session arguments cleanly',async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {ok:true};},()=>true);
 await bridge.catalog();
 await bridge.createProject('proj-1','Bob Agent');
 await bridge.createConversation('conv-2','proj-1','Session 2');
 await bridge.selectConversation('conv-2');
 await bridge.saveViewState({conversationId:'conv-2',draft:'hello',scrollAnchorMessageId:'m1',scrollOffset:120,markRead:true});
 await bridge.savePreferences({sidebarGroupMode:'device',locale:'zh-CN',theme:'dark'});
 assert.deepEqual(calls,[
  {command:'app_catalog',args:undefined},
  {command:'app_project_create',args:{projectId:'proj-1',title:'Bob Agent'}},
  {command:'app_conversation_create',args:{conversationId:'conv-2',projectId:'proj-1',title:'Session 2'}},
  {command:'app_conversation_select',args:{conversationId:'conv-2'}},
  {command:'app_view_state_save',args:{conversationId:'conv-2',draft:'hello',scrollAnchorMessageId:'m1',scrollOffset:120,markRead:true}},
  {command:'app_preferences_save',args:{sidebarGroupMode:'device',locale:'zh-CN',theme:'dark'}}
 ]);
});

test('direct provider BYOK, ModelHub and execution route IPC map arguments cleanly',async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {ok:true};},()=>true);
 await bridge.providerGet();
 await bridge.providerSave({providerKind:'openai_compatible',label:'OpenAI',baseUrl:'https://api.openai.com/v1',modelId:'gpt-4o-mini',apiKey:'sk-test',clearKey:false,enabled:true});
 await bridge.providerTest();
 await bridge.modelHubGet();
 await bridge.providerKeySet('deepseek','sk-ds',false);
 await bridge.modelRoleAssign({role:'main',providerId:'deepseek',modelId:'deepseek-chat',modelName:'DeepSeek V4 Flash',providerName:'DeepSeek'});
 await bridge.providerVariantSet('qwen','coding');
 await bridge.routeGet('conv-1');
 await bridge.routeSet('conv-1','direct_byok');
 assert.deepEqual(calls,[
  {command:'app_provider_get',args:undefined},
  {command:'app_provider_save',args:{providerKind:'openai_compatible',label:'OpenAI',baseUrl:'https://api.openai.com/v1',modelId:'gpt-4o-mini',apiKey:'sk-test',clearKey:false,enabled:true}},
  {command:'app_provider_test',args:undefined},
  {command:'app_model_hub_get',args:undefined},
  {command:'app_provider_key_set',args:{providerId:'deepseek',apiKey:'sk-ds',clearKey:false}},
  {command:'app_model_role_assign',args:{role:'main',modelEntryId:'deepseek::deepseek-chat'}},
  {command:'app_provider_variant_set',args:{providerId:'qwen',variant:'coding'}},
  {command:'app_route_get',args:{sessionId:'conv-1'}},
  {command:'app_route_set',args:{sessionId:'conv-1',route:'direct_byok'}}
 ]);
});

test('responsive aspect-ratio layout resolves mobile portrait vs desktop landscape accurately',()=>{
 const portrait=createLayoutState({innerWidth:390,innerHeight:844},{userAgent:'Mozilla/5.0 (Linux; Android 14; Pixel 8)'});
 assert.equal(portrait.layoutMode,'mobile-portrait');
 assert.equal(portrait.aspect,'portrait');
 assert.equal(portrait.isCompact,true);
 const landscape=createLayoutState({innerWidth:1180,innerHeight:757},{userAgent:'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'});
 assert.equal(landscape.layoutMode,'desktop-wide');
 assert.equal(landscape.aspect,'landscape');
 assert.equal(landscape.isCompact,false);
 assert.equal(getProviderLogo('deepseek'),'/logos/deepseek.png');
 assert.equal(getProviderLogo('gemini'),'/logos/gemini.svg');
});

test('personal ops IPC maps schedule, note, capture, memory and decision commands cleanly',async()=>{
 const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {ok:true};},()=>true);
 await bridge.personalSnapshot();
 await bridge.scheduleCreate({id:'s1',kind:'todo',title:'Review',startAt:1793408400000,notes:'n'});
 await bridge.scheduleStatus('s1','completed');
 await bridge.noteUpsert({id:'n1',title:'Note',body:'Body',tags:['t1']});
 await bridge.captureCreate({id:'c1',messageId:'m1',content:'Snippet'});
 await bridge.memoryState('mem-1','active');
 await bridge.decisionRecord({id:'d1',body:'Use canonical SQLite snapshot'});
 await bridge.decisionStatus('d1','accepted');
 assert.deepEqual(calls,[
  {command:'app_personal_snapshot',args:undefined},
  {command:'app_schedule_create',args:{id:'s1',projectId:null,taskId:null,title:'Review',kind:'todo',startAt:1793408400000,endAt:null,reminderAt:null,notes:'n'}},
  {command:'app_schedule_status',args:{id:'s1',status:'completed'}},
  {command:'app_note_upsert',args:{id:'n1',projectId:null,conversationId:null,title:'Note',bodyMarkdown:'Body',tags:['t1']}},
  {command:'app_capture_create',args:{id:'c1',sourceMessageId:'m1',contentRef:'Snippet'}},
  {command:'app_memory_state',args:{id:'mem-1',newState:'active'}},
  {command:'app_decision_record',args:{id:'d1',projectId:null,taskId:null,sourceMessageId:null,body:'Use canonical SQLite snapshot',rationaleJson:'{}',status:'proposed',supersedesId:null}},
  {command:'app_decision_status',args:{id:'d1',status:'accepted'}}
 ]);
});

test('RV-06 canonical Rust-serialized personal_snapshot.json fixture matches kernel schema',()=>{
 const __dirname=path.dirname(fileURLToPath(import.meta.url));
 const fixturePath=path.join(__dirname,'fixtures','personal_snapshot.json');
 const snap=JSON.parse(fs.readFileSync(fixturePath,'utf-8'));
 assert.ok(Array.isArray(snap.schedules)&&snap.schedules.length>=2);
 assert.ok(snap.schedules.some(s=>s.kind==='event'&&s.status==='completed'&&s.start_at>=1e12));
 assert.ok(snap.schedules.some(s=>s.kind==='todo'&&s.status==='scheduled'&&s.start_at>=1e12));
 assert.ok(Array.isArray(snap.memories)&&snap.memories.length>=1);
 assert.equal(typeof snap.memories[0].body,'string');
 assert.ok(['candidate','active','superseded','forgotten'].includes(snap.memories[0].state));
 assert.ok(Array.isArray(snap.decisions)&&snap.decisions.length>=1);
 assert.equal(typeof snap.decisions[0].body,'string');
 assert.ok(['proposed','accepted','superseded','rejected'].includes(snap.decisions[0].status));
});

test('OL-02..OL-04 Online Work Service client executes PKCE S256 pairing and read-only /api/client/v1 queries',async()=>{
 const requests=[];
 const fetchImpl=async(url,opts={})=>{
  const body=opts.body?JSON.parse(opts.body):null;
  requests.push({url,method:opts.method||'GET',headers:opts.headers||{},body});
  if(url.endsWith('/api/client/v1/pairings')){
   assert.equal(body.challenge_method,'S256');
   assert.ok(body.challenge.length>=40);
   return {ok:true,status:200,json:async()=>({pairing_id:'pr-1',user_code:'ABCD-1234',verification_uri:'http://127.0.0.1:8787/pair'})};
  }
  if(url.endsWith('/api/client/v1/pairings/pr-1/exchange')){
   assert.ok(body.verifier.length>=43);
   return {ok:true,status:200,json:async()=>({access_token:'tok-online-1',device_id:'dev-online-1',scopes:['projects:read','tasks:read']})};
  }
  if(url.endsWith('/api/client/v1/projects')){
   return {ok:true,status:200,json:async()=>({items:[{id:'p-cloud-1',title:'Cloud Agent Deployment'}]})};
  }
  if(url.endsWith('/api/client/v1/projects/p-cloud-1/tasks')){
   return {ok:true,status:200,json:async()=>({items:[{id:'t-cloud-1',title:'Verify /api/client/v1',status:'running'}]})};
  }
  if(url.includes('/api/client/v1/projects/p-cloud-1/messages')){
   return {ok:true,status:200,json:async()=>({items:[{id:'m-cloud-1',role:'assistant',content:'Cloud task running'}]})};
  }
  if(url.endsWith('/api/client/v1/credentials/self/revoke')){
   return {ok:true,status:200,json:async()=>({revoked:true})};
  }
  return {ok:false,status:404,json:async()=>({error:'not_found'})};
 };
 const origin='http://127.0.0.1:8787';
 const client=createOnlineWorkClient({fetchImpl});
 const start=await client.beginPairing({origin,clientInstanceId:'ci-1',clientName:'Pixel 8'});
 assert.equal(start.user_code,'ABCD-1234');
 const exchanged=await client.exchangePairing({origin,pairingId:'pr-1',clientInstanceId:'ci-1',verifier:start.verifier});
 assert.equal(exchanged.access_token,'tok-online-1');
 const projects=await client.fetchProjects({origin,token:exchanged.access_token});
 assert.equal(projects.items[0].id,'p-cloud-1');
 const tasks=await client.fetchProjectTasks({origin,token:exchanged.access_token,projectId:'p-cloud-1'});
 assert.equal(tasks.items[0].status,'running');
 const msgs=await client.fetchProjectMessages({origin,token:exchanged.access_token,projectId:'p-cloud-1'});
 assert.equal(msgs.items[0].content,'Cloud task running');
 const rev=await client.revokeSelf({origin,token:exchanged.access_token});
 assert.equal(rev.revoked,true);
});
