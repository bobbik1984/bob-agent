mod scan_commands;
#[cfg(target_os = "android")]
mod android_scanner;
mod direct_llm;
mod home_host;
mod home_link;
mod device_identity;
mod identity_commands;
mod home_config;
mod pairing_commands;
#[cfg(test)]
mod home_execution_tests;
use home_link::{app_home_sync, HomeLinkState};
#[cfg(target_os = "android")]
mod android_vault;
mod device_vault;
mod runner;
use bob_kernel::app::{AppMessage, AppReadiness, LocalApp};
use bob_kernel::catalog::{CatalogSnapshot, ConversationViewState, DeviceUiPreferences};
use bob_kernel::local_execution::LocalExecution;
use bob_kernel::outbox::LocalAccepted;
use bob_kernel::personal_ops::{CaptureItem, DecisionItem, MemoryItem, NoteItem, ScheduleItem};
pub use runner::worker_entry;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};

struct AppState(Arc<Mutex<Option<LocalApp>>>);
struct RunnerState {
    program: Option<PathBuf>,
    active: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}
impl Default for RunnerState {
    fn default() -> Self {
        Self {
            program: runner::configured_program(),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Serialize)]
struct Bootstrap {
    session_id: String,
    provider_status: &'static str,
}
fn now() -> Result<i64, &'static str> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "CLOCK_UNAVAILABLE")?
        .as_millis();
    i64::try_from(millis).map_err(|_| "CLOCK_UNAVAILABLE")
}
fn with_app<T>(
    state: &Arc<Mutex<Option<LocalApp>>>,
    work: impl FnOnce(&mut LocalApp) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let mut guard = state.lock().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    work(guard.as_mut().ok_or("LOCAL_STORE_UNAVAILABLE")?)
}
#[tauri::command]
async fn app_bootstrap<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Bootstrap, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.lock().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        if guard.is_none() {
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
            std::fs::create_dir_all(&dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
            *guard = Some(
                LocalApp::open(&dir.join("kernel.sqlite"))
                    .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?,
            );
        }
        let local = guard.as_mut().ok_or("LOCAL_STORE_UNAVAILABLE")?;
        for execution in local
            .managed_executions()
            .map_err(|_| "EXECUTION_READ_FAILED")?
        {
            if execution.status == "unknown" {
                if let Some(job) = execution.owner_job.as_ref() {
                    if *job == runner::job_name(&execution.id)
                        && runner::recovered_tree_is_gone(job)
                    {
                        let receipts = app
                            .path()
                            .app_data_dir()
                            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?
                            .join("reply-receipts");
                        match read_receipt(&receipts, &execution.id) {
                            Ok(Some(receipt)) if receipt.cleanup => {
                                local
                                    .restore_local_completion(
                                        &execution.id,
                                        &receipt.stream,
                                        receipt.exit,
                                        receipt.reason.as_deref(),
                                        now()?,
                                    )
                                    .map_err(|_| "RECOVERY_UNCONFIRMED")?;
                                let _ = remove_receipt(&receipts, &execution.id);
                            }
                            Ok(None) => local
                                .reconcile_local_execution(&execution.id, true, now()?)
                                .map_err(|_| "RECOVERY_UNCONFIRMED")?,
                            _ => {} // Corrupt/unconfirmed evidence is retained; no invented terminal.
                        }
                    }
                }
            }
        }
        Ok(Bootstrap {
            session_id: local
                .selected_conversation()
                .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?,
            provider_status: local.prewarm(),
        })
    })
    .await
    .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?
}
#[tauri::command]
async fn app_accept(
    state: State<'_, AppState>,
    request_id: String,
    message_id: String,
    session_id: String,
    content: String,
    expected_draft_revision: Option<i64>,
) -> Result<LocalAccepted, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.accept_with_draft_revision(
                &request_id,
                &message_id,
                &session_id,
                content,
                expected_draft_revision,
                now()?,
            )
            .map_err(|_| "LOCAL_SAVE_UNCONFIRMED")
        })
    })
    .await
    .map_err(|_| "LOCAL_SAVE_UNCONFIRMED")?
}
#[tauri::command]
async fn app_history(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<AppMessage>, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.history(&session_id)
                .map_err(|_| "LOCAL_HISTORY_UNAVAILABLE")
        })
    })
    .await
    .map_err(|_| "LOCAL_HISTORY_UNAVAILABLE")?
}
#[tauri::command]
async fn app_readiness(
    state: State<'_, AppState>,
    runner: State<'_, RunnerState>,
) -> Result<AppReadiness, &'static str> {
    let enabled = runner.program.is_some();
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            let mut status = app.readiness().map_err(|_| "LOCAL_STATUS_UNAVAILABLE")?;
            let direct_ready = app.direct_provider_ready().unwrap_or(false);
            let session_id = app.selected_conversation().unwrap_or_default();
            let route = app
                .conversation_execution_route(&session_id)
                .unwrap_or_else(|_| "auto".to_owned());
            if app.paired_home().map_err(|_| "HOME_STATE_UNAVAILABLE")?.is_some() {
                if route == "direct_byok" && direct_ready {
                    status.mode = "direct_provider";
                    status.execution_available = true;
                } else {
                    status.mode = "home_configured";
                    status.execution_available = false; // Configuration is not remote readiness.
                }
            } else if route == "direct_byok" && direct_ready {
                status.mode = "direct_provider";
                status.execution_available = true;
            } else if enabled {
                status.mode = "codex_readonly";
                status.execution_available = true;
            } else if direct_ready {
                status.mode = "direct_provider";
                status.execution_available = true;
            }
            Ok(status)
        })
    })
    .await
    .map_err(|_| "LOCAL_STATUS_UNAVAILABLE")?
}
#[tauri::command]
fn app_prewarm() -> &'static str {
    "provider_unconfigured"
}

