//! Durable claim/complete boundary. No database borrow is held across network I/O.
use crate::outbox::{digest, AppendMessage, AttemptOutcome, Error, HomeReceipt, Result, Store};
use crate::{
    prewarm::{Trigger, WatchKey},
    wake::{ReadyWatch, WakeAdapter, WakeCoordinator},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
pub struct CancelExecution {
    pub request_id: String,
    pub session_id: String,
    pub origin_device_id: String,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub execution_id: String,
    pub expected_version: i64,
    pub permission_ref: String,
}
#[derive(Clone)]
pub enum WireIntent {
    Message(AppendMessage),
    Cancel(CancelExecution),
}
impl WireIntent {
    pub fn request_id(&self) -> &str {
        match self {
            Self::Message(m) => &m.request_id,
            Self::Cancel(c) => &c.request_id,
        }
    }
    pub fn home(&self) -> (&str, i64) {
        match self {
            Self::Message(m) => (&m.home_device_id, m.home_epoch),
            Self::Cancel(c) => (&c.home_device_id, c.home_epoch),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Send,
    Lookup,
}
pub struct DispatchJob {
    pub intent: WireIntent,
    pub payload_sha256: String,
    pub kind: JobKind,
    attempt: i64,
}
pub enum TransportResult {
    InvalidConfiguration,
    Receipt(HomeReceipt),
    ConfirmedZero,
    Uncertain,
    Absent,
    Unavailable,
}
/// Implementations must authenticate the Home, enforce their own I/O deadlines,
/// and never label generic timeouts ConfirmedZero. Absent is NOT a replay permit.
pub trait Transport {
    fn send<'a>(
        &'a self,
        job: &'a DispatchJob,
        watch: &'a ReadyWatch,
    ) -> Pin<Box<dyn Future<Output = TransportResult> + Send + 'a>>;
    fn lookup<'a>(
        &'a self,
        job: &'a DispatchJob,
        watch: &'a ReadyWatch,
    ) -> Pin<Box<dyn Future<Output = TransportResult> + Send + 'a>>;
}
impl DispatchJob {
    /// Entire transport phase has a bounded deadline; a timed-out send remains
    /// ambiguous. Adapters must abort I/O when their future is dropped.
    pub async fn run(
        &self,
        wake: &WakeCoordinator,
        key: WatchKey,
        adapter: Arc<dyn WakeAdapter>,
        transport: &impl Transport,
        timeout: Duration,
    ) -> TransportResult {
        let session = match &self.intent {
            WireIntent::Message(m) => &m.session_id,
            WireIntent::Cancel(c) => &c.session_id,
        };
        if key.device != self.intent.home().0 || &key.session != session || timeout.is_zero() {
            return TransportResult::InvalidConfiguration;
        }
        let watch = match wake.request(key.clone(), Trigger::Send, adapter).await {
            Ok(watch) => watch,
            Err(_) => {
                return if self.kind == JobKind::Send {
                    TransportResult::ConfirmedZero
                } else {
                    TransportResult::Unavailable
                }
            }
        };
        if !wake.is_current(&key, &watch) {
            return TransportResult::ConfirmedZero;
        }
        let operation = match self.kind {
            JobKind::Send => transport.send(self, &watch),
            JobKind::Lookup => transport.lookup(self, &watch),
        };
        match crate::deadline::bounded(operation, timeout).await {
            Some(result) => result,
            None => TransportResult::Uncertain,
        }
    }
}
#[derive(Clone, Copy)]
pub struct DispatchPolicy {
    pub retry_ms: i64,
    pub lease_ms: i64,
    pub max_attempts: i64,
}
impl DispatchPolicy {
    fn valid(self) -> Result<Self> {
        if self.retry_ms <= 0 || self.lease_ms <= 0 || self.max_attempts <= 0 {
            Err(Error::InvalidTransition)
        } else {
            Ok(self)
        }
    }
}
impl Store {
    pub fn queue_cancel(&mut self, request: &CancelExecution, now: i64) -> Result<()> {
        if request.expected_version < 1
            || [
                &request.request_id,
                &request.session_id,
                &request.origin_device_id,
                &request.home_device_id,
                &request.execution_id,
                &request.permission_ref,
            ]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err(Error::InvalidTransition);
        }
        let hash = digest(&serde_json::to_vec(request)?);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT payload_sha256 FROM intent_outbox WHERE request_id=?1",
                [&request.request_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            if old != hash {
                return Err(Error::IdentityConflict);
            }
        } else {
            let authority: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM home_authority WHERE device_id=?1 AND epoch=?2)",
                params![request.home_device_id, request.home_epoch],
                |r| r.get(0),
            )?;
            if !authority {
                return Err(Error::InvalidTransition);
            }
            tx.execute("INSERT INTO intent_outbox(request_id,session_id,origin_device_id,target_device_id,home_epoch,kind,lane,payload_json,payload_sha256,expected_version,permission_ref,send_state,created_at,transport_evidence) VALUES(?1,?2,?3,?4,?5,'cancel_execution','control',?6,?7,?8,?9,'local_saved',?10,'none')",params![request.request_id,request.session_id,request.origin_device_id,request.home_device_id,request.home_epoch,serde_json::to_string(request)?,hash,request.expected_version,request.permission_ref,now])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// One bounded scheduler tick. Control is selected before normal traffic.
    /// Expired claims become unknown, never automatically safe to send again.
    pub fn claim_next(&mut self, now: i64, policy: DispatchPolicy) -> Result<Option<DispatchJob>> {
        let policy = policy.valid()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE intent_outbox SET send_state='unknown_send',transport_evidence='started_or_possible',lease_until=NULL WHERE send_state IN ('dispatching','dispatched') AND lease_until<=?1",[now])?;
        tx.execute("UPDATE intent_outbox SET send_state='needs_attention',last_error='RETRY_BUDGET_OR_EXPIRY' WHERE send_state IN ('local_saved','queued_for_wakeup','not_dispatched') AND (attempts>=?1 OR expires_at<=?2)",params![policy.max_attempts,now])?;
        let id:Option<String>=tx.query_row("SELECT o.request_id FROM intent_outbox o WHERE o.send_state IN ('local_saved','queued_for_wakeup','not_dispatched','unknown_send') AND (o.next_attempt_at IS NULL OR o.next_attempt_at<=?1) AND (o.lease_until IS NULL OR o.lease_until<=?1) AND NOT EXISTS(SELECT 1 FROM intent_outbox active WHERE active.target_device_id=o.target_device_id AND active.lane=o.lane AND active.lease_until>?1 AND active.send_state IN ('dispatching','dispatched','unknown_send')) AND EXISTS(SELECT 1 FROM home_authority h WHERE h.device_id=o.target_device_id AND h.epoch=o.home_epoch) AND (o.lane='control' OR o.send_state='unknown_send' OR NOT EXISTS(SELECT 1 FROM intent_outbox b WHERE b.session_id=o.session_id AND b.lane='normal' AND b.request_id!=o.request_id AND (b.send_state IN ('dispatching','dispatched','unknown_send') OR (b.send_state IN ('local_saved','queued_for_wakeup','not_dispatched') AND (b.created_at<o.created_at OR (b.created_at=o.created_at AND b.request_id<o.request_id)))))) ORDER BY CASE o.lane WHEN 'control' THEN 0 ELSE 1 END,o.created_at,o.request_id LIMIT 1",[now],|r|r.get(0)).optional()?;
        let Some(id) = id else {
            tx.commit()?;
            return Ok(None);
        };
        let (kind,payload,hash,state,session,message,origin,home,epoch):(String,String,String,String,String,Option<String>,String,String,i64)=tx.query_row("SELECT kind,payload_json,payload_sha256,send_state,session_id,message_id,origin_device_id,target_device_id,home_epoch FROM intent_outbox WHERE request_id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?)))?;
        let intent = match kind.as_str() {
            "append_message" => WireIntent::Message(AppendMessage {
                request_id: id.clone(),
                session_id: session,
                message_id: message.ok_or(Error::InvalidTransition)?,
                origin_device_id: origin,
                home_device_id: home,
                home_epoch: epoch,
                content: serde_json::from_str(&payload)?,
            }),
            "cancel_execution" => WireIntent::Cancel(serde_json::from_str(&payload)?),
            _ => return Err(Error::InvalidTransition),
        };
        let actual = match &intent {
            WireIntent::Message(m) => digest(&serde_json::to_vec(m)?),
            WireIntent::Cancel(c) => digest(&serde_json::to_vec(c)?),
        };
        if actual != hash {
            return Err(Error::IdentityConflict);
        }
        let jobkind = if state == "unknown_send" {
            JobKind::Lookup
        } else {
            JobKind::Send
        };
        tx.execute("UPDATE intent_outbox SET send_state=?2,attempts=attempts+1,lease_until=?3,dispatched_at=?4,transport_evidence='started_or_possible' WHERE request_id=?1",params![id,if jobkind==JobKind::Send {"dispatching"}else{"unknown_send"},now.saturating_add(policy.lease_ms),now])?;
        let attempt = tx.query_row(
            "SELECT attempts FROM intent_outbox WHERE request_id=?1",
            [&id],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(Some(DispatchJob {
            intent,
            payload_sha256: hash,
            kind: jobkind,
            attempt,
        }))
    }

    pub fn complete_job(
        &mut self,
        job: &DispatchJob,
        outcome: TransportResult,
        now: i64,
        policy: DispatchPolicy,
    ) -> Result<()> {
        policy.valid()?;
        let id = job.intent.request_id();
        let current:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM intent_outbox WHERE request_id=?1 AND attempts=?2 AND send_state IN ('dispatching','unknown_send'))",params![id,job.attempt],|r|r.get(0))?;
        if !current {
            return Err(Error::InvalidTransition);
        }
        if job.kind == JobKind::Send && matches!(outcome, TransportResult::InvalidConfiguration) {
            self.db.execute("UPDATE intent_outbox SET send_state='needs_attention',transport_evidence='confirmed_zero',last_error='CONNECTION_CONFIGURATION_INVALID',lease_until=NULL WHERE request_id=?1 AND attempts=?2",params![id,job.attempt])?;
            return Ok(());
        }
        match outcome {
            TransportResult::Receipt(receipt) => {
                if receipt.request_id != id || receipt.payload_sha256 != job.payload_sha256 {
                    return Err(Error::IdentityConflict);
                }
                self.apply_receipt(&receipt, now)?;
            }
            other => {
                if job.kind == JobKind::Send {
                    self.finish_attempt(
                        id,
                        job.attempt,
                        if matches!(other, TransportResult::ConfirmedZero) {
                            AttemptOutcome::ConfirmedZeroBytes
                        } else {
                            AttemptOutcome::PossiblySent
                        },
                    )?;
                }
                self.db.execute("UPDATE intent_outbox SET lease_until=NULL,next_attempt_at=?2,last_error=?3 WHERE request_id=?1",params![id,now.saturating_add(policy.retry_ms),if job.kind==JobKind::Lookup {"RECEIPT_UNCONFIRMED"}else{"TRANSPORT_UNCONFIRMED"}])?;
            }
        }
        Ok(())
    }
}
