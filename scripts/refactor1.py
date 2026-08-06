import re

path = r'D:\OneDrive\Learning\Code\Gemini\bob-agent\src-tauri\src\sync_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Refactor relay_tunnel_sync
match_tunnel = re.search(r'let config = crate::read_config\(\);.*?Ok\(sync_data\) => \{', content, re.DOTALL)
if match_tunnel:
    new_tunnel = '''
        if !RELAY_CONNECTED.load(Ordering::SeqCst) {
            let err_msg = "ERR-SYNC-02: Relay 后台未连接";
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": err_msg}));
            return Err(err_msg.to_string());
        }

        let relay_trace_id = trace.as_ref().map(|value| value.trace_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let relay_sync_id = trace.as_ref().map(|value| value.sync_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Send pull request
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

        info!("[Sync Engine] Sent proxy pull request. Waiting for response...");
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "running", "detail": "请求拉取数据 (对话、日程、票据...)"}));

        match send_relay_request_and_wait(pull_req, tokio::time::Duration::from_secs(45)).await {
            Ok(response) => {
                let sync_data = if let Some(inner_payload) = response.get("payload") {
                    if let Some(data_val) = inner_payload.get("data") {
                        serde_json::from_value::<SyncData>(data_val.clone()).map_err(|e| e.to_string())
                    } else {
                        Err("No data in pull_response".to_string())
                    }
                } else {
                    Err("No payload in pull_response".to_string())
                };

                match sync_data {
                    Ok(sync_data) => {
'''
    content = content[:match_tunnel.start()] + new_tunnel.strip() + content[match_tunnel.end() - 17:]

# Write back
with open(path, 'w', encoding='utf-8') as f:
    f.write(content)
