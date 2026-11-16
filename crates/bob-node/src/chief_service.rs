//! Pluggable stateless Multi-Chief-of-Staff MCP bus & human-gated dispatch orders (`BOB-D-017` / `WP-C1`).
//!
//! External Chiefs of Staff (`bob-online`, `grok_bot`, `meta_muse`, ...) interact exclusively via
//! `/api/bot/v1/mcp` (`bob_briefing`, `bob_order_submit`, `bob_orders`). Orders ALWAYS pause at
//! `"pending_approval"` until explicitly approved by the owner via Jarvis Approval Cards; remote
//! worker nodes report receipts strictly via owner/device-scoped RPC (`app_chief_order_complete`).

use crate::NodeContext;
use bob_kernel::app::LocalApp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
};

const STORE_FILE: &str = "chief_orders.json";
const ALLOWED_TARGET_NODES: &[&str] = &["auto", "x1t", "x1c", "vps1", "vps2", "vps3"];
const ALLOWED_PRIORITIES: &[&str] = &["low", "normal", "high", "urgent"];
const ALLOWED_ORDER_STATUSES: &[&str] = &[
    "pending_approval",
    "approved",
    "rejected",
    "running",
    "completed",
    "failed",
];
const MAX_ARTIFACTS: usize = 32;
const MAX_DELIVERABLES: usize = 16;

static STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static TMP_SEQ: AtomicU64 = AtomicU64::new(1);

fn store_mutex() -> &'static Mutex<()> {
    STORE_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChiefOrderReceipt {
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    pub worker_node: String,
    pub recorded_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChiefOrder {
    pub order_id: String,
    pub approval_id: String,
    pub request_id: String,
    pub payload_hash: String,
    pub chief_actor: String,
    pub project_id: String,
    pub target_node: String,
    pub title: String,
    pub instruction: String,
    pub priority: String,
    #[serde(default)]
    pub deliverables: Vec<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub approved_at: Option<i64>,
    #[serde(default)]
    pub rejected_at: Option<i64>,
    #[serde(default)]
    pub linked_task_id: Option<String>,
    #[serde(default)]
    pub receipt: Option<ChiefOrderReceipt>,
}

#[derive(Default, Serialize, Deserialize)]
struct ChiefOrderStoreData {
    #[serde(default)]
    version: i64,
    #[serde(default)]
    orders: Vec<ChiefOrder>,
}

fn load_store(data_dir: &Path) -> Result<ChiefOrderStoreData, &'static str> {
    let path = data_dir.join(STORE_FILE);
    match fs::read(&path) {
        Ok(bytes) => {
            if bytes.is_empty() {
                return Ok(ChiefOrderStoreData::default());
            }
            serde_json::from_slice(&bytes).map_err(|_| "CHIEF_STORE_CORRUPT")
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(ChiefOrderStoreData::default())
        }
        Err(_) => Err("CHIEF_STORE_READ_FAILED"),
    }
}

fn save_store(data_dir: &Path, store: &ChiefOrderStoreData) -> Result<(), &'static str> {
    fs::create_dir_all(data_dir).map_err(|_| "CHIEF_STORE_WRITE_FAILED")?;
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = data_dir.join(format!("{STORE_FILE}.tmp.{}.{}", std::process::id(), seq));
    let dst = data_dir.join(STORE_FILE);
    let encoded = serde_json::to_vec_pretty(store).map_err(|_| "CHIEF_STORE_ENCODE_FAILED")?;
    {
        let mut f = fs::File::create(&tmp).map_err(|_| "CHIEF_STORE_WRITE_FAILED")?;
        f.write_all(&encoded)
            .map_err(|_| "CHIEF_STORE_WRITE_FAILED")?;
        f.sync_all().map_err(|_| "CHIEF_STORE_WRITE_FAILED")?;
    }
    if fs::rename(&tmp, &dst).is_err() {
        if dst.exists() {
            let _ = fs::remove_file(&dst);
            if fs::rename(&tmp, &dst).is_err() {
                let _ = fs::remove_file(&tmp);
                return Err("CHIEF_STORE_WRITE_FAILED");
            }
        } else {
            let _ = fs::remove_file(&tmp);
            return Err("CHIEF_STORE_WRITE_FAILED");
        }
    }
    Ok(())
}

fn is_project_authorized(projects: &[String], all_projects: bool, pid: &str) -> bool {
    let trimmed = pid.trim();
    !trimmed.is_empty() && (all_projects || projects.iter().any(|p| p == trimmed))
}

fn check_allowed_keys(obj: &serde_json::Map<String, Value>, allowed: &[&str]) -> Result<(), &'static str> {
    if obj.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("BOT_ARGUMENTS_INVALID");
    }
    Ok(())
}

fn required_trimmed_string(args: &Value, key: &str, max_len: usize) -> Result<String, &'static str> {
    let raw = args.get(key).and_then(Value::as_str).ok_or("BOT_ARGUMENTS_INVALID")?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > max_len {
        return Err("BOT_ARGUMENTS_INVALID");
    }
    Ok(trimmed.to_string())
}

