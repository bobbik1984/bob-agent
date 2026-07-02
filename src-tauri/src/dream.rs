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
pub fn system_summarize_session(app: tauri::AppHandle, conversation_id: String, db: tauri::State<'_, crate::db::DbState>) -> bool {
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
        "compressed": false,
        // T-1412: 记忆置信度元数据
        "confidence": 0.8,  // 自动提取的摘要初始置信度 0.8 (非用户显式确认)
        "source": "inferred",  // inferred | user_explicit | corrected
        "lastReferenced": super::now_ms()
    });

    // 写入 session 日志
    let session_path = get_session_log_dir().join(format!("{}.json", conversation_id));
    if let Ok(data) = serde_json::to_string_pretty(&summary) {
        let _ = fs::write(session_path, data);
    }

    // 触发更新晨报 (放进后台线程避免网络请求阻塞 IPC)
    std::thread::spawn(move || {
        generate_dream_report(&app);
    });

    true
}

// ═══════════════════════════════════════════════════════════
// T-1003 + T-1421: 异步记忆压缩 + 语义去重 (Dream V2)
// ═══════════════════════════════════════════════════════════

/// T-1421: 从摘要文本中提取关键词指纹（CJK 按句分词，Latin 按空格分词）
fn extract_topic_fingerprint(text: &str) -> std::collections::HashSet<String> {
    let mut keywords = std::collections::HashSet::new();
    // 移除标点和特殊字符
    let cleaned: String = text.chars()
        .map(|c| if c.is_alphanumeric() || c > '\u{2E80}' || c == ' ' { c } else { ' ' })
        .collect();

    for word in cleaned.split_whitespace() {
        let trimmed = word.trim();
        if trimmed.len() >= 2 { // 至少 2 字符/字
            keywords.insert(trimmed.to_lowercase());
        }
    }

    // 对中文文本按 2-gram 切分（简易 bigram）
    let chars: Vec<char> = text.chars()
        .filter(|c| *c > '\u{2E80}')
        .collect();
    for window in chars.windows(2) {
        keywords.insert(window.iter().collect::<String>());
    }

    keywords
}

/// T-1421: 计算两个关键词集合的 Jaccard 相似度
fn jaccard_similarity(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 { return 0.0; }
    intersection as f64 / union as f64
}

