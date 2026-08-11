use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::complexity_router::{
    RouteDecision, RouteDuration, RouteMode, RouteRisk, RouteSource, RouteTaskKind,
};
use crate::db::DbState;
use crate::work_core::models::WorkObject;

use super::models::{
    ApprovalChoiceSemantic, ApprovalDecisionInput, CreateEvidenceInput, GoalApproval, GoalEvent,
    GoalEvidence, GoalPhase, GoalRun, GoalRunStatus, TransitionRunInput, VerificationState,
};
use super::repository;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRuntimeSummary {
    pub run: GoalRun,
    pub goal: WorkObject,
    pub pending_approval: Option<GoalApproval>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRuntimeDetail {
    pub run: GoalRun,
    pub goal: WorkObject,
    pub pending_approval: Option<GoalApproval>,
    pub evidence: Vec<GoalEvidence>,
    pub events: Vec<GoalEvent>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRunActionInput {
    pub run_id: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecisionOutcome {
    pub approval: GoalApproval,
    pub run: GoalRun,
}

fn get_goal(conn: &rusqlite::Connection, run: &GoalRun) -> Result<WorkObject, String> {
    crate::work_core::repository::get_object(conn, &run.goal_id)?
        .ok_or_else(|| format!("Goal 不存在: {}", run.goal_id))
}

fn summary(conn: &rusqlite::Connection, run: GoalRun) -> Result<GoalRuntimeSummary, String> {
    let goal = get_goal(conn, &run)?;
    let pending_approval = repository::get_pending_approval(conn, &run.run_id)?;
    Ok(GoalRuntimeSummary {
        run,
        goal,
        pending_approval,
    })
}

#[tauri::command]
pub fn goal_runtime_list(
    db: State<'_, DbState>,
    project_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<GoalRuntimeSummary>, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    repository::list_runs(&conn, project_id.as_deref(), limit.unwrap_or(50))?
        .into_iter()
        .map(|run| summary(&conn, run))
        .collect()
}

#[tauri::command]
pub fn goal_runtime_get(
    db: State<'_, DbState>,
    run_id: String,
) -> Result<GoalRuntimeDetail, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    let run =
        repository::get_run(&conn, &run_id)?.ok_or_else(|| format!("Goal Run 不存在: {run_id}"))?;
    let goal = get_goal(&conn, &run)?;
    let pending_approval = repository::get_pending_approval(&conn, &run.run_id)?;
    let evidence = repository::list_evidence(&conn, &run.run_id)?;
    let events = repository::list_events(&conn, &run.run_id, 50)?;
    Ok(GoalRuntimeDetail {
        run,
        goal,
        pending_approval,
        evidence,
        events,
    })
}

#[tauri::command]
pub fn goal_runtime_list_events(
    db: State<'_, DbState>,
    run_id: String,
    limit: Option<usize>,
) -> Result<Vec<GoalEvent>, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    repository::list_events(&conn, &run_id, limit.unwrap_or(50))
}

fn transition_action(
    conn: &mut rusqlite::Connection,
    input: GoalRunActionInput,
    status: GoalRunStatus,
    next_action: Option<&str>,
    operation: &str,
) -> Result<GoalRun, String> {
    let current = repository::get_run(conn, &input.run_id)?
        .ok_or_else(|| format!("Goal Run 不存在: {}", input.run_id))?;
    if repository::get_pending_approval(conn, &input.run_id)?.is_some()
        && status == GoalRunStatus::Ready
    {
        return Err("GOAL-APPROVAL-PENDING: 请先处理当前选择".into());
    }
    if status == GoalRunStatus::Ready && current.risk.requires_approval() {
        let approval = repository::get_latest_resolved_approval(conn, &input.run_id)?
            .ok_or_else(|| "GOAL-APPROVAL-REQUIRED: 此目标尚未获得有效批准".to_string())?;
        let approved = approval
            .selected_choice_id
            .as_deref()
            .and_then(|choice_id| {
                approval
                    .choices
                    .iter()
                    .find(|choice| choice.choice_id == choice_id)
            })
            .is_some_and(|choice| {
                matches!(
                    choice.semantic,
                    ApprovalChoiceSemantic::Approve | ApprovalChoiceSemantic::SelectOption
                )
            });
        if !approved {
            return Err(
                "GOAL-TRUSTED-HANDOFF-PENDING: 交接不等于批准，请在可信且已解锁的设备上完成确认"
                    .into(),
            );
        }
    }
    repository::transition_run(
        conn,
        TransitionRunInput {
            run_id: input.run_id,
            status,
            phase: if status == GoalRunStatus::Ready {
                GoalPhase::Observe
            } else {
                current.phase
            },
            verification_state: current.verification_state,
            expected_revision: input.expected_revision,
            next_action: next_action.map(str::to_string),
            error_code: if status == GoalRunStatus::Ready {
                None
            } else {
                current.last_error_code
            },
            error_detail: if status == GoalRunStatus::Ready {
                None
            } else {
                current.last_error_detail
            },
            actor: "user".into(),
            idempotency_key: format!("{}:{}", operation, input.idempotency_key),
        },
    )
}

fn route_for(run: &GoalRun, contract: &super::models::GoalContract) -> RouteDecision {
    let task_kind =
        if contract.evidence_rules.iter().any(|rule| {
            rule.required && rule.kind == super::models::EvidenceRuleKind::UserAcceptance
        }) {
            RouteTaskKind::Answer
        } else {
            RouteTaskKind::Action
        };
    let risk = match run.risk {
        super::models::GoalRisk::R0 => RouteRisk::R0,
        super::models::GoalRisk::R1 => RouteRisk::R1,
        super::models::GoalRisk::R2 => RouteRisk::R2,
        super::models::GoalRisk::R3 => RouteRisk::R3,
    };
    RouteDecision {
        mode: RouteMode::Advanced,
        task_kind,
        confidence: 1.0,
        risk,
        duration: RouteDuration::Persistent,
        source: RouteSource::Override,
        reason_codes: vec!["goal_runtime_resume".into()],
        requires_project_state: true,
        semantic_fallback_recommended: false,
    }
}

pub async fn continue_existing_run(
    app: AppHandle,
    input: GoalRunActionInput,
) -> Result<Value, String> {
    let (contract, decision) = {
        let state = app.state::<DbState>();
        let mut conn = state.0.lock().map_err(|error| error.to_string())?;
        let run = transition_action(
            &mut conn,
            input,
            GoalRunStatus::Ready,
            Some("goal.next_running"),
            "goal-ui-continue",
        )?;
        let goal = get_goal(&conn, &run)?;
        let contract = super::models::GoalContract::from_value(&goal.data)?;
        let decision = route_for(&run, &contract);
        (contract, decision)
    };
    let request = contract.original_request.clone();
    let messages = vec![serde_json::json!({"role":"user", "content": request})];
    Ok(super::engine::execute_advanced_request(
        app,
        messages,
        contract.created_from.conversation_id.clone(),
        Some("user".into()),
        contract.scope.global_file_access,
        decision,
    )
    .await)
}

pub async fn resume_startup_runs(app: AppHandle) {
    let candidates = {
        let state = app.state::<DbState>();
        let conn = match state.0.lock() {
            Ok(conn) => conn,
            Err(error) => {
                log::warn!("Goal Runtime startup resume lock failed: {error}");
                return;
            }
        };
        let runs = match repository::list_runs(&conn, None, 200) {
            Ok(runs) => runs,
            Err(error) => {
                log::warn!("Goal Runtime startup resume scan failed: {error}");
                return;
            }
        };
        runs.into_iter()
            .filter_map(|run| {
                if run.status != GoalRunStatus::Ready
                    || run.recovery_count == 0
                    || !matches!(
                        run.risk,
                        super::models::GoalRisk::R0 | super::models::GoalRisk::R1
                    )
                {
                    return None;
                }
                let goal = get_goal(&conn, &run).ok()?;
                let contract = super::models::GoalContract::from_value(&goal.data).ok()?;
                contract.recovery_policy.resume_r0_r1_on_startup.then_some((
                    run.run_id,
                    run.revision,
                    run.recovery_count,
                ))
            })
            .collect::<Vec<_>>()
    };
    for (run_id, expected_revision, recovery_count) in candidates {
        let input = GoalRunActionInput {
            run_id: run_id.clone(),
            expected_revision,
            idempotency_key: format!("startup:{run_id}:recovery:{recovery_count}"),
        };
        if let Err(error) = continue_existing_run(app.clone(), input).await {
            log::warn!("Goal Runtime startup resume deferred for {run_id}: {error}");
        }
    }
}

#[tauri::command]
pub async fn goal_runtime_continue(
    app: AppHandle,
    input: GoalRunActionInput,
) -> Result<Value, String> {
    continue_existing_run(app, input).await
}

#[tauri::command]
pub fn goal_runtime_defer(
    db: State<'_, DbState>,
    input: GoalRunActionInput,
) -> Result<GoalRun, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    transition_action(
        &mut conn,
        input,
        GoalRunStatus::WaitingUser,
        Some("goal.next_deferred"),
        "goal-ui-defer",
    )
}

