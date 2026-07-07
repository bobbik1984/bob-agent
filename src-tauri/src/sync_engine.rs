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

pub fn register_device(app: &AppHandle, headers: &axum::http::HeaderMap, ip: std::net::SocketAddr) {
    if let (Some(device_id), Some(platform)) = (
        headers.get("x-device-id").and_then(|v| v.to_str().ok()),
        headers.get("x-platform").and_then(|v| v.to_str().ok()),
    ) {
        let registry = app.state::<Arc<DeviceRegistry>>();
        let device = ConnectedDevice {
            device_id: device_id.to_string(),
            platform: platform.to_string(),
            ip_address: ip.ip().to_string(),
            last_seen: crate::now_ms(),
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
pub async fn relay_handshake(_app: AppHandle, target_device_id: String) -> Result<(), String> {
    let config = crate::read_config();
    let my_device_id = config.get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    
    let relay_url = "wss://relay.bobbik.org".to_string();
    let ws_url = format!("{}/ws/device/{}", relay_url, my_device_id);

    let (mut ws_stream, _) = connect_async(&ws_url).await.map_err(|e| format!("Failed to connect to relay: {}", e))?;
    
    // Send notify
    let notify = serde_json::json!({
        "type": "notify",
        "target_device_id": target_device_id,
        "payload": {}
    });
    ws_stream.send(Message::Text(notify.to_string().into())).await.map_err(|e| e.to_string())?;

    // Wait for ack with timeout
    let timeout = tokio::time::Duration::from_secs(10);
    let ack_task = async {
        while let Some(msg) = ws_stream.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if json.get("type").and_then(|v| v.as_str()) == Some("ack") {
                        return Ok(());
                    }
                }
            }
        }
        Err("Connection closed before ack".to_string())
    };

    tokio::select! {
        res = ack_task => res,
        _ = tokio::time::sleep(timeout) => {
            Err("Handshake timeout: PC did not respond. Please ensure PC Bob is open and online.".to_string())
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncData {
    pub config: serde_json::Value,
    pub settings: Vec<serde_json::Value>,
    pub events: Vec<serde_json::Value>,
    pub cron_jobs: Vec<serde_json::Value>,
    pub kg_nodes: Vec<serde_json::Value>,
    pub kg_edges: Vec<serde_json::Value>,
    pub wiki_fts: Vec<serde_json::Value>,
}

pub fn export_sync_data(app: &AppHandle) -> Result<SyncData, String> {
    let config = crate::read_config();
    let db = app.state::<crate::db::DbState>();
    let conn = db.0.lock().map_err(|_| "Failed to lock db")?;
    
    let extract = |query: &str, cols: &[&str]| -> Result<Vec<serde_json::Value>, String> {
        let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, col) in cols.iter().enumerate() {
                // Simplified string extraction for sync
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

    let settings = extract("SELECT key, value FROM settings", &["key", "value"]).unwrap_or_default();
    let events = extract("SELECT id, title, type, status, date_str, start_time, end_time, description, updated_at FROM events", 
        &["id", "title", "type", "status", "date_str", "start_time", "end_time", "description", "updated_at"]).unwrap_or_default();
    let cron_jobs = extract("SELECT id, title, cron_expr, command, args, created_at FROM cron_jobs", 
        &["id", "title", "cron_expr", "command", "args", "created_at"]).unwrap_or_default();
    let kg_nodes = extract("SELECT id, label, attributes, memory_ids, last_updated FROM kg_nodes", 
        &["id", "label", "attributes", "memory_ids", "last_updated"]).unwrap_or_default();
    let kg_edges = extract("SELECT source_id, target_id, relation_type, weight, memory_ids, last_updated FROM kg_edges", 
        &["source_id", "target_id", "relation_type", "weight", "memory_ids", "last_updated"]).unwrap_or_default();
    let wiki_fts = extract("SELECT file_name, source_path, wiki_path, summary, keywords, category, indexed_at FROM wiki_fts", 
        &["file_name", "source_path", "wiki_path", "summary", "keywords", "category", "indexed_at"]).unwrap_or_default();

    Ok(SyncData { config, settings, events, cron_jobs, kg_nodes, kg_edges, wiki_fts })
}

pub fn import_sync_data(app: &AppHandle, data: SyncData) -> Result<(), String> {
    crate::write_config(&data.config);
    let db = app.state::<crate::db::DbState>();
    let conn = db.0.lock().map_err(|_| "Failed to lock db")?;

    let mut import_table = |table: &str, rows: Vec<serde_json::Value>, cols: &[&str]| {
        if rows.is_empty() { return; }
        let placeholders = vec!["?"; cols.len()].join(", ");
        let query = format!("INSERT OR REPLACE INTO {} ({}) VALUES ({})", table, cols.join(", "), placeholders);
        for row in rows {
            if let Some(obj) = row.as_object() {
                let mut params = Vec::new();
                for col in cols {
                    let val = obj.get(*col).unwrap_or(&serde_json::Value::Null);
                    if let Some(s) = val.as_str() {
                        params.push(rusqlite::types::Value::Text(s.to_string()));
                    } else if let Some(i) = val.as_i64() {
                        params.push(rusqlite::types::Value::Integer(i));
                    } else if let Some(f) = val.as_f64() {
                        params.push(rusqlite::types::Value::Real(f));
                    } else {
                        params.push(rusqlite::types::Value::Null);
                    }
                }
                let _ = conn.execute(&query, rusqlite::params_from_iter(params));
            }
        }
    };

    import_table("settings", data.settings, &["key", "value"]);
    import_table("events", data.events, &["id", "title", "type", "status", "date_str", "start_time", "end_time", "description", "updated_at"]);
    import_table("cron_jobs", data.cron_jobs, &["id", "title", "cron_expr", "command", "args", "created_at"]);
    import_table("kg_nodes", data.kg_nodes, &["id", "label", "attributes", "memory_ids", "last_updated"]);
    import_table("kg_edges", data.kg_edges, &["source_id", "target_id", "relation_type", "weight", "memory_ids", "last_updated"]);
    import_table("wiki_fts", data.wiki_fts, &["file_name", "source_path", "wiki_path", "summary", "keywords", "category", "indexed_at"]);

    Ok(())
}

async fn do_active_sync(app: AppHandle, payload: SyncCommandPayload) -> Result<(), String> {
    info!("[Sync Engine] Starting active sync to device {}", payload.device_id);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let mut sync_success = false;
    for ip in &payload.local_ips {
        let base_url = format!("http://{}:{}", ip, payload.port);
        info!("[Sync Engine] Trying LAN IP: {}", base_url);
        
        // 1. Pull config from PC
        let pull_url = format!("{}/v1/sync/pull", base_url);
        
        let config = crate::read_config();
        let my_device_id = config.get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let platform = std::env::consts::OS.to_string();
        
        match client.get(&pull_url)
            .header("X-Device-Id", &my_device_id)
            .header("X-Platform", &platform)
            .send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(data_val) = json.get("data") {
                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone()) {
                            info!("[Sync Engine] Successfully pulled full sync data from PC!");
                            if let Err(e) = import_sync_data(&app, sync_data) {
                                error!("[Sync Engine] Failed to import sync data: {}", e);
                            } else {
                                // Emit config reconciled event so UI updates
                                let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                                sync_success = true;
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
                error!("[Sync Engine] Pull request failed with status: {}", resp.status());
            }
            Err(e) => {
                error!("[Sync Engine] Request to {} failed: {}", pull_url, e);
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
            break;
        }
    }

    if !sync_success {
        info!("[Sync Engine] All LAN attempts failed. Falling back to Relay Tunnel.");
        
        let config = crate::read_config();
        let my_device_id = config.get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let relay_url = "wss://relay.bobbik.org".to_string();
        let ws_url = format!("{}/ws/device/{}", relay_url, my_device_id);

        let (mut ws_stream, _) = connect_async(&ws_url)
            .await.map_err(|e| format!("LAN and Relay both failed. Relay err: {}", e))?;
            
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
                "action": "pull"
            }
        });
        ws_stream.send(Message::Text(pull_req.to_string().into())).await.map_err(|e| e.to_string())?;

        info!("[Sync Engine] Sent proxy pull request. Waiting for response...");

        let timeout = tokio::time::Duration::from_secs(15);
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
            _ = tokio::time::sleep(timeout) => Err("Relay sync timeout: PC did not respond.".to_string())
        } {
            Ok(sync_data) => {
                info!("[Sync Engine] Successfully pulled sync data via Relay!");
                if let Err(e) = import_sync_data(&app, sync_data) {
                    return Err(format!("Failed to import sync data: {}", e));
                }
                let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
            }
            Err(e) => {
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
                            "data": mock_outbox
                        }
                    });
                    ws_stream.send(Message::Text(push_req.to_string().into())).await.map_err(|e| e.to_string())?;
                    info!("[Sync Engine] Sent proxy push request. (Ignoring response for now)");
                    let _ = std::fs::remove_file(&outbox_path);
                }
            }
        }
    }

    Ok(())
}

