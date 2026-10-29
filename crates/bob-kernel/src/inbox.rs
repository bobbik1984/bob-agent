//! Durable Home acceptance for messages, never a model/runner execution API.
use crate::outbox::{digest, AppendMessage, Error, HomeReceipt, Result, Store};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::json;

use crate::home_auth::authorize;
pub use crate::home_auth::HomeMessageContext;

fn prior(tx: &Transaction<'_>, request: &AppendMessage, hash: &str) -> Result<Option<HomeReceipt>> {
    let found: Option<(String,i64,String,String)> = tx.query_row(
        "SELECT origin_device_id,home_epoch,payload_sha256,outcome FROM inbox_receipts WHERE request_id=?1",
        [&request.request_id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    match found {
        None => Ok(None),
        Some((origin, epoch, stored, outcome)) => {
            if origin != request.origin_device_id || epoch != request.home_epoch || stored != hash {
                return Err(Error::IdentityConflict);
            }
            Ok(Some(HomeReceipt {
                request_id: request.request_id.clone(),
                home_device_id: request.home_device_id.clone(),
                home_epoch: epoch,
                payload_sha256: stored,
                accepted: outcome == "accepted",
            }))
        }
    }
}

impl Store {
    /// Message, event and receipt commit atomically. A repeated accepted request
    /// returns the original outcome without appending another message or event.
    pub fn receive_message(
        &mut self,
        ctx: &HomeMessageContext,
        request: &AppendMessage,
        now: i64,
    ) -> Result<HomeReceipt> {
        let hash = digest(&serde_json::to_vec(request)?);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, ctx, request, now, false)?;
        if let Some(receipt) = prior(&tx, request, &hash)? {
            tx.commit()?;
            return Ok(receipt);
        }
        // Home itself uses the same local-first outbox. Promote only its exact
        // matching local message plus immutable outbox identity, never a
        // conflicting row or an arbitrary Replica-local conversation.
        let content = serde_json::to_string(&request.content)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE id=?1 OR request_id=?2)",
            params![request.message_id, request.request_id],
            |r| r.get(0),
        )?;
        if exists {
            let exact:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM messages m JOIN intent_outbox o ON o.request_id=m.request_id WHERE m.id=?1 AND m.request_id=?2 AND m.conversation_id=?3 AND m.origin_device_id=?4 AND m.role='user' AND m.content_json=?5 AND m.acceptance='local' AND o.payload_sha256=?6 AND o.target_device_id=?7 AND o.home_epoch=?8 AND o.kind='append_message' AND o.send_state NOT IN ('cancelled_local','rejected','needs_attention'))",params![request.message_id,request.request_id,request.session_id,request.origin_device_id,content,hash,request.home_device_id,request.home_epoch],|r|r.get(0))?;
            if !exact || request.origin_device_id != ctx.local_home_device_id {
                return Err(Error::IdentityConflict);
            }
            let updated=tx.execute("UPDATE conversations SET acceptance='home' WHERE id=?1 AND acceptance IN ('local','home') AND archived_at IS NULL",[&request.session_id])?;
            if updated != 1 {
                return Err(Error::InvalidTransition);
            }
            tx.execute(
                "UPDATE messages SET acceptance='home' WHERE id=?1",
                [&request.message_id],
            )?;
        } else {
            tx.execute("INSERT OR IGNORE INTO conversations(id,title,acceptance,version,created_at) VALUES(?1,'','home',1,?2)",params![request.session_id,now])?;
            let writable: bool = tx.query_row(
                "SELECT acceptance='home' AND archived_at IS NULL FROM conversations WHERE id=?1",
                [&request.session_id],
                |r| r.get(0),
            )?;
            if !writable {
                return Err(Error::InvalidTransition);
            }
            tx.execute("INSERT INTO messages(id,conversation_id,origin_device_id,request_id,role,content_json,acceptance,created_at) VALUES(?1,?2,?3,?4,'user',?5,'home',?6)",params![request.message_id,request.session_id,request.origin_device_id,request.request_id,content,now])?;
        }
        let result = serde_json::to_string(
            &json!({"message_id":request.message_id,"conversation_id":request.session_id}),
        )?;
        tx.execute("INSERT INTO domain_events(event_id,authority_epoch,object_kind,object_id,object_version,request_id,source_device_id,type,payload_json,created_at) VALUES(?1,?2,'message',?3,1,?4,?5,'message_accepted',?6,?7)",params![format!("message-accepted:{}",request.request_id),request.home_epoch,request.message_id,request.request_id,request.origin_device_id,result,now])?;
        tx.execute("INSERT INTO inbox_receipts(request_id,origin_device_id,home_epoch,payload_sha256,outcome,result_json,effect_ref,decided_at) VALUES(?1,?2,?3,?4,'accepted',?5,?6,?7)",params![request.request_id,request.origin_device_id,request.home_epoch,hash,result,request.message_id,now])?;
        tx.commit()?;
        Ok(HomeReceipt {
            request_id: request.request_id.clone(),
            home_device_id: request.home_device_id.clone(),
            home_epoch: request.home_epoch,
            payload_sha256: hash,
            accepted: true,
        })
    }

    /// None means "not found in this store", NOT authoritative absence or
    /// permission to resend. Restored/truncated history needs separate fencing.
    pub fn lookup_message_receipt(
        &mut self,
        ctx: &HomeMessageContext,
        request: &AppendMessage,
        now: i64,
    ) -> Result<Option<HomeReceipt>> {
        let hash = digest(&serde_json::to_vec(request)?);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        authorize(&tx, ctx, request, now, false)?;
        let result = prior(&tx, request, &hash)?;
        tx.commit()?;
        Ok(result)
    }
}

