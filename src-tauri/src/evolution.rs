// ═══════════════════════════════════════════════════════════
// 进化引擎 v1.0
//
// 灵感来源: CodeRunner 的 SessionObserver + MemoryExtractor
// 适配: Tauri 桌面端特性 (休眠补偿, tokio::spawn 后台静默)
//
// 子系统:
//   1. capture_observation()  — 零 LLM 成本遥测
//   2. extract_learned_facts() — Clerk 模型自动提取知识
// ═══════════════════════════════════════════════════════════

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;

// ── 冷却缓存: 防止同一会话短时间内重复触发 Clerk 提取 ─────
static LAST_EXTRACTION: std::sync::OnceLock<Mutex<HashMap<String, std::time::Instant>>> = std::sync::OnceLock::new();

// ── 遥测数据结构 ────────────────────────────────────────────

/// 对话执行的零成本观测快照
pub struct ObservationRecord {
    pub conversation_id: String,
    pub model_used: String,
    pub tool_calls_count: i64,
    pub tool_failures: i64,
    pub total_rounds: i64,
    pub duration_ms: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub stop_reason: String,
}

// ═══════════════════════════════════════════════════════════
// 子系统 1: 零成本遥测捕获
// ═══════════════════════════════════════════════════════════

/// 将对话执行快照写入 bob.db/session_observations
/// 纯计数器操作，零 LLM 成本
pub fn capture_observation(record: &ObservationRecord) {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[Evolution] capture_observation DB open failed: {}", e);
            return;
        }
    };

    let now = super::now_ms();
    let result = conn.execute(
        "INSERT INTO session_observations
         (conversation_id, model_used, tool_calls_count, tool_failures,
          total_rounds, duration_ms, tokens_in, tokens_out, stop_reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            record.conversation_id,
            record.model_used,
            record.tool_calls_count,
            record.tool_failures,
            record.total_rounds,
            record.duration_ms,
            record.tokens_in,
            record.tokens_out,
            record.stop_reason,
            now,
        ],
    );

    match result {
        Ok(_) => log::info!(
            "[Evolution] Observation captured: conv={}, rounds={}, tools={}, failures={}",
            record.conversation_id,
            record.total_rounds,
            record.tool_calls_count,
            record.tool_failures,
        ),
        Err(e) => log::warn!("[Evolution] Failed to save observation: {}", e),
    }
}

// ═══════════════════════════════════════════════════════════
// 子系统 2: 对话后自动知识提取
// ═══════════════════════════════════════════════════════════

/// 知识湖目录
fn get_learned_dir() -> PathBuf {
    let dir = super::get_wiki_dir().join("learned");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 三层漏斗判断: 对话是否值得触发 Clerk 知识提取
///
/// Layer 1 (快车道): 检测助手回复中的 <|mem|> 隐式标记
/// Layer 2 (安全网): 物理兜底 — 有工具调用 OR 用户发言 >= 3 轮
fn should_extract(messages: &[Value], total_rounds: i64) -> bool {
    // ── Layer 1: 快车道 — 检测助手回复中是否有隐式标记 ──
    for msg in messages {
        if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                if content.contains("<|mem|>") {
                    log::info!("[Evolution] Triggered via <|mem|> marker");
                    return true;
                }
            }
        }
    }

    // ── Layer 2: 物理安全网 ─────────────────────────────
    // 兜底 A: 发生过工具调用 (说明有实质操作)
    if total_rounds > 0 {
        log::info!("[Evolution] Triggered via fallback (tool rounds: {})", total_rounds);
        return true;
    }

    // 兜底 B: 用户对话深入 (用户发言 >= 3 次)
    let user_msg_count = messages.iter()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .count();
    if user_msg_count >= 3 {
        log::info!("[Evolution] Triggered via fallback (user messages: {})", user_msg_count);
        return true;
    }

    false
}