/// T-1421: 加载所有已压缩的 .md 文件的关键词指纹
fn load_existing_fingerprints(dir: &std::path::Path) -> Vec<(String, std::collections::HashSet<String>)> {
    let mut fingerprints = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |ext| ext != "md") { continue; }
            if let Ok(content) = fs::read_to_string(&path) {
                let filename = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                fingerprints.push((filename, extract_topic_fingerprint(&content)));
            }
        }
    }
    fingerprints
}

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

    // T-1421: 加载已有的 md 摘要指纹（用于去重检测）
    let mut existing_fps = load_existing_fingerprints(&sessions_dir);
    // 同时检查冷存储目录
    existing_fps.extend(load_existing_fingerprints(&get_cold_session_dir()));
    let mut dedup_count = 0u32;

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

        // ── T-1421: 语义去重检测 ──────────────────────────
        let current_fp = extract_topic_fingerprint(&format!("{} {}", topics, highlight));
        let mut dup_found = false;
        for (existing_name, existing_fp) in &existing_fps {
            let sim = jaccard_similarity(&current_fp, existing_fp);
            if sim > 0.6 {
                log::info!(
                    "T-1421: session {} deduplicated (similarity {:.2} with {})",
                    conv_id, sim, existing_name
                );
                // 标记为已去重合并，不重复压缩
                if let Ok(mut obj) = serde_json::from_str::<Value>(&content) {
                    if let Some(map) = obj.as_object_mut() {
                        map.insert("compressed".to_string(), json!(true));
                        map.insert("dedup_merged".to_string(), json!(existing_name));
                        if let Ok(data) = serde_json::to_string_pretty(&obj) {
                            let _ = fs::write(&path, data);
                        }
                    }
                }
                dup_found = true;
                dedup_count += 1;
                break;
            }
        }
        if dup_found { continue; }
        // ── T-1421 END ──────────────────────────────────

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

        // T-1421: 将新压缩的文件指纹加入已有集合（供后续 session 去重参考）
        let new_fp = extract_topic_fingerprint(&md_content);
        let new_name = md_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        existing_fps.push((new_name, new_fp));

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

    if dedup_count > 0 {
        log::info!("T-1421: deduplicated {} sessions (skipped compression)", dedup_count);
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
async fn fetch_morning_weather() -> Option<String> {
    let cache_path = super::get_data_dir().join("weather_cache.json");
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    
    if let Ok(content) = std::fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<serde_json::Value>(&content) {
            if cache.get("date").and_then(|v| v.as_str()) == Some(&today) {
                if let Some(text) = cache.get("text").and_then(|v| v.as_str()) {
                    return Some(text.to_string());
                }
            }
        }
    }

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build().ok()?;
    
    // Check override in bob.db
    let db_path = super::get_data_dir().join("bob.db");
    let mut override_city: Option<String> = None;
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        if let Ok(val) = conn.query_row("SELECT value FROM settings WHERE key = 'weatherCity'", [], |row| row.get::<_, String>(0)) {
            if !val.trim().is_empty() {
                override_city = Some(val.trim().to_string());
            }
        }
    }

    let (lat, lon, city_name) = if let Some(city) = override_city {
        let geo_url = format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=zh", city);
        let geo_json: Value = client.get(&geo_url).send().await.ok()?.json().await.ok()?;
        let loc = geo_json.get("results")?.as_array()?.first()?;
        let lat = loc.get("latitude")?.as_f64()?;
        let lon = loc.get("longitude")?.as_f64()?;
        (lat, lon, city)
    } else {
        let ip_json: Value = client.get("http://ip-api.com/json/?lang=zh-CN").send().await.ok()?.json().await.ok()?;
        let lat = ip_json.get("lat")?.as_f64()?;
        let lon = ip_json.get("lon")?.as_f64()?;
        let city = ip_json.get("city")?.as_str()?.to_string();
        (lat, lon, city)
    };

    let weather_url = format!("https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=weather_code&daily=temperature_2m_max,temperature_2m_min&timezone=Asia%2FShanghai", lat, lon);
    let w_json: Value = client.get(&weather_url).send().await.ok()?.json().await.ok()?;
    
    let current = w_json.get("current")?;
    let daily = w_json.get("daily")?;
    
    let code = current.get("weather_code")?.as_u64().unwrap_or(0);
    let t_max = daily.get("temperature_2m_max")?.as_array()?.first()?.as_f64()?;
    let t_min = daily.get("temperature_2m_min")?.as_array()?.first()?.as_f64()?;

    let condition = match code {
        0 => "晴",
        1..=3 => "多云",
        45..=48 => "雾",
        51..=55 => "毛毛雨",
        61..=65 => "雨",
        71..=75 => "雪",
        95..=99 => "雷暴",
        _ => "未知",
    };

    Some(format!("{} {} {}°C ~ {}°C\n\n", city_name, condition, t_min.round() as i64, t_max.round() as i64))
}

