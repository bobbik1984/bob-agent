//! Jarvis SSE streaming gateway & RPC handler for `bob-node`.
//!
//! Provides:
//! - `/api/jarvis/v1/stream` SSE gateway with real-time token/card streaming.
//! - Snake/camelCase wire format mapping for frontend consumption.
//! - Durable suspended turn persistence across HTTP requests.
//! - Graded action gate approvals and budget RPCs.

use crate::{now_millis, NodeContext};
use bob_kernel::app::LocalApp;
use bob_kernel::direct_provider::{DirectExecutionOutcome, DirectProviderProfile};
use bob_runtime::budget::{BudgetStatus, MonthlyBudgetStore};
use bob_runtime::native_harness::{
    BobNativeHarness, HarnessRole, JarvisStreamEvent, SharedAppAccessor, SuspendedTurn,
    TurnUsageSummary,
};
use bob_runtime::tool_bus::{
    ActionRiskLevel, BobToolBus, PendingApprovalCard, PendingApprovalStore, StructuredCard,
};
use bob_runtime::worker_ledger::{WorkerTaskStatus, WorkerTaskStore};
use bob_runtime::ModelHubRuntime;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Wire-Format Mapper
// ---------------------------------------------------------------------------

pub fn risk_to_wire(r: ActionRiskLevel) -> &'static str {
    match r {
        ActionRiskLevel::ReadOnly => "read_only",
        ActionRiskLevel::SideEffect => "side_effect",
        ActionRiskLevel::Irreversible => "irreversible",
        ActionRiskLevel::SettingsChange => "settings",
    }
}

pub fn card_to_wire(c: &StructuredCard) -> Value {
    let items_wire: Vec<Value> = c
        .items
        .iter()
        .map(|i| {
            json!({
                "title": i.title,
                "detail": i.subtitle.clone().unwrap_or_default(),
                "status": i.badge.clone().unwrap_or_default(),
            })
        })
        .collect();

    json!({
        "id": c.id,
        "kind": c.kind,
        "icon": c.icon,
        "title": c.title,
        "summary": c.summary,
        "riskLevel": risk_to_wire(c.risk_level),
        "risk_level": risk_to_wire(c.risk_level),
        "fields": c.fields,
        "items": items_wire,
        "status": c.status.clone().unwrap_or_else(|| "pending".to_owned()),
        "technicalDetail": c.technical_detail,
        "technical_detail": c.technical_detail,
    })
}

pub fn approval_card_to_wire(approval: &PendingApprovalCard) -> Value {
    json!({
        "id": approval.id,
        "tool_name": approval.tool_name,
        "title": approval.card.title,
        "summary": approval.card.summary,
        "risk_level": risk_to_wire(approval.card.risk_level),
        "riskLevel": risk_to_wire(approval.card.risk_level),
        "fields": approval.card.fields,
        "technical_detail": approval.card.technical_detail,
        "technicalDetail": approval.card.technical_detail,
        "status": "pending",
        "created_at": approval.created_at,
        "expires_at": approval.expires_at,
    })
}

pub fn budget_status_to_json(st: &BudgetStatus) -> Value {
    let cap = st.cap_usd;
    let spent = st.spent_usd;
    let remaining = (cap - spent).max(0.0);
    let ratio = if cap > 0.0 {
        (spent / cap).min(1.0)
    } else {
        1.0
    };
    json!({
        "monthly_cap_usd": cap,
        "monthlyCapUsd": cap,
        "spent_usd": spent,
        "spentUsd": spent,
        "remaining_usd": remaining,
        "remainingUsd": remaining,
        "usage_ratio": ratio,
        "usageRatio": ratio,
        "warning_reached": st.near_80_pct,
        "warningReached": st.near_80_pct,
        "cap_reached": st.exceeded,
        "capReached": st.exceeded,
        "month_key": st.month,
        "monthKey": st.month,
        "warned": st.warned,
    })
}

pub fn event_to_wire_json(evt: &JarvisStreamEvent, reply_message_id: Option<&str>) -> Value {
    match evt {
        JarvisStreamEvent::Mood { mood } => json!({
            "type": "status",
            "mood": mood,
        }),
        JarvisStreamEvent::ThinkingDelta { delta } => json!({
            "type": "thinking_delta",
            "delta": delta,
        }),
        JarvisStreamEvent::TextDelta { delta } => json!({
            "type": "text_delta",
            "text": delta,
            "delta": delta,
        }),
        JarvisStreamEvent::ToolStart { call_id, name } => json!({
            "type": "tool_start",
            "call_id": call_id,
            "name": name,
        }),
        JarvisStreamEvent::ToolEnd {
            call_id,
            name,
            ok,
            summary,
        } => json!({
            "type": "tool_end",
            "call_id": call_id,
            "name": name,
            "ok": ok,
            "summary": summary,
        }),
        JarvisStreamEvent::Card { card } => json!({
            "type": "card",
            "card": card_to_wire(card),
        }),
        JarvisStreamEvent::TaskDispatched {
            task_id,
            title,
            objective,
            maker_model,
        } => json!({
            "type": "task_dispatched",
            "taskId": task_id,
            "task_id": task_id,
            "title": title,
            "objective": objective,
            "makerModel": maker_model,
            "maker_model": maker_model,
            "status": "running",
        }),
        JarvisStreamEvent::FollowupBatched {
            task_id,
            queued_count,
        } => json!({
            "type": "followup_batched",
            "taskId": task_id,
            "task_id": task_id,
            "queuedCount": queued_count,
            "queued_count": queued_count,
        }),
        JarvisStreamEvent::ApprovalRequired { approval } => json!({
            "type": "approval_required",
            "approval": approval_card_to_wire(approval),
        }),
        JarvisStreamEvent::BudgetWarning { status } => json!({
            "type": "budget_warning",
            "status": budget_status_to_json(status),
        }),
        JarvisStreamEvent::Done {
            conversation_id,
            full_text,
            usage,
        } => json!({
            "type": "done",
            "conversation_id": conversation_id,
            "message_id": reply_message_id.unwrap_or(conversation_id),
            "reply": full_text,
            "usage": {
                "prompt_tokens": usage.input_tokens,
                "completion_tokens": usage.output_tokens,
                "total_tokens": usage.total_tokens,
                "cost_usd": usage.cost_usd,
            }
        }),
        JarvisStreamEvent::Error { code, message } => json!({
            "type": "error",
            "code": code,
            "message": message,
        }),
    }
}

// ---------------------------------------------------------------------------
// Suspended Turn Persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSuspendedTurn {
    pub suspended: SuspendedTurn,
    pub execution_id: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SuspendedTurnsFile {
    pub turns: HashMap<String, PersistedSuspendedTurn>,
}

pub fn load_suspended_turns(data_dir: &Path) -> SuspendedTurnsFile {
    let path = data_dir.join("jarvis_suspended_turns.json");
    if let Ok(raw) = std::fs::read_to_string(&path) {
        serde_json::from_str::<SuspendedTurnsFile>(&raw).unwrap_or_default()
    } else {
        SuspendedTurnsFile::default()
    }
}

pub fn save_suspended_turns(
    data_dir: &Path,
    file: &SuspendedTurnsFile,
) -> Result<(), &'static str> {
    let path = data_dir.join("jarvis_suspended_turns.json");
    let raw = serde_json::to_string_pretty(file).map_err(|_| "SUSPENDED_SAVE_FAILED")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, raw.as_bytes()).map_err(|_| "SUSPENDED_SAVE_FAILED")?;
    std::fs::rename(&tmp, &path).map_err(|_| "SUSPENDED_SAVE_FAILED")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Provider & Harness Builder (Single-Owner SQLite App Transfer)
// ---------------------------------------------------------------------------

static PLACEHOLDER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn open_placeholder_app() -> Result<(LocalApp, PathBuf), &'static str> {
    let seq = PLACEHOLDER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dummy_dir = std::env::temp_dir().join(format!(
        "bob_placeholder_{}_{}_{}",
        std::process::id(),
        crate::now_millis(),
        seq
    ));
    std::fs::create_dir_all(&dummy_dir).map_err(|_| "TEMP_DIR_FAILED")?;
    let dummy_db = dummy_dir.join("placeholder.sqlite");
    let dummy_app = LocalApp::open(&dummy_db).map_err(|_| "DUMMY_APP_OPEN_FAILED")?;
    Ok((dummy_app, dummy_dir))
}

pub fn make_shared_app_accessor(state: &Arc<Mutex<NodeContext>>) -> SharedAppAccessor {
    let state_clone = state.clone();
    Arc::new(move |cb: &mut dyn FnMut(&mut LocalApp)| {
        if let Ok(mut guard) = state_clone.lock() {
            cb(&mut guard.app);
        }
    })
}

pub fn take_app_from_ctx(ctx: &mut NodeContext) -> Result<(LocalApp, PathBuf), &'static str> {
    let (dummy_app, dummy_dir) = open_placeholder_app()?;
    let real_app = std::mem::replace(&mut ctx.app, dummy_app);
    Ok((real_app, dummy_dir))
}

pub fn restore_app_to_ctx(ctx: &mut NodeContext, real_app: LocalApp, dummy_dir: &Path) {
    let dummy = std::mem::replace(&mut ctx.app, real_app);
    drop(dummy);
    let _ = std::fs::remove_dir_all(dummy_dir);
}

pub fn restore_app(state: &Arc<Mutex<NodeContext>>, real_app: LocalApp, dummy_dir: &Path) {
    if let Ok(mut guard) = state.lock() {
        restore_app_to_ctx(&mut *guard, real_app, dummy_dir);
    }
}

pub fn build_harness(
    data_dir: &Path,
    mut app: LocalApp,
) -> Result<BobNativeHarness, (String, LocalApp)> {
    let runtime = match ModelHubRuntime::with_file_vault(data_dir.to_path_buf()) {
        Ok(r) => r,
        Err(e) => return Err((format!("VAULT_OPEN_FAILED: {e}"), app)),
    };

    if data_dir.join("model_hub_state.json").is_file() {
        let _ = runtime.build_snapshot_and_sync(&mut app);
    }

    let profile = match app.direct_provider_profile() {
        Ok(Some(p)) if p.enabled && p.api_key_configured => p,
        _ => {
            let _ = runtime.build_snapshot_and_sync(&mut app);
            match app.direct_provider_profile() {
                Ok(Some(p)) if p.enabled && p.api_key_configured => p,
                Ok(_) => return Err(("PROVIDER_UNCONFIGURED".to_string(), app)),
                Err(e) => return Err((format!("PROVIDER_READ_FAILED: {e}"), app)),
            }
        }
    };

    let api_key = runtime
        .secrets
        .get(&profile.api_key_slot)
        .unwrap_or_default();

    let budget_store = match MonthlyBudgetStore::open(data_dir) {
        Ok(bs) => bs,
        Err(e) => return Err((format!("BUDGET_STORE_FAILED: {e}"), app)),
    };

    let mut tool_bus = match BobToolBus::with_approval_store(data_dir) {
        Ok(tb) => tb,
        Err(e) => return Err((format!("TOOL_BUS_FAILED: {e}"), app)),
    };
    crate::browser_ws::register_browser_tools(&mut tool_bus, data_dir);

    Ok(BobNativeHarness::new(
        profile,
        api_key,
        budget_store,
        tool_bus,
        app,
    ))
}

pub fn build_harness_outside_lock(data_dir: &Path) -> Result<BobNativeHarness, String> {
    let db_path = data_dir.join("kernel.sqlite");
    let app = LocalApp::open(&db_path).map_err(|e| format!("KERNEL_OPEN_FAILED: {e}"))?;
    build_harness(data_dir, app).map_err(|(e, _)| e)
}

#[derive(Debug, Clone)]
struct DispatchedWorkerSpec {
    task_id: String,
    title: String,
    objective: String,
}

/// Resolves `(clerk_profile_with_key, maker_profile_with_key)` when dual-model routing (`WP-E1.2`)
/// is configured via `model_hub_state.json` (or explicitly requested via `args.dualMode == true`).
pub fn resolve_dual_mode_profiles(
    data_dir: &Path,
    args: &Value,
) -> Option<((DirectProviderProfile, String), (DirectProviderProfile, String))> {
    if args.get("dualMode").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let forced = args.get("dualMode").and_then(Value::as_bool) == Some(true);
    if !forced && !data_dir.join("model_hub_state.json").is_file() {
        return None;
    }
    let runtime = ModelHubRuntime::with_file_vault(data_dir.to_path_buf()).ok()?;
    let (clerk_prof, clerk_key) = runtime.resolve_role_profile_with_key("clerk").ok()?;
    let (maker_prof, maker_key) = runtime.resolve_role_profile_with_key("maker").ok()?;
    if !forced && clerk_prof.model_id == maker_prof.model_id {
        return None;
    }
    Some(((clerk_prof, clerk_key), (maker_prof, maker_key)))
}