/// 从对话尾部提取持久性事实，写入 wiki/learned/
/// 使用 clerkModel (最便宜的模型) 执行提取
pub async fn extract_learned_facts(app: AppHandle, messages: Vec<Value>, conv_id: String, total_rounds: i64) {
    // ── Step 1: 只读检查冷却 (不占位) ────────────────────
    let now_instant = std::time::Instant::now();
    let cache = LAST_EXTRACTION.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let map = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&last_time) = map.get(&conv_id) {
            if now_instant.duration_since(last_time).as_secs() < 30 {
                log::info!("[Evolution] Skipping extraction for conv={} (cooldown active)", conv_id);
                return;
            }
        }
    } // lock 释放

    // ── Step 2: 三层漏斗判断 ────────────────────────────
    if !should_extract(&messages, total_rounds) {
        log::info!("[Evolution] Skipping extraction for conv={} (trivial chat)", conv_id);
        return; // 不写入冷却，不占位
    }

    // ── Step 3: 通过了！写入冷却时间戳 ──────────────────
    {
        let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(conv_id.clone(), now_instant);
    } // lock 释放

    // 4. 读取 clerkModel 配置
    let config = super::read_config();
    let clerk_model = config.get("clerkModel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if clerk_model.is_empty() {
        log::info!("[Evolution] No clerkModel configured, skipping extraction");
        return;
    }

    // 3. 取最后 10 条消息 (截断长消息)
    let recent: Vec<String> = messages.iter()
        .rev()
        .take(10)
        .rev()
        .filter_map(|m| {
            let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
            let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if content.is_empty() { return None; }
            // 截断每条消息到 2000 字符
            let truncated: String = content.chars().take(2000).collect();
            // 清洗 <|mem|> 标记，不让 Clerk 看到无意义的暗号
            let cleaned = if role == "assistant" {
                truncated.replace("<|mem|>", "")
            } else {
                truncated
            };
            Some(format!("[{}]: {}", role, cleaned))
        })
        .collect();

    if recent.is_empty() {
        return;
    }

    let chat_log = recent.join("\n\n");

    // 4. 构建提取 prompt
    let extraction_prompt = format!(
        r#"你是一个知识提取引擎。分析以下对话，提取出**持久性事实**（不是一次性的操作步骤）。

## 规则
- 只提取 **NEW** 的事实（不重复常识）
- 只提取 **PERSISTENT** 的事实（会在未来的对话中有用）
- 每条事实必须是独立的、可复用的知识点

## 输出格式
返回 JSON 数组（如果没有值得提取的内容，返回空数组 `[]`）：
```json
[
  {{"type": "user", "title": "简短标题", "content": "具体内容"}},
  {{"type": "project", "title": "简短标题", "content": "具体内容"}}
]
```

type 可选值：
- `user`: 用户偏好/习惯/环境信息
- `project`: 项目决策/架构规则/技术选型
- `feedback`: 对 AI 错误的纠正（最重要！防止重犯）
- `reference`: 可复用的代码片段/命令/URL

## 对话记录
{}"#,
        chat_log
    );

    // 5. 调用 Clerk 模型 (使用已有的 LLM 基础设施)
    let (provider, api_key, model_id, base_url) = super::llm::read_llm_config_for_model(&clerk_model);

    if api_key.is_empty() && provider != "offline" {
        log::info!("[Evolution] Clerk model {} has no API key, skipping extraction", clerk_model);
        return;
    }

    // 处理 GCP Token
    let final_api_key = if api_key == "__GCP_TOKEN__" {
        let cred_path = super::gcp_auth::get_gcp_credential_path();
        match super::gcp_auth::GcpTokenManager::from_file(&cred_path) {
            Ok(mgr) => match mgr.get_access_token().await {
                Ok(token) => token,
                Err(_) => { log::warn!("[Evolution] GCP token failed for clerk"); return; }
            },
            Err(_) => { return; }
        }
    } else {
        api_key
    };

    let url = format!("{}/chat/completions", base_url);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build() {
        Ok(c) => c,
        Err(_) => return,
    };

    let body = json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": "You are a knowledge extraction engine. Output ONLY valid JSON." },
            { "role": "user", "content": extraction_prompt }
        ],
        "temperature": 0.1,
        "max_tokens": 2048,
    });

    let resp = match client.post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", final_api_key))
        .json(&body)
        .send()
        .await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            log::info!("[Evolution] Clerk API returned {}, skipping extraction", r.status());
            return;
        }
        Err(e) => {
            log::info!("[Evolution] Clerk API request failed: {}, skipping extraction", e);
            return;
        }
    };

    let resp_json: Value = match resp.json().await {
        Ok(j) => j,
        Err(_) => return,
    };

    // 6. 解析 LLM 响应
    let content_str = resp_json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");

    // 清理 markdown 代码围栏
    let cleaned = content_str
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let facts: Vec<Value> = match serde_json::from_str(cleaned) {
        Ok(arr) => arr,
        Err(_) => {
            log::info!("[Evolution] Could not parse extraction response as JSON array");
            return;
        }
    };

    if facts.is_empty() {
        log::info!("[Evolution] No facts extracted from conv={}", conv_id);
        return;
    }

    // 7. 将事实写入 wiki/learned/ 目录
    let learned_dir = get_learned_dir();
    let now = chrono::Local::now();
    let mut saved_count = 0;

    for fact in &facts {
        let fact_type = fact.get("type").and_then(|v| v.as_str()).unwrap_or("reference");
        let title = fact.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
        let content = fact.get("content").and_then(|v| v.as_str()).unwrap_or("");

        if title.is_empty() || content.is_empty() {
            continue;
        }

        // 生成文件名: {type}_{slug}_{timestamp}.md
        let slug: String = title.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c > '\u{4E00}')
            .take(30)
            .collect();
        let ts = now.format("%m%d%H%M").to_string();
        let filename = format!("{}_{}{}.md", fact_type, slug, ts);
        let file_path = learned_dir.join(&filename);

        // YAML frontmatter + 内容
        let md_content = format!(
            "---\ntype: {}\ntitle: \"{}\"\nsource_conv: \"{}\"\nupdated: \"{}\"\n---\n\n# {}\n\n{}\n",
            fact_type,
            title.replace('"', "'"),
            conv_id,
            now.format("%Y-%m-%d %H:%M"),
            title,
            content,
        );

        match std::fs::write(&file_path, md_content.as_bytes()) {
            Ok(_) => {
                saved_count += 1;
                log::info!("[Evolution] Saved fact: {} -> {:?}", title, file_path);
            }
            Err(e) => {
                log::warn!("[Evolution] Failed to write fact file: {}", e);
            }
        }
    }

    // 8. 同步更新 wiki_fts 索引 (让 brain_search 立即可检索)
    if saved_count > 0 {
        let db_path = super::get_data_dir().join("bob.db");
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            for fact in &facts {
                let fact_type = fact.get("type").and_then(|v| v.as_str()).unwrap_or("reference");
                let title = fact.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let content = fact.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let keywords = format!("{} {}", fact_type, title);

                let slug: String = title.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c > '\u{4E00}')
                    .take(30)
                    .collect();
                let ts = now.format("%m%d%H%M").to_string();
                let filename = format!("{}_{}{}.md", fact_type, slug, ts);

                let wiki_path = format!("wiki/learned/{}", filename);

                let _ = conn.execute(
                    "INSERT INTO wiki_fts (file_name, source_path, wiki_path, summary, keywords, category, indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        filename,
                        conv_id,
                        wiki_path,
                        content.chars().take(300).collect::<String>(),
                        keywords,
                        format!("learned_{}", fact_type),
                        now.format("%Y-%m-%d %H:%M:%S").to_string(),
                    ],
                );
            }
        }

        log::info!(
            "[Evolution] Extraction complete: {} facts saved from conv={}",
            saved_count, conv_id
        );
    }
}

