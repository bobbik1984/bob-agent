// Transport loss is not evidence that a tool finished or an approval was granted.
export function unconfirmActivity(activity){
 if(!activity)return activity;
 return {...activity,
  items:(activity.items||[]).map(item=>item.status==='running'?{...item,status:'unconfirmed'}:{...item}),
  approvals:(activity.approvals||[]).map(item=>['pending','queued'].includes(item.status)?{...item,status:'unconfirmed'}:{...item})};
}

// History owns settled reply text, but execution evidence remains independently useful.
export function settleNativeActivity(draft,status,activity){
 const evidence=activity===undefined?draft?.activity:activity;
 if(!evidence)return null;
 return {...draft,text:'',active:false,terminal:true,completed:status==='completed',
  cancelSupported:false,unconfirmed:false,activity:unconfirmActivity(evidence)};
}

// Snapshot enumeration can follow request IDs; display source observation order.
export function orderedNativeActivities(drafts){
 const time=value=>typeof value==='number'&&Number.isFinite(value)?value:Number.POSITIVE_INFINITY;
 return Object.entries(drafts).filter(([,v])=>v.activity&&((v.activity.items?.length||0)+(v.activity.approvals?.length||0)>0))
  .sort((a,b)=>time(a[1].timestamp)-time(b[1].timestamp))
  .map(([id,v])=>({id,activity:v.activity}));
}

// Only a live, identity-bound and unexpired source request is actionable.
export function actionableApproval(approval,now=Date.now()){
 return approval?.actionable===true&&approval.status==='pending'
  &&typeof approval.permission_id==='string'&&approval.permission_id.length>0
  &&Number.isFinite(approval.expires_at)&&approval.expires_at>now;
}
export function waitingNativeApproval(drafts,now=Date.now()){
 return Object.values(drafts).some(record=>record.active===true&&!record.terminal
  &&(record.activity?.approvals||[]).some(approval=>actionableApproval(approval,now)));
}
