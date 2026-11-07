import test from 'node:test';
import assert from 'node:assert/strict';
import {unconfirmActivity,settleNativeActivity,orderedNativeActivities,actionableApproval,waitingNativeApproval} from '../src/native-activity.mjs';
test('disconnect preserves confirmed results but invalidates active tools and approvals without mutating cache',()=>{
 const original={partial:true,items:[{id:'a',status:'running'},{id:'b',status:'completed'},{id:'c',status:'error'}],approvals:[{id:'p',status:'pending'}]};
 const result=unconfirmActivity(original);
 assert.deepEqual(result.items.map(x=>x.status),['unconfirmed','completed','error']);
 assert.equal(result.approvals[0].status,'unconfirmed');assert.equal(result.partial,true);
 assert.equal(original.items[0].status,'running');assert.equal(original.approvals[0].status,'pending');
 assert.deepEqual(unconfirmActivity(result),result);assert.equal(unconfirmActivity(null),null);
});

test('settled history removes duplicate text while retaining separately arriving activity evidence',()=>{
 const activity={items:[{id:'tool',status:'completed'}],approvals:[{id:'approval',status:'pending'}]};
 const before={text:'already in history',active:true,cancelSupported:true,activity};
 for(const status of ['completed','cancelled','error','not_accepted']){
  const after=settleNativeActivity(before,status);
  assert.equal(after.text,'');assert.equal(after.active,false);assert.equal(after.terminal,true);assert.equal(after.cancelSupported,false);
  assert.equal(after.activity.items[0].status,'completed');assert.equal(after.activity.approvals[0].status,'unconfirmed');
  assert.equal(after.completed,status==='completed');
 }
 assert.equal(before.text,'already in history');assert.equal(before.activity.approvals[0].status,'pending');
 assert.equal(settleNativeActivity({text:'old'},'completed'),null);
 assert.equal(settleNativeActivity({},'completed',activity).activity.items.length,1);
});

test('restored activities follow observed chronology rather than request id order',()=>{
 const activity={items:[{id:'tool',status:'completed'}]};
 const drafts={later:{timestamp:20,activity},unknown:{activity},earlier:{timestamp:10,activity},empty:{timestamp:1}};
 assert.deepEqual(orderedNativeActivities(drafts).map(x=>x.id),['earlier','later','unknown']);
 assert.deepEqual(Object.keys(drafts),['later','unknown','earlier','empty']);
});

test('approval requires live ownership, identity and expiry, not an old cached pending label',()=>{
 const approval={status:'pending',actionable:true,permission_id:'source-bound',expires_at:200};
 const live={active:true,activity:{approvals:[approval]}};
 assert.equal(waitingNativeApproval({live},100),true);
 for(const patch of [{active:false},{terminal:true}])assert.equal(waitingNativeApproval({live:{...live,...patch}},100),false);
 for(const patch of [{status:'queued'},{status:'denied'},{status:'unconfirmed'},{actionable:false},{permission_id:''},{expires_at:100},{expires_at:undefined}]){
  assert.equal(actionableApproval({...approval,...patch},100),false);
 }
 assert.equal(waitingNativeApproval({live:{...live,activity:unconfirmActivity(live.activity)}},100),false);
 assert.equal(waitingNativeApproval({live:settleNativeActivity(live,'completed')},100),false);
 assert.equal(waitingNativeApproval({live},201),false);
 assert.equal(approval.status,'pending');
});
