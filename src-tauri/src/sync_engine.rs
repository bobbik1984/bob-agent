use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::{command, AppHandle, Emitter, Manager};

pub static RELAY_CONNECTED: AtomicBool = AtomicBool::new(false);

use lazy_static::lazy_static;
use std::sync::Mutex;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Debug, Clone, PartialEq)]
pub enum RelayTerminal {
    Ack,
    ProxyResponse,
    CommitAck,
    AnyResponse,
}

pub struct RelayRequestWaiter {
    pub tx: oneshot::Sender<serde_json::Value>,
    pub terminal: RelayTerminal,
}

lazy_static! {
    pub static ref RELAY_TX: RwLock<Option<tokio::sync::mpsc::Sender<Message>>> = RwLock::new(None);
    pub static ref PENDING_REQUESTS: RwLock<HashMap<String, RelayRequestWaiter>> =
        RwLock::new(HashMap::new());
    pub static ref RELAY_RECONNECT_TRIGGER: Mutex<Option<tokio::sync::mpsc::Sender<()>>> =
        Mutex::new(None);
}
use std::fs;
use std::path::PathBuf;

use crate::lan_sync::LanSyncEngine;
use crate::sync_protocol::{
    DiagnosticEvent, DiagnosticStage, DiagnosticStatus, TransportKind, SYNC_PROTOCOL_VERSION,
};

pub async fn send_relay_request_and_wait(
    request: serde_json::Value,
    timeout: tokio::time::Duration,
    terminal: RelayTerminal,
) -> Result<serde_json::Value, String> {
    let trace_id = request
        .get("trace_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let message_id = request
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if trace_id.is_empty() || message_id.is_empty() {
        return Err("Request missing trace_id or message_id".to_string());
    }

    let (tx, rx) = oneshot::channel();
    {
        let mut pending = PENDING_REQUESTS.write().unwrap();
        pending.insert(message_id.clone(), RelayRequestWaiter { tx, terminal });
    }

    let relay_tx = {
        let lock = RELAY_TX.read().unwrap();
        lock.as_ref().cloned()
    };

    let send_result = if let Some(tx) = relay_tx {
        tx.send(Message::Text(request.to_string().into())).await
    } else {
        return Err("ERR-SYNC-02: Relay 后台未连接".to_string());
    };

    if send_result.is_err() {
        let mut pending = PENDING_REQUESTS.write().unwrap();
        pending.remove(&message_id);
        return Err("ERR-SYNC-02: Failed to send to Relay".to_string());
    }

    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(response)) => {
            if response.get("type").and_then(|v| v.as_str()) == Some("error")
                || response.get("type").and_then(|v| v.as_str()) == Some("proxy_error")
            {
                return Err(response
                    .get("error")
                    .or_else(|| response.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Relay error")
                    .to_string());
            }
            Ok(response)
        }
        Ok(Err(_)) => Err("Relay response channel closed".to_string()),
        Err(_) => {
            let mut pending = PENDING_REQUESTS.write().unwrap();
            pending.remove(&message_id);
            Err("ERR-SYNC-02: Relay 请求超时".to_string())
        }
    }
}

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
        let device_name = headers
            .get("x-device-name")
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
    #[serde(default)]
    pub skip_relay: bool,
}

#[derive(Clone)]
struct SyncTraceContext {
    sync_id: String,
    trace_id: String,
}

#[command]
pub async fn trigger_mobile_sync(
    app: AppHandle,
    payload: SyncCommandPayload,
) -> Result<(), String> {
    info!(
        "[Sync Engine] trigger_mobile_sync called, listen_only: {}",
        payload.listen_only
    );
    log_sync_action(
        "Auto Discovery",
        "running",
        if payload.listen_only {
            "Passive listen"
        } else {
            "Active probe"
        },
    );

    if payload.listen_only {
        let lan_engine = app.state::<Arc<LanSyncEngine>>();
        let target_device_id = payload.device_id.clone();
        let payload_clone = payload.clone();
        let app_clone = app.clone();

        lan_engine.start_listen_broadcast(move |discovered_id, ip, port| {
            if discovered_id == target_device_id {
                info!(
                    "[Sync Engine] Discovered paired PC at {}:{}, initiating active sync!",
                    ip, port
                );
                let mut active_payload = payload_clone.clone();
                active_payload.listen_only = false;
                active_payload.local_ips = vec![ip];
                active_payload.port = port;

                let app_for_task = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = do_active_sync(app_for_task, active_payload, None).await {
                        error!("[Sync Engine] Active sync failed: {}", e);
                    }
                });
            }
        });
        return Ok(());
    }

    let started_at = crate::now_ms();
    let sync_id = uuid::Uuid::new_v4().to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();
    let peer_device_id = payload.device_id.clone();
    crate::sync_diagnostics::begin_trace(&trace_id, SYNC_PROTOCOL_VERSION);
    record_diagnostic_event(
        &trace_id,
        &sync_id,
        &peer_device_id,
        TransportKind::Lan,
        DiagnosticStage::LanDirect,
        DiagnosticStatus::Running,
        1,
        None,
    );

    let result = do_active_sync(
        app,
        payload,
        Some(SyncTraceContext {
            sync_id: sync_id.clone(),
            trace_id: trace_id.clone(),
        }),
    )
    .await;
    let (status, transport, summary, error_code) = match &result {
        Ok(transport) => {
            if *transport == TransportKind::Lan {
                record_diagnostic_event(
                    &trace_id,
                    &sync_id,
                    &peer_device_id,
                    *transport,
                    DiagnosticStage::LanDirect,
                    DiagnosticStatus::Success,
                    2,
                    None,
                );
            } else {
                record_diagnostic_event(
                    &trace_id,
                    &sync_id,
                    &peer_device_id,
                    TransportKind::Lan,
                    DiagnosticStage::LanDirect,
                    DiagnosticStatus::Skipped,
                    2,
                    Some("LAN 不可用，已自动切换 Relay"),
                );
                // Removed synthetic success events. The real events are driven by diagnostic_receipts and terminal ACKs.
            }
            (
                DiagnosticStatus::Success,
                Some(*transport),
                Some("同步完成".to_string()),
                None,
            )
        }
        Err(error) => {
            record_diagnostic_event(
                &trace_id,
                &sync_id,
                &peer_device_id,
                TransportKind::Lan,
                DiagnosticStage::LanDirect,
                DiagnosticStatus::Failed,
                2,
                Some(error),
            );
            record_diagnostic_event(
                &trace_id,
                &sync_id,
                &peer_device_id,
                TransportKind::Relay,
                DiagnosticStage::LocalCommit,
                DiagnosticStatus::Unknown,
                2,
                Some(error),
            );
            (
                DiagnosticStatus::Failed,
                None,
                Some("同步未完成".to_string()),
                extract_error_code(error),
            )
        }
    };
    let _ = crate::sync_history::record_run(crate::sync_history::SyncRun {
        sync_id,
        trace_id,
        started_at,
        finished_at: crate::now_ms(),
        status,
        transport,
        peer_device_id: Some(peer_device_id),
        summary,
        error_code,
    });
    result.map(|_| ())
}

fn extract_error_code(error: &str) -> Option<String> {
    error
        .split_whitespace()
        .find(|part| part.starts_with("ERR-"))
        .map(|part| part.trim_end_matches(':').to_string())
}