fn generate_dream_report(app: &tauri::AppHandle) {
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
    let mut briefing = String::new();
    
    if let Some(weather) = tauri::async_runtime::block_on(async { fetch_morning_weather().await }) {
        briefing.push_str(&weather);
    }
    
    let digest_stats = tauri::async_runtime::block_on(async {
        let _ = generate_tag_merge_proposals().await;
        generate_notebook_digest().await
    });
    
    briefing.push_str("## 对话回顾\n\n");
    
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

    // T-1308: 追加今日日程到简报
    let db_path = super::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT title, type, status, start_time, end_time
             FROM events
             WHERE date = ?1 AND status != 'cancelled'
             ORDER BY start_time ASC"
        ) {
            let mut has_schedule = false;
            if let Ok(rows) = stmt.query_map(
                rusqlite::params![today],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            ) {
                for row in rows.flatten() {
                    let (title, etype, status, start_time, _end_time) = row;
                    if !has_schedule {
                        briefing.push_str("\n## 今日日程\n\n");
                        has_schedule = true;
                    }
                    let time_str = start_time.as_deref().unwrap_or("");
                    let status_mark = if status == "done" { "[x]" } else { "[ ]" };
                    let type_tag = if etype == "todo" { "待办" } else { "日程" };
                    briefing.push_str(&format!("- {} **{}** {} {}\n", status_mark, title, type_tag, time_str));
                }
            }
        }
    }

    let stale_count = if sessions.len() > 10 { sessions.len() - 10 } else { 0 };

    // T-1421-b: 统计去重记忆和纠正记忆数量
    let dedup_count = sessions.iter()
        .filter(|s| s.get("dedup_merged").is_some())
        .count();
    let corrected_count = sessions.iter()
        .filter(|s| s.get("source").and_then(|v| v.as_str()) == Some("corrected"))
        .count();

    // T-1421-b + T-1412-b: 追加记忆整理统计到简报
    if dedup_count > 0 || corrected_count > 0 {
        briefing.push_str("\n## 记忆整理\n\n");
        if dedup_count > 0 {
            briefing.push_str(&format!("- 整理了 **{}** 条重复记忆（已自动合并）\n", dedup_count));
        }
        if corrected_count > 0 {
            briefing.push_str(&format!("- 纠正了 **{}** 条过时记忆（置信度已降低）\n", corrected_count));
        }
    }

    if digest_stats.0 > 0 {
        briefing.push_str("\n## 📓 昨日笔记洞察\n\n");
        briefing.push_str(&format!("- 昨夜 AI 帮您重新索引了 **{}** 篇笔记，提取了 **{}** 个关键实体关系并入库图谱。\n", digest_stats.0, digest_stats.1));
    }

    let report = json!({
        "briefing": briefing,
        "stats": {
            "staled": stale_count,
            "merged": dedup_count,
            "corrected": corrected_count,
            "digest_notes": digest_stats.0,
            "digest_entities": digest_stats.1
        },
        "generatedAt": super::now_ms(),
        "dismissed": false
    });

    if let Ok(data) = serde_json::to_string_pretty(&report) {
        let _ = fs::write(get_dream_path(), data);
        let _ = app.emit("dream:completed", &report);
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

// ═══════════════════════════════════════════════════════════
// T-1302: 记忆透明化 — 列出 / 删除记忆条目
// ═══════════════════════════════════════════════════════════

/// 从 session 文件中提取可读标题
/// - .md 文件: 读取首行 `# ...` 标题
/// - .json 文件: 解析 JSON 中的 userTopics / conversationId 生成标题
fn extract_session_title(path: &std::path::Path) -> String {
    let filename = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return filename,
    };

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext == "md" {
        // Markdown: 取首行去掉 # 前缀
        return content.lines().next()
            .map(|l| l.trim().trim_start_matches('#').trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| filename.clone());
    }

    if ext == "json" {
        // JSON: 从 userTopics 和 conversationId 字段生成标题
        if let Ok(obj) = serde_json::from_str::<Value>(&content) {
            // 优先用 userTopics 第一条（截取前 40 字符）
            if let Some(topics) = obj.get("userTopics").and_then(|v| v.as_array()) {
                if let Some(first) = topics.first().and_then(|v| v.as_str()) {
                    let trimmed: String = first.chars().take(40).collect();
                    if !trimmed.is_empty() {
                        let suffix = if first.chars().count() > 40 { "..." } else { "" };
                        return format!("对话摘要: {}{}", trimmed, suffix);
                    }
                }
            }
            // 降级: 用 conversationId
            if let Some(conv_id) = obj.get("conversationId").and_then(|v| v.as_str()) {
                return format!("对话摘要: {}", conv_id);
            }
        }
    }

    filename
}

/// 收集指定目录中的 session 文件，返回 (filename, entry_json) 列表
fn collect_session_entries(
    dir: &std::path::Path,
    entry_type: &str,
    existing_md_set: Option<&std::collections::HashSet<String>>,
) -> Vec<Value> {
    let mut entries = Vec::new();
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return entries,
    };

    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_file() { continue; }

        let filename = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // 如果是 .json 且同名 .md 已存在（已压缩），则跳过
        if let Some(md_set) = existing_md_set {
            if filename.ends_with(".json") {
                let md_name = filename.replace(".json", ".md");
                if md_set.contains(&md_name) {
                    continue;
                }
            }
        }

        let title = extract_session_title(&path);

        let meta = fs::metadata(&path).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta.and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        entries.push(json!({
            "type": entry_type,
            "id": filename,
            "title": title,
            "size": size,
            "modified": modified,
        }));
    }

    entries
}

