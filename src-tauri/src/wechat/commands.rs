use std::sync::Arc;
use serde_json::json;
use rusqlite::{params, Connection};
use tauri::Emitter;
use base64::Engine as _;

use super::types::*;
use super::accounts::resolve_wechat_account;
use super::WechatState;
use super::api::WechatApi;
use super::cdn;

const HELP_TEXT: &str = "可用指令：\n/sessions — 列出最近会话，回复序号切换\n/new      — 开启全新会话\n/status   — 查看当前会话 ID\n/help     — 显示此帮助\n\n直接发送文字即可与 Bob 对话 🤖";

/// T-WX01: 从文本中提取所有 HTTP/HTTPS URL
fn extract_urls(text: &str) -> Vec<String> {
    // 简单但高效的 URL 匹配：https?:// 后跟非空白字符
    let mut urls = Vec::new();
    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            // 清理尾部标点
            let clean = word.trim_end_matches(|c: char| {
                matches!(c, ',' | '.' | '!' | '?' | ')' | ']' | '>' | '。' | '，' | '！' | '？')
            });
            if !clean.is_empty() {
                urls.push(clean.to_string());
            }
        }
    }
    urls
}

/// T-WX01: 从微信 type=49 的卡片消息 XML 中提取 URL
fn extract_url_from_card_xml(xml: &str) -> Option<String> {
    // 微信卡片消息的 XML 结构中，URL 通常在 <url> 标签内
    // 格式: <url><![CDATA[https://mp.weixin.qq.com/...]]></url>
    if let Some(start) = xml.find("<url>") {
        let after = &xml[start + 5..];
        let content = if after.starts_with("<![CDATA[") {
            // 提取 CDATA 包裹的内容
            let inner = &after[9..];
            inner.find("]]>").map(|end| &inner[..end])
        } else {
            after.find("</url>").map(|end| &after[..end])
        };
        if let Some(url) = content {
            let url = url.trim();
            if url.starts_with("http://") || url.starts_with("https://") {
                return Some(url.to_string());
            }
        }
    }
    None
}

// Helper to open DB
fn open_db() -> Option<Connection> {
    let db_path = crate::get_data_dir().join("bob.db");
    Connection::open(db_path).ok()
}

