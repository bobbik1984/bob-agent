use serde::{Deserialize, Serialize};
use tauri::{AppHandle, command, Manager, Emitter};
use std::sync::{Arc, RwLock};
use log::{info, error};
use std::collections::HashMap;

use std::path::PathBuf;
use std::fs;

use crate::lan_sync::LanSyncEngine;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConnectedDevice {
    pub device_id: String,
    pub platform: String,
    pub ip_address: String,
    pub last_seen: i64,
    #[serde(default)]
    pub device_name: Option<String>,
}

#[derive(Default)]
pub struct DeviceRegistry {
    pub devices: RwLock<HashMap<String, ConnectedDevice>>,
}

impl DeviceRegistry {
    pub fn load() -> Self {
        let path = crate::get_data_dir().join("device_registry.json");
        let devices = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str(&data).ok())
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        Self {
            devices: RwLock::new(devices),
        }
    }

    pub fn save(&self) {
        let path = crate::get_data_dir().join("device_registry.json");
        if let Ok(devices) = self.devices.read() {
            if let Ok(json) = serde_json::to_string_pretty(&*devices) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn update_device(&self, device: ConnectedDevice) {
        {
            let mut devices = self.devices.write().unwrap();
            devices.insert(device.device_id.clone(), device);
        }
        self.save();
    }
    
    pub fn get_all(&self) -> Vec<ConnectedDevice> {
        let devices = self.devices.read().unwrap();
        let mut list: Vec<_> = devices.values().cloned().collect();
        // Sort by last_seen descending
        list.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        list
    }
}

#[command]
pub async fn get_connected_devices(app: AppHandle) -> Result<Vec<ConnectedDevice>, String> {
    let registry = app.state::<Arc<DeviceRegistry>>();
    Ok(registry.get_all())
}

#[command]
pub async fn disconnect_device(app: AppHandle, device_id: String) -> Result<(), String> {
    let registry = app.state::<Arc<DeviceRegistry>>();
    {
        let mut devices = registry.devices.write().unwrap();
        devices.remove(&device_id);
    }
    registry.save();
    let _ = app.emit("sync:device_disconnected", device_id);
    Ok(())
}

pub fn register_device(app: &AppHandle, headers: &axum::http::HeaderMap, ip: std::net::SocketAddr) {
    if let (Some(device_id), Some(platform)) = (
        headers.get("x-device-id").and_then(|v| v.to_str().ok()),
        headers.get("x-platform").and_then(|v| v.to_str().ok()),
    ) {
        let registry = app.state::<Arc<DeviceRegistry>>();
        let device_name = headers.get("x-device-name")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let device = ConnectedDevice {
            device_id: device_id.to_string(),
            platform: platform.to_string(),
            ip_address: ip.ip().to_string(),
            last_seen: crate::now_ms(),
            device_name,
        };
        registry.update_device(device.clone());
        let _ = app.emit("sync:device_connected", device);
    }
}

fn get_mobile_outbox_path() -> PathBuf {
    crate::get_data_dir().join("mobile_outbox.json")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncCommandPayload {
    pub device_id: String,
    pub public_key: String,
    pub local_ips: Vec<String>,
    pub port: u16,
    pub relay: String,
    #[serde(default)]
    pub listen_only: bool,
}

#[command]
pub async fn trigger_mobile_sync(app: AppHandle, payload: SyncCommandPayload) -> Result<(), String> {
    info!("[Sync Engine] trigger_mobile_sync called, listen_only: {}", payload.listen_only);

    if payload.listen_only {
        let lan_engine = app.state::<Arc<LanSyncEngine>>();
        let target_device_id = payload.device_id.clone();
        let payload_clone = payload.clone();
        let app_clone = app.clone();
        
        lan_engine.start_listen_broadcast(move |discovered_id, ip, port| {
            if discovered_id == target_device_id {
                info!("[Sync Engine] Discovered paired PC at {}:{}, initiating active sync!", ip, port);
                let mut active_payload = payload_clone.clone();
                active_payload.listen_only = false;
                active_payload.local_ips = vec![ip];
                active_payload.port = port;
                
                let app_for_task = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = do_active_sync(app_for_task, active_payload).await {
                        error!("[Sync Engine] Active sync failed: {}", e);
                    }
                });
            }
        });
        return Ok(());
    }

    do_active_sync(app, payload).await.map_err(|e| e.to_string())
}

#[command]
pub async fn write_mobile_outbox(_app: AppHandle, operations: Vec<serde_json::Value>) -> Result<(), String> {
    let path = get_mobile_outbox_path();
    let mut outbox: Vec<serde_json::Value> = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    outbox.extend(operations);

    let data = serde_json::to_string_pretty(&outbox).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())?;
    
    info!("[Sync Engine] Appended to mobile outbox, total items: {}", outbox.len());
    Ok(())
}