/// 列出所有记忆条目 (热 session + 冷 session + wiki 知识条目)
#[tauri::command]
pub fn system_get_memory_entries() -> Value {
    let wiki_dir = super::get_wiki_dir();
    let mut entries: Vec<Value> = Vec::new();

    // 预扫描冷存储中的 .md 文件名集合，用于去重
    let cold_sessions_dir = wiki_dir.join("sessions");
    let cold_md_set: std::collections::HashSet<String> = if cold_sessions_dir.exists() {
        fs::read_dir(&cold_sessions_dir)
            .into_iter()
            .flat_map(|rd| rd.flatten())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".md") { Some(name) } else { None }
            })
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    // 1. 扫描热存储 memory/sessions/ (最近 7 天内的活跃记忆)
    let hot_dir = get_session_log_dir();
    if hot_dir.exists() {
        entries.extend(collect_session_entries(&hot_dir, "session_hot", None));
    }

    // 2. 扫描冷存储 wiki/sessions/ (已归档的记忆)
    if cold_sessions_dir.exists() {
        entries.extend(collect_session_entries(&cold_sessions_dir, "session", Some(&cold_md_set)));
    }

    // 3. 扫描 wiki/ 根目录下的 .md 文件 (知识条目)
    if let Ok(rd) = fs::read_dir(&wiki_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            if path.extension().map_or(true, |ext| ext != "md") { continue; }

            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let title = fs::read_to_string(&path)
                .ok()
                .and_then(|c| c.lines().next().map(|l| l.trim().trim_start_matches('#').trim().to_string()))
                .unwrap_or_else(|| filename.clone());

            let meta = fs::metadata(&path).ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta.and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            entries.push(json!({
                "type": "wiki",
                "id": filename,
                "title": title,
                "size": size,
                "modified": modified,
            }));
        }
    }

    // 按修改时间倒序排列
    entries.sort_by(|a, b| {
        let ma = a.get("modified").and_then(|v| v.as_i64()).unwrap_or(0);
        let mb = b.get("modified").and_then(|v| v.as_i64()).unwrap_or(0);
        mb.cmp(&ma)
    });

    json!(entries)
}

/// 删除指定的记忆条目
#[tauri::command]
pub fn system_delete_memory_entry(entry_type: String, entry_id: String) -> Value {
    let wiki_dir = super::get_wiki_dir();

    // 根据类型构建目标路径和安全校验根目录
    let (target_path, safe_root) = match entry_type.as_str() {
        "session" => (wiki_dir.join("sessions").join(&entry_id), wiki_dir.clone()),
        "session_hot" => {
            let mem_dir = get_memory_dir();
            (mem_dir.join("sessions").join(&entry_id), mem_dir)
        },
        "wiki" => (wiki_dir.join(&entry_id), wiki_dir.clone()),
        _ => return json!({"error": format!("unknown entry type: {}", entry_type)}),
    };

    // 安全校验: 规范化路径后确认仍在安全根目录内 (防止路径穿越)
    let canonical_root = match fs::canonicalize(&safe_root) {
        Ok(p) => p,
        Err(e) => return json!({"error": format!("root dir resolve failed: {}", e)}),
    };
    let canonical_target = match fs::canonicalize(&target_path) {
        Ok(p) => p,
        Err(_) => return json!({"error": "file not found"}),
    };
    if !canonical_target.starts_with(&canonical_root) {
        return json!({"error": "path traversal blocked"});
    }

    // 执行删除
    match fs::remove_file(&canonical_target) {
        Ok(_) => {
            log::info!("T-1302: deleted memory entry {:?}", canonical_target);
            json!({"ok": true})
        }
        Err(e) => json!({"error": format!("delete failed: {}", e)}),
    }
}

// ═══════════════════════════════════════════════════════════
// T-1412: 记忆置信度衰减与引用更新
// ═══════════════════════════════════════════════════════════