impl Store {
    /// Accept only the request to stop; execution remains running/unknown and
    /// its workspace lease stays held until the runner confirms termination.
    pub fn receive_cancel(
        &mut self,
        ctx: &HomeMessageContext,
        request: &crate::dispatch::CancelExecution,
        now: i64,
    ) -> Result<HomeReceipt> {
        let hash = digest(&serde_json::to_vec(request)?);
        let binding = cancel_binding(request);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, ctx, &binding, now, true)?;
        if let Some(receipt) = prior(&tx, &binding, &hash)? {
            tx.commit()?;
            return Ok(receipt);
        }
        let version: Option<i64>=tx.query_row(
            "SELECT e.version FROM executions e JOIN tasks t ON t.id=e.task_id WHERE e.id=?1 AND (EXISTS(SELECT 1 FROM messages m WHERE m.id=t.source_message_id AND m.conversation_id=?2) OR EXISTS(SELECT 1 FROM native_sessions n WHERE n.id=e.native_session_binding_id AND n.conversation_id=?2)) AND e.status IN ('pending','dispatching','running','paused','unknown') AND e.stop_requested_at IS NULL",
            params![request.execution_id,request.session_id],|r|r.get(0)).optional()?;
        let accepted =
            version == Some(request.expected_version) && !request.permission_ref.trim().is_empty();
        let result = if accepted {
            json!({"execution_id":request.execution_id,"state":"stop_requested"})
        } else {
            json!({"code":"CANCEL_SCOPE_OR_VERSION_REJECTED"})
        };
        if accepted {
            let changed=tx.execute("UPDATE executions SET stop_requested_at=?2,version=version+1 WHERE id=?1 AND version=?3 AND stop_requested_at IS NULL",params![request.execution_id,now,request.expected_version])?;
            if changed != 1 {
                return Err(Error::InvalidTransition);
            }
            tx.execute("INSERT INTO domain_events(event_id,authority_epoch,object_kind,object_id,object_version,request_id,source_device_id,type,payload_json,created_at) VALUES(?1,?2,'execution',?3,?4,?5,?6,'stop_requested',?7,?8)",params![format!("cancel-requested:{}",request.request_id),request.home_epoch,request.execution_id,request.expected_version+1,request.request_id,request.origin_device_id,serde_json::to_string(&result)?,now])?;
        }
        tx.execute("INSERT INTO inbox_receipts(request_id,origin_device_id,home_epoch,payload_sha256,outcome,result_json,effect_ref,decided_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![request.request_id,request.origin_device_id,request.home_epoch,hash,if accepted{"accepted"}else{"rejected"},serde_json::to_string(&result)?,if accepted{Some(&request.execution_id)}else{None},now])?;
        tx.commit()?;
        Ok(HomeReceipt {
            request_id: request.request_id.clone(),
            home_device_id: request.home_device_id.clone(),
            home_epoch: request.home_epoch,
            payload_sha256: hash,
            accepted,
        })
    }

    pub fn lookup_cancel_receipt(
        &mut self,
        ctx: &HomeMessageContext,
        request: &crate::dispatch::CancelExecution,
        now: i64,
    ) -> Result<Option<HomeReceipt>> {
        let hash = digest(&serde_json::to_vec(request)?);
        let binding = cancel_binding(request);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        authorize(&tx, ctx, &binding, now, true)?;
        let receipt = prior(&tx, &binding, &hash)?;
        tx.commit()?;
        Ok(receipt)
    }
}

fn cancel_binding(request: &crate::dispatch::CancelExecution) -> AppendMessage {
    AppendMessage {
        request_id: request.request_id.clone(),
        session_id: request.session_id.clone(),
        message_id: request.execution_id.clone(),
        origin_device_id: request.origin_device_id.clone(),
        home_device_id: request.home_device_id.clone(),
        home_epoch: request.home_epoch,
        content: serde_json::Value::Null,
    }
}