fn record_diagnostic_event(
    trace_id: &str,
    sync_id: &str,
    peer_device_id: &str,
    transport: TransportKind,
    stage: DiagnosticStage,
    status: DiagnosticStatus,
    sequence: u64,
    detail: Option<&str>,
) {
    let event = DiagnosticEvent {
        protocol_version: SYNC_PROTOCOL_VERSION,
        trace_id: trace_id.to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        sync_id: Some(sync_id.to_string()),
        from_device_id: crate::read_config()
            .get("device_id")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
        target_device_id: peer_device_id.to_string(),
        transport,
        stage,
        status,
        sequence,
        timestamp: crate::now_ms(),
        error_code: detail.and_then(extract_error_code),
        detail: detail.map(str::to_string),
    };
    crate::sync_diagnostics::apply_event(&event);
    let _ = crate::sync_history::record_event(event);
}

fn copy_trace_fields(source: &serde_json::Value, target: &mut serde_json::Value, response: bool) {
    if source
        .get("protocol_version")
        .and_then(|value| value.as_u64())
        .unwrap_or(1)
        < SYNC_PROTOCOL_VERSION as u64
    {
        return;
    }
    for key in ["protocol_version", "trace_id", "sync_id"] {
        if let Some(value) = source.get(key) {
            target[key] = value.clone();
        }
    }
    if response {
        target["flow_phase"] = serde_json::json!("response");
        if let Some(msg_id) = source.get("message_id") {
            target["ref_message_id"] = msg_id.clone();
        }
    } else {
        if let Some(msg_id) = source.get("message_id") {
            target["message_id"] = msg_id.clone();
        }
    }
}

fn record_relay_receipt(receipt: &serde_json::Value, peer_device_id: &str) {
    let Some(trace_id) = receipt.get("trace_id").and_then(|value| value.as_str()) else {
        return;
    };
    let sync_id = receipt
        .get("sync_id")
        .and_then(|value| value.as_str())
        .unwrap_or(trace_id);
    let (stage, status) = match receipt.get("receipt").and_then(|value| value.as_str()) {
        Some("relay_request_accepted") => {
            (DiagnosticStage::MobileToRelay, DiagnosticStatus::Success)
        }
        Some("relay_delivered_to_target") => {
            (DiagnosticStage::RelayToPc, DiagnosticStatus::Success)
        }
        Some("relay_response_accepted") => (DiagnosticStage::PcToRelay, DiagnosticStatus::Success),
        Some("relay_delivered_to_origin") => {
            (DiagnosticStage::RelayToMobile, DiagnosticStatus::Success)
        }
        Some("target_offline") | Some("delivery_failed") => {
            (DiagnosticStage::RelayToPc, DiagnosticStatus::Failed)
        }
        _ => return,
    };
    record_diagnostic_event(
        trace_id,
        sync_id,
        peer_device_id,
        TransportKind::Relay,
        stage,
        status,
        receipt
            .get("timestamp")
            .and_then(|value| value.as_u64())
            .unwrap_or(1),
        receipt.get("error_code").and_then(|value| value.as_str()),
    );
}

#[command]
pub async fn write_mobile_outbox(
    _app: AppHandle,
    operations: Vec<serde_json::Value>,
) -> Result<(), String> {
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

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, data).map_err(|e| e.to_string())?;
    fs::rename(&temp_path, &path).map_err(|e| e.to_string())?;

    info!(
        "[Sync Engine] Appended to mobile outbox, total items: {}",
        outbox.len()
    );
    Ok(())
}

#[command]
pub async fn trigger_wakeup_via_relay(app: AppHandle, device_id: String) -> Result<(), String> {
    let config = crate::read_config();
    let my_device_id = config
        .get("device_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    log_sync_action(
        "Relay Wakeup",
        "running",
        &format!("Attempting to wake up device: {}", device_id),
    );

    let msg = serde_json::json!({
        "type": "wakeup",
        "target_device_id": device_id,
        "from_device_id": my_device_id
    });

    let relay_tx = {
        let lock = RELAY_TX.read().unwrap();
        lock.as_ref().cloned()
    };

    if let Some(tx) = relay_tx {
        if let Err(e) = tx
            .send(tokio_tungstenite::tungstenite::Message::Text(
                msg.to_string().into(),
            ))
            .await
        {
            log_sync_action(
                "Relay Wakeup",
                "error",
                &format!("Failed to send wakeup: {}", e),
            );
            return Err(e.to_string());
        }
    } else {
        log_sync_action("Relay Wakeup", "error", "Relay 后台未连接");
        return Err("Relay 后台未连接".to_string());
    }

    log_sync_action(
        "Relay Wakeup",
        "done",
        &format!("Wakeup signal sent to {}", device_id),
    );

    Ok(())
}

#[command]
pub async fn relay_handshake(
    app: AppHandle,
    target_device_id: String,
    auth_code: String,
) -> Result<(), String> {
    let config = crate::read_config();
    let my_device_name = config
        .get("deviceName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let platform = std::env::consts::OS.to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();
    let sync_id = uuid::Uuid::new_v4().to_string();

    // ── Stage 3a: Verify Relay connection ──
    let _ = app.emit(
        "sync:progress",
        serde_json::json!({"stage": "relay_connect", "status": "running"}),
    );

    let relay_tx = {
        let lock = RELAY_TX.read().unwrap();
        lock.as_ref().cloned()
    };

    if relay_tx.is_none() {
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_connect", "status": "error", "detail": "ERR-PAIRING-01: Relay 后台未连接"}));
        let _ = crate::sync_history::record_activity(
            DiagnosticStatus::Failed,
            Some(TransportKind::Relay),
            Some(target_device_id.clone()),
            "Relay 连接失败",
            Some("ERR-PAIRING-01".to_string()),
        );
        return Err("ERR-PAIRING-01: Relay 后台未连接".to_string());
    }

    let _ = app.emit(
        "sync:progress",
        serde_json::json!({"stage": "relay_connect", "status": "done"}),
    );

    // ── Stage 3b & 3c: Send notify to PC via Relay and wait for Ack ──
    let _ = app.emit(
        "sync:progress",
        serde_json::json!({"stage": "relay_notify", "status": "running"}),
    );
    let msg = serde_json::json!({
        "type": "notify",
        "target_device_id": target_device_id,
        "protocol_version": SYNC_PROTOCOL_VERSION,
        "trace_id": trace_id,
        "message_id": uuid::Uuid::new_v4().to_string(),
        "sync_id": sync_id,
        "payload": {
            "device_name": my_device_name,
            "platform": platform,
            "auth_code": auth_code
        }
    });

    match send_relay_request_and_wait(
        msg,
        tokio::time::Duration::from_secs(10),
        RelayTerminal::Ack,
    )
    .await
    {
        Ok(json) => {
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({"stage": "relay_notify", "status": "done"}),
            );
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({"stage": "relay_ack", "status": "running"}),
            );

            if let Some(error_msg) = json.get("error").and_then(|v| v.as_str()) {
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "error", "detail": format!("ERR-PAIRING-04: {}", error_msg)}));
                let _ = crate::sync_history::record_activity(
                    DiagnosticStatus::Failed,
                    Some(TransportKind::Relay),
                    Some(target_device_id.clone()),
                    "目标设备拒绝配对",
                    Some("ERR-PAIRING-04".to_string()),
                );
                return Err(format!("Relay error: {}", error_msg));
            }

            let _ = app.emit(
                "sync:progress",
                serde_json::json!({"stage": "relay_ack", "status": "done"}),
            );
            let _ = crate::sync_history::record_activity(
                DiagnosticStatus::Success,
                Some(TransportKind::Relay),
                Some(target_device_id.clone()),
                "Relay 配对成功",
                None,
            );
            Ok(())
        }
        Err(e) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "error", "detail": format!("ERR-PAIRING-03: {}", e)}));
            let _ = crate::sync_history::record_activity(
                DiagnosticStatus::Timeout,
                Some(TransportKind::Relay),
                Some(target_device_id.clone()),
                "等待目标设备响应超时",
                Some("ERR-PAIRING-03".to_string()),
            );
            Err(format!("ERR-PAIRING-03: {}", e))
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
    #[serde(default)]
    pub captures: Vec<serde_json::Value>,
    pub cron_jobs: Vec<serde_json::Value>,
    pub kg_nodes: Vec<serde_json::Value>,
    pub kg_edges: Vec<serde_json::Value>,
    pub wiki_fts: Vec<serde_json::Value>,
    #[serde(default)]
    pub tombstones: Vec<serde_json::Value>,
}

