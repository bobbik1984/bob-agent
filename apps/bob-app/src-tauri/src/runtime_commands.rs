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