#[tauri::command]
async fn app_catalog(state: State<'_, AppState>) -> Result<CatalogSnapshot, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")
        })
    })
    .await
    .map_err(|_| "CATALOG_READ_FAILED")?
}

#[tauri::command]
async fn app_project_create(
    state: State<'_, AppState>,
    project_id: String,
    title: String,
) -> Result<CatalogSnapshot, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.create_project(&project_id, &title, now()?)
                .map_err(|_| "PROJECT_CREATE_FAILED")?;
            app.catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")
        })
    })
    .await
    .map_err(|_| "PROJECT_CREATE_FAILED")?
}

#[tauri::command]
async fn app_conversation_create(
    state: State<'_, AppState>,
    conversation_id: String,
    project_id: Option<String>,
    title: String,
) -> Result<CatalogSnapshot, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.create_conversation(&conversation_id, project_id.as_deref(), &title, now()?)
                .map_err(|_| "CONVERSATION_CREATE_FAILED")?;
            app.catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")
        })
    })
    .await
    .map_err(|_| "CONVERSATION_CREATE_FAILED")?
}

#[tauri::command]
async fn app_conversation_promote(
    state: State<'_, AppState>,
    conversation_id: String,
    project_id: String,
    title: String,
) -> Result<CatalogSnapshot, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.promote_conversation_to_project(&conversation_id, &project_id, &title, now()?)
                .map_err(|_| "CONVERSATION_PROMOTE_FAILED")
        })
    })
    .await
    .map_err(|_| "CONVERSATION_PROMOTE_FAILED")?
}

#[tauri::command]
async fn app_conversation_select(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<CatalogSnapshot, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.select_conversation(&conversation_id, now()?)
                .map_err(|_| "CONVERSATION_SELECT_FAILED")?;
            app.catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")
        })
    })
    .await
    .map_err(|_| "CONVERSATION_SELECT_FAILED")?
}

#[tauri::command]
async fn app_view_state_save(
    state: State<'_, AppState>,
    conversation_id: String,
    draft: Option<String>,
    scroll_anchor_message_id: Option<Option<String>>,
    scroll_offset: Option<i64>,
    mark_read: Option<bool>,
) -> Result<ConversationViewState, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            let anchor = scroll_anchor_message_id
                .as_ref()
                .map(|inner| inner.as_deref());
            app.save_conversation_view_state(
                &conversation_id,
                draft.as_deref(),
                anchor,
                scroll_offset,
                mark_read.unwrap_or(false),
                now()?,
            )
            .map_err(|_| "VIEW_STATE_SAVE_FAILED")
        })
    })
    .await
    .map_err(|_| "VIEW_STATE_SAVE_FAILED")?
}