#[command]
pub async fn relay_handshake(app: AppHandle, target_device_id: String, auth_code: String) -> Result<(), String> {
    let config = crate::read_config();
    let my_device_id = config.get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let my_device_name = config.get("deviceName").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let platform = std::env::consts::OS.to_string();
    
    let relay_url = "wss://relay.bobbik.org".to_string();
    let ws_url = format!("{}/ws/device/{}", relay_url, url_encode_device_id(&my_device_id));

    // ── Stage 3a: Connect to Relay server ──
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_connect", "status": "running"}));
    let (mut ws_stream, _) = match tokio::time::timeout(tokio::time::Duration::from_secs(15), connect_websocket_robust(&ws_url)).await {
        Ok(Ok(stream)) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_connect", "status": "done"}));
            stream
        }
        Ok(Err(e)) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_connect", "status": "error", "detail": format!("ERR-PAIRING-01: 连接拒绝: {}", e)}));
            return Err(format!("ERR-PAIRING-01: Failed to connect to relay: {}", e));
        }
        Err(_) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_connect", "status": "error", "detail": "ERR-PAIRING-01: 连接超时 (15s)"}));
            return Err("ERR-PAIRING-01: Failed to connect to relay: Timeout".to_string());
        }
    };
    
    // Explicitly register device ID (fixes NGINX URL stripping bugs and ensures we are active)
    let reg_msg = serde_json::json!({
        "type": "register",
        "deviceId": my_device_id
    });
    let _ = ws_stream.send(Message::Text(reg_msg.to_string().into())).await;

    // ── Stage 3b: Send notify to PC via Relay ──
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_notify", "status": "running"}));
    let msg = serde_json::json!({
        "type": "notify",
        "target_device_id": target_device_id,
        "payload": {
            "device_name": my_device_name,
            "platform": platform,
            "auth_code": auth_code
        }
    });
    match ws_stream.send(Message::Text(msg.to_string().into())).await {
        Ok(_) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_notify", "status": "done"}));
        }
        Err(e) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_notify", "status": "error", "detail": format!("ERR-PAIRING-02: {}", e)}));
            return Err(format!("ERR-PAIRING-02: {}", e.to_string()));
        }
    }

    // ── Stage 3c: Wait for PC ack ──
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "running"}));
    let timeout = tokio::time::Duration::from_secs(10);
    let ack_task = async {
        while let Some(msg) = ws_stream.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if json.get("type").and_then(|v| v.as_str()) == Some("ack") {
                        if let Some(error_msg) = json.get("error").and_then(|v| v.as_str()) {
                            return Err(format!("Relay error: {}", error_msg));
                        }
                        return Ok(());
                    }
                    if let Some(error_msg) = json.get("error").and_then(|v| v.as_str()) {
                        return Err(format!("Relay error: {}", error_msg));
                    }
                }
            }
        }
        Err("Connection closed before ack".to_string())
    };

    match tokio::select! {
        res = ack_task => res,
        _ = tokio::time::sleep(timeout) => {
            Err("Handshake timeout: PC did not respond within 10s.".to_string())
        }
    } {
        Ok(()) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "done"}));
            Ok(())
        }
        Err(e) => {
            if e.contains("Unauthorized") {
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "error", "detail": "ERR-PAIRING-04: 鉴权失败 (认证码不匹配)"}));
                Err("ERR-PAIRING-04: 鉴权失败 (认证码不匹配)".to_string())
            } else {
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "error", "detail": format!("ERR-PAIRING-03: {}", e)}));
                Err(e)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncData {
    pub config: serde_json::Value,
    pub settings: Vec<serde_json::Value>,
    pub conversations: Vec<serde_json::Value>,
    pub messages: Vec<serde_json::Value>,
    pub events: Vec<serde_json::Value>,
    pub cron_jobs: Vec<serde_json::Value>,
    pub kg_nodes: Vec<serde_json::Value>,
    pub kg_edges: Vec<serde_json::Value>,
    pub wiki_fts: Vec<serde_json::Value>,
    #[serde(default)]
    pub tombstones: Vec<serde_json::Value>,
}

pub fn export_sync_data(app: &AppHandle, since_ts: i64) -> Result<SyncData, String> {
    let config = crate::read_config();
    let db = app.state::<crate::db::DbState>();
    let conn = db.0.lock().map_err(|_| "Failed to lock db")?;
    
    let extract = |query: &str, params: &[&dyn rusqlite::ToSql], cols: &[&str]| -> Result<Vec<serde_json::Value>, String> {
        let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params, |row| {
            let mut map = serde_json::Map::new();
            for (i, col) in cols.iter().enumerate() {
                let val: Result<String, _> = row.get(i);
                if let Ok(v) = val {
                    map.insert(col.to_string(), serde_json::Value::String(v));
                } else if let Ok(v) = row.get::<_, i64>(i) {
                    map.insert(col.to_string(), serde_json::json!(v));
                } else if let Ok(v) = row.get::<_, f64>(i) {
                    map.insert(col.to_string(), serde_json::json!(v));
                } else {
                    map.insert(col.to_string(), serde_json::Value::Null);
                }
            }
            Ok(serde_json::Value::Object(map))
        }).map_err(|e| e.to_string())?;
        
        let mut result = Vec::new();
        for r in rows {
            if let Ok(v) = r { result.push(v); }
        }
        Ok(result)
    };

    let settings = extract("SELECT key, value FROM settings", &[], &["key", "value"]).unwrap_or_default();
    let conversations = extract("SELECT id, title, model, cost, last_message, last_role, created_at, updated_at FROM conversations WHERE updated_at >= ?1", &[&since_ts], 
        &["id", "title", "model", "cost", "last_message", "last_role", "created_at", "updated_at"]).unwrap_or_default();
    let messages = extract("SELECT id, conversation_id, role, content, image_base64, created_at, from_channel, sync_id FROM messages WHERE created_at >= ?1", &[&since_ts], 
        &["id", "conversation_id", "role", "content", "image_base64", "created_at", "from_channel", "sync_id"]).unwrap_or_default();
    let events = extract("SELECT id, title, type, status, date, start_time, end_time, description, created_at, linked_ticket_id FROM events", &[], 
        &["id", "title", "type", "status", "date", "start_time", "end_time", "description", "created_at", "linked_ticket_id"]).unwrap_or_default();
    let cron_jobs = extract("SELECT id, title, cron_expr, prompt_template, enabled, last_run, created_at FROM cron_jobs", &[], 
        &["id", "title", "cron_expr", "prompt_template", "enabled", "last_run", "created_at"]).unwrap_or_default();
    let kg_nodes = extract("SELECT id, label, node_type, summary, source, metadata, created_at FROM kg_nodes", &[], 
        &["id", "label", "node_type", "summary", "source", "metadata", "created_at"]).unwrap_or_default();
    let kg_edges = extract("SELECT source_id, target_id, relation, confidence, created_at FROM kg_edges", &[], 
        &["source_id", "target_id", "relation", "confidence", "created_at"]).unwrap_or_default();
    let wiki_fts = Vec::new();
    let tombstones = extract("SELECT table_name, record_key, deleted_at FROM sync_tombstones WHERE deleted_at >= ?1", &[&since_ts],
        &["table_name", "record_key", "deleted_at"]).unwrap_or_default();

    Ok(SyncData { config, settings, conversations, messages, events, cron_jobs, kg_nodes, kg_edges, wiki_fts, tombstones })
}

