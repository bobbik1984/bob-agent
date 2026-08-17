//! 飞书 (Feishu/Lark) 连接器
//!
//! 使用 App Token 鉴权（自动获取/刷新 tenant_access_token）。
//! 提供日历、文档搜索、多维表格、消息等工具。

use serde_json::{json, Value};

const LARK_API_BASE: &str = "https://open.feishu.cn/open-apis";

/// 获取或刷新 tenant_access_token
pub async fn refresh_tenant_token() -> Result<String, String> {
    let creds = super::connector::load_credentials("lark").ok_or("飞书未配置凭证")?;

    let app_id = creds.app_id.as_deref().ok_or("Missing app_id")?;
    let app_secret = creds.app_secret.as_deref().ok_or("Missing app_secret")?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/auth/v3/tenant_access_token/internal",
            LARK_API_BASE
        ))
        .json(&json!({
            "app_id": app_id,
            "app_secret": app_secret
        }))
        .send()
        .await
        .map_err(|e| format!("飞书 API 请求失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("飞书响应解析失败: {}", e))?;

    if body.get("code").and_then(|v| v.as_i64()) != Some(0) {
        let msg = body
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(format!("飞书鉴权失败: {}", msg));
    }

    let token = body
        .get("tenant_access_token")
        .and_then(|v| v.as_str())
        .ok_or("响应中缺少 tenant_access_token")?
        .to_string();

    let expire = body.get("expire").and_then(|v| v.as_i64()).unwrap_or(7200);

    // 更新保存的凭证
    let mut new_creds = creds.clone();
    new_creds.tenant_access_token = Some(token.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    new_creds.expires_at = Some(now + expire);
    let _ = super::connector::save_credentials("lark", &new_creds);

    log::info!(
        "[Lark] tenant_access_token refreshed, expires in {}s",
        expire
    );
    Ok(token)
}

/// 获取有效的 tenant_access_token（自动刷新过期 token）
async fn get_token() -> Result<String, String> {
    let creds = super::connector::load_credentials("lark").ok_or("飞书未配置")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // 如果 token 存在且未过期（提前 5 分钟刷新）
    if let Some(ref token) = creds.tenant_access_token {
        if let Some(expires_at) = creds.expires_at {
            if expires_at > now + 300 {
                return Ok(token.clone());
            }
        }
    }

    // 需要刷新
    refresh_tenant_token().await
}

/// 通用 Lark API GET 请求
async fn lark_get(path: &str) -> Result<Value, String> {
    let token = get_token().await?;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}{}", LARK_API_BASE, path))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Lark API error: {}", e))?;

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if body.get("code").and_then(|v| v.as_i64()) != Some(0) {
        let msg = body
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(format!("Lark API error: {}", msg));
    }
    Ok(body)
}

/// 通用 Lark API POST 请求
async fn lark_post(path: &str, body: &Value) -> Result<Value, String> {
    let token = get_token().await?;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}{}", LARK_API_BASE, path))
        .header("Authorization", format!("Bearer {}", token))
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Lark API error: {}", e))?;

    let result: Value = resp.json().await.map_err(|e| e.to_string())?;
    if result.get("code").and_then(|v| v.as_i64()) != Some(0) {
        let msg = result
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(format!("Lark API error: {}", msg));
    }
    Ok(result)
}

// ═══════════════════════════════════════════════════════════
// 工具 Schema
// ═══════════════════════════════════════════════════════════