#[tauri::command]
async fn app_preferences_save(
    state: State<'_, AppState>,
    sidebar_group_mode: Option<String>,
    locale: Option<String>,
    theme: Option<String>,
) -> Result<DeviceUiPreferences, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.save_ui_preferences(
                sidebar_group_mode.as_deref(),
                locale.as_deref(),
                theme.as_deref(),
                now()?,
            )
            .map_err(|_| "PREFERENCES_SAVE_FAILED")
        })
    })
    .await
    .map_err(|_| "PREFERENCES_SAVE_FAILED")?
}

#[derive(Serialize)]
struct PersonalOpsSnapshot {
    schedules: Vec<ScheduleItem>,
    notes: Vec<NoteItem>,
    captures: Vec<CaptureItem>,
    memories: Vec<MemoryItem>,
    decisions: Vec<DecisionItem>,
    pending_sync_count: usize,
}

#[tauri::command]
async fn app_personal_snapshot(
    state: State<'_, AppState>,
) -> Result<PersonalOpsSnapshot, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            let schedules = app
                .list_schedule_items()
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let notes = app
                .list_notes(None)
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let captures = app
                .list_captures()
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let memories = app
                .list_memories(false)
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let decisions = app
                .list_all_decisions()
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let pending_sync_count = app
                .pending_personal_op_syncs()
                .map(|v| v.len())
                .unwrap_or(0);
            Ok(PersonalOpsSnapshot {
                schedules,
                notes,
                captures,
                memories,
                decisions,
                pending_sync_count,
            })
        })
    })
    .await
    .map_err(|_| "PERSONAL_OPS_READ_FAILED")?
}

fn sync_project_todo_markdown(app: &LocalApp, item: &ScheduleItem) {
    if item.kind == "event" {
        return;
    }
    let Some(proj_id) = item.project_id.as_deref() else {
        return;
    };
    let Ok(catalog) = app.catalog_snapshot() else {
        return;
    };
    let proj_title = catalog
        .projects
        .iter()
        .find(|p| p.id == proj_id)
        .map(|p| p.title.as_str())
        .unwrap_or(proj_id);
    let Ok(workspaces) = app.workspace_inventory(Some(proj_id)) else {
        return;
    };
    let Some(ws) = workspaces
        .iter()
        .find(|w| std::path::Path::new(&w.path_ref).is_dir())
    else {
        return;
    };
    let dir_path = std::path::Path::new(&ws.path_ref);
    let todo_path = dir_path.join("todo.md");
    let existing = std::fs::read_to_string(&todo_path).unwrap_or_default();
    if existing.contains(&item.title) {
        return;
    }
    let checkbox = if item.status == "completed" { "[x]" } else { "[ ]" };
    let line = if item.notes.trim().is_empty() {
        format!("- {checkbox} {}\n", item.title.trim())
    } else {
        format!(
            "- {checkbox} {} — {}\n",
            item.title.trim(),
            item.notes.trim().replace('\n', " ")
        )
    };
    let updated = if existing.is_empty() {
        format!("# {proj_title} Todo\n\n{line}")
    } else if existing.ends_with('\n') {
        format!("{existing}{line}")
    } else {
        format!("{existing}\n{line}")
    };
    let _ = std::fs::write(&todo_path, updated.as_bytes());
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn app_schedule_create(
    state: State<'_, AppState>,
    id: String,
    project_id: Option<String>,
    task_id: Option<String>,
    title: String,
    kind: Option<String>,
    start_at: i64,
    end_at: Option<i64>,
    reminder_at: Option<i64>,
    notes: Option<String>,
) -> Result<ScheduleItem, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            let created = app
                .create_schedule_item_with_kind(
                    &id,
                    project_id.as_deref(),
                    task_id.as_deref(),
                    &title,
                    kind.as_deref().unwrap_or("event"),
                    start_at,
                    end_at,
                    reminder_at,
                    notes.as_deref().unwrap_or(""),
                    now()?,
                )
                .map_err(|_| "SCHEDULE_CREATE_FAILED")?;
            sync_project_todo_markdown(app, &created);
            Ok(created)
        })
    })
    .await
    .map_err(|_| "SCHEDULE_CREATE_FAILED")?
}

#[tauri::command]
async fn app_schedule_status(
    state: State<'_, AppState>,
    id: String,
    status: String,
) -> Result<ScheduleItem, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.update_schedule_status(&id, &status, now()?)
                .map_err(|_| "SCHEDULE_UPDATE_FAILED")
        })
    })
    .await
    .map_err(|_| "SCHEDULE_UPDATE_FAILED")?
}

