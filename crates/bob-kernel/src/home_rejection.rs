//! Definite pre-dispatch rejection, durably bound to the original request.
use crate::{app::LocalApp, home_auth::{authorize,HomeMessageContext}, outbox::{digest,AppendMessage,Error,HomeReceipt,Result,Store}};
use rusqlite::{params,OptionalExtension,TransactionBehavior};
impl Store {
    /// Trusted host policy decides these reasons; a peer cannot choose one.
    /// Repeated requests retain their original receipt, including acceptance.
    pub fn reject_message(&mut self, ctx:&HomeMessageContext, request:&AppendMessage, reason:&str, now:i64)->Result<HomeReceipt> {
        if !matches!(reason,"HOME_RUNNER_BUSY"|"HOME_EXECUTION_NOT_APPROVED") {return Err(Error::InvalidTransition);}
        let hash=digest(&serde_json::to_vec(request)?);
        let tx=self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx,ctx,request,now,false)?;
        let prior:Option<(String,i64,String,String)>=tx.query_row("SELECT origin_device_id,home_epoch,payload_sha256,outcome FROM inbox_receipts WHERE request_id=?1",[&request.request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if let Some((origin,epoch,stored,outcome))=prior {
            if origin!=request.origin_device_id || epoch!=request.home_epoch || stored!=hash {return Err(Error::IdentityConflict);}
            return Ok(HomeReceipt{request_id:request.request_id.clone(),home_device_id:request.home_device_id.clone(),home_epoch:epoch,payload_sha256:stored,accepted:outcome=="accepted"});
        }
        tx.execute("INSERT INTO inbox_receipts(request_id,origin_device_id,home_epoch,payload_sha256,outcome,result_json,effect_ref,decided_at) VALUES(?1,?2,?3,?4,'rejected',?5,NULL,?6)",params![request.request_id,request.origin_device_id,request.home_epoch,hash,serde_json::json!({"code":reason,"execution_started":false}).to_string(),now])?;
        tx.commit()?;
        Ok(HomeReceipt{request_id:request.request_id.clone(),home_device_id:request.home_device_id.clone(),home_epoch:request.home_epoch,payload_sha256:hash,accepted:false})
    }
}
impl LocalApp {
    pub fn host_reject_message(&mut self, ctx:&HomeMessageContext, request:&AppendMessage, reason:&str, now:i64)->Result<HomeReceipt> {
        self.store.reject_message(ctx,request,reason,now)
    }
}
