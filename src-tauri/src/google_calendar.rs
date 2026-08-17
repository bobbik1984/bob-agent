//! Google Calendar 连接器
//!
//! OAuth 2.0 Desktop App flow + Calendar API v3

use serde_json::{json, Value};

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GCAL_API_BASE: &str = "https://www.googleapis.com/calendar/v3";

// 内置 OAuth 凭据（Desktop App 类型，可公开）
const GOOGLE_CLIENT_ID: &str = "BOB_GOOGLE_CLIENT_ID"; // TODO: 替换为真实值
const GOOGLE_CLIENT_SECRET: &str = "BOB_GOOGLE_CLIENT_SECRET"; // TODO: 替换为真实值

const SCOPES: &str = "https://www.googleapis.com/auth/calendar.readonly \
    https://www.googleapis.com/auth/calendar.events \
    https://www.googleapis.com/auth/gmail.readonly \
    https://www.googleapis.com/auth/gmail.compose \
    https://www.googleapis.com/auth/gmail.labels";

/// 启动 Google OAuth 流程
pub async fn start_google_oauth() -> Value {
    let creds = match super::connector::load_credentials("google") {
        Some(c) => c,
        None => return json!({"error": "请先在设置中配置 Google OAuth 凭据 (Client ID/Secret)"}),
    };

    let client_id = creds.client_id.as_deref().unwrap_or(GOOGLE_CLIENT_ID);
    if client_id == GOOGLE_CLIENT_ID {
        return json!({"error": "请先配置真实的 Google OAuth 凭据，当前的默认凭据无效。"});
    }

    let (redirect_uri, code_rx) = super::connector::prepare_oauth_callback();

    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        GOOGLE_AUTH_URL,
        client_id,
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(SCOPES)
    );

    // 在后台等待 OAuth 回调
    tokio::spawn(async move {
        match tokio::time::timeout(std::time::Duration::from_secs(300), code_rx).await {
            Ok(Ok(code)) => {
                log::info!("[Google] Received OAuth code, exchanging for token...");
                if let Err(e) = exchange_code_for_token(&code, &redirect_uri).await {
                    log::error!("[Google] Token exchange failed: {}", e);
                }
            }
            Ok(Err(_)) => log::warn!("[Google] OAuth callback channel closed"),
            Err(_) => log::warn!("[Google] OAuth flow timed out (5min)"),
        }
    });

    json!({"ok": true, "url": auth_url})
}

/// 用授权码换取 token
async fn exchange_code_for_token(code: &str, redirect_uri: &str) -> Result<(), String> {
    let creds =
        super::connector::load_credentials("google").ok_or("No credentials found for Google")?;
    let client_id = creds.client_id.as_deref().unwrap_or(GOOGLE_CLIENT_ID);
    let client_secret = creds
        .client_secret
        .as_deref()
        .unwrap_or(GOOGLE_CLIENT_SECRET);

    if client_id == GOOGLE_CLIENT_ID {
        return Err("Please configure real Google OAuth credentials first.".to_string());
    }

    let client = reqwest::Client::new();
    let form_body = format!(
        "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code",
        urlencoding::encode(code),
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret),
        urlencoding::encode(redirect_uri),
    );
    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|e| format!("Token request failed: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Token parse failed: {}", e))?;

    if let Some(error) = body.get("error").and_then(|v| v.as_str()) {
        let desc = body
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return Err(format!("OAuth error: {} - {}", error, desc));
    }

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("Missing access_token")?
        .to_string();
    let refresh_token = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let expires_in = body
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let creds = super::connector::StoredCredentials {
        access_token: Some(access_token),
        refresh_token,
        expires_at: Some(now + expires_in),
        client_id: Some(client_id.to_string()),
        client_secret: Some(client_secret.to_string()),
        connected_at: Some(now),
        ..Default::default()
    };

    super::connector::save_credentials("google", &creds)?;
    log::info!("[Google] OAuth completed, credentials saved");
    Ok(())
}

