use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::work_core::models::{CreateProjectInput, WorkObjectKind};

use super::models::{
    AcquireLeaseInput, ActionChoice, ApprovalChoiceSemantic, ApprovalDecisionInput, ApprovalStatus,
    CreateApprovalInput, CreateCheckpointInput, CreateEvidenceInput, CreateRunInput,
    FinishAttemptInput, GoalApproval, GoalAttempt, GoalCheckpoint, GoalContract, GoalEvent,
    GoalEvidence, GoalPhase, GoalRisk, GoalRun, GoalRunStatus, RecoverySummary, StartAttemptInput,
    TransitionRunInput, VerificationState, GOAL_RUNTIME_SCHEMA_VERSION,
};

pub const PERSONAL_PROJECT_ID: &str = "project_personal_inbox";

const CREATE_RUN_OPERATION: &str = "goal.run.create";
const TRANSITION_OPERATION: &str = "goal.run.transition";
const CHECKPOINT_OPERATION: &str = "goal.checkpoint.create";
const EVIDENCE_OPERATION: &str = "goal.evidence.create";
const APPROVAL_OPERATION: &str = "goal.approval.create";
const DECISION_OPERATION: &str = "goal.approval.decide";
const LEASE_OPERATION: &str = "goal.run.lease";
const START_ATTEMPT_OPERATION: &str = "goal.attempt.start";
const FINISH_ATTEMPT_OPERATION: &str = "goal.attempt.finish";

pub fn init_goal_runtime_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS goal_runs (
            run_id TEXT PRIMARY KEY,
            schema_version INTEGER NOT NULL,
            goal_id TEXT NOT NULL,
            project_id TEXT NOT NULL,
            status TEXT NOT NULL,
            phase TEXT NOT NULL,
            verification_state TEXT NOT NULL,
            risk TEXT NOT NULL,
            model_calls_used INTEGER NOT NULL DEFAULT 0,
            tool_calls_used INTEGER NOT NULL DEFAULT 0,
            repairs_used INTEGER NOT NULL DEFAULT 0,
            runtime_seconds_used INTEGER NOT NULL DEFAULT 0,
            latest_checkpoint_id TEXT,
            lease_owner TEXT,
            lease_expires_at INTEGER,
            recovery_count INTEGER NOT NULL DEFAULT 0,
            last_error_code TEXT,
            last_error_detail TEXT,
            next_action TEXT,
            revision INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            finished_at INTEGER,
            FOREIGN KEY (goal_id) REFERENCES work_objects(id),
            FOREIGN KEY (project_id) REFERENCES work_projects(id)
        );
        CREATE INDEX IF NOT EXISTS idx_goal_runs_project
            ON goal_runs(project_id, status, updated_at);
        CREATE INDEX IF NOT EXISTS idx_goal_runs_status
            ON goal_runs(status, updated_at);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_goal_runs_one_active
            ON goal_runs(goal_id)
            WHERE status NOT IN ('done', 'failed', 'cancelled');

        CREATE TABLE IF NOT EXISTS goal_attempts (
            attempt_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            status TEXT NOT NULL,
            executor TEXT NOT NULL,
            plan_summary TEXT,
            result_summary TEXT,
            tool_receipts_json TEXT NOT NULL DEFAULT '[]',
            error_code TEXT,
            error_detail TEXT,
            started_at INTEGER NOT NULL,
            finished_at INTEGER,
            FOREIGN KEY (run_id) REFERENCES goal_runs(run_id)
        );
        CREATE INDEX IF NOT EXISTS idx_goal_attempts_run
            ON goal_attempts(run_id, started_at);

        CREATE TABLE IF NOT EXISTS goal_evidence (
            evidence_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            evidence_type TEXT NOT NULL,
            reference TEXT NOT NULL,
            content_hash TEXT,
            verification_state TEXT NOT NULL,
            verifier TEXT NOT NULL,
            detail TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (run_id) REFERENCES goal_runs(run_id),
            UNIQUE(run_id, rule_id, reference)
        );
        CREATE INDEX IF NOT EXISTS idx_goal_evidence_rule
            ON goal_evidence(run_id, rule_id, verification_state);

        CREATE TABLE IF NOT EXISTS goal_checkpoints (
            checkpoint_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            checkpoint_type TEXT NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            safe_to_resume INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (run_id) REFERENCES goal_runs(run_id)
        );
        CREATE INDEX IF NOT EXISTS idx_goal_checkpoints_run
            ON goal_checkpoints(run_id, created_at);

        CREATE TABLE IF NOT EXISTS goal_approvals (
            approval_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            summary TEXT NOT NULL,
            risk TEXT NOT NULL,
            choices_json TEXT NOT NULL,
            trusted_device_required INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'pending',
            selected_choice_id TEXT,
            decided_by TEXT,
            decided_device_id TEXT,
            input_modality TEXT,
            expires_at INTEGER,
            revision INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            decided_at INTEGER,
            FOREIGN KEY (run_id) REFERENCES goal_runs(run_id)
        );
        CREATE INDEX IF NOT EXISTS idx_goal_approvals_pending
            ON goal_approvals(run_id, status, created_at);

        CREATE TABLE IF NOT EXISTS goal_events (
            event_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            actor TEXT NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            idempotency_key TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (run_id) REFERENCES goal_runs(run_id)
        );
        CREATE INDEX IF NOT EXISTS idx_goal_events_run
            ON goal_events(run_id, created_at);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_goal_events_idempotency
            ON goal_events(idempotency_key) WHERE idempotency_key IS NOT NULL;

        CREATE TABLE IF NOT EXISTS goal_runtime_receipts (
            idempotency_key TEXT PRIMARY KEY,
            operation TEXT NOT NULL,
            result_id TEXT NOT NULL,
            response_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        ",
    )
    .map_err(|error| format!("初始化 Goal Runtime 数据表失败: {error}"))?;
    Ok(())
}

fn now_ms() -> i64 {
    crate::now_ms() as i64
}

fn clean_actor(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "bob".into()
    } else {
        value.chars().take(100).collect()
    }
}

fn require_key(key: &str) -> Result<(), String> {
    let length = key.trim().chars().count();
    if length == 0 {
        Err("idempotencyKey 不能为空".into())
    } else if length > 200 {
        Err("idempotencyKey 不能超过 200 个字符".into())
    } else {
        Ok(())
    }
}

fn parse_json<T: DeserializeOwned>(raw: String, label: &str) -> rusqlite::Result<T> {
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            raw.len(),
            rusqlite::types::Type::Text,
            format!("{label} JSON 无效: {error}").into(),
        )
    })
}

fn enum_error(label: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        value.len(),
        rusqlite::types::Type::Text,
        format!("未知 {label}: {value}").into(),
    )
}

fn run_from_row(row: &Row<'_>) -> rusqlite::Result<GoalRun> {
    let status_raw: String = row.get(3)?;
    let phase_raw: String = row.get(4)?;
    let verification_raw: String = row.get(5)?;
    let risk_raw: String = row.get(6)?;
    Ok(GoalRun {
        run_id: row.get(0)?,
        goal_id: row.get(1)?,
        project_id: row.get(2)?,
        status: GoalRunStatus::parse(&status_raw)
            .ok_or_else(|| enum_error("Goal status", &status_raw))?,
        phase: GoalPhase::parse(&phase_raw).ok_or_else(|| enum_error("Goal phase", &phase_raw))?,
        verification_state: VerificationState::parse(&verification_raw)
            .ok_or_else(|| enum_error("verification state", &verification_raw))?,
        risk: GoalRisk::parse(&risk_raw).ok_or_else(|| enum_error("Goal risk", &risk_raw))?,
        model_calls_used: row.get::<_, i64>(7)? as u32,
        tool_calls_used: row.get::<_, i64>(8)? as u32,
        repairs_used: row.get::<_, i64>(9)? as u32,
        runtime_seconds_used: row.get::<_, i64>(10)? as u64,
        latest_checkpoint_id: row.get(11)?,
        lease_owner: row.get(12)?,
        lease_expires_at: row.get(13)?,
        recovery_count: row.get::<_, i64>(14)? as u32,
        last_error_code: row.get(15)?,
        last_error_detail: row.get(16)?,
        next_action: row.get(17)?,
        revision: row.get::<_, i64>(18)? as u64,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
        finished_at: row.get(21)?,
    })
}

