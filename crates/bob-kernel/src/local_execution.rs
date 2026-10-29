//! Local official-CLI lifecycle. Uses existing domain tables; never forges a
//! remote Home receipt or a paired device. The native owner must enforce the
//! read-only process policy and supply confirmed cleanup, not just parent exit.
use crate::{
    app::LocalApp,
    harness_reply::ReplyStream,
    outbox::{digest, Error, Result},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::json;

const RUNNER: &str = "local-codex-v1";
#[derive(Debug, Clone, Serialize)]
pub struct LocalExecution {
    pub id: String,
    pub request_id: String,
    pub message_id: String,
    pub status: String,
    pub prompt: String,
    pub stop_requested: bool,
    pub error: Option<String>,
    pub owner_job: Option<String>,
    /// True once, only to the caller that committed the initial dispatch claim.
    /// Listing or re-preparing a request always returns false.
    pub can_start: bool,
}

fn executions(db: &Connection, session: &str, origin: &str) -> Result<Vec<LocalExecution>> {
    let mut q=db.prepare("SELECT e.id,e.request_id,m.id,e.status,m.content_json,e.stop_requested_at IS NOT NULL,d.last_error,e.resolved_json FROM executions e JOIN tasks t ON t.id=e.task_id JOIN messages m ON m.id=t.source_message_id JOIN runner_dispatches d ON d.execution_id=e.id WHERE m.conversation_id=?1 AND m.origin_device_id=?2 AND d.runner_instance_id=?3 ORDER BY e.rowid")?;
    let rows = q.query_map(params![session, origin, RUNNER], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, bool>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (id, request_id, message_id, status, content, stop_requested, error, resolved) = row?;
        let resolved: serde_json::Value = match resolved {
            Some(s) => serde_json::from_str(&s)?,
            None => serde_json::Value::Null,
        };
        let owner_job = if resolved["owner_policy"] == "windows-job-gated-v1" {
            resolved["job_name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        } else {
            None
        };
        let content: serde_json::Value = serde_json::from_str(&content)?;
        let prompt = content["text"]
            .as_str()
            .ok_or(Error::InvalidTransition)?
            .to_owned();
        result.push(LocalExecution {
            id,
            request_id,
            message_id,
            status,
            prompt,
            stop_requested,
            error,
            owner_job,
            can_start: false,
        });
    }
    Ok(result)
}

impl LocalApp {
    pub fn local_executions(&self, session_id: &str) -> Result<Vec<LocalExecution>> {
        if session_id != self.session {
            return Err(Error::InvalidTransition);
        }
        executions(&self.store.db, &self.session, &self.origin)
    }

    /// Must be called before spawning or releasing a suspended child. A lost
    /// response is reconciled by request id, not by launching the command again.
    pub fn prepare_local_execution(
        &mut self,
        request_id: &str,
        workspace_ref: &str,
        now: i64,
    ) -> Result<LocalExecution> {
        if workspace_ref.trim().is_empty() || workspace_ref.len() > 8192 {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(prior) = executions(&tx, &self.session, &self.origin)?
            .into_iter()
            .find(|e| e.request_id == request_id)
        {
            tx.commit()?;
            return Ok(prior);
        }
        let authority: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM home_authority WHERE device_id=?1)",
            [&self.origin],
            |r| r.get(0),
        )?;
        let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM executions e JOIN runner_dispatches d ON d.execution_id=e.id WHERE d.runner_instance_id=?1 AND e.status IN ('pending','dispatching','running','paused','unknown'))",[RUNNER],|r|r.get(0))?;
        if !authority || blocked {
            return Err(Error::InvalidTransition);
        }
        let source:Option<(String,String,Option<String>)>=tx.query_row("SELECT m.id,m.content_json,c.project_id FROM messages m JOIN intent_outbox o ON o.request_id=m.request_id JOIN conversations c ON c.id=m.conversation_id WHERE m.request_id=?1 AND m.conversation_id=?2 AND m.origin_device_id=?3 AND m.role='user' AND o.target_device_id=?3 AND o.send_state='local_saved' AND c.archived_at IS NULL",params![request_id,self.session,self.origin],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (message_id, content, existing_project) = source.ok_or(Error::InvalidTransition)?;
        let content: serde_json::Value = serde_json::from_str(&content)?;
        let prompt = content["text"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or(Error::InvalidTransition)?
            .to_owned();
        let key = digest(request_id.as_bytes());
        let id = format!("local-execution:{key}");
        let task = format!("local-task:{key}");
        let owner_job = format!("Local\\BobReadOnly-{}", id.replace(':', "_"));
        let owner = json!({"job_name":owner_job,"owner_policy":"windows-job-gated-v1"}).to_string();
        let project = existing_project.unwrap_or_else(|| format!("local-project:{}", self.session));
        let workspace = format!(
            "local-workspace:{}",
            digest(format!("{}:{workspace_ref}", self.origin).as_bytes())
        );
        tx.execute("INSERT OR IGNORE INTO projects VALUES(?1,'local conversation','local','active',1,?2,?2)",params![project,now])?;
        tx.execute(
            "INSERT OR IGNORE INTO workspaces VALUES(?1,?2,?3,'codex-read-only-temp-v1',1)",
            params![workspace, self.origin, workspace_ref],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO project_workspaces VALUES(?1,?2)",
            params![project, workspace],
        )?;
        // The conversation remains local until a real Home protocol says otherwise.
        tx.execute(
            "UPDATE conversations SET project_id=?2 WHERE id=?1 AND project_id IS NULL",
            params![self.session, project],
        )?;
        tx.execute("INSERT INTO tasks(id,project_id,source_message_id,title,status,constraints_json,version,created_at,updated_at) VALUES(?1,?2,?3,'local read-only reply','queued',?4,1,?5,?5)",params![task,project,message_id,json!({"read_only":true,"ephemeral_workspace":true}).to_string(),now])?;
        let contract=json!({"kind":"local_codex_reply","source_message_id":message_id,"prompt_sha256":digest(prompt.as_bytes()),"workspace_ref":workspace_ref,"read_only":true,"automatic_retry":false}).to_string();
        tx.execute("INSERT INTO executions(id,project_id,task_id,workspace_id,request_id,status,requested_json,contract_json,contract_sha256,version) VALUES(?1,?2,?3,?4,?5,'dispatching',?6,?7,?8,1)",params![id,project,task,workspace,request_id,json!({"harness":"codex","model":"cli_default","sandbox":"read-only"}).to_string(),contract,digest(contract.as_bytes())])?;
        tx.execute(
            "UPDATE executions SET resolved_json=?2 WHERE id=?1",
            params![id, owner],
        )?;
        tx.execute(
            "INSERT INTO runner_dispatches VALUES(?1,?2,?3,'dispatching',NULL,NULL,?4)",
            params![id, request_id, RUNNER, now],
        )?;
        // Reserve the source intent for this local execution. This is not a
        // peer ACK: generic transport must not later submit the same intent.
        tx.execute("UPDATE intent_outbox SET send_state='needs_attention',last_error='LOCAL_EXECUTION_OWNED' WHERE request_id=?1",[request_id])?;
        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM home_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        tx.execute("INSERT INTO workspace_leases VALUES(?1,?2,?3,1,'held',?4) ON CONFLICT(workspace_id) DO UPDATE SET execution_id=excluded.execution_id,home_epoch=excluded.home_epoch,fencing_token=workspace_leases.fencing_token+1,state='held',updated_at=excluded.updated_at WHERE workspace_leases.state='released'",params![workspace,id,epoch,now])?;
        let held:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM workspace_leases WHERE workspace_id=?1 AND execution_id=?2 AND state='held')",params![workspace,id],|r|r.get(0))?;
        if !held {
            return Err(Error::InvalidTransition);
        }
        tx.commit()?;
        Ok(LocalExecution {
            id,
            request_id: request_id.into(),
            message_id,
            status: "dispatching".into(),
            prompt,
            stop_requested: false,
            error: None,
            owner_job: Some(owner_job),
            can_start: true,
        })
    }

    /// Persist the native ownership identity before releasing the worker gate.
    /// Caller has already created this named kill-on-close Job and assigned the
    /// gated worker. Job creation/assignment failure must never release GO.
    pub fn record_local_owner(&mut self, id: &str, job_name: &str, now: i64) -> Result<()> {
        if job_name.is_empty() || job_name.len() > 256 || job_name.contains('\0') {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let local = executions(&tx, &self.session, &self.origin)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(Error::InvalidTransition)?;
        let value = json!({"job_name":job_name,"owner_policy":"windows-job-gated-v1"}).to_string();
        let old: Option<String> = tx.query_row(
            "SELECT resolved_json FROM executions WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if let Some(old) = old {
            if old != value {
                return Err(Error::IdentityConflict);
            }
            tx.commit()?;
            return Ok(());
        }
        if local.status != "dispatching" {
            return Err(Error::InvalidTransition);
        }
        tx.execute(
            "UPDATE executions SET resolved_json=?2,version=version+1 WHERE id=?1",
            params![id, value],
        )?;
        tx.execute(
            "UPDATE runner_dispatches SET updated_at=?2 WHERE execution_id=?1",
            params![id, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Native-only recovery acknowledgement, not exposed to renderer input.
    /// `cleanup_confirmed` must result from querying this exact recorded Job:
    /// ERROR_FILE_NOT_FOUND or confirmed zero active processes. Access failure,
    /// parent PID absence, or a timeout are not confirmation.
    pub fn reconcile_local_execution(
        &mut self,
        id: &str,
        cleanup_confirmed: bool,
        now: i64,
    ) -> Result<()> {
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let local = executions(&tx, &self.session, &self.origin)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(Error::InvalidTransition)?;
        let owner = local.owner_job.ok_or(Error::InvalidTransition)?;
        if local.status == "failed"
            && local.error.as_deref() == Some("restart_interrupted")
            && cleanup_confirmed
        {
            tx.commit()?;
            return Ok(());
        }
        if local.status != "unknown" {
            return Err(Error::InvalidTransition);
        }
        if !cleanup_confirmed {
            tx.commit()?;
            return Ok(());
        }
        let previous: Option<String> = tx.query_row(
            "SELECT observed_evidence_ref FROM executions WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        let receipt=json!({"owner_job":owner,"owner_policy":"windows-job-gated-v1","cleanup_confirmed":true,"basis":"named_job_absent_or_empty","outcome":"lost_result","automatic_retry":false,"previous_evidence_ref":previous}).to_string();
        let evidence = format!("local-recovery:{id}");
        tx.execute("INSERT INTO evidence VALUES(?1,'local_owner_reconciliation',?2,?3,?4,'Named job absent/empty; lost result, not successful execution; no automatic replay')",params![evidence,id,digest(receipt.as_bytes()),now])?;
        tx.execute(
            "INSERT INTO evidence_links VALUES(?1,'execution',?2,'owner_cleanup_confirmation')",
            params![evidence, id],
        )?;
        tx.execute("UPDATE executions SET status='failed',observed_json=?2,observed_evidence_ref=?3,ended_at=?4,stop_confirmed_at=CASE WHEN stop_requested_at IS NOT NULL THEN ?4 ELSE NULL END,version=version+1 WHERE id=?1",params![id,receipt,evidence,now])?;
        tx.execute("UPDATE runner_dispatches SET state='terminal',last_error='restart_interrupted',native_receipt_ref=?2,updated_at=?3 WHERE execution_id=?1",params![id,evidence,now])?;
        tx.execute(
            "UPDATE workspace_leases SET state='released',updated_at=?2 WHERE execution_id=?1",
            params![id, now],
        )?;
        tx.execute("UPDATE tasks SET status='blocked',updated_at=?2,version=version+1 WHERE id=(SELECT task_id FROM executions WHERE id=?1)",params![id,now])?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_local_started(&mut self, id: &str, now: i64) -> Result<()> {
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let local = executions(&tx, &self.session, &self.origin)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(Error::InvalidTransition)?;
        if local.status != "dispatching" || local.stop_requested || local.owner_job.is_none() {
            return Err(Error::InvalidTransition);
        }
        tx.execute(
            "UPDATE executions SET status='running',started_at=?2,version=version+1 WHERE id=?1",
            params![id, now],
        )?;
        tx.execute(
            "UPDATE runner_dispatches SET state='accepted',updated_at=?2 WHERE execution_id=?1",
            params![id, now],
        )?;
        tx.execute("UPDATE tasks SET status='running',version=version+1,updated_at=?2 WHERE id=(SELECT task_id FROM executions WHERE id=?1)",params![id,now])?;
        tx.commit()?;
        Ok(())
    }

    pub fn request_local_stop(&mut self, id: &str, now: i64) -> Result<()> {
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let local = executions(&tx, &self.session, &self.origin)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(Error::InvalidTransition)?;
        if !matches!(local.status.as_str(), "dispatching" | "running" | "unknown") {
            return Err(Error::InvalidTransition);
        }
        if !local.stop_requested {
            tx.execute(
                "UPDATE executions SET stop_requested_at=?2,version=version+1 WHERE id=?1",
                params![id, now],
            )?;
            tx.execute("UPDATE workspace_leases SET state='stop_pending',updated_at=?2 WHERE execution_id=?1",params![id,now])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Called by the trusted native owner, never a renderer-supplied transcript.
    /// No late callback may turn an already cancelled/failed/unknown run into success.
    pub fn finish_local_execution(
        &mut self,
        id: &str,
        stream: &str,
        exit: Option<i32>,
        reason: Option<&str>,
        cleanup_confirmed: bool,
        now: i64,
    ) -> Result<()> {
        self.finish_local_execution_inner(id, stream, exit, reason, cleanup_confirmed, now, false)
    }

    /// Restore a private durable result spool after the native adapter has
    /// verified the exact named Job is absent or empty. Never a renderer API.
    /// Requires the recorded gated-job policy and does not start any process.
    pub fn restore_local_completion(
        &mut self,
        id: &str,
        stream: &str,
        exit: Option<i32>,
        reason: Option<&str>,
        now: i64,
    ) -> Result<()> {
        self.finish_local_execution_inner(id, stream, exit, reason, true, now, true)
    }

    fn finish_local_execution_inner(
        &mut self,
        id: &str,
        stream: &str,
        exit: Option<i32>,
        reason: Option<&str>,
        cleanup_confirmed: bool,
        now: i64,
        restoring: bool,
    ) -> Result<()> {
        let reason = reason.map(|r| match r {
            "timeout"
            | "cancelled"
            | "spawn_failed"
            | "io_failed"
            | "output_limit"
            | "invalid_reply"
            | "cleanup_unconfirmed" => r,
            _ => "runner_failed",
        });
        let mut parser = ReplyStream::default();
        let parse_ok = stream
            .lines()
            .filter(|s| !s.trim().is_empty())
            .all(|s| parser.push(s).is_ok());
        let reply = if parse_ok {
            parser.finish(exit).ok()
        } else {
            None
        };
        let stream_hash = digest(stream.as_bytes());
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let local = executions(&tx, &self.session, &self.origin)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(Error::InvalidTransition)?;
        if restoring && local.owner_job.is_none() {
            return Err(Error::InvalidTransition);
        }
        let receipt=json!({"exit_code":exit,"reason":reason,"cleanup_confirmed":cleanup_confirmed,"stream_sha256":stream_hash}).to_string();
        if matches!(local.status.as_str(), "completed" | "failed" | "cancelled") {
            let old: Option<String> = tx.query_row(
                "SELECT observed_json FROM executions WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            if old.as_deref() != Some(&receipt) {
                return Err(Error::IdentityConflict);
            }
            tx.commit()?;
            return Ok(());
        }
        if !matches!(local.status.as_str(), "dispatching" | "running")
            && !(restoring && local.status == "unknown")
        {
            return Err(Error::InvalidTransition);
        }
        let (status, error) = if !cleanup_confirmed {
            ("unknown", Some("cleanup_unconfirmed"))
        } else if local.stop_requested || reason == Some("cancelled") {
            ("cancelled", Some("cancelled"))
        } else if reason.is_some() {
            ("failed", reason)
        } else if reply.is_none() {
            ("failed", Some("invalid_reply"))
        } else {
            ("completed", None)
        };
        let evidence_id = format!("local-result:{id}:{}", digest(receipt.as_bytes()));
        tx.execute("INSERT INTO evidence VALUES(?1,'local_codex_result',?2,?3,?4,'CLI exit and reply; no supplier model identity or user acceptance')",params![evidence_id,id,stream_hash,now])?;
        tx.execute(
            "INSERT INTO evidence_links VALUES(?1,'execution',?2,'runner_result')",
            params![evidence_id, id],
        )?;
        if status == "completed" {
            let reply = reply.unwrap();
            let binding = format!("native:{id}");
            let message = format!("reply:{id}");
            tx.execute("INSERT INTO native_sessions(id,conversation_id,device_id,workspace_id,service_instance_id,harness,native_session_id,capability_json) SELECT ?1,?2,?3,workspace_id,?4,'codex',?5,?6 FROM executions WHERE id=?7",params![binding,self.session,self.origin,RUNNER,reply.native_session_id,json!({"ephemeral":true,"read_only":true}).to_string(),id])?;
            tx.execute("INSERT INTO messages VALUES(?1,?2,?3,NULL,'assistant',?4,'local',?5)",params![message,self.session,self.origin,json!({"text":reply.text,"reply_to":local.message_id,"execution_id":id,"source":"codex_execution","native_session_id":reply.native_session_id,"evidence_id":evidence_id}).to_string(),now])?;
            tx.execute(
                "INSERT INTO evidence_links VALUES(?1,'message',?2,'reply_source')",
                params![evidence_id, message],
            )?;
            tx.execute(
                "UPDATE executions SET native_session_binding_id=?2 WHERE id=?1",
                params![id, binding],
            )?;
        }
        // Cancellation is a durable request until confirmed cleanup. A timeout is
        // failure, not cancellation, unless the user had also requested stopping.
        tx.execute("UPDATE executions SET status=?2,observed_json=?3,observed_evidence_ref=?4,ended_at=CASE WHEN ?5 THEN ?6 ELSE NULL END,stop_requested_at=CASE WHEN ?2='cancelled' THEN coalesce(stop_requested_at,?6) ELSE stop_requested_at END,stop_confirmed_at=CASE WHEN ?2='cancelled' AND ?5 THEN ?6 ELSE NULL END,version=version+1 WHERE id=?1",params![id,status,receipt,evidence_id,cleanup_confirmed,now])?;
        tx.execute("UPDATE runner_dispatches SET state=?2,last_error=?3,native_receipt_ref=?4,updated_at=?5 WHERE execution_id=?1",params![id,if status=="unknown"{"unknown"}else{"terminal"},error,evidence_id,now])?;
        tx.execute(
            "UPDATE workspace_leases SET state=?2,updated_at=?3 WHERE execution_id=?1",
            params![
                id,
                if cleanup_confirmed {
                    "released"
                } else {
                    "unknown"
                },
                now
            ],
        )?;
        tx.execute("UPDATE tasks SET status=?2,updated_at=?3,version=version+1 WHERE id=(SELECT task_id FROM executions WHERE id=?1)",params![id,match status {"completed"=>"awaiting_review","cancelled"=>"cancelled",_=>"blocked"},now])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn recover_local_executions(&mut self, now: i64) -> Result<()> {
        // Store has an exclusive lifetime lock: this is recovery, never a second
        // writer deciding a still-owned live execution has ended.
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE executions SET status='unknown',version=version+1 WHERE id IN(SELECT execution_id FROM runner_dispatches WHERE runner_instance_id=?1) AND status IN('pending','dispatching','running','paused')",[RUNNER])?;
        tx.execute("UPDATE runner_dispatches SET state='unknown',last_error=coalesce(last_error,'restart_unconfirmed'),updated_at=?2 WHERE runner_instance_id=?1 AND execution_id IN(SELECT id FROM executions WHERE status='unknown')",params![RUNNER,now])?;
        tx.execute("UPDATE workspace_leases SET state='unknown',updated_at=?2 WHERE execution_id IN(SELECT e.id FROM executions e JOIN runner_dispatches d ON d.execution_id=e.id WHERE d.runner_instance_id=?1 AND e.status='unknown')",params![RUNNER,now])?;
        tx.execute("UPDATE tasks SET status='blocked',updated_at=?2 WHERE id IN(SELECT e.task_id FROM executions e JOIN runner_dispatches d ON d.execution_id=e.id WHERE d.runner_instance_id=?1 AND e.status='unknown')",params![RUNNER,now])?;
        tx.commit()?;
        Ok(())
    }
}