#[tauri::command]
pub fn goal_runtime_cancel(
    db: State<'_, DbState>,
    input: GoalRunActionInput,
) -> Result<GoalRun, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    transition_action(
        &mut conn,
        input,
        GoalRunStatus::Cancelled,
        Some("goal.next_cancelled"),
        "goal-ui-cancel",
    )
}

#[tauri::command]
pub fn goal_runtime_decide_approval(
    app: AppHandle,
    db: State<'_, DbState>,
    input: ApprovalDecisionInput,
) -> Result<ApprovalDecisionOutcome, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    let current = repository::get_approval(&conn, &input.approval_id)?
        .ok_or_else(|| format!("审批不存在: {}", input.approval_id))?;
    let choice = current
        .choices
        .iter()
        .find(|choice| choice.choice_id == input.choice_id)
        .cloned()
        .ok_or_else(|| "审批选项不存在".to_string())?;
    let decision_key = input.idempotency_key.clone();
    let approval = repository::decide_approval(&mut conn, input)?;
    let mut run = repository::get_run(&conn, &approval.run_id)?
        .ok_or_else(|| "Goal Run 不存在".to_string())?;

    let purpose = choice.payload.get("purpose").and_then(Value::as_str);
    if choice.semantic == ApprovalChoiceSemantic::Approve && purpose == Some("user_acceptance") {
        let rule_id = choice
            .payload
            .get("ruleId")
            .and_then(Value::as_str)
            .unwrap_or("user_acceptance");
        run = repository::transition_run(
            &mut conn,
            TransitionRunInput {
                run_id: run.run_id.clone(),
                status: GoalRunStatus::Verifying,
                phase: GoalPhase::Verify,
                verification_state: VerificationState::Pending,
                expected_revision: run.revision,
                next_action: Some("goal.next_record_acceptance".into()),
                error_code: None,
                error_detail: None,
                actor: "user".into(),
                idempotency_key: format!("goal-accept-verifying:{decision_key}"),
            },
        )?;
        repository::create_evidence(
            &mut conn,
            CreateEvidenceInput {
                run_id: run.run_id.clone(),
                rule_id: rule_id.into(),
                evidence_type: "user_acceptance".into(),
                reference: format!("approval:{}", approval.approval_id),
                content_hash: None,
                verification_state: VerificationState::Verified,
                verifier: "user".into(),
                detail: Some("用户通过结构化选项确认结果满足目标".into()),
                expected_revision: run.revision,
                actor: "user".into(),
                idempotency_key: format!("goal-accept-evidence:{decision_key}"),
            },
        )?;
        run = repository::get_run(&conn, &run.run_id)?
            .ok_or_else(|| "Goal Run 不存在".to_string())?;
        run = repository::transition_run(
            &mut conn,
            TransitionRunInput {
                run_id: run.run_id.clone(),
                status: GoalRunStatus::Done,
                phase: GoalPhase::Finish,
                verification_state: VerificationState::Verified,
                expected_revision: run.revision,
                next_action: None,
                error_code: None,
                error_detail: None,
                actor: "user".into(),
                idempotency_key: format!("goal-accept-done:{decision_key}"),
            },
        )?;
    }
    let outcome = ApprovalDecisionOutcome { approval, run };
    drop(conn);
    let _ = app.emit(
        "goal:runtime-state",
        serde_json::json!({"run": &outcome.run}),
    );
    if outcome.run.verification_state == VerificationState::Verified {
        let _ = app.emit(
            "goal:evidence-updated",
            serde_json::json!({"runId": outcome.run.run_id}),
        );
    }
    Ok(outcome)
}