pub fn import_sync_data(app: &AppHandle, data: SyncData, last_sync_ts: i64) -> Result<(), String> {
    crate::write_config(&data.config);
    let db = app.state::<crate::db::DbState>();
    let conn = db.0.lock().map_err(|_| "Failed to lock db")?;
    let ts = crate::now_ms();

    // 1. Process Tombstones FIRST (Physical Deletion)
    if !data.tombstones.is_empty() {
        for t in &data.tombstones {
            if let Some(obj) = t.as_object() {
                let table = obj.get("table_name").and_then(|v| v.as_str()).unwrap_or("");
                let record_key = obj.get("record_key").and_then(|v| v.as_str()).unwrap_or("");
                let deleted_at = obj.get("deleted_at").and_then(|v| v.as_i64()).unwrap_or(0);
                
                let query = match table {
                    "conversations" => Some("DELETE FROM conversations WHERE id = ?1"),
                    "events" => Some("DELETE FROM events WHERE id = ?1"),
                    "kg_nodes" => Some("DELETE FROM kg_nodes WHERE id = ?1"),
                    _ => None,
                };
                
                if let Some(q) = query {
                    // Check local updated_at to ensure deletion is newer than last update
                    let local_updated_at: i64 = conn.query_row(
                        &format!("SELECT updated_at FROM {} WHERE id = ?1", table),
                        rusqlite::params![record_key],
                        |row| row.get(0)
                    ).unwrap_or(0);
                    
                    if deleted_at >= local_updated_at {
                        let _ = conn.execute(q, rusqlite::params![record_key]);
                        // Record tombstone locally to prevent ghost resurrections
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO sync_tombstones (table_name, record_key, deleted_at) VALUES (?1, ?2, ?3)", 
                            rusqlite::params![table, record_key, deleted_at]
                        );
                    }
                }
            }
        }
    }

    // Generic blind replace (for one-way configs and readonly tables)
    let mut import_replace = |table: &str, rows: Vec<serde_json::Value>, cols: &[&str]| {
        if rows.is_empty() { return; }
        let placeholders = vec!["?"; cols.len()].join(", ");
        let query = format!("INSERT OR REPLACE INTO {} ({}) VALUES ({})", table, cols.join(", "), placeholders);
        for row in rows {
            if let Some(obj) = row.as_object() {
                let mut params = Vec::new();
                for col in cols {
                    let val = obj.get(*col).unwrap_or(&serde_json::Value::Null);
                    if let Some(s) = val.as_str() { params.push(rusqlite::types::Value::Text(s.to_string())); }
                    else if let Some(i) = val.as_i64() { params.push(rusqlite::types::Value::Integer(i)); }
                    else if let Some(f) = val.as_f64() { params.push(rusqlite::types::Value::Real(f)); }
                    else { params.push(rusqlite::types::Value::Null); }
                }
                let _ = conn.execute(&query, rusqlite::params_from_iter(params));
            }
        }
    };

    // LWW (Last-Write-Wins) strategy with Conflict Detection
    let mut import_lww = |table: &str, rows: Vec<serde_json::Value>, cols: &[&str]| {
        if rows.is_empty() { return; }
        let placeholders = vec!["?"; cols.len()].join(", ");
        let query_insert = format!("INSERT OR REPLACE INTO {} ({}) VALUES ({})", table, cols.join(", "), placeholders);
        let query_check = format!("SELECT updated_at FROM {} WHERE id = ?1", table);
        
        for row in rows {
            if let Some(obj) = row.as_object() {
                let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let remote_updated_at = obj.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(0);
                
                let local_updated_at: i64 = conn.query_row(&query_check, rusqlite::params![id], |r| r.get(0)).unwrap_or(0);
                
                // CONFLICT DETECTION
                let is_conflict = local_updated_at > last_sync_ts && remote_updated_at > last_sync_ts && local_updated_at != remote_updated_at;
                
                if is_conflict {
                    log::warn!("[Sync Engine] Conflict detected on table {} for id {}. local_updated_at: {}, remote_updated_at: {}, last_sync_ts: {}", table, id, local_updated_at, remote_updated_at, last_sync_ts);
                    
                    // Generate new ULID for the remote conflicted copy
                    let conflict_id = ulid::Ulid::new().to_string();
                    
                    let mut params = Vec::new();
                    for col in cols {
                        let mut val = obj.get(*col).unwrap_or(&serde_json::Value::Null).clone();
                        
                        // Overwrite ID
                        if *col == "id" {
                            val = serde_json::Value::String(conflict_id.clone());
                        }
                        
                        // Append to title/label if it exists
                        if *col == "title" || *col == "label" {
                            if let Some(s) = val.as_str() {
                                val = serde_json::Value::String(format!("{} (手机同步冲突副本)", s));
                            }
                        }
                        
                        if let Some(s) = val.as_str() { params.push(rusqlite::types::Value::Text(s.to_string())); }
                        else if let Some(i) = val.as_i64() { params.push(rusqlite::types::Value::Integer(i)); }
                        else if let Some(f) = val.as_f64() { params.push(rusqlite::types::Value::Real(f)); }
                        else { params.push(rusqlite::types::Value::Null); }
                    }
                    
                    // Insert the conflict copy
                    let _ = conn.execute(&query_insert, rusqlite::params_from_iter(params));
                    
                    // Record to sync_conflicts
                    let ts = crate::now_ms();
                    let _ = conn.execute(
                        "INSERT INTO sync_conflicts (id, table_name, local_id, remote_id, status, created_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
                        rusqlite::params![ulid::Ulid::new().to_string(), table, id, conflict_id, ts]
                    );
                    
                } else if remote_updated_at > local_updated_at {
                    // Normal LWW overwrite
                    let mut params = Vec::new();
                    for col in cols {
                        let val = obj.get(*col).unwrap_or(&serde_json::Value::Null);
                        if let Some(s) = val.as_str() { params.push(rusqlite::types::Value::Text(s.to_string())); }
                        else if let Some(i) = val.as_i64() { params.push(rusqlite::types::Value::Integer(i)); }
                        else if let Some(f) = val.as_f64() { params.push(rusqlite::types::Value::Real(f)); }
                        else { params.push(rusqlite::types::Value::Null); }
                    }
                    let _ = conn.execute(&query_insert, rusqlite::params_from_iter(params));
                }
            }
        }
    };

    // Append-only strategy for messages (de-dupe by sync_id)
    if !data.messages.is_empty() {
        for msg in &data.messages {
            if let Some(obj) = msg.as_object() {
                let sync_id = obj.get("sync_id").and_then(|v| v.as_str()).unwrap_or("");
                if !sync_id.is_empty() {
                    let existing: i32 = conn.query_row("SELECT 1 FROM messages WHERE sync_id = ?1", rusqlite::params![sync_id], |_| Ok(1)).unwrap_or(0);
                    if existing == 0 {
                        let _ = conn.execute(
                            "INSERT INTO messages (conversation_id, role, content, image_base64, created_at, from_channel, sync_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            rusqlite::params![
                                obj.get("conversation_id").and_then(|v| v.as_str()).unwrap_or(""),
                                obj.get("role").and_then(|v| v.as_str()).unwrap_or(""),
                                obj.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                                obj.get("image_base64").and_then(|v| v.as_str()),
                                obj.get("created_at").and_then(|v| v.as_i64()).unwrap_or(ts),
                                obj.get("from_channel").and_then(|v| v.as_str()).unwrap_or("mobile"),
                                sync_id
                            ]
                        );
                    }
                }
            }
        }
    }

    import_replace("settings", data.settings.clone(), &["key", "value"]);
    import_lww("conversations", data.conversations.clone(), &["id", "title", "model", "cost", "last_message", "last_role", "created_at", "updated_at"]);
    import_replace("events", data.events.clone(), &["id", "title", "type", "status", "date", "start_time", "end_time", "description", "created_at", "linked_ticket_id"]);
    import_replace("cron_jobs", data.cron_jobs.clone(), &["id", "title", "cron_expr", "prompt_template", "enabled", "last_run", "created_at"]);
    import_replace("kg_nodes", data.kg_nodes.clone(), &["id", "label", "node_type", "summary", "source", "metadata", "created_at"]);
    import_replace("kg_edges", data.kg_edges.clone(), &["source_id", "target_id", "relation", "confidence", "created_at"]);

    // Append to sync_history.json
    let history_path = crate::get_data_dir().join("sync_history.json");
    let mut history: Vec<serde_json::Value> = if let Ok(existing) = std::fs::read_to_string(&history_path) {
        serde_json::from_str(&existing).unwrap_or_default()
    } else {
        vec![]
    };
    
    let total_records = data.conversations.len() + data.messages.len() + data.events.len() + data.cron_jobs.len() + data.kg_nodes.len() + data.kg_edges.len();
    history.insert(0, serde_json::json!({
        "timestamp": ts,
        "direction": "pull",
        "counts": {
            "conversations": data.conversations.len(),
            "messages": data.messages.len(),
            "events": data.events.len(),
            "settings": data.settings.len(),
            "cron_jobs": data.cron_jobs.len(),
            "kg_nodes": data.kg_nodes.len(),
            "kg_edges": data.kg_edges.len()
        },
        "total_records": total_records,
        "detail": "成功合并云端数据"
    }));
    
    if history.len() > 50 { history.truncate(50); } // Keep last 50
    if let Ok(json_str) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(&history_path, json_str);
    }

    Ok(())
}

