//! Gmail 连接器
//!
//! 共享 Google OAuth token（由 google_calendar.rs 管理）。
//! 提供邮件搜索、阅读、草稿创建等工具。

use serde_json::{json, Value};

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

/// 获取有效的 access token（复用 google_calendar 的 token 管理）
async fn get_token() -> Result<String, String> {
    // Google token 由 google_calendar 模块统一管理
    let creds = super::connector::load_credentials("google")
        .ok_or("Google 未连接，请先连接 Google 账号")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if let Some(ref token) = creds.access_token {
        if let Some(expires_at) = creds.expires_at {
            if expires_at > now + 60 {
                return Ok(token.clone());
            }
        }
    }

    // 需要刷新 — 委托给 google_calendar 的 start_google_oauth 触发
    // 这里尝试自行刷新
    let refresh_token = creds.refresh_token.as_deref()
        .ok_or("No refresh token, please reconnect Google")?;
    let client_id = creds.client_id.as_deref().unwrap_or("BOB_GOOGLE_CLIENT_ID");
    let client_secret = creds.client_secret.as_deref().unwrap_or("BOB_GOOGLE_CLIENT_SECRET");

    let client = reqwest::Client::new();
    let form_body = format!(
        "refresh_token={}&client_id={}&client_secret={}&grant_type=refresh_token",
        urlencoding::encode(refresh_token),
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret),
    );
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|e| format!("Gmail token refresh failed: {}", e))?;

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let new_token = body.get("access_token").and_then(|v| v.as_str())
        .ok_or("Refresh response missing access_token")?.to_string();
    let expires_in = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);

    let mut new_creds = creds.clone();
    new_creds.access_token = Some(new_token.clone());
    new_creds.expires_at = Some(now + expires_in);
    let _ = super::connector::save_credentials("google", &new_creds);

    Ok(new_token)
}

/// Gmail API GET 请求
async fn gmail_get(path: &str) -> Result<Value, String> {
    let token = get_token().await?;
    let client = reqwest::Client::new();
    let url = format!("{}{}", GMAIL_API_BASE, path);
    let resp = client.get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Gmail API error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Gmail API {} : {}", status, &body[..body.len().min(200)]));
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
                "name": "gmail_search",
                "description": "搜索 Gmail 邮件。支持 Gmail 搜索语法 (from:, subject:, after:, before:, is:unread 等)。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Gmail 搜索查询语法" },
                        "max_results": { "type": "integer", "description": "最大返回数, 默认 5" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "gmail_read_message",
                "description": "读取一封 Gmail 邮件的详细内容（标题、发件人、正文摘要）。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "message_id": { "type": "string", "description": "邮件 ID (从 gmail_search 结果中获取)" }
                    },
                    "required": ["message_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "gmail_create_draft",
                "description": "在 Gmail 创建草稿邮件。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "to": { "type": "string", "description": "收件人邮箱地址" },
                        "subject": { "type": "string", "description": "邮件主题" },
                        "body": { "type": "string", "description": "邮件正文 (纯文本)" }
                    },
                    "required": ["to", "subject", "body"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "gmail_list_labels",
                "description": "列出 Gmail 中所有标签/文件夹。",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
    ]
}

// ═══════════════════════════════════════════════════════════
// 工具执行
// ═══════════════════════════════════════════════════════════

pub async fn execute_tool(name: &str, args: &Value) -> Value {
    match name {
        "gmail_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let max = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            tool_search(query, max).await
        }
        "gmail_read_message" => {
            let id = args.get("message_id").and_then(|v| v.as_str()).unwrap_or("");
            tool_read_message(id).await
        }
        "gmail_create_draft" => {
            tool_create_draft(args).await
        }
        "gmail_list_labels" => {
            tool_list_labels().await
        }
        _ => json!({"error": format!("Unknown gmail tool: {}", name)}),
    }
}

// ── 工具实现 ──────────────────────────────────────────

