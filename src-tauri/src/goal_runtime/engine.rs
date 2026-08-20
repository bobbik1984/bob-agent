use serde::Serialize;
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

use crate::assistant_context::{
    resolve_context_from_work_core, AssistantContext, ContextSourceSnapshot,
};
use crate::complexity_router::{RouteDecision, RouteTaskKind};
use crate::db::DbState;
use crate::work_core::models::{CreateWorkObjectInput, WorkObjectKind};

use super::compiler;
use super::models::{
    default_runtime_choices, AcquireLeaseInput, ActionChoice, ApprovalChoiceSemantic,
    CreateApprovalInput, CreateCheckpointInput, CreateEvidenceInput, CreateRunInput,
    FinishAttemptInput, GoalPhase, GoalRisk, GoalRun, GoalRunStatus, StartAttemptInput,
    TransitionRunInput, VerificationState,
};
use super::{repository, verifier};

fn stable_key(parts: &[&str]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn language_is_english() -> bool {
    crate::read_config()
        .get("language")
        .and_then(Value::as_str)
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("en"))
}

fn status_message(status: GoalRunStatus) -> &'static str {
    if language_is_english() {
        match status {
            GoalRunStatus::WaitingUser => {
                "A persistent goal was created and is waiting for your choice."
            }
            GoalRunStatus::Done => "The goal completed and its required evidence was verified.",
            GoalRunStatus::Blocked => {
                "The goal is saved, but execution is blocked. Review the reason before continuing."
            }
            GoalRunStatus::Ready | GoalRunStatus::Running | GoalRunStatus::Verifying => {
                "The goal is saved and execution is in progress."
            }
            GoalRunStatus::Draft => "The goal was saved as a draft and needs more information.",
            GoalRunStatus::Failed => "The goal stopped after reaching its execution limit.",
            GoalRunStatus::Cancelled => "The goal was cancelled.",
        }
    } else {
        match status {
            GoalRunStatus::WaitingUser => "已创建持久目标，正在等待你的选择。",
            GoalRunStatus::Done => "目标已完成，且必需证据已通过验证。",
            GoalRunStatus::Blocked => "目标已经保存，但执行被阻塞；继续前请查看具体原因。",
            GoalRunStatus::Ready | GoalRunStatus::Running | GoalRunStatus::Verifying => {
                "目标已经保存并正在执行。"
            }
            GoalRunStatus::Draft => "目标已保存为草稿，还需要补充关键信息。",
            GoalRunStatus::Failed => "目标达到执行上限后停止。",
            GoalRunStatus::Cancelled => "目标已取消。",
        }
    }
}

fn emit_state(app: &AppHandle, run: &GoalRun) {
    let _ = app.emit("goal:runtime-state", json!({ "run": run }));
}

fn refreshed_run(conn: &rusqlite::Connection, current: GoalRun) -> GoalRun {
    let run_id = current.run_id.clone();
    repository::get_run(conn, &run_id)
        .ok()
        .flatten()
        .unwrap_or(current)
}

fn goal_title(outcome: &str) -> String {
    let title = outcome.trim().chars().take(80).collect::<String>();
    if title.is_empty() {
        "持续目标".into()
    } else {
        title
    }
}

fn approval_choices_for_information() -> Vec<ActionChoice> {
    vec![
        ActionChoice {
            choice_id: "continue_safe".into(),
            label_key: "goal.approval_continue_safe".into(),
            semantic: ApprovalChoiceSemantic::Approve,
            payload: json!({"purpose":"continue_safe"}),
        },
        ActionChoice {
            choice_id: "defer".into(),
            label_key: "goal.approval_defer".into(),
            semantic: ApprovalChoiceSemantic::Defer,
            payload: json!({}),
        },
        ActionChoice {
            choice_id: "cancel".into(),
            label_key: "goal.approval_cancel".into(),
            semantic: ApprovalChoiceSemantic::Reject,
            payload: json!({}),
        },
    ]
}