fn parse_optional_project_filter<'a>(
    args: &'a Value,
    projects: &[String],
    all_projects: bool,
) -> Result<Option<&'a str>, &'static str> {
    match args.get("project_id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.len() > 128 {
                return Err("BOT_ARGUMENTS_INVALID");
            }
            if !is_project_authorized(projects, all_projects, trimmed) {
                return Err("BOT_SCOPE_DENIED");
            }
            Ok(Some(trimmed))
        }
        _ => Err("BOT_ARGUMENTS_INVALID"),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn resolve_external_briefing_path(data_dir: &Path) -> Option<PathBuf> {
    let local_override = data_dir.join("chief_briefing.json");
    if local_override.is_file() {
        return Some(local_override);
    }
    if let Some(env_path) = std::env::var_os("BOB_BRIEFING_JSON_PATH") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    for var in ["USERPROFILE", "HOME"] {
        if let Some(home) = std::env::var_os(var) {
            let p = PathBuf::from(home)
                .join("work")
                .join("common")
                .join("interchange")
                .join("chief_briefing.json");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn sanitize_relative_path(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    if s.starts_with("~/") || s == "—" || s.is_empty() {
        Some(s.to_string())
    } else {
        None
    }
}

fn load_external_briefing(data_dir: &Path) -> Option<Value> {
    let path = resolve_external_briefing_path(data_dir)?;
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn execute(
    ctx: &mut NodeContext,
    actor: &str,
    projects: &[String],
    all_projects: bool,
    briefing_read: bool,
    orders_submit: bool,
    orders_read: bool,
    name: &str,
    args: &Value,
    now: i64,
) -> Result<Value, &'static str> {
    let obj = args.as_object().ok_or("BOT_ARGUMENTS_INVALID")?;
    match name {
        "bob_briefing" => {
            if !briefing_read {
                return Err("BOT_SCOPE_DENIED");
            }
            check_allowed_keys(obj, &["project_id", "include_todos"])?;
            execute_briefing(ctx, actor, projects, all_projects, args, now)
        }
        "bob_order_submit" => {
            if !orders_submit {
                return Err("BOT_SCOPE_DENIED");
            }
            check_allowed_keys(
                obj,
                &[
                    "request_id",
                    "project_id",
                    "title",
                    "instruction",
                    "target_node",
                    "priority",
                    "deliverables",
                ],
            )?;
            execute_order_submit(
                &ctx.runtime.data_dir,
                actor,
                projects,
                all_projects,
                args,
                now,
            )
        }
        "bob_orders" => {
            if !orders_read {
                return Err("BOT_SCOPE_DENIED");
            }
            check_allowed_keys(obj, &["project_id", "order_id", "status", "offset"])?;
            execute_orders_list(
                &ctx.runtime.data_dir,
                projects,
                all_projects,
                args,
            )
        }
        _ => Err("BOT_SCOPE_DENIED"),
    }
}

fn execute_briefing(
    ctx: &NodeContext,
    actor: &str,
    projects: &[String],
    all_projects: bool,
    args: &Value,
    now: i64,
) -> Result<Value, &'static str> {
    let project_filter = parse_optional_project_filter(args, projects, all_projects)?;
    let include_todos = match args.get("include_todos") {
        None => true,
        Some(v) => v.as_bool().ok_or("BOT_ARGUMENTS_INVALID")?,
    };

    let catalog = ctx.app.catalog_snapshot().map_err(|_| "BOT_READ_FAILED")?;
    let mut merged_projects: BTreeMap<String, Value> = BTreeMap::new();

    for p in catalog.projects {
        if !is_project_authorized(projects, all_projects, &p.id) {
            continue;
        }
        if project_filter.is_some_and(|f| f != p.id) {
            continue;
        }
        merged_projects.insert(
            p.id.clone(),
            json!({
                "id": p.id,
                "title": p.title,
                "status": p.status,
                "updated_at": p.updated_at,
            }),
        );
    }

    let external_raw = load_external_briefing(&ctx.runtime.data_dir);
    let mut snapshot_meta = json!({ "available": false });
    if let Some(ext) = external_raw.as_ref() {
        let mut authorized_ext_count = 0usize;
        if let Some(arr) = ext.get("projects").and_then(Value::as_array) {
            for item in arr {
                let Some(id) = item.get("id").and_then(Value::as_str).map(str::trim) else {
                    continue;
                };
                if !is_project_authorized(projects, all_projects, id) {
                    continue;
                }
                if project_filter.is_some_and(|f| f != id) {
                    continue;
                }
                authorized_ext_count += 1;
                let entry = merged_projects.entry(id.to_string()).or_insert_with(|| {
                    json!({
                        "id": id,
                        "title": item.get("name").and_then(Value::as_str).unwrap_or(id),
                        "status": item.get("stage").and_then(Value::as_str).unwrap_or("dev"),
                    })
                });
                if let Some(obj) = entry.as_object_mut() {
                    if let Some(stage) = item.get("stage").and_then(Value::as_str) {
                        obj.insert("stage".into(), json!(stage));
                    }
                    if let Some(focus) = item.get("focus").and_then(Value::as_str) {
                        obj.insert("focus".into(), json!(focus));
                    }
                    if let Some(code_rel) =
                        sanitize_relative_path(item.get("local_code").and_then(Value::as_str))
                    {
                        obj.insert("local_code".into(), json!(code_rel));
                    }
                    if let Some(branch) = item.get("git_branch").and_then(Value::as_str) {
                        obj.insert("git_branch".into(), json!(branch));
                    }
                    if let Some(head) = item.get("git_head").and_then(Value::as_str) {
                        obj.insert("git_head".into(), json!(head));
                    }
                    if let Some(todo_count) = item.get("todo_count").and_then(Value::as_u64) {
                        obj.insert("briefing_todo_count".into(), json!(todo_count));
                    }
                    if let Some(top_todos) = item.get("top_todos").and_then(Value::as_array) {
                        obj.insert("top_todos".into(), json!(top_todos));
                    }
                }
            }
        }
        snapshot_meta = json!({
            "available": true,
            "generated_at": ext.get("generated_at").cloned().unwrap_or(Value::Null),
            "host": ext.get("host").cloned().unwrap_or(Value::Null),
            "authorized_projects_count": authorized_ext_count,
        });
    }

    let todos = if include_todos {
        let mut items = ctx
            .app
            .list_schedule_items()
            .map_err(|_| "BOT_READ_FAILED")?
            .into_iter()
            .filter(|r| {
                r.kind == "todo"
                    && !matches!(r.status.as_str(), "completed" | "done" | "cancelled")
                    && r.project_id.as_deref().is_some_and(|pid| {
                        is_project_authorized(projects, all_projects, pid)
                            && project_filter.is_none_or(|f| f == pid)
                    })
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        items
            .into_iter()
            .take(50)
            .map(|r| {
                json!({
                    "id": r.id,
                    "project_id": r.project_id,
                    "title": r.title,
                    "status": r.status,
                    "updated_at": r.updated_at,
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let recent_orders = {
        let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
        let store = load_store(&ctx.runtime.data_dir)?;
        let mut authorized_orders: Vec<ChiefOrder> = store
            .orders
            .into_iter()
            .filter(|o| {
                is_project_authorized(projects, all_projects, &o.project_id)
                    && project_filter.is_none_or(|f| f == o.project_id)
            })
            .collect();
        authorized_orders.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then(b.order_id.cmp(&a.order_id))
        });
        authorized_orders.truncate(10);
        authorized_orders
    };

    Ok(json!({
        "generated_at": now,
        "actor": actor,
        "projects": merged_projects.into_values().collect::<Vec<_>>(),
        "todos": todos,
        "recent_orders": recent_orders,
        "snapshot": snapshot_meta,
    }))
}

fn execute_order_submit(
    data_dir: &Path,
    actor: &str,
    projects: &[String],
    all_projects: bool,
    args: &Value,
    now: i64,
) -> Result<Value, &'static str> {
    let request_id = required_trimmed_string(args, "request_id", 128)?;
    let project_id = required_trimmed_string(args, "project_id", 128)?;
    if !is_project_authorized(projects, all_projects, &project_id) {
        return Err("BOT_SCOPE_DENIED");
    }
    let title = required_trimmed_string(args, "title", 256)?;
    let instruction = required_trimmed_string(args, "instruction", 16384)?;
    let target_node = match args.get("target_node") {
        None => "auto".to_string(),
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("BOT_ARGUMENTS_INVALID")?;
            if !ALLOWED_TARGET_NODES.contains(&s) {
                return Err("BOT_ARGUMENTS_INVALID");
            }
            s.to_string()
        }
    };
    let priority = match args.get("priority") {
        None => "normal".to_string(),
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("BOT_ARGUMENTS_INVALID")?;
            if !ALLOWED_PRIORITIES.contains(&s) {
                return Err("BOT_ARGUMENTS_INVALID");
            }
            s.to_string()
        }
    };
    let deliverables = match args.get("deliverables") {
        None => Vec::new(),
        Some(v) => {
            let arr = v.as_array().ok_or("BOT_ARGUMENTS_INVALID")?;
            if arr.len() > MAX_DELIVERABLES {
                return Err("BOT_ARGUMENTS_INVALID");
            }
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let s = item.as_str().map(str::trim).ok_or("BOT_ARGUMENTS_INVALID")?;
                if s.is_empty() || s.len() > 256 {
                    return Err("BOT_ARGUMENTS_INVALID");
                }
                out.push(s.to_string());
            }
            out
        }
    };

    let canonical = serde_json::to_vec(&json!([
        project_id,
        target_node,
        title,
        instruction,
        priority,
        deliverables
    ]))
    .map_err(|_| "CHIEF_STORE_ENCODE_FAILED")?;
    let payload_hash = sha256_hex(&canonical);

    let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
    let mut store = load_store(data_dir)?;
    if let Some(existing) = store
        .orders
        .iter()
        .find(|o| o.chief_actor == actor && o.request_id == request_id)
    {
        if existing.payload_hash == payload_hash {
            return Ok(json!({
                "ok": true,
                "replayed": true,
                "order": existing,
            }));
        }
        return Err("BOT_ORDER_REQUEST_CONFLICT");
    }

    let next_seq = store
        .orders
        .iter()
        .filter_map(|o| o.order_id.rsplit('-').next()?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let order_id = format!("ord-{now}-{next_seq}");
    let approval_id = format!("chief-order-{order_id}");
    let order = ChiefOrder {
        order_id,
        approval_id,
        request_id,
        payload_hash,
        chief_actor: actor.to_string(),
        project_id,
        target_node,
        title,
        instruction,
        priority,
        deliverables,
        status: "pending_approval".to_string(),
        created_at: now,
        updated_at: now,
        approved_at: None,
        rejected_at: None,
        linked_task_id: None,
        receipt: None,
    };
    store.orders.push(order.clone());
    store.version += 1;
    save_store(data_dir, &store)?;

    Ok(json!({
        "ok": true,
        "replayed": false,
        "order": order,
    }))
}

fn execute_orders_list(
    data_dir: &Path,
    projects: &[String],
    all_projects: bool,
    args: &Value,
) -> Result<Value, &'static str> {
    let project_filter = parse_optional_project_filter(args, projects, all_projects)?;
    let order_id_filter = match args.get("order_id") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("BOT_ARGUMENTS_INVALID")?;
            if s.is_empty() || s.len() > 128 {
                return Err("BOT_ARGUMENTS_INVALID");
            }
            Some(s.to_string())
        }
    };
    let status_filter = match args.get("status") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("BOT_ARGUMENTS_INVALID")?;
            if !ALLOWED_ORDER_STATUSES.contains(&s) {
                return Err("BOT_ARGUMENTS_INVALID");
            }
            Some(s.to_string())
        }
    };
    let offset = match args.get("offset") {
        None => 0,
        Some(v) => v
            .as_u64()
            .filter(|n| *n <= 100_000)
            .ok_or("BOT_ARGUMENTS_INVALID")? as usize,
    };

    let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
    let store = load_store(data_dir)?;
    let mut rows: Vec<ChiefOrder> = store
        .orders
        .into_iter()
        .filter(|o| {
            is_project_authorized(projects, all_projects, &o.project_id)
                && project_filter.is_none_or(|f| f == o.project_id)
                && order_id_filter
                    .as_deref()
                    .is_none_or(|id| id == o.order_id)
                && status_filter.as_deref().is_none_or(|st| st == o.status)
        })
        .collect();
    rows.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then(b.order_id.cmp(&a.order_id))
    });
    let total = rows.len();
    let page: Vec<ChiefOrder> = rows.into_iter().skip(offset).take(100).collect();
    let next_offset = if offset.saturating_add(100) < total {
        Some(offset + 100)
    } else {
        None
    };

    Ok(json!({
        "orders": page,
        "total": total,
        "next_offset": next_offset,
    }))
}