#[tauri::command]
async fn app_note_upsert(
    state: State<'_, AppState>,
    id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    title: String,
    body_markdown: String,
    tags: Option<Vec<String>>,
) -> Result<NoteItem, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.upsert_note(
                &id,
                project_id.as_deref(),
                conversation_id.as_deref(),
                &title,
                &body_markdown,
                &tags.unwrap_or_default(),
                now()?,
            )
            .map_err(|_| "NOTE_SAVE_FAILED")
        })
    })
    .await
    .map_err(|_| "NOTE_SAVE_FAILED")?
}

#[tauri::command]
async fn app_capture_media_put(state: State<'_, AppState>, mime:String, base64:String)->Result<serde_json::Value,&'static str> {
 let state=state.0.clone();
 tauri::async_runtime::spawn_blocking(move||with_app(&state,|app|{
  let id=app.put_capture_media(&mime,&base64,now()?).map_err(|_|"CAPTURE_IMAGE_INVALID")?;
  Ok(serde_json::json!({"id":id}))
 })).await.map_err(|_|"CAPTURE_IMAGE_INVALID")?
}
#[tauri::command]
async fn app_capture_media_get(state: State<'_, AppState>, media_id:Option<String>, capture_id:Option<String>)->Result<serde_json::Value,&'static str> {
 let state=state.0.clone();
 tauri::async_runtime::spawn_blocking(move||with_app(&state,|app|{
  let id=match media_id {Some(id)=>Some(id),None=>app.capture_image_id(capture_id.as_deref().unwrap_or("")).map_err(|_|"CAPTURE_IMAGE_MISSING")?};
  match id {Some(id)=>serde_json::to_value(app.capture_media(&id).map_err(|_|"CAPTURE_IMAGE_MISSING")?).map_err(|_|"CAPTURE_IMAGE_MISSING"),None=>Ok(serde_json::Value::Null)}
 })).await.map_err(|_|"CAPTURE_IMAGE_MISSING")?
}
#[tauri::command]
async fn app_capture_create(
    state: State<'_, AppState>,
    id: String,
    source_message_id: Option<String>,
    content_ref: String,
    project_id: Option<String>,
    new_project_title: Option<String>,
    media_id: Option<String>,
) -> Result<CaptureItem, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.capture_with_image(&id, source_message_id.as_deref(), &content_ref, project_id.as_deref(), new_project_title.as_deref(), media_id.as_deref(), now()?)
                .map_err(|_| "CAPTURE_SAVE_FAILED")
        })
    })
    .await
    .map_err(|_| "CAPTURE_SAVE_FAILED")?
}

#[tauri::command]
async fn app_memory_state(
    state: State<'_, AppState>,
    id: String,
    new_state: String,
) -> Result<MemoryItem, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.update_memory_state(&id, &new_state)
                .map_err(|_| "MEMORY_UPDATE_FAILED")
        })
    })
    .await
    .map_err(|_| "MEMORY_UPDATE_FAILED")?
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn app_decision_record(
    state: State<'_, AppState>,
    id: String,
    project_id: Option<String>,
    task_id: Option<String>,
    source_message_id: Option<String>,
    body: String,
    rationale_json: Option<String>,
    status: Option<String>,
    supersedes_id: Option<String>,
) -> Result<DecisionItem, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            let at = now()?;
            // Ensure a project and source message exist if caller didn't pass one
            let proj_id = match project_id.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(pid) => pid.to_owned(),
                None => {
                    let snap = app
                        .catalog_snapshot()
                        .map_err(|_| "DECISION_SAVE_FAILED")?;
                    if let Some(first) = snap.projects.first() {
                        first.id.clone()
                    } else {
                        let pid = "proj-default".to_owned();
                        let _ = app.create_project(&pid, "Default Workspace", at);
                        pid
                    }
                }
            };
            let msg_id = match source_message_id.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(mid) => mid.to_owned(),
                None => {
                    let session_id = app
                        .selected_conversation()
                        .map_err(|_| "DECISION_SAVE_FAILED")?;
                    let hist = app.history(&session_id).unwrap_or_default();
                    if let Some(last) = hist.last() {
                        last.id.clone()
                    } else {
                        let req_id = format!("dec-src-req-{id}");
                        let mid = format!("dec-src-msg-{id}");
                        app.accept(&req_id, &mid, &session_id, format!("Decision record: {body}"), at)
                            .map_err(|_| "DECISION_SAVE_FAILED")?;
                        mid
                    }
                }
            };
            app.record_decision(
                &id,
                &proj_id,
                task_id.as_deref(),
                &msg_id,
                &body,
                rationale_json.as_deref().unwrap_or("{}"),
                status.as_deref().unwrap_or("proposed"),
                supersedes_id.as_deref(),
            )
            .map_err(|_| "DECISION_SAVE_FAILED")
        })
    })
    .await
    .map_err(|_| "DECISION_SAVE_FAILED")?
}