// ═══════════════════════════════════════════════════════════
// 子系统 3: 静默做梦引擎 (Dream Worker)
//
// 桌面端特性: 不依赖固定 Cron，而是基于 last_dream_timestamp
// 的时差补偿触发。Bob 每日"醒来"时静默运行。
// ═══════════════════════════════════════════════════════════

/// 24 小时（毫秒）
const DREAM_INTERVAL_MS: i64 = 24 * 3600 * 1000;

/// 获取上次做梦时间戳 (从 evolution_log 表读取)
fn get_last_dream_timestamp() -> i64 {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    conn.query_row(
        "SELECT MAX(created_at) FROM evolution_log",
        [],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0)
}

/// 检查是否需要做梦，如果需要则执行 (被 scheduler.rs 的 tick 调用)
pub async fn check_and_dream(app: AppHandle) {
    let now = super::now_ms();
    let last = get_last_dream_timestamp();

    if last > 0 && (now - last) < DREAM_INTERVAL_MS {
        return; // 距离上次做梦不到 24 小时，跳过
    }

    log::info!("[Evolution] Dream triggered: last_dream={}, gap={}h",
        last, if last > 0 { (now - last) / 3_600_000 } else { 999 });

    // 执行做梦流水线
    let report = run_dream_pipeline(&app).await;

    // 记录做梦日志
    let db_path = super::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let _ = conn.execute(
            "INSERT INTO evolution_log
             (dream_type, facts_extracted, stale_cleaned, memories_merged, soul_refined, report_text, soul_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "daily_catchup",
                report.facts_extracted,
                report.stale_cleaned,
                report.memories_merged,
                report.soul_refined as i64,
                report.summary,
                report.soul_hash,
                now,
            ],
        );
    }

    log::info!("[Evolution] Dream complete: stale={}, merged={}, soul_refined={}",
        report.stale_cleaned, report.memories_merged, report.soul_refined);
}