pub fn list_pending_approval_cards(data_dir: &Path) -> Result<Vec<Value>, &'static str> {
    let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
    let store = load_store(data_dir)?;
    let mut pending: Vec<&ChiefOrder> = store
        .orders
        .iter()
        .filter(|o| o.status == "pending_approval")
        .collect();
    pending.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(pending
        .into_iter()
        .map(|order| {
            let risk = if matches!(order.priority.as_str(), "urgent" | "high") {
                "high"
            } else {
                "medium"
            };
            let deliverables_text = if order.deliverables.is_empty() {
                "none".to_string()
            } else {
                order.deliverables.join(", ")
            };
            json!({
                "id": order.approval_id,
                "kind": "approval",
                "icon": "shield",
                "title": format!("[{}] {}", order.chief_actor, order.title),
                "summary": format!("Project {} -> {} ({})", order.project_id, order.target_node, order.priority),
                "riskLevel": risk,
                "risk_level": risk,
                "fields": [
                    {"label": "Chief", "value": order.chief_actor},
                    {"label": "Project", "value": order.project_id},
                    {"label": "Target Node", "value": order.target_node},
                    {"label": "Priority", "value": order.priority},
                    {"label": "Deliverables", "value": deliverables_text},
                ],
                "technicalDetail": order.instruction,
                "technical_detail": order.instruction,
                "status": "pending",
                "created_at": order.created_at,
                "expires_at": Value::Null,
            })
        })
        .collect())
}