pub fn export_sync_data(
    app: &AppHandle,
    since_ts: i64,
    is_relay: bool,
) -> Result<SyncData, String> {
    let config = crate::read_config();
    let db = app.state::<crate::db::DbState>();
    let conn = db.0.lock().map_err(|_| "Failed to lock db")?;

    let extract = |query: &str,
                   params: &[&dyn rusqlite::ToSql],
                   cols: &[&str]|
     -> Result<Vec<serde_json::Value>, String> {
        let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params, |row| {
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
            })
            .map_err(|e| e.to_string())?;

        let mut result = Vec::new();
        for r in rows {
            if let Ok(v) = r {
                result.push(v);
            }
        }
        Ok(result)
    };

    let settings =
        extract("SELECT key, value FROM settings", &[], &["key", "value"]).unwrap_or_default();
    let conversations = extract("SELECT id, title, model, cost, last_message, last_role, created_at, updated_at FROM conversations WHERE updated_at >= ?1", &[&since_ts], 
        &["id", "title", "model", "cost", "last_message", "last_role", "created_at", "updated_at"]).unwrap_or_default();
    let messages = if is_relay {
        extract("SELECT id, conversation_id, role, content, NULL as image_base64, created_at, from_channel, sync_id FROM messages WHERE created_at >= ?1", &[&since_ts], 
        &["id", "conversation_id", "role", "content", "image_base64", "created_at", "from_channel", "sync_id"]).unwrap_or_default()
    } else {
        extract("SELECT id, conversation_id, role, content, image_base64, created_at, from_channel, sync_id FROM messages WHERE created_at >= ?1", &[&since_ts], 
        &["id", "conversation_id", "role", "content", "image_base64", "created_at", "from_channel", "sync_id"]).unwrap_or_default()
    };
    let events = extract("SELECT id, title, type, status, date, start_time, end_time, description, created_at, updated_at, completed_at, linked_ticket_id FROM events WHERE updated_at >= ?1", &[&since_ts], 
        &["id", "title", "type", "status", "date", "start_time", "end_time", "description", "created_at", "updated_at", "completed_at", "linked_ticket_id"]).unwrap_or_default();
    let captures = extract("SELECT capture_id, schema_version, entry_point, source_device, original_content, source_url, file_path, explicit_intent, content_hash, idempotency_key, language, privacy_scope, sync_scope, status, error_stage, error_message, derived_refs, retry_count, next_retry_at, created_at, updated_at FROM capture_journal WHERE updated_at >= ?1 AND sync_scope != 'local_only'", &[&since_ts],
        &["capture_id", "schema_version", "entry_point", "source_device", "original_content", "source_url", "file_path", "explicit_intent", "content_hash", "idempotency_key", "language", "privacy_scope", "sync_scope", "status", "error_stage", "error_message", "derived_refs", "retry_count", "next_retry_at", "created_at", "updated_at"]).unwrap_or_default();
    let cron_jobs = extract("SELECT id, title, cron_expr, prompt_template, enabled, last_run, created_at FROM cron_jobs", &[], 
        &["id", "title", "cron_expr", "prompt_template", "enabled", "last_run", "created_at"]).unwrap_or_default();

    let kg_nodes = if is_relay {
        vec![]
    } else {
        extract(
            "SELECT id, label, node_type, summary, source, metadata, created_at FROM kg_nodes",
            &[],
            &[
                "id",
                "label",
                "node_type",
                "summary",
                "source",
                "metadata",
                "created_at",
            ],
        )
        .unwrap_or_default()
    };
    let kg_edges = if is_relay {
        vec![]
    } else {
        extract(
            "SELECT source_id, target_id, relation, confidence, created_at FROM kg_edges",
            &[],
            &[
                "source_id",
                "target_id",
                "relation",
                "confidence",
                "created_at",
            ],
        )
        .unwrap_or_default()
    };
    let wiki_fts = Vec::new();
    let tombstones = extract(
        "SELECT table_name, record_key, deleted_at FROM sync_tombstones WHERE deleted_at >= ?1",
        &[&since_ts],
        &["table_name", "record_key", "deleted_at"],
    )
    .unwrap_or_default();

    Ok(SyncData {
        config,
        settings,
        conversations,
        messages,
        events,
        captures,
        cron_jobs,
        kg_nodes,
        kg_edges,
        wiki_fts,
        tombstones,
    })
}