struct DreamReport {
    facts_extracted: i64,
    stale_cleaned: i64,
    memories_merged: i64,
    soul_refined: bool,
    summary: String,
    soul_hash: String,
}

/// 四阶段梦境流水线
async fn run_dream_pipeline(_app: &AppHandle) -> DreamReport {
    let mut report = DreamReport {
        facts_extracted: 0,
        stale_cleaned: 0,
        memories_merged: 0,
        soul_refined: false,
        summary: String::new(),
        soul_hash: String::new(),
    };

    // ── Phase 1: 过时淘汰 ──────────────────────────────────
    report.stale_cleaned = phase_stale_cleanup();

    // ── Phase 2: 相似合并 ──────────────────────────────────
    report.memories_merged = phase_merge_similar();

    // ── Phase 3: SOUL 精炼 ─────────────────────────────────
    let (refined, hash) = phase_soul_refinement(_app).await;
    report.soul_refined = refined;
    report.soul_hash = hash;

    // 构建摘要
    let mut summary_parts = Vec::new();
    if report.stale_cleaned > 0 {
        summary_parts.push(format!("清理 {} 条过时记忆", report.stale_cleaned));
    }
    if report.memories_merged > 0 {
        summary_parts.push(format!("合并 {} 条相似记忆", report.memories_merged));
    }
    if report.soul_refined {
        summary_parts.push("SOUL.md 已精炼".to_string());
    }
    report.summary = if summary_parts.is_empty() {
        "无需更新".to_string()
    } else {
        summary_parts.join("; ")
    };

    report
}