/// 获取有效的 access token（自动刷新）
async fn get_token() -> Result<String, String> {
    let creds = super::connector::load_credentials("google").ok_or("Google 未连接")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Token 未过期
    if let Some(ref token) = creds.access_token {
        if let Some(expires_at) = creds.expires_at {
            if expires_at > now + 60 {
                return Ok(token.clone());
            }
        }
    }

    // 需要刷新
    let refresh_token = creds
        .refresh_token
        .as_deref()
        .ok_or("No refresh token, please reconnect Google")?;
    let client_id = creds.client_id.as_deref().unwrap_or(GOOGLE_CLIENT_ID);
    let client_secret = creds
        .client_secret
        .as_deref()
        .unwrap_or(GOOGLE_CLIENT_SECRET);

    let client = reqwest::Client::new();
    let form_body = format!(
        "refresh_token={}&client_id={}&client_secret={}&grant_type=refresh_token",
        urlencoding::encode(refresh_token),
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret),
    );
    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;

    let new_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("Refresh response missing access_token")?
        .to_string();
    let expires_in = body
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);

    // 更新凭证
    let mut new_creds = creds.clone();
    new_creds.access_token = Some(new_token.clone());
    new_creds.expires_at = Some(now + expires_in);
    let _ = super::connector::save_credentials("google", &new_creds);

    Ok(new_token)
}

/// Google API GET 请求
async fn google_get(url: &str) -> Result<Value, String> {
    let token = get_token().await?;
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Google API error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Google API {} : {}",
            status,
            &body[..body.len().min(200)]
        ));
    }
    resp.json().await.map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════
// 工具 Schema
// ═══════════════════════════════════════════════════════════

pub fn get_tool_schemas() -> Vec<Value> {
    if super::connector::load_credentials("google").is_none() {
        return vec![];
    }

    vec![
        json!({
            "type": "function",
            "function": {
                "name": "google_calendar_list_events",
                "description": "查询 Google Calendar 事件。返回指定时间范围内的日历事件。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "time_min": { "type": "string", "description": "起始时间 (RFC3339, 如 2024-01-15T00:00:00Z)" },
                        "time_max": { "type": "string", "description": "结束时间 (RFC3339)" },
                        "max_results": { "type": "integer", "description": "最大结果数, 默认 10" }
                    },
                    "required": ["time_min", "time_max"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "google_calendar_create_event",
                "description": "在 Google Calendar 创建新事件。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string", "description": "事件标题" },
                        "start_time": { "type": "string", "description": "开始时间 (RFC3339)" },
                        "end_time": { "type": "string", "description": "结束时间 (RFC3339)" },
                        "description": { "type": "string", "description": "事件描述" },
                        "location": { "type": "string", "description": "地点" }
                    },
                    "required": ["summary", "start_time", "end_time"]
                }
            }
        }),
    ]
}

// ═══════════════════════════════════════════════════════════
// 工具执行
// ═══════════════════════════════════════════════════════════

pub async fn execute_tool(name: &str, args: &Value) -> Value {
    match name {
        "google_calendar_list_events" => {
            let time_min = args.get("time_min").and_then(|v| v.as_str()).unwrap_or("");
            let time_max = args.get("time_max").and_then(|v| v.as_str()).unwrap_or("");
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(10);
            tool_list_events(time_min, time_max, max_results as usize).await
        }
        "google_calendar_create_event" => tool_create_event(args).await,
        _ => json!({"error": format!("Unknown google_calendar tool: {}", name)}),
    }
}

