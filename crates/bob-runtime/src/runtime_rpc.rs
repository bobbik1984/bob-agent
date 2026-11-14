//! Platform-neutral RPC command dispatcher for `bob-runtime`.
//! Shared across `bob-node` (headless server) and `bob-app` (Tauri Desktop/Android).

use crate::capability_borrowing;
use crate::claude_rpc;
use crate::context_capsule;
use crate::harness_auth;
use crate::subagent_pool::{self, SubagentPoolCoordinator};
use crate::FileVaultSecretStore;
use bob_kernel::app::LocalApp;
use serde_json::{json, Value};
use std::path::Path;

/// Check if a command is a platform-neutral runtime RPC command.
pub fn is_runtime_rpc_command(command: &str) -> bool {
    matches!(
        command,
        "app_context_list"
            | "app_context_read"
            | "app_context_refresh"
            | "app_context_tasks"
            | "app_harness_discover"
            | "app_harness_check_auth"
            | "app_harness_recover_auth"
            | "app_harness_begin_login"
            | "app_harness_verify_capabilities"
            | "app_claude_capabilities"
            | "app_claude_sessions"
            | "app_claude_history"
            | "app_claude_prompt"
            | "app_claude_cancel"
            | "app_claude_reconcile"
            | "app_claude_review"
            | "app_claude_artifacts"
            | "app_claude_preview"
            | "app_capability_registry_get"
            | "app_vault_rotate_seed"
            | "app_egress_audit_list"
            | "app_render_grants_list"
            | "app_subagent_dispatch"
            | "app_subagent_execute"
            | "app_subagent_cancel"
            | "app_subagent_reconcile"
            | "app_subagent_status"
            | "app_subagent_list"
            | "app_features_list"
            | "app_feature_keep"
            | "app_feature_rollback"
            | "app_feature_toggle"
            | "app_feature_call"
            | "app_feature_storage_save"
            | "app_feature_report_error"
            | "app_model_presets_list"
            | "app_routines_list"
            | "app_routine_save"
            | "app_routine_trial"
            | "app_routine_toggle"
            | "app_routine_delete"
            | "app_routine_drafts_list"
            | "app_jarvis_approvals_list"
            | "app_jarvis_approve"
            | "app_jarvis_reject"
            | "app_jarvis_budget_get"
            | "app_jarvis_budget_set"
            | "app_jarvis_worker_tasks"
            | "app_jarvis_worker_cancel"
            | "app_proactivity_get"
            | "app_proactivity_set"
            | "app_morning_brief"
            | "app_dream_run"
            | "app_kb_search"
            | "app_browser_sessions_list"
            | "app_browser_session_open"
            | "app_browser_session_clear"
            | "app_browser_takeover_complete"
            | "app_push_vapid_public_key"
            | "app_push_subscribe"
            | "app_host_workspaces_list"
            | "app_host_workspace_grant"
            | "app_host_workspace_revoke"
            | "app_host_queue_list"
            | "app_host_queue_enqueue"
            | "app_host_queue_reconcile"
    )
}