/// Phase 1: 过时淘汰 — 清理 30 天未更新且未被引用的 learned 记忆
fn phase_stale_cleanup() -> i64 {
    let learned_dir = get_learned_dir();
    if !learned_dir.exists() { return 0; }

    let thirty_days_ago = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(30 * 24 * 3600))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let mut cleaned = 0i64;

    let entries: Vec<PathBuf> = match std::fs::read_dir(&learned_dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect(),
        Err(_) => return 0,
    };

    for path in entries {
        let modified = match std::fs::metadata(&path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if modified < thirty_days_ago {
            // 读取 frontmatter 检查是否有 superseded 标记
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains("superseded: true") {
                    // 已标记为过时的，直接物理删除
                    let _ = std::fs::remove_file(&path);
                    cleaned += 1;
                    continue;
                }
            }
            // 30天以上但未标记：打上 superseded 标记（下次做梦时删除）
            if let Ok(content) = std::fs::read_to_string(&path) {
                let marked = content.replacen("---\n", "---\nsuperseded: true\n", 1);
                let _ = std::fs::write(&path, marked);
                cleaned += 1;
            }
        }
    }

    if cleaned > 0 {
        log::info!("[Evolution] Dream Phase 1: cleaned/marked {} stale memories", cleaned);
    }
    cleaned
}

/// Phase 2: 相似合并 — 基于标题文本重叠率去重
fn phase_merge_similar() -> i64 {
    let learned_dir = get_learned_dir();
    if !learned_dir.exists() { return 0; }

    let entries: Vec<PathBuf> = match std::fs::read_dir(&learned_dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| {
            p.is_file() && p.extension().map_or(false, |ext| ext == "md")
        }).collect(),
        Err(_) => return 0,
    };

    // 提取所有标题
    let mut titles: Vec<(PathBuf, String, std::time::SystemTime)> = Vec::new();
    for path in &entries {
        if let Ok(content) = std::fs::read_to_string(path) {
            // 跳过已标记为过时的
            if content.contains("superseded: true") { continue; }

            let title = content.lines()
                .find(|l| l.starts_with("title:"))
                .map(|l| l.trim_start_matches("title:").trim().trim_matches('"').to_string())
                .unwrap_or_default();

            let modified = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);

            if !title.is_empty() {
                titles.push((path.clone(), title, modified));
            }
        }
    }

    let mut merged = 0i64;
    let mut removed_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for i in 0..titles.len() {
        if removed_paths.contains(&titles[i].0) { continue; }

        for j in (i + 1)..titles.len() {
            if removed_paths.contains(&titles[j].0) { continue; }

            let similarity = title_similarity(&titles[i].1, &titles[j].1);
            if similarity > 0.7 {
                // 保留较新的，标记较旧的为过时
                let older = if titles[i].2 < titles[j].2 { &titles[i].0 } else { &titles[j].0 };
                if let Ok(content) = std::fs::read_to_string(older) {
                    let marked = content.replacen("---\n", "---\nsuperseded: true\n", 1);
                    let _ = std::fs::write(older, marked);
                }
                removed_paths.insert(older.clone());
                merged += 1;
            }
        }
    }

    if merged > 0 {
        log::info!("[Evolution] Dream Phase 2: merged {} similar memories", merged);
    }
    merged
}

/// 简易标题相似度 (Jaccard 字符 N-gram)
fn title_similarity(a: &str, b: &str) -> f64 {
    let a_chars: std::collections::HashSet<char> = a.chars().collect();
    let b_chars: std::collections::HashSet<char> = b.chars().collect();
    if a_chars.is_empty() && b_chars.is_empty() { return 1.0; }
    let intersection = a_chars.intersection(&b_chars).count();
    let union = a_chars.union(&b_chars).count();
    if union == 0 { return 0.0; }
    intersection as f64 / union as f64
}