/// 置信度衰减: 扫描所有 session JSON，将超过 30 天未被引用的记忆的 confidence 衰减
/// 在 `migrate_stale_sessions` 之后调用（冷迁移阶段顺带处理）
pub fn decay_stale_confidence() {
    let sessions_dir = get_session_log_dir();
    if !sessions_dir.exists() { return; }

    let thirty_days_ms: i64 = 30 * 24 * 3600 * 1000;
    let now_ms = super::now_ms();
    let mut decayed = 0u32;

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
        let mut session: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let last_ref = session.get("lastReferenced").and_then(|v| v.as_i64()).unwrap_or(0);
        let confidence = session.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.8);

        // 如果超过 30 天未被引用且 confidence > 0.1，衰减 20%
        if last_ref > 0 && (now_ms - last_ref) > thirty_days_ms && confidence > 0.1 {
            let new_confidence = (confidence * 0.8).max(0.0);
            if let Some(obj) = session.as_object_mut() {
                obj.insert("confidence".to_string(), json!(new_confidence));
                if let Ok(data) = serde_json::to_string_pretty(&session) {
                    let _ = fs::write(&path, data);
                    decayed += 1;
                }
            }
        }
    }

    if decayed > 0 {
        log::info!("T-1412: decayed confidence for {} stale memories", decayed);
    }
}

/// 更新指定 session 的 lastReferenced 时间戳（当记忆被注入上下文时调用）
pub fn touch_memory_reference(session_id: &str) {
    let path = get_session_log_dir().join(format!("{}.json", session_id));
    if !path.exists() { return; }

    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(mut session) = serde_json::from_str::<Value>(&content) {
            if let Some(obj) = session.as_object_mut() {
                obj.insert("lastReferenced".to_string(), json!(super::now_ms()));
                if let Ok(data) = serde_json::to_string_pretty(&session) {
                    let _ = fs::write(&path, data);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 2.5: Tag Deduplication Pipeline
// ═══════════════════════════════════════════════════════════

pub async fn generate_tag_merge_proposals() {
    let res = match crate::notebook::notebook_list_all_tags() {
        Ok(v) => v,
        Err(_) => return,
    };
    
    let tags_list = match res.get("tags").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return,
    };
    
    if tags_list.len() < 2 {
        return;
    }
    
    let tag_names: Vec<String> = tags_list.iter()
        .filter_map(|v| v.get("tag").and_then(|t| t.as_str()).map(|s| s.to_string()))
        .collect();
        
    let mut exclusions: Vec<Vec<String>> = Vec::new();
    let parent_dir = match crate::notebook::get_notes_dir().parent() {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    let exclusions_path = parent_dir.join("tag_merge_exclusions.json");
    if let Ok(content) = fs::read_to_string(&exclusions_path) {
        if let Ok(json_arr) = serde_json::from_str::<Vec<Vec<String>>>(&content) {
            exclusions = json_arr;
        }
    }
    
    let exclusions_str = serde_json::to_string(&exclusions).unwrap_or_else(|_| "[]".to_string());
    let tags_str = serde_json::to_string(&tag_names).unwrap_or_else(|_| "[]".to_string());
    
    let (_, api_key, model_id, base_url) = crate::llm::read_llm_config_for_model("clerk");
    if api_key.is_empty() {
        return;
    }
    
    let prompt_system = "你是一个标签整理助手。你的任务是从以下标签列表中，找出语义相同或极度相似的标签（如 'js' 和 'javascript'，'ai' 和 '人工智能'，'llm' 和 '大模型'，或是单复数、大小写不同），并将它们归类。如果没有任何相似的标签，请返回空数组。必须返回合法的JSON数组，格式为: `[{\"canonical\": \"主标签名称\", \"aliases\": [\"需要被合并的标签1\", \"需要被合并的标签2\"]}]`。注意：不要将只是宽泛相关的标签合并（比如不要把 javascript 和 typescript 合并，它们是两门独立语言）。";
    let prompt_user = format!("标签列表：{}\n\n排除列表（这些是用户明确拒绝合并的，绝对不要把它们合并）：{}", tags_str, exclusions_str);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
        
    let body = json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": prompt_system },
            { "role": "user", "content": prompt_user }
        ],
        "max_tokens": 1024,
        "temperature": 0.1,
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
            log::warn!("Failed to request Clerk for tag proposals: {}", e);
            return;
        }
    };
    
    if !resp.status().is_success() {
        log::warn!("Tag proposal API error: {}", resp.status());
        return;
    }
    
    let resp_json: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return,
    };
    
    let mut content = resp_json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
        
    let start_idx = content.find('[');
    let end_idx = content.rfind(']');
    if let (Some(s), Some(e)) = (start_idx, end_idx) {
        if e > s {
            content = content[s..e+1].to_string();
        }
    }
    
    if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
        if parsed.is_array() {
            let proposals_path = get_memory_dir().join("tag_merge_proposals.json");
            if let Ok(data) = serde_json::to_string_pretty(&parsed) {
                let _ = fs::write(&proposals_path, data);
            }
        }
    }
}

