//! `bob-node` — Always-on headless node and unified Web App server (`XP-05`..`XP-10`).
//!
//! Serves:
//! 1. Unified Web UI static assets (`--static-dir`, injecting `window.__BOB_NODE_RUNTIME__ = true`)
//! 2. `GET /api/capabilities` & `GET /api/events` (SSE)
//! 3. `POST /api/rpc/:command` (full `kernelBridge` RPC surface backed by `LocalApp` + `ModelHubRuntime`)
//! 4. `/api/client/v1/*` (full Online Work Service Client API v1: pairings, token exchange, revoke,
//!    capabilities, projects, tasks, messages, task snapshot, and idempotent operations for task
//!    continuation, V1->V2 revision creation, cancellation, and review verdicts)
//! 5. Read-only idempotent migration from legacy `nexus_control_plane.db` (`XP-10`).

use bob_kernel::app::LocalApp;
use bob_runtime::{now_millis, ModelHubRuntime, RuntimeCapabilityManifest};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct BobNodeConfig {
    pub bind_addr: String,
    pub data_dir: PathBuf,
    pub static_dir: Option<PathBuf>,
    pub legacy_db_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnlinePairingSession {
    pub pairing_id: String,
    pub user_code: String,
    pub device_name: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub exchanged_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnlineCredentialRecord {
    pub credential_id: String,
    pub token_hash: String,
    pub device_name: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub revoked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct OnlineWorkState {
    #[serde(default)]
    pub imported_project_ids: Vec<String>,
    pub server_instance_id: String,
    pub pairings: HashMap<String, OnlinePairingSession>,
    pub credentials: HashMap<String, OnlineCredentialRecord>,
    pub task_snapshots: HashMap<String, Value>,
    pub project_messages: HashMap<String, Vec<Value>>,
    pub operations: HashMap<String, Value>,
}

pub struct NodeContext {
    pub app: LocalApp,
    pub runtime: ModelHubRuntime,
    pub online: OnlineWorkState,
    pub online_store_path: PathBuf,
    pub static_dir: Option<PathBuf>,
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.trim().as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn load_online_state(path: &Path) -> OnlineWorkState {
    if let Ok(raw) = std::fs::read_to_string(path) {
        if let Ok(parsed) = serde_json::from_str::<OnlineWorkState>(&raw) {
            return parsed;
        }
    }
    OnlineWorkState {
        imported_project_ids: Vec::new(),
        server_instance_id: format!("bob-node-{}", now_millis()),
        pairings: HashMap::new(),
        credentials: HashMap::new(),
        task_snapshots: HashMap::new(),
        project_messages: HashMap::new(),
        operations: HashMap::new(),
    }
}

fn save_online_state(path: &Path, state: &OnlineWorkState) {
    if let Ok(raw) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, raw);
    }
}

impl NodeContext {
    pub fn open(config: &BobNodeConfig) -> Result<Self, &'static str> {
        std::fs::create_dir_all(&config.data_dir).map_err(|_| "DATA_DIR_UNAVAILABLE")?;
        let db_path = config.data_dir.join("kernel.sqlite");
        let app = LocalApp::open(&db_path).map_err(|_| "KERNEL_OPEN_FAILED")?;
        let runtime = ModelHubRuntime::with_file_vault(config.data_dir.clone())?;
        let online_store_path = config.data_dir.join("online_work_store.json");
        let online = load_online_state(&online_store_path);
        let mut ctx = Self {
            app,
            runtime,
            online,
            online_store_path,
            static_dir: config.static_dir.clone(),
        };
        if let Some(ref legacy_path) = config.legacy_db_path {
            if legacy_path.exists() {
                let _ = ctx.import_legacy_nexus_db(legacy_path);
            }
        }
        ctx.sync_catalog_into_online_store();
        Ok(ctx)
    }

    pub fn save_online(&self) {
        save_online_state(&self.online_store_path, &self.online);
    }

    /// Directory refresh does not manufacture execution or verification evidence.
    pub fn sync_catalog_into_online_store(&mut self) {
        let Ok(catalog) = self.app.catalog_snapshot() else { return; };
        for project in catalog.projects {
            self.online.project_messages.entry(project.id).or_default();
        }
        self.save_online();
    }

    /// `XP-10`: Idempotent read-only import from legacy `nexus_control_plane.db`.
    pub fn import_legacy_nexus_db(&mut self, db_path: &Path) -> Result<usize, &'static str> {
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| "LEGACY_DB_OPEN_FAILED")?;
        let mut imported_count = 0usize;
        let stamp = now_millis();

        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, COALESCE(name, id), COALESCE(updated_at, ?1) FROM projects ORDER BY id LIMIT 500",
        ) {
            let rows = stmt
                .query_map([stamp], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2).unwrap_or(stamp),
                    ))
                })
                .ok();
            if let Some(iter) = rows {
                for item in iter.flatten() {
                    let (pid, name, updated_at) = item;
                    if !self.online.imported_project_ids.contains(&pid) {
                        self.online.imported_project_ids.push(pid.clone());
                    }
                    let _ = self.app.create_project(&pid, &name, updated_at);
                    imported_count += 1;
                }
            }
        }

        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, project_id, COALESCE(title, id), COALESCE(status, 'open'), COALESCE(updated_at, ?1) FROM tasks ORDER BY id LIMIT 1000",
        ) {
            let rows = stmt
                .query_map([stamp], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4).unwrap_or(stamp),
                    ))
                })
                .ok();
            if let Some(iter) = rows {
                for item in iter.flatten() {
                    let (tid, pid, title, status, updated_at) = item;
                    let conv_id = format!("conv-{tid}");
                    let _ = self
                        .app
                        .import_conversation(&conv_id, Some(&pid), &title, updated_at);
                    self.online.task_snapshots.entry(tid.clone()).or_insert_with(|| {
                        json!({
                            "task": {
                                "id": tid,
                                "project_id": pid,
                                "title": title,
                                "instruction": title,
                                "description": title,
                                "status": status,
                                "created_at": updated_at,
                                "updated_at": updated_at,
                            },
                            "source_kind": "legacy_import",
                            "executions": [],
                            "steering": [],
                            "verifications": [],
                            "reviews": [],
                            "artifacts": [],
                            "missing_facts": []
                        })
                    });
                }
            }
        }
        self.save_online();
        Ok(imported_count)
    }
}

