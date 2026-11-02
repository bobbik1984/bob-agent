//! Platform-neutral RPC command dispatcher for `bob-runtime`.
//! Shared across `bob-node` (headless server) and `bob-app` (Tauri Desktop/Android).

use crate::capability_borrowing;
use crate::claude_rpc;
use crate::context_capsule;
use crate::harness_auth;
use crate::subagent_pool::{self, SubagentPoolCoordinator};
use crate::FileVaultSecretStore;
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

        _ => Err("COMMAND_UNKNOWN"),
    }
}
