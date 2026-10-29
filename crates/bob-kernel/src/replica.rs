//! Replica facade. Pairing confirmation is a trusted native input, never an RPC.
use crate::{
    app::LocalApp,
    dispatch::{DispatchJob, DispatchPolicy, TransportResult},
    outbox::{Error, Result},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl LocalApp {
    pub fn device_id(&self) -> &str {
        &self.origin
    }
    /// Called only after authenticated pairing AND local user confirmation.
    /// Existing intents keep their immutable original target; they are not sent
    /// retroactively to the newly paired Home. Authority changes require an
    /// explicit separate migration and are rejected here.
    pub fn bind_confirmed_home(
        &mut self,
        home_id: &str,
        epoch: i64,
        identity_sha256: &str,
        now: i64,
    ) -> Result<()> {
        if home_id.trim().is_empty()
            || home_id == self.origin
            || epoch < 1
            || identity_sha256.len() != 64
            || !identity_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (old, old_epoch): (String, i64) = tx.query_row(
            "SELECT device_id,epoch FROM home_authority WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if old != self.origin && (old != home_id || old_epoch != epoch) {
            return Err(Error::IdentityConflict);
        }
        let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM executions WHERE status IN ('pending','dispatching','running','paused','unknown'))",[],|r|r.get(0))?;
        if active {
            return Err(Error::InvalidTransition);
        }
        if old == home_id {
            let pin: String = tx.query_row(
                "SELECT public_key_ref FROM devices WHERE id=?1",
                [home_id],
                |r| r.get(0),
            )?;
            if pin != identity_sha256 {
                return Err(Error::IdentityConflict);
            }
        } else {
            tx.execute("INSERT INTO devices(id,label,public_key_ref,trust_state,created_at) VALUES(?1,'Home',?2,'paired',?3)",params![home_id,identity_sha256,now])?;
            tx.execute(
                "UPDATE home_authority SET device_id=?1,epoch=?2 WHERE singleton=1",
                params![home_id, epoch],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn paired_home(&self) -> Result<Option<(String, i64, String)>> {
        let (id, epoch, pin): (String, i64, Option<String>) = self.store.db.query_row(
            "SELECT h.device_id,h.epoch,d.public_key_ref FROM home_authority h JOIN devices d ON d.id=h.device_id WHERE h.singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if id == self.origin {
            return Ok(None);
        }
        Ok(Some((id, epoch, pin.ok_or(Error::InvalidTransition)?)))
    }
    pub fn claim_home_delivery(
        &mut self,
        now: i64,
        policy: DispatchPolicy,
    ) -> Result<Option<DispatchJob>> {
        if self.paired_home()?.is_none() {
            return Ok(None);
        }
        self.store.claim_next(now, policy)
    }
    pub fn complete_home_delivery(
        &mut self,
        job: &DispatchJob,
        result: TransportResult,
        now: i64,
        policy: DispatchPolicy,
    ) -> Result<()> {
        let (id, epoch, _) = self.paired_home()?.ok_or(Error::InvalidTransition)?;
        if job.intent.home() != (&id, epoch) {
            return Err(Error::IdentityConflict);
        }
        self.store.complete_job(job, result, now, policy)
    }
}

impl LocalApp {
    /// Native adapter must authenticate the pinned Home before applying this
    /// projection. It is deliberately NOT a renderer command.
    pub fn apply_authenticated_home_result(
        &mut self,
        request: &crate::outbox::AppendMessage,
        result: &crate::home_result::HomeResult,
        now: i64,
    ) -> Result<()> {
        let (home, epoch, _) = self.paired_home()?.ok_or(Error::InvalidTransition)?;
        if request.home_device_id != home
            || request.home_epoch != epoch
            || request.origin_device_id != self.origin
        {
            return Err(Error::IdentityConflict);
        }
        if self.ensure_active_conversation(&request.session_id).is_err() {
            return Err(Error::IdentityConflict);
        }
        let hash = crate::outbox::digest(&serde_json::to_vec(request)?);
        let receipt = result.receipt.as_ref().ok_or(Error::InvalidTransition)?;
        if !receipt.accepted
            || receipt.home_device_id != home
            || receipt.home_epoch != epoch
            || receipt.request_id != request.request_id
            || receipt.payload_sha256 != hash
        {
            return Err(Error::IdentityConflict);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM intent_outbox WHERE request_id=?1 AND message_id=?2 AND session_id=?3 AND payload_sha256=?4 AND send_state='peer_accepted')",params![request.request_id,request.message_id,request.session_id,hash],|r|r.get(0))?;
        if !valid {
            return Err(Error::InvalidTransition);
        }
        let Some(e) = result.execution.as_ref() else {
            return Ok(());
        };
        let terminal = |s: &str| matches!(s, "completed" | "failed" | "cancelled");
        if e.execution_id.is_empty()
            || e.version < 1
            || !matches!(
                e.status.as_str(),
                "pending"
                    | "dispatching"
                    | "running"
                    | "paused"
                    | "unknown"
                    | "completed"
                    | "failed"
                    | "cancelled"
            )
            || (e.stop_confirmed && (!e.stop_requested || !terminal(&e.status)))
            || (e.reply.is_some() && e.status != "completed")
        {
            return Err(Error::InvalidTransition);
        }
        let serialized = serde_json::to_string(e)?;
        let old: Option<(i64, String)> = tx.query_row("SELECT object_version,payload_json FROM domain_events WHERE object_kind='remote_execution' AND request_id=?1 ORDER BY object_version DESC LIMIT 1",[&request.request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((version, json)) = old {
            let prior: crate::home_result::HomeExecutionResult = serde_json::from_str(&json)?;
            if prior.execution_id != e.execution_id {
                return Err(Error::IdentityConflict);
            }
            if e.version < version {
                return Ok(());
            }
            if e.version == version {
                return if json == serialized {
                    Ok(())
                } else {
                    Err(Error::IdentityConflict)
                };
            }
            if terminal(&prior.status)
                || (prior.stop_requested && !e.stop_requested)
                || (prior.stop_confirmed && !e.stop_confirmed)
            {
                return Err(Error::InvalidTransition);
            }
        }
        if let Some(reply) = &e.reply {
            if reply.message_id.is_empty()
                || reply.text.trim().is_empty()
                || reply.text.len() > 192 * 1024
            {
                return Err(Error::InvalidTransition);
            }
            let content = serde_json::json!({"text":reply.text,"reply_to":request.message_id,"execution_id":e.execution_id,"source":"home_codex_execution"}).to_string();
            tx.execute("INSERT INTO messages(id,conversation_id,origin_device_id,request_id,role,content_json,acceptance,created_at) VALUES(?1,?2,?3,NULL,'assistant',?4,'home',?5)",params![format!("home-reply:{}",crate::outbox::digest(format!("{home}:{}",reply.message_id).as_bytes())),request.session_id,self.origin,content,reply.created_at])?;
        }
        tx.execute("INSERT INTO domain_events(event_id,authority_epoch,object_kind,object_id,object_version,request_id,source_device_id,type,payload_json,created_at) VALUES(?1,?2,'remote_execution',?3,?4,?5,?6,'home_execution_observed',?7,?8)",params![format!("home-result:{}:{}",e.execution_id,e.version),epoch,e.execution_id,e.version,request.request_id,home,serialized,now])?;
        tx.commit()?;
        Ok(())
    }
}

impl LocalApp {
    /// Select one request across the complete pending set across all active
    /// conversations. A bounded first page or conversation switch must never
    /// strand in-flight requests behind another conversation.
    pub fn pending_home_result_at(
        &self,
        index: usize,
    ) -> Result<Option<crate::outbox::AppendMessage>> {
        let Some((home, epoch, _)) = self.paired_home()? else {
            return Ok(None);
        };
        let row: Option<(String, String, String, String, String)> = self
            .store
            .db
            .query_row(
                "WITH eligible AS (SELECT o.request_id,o.message_id,o.session_id,o.origin_device_id,o.payload_json,o.created_at FROM intent_outbox o WHERE o.kind='append_message' AND o.send_state='peer_accepted' AND o.target_device_id=?1 AND o.home_epoch=?2 AND o.origin_device_id=?3 AND NOT EXISTS(SELECT 1 FROM domain_events e WHERE e.object_kind='remote_execution' AND e.request_id=o.request_id AND json_extract(e.payload_json,'$.status') IN ('completed','failed','cancelled'))) SELECT request_id,message_id,session_id,origin_device_id,payload_json FROM eligible ORDER BY created_at,request_id LIMIT 1 OFFSET (?4 % MAX((SELECT COUNT(*) FROM eligible),1))",
                params![home, epoch, self.origin, (index % i64::MAX as usize) as i64],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        match row {
            Some((request_id, message_id, session_id, origin_device_id, payload)) => {
                Ok(Some(crate::outbox::AppendMessage {
                    request_id,
                    message_id,
                    session_id,
                    origin_device_id,
                    home_device_id: home,
                    home_epoch: epoch,
                    content: serde_json::from_str(&payload)?,
                }))
            }
            None => Ok(None),
        }
    }
}

/// Read-only projection for the same conversation UI; no local execution is
/// fabricated for work owned by Home. Local stop intent is shown separately.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RemoteExecutionView {
    pub id: String,
    pub request_id: String,
    pub message_id: String,
    pub status: String,
    pub stop_requested: bool,
    pub stop_confirmed: bool,
    pub stop_pending: bool,
    pub cancel_rejected: bool,
    pub can_start: bool,
}
impl LocalApp {
    pub fn remote_executions(&self, session: &str) -> Result<Vec<RemoteExecutionView>> {
        self.ensure_active_conversation(session)?;
        let Some((home, epoch, _)) = self.paired_home()? else {
            return Ok(vec![]);
        };
        let mut q=self.store.db.prepare("SELECT e.payload_json,o.request_id,o.message_id,COALESCE((SELECT c.send_state FROM intent_outbox c WHERE c.kind='cancel_execution' AND json_extract(c.payload_json,'$.execution_id')=e.object_id AND c.origin_device_id=o.origin_device_id AND c.session_id=o.session_id AND c.target_device_id=o.target_device_id AND c.home_epoch=o.home_epoch ORDER BY CAST(json_extract(c.payload_json,'$.expected_version') AS INTEGER) DESC,c.created_at DESC,c.request_id DESC LIMIT 1) NOT IN ('rejected','cancelled_local','needs_attention'),0),COALESCE((SELECT c.send_state FROM intent_outbox c WHERE c.kind='cancel_execution' AND json_extract(c.payload_json,'$.execution_id')=e.object_id AND c.origin_device_id=o.origin_device_id AND c.session_id=o.session_id AND c.target_device_id=o.target_device_id AND c.home_epoch=o.home_epoch ORDER BY CAST(json_extract(c.payload_json,'$.expected_version') AS INTEGER) DESC,c.created_at DESC,c.request_id DESC LIMIT 1) IN ('rejected','needs_attention'),0) FROM domain_events e JOIN intent_outbox o ON o.request_id=e.request_id WHERE e.object_kind='remote_execution' AND o.session_id=?1 AND o.origin_device_id=?2 AND o.target_device_id=?3 AND o.home_epoch=?4 AND NOT EXISTS(SELECT 1 FROM domain_events newer WHERE newer.object_kind='remote_execution' AND newer.request_id=e.request_id AND newer.object_version>e.object_version) ORDER BY o.created_at")?;
        let rows = q.query_map(params![session, self.origin, home, epoch], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, bool>(4)?,
            ))
        })?;
        let mut out = vec![];
        for row in rows {
            let (json, request_id, message_id, pending, rejected) = row?;
            let e: crate::home_result::HomeExecutionResult = serde_json::from_str(&json)?;
            let terminal = matches!(e.status.as_str(), "completed" | "failed" | "cancelled");
            out.push(RemoteExecutionView {
                id: e.execution_id,
                request_id,
                message_id,
                status: e.status,
                stop_requested: e.stop_requested,
                stop_confirmed: e.stop_confirmed,
                stop_pending: pending && !e.stop_requested && !terminal,
                cancel_rejected: rejected && !e.stop_requested && !terminal,
                can_start: false,
            });
        }
        Ok(out)
    }
    /// Creates only a durable control intent. Home stop confirmation arrives later.
    pub fn request_remote_stop(&mut self, execution: &str, now: i64) -> Result<RemoteExecutionView> {
        let session_id: String = self
            .store
            .db
            .query_row(
                "SELECT o.session_id FROM domain_events e JOIN intent_outbox o ON o.request_id=e.request_id WHERE e.object_kind='remote_execution' AND e.object_id=?1 AND o.origin_device_id=?2 ORDER BY e.object_version DESC LIMIT 1",
                params![execution, self.origin],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::InvalidTransition)?;
        let mut view = self
            .remote_executions(&session_id)?
            .into_iter()
            .find(|e| e.id == execution)
            .ok_or(Error::InvalidTransition)?;
        if view.stop_pending || view.stop_requested {
            return Ok(view);
        }
        if !matches!(
            view.status.as_str(),
            "pending" | "dispatching" | "running" | "paused" | "unknown"
        ) {
            return Err(Error::InvalidTransition);
        }
        let version: i64 = self.store.db.query_row(
            "SELECT max(object_version) FROM domain_events WHERE object_kind='remote_execution' AND object_id=?1",
            [execution],
            |r| r.get(0),
        )?;
        let (home, epoch, _) = self.paired_home()?.ok_or(Error::InvalidTransition)?;
        let request = crate::dispatch::CancelExecution {
            request_id: format!(
                "cancel:{}:{}",
                crate::outbox::digest(execution.as_bytes()),
                version
            ),
            session_id: session_id.clone(),
            origin_device_id: self.origin.clone(),
            home_device_id: home,
            home_epoch: epoch,
            execution_id: execution.into(),
            expected_version: version,
            permission_ref: format!("paired-conversation:{session_id}"),
        };
        self.store.queue_cancel(&request, now)?;
        view = self
            .remote_executions(&session_id)?
            .into_iter()
            .find(|e| e.id == execution)
            .ok_or(Error::InvalidTransition)?;
        Ok(view)
    }
}
