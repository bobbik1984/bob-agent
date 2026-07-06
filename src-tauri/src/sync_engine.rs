use serde::{Deserialize, Serialize};
use tauri::{AppHandle, command, Manager, Emitter};
use std::sync::Arc;
use log::{info, error};

use std::path::PathBuf;
use std::fs;

use crate::lan_sync::LanSyncEngine;

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
        match client.get(&pull_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(config) = json.get("config") {
                        info!("[Sync Engine] Successfully pulled config from PC!");
                        // Save to local filesystem
                        crate::write_config(&config);
                        
                        // Emit config reconciled event so UI updates
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
                        match client.post(&outbox_url).json(&mock_outbox).send().await {
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
        error!("[Sync Engine] Failed to connect to any LAN IP. Fallback to relay not yet implemented.");
        return Err("LAN connection failed".to_string());
    }

    Ok(())
}
