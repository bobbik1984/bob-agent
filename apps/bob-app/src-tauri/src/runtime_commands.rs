//! Native Tauri IPC command handlers for platform-neutral runtime operations.
//! Bridging `app_context_*`, `app_claude_*`, `app_capability_*`, and `app_subagent_*`
//! directly to `bob_runtime::runtime_rpc::dispatch_runtime_rpc`.

use bob_runtime::runtime_rpc::dispatch_runtime_rpc;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};

fn execute_rpc<R: Runtime>(
    host: &AppHandle<R>,
    command: &'static str,
    args: Value,
) -> Result<Value, &'static str> {
    let dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    dispatch_runtime_rpc(&dir, command, &args)
}

// ---------------------------------------------------------------------------
// 1. Context Capsule & Task Cards (BP4-04)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_context_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || execute_rpc(&host, "app_context_list", json!({})))
        .await
        .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_context_read<R: Runtime>(
    host: AppHandle<R>,
    grant_id: String,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_context_read", json!({ "grantId": grant_id }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_context_refresh<R: Runtime>(
    host: AppHandle<R>,
    grant_id: String,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_context_refresh", json!({ "grantId": grant_id }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_context_tasks<R: Runtime>(
    host: AppHandle<R>,
    grant_id: String,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_context_tasks", json!({ "grantId": grant_id }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 2. Claude Code Native Harness & Deliverables V1->V2 (BP4-06)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_claude_capabilities<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_capabilities",
            json!({ "connectionId": connection_id }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_sessions<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_sessions",
            json!({ "connectionId": connection_id }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_history<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
    session_id: Option<String>,
    grant_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_history",
            json!({
                "connectionId": connection_id,
                "sessionId": session_id,
                "grantId": grant_id
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_prompt<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
    session_id: Option<String>,
    grant_id: Option<String>,
    text: Option<String>,
    request_id: Option<String>,
    parent_version_id: Option<String>,
    detached_hold_ms: Option<u64>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_prompt",
            json!({
                "connectionId": connection_id,
                "sessionId": session_id,
                "grantId": grant_id,
                "text": text,
                "requestId": request_id,
                "parentVersionId": parent_version_id,
                "detachedHoldMs": detached_hold_ms
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_cancel<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
    session_id: Option<String>,
    grant_id: Option<String>,
    request_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_cancel",
            json!({
                "connectionId": connection_id,
                "sessionId": session_id,
                "grantId": grant_id,
                "requestId": request_id
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_reconcile<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
    session_id: Option<String>,
    grant_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_reconcile",
            json!({
                "connectionId": connection_id,
                "sessionId": session_id,
                "grantId": grant_id
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_review<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
    session_id: Option<String>,
    grant_id: Option<String>,
    version_id: Option<String>,
    verdict: Option<String>,
    note: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_review",
            json!({
                "connectionId": connection_id,
                "sessionId": session_id,
                "grantId": grant_id,
                "versionId": version_id,
                "verdict": verdict,
                "note": note
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_artifacts<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
    session_id: Option<String>,
    grant_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_artifacts",
            json!({
                "connectionId": connection_id,
                "sessionId": session_id,
                "grantId": grant_id
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_claude_preview<R: Runtime>(
    host: AppHandle<R>,
    connection_id: Option<String>,
    session_id: Option<String>,
    grant_id: Option<String>,
    path: Option<String>,
    version_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_claude_preview",
            json!({
                "connectionId": connection_id,
                "sessionId": session_id,
                "grantId": grant_id,
                "path": path,
                "versionId": version_id
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 3. Node Capability Borrowing & Secret Vault (BP4-07)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_capability_registry_get<R: Runtime>(
    host: AppHandle<R>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_capability_registry_get", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_vault_rotate_seed<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_vault_rotate_seed", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_egress_audit_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_egress_audit_list", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_render_grants_list<R: Runtime>(
    host: AppHandle<R>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_render_grants_list", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 4. Bounded Native API Subagents (BP4-05)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_subagent_dispatch<R: Runtime>(
    host: AppHandle<R>,
    grant: Value,
    request: Value,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_subagent_dispatch",
            json!({ "grant": grant, "request": request }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_subagent_execute<R: Runtime>(
    host: AppHandle<R>,
    grant: Value,
    run_id: String,
    actions: Value,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_subagent_execute",
            json!({ "grant": grant, "runId": run_id, "actions": actions }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_subagent_cancel<R: Runtime>(
    host: AppHandle<R>,
    parent_task_id: String,
    run_id: Option<String>,
    reason: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_subagent_cancel",
            json!({
                "parentTaskId": parent_task_id,
                "runId": run_id,
                "reason": reason.unwrap_or_else(|| "EXPLICIT_CANCEL".to_string())
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_subagent_reconcile<R: Runtime>(
    host: AppHandle<R>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || execute_rpc(&host, "app_subagent_reconcile", json!({})))
        .await
        .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_subagent_status<R: Runtime>(
    host: AppHandle<R>,
    run_id: String,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_subagent_status", json!({ "runId": run_id }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_subagent_list<R: Runtime>(
    host: AppHandle<R>,
    parent_task_id: String,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_subagent_list",
            json!({ "parentTaskId": parent_task_id }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 5. Feature Packages & Sandbox RPCs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_features_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || execute_rpc(&host, "app_features_list", json!({})))
        .await
        .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_feature_keep<R: Runtime>(host: AppHandle<R>, name: String) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_feature_keep", json!({ "name": name }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_feature_rollback<R: Runtime>(
    host: AppHandle<R>,
    name: String,
    target_version: Option<u32>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_feature_rollback",
            json!({ "name": name, "targetVersion": target_version }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_feature_toggle<R: Runtime>(
    host: AppHandle<R>,
    name: String,
    enabled: bool,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_feature_toggle",
            json!({ "name": name, "enabled": enabled }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_feature_call<R: Runtime>(
    host: AppHandle<R>,
    name: Option<String>,
    route: Option<String>,
    payload: Option<Value>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_feature_call",
            json!({
                "name": name,
                "route": route,
                "payload": payload.unwrap_or(Value::Null)
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_feature_storage_save<R: Runtime>(
    host: AppHandle<R>,
    name: String,
    storage: Option<Value>,
    patch: Option<Value>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_feature_storage_save",
            json!({
                "name": name,
                "storage": storage.or(patch).unwrap_or(Value::Null)
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 6. Routine & Draft RPCs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_routines_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || execute_rpc(&host, "app_routines_list", json!({})))
        .await
        .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_routine_save<R: Runtime>(
    host: AppHandle<R>,
    id: Option<String>,
    title: String,
    prompt_instruction: Option<String>,
    recurrence: Option<Value>,
    routine_steps: Option<Value>,
    utc_offset_minutes: Option<i32>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_routine_save",
            json!({
                "id": id,
                "title": title,
                "promptInstruction": prompt_instruction,
                "recurrence": recurrence,
                "routineSteps": routine_steps,
                "utcOffsetMinutes": utc_offset_minutes
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_routine_trial<R: Runtime>(
    host: AppHandle<R>,
    id: Option<String>,
    routine_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_routine_trial",
            json!({ "id": id.or(routine_id) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_routine_toggle<R: Runtime>(
    host: AppHandle<R>,
    id: Option<String>,
    routine_id: Option<String>,
    enabled: bool,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_routine_toggle",
            json!({ "id": id.or(routine_id), "enabled": enabled }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_routine_delete<R: Runtime>(
    host: AppHandle<R>,
    id: Option<String>,
    routine_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_routine_delete",
            json!({ "id": id.or(routine_id) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_routine_drafts_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_routine_drafts_list", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 7. Growth & Budget & Approvals RPCs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_jarvis_chat<R: Runtime>(
    _host: AppHandle<R>,
    _text: Option<String>,
    _conversation_id: Option<String>,
    _streaming: Option<bool>,
) -> Result<Value, &'static str> {
    Ok(json!({ "status": "direct_mode", "text": "Direct mode response" }))
}

#[tauri::command]
pub async fn app_jarvis_approvals_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_jarvis_approvals_list", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_jarvis_approve<R: Runtime>(
    host: AppHandle<R>,
    approval_id: Option<String>,
    id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_jarvis_approve",
            json!({ "approvalId": approval_id.or(id) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_jarvis_reject<R: Runtime>(
    host: AppHandle<R>,
    approval_id: Option<String>,
    id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_jarvis_reject",
            json!({ "approvalId": approval_id.or(id) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_jarvis_budget_get<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_jarvis_budget_get", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_jarvis_budget_set<R: Runtime>(
    host: AppHandle<R>,
    monthly_cap_usd: Option<f64>,
    cap: Option<f64>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_jarvis_budget_set",
            json!({ "monthlyCapUsd": monthly_cap_usd.or(cap) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_proactivity_get<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_proactivity_get", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_proactivity_set<R: Runtime>(
    host: AppHandle<R>,
    level: Option<Value>,
    quiet_hours_start: Option<String>,
    quiet_hours_end: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_proactivity_set",
            json!({
                "level": level,
                "quietHoursStart": quiet_hours_start,
                "quietHoursEnd": quiet_hours_end
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_morning_brief<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_morning_brief", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_dream_run<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || execute_rpc(&host, "app_dream_run", json!({})))
        .await
        .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_kb_search<R: Runtime>(
    host: AppHandle<R>,
    query: String,
    limit: Option<usize>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_kb_search", json!({ "query": query, "limit": limit }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 8. Browser Session Fallback RPCs & Push RPCs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_browser_sessions_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_browser_sessions_list", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_browser_session_open<R: Runtime>(
    host: AppHandle<R>,
    target: Option<String>,
    url: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_browser_session_open",
            json!({ "target": target.or(url) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_browser_session_clear<R: Runtime>(
    host: AppHandle<R>,
    domain: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_browser_session_clear",
            json!({ "domain": domain.unwrap_or_else(|| "*".to_string()) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_browser_takeover_complete<R: Runtime>(
    host: AppHandle<R>,
    takeover_id: Option<String>,
    id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_browser_takeover_complete",
            json!({ "takeoverId": takeover_id.or(id) }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_push_vapid_public_key<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_push_vapid_public_key", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_push_subscribe<R: Runtime>(
    host: AppHandle<R>,
    subscription: Option<Value>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_push_subscribe",
            json!({ "subscription": subscription }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

// ---------------------------------------------------------------------------
// 9. Host Workspaces & Offline Queue RPCs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn app_host_workspaces_list<R: Runtime>(host: AppHandle<R>) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_host_workspaces_list", json!({}))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_host_workspace_grant<R: Runtime>(
    host: AppHandle<R>,
    path: String,
    label: Option<String>,
    read_only: Option<bool>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_host_workspace_grant",
            json!({
                "path": path,
                "label": label,
                "readOnly": read_only
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_host_workspace_revoke<R: Runtime>(
    host: AppHandle<R>,
    id: String,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_host_workspace_revoke", json!({ "id": id }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_host_queue_list<R: Runtime>(
    host: AppHandle<R>,
    node_id: Option<String>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(&host, "app_host_queue_list", json!({ "nodeId": node_id }))
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_host_queue_enqueue<R: Runtime>(
    host: AppHandle<R>,
    node_id: String,
    tool_name: String,
    args: Option<Value>,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_host_queue_enqueue",
            json!({
                "nodeId": node_id,
                "toolName": tool_name,
                "args": args.unwrap_or(json!({}))
            }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}

#[tauri::command]
pub async fn app_host_queue_reconcile<R: Runtime>(
    host: AppHandle<R>,
    node_id: String,
) -> Result<Value, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_rpc(
            &host,
            "app_host_queue_reconcile",
            json!({ "nodeId": node_id }),
        )
    })
    .await
    .map_err(|_| "RUNTIME_RPC_FAILED")?
}
