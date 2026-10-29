mod device_vault;
mod runner;
use bob_kernel::app::{AppMessage, AppReadiness, LocalApp};
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
        let session = local
            .default_session()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        for execution in local
            .local_executions(&session)
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
                .default_session()
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
) -> Result<LocalAccepted, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.accept(&request_id, &message_id, &session_id, content, now()?)
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
            if enabled {
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
async fn app_executions(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<LocalExecution>, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.local_executions(&session_id)
                .map_err(|_| "EXECUTION_READ_FAILED")
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
) -> Result<LocalExecution, &'static str> {
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
        let session = app
            .default_session()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        app.local_executions(&session)
            .map_err(|_| "EXECUTION_READ_FAILED")?
            .into_iter()
            .find(|e| e.id == execution_id)
            .ok_or("EXECUTION_NOT_FOUND")
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
        let session = app
            .default_session()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        app.local_executions(&session)
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
    builder
        .manage(AppState(Arc::new(Mutex::new(None))))
        .manage(RunnerState::default())
        .invoke_handler(tauri::generate_handler![
            app_bootstrap,
            app_accept,
            app_history,
            app_prewarm,
            app_readiness,
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