pub fn build_clerk_harness(
    data_dir: &Path,
    app: LocalApp,
    clerk_profile: DirectProviderProfile,
    clerk_key: String,
    maker_model: String,
) -> Result<BobNativeHarness, (String, LocalApp)> {
    let budget_store = match MonthlyBudgetStore::open(data_dir) {
        Ok(bs) => bs,
        Err(e) => return Err((format!("BUDGET_STORE_FAILED: {e}"), app)),
    };
    let mut tool_bus = match BobToolBus::with_approval_store(data_dir) {
        Ok(tb) => tb,
        Err(e) => return Err((format!("TOOL_BUS_FAILED: {e}"), app)),
    };
    crate::browser_ws::register_browser_tools(&mut tool_bus, data_dir);
    Ok(BobNativeHarness::new_clerk(
        clerk_profile,
        clerk_key,
        budget_store,
        tool_bus,
        app,
        data_dir.to_path_buf(),
        maker_model,
    ))
}

pub fn build_worker_harness(
    data_dir: &Path,
    app: LocalApp,
    maker_profile: DirectProviderProfile,
    maker_key: String,
    task_id: String,
) -> Result<BobNativeHarness, (String, LocalApp)> {
    let budget_store = match MonthlyBudgetStore::open(data_dir) {
        Ok(bs) => bs,
        Err(e) => return Err((format!("BUDGET_STORE_FAILED: {e}"), app)),
    };
    let mut tool_bus = match BobToolBus::with_approval_store(data_dir) {
        Ok(tb) => tb,
        Err(e) => return Err((format!("TOOL_BUS_FAILED: {e}"), app)),
    };
    crate::browser_ws::register_browser_tools(&mut tool_bus, data_dir);
    Ok(BobNativeHarness::new_worker(
        maker_profile,
        maker_key,
        budget_store,
        tool_bus,
        app,
        data_dir.to_path_buf(),
        task_id,
    ))
}

