use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tauri::Emitter;

/// T-604/T-1003/T-1004: 做梦引擎 — 对话记忆摘要与晨间简报
///
/// 架构:
///   - summarizeSession: 对话结束时，V1 直接提取关键词写入 memory/sessions/
///   - compress_session_async: T-1003 异步后台用 Clerk 模型生成高质量摘要
///   - migrate_stale_sessions: T-1004 将 >7天 的热记忆归档到 wiki/sessions/
///   - getDreamReport: 启动时检查是否有未读的简报
///   - dismissDream: 标记简报已读

fn get_memory_dir() -> PathBuf {
    let dir = super::get_data_dir().join("memory");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn get_dream_path() -> PathBuf {
    get_memory_dir().join("dream_report.json")
}

fn get_session_log_dir() -> PathBuf {
    let dir = get_memory_dir().join("sessions");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// T-1004: 冷存储目录 (wiki/sessions/)
fn get_cold_session_dir() -> PathBuf {
    let dir = super::get_wiki_dir().join("sessions");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// 对话结束时，提取对话摘要并存为 session 日志
#[tauri::command]
pub fn system_summarize_session(conversation_id: String, db: tauri::State<'_, super::DbState>) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };

    // 获取最近的消息（最多 20 条）
    let mut stmt = match conn.prepare(
        "SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY created_at DESC LIMIT 20"
    ) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let messages: Vec<(String, String)> = match stmt.query_map(
        rusqlite::params![conversation_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    ) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => return false,
    };

    if messages.is_empty() {
        return true; // 空对话，不需要摘要
    }

    // V1 简易摘要: 提取用户的核心问题 + 助手的最后回复片段
    let user_questions: Vec<&str> = messages.iter()
        .filter(|(role, _)| role == "user")
        .map(|(_, content)| content.as_str())
        .collect();

    let assistant_replies: Vec<&str> = messages.iter()
        .filter(|(role, _)| role == "assistant")
        .map(|(_, content)| content.as_str())
        .collect();

    let summary = json!({
        "conversationId": conversation_id,
        "timestamp": super::now_ms(),
        "userTopics": user_questions.iter().take(3).map(|q| {
            let s: String = q.chars().take(80).collect();
            s
        }).collect::<Vec<String>>(),
        "assistantHighlight": assistant_replies.first().map(|r| {
            let s: String = r.chars().take(200).collect();
            s
        }),
        "messageCount": messages.len(),
        "compressed": false
    });

    // 写入 session 日志
    let session_path = get_session_log_dir().join(format!("{}.json", conversation_id));
    if let Ok(data) = serde_json::to_string_pretty(&summary) {
        let _ = fs::write(session_path, data);
    }

    // 生成/更新晨间简报
    generate_dream_report();

    true
}

// ═══════════════════════════════════════════════════════════
// T-1003: 异步记忆压缩 (Dream V2)
// ═══════════════════════════════════════════════════════════

/// 后台异步压缩：用 Clerk 模型将 V1 的简易摘要升级为高质量 Markdown 总结
/// 在 setup 阶段由 tokio::spawn 调用，不阻塞主线程
pub async fn compress_sessions_async(app: tauri::AppHandle) {
    let sessions_dir = get_session_log_dir();
    if !sessions_dir.exists() { return; }

    let entries: Vec<PathBuf> = match fs::read_dir(&sessions_dir) {
        Ok(rd) => rd.flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "json"))
            .collect(),
        Err(_) => return,
    };

    for path in entries {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let session: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // 跳过已压缩的
        if session.get("compressed").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }

        // 获取 Clerk 模型配置
        let config = super::read_config();
        let clerk_model = config.get("clerkModel").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // 如果没有配置 Clerk 模型，跳过 LLM 压缩
        if clerk_model.is_empty() {
            continue;
        }

        // 构建压缩 prompt
        let topics = session.get("userTopics")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("; "))
            .unwrap_or_default();
        let highlight = session.get("assistantHighlight")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let msg_count = session.get("messageCount").and_then(|v| v.as_u64()).unwrap_or(0);
        let conv_id = session.get("conversationId").and_then(|v| v.as_str()).unwrap_or("unknown");

        let compress_prompt = format!(
            "请用中文将以下对话摘要压缩为一段简洁的 Markdown 总结（3-5 句话），\
            保留关键事实、决策和结论。不要加标题，直接输出纯文本段落。\n\n\
            对话 ID: {}\n消息数: {}\n用户话题: {}\n助手回复摘要: {}",
            conv_id, msg_count, topics, highlight
        );

        // 调用 Clerk 模型（使用内部 LLM 调用，非流式简化版）
        let (_, api_key, model_id, base_url) = super::llm::read_llm_config_for_model(&clerk_model);
        if api_key.is_empty() { continue; }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let body = json!({
            "model": model_id,
            "messages": [
                { "role": "system", "content": "你是一个对话记忆压缩引擎。只输出压缩后的摘要，不要输出其他内容。" },
                { "role": "user", "content": compress_prompt }
            ],
            "max_tokens": 300,
            "stream": false
        });

        let url = format!("{}/chat/completions", base_url);
        let resp = match client.post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Dream V2 compress failed for {}: {}", conv_id, e);
                continue;
            }
        };

        if !resp.status().is_success() {
            log::warn!("Dream V2 compress API error for {}: {}", conv_id, resp.status());
            continue;
        }

        let resp_json: Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => continue,
        };

        let compressed_text = resp_json
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if compressed_text.is_empty() { continue; }

        // 写入压缩后的 Markdown 文件（替换 JSON）
        let md_path = path.with_extension("md");
        let md_content = format!(
            "# 对话摘要: {}\n\n> 消息数: {} | 压缩时间: {}\n\n{}\n",
            conv_id,
            msg_count,
            chrono::Local::now().format("%Y-%m-%d %H:%M"),
            compressed_text
        );
        let _ = fs::write(&md_path, &md_content);

        // 更新原 JSON 标记为已压缩
        if let Ok(mut obj) = serde_json::from_str::<Value>(&content) {
            if let Some(map) = obj.as_object_mut() {
                map.insert("compressed".to_string(), json!(true));
                map.insert("compressedFile".to_string(), json!(md_path.file_name().unwrap_or_default().to_string_lossy()));
                if let Ok(data) = serde_json::to_string_pretty(&obj) {
                    let _ = fs::write(&path, data);
                }
            }
        }

        log::info!("Dream V2: compressed session {} -> {}", conv_id, md_path.display());

        // 避免对 API 造成突发压力，每次压缩后等 1 秒
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let _ = app.emit("dream:compress-done", json!({"status": "ok"}));
}

