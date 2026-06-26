use serde_json::{json, Value};
use tauri::AppHandle;
use std::time::Duration;
use rusqlite::params;
use tokio::sync::Mutex;
use std::sync::Arc;

lazy_static::lazy_static! {
    static ref TG_RUNNING: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
}

/// T-1500: Telegram Bot Integration
pub async fn init(app: AppHandle) {
    let db_path = crate::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let _ = conn.execute_batch("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
        if let Ok(token) = conn.query_row(
            "SELECT value FROM settings WHERE key = 'telegram_token'",
            [],
            |row| row.get::<_, String>(0)
        ) {
            if !token.is_empty() {
                start_telegram_bot(app, token).await;
            }
        }
    }
}

#[tauri::command]
pub async fn system_save_telegram_token(app: AppHandle, token: String) -> Value {
    let db_path = crate::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('telegram_token', ?1)",
            params![token]
        );
        
        let mut running = TG_RUNNING.lock().await;
        if !*running && !token.is_empty() {
            start_telegram_bot(app, token).await;
            *running = true;
        }
        json!({ "ok": true })
    } else {
        json!({ "error": "Database error" })
    }
}

#[tauri::command]
pub async fn system_get_telegram_token() -> Value {
    let db_path = crate::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        if let Ok(token) = conn.query_row(
            "SELECT value FROM settings WHERE key = 'telegram_token'",
            [],
            |row| row.get::<_, String>(0)
        ) {
            return json!({ "ok": true, "token": token });
        }
    }
    json!({ "ok": true, "token": "" })
}

pub async fn start_telegram_bot(app: AppHandle, token: String) {
    let mut running = TG_RUNNING.lock().await;
    if *running { return; }
    *running = true;
    
    tokio::spawn(async move {
        log::info!("Telegram bot starting with token len {}", token.len());
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap_or_default();
        let mut offset = 0;
        
        loop {
            let url = format!("https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=30", token, offset);
            match client.get(&url).send().await {
                Ok(resp) => {
                    if let Ok(json) = resp.json::<Value>().await {
                        if let Some(result) = json.get("result").and_then(|v| v.as_array()) {
                            for update in result {
                                if let Some(update_id) = update.get("update_id").and_then(|v| v.as_i64()) {
                                    offset = update_id + 1;
                                }
                                if let Some(message) = update.get("message") {
                                    handle_message(&app, &client, &token, message).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Telegram poll error: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });
}

async fn handle_message(app: &AppHandle, client: &reqwest::Client, token: &str, message: &Value) {
    if let Some(text) = message.get("text").and_then(|v| v.as_str()) {
        let chat_id = message.get("chat").and_then(|c| c.get("id")).and_then(|v| v.as_i64()).unwrap_or(0);
        let user_id = format!("tg-{}", chat_id);
        
        log::info!("Telegram received message from {}: {}", chat_id, text);

        // 拦截全局 /sessions 等指令
        if let Some(reply_text) = crate::im_sessions::handle_im_command(&user_id, text) {
            let _ = client.post(&format!("https://api.telegram.org/bot{}/sendMessage", token))
                .json(&serde_json::json!({ "chat_id": chat_id, "text": reply_text }))
                .send().await;
            return;
        }

        let conv_id = crate::im_sessions::get_or_create_conv_id(&user_id);

        // Send typing action
        let _ = client.post(&format!("https://api.telegram.org/bot{}/sendChatAction", token))
            .json(&json!({ "chat_id": chat_id, "action": "typing" }))
            .send().await;

        let messages = vec![json!({ "role": "user", "content": text })];
        
        // Let the LLM process the message. Note we pass global_file_access = false
        // By default, external bots shouldn't have arbitrary file access for security
        let result = crate::llm::stream_chat(
            app.clone(), 
            messages, 
            Some(conv_id.clone()), 
            Some(chat_id.to_string()),
            false,
            "default".to_string()
        ).await;

        let reply = result.get("content").and_then(|v| v.as_str()).unwrap_or("[无响应]");
        
        // Send reply back
        let _ = client.post(&format!("https://api.telegram.org/bot{}/sendMessage", token))
            .json(&json!({ "chat_id": chat_id, "text": reply }))
            .send().await;
    }
}