pub fn approve_order_by_approval_id(
    data_dir: &Path,
    approval_id: &str,
    mut app: Option<&mut LocalApp>,
    now: i64,
) -> Result<Value, &'static str> {
    let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
    let mut store = load_store(data_dir)?;
    let pos = store
        .orders
        .iter()
        .position(|o| o.approval_id == approval_id)
        .ok_or("CHIEF_ORDER_NOT_FOUND")?;
    if store.orders[pos].status != "pending_approval" {
        return Err("CHIEF_ORDER_NOT_PENDING");
    }

    let order_id = store.orders[pos].order_id.clone();
    let project_id = store.orders[pos].project_id.clone();
    let title = store.orders[pos].title.clone();
    let instruction = store.orders[pos].instruction.clone();

    let linked_task_id = app.as_deref_mut().and_then(|local_app| {
        let todo_id = format!("chief-todo-{order_id}");
        local_app
            .create_schedule_item_with_kind(
                &todo_id,
                Some(&project_id),
                None,
                &title,
                "todo",
                0,
                None,
                None,
                &instruction,
                now,
            )
            .ok()
            .map(|item| item.id)
    });

    let order = &mut store.orders[pos];
    order.status = "approved".to_string();
    order.approved_at = Some(now);
    order.updated_at = now;
    if linked_task_id.is_some() {
        order.linked_task_id = linked_task_id;
    }
    let updated_order = order.clone();
    store.version += 1;
    save_store(data_dir, &store)?;

    Ok(json!({
        "ok": true,
        "approvalId": approval_id,
        "reply": "",
        "cards": [],
        "order": updated_order,
    }))
}

pub fn reject_order_by_approval_id(
    data_dir: &Path,
    approval_id: &str,
    now: i64,
) -> Result<Value, &'static str> {
    let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
    let mut store = load_store(data_dir)?;
    let pos = store
        .orders
        .iter()
        .position(|o| o.approval_id == approval_id)
        .ok_or("CHIEF_ORDER_NOT_FOUND")?;
    if store.orders[pos].status != "pending_approval" {
        return Err("CHIEF_ORDER_NOT_PENDING");
    }
    let order = &mut store.orders[pos];
    order.status = "rejected".to_string();
    order.rejected_at = Some(now);
    order.updated_at = now;
    store.version += 1;
    save_store(data_dir, &store)?;

    Ok(json!({
        "ok": true,
        "approvalId": approval_id,
        "reply": "",
    }))
}