pub fn log_sync_action(action: &str, status: &str, detail: &str) {
    let history_path = crate::get_data_dir().join("sync_history.json");
    let mut history: Vec<serde_json::Value> = if let Ok(existing) = std::fs::read_to_string(&history_path) {
        serde_json::from_str(&existing).unwrap_or_default()
    } else {
        vec![]
    };
    
    history.insert(0, serde_json::json!({
        "timestamp": crate::now_ms(),
        "action": action,
        "status": status,
        "detail": detail
    }));
    
    if history.len() > 50 {
        history.truncate(50);
    }
    
    if let Ok(json_str) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(history_path, json_str);
    }
}

pub async fn do_active_sync(app: AppHandle, payload: SyncCommandPayload) -> Result<(), String> {
    info!("[Sync Engine] Starting active sync to device {}", payload.device_id);
    
    // ── Stage 4: LAN sync ──
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "running"}));
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let last_sync_ts: i64 = match app.state::<crate::db::DbState>().0.lock() {
        Ok(conn) => {
            let s: String = conn.query_row("SELECT value FROM settings WHERE key = 'last_sync_ts'", [], |row| row.get(0)).unwrap_or_else(|_| "0".to_string());
            s.parse::<i64>().unwrap_or(0)
        }
        Err(_) => 0,
    };

    let mut sync_success = false;
    for ip in &payload.local_ips {
        let base_url = format!("http://{}:{}", ip, payload.port);
        info!("[Sync Engine] Trying LAN IP: {}", base_url);
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "running", "detail": format!("尝试 {}", ip)}));
        
        // 1. Pull config from PC
        let pull_url = format!("{}/v1/sync/pull", base_url);
        
        let config = crate::read_config();
        let my_device_id = config.get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let my_device_name = config.get("deviceName").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let platform = std::env::consts::OS.to_string();
        
        match client.get(&pull_url)
            .header("X-Device-Id", &my_device_id)
            .header("X-Platform", &platform)
            .header("X-Device-Name", &my_device_name)
            .header("X-Since-Ts", last_sync_ts.to_string())
            .header("Authorization", &payload.public_key)
            .send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(data_val) = json.get("data") {
                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone()) {
                            info!("[Sync Engine] Successfully pulled full/incremental sync data from PC!");
                            if let Err(e) = import_sync_data(&app, sync_data, last_sync_ts) {
                                error!("[Sync Engine] Failed to import sync data: {}", e);
                            } else {
                                sync_success = true;
                                
                                // Update last_sync_ts
                                let now = crate::now_ms();
                                if let Ok(conn) = app.state::<crate::db::DbState>().0.lock() {
                                    let _ = conn.execute("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []);
                                    let _ = conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('last_sync_ts', ?1)", rusqlite::params![now.to_string()]);
                                }

                                // Also pull skills zip
                                let skills_url = format!("{}/v1/sync/skills/download", base_url);
                                let _ = app.emit("sync:progress", serde_json::json!({"stage": "skills_sync", "status": "running"}));
                                if let Ok(s_resp) = client.get(&skills_url).header("X-Device-Id", &my_device_id).header("X-Device-Name", &my_device_name).header("Authorization", &payload.public_key).send().await {
                                    if s_resp.status().is_success() {
                                        if let Ok(bytes) = s_resp.bytes().await {
                                            let ext_dir = crate::get_external_skills_dir_or_default(&config);
                                            let _ = crate::skills_sync::unpack_skills(&bytes, &ext_dir);
                                            info!("[Sync Engine] Successfully pulled and unpacked skills");
                                        }
                                    }
                                }

                                // Also pull notes zip
                                let notes_url = format!("{}/v1/sync/notes/download", base_url);
                                let _ = app.emit("sync:progress", serde_json::json!({"stage": "notes_sync", "status": "running"}));
                                if let Ok(n_resp) = client.get(&notes_url).header("X-Device-Id", &my_device_id).header("X-Device-Name", &my_device_name).header("Authorization", &payload.public_key).send().await {
                                    if n_resp.status().is_success() {
                                        if let Ok(bytes) = n_resp.bytes().await {
                                            let notes_dir = crate::get_data_dir().join("notebook").join("notes");
                                            let _ = crate::skills_sync::unpack_skills(&bytes, &notes_dir);
                                            info!("[Sync Engine] Successfully pulled and unpacked notes");
                                        }
                                    }
                                }

                                // Emit config reconciled event so UI updates
                                let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                                sync_success = true;
                                let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "done", "detail": format!("通过 {} 同步成功", ip)}));
                            }
                        }
                    } else if let Some(config) = json.get("config") { // Fallback for old PC version
                        info!("[Sync Engine] Successfully pulled config from PC!");
                        crate::write_config(&config);
                        let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                        sync_success = true;
                    }
                }
            }
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
                    let err_msg = "ERR-SYNC-05: 鉴权失败 (无效的配对凭证)";
                    let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "error", "detail": err_msg}));
                    log_sync_action("LAN Pull", "error", err_msg);
                    sync_success = false;
                } else {
                    let err_msg = format!("HTTP {}", resp.status());
                    error!("[Sync Engine] Pull request failed: {}", err_msg);
                    log_sync_action("LAN Pull", "error", &err_msg);
                }
            }
            Err(e) => {
                error!("[Sync Engine] Request to {} failed: {}", pull_url, e);
                log_sync_action("LAN Pull", "error", &e.to_string());
            }
        }

        if sync_success {
            // 2. Push outbox to PC (if any)
            let outbox_path = get_mobile_outbox_path();
            if outbox_path.exists() {
                if let Ok(data) = fs::read_to_string(&outbox_path) {
                    if let Ok(mock_outbox) = serde_json::from_str::<serde_json::Value>(&data) {
                        let outbox_url = format!("{}/v1/sync/push", base_url);
                        match client.post(&outbox_url)
                            .header("X-Device-Id", &my_device_id)
                            .header("X-Platform", &platform)
                            .header("X-Device-Name", &my_device_name)
                            .header("Authorization", &payload.public_key)
                            .json(&mock_outbox).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                info!("[Sync Engine] Successfully pushed outbox to PC!");
                                let _ = fs::remove_file(&outbox_path);
                            }
                            _ => {
                                error!("[Sync Engine] Failed to push outbox to PC.");
                            }
                        }
                    }
                }
            }
            
            // 3. Push local DB changes to PC
            if let Ok(local_sync_data) = export_sync_data(&app, last_sync_ts) {
                let push_db_url = format!("{}/v1/sync/push_db", base_url);
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "running", "detail": "推送本地数据到电脑..."}));
                match client.post(&push_db_url)
                    .header("X-Device-Id", &my_device_id)
                    .header("X-Platform", &platform)
                    .header("X-Device-Name", &my_device_name)
                    .header("Authorization", &payload.public_key)
                    .json(&local_sync_data).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        info!("[Sync Engine] Successfully pushed SQLite data to PC!");
                    }
                    _ => {
                        error!("[Sync Engine] Failed to push SQLite data to PC.");
                    }
                }
            }
            break;
        }
    }

    if !sync_success {
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "error", "detail": "ERR-SYNC-01: 所有局域网 IP 均不可达"}));
        info!("[Sync Engine] All LAN attempts failed. Falling back to Relay Tunnel.");
        
        // ── Stage 5: Relay tunnel sync ──
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "running", "detail": "连接 Relay 隧道..."}));
        
        let config = crate::read_config();
        let my_device_id = config.get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let relay_url = "wss://relay.bobbik.org".to_string();
        let ws_url = format!("{}/ws/device/{}", relay_url, url_encode_device_id(&my_device_id));

        let (mut ws_stream, _) = match tokio::time::timeout(tokio::time::Duration::from_secs(15), connect_websocket_robust(&ws_url)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let err_msg = format!("ERR-SYNC-02: Relay 连接拒绝: {}", e);
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                log_sync_action("Relay Connect", "error", &err_msg);
                return Err(format!("ERR-SYNC-02: Failed to connect to relay: {}", e));
            }
            Err(_) => {
                let err_msg = "ERR-SYNC-02: Relay 连接超时 (15s)";
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": err_msg}));
                log_sync_action("Relay Connect", "error", err_msg);
                return Err(err_msg.to_string());
            }
        };
            
        // Register
        let reg_msg = serde_json::json!({
            "type": "register",
            "deviceId": my_device_id
        });
        ws_stream.send(Message::Text(reg_msg.to_string().into())).await.map_err(|e| e.to_string())?;

        // Send pull request
        let pull_req = serde_json::json!({
            "type": "proxy",
            "target_device_id": payload.device_id,
            "payload": {
                "action": "pull",
                "auth_code": payload.public_key
            }
        });
        ws_stream.send(Message::Text(pull_req.to_string().into())).await.map_err(|e| e.to_string())?;

        info!("[Sync Engine] Sent proxy pull request. Waiting for response...");
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "running", "detail": "请求拉取数据 (对话、日程、票据)..."}));

        let timeout = tokio::time::Duration::from_secs(45);
        let pull_task = async {
            while let Some(msg) = ws_stream.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if json.get("type").and_then(|v| v.as_str()) == Some("proxy") {
                            if let Some(inner_payload) = json.get("payload") {
                                if inner_payload.get("action").and_then(|v| v.as_str()) == Some("pull_response") {
                                    if let Some(data_val) = inner_payload.get("data") {
                                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone()) {
                                            return Ok(sync_data);
                                        }
                                    }
                                } else if let Some(error_msg) = inner_payload.get("error").and_then(|v| v.as_str()) {
                                    if error_msg == "Unauthorized" {
                                        return Err("ERR-SYNC-05: 鉴权失败 (无效的配对凭证)".to_string());
                                    }
                                    return Err(format!("Relay proxy error: {}", error_msg));
                                }
                            }
                        } else if json.get("type").and_then(|v| v.as_str()) == Some("proxy_error") {
                            return Err(json.get("message").and_then(|v| v.as_str()).unwrap_or("Proxy error").to_string());
                        }
                    }
                }
            }
            Err("Relay connection closed".to_string())
        };

        match tokio::select! {
            res = pull_task => res,
            _ = tokio::time::sleep(timeout) => {
                let err_msg = "ERR-SYNC-02: Relay 拉取超时 (45s)";
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": err_msg}));
                log_sync_action("Relay Pull", "error", err_msg);
                Err("Relay pull timeout".to_string())
            }
        } {
            Ok(sync_data) => {
                info!("[Sync Engine] Successfully pulled sync data via Relay!");
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "running", "detail": "收到云端数据，正在进行本地合并..."}));
                if let Err(e) = import_sync_data(&app, sync_data, 0) { // For relay pull we might not have accurate last_sync_ts right now
                    let err_msg = format!("ERR-SYNC-03: 导入数据失败: {}", e);
                    let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                    log_sync_action("Relay Import", "error", &err_msg);
                    return Err(format!("Failed to import sync data: {}", e));
                }
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "done"}));
                let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
            }
            Err(e) => {
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": format!("ERR-SYNC-04: {}", e)}));
                return Err(format!("Relay Pull Error: {}", e));
            }
        }

        // 2. Push outbox to PC via Relay
        let outbox_path = get_mobile_outbox_path();
        if outbox_path.exists() {
            if let Ok(data) = std::fs::read_to_string(&outbox_path) {
                if let Ok(mock_outbox) = serde_json::from_str::<serde_json::Value>(&data) {
                    let push_req = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": payload.device_id,
                        "payload": {
                            "action": "push",
                            "auth_code": payload.public_key,
                            "data": mock_outbox
                        }
                    });
                    ws_stream.send(Message::Text(push_req.to_string().into())).await.map_err(|e| e.to_string())?;
                    info!("[Sync Engine] Sent proxy push request. (Ignoring response for now)");
                    let _ = std::fs::remove_file(&outbox_path);
                }
            }
        }
        
        // 3. Push local DB changes to PC via Relay
        if let Ok(local_sync_data) = export_sync_data(&app, last_sync_ts) {
            let push_db_req = serde_json::json!({
                "type": "proxy",
                "target_device_id": payload.device_id,
                "payload": {
                    "action": "push_db",
                    "auth_code": payload.public_key,
                    "data": local_sync_data
                }
            });
            let _ = ws_stream.send(Message::Text(push_db_req.to_string().into())).await;
            info!("[Sync Engine] Sent proxy push_db request via Relay.");
        }
    }

    Ok(())
}