/// 将 ms 时间戳转为人类友好的相对时间
fn format_relative_time(timestamp_ms: i64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let diff_secs = (now_ms - timestamp_ms) / 1000;

    if diff_secs < 60 {
        "刚刚".to_string()
    } else if diff_secs < 3600 {
        format!("{}分钟前", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}小时前", diff_secs / 3600)
    } else if diff_secs < 172800 {
        "昨天".to_string()
    } else if diff_secs < 604800 {
        format!("{}天前", diff_secs / 86400)
    } else {
        // 超过一周显示日期
        let secs = (timestamp_ms / 1000) as u64;
        let dt = UNIX_EPOCH + std::time::Duration::from_secs(secs);
        let datetime: chrono::DateTime<chrono::Local> = dt.into();
        datetime.format("%m月%d日").to_string()
    }
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
            return format!("当前会话：{}…（发送 /sessions 可切换）", short);
        } else {
            return "当前无活跃会话（下条消息将自动新建）".to_string();
        }
    }

    if text == "/sessions" || text == "/chat" || text == "/list" {
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
        let mut reply = "📋 请回复序号切换会话（60 秒内有效）：\n".to_string();
        for (i, c) in list.iter().enumerate() {
            let title = if c.title.is_empty() { "未命名对话" } else { &c.title };
            // 将 ms 时间戳转为人类可读的相对时间
            let time_label = format_relative_time(c.updated_at);
            reply.push_str(&format!("[{}] {} ({}）\n", i + 1, title, time_label));
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
                    return format!("❌ 序号无效，请回复 1-{} 之间的数字，或发送 /sessions 重新列出。", list.len());
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

    // 通知桌面前端：用户发来新消息，准备进入思考状态
    let _ = app_handle.emit("remote:new-message", serde_json::json!({
        "conversation_id": &conversation_id,
        "from_channel": "wechat",
        "status": "thinking"
    }));


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

    // ── T-WX01: URL 预抓取 ──────────────────────────────
    // 检测用户消息中的链接，自动抓取内容注入上下文
    let urls = extract_urls(text);
    if !urls.is_empty() {
        let mut url_context = String::new();
        for url in urls.iter().take(2) { // 最多抓取 2 个链接
            let result = crate::web::system_fetch_url(url.to_string()).await;
            if result.get("error").is_none() {
                let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let content = result.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let preview: String = content.chars().take(2000).collect();
                url_context.push_str(&format!(
                    "\n[来源: {}]\n标题: {}\n{}\n",
                    url, title, preview
                ));
                log::info!("[wechat] T-WX01: pre-fetched URL {} ({} chars)", url, preview.len());
            }
        }
        if !url_context.is_empty() {
            messages.push(json!({
                "role": "system",
                "content": format!("用户消息中包含链接，以下是自动抓取的网页内容供你参考：\n{}", url_context)
            }));
        }
    }

    // Call LLM
    // We will need to send typing heartbeats while waiting. 
    // For now we just call it.
    let result = crate::llm::stream_chat(app_handle.clone(), messages, Some(conversation_id.clone()), Some(wxid.to_string()), false, "default".to_string()).await;
    
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

    // 通知桌面前端：有新的远程消息到达，刷新侧边栏和当前对话
    let _ = app_handle.emit("remote:new-message", serde_json::json!({
        "conversation_id": &conversation_id,
        "from_channel": "wechat",
    }));

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
            // T-WX01: 尝试从 type=49 卡片消息中提取 URL
            if msg_type == 49 {
                // type=49 的内容可能在 text_item 的 text 字段中以 XML 形式存在
                // 也可能在其他 item 的 text 中
                let card_xml = item_list.iter()
                    .filter_map(|i| i.text_item.as_ref()?.text.as_ref())
                    .next()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                
                if let Some(url) = extract_url_from_card_xml(&card_xml) {
                    log::info!("[wechat] T-WX01: extracted URL from type=49 card: {}", url);
                    format!("帮我看看这个链接的内容: {}", url)
                } else {
                    let reply = "收到了一个分享链接，但无法提取 URL。请直接发送链接地址。".to_string();
                    send_reply(&from_user_id, &reply, &state, msg.context_token).await?;
                    return Ok(());
                }
            } else {
                let type_name = match msg_type {
                    3 => "图片",
                    34 => "语音",
                    43 => "视频",
                    _ => "该类型消息",
                };
                let reply = format!("暂不支持接收{}，请发送文字消息。", type_name);
                send_reply(&from_user_id, &reply, &state, msg.context_token).await?;
                return Ok(());
            }
        }
    };

    if text.trim().is_empty() {
        return Ok(());
    }

    log::info!("[wechat] process_message: from={} text=\"{}\"", from_user_id, text.chars().take(30).collect::<String>());

    let reply = handle_message(&from_user_id, &text, state.clone(), msg.context_token.clone()).await;
    log::info!("[wechat] process_message: reply ready ({} chars), sending...", reply.chars().count());
    send_reply(&from_user_id, &reply, &state, msg.context_token).await?;
    log::info!("[wechat] process_message: reply sent to {}", from_user_id);

    Ok(())
}