#[tauri::command]
async fn app_decision_status(
    state: State<'_, AppState>,
    id: String,
    status: String,
) -> Result<DecisionItem, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.update_decision_status(&id, &status)
                .map_err(|_| "DECISION_UPDATE_FAILED")
        })
    })
    .await
    .map_err(|_| "DECISION_UPDATE_FAILED")?
}

#[tauri::command]
async fn app_executions(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<serde_json::Value>, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            let mut result = app
                .local_executions(&session_id)
                .map_err(|_| "EXECUTION_READ_FAILED")?
                .into_iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "EXECUTION_READ_FAILED")?;
            result.extend(
                app.remote_executions(&session_id)
                    .map_err(|_| "EXECUTION_READ_FAILED")?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "EXECUTION_READ_FAILED")?,
            );
            Ok(result)
        })
    })
    .await
    .map_err(|_| "EXECUTION_READ_FAILED")?
}
#[tauri::command]
async fn app_cancel(
    state: State<'_, AppState>,
    runner: State<'_, RunnerState>,
    execution_id: String,
) -> Result<serde_json::Value, &'static str> {
    if execution_id.starts_with("direct-execution:") {
        with_app(&state.0, |app| {
            let changed = app
                .request_direct_stop(&execution_id, now()?)
                .map_err(|_| "CANCEL_UNCONFIRMED")?;
            if !changed {
                return Err("CANCEL_UNCONFIRMED");
            }
            Ok(())
        })?;
        if let Some(flag) = runner
            .active
            .lock()
            .map_err(|_| "RUNNER_UNAVAILABLE")?
            .get(&execution_id)
        {
            flag.store(true, Ordering::SeqCst);
        }
        return with_app(&state.0, |app| {
            let session = app
                .selected_conversation()
                .map_err(|_| "EXECUTION_READ_FAILED")?;
            app.local_executions(&session)
                .map_err(|_| "EXECUTION_READ_FAILED")?
                .into_iter()
                .find(|e| e.id == execution_id)
                .ok_or("EXECUTION_NOT_FOUND")
                .and_then(|e| serde_json::to_value(e).map_err(|_| "EXECUTION_READ_FAILED"))
        });
    }
    if let Ok(remote_view) = with_app(&state.0, |app| {
        let view = app
            .request_remote_stop(&execution_id, now()?)
            .map_err(|_| "CANCEL_UNCONFIRMED")?;
        serde_json::to_value(view).map_err(|_| "CANCEL_UNCONFIRMED")
    }) {
        return Ok(remote_view);
    }
    // Durable intent first. A signal request is not termination confirmation.
    with_app(&state.0, |app| {
        app.request_local_stop(&execution_id, now()?)
            .map_err(|_| "CANCEL_UNCONFIRMED")
    })?;
    if let Some(flag) = runner
        .active
        .lock()
        .map_err(|_| "RUNNER_UNAVAILABLE")?
        .get(&execution_id)
    {
        flag.store(true, Ordering::SeqCst);
    }
    with_app(&state.0, |app| {
        app.managed_executions()
            .map_err(|_| "EXECUTION_READ_FAILED")?
            .into_iter()
            .find(|e| e.id == execution_id)
            .ok_or("EXECUTION_NOT_FOUND")
            .and_then(|e| serde_json::to_value(e).map_err(|_| "EXECUTION_READ_FAILED"))
    })
}
#[tauri::command]
async fn app_execute<R: tauri::Runtime>(
    host: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    runner: State<'_, RunnerState>,
    request_id: String,
) -> Result<LocalExecution, &'static str> {
    let use_direct = with_app(&state.0, |app| {
        let direct_ready = app.direct_provider_ready().unwrap_or(false);
        if !direct_ready {
            return Ok(false);
        }
        let session = app.selected_conversation().unwrap_or_default();
        let route = app
            .conversation_execution_route(&session)
            .unwrap_or_else(|_| "auto".to_owned());
        let has_paired_home = app.paired_home().unwrap_or(None).is_some();
        Ok(route == "direct_byok"
            || (!has_paired_home && runner.program.is_none() && route != "local_cli"))
    })?;
    if use_direct {
        {
            let active = runner.active.lock().map_err(|_| "RUNNER_UNAVAILABLE")?;
            if !active.is_empty() {
                return Err("RUNNER_BUSY");
            }
        }
        let prepared = with_app(&state.0, |app| {
            app.prepare_direct_execution(&request_id, now()?)
                .map_err(|_| "EXECUTION_PREPARE_FAILED")
        })?;
        return direct_llm::execute_direct(host, &state, &runner, prepared).await;
    }
    let program = runner.program.clone().ok_or("PROVIDER_UNCONFIGURED")?;
    let receipts = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?
        .join("reply-receipts");
    std::fs::create_dir_all(&receipts).map_err(|_| "RECEIPT_STORE_UNAVAILABLE")?;
    let mut active = runner.active.lock().map_err(|_| "RUNNER_UNAVAILABLE")?;
    // One local CLI at a time. Renderer retries can read, but cannot create a second process.
    let existing = with_app(&state.0, |app| {
        app.managed_executions()
            .map_err(|_| "EXECUTION_READ_FAILED")
    })?;
    if let Some(mut execution) = existing.into_iter().find(|e| e.request_id == request_id) {
        execution.can_start = false;
        return Ok(execution);
    }
    if !active.is_empty() {
        return Err("RUNNER_BUSY");
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "CLOCK_UNAVAILABLE")?
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("bob-readonly-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&directory).map_err(|_| "WORKSPACE_UNAVAILABLE")?;
    let execution = with_app(&state.0, |app| {
        app.prepare_local_execution(&request_id, &directory.to_string_lossy(), now()?)
            .map_err(|_| "EXECUTION_PREPARE_FAILED")
    })?;
    if !execution.can_start {
        return Ok(execution);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    active.insert(execution.id.clone(), cancel.clone());
    let owner = state.0.clone();
    let runners = runner.active.clone();
    let task = execution.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let job = runner::job_name(&task.id);
        let result = runner::execute(
            &program,
            &directory,
            task.prompt.clone(),
            cancel,
            Duration::from_secs(120),
            &job,
            || {
                with_app(&owner, |app| {
                    app.record_local_owner(&task.id, &job, now()?)
                        .map_err(|_| "OWNER_SAVE_FAILED")?;
                    app.mark_local_started(&task.id, now()?)
                        .map_err(|_| "START_NOT_CONFIRMED")
                })
                .is_ok()
            },
        );
        let receipt = CompletionReceipt {
            execution_id: task.id.clone(),
            stream: result.stream.clone(),
            exit: result.exit,
            reason: result.reason.map(str::to_owned),
            cleanup: result.cleanup,
        };
        let _saved = save_receipt(&receipts, &receipt).is_ok();
        let mut committed = Err("RESULT_SAVE_UNCONFIRMED");
        for _ in 0..3 {
            committed = with_app(&owner, |app| {
                app.finish_local_execution(
                    &task.id,
                    &result.stream,
                    result.exit,
                    result.reason,
                    result.cleanup,
                    now()?,
                )
                .map_err(|_| "RESULT_SAVE_UNCONFIRMED")
            });
            if committed.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if committed.is_ok() {
            // Both a complete spool and an interrupted pending write are now
            // redundant: the exact outcome has committed to the kernel.
            let _ = remove_receipt(&receipts, &task.id);
        }
        // Retain capacity if either cleanup or durable outcome is unconfirmed.
        if result.cleanup && committed.is_ok() {
            if let Ok(mut active) = runners.lock() {
                active.remove(&task.id);
            }
        }
        // Empty workspace only; do not recursively delete any model-created material.
        if result.cleanup {
            let _ = std::fs::remove_dir(&directory);
        }
    });
    Ok(execution)
}