const RUN_SELECT: &str = "SELECT run_id, goal_id, project_id, status, phase, verification_state, risk, model_calls_used, tool_calls_used, repairs_used, runtime_seconds_used, latest_checkpoint_id, lease_owner, lease_expires_at, recovery_count, last_error_code, last_error_detail, next_action, revision, created_at, updated_at, finished_at FROM goal_runs";

fn approval_from_row(row: &Row<'_>) -> rusqlite::Result<GoalApproval> {
    let risk_raw: String = row.get(3)?;
    let choices_raw: String = row.get(4)?;
    let status_raw: String = row.get(6)?;
    Ok(GoalApproval {
        approval_id: row.get(0)?,
        run_id: row.get(1)?,
        summary: row.get(2)?,
        risk: GoalRisk::parse(&risk_raw).ok_or_else(|| enum_error("approval risk", &risk_raw))?,
        choices: parse_json(choices_raw, "approval choices")?,
        trusted_device_required: row.get::<_, i64>(5)? != 0,
        status: ApprovalStatus::parse(&status_raw)
            .ok_or_else(|| enum_error("approval status", &status_raw))?,
        selected_choice_id: row.get(7)?,
        decided_by: row.get(8)?,
        decided_device_id: row.get(9)?,
        input_modality: row.get(10)?,
        expires_at: row.get(11)?,
        revision: row.get::<_, i64>(12)? as u64,
        created_at: row.get(13)?,
        decided_at: row.get(14)?,
    })
}

const APPROVAL_SELECT: &str = "SELECT approval_id, run_id, summary, risk, choices_json, trusted_device_required, status, selected_choice_id, decided_by, decided_device_id, input_modality, expires_at, revision, created_at, decided_at FROM goal_approvals";

