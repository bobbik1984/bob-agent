import test from 'node:test';
import assert from 'node:assert/strict';
import {isNotAcceptedReceipt} from '../src/native-receipt.mjs';
test('only an identity-bound explicit negative receipt releases a pending draft',()=>{
 const base={session_id:'agy:session',request_id:'request'};
 for(const fields of [{status:'not_accepted'},{status:'accepted',execution_status:'not_accepted'}]) {
  assert.equal(isNotAcceptedReceipt({...base,...fields},base.session_id,base.request_id),true);
  assert.equal(isNotAcceptedReceipt({...base,...fields},'other',base.request_id),false);
  assert.equal(isNotAcceptedReceipt({...base,...fields},base.session_id,'other'),false);
 }
 for(const status of ['unknown','accepted','error',undefined])assert.equal(isNotAcceptedReceipt({...base,status},base.session_id,base.request_id),false);
 assert.equal(isNotAcceptedReceipt(null,base.session_id,base.request_id),false);
});