async fn tool_list_events(time_min: &str, time_max: &str, max_results: usize) -> Value {
    let url = format!(
        "{}/calendars/primary/events?timeMin={}&timeMax={}&maxResults={}&singleEvents=true&orderBy=startTime",
        GCAL_API_BASE,
        urlencoding::encode(time_min),
        urlencoding::encode(time_max),
        max_results
    );
    match google_get(&url).await {
        Ok(body) => {
            let items = body.get("items").cloned().unwrap_or(json!([]));
            // 精简返回
            if let Some(arr) = items.as_array() {
                let simplified: Vec<Value> = arr
                    .iter()
                    .map(|e| {
                        json!({
                            "summary": e.get("summary"),
                            "start": e.get("start"),
                            "end": e.get("end"),
                            "location": e.get("location"),
                            "status": e.get("status"),
                            "htmlLink": e.get("htmlLink"),
                        })
                    })
                    .collect();
                json!({"ok": simplified})
            } else {
                json!({"ok": items})
            }
        }
        Err(e) => json!({"error": e}),
    }
}

async fn tool_create_event(args: &Value) -> Value {
    let token = match get_token().await {
        Ok(t) => t,
        Err(e) => return json!({"error": e}),
    };

    let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let start_time = args
        .get("start_time")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let end_time = args.get("end_time").and_then(|v| v.as_str()).unwrap_or("");
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let location = args.get("location").and_then(|v| v.as_str()).unwrap_or("");

    let event_body = json!({
        "summary": summary,
        "description": description,
        "location": location,
        "start": { "dateTime": start_time },
        "end": { "dateTime": end_time }
    });

    let client = reqwest::Client::new();
    let url = format!("{}/calendars/primary/events", GCAL_API_BASE);
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&event_body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: Value = r.json().await.unwrap_or(json!({}));
            let link = body.get("htmlLink").and_then(|v| v.as_str()).unwrap_or("");
            json!({"ok": format!("事件 '{}' 已创建: {}", summary, link)})
        }
        Ok(r) => {
            let text = r.text().await.unwrap_or_default();
            json!({"error": format!("创建事件失败: {}", &text[..text.len().min(200)])})
        }
        Err(e) => json!({"error": format!("请求失败: {}", e)}),
    }
}

// ── 后台静默同步 ──
pub async fn start_background_sync(app_handle: tauri::AppHandle) {
    use std::time::Duration;
    use tauri::Manager;
    use tokio::time;

    let mut interval = time::interval(Duration::from_secs(3600)); // 每小时执行一次

    loop {
        interval.tick().await;

        // 检查是否配置了 Google OAuth
        let creds = match super::connector::load_credentials("google") {
            Some(c) => c,
            None => continue,
        };
        let client_id = creds.client_id.as_deref().unwrap_or(GOOGLE_CLIENT_ID);
        if client_id == GOOGLE_CLIENT_ID {
            continue; // 未配置真实的凭据
        }

        // 获取过去一周到未来一个月的事件
        let now = chrono::Utc::now();
        let one_week_ago = now - chrono::Duration::days(7);
        let one_month_later = now + chrono::Duration::days(30);
        let time_min = one_week_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let time_max = one_month_later.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let events_value = tool_list_events(&time_min, &time_max, 100).await;

        // 将事件写入 SQLite DB
        if let Some(events_arr) = events_value.as_array() {
            let db_state = app_handle.state::<crate::db::DbState>();
            let conn = match db_state.0.lock() {
                Ok(c) => c,
                Err(_) => continue,
            };

            for ev in events_arr {
                let summary = ev
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let start_time = ev
                    .get("start")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let end_time = ev
                    .get("end")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let location = ev
                    .get("location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let event_id = ev
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if event_id.is_empty() {
                    continue;
                }
                let local_id = format!("gcal-{}", event_id);

                // Upsert
                let _ = conn.execute(
                    "INSERT INTO events (id, title, type, status, start_time, end_time, description, created_at)
                     VALUES (?1, ?2, 'event', 'pending', ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET
                        title = excluded.title,
                        start_time = excluded.start_time,
                        end_time = excluded.end_time,
                        description = excluded.description",
                    rusqlite::params![local_id, summary, start_time, end_time, location, super::now_ms()],
                );
            }
        }
    }
}