fn configure<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(android_vault::init())
        .plugin(android_scanner::init());
    builder
        .manage(AppState(Arc::new(Mutex::new(None))))
        .manage(RunnerState::default())
        .manage(HomeLinkState::default())
        .manage(pairing_commands::PairingState::default())
        .invoke_handler(tauri::generate_handler![
            pairing_commands::app_pairing_device_code,
            pairing_commands::app_pairing_home_invite,
            pairing_commands::app_pairing_status,
            pairing_commands::app_pairing_propose,
            pairing_commands::app_pairing_home_confirm,
            pairing_commands::app_pairing_revoke,
            pairing_commands::app_pairing_finish,
            pairing_commands::app_pairing_enable_home,
            pairing_commands::app_pairing_reset,
            pairing_commands::app_pairing_recover,
            identity_commands::app_identity_create,
            identity_commands::app_identity_public,
            scan_commands::app_scan_pairing_code,
            pairing_commands::app_pairing_capabilities,
            pairing_commands::app_pairing_qr_create,
            pairing_commands::app_pairing_qr_propose,
            app_home_sync,
            app_bootstrap,
            app_accept,
            app_history,
            app_prewarm,
            app_readiness,
            app_catalog,
            app_project_create,
            app_conversation_create,
            app_conversation_promote,
            app_conversation_select,
            app_view_state_save,
            app_preferences_save,
            app_personal_snapshot,
            app_schedule_create,
            app_schedule_status,
            app_note_upsert,
            app_capture_create,
            app_capture_media_put,
            app_capture_media_get,
            direct_llm::app_capture_vision_target,
            direct_llm::app_capture_recognize,
            app_memory_state,
            app_decision_record,
            app_decision_status,
            direct_llm::app_provider_get,
            direct_llm::app_provider_save,
            direct_llm::app_provider_test,
            direct_llm::app_route_get,
            direct_llm::app_route_set,
            direct_llm::app_model_hub_get,
            direct_llm::app_provider_key_set,
            direct_llm::app_model_role_assign,
            direct_llm::app_provider_variant_set,
            direct_llm::app_custom_model_save,
            direct_llm::app_custom_model_delete,
            direct_llm::app_remote_node_save,
            direct_llm::app_remote_node_delete,
            direct_llm::app_registry_save,
            direct_llm::app_model_visibility_set,
            direct_llm::app_models_discover,
            direct_llm::app_online_begin_pairing,
            direct_llm::app_online_exchange_pairing,
            direct_llm::app_online_sync,
            direct_llm::app_online_submit_operation,
            direct_llm::app_online_revoke,
            app_execute,
            app_executions,
            app_cancel
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("native host failed");
}

#[cfg(test)]
mod tests;

#[derive(Serialize, Deserialize)]
struct CompletionReceipt {
    execution_id: String,
    stream: String,
    exit: Option<i32>,
    reason: Option<String>,
    cleanup: bool,
}
fn receipt_path(root: &std::path::Path, id: &str) -> PathBuf {
    root.join(format!("{}.json", id.replace(':', "_")))
}
const RECEIPT_ENCODED_LIMIT: usize = 16 * 1024 * 1024;
const RECEIPT_STREAM_LIMIT: usize = 2 * 1024 * 1024;

fn save_receipt(root: &std::path::Path, receipt: &CompletionReceipt) -> std::io::Result<()> {
    if receipt.stream.len() > RECEIPT_STREAM_LIMIT {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let bytes = serde_json::to_vec(receipt)?;
    if bytes.len() > RECEIPT_ENCODED_LIMIT {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let path = receipt_path(root, &receipt.execution_id);
    let temp = path.with_extension("pending");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temp, path)
}

fn read_receipt(root: &std::path::Path, id: &str) -> std::io::Result<Option<CompletionReceipt>> {
    let path = receipt_path(root, id);
    // A crash may happen after sync_all and before rename. A valid pending
    // record is equivalent result evidence; malformed pending is NOT absence.
    // An existing invalid final record cannot be bypassed via another file.
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::File::open(path.with_extension("pending")) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            }
        }
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    file.take((RECEIPT_ENCODED_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > RECEIPT_ENCODED_LIMIT {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let receipt: CompletionReceipt = serde_json::from_slice(&bytes)?;
    if receipt.execution_id != id || receipt.stream.len() > RECEIPT_STREAM_LIMIT {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    Ok(Some(receipt))
}

/// Invoke only after the matching kernel result transaction has committed.
fn remove_receipt(root: &std::path::Path, id: &str) -> std::io::Result<()> {
    let path = receipt_path(root, id);
    let mut failure = None;
    for candidate in [path.clone(), path.with_extension("pending")] {
        if let Err(error) = std::fs::remove_file(candidate) {
            if error.kind() != std::io::ErrorKind::NotFound && failure.is_none() {
                failure = Some(error);
            }
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