fn save_worker_assistant_reply(data_dir: &Path, conv_id: &str, task_id: &str, reply_text: &str) {
    let trimmed = reply_text.trim();
    if trimmed.is_empty() {
        return;
    }
    let db_path = data_dir.join("kernel.sqlite");
    let Ok(conn) = rusqlite::Connection::open(&db_path) else {
        return;
    };
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    let now = now_millis();
    let origin: String = conn
        .query_row(
            "SELECT origin_device_id FROM app_local_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "local".to_owned());
    let reply_id = format!("worker-reply:{task_id}");
    let payload = json!({
        "text": trimmed,
        "source": "jarvis_worker",
        "worker_task_id": task_id,
    });
    let _ = conn.execute(
        "INSERT OR REPLACE INTO messages(id, conversation_id, origin_device_id, role, content_json, acceptance, created_at)
         VALUES(?1, ?2, ?3, 'assistant', ?4, 'local', ?5)",
        params![
            reply_id,
            conv_id,
            origin,
            serde_json::to_string(&payload).unwrap_or_default(),
            now
        ],
    );
}

pub fn build_shared_standard_harness(
    state: &Arc<Mutex<NodeContext>>,
    data_dir: &Path,
) -> Result<(BobNativeHarness, PathBuf), String> {
    let runtime = ModelHubRuntime::with_file_vault(data_dir.to_path_buf())
        .map_err(|e| format!("VAULT_OPEN_FAILED: {e}"))?;

    let profile = {
        let mut guard = state
            .lock()
            .map_err(|_| "NODE_LOCK_POISONED".to_string())?;
        if data_dir.join("model_hub_state.json").is_file() {
            let _ = runtime.build_snapshot_and_sync(&mut guard.app);
        }
        match guard.app.direct_provider_profile() {
            Ok(Some(p)) if p.enabled && p.api_key_configured => p,
            _ => {
                let _ = runtime.build_snapshot_and_sync(&mut guard.app);
                match guard.app.direct_provider_profile() {
                    Ok(Some(p)) if p.enabled && p.api_key_configured => p,
                    Ok(_) => return Err("PROVIDER_UNCONFIGURED".to_string()),
                    Err(e) => return Err(format!("PROVIDER_READ_FAILED: {e}")),
                }
            }
        }
    };

    let api_key = runtime
        .secrets
        .get(&profile.api_key_slot)
        .unwrap_or_default();

    let budget_store =
        MonthlyBudgetStore::open(data_dir).map_err(|e| format!("BUDGET_STORE_FAILED: {e}"))?;
    let mut tool_bus =
        BobToolBus::with_approval_store(data_dir).map_err(|e| format!("TOOL_BUS_FAILED: {e}"))?;
    crate::browser_ws::register_browser_tools(&mut tool_bus, data_dir);

    let (placeholder_app, dummy_dir) = open_placeholder_app().map_err(str::to_owned)?;
    let harness = BobNativeHarness::new(profile, api_key, budget_store, tool_bus, placeholder_app)
        .with_app_accessor(make_shared_app_accessor(state));
    Ok((harness, dummy_dir))
}

fn spawn_background_worker_task(
    state: Arc<Mutex<NodeContext>>,
    data_dir: PathBuf,
    conv_id: String,
    user_text: String,
    spec: DispatchedWorkerSpec,
    maker_profile: DirectProviderProfile,
    maker_key: String,
) {
    let _ = WorkerTaskStore::mark_status(&data_dir, &spec.task_id, WorkerTaskStatus::Running);
    std::thread::spawn(move || {
        let task_id = spec.task_id.clone();
        let (placeholder_app, dummy_dir) = match open_placeholder_app() {
            Ok(pair) => pair,
            Err(e) => {
                let _ = WorkerTaskStore::fail_task(&data_dir, &task_id, e, None);
                return;
            }
        };

        let mut worker_harness = match build_worker_harness(
            &data_dir,
            placeholder_app,
            maker_profile,
            maker_key,
            task_id.clone(),
        ) {
            Ok(h) => h.with_app_accessor(make_shared_app_accessor(&state)),
            Err((err_msg, _)) => {
                let _ = std::fs::remove_dir_all(&dummy_dir);
                let _ = WorkerTaskStore::fail_task(&data_dir, &task_id, &err_msg, None);
                return;
            }
        };

        let context_summary = WorkerTaskStore::get_task(&data_dir, &task_id)
            .map(|t| t.context_summary)
            .unwrap_or_default();

        let mut worker_prompt = format!(
            "[Background Worker Task: {}]\nObjective: {}\nOriginal User Request: {}",
            spec.title, spec.objective, user_text
        );
        if !context_summary.trim().is_empty() {
            worker_prompt.push_str(&format!("\nContext Summary: {}", context_summary.trim()));
        }

        let mut captured_done_text = String::new();
        let mut saved_in_done = false;
        let res = worker_harness.run_turn(&conv_id, &worker_prompt, &mut |evt| {
            if let JarvisStreamEvent::Done { ref full_text, .. } = evt {
                captured_done_text = full_text.clone();
                save_worker_assistant_reply(&data_dir, &conv_id, &task_id, full_text);
                saved_in_done = true;
            }
        });

        let suspended = worker_harness.suspended_turn.take();
        drop(worker_harness);
        let _ = std::fs::remove_dir_all(&dummy_dir);

        if let Some(suspended) = suspended {
            let mut st = load_suspended_turns(&data_dir);
            st.turns.insert(
                suspended.approval_id.clone(),
                PersistedSuspendedTurn {
                    suspended,
                    execution_id: None,
                    request_id: Some(format!("worker-task:{task_id}")),
                },
            );
            let _ = save_suspended_turns(&data_dir, &st);
            return;
        }

        match res {
            Ok(final_text) => {
                if !saved_in_done {
                    let text_to_save = if !final_text.trim().is_empty() {
                        final_text
                    } else {
                        captured_done_text
                    };
                    save_worker_assistant_reply(&data_dir, &conv_id, &task_id, &text_to_save);
                }
            }
            Err(err_code) => {
                if let Some(cur) = WorkerTaskStore::get_task(&data_dir, &task_id) {
                    if !cur.status.is_terminal() {
                        let _ = WorkerTaskStore::fail_task(&data_dir, &task_id, &err_code, None);
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Turn Preparation Helper
// ---------------------------------------------------------------------------

fn prepare_turn(
    ctx: &mut NodeContext,
    args: &Value,
    at: i64,
) -> Result<(String, String, String, String, PathBuf), &'static str> {
    let session_id_arg = args
        .get("sessionId")
        .or_else(|| args.get("session_id"))
        .and_then(Value::as_str);
    let request_id_arg = args
        .get("requestId")
        .or_else(|| args.get("request_id"))
        .and_then(Value::as_str);
    let message_id_arg = args
        .get("messageId")
        .or_else(|| args.get("message_id"))
        .and_then(Value::as_str);
    let content_arg = args.get("content").and_then(Value::as_str);

    let conv_id = if let Some(s) = session_id_arg.filter(|s| !s.trim().is_empty()) {
        let _ = ctx.app.import_conversation(s, None, "", at);
        s.to_owned()
    } else {
        ctx.app
            .assistant_conversation(at)
            .unwrap_or_else(|_| "default".to_string())
    };

    let data_dir = ctx.runtime.data_dir.clone();

    if ctx.app.direct_provider_profile().ok().flatten().is_none() {
        let _ = ctx.runtime.build_snapshot_and_sync(&mut ctx.app);
    }

    let history = ctx.app.history(&conv_id).unwrap_or_default();

    let (req_id, user_text) = if let Some(req) = request_id_arg.filter(|r| !r.trim().is_empty()) {
        if let Some(user_msg) = history
            .iter()
            .find(|m| m.role == "user" && m.request_id.as_deref() == Some(req))
        {
            (req.to_owned(), user_msg.content.clone())
        } else if let Some(content) = content_arg.filter(|c| !c.trim().is_empty()) {
            let msg_id = message_id_arg.unwrap_or(req);
            let _ = ctx
                .app
                .accept_with_draft_revision(req, msg_id, &conv_id, content.to_owned(), None, at);
            (req.to_owned(), content.to_owned())
        } else {
            return Err("REQUEST_ID_NOT_FOUND");
        }
    } else if let Some(content) = content_arg.filter(|c| !c.trim().is_empty()) {
        static TURN_REQ_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let seq = TURN_REQ_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let gen_req = format!(
            "req-{}",
            &bob_kernel::outbox::digest(format!("{conv_id}:{at}:{seq}:{content}").as_bytes())[..12]
        );
        let gen_msg = message_id_arg.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "msg-{}",
                &bob_kernel::outbox::digest(format!("{gen_req}:{at}:{seq}").as_bytes())[..12]
            )
        });
        ctx.app
            .accept_with_draft_revision(&gen_req, &gen_msg, &conv_id, content.to_owned(), None, at)
            .map_err(|e| {
                eprintln!("ACCEPT_FAILED: {e:?}");
                "LOCAL_SAVE_UNCONFIRMED"
            })?;
        (gen_req, content.to_owned())
    } else {
        return Err("CONTENT_OR_REQUEST_REQUIRED");
    };

    let prep = ctx
        .app
        .prepare_direct_execution(&req_id, at)
        .map_err(|_| "EXECUTION_PREPARE_FAILED")?;

    Ok((conv_id, req_id, prep.execution_id, user_text, data_dir))
}

fn update_or_save_assistant_reply(
    data_dir: &Path,
    conv_id: &str,
    execution_id: Option<&str>,
    reply_text: &str,
) {
    let db_path = data_dir.join("kernel.sqlite");
    let Ok(conn) = rusqlite::Connection::open(&db_path) else {
        return;
    };
    let now = now_millis();

    if let Some(exec_id) = execution_id {
        let reply_id = format!("direct-reply:{exec_id}");
        let existing: Option<String> = conn
            .query_row(
                "SELECT content_json FROM messages WHERE id = ?1",
                params![reply_id],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);

        if let Some(content_str) = existing {
            let mut val: Value = serde_json::from_str(&content_str).unwrap_or_else(|_| json!({}));
            val["text"] = json!(reply_text);
            let _ = conn.execute(
                "UPDATE messages SET content_json = ?2 WHERE id = ?1",
                params![reply_id, serde_json::to_string(&val).unwrap_or_default()],
            );
            return;
        }
    }

    let origin: String = conn
        .query_row(
            "SELECT origin_device_id FROM app_local_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "local".to_owned());
    let reply_id = format!("jarvis-reply:{now}");
    let payload = json!({
        "text": reply_text,
        "source": "direct_provider",
        "execution_id": execution_id,
    });
    let _ = conn.execute(
        "INSERT INTO messages(id, conversation_id, origin_device_id, role, content_json, acceptance, created_at)
         VALUES(?1, ?2, ?3, 'assistant', ?4, 'local', ?5)",
        params![
            reply_id,
            conv_id,
            origin,
            serde_json::to_string(&payload).unwrap_or_default(),
            now
        ],
    );
}

// ---------------------------------------------------------------------------
// HTTP SSE Stream Handler
// ---------------------------------------------------------------------------

pub fn handle_http_stream(
    mut stream: TcpStream,
    state: &Arc<Mutex<NodeContext>>,
    parsed_body: &Value,
) {
    let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache, no-transform\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
    if stream.write_all(sse_headers.as_bytes()).is_err() {
        return;
    }
    let _ = stream.flush();

    let write_event = |stream: &mut TcpStream, val: &Value| -> bool {
        let payload = format!("data: {}\n\n", serde_json::to_string(val).unwrap_or_default());
        if stream.write_all(payload.as_bytes()).is_err() {
            return false;
        }
        stream.flush().is_ok()
    };

    let args = parsed_body.get("args").unwrap_or(parsed_body);
    let at = now_millis();

    let pre_data_dir = match state.lock() {
        Ok(g) => g.runtime.data_dir.clone(),
        Err(_) => {
            let err_wire = event_to_wire_json(
                &JarvisStreamEvent::Error {
                    code: "NODE_LOCK_POISONED".to_string(),
                    message: "Node lock poisoned".to_string(),
                },
                None,
            );
            let _ = write_event(&mut stream, &err_wire);
            return;
        }
    };

    if let Ok(mut bs) = MonthlyBudgetStore::open(&pre_data_dir) {
        if let Err(code) = bs.can_start_call() {
            let err_wire = event_to_wire_json(
                &JarvisStreamEvent::Error {
                    code: code.to_string(),
                    message: "Monthly spending budget cap reached".to_string(),
                },
                None,
            );
            let _ = write_event(&mut stream, &err_wire);
            return;
        }
    }

    let (conv_id, req_id, execution_id, user_text, data_dir, active_worker) = {
        let mut guard = match state.lock() {
            Ok(g) => g,
            Err(_) => {
                let err_wire = event_to_wire_json(
                    &JarvisStreamEvent::Error {
                        code: "NODE_LOCK_POISONED".to_string(),
                        message: "Node lock poisoned".to_string(),
                    },
                    None,
                );
                let _ = write_event(&mut stream, &err_wire);
                return;
            }
        };
        let (conv_id, req_id, execution_id, user_text, data_dir) = match prepare_turn(&mut *guard, args, at) {
            Ok(t) => t,
            Err(err) => {
                let err_wire = event_to_wire_json(
                    &JarvisStreamEvent::Error {
                        code: err.to_string(),
                        message: err.to_string(),
                    },
                    None,
                );
                let _ = write_event(&mut stream, &err_wire);
                return;
            }
        };

        // WP-E1.2 Thread Direct Subscription: route mid-flight follow-ups directly to active Worker
        if let Some(active_task) = WorkerTaskStore::active_task_for_conversation(&data_dir, &conv_id) {
            if let Ok((_, queued_count)) = WorkerTaskStore::enqueue_followup(
                &data_dir,
                &active_task.task_id,
                &req_id,
                &user_text,
            ) {
                let ack = format!(
                    "Queued for active task ({}) — will incorporate at next step.",
                    active_task.title
                );
                let _ = guard.app.finish_direct_execution(
                    &execution_id,
                    DirectExecutionOutcome::Completed {
                        text: ack.clone(),
                        provider_reported_model: Some(active_task.maker_model.clone()),
                        input_tokens: Some(0),
                        output_tokens: Some(0),
                    },
                    now_millis(),
                );
                (
                    conv_id,
                    req_id,
                    execution_id,
                    user_text,
                    data_dir,
                    Some((active_task.task_id, queued_count, ack)),
                )
            } else {
                (conv_id, req_id, execution_id, user_text, data_dir, None)
            }
        } else {
            (conv_id, req_id, execution_id, user_text, data_dir, None)
        }
    };

    if let Some((task_id, queued_count, ack_text)) = active_worker {
        let reply_message_id = format!("direct-reply:{execution_id}");
        let _ = write_event(
            &mut stream,
            &event_to_wire_json(
                &JarvisStreamEvent::Mood {
                    mood: "speaking".to_string(),
                },
                Some(&reply_message_id),
            ),
        );
        let _ = write_event(
            &mut stream,
            &event_to_wire_json(
                &JarvisStreamEvent::TextDelta {
                    delta: ack_text.clone(),
                },
                Some(&reply_message_id),
            ),
        );
        let _ = write_event(
            &mut stream,
            &event_to_wire_json(
                &JarvisStreamEvent::FollowupBatched {
                    task_id,
                    queued_count,
                },
                Some(&reply_message_id),
            ),
        );
        let _ = write_event(
            &mut stream,
            &event_to_wire_json(
                &JarvisStreamEvent::Mood {
                    mood: "idle".to_string(),
                },
                Some(&reply_message_id),
            ),
        );
        let _ = write_event(
            &mut stream,
            &event_to_wire_json(
                &JarvisStreamEvent::Done {
                    conversation_id: conv_id,
                    full_text: ack_text,
                    usage: TurnUsageSummary::default(),
                },
                Some(&reply_message_id),
            ),
        );
        return;
    }

    let dual_profiles = resolve_dual_mode_profiles(&data_dir, args);
    let mut pending_maker: Option<(DirectProviderProfile, String)> = None;

    let (mut harness, dummy_dir) = if let Some(((clerk_prof, clerk_key), (maker_prof, maker_key))) = dual_profiles {
        let maker_model = maker_prof.model_id.clone();
        pending_maker = Some((maker_prof, maker_key));
        let (placeholder_app, dummy_dir) = match open_placeholder_app() {
            Ok(pair) => pair,
            Err(err) => {
                let err_wire = event_to_wire_json(
                    &JarvisStreamEvent::Error {
                        code: err.to_string(),
                        message: err.to_string(),
                    },
                    None,
                );
                let _ = write_event(&mut stream, &err_wire);
                return;
            }
        };
        match build_clerk_harness(&data_dir, placeholder_app, clerk_prof, clerk_key, maker_model) {
            Ok(h) => (h.with_app_accessor(make_shared_app_accessor(state)), dummy_dir),
            Err((e, _)) => {
                let _ = std::fs::remove_dir_all(&dummy_dir);
                let err_wire = event_to_wire_json(
                    &JarvisStreamEvent::Error {
                        code: "HARNESS_BUILD_FAILED".to_string(),
                        message: e,
                    },
                    None,
                );
                let _ = write_event(&mut stream, &err_wire);
                if let Ok(mut guard) = state.lock() {
                    let _ = guard.app.finish_direct_execution(
                        &execution_id,
                        DirectExecutionOutcome::Failed {
                            reason: "HARNESS_BUILD_FAILED".to_string(),
                        },
                        now_millis(),
                    );
                }
                return;
            }
        }
    } else {
        match build_shared_standard_harness(state, &data_dir) {
            Ok(pair) => pair,
            Err(e) => {
                let err_wire = event_to_wire_json(
                    &JarvisStreamEvent::Error {
                        code: "HARNESS_BUILD_FAILED".to_string(),
                        message: e,
                    },
                    None,
                );
                let _ = write_event(&mut stream, &err_wire);
                if let Ok(mut guard) = state.lock() {
                    let _ = guard.app.finish_direct_execution(
                        &execution_id,
                        DirectExecutionOutcome::Failed {
                            reason: "HARNESS_BUILD_FAILED".to_string(),
                        },
                        now_millis(),
                    );
                }
                return;
            }
        }
    };

    let reply_message_id = format!("direct-reply:{execution_id}");
    let reply_msg_clone = reply_message_id.clone();
    let mut captured_full_text = String::new();
    let mut captured_usage = TurnUsageSummary::default();
    let mut dispatched_workers: Vec<DispatchedWorkerSpec> = Vec::new();
    let mut deferred_done_wire: Option<Value> = None;

    let run_res = harness.run_turn(&conv_id, &user_text, &mut |evt| {
        if let JarvisStreamEvent::TaskDispatched {
            ref task_id,
            ref title,
            ref objective,
            ..
        } = evt
        {
            dispatched_workers.push(DispatchedWorkerSpec {
                task_id: task_id.clone(),
                title: title.clone(),
                objective: objective.clone(),
            });
        }
        if let JarvisStreamEvent::Done {
            ref full_text,
            ref usage,
            ..
        } = evt
        {
            captured_full_text = full_text.clone();
            captured_usage = usage.clone();
            deferred_done_wire = Some(event_to_wire_json(&evt, Some(&reply_msg_clone)));
            return;
        }
        let wire = event_to_wire_json(&evt, Some(&reply_msg_clone));
        let _ = write_event(&mut stream, &wire);
    });

    let model_id = harness.profile.model_id.clone();
    let suspended = harness.suspended_turn.take();
    drop(harness);
    let _ = std::fs::remove_dir_all(&dummy_dir);

    if let Some(suspended) = suspended {
        let mut st = load_suspended_turns(&data_dir);
        st.turns.insert(
            suspended.approval_id.clone(),
            PersistedSuspendedTurn {
                suspended,
                execution_id: Some(execution_id.clone()),
                request_id: Some(req_id.clone()),
            },
        );
        let _ = save_suspended_turns(&data_dir, &st);

        let waiting_text = if !captured_full_text.trim().is_empty() {
            captured_full_text
        } else {
            "[Waiting for confirmation]".to_string()
        };

        if let Ok(mut guard) = state.lock() {
            let _ = guard.app.finish_direct_execution(
                &execution_id,
                DirectExecutionOutcome::Completed {
                    text: waiting_text,
                    provider_reported_model: Some(model_id),
                    input_tokens: Some(captured_usage.input_tokens as i64),
                    output_tokens: Some(captured_usage.output_tokens as i64),
                },
                now_millis(),
            );
        }
        return;
    }

    match run_res {
        Ok(final_text) => {
            let text_to_save = if !final_text.is_empty() {
                final_text
            } else {
                captured_full_text
            };
            if let Ok(mut guard) = state.lock() {
                let _ = guard.app.finish_direct_execution(
                    &execution_id,
                    DirectExecutionOutcome::Completed {
                        text: text_to_save,
                        provider_reported_model: Some(model_id),
                        input_tokens: Some(captured_usage.input_tokens as i64),
                        output_tokens: Some(captured_usage.output_tokens as i64),
                    },
                    now_millis(),
                );
            }
            if let Some((maker_prof, maker_key)) = pending_maker {
                for spec in dispatched_workers {
                    spawn_background_worker_task(
                        state.clone(),
                        data_dir.clone(),
                        conv_id.clone(),
                        user_text.clone(),
                        spec,
                        maker_prof.clone(),
                        maker_key.clone(),
                    );
                }
            }
            if let Some(done_wire) = deferred_done_wire {
                let _ = write_event(&mut stream, &done_wire);
            }
        }
        Err(e) => {
            if let Ok(mut guard) = state.lock() {
                let _ = guard.app.finish_direct_execution(
                    &execution_id,
                    DirectExecutionOutcome::Failed { reason: e },
                    now_millis(),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RPC Command Handlers
// ---------------------------------------------------------------------------

pub fn chat(state: &Arc<Mutex<NodeContext>>, args: &Value) -> Result<Value, &'static str> {
    let at = now_millis();
    let (conv_id, req_id, exec_id, user_text, data_dir, active_worker) = {
        let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        let (conv_id, req_id, exec_id, user_text, data_dir) = prepare_turn(&mut *guard, args, at)?;
        if let Some(active_task) = WorkerTaskStore::active_task_for_conversation(&data_dir, &conv_id) {
            if let Ok((_, queued_count)) = WorkerTaskStore::enqueue_followup(
                &data_dir,
                &active_task.task_id,
                &req_id,
                &user_text,
            ) {
                let ack = format!(
                    "Queued for active task ({}) — will incorporate at next step.",
                    active_task.title
                );
                let _ = guard.app.finish_direct_execution(
                    &exec_id,
                    DirectExecutionOutcome::Completed {
                        text: ack.clone(),
                        provider_reported_model: Some(active_task.maker_model.clone()),
                        input_tokens: Some(0),
                        output_tokens: Some(0),
                    },
                    now_millis(),
                );
                (
                    conv_id,
                    req_id,
                    exec_id,
                    user_text,
                    data_dir,
                    Some((active_task.task_id, queued_count, ack)),
                )
            } else {
                (conv_id, req_id, exec_id, user_text, data_dir, None)
            }
        } else {
            (conv_id, req_id, exec_id, user_text, data_dir, None)
        }
    };

    if let Some((task_id, queued_count, ack_text)) = active_worker {
        return Ok(json!({
            "id": exec_id,
            "message_id": format!("direct-reply:{exec_id}"),
            "reply": ack_text,
            "followupBatched": {
                "taskId": task_id,
                "queuedCount": queued_count,
            },
            "cards": [],
            "approvals": [],
            "usage": {
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0,
                "cost_usd": 0.0,
            }
        }));
    }

    let dual_profiles = resolve_dual_mode_profiles(&data_dir, args);
    let mut pending_maker: Option<(DirectProviderProfile, String)> = None;

    let (mut harness, dummy_dir) = if let Some(((clerk_prof, clerk_key), (maker_prof, maker_key))) = dual_profiles {
        let maker_model = maker_prof.model_id.clone();
        pending_maker = Some((maker_prof, maker_key));
        let (placeholder_app, dummy_dir) = open_placeholder_app()?;
        match build_clerk_harness(&data_dir, placeholder_app, clerk_prof, clerk_key, maker_model) {
            Ok(h) => (h.with_app_accessor(make_shared_app_accessor(state)), dummy_dir),
            Err((e, _)) => {
                let _ = std::fs::remove_dir_all(&dummy_dir);
                if let Ok(mut guard) = state.lock() {
                    let _ = guard.app.finish_direct_execution(
                        &exec_id,
                        DirectExecutionOutcome::Failed { reason: e },
                        now_millis(),
                    );
                }
                return Err("HARNESS_BUILD_FAILED");
            }
        }
    } else {
        match build_shared_standard_harness(state, &data_dir) {
            Ok(pair) => pair,
            Err(e) => {
                if let Ok(mut guard) = state.lock() {
                    let _ = guard.app.finish_direct_execution(
                        &exec_id,
                        DirectExecutionOutcome::Failed { reason: e },
                        now_millis(),
                    );
                }
                return Err("HARNESS_BUILD_FAILED");
            }
        }
    };

    let model_id = harness.profile.model_id.clone();
    let mut cards = Vec::new();
    let mut approvals = Vec::new();
    let mut dispatched_workers: Vec<DispatchedWorkerSpec> = Vec::new();
    let mut usage = TurnUsageSummary::default();
    let mut captured_full_text = String::new();

    let run_res = harness.run_turn(&conv_id, &user_text, &mut |evt| {
        match evt {
            JarvisStreamEvent::TaskDispatched {
                task_id,
                title,
                objective,
                ..
            } => {
                dispatched_workers.push(DispatchedWorkerSpec {
                    task_id,
                    title,
                    objective,
                });
            }
            JarvisStreamEvent::Card { card } => cards.push(card_to_wire(&card)),
            JarvisStreamEvent::ApprovalRequired { approval } => {
                approvals.push(approval_card_to_wire(&approval));
            }
            JarvisStreamEvent::Done {
                full_text: ft,
                usage: u,
                ..
            } => {
                captured_full_text = ft;
                usage = u;
            }
            _ => {}
        }
    });

    let suspended = harness.suspended_turn.take();
    drop(harness);
    let _ = std::fs::remove_dir_all(&dummy_dir);

    if let Some(suspended) = suspended {
        let mut st = load_suspended_turns(&data_dir);
        st.turns.insert(
            suspended.approval_id.clone(),
            PersistedSuspendedTurn {
                suspended,
                execution_id: Some(exec_id.clone()),
                request_id: Some(req_id.clone()),
            },
        );
        let _ = save_suspended_turns(&data_dir, &st);

        let waiting_text = if !captured_full_text.trim().is_empty() {
            captured_full_text
        } else {
            "[Waiting for confirmation]".to_string()
        };

        let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        let _ = guard.app.finish_direct_execution(
            &exec_id,
            DirectExecutionOutcome::Completed {
                text: waiting_text.clone(),
                provider_reported_model: Some(model_id),
                input_tokens: Some(usage.input_tokens as i64),
                output_tokens: Some(usage.output_tokens as i64),
            },
            now_millis(),
        );

        return Ok(json!({
            "id": exec_id,
            "message_id": format!("direct-reply:{exec_id}"),
            "reply": waiting_text,
            "cards": cards,
            "approvals": approvals,
            "usage": {
                "prompt_tokens": usage.input_tokens,
                "completion_tokens": usage.output_tokens,
                "total_tokens": usage.total_tokens,
                "cost_usd": usage.cost_usd,
            }
        }));
    }

    match run_res {
        Ok(final_text) => {
            let text_to_save = if !final_text.is_empty() {
                final_text
            } else {
                captured_full_text
            };
            let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
            let _ = guard.app.finish_direct_execution(
                &exec_id,
                DirectExecutionOutcome::Completed {
                    text: text_to_save.clone(),
                    provider_reported_model: Some(model_id),
                    input_tokens: Some(usage.input_tokens as i64),
                    output_tokens: Some(usage.output_tokens as i64),
                },
                now_millis(),
            );
            drop(guard);
            let first_task_id = dispatched_workers.first().map(|d| d.task_id.clone());
            if let Some((maker_prof, maker_key)) = pending_maker {
                for spec in dispatched_workers {
                    spawn_background_worker_task(
                        state.clone(),
                        data_dir.clone(),
                        conv_id.clone(),
                        user_text.clone(),
                        spec,
                        maker_prof.clone(),
                        maker_key.clone(),
                    );
                }
            }
            Ok(json!({
                "id": exec_id,
                "message_id": format!("direct-reply:{exec_id}"),
                "reply": text_to_save,
                "taskDispatched": first_task_id,
                "cards": cards,
                "approvals": approvals,
                "usage": {
                    "prompt_tokens": usage.input_tokens,
                    "completion_tokens": usage.output_tokens,
                    "total_tokens": usage.total_tokens,
                    "cost_usd": usage.cost_usd,
                }
            }))
        }
        Err(e) => {
            let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
            let _ = guard.app.finish_direct_execution(
                &exec_id,
                DirectExecutionOutcome::Failed { reason: e },
                now_millis(),
            );
            Err("HARNESS_RUN_FAILED")
        }
    }
}

pub fn chat_with_ctx(ctx: &mut NodeContext, args: &Value) -> Result<Value, &'static str> {
    let at = now_millis();
    let (conv_id, req_id, exec_id, user_text, data_dir) = prepare_turn(ctx, args, at)?;
    let (real_app, dummy_dir) = take_app_from_ctx(ctx)?;

    let mut harness = match build_harness(&data_dir, real_app) {
        Ok(h) => h,
        Err((e, returned_app)) => {
            restore_app_to_ctx(ctx, returned_app, &dummy_dir);
            let _ = ctx.app.finish_direct_execution(
                &exec_id,
                DirectExecutionOutcome::Failed { reason: e },
                now_millis(),
            );
            return Err("HARNESS_BUILD_FAILED");
        }
    };

    let model_id = harness.profile.model_id.clone();
    let mut cards = Vec::new();
    let mut approvals = Vec::new();
    let mut usage = TurnUsageSummary::default();
    let mut captured_full_text = String::new();

    let run_res = harness.run_turn(&conv_id, &user_text, &mut |evt| {
        match evt {
            JarvisStreamEvent::Card { card } => cards.push(card_to_wire(&card)),
            JarvisStreamEvent::ApprovalRequired { approval } => {
                approvals.push(approval_card_to_wire(&approval));
            }
            JarvisStreamEvent::Done {
                full_text: ft,
                usage: u,
                ..
            } => {
                captured_full_text = ft;
                usage = u;
            }
            _ => {}
        }
    });

    let suspended = harness.suspended_turn.take();
    let real_app = harness.app.into_inner().unwrap();
    restore_app_to_ctx(ctx, real_app, &dummy_dir);

    if let Some(suspended) = suspended {
        let mut st = load_suspended_turns(&data_dir);
        st.turns.insert(
            suspended.approval_id.clone(),
            PersistedSuspendedTurn {
                suspended,
                execution_id: Some(exec_id.clone()),
                request_id: Some(req_id.clone()),
            },
        );
        let _ = save_suspended_turns(&data_dir, &st);

        let waiting_text = if !captured_full_text.trim().is_empty() {
            captured_full_text
        } else {
            "[Waiting for confirmation]".to_string()
        };

        let _ = ctx.app.finish_direct_execution(
            &exec_id,
            DirectExecutionOutcome::Completed {
                text: waiting_text.clone(),
                provider_reported_model: Some(model_id),
                input_tokens: Some(usage.input_tokens as i64),
                output_tokens: Some(usage.output_tokens as i64),
            },
            now_millis(),
        );

        return Ok(json!({
            "id": exec_id,
            "message_id": format!("direct-reply:{exec_id}"),
            "reply": waiting_text,
            "cards": cards,
            "approvals": approvals,
            "usage": {
                "prompt_tokens": usage.input_tokens,
                "completion_tokens": usage.output_tokens,
                "total_tokens": usage.total_tokens,
                "cost_usd": usage.cost_usd,
            }
        }));
    }

    match run_res {
        Ok(final_text) => {
            let text_to_save = if !final_text.is_empty() {
                final_text
            } else {
                captured_full_text
            };
            let _ = ctx.app.finish_direct_execution(
                &exec_id,
                DirectExecutionOutcome::Completed {
                    text: text_to_save.clone(),
                    provider_reported_model: Some(model_id),
                    input_tokens: Some(usage.input_tokens as i64),
                    output_tokens: Some(usage.output_tokens as i64),
                },
                now_millis(),
            );
            Ok(json!({
                "id": exec_id,
                "message_id": format!("direct-reply:{exec_id}"),
                "reply": text_to_save,
                "cards": cards,
                "approvals": approvals,
                "usage": {
                    "prompt_tokens": usage.input_tokens,
                    "completion_tokens": usage.output_tokens,
                    "total_tokens": usage.total_tokens,
                    "cost_usd": usage.cost_usd,
                }
            }))
        }
        Err(e) => {
            let _ = ctx.app.finish_direct_execution(
                &exec_id,
                DirectExecutionOutcome::Failed { reason: e },
                now_millis(),
            );
            Err("HARNESS_RUN_FAILED")
        }
    }
}

pub fn approve(state: &Arc<Mutex<NodeContext>>, args: &Value) -> Result<Value, &'static str> {
    let approval_id = args
        .get("approvalId")
        .or_else(|| args.get("approval_id"))
        .and_then(Value::as_str)
        .ok_or("APPROVAL_ID_REQUIRED")?;

    let data_dir = {
        let guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        guard.runtime.data_dir.clone()
    };

    let mut st = load_suspended_turns(&data_dir);
    if let Some(persisted) = st.turns.remove(approval_id) {
        let _ = save_suspended_turns(&data_dir, &st);

        let worker_task_id = persisted
            .request_id
            .as_deref()
            .and_then(|r| r.strip_prefix("worker-task:"))
            .map(str::to_owned);

        let (mut harness, dummy_dir) = if let Some(ref tid) = worker_task_id {
            let _ = WorkerTaskStore::mark_status(&data_dir, tid, WorkerTaskStatus::Running);
            if let Some((_, (maker_prof, maker_key))) = resolve_dual_mode_profiles(&data_dir, &json!({ "dualMode": true })) {
                let (placeholder_app, dummy_dir) = open_placeholder_app()?;
                match build_worker_harness(&data_dir, placeholder_app, maker_prof, maker_key, tid.clone()) {
                    Ok(h) => (h.with_app_accessor(make_shared_app_accessor(state)), dummy_dir),
                    Err((_, _)) => {
                        let _ = std::fs::remove_dir_all(&dummy_dir);
                        return Err("HARNESS_BUILD_FAILED");
                    }
                }
            } else {
                match build_shared_standard_harness(state, &data_dir) {
                    Ok((h, dummy_dir)) => (
                        h.with_role(HarnessRole::Worker {
                            data_dir: data_dir.clone(),
                            task_id: tid.clone(),
                        }),
                        dummy_dir,
                    ),
                    Err(_) => return Err("HARNESS_BUILD_FAILED"),
                }
            }
        } else {
            build_shared_standard_harness(state, &data_dir).map_err(|_| "HARNESS_BUILD_FAILED")?
        };
        harness.suspended_turn = Some(persisted.suspended.clone());

        let mut cards = Vec::new();
        let mut resumed_full_text = String::new();
        let res = harness.resume_after_approval(approval_id, true, &mut |evt| {
            match evt {
                JarvisStreamEvent::Card { card } => cards.push(card_to_wire(&card)),
                JarvisStreamEvent::Done { full_text: ft, .. } => {
                    resumed_full_text = ft;
                }
                _ => {}
            }
        });

        let new_suspended = harness.suspended_turn.take();
        drop(harness);
        let _ = std::fs::remove_dir_all(&dummy_dir);

        match res {
            Ok(final_text) => {
                if let Some(new_suspended) = new_suspended {
                    let mut st = load_suspended_turns(&data_dir);
                    st.turns.insert(
                        new_suspended.approval_id.clone(),
                        PersistedSuspendedTurn {
                            suspended: new_suspended,
                            execution_id: persisted.execution_id.clone(),
                            request_id: persisted.request_id.clone(),
                        },
                    );
                    let _ = save_suspended_turns(&data_dir, &st);
                    return Ok(json!({
                        "ok": true,
                        "approvalId": approval_id,
                        "reply": "[Waiting for confirmation]",
                        "cards": cards,
                    }));
                }
                let reply_to_save = if !final_text.is_empty() {
                    final_text
                } else {
                    resumed_full_text
                };
                if let Some(ref exec_id) = persisted.execution_id {
                    if let Ok(mut guard) = state.lock() {
                        let _ = guard.app.finish_direct_execution(
                            exec_id,
                            DirectExecutionOutcome::Completed {
                                text: reply_to_save.clone(),
                                provider_reported_model: None,
                                input_tokens: None,
                                output_tokens: None,
                            },
                            now_millis(),
                        );
                    }
                }
                update_or_save_assistant_reply(
                    &data_dir,
                    &persisted.suspended.conversation_id,
                    persisted.execution_id.as_deref(),
                    &reply_to_save,
                );
                Ok(json!({
                    "ok": true,
                    "approvalId": approval_id,
                    "reply": reply_to_save,
                    "cards": cards,
                }))
            }
            Err(_) => Err("RESUME_AFTER_APPROVAL_FAILED"),
        }
    } else {
        // Standalone approval card or unattended routine draft
        if approval_id.starts_with("draft-") {
            let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
            let mut bus = BobToolBus::with_approval_store(&data_dir).unwrap_or_else(|_| BobToolBus::new());
            crate::browser_ws::register_browser_tools(&mut bus, &data_dir);
            let mut draft_store = bob_runtime::routines::UnattendedDraftStore::open(&data_dir)
                .map_err(|_| "DRAFT_STORE_OPEN_FAILED")?;
            let output = draft_store
                .approve_draft_once(approval_id, &bus, &mut guard.app, now_millis())
                .map_err(|_| "APPROVE_DRAFT_FAILED")?;
            return Ok(json!({
                "ok": true,
                "approvalId": approval_id,
                "reply": "",
                "cards": [],
                "output": output,
            }));
        }
        let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        let mut bus = BobToolBus::with_approval_store(&data_dir).unwrap_or_else(|_| BobToolBus::new());
        crate::browser_ws::register_browser_tools(&mut bus, &data_dir);
        let mut store = PendingApprovalStore::open(&data_dir).map_err(|_| "APPROVALS_OPEN_FAILED")?;
        store
            .approve_once(approval_id, &bus, &mut guard.app)
            .map_err(|_| "APPROVE_FAILED")?;
        Ok(json!({
            "ok": true,
            "approvalId": approval_id,
            "reply": "",
            "cards": [],
        }))
    }
}

pub fn approve_with_ctx(ctx: &mut NodeContext, args: &Value) -> Result<Value, &'static str> {
    let approval_id = args
        .get("approvalId")
        .or_else(|| args.get("approval_id"))
        .and_then(Value::as_str)
        .ok_or("APPROVAL_ID_REQUIRED")?;
    let data_dir = ctx.runtime.data_dir.clone();

    let mut st = load_suspended_turns(&data_dir);
    if let Some(persisted) = st.turns.remove(approval_id) {
        let _ = save_suspended_turns(&data_dir, &st);

        let (real_app, dummy_dir) = take_app_from_ctx(ctx)?;

        let mut harness = match build_harness(&data_dir, real_app) {
            Ok(h) => h,
            Err((_, returned_app)) => {
                restore_app_to_ctx(ctx, returned_app, &dummy_dir);
                return Err("HARNESS_BUILD_FAILED");
            }
        };
        harness.suspended_turn = Some(persisted.suspended.clone());

        let mut cards = Vec::new();
        let mut resumed_full_text = String::new();
        let res = harness.resume_after_approval(approval_id, true, &mut |evt| {
            match evt {
                JarvisStreamEvent::Card { card } => cards.push(card_to_wire(&card)),
                JarvisStreamEvent::Done { full_text: ft, .. } => {
                    resumed_full_text = ft;
                }
                _ => {}
            }
        });

        let new_suspended = harness.suspended_turn.take();
        let real_app = harness.app.into_inner().unwrap();
        restore_app_to_ctx(ctx, real_app, &dummy_dir);

        match res {
            Ok(final_text) => {
                if let Some(new_suspended) = new_suspended {
                    let mut st = load_suspended_turns(&data_dir);
                    st.turns.insert(
                        new_suspended.approval_id.clone(),
                        PersistedSuspendedTurn {
                            suspended: new_suspended,
                            execution_id: persisted.execution_id.clone(),
                            request_id: persisted.request_id.clone(),
                        },
                    );
                    let _ = save_suspended_turns(&data_dir, &st);
                    return Ok(json!({
                        "ok": true,
                        "approvalId": approval_id,
                        "reply": "[Waiting for confirmation]",
                        "cards": cards,
                    }));
                }
                let reply_to_save = if !final_text.is_empty() {
                    final_text
                } else {
                    resumed_full_text
                };
                if let Some(ref exec_id) = persisted.execution_id {
                    let _ = ctx.app.finish_direct_execution(
                        exec_id,
                        DirectExecutionOutcome::Completed {
                            text: reply_to_save.clone(),
                            provider_reported_model: None,
                            input_tokens: None,
                            output_tokens: None,
                        },
                        now_millis(),
                    );
                }
                update_or_save_assistant_reply(
                    &data_dir,
                    &persisted.suspended.conversation_id,
                    persisted.execution_id.as_deref(),
                    &reply_to_save,
                );
                Ok(json!({
                    "ok": true,
                    "approvalId": approval_id,
                    "reply": reply_to_save,
                    "cards": cards,
                }))
            }
            Err(_) => Err("RESUME_AFTER_APPROVAL_FAILED"),
        }
    } else {
        if approval_id.starts_with("draft-") {
            let mut bus = BobToolBus::with_approval_store(&data_dir).unwrap_or_else(|_| BobToolBus::new());
            crate::browser_ws::register_browser_tools(&mut bus, &data_dir);
            let mut draft_store = bob_runtime::routines::UnattendedDraftStore::open(&data_dir)
                .map_err(|_| "DRAFT_STORE_OPEN_FAILED")?;
            let output = draft_store
                .approve_draft_once(approval_id, &bus, &mut ctx.app, now_millis())
                .map_err(|_| "APPROVE_DRAFT_FAILED")?;
            return Ok(json!({
                "ok": true,
                "approvalId": approval_id,
                "reply": "",
                "cards": [],
                "output": output,
            }));
        }
        let mut bus = BobToolBus::with_approval_store(&data_dir).unwrap_or_else(|_| BobToolBus::new());
        crate::browser_ws::register_browser_tools(&mut bus, &data_dir);
        let mut store = PendingApprovalStore::open(&data_dir).map_err(|_| "APPROVALS_OPEN_FAILED")?;
        store
            .approve_once(approval_id, &bus, &mut ctx.app)
            .map_err(|_| "APPROVE_FAILED")?;
        Ok(json!({
            "ok": true,
            "approvalId": approval_id,
            "reply": "",
            "cards": [],
        }))
    }
}

pub fn approve_direct(data_dir: &Path, args: &Value) -> Result<Value, &'static str> {
    let approval_id = args
        .get("approvalId")
        .or_else(|| args.get("approval_id"))
        .and_then(Value::as_str)
        .ok_or("APPROVAL_ID_REQUIRED")?;

    let mut st = load_suspended_turns(data_dir);
    if let Some(persisted) = st.turns.remove(approval_id) {
        let _ = save_suspended_turns(data_dir, &st);
        let mut harness =
            build_harness_outside_lock(data_dir).map_err(|_| "HARNESS_BUILD_FAILED")?;
        harness.suspended_turn = Some(persisted.suspended.clone());

        let mut cards = Vec::new();
        let mut resumed_full_text = String::new();
        let res = harness.resume_after_approval(approval_id, true, &mut |evt| {
            match evt {
                JarvisStreamEvent::Card { card } => cards.push(card_to_wire(&card)),
                JarvisStreamEvent::Done { full_text: ft, .. } => {
                    resumed_full_text = ft;
                }
                _ => {}
            }
        });

        match res {
            Ok(final_text) => {
                if let Some(new_suspended) = harness.suspended_turn.take() {
                    let mut st = load_suspended_turns(data_dir);
                    st.turns.insert(
                        new_suspended.approval_id.clone(),
                        PersistedSuspendedTurn {
                            suspended: new_suspended,
                            execution_id: persisted.execution_id.clone(),
                            request_id: persisted.request_id.clone(),
                        },
                    );
                    let _ = save_suspended_turns(data_dir, &st);
                    return Ok(json!({
                        "ok": true,
                        "approvalId": approval_id,
                        "reply": "[Waiting for confirmation]",
                        "cards": cards,
                    }));
                }
                let reply_to_save = if !final_text.is_empty() {
                    final_text
                } else {
                    resumed_full_text
                };
                update_or_save_assistant_reply(
                    data_dir,
                    &persisted.suspended.conversation_id,
                    persisted.execution_id.as_deref(),
                    &reply_to_save,
                );
                Ok(json!({
                    "ok": true,
                    "approvalId": approval_id,
                    "reply": reply_to_save,
                    "cards": cards,
                }))
            }
            Err(_) => Err("RESUME_AFTER_APPROVAL_FAILED"),
        }
    } else {
        // Standalone approval card or unattended routine draft
        if approval_id.starts_with("draft-") {
            let db_path = data_dir.join("kernel.sqlite");
            let mut app = LocalApp::open(&db_path).map_err(|_| "KERNEL_OPEN_FAILED")?;
            let mut bus = BobToolBus::with_approval_store(data_dir).unwrap_or_else(|_| BobToolBus::new());
            crate::browser_ws::register_browser_tools(&mut bus, data_dir);
            let mut draft_store = bob_runtime::routines::UnattendedDraftStore::open(data_dir)
                .map_err(|_| "DRAFT_STORE_OPEN_FAILED")?;
            let output = draft_store
                .approve_draft_once(approval_id, &bus, &mut app, now_millis())
                .map_err(|_| "APPROVE_DRAFT_FAILED")?;
            return Ok(json!({
                "ok": true,
                "approvalId": approval_id,
                "reply": "",
                "cards": [],
                "output": output,
            }));
        }
        let db_path = data_dir.join("kernel.sqlite");
        let mut app = LocalApp::open(&db_path).map_err(|_| "KERNEL_OPEN_FAILED")?;
        let mut bus = BobToolBus::with_approval_store(data_dir).unwrap_or_else(|_| BobToolBus::new());
        crate::browser_ws::register_browser_tools(&mut bus, data_dir);
        let mut store =
            PendingApprovalStore::open(data_dir).map_err(|_| "APPROVALS_OPEN_FAILED")?;
        store
            .approve_once(approval_id, &bus, &mut app)
            .map_err(|_| "APPROVE_FAILED")?;
        Ok(json!({
            "ok": true,
            "approvalId": approval_id,
            "reply": "",
            "cards": [],
        }))
    }
}

pub fn reject(state: &Arc<Mutex<NodeContext>>, args: &Value) -> Result<Value, &'static str> {
    let approval_id = args
        .get("approvalId")
        .or_else(|| args.get("approval_id"))
        .and_then(Value::as_str)
        .ok_or("APPROVAL_ID_REQUIRED")?;

    let data_dir = {
        let guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        guard.runtime.data_dir.clone()
    };

    let mut st = load_suspended_turns(&data_dir);
    if let Some(persisted) = st.turns.remove(approval_id) {
        let _ = save_suspended_turns(&data_dir, &st);

        let (real_app, dummy_dir) = {
            let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
            take_app_from_ctx(&mut *guard)?
        };

        let worker_task_id = persisted
            .request_id
            .as_deref()
            .and_then(|r| r.strip_prefix("worker-task:"))
            .map(str::to_owned);

        let mut harness = if let Some(ref tid) = worker_task_id {
            let _ = WorkerTaskStore::mark_status(&data_dir, tid, WorkerTaskStatus::Running);
            if let Some((_, (maker_prof, maker_key))) = resolve_dual_mode_profiles(&data_dir, &json!({ "dualMode": true })) {
                match build_worker_harness(&data_dir, real_app, maker_prof, maker_key, tid.clone()) {
                    Ok(h) => h,
                    Err((_, returned_app)) => {
                        restore_app(state, returned_app, &dummy_dir);
                        return Err("HARNESS_BUILD_FAILED");
                    }
                }
            } else {
                match build_harness(&data_dir, real_app) {
                    Ok(h) => h.with_role(HarnessRole::Worker {
                        data_dir: data_dir.clone(),
                        task_id: tid.clone(),
                    }),
                    Err((_, returned_app)) => {
                        restore_app(state, returned_app, &dummy_dir);
                        return Err("HARNESS_BUILD_FAILED");
                    }
                }
            }
        } else {
            match build_harness(&data_dir, real_app) {
                Ok(h) => h,
                Err((_, returned_app)) => {
                    restore_app(state, returned_app, &dummy_dir);
                    return Err("HARNESS_BUILD_FAILED");
                }
            }
        };
        harness.suspended_turn = Some(persisted.suspended.clone());

        let mut resumed_full_text = String::new();
        let res = harness.resume_after_approval(approval_id, false, &mut |evt| {
            if let JarvisStreamEvent::Done { full_text: ft, .. } = evt {
                resumed_full_text = ft;
            }
        });

        let real_app = harness.app.into_inner().unwrap();
        restore_app(state, real_app, &dummy_dir);

        match res {
            Ok(final_text) => {
                let reply_to_save = if !final_text.is_empty() {
                    final_text
                } else {
                    resumed_full_text
                };
                if let Some(ref exec_id) = persisted.execution_id {
                    if let Ok(mut guard) = state.lock() {
                        let _ = guard.app.finish_direct_execution(
                            exec_id,
                            DirectExecutionOutcome::Completed {
                                text: reply_to_save.clone(),
                                provider_reported_model: None,
                                input_tokens: None,
                                output_tokens: None,
                            },
                            now_millis(),
                        );
                    }
                }
                update_or_save_assistant_reply(
                    &data_dir,
                    &persisted.suspended.conversation_id,
                    persisted.execution_id.as_deref(),
                    &reply_to_save,
                );
                Ok(json!({
                    "ok": true,
                    "approvalId": approval_id,
                    "reply": reply_to_save,
                }))
            }
            Err(_) => Err("RESUME_AFTER_APPROVAL_FAILED"),
        }
    } else {
        if approval_id.starts_with("draft-") {
            let mut draft_store = bob_runtime::routines::UnattendedDraftStore::open(&data_dir)
                .map_err(|_| "DRAFT_STORE_OPEN_FAILED")?;
            draft_store
                .reject_draft(approval_id, now_millis())
                .map_err(|_| "REJECT_DRAFT_FAILED")?;
            return Ok(json!({
                "ok": true,
                "approvalId": approval_id,
                "reply": "",
            }));
        }
        let mut store =
            PendingApprovalStore::open(&data_dir).map_err(|_| "APPROVALS_OPEN_FAILED")?;
        store.reject(approval_id).map_err(|_| "REJECT_FAILED")?;
        Ok(json!({
            "ok": true,
            "approvalId": approval_id,
            "reply": "",
        }))
    }
}

pub fn reject_with_ctx(ctx: &mut NodeContext, args: &Value) -> Result<Value, &'static str> {
    let approval_id = args
        .get("approvalId")
        .or_else(|| args.get("approval_id"))
        .and_then(Value::as_str)
        .ok_or("APPROVAL_ID_REQUIRED")?;
    let data_dir = ctx.runtime.data_dir.clone();

    let mut st = load_suspended_turns(&data_dir);
    if let Some(persisted) = st.turns.remove(approval_id) {
        let _ = save_suspended_turns(&data_dir, &st);

        let (real_app, dummy_dir) = take_app_from_ctx(ctx)?;

        let mut harness = match build_harness(&data_dir, real_app) {
            Ok(h) => h,
            Err((_, returned_app)) => {
                restore_app_to_ctx(ctx, returned_app, &dummy_dir);
                return Err("HARNESS_BUILD_FAILED");
            }
        };
        harness.suspended_turn = Some(persisted.suspended.clone());

        let mut resumed_full_text = String::new();
        let res = harness.resume_after_approval(approval_id, false, &mut |evt| {
            if let JarvisStreamEvent::Done { full_text: ft, .. } = evt {
                resumed_full_text = ft;
            }
        });

        let real_app = harness.app.into_inner().unwrap();
        restore_app_to_ctx(ctx, real_app, &dummy_dir);

        match res {
            Ok(final_text) => {
                let reply_to_save = if !final_text.is_empty() {
                    final_text
                } else {
                    resumed_full_text
                };
                if let Some(ref exec_id) = persisted.execution_id {
                    let _ = ctx.app.finish_direct_execution(
                        exec_id,
                        DirectExecutionOutcome::Completed {
                            text: reply_to_save.clone(),
                            provider_reported_model: None,
                            input_tokens: None,
                            output_tokens: None,
                        },
                        now_millis(),
                    );
                }
                update_or_save_assistant_reply(
                    &data_dir,
                    &persisted.suspended.conversation_id,
                    persisted.execution_id.as_deref(),
                    &reply_to_save,
                );
                Ok(json!({
                    "ok": true,
                    "approvalId": approval_id,
                    "reply": reply_to_save,
                }))
            }
            Err(_) => Err("RESUME_AFTER_APPROVAL_FAILED"),
        }
    } else {
        if approval_id.starts_with("draft-") {
            let mut draft_store = bob_runtime::routines::UnattendedDraftStore::open(&data_dir)
                .map_err(|_| "DRAFT_STORE_OPEN_FAILED")?;
            draft_store
                .reject_draft(approval_id, now_millis())
                .map_err(|_| "REJECT_DRAFT_FAILED")?;
            return Ok(json!({
                "ok": true,
                "approvalId": approval_id,
                "reply": "",
            }));
        }
        let mut store =
            PendingApprovalStore::open(&data_dir).map_err(|_| "APPROVALS_OPEN_FAILED")?;
        store.reject(approval_id).map_err(|_| "REJECT_FAILED")?;
        Ok(json!({
            "ok": true,
            "approvalId": approval_id,
            "reply": "",
        }))
    }
}

pub fn reject_direct(data_dir: &Path, args: &Value) -> Result<Value, &'static str> {
    let approval_id = args
        .get("approvalId")
        .or_else(|| args.get("approval_id"))
        .and_then(Value::as_str)
        .ok_or("APPROVAL_ID_REQUIRED")?;

    let mut st = load_suspended_turns(data_dir);
    if let Some(persisted) = st.turns.remove(approval_id) {
        let _ = save_suspended_turns(data_dir, &st);
        let mut harness =
            build_harness_outside_lock(data_dir).map_err(|_| "HARNESS_BUILD_FAILED")?;
        harness.suspended_turn = Some(persisted.suspended.clone());

        let mut resumed_full_text = String::new();
        let res = harness.resume_after_approval(approval_id, false, &mut |evt| {
            if let JarvisStreamEvent::Done { full_text: ft, .. } = evt {
                resumed_full_text = ft;
            }
        });

        match res {
            Ok(final_text) => {
                let reply_to_save = if !final_text.is_empty() {
                    final_text
                } else {
                    resumed_full_text
                };
                update_or_save_assistant_reply(
                    data_dir,
                    &persisted.suspended.conversation_id,
                    persisted.execution_id.as_deref(),
                    &reply_to_save,
                );
                Ok(json!({
                    "ok": true,
                    "approvalId": approval_id,
                    "reply": reply_to_save,
                }))
            }
            Err(_) => Err("RESUME_AFTER_APPROVAL_FAILED"),
        }
    } else {
        if approval_id.starts_with("draft-") {
            let mut draft_store = bob_runtime::routines::UnattendedDraftStore::open(data_dir)
                .map_err(|_| "DRAFT_STORE_OPEN_FAILED")?;
            draft_store
                .reject_draft(approval_id, now_millis())
                .map_err(|_| "REJECT_DRAFT_FAILED")?;
            return Ok(json!({
                "ok": true,
                "approvalId": approval_id,
                "reply": "",
            }));
        }
        let mut store =
            PendingApprovalStore::open(data_dir).map_err(|_| "APPROVALS_OPEN_FAILED")?;
        store.reject(approval_id).map_err(|_| "REJECT_FAILED")?;
        Ok(json!({
            "ok": true,
            "approvalId": approval_id,
            "reply": "",
        }))
    }
}

pub fn approvals_list(data_dir: &Path, _args: &Value) -> Result<Value, &'static str> {
    let now = now_millis();
    let mut store = PendingApprovalStore::open(data_dir).map_err(|_| "APPROVALS_OPEN_FAILED")?;
    let pending = store.list_pending(now);
    let mut list: Vec<Value> = pending
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "kind": "approval",
                "icon": "shield",
                "title": a.card.title,
                "summary": a.card.summary,
                "riskLevel": risk_to_wire(a.card.risk_level),
                "risk_level": risk_to_wire(a.card.risk_level),
                "fields": a.card.fields,
                "technicalDetail": a.card.technical_detail,
                "technical_detail": a.card.technical_detail,
                "status": "pending",
                "created_at": a.created_at,
                "expires_at": a.expires_at,
            })
        })
        .collect();

    if let Ok(mut draft_store) = bob_runtime::routines::UnattendedDraftStore::open(data_dir) {
        let drafts = draft_store.list_pending(now);
        for d in drafts {
            list.push(json!({
                "id": d.id,
                "kind": "routine_draft",
                "icon": d.card.icon.unwrap_or_else(|| "clock".to_string()),
                "title": d.card.title,
                "summary": d.card.summary,
                "riskLevel": risk_to_wire(d.card.risk_level),
                "risk_level": risk_to_wire(d.card.risk_level),
                "fields": d.card.fields,
                "technicalDetail": d.card.technical_detail,
                "technical_detail": d.card.technical_detail,
                "status": "pending",
                "routine_id": d.routine_id,
                "routine_title": d.routine_title,
                "tool_name": d.tool_name,
                "created_at": d.created_at,
                "expires_at": d.expires_at,
            }));
        }
    }

    Ok(json!(list))
}

