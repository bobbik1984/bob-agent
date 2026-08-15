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
static LAST_EXTRACTION: std::sync::OnceLock<Mutex<HashMap<String, std::time::Instant>>> =
    std::sync::OnceLock::new();

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

fn latest_user_message(messages: &[Value]) -> &str {
    messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .and_then(|m| m.get("content").and_then(|c| c.as_str()))
        .unwrap_or("")
}

fn user_blocks_persistent_learning(message: &str) -> bool {
    let normalized = message.to_lowercase();
    [
        "不要记忆",
        "不要记住",
        "别记住",
        "不要保存",
        "不要写入",
        "不要修改",
        "只读",
        "do not remember",
        "don't remember",
        "do not save",
        "don't save",
        "read-only",
        "read only",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn user_requests_persistent_learning(message: &str) -> bool {
    let normalized = message.to_lowercase();
    [
        "请记住",
        "帮我记住",
        "记到长期记忆",
        "保存到记忆",
        "remember this",
        "save this to memory",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn user_provides_durable_memory(message: &str) -> bool {
    let normalized = message.to_lowercase();
    [
        "我喜欢",
        "我更喜欢",
        "我的偏好",
        "以后都",
        "以后不要",
        "一直用",
        "你记错了",
        "纠正一下",
        "项目决定",
        "架构原则",
        "i prefer",
        "my preference",
        "from now on",
        "you remembered incorrectly",
        "project decision",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

/// 判断一次对话是否值得触发知识提取。
///
/// 记忆默认慢生长：尊重用户的只读/不记忆要求；失败探索不沉淀为事实；
/// 单次工具调用本身也不再等价于“值得长期记住”。
fn should_extract(messages: &[Value], tool_failures: i64) -> bool {
    let latest_user = latest_user_message(messages);
    if user_blocks_persistent_learning(latest_user) {
        log::info!("[Evolution] Skipping extraction (user requested no persistent changes)");
        return false;
    }

    let explicit_memory_request = user_requests_persistent_learning(latest_user);
    if tool_failures > 0 && !explicit_memory_request {
        log::info!(
            "[Evolution] Skipping extraction (tool failures: {})",
            tool_failures
        );
        return false;
    }

    if explicit_memory_request {
        log::info!("[Evolution] Triggered via explicit user memory request");
        return true;
    }

    // ── Layer 1: 快车道 — 检测助手回复中是否有隐式标记 ──
    for msg in messages {
        if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                if content.contains("<|mem|>") {
                    if user_provides_durable_memory(latest_user) {
                        log::info!("[Evolution] Triggered via durable user statement");
                        return true;
                    }
                    log::info!("[Evolution] Ignoring marker without durable user evidence");
                }
            }
        }
    }

    // ── Layer 2: 深入对话安全网 ─────────────────────────
    false
}

#[cfg(test)]
mod extraction_policy_tests {
    use super::*;

    fn message(role: &str, content: &str) -> Value {
        json!({ "role": role, "content": content })
    }

    #[test]
    fn explicit_no_change_request_blocks_learning() {
        let messages = vec![
            message("user", "请检查环境，不要猜测，也不要修改任何内容。"),
            message("assistant", "发现了一个可执行文件。<|mem|>"),
        ];
        assert!(!should_extract(&messages, 0));
    }

    #[test]
    fn failed_tool_exploration_does_not_become_memory() {
        let messages = vec![
            message("user", "检查一下当前环境"),
            message("assistant", "我认为已经确认。<|mem|>"),
        ];
        assert!(!should_extract(&messages, 1));
    }

    #[test]
    fn tool_use_alone_is_not_a_learning_signal() {
        let messages = vec![message("user", "检查一下当前环境")];
        assert!(!should_extract(&messages, 0));
    }

    #[test]
    fn explicit_user_memory_request_remains_supported() {
        let messages = vec![message("user", "请记住我更喜欢简短的回答")];
        assert!(should_extract(&messages, 1));
    }

    #[test]
    fn long_conversation_without_durable_user_evidence_does_not_learn() {
        let messages = vec![
            message("user", "先看看现状"),
            message("assistant", "好的"),
            message("user", "再检查一下"),
            message("assistant", "继续"),
            message("user", "给我结论"),
            message("assistant", "这是结论 <|mem|>"),
        ];
        assert!(!should_extract(&messages, 0));
    }

    #[test]
    fn durable_preference_with_marker_can_learn() {
        let messages = vec![
            message("user", "我更喜欢简短直接的回复"),
            message("assistant", "明白了 <|mem|>"),
        ];
        assert!(should_extract(&messages, 0));
    }

    #[test]
    fn repeated_failures_become_candidates_but_single_failures_do_not() {
        let errors = vec![
            json!({"id":1,"tool_name":"read_file","error_type":"permission_denied"}),
            json!({"id":2,"tool_name":"read_file","error_type":"permission_denied"}),
            json!({"id":3,"tool_name":"web_search","error_type":"transient_network"}),
        ];
        let candidates = build_diagnostic_candidates(&errors);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].claim.contains("read_file"));
        assert!(candidates[0].claim.contains("需要复核根因"));
        assert!(!candidates[0].claim.contains("web_search"));
    }
}

/// 从对话尾部提取持久性事实，写入 wiki/learned/
/// 使用 clerkModel (最便宜的模型) 执行提取
pub async fn extract_learned_facts(
    _app: AppHandle,
    messages: Vec<Value>,
    conv_id: String,
    tool_failures: i64,
) {
    // ── Step 1: 只读检查冷却 (不占位) ────────────────────
    let now_instant = std::time::Instant::now();
    let cache = LAST_EXTRACTION.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let map = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&last_time) = map.get(&conv_id) {
            if now_instant.duration_since(last_time).as_secs() < 30 {
                log::info!(
                    "[Evolution] Skipping extraction for conv={} (cooldown active)",
                    conv_id
                );
                return;
            }
        }
    } // lock 释放

    // ── Step 2: 三层漏斗判断 ────────────────────────────
    if !should_extract(&messages, tool_failures) {
        log::info!(
            "[Evolution] Skipping extraction for conv={} (trivial chat)",
            conv_id
        );
        return; // 不写入冷却，不占位
    }

    // ── Step 3: 通过了！写入冷却时间戳 ──────────────────
    {
        let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(conv_id.clone(), now_instant);
    } // lock 释放

    // 4. 读取 clerkModel 配置
    let config = super::read_config();
    let clerk_model = config
        .get("clerkModel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if clerk_model.is_empty() {
        log::info!("[Evolution] No clerkModel configured, skipping extraction");
        return;
    }

    // 3. 取最后 10 条消息 (截断长消息)
    let recent: Vec<String> = messages
        .iter()
        .rev()
        .take(10)
        .rev()
        .filter_map(|m| {
            let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
            let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if content.is_empty() {
                return None;
            }
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
- 用户明确要求不记忆、不保存、只读或不修改时，必须返回 `[]`
- 环境与项目状态只接受用户明确陈述，或工具结果直接证明的事实
- 不得从目录名、常见默认值、路径不存在、失败的工具调用或助手自己的推测中推导事实
- “未找到”不等于“不存在”；无法核验时返回 `[]`

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
    let (provider, api_key, model_id, base_url) =
        super::llm::read_llm_config_for_model(&clerk_model);

    if api_key.is_empty() && provider != "offline" {
        log::info!(
            "[Evolution] Clerk model {} has no API key, skipping extraction",
            clerk_model
        );
        return;
    }

    // 处理 GCP Token
    let final_api_key = if api_key == "__GCP_TOKEN__" {
        let cred_path = super::gcp_auth::get_gcp_credential_path();
        match super::gcp_auth::GcpTokenManager::from_file(&cred_path) {
            Ok(mgr) => match mgr.get_access_token().await {
                Ok(token) => token,
                Err(_) => {
                    log::warn!("[Evolution] GCP token failed for clerk");
                    return;
                }
            },
            Err(_) => {
                return;
            }
        }
    } else {
        api_key
    };

    let url = format!("{}/chat/completions", base_url);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
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

    let resp = match client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", final_api_key))
        .json(&body)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            log::info!(
                "[Evolution] Clerk API returned {}, skipping extraction",
                r.status()
            );
            return;
        }
        Err(e) => {
            log::info!(
                "[Evolution] Clerk API request failed: {}, skipping extraction",
                e
            );
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
        let fact_type = fact
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("reference");
        let title = fact
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let content = fact.get("content").and_then(|v| v.as_str()).unwrap_or("");

        if title.is_empty() || content.is_empty() {
            continue;
        }

        // 生成文件名: {type}_{slug}_{timestamp}.md
        let slug: String = title
            .chars()
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
                sync_fact_to_memory_index(fact_type, title, content, &file_path, &conv_id);
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
                let fact_type = fact
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("reference");
                let title = fact.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let content = fact.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let keywords = format!("{} {}", fact_type, title);

                let slug: String = title
                    .chars()
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
            saved_count,
            conv_id
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
    conn.query_row("SELECT MAX(created_at) FROM evolution_log", [], |row| {
        row.get::<_, i64>(0)
    })
    .unwrap_or(0)
}

/// 检查是否需要做梦，如果需要则执行 (被 scheduler.rs 的 tick 调用)
pub async fn check_and_dream(app: AppHandle) {
    let now = super::now_ms();
    let last = get_last_dream_timestamp();

    if last > 0 && (now - last) < DREAM_INTERVAL_MS {
        return; // 距离上次做梦不到 24 小时，跳过
    }

    log::info!(
        "[Evolution] Dream triggered: last_dream={}, gap={}h",
        last,
        if last > 0 {
            (now - last) / 3_600_000
        } else {
            999
        }
    );

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

    log::info!(
        "[Evolution] Dream complete: stale={}, merged={}, soul_refined={}, failure_insights={}",
        report.stale_cleaned,
        report.memories_merged,
        report.soul_refined,
        report.failure_insights
    );
}

struct DreamReport {
    facts_extracted: i64,
    stale_cleaned: i64,
    memories_merged: i64,
    soul_refined: bool,
    failure_insights: i64,
    notebook_notes_digested: i64,
    summary: String,
    soul_hash: String,
}

/// 夜间整理流水线。
///
/// SOUL 只允许用户显式维护；Dream 只能整理事实、生成待审阅诊断候选，
/// 绝不能自动改写人格、权限或执行策略。
async fn run_dream_pipeline(_app: &AppHandle) -> DreamReport {
    let mut report = DreamReport {
        facts_extracted: 0,
        stale_cleaned: 0,
        memories_merged: 0,
        soul_refined: false,
        failure_insights: 0,
        notebook_notes_digested: 0,
        summary: String::new(),
        soul_hash: String::new(),
    };

    // ── Phase 1: 过时淘汰 ──────────────────────────────────
    report.stale_cleaned = phase_stale_cleanup();

    // ── Phase 2: 相似合并 ──────────────────────────────────
    report.memories_merged = phase_merge_similar();

    // SOUL 是稳定身份边界。仅记录当前 hash 用于审计，不修改文件。
    report.soul_hash = current_soul_hash();

    // ── Phase 3: 失败模式候选（只进入 diagnostic candidate）
    report.failure_insights = phase_failure_analysis().await;

    // ── Phase 4: 笔记语义消化 ──────────────────────────────
    report.notebook_notes_digested = phase_notebook_digest(_app).await;

    // 构建摘要
    let mut summary_parts = Vec::new();
    if report.stale_cleaned > 0 {
        summary_parts.push(format!("清理 {} 条过时记忆", report.stale_cleaned));
    }
    if report.memories_merged > 0 {
        summary_parts.push(format!("合并 {} 条相似记忆", report.memories_merged));
    }
    if report.failure_insights > 0 {
        summary_parts.push(format!("生成 {} 条待审阅诊断候选", report.failure_insights));
    }
    if report.notebook_notes_digested > 0 {
        summary_parts.push(format!(
            "语义消化了 {} 篇笔记",
            report.notebook_notes_digested
        ));
    }
    report.summary = if summary_parts.is_empty() {
        "无需更新".to_string()
    } else {
        summary_parts.join("; ")
    };

    report
}

#[derive(Clone, Debug, PartialEq)]
struct DiagnosticCandidate {
    id: String,
    claim: String,
    evidence: String,
}

fn build_diagnostic_candidates(errors: &[Value]) -> Vec<DiagnosticCandidate> {
    let mut groups: HashMap<(String, String), Vec<i64>> = HashMap::new();
    for error in errors {
        let tool = error
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let error_type = error
            .get("error_type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let error_id = error.get("id").and_then(Value::as_i64).unwrap_or_default();
        groups.entry((tool, error_type)).or_default().push(error_id);
    }

    let mut candidates = groups
        .into_iter()
        .filter(|(_, ids)| ids.len() >= 2)
        .map(|((tool, error_type), ids)| {
            let signature = format!("{tool}/{error_type}");
            DiagnosticCandidate {
                id: format!("diagnostic-{}", simple_hash(&signature)),
                claim: format!(
                    "工具 {tool} 在最近诊断窗口重复出现 {error_type}（{} 次），需要复核根因后才能提升为程序性策略。",
                    ids.len()
                ),
                evidence: json!({
                    "tool": tool,
                    "errorType": error_type,
                    "errorIds": ids,
                })
                .to_string(),
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    candidates
}

/// 扫描 execution_errors，只生成待审阅的诊断候选。
/// 单次错误继续保留在诊断表，不形成长期规则；任何错误都不得写入 SOUL。
async fn phase_failure_analysis() -> i64 {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    // 读取未分析的错误记录 (48h 窗口)
    let errors = crate::db::get_unanalyzed_errors(&conn, 48);
    if errors.is_empty() {
        return 0;
    }

    let ids_to_mark = errors
        .iter()
        .filter_map(|error| error.get("id").and_then(Value::as_i64))
        .collect::<Vec<_>>();
    let candidates = build_diagnostic_candidates(&errors);
    let now = super::now_ms();
    for candidate in &candidates {
        if let Err(error) = conn.execute(
            "INSERT INTO memory_entries
             (id, claim, memory_type, scope, source, confidence, evidence,
              first_seen, last_confirmed, status, version, created_at, updated_at)
             VALUES (?1, ?2, 'diagnostic', 'global', 'dream_diagnostic', 0.5, ?3,
                     ?4, ?4, 'candidate', 1, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
               claim=excluded.claim,
               evidence=excluded.evidence,
               last_confirmed=excluded.last_confirmed,
               status='candidate',
               version=memory_entries.version + 1,
               updated_at=excluded.updated_at",
            rusqlite::params![candidate.id, candidate.claim, candidate.evidence, now],
        ) {
            log::warn!("[Evolution] Failed to save diagnostic candidate: {}", error);
        }
    }
    if let Err(error) = crate::db::mark_errors_analyzed(&conn, &ids_to_mark) {
        log::warn!(
            "[Evolution] Failed to mark diagnostic errors analyzed: {}",
            error
        );
    }
    candidates.len() as i64
}

/// Phase 1: 过时淘汰 — 清理 30 天未更新且未被引用的 learned 记忆
fn phase_stale_cleanup() -> i64 {
    let learned_dir = get_learned_dir();
    if !learned_dir.exists() {
        return 0;
    }

    let thirty_days_ago = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(30 * 24 * 3600))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let mut cleaned = 0i64;

    let entries: Vec<PathBuf> = match std::fs::read_dir(&learned_dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect(),
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
        log::info!(
            "[Evolution] Dream Phase 1: cleaned/marked {} stale memories",
            cleaned
        );
    }
    cleaned
}

/// Phase 2: 相似合并 — 基于标题文本重叠率去重
fn phase_merge_similar() -> i64 {
    let learned_dir = get_learned_dir();
    if !learned_dir.exists() {
        return 0;
    }

    let entries: Vec<PathBuf> = match std::fs::read_dir(&learned_dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "md"))
            .collect(),
        Err(_) => return 0,
    };

    // 提取所有标题
    let mut titles: Vec<(PathBuf, String, std::time::SystemTime)> = Vec::new();
    for path in &entries {
        if let Ok(content) = std::fs::read_to_string(path) {
            // 跳过已标记为过时的
            if content.contains("superseded: true") {
                continue;
            }

            let title = content
                .lines()
                .find(|l| l.starts_with("title:"))
                .map(|l| {
                    l.trim_start_matches("title:")
                        .trim()
                        .trim_matches('"')
                        .to_string()
                })
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
        if removed_paths.contains(&titles[i].0) {
            continue;
        }

        for j in (i + 1)..titles.len() {
            if removed_paths.contains(&titles[j].0) {
                continue;
            }

            let similarity = title_similarity(&titles[i].1, &titles[j].1);
            if similarity > 0.7 {
                // 保留较新的，标记较旧的为过时
                let older = if titles[i].2 < titles[j].2 {
                    &titles[i].0
                } else {
                    &titles[j].0
                };
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
        log::info!(
            "[Evolution] Dream Phase 2: merged {} similar memories",
            merged
        );
    }
    merged
}

/// 简易标题相似度 (Jaccard 字符 N-gram)
fn title_similarity(a: &str, b: &str) -> f64 {
    let a_chars: std::collections::HashSet<char> = a.chars().collect();
    let b_chars: std::collections::HashSet<char> = b.chars().collect();
    if a_chars.is_empty() && b_chars.is_empty() {
        return 1.0;
    }
    let intersection = a_chars.intersection(&b_chars).count();
    let union = a_chars.union(&b_chars).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

fn current_soul_hash() -> String {
    let memory_dir = super::get_data_dir().join("memory");
    let soul_path = memory_dir.join("SOUL.md");
    let current_soul = std::fs::read_to_string(soul_path).unwrap_or_default();
    simple_hash(&current_soul)
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
    let obs_stats = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(tool_calls_count), 0), COALESCE(SUM(tool_failures), 0),
                COALESCE(SUM(tokens_in), 0), COALESCE(SUM(tokens_out), 0)
         FROM session_observations",
            [],
            |row| {
                Ok(json!({
                    "total_conversations": row.get::<_, i64>(0).unwrap_or(0),
                    "total_tool_calls": row.get::<_, i64>(1).unwrap_or(0),
                    "total_tool_failures": row.get::<_, i64>(2).unwrap_or(0),
                    "total_tokens_in": row.get::<_, i64>(3).unwrap_or(0),
                    "total_tokens_out": row.get::<_, i64>(4).unwrap_or(0),
                }))
            },
        )
        .unwrap_or(json!({
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
         FROM evolution_log ORDER BY created_at DESC LIMIT 10",
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

// ═══════════════════════════════════════════════════════════
// Phase 5 (目标 19 Phase 3): 笔记语义消化 (Notebook Digest)
// ═══════════════════════════════════════════════════════════
async fn phase_notebook_digest(_app: &AppHandle) -> i64 {
    let last_dream = get_last_dream_timestamp();
    let notes_dir = super::get_data_dir().join("notes");

    // 我们扫描 notes/daily 和 notes/topics
    let mut files_to_digest = Vec::new();
    let dirs = vec![notes_dir.join("daily"), notes_dir.join("topics")];
    for d in dirs {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    // 如果文件是以 .md 结尾且修改时间大于上次做梦时间
                    if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                        if let Ok(mtime) = meta.modified() {
                            let mtime_ms = mtime
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as i64;
                            if mtime_ms > last_dream {
                                files_to_digest.push(entry.path());
                            }
                        }
                    }
                }
            }
        }
    }

    if files_to_digest.is_empty() {
        return 0;
    }

    log::info!(
        "[Evolution] Phase 5: {} notes to digest",
        files_to_digest.len()
    );
    let mut digested_count = 0;

    let prompt = r#"你是一个高智能知识分拣引擎 (Dream Engine)。
你将阅读用户在今天写下的笔记 (可能包含碎碎念、灵感、或者是具体的待办事项)。
你需要将这篇笔记的内容，进行意图识别和结构化提取。
请严格按照以下 JSON 格式输出，不要输出任何其他解释性文字：

{
  "intents": [
    {
      "type": "action",
      "title": "具体的待办事项标题"
    },
    {
      "type": "seed",
      "title": "规划某个新项目或新思考主题的文件名",
      "tags": ["seed", "具体的领域标签"]
    },
    {
      "type": "knowledge",
      "target_entity": "关联的现有主题或项目名称",
      "entity_type": "concept|project|person|topic|technology",
      "summary": "一句话精华总结"
    }
  ]
}

- 如果是具体待办 (Action)，比如“打电话给老王”，提取为 action。
- 如果是极具扩展性的新灵感 (Seed)，比如“构思一款盖楼游戏”，提取为 seed。
- 如果是纯粹的知识点或随想 (Knowledge)，比如“发现RAG在长文本下容易丢失焦点”，提取为 knowledge，并归类到相关实体下。"#;

    let db_path = super::get_data_dir().join("bob.db");

    for path in files_to_digest {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled");
        let note_text = format!("笔记标题: {}\n内容: {}", title, content);

        if let Some(resp) = crate::llm::call_clerk_oneshot(prompt, &note_text, 1024).await {
            // 解析 JSON
            let json_start = resp.find('{').unwrap_or(0);
            let json_end = resp.rfind('}').unwrap_or(resp.len() - 1) + 1;
            let json_str = &resp[json_start..json_end];

            if let Ok(parsed) = serde_json::from_str::<Value>(json_str) {
                if let Some(intents) = parsed.get("intents").and_then(|v| v.as_array()) {
                    let conn = rusqlite::Connection::open(&db_path).unwrap();
                    for intent in intents {
                        let itype = intent.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match itype {
                            "action" => {
                                if let Some(title) = intent.get("title").and_then(|v| v.as_str()) {
                                    // 添加到 events 表中
                                    let _ = conn.execute(
                                        "INSERT INTO events (title, event_type, status, created_at, date) VALUES (?1, 'todo', 'pending', ?2, ?3)",
                                        rusqlite::params![title, super::now_ms(), chrono::Local::now().format("%Y-%m-%d").to_string()]
                                    );
                                }
                            }
                            "seed" => {
                                if let Some(title) = intent.get("title").and_then(|v| v.as_str()) {
                                    // 在 topics 目录下新建文件
                                    let new_path = notes_dir.join("topics").join(format!(
                                        "{}.md",
                                        title.replace('/', "_").replace('\\', "_")
                                    ));
                                    if !new_path.exists() {
                                        let _ =
                                            std::fs::write(&new_path, format!("# {}\n\n", title));

                                        // 添加到 KG 节点并打上 seed metadata
                                        let tags =
                                            intent.get("tags").unwrap_or(&json!(["seed"])).clone();
                                        let node_id = format!("note_{}", title);
                                        let metadata = json!({"is_seed": true, "tags": tags, "source_note": path.to_string_lossy()});
                                        let _ = conn.execute(
                                            "INSERT OR REPLACE INTO kg_nodes (id, label, node_type, summary, metadata) VALUES (?1, ?2, 'note', ?3, ?4)",
                                            rusqlite::params![node_id, title, "灵感种子", metadata.to_string()]
                                        );
                                    }
                                }
                            }
                            "knowledge" => {
                                if let (Some(target), Some(etype), Some(summary)) = (
                                    intent.get("target_entity").and_then(|v| v.as_str()),
                                    intent.get("entity_type").and_then(|v| v.as_str()),
                                    intent.get("summary").and_then(|v| v.as_str()),
                                ) {
                                    // 确保目标节点存在
                                    let target_id =
                                        crate::kg::resolve_node_id(&conn, target, etype);
                                    let _ = crate::kg::upsert_node(
                                        &conn, &target_id, target, etype, "", "", "",
                                    );

                                    // 创建当前笔记的 note 节点
                                    let note_id = format!("note_{}", title);
                                    let _ = crate::kg::upsert_node(
                                        &conn,
                                        &note_id,
                                        title,
                                        "note",
                                        summary,
                                        &path.to_string_lossy(),
                                        "",
                                    );

                                    // 建立关系
                                    let _ = crate::kg::insert_edge(
                                        &conn, &note_id, &target_id, "mentions", 1.0,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        digested_count += 1;
    }

    digested_count
}

fn sync_fact_to_memory_index(
    fact_type: &str,
    title: &str,
    content: &str,
    file_path: &std::path::Path,
    conv_id: &str,
) {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let (memory_type, scope, confidence) = match fact_type {
        "user" => ("identity", "global".to_string(), 0.8),
        "feedback" => ("correction", "global".to_string(), 1.0),
        "project" => ("fact", format!("project:{}", title), 0.8),
        _ => ("fact", "global".to_string(), 0.8),
    };

    let claim = format!(
        "{}: {}",
        title,
        content.chars().take(200).collect::<String>()
    );
    let now = super::now_ms();
    let evidence = serde_json::json!([{"conv_id": conv_id, "timestamp": now}]).to_string();
    let file_path_str = file_path.to_string_lossy().to_string();

    // 用户反馈属于明确纠错，复用统一的版本替代入口，避免与即时纠错形成多个 active 版本。
    if fact_type == "feedback" {
        if let Err(error) =
            crate::db::store_explicit_correction(&claim, &scope, &evidence, Some(&file_path_str))
        {
            log::warn!("Failed to index explicit feedback correction: {}", error);
        }
        return;
    }

    let id = ulid::Ulid::new().to_string();

    let _ = conn.execute(
        "INSERT OR REPLACE INTO memory_entries 
         (id, claim, memory_type, scope, source, confidence, evidence, file_path, first_seen, last_confirmed, status, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            id,
            claim,
            memory_type,
            scope,
            "inferred",
            confidence,
            evidence,
            file_path_str,
            now,
            now,
            "active",
            1
        ],
    );
}