use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{StreamExt, SinkExt};
use tokio::net::TcpStream;
use tokio_tungstenite::client_async_tls;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// URL-encode Base64 device_id to prevent +, /, = from being mangled in URL paths
fn url_encode_device_id(id: &str) -> String {
    id.replace('+', "%2B").replace('/', "%2F").replace('=', "%3D")
}

async fn connect_websocket_robust(ws_url: &str) -> Result<(tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>, tokio_tungstenite::tungstenite::handshake::client::Response), String> {
    log::info!("[WS Robust] Connecting to {} using standard connect_async", ws_url);
    tokio_tungstenite::connect_async(ws_url).await.map_err(|e| e.to_string())
}

pub fn start_relay_listener(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Wait for device identity to be loaded
        let mut device_id_opt = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let config = crate::read_config();
            if let Some(id) = config.get("device_id").and_then(|v| v.as_str()) {
                device_id_opt = Some(id.to_string());
                break;
            }
            log::warn!("[Sync Engine] start_relay_listener: could not get device_id, retrying in 3s...");
        }
        let device_id = device_id_opt.unwrap();

        let relay_url = "wss://relay.bobbik.org".to_string();
        let ws_url = format!("{}/ws/device/{}", relay_url, url_encode_device_id(&device_id));

        loop {
            match connect_websocket_robust(&ws_url).await {
                Ok((mut ws_stream, _)) => {
                    log::info!("[Sync Engine] Connected to Relay WebSocket: {}", ws_url);
                    
                    // Explicitly register device ID (fixes NGINX URL stripping bugs)
                    let reg_msg = serde_json::json!({
                        "type": "register",
                        "deviceId": device_id
                    });
                    let _ = ws_stream.send(Message::Text(reg_msg.to_string().into())).await;

                    use futures_util::{StreamExt, SinkExt};
                    let (mut tx, mut rx) = ws_stream.split();
                    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

                    loop {
                        tokio::select! {
                            _ = ping_interval.tick() => {
                                if let Err(e) = tx.send(Message::Ping(bytes::Bytes::new())).await {
                                    log::error!("[Sync Engine] Ping failed: {}", e);
                                    break;
                                }
                            }
                            msg_opt = rx.next() => {
                                let msg = match msg_opt {
                                    Some(m) => m,
                                    None => {
                                        log::error!("[Sync Engine] Relay WS connection closed (None)");
                                        break;
                                    }
                                };
                                match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                        if msg_type == "notify" {
                                            let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                            log::info!("[Sync Engine] Received notify from {}", from_id);
                                            
                                            // Verify auth code
                                            let provided_auth = json.get("payload").and_then(|p| p.get("auth_code")).and_then(|a| a.as_str());
                                            let expected_auth = crate::crypto::get_pairing_payload(app.state::<crate::crypto::DeviceIdentityState>()).map(|p| p.public_key).unwrap_or_default();
                                            if provided_auth != Some(expected_auth.as_str()) {
                                                log::error!("[Sync Engine] Auth code mismatch in notify from {}", from_id);
                                                let ack = serde_json::json!({
                                                    "type": "ack",
                                                    "target_device_id": from_id,
                                                    "error": "Unauthorized"
                                                });
                                                let _ = tx.send(Message::Text(ack.to_string().into())).await;
                                                continue;
                                            }

                                            let device_name = json.get("payload").and_then(|p| p.get("device_name")).and_then(|v| v.as_str()).map(|s| s.to_string());
                                            let platform = json.get("payload").and_then(|p| p.get("platform")).and_then(|v| v.as_str()).unwrap_or("mobile").to_string();
                                            
                                            // Register device in DeviceRegistry
                                            let registry = app.state::<Arc<DeviceRegistry>>();
                                            registry.update_device(ConnectedDevice {
                                                device_id: from_id.to_string(),
                                                platform: platform.clone(),
                                                ip_address: "relay".to_string(),
                                                last_seen: crate::now_ms(),
                                                device_name: device_name.clone(),
                                            });

                                            // Send Ack back
                                            let ack = serde_json::json!({
                                                "type": "ack",
                                                "target_device_id": from_id,
                                            });
                                            let _ = tx.send(Message::Text(ack.to_string().into())).await;
                                            
                                            // Emit to frontend
                                            let _ = app.emit("sync:device_connected", serde_json::json!({
                                                "device_id": from_id,
                                                "platform": platform,
                                                "device_name": device_name
                                            }));
                                        } else if msg_type == "proxy" {
                                            if let Some(inner_payload) = json.get("payload") {
                                                let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                
                                                // Verify auth code for proxy
                                                let provided_auth = inner_payload.get("auth_code").and_then(|a| a.as_str());
                                                let expected_auth = crate::crypto::get_pairing_payload(app.state::<crate::crypto::DeviceIdentityState>()).map(|p| p.public_key).unwrap_or_default();
                                                if provided_auth != Some(expected_auth.as_str()) {
                                                    log::error!("[Sync Engine] Auth code mismatch in proxy from {}", from_id);
                                                    let err_resp = serde_json::json!({
                                                        "type": "proxy",
                                                        "target_device_id": from_id,
                                                        "payload": {
                                                            "action": "error",
                                                            "error": "Unauthorized"
                                                        }
                                                    });
                                                    let _ = tx.send(Message::Text(err_resp.to_string().into())).await;
                                                    continue;
                                                }
                                                
                                                let action = inner_payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
                                                
                                                let _ = app.emit("sync:device_syncing", serde_json::json!({
                                                    "device_id": from_id,
                                                    "status": "syncing"
                                                }));

                                                if action == "pull" {
                                                    log::info!("[Sync Engine] Received proxy pull request from {}", from_id);
                                                    let since_ts = inner_payload.get("since_ts").and_then(|v| v.as_i64()).unwrap_or(0);
                                                    if let Ok(sync_data) = export_sync_data(&app, since_ts) {
                                                        let pull_resp = serde_json::json!({
                                                            "type": "proxy",
                                                            "target_device_id": from_id,
                                                            "payload": {
                                                                "action": "pull_response",
                                                                "data": sync_data
                                                            }
                                                        });
                                                        let _ = tx.send(Message::Text(pull_resp.to_string().into())).await;
                                                    }
                                                } else if action == "push" {
                                                    log::info!("[Sync Engine] Received proxy push request from {}", from_id);
                                                    if let Some(data_val) = inner_payload.get("data") {
                                                        if let Some(arr) = data_val.as_array() {
                                                            log::info!("[Sync Engine] Pushing {} operations to PC outbox via Relay", arr.len());
                                                            crate::outbox::write_outbox(arr.clone());
                                                        }
                                                    }
                                                } else if action == "push_db" {
                                                    log::info!("[Sync Engine] Received proxy push_db request from {}", from_id);
                                                    if let Some(data_val) = inner_payload.get("data") {
                                                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone()) {
                                                            let last_sync_ts_pc: i64 = match app.state::<crate::db::DbState>().0.lock() {
                                                                Ok(conn) => {
                                                                    let s: String = conn.query_row("SELECT value FROM settings WHERE key = 'last_sync_ts'", [], |row| row.get(0)).unwrap_or_else(|_| "0".to_string());
                                                                    s.parse::<i64>().unwrap_or(0)
                                                                }
                                                                Err(_) => 0,
                                                            };
                                                            
                                                            if let Err(e) = import_sync_data(&app, sync_data, last_sync_ts_pc) {
                                                                log::error!("[Sync Engine] Failed to import relay pushed DB data: {}", e);
                                                            } else {
                                                                let now = crate::now_ms();
                                                                if let Ok(conn) = app.state::<crate::db::DbState>().0.lock() {
                                                                    let _ = conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('last_sync_ts', ?1)", rusqlite::params![now.to_string()]);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                
                                                let _ = app.emit("sync:device_syncing", serde_json::json!({
                                                    "device_id": from_id,
                                                    "status": "idle"
                                                }));
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("[Sync Engine] Relay WS error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                            } // closes rx.next() =>
                        } // closes tokio::select!
                    } // closes loop
                } // closes Ok((ws_stream, _)) =>
                Err(e) => {
                    log::error!("[Sync Engine] Failed to connect to Relay: {}", e);
                }
            }
            
            // Reconnect backoff
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

#[tauri::command]
pub fn get_sync_logs(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let path = crate::get_data_dir().join("sync_history.json");
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(logs) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
            return Ok(logs);
        }
    }
    Ok(vec![])
}