pub fn import_sync_data(app: &AppHandle, data: SyncData, last_sync_ts: i64) -> Result<(), String> {
    crate::write_config(&data.config);
    let db = app.state::<crate::db::DbState>();
    let mut conn = db.0.lock().map_err(|_| "Failed to lock db")?;
    let tx_sql = conn.transaction().map_err(|e| e.to_string())?;
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
                    let local_updated_at: i64 = tx_sql
                        .query_row(
                            &format!("SELECT updated_at FROM {} WHERE id = ?1", table),
                            rusqlite::params![record_key],
                            |row| row.get(0),
                        )
                        .unwrap_or(0);

                    if deleted_at >= local_updated_at {
                        tx_sql
                            .execute(q, rusqlite::params![record_key])
                            .map_err(|e| e.to_string())?;
                        // Record tombstone locally to prevent ghost resurrections
                        tx_sql.execute(
                            "INSERT OR REPLACE INTO sync_tombstones (table_name, record_key, deleted_at) VALUES (?1, ?2, ?3)", 
                            rusqlite::params![table, record_key, deleted_at]
                        ).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }

    // Generic blind replace (for one-way configs and readonly tables)
    let mut import_replace =
        |table: &str, rows: Vec<serde_json::Value>, cols: &[&str]| -> Result<(), rusqlite::Error> {
            if rows.is_empty() {
                return Ok(());
            }
            let placeholders = vec!["?"; cols.len()].join(", ");
            let query = format!(
                "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
                table,
                cols.join(", "),
                placeholders
            );
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
                    tx_sql.execute(&query, rusqlite::params_from_iter(params))?;
                }
            }
            Ok(())
        };

    // LWW (Last-Write-Wins) strategy with Conflict Detection
    let mut import_lww = |table: &str,
                          rows: Vec<serde_json::Value>,
                          cols: &[&str]|
     -> Result<(), rusqlite::Error> {
        if rows.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; cols.len()].join(", ");
        let query_insert = format!(
            "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
            table,
            cols.join(", "),
            placeholders
        );
        let query_check = format!("SELECT updated_at FROM {} WHERE id = ?1", table);

        for row in rows {
            if let Some(obj) = row.as_object() {
                let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let remote_updated_at = obj.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(0);

                let local_updated_at: i64 = tx_sql
                    .query_row(&query_check, rusqlite::params![id], |r| r.get(0))
                    .unwrap_or(0);

                // CONFLICT DETECTION
                let is_conflict = local_updated_at > last_sync_ts
                    && remote_updated_at > last_sync_ts
                    && local_updated_at != remote_updated_at;

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
                                val =
                                    serde_json::Value::String(format!("{} (手机同步冲突副本)", s));
                            }
                        }

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

                    // Insert the conflict copy
                    tx_sql.execute(&query_insert, rusqlite::params_from_iter(params))?;

                    // Record to sync_conflicts
                    let ts = crate::now_ms();
                    tx_sql.execute(
                        "INSERT INTO sync_conflicts (id, table_name, local_id, remote_id, status, created_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
                        rusqlite::params![ulid::Ulid::new().to_string(), table, id, conflict_id, ts]
                    )?;
                } else if remote_updated_at > local_updated_at {
                    // Normal LWW overwrite
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
                    tx_sql.execute(&query_insert, rusqlite::params_from_iter(params))?;
                }
            }
        }
        Ok(())
    };

    import_replace("settings", data.settings.clone(), &["key", "value"])
        .map_err(|e| e.to_string())?;
    import_lww(
        "conversations",
        data.conversations.clone(),
        &[
            "id",
            "title",
            "model",
            "cost",
            "last_message",
            "last_role",
            "created_at",
            "updated_at",
        ],
    )
    .map_err(|e| e.to_string())?;

    // Append-only strategy for messages (de-dupe by sync_id)
    if !data.messages.is_empty() {
        for msg in &data.messages {
            if let Some(obj) = msg.as_object() {
                let sync_id = obj.get("sync_id").and_then(|v| v.as_str()).unwrap_or("");
                if !sync_id.is_empty() {
                    let existing: i32 = tx_sql
                        .query_row(
                            "SELECT 1 FROM messages WHERE sync_id = ?1",
                            rusqlite::params![sync_id],
                            |_| Ok(1),
                        )
                        .unwrap_or(0);
                    if existing == 0 {
                        tx_sql.execute(
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
                        ).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }

    import_lww(
        "events",
        data.events.clone(),
        &[
            "id",
            "title",
            "type",
            "status",
            "date",
            "start_time",
            "end_time",
            "description",
            "created_at",
            "updated_at",
            "completed_at",
            "linked_ticket_id",
        ],
    )
    .map_err(|e| e.to_string())?;

    // Capture Journal uses a content-derived idempotency key across entry points and devices.
    // Keep the original capture_id, but accept a newer processing state from a peer.
    for capture in &data.captures {
        if let Some(obj) = capture.as_object() {
            crate::capture::merge_capture_record(&tx_sql, obj, ts)?;
        }
    }
    import_replace(
        "cron_jobs",
        data.cron_jobs.clone(),
        &[
            "id",
            "title",
            "cron_expr",
            "prompt_template",
            "enabled",
            "last_run",
            "created_at",
        ],
    )
    .map_err(|e| e.to_string())?;
    import_replace(
        "kg_nodes",
        data.kg_nodes.clone(),
        &[
            "id",
            "label",
            "node_type",
            "summary",
            "source",
            "metadata",
            "created_at",
        ],
    )
    .map_err(|e| e.to_string())?;
    import_replace(
        "kg_edges",
        data.kg_edges.clone(),
        &[
            "source_id",
            "target_id",
            "relation",
            "confidence",
            "created_at",
        ],
    )
    .map_err(|e| e.to_string())?;

    tx_sql.commit().map_err(|e| e.to_string())?;

    // Append to sync_history.json
    let history_path = crate::get_data_dir().join("sync_history.json");
    let mut history: Vec<serde_json::Value> =
        if let Ok(existing) = std::fs::read_to_string(&history_path) {
            serde_json::from_str(&existing).unwrap_or_default()
        } else {
            vec![]
        };

    let total_records = data.conversations.len()
        + data.messages.len()
        + data.events.len()
        + data.captures.len()
        + data.cron_jobs.len()
        + data.kg_nodes.len()
        + data.kg_edges.len();
    let mut detail_parts = Vec::new();
    if data.conversations.len() > 0 {
        detail_parts.push(format!("会话 {} 项", data.conversations.len()));
    }
    if data.messages.len() > 0 {
        detail_parts.push(format!("消息 {} 项", data.messages.len()));
    }
    if data.events.len() > 0 {
        detail_parts.push(format!("待办日程 {} 项", data.events.len()));
    }
    if data.captures.len() > 0 {
        detail_parts.push(format!("捕获记录 {} 项", data.captures.len()));
    }
    if data.settings.len() > 0 {
        detail_parts.push(format!("配置 {} 项", data.settings.len()));
    }
    if data.cron_jobs.len() > 0 {
        detail_parts.push(format!("定时任务 {} 项", data.cron_jobs.len()));
    }
    if data.kg_nodes.len() > 0 {
        detail_parts.push(format!("知识节点 {} 项", data.kg_nodes.len()));
    }

    let detail_str = if detail_parts.is_empty() {
        "成功合并云端数据 (无新增)".to_string()
    } else {
        format!("同步更新：{}", detail_parts.join(", "))
    };

    history.insert(
        0,
        serde_json::json!({
            "timestamp": ts,
            "direction": "pull",
            "counts": {
                "conversations": data.conversations.len(),
                "messages": data.messages.len(),
                "events": data.events.len(),
                "captures": data.captures.len(),
                "settings": data.settings.len(),
                "cron_jobs": data.cron_jobs.len(),
                "kg_nodes": data.kg_nodes.len(),
                "kg_edges": data.kg_edges.len()
            },
            "total_records": total_records,
            "detail": detail_str
        }),
    );

    if history.len() > 50 {
        history.truncate(50);
    } // Keep last 50
    if let Ok(json_str) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(&history_path, json_str);
    }

    Ok(())
}

pub fn log_sync_action(action: &str, status: &str, detail: &str) {
    let history_path = crate::get_data_dir().join("sync_history.json");
    let mut history: Vec<serde_json::Value> =
        if let Ok(existing) = std::fs::read_to_string(&history_path) {
            serde_json::from_str(&existing).unwrap_or_default()
        } else {
            vec![]
        };

    history.insert(
        0,
        serde_json::json!({
            "timestamp": crate::now_ms(),
            "action": action,
            "status": status,
            "detail": detail
        }),
    );

    if history.len() > 50 {
        history.truncate(50);
    }

    if let Ok(json_str) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(history_path, json_str);
    }
}

async fn do_active_sync(
    app: AppHandle,
    payload: SyncCommandPayload,
    trace: Option<SyncTraceContext>,
) -> Result<TransportKind, String> {
    info!(
        "[Sync Engine] Starting active sync to device {}",
        payload.device_id
    );

    // ── Stage: Route Discovery ──
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "route_discovery", "status": "running", "detail": "多路并发探测中..."}));

    use futures_util::stream::FuturesUnordered;
    use futures_util::stream::StreamExt;

    let config = crate::read_config();
    let my_device_id = config
        .get("device_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let my_device_name = config
        .get("deviceName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let platform = std::env::consts::OS.to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(5000)) // 增加到 5 秒以防部分手机休眠唤醒慢
        .build()
        .map_err(|e| e.to_string())?;

    let mut tasks = FuturesUnordered::new();

    for ip in &payload.local_ips {
        let ip_clone = ip.clone();
        let client_clone = client.clone();
        let port = payload.port;
        tasks.push(tauri::async_runtime::spawn(async move {
            let url = format!("http://{}:{}/v1/health", ip_clone, port);
            match client_clone.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    Some((TransportKind::Lan, Some(ip_clone)))
                }
                _ => None,
            }
        }));
    }

    if !payload.skip_relay {
        let target_device_id = payload.device_id.clone();
        let public_key = payload.public_key.clone();
        let device_name = my_device_name.clone();
        let platform_clone = platform.clone();

        tasks.push(tauri::async_runtime::spawn(async move {
            let mut waited = 0;
            // 手机刚扫码唤醒时，后台 Websocket 可能还未重连成功。给予最多 8 秒的重连宽限期。
            while !RELAY_CONNECTED.load(Ordering::SeqCst) && waited < 16 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                waited += 1;
            }

            if !RELAY_CONNECTED.load(Ordering::SeqCst) {
                return None;
            }

            let msg = serde_json::json!({
                "type": "notify",
                "target_device_id": target_device_id,
                "protocol_version": SYNC_PROTOCOL_VERSION,
                "trace_id": uuid::Uuid::new_v4().to_string(),
                "message_id": uuid::Uuid::new_v4().to_string(),
                "sync_id": uuid::Uuid::new_v4().to_string(),
                "payload": {
                    "device_name": device_name,
                    "platform": platform_clone,
                    "auth_code": public_key
                }
            });

            match send_relay_request_and_wait(
                msg,
                tokio::time::Duration::from_secs(5),
                RelayTerminal::Ack,
            )
            .await
            {
                Ok(json) => {
                    if json.get("error").is_none() {
                        Some((TransportKind::Relay, None))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        }));
    }

    let mut winning_transport = None;
    while let Some(res) = tasks.next().await {
        if let Ok(Some(transport)) = res {
            winning_transport = Some(transport);
            break;
        }
    }

    let (transport, lan_ip) = match winning_transport {
        Some(t) => t,
        None => {
            let err_msg = "ERR-SYNC-01: 所有网络通道 (LAN/Relay) 均不可达或超时";
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "route_discovery", "status": "error", "detail": err_msg}));
            log_sync_action("Route Discovery", "error", err_msg);
            return Err(err_msg.to_string());
        }
    };

    let route_name = if transport == TransportKind::Lan {
        format!("局域网 ({})", lan_ip.as_deref().unwrap_or("unknown"))
    } else {
        "Relay 外网".to_string()
    };
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "route_discovery", "status": "done", "detail": format!("通道建立成功: {}", route_name)}));
    info!("[Sync Engine] Route discovery won by {:?}", transport);

    let last_sync_ts: i64 = match app.state::<crate::db::DbState>().0.lock() {
        Ok(conn) => {
            let s: String = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'last_sync_ts'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| "0".to_string());
            s.parse::<i64>().unwrap_or(0)
        }
        Err(_) => 0,
    };

    if transport == TransportKind::Lan {
        let ip = lan_ip.unwrap();
        let base_url = format!("http://{}:{}", ip, payload.port);
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "running", "detail": format!("通过 {} 传输数据", ip)}));

        let client_full = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        let pull_url = format!("{}/v1/sync/pull", base_url);
        match client_full
            .get(&pull_url)
            .header("X-Device-Id", &my_device_id)
            .header("X-Platform", &platform)
            .header("X-Device-Name", &my_device_name)
            .header("X-Since-Ts", last_sync_ts.to_string())
            .header("Authorization", &payload.public_key)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(data_val) = json.get("data") {
                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone())
                        {
                            if let Err(e) = import_sync_data(&app, sync_data, last_sync_ts) {
                                let err_msg = format!("导入失败: {}", e);
                                let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "error", "detail": &err_msg}));
                                return Err(err_msg);
                            }

                            let now = crate::now_ms();
                            if let Ok(conn) = app.state::<crate::db::DbState>().0.lock() {
                                let _ = conn.execute("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []);
                                let _ = conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('last_sync_ts', ?1)", rusqlite::params![now.to_string()]);
                            }

                            let _ = app.emit(
                                "sync:progress",
                                serde_json::json!({"stage": "skills_sync", "status": "running"}),
                            );
                            let _ = client_full
                                .get(format!("{}/v1/sync/skills/download", base_url))
                                .header("X-Device-Id", &my_device_id)
                                .header("X-Device-Name", &my_device_name)
                                .header("Authorization", &payload.public_key)
                                .send()
                                .await
                                .map(|r| async {
                                    if let Ok(bytes) = r.bytes().await {
                                        let _ = crate::skills_sync::unpack_skills(
                                            &bytes,
                                            &crate::get_external_skills_dir_or_default(&config),
                                        );
                                    }
                                });

                            let _ = app.emit(
                                "sync:progress",
                                serde_json::json!({"stage": "notes_sync", "status": "running"}),
                            );
                            let _ = client_full
                                .get(format!("{}/v1/sync/notes/download", base_url))
                                .header("X-Device-Id", &my_device_id)
                                .header("X-Device-Name", &my_device_name)
                                .header("Authorization", &payload.public_key)
                                .send()
                                .await
                                .map(|r| async {
                                    if let Ok(bytes) = r.bytes().await {
                                        let _ = crate::skills_sync::unpack_skills(
                                            &bytes,
                                            &crate::get_data_dir().join("notebook").join("notes"),
                                        );
                                    }
                                });

                            let _ =
                                app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                        }
                    } else if let Some(config_val) = json.get("config") {
                        crate::write_config(&config_val);
                        let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                    }
                }
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::UNAUTHORIZED => {
                let err_msg = "ERR-SYNC-05: 鉴权失败 (无效的配对凭证)";
                let _ = app.emit(
                    "sync:progress",
                    serde_json::json!({"stage": "lan_sync", "status": "error", "detail": err_msg}),
                );
                return Err(err_msg.to_string());
            }
            Ok(resp) => {
                let err_msg = format!("HTTP {}", resp.status());
                let _ = app.emit(
                    "sync:progress",
                    serde_json::json!({"stage": "lan_sync", "status": "error", "detail": &err_msg}),
                );
                return Err(err_msg);
            }
            Err(e) => {
                let err_msg = e.to_string();
                let _ = app.emit(
                    "sync:progress",
                    serde_json::json!({"stage": "lan_sync", "status": "error", "detail": &err_msg}),
                );
                return Err(err_msg);
            }
        }

        use std::fs;
        let outbox_path = get_mobile_outbox_path();
        if outbox_path.exists() {
            if let Ok(data) = fs::read_to_string(&outbox_path) {
                if let Ok(mock_outbox) = serde_json::from_str::<serde_json::Value>(&data) {
                    let outbox_url = format!("{}/v1/sync/push", base_url);
                    let _ = client_full
                        .post(&outbox_url)
                        .header("X-Device-Id", &my_device_id)
                        .header("X-Platform", &platform)
                        .header("X-Device-Name", &my_device_name)
                        .header("Authorization", &payload.public_key)
                        .json(&mock_outbox)
                        .send()
                        .await;
                    let _ = fs::remove_file(&outbox_path);
                }
            }
        }

        if let Ok(local_sync_data) = export_sync_data(&app, last_sync_ts, false) {
            let push_db_url = format!("{}/v1/sync/push_db", base_url);
            let _ = client_full
                .post(&push_db_url)
                .header("X-Device-Id", &my_device_id)
                .header("X-Platform", &platform)
                .header("X-Device-Name", &my_device_name)
                .header("Authorization", &payload.public_key)
                .json(&local_sync_data)
                .send()
                .await;
        }

        let _ = app.emit(
            "sync:progress",
            serde_json::json!({"stage": "lan_sync", "status": "done"}),
        );
        return Ok(TransportKind::Lan);
    } else {
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "running", "detail": "通过 Relay 传输数据..."}));

        let relay_trace_id = trace
            .as_ref()
            .map(|value| value.trace_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let relay_sync_id = trace
            .as_ref()
            .map(|value| value.sync_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let pull_req = serde_json::json!({
            "type": "proxy",
            "target_device_id": payload.device_id,
            "protocol_version": SYNC_PROTOCOL_VERSION,
            "trace_id": relay_trace_id,
            "message_id": uuid::Uuid::new_v4().to_string(),
            "sync_id": relay_sync_id,
            "payload": {
                "action": "pull",
                "auth_code": payload.public_key
            }
        });

        match send_relay_request_and_wait(
            pull_req,
            tokio::time::Duration::from_secs(45),
            RelayTerminal::ProxyResponse,
        )
        .await
        {
            Ok(response) => {
                if let Some(inner_payload) = response.get("payload") {
                    if let Some(data_val) = inner_payload.get("data") {
                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone())
                        {
                            if let Err(e) = import_sync_data(&app, sync_data, 0) {
                                let err_msg = format!("导入失败: {}", e);
                                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                                return Err(err_msg);
                            }
                            let _ =
                                app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                        }
                    } else {
                        let err_msg = "响应中缺少 data 字段".to_string();
                        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                        return Err(err_msg);
                    }
                } else {
                    let err_msg = "响应中缺少 payload 字段".to_string();
                    let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                    return Err(err_msg);
                }
            }
            Err(e) => {
                let err_msg = format!("Relay Pull 失败: {}", e);
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                return Err(err_msg);
            }
        }

        use std::fs;
        let outbox_path = get_mobile_outbox_path();
        if outbox_path.exists() {
            if let Ok(data) = fs::read_to_string(&outbox_path) {
                if let Ok(mock_outbox) = serde_json::from_str::<serde_json::Value>(&data) {
                    let push_req = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": payload.device_id,
                        "trace_id": trace.as_ref().map(|t| t.trace_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                        "message_id": uuid::Uuid::new_v4().to_string(),
                        "sync_id": trace.as_ref().map(|t| t.sync_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                        "protocol_version": SYNC_PROTOCOL_VERSION,
                        "payload": {
                            "action": "push",
                            "auth_code": payload.public_key,
                            "data": mock_outbox
                        }
                    });

                    if let Ok(_) = send_relay_request_and_wait(
                        push_req,
                        tokio::time::Duration::from_secs(45),
                        RelayTerminal::CommitAck,
                    )
                    .await
                    {
                        let _ = fs::remove_file(&outbox_path);
                    } else {
                        log::warn!(
                            "[Sync Engine] Push request failed to get CommitAck, outbox retained."
                        );
                    }
                }
            }
        }

        if let Ok(local_sync_data) = export_sync_data(&app, last_sync_ts, true) {
            let push_db_req = serde_json::json!({
                "type": "proxy",
                "target_device_id": payload.device_id,
                "trace_id": trace.as_ref().map(|t| t.trace_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                "message_id": uuid::Uuid::new_v4().to_string(),
                "sync_id": trace.as_ref().map(|t| t.sync_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                "protocol_version": SYNC_PROTOCOL_VERSION,
                "payload": {
                    "action": "push_db",
                    "auth_code": payload.public_key,
                    "data": local_sync_data
                }
            });
            let _ = send_relay_request_and_wait(
                push_db_req,
                tokio::time::Duration::from_secs(45),
                RelayTerminal::CommitAck,
            )
            .await;
        }

        let _ = app.emit(
            "sync:progress",
            serde_json::json!({"stage": "relay_sync", "status": "done"}),
        );
        return Ok(TransportKind::Relay);
    }
}

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::client_async_tls;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// URL-encode Base64 device_id to prevent +, /, = from being mangled in URL paths
fn url_encode_device_id(id: &str) -> String {
    id.replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D")
}

async fn connect_websocket_robust(
    ws_url: &str,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    String,
> {
    log::info!(
        "[WS Robust] Connecting to {} using standard connect_async",
        ws_url
    );
    tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| e.to_string())
}

pub fn start_relay_listener(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            crate::sync_diagnostics::set_relay_state(
                crate::sync_diagnostics::RelayConnectionState::Connecting,
            );
            // Re-fetch device_id on every reconnect attempt to handle Identity resets
            let mut current_device_id = String::new();
            for _ in 0..10 {
                let config = crate::read_config();
                if let Some(id) = config.get("device_id").and_then(|v| v.as_str()) {
                    current_device_id = id.to_string();
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            if current_device_id.is_empty() {
                crate::sync_diagnostics::set_local_identity(
                    crate::sync_diagnostics::LocalIdentityState::Uninitialized,
                );
                crate::sync_diagnostics::set_relay_state(
                    crate::sync_diagnostics::RelayConnectionState::Disconnected,
                );
                log::warn!("[Sync Engine] start_relay_listener: could not get device_id, retrying in 5s...");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            crate::sync_diagnostics::set_local_identity(
                crate::sync_diagnostics::LocalIdentityState::Ready,
            );

            let relay_url = "wss://relay.bobbik.org".to_string();
            let ws_url = format!(
                "{}/ws/device/{}",
                relay_url,
                url_encode_device_id(&current_device_id)
            );

            match connect_websocket_robust(&ws_url).await {
                Ok((mut ws_stream, _)) => {
                    log::info!("[Sync Engine] Connected to Relay WebSocket: {}", ws_url);
                    RELAY_CONNECTED.store(true, Ordering::SeqCst);
                    crate::sync_diagnostics::set_relay_state(
                        crate::sync_diagnostics::RelayConnectionState::Registered,
                    );

                    // Explicitly register device ID (fixes NGINX URL stripping bugs)
                    let reg_msg = serde_json::json!({
                        "type": "register",
                        "deviceId": current_device_id
                    });
                    let _ = ws_stream
                        .send(Message::Text(reg_msg.to_string().into()))
                        .await;

                    use futures_util::{SinkExt, StreamExt};
                    let (mut tx, mut rx) = ws_stream.split();
                    let (tx_mpsc, mut rx_mpsc) = tokio::sync::mpsc::channel::<Message>(100);
                    let (reconnect_tx, mut reconnect_rx) = tokio::sync::mpsc::channel::<()>(1);
                    let mut ping_interval =
                        tokio::time::interval(std::time::Duration::from_secs(15));

                    {
                        let mut lock = RELAY_TX.write().unwrap();
                        *lock = Some(tx_mpsc.clone());
                    }
                    {
                        let mut lock = PENDING_REQUESTS.write().unwrap();
                        lock.clear();
                    }
                    {
                        let mut lock = RELAY_RECONNECT_TRIGGER.lock().unwrap();
                        *lock = Some(reconnect_tx);
                    }

                    let mut last_activity = crate::now_ms();

                    loop {
                        tokio::select! {
                            _ = reconnect_rx.recv() => {
                                log::info!("[Sync Engine] Received manual reconnect trigger!");
                                break;
                            }
                            _ = ping_interval.tick() => {
                                if crate::now_ms() - last_activity > 45_000 {
                                    log::error!("[Sync Engine] Relay connection timeout: No activity for 45s. Reconnecting...");
                                    break;
                                }

                                // Check if device_id was reset/changed by the user
                                let latest_device_id = crate::read_config().get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                                if latest_device_id != "unknown" && latest_device_id != current_device_id {
                                    log::info!("[Sync Engine] Device ID changed! Breaking relay connection to reconnect...");
                                    break;
                                }

                                let _ = tx_mpsc.send(Message::Ping(bytes::Bytes::new())).await;
                            }
                            mpsc_msg_opt = rx_mpsc.recv() => {
                                if let Some(msg) = mpsc_msg_opt {
                                    if let Err(e) = tx.send(msg).await {
                                        log::error!("[Sync Engine] Failed to send WS message: {}", e);
                                        break;
                                    }
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
                                last_activity = crate::now_ms();
                                match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    // Intercept routed responses
                                    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                        if msg_type == "diagnostic_receipt" {
                                            let peer_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                            record_relay_receipt(&json, peer_id);
                                            if json.get("status").and_then(|v| v.as_str()) == Some("failed") {
                                                if let Some(ref_msg_id) = json.get("ref_message_id").and_then(|v| v.as_str()).or_else(|| json.get("message_id").and_then(|v| v.as_str())) {
                                                    let mut pending = PENDING_REQUESTS.write().unwrap();
                                                    if let Some(waiter) = pending.remove(ref_msg_id) {
                                                        let mut err_resp = json.clone();
                                                        err_resp["type"] = serde_json::json!("error");
                                                        err_resp["error"] = serde_json::json!(format!("{}: Relay delivery failed", json.get("error_code").and_then(|v| v.as_str()).unwrap_or("RLY-UNKNOWN")));
                                                        let _ = waiter.tx.send(err_resp);
                                                    }
                                                }
                                            }
                                            continue;
                                        }
                                    }

                                    let target_msg_id = json.get("ref_message_id").and_then(|v| v.as_str()).or_else(|| json.get("message_id").and_then(|v| v.as_str()));
                                    if let Some(ref_msg_id) = target_msg_id {
                                        let mut pending = PENDING_REQUESTS.write().unwrap();
                                        let is_match = if let Some(waiter) = pending.get(ref_msg_id) {
                                            let term_match = match waiter.terminal {
                                                RelayTerminal::Ack => json.get("type").and_then(|v| v.as_str()) == Some("ack"),
                                                RelayTerminal::CommitAck => json.get("type").and_then(|v| v.as_str()) == Some("commit_ack"),
                                                RelayTerminal::ProxyResponse => {
                                                    json.get("type").and_then(|v| v.as_str()) == Some("proxy") &&
                                                    json.get("payload").and_then(|p| p.get("action")).map(|v| v.as_str() == Some("pull_response") || v.as_str() == Some("error")).unwrap_or(false)
                                                }
                                                RelayTerminal::AnyResponse => true,
                                            };
                                            term_match || json.get("type").and_then(|v| v.as_str()) == Some("error") || json.get("type").and_then(|v| v.as_str()) == Some("proxy_error")
                                        } else {
                                            false
                                        };

                                        if is_match {
                                            if let Some(waiter) = pending.remove(ref_msg_id) {
                                                let _ = waiter.tx.send(json.clone());
                                                continue;
                                            }
                                        }
                                    }

                                    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                        if msg_type == "notify" {
                                            let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                            log::info!("[Sync Engine] Received notify from {}", from_id);

                                            // Verify auth code
                                            let provided_auth = json.get("payload").and_then(|p| p.get("auth_code")).and_then(|a| a.as_str());
                                            let expected_auth_res = crate::crypto::get_pairing_payload(app.state::<crate::crypto::DeviceIdentityState>());
                                            let expected_auth = expected_auth_res.as_ref().map(|p| p.public_key.as_str()).unwrap_or_default();
                                            if provided_auth != Some(expected_auth) {
                                                if expected_auth_res.is_err() {
                                                    log::error!("[Sync Engine] Auth code mismatch in notify from {}: Local keys are locked/unavailable", from_id);
                                                } else {
                                                    log::error!("[Sync Engine] Auth code mismatch in notify from {}", from_id);
                                                }
                                                let _ = crate::sync_history::record_activity(
                                                    DiagnosticStatus::Failed,
                                                    Some(TransportKind::Relay),
                                                    Some(from_id.to_string()),
                                                    "拒绝移动端 Relay 配对请求",
                                                    Some("ERR-PAIRING-04".to_string()),
                                                );
                                                let mut ack = serde_json::json!({
                                                    "type": "ack",
                                                    "target_device_id": from_id,
                                                    "error": "Unauthorized"
                                                });
                                                copy_trace_fields(&json, &mut ack, true);
                                                let _ = tx_mpsc.send(Message::Text(ack.to_string().into())).await;
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
                                            let mut ack = serde_json::json!({
                                                "type": "ack",
                                                "target_device_id": from_id,
                                            });
                                            copy_trace_fields(&json, &mut ack, true);
                                            let _ = tx_mpsc.send(Message::Text(ack.to_string().into())).await;
                                            let _ = crate::sync_history::record_activity(
                                                DiagnosticStatus::Success,
                                                Some(TransportKind::Relay),
                                                Some(from_id.to_string()),
                                                "已确认移动端 Relay 配对",
                                                None,
                                            );

                                            // Emit to frontend
                                            let _ = app.emit("sync:device_connected", serde_json::json!({
                                                "device_id": from_id,
                                                "platform": platform,
                                                "device_name": device_name
                                            }));
                                        } else if msg_type == "wakeup" {
                                            let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                            log::info!("[Sync Engine] Received wakeup from {}", from_id);
                                            log_sync_action("Relay Wakeup", "done", &format!("Received wakeup from PC {}", from_id));

                                            // Emit to frontend to trigger mobile sync
                                            let _ = app.emit("sync:wakeup", serde_json::json!({
                                                "device_id": from_id
                                            }));
                                        } else if msg_type == "proxy" {
                                            if let Some(inner_payload) = json.get("payload") {
                                                let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");

                                                // Verify auth code for proxy
                                                let provided_auth = inner_payload.get("auth_code").and_then(|a| a.as_str());
                                                let expected_auth_res = crate::crypto::get_pairing_payload(app.state::<crate::crypto::DeviceIdentityState>());
                                                let expected_auth = expected_auth_res.as_ref().map(|p| p.public_key.as_str()).unwrap_or_default();
                                                if provided_auth != Some(expected_auth) {
                                                    if expected_auth_res.is_err() {
                                                        log::error!("[Sync Engine] Auth code mismatch in proxy from {}: Local keys are locked/unavailable", from_id);
                                                    } else {
                                                        log::error!("[Sync Engine] Auth code mismatch in proxy from {}", from_id);
                                                    }
                                                    let err_resp = serde_json::json!({
                                                        "type": "proxy",
                                                        "target_device_id": from_id,
                                                        "payload": {
                                                            "action": "error",
                                                            "error": "Unauthorized"
                                                        }
                                                    });
                                                    let _ = tx_mpsc.send(Message::Text(err_resp.to_string().into())).await;
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
                                                    if let Ok(sync_data) = export_sync_data(&app, since_ts, true) {
                                                        let mut pull_resp = serde_json::json!({
                                                            "type": "proxy",
                                                            "target_device_id": from_id,
                                                            "payload": {
                                                                "action": "pull_response",
                                                                "data": sync_data
                                                            }
                                                        });
                                                        copy_trace_fields(&json, &mut pull_resp, true);
                                                        if let Err(e) = tx_mpsc.send(Message::Text(pull_resp.to_string().into())).await {
                                                            log::error!("[Sync Engine] Failed to send pull_response to {}: {}", from_id, e);
                                                            let _ = crate::sync_history::record_activity(
                                                                DiagnosticStatus::Failed,
                                                                Some(TransportKind::Relay),
                                                                Some(from_id.to_string()),
                                                                "向移动端返回同步数据失败",
                                                                Some("ERR-SYNC-RELAY-SEND".to_string()),
                                                            );
                                                        } else {
                                                            log::info!("[Sync Engine] Sent pull_response to {} successfully", from_id);
                                                            let _ = crate::sync_history::record_activity(
                                                                DiagnosticStatus::Success,
                                                                Some(TransportKind::Relay),
                                                                Some(from_id.to_string()),
                                                                "已通过 Relay 向移动端返回同步数据",
                                                                None,
                                                            );
                                                        }
                                                    }
                                                } else if action == "push" {
                                                    log::info!("[Sync Engine] Received proxy push request from {}", from_id);
                                                    if let Some(data_val) = inner_payload.get("data") {
                                                        if let Some(arr) = data_val.as_array() {
                                                            log::info!("[Sync Engine] Pushing {} operations to PC outbox via Relay", arr.len());
                                                            crate::outbox::write_outbox(arr.clone());

                                                            let mut commit_ack = serde_json::json!({
                                                                "type": "commit_ack",
                                                                "target_device_id": from_id,
                                                            });
                                                            copy_trace_fields(&json, &mut commit_ack, true);
                                                            let _ = tx_mpsc.send(Message::Text(commit_ack.to_string().into())).await;
                                                        }
                                                    }
                                                } else if action == "rpc_request" {
                                                    log::info!("[Sync Engine] Received proxy rpc_request from {}", from_id);
                                                    let request_id = inner_payload.get("request_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                    let instruction = inner_payload.get("instruction").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                    let from_id_clone = from_id.to_string();
                                                    let app_clone = app.clone();
                                                    let tx_mpsc_clone = tx_mpsc.clone();

                                                    tauri::async_runtime::spawn(async move {
                                                        let msgs = vec![serde_json::json!({
                                                            "role": "user",
                                                            "content": format!("[移动端 RPC 指令]\n{}", instruction)
                                                        })];

                                                        let conv_id = format!("headless_{}", request_id);
                                                        log::info!("[Sync Engine] Executing Headless Agent for RPC {}", request_id);

                                                        let result = crate::llm::stream_chat(
                                                            app_clone,
                                                            msgs,
                                                            Some(conv_id.clone()),
                                                            None,
                                                            true,
                                                            "standard".to_string(),
                                                        ).await;

                                                        let result_text = if let Some(arr) = result.as_array() {
                                                            if let Some(last) = arr.last() {
                                                                last.get("content").and_then(|v| v.as_str()).unwrap_or("Empty response").to_string()
                                                            } else {
                                                                "Empty array".to_string()
                                                            }
                                                        } else {
                                                            "Error formatting result".to_string()
                                                        };

                                                        let resp = serde_json::json!({
                                                            "type": "proxy",
                                                            "target_device_id": from_id_clone,
                                                            "payload": {
                                                                "action": "rpc_response",
                                                                "request_id": request_id,
                                                                "status": "success",
                                                                "result": result_text
                                                            }
                                                        });

                                                        let _ = tx_mpsc_clone.send(Message::Text(resp.to_string().into())).await;
                                                    });
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
                                                                let _ = crate::sync_history::record_activity(
                                                                    DiagnosticStatus::Failed,
                                                                    Some(TransportKind::Relay),
                                                                    Some(from_id.to_string()),
                                                                    "Relay 数据写入 PC 失败",
                                                                    Some("ERR-SYNC-RELAY-IMPORT".to_string()),
                                                                );

                                                                let mut err_resp = serde_json::json!({
                                                                    "type": "error",
                                                                    "target_device_id": from_id,
                                                                    "error": format!("DB Import failed: {}", e)
                                                                });
                                                                copy_trace_fields(&json, &mut err_resp, true);
                                                                let _ = tx_mpsc.send(Message::Text(err_resp.to_string().into())).await;
                                                            } else {
                                                                let now = crate::now_ms();
                                                                if let Ok(conn) = app.state::<crate::db::DbState>().0.lock() {
                                                                    let _ = conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('last_sync_ts', ?1)", rusqlite::params![now.to_string()]);
                                                                }
                                                                let _ = crate::sync_history::record_activity(
                                                                    DiagnosticStatus::Success,
                                                                    Some(TransportKind::Relay),
                                                                    Some(from_id.to_string()),
                                                                    "已通过 Relay 接收并写入移动端数据",
                                                                    None,
                                                                );

                                                                let mut commit_ack = serde_json::json!({
                                                                    "type": "commit_ack",
                                                                    "target_device_id": from_id,
                                                                });
                                                                copy_trace_fields(&json, &mut commit_ack, true);
                                                                let _ = tx_mpsc.send(Message::Text(commit_ack.to_string().into())).await;
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
                    RELAY_CONNECTED.store(false, Ordering::SeqCst);
                } // closes Ok((ws_stream, _)) =>
                Err(e) => {
                    log::error!("[Sync Engine] Failed to connect to Relay: {}", e);
                    RELAY_CONNECTED.store(false, Ordering::SeqCst);
                }
            }
            RELAY_CONNECTED.store(false, Ordering::SeqCst);
            crate::sync_diagnostics::set_relay_state(
                crate::sync_diagnostics::RelayConnectionState::Disconnected,
            );

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

#[tauri::command]
pub fn get_shared_intents(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let mut results = vec![];
    if let Ok(cache_dir) = app.path().cache_dir() {
        let incoming_dir = cache_dir.join("shared_incoming");
        if let Ok(entries) = std::fs::read_dir(&incoming_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();

                if file_name.ends_with(".txt") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        results.push(serde_json::json!({
                            "type": "text",
                            "filename": file_name,
                            "content": content
                        }));
                    }
                } else {
                    // Treat as image or binary file
                    results.push(serde_json::json!({
                        "type": "file",
                        "filename": file_name,
                        "path": path.to_string_lossy().into_owned()
                    }));
                }
            }
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn clear_shared_intent(app: tauri::AppHandle, filename: String) -> Result<(), String> {
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "分享缓存文件名无效".to_string())?;
    if safe_name != filename {
        return Err("分享缓存路径越界".to_string());
    }
    if let Ok(cache_dir) = app.path().cache_dir() {
        let incoming_dir = cache_dir.join("shared_incoming");
        let file_path = incoming_dir.join(safe_name);
        if file_path.exists() {
            std::fs::remove_file(file_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_p2p_relay_status() -> bool {
    RELAY_CONNECTED.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn force_relay_reconnect() {
    log::info!("[Sync Engine] Force reconnect triggered by frontend network change");
    if let Some(tx) = RELAY_RECONNECT_TRIGGER.lock().unwrap().as_ref() {
        let _ = tx.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_receipt_does_not_complete_waiter() {
        let waiter = RelayRequestWaiter {
            terminal: RelayTerminal::Ack, // We wait for Ack, not receipt
            tx: tokio::sync::oneshot::channel().0,
        };

        let receipt = json!({
            "type": "receipt",
            "receipt": "relay_delivered_to_target",
            "message_id": "msg1"
        });

        // This is a receipt, it should NOT complete the waiter since the terminal is Ack
        assert_ne!(receipt.get("type").unwrap().as_str().unwrap(), "ack");
        assert_eq!(waiter.terminal, RelayTerminal::Ack);
    }
}