// ═══════════════════════════════════════════════════════════
// T-1004: 冷热记忆迁移
// ═══════════════════════════════════════════════════════════

/// 将 >7天 的热记忆从 memory/sessions/ 归档到 wiki/sessions/
/// 在 setup 阶段由 tokio::spawn 调用
pub fn migrate_stale_sessions() {
    let hot_dir = get_session_log_dir();
    let cold_dir = get_cold_session_dir();

    if !hot_dir.exists() { return; }

    let seven_days_ago = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(7 * 24 * 3600))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let entries: Vec<PathBuf> = match fs::read_dir(&hot_dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
        Err(_) => return,
    };

    let mut migrated = 0u32;

    for path in entries {
        if !path.is_file() { continue; }

        let modified = match fs::metadata(&path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if modified < seven_days_ago {
            let filename = match path.file_name() {
                Some(n) => n.to_owned(),
                None => continue,
            };
            let dest = cold_dir.join(&filename);

            // 移动文件（复制+删除，跨盘兼容）
            if let Ok(content) = fs::read(&path) {
                if fs::write(&dest, &content).is_ok() {
                    let _ = fs::remove_file(&path);
                    migrated += 1;
                }
            }
        }
    }

    if migrated > 0 {
        log::info!("Dream T-1004: migrated {} stale sessions to cold storage (wiki/sessions/)", migrated);
    }
}

// ═══════════════════════════════════════════════════════════
// 晨间简报生成（V1 保留）
// ═══════════════════════════════════════════════════════════

/// 从最近的 session 日志中聚合生成一份晨间简报
fn generate_dream_report() {
    let sessions_dir = get_session_log_dir();
    let mut sessions: Vec<Value> = Vec::new();

    if let Ok(entries) = fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(session) = serde_json::from_str::<Value>(&content) {
                    sessions.push(session);
                }
            }
        }
    }

    if sessions.is_empty() {
        return;
    }

    // 按时间戳排序（最近的在前）
    sessions.sort_by(|a, b| {
        let ts_a = a.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
        let ts_b = b.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
        ts_b.cmp(&ts_a)
    });

    // V1 简易简报: 列出最近的几个话题
    let recent = &sessions[..sessions.len().min(5)];
    let mut briefing = String::from("## 最近对话回顾\n\n");
    
    for (i, session) in recent.iter().enumerate() {
        let topics = session.get("userTopics")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("; "))
            .unwrap_or_else(|| "(无主题)".to_string());
        
        let msg_count = session.get("messageCount").and_then(|v| v.as_u64()).unwrap_or(0);
        let compressed = session.get("compressed").and_then(|v| v.as_bool()).unwrap_or(false);
        let tag = if compressed { " [已压缩]" } else { "" };
        
        briefing.push_str(&format!("{}. **{}** ({} 条消息{})\n", i + 1, topics, msg_count, tag));
    }

    let stale_count = if sessions.len() > 10 { sessions.len() - 10 } else { 0 };

    let report = json!({
        "briefing": briefing,
        "stats": {
            "staled": stale_count,
            "merged": 0
        },
        "generatedAt": super::now_ms(),
        "dismissed": false
    });

    if let Ok(data) = serde_json::to_string_pretty(&report) {
        let _ = fs::write(get_dream_path(), data);
    }
}

/// 获取晨间简报（如果存在且未 dismiss）
#[tauri::command]
pub fn system_get_dream_report() -> Value {
    let path = get_dream_path();
    if !path.exists() {
        return Value::Null;
    }

    match fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<Value>(&content) {
                Ok(report) => {
                    // 如果已 dismissed，返回 null
                    if report.get("dismissed").and_then(|v| v.as_bool()).unwrap_or(false) {
                        Value::Null
                    } else {
                        report
                    }
                }
                Err(_) => Value::Null,
            }
        }
        Err(_) => Value::Null,
    }
}

/// 标记晨间简报已读
#[tauri::command]
pub fn system_dismiss_dream() -> bool {
    let path = get_dream_path();
    if !path.exists() {
        return true;
    }

    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(mut report) = serde_json::from_str::<Value>(&content) {
            if let Some(obj) = report.as_object_mut() {
                obj.insert("dismissed".to_string(), json!(true));
                if let Ok(data) = serde_json::to_string_pretty(&report) {
                    let _ = fs::write(&path, data);
                }
            }
        }
    }
    true
}