async fn tool_search(query: &str, max_results: usize) -> Value {
    let path = format!("/messages?q={}&maxResults={}", urlencoding::encode(query), max_results);
    match gmail_get(&path).await {
        Ok(body) => {
            let messages = body.get("messages").and_then(|m| m.as_array());
            match messages {
                Some(msgs) if !msgs.is_empty() => {
                    // 获取每封邮件的摘要信息
                    let mut results = Vec::new();
                    for msg in msgs.iter().take(max_results) {
                        let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        if let Ok(detail) = gmail_get(&format!("/messages/{}?format=metadata&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=Date", msg_id)).await {
                            let headers = detail.get("payload")
                                .and_then(|p| p.get("headers"))
                                .and_then(|h| h.as_array());

                            let mut subject = String::new();
                            let mut from = String::new();
                            let mut date = String::new();

                            if let Some(hdrs) = headers {
                                for h in hdrs {
                                    let name = h.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                    let value = h.get("value").and_then(|v| v.as_str()).unwrap_or("");
                                    match name {
                                        "Subject" => subject = value.to_string(),
                                        "From" => from = value.to_string(),
                                        "Date" => date = value.to_string(),
                                        _ => {}
                                    }
                                }
                            }

                            let snippet = detail.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                            results.push(json!({
                                "id": msg_id,
                                "subject": subject,
                                "from": from,
                                "date": date,
                                "snippet": snippet
                            }));
                        }
                    }
                    json!({"ok": results})
                }
                _ => json!({"ok": "没有找到匹配的邮件"}),
            }
        }
        Err(e) => json!({"error": e}),
    }
}

async fn tool_read_message(message_id: &str) -> Value {
    let path = format!("/messages/{}?format=full", message_id);
    match gmail_get(&path).await {
        Ok(body) => {
            let snippet = body.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            let headers = body.get("payload")
                .and_then(|p| p.get("headers"))
                .and_then(|h| h.as_array());

            let mut subject = String::new();
            let mut from = String::new();
            let mut to = String::new();
            let mut date = String::new();

            if let Some(hdrs) = headers {
                for h in hdrs {
                    let name = h.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let value = h.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    match name {
                        "Subject" => subject = value.to_string(),
                        "From" => from = value.to_string(),
                        "To" => to = value.to_string(),
                        "Date" => date = value.to_string(),
                        _ => {}
                    }
                }
            }

            // 尝试提取纯文本正文
            let text_body = extract_text_body(&body).unwrap_or_else(|| snippet.to_string());

            json!({
                "ok": {
                    "subject": subject,
                    "from": from,
                    "to": to,
                    "date": date,
                    "body": text_body
                }
            })
        }
        Err(e) => json!({"error": e}),
    }
}

/// 从 Gmail message payload 中提取纯文本正文
fn extract_text_body(msg: &Value) -> Option<String> {
    let payload = msg.get("payload")?;

    // 单 part 消息
    if let Some(body_data) = payload.get("body").and_then(|b| b.get("data")).and_then(|d| d.as_str()) {
        return decode_base64url(body_data);
    }

    // 多 part 消息
    if let Some(parts) = payload.get("parts").and_then(|p| p.as_array()) {
        for part in parts {
            let mime = part.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
            if mime == "text/plain" {
                if let Some(data) = part.get("body").and_then(|b| b.get("data")).and_then(|d| d.as_str()) {
                    return decode_base64url(data);
                }
            }
        }
    }

    None
}

/// 解码 Gmail 的 base64url 编码
fn decode_base64url(data: &str) -> Option<String> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let bytes = engine.decode(data).ok()?;
    String::from_utf8(bytes).ok()
}

async fn tool_create_draft(args: &Value) -> Value {
    let token = match get_token().await {
        Ok(t) => t,
        Err(e) => return json!({"error": e}),
    };

    let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("");
    let subject = args.get("subject").and_then(|v| v.as_str()).unwrap_or("");
    let body_text = args.get("body").and_then(|v| v.as_str()).unwrap_or("");

    // 构建 RFC 2822 格式邮件
    let raw_message = format!(
        "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}",
        to, subject, body_text
    );

    use base64::Engine;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let encoded = engine.encode(raw_message.as_bytes());

    let draft_body = json!({
        "message": {
            "raw": encoded
        }
    });

    let client = reqwest::Client::new();
    let url = format!("{}/drafts", GMAIL_API_BASE);
    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&draft_body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            json!({"ok": format!("草稿已创建: To={}, Subject={}", to, subject)})
        }
        Ok(r) => {
            let text = r.text().await.unwrap_or_default();
            json!({"error": format!("创建草稿失败: {}", &text[..text.len().min(200)])})
        }
        Err(e) => json!({"error": format!("请求失败: {}", e)}),
    }
}

async fn tool_list_labels() -> Value {
    match gmail_get("/labels").await {
        Ok(body) => {
            let labels = body.get("labels").and_then(|l| l.as_array());
            match labels {
                Some(arr) => {
                    let simplified: Vec<Value> = arr.iter().map(|l| {
                        json!({
                            "id": l.get("id"),
                            "name": l.get("name"),
                            "type": l.get("type"),
                        })
                    }).collect();
                    json!({"ok": simplified})
                }
                None => json!({"ok": []}),
            }
        }
        Err(e) => json!({"error": e}),
    }
}