pub fn get_tool_schemas() -> Vec<Value> {
    // 只有在已配置凭证时才返回工具
    if super::connector::load_credentials("lark").is_none() {
        return vec![];
    }

    vec![
        json!({
            "type": "function",
            "function": {
                "name": "lark_list_calendar_events",
                "description": "查询飞书日历事件列表。返回指定时间范围内的所有日程。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "start_time": { "type": "string", "description": "开始时间 (RFC3339 格式, 如 2024-01-15T00:00:00+08:00)" },
                        "end_time": { "type": "string", "description": "结束时间 (RFC3339 格式)" }
                    },
                    "required": ["start_time", "end_time"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "lark_search_docs",
                "description": "搜索飞书云文档。支持按关键词搜索文档标题和内容。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "搜索关键词" },
                        "count": { "type": "integer", "description": "最大返回数量，默认 10" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "lark_send_message",
                "description": "向飞书群或用户发送消息。需要指定接收者 ID。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "receive_id": { "type": "string", "description": "接收者 ID (open_id, user_id, 或 chat_id)" },
                        "receive_id_type": { "type": "string", "description": "ID 类型: open_id, user_id, chat_id，默认 chat_id" },
                        "msg_type": { "type": "string", "description": "消息类型: text, interactive，默认 text" },
                        "content": { "type": "string", "description": "消息内容 (JSON 字符串)" }
                    },
                    "required": ["receive_id", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "lark_create_bitable_record",
                "description": "在飞书多维表格中新增一行记录。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "app_token": { "type": "string", "description": "多维表格 App Token" },
                        "table_id": { "type": "string", "description": "数据表 ID" },
                        "fields": { "type": "object", "description": "字段名-值映射" }
                    },
                    "required": ["app_token", "table_id", "fields"]
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
        "lark_list_calendar_events" => {
            let start = args
                .get("start_time")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let end = args.get("end_time").and_then(|v| v.as_str()).unwrap_or("");
            tool_list_calendar_events(start, end).await
        }
        "lark_search_docs" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            tool_search_docs(query, count).await
        }
        "lark_send_message" => tool_send_message(args).await,
        "lark_create_bitable_record" => tool_create_bitable_record(args).await,
        _ => json!({"error": format!("Unknown lark tool: {}", name)}),
    }
}

// ── 工具实现 ──────────────────────────────────────────

async fn tool_list_calendar_events(start_time: &str, end_time: &str) -> Value {
    let path = format!(
        "/calendar/v4/calendars/primary/events?start_time={}&end_time={}",
        start_time, end_time
    );
    match lark_get(&path).await {
        Ok(body) => {
            let events = body
                .get("data")
                .and_then(|d| d.get("items"))
                .cloned()
                .unwrap_or(json!([]));
            json!({"ok": events})
        }
        Err(e) => json!({"error": e}),
    }
}

async fn tool_search_docs(query: &str, count: usize) -> Value {
    let body = json!({
        "search_key": query,
        "count": count,
        "docs_types": ["doc", "docx", "sheet", "bitable"]
    });
    match lark_post("/suite/docs-api/search/object", &body).await {
        Ok(resp) => {
            let docs = resp
                .get("data")
                .and_then(|d| d.get("docs_entities"))
                .cloned()
                .unwrap_or(json!([]));
            json!({"ok": docs})
        }
        Err(e) => json!({"error": e}),
    }
}

async fn tool_send_message(args: &Value) -> Value {
    let receive_id = args
        .get("receive_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let receive_id_type = args
        .get("receive_id_type")
        .and_then(|v| v.as_str())
        .unwrap_or("chat_id");
    let msg_type = args
        .get("msg_type")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

    // 如果是纯文本，包装成飞书格式
    let formatted_content = if msg_type == "text" && !content.starts_with('{') {
        format!("{{\"text\":\"{}\"}}", content.replace('"', "\\\""))
    } else {
        content.to_string()
    };

    let path = format!("/im/v1/messages?receive_id_type={}", receive_id_type);
    let body = json!({
        "receive_id": receive_id,
        "msg_type": msg_type,
        "content": formatted_content
    });

    match lark_post(&path, &body).await {
        Ok(resp) => {
            let msg_id = resp
                .get("data")
                .and_then(|d| d.get("message_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            json!({"ok": format!("消息已发送 (message_id: {})", msg_id)})
        }
        Err(e) => json!({"error": e}),
    }
}

async fn tool_create_bitable_record(args: &Value) -> Value {
    let app_token = args.get("app_token").and_then(|v| v.as_str()).unwrap_or("");
    let table_id = args.get("table_id").and_then(|v| v.as_str()).unwrap_or("");
    let fields = args.get("fields").cloned().unwrap_or(json!({}));

    let path = format!("/bitable/v1/apps/{}/tables/{}/records", app_token, table_id);
    let body = json!({ "fields": fields });

    match lark_post(&path, &body).await {
        Ok(resp) => {
            let record_id = resp
                .get("data")
                .and_then(|d| d.get("record"))
                .and_then(|r| r.get("record_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            json!({"ok": format!("记录已创建 (record_id: {})", record_id)})
        }
        Err(e) => json!({"error": e}),
    }
}