use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{StreamExt, SinkExt};

pub fn start_relay_listener(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Wait for device identity to be loaded
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        
        let config = crate::read_config();
        let device_id = match config.get("device_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => {
                log::warn!("[Sync Engine] start_relay_listener: could not get device_id");
                return;
            }
        };

        let relay_url = "wss://relay.bobbik.org".to_string();
        let ws_url = format!("{}/ws/device/{}", relay_url, device_id);

        loop {
            match connect_async(&ws_url).await {
                Ok((mut ws_stream, _)) => {
                    log::info!("[Sync Engine] Connected to Relay WebSocket: {}", ws_url);
                    
                    // Explicitly register device ID (fixes NGINX URL stripping bugs)
                    let reg_msg = serde_json::json!({
                        "type": "register",
                        "deviceId": device_id
                    });
                    let _ = ws_stream.send(Message::Text(reg_msg.to_string().into())).await;

                    while let Some(msg) = ws_stream.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                        if msg_type == "notify" {
                                            let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                            log::info!("[Sync Engine] Received notify from {}", from_id);
                                            
                                            // Register device in DeviceRegistry
                                            let registry = app.state::<Arc<DeviceRegistry>>();
                                            registry.update_device(ConnectedDevice {
                                                device_id: from_id.to_string(),
                                                platform: "mobile".to_string(),
                                                ip_address: "relay".to_string(),
                                                last_seen: crate::now_ms(),
                                            });

                                            // Send Ack back
                                            let ack = serde_json::json!({
                                                "type": "ack",
                                                "target_device_id": from_id,
                                            });
                                            let _ = ws_stream.send(Message::Text(ack.to_string().into())).await;
                                            
                                            // Emit to frontend
                                            let _ = app.emit("sync:device_connected", serde_json::json!({
                                                "device_id": from_id,
                                                "platform": "mobile"
                                            }));
                                        } else if msg_type == "proxy" {
                                            if let Some(inner_payload) = json.get("payload") {
                                                let action = inner_payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
                                                let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                
                                                if action == "pull" {
                                                    log::info!("[Sync Engine] Received proxy pull request from {}", from_id);
                                                    if let Ok(sync_data) = export_sync_data(&app) {
                                                        let pull_resp = serde_json::json!({
                                                            "type": "proxy",
                                                            "target_device_id": from_id,
                                                            "payload": {
                                                                "action": "pull_response",
                                                                "data": sync_data
                                                            }
                                                        });
                                                        let _ = ws_stream.send(Message::Text(pull_resp.to_string().into())).await;
                                                    }
                                                } else if action == "push" {
                                                    log::info!("[Sync Engine] Received proxy push request from {}", from_id);
                                                    if let Some(data_val) = inner_payload.get("data") {
                                                        if let Some(arr) = data_val.as_array() {
                                                            log::info!("[Sync Engine] Pushing {} operations to PC outbox via Relay", arr.len());
                                                            crate::outbox::write_outbox(arr.clone());
                                                        }
                                                    }
                                                }
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
                    }
                }
                Err(e) => {
                    log::error!("[Sync Engine] Failed to connect to Relay: {}", e);
                }
            }
            
            // Reconnect backoff
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}