pub async fn send_reply(to: &str, text: &str, state: &Arc<WechatState>, context_token: Option<String>) -> Result<(), String> {
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
        from_user_id: Some(String::new()),  // must be empty string, not None
        to_user_id: Some(to.to_string()),
        client_id: Some(format!("bob-{}", crate::now_ms())),  // unique client ID required
        create_time_ms: None,
        update_time_ms: None,
        delete_time_ms: None,
        session_id: None,
        group_id: None,
        message_type: Some(2),   // MESSAGE_TYPE_BOT = 2
        message_state: Some(2),  // MESSAGE_STATE_FINISH = 2
        item_list: if text.is_empty() { None } else { Some(vec![MessageItem {
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
        }]) },
        context_token,
    };

    let req = SendMessageReq {
        msg: Some(reply_msg),
        base_info: None,
    };

    match api.send_message(req, 15_000).await {
        Ok(resp) => {
            let ret = resp.ret.unwrap_or(0);
            let errmsg = resp.errmsg.as_deref().unwrap_or("");
            if ret != 0 {
                log::warn!("[wechat] sendmessage to {} returned ret={} errmsg={}", to, ret, errmsg);
            } else {
                log::info!("[wechat] sendmessage to {} OK (ret={})", to, ret);
            }
            Ok(())
        }
        Err(e) => {
            log::error!("[wechat] sendmessage to {} failed: {}", to, e);
            Err(format!("send_message error: {}", e))
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 媒体发送：图片 / 文件
// ═══════════════════════════════════════════════════════════

/// 发送媒体消息 (图片或文件) 到微信用户
/// 
/// 流程：上传到 CDN → 构造 ImageItem/FileItem → sendmessage API
/// 
/// 由 LLM tool `send_wechat_file` 和未来的 Tauri command 调用。
pub async fn send_wechat_file(
    wxid: &str,
    file_path: &str,
    caption: Option<&str>,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    // 1. 解析账户信息
    let account = match resolve_wechat_account(None) {
        Ok(acc) if acc.configured => acc,
        Ok(_) => return Err("微信账户未配置 (无 token)，请先在设置中绑定微信 Bot".to_string()),
        Err(e) => return Err(format!("微信账户解析失败: {}", e)),
    };

    let api = WechatApi::new(account.base_url, account.token);

    // 2. 判断媒体类型和大小
    let file_meta = std::fs::metadata(file_path).map_err(|e| format!("无法读取文件元数据: {}", e))?;
    let file_size_mb = file_meta.len() as f64 / 1024.0 / 1024.0;
    
    if file_size_mb > 25.0 {
        // ── 大文件降级：通过 Web Drop 中继，生成公网可达的下载链接 ──
        log::info!(
            "[wechat] send_wechat_file: file too large ({:.1}MB), fallback to Web Drop",
            file_size_mb
        );
        let download_url = crate::web_drop::start_web_drop(file_path.to_string())
            .await
            .map_err(|e| format!("生成 Web Drop 链接失败: {}", e))?;

        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");

        let link_text = format!(
            "📎 文件 \"{}\" ({:.1}MB) 体积较大，请通过以下链接下载：\n{}",
            file_name, file_size_mb, download_url
        );

        // 如果有 caption，合并发送
        let full_text = if let Some(cap) = caption {
            if !cap.trim().is_empty() {
                format!("{}\n\n{}", cap.trim(), link_text)
            } else {
                link_text
            }
        } else {
            link_text
        };

        // 以纯文本消息发送链接
        let text_item = MessageItem {
            r#type: Some(MESSAGE_ITEM_TYPE_TEXT),
            create_time_ms: None,
            update_time_ms: None,
            is_completed: None,
            msg_id: None,
            ref_msg: None,
            text_item: Some(TextItem {
                text: Some(full_text),
            }),
            image_item: None,
            voice_item: None,
            file_item: None,
            video_item: None,
        };

        let msg = WeixinMessage {
            seq: None,
            message_id: None,
            from_user_id: Some(String::new()),
            to_user_id: Some(wxid.to_string()),
            client_id: Some(format!("bob-link-{}", crate::now_ms())),
            create_time_ms: None,
            update_time_ms: None,
            delete_time_ms: None,
            session_id: None,
            group_id: None,
            message_type: Some(MESSAGE_TYPE_BOT),
            message_state: Some(MESSAGE_STATE_FINISH),
            item_list: Some(vec![text_item]),
            context_token: None,
        };

        let req = SendMessageReq {
            msg: Some(msg),
            base_info: None,
        };

        api.send_message(req, 15_000).await
            .map_err(|e| format!("发送下载链接消息失败: {}", e))?;

        return Ok(format!(
            "✅ 文件 \"{}\" ({:.1}MB) 超过微信 CDN 限制，已通过 Web Drop 生成公网链接并发送给用户：{}",
            file_name, file_size_mb, download_url
        ));
    }

    let is_image = cdn::is_image_file(file_path);
    let media_type = if is_image {
        UPLOAD_MEDIA_TYPE_IMAGE
    } else {
        UPLOAD_MEDIA_TYPE_FILE
    };

    let type_label = if is_image { "图片" } else { "文件" };
    log::info!("[wechat] send_wechat_file: to={} path={} type={}", wxid, file_path, type_label);

    // 3. 上传到 CDN（带实时进度推送）
    let uploaded = cdn::upload_media(&api, file_path, wxid, media_type, app).await?;
    log::info!(
        "[wechat] CDN upload done: filekey={} size={} ciphertext_size={}",
        uploaded.filekey, uploaded.file_size, uploaded.file_size_ciphertext
    );

    // 4. 构造 CdnMedia (共用)
    let cdn_media = CdnMedia {
        encrypt_query_param: Some(uploaded.download_encrypted_query_param.clone()),
        aes_key: Some(base64::engine::general_purpose::STANDARD.encode(uploaded.aeskey_hex.as_bytes())),
        encrypt_type: Some(1),
        full_url: None,
    };

    // 5. 构造 MessageItem
    let media_item = if is_image {
        MessageItem {
            r#type: Some(MESSAGE_ITEM_TYPE_IMAGE),
            create_time_ms: None,
            update_time_ms: None,
            is_completed: None,
            msg_id: None,
            ref_msg: None,
            text_item: None,
            image_item: Some(ImageItem {
                media: Some(cdn_media),
                thumb_media: None,
                aeskey: None,
                url: None,
                mid_size: Some(uploaded.file_size_ciphertext as i32),
                thumb_size: None,
                thumb_height: None,
                thumb_width: None,
                hd_size: None,
            }),
            voice_item: None,
            file_item: None,
            video_item: None,
        }
    } else {
        // 文件（含视频）
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        MessageItem {
            r#type: Some(MESSAGE_ITEM_TYPE_FILE),
            create_time_ms: None,
            update_time_ms: None,
            is_completed: None,
            msg_id: None,
            ref_msg: None,
            text_item: None,
            image_item: None,
            voice_item: None,
            file_item: Some(FileItem {
                media: Some(cdn_media),
                file_name: Some(file_name),
                md5: None,
                len: Some(uploaded.file_size.to_string()),
            }),
            video_item: None,
        }
    };

    // 6. 发送：先发 caption (如有)，再发媒体
    let mut items_to_send: Vec<MessageItem> = Vec::new();

    if let Some(text) = caption {
        if !text.trim().is_empty() {
            items_to_send.push(MessageItem {
                r#type: Some(MESSAGE_ITEM_TYPE_TEXT),
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
            });
        }
    }
    items_to_send.push(media_item);

    // 每个 item 单独发一条消息（与 Node.js 参考实现一致）
    for item in items_to_send {
        let msg = WeixinMessage {
            seq: None,
            message_id: None,
            from_user_id: Some(String::new()),
            to_user_id: Some(wxid.to_string()),
            client_id: Some(format!("bob-media-{}", crate::now_ms())),
            create_time_ms: None,
            update_time_ms: None,
            delete_time_ms: None,
            session_id: None,
            group_id: None,
            message_type: Some(MESSAGE_TYPE_BOT),
            message_state: Some(MESSAGE_STATE_FINISH),
            item_list: Some(vec![item]),
            context_token: None,
        };

        let req = SendMessageReq {
            msg: Some(msg),
            base_info: None,
        };

        api.send_message(req, 15_000).await
            .map_err(|e| format!("发送{}消息失败: {}", type_label, e))?;
    }

    let file_name = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let result_msg = format!("✅ 已成功发送{} \"{}\" ({}KB) 到微信用户 {}",
        type_label,
        file_name,
        uploaded.file_size / 1024,
        wxid,
    );
    log::info!("[wechat] {}", result_msg);
    Ok(result_msg)
}