pub fn list_owner_orders(data_dir: &Path, args: &Value) -> Result<Value, &'static str> {
    let obj = match args {
        Value::Null => serde_json::Map::new(),
        Value::Object(m) => m.clone(),
        _ => return Err("CHIEF_ARGUMENTS_INVALID"),
    };
    if obj
        .keys()
        .any(|k| !matches!(k.as_str(), "project_id" | "status" | "chief_actor" | "offset"))
    {
        return Err("CHIEF_ARGUMENTS_INVALID");
    }
    let project_filter = obj
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let status_filter = match obj.get("status") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("CHIEF_ARGUMENTS_INVALID")?;
            if !ALLOWED_ORDER_STATUSES.contains(&s) {
                return Err("CHIEF_ARGUMENTS_INVALID");
            }
            Some(s)
        }
    };
    let actor_filter = obj
        .get("chief_actor")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let offset = match obj.get("offset") {
        None => 0,
        Some(v) => v
            .as_u64()
            .filter(|n| *n <= 100_000)
            .ok_or("CHIEF_ARGUMENTS_INVALID")? as usize,
    };

    let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
    let store = load_store(data_dir)?;
    let mut rows: Vec<ChiefOrder> = store
        .orders
        .into_iter()
        .filter(|o| {
            project_filter.is_none_or(|p| p == o.project_id)
                && status_filter.is_none_or(|st| st == o.status)
                && actor_filter.is_none_or(|a| a == o.chief_actor)
        })
        .collect();
    rows.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then(b.order_id.cmp(&a.order_id))
    });
    let total = rows.len();
    let page: Vec<ChiefOrder> = rows.into_iter().skip(offset).take(100).collect();
    let next_offset = if offset.saturating_add(100) < total {
        Some(offset + 100)
    } else {
        None
    };

    Ok(json!({
        "orders": page,
        "total": total,
        "next_offset": next_offset,
    }))
}

