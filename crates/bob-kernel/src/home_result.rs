//! Authenticated, exact-request result projection; no execution or replay.
use crate::{
    home_auth::{authorize, HomeMessageContext},
    outbox::{digest, AppendMessage, Error, HomeReceipt, Result, Store},
    personal_ops::PeerSyncBundle,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeResult {
    pub receipt: Option<HomeReceipt>,
    pub execution: Option<HomeExecutionResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_bundle: Option<PeerSyncBundle>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeExecutionResult {
    pub execution_id: String,
    pub version: i64,
    pub status: String,
    pub stop_requested: bool,
    pub stop_confirmed: bool,
    pub reply: Option<HomeReply>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeReply {
    pub message_id: String,
    pub text: String,
    pub created_at: i64,
}
impl Store {
    pub fn lookup_home_result(
        &mut self,
        ctx: &HomeMessageContext,
        request: &AppendMessage,
        now: i64,
    ) -> Result<HomeResult> {
        let hash = digest(&serde_json::to_vec(request)?);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        authorize(&tx, ctx, request, now, false)?;
        let stored: Option<(String,i64,String,String)>=tx.query_row("SELECT origin_device_id,home_epoch,payload_sha256,outcome FROM inbox_receipts WHERE request_id=?1",[&request.request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let receipt = match stored {
            None => {
                tx.commit()?;
                return Ok(HomeResult {
                    receipt: None,
                    execution: None,
                    sync_bundle: None,
                });
            }
            Some((origin, epoch, stored, outcome)) => {
                if origin != request.origin_device_id
                    || epoch != request.home_epoch
                    || stored != hash
                {
                    return Err(Error::IdentityConflict);
                }
                HomeReceipt {
                    request_id: request.request_id.clone(),
                    home_device_id: request.home_device_id.clone(),
                    home_epoch: epoch,
                    payload_sha256: hash,
                    accepted: outcome == "accepted",
                }
            }
        };
        let mut execution:Option<HomeExecutionResult>=tx.query_row("SELECT e.id,e.version,e.status,e.stop_requested_at IS NOT NULL,e.stop_confirmed_at IS NOT NULL FROM executions e JOIN tasks t ON t.id=e.task_id JOIN messages m ON m.id=t.source_message_id WHERE e.request_id=?1 AND m.id=?2 AND m.conversation_id=?3 AND m.origin_device_id=?4",params![request.request_id,request.message_id,request.session_id,request.origin_device_id],|r|Ok(HomeExecutionResult {execution_id:r.get(0)?,version:r.get(1)?,status:r.get(2)?,stop_requested:r.get(3)?,stop_confirmed:r.get(4)?,reply:None})).optional()?;
        if let Some(e) = execution.as_mut() {
            // Only publish a persisted successful reply, linked to this exact
            // execution and source message. Never expose raw terminal/credentials.
            if e.status == "completed" {
                e.reply=tx.query_row("SELECT id,json_extract(content_json,'$.text'),created_at FROM messages WHERE conversation_id=?1 AND role='assistant' AND json_extract(content_json,'$.execution_id')=?2 AND json_extract(content_json,'$.reply_to')=?3 ORDER BY created_at DESC,rowid DESC LIMIT 1",params![request.session_id,e.execution_id,request.message_id],|r|Ok(HomeReply {message_id:r.get(0)?,text:r.get(1)?,created_at:r.get(2)?})).optional()?;
                if e.reply.as_ref().is_some_and(|r| r.text.len() > 192 * 1024) {
                    return Err(Error::InvalidTransition);
                }
            }
        }
        tx.commit()?;
        Ok(HomeResult {
            receipt: Some(receipt),
            execution,
            sync_bundle: None,
        })
    }
}
