use serde_json::{json, Value};
use tauri::AppHandle;
use std::time::Duration;
use rusqlite::params;
use tokio::sync::Mutex;
use std::sync::Arc;
use futures_util::{StreamExt, SinkExt};

lazy_static::lazy_static! {
    static ref DISCORD_RUNNING: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
}

/// T-1501: Discord Bot Integration
pub async fn init(app: AppHandle) {
    let db_path = crate::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let _ = conn.execute_batch("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
        if let Ok(token) = conn.query_row(
            "SELECT value FROM settings WHERE key = 'discord_token'",
            [],
            |row| row.get::<_, String>(0)
        ) {
            if !token.is_empty() {
                start_discord_bot(app, token).await;
            }
        }
    }
}

#[tauri::command]
pub async fn system_save_discord_token(app: AppHandle, token: String) -> Value {
    let db_path = crate::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('discord_token', ?1)",
            params![token]
        );
        
        let mut running = DISCORD_RUNNING.lock().await;
        if !*running && !token.is_empty() {
            start_discord_bot(app, token).await;
            *running = true;
        }
        json!({ "ok": true })
    } else {
        json!({ "error": "Database error" })
    }
}

#[tauri::command]
pub async fn system_get_discord_token() -> Value {
    let db_path = crate::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        if let Ok(token) = conn.query_row(
            "SELECT value FROM settings WHERE key = 'discord_token'",
            [],
            |row| row.get::<_, String>(0)
        ) {
            return json!({ "ok": true, "token": token });
        }
    }
    json!({ "ok": true, "token": "" })
}

pub async fn start_discord_bot(app: AppHandle, token: String) {
    let mut running = DISCORD_RUNNING.lock().await;
    if *running { return; }
    *running = true;
    
    tokio::spawn(async move {
        log::info!("Discord bot starting with token len {}", token.len());
        
        let client = reqwest::Client::new();
        let gateway_url = "wss://gateway.discord.gg/?v=10&encoding=json";

        loop {
            match tokio_tungstenite::connect_async(gateway_url).await {
                Ok((ws_stream, _)) => {
                    log::info!("Discord connected to Gateway");
                    let (mut write, mut read) = ws_stream.split();
                    
                    let mut heartbeat_interval = 41250;
                    let seq: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));
                    
                    // Wait for Hello (Opcode 10)
                    if let Some(Ok(msg)) = read.next().await {
                        if let Ok(text) = msg.to_text() {
                            if let Ok(payload) = serde_json::from_str::<Value>(text) {
                                if payload.get("op").and_then(|v| v.as_i64()) == Some(10) {
                                    if let Some(d) = payload.get("d") {
                                        if let Some(interval) = d.get("heartbeat_interval").and_then(|v| v.as_u64()) {
                                            heartbeat_interval = interval;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Send Identify (Opcode 2)
                    let intents = 512 | 4096 | 32768; // GUILD_MESSAGES | DIRECT_MESSAGES | MESSAGE_CONTENT
                    let identify = json!({
                        "op": 2,
                        "d": {
                            "token": token,
                            "intents": intents,
                            "properties": {
                                "os": "windows",
                                "browser": "bob-agent",
                                "device": "bob-agent"
                            }
                        }
                    });
                    
                    if let Err(e) = write.send(tokio_tungstenite::tungstenite::Message::Text(identify.to_string().into())).await {
                        log::error!("Discord identify error: {}", e);
                        continue;
                    }

                    // Start heartbeating in a separate task
                    let (hb_tx, mut hb_rx) = tokio::sync::mpsc::channel::<()>(1);
                    let seq_clone = seq.clone();
                    
                    // We need a lock around the websocket writer to share it.
                    let write_mutex = Arc::new(Mutex::new(write));
                    let write_for_hb = write_mutex.clone();

                    tokio::spawn(async move {
                        let mut interval = tokio::time::interval(Duration::from_millis(heartbeat_interval));
                        loop {
                            tokio::select! {
                                _ = interval.tick() => {
                                    let s = *seq_clone.lock().await;
                                    let heartbeat = json!({ "op": 1, "d": s });
                                    let mut w = write_for_hb.lock().await;
                                    let _ = w.send(tokio_tungstenite::tungstenite::Message::Text(heartbeat.to_string().into())).await;
                                }
                                _ = hb_rx.recv() => {
                                    break;
                                }
                            }
                        }
                    });

                    // Read loop
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                if let Ok(payload) = serde_json::from_str::<Value>(&text) {
                                    if let Some(s) = payload.get("s").and_then(|v| v.as_i64()) {
                                        *seq.lock().await = Some(s);
                                    }
                                    
                                    let op = payload.get("op").and_then(|v| v.as_i64());
                                    let t = payload.get("t").and_then(|v| v.as_str());
                                    
                                    if op == Some(0) && t == Some("MESSAGE_CREATE") {
                                        if let Some(d) = payload.get("d") {
                                            // Check if it's from a bot to avoid loops
                                            let is_bot = d.get("author").and_then(|a| a.get("bot")).and_then(|b| b.as_bool()).unwrap_or(false);
                                            if !is_bot {
                                                handle_message(&app, &client, &token, d).await;
                                            }
                                        }
                                    } else if op == Some(7) {
                                        // Reconnect requested
                                        break;
                                    } else if op == Some(9) {
                                        // Invalid Session
                                        break;
                                    }
                                }
                            }
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                                break;
                            }
                            Err(e) => {
                                log::error!("Discord ws read error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }

                    // Stop heartbeat task
                    let _ = hb_tx.send(()).await;
                }
                Err(e) => {
                    log::error!("Discord ws connect error: {}", e);
                }
            }
            
            // Reconnect delay
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn handle_message(app: &AppHandle, client: &reqwest::Client, token: &str, message: &Value) {
    if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
        let channel_id = message.get("channel_id").and_then(|v| v.as_str()).unwrap_or("");
        if channel_id.is_empty() || text.is_empty() { return; }

        let conv_id = format!("discord-{}", channel_id);
        log::info!("Discord received message from {}: {}", channel_id, text);

        // Send typing action
        let _ = client.post(&format!("https://discord.com/api/v10/channels/{}/typing", channel_id))
            .header("Authorization", format!("Bot {}", token))
            .send().await;

        let messages = vec![json!({ "role": "user", "content": text })];
        
        // Let the LLM process the message. Note we pass global_file_access = false
        let result = crate::llm::stream_chat(
            app.clone(), 
            messages, 
            Some(conv_id.clone()), 
            Some(channel_id.to_string()),
            false,
            "default".to_string()
        ).await;

        let reply = result.get("content").and_then(|v| v.as_str()).unwrap_or("[无响应]");
        
        // Send reply back
        let _ = client.post(&format!("https://discord.com/api/v10/channels/{}/messages", channel_id))
            .header("Authorization", format!("Bot {}", token))
            .json(&json!({ "content": reply }))
            .send().await;
    }
}