/// Dispatches a platform-neutral runtime RPC command using the provided data directory.
pub fn dispatch_runtime_rpc(
    data_dir: &Path,
    command: &str,
    args: &Value,
) -> Result<Value, &'static str> {
    match command {
        "app_context_list"
        | "app_context_read"
        | "app_context_refresh"
        | "app_context_tasks" => context_capsule::rpc(data_dir, command, args),

        "app_harness_discover"
        | "app_harness_check_auth"
        | "app_harness_recover_auth"
        | "app_harness_begin_login"
        | "app_harness_verify_capabilities" => {
            let connection_id = args
                .get("connectionId")
                .or_else(|| args.get("connection_id"))
                .and_then(Value::as_str)
                .ok_or("HARNESS_CONNECTION_REQUIRED")?;
            let tool = args
                .get("tool")
                .and_then(Value::as_str)
                .ok_or("HARNESS_TOOL_REQUIRED")?;
            let report = match command {
                "app_harness_discover" => harness_auth::discover_tool(data_dir, connection_id, tool)?,
                "app_harness_check_auth" => harness_auth::check_auth(data_dir, connection_id, tool)?,
                "app_harness_recover_auth" => harness_auth::recover_auth(data_dir, connection_id, tool)?,
                "app_harness_begin_login" => harness_auth::begin_login(data_dir, connection_id, tool)?,
                _ => harness_auth::verify_capabilities(data_dir, connection_id, tool)?,
            };
            serde_json::to_value(report).map_err(|_| "HARNESS_SERIALIZE_FAILED")
        }

        "app_claude_capabilities"
        | "app_claude_sessions"
        | "app_claude_history"
        | "app_claude_prompt"
        | "app_claude_cancel"
        | "app_claude_reconcile"
        | "app_claude_review"
        | "app_claude_artifacts"
        | "app_claude_preview" => claude_rpc::rpc(data_dir, command, args),

        "app_capability_registry_get" => {
            serde_json::to_value(capability_borrowing::load_capabilities(data_dir)?)
                .map_err(|_| "CAPABILITY_READ_FAILED")
        }
        "app_vault_rotate_seed" => {
            let mut store = FileVaultSecretStore::open(data_dir)?;
            let rotated = store.rotate_master_seed()?;
            Ok(json!({ "rotated_slots": rotated }))
        }
        "app_egress_audit_list" => {
            serde_json::to_value(capability_borrowing::list_egress_audit_records(data_dir)?)
                .map_err(|_| "EGRESS_AUDIT_READ_FAILED")
        }
        "app_render_grants_list" => {
            serde_json::to_value(capability_borrowing::load_render_grants(data_dir)?)
                .map_err(|_| "RENDER_GRANTS_READ_FAILED")
        }

        "app_subagent_dispatch" => {
            let grant: subagent_pool::SubagentGrant =
                serde_json::from_value(args.get("grant").cloned().unwrap_or(Value::Null))
                    .map_err(|_| "SUBAGENT_GRANT_INVALID")?;
            let request: subagent_pool::SubagentInvocationRequest =
                serde_json::from_value(args.get("request").cloned().unwrap_or(Value::Null))
                    .map_err(|_| "SUBAGENT_REQUEST_INVALID")?;
            let record = SubagentPoolCoordinator::dispatch(data_dir, &grant, &request)?;
            serde_json::to_value(record).map_err(|_| "SUBAGENT_SERIALIZE_FAILED")
        }
        "app_subagent_execute" => {
            let grant: subagent_pool::SubagentGrant =
                serde_json::from_value(args.get("grant").cloned().unwrap_or(Value::Null))
                    .map_err(|_| "SUBAGENT_GRANT_INVALID")?;
            let run_id = args
                .get("runId")
                .or_else(|| args.get("run_id"))
                .and_then(Value::as_str)
                .ok_or("SUBAGENT_RUN_ID_REQUIRED")?;
            let actions: Vec<subagent_pool::SubagentStepAction> =
                serde_json::from_value(args.get("actions").cloned().unwrap_or(Value::Null))
                    .map_err(|_| "SUBAGENT_ACTIONS_INVALID")?;
            let record =
                SubagentPoolCoordinator::execute_bounded_turn(data_dir, &grant, run_id, &actions)?;
            serde_json::to_value(record).map_err(|_| "SUBAGENT_SERIALIZE_FAILED")
        }
        "app_subagent_cancel" => {
            let parent_task_id = args
                .get("parentTaskId")
                .or_else(|| args.get("parent_task_id"))
                .and_then(Value::as_str)
                .ok_or("SUBAGENT_PARENT_TASK_REQUIRED")?;
            let run_id = args
                .get("runId")
                .or_else(|| args.get("run_id"))
                .and_then(Value::as_str);
            let reason = args
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("EXPLICIT_CANCEL");
            let record = SubagentPoolCoordinator::cancel(data_dir, parent_task_id, run_id, reason)?;
            serde_json::to_value(record).map_err(|_| "SUBAGENT_SERIALIZE_FAILED")
        }
        "app_subagent_reconcile" => {
            let reconciled = SubagentPoolCoordinator::reconcile_on_restart(data_dir)?;
            serde_json::to_value(reconciled).map_err(|_| "SUBAGENT_SERIALIZE_FAILED")
        }
        "app_subagent_status" => {
            let run_id = args
                .get("runId")
                .or_else(|| args.get("run_id"))
                .and_then(Value::as_str)
                .ok_or("SUBAGENT_RUN_ID_REQUIRED")?;
            let record = SubagentPoolCoordinator::get_run(data_dir, run_id)?;
            serde_json::to_value(record).map_err(|_| "SUBAGENT_SERIALIZE_FAILED")
        }
        "app_subagent_list" => {
            let parent_task_id = args
                .get("parentTaskId")
                .or_else(|| args.get("parent_task_id"))
                .and_then(Value::as_str)
                .ok_or("SUBAGENT_PARENT_TASK_REQUIRED")?;
            let list = SubagentPoolCoordinator::list_runs_for_task(data_dir, parent_task_id)?;
            serde_json::to_value(list).map_err(|_| "SUBAGENT_SERIALIZE_FAILED")
        }

        // -------------------------------------------------------------------
        // Feature Sandbox RPCs
        // -------------------------------------------------------------------
        "app_features_list" => {
            let mut store = crate::features::FeatureStore::open(data_dir)?;
            let list = store.list(crate::now_millis());
            serde_json::to_value(list).map_err(|_| "FEATURE_SERIALIZE_FAILED")
        }
        "app_feature_keep" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("FEATURE_NAME_REQUIRED")?;
            let mut store = crate::features::FeatureStore::open(data_dir)?;
            let pkg = store
                .keep_feature(name, crate::now_millis())
                .map_err(|_| "FEATURE_KEEP_FAILED")?;
            serde_json::to_value(pkg).map_err(|_| "FEATURE_SERIALIZE_FAILED")
        }
        "app_feature_rollback" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("FEATURE_NAME_REQUIRED")?;
            let target_version = args
                .get("targetVersion")
                .or_else(|| args.get("target_version"))
                .and_then(Value::as_u64)
                .map(|v| v as u32);
            let mut store = crate::features::FeatureStore::open(data_dir)?;
            let pkg = store
                .rollback_feature(name, target_version, crate::now_millis())
                .map_err(|_| "FEATURE_ROLLBACK_FAILED")?;
            serde_json::to_value(pkg).map_err(|_| "FEATURE_SERIALIZE_FAILED")
        }
        "app_feature_toggle" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("FEATURE_NAME_REQUIRED")?;
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("FEATURE_ENABLED_REQUIRED")?;
            let mut store = crate::features::FeatureStore::open(data_dir)?;
            let pkg = store
                .toggle_feature(name, enabled, crate::now_millis())
                .map_err(|_| "FEATURE_TOGGLE_FAILED")?;
            serde_json::to_value(pkg).map_err(|_| "FEATURE_SERIALIZE_FAILED")
        }
        "app_feature_call" => crate::features::execute_feature_call(data_dir, args),
        "app_feature_storage_save" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("FEATURE_NAME_REQUIRED")?;
            let patch = args
                .get("storage")
                .or_else(|| args.get("patch"))
                .cloned()
                .unwrap_or(json!({}));
            let mut store = crate::features::FeatureStore::open(data_dir)?;
            let storage = store
                .save_storage(name, patch, crate::now_millis())
                .map_err(|_| "FEATURE_STORAGE_FAILED")?;
            Ok(storage)
        }
        "app_feature_report_error" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("FEATURE_NAME_REQUIRED")?;
            let error = args
                .get("error")
                .or_else(|| args.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("WIDGET_RUNTIME_ERROR");
            let now_ms = args
                .get("now")
                .and_then(Value::as_i64)
                .unwrap_or_else(crate::now_millis);
            crate::features::FeaturePackageManager::open_at(data_dir, now_ms)?
                .report_runtime_error(name, error, now_ms)
        }
        "app_model_presets_list" => Ok(Value::Array(crate::model_presets::list_dual_presets_json())),

        // -------------------------------------------------------------------
        // Routine & Draft RPCs
        // -------------------------------------------------------------------
        "app_routines_list" => {
            let store = crate::routines::RoutineStore::open(data_dir)?;
            let mut draft_store = crate::routines::UnattendedDraftStore::open(data_dir)?;
            let routines = store.list();
            let drafts = draft_store.list_pending(crate::now_millis());
            Ok(json!({
                "routines": routines,
                "drafts": drafts,
            }))
        }
        "app_routine_save" => {
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .ok_or("ROUTINE_TITLE_REQUIRED")?;
            let prompt_instruction = args
                .get("prompt_instruction")
                .or_else(|| args.get("promptInstruction"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let recurrence: crate::routines::RecurrenceRule = if let Some(rec) = args.get("recurrence") {
                serde_json::from_value(rec.clone()).map_err(|_| "INVALID_RECURRENCE_RULE")?
            } else {
                crate::routines::RecurrenceRule::Daily { hour: 8, minute: 0 }
            };
            let steps: Vec<crate::routines::RoutineStep> = args
                .get("routine_steps")
                .or_else(|| args.get("routineSteps"))
                .or_else(|| args.get("steps"))
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .enumerate()
                        .filter_map(|(idx, v)| {
                            let tool = v
                                .get("tool_name")
                                .or_else(|| v.get("toolName"))
                                .and_then(Value::as_str)?;
                            let desc = v.get("description").and_then(Value::as_str).unwrap_or("");
                            let template = v
                                .get("args_template")
                                .or_else(|| v.get("argsTemplate"))
                                .or_else(|| v.get("args"))
                                .cloned()
                                .unwrap_or(json!({}));
                            Some(crate::routines::RoutineStep {
                                step_index: idx,
                                tool_name: tool.to_string(),
                                description: desc.to_string(),
                                args_template: template,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let offset = args
                .get("utc_offset_minutes")
                .or_else(|| args.get("utcOffsetMinutes"))
                .and_then(Value::as_i64)
                .map(|v| v as i32)
                .unwrap_or(crate::routines::DEFAULT_UTC_OFFSET_MINUTES);
            let id = args.get("id").and_then(Value::as_str);

            let routine = crate::routines::capture_walkthrough_to_routine(
                id,
                title,
                prompt_instruction,
                recurrence,
                steps,
                Some(offset),
            );
            let mut store = crate::routines::RoutineStore::open(data_dir)?;
            let saved = store.save_routine(routine).map_err(|_| "ROUTINE_SAVE_FAILED")?;

            if let Ok(mut app) = LocalApp::open(&data_dir.join("kernel.sqlite")) {
                let _ = crate::routines::mirror_routine_to_schedule(
                    &mut app,
                    &saved,
                    crate::now_millis(),
                );
            }
            serde_json::to_value(saved).map_err(|_| "ROUTINE_SERIALIZE_FAILED")
        }
        "app_routine_trial" => {
            let id = args
                .get("id")
                .or_else(|| args.get("routine_id"))
                .or_else(|| args.get("routineId"))
                .and_then(Value::as_str)
                .ok_or("ROUTINE_ID_REQUIRED")?;
            let mut store = crate::routines::RoutineStore::open(data_dir)?;
            let mut draft_store = crate::routines::UnattendedDraftStore::open(data_dir)?;
            let mut bus = crate::tool_bus::BobToolBus::with_approval_store(data_dir)?;
            let mut app = LocalApp::open(&data_dir.join("kernel.sqlite"))
                .map_err(|_| "KERNEL_STORE_UNAVAILABLE")?;
            let (routine, report, drafts) = store
                .run_trial(id, &mut bus, &mut draft_store, &mut app, crate::now_millis())
                .map_err(|_| "TRIAL_RUN_FAILED")?;
            Ok(json!({
                "routine": routine,
                "report": report,
                "drafts": drafts,
            }))
        }
        "app_routine_toggle" => {
            let id = args
                .get("id")
                .or_else(|| args.get("routine_id"))
                .or_else(|| args.get("routineId"))
                .and_then(Value::as_str)
                .ok_or("ROUTINE_ID_REQUIRED")?;
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("ROUTINE_ENABLED_REQUIRED")?;
            let mut store = crate::routines::RoutineStore::open(data_dir)?;
            let updated = store
                .enable_routine(id, enabled, crate::now_millis())
                .map_err(|_| "ROUTINE_ENABLE_FAILED")?;
            if let Ok(mut app) = LocalApp::open(&data_dir.join("kernel.sqlite")) {
                if updated.enabled {
                    let _ = crate::routines::mirror_routine_to_schedule(
                        &mut app,
                        &updated,
                        crate::now_millis(),
                    );
                } else {
                    let _ = crate::routines::unmirror_routine_from_schedule(
                        &mut app,
                        &updated,
                        crate::now_millis(),
                    );
                }
            }
            serde_json::to_value(updated).map_err(|_| "ROUTINE_SERIALIZE_FAILED")
        }
        "app_routine_delete" => {
            let id = args
                .get("id")
                .or_else(|| args.get("routine_id"))
                .or_else(|| args.get("routineId"))
                .and_then(Value::as_str)
                .ok_or("ROUTINE_ID_REQUIRED")?;
            let mut store = crate::routines::RoutineStore::open(data_dir)?;
            let deleted = store
                .delete_routine(id)
                .map_err(|_| "ROUTINE_DELETE_FAILED")?;
            if let Ok(mut app) = LocalApp::open(&data_dir.join("kernel.sqlite")) {
                let _ = crate::routines::unmirror_routine_from_schedule(
                    &mut app,
                    &deleted,
                    crate::now_millis(),
                );
            }
            serde_json::to_value(deleted).map_err(|_| "ROUTINE_SERIALIZE_FAILED")
        }
        "app_routine_drafts_list" => {
            let mut store = crate::routines::UnattendedDraftStore::open(data_dir)?;
            let list = store.list_pending(crate::now_millis());
            serde_json::to_value(list).map_err(|_| "ROUTINE_DRAFTS_LIST_FAILED")
        }

        // -------------------------------------------------------------------
        // Growth & Budget & Approvals RPCs
        // -------------------------------------------------------------------
        "app_jarvis_approvals_list" => {
            let mut store = crate::tool_bus::PendingApprovalStore::open(data_dir)?;
            let list = store.list_pending(crate::now_millis());
            serde_json::to_value(list).map_err(|_| "APPROVALS_SERIALIZE_FAILED")
        }
        "app_jarvis_approve" => {
            let approval_id = args
                .get("approvalId")
                .or_else(|| args.get("approval_id"))
                .or_else(|| args.get("id"))
                .and_then(Value::as_str)
                .ok_or("APPROVAL_ID_REQUIRED")?;
            let mut bus = crate::tool_bus::BobToolBus::with_approval_store(data_dir)?;
            let mut app = LocalApp::open(&data_dir.join("kernel.sqlite"))
                .map_err(|_| "KERNEL_STORE_UNAVAILABLE")?;
            let outcome = bus.approve_once(approval_id, &mut app)?;
            match outcome {
                crate::tool_bus::ToolBusOutcome::Executed { output, card } => {
                    Ok(json!({ "executed": true, "output": output, "card": card }))
                }
                _ => Ok(json!({ "executed": true })),
            }
        }
        "app_jarvis_reject" => {
            let approval_id = args
                .get("approvalId")
                .or_else(|| args.get("approval_id"))
                .or_else(|| args.get("id"))
                .and_then(Value::as_str)
                .ok_or("APPROVAL_ID_REQUIRED")?;
            let mut bus = crate::tool_bus::BobToolBus::with_approval_store(data_dir)?;
            let (tool_name, tool_args) = bus.reject(approval_id)?;
            Ok(json!({ "rejected": true, "tool_name": tool_name, "args": tool_args }))
        }
        "app_jarvis_budget_get" => {
            let mut store = crate::budget::MonthlyBudgetStore::open(data_dir)?;
            let status = store.status();
            Ok(json!({
                "monthly_cap_usd": status.cap_usd,
                "monthlyCapUsd": status.cap_usd,
                "spent_usd": status.spent_usd,
                "spentUsd": status.spent_usd,
                "remaining_usd": (status.cap_usd - status.spent_usd).max(0.0),
                "remainingUsd": (status.cap_usd - status.spent_usd).max(0.0),
                "cap_reached": status.exceeded,
                "capReached": status.exceeded,
                "warning_reached": status.near_80_pct,
                "warningReached": status.near_80_pct,
                "month_key": status.month,
                "monthKey": status.month,
                "warned": status.warned,
            }))
        }
        "app_jarvis_budget_set" => {
            let cap = args
                .get("monthlyCapUsd")
                .or_else(|| args.get("monthly_cap_usd"))
                .or_else(|| args.get("cap"))
                .and_then(Value::as_f64)
                .ok_or("CAP_REQUIRED")?;
            let mut store = crate::budget::MonthlyBudgetStore::open(data_dir)?;
            store.set_cap_usd(cap).map_err(|_| "BUDGET_SAVE_FAILED")?;
            let status = store.status();
            Ok(json!({
                "monthly_cap_usd": status.cap_usd,
                "monthlyCapUsd": status.cap_usd,
                "spent_usd": status.spent_usd,
                "spentUsd": status.spent_usd,
                "remaining_usd": (status.cap_usd - status.spent_usd).max(0.0),
                "remainingUsd": (status.cap_usd - status.spent_usd).max(0.0),
                "cap_reached": status.exceeded,
                "capReached": status.exceeded,
            }))
        }
        "app_jarvis_worker_tasks" => {
            let session_id = args
                .get("conversationId")
                .or_else(|| args.get("conversation_id"))
                .or_else(|| args.get("sessionId"))
                .or_else(|| args.get("session_id"))
                .and_then(Value::as_str);
            let after_seq = args
                .get("afterSeq")
                .or_else(|| args.get("after_seq"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let tasks =
                crate::worker_ledger::WorkerTaskStore::list_tasks(data_dir, session_id, after_seq);
            Ok(Value::Array(tasks))
        }
        "app_jarvis_worker_cancel" => {
            let task_id = args
                .get("taskId")
                .or_else(|| args.get("task_id"))
                .or_else(|| args.get("id"))
                .and_then(Value::as_str)
                .ok_or("WORKER_TASK_ID_REQUIRED")?;
            let cancelled = crate::worker_ledger::WorkerTaskStore::request_cancel(data_dir, task_id)?;
            serde_json::to_value(cancelled).map_err(|_| "WORKER_TASK_SERIALIZE_FAILED")
        }
        "app_proactivity_get" => {
            let store = crate::growth::ProactivityStore::open(data_dir)?;
            Ok(store.config.to_json_value())
        }
        "app_proactivity_set" => {
            let level_val = args.get("level").and_then(|v| {
                if let Some(f) = v.as_f64() {
                    Some(f)
                } else if let Some(s) = v.as_str() {
                    match s {
                        "silent" => Some(0.0),
                        "morning_brief" => Some(0.33),
                        "reminders" => Some(0.66),
                        "proactive_care" => Some(1.0),
                        _ => None,
                    }
                } else {
                    None
                }
            });
            let q_start = args
                .get("quietHoursStart")
                .or_else(|| args.get("quiet_hours_start"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let q_end = args
                .get("quietHoursEnd")
                .or_else(|| args.get("quiet_hours_end"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let mut store = crate::growth::ProactivityStore::open(data_dir)?;
            let updated = store
                .set_config(level_val, q_start, q_end, None)
                .map_err(|_| "PROACTIVITY_SAVE_FAILED")?;
            Ok(updated.to_json_value())
        }
        "app_morning_brief" => {
            let mut app = LocalApp::open(&data_dir.join("kernel.sqlite"))
                .map_err(|_| "KERNEL_STORE_UNAVAILABLE")?;
            let brief = crate::growth::generate_morning_brief(data_dir, &mut app, crate::now_millis())
                .map_err(|_| "BRIEF_FAILED")?;
            serde_json::to_value(brief).map_err(|_| "BRIEF_SERIALIZE_FAILED")
        }
        "app_dream_run" => {
            let mut app = LocalApp::open(&data_dir.join("kernel.sqlite"))
                .map_err(|_| "KERNEL_STORE_UNAVAILABLE")?;
            let mut bus = crate::tool_bus::BobToolBus::with_approval_store(data_dir)?;
            let summary = crate::growth::run_nightly_dream(data_dir, &mut app, &mut bus, crate::now_millis())
                .map_err(|_| "DREAM_CYCLE_FAILED")?;
            serde_json::to_value(summary).map_err(|_| "DREAM_SERIALIZE_FAILED")
        }
        "app_kb_search" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or("QUERY_REQUIRED")?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize;
            let app = LocalApp::open(&data_dir.join("kernel.sqlite"))
                .map_err(|_| "KERNEL_STORE_UNAVAILABLE")?;
            let hits = app.kb_search(query, limit).map_err(|_| "KB_SEARCH_FAILED")?;
            Ok(json!({ "hits": hits, "count": hits.len() }))
        }

        // -------------------------------------------------------------------
        // Browser Session Fallback RPCs
        // -------------------------------------------------------------------
        "app_browser_sessions_list" => {
            let p = data_dir.join("browser_sessions.json");
            if p.is_file() {
                let data = std::fs::read_to_string(&p).unwrap_or_else(|_| "[]".to_string());
                let parsed: Value = serde_json::from_str(&data).unwrap_or_else(|_| json!([]));
                if parsed.is_array() {
                    Ok(parsed)
                } else {
                    Ok(json!([]))
                }
            } else {
                Ok(json!([]))
            }
        }
        "app_browser_session_open" => {
            let target = args
                .get("target")
                .or_else(|| args.get("url"))
                .and_then(Value::as_str)
                .unwrap_or("https://example.com");
            let takeover_id = format!("takeover-{}", crate::now_millis());
            Ok(json!({
                "id": takeover_id,
                "takeoverId": takeover_id,
                "takeover_id": takeover_id,
                "url": target,
                "width": 390,
                "height": 844,
                "desktopMode": false,
                "desktop_mode": false
            }))
        }
        "app_browser_session_clear" => {
            let domain = args
                .get("domain")
                .and_then(Value::as_str)
                .unwrap_or("*");
            let p = data_dir.join("browser_sessions.json");
            if domain == "*" {
                let _ = std::fs::write(&p, "[]");
            }
            Ok(json!({ "cleared": true, "domain": domain }))
        }
        "app_browser_takeover_complete" => {
            let id = args
                .get("takeoverId")
                .or_else(|| args.get("takeover_id"))
                .or_else(|| args.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(json!({ "completed": true, "takeoverId": id, "takeover_id": id }))
        }

        // -------------------------------------------------------------------
        // Push Notification Stubs
        // -------------------------------------------------------------------
        "app_push_vapid_public_key" => {
            Ok(json!({ "publicKey": "", "public_key": "" }))
        }
        "app_push_subscribe" => {
            Ok(json!({ "status": "subscribed" }))
        }

        // -------------------------------------------------------------------
        // Host Workspace & Offline Queue RPCs
        // -------------------------------------------------------------------
        "app_host_workspaces_list" => {
            let store = crate::host_tools::WorkspaceGrantStore::open(data_dir)?;
            Ok(serde_json::to_value(store.list_workspaces()).unwrap_or_else(|_| json!([])))
        }
        "app_host_workspace_grant" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("PATH_REQUIRED")?;
            let label = args.get("label").and_then(Value::as_str).unwrap_or("");
            let read_only = args
                .get("readOnly")
                .or_else(|| args.get("read_only"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut store = crate::host_tools::WorkspaceGrantStore::open(data_dir)?;
            let grant = store.authorize_workspace(path, label, read_only, crate::now_millis())?;
            serde_json::to_value(grant).map_err(|_| "WORKSPACE_SERIALIZE_FAILED")
        }
        "app_host_workspace_revoke" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or("WORKSPACE_ID_REQUIRED")?;
            let mut store = crate::host_tools::WorkspaceGrantStore::open(data_dir)?;
            let removed = store.revoke_workspace(id)?;
            Ok(json!({ "revoked": true, "workspace": removed }))
        }
        "app_host_queue_list" => {
            let node_id = args
                .get("nodeId")
                .or_else(|| args.get("node_id"))
                .and_then(Value::as_str);
            let store = crate::host_tools::HostOfflineQueueStore::open(data_dir)?;
            Ok(serde_json::to_value(store.list_queue(node_id)).unwrap_or_else(|_| json!([])))
        }
        "app_host_queue_enqueue" => {
            let node_id = args
                .get("nodeId")
                .or_else(|| args.get("node_id"))
                .and_then(Value::as_str)
                .ok_or("NODE_ID_REQUIRED")?;
            let tool_name = args
                .get("toolName")
                .or_else(|| args.get("tool_name"))
                .and_then(Value::as_str)
                .ok_or("TOOL_NAME_REQUIRED")?;
            let tool_args = args.get("args").cloned().unwrap_or(json!({}));
            let mut store = crate::host_tools::HostOfflineQueueStore::open(data_dir)?;
            let item = store.enqueue_for_pc(node_id, tool_name, tool_args, crate::now_millis())?;
            serde_json::to_value(item).map_err(|_| "QUEUE_SERIALIZE_FAILED")
        }
        "app_host_queue_reconcile" => {
            let node_id = args
                .get("nodeId")
                .or_else(|| args.get("node_id"))
                .and_then(Value::as_str)
                .ok_or("NODE_ID_REQUIRED")?;
            let mut bus = crate::tool_bus::BobToolBus::with_approval_store(data_dir)?;
            let mut app = LocalApp::open(&data_dir.join("kernel.sqlite"))
                .map_err(|_| "KERNEL_STORE_UNAVAILABLE")?;
            let list = crate::host_tools::HostOfflineQueueStore::reconcile_when_pc_online(
                data_dir,
                node_id,
                &mut bus,
                &mut app,
                crate::now_millis(),
            )
            .map_err(|_| "RECONCILE_FAILED")?;
            serde_json::to_value(list).map_err(|_| "QUEUE_SERIALIZE_FAILED")
        }

        _ => Err("COMMAND_UNKNOWN"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker_ledger::WorkerTaskStore;

    #[test]
    fn test_wp_e1_2_and_e1_4_runtime_rpc_commands_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        for cmd in [
            "app_model_presets_list",
            "app_jarvis_worker_tasks",
            "app_jarvis_worker_cancel",
            "app_feature_report_error",
        ] {
            assert!(is_runtime_rpc_command(cmd), "{cmd} must be registered");
        }

        // 1. app_model_presets_list
        let presets = dispatch_runtime_rpc(dir, "app_model_presets_list", &json!({})).unwrap();
        let arr = presets.as_array().unwrap();
        assert_eq!(arr.len(), 10);
        assert!(arr.iter().any(|p| p["provider_id"] == "deepseek" && p["providerId"] == "deepseek"));

        // 2. app_jarvis_worker_tasks + app_jarvis_worker_cancel
        let created = WorkerTaskStore::create_task(
            dir,
            "conv-rpc-1",
            "req-rpc-1",
            "Analyze report",
            "Summarize Q4 numbers",
            "",
            "deepseek-v4.1-flash-1017",
            "deepseek-v4-pro",
        );

        let listed = dispatch_runtime_rpc(
            dir,
            "app_jarvis_worker_tasks",
            &json!({ "conversationId": "conv-rpc-1", "afterSeq": 0 }),
        )
        .unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        let cancelled = dispatch_runtime_rpc(
            dir,
            "app_jarvis_worker_cancel",
            &json!({ "taskId": created.task_id }),
        )
        .unwrap();
        assert_eq!(cancelled["status"], "cancelled");
        assert_eq!(cancelled["cancel_requested"], true);
    }
}