/// Phase 3: SOUL 精炼 — 结合新记忆重写 SOUL.md
/// 附带 hash 防冲突保护：如果用户手动编辑过 SOUL，跳过重写
async fn phase_soul_refinement(app: &AppHandle) -> (bool, String) {
    let memory_dir = super::get_data_dir().join("memory");
    let soul_path = memory_dir.join("SOUL.md");

    // 读取当前 SOUL
    let current_soul = if soul_path.exists() {
        std::fs::read_to_string(&soul_path).unwrap_or_default()
    } else {
        String::new()
    };

    // 计算当前 hash (SHA256 简化版: 取前 16 位)
    let current_hash = simple_hash(&current_soul);

    // 检查上次做梦时记录的 hash，如果不同说明用户手动编辑过
    let db_path = super::get_data_dir().join("bob.db");
    let last_hash = if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        conn.query_row(
            "SELECT soul_hash FROM evolution_log ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap_or_default()
    } else {
        String::new()
    };

    if !last_hash.is_empty() && last_hash != current_hash {
        log::info!("[Evolution] SOUL.md manually edited (hash mismatch), skipping refinement");
        return (false, current_hash);
    }

    // 如果 SOUL 为空，暂不生成（需要用户先写一版初稿）
    if current_soul.trim().is_empty() {
        return (false, current_hash);
    }

    // 收集最新的 learned 事实（最多 10 条最新的）
    let learned_dir = get_learned_dir();
    let mut recent_facts = Vec::new();
    if learned_dir.exists() {
        let mut entries: Vec<(PathBuf, std::time::SystemTime)> = std::fs::read_dir(&learned_dir)
            .into_iter()
            .flat_map(|rd| rd.flatten())
            .filter_map(|e| {
                let p = e.path();
                if p.is_file() {
                    let m = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
                    Some((p, m))
                } else { None }
            })
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        for (path, _) in entries.into_iter().take(10) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains("superseded: true") { continue; }
                recent_facts.push(content);
            }
        }
    }

    if recent_facts.is_empty() {
        return (false, current_hash);
    }

    // 调用 Clerk 模型精炼 SOUL
    let config = super::read_config();
    let clerk_model = config.get("clerkModel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if clerk_model.is_empty() {
        return (false, current_hash);
    }

    let (provider, api_key, model_id, base_url) = super::llm::read_llm_config_for_model(&clerk_model);
    if api_key.is_empty() && provider != "offline" {
        return (false, current_hash);
    }

    let final_api_key = if api_key == "__GCP_TOKEN__" {
        let cred_path = super::gcp_auth::get_gcp_credential_path();
        match super::gcp_auth::GcpTokenManager::from_file(&cred_path) {
            Ok(mgr) => match mgr.get_access_token().await {
                Ok(token) => token,
                Err(_) => return (false, current_hash),
            },
            Err(_) => return (false, current_hash),
        }
    } else {
        api_key
    };

    let facts_text = recent_facts.join("\n---\n");
    let refinement_prompt = format!(
        r#"你是一个人格精炼引擎。根据以下最新的知识事实，精炼更新下面的 SOUL 人格文件。

## 规则
- 保持 SOUL 在 300-500 字以内
- 只做精细微调（更新事实、修正偏差），不要大幅改写风格
- 保留用户已有的核心人格设定
- 输出 SOUL.md 的完整新内容（纯 Markdown，不要代码围栏）

## 当前 SOUL.md
{current_soul}

## 最新知识事实
{facts_text}"#
    );

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build() {
        Ok(c) => c,
        Err(_) => return (false, current_hash),
    };

    let body = json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": "你是 SOUL.md 人格精炼引擎。只输出精炼后的完整 SOUL 内容。" },
            { "role": "user", "content": refinement_prompt }
        ],
        "temperature": 0.3,
        "max_tokens": 1024,
    });

    let url = format!("{}/chat/completions", base_url);
    let resp = match client.post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", final_api_key))
        .json(&body)
        .send()
        .await {
        Ok(r) if r.status().is_success() => r,
        _ => return (false, current_hash),
    };

    let resp_json: Value = match resp.json().await {
        Ok(j) => j,
        Err(_) => return (false, current_hash),
    };

    let new_soul = resp_json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .trim_start_matches("```markdown")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    if new_soul.is_empty() || new_soul.len() < 50 {
        log::warn!("[Evolution] SOUL refinement output too short, skipping");
        return (false, current_hash);
    }

    // 字数检查 (≤500 字硬上限)
    let char_count = new_soul.chars().count();
    if char_count > 600 {
        log::warn!("[Evolution] SOUL refinement output too long ({}字), skipping", char_count);
        return (false, current_hash);
    }

    // 写入新 SOUL
    let _ = std::fs::create_dir_all(&memory_dir);
    match std::fs::write(&soul_path, &new_soul) {
        Ok(_) => {
            let new_hash = simple_hash(&new_soul);
            log::info!("[Evolution] SOUL.md refined: {}字, hash={}", char_count, new_hash);
            (true, new_hash)
        }
        Err(e) => {
            log::warn!("[Evolution] Failed to write SOUL.md: {}", e);
            (false, current_hash)
        }
    }
}