fn approval_choices_for_acceptance() -> Vec<ActionChoice> {
    vec![
        ActionChoice {
            choice_id: "accept_result".into(),
            label_key: "goal.approval_accept_result".into(),
            semantic: ApprovalChoiceSemantic::Approve,
            payload: json!({"purpose":"user_acceptance","ruleId":"user_acceptance"}),
        },
        ActionChoice {
            choice_id: "continue".into(),
            label_key: "goal.approval_continue".into(),
            semantic: ApprovalChoiceSemantic::Defer,
            payload: json!({"purpose":"continue_work"}),
        },
        ActionChoice {
            choice_id: "cancel".into(),
            label_key: "goal.approval_cancel".into(),
            semantic: ApprovalChoiceSemantic::Reject,
            payload: json!({}),
        },
    ]
}

fn response_value(
    content: String,
    route: &RouteDecision,
    run: &GoalRun,
    approval: Option<Value>,
) -> Value {
    json!({
        "content": content,
        "thinking": Value::Null,
        "usage": Value::Null,
        "pricing": { "input": 0.0, "output": 0.0 },
        "model": "goal-runtime",
        "route": route,
        "goal": {
            "goalId": run.goal_id,
            "runId": run.run_id,
            "projectId": run.project_id,
            "status": run.status,
            "phase": run.phase,
            "verificationState": run.verification_state,
            "nextAction": run.next_action,
            "approval": approval,
        },
        "resultReceipt": {
            "decisionId": run.run_id,
            "status": run.status,
            "verifiedEvidence": [],
            "stateChanges": [],
            "sideEffectState": "none",
            "correctionRefs": [],
            "completedAt": run.finished_at,
        },
        "tool_summary": { "total_calls": 0, "total_failures": 0, "calls": [] },
    })
}

fn attach_execution_metadata(response: &mut Value, execution: &Value) {
    for field in ["usage", "pricing", "model", "tool_summary", "resultReceipt"] {
        if let Some(value) = execution.get(field) {
            response[field] = value.clone();
        }
    }
}

