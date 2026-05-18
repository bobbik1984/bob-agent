use std::sync::Arc;
use serde_json::json;
use rusqlite::{params, Connection};

use super::types::{WeixinMessage, SendMessageReq, MessageItem, TextItem};
use super::accounts::resolve_wechat_account;
use super::WechatState;
use super::api::WechatApi;

const HELP_TEXT: &str = "可用指令：\n/chat  — 列出最近会话，回复序号切换\n/new   — 开启全新会话\n/status — 查看当前会话 ID\n/help  — 显示此帮助\n\n直接发送文字即可与 Bob 对话 🤖";

// Helper to open DB
fn open_db() -> Option<Connection> {
    let db_path = crate::get_data_dir().join("bob.db");
    Connection::open(db_path).ok()
}

async fn handle_message(
    wxid: &str,
    text: &str,
    state: Arc<WechatState>,
    _context_token: Option<String>,
) -> String {
    let text = text.trim();

    if text == "/help" {
        return HELP_TEXT.to_string();
    }

    if text == "/new" {
        state.session_mgr.bind_session(wxid, None);
        return "✅ 已开启全新会话, 接下来的消息将进入新对话。".to_string();
    }

    if text == "/status" {
        if let Some(conv_id) = state.session_mgr.get_conv_id(wxid) {
            let short = if conv_id.len() > 8 { &conv_id[..8] } else { &conv_id };
            return format!("当前会话：{}…（发送 /chat 可切换）", short);
        } else {
            return "当前无活跃会话（下条消息将自动新建）".to_string();
        }
    }

    if text == "/chat" || text == "/list" {
        let conn = match open_db() {
            Some(c) => c,
            None => return "❌ 数据库连接失败".to_string(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC LIMIT 5"
        ) {
            Ok(s) => s,
            Err(_) => return "❌ 数据库查询失败".to_string(),
        };
        let rows = stmt.query_map([], |row| {
            Ok(super::session_mgr::ConversationMeta {
                id: row.get(0)?,
                title: row.get(1)?,
                updated_at: row.get(2)?,
            })
        });
        let mut list = Vec::new();
        if let Ok(iter) = rows {
            for row in iter.flatten() {
                list.push(row);
            }
        }
        if list.is_empty() {
            return "暂无历史会话，直接发消息开始吧 💬".to_string();
        }
        state.session_mgr.set_selecting(wxid, list.clone());
        let mut reply = "请回复序号切换会话（60 秒内有效）：\n".to_string();
        for (i, c) in list.iter().enumerate() {
            let title = if c.title.is_empty() { "未命名对话" } else { &c.title };
            reply.push_str(&format!("[{}] {}\n", i + 1, title));
        }
        reply.push_str("[0] 开启全新会话");
        return reply;
    }

    if state.session_mgr.is_selecting(wxid) {
        if let Ok(idx) = text.parse::<usize>() {
            if idx == 0 {
                state.session_mgr.bind_session(wxid, None);
                return "✅ 已开启全新会话。".to_string();
            }
            if let Some(list) = state.session_mgr.get_pending_list(wxid) {
                if idx > 0 && idx <= list.len() {
                    let target = &list[idx - 1];
                    state.session_mgr.bind_session(wxid, Some(target.id.clone()));
                    state.session_mgr.cancel_selecting(wxid);
                    let title = if target.title.is_empty() { "未命名对话" } else { &target.title };
                    return format!("✅ 已切换至「{}」，继续上下文吧。", title);
                } else {
                    return format!("❌ 序号无效，请回复 1-{} 之间的数字，或发送 /chat 重新列出。", list.len());
                }
            }
        } else {
            // Not a number, exit selecting mode
            state.session_mgr.cancel_selecting(wxid);
        }
    }

    // Normal message handling
    let app_handle = {
        let app = state.app.read().unwrap();
        if let Some(h) = app.as_ref() {
            h.clone()
        } else {
            return "❌ 系统未完全初始化，请稍后重试。".to_string();
        }
    };

    let conn = match open_db() {
        Some(c) => c,
        None => return "❌ 数据库连接失败".to_string(),
    };

    let active_conv_id = state.session_mgr.get_conv_id(wxid);

    let conversation_id = if let Some(id) = active_conv_id {
        let exists: bool = conn
            .query_row("SELECT 1 FROM conversations WHERE id = ?1", params![id], |_| Ok(true))
            .unwrap_or(false);
        if exists {
            id
        } else {
            let title: String = text.chars().take(20).collect();
            let new_id = format!("conv-{}", crate::now_ms());
            let ts = crate::now_ms();
            let _ = conn.execute(
                "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, '', ?3, ?4)",
                params![new_id, title, ts, ts],
            );
            new_id
        }
    } else {
        let title: String = text.chars().take(20).collect();
        let new_id = format!("conv-{}", crate::now_ms());
        let ts = crate::now_ms();
        let _ = conn.execute(
            "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, '', ?3, ?4)",
            params![new_id, title, ts, ts],
        );
        new_id
    };

    // Append user message
    let ts = crate::now_ms();
    let _ = conn.execute(
        "INSERT INTO messages (conversation_id, role, content, created_at, from_channel) VALUES (?1, ?2, ?3, ?4, 'wechat')",
        params![conversation_id, "user", text, ts],
    );
    let preview: String = text.chars().take(20).collect();
    let _ = conn.execute(
        "UPDATE conversations SET last_message = ?1, last_role = ?2, updated_at = ?3 WHERE id = ?4",
        params![preview, "user", ts, conversation_id],
    );

    // Load history
    let mut messages = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC") {
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok(json!({
                "role": row.get::<_, String>(0)?,
                "content": row.get::<_, String>(1)?,
            }))
        });
        if let Ok(iter) = rows {
            for row in iter.flatten() {
                messages.push(row);
            }
        }
    }

    // Call LLM
    // We will need to send typing heartbeats while waiting. 
    // For now we just call it.
    let result = crate::llm::stream_chat(app_handle.clone(), messages).await;
    
    let mut full_text = String::new();
    if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
        full_text.push_str(content);
    }
    
    if full_text.is_empty() {
        return "（Bob 未返回内容，请稍后重试）".to_string();
    }

    // Update conversation_id to latest
    state.session_mgr.bind_session(wxid, Some(conversation_id.clone()));

    // Save assistant message to DB
    let ts2 = crate::now_ms();
    let _ = conn.execute(
        "INSERT INTO messages (conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![conversation_id, "assistant", full_text, ts2],
    );

    if full_text.chars().count() > 3800 {
        let truncated: String = full_text.chars().take(3800).collect();
        return format!("{}\n…（由于微信长度限制，内容已截断）", truncated);
    }

    full_text
}

