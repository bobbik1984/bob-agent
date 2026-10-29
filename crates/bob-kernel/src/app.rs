//! Thin desktop/mobile command facade. It only promises local durable storage.
//! No credentials, network endpoint, or model capability is implied by opening it.
use crate::outbox::{digest, AppendMessage, Error, LocalAccepted, Result, Store};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
const IDENTITY_SCHEMA: &str = include_str!("../migrations/0002.sql");
pub struct LocalApp {
    pub(crate) store: Store,
    pub(crate) origin: String,
    pub(crate) session: String,
}
#[derive(Debug, Serialize)]
pub struct AppMessage {
    pub id: String,
    pub content: String,
    pub send_state: String,
    pub role: String,
    pub reply_to: Option<String>,
    pub request_id: Option<String>,
    pub source: String,
}
#[derive(Debug, Serialize)]
pub struct AppReadiness {
    pub mode: &'static str,
    pub execution_available: bool,
    pub saved_messages: i64,
    pub pending_messages: i64,
}
impl LocalApp {
    pub fn open(path: &Path) -> Result<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::InvalidTransition)?
            .as_millis()
            .min(i64::MAX as u128) as i64;
        let mut store = Store::open(path, now)?;
        let tx = store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let checksum = digest(IDENTITY_SCHEMA.replace("\r\n", "\n").as_bytes());
        let old: Option<String> = tx
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version=2",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            if old != checksum {
                return Err(Error::SchemaMismatch);
            }
        } else {
            tx.execute_batch(IDENTITY_SCHEMA)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(2,?1,?2)",
                params![checksum, now],
            )?;
        }
        let identity:Option<(String,String)>=tx.query_row("SELECT origin_device_id,default_session_id FROM app_local_identity WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (origin, session) = if let Some(ids) = identity {
            ids
        } else {
            let origin: String =
                tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
            let session: String =
                tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
            tx.execute("INSERT INTO devices(id,label,trust_state,created_at) VALUES(?1,'local','unpaired',?2)",params![origin,now])?;
            // Local-only initial authority. This is not device pairing or remote readiness.
            tx.execute(
                "INSERT OR IGNORE INTO home_authority VALUES(1,?1,1)",
                [&origin],
            )?;
            tx.execute(
                "INSERT INTO app_local_identity VALUES(1,?1,?2)",
                params![origin, session],
            )?;
            (origin, session)
        };
        tx.commit()?;
        let mut app = Self { store, origin, session };
        app.recover_local_executions(now)?;
        Ok(app)
    }
    pub fn default_session(&self) -> Result<String> {
        Ok(self.session.clone())
    }
    pub fn accept(
        &mut self,
        request_id: &str,
        message_id: &str,
        session_id: &str,
        content: String,
        now: i64,
    ) -> Result<LocalAccepted> {
        if session_id != self.session || content.trim().is_empty() || content.len() > 256 * 1024 {
            return Err(Error::InvalidTransition);
        }
        let (home, epoch) = self.store.db.query_row(
            "SELECT device_id,epoch FROM home_authority WHERE singleton=1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )?;
        self.store.accept_message(
            &AppendMessage {
                request_id: request_id.into(),
                message_id: message_id.into(),
                session_id: session_id.into(),
                origin_device_id: self.origin.clone(),
                home_device_id: home,
                home_epoch: epoch,
                content: serde_json::json!({"text":content}),
            },
            now,
        )
    }
    pub fn history(&self, session_id: &str) -> Result<Vec<AppMessage>> {
        if session_id != self.session {
            return Err(Error::InvalidTransition);
        }
        let mut query=self.store.db.prepare("SELECT m.id,m.content_json,coalesce((SELECT 'execution_' || e.status FROM executions e JOIN runner_dispatches d ON d.execution_id=e.id WHERE e.request_id=m.request_id AND d.runner_instance_id='local-codex-v1'),o.send_state,'reply_saved'),m.role,m.request_id FROM messages m LEFT JOIN intent_outbox o ON o.request_id=m.request_id WHERE m.conversation_id=?1 AND m.origin_device_id=?2 ORDER BY m.created_at,m.rowid")?;
        let rows = query.query_map(params![session_id, self.origin], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, json, send_state, role, request_id) = row?;
            let parsed: serde_json::Value = serde_json::from_str(&json)?;
            let content = parsed
                .get("text")
                .and_then(|x| x.as_str())
                .ok_or(Error::InvalidTransition)?
                .to_string();
            result.push(AppMessage {
                id,
                content,
                send_state,
                role,
                request_id,
                source: parsed.get("source").and_then(|v|v.as_str()).unwrap_or("local").to_owned(),
                reply_to: parsed.get("reply_to").and_then(|v|v.as_str()).map(str::to_owned),
            });
        }
        Ok(result)
    }
    /// Counts come from durable records, not optimistic UI or inferred network state.
    pub fn readiness(&self) -> Result<AppReadiness> {
        let (saved, pending) = self.store.db.query_row(
            "SELECT count(*),coalesce(sum(CASE WHEN o.send_state IN ('local_saved','queued_for_wakeup','not_dispatched','dispatching','dispatched','unknown_send','needs_attention') AND NOT EXISTS(SELECT 1 FROM executions e JOIN runner_dispatches d ON d.execution_id=e.id WHERE e.request_id=m.request_id AND d.runner_instance_id='local-codex-v1') THEN 1 ELSE 0 END),0) FROM messages m JOIN intent_outbox o ON o.request_id=m.request_id WHERE m.conversation_id=?1 AND m.origin_device_id=?2",
            params![self.session,self.origin], |r| Ok((r.get(0)?,r.get(1)?)))?;
        // No execution adapter is installed in this facade yet. Persisted messages
        // must never be presented as scheduled or running just because they exist.
        Ok(AppReadiness { mode: "local_only", execution_available: false,
            saved_messages: saved, pending_messages: pending })
    }
    pub fn prewarm(&self) -> &'static str {
        "provider_unconfigured"
    }
}