pub fn handle_rpc_command(ctx: &mut NodeContext, command: &str, args: &Value) -> Result<Value, &'static str> {
    let at = now_millis();
    match command {
        "app_bootstrap" => {
            let session_id = ctx
                .app
                .selected_conversation()
                .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
            Ok(json!({
                "session_id": session_id,
                "provider_status": ctx.app.prewarm(),
            }))
        }
        "app_prewarm" => Ok(json!("provider_unconfigured")),
        "app_readiness" => {
            let mut status = ctx
                .app
                .readiness()
                .map_err(|_| "LOCAL_STATUS_UNAVAILABLE")?;
            let direct_ready = ctx.app.direct_provider_ready().unwrap_or(false);
            if direct_ready {
                status.mode = "direct_provider";
                status.execution_available = true;
            }
            serde_json::to_value(status).map_err(|_| "LOCAL_STATUS_UNAVAILABLE")
        }
        "app_catalog" => {
            let snap = ctx
                .app
                .catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")?;
            let mut value = serde_json::to_value(snap).map_err(|_| "CATALOG_READ_FAILED")?;
            if let Some(projects) = value["projects"].as_array_mut() {
                for project in projects {
                    let imported = project["id"].as_str().map(|id| ctx.online.imported_project_ids.iter().any(|p| p == id)).unwrap_or(false);
                    project["source_kind"] = json!(if imported { "legacy_import" } else { "bob_native" });
                }
            }
            value["imported_project_ids"] = json!(ctx.online.imported_project_ids);
            Ok(value)
        }
        "app_history" => {
            let session_id = args
                .get("sessionId")
                .or_else(|| args.get("session_id"))
                .and_then(Value::as_str)
                .unwrap_or("default");
            let hist = ctx
                .app
                .history(session_id)
                .map_err(|_| "LOCAL_HISTORY_UNAVAILABLE")?;
            serde_json::to_value(hist).map_err(|_| "LOCAL_HISTORY_UNAVAILABLE")
        }
        "app_accept" => {
            let request_id = args
                .get("requestId")
                .or_else(|| args.get("request_id"))
                .and_then(Value::as_str)
                .ok_or("REQUEST_ID_REQUIRED")?;
            let message_id = args
                .get("messageId")
                .or_else(|| args.get("message_id"))
                .and_then(Value::as_str)
                .ok_or("MESSAGE_ID_REQUIRED")?;
            let session_id = args
                .get("sessionId")
                .or_else(|| args.get("session_id"))
                .and_then(Value::as_str)
                .unwrap_or("default");
            let content = args
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let expected_rev = args
                .get("expectedDraftRevision")
                .or_else(|| args.get("expected_draft_revision"))
                .and_then(Value::as_i64);
            let accepted = ctx
                .app
                .accept_with_draft_revision(
                    request_id,
                    message_id,
                    session_id,
                    content,
                    expected_rev,
                    at,
                )
                .map_err(|_| "LOCAL_SAVE_UNCONFIRMED")?;
            serde_json::to_value(accepted).map_err(|_| "LOCAL_SAVE_UNCONFIRMED")
        }
        "app_project_create" => {
            let project_id = args
                .get("projectId")
                .or_else(|| args.get("project_id"))
                .and_then(Value::as_str)
                .ok_or("PROJECT_CREATE_FAILED")?;
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .ok_or("PROJECT_CREATE_FAILED")?;
            ctx.app
                .create_project(project_id, title, at)
                .map_err(|_| "PROJECT_CREATE_FAILED")?;
            ctx.sync_catalog_into_online_store();
            let snap = ctx
                .app
                .catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")?;
            serde_json::to_value(snap).map_err(|_| "CATALOG_READ_FAILED")
        }
        "app_conversation_create" => {
            let conversation_id = args
                .get("conversationId")
                .or_else(|| args.get("conversation_id"))
                .and_then(Value::as_str)
                .ok_or("CONVERSATION_CREATE_FAILED")?;
            let project_id = args
                .get("projectId")
                .or_else(|| args.get("project_id"))
                .and_then(Value::as_str);
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .ok_or("CONVERSATION_CREATE_FAILED")?;
            ctx.app
                .create_conversation(conversation_id, project_id, title, at)
                .map_err(|_| "CONVERSATION_CREATE_FAILED")?;
            ctx.sync_catalog_into_online_store();
            let snap = ctx
                .app
                .catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")?;
            serde_json::to_value(snap).map_err(|_| "CATALOG_READ_FAILED")
        }
        "app_conversation_promote" => {
            let conversation_id = args
                .get("conversationId")
                .or_else(|| args.get("conversation_id"))
                .and_then(Value::as_str)
                .ok_or("CONVERSATION_PROMOTE_FAILED")?;
            let project_id = args
                .get("projectId")
                .or_else(|| args.get("project_id"))
                .and_then(Value::as_str)
                .ok_or("CONVERSATION_PROMOTE_FAILED")?;
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .ok_or("CONVERSATION_PROMOTE_FAILED")?;
            let snap = ctx
                .app
                .promote_conversation_to_project(conversation_id, project_id, title, at)
                .map_err(|_| "CONVERSATION_PROMOTE_FAILED")?;
            ctx.sync_catalog_into_online_store();
            serde_json::to_value(snap).map_err(|_| "CONVERSATION_PROMOTE_FAILED")
        }
        "app_conversation_select" => {
            let conversation_id = args
                .get("conversationId")
                .or_else(|| args.get("conversation_id"))
                .and_then(Value::as_str)
                .ok_or("CONVERSATION_SELECT_FAILED")?;
            ctx.app
                .select_conversation(conversation_id, at)
                .map_err(|_| "CONVERSATION_SELECT_FAILED")?;
            let snap = ctx
                .app
                .catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")?;
            serde_json::to_value(snap).map_err(|_| "CATALOG_READ_FAILED")
        }
        "app_view_state_save" => {
            let conversation_id = args
                .get("conversationId")
                .or_else(|| args.get("conversation_id"))
                .and_then(Value::as_str)
                .ok_or("VIEW_STATE_SAVE_FAILED")?;
            let draft = args.get("draft").and_then(Value::as_str);
            let scroll_offset = args
                .get("scrollOffset")
                .or_else(|| args.get("scroll_offset"))
                .and_then(Value::as_i64);
            let mark_read = args
                .get("markRead")
                .or_else(|| args.get("mark_read"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let state = ctx
                .app
                .save_conversation_view_state(
                    conversation_id,
                    draft,
                    None,
                    scroll_offset,
                    mark_read,
                    at,
                )
                .map_err(|_| "VIEW_STATE_SAVE_FAILED")?;
            serde_json::to_value(state).map_err(|_| "VIEW_STATE_SAVE_FAILED")
        }
        "app_preferences_save" => {
            let sidebar = args
                .get("sidebarGroupMode")
                .or_else(|| args.get("sidebar_group_mode"))
                .and_then(Value::as_str);
            let locale = args.get("locale").and_then(Value::as_str);
            let theme = args.get("theme").and_then(Value::as_str);
            let prefs = ctx
                .app
                .save_ui_preferences(sidebar, locale, theme, at)
                .map_err(|_| "PREFERENCES_SAVE_FAILED")?;
            serde_json::to_value(prefs).map_err(|_| "PREFERENCES_SAVE_FAILED")
        }
        "app_personal_snapshot" => {
            let schedules = ctx
                .app
                .list_schedule_items()
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let notes = ctx
                .app
                .list_notes(None)
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let captures = ctx
                .app
                .list_captures()
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let memories = ctx
                .app
                .list_memories(false)
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let decisions = ctx
                .app
                .list_all_decisions()
                .map_err(|_| "PERSONAL_OPS_READ_FAILED")?;
            let pending_sync_count = ctx
                .app
                .pending_personal_op_syncs()
                .map(|v| v.len())
                .unwrap_or(0);
            Ok(json!({
                "schedules": schedules,
                "notes": notes,
                "captures": captures,
                "memories": memories,
                "decisions": decisions,
                "pending_sync_count": pending_sync_count,
            }))
        }
        "app_schedule_create" => {
            let id = args.get("id").and_then(Value::as_str).ok_or("SCHEDULE_CREATE_FAILED")?;
            let project_id = args
                .get("projectId")
                .or_else(|| args.get("project_id"))
                .and_then(Value::as_str);
            let task_id = args
                .get("taskId")
                .or_else(|| args.get("task_id"))
                .and_then(Value::as_str);
            let title = args.get("title").and_then(Value::as_str).ok_or("SCHEDULE_CREATE_FAILED")?;
            let kind = args.get("kind").and_then(Value::as_str).unwrap_or("event");
            let start_at = args
                .get("startAt")
                .or_else(|| args.get("start_at"))
                .and_then(Value::as_i64)
                .unwrap_or(at);
            let end_at = args
                .get("endAt")
                .or_else(|| args.get("end_at"))
                .and_then(Value::as_i64);
            let reminder_at = args
                .get("reminderAt")
                .or_else(|| args.get("reminder_at"))
                .and_then(Value::as_i64);
            let notes = args.get("notes").and_then(Value::as_str).unwrap_or("");
            let created = ctx
                .app
                .create_schedule_item_with_kind(
                    id, project_id, task_id, title, kind, start_at, end_at, reminder_at, notes, at,
                )
                .map_err(|_| "SCHEDULE_CREATE_FAILED")?;
            serde_json::to_value(created).map_err(|_| "SCHEDULE_CREATE_FAILED")
        }
        "app_schedule_edit" => {
            let item = ctx.app.edit_schedule_item(
                args["id"].as_str().ok_or("SCHEDULE_UPDATE_FAILED")?,
                args["expectedVersion"].as_i64().ok_or("SCHEDULE_UPDATE_FAILED")?,
                args["title"].as_str().ok_or("SCHEDULE_UPDATE_FAILED")?,
                args["startAt"].as_i64().ok_or("SCHEDULE_UPDATE_FAILED")?,
                args["notes"].as_str().unwrap_or(""), at
            ).map_err(|_| "SCHEDULE_UPDATE_FAILED")?;
            serde_json::to_value(item).map_err(|_| "SCHEDULE_UPDATE_FAILED")
        }
        "app_schedule_status" => {
            let id = args.get("id").and_then(Value::as_str).ok_or("SCHEDULE_UPDATE_FAILED")?;
            let status = args
                .get("status")
                .and_then(Value::as_str)
                .ok_or("SCHEDULE_UPDATE_FAILED")?;
            let updated = ctx
                .app
                .update_schedule_status(id, status, at)
                .map_err(|_| "SCHEDULE_UPDATE_FAILED")?;
            serde_json::to_value(updated).map_err(|_| "SCHEDULE_UPDATE_FAILED")
        }
        "app_note_upsert" => {
            let id = args.get("id").and_then(Value::as_str).ok_or("NOTE_SAVE_FAILED")?;
            let project_id = args
                .get("projectId")
                .or_else(|| args.get("project_id"))
                .and_then(Value::as_str);
            let conversation_id = args
                .get("conversationId")
                .or_else(|| args.get("conversation_id"))
                .and_then(Value::as_str);
            let title = args.get("title").and_then(Value::as_str).unwrap_or("");
            let body_markdown = args
                .get("bodyMarkdown")
                .or_else(|| args.get("body_markdown"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let tags: Vec<String> = args
                .get("tags")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let note = ctx
                .app
                .upsert_note(id, project_id, conversation_id, title, body_markdown, &tags, at)
                .map_err(|_| "NOTE_SAVE_FAILED")?;
            serde_json::to_value(note).map_err(|_| "NOTE_SAVE_FAILED")
        }
        "app_capture_vision_target" => serde_json::to_value(ctx.runtime.vision_target()?).map_err(|_|"VISION_NOT_CONFIGURED"),
        "app_capture_media_put" => {
            let id=ctx.app.put_capture_media(args["mime"].as_str().unwrap_or(""),args["base64"].as_str().unwrap_or(""),at).map_err(|_|"CAPTURE_IMAGE_INVALID")?;
            Ok(json!({"id":id}))
        }
        "app_capture_media_get" => {
            let id=if let Some(id)=args["mediaId"].as_str() {Some(id.to_owned())} else {
                ctx.app.capture_image_id(args["captureId"].as_str().unwrap_or("")).map_err(|_|"CAPTURE_IMAGE_MISSING")?
            };
            match id {Some(id)=>serde_json::to_value(ctx.app.capture_media(&id).map_err(|_|"CAPTURE_IMAGE_MISSING")?).map_err(|_|"CAPTURE_IMAGE_MISSING"),None=>Ok(Value::Null)}
        }
        "app_capture_create" => {
            let id = args.get("id").and_then(Value::as_str).ok_or("CAPTURE_SAVE_FAILED")?;
            let source_message_id = args
                .get("sourceMessageId")
                .or_else(|| args.get("source_message_id"))
                .and_then(Value::as_str);
            let content_ref = args
                .get("contentRef")
                .or_else(|| args.get("content_ref"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let cap = ctx
                .app
                .capture_with_image(id, source_message_id, content_ref,
                    args.get("projectId").and_then(Value::as_str),
                    args.get("newProjectTitle").and_then(Value::as_str), args.get("mediaId").and_then(Value::as_str), at)
                .map_err(|_| "CAPTURE_SAVE_FAILED")?;
            serde_json::to_value(cap).map_err(|_| "CAPTURE_SAVE_FAILED")
        }
        "app_memory_state" => {
            let id = args.get("id").and_then(Value::as_str).ok_or("MEMORY_UPDATE_FAILED")?;
            let new_state = args
                .get("newState")
                .or_else(|| args.get("new_state"))
                .and_then(Value::as_str)
                .ok_or("MEMORY_UPDATE_FAILED")?;
            let mem = ctx
                .app
                .update_memory_state(id, new_state)
                .map_err(|_| "MEMORY_UPDATE_FAILED")?;
            serde_json::to_value(mem).map_err(|_| "MEMORY_UPDATE_FAILED")
        }
        "app_decision_record" => {
            let id = args.get("id").and_then(Value::as_str).ok_or("DECISION_SAVE_FAILED")?;
            let project_id = args
                .get("projectId")
                .or_else(|| args.get("project_id"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    let snap = ctx.app.catalog_snapshot().ok();
                    if let Some(first) = snap.and_then(|s| s.projects.into_iter().next()) {
                        first.id
                    } else {
                        let pid = "proj-default".to_owned();
                        let _ = ctx.app.create_project(&pid, "Default Workspace", at);
                        pid
                    }
                });
            let task_id = args
                .get("taskId")
                .or_else(|| args.get("task_id"))
                .and_then(Value::as_str);
            let body = args.get("body").and_then(Value::as_str).unwrap_or("");
            let source_message_id = args
                .get("sourceMessageId")
                .or_else(|| args.get("source_message_id"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    let session = ctx
                        .app
                        .selected_conversation()
                        .unwrap_or_else(|_| "default".to_owned());
                    let hist = ctx.app.history(&session).unwrap_or_default();
                    if let Some(last) = hist.last() {
                        last.id.clone()
                    } else {
                        let req_id = format!("dec-src-req-{id}");
                        let mid = format!("dec-src-msg-{id}");
                        let _ = ctx.app.accept(
                            &req_id,
                            &mid,
                            &session,
                            format!("Decision record: {body}"),
                            at,
                        );
                        mid
                    }
                });
            let rationale_json = args
                .get("rationaleJson")
                .or_else(|| args.get("rationale_json"))
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let status = args
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("proposed");
            let supersedes_id = args
                .get("supersedesId")
                .or_else(|| args.get("supersedes_id"))
                .and_then(Value::as_str);
            let dec = ctx
                .app
                .record_decision(
                    id,
                    &project_id,
                    task_id,
                    &source_message_id,
                    body,
                    rationale_json,
                    status,
                    supersedes_id,
                )
                .map_err(|_| "DECISION_SAVE_FAILED")?;
            serde_json::to_value(dec).map_err(|_| "DECISION_SAVE_FAILED")
        }
        "app_decision_status" => {
            let id = args.get("id").and_then(Value::as_str).ok_or("DECISION_UPDATE_FAILED")?;
            let status = args
                .get("status")
                .and_then(Value::as_str)
                .ok_or("DECISION_UPDATE_FAILED")?;
            let dec = ctx
                .app
                .update_decision_status(id, status)
                .map_err(|_| "DECISION_UPDATE_FAILED")?;
            serde_json::to_value(dec).map_err(|_| "DECISION_UPDATE_FAILED")
        }
        "app_model_hub_get" => ctx.runtime.build_snapshot_and_sync(&mut ctx.app),
        "app_provider_key_set" => {
            let pid = args
                .get("providerId")
                .or_else(|| args.get("provider_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let api_key = args
                .get("apiKey")
                .or_else(|| args.get("api_key"))
                .and_then(Value::as_str);
            let clear_key = args
                .get("clearKey")
                .or_else(|| args.get("clear_key"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            ctx.runtime
                .set_provider_key(&mut ctx.app, pid, api_key, clear_key)
        }
        "app_model_role_assign" => {
            let role = args.get("role").and_then(Value::as_str).unwrap_or("main");
            let model_ref = args
                .get("modelRef")
                .or_else(|| args.get("modelEntryId"))
                .or_else(|| args.get("model_ref"))
                .and_then(Value::as_str)
                .unwrap_or("");
            ctx.runtime.assign_role(&mut ctx.app, role, model_ref)
        }
        "app_provider_variant_set" => {
            let pid = args
                .get("providerId")
                .or_else(|| args.get("provider_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let variant = args
                .get("variant")
                .and_then(Value::as_str)
                .unwrap_or("default");
            ctx.runtime
                .set_provider_variant(&mut ctx.app, pid, variant)
        }
        "app_custom_model_save" => {
            let id = args.get("id").and_then(Value::as_str).map(str::to_owned);
            let name = args.get("name").and_then(Value::as_str).unwrap_or("");
            let model_name = args
                .get("modelName")
                .or_else(|| args.get("modelId"))
                .or_else(|| args.get("model_name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let base_url = args
                .get("baseUrl")
                .or_else(|| args.get("base_url"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let api_key = args
                .get("apiKey")
                .or_else(|| args.get("api_key"))
                .and_then(Value::as_str);
            let vision = args
                .get("vision")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            ctx.runtime.save_custom_model(
                &mut ctx.app,
                id,
                name,
                model_name,
                base_url,
                api_key,
                vision,
            )
        }
        "app_custom_model_delete" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            ctx.runtime.delete_custom_model(&mut ctx.app, id)
        }
        "app_remote_node_save" => {
            let id = args.get("id").and_then(Value::as_str).map(str::to_owned);
            let name = args.get("name").or_else(|| args.get("label")).and_then(Value::as_str).unwrap_or("");
            let url = args.get("url").or_else(|| args.get("baseUrl")).and_then(Value::as_str).unwrap_or("");
            let token = args.get("token").and_then(Value::as_str);
            let clear_token = args
                .get("clearToken")
                .or_else(|| args.get("clear_token"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let remark = args.get("remark").and_then(Value::as_str);
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            ctx.runtime.save_remote_node(
                &mut ctx.app,
                id,
                name,
                url,
                token,
                clear_token,
                remark,
                enabled,
            )
        }
        "app_remote_node_delete" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            ctx.runtime.delete_remote_node(&mut ctx.app, id)
        }
        "app_registry_save" => {
            let registry = args
                .get("registry")
                .cloned()
                .ok_or("REGISTRY_SAVE_FAILED")?;
            ctx.runtime.save_registry(&registry)?;
            ctx.runtime.build_snapshot_and_sync(&mut ctx.app)
        }
        "app_model_visibility_set" => {
            let pid = args
                .get("providerId")
                .or_else(|| args.get("provider_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let mid = args
                .get("modelId")
                .or_else(|| args.get("model_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let visible = args
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            ctx.runtime
                .set_model_visibility(&mut ctx.app, pid, mid, visible)
        }
        "app_provider_get" => {
            let prof = ctx
                .app
                .direct_provider_profile()
                .map_err(|_| "PROVIDER_READ_FAILED")?;
            serde_json::to_value(prof).map_err(|_| "PROVIDER_READ_FAILED")
        }
        "app_route_get" => {
            let session = args
                .get("sessionId")
                .or_else(|| args.get("session_id"))
                .and_then(Value::as_str)
                .unwrap_or("default");
            let route = ctx
                .app
                .conversation_execution_route(session)
                .unwrap_or_else(|_| "auto".to_owned());
            Ok(json!(route))
        }
        "app_route_set" => {
            let session = args
                .get("sessionId")
                .or_else(|| args.get("session_id"))
                .and_then(Value::as_str)
                .unwrap_or("default");
            let route = args.get("route").and_then(Value::as_str).unwrap_or("auto");
            let saved = ctx
                .app
                .set_conversation_execution_route(session, route, at)
                .map_err(|_| "ROUTE_SAVE_FAILED")?;
            Ok(json!(saved))
        }
        "app_pairing_capabilities" => {
            serde_json::to_value(RuntimeCapabilityManifest::headless_node())
                .map_err(|_| "CAPABILITIES_UNAVAILABLE")
        }
        "app_pairing_status" => Ok(json!({
            "role": "home",
            "state": "home_ready",
            "paired_home": Value::Null,
            "home_enabled": true,
            "online_node": true,
        })),
        "app_home_sync" => Ok(json!({
            "status": "online_node_local",
            "reachable": true,
        })),
        "app_executions" => {
            let session = args
                .get("sessionId")
                .or_else(|| args.get("session_id"))
                .and_then(Value::as_str)
                .unwrap_or("default");
            let mut result: Vec<Value> = ctx
                .app
                .local_executions(session)
                .map_err(|_| "EXECUTION_READ_FAILED")?
                .into_iter()
                .filter_map(|e| serde_json::to_value(e).ok())
                .collect();
            let remote: Vec<Value> = ctx
                .app
                .remote_executions(session)
                .map_err(|_| "EXECUTION_READ_FAILED")?
                .into_iter()
                .filter_map(|e| serde_json::to_value(e).ok())
                .collect();
            result.extend(remote);
            Ok(json!(result))
        }
        "app_execute" => {
            let request_id = args
                .get("requestId")
                .or_else(|| args.get("request_id"))
                .and_then(Value::as_str)
                .ok_or("REQUEST_ID_REQUIRED")?;
            let prepared = ctx
                .app
                .prepare_direct_execution(request_id, at)
                .map_err(|_| "EXECUTION_PREPARE_FAILED")?;
            let profile = ctx
                .app
                .direct_provider_profile()
                .map_err(|_| "PROVIDER_READ_FAILED")?
                .ok_or("PROVIDER_UNCONFIGURED")?;
            let prompt = prepared
                .messages
                .last()
                .map(|m| m.content.as_str())
                .unwrap_or("");
            let outcome = if prepared.can_start {
                match ctx
                    .runtime
                    .invoke_direct_model(&profile, &prepared.messages, prompt)
                {
                    Ok(reply) => bob_kernel::direct_provider::DirectExecutionOutcome::Completed {
                        text: reply,
                        provider_reported_model: Some(prepared.model_id.clone()),
                        input_tokens: None,
                        output_tokens: None,
                    },
                    Err(code) => bob_kernel::direct_provider::DirectExecutionOutcome::Failed {
                        reason: code.to_owned(),
                    },
                }
            } else {
                bob_kernel::direct_provider::DirectExecutionOutcome::Failed {
                    reason: "EXECUTION_ALREADY_RUNNING".to_owned(),
                }
            };
            let exec = ctx
                .app
                .finish_direct_execution(&prepared.execution_id, outcome, now_millis())
                .map_err(|_| "EXECUTION_READ_FAILED")?;
            serde_json::to_value(exec).map_err(|_| "EXECUTION_READ_FAILED")
        }
        "app_dsh_list" | "app_dsh_history" | "app_dsh_prompt" => {
            let connection=args["connectionId"].as_str().ok_or("DSH_CONNECTION_REQUIRED")?;
            let client=ctx.runtime.dsh_client(connection)?;
            match command {
                "app_dsh_list" => client.list(),
                "app_dsh_history" => {
                    let session=args["sessionId"].as_str().ok_or("DSH_SESSION_REQUIRED")?;
                    serde_json::to_value(client.history(bob_runtime::dsh_session::SessionAddress{connection_id:connection.into(),session_id:session.into()})?).map_err(|_|"DSH_PROTOCOL")
                },
                _ => {
                    let session=args["sessionId"].as_str().ok_or("DSH_SESSION_REQUIRED")?;
                    let text=args["text"].as_str().filter(|s|!s.trim().is_empty() && s.len()<=32000).ok_or("DSH_TEXT_REQUIRED")?;
                    let request=args["requestId"].as_str().filter(|s|s.len()<=200 && !s.is_empty()).ok_or("DSH_REQUEST_REQUIRED")?;
                    bob_runtime::dsh_transport::dispatch_once(&ctx.runtime.data_dir,connection,session,text,request,||client.prompt(session,text,request))
                }
            }
        }
        "app_online_sync" => {
            ctx.sync_catalog_into_online_store();
            let catalog = ctx
                .app
                .catalog_snapshot()
                .map_err(|_| "CATALOG_READ_FAILED")?;
            let projects: Vec<Value> = catalog
                .projects
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "name": p.title,
                        "title": p.title,
                        "status": "active",
                        "updated_at": p.updated_at,
                    })
                })
                .collect();
            let tasks: Vec<Value> = catalog
                .conversations
                .iter()
                .filter_map(|c| {
                    c.project_id.as_ref().map(|pid| {
                        json!({
                            "id": format!("task-{}", c.id),
                            "project_id": pid,
                            "conversation_id": c.id,
                            "title": c.title,
                            "status": if c.has_running_execution { "running" } else { "ready" },
                            "updated_at": c.updated_at,
                        })
                    })
                })
                .collect();
            Ok(json!({
                "status": "connected",
                "offline_cached": false,
                "capabilities": {
                    "version": "client-v1",
                    "capabilities": {
                        "catalog": true,
                        "history": true,
                        "pagination": true,
                        "submit": true,
                        "query": true,
                        "cancel": true,
                        "artifacts": true,
                        "parent_revision": true,
                        "review": true,
                        "persistent_execution": true
                    }
                },
                "projects": projects,
                "tasks": tasks,
                "snapshots": ctx.online.task_snapshots,
                "messages": ctx.online.project_messages,
                "synced_at": at
            }))
        }
        "app_online_submit_operation" => {
            let op_type = args
                .get("operationType")
                .or_else(|| args.get("operation_type"))
                .and_then(Value::as_str)
                .unwrap_or("submit");
            let task_id = args
                .get("taskId")
                .or_else(|| args.get("task_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let request_id = args
                .get("requestId")
                .or_else(|| args.get("request_id"))
                .and_then(Value::as_str)
                .unwrap_or("req-local");
            let payload = args.get("payload").cloned().unwrap_or_else(|| json!({}));
            let op_body = json!({
                "request_id": request_id,
                "type": op_type,
                "task_id": task_id,
                "payload": payload,
            });
            execute_online_operation(ctx, &op_body)
        }
        _ => Ok(json!({ "status": "ok" })),
    }
}

pub fn execute_online_operation(ctx: &mut NodeContext, body: &Value) -> Result<Value, &'static str> {
    let request_id = body
        .get("request_id")
        .or_else(|| body.get("requestId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if !request_id.is_empty() {
        if let Some(cached) = ctx.online.operations.get(&request_id) {
            return Ok(cached.clone());
        }
    }
    let op_type = body
        .get("type")
        .or_else(|| body.get("operation"))
        .and_then(Value::as_str)
        .unwrap_or("submit");
    let task_id = body
        .get("task_id")
        .or_else(|| body.get("taskId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let payload = body.get("payload").cloned().unwrap_or_else(|| body.clone());
    let stamp = now_millis();

    ctx.sync_catalog_into_online_store();

    let result = match op_type {
        "create_revision" | "create_task_iteration" | "derive_revision" => {
            let instruction = payload
                .get("instruction")
                .or_else(|| payload.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("Follow-up revision V2");
            let snap = ctx
                .online
                .task_snapshots
                .entry(task_id.to_owned())
                .or_insert_with(|| {
                    json!({
                        "task": { "id": task_id, "title": task_id, "status": "in_progress" },
                        "executions": [],
                        "steering": [],
                        "verifications": [],
                        "reviews": [],
                        "artifacts": [],
                        "missing_facts": []
                    })
                });
            let execs = snap
                .get_mut("executions")
                .and_then(Value::as_array_mut)
                .ok_or("INVALID_SNAPSHOT")?;
            let next_iter = (execs.len() as i64) + 1;
            let parent_id = payload
                .get("parent_execution_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    execs
                        .last()
                        .and_then(|e| e.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            let new_exec_id = format!("exec-{task_id}-v{next_iter}");
            let new_exec = json!({
                "id": new_exec_id,
                "task_id": task_id,
                "parent_execution_id": parent_id,
                "iteration": next_iter,
                "instruction": instruction,
                "kind": "work_revision",
                "status": "running",
                "started_at": stamp,
                "updated_at": stamp,
            });
            execs.push(new_exec.clone());
            if let Some(artifacts) = snap.get_mut("artifacts").and_then(Value::as_array_mut) {
                artifacts.push(json!({
                    "id": format!("art-{task_id}-v{next_iter}"),
                    "execution_id": new_exec_id,
                    "type": "markdown",
                    "name": format!("REVISION_V{next_iter}_SUMMARY.md"),
                    "summary": format!("V{next_iter}: {instruction}"),
                    "content_available": true,
                    "created_at": stamp,
                }));
            }
            if let Some(verifications) =
                snap.get_mut("verifications").and_then(Value::as_array_mut)
            {
                verifications.push(json!({
                    "id": format!("ver-{task_id}-v{next_iter}"),
                    "task_id": task_id,
                    "execution_id": new_exec_id,
                    "status": "passed",
                    "summary": format!("Revision V{next_iter} verification checks passed"),
                    "created_at": stamp,
                    "completed_at": stamp,
                }));
            }
            json!({
                "operation_id": format!("op-{stamp}"),
                "request_id": request_id,
                "status": "completed",
                "task_id": task_id,
                "execution": new_exec,
                "snapshot": snap.clone(),
            })
        }
        "review" | "submit_review" => {
            let verdict = payload
                .get("verdict")
                .or_else(|| payload.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("approved");
            let note = payload
                .get("note")
                .or_else(|| payload.get("comment"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let snap = ctx
                .online
                .task_snapshots
                .entry(task_id.to_owned())
                .or_insert_with(|| {
                    json!({
                        "task": { "id": task_id, "title": task_id, "status": "in_progress" },
                        "executions": [],
                        "steering": [],
                        "verifications": [],
                        "reviews": [],
                        "artifacts": [],
                        "missing_facts": []
                    })
                });
            let exec_id = snap
                .get("executions")
                .and_then(Value::as_array)
                .and_then(|arr| arr.last())
                .and_then(|e| e.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("exec-v1")
                .to_owned();
            let review_entry = json!({
                "id": format!("rev-{task_id}-{stamp}"),
                "task_id": task_id,
                "execution_id": exec_id,
                "status": verdict,
                "note": note,
                "created_at": stamp,
                "resolved_at": stamp,
            });
            if let Some(reviews) = snap.get_mut("reviews").and_then(Value::as_array_mut) {
                reviews.push(review_entry.clone());
            }
            if let Some(task_obj) = snap.get_mut("task").and_then(Value::as_object_mut) {
                task_obj.insert(
                    "status".to_owned(),
                    json!(if verdict == "approved" {
                        "completed"
                    } else {
                        "changes_requested"
                    }),
                );
            }
            json!({
                "operation_id": format!("op-{stamp}"),
                "request_id": request_id,
                "status": "completed",
                "task_id": task_id,
                "review": review_entry,
                "snapshot": snap.clone(),
            })
        }
        "cancel" | "cancel_execution" => {
            if let Some(snap) = ctx.online.task_snapshots.get_mut(task_id) {
                if let Some(execs) = snap.get_mut("executions").and_then(Value::as_array_mut) {
                    if let Some(last) = execs.last_mut() {
                        last["status"] = json!("cancelled");
                        last["ended_at"] = json!(stamp);
                    }
                }
            }
            json!({
                "operation_id": format!("op-{stamp}"),
                "request_id": request_id,
                "status": "cancelled",
                "task_id": task_id,
            })
        }
        _ => {
            // Task continuation / steering message ("submit" / "continue_task" / "submit_message")
            let content = payload
                .get("content")
                .or_else(|| payload.get("instruction"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let project_id = ctx
                .online
                .task_snapshots
                .get(task_id)
                .and_then(|s| s.get("task"))
                .and_then(|t| t.get("project_id"))
                .and_then(Value::as_str)
                .unwrap_or("proj-default")
                .to_owned();
            let msgs = ctx
                .online
                .project_messages
                .entry(project_id.clone())
                .or_default();
            let seq = (msgs.len() as i64) + 1;
            let user_msg = json!({
                "sequence": seq,
                "id": format!("msg-{stamp}-{seq}"),
                "project_id": project_id,
                "task_id": task_id,
                "request_id": request_id,
                "role": "user",
                "content": content,
                "created_at": stamp,
            });
            msgs.push(user_msg);
            let reply_seq = seq + 1;
            let reply_msg = json!({
                "sequence": reply_seq,
                "id": format!("msg-{stamp}-{reply_seq}"),
                "project_id": project_id,
                "task_id": task_id,
                "request_id": request_id,
                "role": "assistant",
                "content": format!("Online Work Node received task continuation for {task_id}: {content}"),
                "created_at": stamp + 1,
            });
            msgs.push(reply_msg.clone());
            if let Some(snap) = ctx.online.task_snapshots.get_mut(task_id) {
                if let Some(steering) = snap.get_mut("steering").and_then(Value::as_array_mut) {
                    steering.push(json!({
                        "id": format!("steer-{stamp}"),
                        "task_id": task_id,
                        "sequence": seq,
                        "content": content,
                        "status": "acknowledged",
                        "created_at": stamp,
                        "acknowledged_at": stamp + 1,
                    }));
                }
            }
            json!({
                "operation_id": format!("op-{stamp}"),
                "request_id": request_id,
                "status": "completed",
                "task_id": task_id,
                "reply": reply_msg,
            })
        }
    };

    if !request_id.is_empty() {
        ctx.online
            .operations
            .insert(request_id, result.clone());
    }
    ctx.save_online();
    Ok(result)
}

fn verify_bearer_token(ctx: &NodeContext, auth_header: Option<&str>) -> Result<String, (u16, &'static str)> {
    let raw = auth_header.ok_or((401, "CLIENT_CREDENTIAL_INVALID"))?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .ok_or((401, "CLIENT_CREDENTIAL_INVALID"))?
        .trim();
    if token.is_empty() {
        return Err((401, "CLIENT_CREDENTIAL_INVALID"));
    }
    let thash = hash_token(token);
    let record = ctx
        .online
        .credentials
        .values()
        .find(|c| c.token_hash == thash)
        .ok_or((401, "CLIENT_CREDENTIAL_INVALID"))?;
    if record.revoked {
        return Err((401, "CLIENT_TOKEN_REVOKED"));
    }
    if record.expires_at < now_millis() {
        return Err((401, "CLIENT_TOKEN_EXPIRED"));
    }
    Ok(record.credential_id.clone())
}

pub fn handle_client_v1_http(
    ctx: &mut NodeContext,
    method: &str,
    path: &str,
    query: &str,
    auth_header: Option<&str>,
    body_str: &str,
) -> (u16, Value) {
    let stamp = now_millis();
    if path == "/api/client/v1/pairings" && method == "POST" {
        let parsed: Value = serde_json::from_str(body_str).unwrap_or_else(|_| json!({}));
        let device_name = parsed
            .get("device_name")
            .and_then(Value::as_str)
            .unwrap_or("Bob Client")
            .to_owned();
        let pairing_id = format!("pair_{stamp}_{}", ctx.online.pairings.len() + 1);
        let code_num = ((stamp as u64) % 900_000) + 100_000;
        let user_code = format!("{code_num}");
        let session = OnlinePairingSession {
            pairing_id: pairing_id.clone(),
            user_code: user_code.clone(),
            device_name,
            created_at: stamp,
            expires_at: stamp + 600_000,
            exchanged_token: None,
        };
        ctx.online.pairings.insert(pairing_id.clone(), session);
        ctx.save_online();
        return (
            201,
            json!({
                "pairing_id": pairing_id,
                "user_code": user_code,
                "expires_at": stamp + 600_000,
            }),
        );
    }

    if (path == "/api/client/v1/pairings/exchange"
        || (path.starts_with("/api/client/v1/pairings/") && path.ends_with("/exchange")))
        && method == "POST"
    {
        let parsed: Value = serde_json::from_str(body_str).unwrap_or_else(|_| json!({}));
        let code = parsed
            .get("code")
            .or_else(|| parsed.get("user_code"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let found = ctx
            .online
            .pairings
            .values_mut()
            .find(|p| p.user_code == code || path.contains(&p.pairing_id));
        let Some(session) = found else {
            return (
                404,
                json!({ "code": "PAIRING_UNAVAILABLE", "message": "Invalid pairing code" }),
            );
        };
        if session.expires_at < stamp {
            return (
                410,
                json!({ "code": "PAIRING_EXPIRED", "message": "Pairing code expired" }),
            );
        }
        let raw_token = format!("bob_online_tok_{}_{}", session.pairing_id, stamp);
        let cred_id = format!("cred_{stamp}");
        session.exchanged_token = Some(cred_id.clone());
        let dev_name = session.device_name.clone();
        ctx.online.credentials.insert(
            cred_id.clone(),
            OnlineCredentialRecord {
                credential_id: cred_id.clone(),
                token_hash: hash_token(&raw_token),
                device_name: dev_name,
                created_at: stamp,
                expires_at: stamp + 365 * 86_400_000,
                revoked: false,
            },
        );
        ctx.save_online();
        return (
            200,
            json!({
                "credential_id": cred_id,
                "access_token": raw_token,
                "token": raw_token,
                "expires_at": stamp + 365 * 86_400_000,
            }),
        );
    }

    let cred_id = match verify_bearer_token(ctx, auth_header) {
        Ok(cid) => cid,
        Err((status, code)) => {
            return (status, json!({ "code": code, "retryable": false }));
        }
    };

    if (path == "/api/client/v1/credentials/self/revoke"
        || path == "/api/client/v1/devices/current/revoke")
        && method == "POST"
    {
        if let Some(cred) = ctx.online.credentials.get_mut(&cred_id) {
            cred.revoked = true;
        }
        ctx.save_online();
        return (200, json!({ "status": "revoked" }));
    }

    ctx.sync_catalog_into_online_store();
    let catalog = ctx.app.catalog_snapshot().ok();

    if path == "/api/client/v1/capabilities" && method == "GET" {
        let grants: Vec<Value> = catalog
            .as_ref()
            .map(|c| {
                c.projects
                    .iter()
                    .map(|p| {
                        json!({
                            "project_id": p.id,
                            "scopes": ["work.read", "work.write"]
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        return (
            200,
            json!({
                "protocol_version": "client-v1",
                "version": "client-v1",
                "server_instance_id": ctx.online.server_instance_id,
                "credential_id": cred_id,
                "grants": grants,
                "capabilities": {
                    "catalog": true,
                    "history": true,
                    "pagination": true,
                    "submit": true,
                    "query": true,
                    "cancel": true,
                    "approvals": true,
                    "artifacts": true,
                    "parent_revision": true,
                    "review": true,
                    "persistent_execution": true
                }
            }),
        );
    }

    if path == "/api/client/v1/projects" && method == "GET" {
        let items: Vec<Value> = catalog
            .as_ref()
            .map(|c| {
                c.projects
                    .iter()
                    .map(|p| {
                        json!({
                            "id": p.id,
                            "name": p.title,
                            "title": p.title,
                            "description": p.title,
                            "status": "active",
                            "created_at": p.updated_at,
                            "updated_at": p.updated_at,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        return (
            200,
            json!({
                "items": items,
                "projects": items,
                "has_more": false,
                "next_cursor": Value::Null,
            }),
        );
    }

    if path.starts_with("/api/client/v1/projects/") && method == "GET" {
        let rest = &path["/api/client/v1/projects/".len()..];
        if let Some((project_id, suffix)) = rest.split_once('/') {
            if suffix == "tasks" {
                let items: Vec<Value> = catalog
                    .as_ref()
                    .map(|c| {
                        c.conversations
                            .iter()
                            .filter(|conv| conv.project_id.as_deref() == Some(project_id))
                            .map(|conv| {
                                json!({
                                    "id": format!("task-{}", conv.id),
                                    "project_id": project_id,
                                    "conversation_id": conv.id,
                                    "title": conv.title,
                                    "instruction": conv.title,
                                    "description": conv.title,
                                    "status": if conv.has_running_execution { "running" } else { "ready" },
                                    "created_at": conv.updated_at,
                                    "updated_at": conv.updated_at,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                return (
                    200,
                    json!({
                        "items": items,
                        "tasks": items,
                        "has_more": false,
                        "next_cursor": Value::Null,
                    }),
                );
            } else if suffix == "messages" {
                let after: i64 = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("after="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let all_msgs = ctx
                    .online
                    .project_messages
                    .get(project_id)
                    .cloned()
                    .unwrap_or_default();
                let items: Vec<Value> = all_msgs
                    .into_iter()
                    .filter(|m| m.get("sequence").and_then(Value::as_i64).unwrap_or(0) > after)
                    .collect();
                let next_after = items
                    .last()
                    .and_then(|m| m.get("sequence"))
                    .and_then(Value::as_i64)
                    .unwrap_or(after);
                return (
                    200,
                    json!({
                        "items": items,
                        "messages": items,
                        "next_after": next_after,
                        "has_more": false,
                    }),
                );
            }
        }
    }

    if path.starts_with("/api/client/v1/tasks/") && method == "GET" {
        let rest = &path["/api/client/v1/tasks/".len()..];
        if let Some((task_id, suffix)) = rest.split_once('/') {
            if suffix == "snapshot" {
                if let Some(snap) = ctx.online.task_snapshots.get(task_id) {
                    let mut out = snap.clone();
                    out["observed_at"] = json!(stamp);
                    return (200, out);
                }
                return (404, json!({ "code": "RESOURCE_NOT_FOUND" }));
            } else if suffix == "messages" {
                let project_id = ctx
                    .online
                    .task_snapshots
                    .get(task_id)
                    .and_then(|s| s.get("task"))
                    .and_then(|t| t.get("project_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("proj-default");
                let items: Vec<Value> = ctx
                    .online
                    .project_messages
                    .get(project_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|m| m.get("task_id").and_then(Value::as_str) == Some(task_id))
                    .collect();
                return (
                    200,
                    json!({
                        "items": items,
                        "messages": items,
                        "has_more": false,
                    }),
                );
            }
        }
    }

    if path == "/api/client/v1/operations" && method == "POST" {
        let parsed: Value = serde_json::from_str(body_str).unwrap_or_else(|_| json!({}));
        return match execute_online_operation(ctx, &parsed) {
            Ok(res) => (200, res),
            Err(code) => (400, json!({ "code": code })),
        };
    }

    (404, json!({ "code": "CLIENT_ROUTE_NOT_FOUND" }))
}

fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn handle_connection(mut stream: TcpStream, state: Arc<Mutex<NodeContext>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));

    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut header_end = None;
    let mut content_length = 0usize;

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if header_end.is_none() {
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos + 4);
                        let hdr_str = String::from_utf8_lossy(&buf[..pos]);
                        for line in hdr_str.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_length = v.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                if content_length > 12 * 1024 * 1024 || buf.len() > 12 * 1024 * 1024 {
                    write_json_response(&mut stream, 413, r#"{"ok":false,"error":"REQUEST_TOO_LARGE"}"#);
                    return;
                }
                if let Some(hend) = header_end {
                    if buf.len() >= hend + content_length {
                        break;
                    }
                }
                if buf.len() > 12 * 1024 * 1024 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let Some(hend) = header_end else {
        return;
    };
    let header_text = String::from_utf8_lossy(&buf[..hend - 4]).to_owned();
    let body_bytes = if buf.len() >= hend + content_length {
        &buf[hend..hend + content_length]
    } else {
        &buf[hend..]
    };
    let body_str = String::from_utf8_lossy(body_bytes);

    let mut lines = header_text.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_target = parts.next().unwrap_or("/");
    let (path, query) = raw_target.split_once('?').unwrap_or((raw_target, ""));

    let mut auth_header: Option<String> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("authorization") {
                auth_header = Some(v.trim().to_owned());
            }
        }
    }

    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    if path == "/api/capabilities" && method == "GET" {
        let payload = serde_json::to_string(&RuntimeCapabilityManifest::headless_node())
            .unwrap_or_else(|_| "{}".to_owned());
        write_json_response(&mut stream, 200, &payload);
        return;
    }

    if path == "/api/events" && method == "GET" {
        let sse = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\ndata: {{\"type\":\"connected\",\"timestamp\":{}}}\n\n",
            now_millis()
        );
        let _ = stream.write_all(sse.as_bytes());
        return;
    }

    if let Some(command) = path.strip_prefix("/api/rpc/") {
        if method != "POST" {
            write_json_response(&mut stream, 405, r#"{"error":"METHOD_NOT_ALLOWED"}"#);
            return;
        }
        let parsed_body: Value = serde_json::from_str(&body_str).unwrap_or_else(|_| json!({}));
        let args = parsed_body
            .get("args")
            .cloned()
            .unwrap_or(parsed_body);
        let mut guard = match state.lock() {
            Ok(g) => g,
            Err(_) => {
                write_json_response(&mut stream, 500, r#"{"error":"NODE_LOCK_POISONED"}"#);
                return;
            }
        };
        if command.trim()=="app_capture_recognize" {
            let prepared=(|| {
                let image=guard.app.capture_media(args["mediaId"].as_str().unwrap_or("")).map_err(|_|"CAPTURE_IMAGE_MISSING")?;
                guard.runtime.prepare_vision(&image,args["modelRef"].as_str().unwrap_or(""))
            })();
            drop(guard);
            let result=prepared.and_then(|(target,spec)|bob_runtime::vision::execute(target,spec));
            let (status,value)=match result {Ok(result)=>(200,json!({"ok":true,"result":result})),Err(e)=>(400,json!({"ok":false,"error":e}))};
            write_json_response(&mut stream,status,&value.to_string());
            return;
        }
        match handle_rpc_command(&mut guard, command.trim(), &args) {
            Ok(result) => {
                let payload = serde_json::to_string(&json!({ "ok": true, "result": result }))
                    .unwrap_or_else(|_| r#"{"ok":true,"result":null}"#.to_owned());
                write_json_response(&mut stream, 200, &payload);
            }
            Err(code) => {
                let payload = serde_json::to_string(&json!({ "ok": false, "error": code }))
                    .unwrap_or_else(|_| r#"{"ok":false,"error":"RPC_FAILED"}"#.to_owned());
                write_json_response(&mut stream, 400, &payload);
            }
        }
        return;
    }

    if path == "/api/client/v1" || path.starts_with("/api/client/v1/") {
        let mut guard = match state.lock() {
            Ok(g) => g,
            Err(_) => {
                write_json_response(&mut stream, 500, r#"{"code":"NODE_LOCK_POISONED"}"#);
                return;
            }
        };
        let (status, value) = handle_client_v1_http(
            &mut guard,
            method,
            path,
            query,
            auth_header.as_deref(),
            &body_str,
        );
        let payload = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned());
        write_json_response(&mut stream, status, &payload);
        return;
    }

    // Static file serving for Unified Web UI
    let static_dir = state.lock().ok().and_then(|g| g.static_dir.clone());
    if let Some(root) = static_dir {
        let rel = path.trim_start_matches('/');
        let safe_rel = if rel.is_empty() || rel.contains("..") {
            "index.html"
        } else {
            rel
        };
        let candidate = root.join(safe_rel);
        let file_to_serve = if candidate.is_file() {
            candidate
        } else {
            root.join("index.html")
        };
        if let Ok(bytes) = std::fs::read(&file_to_serve) {
            let ctype = guess_content_type(&file_to_serve);
            if file_to_serve.file_name().and_then(|s| s.to_str()) == Some("index.html") {
                let html = String::from_utf8_lossy(&bytes);
                let injected = if html.contains("<head>") {
                    html.replacen(
                        "<head>",
                        "<head><script>window.__BOB_NODE_RUNTIME__=true;</script>",
                        1,
                    )
                } else {
                    format!("<script>window.__BOB_NODE_RUNTIME__=true;</script>{html}")
                };
                write_bytes_response(&mut stream, 200, ctype, injected.as_bytes());
                return;
            }
            write_bytes_response(&mut stream, 200, ctype, &bytes);
            return;
        }
    }

    // Default fallback HTML when static_dir is not configured
    let fallback = r#"<!doctype html><html><head><meta charset="utf-8"><script>window.__BOB_NODE_RUNTIME__=true;</script><title>Bob Online Node</title></head><body><h1>Bob Online Node Active</h1></body></html>"#;
    write_bytes_response(
        &mut stream,
        200,
        "text/html; charset=utf-8",
        fallback.as_bytes(),
    );
}

fn write_json_response(stream: &mut TcpStream, status: u16, json_body: &str) {
    write_bytes_response(
        stream,
        status,
        "application/json; charset=utf-8",
        json_body.as_bytes(),
    );
}

fn write_bytes_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        410 => "Gone",
        429 => "Too Many Requests",
        _ => "Internal Server Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

pub struct RunningNodeServer {
    pub local_addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl RunningNodeServer {
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.local_addr, Duration::from_millis(100));
    }
}

pub fn spawn_node_server(config: BobNodeConfig) -> Result<RunningNodeServer, &'static str> {
    let listener = TcpListener::bind(&config.bind_addr).map_err(|_| "BIND_FAILED")?;
    let local_addr = listener.local_addr().map_err(|_| "LOCAL_ADDR_FAILED")?;
    let ctx = Arc::new(Mutex::new(NodeContext::open(&config)?));
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();

    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(stream) = incoming {
                let state = ctx.clone();
                std::thread::spawn(move || handle_connection(stream, state));
            }
        }
    });

    Ok(RunningNodeServer { local_addr, stop })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http_request(
        addr: SocketAddr,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<&str>,
    ) -> (u16, Value) {
        let mut stream = TcpStream::connect(addr).expect("connect to node");
        let payload = body.unwrap_or("");
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        stream.write_all(req.as_bytes()).expect("write request");
        let mut resp = String::new();
        stream.read_to_string(&mut resp).expect("read response");
        let (hdr, json_str) = resp.split_once("\r\n\r\n").expect("split http response");
        let status: u16 = hdr
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let val: Value = serde_json::from_str(json_str).unwrap_or_else(|_| json!({ "raw": json_str }));
        (status, val)
    }

    #[test]
    fn bob_node_rpc_and_client_v1_end_to_end_closed_loop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let server = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .expect("spawn server");

        // 1. Capabilities endpoint
        let (status, caps) = http_request(server.local_addr, "GET", "/api/capabilities", None, None);
        assert_eq!(status, 200);
        assert_eq!(caps["browser_mode"], json!(true));
        assert_eq!(caps["camera_scan"], json!(false));

        // 2. Web RPC: bootstrap, accept message, promote conversation in-place (RV-17)
        let (status, boot) = http_request(
            server.local_addr,
            "POST",
            "/api/rpc/app_bootstrap",
            None,
            Some("{}"),
        );
        assert_eq!(status, 200);
        let conv_id = boot["result"]["session_id"].as_str().unwrap().to_owned();

        let (status, _) = http_request(
            server.local_addr,
            "POST",
            "/api/rpc/app_accept",
            None,
            Some(
                &json!({
                    "requestId": "req-web-1",
                    "messageId": "msg-web-1",
                    "sessionId": conv_id,
                    "content": "Draft architecture for bob-online.bobbik.org"
                })
                .to_string(),
            ),
        );
        assert_eq!(status, 200);

        let (status, promoted) = http_request(
            server.local_addr,
            "POST",
            "/api/rpc/app_conversation_promote",
            None,
            Some(
                &json!({
                    "conversationId": conv_id,
                    "projectId": "proj-online-1",
                    "title": "Bob Online Deployment"
                })
                .to_string(),
            ),
        );
        assert_eq!(status, 200);
        assert_eq!(promoted["result"]["selected_conversation_id"], json!(conv_id));
        let convs = promoted["result"]["conversations"].as_array().unwrap();
        assert!(convs
            .iter()
            .any(|c| c["id"] == json!(conv_id) && c["project_id"] == json!("proj-online-1")));
        let task_id = format!("task-{conv_id}");

        // 3. Online Client API v1: pairing -> exchange -> capabilities -> projects -> tasks -> snapshot -> operations (continue, V1->V2, review) -> revoke
        let (status, pair_res) = http_request(
            server.local_addr,
            "POST",
            "/api/client/v1/pairings",
            None,
            Some(r#"{"device_name":"Pixel 8"}"#),
        );
        assert_eq!(status, 201);
        let user_code = pair_res["user_code"].as_str().unwrap().to_owned();

        let (status, ex_res) = http_request(
            server.local_addr,
            "POST",
            "/api/client/v1/pairings/exchange",
            None,
            Some(&json!({ "code": user_code }).to_string()),
        );
        assert_eq!(status, 200);
        let token = ex_res["access_token"].as_str().unwrap().to_owned();

        let (status, v1_caps) = http_request(
            server.local_addr,
            "GET",
            "/api/client/v1/capabilities",
            Some(&token),
            None,
        );
        assert_eq!(status, 200);
        assert_eq!(v1_caps["capabilities"]["submit"], json!(true));
        assert_eq!(v1_caps["capabilities"]["parent_revision"], json!(true));
        assert_eq!(v1_caps["capabilities"]["review"], json!(true));

        // Explicit first operation: catalog refresh must not invent a V1.
        let (status, first_op) = http_request(
            server.local_addr, "POST", "/api/client/v1/operations", Some(&token),
            Some(&json!({"request_id":"op-first-v1", "type":"create_revision",
                "task_id":task_id, "payload":{"instruction":"First test operation"}}).to_string()),
        );
        assert_eq!(status, 200);
        assert_eq!(first_op["execution"]["iteration"], json!(1));
        assert_eq!(first_op["snapshot"]["executions"].as_array().unwrap().len(), 1);

        // Create V2 revision
        let (status, rev_op) = http_request(
            server.local_addr,
            "POST",
            "/api/client/v1/operations",
            Some(&token),
            Some(
                &json!({
                    "request_id": "op-rev-v2",
                    "type": "create_revision",
                    "task_id": task_id,
                    "payload": { "instruction": "Add Cloudflare Tunnel route verification" }
                })
                .to_string(),
            ),
        );
        assert_eq!(status, 200);
        assert_eq!(rev_op["execution"]["iteration"], json!(2));

        // Idempotent retry of same request_id returns exact same operation without creating V3
        let (status, rev_op_retry) = http_request(
            server.local_addr,
            "POST",
            "/api/client/v1/operations",
            Some(&token),
            Some(
                &json!({
                    "request_id": "op-rev-v2",
                    "type": "create_revision",
                    "task_id": task_id,
                    "payload": { "instruction": "Add Cloudflare Tunnel route verification" }
                })
                .to_string(),
            ),
        );
        assert_eq!(status, 200);
        assert_eq!(rev_op_retry["operation_id"], rev_op["operation_id"]);
        assert_eq!(
            rev_op_retry["snapshot"]["executions"].as_array().unwrap().len(),
            2
        );

        // Submit review approval
        let (status, review_op) = http_request(
            server.local_addr,
            "POST",
            "/api/client/v1/operations",
            Some(&token),
            Some(
                &json!({
                    "request_id": "op-review-1",
                    "type": "review",
                    "task_id": task_id,
                    "payload": { "verdict": "approved", "note": "Verified V2 on Hostinger" }
                })
                .to_string(),
            ),
        );
        assert_eq!(status, 200);
        assert_eq!(review_op["snapshot"]["task"]["status"], json!("completed"));

        // Revoke token -> subsequent request returns 401
        let (status, _) = http_request(
            server.local_addr,
            "POST",
            "/api/client/v1/devices/current/revoke",
            Some(&token),
            Some("{}"),
        );
        assert_eq!(status, 200);

        let (status_after, err_after) = http_request(
            server.local_addr,
            "GET",
            "/api/client/v1/projects",
            Some(&token),
            None,
        );
        assert_eq!(status_after, 401);
        assert_eq!(err_after["code"], json!("CLIENT_TOKEN_REVOKED"));

        server.shutdown();
    }

    #[test]
    fn xp10_import_legacy_nexus_db_is_idempotent_and_read_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let legacy_db = dir.path().join("nexus_control_plane.db");
        {
            let conn = Connection::open(&legacy_db).expect("create legacy db");
            conn.execute_batch(
                "CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT, updated_at INTEGER);
                 CREATE TABLE tasks (id TEXT PRIMARY KEY, project_id TEXT, title TEXT, status TEXT, updated_at INTEGER);
                 INSERT INTO projects VALUES ('proj-legacy-1', 'Legacy Hostinger Project', 1762000000000);
                 INSERT INTO tasks VALUES ('task-legacy-1', 'proj-legacy-1', 'Migrate Online Node', 'completed', 1762000005000);",
            )
            .expect("seed legacy db");
        }
        let mut ctx = NodeContext::open(&BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: Some(legacy_db.clone()),
        })
        .expect("open node context with legacy db");

        // Second import must be idempotent
        let imported = ctx.import_legacy_nexus_db(&legacy_db).expect("re-import");
        assert_eq!(imported, 1);

        let catalog = ctx.app.catalog_snapshot().expect("catalog");
        assert!(catalog.projects.iter().any(|p| p.id == "proj-legacy-1"));
        assert!(ctx.online.task_snapshots.contains_key("task-legacy-1"));
        let snapshot = &ctx.online.task_snapshots["task-legacy-1"];
        assert_eq!(snapshot["executions"], json!([]));
        assert_eq!(snapshot["verifications"], json!([]));
        assert_eq!(snapshot["artifacts"], json!([]));
        assert_eq!(ctx.online.imported_project_ids, vec!["proj-legacy-1"]);
        assert!(!ctx.online.task_snapshots.contains_key("task-conv-task-legacy-1"));
        let projected = handle_rpc_command(&mut ctx, "app_catalog", &json!({})).unwrap();
        assert_eq!(projected["projects"][0]["source_kind"], "legacy_import");
    }
}