pub fn record_order_receipt(
    data_dir: &Path,
    args: &Value,
    now: i64,
) -> Result<Value, &'static str> {
    let obj = args.as_object().ok_or("CHIEF_ARGUMENTS_INVALID")?;
    if obj.keys().any(|k| {
        !matches!(
            k.as_str(),
            "order_id" | "status" | "summary" | "artifacts" | "worker_node"
        )
    }) {
        return Err("CHIEF_ARGUMENTS_INVALID");
    }
    let order_id = obj
        .get("order_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or("CHIEF_ARGUMENTS_INVALID")?;
    let next_status = match obj.get("status") {
        None => "completed",
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("CHIEF_ARGUMENTS_INVALID")?;
            if !matches!(s, "completed" | "failed" | "running") {
                return Err("CHIEF_ARGUMENTS_INVALID");
            }
            s
        }
    };
    let summary = match obj.get("summary") {
        None if next_status == "running" => String::new(),
        None => return Err("CHIEF_ARGUMENTS_INVALID"),
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("CHIEF_ARGUMENTS_INVALID")?;
            if (next_status != "running" && s.is_empty()) || s.len() > 16384 {
                return Err("CHIEF_ARGUMENTS_INVALID");
            }
            s.to_string()
        }
    };
    let worker_node = match obj.get("worker_node") {
        None => "x1t".to_string(),
        Some(v) => {
            let s = v.as_str().map(str::trim).ok_or("CHIEF_ARGUMENTS_INVALID")?;
            if s.is_empty() || s.len() > 64 {
                return Err("CHIEF_ARGUMENTS_INVALID");
            }
            s.to_string()
        }
    };
    let artifacts = match obj.get("artifacts") {
        None => Vec::new(),
        Some(v) => {
            let arr = v.as_array().ok_or("CHIEF_ARGUMENTS_INVALID")?;
            if arr.len() > MAX_ARTIFACTS {
                return Err("CHIEF_ARGUMENTS_INVALID");
            }
            let mut seen = HashSet::new();
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let s = item.as_str().map(str::trim).ok_or("CHIEF_ARGUMENTS_INVALID")?;
                if s.is_empty() || s.len() > 512 {
                    return Err("CHIEF_ARGUMENTS_INVALID");
                }
                if seen.insert(s.to_string()) {
                    out.push(s.to_string());
                }
            }
            out
        }
    };

    let _guard = store_mutex().lock().map_err(|_| "CHIEF_STORE_LOCKED")?;
    let mut store = load_store(data_dir)?;
    let pos = store
        .orders
        .iter()
        .position(|o| o.order_id == order_id)
        .ok_or("CHIEF_ORDER_NOT_FOUND")?;
    if !matches!(store.orders[pos].status.as_str(), "approved" | "running") {
        return Err("CHIEF_ORDER_NOT_APPROVED");
    }

    let order = &mut store.orders[pos];
    order.status = next_status.to_string();
    order.updated_at = now;
    order.receipt = Some(ChiefOrderReceipt {
        status: next_status.to_string(),
        summary,
        artifacts,
        worker_node,
        recorded_at: now,
    });
    let updated = order.clone();
    store.version += 1;
    save_store(data_dir, &store)?;

    Ok(json!({
        "ok": true,
        "order": updated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bot_access::{self, Grant, Session},
        hash_token, jarvis, spawn_node_server, BobNodeConfig,
    };
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpStream},
    };

    const MUSE_TOKEN: &str = "bob_bot_muse_test_only_not_a_real_credential";
    const GROK_TOKEN: &str = "bob_bot_grok_test_only_not_a_real_credential";

    fn make_grant(
        username: &str,
        token: &str,
        projects: Vec<&str>,
        revoked: bool,
        briefing_read: bool,
        orders_submit: bool,
        orders_read: bool,
    ) -> Grant {
        Grant {
            username: username.to_string(),
            token_hash: hash_token(token),
            expires_at: 9_999_999_999_999,
            no_expiry: false,
            revoked,
            projects: projects.into_iter().map(String::from).collect(),
            all_projects: false,
            sessions: vec![Session {
                connection_id: "x1t".to_string(),
                harness: "dsh".to_string(),
                session_id: "sess-1".to_string(),
            }],
            organize: false,
            todos_read: false,
            todos_create: false,
            todos_unassigned: false,
            todos_update: false,
            briefing_read,
            orders_submit,
            orders_read,
        }
    }

    fn setup_ctx() -> (tempfile::TempDir, NodeContext) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = BobNodeConfig {
            bind_addr: "127.0.0.1:0".into(),
            data_dir: dir.path().into(),
            static_dir: None,
            legacy_db_path: None,
        };
        let mut ctx = NodeContext::open(&cfg).unwrap();
        ctx.app.create_project("allowed", "Allowed Project", 1).unwrap();
        ctx.app.create_project("private", "Private Project", 1).unwrap();
        ctx.app
            .create_schedule_item_with_kind(
                "todo-allowed-1",
                Some("allowed"),
                None,
                "Verify remote X1T sync",
                "todo",
                0,
                None,
                None,
                "Check git HEAD on X1T",
                100,
            )
            .unwrap();
        ctx.app
            .create_schedule_item_with_kind(
                "todo-private-1",
                Some("private"),
                None,
                "Secret private task",
                "todo",
                0,
                None,
                None,
                "Must never leak",
                100,
            )
            .unwrap();
        ctx.app
            .create_schedule_item_with_kind(
                "todo-unassigned-1",
                None,
                None,
                "Unassigned owner reminder",
                "todo",
                0,
                None,
                None,
                "Must not appear in briefing",
                100,
            )
            .unwrap();

        fs::write(
            dir.path().join("chief_briefing.json"),
            serde_json::to_vec_pretty(&json!({
                "generated_at": "2026-11-16T03:00:00Z",
                "host": "vps2-test",
                "projects": [
                    {
                        "id": "allowed",
                        "name": "Allowed Project",
                        "stage": "dev",
                        "local_code": "~/work/code/allowed",
                        "git_branch": "main",
                        "git_head": "abc1234",
                        "todo_count": 1,
                        "top_todos": ["Verify remote X1T sync"]
                    },
                    {
                        "id": "private",
                        "name": "Private Project",
                        "stage": "dev",
                        "local_code": "C:/secret/private",
                        "todo_count": 1
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let grants = vec![
            make_grant("meta_muse", MUSE_TOKEN, vec!["allowed"], false, true, true, true),
            make_grant("grok_bot", GROK_TOKEN, vec!["allowed"], false, true, true, true),
        ];
        fs::write(
            dir.path().join("bot-grants.json"),
            serde_json::to_vec(&grants).unwrap(),
        )
        .unwrap();

        (dir, ctx)
    }

    fn mcp_rpc(ctx: &mut NodeContext, token: &str, method: &str, params: Value, now: i64) -> (u16, Value) {
        bot_access::handle(
            ctx,
            Some(&format!("Bearer {token}")),
            &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
            now,
        )
    }

    fn call_tool(ctx: &mut NodeContext, token: &str, name: &str, args: Value, now: i64) -> Value {
        mcp_rpc(
            ctx,
            token,
            "tools/call",
            json!({"name": name, "arguments": args}),
            now,
        )
        .1
    }

    fn parse_tool_json(resp: &Value) -> Value {
        assert_eq!(resp["result"]["isError"], false, "unexpected tool error: {resp}");
        let txt = resp["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(txt).unwrap()
    }

    fn http_post(addr: SocketAddr, path: &str, token: Option<&str>, body: &str) -> (u16, Value) {
        let mut stream = TcpStream::connect(addr).expect("connect test server");
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        let (hdr, json_body) = raw.split_once("\r\n\r\n").unwrap();
        let status: u16 = hdr
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let parsed = serde_json::from_str(json_body).unwrap_or_else(|_| json!({ "raw": json_body }));
        (status, parsed)
    }

    #[test]
    fn multi_chief_pluggable_lifecycle_with_human_approval() {
        let (dir, mut ctx) = setup_ctx();

        // 1. Muse calls bob_briefing -> sees "allowed" & its todo only; never sees "private" or unassigned
        let briefing_resp = call_tool(&mut ctx, MUSE_TOKEN, "bob_briefing", json!({}), 1000);
        let briefing = parse_tool_json(&briefing_resp);
        let project_ids: Vec<&str> = briefing["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap())
            .collect();
        assert_eq!(project_ids, vec!["allowed"]);
        assert_eq!(briefing["projects"][0]["git_head"], "abc1234");
        assert_eq!(briefing["snapshot"]["authorized_projects_count"], 1);
        let todo_titles: Vec<&str> = briefing["todos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap())
            .collect();
        assert_eq!(todo_titles, vec!["Verify remote X1T sync"]);

        // 2. Muse submits an order via bob_order_submit -> enters pending_approval
        let submit_resp = call_tool(
            &mut ctx,
            MUSE_TOKEN,
            "bob_order_submit",
            json!({
                "request_id": "muse-req-1",
                "project_id": "allowed",
                "target_node": "x1t",
                "title": "Run X1T integration check",
                "instruction": "Pull main on X1T and verify unit tests pass.",
                "priority": "high",
                "deliverables": ["test report", "git commit sha"]
            }),
            2000,
        );
        let submitted = parse_tool_json(&submit_resp);
        assert_eq!(submitted["replayed"], false);
        assert_eq!(submitted["order"]["status"], "pending_approval");
        assert_eq!(submitted["order"]["chief_actor"], "meta_muse");
        let order_id = submitted["order"]["order_id"].as_str().unwrap().to_string();
        let approval_id = submitted["order"]["approval_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(approval_id.starts_with("chief-order-"));

        // 3. Jarvis approval list surfaces the chief-order card
        let cards = jarvis::approvals_list(dir.path(), &json!({})).unwrap();
        let arr = cards.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], approval_id);
        assert_eq!(arr[0]["riskLevel"], "high");

        // 4. Owner approves via jarvis::approve_with_ctx -> card removed, status=approved, kernel todo linked
        let approved = jarvis::approve_with_ctx(
            &mut ctx,
            &json!({ "approvalId": approval_id }),
        )
        .unwrap();
        assert_eq!(approved["ok"], true);
        assert_eq!(approved["order"]["status"], "approved");
        assert!(approved["order"]["linked_task_id"].as_str().is_some());
        assert_eq!(
            jarvis::approvals_list(dir.path(), &json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            0
        );

        // 5. Owner/worker executes and records completion receipt via app_chief_order_complete
        let completed = record_order_receipt(
            dir.path(),
            &json!({
                "order_id": order_id,
                "status": "completed",
                "summary": "X1T pulled main and all checks passed.",
                "artifacts": ["docs/CURRENT_STAGE.md", "abc1234"],
                "worker_node": "x1t"
            }),
            3000,
        )
        .unwrap();
        assert_eq!(completed["order"]["status"], "completed");
        assert_eq!(completed["order"]["receipt"]["worker_node"], "x1t");

        // Owner inspection RPC returns the completed order
        let owner_list = list_owner_orders(dir.path(), &json!({ "status": "completed" })).unwrap();
        assert_eq!(owner_list["total"], 1);
        assert_eq!(owner_list["orders"][0]["order_id"], order_id);

        // 6. Grok (independent Chief of Staff) queries bob_orders & bob_briefing and sees Muse's receipt
        let grok_orders = parse_tool_json(&call_tool(
            &mut ctx,
            GROK_TOKEN,
            "bob_orders",
            json!({ "project_id": "allowed" }),
            4000,
        ));
        assert_eq!(grok_orders["total"], 1);
        assert_eq!(grok_orders["orders"][0]["chief_actor"], "meta_muse");
        assert_eq!(
            grok_orders["orders"][0]["receipt"]["summary"],
            "X1T pulled main and all checks passed."
        );

        let grok_briefing = parse_tool_json(&call_tool(
            &mut ctx,
            GROK_TOKEN,
            "bob_briefing",
            json!({}),
            4100,
        ));
        assert_eq!(
            grok_briefing["recent_orders"][0]["status"],
            "completed"
        );
    }

    #[test]
    fn revoking_chief_grant_stops_access_without_affecting_other_chiefs_or_orders() {
        let (dir, mut ctx) = setup_ctx();

        // Muse creates an order and owner rejects vs approves cleanly
        let sub1 = parse_tool_json(&call_tool(
            &mut ctx,
            MUSE_TOKEN,
            "bob_order_submit",
            json!({
                "request_id": "muse-keep-1",
                "project_id": "allowed",
                "title": "Muse historical order",
                "instruction": "Preserve this audit trail after Muse is unsubscribed."
            }),
            1500,
        ));
        let approval_id = sub1["order"]["approval_id"].as_str().unwrap();
        jarvis::approve_with_ctx(&mut ctx, &json!({ "approvalId": approval_id })).unwrap();

        // Simulate unsubscribing Muse (revoked = true in bot-grants.json)
        let path = dir.path().join("bot-grants.json");
        let mut grants: Vec<Grant> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        grants[0].revoked = true;
        fs::write(&path, serde_json::to_vec(&grants).unwrap()).unwrap();

        // Muse's token is immediately rejected across MCP endpoints
        let (st_list, err_list) = mcp_rpc(&mut ctx, MUSE_TOKEN, "tools/list", json!({}), 2000);
        assert_eq!(st_list, 401);
        assert_eq!(err_list["error"], "BOT_AUTH_REQUIRED");
        for tool in ["bob_briefing", "bob_order_submit", "bob_orders"] {
            let (st, err) = mcp_rpc(
                &mut ctx,
                MUSE_TOKEN,
                "tools/call",
                json!({"name": tool, "arguments": {}}),
                2000,
            );
            assert_eq!(st, 401);
            assert_eq!(err["error"], "BOT_AUTH_REQUIRED");
        }

        // Grok keeps full capability and sees Muse's historical order untouched
        let grok_orders = parse_tool_json(&call_tool(
            &mut ctx,
            GROK_TOKEN,
            "bob_orders",
            json!({}),
            2100,
        ));
        assert_eq!(grok_orders["total"], 1);
        assert_eq!(grok_orders["orders"][0]["title"], "Muse historical order");

        // Grok can also submit and owner can reject an order cleanly
        let grok_sub = parse_tool_json(&call_tool(
            &mut ctx,
            GROK_TOKEN,
            "bob_order_submit",
            json!({
                "request_id": "grok-req-1",
                "project_id": "allowed",
                "title": "Follow-up task from Grok",
                "instruction": "Continue operating without Muse."
            }),
            2200,
        ));
        let grok_appr_id = grok_sub["order"]["approval_id"].as_str().unwrap();
        let rej = jarvis::reject_with_ctx(&mut ctx, &json!({ "approvalId": grok_appr_id })).unwrap();
        assert_eq!(rej["ok"], true);
        let grok_after = parse_tool_json(&call_tool(
            &mut ctx,
            GROK_TOKEN,
            "bob_orders",
            json!({ "status": "rejected" }),
            2300,
        ));
        assert_eq!(grok_after["total"], 1);
    }

    #[test]
    fn order_idempotency_conflict_scope_and_owner_rpc_isolation() {
        let (dir, mut ctx) = setup_ctx();

        let valid_args = json!({
            "request_id": "idem-1",
            "project_id": "allowed",
            "title": "Build status snapshot",
            "instruction": "Collect active build checks."
        });
        let first = parse_tool_json(&call_tool(
            &mut ctx,
            MUSE_TOKEN,
            "bob_order_submit",
            valid_args.clone(),
            1000,
        ));
        assert_eq!(first["replayed"], false);
        let order_id = first["order"]["order_id"].as_str().unwrap().to_string();

        // Identical request_id + payload -> replayed: true, same order_id
        let replay = parse_tool_json(&call_tool(
            &mut ctx,
            MUSE_TOKEN,
            "bob_order_submit",
            valid_args,
            1005,
        ));
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["order"]["order_id"], order_id);

        // Same request_id + modified payload -> BOT_ORDER_REQUEST_CONFLICT
        let conflict = call_tool(
            &mut ctx,
            MUSE_TOKEN,
            "bob_order_submit",
            json!({
                "request_id": "idem-1",
                "project_id": "allowed",
                "title": "Tampered title",
                "instruction": "Different body."
            }),
            1010,
        );
        assert_eq!(conflict["result"]["isError"], true);
        assert_eq!(
            conflict["result"]["content"][0]["text"],
            "BOT_ORDER_REQUEST_CONFLICT"
        );

        // Out-of-scope project_id & unknown key are rejected
        let out_of_scope = call_tool(
            &mut ctx,
            MUSE_TOKEN,
            "bob_order_submit",
            json!({
                "request_id": "bad-proj",
                "project_id": "private",
                "title": "Cross-scope",
                "instruction": "Should fail."
            }),
            1020,
        );
        assert_eq!(out_of_scope["result"]["isError"], true);

        let unknown_key = call_tool(
            &mut ctx,
            MUSE_TOKEN,
            "bob_briefing",
            json!({ "unexpected": true }),
            1030,
        );
        assert_eq!(unknown_key["result"]["isError"], true);

        // Unapproved pending order cannot receive a completion receipt
        assert_eq!(
            record_order_receipt(
                dir.path(),
                &json!({ "order_id": order_id, "summary": "premature" }),
                1040,
            ),
            Err("CHIEF_ORDER_NOT_APPROVED")
        );

        // Token with orders_submit = false omits bob_order_submit from tools/list and denies call
        let readonly_tok = "bob_bot_readonly_test_only_not_a_credential";
        let grants_path = dir.path().join("bot-grants.json");
        let mut grants: Vec<Grant> =
            serde_json::from_slice(&fs::read(&grants_path).unwrap()).unwrap();
        grants.push(make_grant(
            "reader_bot",
            readonly_tok,
            vec!["allowed"],
            false,
            true,
            false,
            true,
        ));
        fs::write(&grants_path, serde_json::to_vec(&grants).unwrap()).unwrap();

        let (st, list_resp) = mcp_rpc(&mut ctx, readonly_tok, "tools/list", json!({}), 1050);
        assert_eq!(st, 200);
        let tool_names: Vec<&str> = list_resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(tool_names.contains(&"bob_briefing"));
        assert!(tool_names.contains(&"bob_orders"));
        assert!(!tool_names.contains(&"bob_order_submit"));

        let denied_submit = call_tool(
            &mut ctx,
            readonly_tok,
            "bob_order_submit",
            json!({
                "request_id": "ro-1",
                "project_id": "allowed",
                "title": "Should fail",
                "instruction": "Readonly token cannot submit."
            }),
            1060,
        );
        assert_eq!(denied_submit["result"]["isError"], true);
        drop(ctx);

        // Live HTTP isolation: bob_bot_* token works on /api/bot/v1/mcp, gets 403 BOT_SCOPE_DENIED on /api/rpc/*
        let srv = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".into(),
            data_dir: dir.path().into(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();
        let (mcp_st, mcp_body) = http_post(
            srv.local_addr,
            "/api/bot/v1/mcp",
            Some(MUSE_TOKEN),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"bob_orders","arguments":{}}}"#,
        );
        assert_eq!(mcp_st, 200);
        assert_eq!(mcp_body["result"]["isError"], false);

        for rpc_path in [
            "/api/rpc/app_chief_order_complete",
            "/api/rpc/app_chief_orders_list",
        ] {
            let (st_rpc, err_rpc) = http_post(
                srv.local_addr,
                rpc_path,
                Some(MUSE_TOKEN),
                r#"{"order_id":"ord-1","summary":"bypass"}"#,
            );
            assert_eq!(st_rpc, 403);
            assert_eq!(err_rpc["error"], "BOT_SCOPE_DENIED");
        }
        srv.shutdown();
    }
}
