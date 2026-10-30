//! F1 Direct Model Provider (BYOK) configuration, execution lifecycle, and
//! paired-Replica offline direct execution convergence into Home authority.
//!
//! Plaintext API keys never enter SQLite, logs, or domain events; only a vault
//! slot reference (`api_key_slot`) and boolean `api_key_configured` are stored here.
use crate::{
    app::LocalApp,
    home_auth::{authorize, HomeMessageContext},
    local_execution::LocalExecution,
    outbox::{digest, AppendMessage, Error, HomeReceipt, Result, Store},
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;

const MIGRATION_0006: &str = include_str!("../migrations/0006.sql");
pub const DIRECT_RUNNER: &str = "direct-provider-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectProviderProfile {
    pub id: String,
    pub provider_kind: String,
    pub label: String,
    pub base_url: String,
    pub model_id: String,
    pub api_key_slot: String,
    pub api_key_configured: bool,
    pub enabled: bool,
    pub last_check_status: String,
    pub last_check_reason: String,
    pub last_checked_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectProviderProfileInput {
    pub id: String,
    pub provider_kind: String,
    pub label: String,
    pub base_url: String,
    pub model_id: String,
    pub api_key_slot: String,
    pub api_key_configured: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectChatTurn {
    pub message_id: String,
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectExecutionPrepared {
    pub execution_id: String,
    pub request_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub base_url: String,
    pub model_id: String,
    pub api_key_slot: String,
    pub messages: Vec<DirectChatTurn>,
    pub can_start: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectExecutionOutcome {
    Completed {
        text: String,
        provider_reported_model: Option<String>,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
    },
    Failed {
        reason: String,
    },
    Cancelled {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaDirectTurnPayload {
    pub sync_request_id: String,
    pub request_id: String,
    pub conversation_id: String,
    pub conversation_title: String,
    pub user_message_id: String,
    pub user_text: String,
    pub user_created_at: i64,
    pub execution_id: String,
    pub reply_message_id: String,
    pub reply_text: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub model_id: String,
    pub completed_at: i64,
    pub origin_device_id: String,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub payload_sha256: String,
}

pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let hash = digest(MIGRATION_0006.replace("\r\n", "\n").as_bytes());
    let previous: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=6",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match previous {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => {}
        None => {
            tx.execute_batch(MIGRATION_0006)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(6,?1,?2)",
                params![hash, now],
            )?;
        }
    }
    Ok(())
}

fn validate_url(url: &str) -> bool {
    let trimmed = url.trim();
    (trimmed.starts_with("https://") || trimmed.starts_with("http://127.0.0.1") || trimmed.starts_with("http://localhost"))
        && trimmed.len() <= 1024
        && !trimmed.chars().any(char::is_control)
}

impl LocalApp {
    pub fn upsert_direct_provider_profile(
        &mut self,
        input: &DirectProviderProfileInput,
        now: i64,
    ) -> Result<DirectProviderProfile> {
        let id = input.id.trim();
        let label = input.label.trim();
        let base_url = input.base_url.trim().trim_end_matches('/');
        let model_id = input.model_id.trim();
        let slot = input.api_key_slot.trim();
        if id.is_empty()
            || id.len() > 128
            || label.is_empty()
            || label.len() > 128
            || model_id.is_empty()
            || model_id.len() > 256
            || slot.is_empty()
            || slot.len() > 64
            || !matches!(
                input.provider_kind.as_str(),
                "openai_compatible" | "anthropic_messages" | "gemini_generate_content"
            )
            || !validate_url(base_url)
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO direct_provider_profiles(id,provider_kind,label,base_url,model_id,api_key_slot,api_key_configured,enabled,last_check_status,last_check_reason,last_checked_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'untested','',NULL,?9)
             ON CONFLICT(id) DO UPDATE SET
               provider_kind=excluded.provider_kind,
               label=excluded.label,
               base_url=excluded.base_url,
               model_id=excluded.model_id,
               api_key_slot=excluded.api_key_slot,
               api_key_configured=excluded.api_key_configured,
               enabled=excluded.enabled,
               updated_at=excluded.updated_at",
            params![
                id,
                input.provider_kind,
                label,
                base_url,
                model_id,
                slot,
                i64::from(input.api_key_configured),
                i64::from(input.enabled),
                now
            ],
        )?;
        tx.commit()?;
        self.direct_provider_profile_by_id(id)?
            .ok_or(Error::InvalidTransition)
    }

    pub fn direct_provider_profile(&self) -> Result<Option<DirectProviderProfile>> {
        self.direct_provider_profile_by_id("default")
    }

    pub fn direct_provider_profile_by_id(&self, id: &str) -> Result<Option<DirectProviderProfile>> {
        self.store
            .db
            .query_row(
                "SELECT id,provider_kind,label,base_url,model_id,api_key_slot,api_key_configured,enabled,last_check_status,last_check_reason,last_checked_at,updated_at
                 FROM direct_provider_profiles WHERE id=?1",
                [id],
                |r| {
                    Ok(DirectProviderProfile {
                        id: r.get(0)?,
                        provider_kind: r.get(1)?,
                        label: r.get(2)?,
                        base_url: r.get(3)?,
                        model_id: r.get(4)?,
                        api_key_slot: r.get(5)?,
                        api_key_configured: r.get::<_, i64>(6)? != 0,
                        enabled: r.get::<_, i64>(7)? != 0,
                        last_check_status: r.get(8)?,
                        last_check_reason: r.get(9)?,
                        last_checked_at: r.get(10)?,
                        updated_at: r.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn record_direct_provider_check(
        &mut self,
        id: &str,
        status: &str,
        reason: &str,
        now: i64,
    ) -> Result<DirectProviderProfile> {
        if !matches!(
            status,
            "untested"
                | "ok"
                | "auth_failed"
                | "network_error"
                | "rate_limited"
                | "model_not_found"
                | "invalid_config"
        ) || reason.len() > 512
        {
            return Err(Error::InvalidTransition);
        }
        let changed = self.store.db.execute(
            "UPDATE direct_provider_profiles SET last_check_status=?2,last_check_reason=?3,last_checked_at=?4,updated_at=?4 WHERE id=?1",
            params![id, status, reason.trim(), now],
        )?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        self.direct_provider_profile_by_id(id)?
            .ok_or(Error::InvalidTransition)
    }

    pub fn direct_provider_ready(&self) -> Result<bool> {
        Ok(self
            .direct_provider_profile()?
            .is_some_and(|p| p.enabled && p.api_key_configured))
    }

    pub fn set_conversation_execution_route(
        &mut self,
        conversation_id: &str,
        route: &str,
        now: i64,
    ) -> Result<String> {
        self.ensure_active_conversation(conversation_id)?;
        if !matches!(
            route,
            "auto" | "direct_byok" | "local_cli" | "remote_home" | "online_node"
        ) {
            return Err(Error::InvalidTransition);
        }
        self.store.db.execute(
            "INSERT INTO conversation_execution_routes(conversation_id,execution_route,updated_at)
             VALUES(?1,?2,?3)
             ON CONFLICT(conversation_id) DO UPDATE SET execution_route=excluded.execution_route,updated_at=excluded.updated_at",
            params![conversation_id, route, now],
        )?;
        Ok(route.to_owned())
    }

    pub fn conversation_execution_route(&self, conversation_id: &str) -> Result<String> {
        self.ensure_active_conversation(conversation_id)?;
        let route: Option<String> = self
            .store
            .db
            .query_row(
                "SELECT execution_route FROM conversation_execution_routes WHERE conversation_id=?1",
                [conversation_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(route.unwrap_or_else(|| "auto".to_owned()))
    }

    /// Prepares a direct model execution on the current device (standalone OR
    /// paired Replica when Home is offline / `direct_byok` is selected).
    ///
    /// If the message was already dispatched or is `unknown_send` to Home,
    /// direct execution is rejected to prevent duplicate execution across nodes.
    pub fn prepare_direct_execution(
        &mut self,
        request_id: &str,
        now: i64,
    ) -> Result<DirectExecutionPrepared> {
        let profile = self
            .direct_provider_profile()?
            .filter(|p| p.enabled && p.api_key_configured)
            .ok_or(Error::InvalidTransition)?;

        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Check if a direct execution for this request_id already exists.
        let existing: Option<(String, String, String)> = tx
            .query_row(
                "SELECT e.id, m.conversation_id, m.id
                 FROM executions e
                 JOIN tasks t ON t.id = e.task_id
                 JOIN messages m ON m.id = t.source_message_id
                 JOIN runner_dispatches d ON d.execution_id = e.id
                 WHERE e.request_id = ?1 AND d.runner_instance_id = ?2",
                params![request_id, DIRECT_RUNNER],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((exec_id, conv_id, msg_id)) = existing {
            let turns = load_conversation_turns(&tx, &conv_id, &msg_id)?;
            tx.commit()?;
            return Ok(DirectExecutionPrepared {
                execution_id: exec_id,
                request_id: request_id.to_owned(),
                conversation_id: conv_id,
                message_id: msg_id,
                provider_id: profile.id,
                provider_kind: profile.provider_kind,
                base_url: profile.base_url,
                model_id: profile.model_id,
                api_key_slot: profile.api_key_slot,
                messages: turns,
                can_start: false,
            });
        }

        // Block if another direct execution is already active on this device.
        let busy: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM executions e
               JOIN runner_dispatches d ON d.execution_id = e.id
               WHERE d.runner_instance_id = ?1
                 AND e.status IN ('pending','dispatching','running','paused','unknown')
             )",
            [DIRECT_RUNNER],
            |r| r.get(0),
        )?;
        if busy {
            return Err(Error::InvalidTransition);
        }

        // Verify the user message and its outbox state.
        let msg_row: Option<(String, String, String)> = tx
            .query_row(
                "SELECT m.id, m.conversation_id, o.send_state
                 FROM messages m
                 JOIN intent_outbox o ON o.request_id = m.request_id AND o.message_id = m.id
                 WHERE m.request_id = ?1 AND m.origin_device_id = ?2 AND m.role = 'user'",
                params![request_id, self.origin],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((message_id, conversation_id, send_state)) = msg_row else {
            return Err(Error::InvalidTransition);
        };

        // Zero-bytes boundary check: if the message was already sent to or accepted by
        // a remote Home (or local CLI runner), we MUST NOT run a second execution.
        if !matches!(
            send_state.as_str(),
            "local_saved" | "queued_for_wakeup" | "not_dispatched"
        ) {
            return Err(Error::InvalidTransition);
        }

        // Claim the outbox row so the background Home dispatcher will never send this
        // request to Home as an unexecuted remote job.
        let claimed = tx.execute(
            "UPDATE intent_outbox
             SET send_state='peer_accepted',
                 transport_evidence='peer_receipt',
                 acked_at=?2,
                 lease_owner=NULL,
                 lease_until=NULL,
                 next_attempt_at=NULL,
                 last_error='local_direct_provider'
             WHERE request_id=?1
               AND send_state IN ('local_saved','queued_for_wakeup','not_dispatched')",
            params![request_id, now],
        )?;
        if claimed != 1 {
            return Err(Error::InvalidTransition);
        }

        let turns = load_conversation_turns(&tx, &conversation_id, &message_id)?;
        let key = digest(request_id.as_bytes());
        let project_id = format!("direct-project:{key}");
        let workspace_id = format!("direct-workspace:{key}");
        let task_id = format!("direct-task:{key}");
        let execution_id = format!("direct-execution:{key}");
        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM home_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;

        let requested = json!({
            "harness": "direct_api",
            "mode": "direct_byok",
            "provider_id": profile.id,
            "provider_kind": profile.provider_kind,
            "model_id": profile.model_id,
            "conversation_id": conversation_id,
            "source_message_id": message_id,
        });
        let contract = json!({
            "schema_version": 1,
            "task_id": task_id,
            "workspace_id": workspace_id,
            "conversation_id": conversation_id,
            "execution_route": "direct_byok",
            "continuation_mode": "bounded_history_context",
            "turn_count": turns.len(),
            "allowed_side_effects": "none",
            "required_checks": []
        });
        let contract_str = serde_json::to_string(&contract)?;
        let contract_hash = digest(contract_str.as_bytes());
        let resolved = json!({
            "runner": DIRECT_RUNNER,
            "provider_id": profile.id,
            "provider_kind": profile.provider_kind,
            "model_id": profile.model_id
        });
        let title: String = turns
            .last()
            .map(|t| t.content.chars().take(80).collect())
            .unwrap_or_else(|| "Direct chat".to_owned());

        tx.execute(
            "INSERT INTO projects(id,title,acceptance,status,version,created_at,updated_at)
             VALUES(?1,?2,'local','active',1,?3,?3)",
            params![project_id, title, now],
        )?;
        tx.execute(
            "INSERT INTO workspaces(id,device_id,directory_ref,policy_ref,version)
             VALUES(?1,?2,?3,'direct_byok_virtual_v1',1)",
            params![workspace_id, self.origin, format!("direct://{execution_id}")],
        )?;
        tx.execute(
            "INSERT INTO project_workspaces(project_id,workspace_id) VALUES(?1,?2)",
            params![project_id, workspace_id],
        )?;
        tx.execute(
            "INSERT INTO tasks(id,project_id,source_message_id,title,status,constraints_json,version,created_at,updated_at)
             VALUES(?1,?2,?3,?4,'running','{\"mode\":\"direct_byok\"}',1,?5,?5)",
            params![task_id, project_id, message_id, title, now],
        )?;
        tx.execute(
            "INSERT INTO executions(id,project_id,task_id,workspace_id,request_id,status,requested_json,resolved_json,contract_json,contract_sha256,started_at,version)
             VALUES(?1,?2,?3,?4,?5,'running',?6,?7,?8,?9,?10,1)",
            params![
                execution_id,
                project_id,
                task_id,
                workspace_id,
                request_id,
                serde_json::to_string(&requested)?,
                serde_json::to_string(&resolved)?,
                contract_str,
                contract_hash,
                now
            ],
        )?;
        tx.execute(
            "INSERT INTO workspace_leases(workspace_id,execution_id,home_epoch,fencing_token,state,updated_at)
             VALUES(?1,?2,?3,1,'held',?4)",
            params![workspace_id, execution_id, epoch, now],
        )?;
        tx.execute(
            "INSERT INTO runner_dispatches(execution_id,request_id,runner_instance_id,state,updated_at)
             VALUES(?1,?2,?3,'dispatching',?4)",
            params![execution_id, request_id, DIRECT_RUNNER, now],
        )?;
        tx.commit()?;

        Ok(DirectExecutionPrepared {
            execution_id,
            request_id: request_id.to_owned(),
            conversation_id,
            message_id,
            provider_id: profile.id,
            provider_kind: profile.provider_kind,
            base_url: profile.base_url,
            model_id: profile.model_id,
            api_key_slot: profile.api_key_slot,
            messages: turns,
            can_start: true,
        })
    }

    pub fn request_direct_stop(&mut self, execution_id: &str, now: i64) -> Result<bool> {
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE executions SET stop_requested_at=coalesce(stop_requested_at,?2),version=version+1
             WHERE id=?1
               AND EXISTS(SELECT 1 FROM runner_dispatches d WHERE d.execution_id=executions.id AND d.runner_instance_id=?3)
               AND status IN ('pending','dispatching','running','paused')",
            params![execution_id, now, DIRECT_RUNNER],
        )?;
        if changed == 1 {
            tx.execute(
                "UPDATE workspace_leases SET state='stop_pending',updated_at=?2 WHERE execution_id=?1 AND state='held'",
                params![execution_id, now],
            )?;
        }
        tx.commit()?;
        Ok(changed == 1)
    }

    pub fn finish_direct_execution(
        &mut self,
        execution_id: &str,
        outcome: DirectExecutionOutcome,
        now: i64,
    ) -> Result<LocalExecution> {
        let paired_home = self.paired_home()?;
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let row: Option<(String, String, String, String, String, String, String, i64, Option<i64>, String)> = tx
            .query_row(
                "SELECT e.status, e.task_id, e.workspace_id, e.request_id, m.id, m.conversation_id, m.content_json, m.created_at, e.stop_requested_at, e.resolved_json
                 FROM executions e
                 JOIN tasks t ON t.id = e.task_id
                 JOIN messages m ON m.id = t.source_message_id
                 JOIN runner_dispatches d ON d.execution_id = e.id
                 WHERE e.id = ?1 AND d.runner_instance_id = ?2",
                params![execution_id, DIRECT_RUNNER],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get::<_, Option<String>>(9)?.unwrap_or_else(|| "{}".to_owned()),
                    ))
                },
            )
            .optional()?;
        let Some((
            current_status,
            task_id,
            workspace_id,
            request_id,
            user_message_id,
            conversation_id,
            user_content_json,
            user_created_at,
            stop_requested_at,
            resolved_json_str,
        )) = row
        else {
            return Err(Error::InvalidTransition);
        };

        if matches!(current_status.as_str(), "completed" | "failed" | "cancelled") {
            tx.commit()?;
            return self
                .local_executions(&conversation_id)?
                .into_iter()
                .find(|e| e.id == execution_id)
                .ok_or(Error::InvalidTransition);
        }

        let resolved_val: serde_json::Value =
            serde_json::from_str(&resolved_json_str).unwrap_or_else(|_| json!({}));
        let provider_id = resolved_val["provider_id"]
            .as_str()
            .unwrap_or("default")
            .to_owned();
        let provider_kind = resolved_val["provider_kind"]
            .as_str()
            .unwrap_or("openai_compatible")
            .to_owned();
        let configured_model = resolved_val["model_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned();

        match outcome {
            DirectExecutionOutcome::Completed {
                text,
                provider_reported_model,
                input_tokens,
                output_tokens,
            } => {
                let clean_text = text.trim();
                if clean_text.is_empty() || clean_text.len() > 256 * 1024 {
                    return Err(Error::InvalidTransition);
                }
                let effective_model = provider_reported_model
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(&configured_model)
                    .to_owned();
                let evidence_id = format!("direct-evidence:{execution_id}");
                let obs_id = format!("direct-obs:{execution_id}");
                let reply_id = format!("direct-reply:{execution_id}");
                let obs_val = json!({
                    "harness": "direct_api",
                    "provider_id": provider_id,
                    "provider_kind": provider_kind,
                    "model_id": effective_model,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens
                });
                let obs_str = serde_json::to_string(&obs_val)?;
                let obs_hash = digest(obs_str.as_bytes());

                tx.execute(
                    "INSERT INTO evidence(id,source_kind,source_ref,content_sha256,captured_at,verification_scope)
                     VALUES(?1,'direct_provider_response',?2,?3,?4,'direct_byok')",
                    params![evidence_id, format!("direct://{execution_id}"), obs_hash, now],
                )?;
                tx.execute(
                    "INSERT INTO execution_observations(id,execution_id,field,value_json,basis,evidence_ref,observed_at)
                     VALUES(?1,?2,'direct_completion',?3,'provider_reported',?4,?5)",
                    params![obs_id, execution_id, obs_str, evidence_id, now],
                )?;

                // Determine message source & acceptance based on whether this device is paired to a remote Home.
                let (source_tag, acceptance_tag) = if paired_home.is_some() {
                    ("replica_direct_provider", "local")
                } else {
                    ("direct_provider", "home")
                };
                let reply_payload = json!({
                    "text": clean_text,
                    "source": source_tag,
                    "reply_to": user_message_id,
                    "execution_id": execution_id,
                    "provider_id": provider_id,
                    "model_id": effective_model
                });
                tx.execute(
                    "INSERT INTO messages(id,conversation_id,origin_device_id,role,content_json,acceptance,created_at)
                     VALUES(?1,?2,?3,'assistant',?4,?5,?6)",
                    params![
                        reply_id,
                        conversation_id,
                        self.origin,
                        serde_json::to_string(&reply_payload)?,
                        acceptance_tag,
                        now
                    ],
                )?;
                tx.execute(
                    "INSERT INTO evidence_links(evidence_id,object_kind,object_id,relation)
                     VALUES(?1,'message',?2,'produced'),(?1,'execution',?3,'observed')",
                    params![evidence_id, reply_id, execution_id],
                )?;
                tx.execute(
                    "UPDATE executions SET status='completed',observed_json=?2,observed_evidence_ref=?3,ended_at=?4,version=version+1 WHERE id=?1",
                    params![execution_id, obs_str, evidence_id, now],
                )?;
                tx.execute(
                    "UPDATE tasks SET status='awaiting_review',updated_at=?2,version=version+1 WHERE id=?1",
                    params![task_id, now],
                )?;
                tx.execute(
                    "UPDATE runner_dispatches SET state='terminal',last_error=NULL,updated_at=?2 WHERE execution_id=?1",
                    params![execution_id, now],
                )?;
                tx.execute(
                    "UPDATE workspace_leases SET state='released',updated_at=?2 WHERE workspace_id=?1",
                    params![workspace_id, now],
                )?;

                // If this device is a Replica paired to a remote Home, enqueue the completed
                // turn for idempotent convergence into Home when Home comes back online.
                if let Some((home_id, home_epoch, _)) = paired_home {
                    let user_parsed: serde_json::Value =
                        serde_json::from_str(&user_content_json).unwrap_or_else(|_| json!({}));
                    let user_text = user_parsed["text"].as_str().unwrap_or("").to_owned();
                    let conv_title: String = tx.query_row(
                        "SELECT title FROM conversations WHERE id=?1",
                        [&conversation_id],
                        |r| r.get(0),
                    )?;
                    let sync_request_id = format!("replica-direct-sync:{request_id}");
                    let mut turn_payload = ReplicaDirectTurnPayload {
                        sync_request_id: sync_request_id.clone(),
                        request_id: request_id.clone(),
                        conversation_id: conversation_id.clone(),
                        conversation_title: conv_title,
                        user_message_id: user_message_id.clone(),
                        user_text,
                        user_created_at,
                        execution_id: execution_id.to_owned(),
                        reply_message_id: reply_id.clone(),
                        reply_text: clean_text.to_owned(),
                        provider_id,
                        provider_kind,
                        model_id: effective_model,
                        completed_at: now,
                        origin_device_id: self.origin.clone(),
                        home_device_id: home_id.clone(),
                        home_epoch,
                        payload_sha256: String::new(),
                    };
                    turn_payload.payload_sha256 = hash_replica_direct_turn(&turn_payload)?;
                    let payload_json = serde_json::to_string(&turn_payload)?;
                    tx.execute(
                        "INSERT INTO replica_direct_sync_outbox(sync_request_id,request_id,conversation_id,user_message_id,execution_id,reply_message_id,origin_device_id,home_device_id,home_epoch,payload_json,payload_sha256,sync_state,created_at)
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'pending',?12)",
                        params![
                            sync_request_id,
                            request_id,
                            conversation_id,
                            user_message_id,
                            execution_id,
                            reply_id,
                            self.origin,
                            home_id,
                            home_epoch,
                            payload_json,
                            turn_payload.payload_sha256,
                            now
                        ],
                    )?;
                }
            }
            DirectExecutionOutcome::Failed { reason } => {
                let err_msg = if reason.trim().is_empty() {
                    "DIRECT_PROVIDER_FAILED"
                } else {
                    reason.trim()
                };
                tx.execute(
                    "UPDATE executions SET status='failed',ended_at=?2,version=version+1 WHERE id=?1",
                    params![execution_id, now],
                )?;
                tx.execute(
                    "UPDATE tasks SET status='blocked',updated_at=?2,version=version+1 WHERE id=?1",
                    params![task_id, now],
                )?;
                tx.execute(
                    "UPDATE runner_dispatches SET state='terminal',last_error=?2,updated_at=?3 WHERE execution_id=?1",
                    params![execution_id, err_msg, now],
                )?;
                tx.execute(
                    "UPDATE workspace_leases SET state='released',updated_at=?2 WHERE workspace_id=?1",
                    params![workspace_id, now],
                )?;
            }
            DirectExecutionOutcome::Cancelled { reason } => {
                let err_msg = if reason.trim().is_empty() {
                    "DIRECT_PROVIDER_CANCELLED"
                } else {
                    reason.trim()
                };
                tx.execute(
                    "UPDATE executions SET status='cancelled',stop_requested_at=coalesce(?2,stop_requested_at,?3),stop_confirmed_at=?3,ended_at=?3,version=version+1 WHERE id=?1",
                    params![execution_id, stop_requested_at, now],
                )?;
                tx.execute(
                    "UPDATE tasks SET status='cancelled',updated_at=?2,version=version+1 WHERE id=?1",
                    params![task_id, now],
                )?;
                tx.execute(
                    "UPDATE runner_dispatches SET state='terminal',last_error=?2,updated_at=?3 WHERE execution_id=?1",
                    params![execution_id, err_msg, now],
                )?;
                tx.execute(
                    "UPDATE workspace_leases SET state='released',updated_at=?2 WHERE workspace_id=?1",
                    params![workspace_id, now],
                )?;
            }
        }

        tx.commit()?;
        self.local_executions(&conversation_id)?
            .into_iter()
            .find(|e| e.id == execution_id)
            .ok_or(Error::InvalidTransition)
    }

    pub fn pending_replica_direct_syncs(&self) -> Result<Vec<ReplicaDirectTurnPayload>> {
        let Some((home_id, home_epoch, _)) = self.paired_home()? else {
            return Ok(Vec::new());
        };
        let mut stmt = self.store.db.prepare(
            "SELECT payload_json FROM replica_direct_sync_outbox
             WHERE sync_state='pending' AND home_device_id=?1 AND home_epoch=?2
             ORDER BY created_at, rowid",
        )?;
        let rows = stmt.query_map(params![home_id, home_epoch], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            let parsed: ReplicaDirectTurnPayload = serde_json::from_str(&row?)?;
            out.push(parsed);
        }
        Ok(out)
    }

    pub fn mark_replica_direct_turn_synced(
        &mut self,
        turn: &ReplicaDirectTurnPayload,
        receipt: &HomeReceipt,
        now: i64,
    ) -> Result<()> {
        let (home_id, home_epoch, _) = self.paired_home()?.ok_or(Error::InvalidTransition)?;
        let expected_hash = hash_replica_direct_turn(turn)?;
        if !receipt.accepted
            || receipt.request_id != turn.sync_request_id
            || receipt.home_device_id != home_id
            || receipt.home_epoch != home_epoch
            || receipt.payload_sha256 != expected_hash
            || turn.payload_sha256 != expected_hash
        {
            return Err(Error::IdentityConflict);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = tx.execute(
            "UPDATE replica_direct_sync_outbox
             SET sync_state='synced', synced_at=?3, last_error=NULL
             WHERE sync_request_id=?1 AND payload_sha256=?2 AND sync_state='pending'",
            params![turn.sync_request_id, expected_hash, now],
        )?;
        if updated != 1 {
            return Err(Error::InvalidTransition);
        }
        tx.execute(
            "UPDATE messages SET acceptance='home' WHERE id IN (?1, ?2)",
            params![turn.user_message_id, turn.reply_message_id],
        )?;
        tx.execute(
            "UPDATE conversations SET acceptance='home' WHERE id=?1",
            [&turn.conversation_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn host_receive_replica_direct_turn(
        &mut self,
        ctx: &HomeMessageContext,
        turn: &ReplicaDirectTurnPayload,
        now: i64,
    ) -> Result<HomeReceipt> {
        self.store.receive_replica_direct_turn(ctx, turn, now)
    }
}

impl Store {
    /// Home authority endpoint that idempotently merges a turn executed directly
    /// on a paired Replica while Home was offline (or in `direct_byok` mode).
    ///
    /// Home records the user message, completed Execution, and assistant reply
    /// under Home authority (`acceptance = 'home'`) without re-running the model.
    pub fn receive_replica_direct_turn(
        &mut self,
        ctx: &HomeMessageContext,
        turn: &ReplicaDirectTurnPayload,
        now: i64,
    ) -> Result<HomeReceipt> {
        let expected_hash = hash_replica_direct_turn(turn)?;
        if turn.payload_sha256 != expected_hash
            || turn.sync_request_id.trim().is_empty()
            || turn.request_id.trim().is_empty()
            || turn.user_text.trim().is_empty()
            || turn.reply_text.trim().is_empty()
        {
            return Err(Error::IdentityConflict);
        }
        let auth_request = AppendMessage {
            request_id: turn.sync_request_id.clone(),
            session_id: turn.conversation_id.clone(),
            message_id: turn.user_message_id.clone(),
            origin_device_id: turn.origin_device_id.clone(),
            home_device_id: turn.home_device_id.clone(),
            home_epoch: turn.home_epoch,
            content: json!({"text": turn.user_text}),
        };

        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, ctx, &auth_request, now, false)?;

        // Check prior inbox receipt for idempotency.
        let prior_row: Option<(String, i64, String, String)> = tx
            .query_row(
                "SELECT origin_device_id, home_epoch, payload_sha256, outcome
                 FROM inbox_receipts WHERE request_id=?1",
                [&turn.sync_request_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        if let Some((origin, epoch, stored_hash, outcome)) = prior_row {
            if origin != turn.origin_device_id
                || epoch != turn.home_epoch
                || stored_hash != expected_hash
            {
                return Err(Error::IdentityConflict);
            }
            tx.commit()?;
            return Ok(HomeReceipt {
                request_id: turn.sync_request_id.clone(),
                home_device_id: turn.home_device_id.clone(),
                home_epoch: epoch,
                payload_sha256: stored_hash,
                accepted: outcome == "accepted",
            });
        }

        // Reject if Home already has a different message or execution for `turn.request_id`.
        let conflicting_msg: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE request_id=?1 AND id!=?2)",
            params![turn.request_id, turn.user_message_id],
            |r| r.get(0),
        )?;
        let conflicting_exec: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM executions WHERE request_id=?1 AND id!=?2)",
            params![turn.request_id, turn.execution_id],
            |r| r.get(0),
        )?;
        if conflicting_msg || conflicting_exec {
            return Err(Error::IdentityConflict);
        }

        // Ensure conversation exists and is promoted to 'home' acceptance.
        let conv_title = if turn.conversation_title.trim().is_empty() {
            turn.user_text.chars().take(60).collect::<String>()
        } else {
            turn.conversation_title.clone()
        };
        tx.execute(
            "INSERT INTO conversations(id,project_id,title,acceptance,version,created_at)
             VALUES(?1,NULL,?2,'home',1,?3)
             ON CONFLICT(id) DO UPDATE SET acceptance='home'",
            params![turn.conversation_id, conv_title, turn.user_created_at],
        )?;

        // Insert user message with `acceptance = 'home'`.
        let user_json = serde_json::to_string(&json!({
            "text": turn.user_text,
            "source": "replica_direct_user"
        }))?;
        tx.execute(
            "INSERT OR IGNORE INTO messages(id,conversation_id,origin_device_id,request_id,role,content_json,acceptance,created_at)
             VALUES(?1,?2,?3,?4,'user',?5,'home',?6)",
            params![
                turn.user_message_id,
                turn.conversation_id,
                turn.origin_device_id,
                turn.request_id,
                user_json,
                turn.user_created_at
            ],
        )?;

        // Insert completed execution & assistant reply with `acceptance = 'home'`.
        let key = digest(turn.request_id.as_bytes());
        let project_id = format!("direct-project:{key}");
        let workspace_id = format!("direct-workspace:{key}");
        let task_id = format!("direct-task:{key}");
        let contract_str = serde_json::to_string(&json!({
            "schema_version": 1,
            "task_id": task_id,
            "workspace_id": workspace_id,
            "conversation_id": turn.conversation_id,
            "execution_route": "direct_byok",
            "origin_device_id": turn.origin_device_id,
            "allowed_side_effects": "none",
            "required_checks": []
        }))?;
        let contract_hash = digest(contract_str.as_bytes());
        let requested_str = serde_json::to_string(&json!({
            "harness": "direct_api",
            "mode": "replica_direct_byok",
            "provider_id": turn.provider_id,
            "provider_kind": turn.provider_kind,
            "model_id": turn.model_id
        }))?;

        tx.execute(
            "INSERT OR IGNORE INTO projects(id,title,acceptance,status,version,created_at,updated_at)
             VALUES(?1,?2,'home','active',1,?3,?3)",
            params![project_id, conv_title, turn.completed_at],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO workspaces(id,device_id,directory_ref,policy_ref,version)
             VALUES(?1,?2,?3,'direct_byok_virtual_v1',1)",
            params![
                workspace_id,
                turn.origin_device_id,
                format!("direct://{}", turn.execution_id)
            ],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO project_workspaces(project_id,workspace_id) VALUES(?1,?2)",
            params![project_id, workspace_id],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO tasks(id,project_id,source_message_id,title,status,constraints_json,version,created_at,updated_at)
             VALUES(?1,?2,?3,?4,'awaiting_review','{\"mode\":\"replica_direct_byok\"}',1,?5,?5)",
            params![task_id, project_id, turn.user_message_id, conv_title, turn.completed_at],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO executions(id,project_id,task_id,workspace_id,request_id,status,requested_json,contract_json,contract_sha256,started_at,ended_at,version)
             VALUES(?1,?2,?3,?4,?5,'completed',?6,?7,?8,?9,?10,1)",
            params![
                turn.execution_id,
                project_id,
                task_id,
                workspace_id,
                turn.request_id,
                requested_str,
                contract_str,
                contract_hash,
                turn.user_created_at,
                turn.completed_at
            ],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO runner_dispatches(execution_id,request_id,runner_instance_id,state,updated_at)
             VALUES(?1,?2,?3,'terminal',?4)",
            params![turn.execution_id, turn.request_id, DIRECT_RUNNER, turn.completed_at],
        )?;

        let reply_json = serde_json::to_string(&json!({
            "text": turn.reply_text,
            "source": "replica_direct_provider",
            "reply_to": turn.user_message_id,
            "execution_id": turn.execution_id,
            "provider_id": turn.provider_id,
            "model_id": turn.model_id,
            "origin_device_id": turn.origin_device_id
        }))?;
        tx.execute(
            "INSERT OR IGNORE INTO messages(id,conversation_id,origin_device_id,role,content_json,acceptance,created_at)
             VALUES(?1,?2,?3,'assistant',?4,'home',?5)",
            params![
                turn.reply_message_id,
                turn.conversation_id,
                turn.origin_device_id,
                reply_json,
                turn.completed_at
            ],
        )?;

        let event_payload = serde_json::to_string(&json!({
            "conversation_id": turn.conversation_id,
            "user_message_id": turn.user_message_id,
            "execution_id": turn.execution_id,
            "reply_message_id": turn.reply_message_id,
            "origin_device_id": turn.origin_device_id,
            "source": "replica_direct_provider"
        }))?;
        tx.execute(
            "INSERT INTO domain_events(event_id,authority_epoch,object_kind,object_id,object_version,request_id,source_device_id,type,payload_json,created_at)
             VALUES(?1,?2,'execution',?3,1,?4,?5,'replica_direct_turn_merged',?6,?7)",
            params![
                format!("replica-direct-merged:{}", turn.sync_request_id),
                turn.home_epoch,
                turn.execution_id,
                turn.sync_request_id,
                turn.origin_device_id,
                event_payload,
                now
            ],
        )?;
        tx.execute(
            "INSERT INTO inbox_receipts(request_id,origin_device_id,home_epoch,payload_sha256,outcome,result_json,effect_ref,decided_at)
             VALUES(?1,?2,?3,?4,'accepted',?5,?6,?7)",
            params![
                turn.sync_request_id,
                turn.origin_device_id,
                turn.home_epoch,
                expected_hash,
                event_payload,
                turn.reply_message_id,
                now
            ],
        )?;
        tx.commit()?;

        Ok(HomeReceipt {
            request_id: turn.sync_request_id.clone(),
            home_device_id: turn.home_device_id.clone(),
            home_epoch: turn.home_epoch,
            payload_sha256: expected_hash,
            accepted: true,
        })
    }
}

fn hash_replica_direct_turn(turn: &ReplicaDirectTurnPayload) -> Result<String> {
    let canonical = json!({
        "sync_request_id": turn.sync_request_id,
        "request_id": turn.request_id,
        "conversation_id": turn.conversation_id,
        "user_message_id": turn.user_message_id,
        "user_text": turn.user_text,
        "user_created_at": turn.user_created_at,
        "execution_id": turn.execution_id,
        "reply_message_id": turn.reply_message_id,
        "reply_text": turn.reply_text,
        "provider_id": turn.provider_id,
        "provider_kind": turn.provider_kind,
        "model_id": turn.model_id,
        "completed_at": turn.completed_at,
        "origin_device_id": turn.origin_device_id,
        "home_device_id": turn.home_device_id,
        "home_epoch": turn.home_epoch,
    });
    Ok(digest(&serde_json::to_vec(&canonical)?))
}

fn load_conversation_turns(
    tx: &Transaction<'_>,
    conversation_id: &str,
    current_message_id: &str,
) -> Result<Vec<DirectChatTurn>> {
    let mut stmt = tx.prepare(
        "SELECT id, role, content_json
         FROM messages
         WHERE conversation_id = ?1
           AND role IN ('user','assistant')
           AND acceptance IN ('local','home')
         ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map([conversation_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut all = Vec::new();
    for row in rows {
        let (id, role, content_json) = row?;
        let parsed: serde_json::Value = serde_json::from_str(&content_json)?;
        if let Some(text) = parsed["text"].as_str() {
            if !text.trim().is_empty() {
                all.push(DirectChatTurn {
                    message_id: id.clone(),
                    role,
                    content: text.to_owned(),
                });
            }
        }
        if id == current_message_id {
            break;
        }
    }
    // Keep up to the most recent 12 turns to bound token usage while preserving continuity.
    let start = all.len().saturating_sub(12);
    Ok(all.into_iter().skip(start).collect())
}
