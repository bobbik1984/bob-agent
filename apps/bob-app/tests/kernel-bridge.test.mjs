import test from 'node:test';
import assert from 'node:assert/strict';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { createSendController } from '../src/send-controller.mjs';
import { createLayoutState, getProviderLogo } from '../src/layout-mode.mjs';

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
