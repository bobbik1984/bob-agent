mod scan_commands;
#[cfg(target_os = "android")]
mod android_scanner;
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
            if app.paired_home().map_err(|_| "HOME_STATE_UNAVAILABLE")?.is_some() {
                status.mode = "home_configured";
                status.execution_available = false; // Configuration is not remote readiness.
            } else if enabled {
                status.mode = "codex_readonly";
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
            app_conversation_select,
            app_view_state_save,
            app_preferences_save,
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
