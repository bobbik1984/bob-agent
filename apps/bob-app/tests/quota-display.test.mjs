import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/composables/useModelHub.mjs',import.meta.url),'utf8').replace(/^import .*;\r?\n/gm,'').replace('export function useModelHub','function useModelHub');
test('missing and legacy quota snapshots cannot display example subscription balances',()=>{
 const ctx={localStorage:{getItem:()=>null},ref:value=>({value}),reactive:x=>x,computed:fn=>({get value(){return fn()}}),defaultRegistryData:{providers:[]},mobileModelsData:[]};
 vm.createContext(ctx);vm.runInContext(source+';globalThis.create=useModelHub;',ctx);
 const hub=ctx.create({t:key=>key});
 assert.equal(hub.subscriptionQuotas.value.length,3);
 hub.modelHub.value.state.subscription_quotas={codex:{remaining_weekly_pct:93,reset_time_pct:28}};
 for(const row of hub.subscriptionQuotas.value){assert.equal(row.remainingWeeklyPct,undefined);assert.equal(row.resetTimePct,undefined);}
 for(const row of hub.prepaidUsageList.value){assert.equal(row.tokensText,'quotaUnavailable');assert.equal(row.costText,'quotaUnavailable');}
});

test('saving a token does not establish a verified online connection',()=>{
 const ctx={localStorage:{getItem:()=>null},ref:value=>({value}),reactive:x=>x,computed:fn=>({get value(){return fn()}}),defaultRegistryData:{providers:[]},mobileModelsData:[]};
 vm.createContext(ctx);vm.runInContext(source+';globalThis.create=useModelHub;',ctx);
 const onlineState={connected:false};let refreshes=0;
 const hub=ctx.create({t:key=>key,onlineState,restoreRemoteSelection:{value:false},handleOnlineRefresh:()=>{assert.equal(onlineState.connected,false);refreshes++;}});
 hub.applyModelHubSnapshot({remoteNodes:[{id:'path',baseUrl:'https://node.example',hasToken:true}]});
 assert.equal(onlineState.connected,false);assert.equal(refreshes,1);
});