#[tauri::command]
pub fn system_get_tag_proposals() -> Value {
    let path = get_memory_dir().join("tag_merge_proposals.json");
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(val) = serde_json::from_str::<Value>(&content) {
            return val;
        }
    }
    json!([])
}

#[tauri::command]
pub fn system_clear_tag_proposals() -> bool {
    let path = get_memory_dir().join("tag_merge_proposals.json");
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    true
}


pub async fn generate_notebook_digest() -> (usize, usize) {
    let notes_dir = crate::notebook::get_notes_dir();
    let topics_dir = notes_dir.join("topics");
    let projects_dir = notes_dir.join("projects");
    
    let mut files_to_process = Vec::new();
    let threshold = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 3600);
    
    for dir in &[topics_dir, projects_dir] {
        if !dir.exists() { continue; }
        for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if modified > threshold {
                            files_to_process.push(entry.path().to_path_buf());
                        }
                    }
                }
            }
        }
    }
    
    if files_to_process.is_empty() { return (0, 0); }
    
    let (_, api_key, model_id, base_url) = crate::llm::read_llm_config_for_model("clerk");
    if api_key.is_empty() { return (0, 0); }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    
    let mut processed = 0;
    let mut entities_extracted = 0;
    
    let db_path = crate::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    
    let prompt_system = "你是一个专业的信息提取员。请从我提供的笔记内容中，提取出最重要的实体（如：人物、技术名词、书籍、概念、项目等），并简要概括它们与该笔记的关系。必须严格返回 JSON 数组格式，例如：[{\"entity\": \"Elon Musk\", \"type\": \"person\", \"relation\": \"提到了\"}]。允许的 type 包括: person, topic, concept, project, tag。如果没有明显的实体，请返回空数组 []。";
    
    for path in files_to_process.iter().take(5) { // max 5 notes per dream to save tokens
        let content = fs::read_to_string(path).unwrap_or_default();
        if content.len() < 50 { continue; }
        let rel_path = path.strip_prefix(&notes_dir).unwrap_or(path).to_string_lossy().replace("\\", "/");
        let note_id = format!("note_{}", rel_path);
        
        let prompt_user = format!("笔记内容：\n\n{}", content.chars().take(2000).collect::<String>());
        let body = serde_json::json!({
            "model": model_id,
            "messages": [
                { "role": "system", "content": prompt_system },
                { "role": "user", "content": prompt_user }
            ],
            "max_tokens": 1024,
            "temperature": 0.1,
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
            Err(_) => continue,
        };
        
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(content) = json.get("choices").and_then(|c| c.as_array()).and_then(|arr| arr.get(0)).and_then(|c| c.get("message")).and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                let cleaned = content.replace("```json", "").replace("```", "").trim().to_string();
                if let Ok(entities) = serde_json::from_str::<Vec<serde_json::Value>>(&cleaned) {
                    for entity in entities {
                        let name = entity.get("entity").and_then(|v| v.as_str()).unwrap_or("").trim();
                        let etype = entity.get("type").and_then(|v| v.as_str()).unwrap_or("topic").trim();
                        let relation = entity.get("relation").and_then(|v| v.as_str()).unwrap_or("mentions").trim();
                        
                        if name.is_empty() { continue; }
                        
                        let target_id = crate::kg::resolve_node_id(&conn, name, etype);
                        let note_label = path.file_name().unwrap_or_default().to_string_lossy();
                        let _ = crate::kg::upsert_node(&conn, &note_id, &note_label, "file", "", "dream_digest", "");
                        let _ = crate::kg::upsert_node(&conn, &target_id, name, etype, "", "dream_digest", "");
                        let _ = crate::kg::insert_edge(&conn, &note_id, &target_id, relation, 0.7);
                        entities_extracted += 1;
                    }
                    processed += 1;
                }
            }
        }
    }
    
    (processed, entities_extracted)
}