pub fn list_approvals(data_dir: &Path, args: &Value) -> Result<Value, &'static str> {
    approvals_list(data_dir, args)
}

pub fn budget_get(data_dir: &Path, _args: &Value) -> Result<Value, &'static str> {
    let mut store = MonthlyBudgetStore::open(data_dir).map_err(|_| "BUDGET_OPEN_FAILED")?;
    let status = store.status();
    Ok(budget_status_to_json(&status))
}

pub fn budget_set(data_dir: &Path, args: &Value) -> Result<Value, &'static str> {
    let cap = args
        .get("monthlyCapUsd")
        .or_else(|| args.get("monthly_cap_usd"))
        .and_then(Value::as_f64)
        .ok_or("MONTHLY_CAP_USD_REQUIRED")?;
    let mut store = MonthlyBudgetStore::open(data_dir).map_err(|_| "BUDGET_OPEN_FAILED")?;
    let status = store.set_cap_usd(cap).map_err(|_| "BUDGET_SET_FAILED")?;
    Ok(budget_status_to_json(&status))
}

// ---------------------------------------------------------------------------
// Unit & Integration Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{spawn_node_server, BobNodeConfig};
    use bob_kernel::direct_provider::DirectProviderProfileInput;
    use bob_runtime::tool_bus::CardField;
    use bob_runtime::SecretStore;
    use std::io::{BufRead, BufReader, Read};
    use std::net::{SocketAddr, TcpListener};

    fn drain_http_request(stream: &mut TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse().unwrap_or(0);
            } else if let Some(rest) = trimmed.strip_prefix("content-length:") {
                content_length = rest.trim().parse().unwrap_or(0);
            }
        }
        if content_length > 0 {
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
        }
    }

    fn http_rpc(
        addr: SocketAddr,
        command: &str,
        args: &Value,
    ) -> (u16, Value) {
        let mut stream = TcpStream::connect(addr).expect("connect to node");
        let payload = serde_json::to_string(args).unwrap();
        let req = format!(
            "POST /api/rpc/{command} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        let (hdr, body) = resp.split_once("\r\n\r\n").unwrap();
        let status: u16 = hdr
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let val: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({ "raw": body }));
        (status, val)
    }

    fn http_custom(
        addr: SocketAddr,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> (u16, Value) {
        let mut stream = TcpStream::connect(addr).expect("connect to node");
        let payload = body.map(|b| serde_json::to_string(b).unwrap()).unwrap_or_default();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        let (hdr, body) = resp.split_once("\r\n\r\n").unwrap();
        let status: u16 = hdr
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let val: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({ "raw": body }));
        (status, val)
    }

    fn http_stream_post(
        addr: SocketAddr,
        body: &Value,
    ) -> (u16, Vec<Value>) {
        let mut stream = TcpStream::connect(addr).expect("connect to node");
        let payload = serde_json::to_string(body).unwrap();
        let req = format!(
            "POST /api/jarvis/v1/stream HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        stream.write_all(req.as_bytes()).unwrap();

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();

        // Drain headers
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                break;
            }
        }

        let mut events = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("data:") {
                let json_str = rest.trim();
                if !json_str.is_empty() {
                    if let Ok(v) = serde_json::from_str::<Value>(json_str) {
                        events.push(v);
                    }
                }
            }
        }
        (status, events)
    }

    #[test]
    fn test_event_to_wire_format_contract() {
        let card = StructuredCard {
            id: "card-1".to_string(),
            kind: "info".to_string(),
            icon: Some("calendar".to_string()),
            title: "Meeting".to_string(),
            summary: "Team Sync".to_string(),
            risk_level: ActionRiskLevel::SideEffect,
            fields: vec![CardField {
                label: "Time".to_string(),
                value: "10:00".to_string(),
            }],
            items: vec![],
            status: Some("pending".to_string()),
            technical_detail: Some("tech info".to_string()),
        };

        // 1. Status event
        let wire_status = event_to_wire_json(&JarvisStreamEvent::Mood { mood: "speaking".into() }, None);
        assert_eq!(wire_status["type"], "status");
        assert_eq!(wire_status["mood"], "speaking");

        // 2. TextDelta event
        let wire_text = event_to_wire_json(&JarvisStreamEvent::TextDelta { delta: "hi".into() }, None);
        assert_eq!(wire_text["type"], "text_delta");
        assert_eq!(wire_text["text"], "hi");
        assert_eq!(wire_text["delta"], "hi");

        // 3. Card event
        let wire_card = event_to_wire_json(&JarvisStreamEvent::Card { card: card.clone() }, None);
        assert_eq!(wire_card["type"], "card");
        assert_eq!(wire_card["card"]["riskLevel"], "side_effect");
        assert_eq!(wire_card["card"]["risk_level"], "side_effect");
        assert_eq!(wire_card["card"]["technicalDetail"], "tech info");
        assert_eq!(wire_card["card"]["technical_detail"], "tech info");

        // 4. ApprovalRequired event
        let approval = PendingApprovalCard {
            id: "appr-1".to_string(),
            tool_name: "schedule_delete".to_string(),
            args: json!({"id": "sched-1"}),
            card: card.clone(),
            created_at: 1000,
            expires_at: 2000,
            status: bob_runtime::tool_bus::ApprovalStatus::Pending,
        };
        let wire_appr = event_to_wire_json(&JarvisStreamEvent::ApprovalRequired { approval }, None);
        assert_eq!(wire_appr["type"], "approval_required");
        assert_eq!(wire_appr["approval"]["riskLevel"], "side_effect");
        assert_eq!(wire_appr["approval"]["tool_name"], "schedule_delete");
        assert_eq!(wire_appr["approval"]["status"], "pending");

        // 5. Done event
        let usage = TurnUsageSummary {
            input_tokens: 15,
            output_tokens: 25,
            cache_tokens: 0,
            total_tokens: 40,
            cost_usd: 0.0001,
        };
        let wire_done = event_to_wire_json(
            &JarvisStreamEvent::Done {
                conversation_id: "conv-1".into(),
                full_text: "Done text".into(),
                usage,
            },
            Some("msg-reply-1"),
        );
        assert_eq!(wire_done["type"], "done");
        assert_eq!(wire_done["conversation_id"], "conv-1");
        assert_eq!(wire_done["message_id"], "msg-reply-1");
        assert_eq!(wire_done["reply"], "Done text");
        assert_eq!(wire_done["usage"]["prompt_tokens"], 15);
        assert_eq!(wire_done["usage"]["completion_tokens"], 25);
    }

    #[test]
    fn test_real_sse_streaming_e2e_with_mock_llm() {
        let dir = tempfile::tempdir().unwrap();

        // 1. Mock OpenAI SSE HTTP Server
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mock_port = listener.local_addr().unwrap().port();

        let mock_server = std::thread::spawn(move || {
            // Step 1: Tool call show_card
            let (mut s1, _) = listener.accept().unwrap();
            drain_http_request(&mut s1);

            let sse_step1 = concat!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                "data: {\"choices\": [{\"delta\": {\"tool_calls\": [{\"index\": 0, \"id\": \"call_card_1\", \"type\": \"function\", \"function\": {\"name\": \"show_card\", \"arguments\": \"{\\\"title\\\": \\\"Weekly Plan\\\", \\\"summary\\\": \\\"3 items\\\"}\"}}]}}]}\n\n",
                "data: {\"usage\": {\"prompt_tokens\": 30, \"completion_tokens\": 20}}\n\n",
                "data: [DONE]\n\n"
            );
            s1.write_all(sse_step1.as_bytes()).unwrap();
            s1.flush().unwrap();
            let _ = s1.shutdown(std::net::Shutdown::Write);
            let mut discard1 = Vec::new();
            let _ = s1.read_to_end(&mut discard1);

            // Step 2: Final text stream
            let (mut s2, _) = listener.accept().unwrap();
            drain_http_request(&mut s2);

            let sse_step2 = concat!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                "data: {\"choices\": [{\"delta\": {\"content\": \"Here is \"}}]}\n\n",
                "data: {\"choices\": [{\"delta\": {\"content\": \"your card!\"}}]}\n\n",
                "data: {\"usage\": {\"prompt_tokens\": 50, \"completion_tokens\": 10}}\n\n",
                "data: [DONE]\n\n"
            );
            s2.write_all(sse_step2.as_bytes()).unwrap();
            s2.flush().unwrap();
            let _ = s2.shutdown(std::net::Shutdown::Write);
            let mut discard2 = Vec::new();
            let _ = s2.read_to_end(&mut discard2);
        });

        // 2. Pre-configure node profile
        {
            let db_path = dir.path().join("kernel.sqlite");
            let mut app = LocalApp::open(&db_path).unwrap();
            app.upsert_direct_provider_profile(
                &DirectProviderProfileInput {
                    id: "default".to_owned(),
                    provider_kind: "openai_compatible".to_owned(),
                    label: "Mock Provider".to_owned(),
                    base_url: format!("http://127.0.0.1:{mock_port}"),
                    model_id: "deepseek-chat".to_owned(),
                    api_key_slot: "slot1".to_owned(),
                    api_key_configured: true,
                    enabled: true,
                },
                crate::now_millis(),
            )
            .unwrap();

            let vault = bob_runtime::FileVaultSecretStore::open(dir.path()).unwrap();
            let _ = vault.set("slot1", "test-secret-key");
        }

        // 3. Spawn Node Server
        let server = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        // 4. POST /api/jarvis/v1/stream
        let stream_req = json!({
            "sessionId": "conv-jarvis-stream",
            "content": "Show my weekly plan",
        });
        let (status, events) = http_stream_post(server.local_addr, &stream_req);
        assert_eq!(status, 200);

        mock_server.join().unwrap();

        // 5. Verify events in order
        assert!(events.iter().any(|e| e["type"] == "status" && e["mood"] == "thinking"));
        assert!(events.iter().any(|e| e["type"] == "tool_start" && e["name"] == "show_card"));
        assert!(events.iter().any(|e| e["type"] == "tool_end" && e["name"] == "show_card"));
        assert!(events.iter().any(|e| e["type"] == "card" && e["card"]["title"] == "Weekly Plan"));
        assert!(events.iter().any(|e| e["type"] == "status" && e["mood"] == "speaking"));

        let full_streamed: String = events
            .iter()
            .filter(|e| e["type"] == "text_delta")
            .filter_map(|e| e["delta"].as_str())
            .collect();
        assert_eq!(full_streamed, "Here is your card!");

        let done_event = events.iter().find(|e| e["type"] == "done").expect("done event");
        assert_eq!(done_event["reply"], "Here is your card!");
        assert_eq!(done_event["conversation_id"], "conv-jarvis-stream");

        // 6. Verify history in LocalApp contains the assistant reply
        let (hist_status, hist) = http_rpc(
            server.local_addr,
            "app_history",
            &json!({ "sessionId": "conv-jarvis-stream" }),
        );
        assert_eq!(hist_status, 200);
        let msgs = hist["result"].as_array().expect("array of messages");
        assert!(msgs.iter().any(|m| m["role"] == "assistant" && m["content"].as_str().unwrap().contains("Here is your card!")));
    }

    #[test]
    fn test_graded_action_gate_e2e_approval_and_resumption() {
        let dir = tempfile::tempdir().unwrap();

        // Mock server: Step 1 emits schedule_delete, Step 2 acknowledges deletion
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mock_port = listener.local_addr().unwrap().port();

        let mock_server = std::thread::spawn(move || {
            // Turn 1: schedule_delete call
            let (mut s1, _) = listener.accept().unwrap();
            drain_http_request(&mut s1);

            let sse_step1 = concat!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                "data: {\"choices\": [{\"delta\": {\"tool_calls\": [{\"index\": 0, \"id\": \"call_del\", \"type\": \"function\", \"function\": {\"name\": \"schedule_delete\", \"arguments\": \"{\\\"id\\\": \\\"sched-to-del\\\"}\"}}]}}]}\n\n",
                "data: {\"usage\": {\"prompt_tokens\": 30, \"completion_tokens\": 20}}\n\n",
                "data: [DONE]\n\n"
            );
            s1.write_all(sse_step1.as_bytes()).unwrap();
            s1.flush().unwrap();
            let _ = s1.shutdown(std::net::Shutdown::Write);
            let mut discard1 = Vec::new();
            let _ = s1.read_to_end(&mut discard1);

            // Turn 2 (Resumed after approval): Ack message
            let (mut s2, _) = listener.accept().unwrap();
            drain_http_request(&mut s2);

            let sse_step2 = concat!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                "data: {\"choices\": [{\"delta\": {\"content\": \"Schedule item deleted successfully.\"}}]}\n\n",
                "data: {\"usage\": {\"prompt_tokens\": 40, \"completion_tokens\": 10}}\n\n",
                "data: [DONE]\n\n"
            );
            s2.write_all(sse_step2.as_bytes()).unwrap();
            s2.flush().unwrap();
            let _ = s2.shutdown(std::net::Shutdown::Write);
            let mut discard2 = Vec::new();
            let _ = s2.read_to_end(&mut discard2);
        });

        // Setup provider & a schedule item in LocalApp
        {
            let db_path = dir.path().join("kernel.sqlite");
            let mut app = LocalApp::open(&db_path).unwrap();
            app.upsert_direct_provider_profile(
                &DirectProviderProfileInput {
                    id: "default".to_owned(),
                    provider_kind: "openai_compatible".to_owned(),
                    label: "Mock".to_owned(),
                    base_url: format!("http://127.0.0.1:{mock_port}"),
                    model_id: "deepseek-chat".to_owned(),
                    api_key_slot: "slot1".to_owned(),
                    api_key_configured: true,
                    enabled: true,
                },
                crate::now_millis(),
            )
            .unwrap();

            let vault = bob_runtime::FileVaultSecretStore::open(dir.path()).unwrap();
            let _ = vault.set("slot1", "secret-test");

            // Create schedule item in LocalApp
            app.create_schedule_item_with_kind(
                "sched-to-del",
                None,
                None,
                "Meeting to Delete",
                "event",
                1700000000000i64,
                None,
                None,
                "",
                crate::now_millis(),
            )
            .unwrap();
        }

        let server = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        // 1. Send stream request that requires approval
        let (status, events) = http_stream_post(
            server.local_addr,
            &json!({
                "sessionId": "conv-gate-test",
                "content": "Please delete schedule item sched-to-del",
            }),
        );
        assert_eq!(status, 200);

        let appr_event = events
            .iter()
            .find(|e| e["type"] == "approval_required")
            .expect("must emit approval_required");
        let approval_id = appr_event["approval"]["id"].as_str().unwrap().to_owned();
        assert_eq!(appr_event["approval"]["tool_name"], "schedule_delete");
        assert_eq!(appr_event["approval"]["riskLevel"], "side_effect");

        // 2. Query app_jarvis_approvals_list
        let (st, appr_list) = http_rpc(server.local_addr, "app_jarvis_approvals_list", &json!({}));
        assert_eq!(st, 200);
        let list = appr_list["result"].as_array().expect("result array");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], approval_id);

        // 3. Approve via app_jarvis_approve
        let (appr_st, appr_res) = http_rpc(
            server.local_addr,
            "app_jarvis_approve",
            &json!({ "approvalId": approval_id }),
        );
        assert_eq!(appr_st, 200);
        assert_eq!(appr_res["ok"], true);
        assert_eq!(appr_res["result"]["reply"], "Schedule item deleted successfully.");

        mock_server.join().unwrap();

        // 4. Verify approvals list is now empty
        let (_, empty_list) = http_rpc(server.local_addr, "app_jarvis_approvals_list", &json!({}));
        assert_eq!(empty_list["result"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_monthly_budget_rpc_and_enforcement() {
        let dir = tempfile::tempdir().unwrap();

        let server = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        // 1. Get initial budget (default $10.00)
        let (status, res) = http_rpc(server.local_addr, "app_jarvis_budget_get", &json!({}));
        assert_eq!(status, 200);
        let initial_cap = res["result"]["monthly_cap_usd"].as_f64().unwrap();
        assert_eq!(initial_cap, 10.0);

        // 2. Set cap to $25.50
        let (status, res) = http_rpc(
            server.local_addr,
            "app_jarvis_budget_set",
            &json!({ "monthlyCapUsd": 25.5 }),
        );
        assert_eq!(status, 200);
        assert_eq!(res["result"]["monthly_cap_usd"], 25.5);

        // 3. Verify get reflects updated cap
        let (status, res) = http_rpc(server.local_addr, "app_jarvis_budget_get", &json!({}));
        assert_eq!(status, 200);
        assert_eq!(res["result"]["monthly_cap_usd"], 25.5);

        // 4. Set cap to $0.00 to simulate hard budget block
        let (status, res) = http_rpc(
            server.local_addr,
            "app_jarvis_budget_set",
            &json!({ "monthlyCapUsd": 0.0 }),
        );
        assert_eq!(status, 200);
        assert_eq!(res["result"]["cap_reached"], true);

        // 5. Attempting to stream with cap = 0.0 blocks with MONTHLY_BUDGET_EXCEEDED error event
        let (status, events) = http_stream_post(
            server.local_addr,
            &json!({
                "sessionId": "conv-budget-blocked",
                "content": "This should be blocked by budget cap",
            }),
        );
        assert_eq!(status, 200);
        let err_evt = events
            .iter()
            .find(|e| e["type"] == "error")
            .expect("must emit error event");
        assert_eq!(err_evt["code"], "MONTHLY_BUDGET_EXCEEDED");
    }

    #[test]
    fn test_feature_package_sandbox_rpcs_and_http_endpoints_e2e() {
        let dir = tempfile::tempdir().unwrap();

        // 1. Seed feature package directly in FeatureStore
        {
            let mut store = bob_runtime::features::FeatureStore::open(dir.path()).unwrap();
            let mut routes = std::collections::HashMap::new();
            routes.insert(
                "tick".to_string(),
                bob_runtime::features::FeatureRouteSpec::StoragePatch {
                    patch: Some(json!({ "today_count": 1 })),
                },
            );
            routes.insert(
                "info".to_string(),
                bob_runtime::features::FeatureRouteSpec::Static {
                    template: json!({ "app": "pomodoro", "ok": true }),
                    cache_result: Some(true),
                },
            );
            let pkg = store
                .create_or_update(
                    "pomodoro",
                    "Pomodoro Timer",
                    "Focus timer widget",
                    Some("<div>Pomodoro</div>".to_string()),
                    routes,
                    vec![],
                    Some(json!({ "today_count": 0 })),
                    crate::now_millis(),
                )
                .unwrap();
            assert_eq!(pkg.status, "trial");
            assert!(pkg.trial_expires_at.is_some());
        }

        let server = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        // 2. Query app_features_list -> MUST return JSON array [...]
        let (status, list_res) = http_rpc(server.local_addr, "app_features_list", &json!({}));
        assert_eq!(status, 200);
        let list = list_res["result"].as_array().expect("result must be array");
        assert_eq!(list.len(), 1);
        let pom = &list[0];
        assert_eq!(pom["name"], "pomodoro");
        assert_eq!(pom["title"], "Pomodoro Timer");
        assert_eq!(pom["status"], "trial");
        assert_eq!(pom["has_widget"], true);
        assert_eq!(pom["widget_html"], "<div>Pomodoro</div>");
        assert_eq!(pom["storage"]["today_count"], 0);

        // 3. Keep trial -> active
        let (status, keep_res) = http_rpc(
            server.local_addr,
            "app_feature_keep",
            &json!({ "name": "pomodoro" }),
        );
        assert_eq!(status, 200);
        assert_eq!(keep_res["result"]["status"], "active");
        assert!(keep_res["result"]["trial_expires_at"].is_null());

        // 4. Storage quota save
        // 4a. Valid save <= 8192 bytes
        let (status, save_res) = http_rpc(
            server.local_addr,
            "app_feature_storage_save",
            &json!({ "name": "pomodoro", "storage": { "today_count": 42 } }),
        );
        assert_eq!(status, 200);
        assert_eq!(save_res["result"]["ok"], true);
        assert_eq!(save_res["result"]["storage"]["today_count"], 42);

        // 4b. Oversized save > 8192 bytes -> error FEATURE_STORAGE_LIMIT_EXCEEDED (HTTP 400)
        let huge = "a".repeat(9000);
        let (status, huge_res) = http_rpc(
            server.local_addr,
            "app_feature_storage_save",
            &json!({ "name": "pomodoro", "storage": { "blob": huge } }),
        );
        assert_eq!(status, 400);
        assert_eq!(huge_res["ok"], false);
        assert_eq!(huge_res["error"], "FEATURE_STORAGE_LIMIT_EXCEEDED");

        // 5. Test direct HTTP endpoints: POST and GET /api/f/pomodoro/*
        let (status, static_val) = http_custom(
            server.local_addr,
            "GET",
            "/api/f/pomodoro/info",
            None,
        );
        assert_eq!(status, 200);
        assert_eq!(static_val["ok"], true);
        assert_eq!(static_val["data"]["app"], "pomodoro");

        let (status, patch_val) = http_custom(
            server.local_addr,
            "POST",
            "/api/f/pomodoro/tick",
            Some(&json!({})),
        );
        assert_eq!(status, 200);
        assert_eq!(patch_val["ok"], true);
        assert_eq!(patch_val["cached_state"]["today_count"], 1);

        // 6. Test app_feature_call RPC
        let (status, call_res) = http_rpc(
            server.local_addr,
            "app_feature_call",
            &json!({ "name": "pomodoro", "route": "info", "payload": {} }),
        );
        assert_eq!(status, 200);
        assert_eq!(call_res["result"]["ok"], true);
        assert_eq!(call_res["result"]["data"]["app"], "pomodoro");

        // 7. Toggle feature
        let (status, toggle_res) = http_rpc(
            server.local_addr,
            "app_feature_toggle",
            &json!({ "name": "pomodoro", "enabled": false }),
        );
        assert_eq!(status, 200);
        assert_eq!(toggle_res["result"]["enabled"], false);
        assert_eq!(toggle_res["result"]["status"], "disabled");

        // Calling disabled feature via HTTP returns 403
        let (status, dis_val) = http_custom(
            server.local_addr,
            "GET",
            "/api/f/pomodoro/info",
            None,
        );
        assert_eq!(status, 403);
        assert_eq!(dis_val["error"], "FEATURE_DISABLED");

        // Re-enable
        let (status, toggle_res2) = http_rpc(
            server.local_addr,
            "app_feature_toggle",
            &json!({ "name": "pomodoro", "enabled": true }),
        );
        assert_eq!(status, 200);
        assert_eq!(toggle_res2["result"]["enabled"], true);
        assert_eq!(toggle_res2["result"]["status"], "active");

        // 8. Rollback feature
        let (status, roll_res) = http_rpc(
            server.local_addr,
            "app_feature_rollback",
            &json!({ "name": "pomodoro" }),
        );
        assert_eq!(status, 200);
        assert_eq!(roll_res["result"]["rolled_back"], true);

        // Feature was v1 with no prior version, so rolling back removed it
        let (_, final_list) = http_rpc(server.local_addr, "app_features_list", &json!({}));
        let list_arr = final_list["result"].as_array().unwrap();
        assert_eq!(list_arr.len(), 0);
    }

    #[test]
    fn test_wp_e1_2_dual_mode_clerk_worker_thread_subscription_batching_and_ledger_recovery() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let llm_addr = listener.local_addr().unwrap();
        let clerk_calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = Arc::new(AtomicUsize::new(0));
        let captured_worker_bodies: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));

        let clerk_calls_srv = clerk_calls.clone();
        let worker_calls_srv = worker_calls.clone();
        let worker_bodies_srv = captured_worker_bodies.clone();

        std::thread::spawn(move || {
            for _ in 0..4 {
                let Ok((mut sock, _)) = listener.accept() else {
                    break;
                };
                let mut reader = BufReader::new(sock.try_clone().unwrap());
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some((k, v)) = trimmed.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("content-length") {
                            content_length = v.trim().parse().unwrap_or(0);
                        }
                    }
                }
                let mut body_bytes = vec![0u8; content_length];
                if content_length > 0 {
                    let _ = reader.read_exact(&mut body_bytes);
                }
                let req_json: Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|_| json!({}));
                let model = req_json["model"].as_str().unwrap_or("").to_owned();

                let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
                let _ = sock.write_all(sse_headers.as_bytes());

                if model == "deepseek-v4.1-flash-1017" {
                    let idx = clerk_calls_srv.fetch_add(1, Ordering::SeqCst);
                    if idx == 0 {
                        let chunk1 = json!({
                            "choices": [{
                                "delta": {
                                    "tool_calls": [{
                                        "index": 0,
                                        "id": "call_dispatch_1",
                                        "type": "function",
                                        "function": {
                                            "name": "dispatch_background_task",
                                            "arguments": "{\"title\":\"Weekly Ops Digest\",\"objective\":\"Compile notes and build weekly digest\",\"context_summary\":\"User requested deep weekly ops review\"}"
                                        }
                                    }]
                                },
                                "finish_reason": "tool_calls"
                            }],
                            "usage": { "prompt_tokens": 30, "completion_tokens": 20, "total_tokens": 50 }
                        });
                        let _ = sock.write_all(format!("data: {chunk1}\n\ndata: [DONE]\n\n").as_bytes());
                    } else {
                        let chunk2 = json!({
                            "choices": [{
                                "delta": { "content": "Got it — started the weekly ops digest in the background. Feel free to keep chatting." },
                                "finish_reason": "stop"
                            }],
                            "usage": { "prompt_tokens": 45, "completion_tokens": 18, "total_tokens": 63 }
                        });
                        let _ = sock.write_all(format!("data: {chunk2}\n\ndata: [DONE]\n\n").as_bytes());
                    }
                } else if model == "deepseek-v4-pro" {
                    let idx = worker_calls_srv.fetch_add(1, Ordering::SeqCst);
                    worker_bodies_srv.lock().unwrap().push(req_json);
                    if idx == 0 {
                        // Hold step 1 briefly so mid-flight follow-up messages arrive while Worker is active
                        std::thread::sleep(Duration::from_millis(350));
                        let wchunk1 = json!({
                            "choices": [{
                                "delta": {
                                    "reasoning_content": "Inspecting schedules and notes first.",
                                    "tool_calls": [{
                                        "index": 0,
                                        "id": "call_worker_note_1",
                                        "type": "function",
                                        "function": {
                                            "name": "note_save",
                                            "arguments": "{\"title\":\"Weekly Ops Draft\",\"body\":\"Initial structured ops summary\"}"
                                        }
                                    }]
                                },
                                "finish_reason": "tool_calls"
                            }],
                            "usage": { "prompt_tokens": 80, "completion_tokens": 35, "total_tokens": 115 }
                        });
                        let _ = sock.write_all(format!("data: {wchunk1}\n\ndata: [DONE]\n\n").as_bytes());
                    } else {
                        let wchunk2 = json!({
                            "choices": [{
                                "delta": {
                                    "content": "Weekly Ops Digest completed with urgent deadlines highlighted and compact mobile layout."
                                },
                                "finish_reason": "stop"
                            }],
                            "usage": { "prompt_tokens": 120, "completion_tokens": 28, "total_tokens": 148 }
                        });
                        let _ = sock.write_all(format!("data: {wchunk2}\n\ndata: [DONE]\n\n").as_bytes());
                    }
                }
                let _ = sock.flush();
            }
        });

        let dir = tempfile::tempdir().unwrap();
        {
            let mut app = LocalApp::open(&dir.path().join("kernel.sqlite")).unwrap();
            let runtime = ModelHubRuntime::with_file_vault(dir.path().to_path_buf()).unwrap();
            runtime
                .apply_provider_dual_preset_with_base_url(
                    &mut app,
                    "deepseek",
                    Some("sk-dual-test-key"),
                    Some(&format!("http://{llm_addr}/v1")),
                    None,
                    None,
                )
                .unwrap();
        }

        let server = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        // 1. Send complex request -> Clerk responds in <1s and dispatches background Worker
        let t0 = Instant::now();
        let (status, sse_events) = http_stream_post(
            server.local_addr,
            &json!({
                "sessionId": "conv-dual-e2e",
                "content": "Please compile all my notes into a deep weekly ops digest."
            }),
        );
        let clerk_elapsed = t0.elapsed();
        assert_eq!(status, 200);
        assert!(
            clerk_elapsed < Duration::from_millis(1000),
            "Clerk fast path must complete under 1s, took {clerk_elapsed:?}"
        );

        let dispatched_evt = sse_events
            .iter()
            .find(|e| e["type"] == "task_dispatched")
            .expect("Clerk SSE stream must emit task_dispatched");
        let task_id = dispatched_evt["taskId"].as_str().unwrap().to_owned();
        assert_eq!(dispatched_evt["makerModel"], "deepseek-v4-pro");
        assert!(sse_events.iter().any(|e| e["type"] == "done"));
        assert_eq!(clerk_calls.load(Ordering::SeqCst), 2);

        // 2. While Worker is running (350ms window), send TWO follow-up messages in the same session.
        // Verify Thread direct subscription: instant acknowledgement + followup_batched, ZERO new Clerk LLM calls!
        let (f1_status, f1_events) = http_stream_post(
            server.local_addr,
            &json!({
                "sessionId": "conv-dual-e2e",
                "content": "Also highlight urgent Friday deadlines."
            }),
        );
        assert_eq!(f1_status, 200);
        assert!(f1_events.iter().any(|e| e["type"] == "followup_batched" && e["taskId"] == task_id));

        let (f2_status, f2_events) = http_stream_post(
            server.local_addr,
            &json!({
                "sessionId": "conv-dual-e2e",
                "content": "Keep the summary compact for mobile screens."
            }),
        );
        assert_eq!(f2_status, 200);
        let f2_batch = f2_events
            .iter()
            .find(|e| e["type"] == "followup_batched")
            .expect("second follow-up must also batch onto active worker");
        assert_eq!(f2_batch["queuedCount"], 2);
        assert_eq!(
            clerk_calls.load(Ordering::SeqCst),
            2,
            "Thread direct subscription must bypass Clerk LLM for active conversation worker"
        );

        // 3. Disconnect recovery: poll app_jarvis_worker_tasks until Worker completes
        let wait_start = Instant::now();
        let mut completed_task: Option<Value> = None;
        while wait_start.elapsed() < Duration::from_secs(5) {
            let (st_code, tasks_res) = http_rpc(
                server.local_addr,
                "app_jarvis_worker_tasks",
                &json!({ "sessionId": "conv-dual-e2e", "afterSeq": 0 }),
            );
            assert_eq!(st_code, 200);
            if let Some(arr) = tasks_res["result"].as_array() {
                if let Some(t) = arr.iter().find(|t| t["task_id"] == task_id) {
                    if t["status"] == "completed" {
                        completed_task = Some(t.clone());
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        let task_val = completed_task.expect("Background Worker must reach completed status");
        assert_eq!(task_val["clerk_model"], "deepseek-v4.1-flash-1017");
        assert_eq!(task_val["maker_model"], "deepseek-v4-pro");
        let step_kinds: Vec<&str> = task_val["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["kind"].as_str())
            .collect();
        assert!(step_kinds.contains(&"dispatched"));
        assert!(step_kinds.contains(&"tool_start"));
        assert!(step_kinds.contains(&"tool_end"));
        assert!(step_kinds.contains(&"followup_batched"));
        assert!(step_kinds.contains(&"completed"));

        // Verify both follow-up messages were coalesced into Worker's second LLM call
        let bodies = captured_worker_bodies.lock().unwrap();
        assert_eq!(bodies.len(), 2);
        let second_req_str = serde_json::to_string(&bodies[1]).unwrap();
        assert!(second_req_str.contains("Also highlight urgent Friday deadlines."));
        assert!(second_req_str.contains("Keep the summary compact for mobile screens."));

        // Verify final Worker assistant deliverable was persisted to LocalApp conversation history
        let (hist_code, hist_res) = http_rpc(
            server.local_addr,
            "app_history",
            &json!({ "sessionId": "conv-dual-e2e" }),
        );
        assert_eq!(hist_code, 200);
        let hist_str = serde_json::to_string(&hist_res["result"]).unwrap();
        assert!(hist_str.contains("Weekly Ops Digest completed with urgent deadlines highlighted"));

        server.shutdown();
    }
}