fn read_receipt<T: DeserializeOwned>(
    conn: &Connection,
    key: &str,
    operation: &str,
) -> Result<Option<T>, String> {
    let receipt = conn
        .query_row(
            "SELECT operation, response_json FROM goal_runtime_receipts WHERE idempotency_key = ?1",
            params![key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((saved_operation, response)) = receipt else {
        return Ok(None);
    };
    if saved_operation != operation {
        return Err("idempotencyKey 已被其他 Goal Runtime 操作使用".into());
    }
    serde_json::from_str(&response)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn save_receipt<T: serde::Serialize>(
    tx: &Transaction<'_>,
    key: &str,
    operation: &str,
    result_id: &str,
    response: &T,
    now: i64,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO goal_runtime_receipts (idempotency_key, operation, result_id, response_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![key, operation, result_id, serde_json::to_string(response).map_err(|error| error.to_string())?, now],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

fn append_event(
    tx: &Transaction<'_>,
    run_id: &str,
    event_type: &str,
    actor: &str,
    payload: &Value,
    idempotency_key: Option<&str>,
    now: i64,
) -> Result<GoalEvent, String> {
    let event = GoalEvent {
        event_id: format!("goal_event_{}", ulid::Ulid::new()),
        run_id: run_id.into(),
        event_type: event_type.into(),
        actor: clean_actor(actor),
        payload: payload.clone(),
        idempotency_key: idempotency_key.map(str::to_string),
        created_at: now,
    };
    tx.execute(
        "INSERT INTO goal_events (event_id, run_id, event_type, actor, payload_json, idempotency_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![&event.event_id, &event.run_id, &event.event_type, &event.actor, serde_json::to_string(&event.payload).map_err(|error| error.to_string())?, &event.idempotency_key, event.created_at],
    ).map_err(|error| error.to_string())?;
    Ok(event)
}

pub fn ensure_personal_workspace(conn: &mut Connection) -> Result<String, String> {
    if crate::work_core::repository::get_project(conn, PERSONAL_PROJECT_ID)?.is_some() {
        return Ok(PERSONAL_PROJECT_ID.into());
    }
    let project = crate::work_core::repository::create_project(
        conn,
        CreateProjectInput {
            project_id: Some(PERSONAL_PROJECT_ID.into()),
            title: "个人工作区".into(),
            mission: "保存尚未归入正式项目的持续工作".into(),
            current_phase: Some("持续处理".into()),
            summary: Some("Bob 自动创建的低摩擦 Goal 收件区".into()),
            source_ref: None,
            metadata: json!({ "systemManaged": true }),
            actor: Some("bob".into()),
            idempotency_key: "goal-runtime-personal-workspace-v1".into(),
        },
    )?;
    Ok(project.id)
}

pub fn get_run(conn: &Connection, run_id: &str) -> Result<Option<GoalRun>, String> {
    conn.query_row(
        &format!("{RUN_SELECT} WHERE run_id = ?1"),
        params![run_id],
        run_from_row,
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn get_active_run_for_goal(
    conn: &Connection,
    goal_id: &str,
) -> Result<Option<GoalRun>, String> {
    conn.query_row(
        &format!("{RUN_SELECT} WHERE goal_id = ?1 AND status NOT IN ('done','failed','cancelled') ORDER BY created_at DESC LIMIT 1"),
        params![goal_id], run_from_row,
    ).optional().map_err(|error| error.to_string())
}

pub fn list_runs(
    conn: &Connection,
    project_id: Option<&str>,
    limit: usize,
) -> Result<Vec<GoalRun>, String> {
    let limit = limit.clamp(1, 200) as i64;
    let mut runs = Vec::new();
    if let Some(project_id) = project_id {
        let mut stmt = conn
            .prepare(&format!(
                "{RUN_SELECT} WHERE project_id = ?1 ORDER BY updated_at DESC LIMIT ?2"
            ))
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![project_id, limit], run_from_row)
            .map_err(|error| error.to_string())?;
        for row in rows {
            runs.push(row.map_err(|error| error.to_string())?);
        }
    } else {
        let mut stmt = conn
            .prepare(&format!("{RUN_SELECT} ORDER BY updated_at DESC LIMIT ?1"))
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![limit], run_from_row)
            .map_err(|error| error.to_string())?;
        for row in rows {
            runs.push(row.map_err(|error| error.to_string())?);
        }
    }
    Ok(runs)
}

pub fn create_run(conn: &mut Connection, input: CreateRunInput) -> Result<GoalRun, String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, CREATE_RUN_OPERATION)? {
        return Ok(saved);
    }
    let goal = crate::work_core::repository::get_object(conn, &input.goal_id)?
        .filter(|object| object.deleted_at.is_none() && object.kind == WorkObjectKind::Goal)
        .ok_or_else(|| format!("活动 Goal 不存在: {}", input.goal_id))?;
    if goal.project_id != input.project_id {
        return Err("Goal 与 Run 必须属于同一项目".into());
    }
    let contract = GoalContract::from_value(&goal.data)?;
    if input.initial_status == GoalRunStatus::Ready && !contract.ready_for_execution() {
        return Err("Goal Contract 缺少必需 EvidenceRule，不能进入 ready".into());
    }
    if input.risk.requires_approval() && input.initial_status != GoalRunStatus::WaitingUser {
        return Err("R2/R3 Goal 必须先进入 waiting_user".into());
    }
    let now = now_ms();
    let run = GoalRun {
        run_id: format!("goal_run_{}", ulid::Ulid::new()),
        goal_id: input.goal_id,
        project_id: input.project_id,
        status: input.initial_status,
        phase: GoalPhase::Compile,
        verification_state: VerificationState::Pending,
        risk: input.risk,
        model_calls_used: 0,
        tool_calls_used: 0,
        repairs_used: 0,
        runtime_seconds_used: 0,
        latest_checkpoint_id: None,
        lease_owner: None,
        lease_expires_at: None,
        recovery_count: 0,
        last_error_code: None,
        last_error_detail: None,
        next_action: input.next_action,
        revision: 1,
        created_at: now,
        updated_at: now,
        finished_at: None,
    };
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO goal_runs (run_id, schema_version, goal_id, project_id, status, phase, verification_state, risk, next_action, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![&run.run_id, GOAL_RUNTIME_SCHEMA_VERSION, &run.goal_id, &run.project_id, run.status.as_str(), run.phase.as_str(), run.verification_state.as_str(), run.risk.as_str(), &run.next_action, now],
    ).map_err(|error| format!("创建 Goal Run 失败: {error}"))?;
    append_event(
        &tx,
        &run.run_id,
        "goal.run.created",
        &input.actor,
        &json!({ "goalId": run.goal_id, "status": run.status, "risk": run.risk }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        CREATE_RUN_OPERATION,
        &run.run_id,
        &run,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(run)
}

fn required_evidence_verified(conn: &Connection, run: &GoalRun) -> Result<bool, String> {
    let goal = crate::work_core::repository::get_object(conn, &run.goal_id)?
        .ok_or_else(|| format!("Goal 不存在: {}", run.goal_id))?;
    let contract = GoalContract::from_value(&goal.data)?;
    let required = contract
        .evidence_rules
        .iter()
        .filter(|rule| rule.required)
        .collect::<Vec<_>>();
    if required.is_empty() {
        return Ok(false);
    }
    for rule in required {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM goal_evidence WHERE run_id = ?1 AND rule_id = ?2 AND verification_state = 'verified'",
            params![&run.run_id, &rule.rule_id], |row| row.get(0),
        ).map_err(|error| error.to_string())?;
        if count == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn reflect_terminal_goal_status(conn: &mut Connection, run: &GoalRun) {
    if !run.status.is_terminal() {
        return;
    }
    if let Ok(Some(goal)) = crate::work_core::repository::get_object(conn, &run.goal_id) {
        let reflected = crate::work_core::repository::update_goal_status_from_runtime(
            conn,
            crate::work_core::models::UpdateWorkStatusInput {
                object_id: run.goal_id.clone(),
                status: run.status.as_str().into(),
                expected_revision: goal.revision,
                actor: Some("bob".into()),
                idempotency_key: format!(
                    "goal-runtime-reflect:{}:{}",
                    run.run_id,
                    run.status.as_str()
                ),
            },
        );
        if let Err(error) = reflected {
            log::warn!(
                "Goal Runtime terminal status reflection deferred for {}: {}",
                run.run_id,
                error
            );
        }
    }
}

pub fn transition_run(conn: &mut Connection, input: TransitionRunInput) -> Result<GoalRun, String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, TRANSITION_OPERATION)? {
        reflect_terminal_goal_status(conn, &saved);
        return Ok(saved);
    }
    let current = get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if current.revision != input.expected_revision {
        return Err("revision 冲突：Goal Run 已被其他操作更新".into());
    }
    if !current.status.can_transition_to(input.status) {
        return Err(format!(
            "不允许的 Goal 状态转换: {} -> {}",
            current.status.as_str(),
            input.status.as_str()
        ));
    }
    if input.status == GoalRunStatus::Done && !required_evidence_verified(conn, &current)? {
        return Err("GOAL-EVIDENCE-MISSING: 必需 Evidence 尚未全部验证".into());
    }
    let now = now_ms();
    let mut updated = current.clone();
    updated.status = input.status;
    updated.phase = input.phase;
    updated.verification_state = input.verification_state;
    updated.next_action = input.next_action;
    updated.last_error_code = input.error_code;
    updated.last_error_detail = input.error_detail;
    updated.revision += 1;
    updated.updated_at = now;
    updated.finished_at = if updated.status.is_terminal() {
        Some(now)
    } else {
        None
    };
    if !matches!(
        updated.status,
        GoalRunStatus::Running | GoalRunStatus::Verifying
    ) {
        updated.lease_owner = None;
        updated.lease_expires_at = None;
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let changed = tx.execute(
        "UPDATE goal_runs SET status=?1, phase=?2, verification_state=?3, next_action=?4, last_error_code=?5, last_error_detail=?6, revision=?7, updated_at=?8, finished_at=?9, lease_owner=?10, lease_expires_at=?11 WHERE run_id=?12 AND revision=?13",
        params![updated.status.as_str(), updated.phase.as_str(), updated.verification_state.as_str(), &updated.next_action, &updated.last_error_code, &updated.last_error_detail, updated.revision as i64, now, updated.finished_at, &updated.lease_owner, updated.lease_expires_at, &updated.run_id, current.revision as i64],
    ).map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("revision 冲突：Goal Run 更新失败".into());
    }
    append_event(
        &tx,
        &updated.run_id,
        "goal.run.transitioned",
        &input.actor,
        &json!({ "from": current.status, "to": updated.status, "phase": updated.phase, "verificationState": updated.verification_state }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        TRANSITION_OPERATION,
        &updated.run_id,
        &updated,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    reflect_terminal_goal_status(conn, &updated);
    Ok(updated)
}

pub fn create_checkpoint(
    conn: &mut Connection,
    input: CreateCheckpointInput,
) -> Result<GoalCheckpoint, String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, CHECKPOINT_OPERATION)? {
        return Ok(saved);
    }
    let run = get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if run.revision != input.expected_revision {
        return Err("revision 冲突：Goal Run 已被其他操作更新".into());
    }
    let checkpoint_type = input.checkpoint_type.trim();
    if checkpoint_type.is_empty() {
        return Err("checkpointType 不能为空".into());
    }
    let now = now_ms();
    let checkpoint = GoalCheckpoint {
        checkpoint_id: format!("goal_checkpoint_{}", ulid::Ulid::new()),
        run_id: run.run_id.clone(),
        phase: input.phase,
        checkpoint_type: checkpoint_type.into(),
        payload: input.payload,
        safe_to_resume: input.safe_to_resume,
        created_at: now,
    };
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO goal_checkpoints (checkpoint_id, run_id, phase, checkpoint_type, payload_json, safe_to_resume, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![&checkpoint.checkpoint_id, &checkpoint.run_id, checkpoint.phase.as_str(), &checkpoint.checkpoint_type, serde_json::to_string(&checkpoint.payload).map_err(|error| error.to_string())?, if checkpoint.safe_to_resume {1} else {0}, now],
    ).map_err(|error| error.to_string())?;
    let changed = tx.execute("UPDATE goal_runs SET latest_checkpoint_id=?1, revision=revision+1, updated_at=?2 WHERE run_id=?3 AND revision=?4", params![&checkpoint.checkpoint_id, now, &checkpoint.run_id, run.revision as i64]).map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("revision 冲突：checkpoint 未写入 Run".into());
    }
    append_event(
        &tx,
        &checkpoint.run_id,
        "goal.checkpoint.created",
        &input.actor,
        &json!({ "checkpointId": checkpoint.checkpoint_id, "phase": checkpoint.phase, "safeToResume": checkpoint.safe_to_resume }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        CHECKPOINT_OPERATION,
        &checkpoint.checkpoint_id,
        &checkpoint,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(checkpoint)
}

pub fn create_evidence(
    conn: &mut Connection,
    input: CreateEvidenceInput,
) -> Result<GoalEvidence, String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, EVIDENCE_OPERATION)? {
        return Ok(saved);
    }
    let run = get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if run.revision != input.expected_revision {
        return Err("revision 冲突：Goal Run 已被其他操作更新".into());
    }
    let goal = crate::work_core::repository::get_object(conn, &run.goal_id)?
        .ok_or_else(|| "Goal 不存在".to_string())?;
    let contract = GoalContract::from_value(&goal.data)?;
    if !contract
        .evidence_rules
        .iter()
        .any(|rule| rule.rule_id == input.rule_id)
    {
        return Err(format!("EvidenceRule 不存在: {}", input.rule_id));
    }
    if input.reference.trim().is_empty() || input.evidence_type.trim().is_empty() {
        return Err("Evidence type/reference 不能为空".into());
    }
    let now = now_ms();
    let evidence = GoalEvidence {
        evidence_id: format!("goal_evidence_{}", ulid::Ulid::new()),
        run_id: run.run_id.clone(),
        rule_id: input.rule_id.trim().into(),
        evidence_type: input.evidence_type.trim().into(),
        reference: input.reference.trim().into(),
        content_hash: input.content_hash,
        verification_state: input.verification_state,
        verifier: input.verifier.trim().into(),
        detail: input.detail,
        created_at: now,
    };
    if evidence.verifier.is_empty() {
        return Err("Evidence verifier 不能为空".into());
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO goal_evidence (evidence_id, run_id, rule_id, evidence_type, reference, content_hash, verification_state, verifier, detail, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![&evidence.evidence_id, &evidence.run_id, &evidence.rule_id, &evidence.evidence_type, &evidence.reference, &evidence.content_hash, evidence.verification_state.as_str(), &evidence.verifier, &evidence.detail, now],
    ).map_err(|error| error.to_string())?;
    let changed = tx.execute("UPDATE goal_runs SET verification_state=?1, revision=revision+1, updated_at=?2 WHERE run_id=?3 AND revision=?4", params![evidence.verification_state.as_str(), now, &run.run_id, run.revision as i64]).map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("revision 冲突：evidence 未写入 Run".into());
    }
    append_event(
        &tx,
        &run.run_id,
        "goal.evidence.created",
        &input.actor,
        &json!({ "evidenceId": evidence.evidence_id, "ruleId": evidence.rule_id, "verificationState": evidence.verification_state }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        EVIDENCE_OPERATION,
        &evidence.evidence_id,
        &evidence,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(evidence)
}

fn validate_choices(choices: &[ActionChoice]) -> Result<(), String> {
    if !(2..=4).contains(&choices.len()) {
        return Err("审批选项必须为 2–4 个".into());
    }
    let mut ids = HashSet::new();
    for choice in choices {
        if choice.choice_id.trim().is_empty() || choice.label_key.trim().is_empty() {
            return Err("审批选项缺少 choiceId 或 labelKey".into());
        }
        if !ids.insert(choice.choice_id.trim()) {
            return Err("审批选项 choiceId 重复".into());
        }
    }
    Ok(())
}

pub fn create_approval(
    conn: &mut Connection,
    input: CreateApprovalInput,
) -> Result<GoalApproval, String> {
    require_key(&input.idempotency_key)?;
    validate_choices(&input.choices)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, APPROVAL_OPERATION)? {
        return Ok(saved);
    }
    let run = get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if run.revision != input.expected_revision {
        return Err("revision 冲突：Goal Run 已被其他操作更新".into());
    }
    if run.status.is_terminal() {
        return Err("终态 Goal 不能创建审批".into());
    }
    let summary = input.summary.trim();
    if summary.is_empty() {
        return Err("审批摘要不能为空".into());
    }
    let now = now_ms();
    let approval = GoalApproval {
        approval_id: format!("goal_approval_{}", ulid::Ulid::new()),
        run_id: run.run_id.clone(),
        summary: summary.into(),
        risk: input.risk,
        choices: input.choices,
        trusted_device_required: input.trusted_device_required || input.risk == GoalRisk::R3,
        status: ApprovalStatus::Pending,
        selected_choice_id: None,
        decided_by: None,
        decided_device_id: None,
        input_modality: None,
        expires_at: input.expires_at,
        revision: 1,
        created_at: now,
        decided_at: None,
    };
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO goal_approvals (approval_id, run_id, summary, risk, choices_json, trusted_device_required, status, expires_at, revision, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, 1, ?8)",
        params![&approval.approval_id, &approval.run_id, &approval.summary, approval.risk.as_str(), serde_json::to_string(&approval.choices).map_err(|error| error.to_string())?, if approval.trusted_device_required {1} else {0}, approval.expires_at, now],
    ).map_err(|error| error.to_string())?;
    let changed = tx.execute("UPDATE goal_runs SET status='waiting_user', next_action=?1, revision=revision+1, updated_at=?2 WHERE run_id=?3 AND revision=?4", params!["goal.next_waiting_choice", now, &run.run_id, run.revision as i64]).map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("revision 冲突：approval 未写入 Run".into());
    }
    append_event(
        &tx,
        &run.run_id,
        "goal.approval.requested",
        &input.actor,
        &json!({ "approvalId": approval.approval_id, "risk": approval.risk, "choiceCount": approval.choices.len() }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        APPROVAL_OPERATION,
        &approval.approval_id,
        &approval,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(approval)
}

pub fn get_approval(conn: &Connection, approval_id: &str) -> Result<Option<GoalApproval>, String> {
    conn.query_row(
        &format!("{APPROVAL_SELECT} WHERE approval_id=?1"),
        params![approval_id],
        approval_from_row,
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn get_pending_approval(
    conn: &Connection,
    run_id: &str,
) -> Result<Option<GoalApproval>, String> {
    conn.query_row(&format!("{APPROVAL_SELECT} WHERE run_id=?1 AND status='pending' ORDER BY created_at DESC LIMIT 1"), params![run_id], approval_from_row).optional().map_err(|error| error.to_string())
}

pub fn get_latest_resolved_approval(
    conn: &Connection,
    run_id: &str,
) -> Result<Option<GoalApproval>, String> {
    conn.query_row(
        &format!("{APPROVAL_SELECT} WHERE run_id=?1 AND status='resolved' ORDER BY decided_at DESC, created_at DESC LIMIT 1"),
        params![run_id], approval_from_row,
    ).optional().map_err(|error| error.to_string())
}

pub fn decide_approval(
    conn: &mut Connection,
    input: ApprovalDecisionInput,
) -> Result<GoalApproval, String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, DECISION_OPERATION)? {
        return Ok(saved);
    }
    let current = get_approval(conn, &input.approval_id)?
        .ok_or_else(|| format!("审批不存在: {}", input.approval_id))?;
    if current.status != ApprovalStatus::Pending {
        return Err("审批已经结束".into());
    }
    if current.revision != input.expected_revision {
        return Err("revision 冲突：审批已被其他设备更新".into());
    }
    let now = now_ms();
    if current.expires_at.is_some_and(|expires| expires <= now) {
        return Err("审批已过期".into());
    }
    let choice = current
        .choices
        .iter()
        .find(|choice| choice.choice_id == input.choice_id)
        .ok_or_else(|| "审批选项不存在".to_string())?;
    if current.trusted_device_required
        && choice.semantic == ApprovalChoiceSemantic::Approve
        && !input.trusted_device
    {
        return Err("该操作需要在受信任且已解锁的设备上确认".into());
    }
    let run = get_run(conn, &current.run_id)?.ok_or_else(|| "Goal Run 不存在".to_string())?;
    let target_status = match choice.semantic {
        ApprovalChoiceSemantic::Approve | ApprovalChoiceSemantic::SelectOption => {
            GoalRunStatus::Ready
        }
        ApprovalChoiceSemantic::Reject => GoalRunStatus::Cancelled,
        ApprovalChoiceSemantic::Defer | ApprovalChoiceSemantic::Handoff => {
            GoalRunStatus::WaitingUser
        }
    };
    if !run.status.can_transition_to(target_status) {
        return Err("当前 Goal 状态不能应用该审批结果".into());
    }
    let mut updated = current.clone();
    updated.status = ApprovalStatus::Resolved;
    updated.selected_choice_id = Some(choice.choice_id.clone());
    updated.decided_by = Some(clean_actor(&input.actor));
    updated.decided_device_id = Some(input.device_id.trim().into());
    updated.input_modality = Some(input.input_modality.trim().into());
    updated.decided_at = Some(now);
    updated.revision += 1;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE goal_approvals SET status='resolved', selected_choice_id=?1, decided_by=?2, decided_device_id=?3, input_modality=?4, decided_at=?5, revision=?6 WHERE approval_id=?7 AND revision=?8",
        params![&updated.selected_choice_id, &updated.decided_by, &updated.decided_device_id, &updated.input_modality, now, updated.revision as i64, &updated.approval_id, current.revision as i64],
    ).map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE goal_runs SET status=?1, phase=?2, next_action=?3, revision=revision+1, updated_at=?4, finished_at=?5 WHERE run_id=?6 AND revision=?7",
        params![target_status.as_str(), if target_status == GoalRunStatus::Ready {GoalPhase::Observe.as_str()} else {run.phase.as_str()}, match choice.semantic { ApprovalChoiceSemantic::Approve | ApprovalChoiceSemantic::SelectOption => "goal.next_approved", ApprovalChoiceSemantic::Reject => "goal.next_cancelled", ApprovalChoiceSemantic::Defer => "goal.next_deferred", ApprovalChoiceSemantic::Handoff => "goal.next_handoff" }, now, if target_status.is_terminal() {Some(now)} else {None}, &run.run_id, run.revision as i64],
    ).map_err(|error| error.to_string())?;
    append_event(
        &tx,
        &run.run_id,
        "goal.approval.decided",
        &input.actor,
        &json!({ "approvalId": updated.approval_id, "choiceId": choice.choice_id, "semantic": choice.semantic, "deviceId": input.device_id, "inputModality": input.input_modality }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        DECISION_OPERATION,
        &updated.approval_id,
        &updated,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(updated)
}

pub fn list_events(
    conn: &Connection,
    run_id: &str,
    limit: usize,
) -> Result<Vec<GoalEvent>, String> {
    let mut stmt = conn.prepare("SELECT event_id, run_id, event_type, actor, payload_json, idempotency_key, created_at FROM goal_events WHERE run_id=?1 ORDER BY created_at DESC, event_id DESC LIMIT ?2").map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![run_id, limit.clamp(1, 50) as i64], |row| {
            Ok(GoalEvent {
                event_id: row.get(0)?,
                run_id: row.get(1)?,
                event_type: row.get(2)?,
                actor: row.get(3)?,
                payload: parse_json(row.get(4)?, "goal event")?,
                idempotency_key: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn list_evidence(conn: &Connection, run_id: &str) -> Result<Vec<GoalEvidence>, String> {
    let mut stmt = conn.prepare(
        "SELECT evidence_id, run_id, rule_id, evidence_type, reference, content_hash, verification_state, verifier, detail, created_at FROM goal_evidence WHERE run_id=?1 ORDER BY created_at DESC, evidence_id DESC"
    ).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![run_id], |row| {
            let state: String = row.get(6)?;
            let verification_state =
                VerificationState::parse(&state).ok_or(rusqlite::Error::InvalidQuery)?;
            Ok(GoalEvidence {
                evidence_id: row.get(0)?,
                run_id: row.get(1)?,
                rule_id: row.get(2)?,
                evidence_type: row.get(3)?,
                reference: row.get(4)?,
                content_hash: row.get(5)?,
                verification_state,
                verifier: row.get(7)?,
                detail: row.get(8)?,
                created_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn acquire_lease(conn: &mut Connection, input: AcquireLeaseInput) -> Result<GoalRun, String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, LEASE_OPERATION)? {
        return Ok(saved);
    }
    if !(5..=600).contains(&input.ttl_seconds) {
        return Err("lease TTL 必须为 5–600 秒".into());
    }
    let current = get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if current.revision != input.expected_revision {
        return Err("revision 冲突：Goal Run 已被其他操作更新".into());
    }
    if !matches!(
        current.status,
        GoalRunStatus::Ready | GoalRunStatus::Running
    ) {
        return Err("只有 ready/running Goal 可以获取执行租约".into());
    }
    let owner = input.owner.trim();
    if owner.is_empty() {
        return Err("lease owner 不能为空".into());
    }
    let now = now_ms();
    let active_other: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goal_runs WHERE run_id<>?1 AND status='running' AND lease_expires_at>?2",
        params![&current.run_id, now], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    if active_other > 0 {
        return Err("GOAL-RUNTIME-BUSY: 已有 Goal 正在执行".into());
    }
    if current
        .lease_expires_at
        .is_some_and(|expires| expires > now)
        && current.lease_owner.as_deref() != Some(owner)
    {
        return Err("Goal Run 已被其他执行器租用".into());
    }
    let expires = now + (input.ttl_seconds as i64 * 1_000);
    let mut updated = current.clone();
    updated.status = GoalRunStatus::Running;
    updated.phase = GoalPhase::Observe;
    updated.lease_owner = Some(owner.into());
    updated.lease_expires_at = Some(expires);
    updated.revision += 1;
    updated.updated_at = now;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let changed = tx.execute(
        "UPDATE goal_runs SET status='running', phase='observe', lease_owner=?1, lease_expires_at=?2, revision=?3, updated_at=?4 WHERE run_id=?5 AND revision=?6",
        params![owner, expires, updated.revision as i64, now, &updated.run_id, current.revision as i64],
    ).map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("revision 冲突：无法获取 Goal 租约".into());
    }
    append_event(
        &tx,
        &updated.run_id,
        "goal.lease.acquired",
        owner,
        &json!({"expiresAt": expires}),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        LEASE_OPERATION,
        &updated.run_id,
        &updated,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(updated)
}

pub fn start_attempt(
    conn: &mut Connection,
    input: StartAttemptInput,
) -> Result<(GoalAttempt, GoalRun), String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, START_ATTEMPT_OPERATION)? {
        return Ok(saved);
    }
    let current = get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if current.revision != input.expected_revision {
        return Err("revision 冲突：Goal Run 已被其他操作更新".into());
    }
    if current.status != GoalRunStatus::Running {
        return Err("只有 running Goal 可以开始 attempt".into());
    }
    let goal = crate::work_core::repository::get_object(conn, &current.goal_id)?
        .ok_or_else(|| "Goal 不存在".to_string())?;
    let contract = GoalContract::from_value(&goal.data)?;
    if current.model_calls_used >= contract.budget.max_model_calls {
        return Err("GOAL-BUDGET-MODEL: 模型调用预算已用尽".into());
    }
    if current.tool_calls_used >= contract.budget.max_tool_calls {
        return Err("GOAL-BUDGET-TOOL: 工具调用预算已用尽".into());
    }
    if current.runtime_seconds_used >= contract.budget.max_runtime_seconds {
        return Err("GOAL-BUDGET-RUNTIME: 运行时间预算已用尽".into());
    }
    if input.phase == GoalPhase::Repair && current.repairs_used >= contract.budget.max_repairs {
        return Err("GOAL-BUDGET-REPAIR: 修复预算已用尽".into());
    }
    let executor = input.executor.trim();
    if executor.is_empty() {
        return Err("attempt executor 不能为空".into());
    }
    let now = now_ms();
    let attempt = GoalAttempt {
        attempt_id: format!("goal_attempt_{}", ulid::Ulid::new()),
        run_id: current.run_id.clone(),
        phase: input.phase,
        status: "running".into(),
        executor: executor.into(),
        plan_summary: input.plan_summary,
        result_summary: None,
        tool_receipts: json!([]),
        error_code: None,
        error_detail: None,
        started_at: now,
        finished_at: None,
    };
    let mut updated = current.clone();
    updated.phase = input.phase;
    updated.model_calls_used += 1;
    updated.revision += 1;
    updated.updated_at = now;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO goal_attempts (attempt_id, run_id, phase, status, executor, plan_summary, tool_receipts_json, started_at) VALUES (?1, ?2, ?3, 'running', ?4, ?5, '[]', ?6)",
        params![&attempt.attempt_id, &attempt.run_id, attempt.phase.as_str(), &attempt.executor, &attempt.plan_summary, now],
    ).map_err(|error| error.to_string())?;
    tx.execute("UPDATE goal_runs SET phase=?1, model_calls_used=?2, revision=?3, updated_at=?4 WHERE run_id=?5 AND revision=?6", params![updated.phase.as_str(), updated.model_calls_used as i64, updated.revision as i64, now, &updated.run_id, current.revision as i64]).map_err(|error| error.to_string())?;
    append_event(
        &tx,
        &updated.run_id,
        "goal.attempt.started",
        &input.actor,
        &json!({"attemptId": attempt.attempt_id, "phase": attempt.phase, "executor": attempt.executor}),
        Some(&input.idempotency_key),
        now,
    )?;
    let response = (attempt.clone(), updated.clone());
    save_receipt(
        &tx,
        &input.idempotency_key,
        START_ATTEMPT_OPERATION,
        &attempt.attempt_id,
        &response,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(response)
}

pub fn finish_attempt(
    conn: &mut Connection,
    input: FinishAttemptInput,
) -> Result<(GoalAttempt, GoalRun), String> {
    require_key(&input.idempotency_key)?;
    if let Some(saved) = read_receipt(conn, &input.idempotency_key, FINISH_ATTEMPT_OPERATION)? {
        return Ok(saved);
    }
    let current = get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if current.revision != input.expected_revision {
        return Err("revision 冲突：Goal Run 已被其他操作更新".into());
    }
    let existing = conn.query_row(
        "SELECT attempt_id, run_id, phase, status, executor, plan_summary, result_summary, tool_receipts_json, error_code, error_detail, started_at, finished_at FROM goal_attempts WHERE attempt_id=?1 AND run_id=?2",
        params![&input.attempt_id, &input.run_id], |row| {
            let phase_raw: String = row.get(2)?;
            Ok(GoalAttempt { attempt_id: row.get(0)?, run_id: row.get(1)?, phase: GoalPhase::parse(&phase_raw).ok_or_else(|| enum_error("attempt phase", &phase_raw))?, status: row.get(3)?, executor: row.get(4)?, plan_summary: row.get(5)?, result_summary: row.get(6)?, tool_receipts: parse_json(row.get(7)?, "attempt receipts")?, error_code: row.get(8)?, error_detail: row.get(9)?, started_at: row.get(10)?, finished_at: row.get(11)? })
        },
    ).optional().map_err(|error| error.to_string())?.ok_or_else(|| "Goal attempt 不存在".to_string())?;
    if existing.finished_at.is_some() {
        return Err("Goal attempt 已经结束".into());
    }
    let status = input.status.trim();
    if !matches!(status, "succeeded" | "failed" | "unverified") {
        return Err("attempt status 无效".into());
    }
    let now = now_ms();
    let mut attempt = existing;
    attempt.status = status.into();
    attempt.result_summary = input.result_summary;
    attempt.tool_receipts = input.tool_receipts;
    attempt.error_code = input.error_code;
    attempt.error_detail = input.error_detail;
    attempt.finished_at = Some(now);
    let mut updated = current.clone();
    updated.tool_calls_used = updated
        .tool_calls_used
        .saturating_add(input.tool_calls_used);
    updated.runtime_seconds_used = updated
        .runtime_seconds_used
        .saturating_add(input.runtime_seconds_used);
    if status != "succeeded" {
        updated.repairs_used = updated.repairs_used.saturating_add(1);
    }
    updated.revision += 1;
    updated.updated_at = now;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute("UPDATE goal_attempts SET status=?1, result_summary=?2, tool_receipts_json=?3, error_code=?4, error_detail=?5, finished_at=?6 WHERE attempt_id=?7 AND finished_at IS NULL", params![&attempt.status, &attempt.result_summary, serde_json::to_string(&attempt.tool_receipts).map_err(|error| error.to_string())?, &attempt.error_code, &attempt.error_detail, now, &attempt.attempt_id]).map_err(|error| error.to_string())?;
    tx.execute("UPDATE goal_runs SET tool_calls_used=?1, repairs_used=?2, runtime_seconds_used=?3, revision=?4, updated_at=?5 WHERE run_id=?6 AND revision=?7", params![updated.tool_calls_used as i64, updated.repairs_used as i64, updated.runtime_seconds_used as i64, updated.revision as i64, now, &updated.run_id, current.revision as i64]).map_err(|error| error.to_string())?;
    append_event(
        &tx,
        &updated.run_id,
        "goal.attempt.finished",
        &input.actor,
        &json!({"attemptId": attempt.attempt_id, "status": attempt.status, "toolCalls": input.tool_calls_used}),
        Some(&input.idempotency_key),
        now,
    )?;
    let response = (attempt.clone(), updated.clone());
    save_receipt(
        &tx,
        &input.idempotency_key,
        FINISH_ATTEMPT_OPERATION,
        &attempt.attempt_id,
        &response,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(response)
}

pub fn recover_incomplete_runs(conn: &mut Connection) -> Result<RecoverySummary, String> {
    let now = now_ms();
    let candidates = {
        let mut stmt = conn.prepare(&format!("{RUN_SELECT} WHERE status IN ('running','verifying') AND (lease_expires_at IS NULL OR lease_expires_at<=?1) ORDER BY updated_at")).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![now], run_from_row)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    let mut summary = RecoverySummary {
        recovered_ready: 0,
        blocked_unknown_side_effect: 0,
        untouched: 0,
    };
    for run in candidates {
        let checkpoint = conn.query_row(
            "SELECT checkpoint_type, safe_to_resume FROM goal_checkpoints WHERE run_id=?1 ORDER BY created_at DESC LIMIT 1",
            params![&run.run_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)),
        ).optional().map_err(|error| error.to_string())?;
        let unknown_side_effect = checkpoint
            .as_ref()
            .is_some_and(|(kind, safe)| kind == "pre_action" && !safe);
        let (status, verification, error_code, detail) = if unknown_side_effect {
            summary.blocked_unknown_side_effect += 1;
            (
                GoalRunStatus::Blocked,
                VerificationState::Unverified,
                Some("GOAL-SIDE-EFFECT-UNKNOWN"),
                Some("应用在副作用回执写入前中断，未自动重放"),
            )
        } else {
            summary.recovered_ready += 1;
            (GoalRunStatus::Ready, run.verification_state, None, None)
        };
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        tx.execute(
            "UPDATE goal_runs SET status=?1, phase='observe', verification_state=?2, lease_owner=NULL, lease_expires_at=NULL, recovery_count=recovery_count+1, last_error_code=?3, last_error_detail=?4, next_action=?5, revision=revision+1, updated_at=?6 WHERE run_id=?7 AND revision=?8",
            params![status.as_str(), verification.as_str(), error_code, detail, if unknown_side_effect {"goal.next_confirm_side_effect"} else {"goal.next_resume_checkpoint"}, now, &run.run_id, run.revision as i64],
        ).map_err(|error| error.to_string())?;
        append_event(
            &tx,
            &run.run_id,
            "goal.recovered",
            "bob",
            &json!({"status": status, "unknownSideEffect": unknown_side_effect}),
            None,
            now,
        )?;
        tx.commit().map_err(|error| error.to_string())?;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_runtime::models::{
        default_runtime_choices, CreateEvidenceInput, EvidenceRule, EvidenceRuleKind,
        GoalBlockerPolicy, GoalBudget, GoalCreatedFrom, GoalRecoveryPolicy, GoalRiskPolicy,
        GoalScope,
    };
    use crate::work_core::models::{CreateWorkObjectInput, WorkObjectKind};

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        crate::work_core::init_work_core_tables(&conn).unwrap();
        init_goal_runtime_tables(&conn).unwrap();
        conn
    }

    fn setup_goal(conn: &mut Connection) -> (String, String) {
        let project_id = ensure_personal_workspace(conn).unwrap();
        let contract = GoalContract {
            schema_version: GOAL_RUNTIME_SCHEMA_VERSION,
            original_request: "生成报告".into(),
            outcome: "报告已生成".into(),
            evidence_rules: vec![EvidenceRule {
                rule_id: "report".into(),
                description: "报告文件存在".into(),
                kind: EvidenceRuleKind::Deterministic,
                required: true,
                allowed_evidence_types: vec!["file".into()],
                verifier: json!({"kind":"file_exists"}),
                verification_state: VerificationState::Pending,
            }],
            scope: GoalScope {
                project_id: Some(project_id.clone()),
                allowed_refs: vec![],
                global_file_access: false,
            },
            constraints: vec![],
            budget: GoalBudget::default(),
            risk_policy: GoalRiskPolicy::default(),
            blocker_policy: GoalBlockerPolicy::default(),
            recovery_policy: GoalRecoveryPolicy::default(),
            created_from: GoalCreatedFrom::default(),
        };
        let goal = crate::work_core::repository::create_object(
            conn,
            CreateWorkObjectInput {
                kind: WorkObjectKind::Goal,
                project_id: project_id.clone(),
                parent_id: None,
                title: "生成报告".into(),
                status: Some("ready".into()),
                description: None,
                data: contract.into_value().unwrap(),
                source_capture_id: None,
                actor: Some("user".into()),
                idempotency_key: "goal-object".into(),
            },
        )
        .unwrap();
        (project_id, goal.id)
    }

    fn create_ready_run(conn: &mut Connection) -> GoalRun {
        let (project_id, goal_id) = setup_goal(conn);
        create_run(
            conn,
            CreateRunInput {
                goal_id,
                project_id,
                risk: GoalRisk::R1,
                initial_status: GoalRunStatus::Ready,
                next_action: Some("开始".into()),
                actor: "bob".into(),
                idempotency_key: "run-create".into(),
            },
        )
        .unwrap()
    }

    #[test]
    fn run_creation_is_idempotent_and_one_active_run_per_goal() {
        let mut conn = database();
        let run = create_ready_run(&mut conn);
        let repeated: GoalRun = read_receipt(&conn, "run-create", CREATE_RUN_OPERATION)
            .unwrap()
            .unwrap();
        assert_eq!(run, repeated);
        let error = create_run(
            &mut conn,
            CreateRunInput {
                goal_id: run.goal_id.clone(),
                project_id: run.project_id.clone(),
                risk: GoalRisk::R1,
                initial_status: GoalRunStatus::Ready,
                next_action: None,
                actor: "bob".into(),
                idempotency_key: "second-run".into(),
            },
        )
        .unwrap_err();
        assert!(error.contains("创建 Goal Run 失败"));
    }

    #[test]
    fn done_is_rejected_until_required_evidence_is_verified() {
        let mut conn = database();
        let run = create_ready_run(&mut conn);
        let verifying = transition_run(
            &mut conn,
            TransitionRunInput {
                run_id: run.run_id.clone(),
                status: GoalRunStatus::Running,
                phase: GoalPhase::Act,
                verification_state: VerificationState::Pending,
                expected_revision: run.revision,
                next_action: None,
                error_code: None,
                error_detail: None,
                actor: "bob".into(),
                idempotency_key: "start".into(),
            },
        )
        .unwrap();
        let verifying = transition_run(
            &mut conn,
            TransitionRunInput {
                run_id: run.run_id.clone(),
                status: GoalRunStatus::Verifying,
                phase: GoalPhase::Verify,
                verification_state: VerificationState::Unverified,
                expected_revision: verifying.revision,
                next_action: None,
                error_code: None,
                error_detail: None,
                actor: "bob".into(),
                idempotency_key: "verify".into(),
            },
        )
        .unwrap();
        let error = transition_run(
            &mut conn,
            TransitionRunInput {
                run_id: run.run_id.clone(),
                status: GoalRunStatus::Done,
                phase: GoalPhase::Finish,
                verification_state: VerificationState::Verified,
                expected_revision: verifying.revision,
                next_action: None,
                error_code: None,
                error_detail: None,
                actor: "bob".into(),
                idempotency_key: "done-too-early".into(),
            },
        )
        .unwrap_err();
        assert!(error.contains("GOAL-EVIDENCE-MISSING"));
        let evidence = create_evidence(
            &mut conn,
            CreateEvidenceInput {
                run_id: run.run_id.clone(),
                rule_id: "report".into(),
                evidence_type: "file".into(),
                reference: "D:/report.md".into(),
                content_hash: Some("abc".into()),
                verification_state: VerificationState::Verified,
                verifier: "file_exists".into(),
                detail: None,
                expected_revision: verifying.revision,
                actor: "bob".into(),
                idempotency_key: "evidence".into(),
            },
        )
        .unwrap();
        assert_eq!(evidence.verification_state, VerificationState::Verified);
        let current = get_run(&conn, &run.run_id).unwrap().unwrap();
        let done = transition_run(
            &mut conn,
            TransitionRunInput {
                run_id: run.run_id,
                status: GoalRunStatus::Done,
                phase: GoalPhase::Finish,
                verification_state: VerificationState::Verified,
                expected_revision: current.revision,
                next_action: None,
                error_code: None,
                error_detail: None,
                actor: "bob".into(),
                idempotency_key: "done".into(),
            },
        )
        .unwrap();
        assert_eq!(done.status, GoalRunStatus::Done);
        let work_goal = crate::work_core::repository::get_object(&conn, &done.goal_id)
            .unwrap()
            .unwrap();
        assert_eq!(work_goal.status, "done");
    }

    #[test]
    fn approval_is_structured_and_first_valid_device_decision_wins() {
        let mut conn = database();
        let run = create_ready_run(&mut conn);
        let approval = create_approval(
            &mut conn,
            CreateApprovalInput {
                run_id: run.run_id.clone(),
                summary: "发送报告".into(),
                risk: GoalRisk::R2,
                choices: default_runtime_choices(GoalRisk::R2),
                trusted_device_required: false,
                expires_at: None,
                expected_revision: run.revision,
                actor: "bob".into(),
                idempotency_key: "approval".into(),
            },
        )
        .unwrap();
        let decided = decide_approval(
            &mut conn,
            ApprovalDecisionInput {
                approval_id: approval.approval_id.clone(),
                choice_id: "approve".into(),
                expected_revision: approval.revision,
                actor: "user".into(),
                device_id: "watch-1".into(),
                input_modality: "rotary".into(),
                trusted_device: false,
                idempotency_key: "decision".into(),
            },
        )
        .unwrap();
        assert_eq!(decided.input_modality.as_deref(), Some("rotary"));
        let repeated = decide_approval(
            &mut conn,
            ApprovalDecisionInput {
                approval_id: approval.approval_id,
                choice_id: "approve".into(),
                expected_revision: approval.revision,
                actor: "user".into(),
                device_id: "watch-1".into(),
                input_modality: "rotary".into(),
                trusted_device: false,
                idempotency_key: "decision".into(),
            },
        )
        .unwrap();
        assert_eq!(decided, repeated);
    }

    #[test]
    fn run_and_checkpoint_survive_sqlite_reopen() {
        let root = std::env::temp_dir().join(format!("bob-goal-runtime-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("runtime.db");
        let run_id = {
            let mut conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            crate::work_core::init_work_core_tables(&conn).unwrap();
            init_goal_runtime_tables(&conn).unwrap();
            let run = create_ready_run(&mut conn);
            create_checkpoint(
                &mut conn,
                CreateCheckpointInput {
                    run_id: run.run_id.clone(),
                    phase: GoalPhase::Observe,
                    checkpoint_type: "safe".into(),
                    payload: json!({"next":"plan"}),
                    safe_to_resume: true,
                    expected_revision: run.revision,
                    actor: "bob".into(),
                    idempotency_key: "checkpoint".into(),
                },
            )
            .unwrap();
            run.run_id
        };
        let conn = Connection::open(&path).unwrap();
        let run = get_run(&conn, &run_id).unwrap().unwrap();
        assert!(run.latest_checkpoint_id.is_some());
        assert_eq!(list_events(&conn, &run_id, 50).unwrap().len(), 2);
        drop(conn);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_one_goal_can_hold_the_global_execution_slice() {
        let mut conn = database();
        let first = create_ready_run(&mut conn);
        let first_goal = crate::work_core::repository::get_object(&conn, &first.goal_id)
            .unwrap()
            .unwrap();
        let second_goal = crate::work_core::repository::create_object(
            &mut conn,
            CreateWorkObjectInput {
                kind: WorkObjectKind::Goal,
                project_id: first.project_id.clone(),
                parent_id: None,
                title: "第二个目标".into(),
                status: Some("ready".into()),
                description: None,
                data: first_goal.data,
                source_capture_id: None,
                actor: Some("user".into()),
                idempotency_key: "goal-object-2".into(),
            },
        )
        .unwrap();
        let second = create_run(
            &mut conn,
            CreateRunInput {
                goal_id: second_goal.id,
                project_id: first.project_id.clone(),
                risk: GoalRisk::R1,
                initial_status: GoalRunStatus::Ready,
                next_action: None,
                actor: "bob".into(),
                idempotency_key: "run-create-2".into(),
            },
        )
        .unwrap();
        acquire_lease(
            &mut conn,
            AcquireLeaseInput {
                run_id: first.run_id,
                owner: "desktop-a".into(),
                ttl_seconds: 60,
                expected_revision: first.revision,
                idempotency_key: "lease-1".into(),
            },
        )
        .unwrap();
        let error = acquire_lease(
            &mut conn,
            AcquireLeaseInput {
                run_id: second.run_id,
                owner: "desktop-b".into(),
                ttl_seconds: 60,
                expected_revision: second.revision,
                idempotency_key: "lease-2".into(),
            },
        )
        .unwrap_err();
        assert!(error.contains("GOAL-RUNTIME-BUSY"));
    }

    #[test]
    fn startup_recovery_resumes_only_safe_checkpoints() {
        let mut safe_conn = database();
        let safe = create_ready_run(&mut safe_conn);
        let safe = acquire_lease(
            &mut safe_conn,
            AcquireLeaseInput {
                run_id: safe.run_id,
                owner: "desktop".into(),
                ttl_seconds: 60,
                expected_revision: safe.revision,
                idempotency_key: "safe-lease".into(),
            },
        )
        .unwrap();
        create_checkpoint(
            &mut safe_conn,
            CreateCheckpointInput {
                run_id: safe.run_id.clone(),
                phase: GoalPhase::Observe,
                checkpoint_type: "safe".into(),
                payload: json!({}),
                safe_to_resume: true,
                expected_revision: safe.revision,
                actor: "bob".into(),
                idempotency_key: "safe-checkpoint".into(),
            },
        )
        .unwrap();
        safe_conn
            .execute(
                "UPDATE goal_runs SET lease_expires_at=0 WHERE run_id=?1",
                params![&safe.run_id],
            )
            .unwrap();
        let safe_summary = recover_incomplete_runs(&mut safe_conn).unwrap();
        let safe_recovered = get_run(&safe_conn, &safe.run_id).unwrap().unwrap();
        assert_eq!(safe_summary.recovered_ready, 1);
        assert_eq!(safe_recovered.status, GoalRunStatus::Ready);
        assert_eq!(safe_recovered.recovery_count, 1);

        let mut unsafe_conn = database();
        let unsafe_run = create_ready_run(&mut unsafe_conn);
        let unsafe_run = acquire_lease(
            &mut unsafe_conn,
            AcquireLeaseInput {
                run_id: unsafe_run.run_id,
                owner: "desktop".into(),
                ttl_seconds: 60,
                expected_revision: unsafe_run.revision,
                idempotency_key: "unsafe-lease".into(),
            },
        )
        .unwrap();
        create_checkpoint(
            &mut unsafe_conn,
            CreateCheckpointInput {
                run_id: unsafe_run.run_id.clone(),
                phase: GoalPhase::Act,
                checkpoint_type: "pre_action".into(),
                payload: json!({"sideEffect":"unknown"}),
                safe_to_resume: false,
                expected_revision: unsafe_run.revision,
                actor: "bob".into(),
                idempotency_key: "unsafe-checkpoint".into(),
            },
        )
        .unwrap();
        unsafe_conn
            .execute(
                "UPDATE goal_runs SET lease_expires_at=0 WHERE run_id=?1",
                params![&unsafe_run.run_id],
            )
            .unwrap();
        let unsafe_summary = recover_incomplete_runs(&mut unsafe_conn).unwrap();
        let unsafe_recovered = get_run(&unsafe_conn, &unsafe_run.run_id).unwrap().unwrap();
        assert_eq!(unsafe_summary.blocked_unknown_side_effect, 1);
        assert_eq!(unsafe_recovered.status, GoalRunStatus::Blocked);
        assert_eq!(
            unsafe_recovered.verification_state,
            VerificationState::Unverified
        );
        assert_eq!(
            unsafe_recovered.last_error_code.as_deref(),
            Some("GOAL-SIDE-EFFECT-UNKNOWN")
        );
    }

    #[test]
    fn attempts_persist_bounded_usage_and_receipts() {
        let mut conn = database();
        let run = create_ready_run(&mut conn);
        let run = acquire_lease(
            &mut conn,
            AcquireLeaseInput {
                run_id: run.run_id,
                owner: "desktop".into(),
                ttl_seconds: 60,
                expected_revision: run.revision,
                idempotency_key: "attempt-lease".into(),
            },
        )
        .unwrap();
        let (attempt, run) = start_attempt(
            &mut conn,
            StartAttemptInput {
                run_id: run.run_id.clone(),
                phase: GoalPhase::Act,
                executor: "test".into(),
                plan_summary: Some("生成报告".into()),
                expected_revision: run.revision,
                actor: "bob".into(),
                idempotency_key: "attempt-start".into(),
            },
        )
        .unwrap();
        let (_, updated) = finish_attempt(
            &mut conn,
            FinishAttemptInput {
                attempt_id: attempt.attempt_id,
                run_id: run.run_id.clone(),
                status: "unverified".into(),
                result_summary: Some("缺少文件证据".into()),
                tool_receipts: json!({"total_calls":2}),
                error_code: Some("GOAL-EVIDENCE-UNVERIFIED".into()),
                error_detail: None,
                tool_calls_used: 2,
                runtime_seconds_used: 3,
                expected_revision: run.revision,
                actor: "bob".into(),
                idempotency_key: "attempt-finish".into(),
            },
        )
        .unwrap();
        assert_eq!(updated.model_calls_used, 1);
        assert_eq!(updated.tool_calls_used, 2);
        assert_eq!(updated.repairs_used, 1);
        assert_eq!(updated.runtime_seconds_used, 3);
        assert!(list_events(&conn, &updated.run_id, 50)
            .unwrap()
            .iter()
            .any(|event| event.event_type == "goal.attempt.finished"));
    }
}