pub async fn process_message(msg: WeixinMessage, state: Arc<WechatState>) -> Result<(), String> {
    let from_user_id = match msg.from_user_id {
        Some(ref id) => id.clone(),
        None => return Ok(()),
    };

    let item_list = match msg.item_list {
        Some(list) => list,
        None => return Ok(()),
    };

    let msg_type = item_list.first().and_then(|i| i.r#type).unwrap_or(0);

    let text_item = item_list.iter().find(|i| i.r#type == Some(1)).and_then(|i| i.text_item.clone());
    let text = match text_item.and_then(|t| t.text) {
        Some(t) => t,
        None => {
            let type_name = match msg_type {
                3 => "图片",
                34 => "语音",
                43 => "视频",
                49 => "文件",
                _ => "该类型消息",
            };
            let reply = format!("暂不支持接收{}，请发送文字消息。", type_name);
            send_reply(&from_user_id, &reply, &state, msg.context_token).await?;
            return Ok(());
        }
    };

    if text.trim().is_empty() {
        return Ok(());
    }

    let reply = handle_message(&from_user_id, &text, state.clone(), msg.context_token.clone()).await;
    send_reply(&from_user_id, &reply, &state, msg.context_token).await?;

    Ok(())
}

async fn send_reply(to: &str, text: &str, state: &Arc<WechatState>, context_token: Option<String>) -> Result<(), String> {
    let account_id = state.account_id.read().unwrap().clone();
    let account = match resolve_wechat_account(account_id.as_deref()) {
        Ok(acc) if acc.configured => acc,
        Ok(_) => return Err("WeChat account not configured (no token)".to_string()),
        Err(e) => return Err(e),
    };

    let api = WechatApi::new(account.base_url, account.token);
    
    let reply_msg = WeixinMessage {
        seq: None,
        message_id: None,
        from_user_id: None,
        to_user_id: Some(to.to_string()),
        client_id: None,
        create_time_ms: None,
        update_time_ms: None,
        delete_time_ms: None,
        session_id: None,
        group_id: None,
        message_type: None,
        message_state: None,
        item_list: Some(vec![MessageItem {
            r#type: Some(1),
            create_time_ms: None,
            update_time_ms: None,
            is_completed: None,
            msg_id: None,
            ref_msg: None,
            text_item: Some(TextItem {
                text: Some(text.to_string()),
            }),
            image_item: None,
            voice_item: None,
            file_item: None,
            video_item: None,
        }]),
        context_token,
    };

    let req = SendMessageReq {
        msg: Some(reply_msg),
        base_info: None,
    };

    api.send_message(req, 15_000).await.map(|_| ()).map_err(|e| format!("send_message error: {}", e))
}