fn runtime_error_response(error: impl Into<String>, route: &RouteDecision, run: &GoalRun) -> Value {
    let mut response = response_value(status_message(run.status).into(), route, run, None);
    let error = error.into();
    response["runtimeError"] = Value::String(error.clone());
    response["goal"]["runtimeError"] = Value::String(error);
    response
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectCandidate {
    project_id: String,
    title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GoalProjectResolution {
    Selected(String),
    Personal,
    Ambiguous(Vec<ProjectCandidate>),
}

fn goal_project_resolution(
    context: &AssistantContext,
    snapshot: &ContextSourceSnapshot,
) -> GoalProjectResolution {
    if let Some(conflict) = context.conflicts.first() {
        let candidates = conflict
            .candidate_ids
            .iter()
            .filter_map(|project_id| {
                snapshot
                    .projects
                    .iter()
                    .find(|project| project.id == *project_id)
                    .map(|project| ProjectCandidate {
                        project_id: project.id.clone(),
                        title: project.title.clone(),
                    })
            })
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            return GoalProjectResolution::Ambiguous(candidates);
        }
    }
    context
        .active_object
        .as_ref()
        .map(|object| GoalProjectResolution::Selected(object.project_id.clone()))
        .unwrap_or(GoalProjectResolution::Personal)
}

fn ambiguity_response(candidates: Vec<ProjectCandidate>, route: &RouteDecision) -> Value {
    let names = candidates
        .iter()
        .map(|candidate| candidate.title.as_str())
        .collect::<Vec<_>>()
        .join(if language_is_english() { ", " } else { "、" });
    let content = if language_is_english() {
        format!("I found several possible projects ({names}). Which one should I continue?")
    } else {
        format!("我找到了多个可能的项目（{names}）。你希望我继续哪一个？")
    };
    json!({
        "content": content,
        "thinking": Value::Null,
        "usage": Value::Null,
        "pricing": { "input": 0.0, "output": 0.0 },
        "model": "goal-runtime",
        "route": route,
        "needsClarification": true,
        "clarification": {
            "reasonCode": "context.ambiguous_candidates",
            "candidates": candidates,
        },
        "tool_summary": { "total_calls": 0, "total_failures": 0, "calls": [] },
    })
}

pub async fn execute_advanced_request(
    app: AppHandle,
    messages: Vec<Value>,
    conv_id: Option<String>,
    from_user: Option<String>,
    global_file_access: bool,
    decision: RouteDecision,
) -> Value {
    let request = crate::complexity_router::last_user_text(&messages);
    let project_id = {
        let Some(state) = app.try_state::<DbState>() else {
            return json!({"content": status_message(GoalRunStatus::Failed), "error": "GOAL-DB-UNAVAILABLE", "route": decision});
        };
        let mut conn = match state.0.lock() {
            Ok(conn) => conn,
            Err(error) => {
                return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
            }
        };
        let resolution =
            match resolve_context_from_work_core(&conn, &request, decision.mode, crate::now_ms()) {
                Ok((_purpose, snapshot, context)) => goal_project_resolution(&context, &snapshot),
                Err(error) => {
                    log::warn!("[GoalRuntime] context resolution unavailable: {error}");
                    GoalProjectResolution::Personal
                }
            };
        match resolution {
            GoalProjectResolution::Selected(project_id) => project_id,
            GoalProjectResolution::Personal => {
                match repository::ensure_personal_workspace(&mut conn) {
                    Ok(project_id) => project_id,
                    Err(error) => {
                        return json!({"content": status_message(GoalRunStatus::Failed), "error": error, "route": decision})
                    }
                }
            }
            GoalProjectResolution::Ambiguous(candidates) => {
                return ambiguity_response(candidates, &decision)
            }
        }
    };

    let compilation =
        compiler::compile_contract(&request, &decision, &project_id, conv_id.clone()).await;
    let contract = compilation.contract;
    let risk = contract.risk_policy.max_auto_risk;
    let initial_status = if compilation.needs_user_input || risk.requires_approval() {
        GoalRunStatus::WaitingUser
    } else if contract.ready_for_execution() {
        GoalRunStatus::Ready
    } else {
        GoalRunStatus::Draft
    };
    let request_key = stable_key(&[conv_id.as_deref().unwrap_or("no-conversation"), &request]);
    let (mut run, approval_value) = {
        let Some(state) = app.try_state::<DbState>() else {
            return json!({"content": status_message(GoalRunStatus::Failed), "error": "GOAL-DB-UNAVAILABLE", "route": decision});
        };
        let mut conn = match state.0.lock() {
            Ok(conn) => conn,
            Err(error) => {
                return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
            }
        };
        let goal = match crate::work_core::repository::create_object(
            &mut conn,
            CreateWorkObjectInput {
                kind: WorkObjectKind::Goal,
                project_id: project_id.clone(),
                parent_id: None,
                title: goal_title(&contract.outcome),
                status: Some(if initial_status == GoalRunStatus::Draft {
                    "draft".into()
                } else {
                    "ready".into()
                }),
                description: compilation.warning_code.clone(),
                data: match contract.clone().into_value() {
                    Ok(value) => value,
                    Err(error) => {
                        return json!({"content": status_message(GoalRunStatus::Failed), "error": error, "route": decision})
                    }
                },
                source_capture_id: None,
                actor: Some("bob".into()),
                idempotency_key: format!("goal-runtime-object-{request_key}"),
            },
        ) {
            Ok(goal) => goal,
            Err(error) => {
                return json!({"content": status_message(GoalRunStatus::Failed), "error": error, "route": decision})
            }
        };
        let created_run = match repository::create_run(
            &mut conn,
            CreateRunInput {
                goal_id: goal.id,
                project_id: project_id.clone(),
                risk,
                initial_status,
                next_action: Some(if initial_status == GoalRunStatus::WaitingUser {
                    "goal.next_waiting_choice".into()
                } else {
                    "goal.next_observe".into()
                }),
                actor: "bob".into(),
                idempotency_key: format!("goal-runtime-run-{request_key}"),
            },
        ) {
            Ok(run) => run,
            Err(error) => {
                return json!({"content": status_message(GoalRunStatus::Failed), "error": error, "route": decision})
            }
        };
        // Creation receipts intentionally preserve the original response. Always
        // re-read the mutable Run before deciding whether an idempotent replay
        // should execute, wait, or simply return its terminal state.
        let mut run = refreshed_run(&conn, created_run);
        if run.status.is_terminal() {
            return response_value(status_message(run.status).into(), &decision, &run, None);
        }
        let approval = if initial_status == GoalRunStatus::WaitingUser {
            let choices = if compilation.needs_user_input {
                approval_choices_for_information()
            } else {
                default_runtime_choices(risk)
            };
            match repository::create_approval(
                &mut conn,
                CreateApprovalInput {
                    run_id: run.run_id.clone(),
                    summary: if compilation.needs_user_input {
                        "goal.summary_need_choice".into()
                    } else {
                        "goal.summary_confirm_execution".into()
                    },
                    risk,
                    choices,
                    trusted_device_required: risk == GoalRisk::R3,
                    expires_at: None,
                    expected_revision: run.revision,
                    actor: "bob".into(),
                    idempotency_key: format!("goal-runtime-approval-{request_key}"),
                },
            ) {
                Ok(approval) => {
                    run = refreshed_run(&conn, run);
                    Some(json!(approval))
                }
                Err(error) => {
                    return json!({"content": status_message(GoalRunStatus::Failed), "error": error, "route": decision})
                }
            }
        } else {
            None
        };
        (run, approval)
    };
    emit_state(&app, &run);
    if let Some(approval) = approval_value.as_ref() {
        let _ = app.emit("goal:approval-required", approval.clone());
    }
    if run.status != GoalRunStatus::Ready {
        return response_value(
            status_message(run.status).into(),
            &decision,
            &run,
            approval_value,
        );
    }

    let owner = format!("desktop-{}", std::process::id());
    // Include the current revision so a deliberate retry after blocked/deferred
    // starts a fresh bounded slice, while an exact replay remains idempotent.
    let execution_key = format!(
        "{request_key}-recovery-{}-revision-{}",
        run.recovery_count, run.revision
    );
    run = {
        let state = app.state::<DbState>();
        let mut conn = match state.0.lock() {
            Ok(conn) => conn,
            Err(error) => {
                return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
            }
        };
        match repository::acquire_lease(
            &mut conn,
            AcquireLeaseInput {
                run_id: run.run_id.clone(),
                owner: owner.clone(),
                ttl_seconds: 300,
                expected_revision: run.revision,
                idempotency_key: format!("goal-runtime-lease-{execution_key}"),
            },
        ) {
            Ok(run) => run,
            Err(error) => return runtime_error_response(error, &decision, &run),
        }
    };
    emit_state(&app, &run);

    run = {
        let state = app.state::<DbState>();
        let mut conn = match state.0.lock() {
            Ok(conn) => conn,
            Err(error) => return runtime_error_response(error.to_string(), &decision, &run),
        };
        match repository::transition_run(
            &mut conn,
            TransitionRunInput {
                run_id: run.run_id.clone(),
                status: GoalRunStatus::Running,
                phase: GoalPhase::Plan,
                verification_state: run.verification_state,
                expected_revision: run.revision,
                next_action: Some("goal.next_plan_slice".into()),
                error_code: None,
                error_detail: None,
                actor: "bob".into(),
                idempotency_key: format!("goal-runtime-plan-{execution_key}"),
            },
        ) {
            Ok(run) => run,
            Err(error) => return runtime_error_response(error, &decision, &run),
        }
    };
    emit_state(&app, &run);

    let needs_user_acceptance = contract
        .evidence_rules
        .iter()
        .any(|rule| rule.required && rule.kind == super::models::EvidenceRuleKind::UserAcceptance);
    let max_attempts = if needs_user_acceptance {
        1
    } else {
        contract.budget.max_repairs.saturating_add(1).min(3)
    };
    let mut execution_messages = messages;
    let mut final_result = json!({});
    let mut verified = false;
    for attempt_index in 0..max_attempts {
        let checkpoint = {
            let state = app.state::<DbState>();
            let mut conn = match state.0.lock() {
                Ok(conn) => conn,
                Err(error) => {
                    return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                }
            };
            repository::create_checkpoint(
                &mut conn,
                CreateCheckpointInput {
                    run_id: run.run_id.clone(),
                    phase: if attempt_index == 0 {
                        GoalPhase::Act
                    } else {
                        GoalPhase::Repair
                    },
                    checkpoint_type: "pre_action".into(),
                    payload: json!({"attempt": attempt_index + 1}),
                    safe_to_resume: decision.task_kind == RouteTaskKind::Answer,
                    expected_revision: run.revision,
                    actor: "bob".into(),
                    idempotency_key: format!(
                        "goal-runtime-checkpoint-{execution_key}-{attempt_index}"
                    ),
                },
            )
        };
        if let Err(error) = checkpoint {
            return runtime_error_response(error, &decision, &run);
        }
        run = {
            let state = app.state::<DbState>();
            let conn = match state.0.lock() {
                Ok(conn) => conn,
                Err(error) => {
                    return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                }
            };
            refreshed_run(&conn, run)
        };
        let (attempt, started_run) = {
            let state = app.state::<DbState>();
            let mut conn = match state.0.lock() {
                Ok(conn) => conn,
                Err(error) => {
                    return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                }
            };
            match repository::start_attempt(
                &mut conn,
                StartAttemptInput {
                    run_id: run.run_id.clone(),
                    phase: if attempt_index == 0 {
                        GoalPhase::Act
                    } else {
                        GoalPhase::Repair
                    },
                    executor: "api-agent".into(),
                    plan_summary: Some(contract.outcome.clone()),
                    expected_revision: run.revision,
                    actor: "bob".into(),
                    idempotency_key: format!(
                        "goal-runtime-attempt-start-{execution_key}-{attempt_index}"
                    ),
                },
            ) {
                Ok(value) => value,
                Err(error) => return runtime_error_response(error, &decision, &run),
            }
        };
        run = started_run;
        let started = Instant::now();
        let slice = tokio::time::timeout(
            std::time::Duration::from_secs(contract.budget.max_slice_seconds.max(1)),
            crate::llm::stream_internal(
                app.clone(),
                execution_messages.clone(),
                conv_id.clone(),
                from_user.clone(),
                global_file_access,
                if decision.task_kind == RouteTaskKind::Answer {
                    "goal_runtime_read".into()
                } else {
                    "goal_runtime_action".into()
                },
            ),
        )
        .await;
        let slice_timed_out = slice.is_err();
        final_result = slice.unwrap_or_else(|_| {
            json!({
                "content": "", "error": "GOAL-SLICE-TIMEOUT",
                "tool_summary": {"total_calls":0,"total_failures":0,"calls":[]}
            })
        });
        let tool_summary = final_result
            .get("tool_summary")
            .cloned()
            .unwrap_or_else(|| json!({"total_calls":0,"total_failures":0,"calls":[]}));
        let tool_calls = tool_summary
            .get("total_calls")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;
        let rule = contract.evidence_rules.iter().find(|rule| rule.required);
        let verification = rule.map(|rule| verifier::verify_tool_summary(rule, &tool_summary));
        let recovery_action = crate::execution_error::recovery_from_tool_summary(
            &tool_summary,
            verification
                .as_ref()
                .is_none_or(|outcome| outcome.state != VerificationState::Verified),
            attempt_index,
        );
        let attempt_status = if verification
            .as_ref()
            .is_some_and(|outcome| outcome.state == VerificationState::Verified)
        {
            "succeeded"
        } else if tool_summary
            .get("total_failures")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        {
            "failed"
        } else {
            "unverified"
        };
        run = {
            let state = app.state::<DbState>();
            let mut conn = match state.0.lock() {
                Ok(conn) => conn,
                Err(error) => {
                    return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                }
            };
            match repository::finish_attempt(
                &mut conn,
                FinishAttemptInput {
                    attempt_id: attempt.attempt_id,
                    run_id: run.run_id.clone(),
                    status: attempt_status.into(),
                    result_summary: final_result
                        .get("content")
                        .and_then(Value::as_str)
                        .map(|value| value.chars().take(1_000).collect()),
                    tool_receipts: tool_summary.clone(),
                    error_code: if attempt_status == "succeeded" {
                        None
                    } else if slice_timed_out {
                        Some("GOAL-SLICE-TIMEOUT".into())
                    } else {
                        Some("GOAL-EVIDENCE-UNVERIFIED".into())
                    },
                    error_detail: if slice_timed_out {
                        Some("单次执行已达到时间上限".into())
                    } else {
                        verification.as_ref().map(|outcome| outcome.detail.clone())
                    },
                    tool_calls_used: tool_calls,
                    runtime_seconds_used: started.elapsed().as_secs(),
                    expected_revision: run.revision,
                    actor: "bob".into(),
                    idempotency_key: format!(
                        "goal-runtime-attempt-finish-{execution_key}-{attempt_index}"
                    ),
                },
            ) {
                Ok((_, run)) => run,
                Err(error) => return runtime_error_response(error, &decision, &run),
            }
        };
        if let (Some(rule), Some(outcome)) = (rule, verification) {
            if outcome.state == VerificationState::Verified {
                run = {
                    let state = app.state::<DbState>();
                    let mut conn = match state.0.lock() {
                        Ok(conn) => conn,
                        Err(error) => {
                            return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                        }
                    };
                    match repository::transition_run(
                        &mut conn,
                        TransitionRunInput {
                            run_id: run.run_id.clone(),
                            status: GoalRunStatus::Verifying,
                            phase: GoalPhase::Verify,
                            verification_state: VerificationState::Verified,
                            expected_revision: run.revision,
                            next_action: Some("goal.next_verify_evidence".into()),
                            error_code: None,
                            error_detail: None,
                            actor: "bob".into(),
                            idempotency_key: format!("goal-runtime-verifying-{execution_key}"),
                        },
                    ) {
                        Ok(run) => run,
                        Err(error) => return runtime_error_response(error, &decision, &run),
                    }
                };
                run = {
                    let state = app.state::<DbState>();
                    let mut conn = match state.0.lock() {
                        Ok(conn) => conn,
                        Err(error) => {
                            return runtime_error_response(error.to_string(), &decision, &run)
                        }
                    };
                    match repository::create_checkpoint(
                        &mut conn,
                        CreateCheckpointInput {
                            run_id: run.run_id.clone(),
                            phase: GoalPhase::Verify,
                            checkpoint_type: "post_action".into(),
                            payload: json!({
                                "attempt": attempt_index + 1,
                                "ruleId": rule.rule_id,
                                "verificationState": "verified"
                            }),
                            safe_to_resume: true,
                            expected_revision: run.revision,
                            actor: "bob".into(),
                            idempotency_key: format!(
                                "goal-runtime-checkpoint-complete-{execution_key}-{attempt_index}"
                            ),
                        },
                    ) {
                        Ok(_) => refreshed_run(&conn, run),
                        Err(error) => return runtime_error_response(error, &decision, &run),
                    }
                };
                run = {
                    let state = app.state::<DbState>();
                    let mut conn = match state.0.lock() {
                        Ok(conn) => conn,
                        Err(error) => {
                            return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                        }
                    };
                    match repository::create_evidence(
                        &mut conn,
                        CreateEvidenceInput {
                            run_id: run.run_id.clone(),
                            rule_id: rule.rule_id.clone(),
                            evidence_type: "tool_receipt".into(),
                            reference: format!("attempt:{}", attempt_index + 1),
                            content_hash: None,
                            verification_state: VerificationState::Verified,
                            verifier: outcome.verifier,
                            detail: Some(outcome.detail),
                            expected_revision: run.revision,
                            actor: "bob".into(),
                            idempotency_key: format!("goal-runtime-evidence-{execution_key}"),
                        },
                    ) {
                        Ok(evidence) => {
                            let _ =
                                app.emit("goal:evidence-updated", json!({"evidence": evidence}));
                            refreshed_run(&conn, run)
                        }
                        Err(error) => return runtime_error_response(error, &decision, &run),
                    }
                };
                run = {
                    let state = app.state::<DbState>();
                    let mut conn = match state.0.lock() {
                        Ok(conn) => conn,
                        Err(error) => {
                            return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                        }
                    };
                    match repository::transition_run(
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
                            actor: "bob".into(),
                            idempotency_key: format!("goal-runtime-done-{execution_key}"),
                        },
                    ) {
                        Ok(run) => run,
                        Err(error) => return runtime_error_response(error, &decision, &run),
                    }
                };
                {
                    let state = app.state::<DbState>();
                    if let Ok(conn) = state.0.lock() {
                        if let Err(error) =
                            crate::result_receipt::persist_verified_experience_candidate(
                                &conn,
                                &run.goal_id,
                                &run.run_id,
                                &contract.outcome,
                                &tool_summary,
                            )
                        {
                            log::warn!(
                                "[GoalRuntime] verified experience candidate unavailable: {error}"
                            );
                        }
                    };
                }
                verified = true;
                break;
            }
        }
        if attempt_index + 1 < max_attempts {
            if run.model_calls_used >= contract.budget.max_model_calls
                || run.tool_calls_used >= contract.budget.max_tool_calls
                || run.runtime_seconds_used >= contract.budget.max_runtime_seconds
                || run.repairs_used >= contract.budget.max_repairs
            {
                break;
            }
            let Some(prompt) = crate::execution_error::recovery_prompt(recovery_action) else {
                break;
            };
            execution_messages.push(json!({"role":"user","content":prompt}));
        }
    }

    if !verified {
        let required_rule = contract.evidence_rules.iter().find(|rule| rule.required);
        if required_rule
            .is_some_and(|rule| rule.kind == super::models::EvidenceRuleKind::UserAcceptance)
        {
            let approval = {
                let state = app.state::<DbState>();
                let mut conn = match state.0.lock() {
                    Ok(conn) => conn,
                    Err(error) => {
                        return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                    }
                };
                match repository::create_approval(
                    &mut conn,
                    CreateApprovalInput {
                        run_id: run.run_id.clone(),
                        summary: "goal.summary_accept_result".into(),
                        risk: GoalRisk::R0,
                        choices: approval_choices_for_acceptance(),
                        trusted_device_required: false,
                        expires_at: None,
                        expected_revision: run.revision,
                        actor: "bob".into(),
                        idempotency_key: format!("goal-runtime-acceptance-{execution_key}"),
                    },
                ) {
                    Ok(approval) => {
                        run = refreshed_run(&conn, run);
                        Some(json!(approval))
                    }
                    Err(error) => return runtime_error_response(error, &decision, &run),
                }
            };
            emit_state(&app, &run);
            if let Some(approval) = approval.as_ref() {
                let _ = app.emit("goal:approval-required", approval.clone());
            }
            let content = final_result
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut response = response_value(
                format!("{}\n\n{}", content, status_message(run.status)),
                &decision,
                &run,
                approval,
            );
            attach_execution_metadata(&mut response, &final_result);
            return response;
        }
        run = {
            let state = app.state::<DbState>();
            let mut conn = match state.0.lock() {
                Ok(conn) => conn,
                Err(error) => {
                    return json!({"content": status_message(GoalRunStatus::Failed), "error": error.to_string(), "route": decision})
                }
            };
            repository::transition_run(
                &mut conn,
                TransitionRunInput {
                    run_id: run.run_id.clone(),
                    status: GoalRunStatus::Blocked,
                    phase: GoalPhase::Verify,
                    verification_state: VerificationState::Unverified,
                    expected_revision: run.revision,
                    next_action: Some("goal.next_review_failure".into()),
                    error_code: Some("GOAL-EVIDENCE-UNVERIFIED".into()),
                    error_detail: Some("在修复预算内没有获得可验证工具回执".into()),
                    actor: "bob".into(),
                    idempotency_key: format!("goal-runtime-blocked-{execution_key}"),
                },
            )
            .unwrap_or(run)
        };
    }
    emit_state(&app, &run);
    let content = final_result
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut response = response_value(
        format!("{}\n\n{}", content, status_message(run.status)),
        &decision,
        &run,
        None,
    );
    attach_execution_metadata(&mut response, &final_result);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant_context::{ContextConflict, ContextObjectRef};
    use crate::work_core::models::WorkProject;

    fn project(id: &str, title: &str) -> WorkProject {
        WorkProject {
            schema_version: 1,
            id: id.into(),
            title: title.into(),
            mission: String::new(),
            status: "active".into(),
            current_phase: None,
            summary: None,
            source_ref: None,
            metadata: Value::Null,
            revision: 1,
            created_at: 1,
            updated_at: 1,
            deleted_at: None,
        }
    }

    fn empty_context() -> AssistantContext {
        AssistantContext {
            active_object: None,
            relevant_facts: Vec::new(),
            conflicts: Vec::new(),
            confidence: 0.0,
            reason_codes: Vec::new(),
            generated_at: 1,
        }
    }

    #[test]
    fn goal_project_uses_the_resolved_project() {
        let mut context = empty_context();
        context.active_object = Some(ContextObjectRef {
            object_id: "project_bob".into(),
            object_kind: "project".into(),
            project_id: "project_bob".into(),
            title: "Bob".into(),
            source_ref: "work_project:project_bob@1".into(),
        });
        assert_eq!(
            goal_project_resolution(&context, &ContextSourceSnapshot::default()),
            GoalProjectResolution::Selected("project_bob".into())
        );
    }

    #[test]
    fn ambiguous_projects_require_clarification_before_goal_creation() {
        let mut context = empty_context();
        context.conflicts.push(ContextConflict {
            candidate_ids: vec!["project_north".into(), "project_south".into()],
            reason_code: "context.ambiguous_candidates".into(),
        });
        let snapshot = ContextSourceSnapshot {
            projects: vec![
                project("project_north", "North"),
                project("project_south", "South"),
            ],
            aggregates: Vec::new(),
            focus_project_ids: Vec::new(),
        };
        assert_eq!(
            goal_project_resolution(&context, &snapshot),
            GoalProjectResolution::Ambiguous(vec![
                ProjectCandidate {
                    project_id: "project_north".into(),
                    title: "North".into(),
                },
                ProjectCandidate {
                    project_id: "project_south".into(),
                    title: "South".into(),
                },
            ])
        );
    }

    #[test]
    fn no_project_context_falls_back_to_personal_workspace() {
        assert_eq!(
            goal_project_resolution(&empty_context(), &ContextSourceSnapshot::default()),
            GoalProjectResolution::Personal
        );
    }
}