impl LocalApp {
    /// Ingest a captured, externally executed CLI probe, NOT a Bob dispatch.
    /// Its source request must have been saved explicitly in this same session.
    /// The evidence type preserves that boundary; no task/receipt is fabricated.
    pub fn record_cli_probe(&mut self, request_id: &str, stream: &str, exit: Option<i32>, now: i64) -> Result<String> {
        let mut reducer=crate::harness_reply::ReplyStream::default();
        for line in stream.lines().filter(|line|!line.trim().is_empty()) {
            reducer.push(line).map_err(|_|Error::InvalidTransition)?;
        }
        let reply=reducer.finish(exit).map_err(|_|Error::InvalidTransition)?;
        let tx=self.store.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source:Option<String>=tx.query_row("SELECT m.id FROM messages m JOIN intent_outbox o ON o.request_id=m.request_id WHERE m.request_id=?1 AND m.conversation_id=?2 AND m.origin_device_id=?3 AND m.role='user' AND o.send_state='local_saved'",params![request_id,self.session,self.origin],|r|r.get(0)).optional()?;
        let source=source.ok_or(Error::InvalidTransition)?;
        let id=format!("cli-probe-reply:{}",digest(request_id.as_bytes()));
        let evidence_id=format!("evidence:{id}");
        let content=serde_json::to_string(&serde_json::json!({"text":reply.text,"reply_to":source,"native_session_id":reply.native_session_id,"evidence_id":evidence_id,"source":"external_cli_probe"}))?;
        let hash=digest(stream.as_bytes());
        let prior:Option<(String,String)>=tx.query_row("SELECT m.content_json,e.content_sha256 FROM messages m JOIN evidence e ON e.id=?2 WHERE m.id=?1",params![id,evidence_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((old,old_hash))=prior {
            if old!=content || old_hash!=hash {return Err(Error::IdentityConflict)}
            tx.commit()?; return Ok(id);
        }
        // A native session from one probe cannot be reassigned to another request.
        let reused:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM evidence WHERE source_kind='external_cli_probe' AND source_ref=?1)",[&reply.native_session_id],|r|r.get(0))?;
        if reused {return Err(Error::IdentityConflict)}
        tx.execute("INSERT INTO evidence VALUES(?1,'external_cli_probe',?2,?3,?4,'reply only; not Home delivery or Bob execution')",params![evidence_id,reply.native_session_id,hash,now])?;
        tx.execute("INSERT INTO messages VALUES(?1,?2,?3,NULL,'assistant',?4,'local',?5)",params![id,self.session,self.origin,content,now])?;
        tx.execute("INSERT INTO evidence_links VALUES(?1,'message',?2,'reply_source')",params![evidence_id,id])?;
        tx.execute("INSERT INTO evidence_links VALUES(?1,'message',?2,'probe_request')",params![evidence_id,source])?;
        tx.commit()?;
        Ok(id)
    }
}