/// 简易字符串 hash (用于 SOUL 防冲突检测)
fn simple_hash(s: &str) -> String {
    // 使用 FNV-1a 32-bit hash 的简化实现
    let mut hash: u32 = 2166136261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    format!("{:08x}", hash)
}

// ═══════════════════════════════════════════════════════════
// IPC 接口: 前端看板数据源
// ═══════════════════════════════════════════════════════════

/// 返回进化引擎的统计数据，供前端看板展示
#[tauri::command]
pub fn system_get_evolution_stats() -> Value {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return json!({ "error": "数据库打开失败" }),
    };

    // ── 观测统计 ──────────────────────────────────────────
    let obs_stats = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(tool_calls_count), 0), COALESCE(SUM(tool_failures), 0),
                COALESCE(SUM(tokens_in), 0), COALESCE(SUM(tokens_out), 0)
         FROM session_observations",
        [],
        |row| Ok(json!({
            "total_conversations": row.get::<_, i64>(0).unwrap_or(0),
            "total_tool_calls": row.get::<_, i64>(1).unwrap_or(0),
            "total_tool_failures": row.get::<_, i64>(2).unwrap_or(0),
            "total_tokens_in": row.get::<_, i64>(3).unwrap_or(0),
            "total_tokens_out": row.get::<_, i64>(4).unwrap_or(0),
        }))
    ).unwrap_or(json!({
        "total_conversations": 0,
        "total_tool_calls": 0,
        "total_tool_failures": 0,
        "total_tokens_in": 0,
        "total_tokens_out": 0,
    }));

    // ── 做梦历史 (最近 10 条) ──────────────────────────────
    let mut dream_history = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT dream_type, facts_extracted, stale_cleaned, memories_merged,
                soul_refined, report_text, created_at
         FROM evolution_log ORDER BY created_at DESC LIMIT 10"
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(json!({
                "dream_type": row.get::<_, String>(0).unwrap_or_default(),
                "facts_extracted": row.get::<_, i64>(1).unwrap_or(0),
                "stale_cleaned": row.get::<_, i64>(2).unwrap_or(0),
                "memories_merged": row.get::<_, i64>(3).unwrap_or(0),
                "soul_refined": row.get::<_, i64>(4).unwrap_or(0) != 0,
                "report": row.get::<_, String>(5).unwrap_or_default(),
                "created_at": row.get::<_, i64>(6).unwrap_or(0),
            }))
        }) {
            dream_history = rows.filter_map(|r| r.ok()).collect();
        }
    }

    // ── 知识库统计 ────────────────────────────────────────
    let learned_dir = get_learned_dir();
    let learned_count = if learned_dir.exists() {
        std::fs::read_dir(&learned_dir)
            .map(|rd| rd.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0)
    } else {
        0
    };

    // ── 最近一次做梦时间 ──────────────────────────────────
    let last_dream_at = get_last_dream_timestamp();

    json!({
        "observations": obs_stats,
        "dream_history": dream_history,
        "learned_facts_count": learned_count,
        "last_dream_at": last_dream_at,
    })
}
