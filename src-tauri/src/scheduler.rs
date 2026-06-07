use rusqlite::params;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

/// T-1211: Cron 调度引擎
///
/// 架构:
///   - cron_jobs 表存储用户定义的定时任务 (cron 表达式 + prompt 模板)
///   - 后台 Scheduler 每分钟轮询一次，匹配到的任务交给 LLM 执行
///   - 执行结果写入 calendar events 表 (type='cron_result') 并记录审计日志
///
/// cron 表达式格式: 分 时 日 月 周 (标准 5 字段)
///   支持: * (任意), N (具体值), N-M (范围), */N (步长), N,M,... (列表)

// ═══════════════════════════════════════════════════════════
// DB 初始化
// ═══════════════════════════════════════════════════════════

/// 初始化 cron_jobs 表（在 init_db 中调用）
pub fn init_cron_table(conn: &rusqlite::Connection) {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS cron_jobs (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            cron_expr TEXT NOT NULL,
            prompt_template TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            last_run INTEGER DEFAULT 0,
            created_at INTEGER NOT NULL
        );
    ").unwrap_or_default();
}

// ═══════════════════════════════════════════════════════════
// Cron 表达式解析器 (无外部依赖)
// ═══════════════════════════════════════════════════════════

/// 检查当前分钟是否匹配 cron 表达式
/// 格式: "minute hour day_of_month month day_of_week"
/// 支持: * (任意), N (具体值), N-M (范围), */N (步长), N,M,K (列表)
fn matches_cron(expr: &str) -> bool {
    let now = chrono::Local::now();
    let minute = now.format("%M").to_string().parse::<u32>().unwrap_or(0);
    let hour = now.format("%H").to_string().parse::<u32>().unwrap_or(0);
    let day = now.format("%d").to_string().parse::<u32>().unwrap_or(1);
    let month = now.format("%m").to_string().parse::<u32>().unwrap_or(1);
    // chrono %u: Monday=1 .. Sunday=7; cron: Sunday=0, Monday=1 .. Saturday=6
    let weekday_chrono = now.format("%u").to_string().parse::<u32>().unwrap_or(1);
    let weekday = if weekday_chrono == 7 { 0 } else { weekday_chrono };

    let fields: Vec<&str> = expr.trim().split_whitespace().collect();
    if fields.len() != 5 {
        log::warn!("Scheduler: invalid cron expression (need 5 fields): '{}'", expr);
        return false;
    }

    field_matches(fields[0], minute, 0, 59)
        && field_matches(fields[1], hour, 0, 23)
        && field_matches(fields[2], day, 1, 31)
        && field_matches(fields[3], month, 1, 12)
        && field_matches(fields[4], weekday, 0, 6)
}

/// 检查单个 cron 字段是否匹配给定值
/// 支持: "*", "*/N", "N", "N-M", "N,M,K", "N-M/S", 及以上的组合
fn field_matches(field: &str, value: u32, min: u32, max: u32) -> bool {
    // 逗号分隔 — 任一子表达式匹配即可
    for part in field.split(',') {
        if part_matches(part.trim(), value, min, max) {
            return true;
        }
    }
    false
}

/// 匹配单个子表达式 (不含逗号)
fn part_matches(part: &str, value: u32, min: u32, max: u32) -> bool {
    // 1. 带步长: "*/N" 或 "N-M/S"
    if let Some((range_part, step_str)) = part.split_once('/') {
        let step: u32 = match step_str.parse() {
            Ok(s) if s > 0 => s,
            _ => return false,
        };
        let (range_start, range_end) = if range_part == "*" {
            (min, max)
        } else if let Some((a, b)) = range_part.split_once('-') {
            match (a.parse::<u32>(), b.parse::<u32>()) {
                (Ok(start), Ok(end)) => (start, end),
                _ => return false,
            }
        } else {
            // 单个数字 + 步长 (e.g. "5/10") — 从该数字开始每隔 step
            match range_part.parse::<u32>() {
                Ok(start) => (start, max),
                _ => return false,
            }
        };
        // 检查 value 是否落在 [range_start, range_end] 且满足步长
        if value < range_start || value > range_end {
            return false;
        }
        return (value - range_start) % step == 0;
    }

    // 2. 范围: "N-M"
    if let Some((a, b)) = part.split_once('-') {
        return match (a.parse::<u32>(), b.parse::<u32>()) {
            (Ok(start), Ok(end)) => value >= start && value <= end,
            _ => false,
        };
    }

    // 3. 通配符: "*"
    if part == "*" {
        return true;
    }

    // 4. 具体数值: "N"
    if let Ok(n) = part.parse::<u32>() {
        return value == n;
    }

    false
}

// ═══════════════════════════════════════════════════════════
// 后台调度循环
// ═══════════════════════════════════════════════════════════

/// 启动 Scheduler 后台守护循环
/// 每 60 秒 tick 一次，扫描并执行匹配的 cron 任务
pub async fn start_scheduler(app: AppHandle) {
    // 延迟 15 秒，让其他初始化 (DB/Reconciler/ModelRegistry) 先完成
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
    log::info!("Scheduler: background cron loop started");

    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(60));

    loop {
        ticker.tick().await;

        // 每次 tick 独立打开 DB 连接（避免跨 await 持有锁）
        let db_path = super::get_data_dir().join("bob.db");
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Scheduler: failed to open DB: {}", e);
                continue;
            }
        };

        // 读取所有已启用的 cron 任务
        let jobs = match load_enabled_jobs(&conn) {
            Ok(j) => j,
            Err(e) => {
                log::warn!("Scheduler: failed to load jobs: {}", e);
                continue;
            }
        };

        for (id, title, cron_expr, prompt) in &jobs {
            if matches_cron(cron_expr) {
                log::info!("Scheduler: cron matched for job '{}' ({})", title, id);
                execute_cron_job(&app, id, title, prompt).await;

                // 更新 last_run 时间戳
                let now = super::now_ms();
                let _ = conn.execute(
                    "UPDATE cron_jobs SET last_run = ?1 WHERE id = ?2",
                    params![now, id],
                );
            }
        }

        // T-1307: 每轮 tick 也检查即将到期的待办
        check_upcoming_todos(&app, &db_path);
    }
}

/// T-1307: 检查即将到期的待办/事件，发射提醒通知
fn check_upcoming_todos(app: &AppHandle, db_path: &std::path::Path) {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let now_ms = super::now_ms() as i64;
    // 6 小时内未重复提醒 (21600000 ms)
    let cooldown: i64 = 6 * 3600 * 1000;

    let mut stmt = match conn.prepare(
        "SELECT id, title, type, date, start_time
         FROM events
         WHERE status = 'pending'
           AND date <= ?1
           AND (last_notified IS NULL OR last_notified < ?2)
         ORDER BY date ASC, start_time ASC
         LIMIT 5"
    ) {
        Ok(s) => s,
        Err(_) => return,
    };

    let rows: Vec<(String, String, String, Option<String>, Option<String>)> = match stmt.query_map(
        params![today, now_ms - cooldown],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    ) {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(_) => return,
    };

    if rows.is_empty() { return; }

    for (id, title, etype, date, start_time) in &rows {
        let _ = app.emit("todo:reminder", json!({
            "id": id,
            "title": title,
            "type": etype,
            "date": date,
            "start_time": start_time,
        }));

        // T-1307: 调用 Windows 原生弹窗
        let _ = app.notification()
            .builder()
            .title("Bob 提醒：今日待办")
            .body(title)
            .show();

        // 更新 last_notified 时间戳
        let _ = conn.execute(
            "UPDATE events SET last_notified = ?1 WHERE id = ?2",
            params![now_ms, id],
        );
    }

    log::info!("Scheduler T-1307: sent {} todo reminders", rows.len());
}

/// 从 DB 加载所有 enabled=1 的 cron 任务
fn load_enabled_jobs(conn: &rusqlite::Connection) -> Result<Vec<(String, String, String, String)>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, title, cron_expr, prompt_template FROM cron_jobs WHERE enabled = 1"
    ).map_err(|e| format!("{}", e))?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    }).map_err(|e| format!("{}", e))?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

// ═══════════════════════════════════════════════════════════
// Cron 任务执行器
// ═══════════════════════════════════════════════════════════

/// 执行单个 cron 任务
/// 1. 为该 cron job 创建/查找专属对话
/// 2. 插入 user 消息 (prompt)
/// 3. 调用 LLM stream_chat 获取响应
/// 4. 保存 assistant 回复到 DB
/// 5. 写入 calendar events (type=cron_result)
/// 6. 发射前端事件 + 写入审计日志
async fn execute_cron_job(app: &AppHandle, job_id: &str, title: &str, prompt: &str) {
    let db_path = super::get_data_dir().join("bob.db");
    let now = super::now_ms();

    // ── Step 1: 获取或创建专属对话 ──
    let conv_id = match get_or_create_cron_conversation(&db_path, job_id, title) {
        Ok(id) => id,
        Err(e) => {
            log::error!("Scheduler: failed to get/create conversation for job '{}': {}", job_id, e);
            return;
        }
    };

    // ── Step 2: 插入 user 消息 ──
    {
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Scheduler: DB open failed for message insert: {}", e);
                return;
            }
        };
        let _ = conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at, from_channel)
             VALUES (?1, 'user', ?2, ?3, 'cron')",
            params![conv_id, prompt, now],
        );
        // 更新对话元数据
        let preview: String = prompt.chars().take(20).collect();
        let _ = conn.execute(
            "UPDATE conversations SET last_message = ?1, last_role = 'user', updated_at = ?2 WHERE id = ?3",
            params![preview, now, conv_id],
        );
    }

    // ── Step 3: 构建消息列表，调用 LLM ──
    let messages = vec![
        json!({ "role": "user", "content": prompt }),
    ];
    let result = crate::llm::stream_chat(
        app.clone(),
        messages,
        Some(conv_id.clone()),
        None,
    ).await;

    // ── Step 4: 提取 assistant 回复并保存 ──
    let response_text = result.get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("[无响应]")
        .to_string();

    {
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Scheduler: DB open failed for response save: {}", e);
                return;
            }
        };
        let save_ts = super::now_ms();
        let _ = conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at, from_channel)
             VALUES (?1, 'assistant', ?2, ?3, 'cron')",
            params![conv_id, response_text, save_ts],
        );
        let preview: String = response_text.chars().take(20).collect();
        let _ = conn.execute(
            "UPDATE conversations SET last_message = ?1, last_role = 'assistant', updated_at = ?2 WHERE id = ?3",
            params![preview, save_ts, conv_id],
        );
    }

    // ── Step 5: 写入 calendar events 表 (type=cron_result) ──
    {
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(_) => {
                log::warn!("Scheduler: skipping calendar event write (DB open failed)");
                // non-fatal, continue
                rusqlite::Connection::open(&db_path).unwrap() // fallback
            }
        };
        let event_id = format!("cron-{}", super::now_ms());
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let time_now = chrono::Local::now().format("%H:%M").to_string();
        let desc_preview: String = response_text.chars().take(200).collect();
        let _ = conn.execute(
            "INSERT INTO events (id, title, type, status, date, start_time, description, created_at)
             VALUES (?1, ?2, 'cron_result', 'done', ?3, ?4, ?5, ?6)",
            params![
                event_id,
                format!("[Cron] {}", title),
                today,
                time_now,
                desc_preview,
                super::now_ms(),
            ],
        );
    }

    // ── Step 6: 发射前端事件 ──
    let _ = app.emit("scheduler:completed", json!({
        "job_id": job_id,
        "title": title,
        "conversation_id": conv_id,
        "response_preview": response_text.chars().take(100).collect::<String>(),
        "timestamp": super::now_ms(),
    }));

    // ── Step 7: 审计日志 ──
    write_scheduler_audit(&format!(
        "EXECUTED job '{}' ({}) → conv={}, response_len={}",
        title, job_id, conv_id, response_text.len()
    ));

    log::info!("Scheduler: job '{}' executed successfully (response {} chars)", title, response_text.len());
}

/// 获取或创建 cron 任务的专属对话
/// 对话 ID 格式: "cron-{job_id}" (确定性映射，每个 job 固定一个对话)
fn get_or_create_cron_conversation(
    db_path: &std::path::Path,
    job_id: &str,
    title: &str,
) -> Result<String, String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("DB open failed: {}", e))?;

    let conv_id = format!("cron-{}", job_id);

    // 检查是否已存在
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM conversations WHERE id = ?1",
        params![conv_id],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if !exists {
        let now = super::now_ms();
        conn.execute(
            "INSERT INTO conversations (id, title, model, created_at, updated_at)
             VALUES (?1, ?2, '', ?3, ?4)",
            params![conv_id, format!("[Cron] {}", title), now, now],
        ).map_err(|e| format!("Create conversation failed: {}", e))?;
    }

    Ok(conv_id)
}

// ═══════════════════════════════════════════════════════════
// IPC Commands
// ═══════════════════════════════════════════════════════════

/// 列出所有 cron 任务
#[tauri::command]
pub async fn system_list_cron_jobs() -> Value {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("DB 打开失败: {}", e) }),
    };

    let mut stmt = match conn.prepare(
        "SELECT id, title, cron_expr, prompt_template, enabled, last_run, created_at
         FROM cron_jobs ORDER BY created_at DESC"
    ) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("查询失败: {}", e) }),
    };

    let rows = match stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "cron_expr": row.get::<_, String>(2)?,
            "prompt_template": row.get::<_, String>(3)?,
            "enabled": row.get::<_, i64>(4)? == 1,
            "last_run": row.get::<_, i64>(5).unwrap_or(0),
            "created_at": row.get::<_, i64>(6)?,
        }))
    }) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("查询失败: {}", e) }),
    };

    let jobs: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
    json!({ "ok": true, "jobs": jobs, "count": jobs.len() })
}

/// 添加新 cron 任务
#[tauri::command]
pub async fn system_add_cron_job(title: String, cron_expr: String, prompt: String) -> Value {
    // 基础校验
    if cron_expr.trim().split_whitespace().count() != 5 {
        return json!({ "ok": false, "error": "cron 表达式必须包含 5 个字段 (分 时 日 月 周)" });
    }
    if prompt.trim().is_empty() {
        return json!({ "ok": false, "error": "prompt 不能为空" });
    }

    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "error": format!("DB 打开失败: {}", e) }),
    };

    let id = format!("cj-{}", super::now_ms());
    let now = super::now_ms();

    match conn.execute(
        "INSERT INTO cron_jobs (id, title, cron_expr, prompt_template, enabled, last_run, created_at)
         VALUES (?1, ?2, ?3, ?4, 1, 0, ?5)",
        params![id, title, cron_expr, prompt, now],
    ) {
        Ok(_) => {
            write_scheduler_audit(&format!("ADDED job '{}' ({}) cron={}", title, id, cron_expr));
            json!({ "ok": true, "id": id })
        }
        Err(e) => json!({ "ok": false, "error": format!("插入失败: {}", e) }),
    }
}

/// 删除 cron 任务
#[tauri::command]
pub async fn system_remove_cron_job(id: String) -> Value {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "error": format!("DB 打开失败: {}", e) }),
    };

    match conn.execute("DELETE FROM cron_jobs WHERE id = ?1", params![id]) {
        Ok(changed) => {
            if changed > 0 {
                write_scheduler_audit(&format!("REMOVED job {}", id));
                json!({ "ok": true })
            } else {
                json!({ "ok": false, "error": format!("任务 '{}' 不存在", id) })
            }
        }
        Err(e) => json!({ "ok": false, "error": format!("删除失败: {}", e) }),
    }
}

/// 启用/禁用 cron 任务
#[tauri::command]
pub async fn system_toggle_cron_job(id: String, enabled: bool) -> Value {
    let db_path = super::get_data_dir().join("bob.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "error": format!("DB 打开失败: {}", e) }),
    };

    let flag: i64 = if enabled { 1 } else { 0 };
    match conn.execute(
        "UPDATE cron_jobs SET enabled = ?1 WHERE id = ?2",
        params![flag, id],
    ) {
        Ok(changed) => {
            if changed > 0 {
                write_scheduler_audit(&format!("TOGGLED job {} → enabled={}", id, enabled));
                json!({ "ok": true, "enabled": enabled })
            } else {
                json!({ "ok": false, "error": format!("任务 '{}' 不存在", id) })
            }
        }
        Err(e) => json!({ "ok": false, "error": format!("更新失败: {}", e) }),
    }
}

// ═══════════════════════════════════════════════════════════
// 审计日志
// ═══════════════════════════════════════════════════════════

fn write_scheduler_audit(message: &str) {
    let logs_dir = super::get_data_dir().join("logs");
    let _ = fs::create_dir_all(&logs_dir);
    let log_path = logs_dir.join("scheduler.log");

    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let line = format!("[{}] {}\n", timestamp, message);

    super::write_log_with_rotation(&log_path, &line, 5 * 1024 * 1024);
}

// ═══════════════════════════════════════════════════════════
// 单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_matches_star() {
        assert!(field_matches("*", 0, 0, 59));
        assert!(field_matches("*", 30, 0, 59));
        assert!(field_matches("*", 59, 0, 59));
    }

    #[test]
    fn test_field_matches_specific() {
        assert!(field_matches("0", 0, 0, 59));
        assert!(!field_matches("0", 1, 0, 59));
        assert!(field_matches("15", 15, 0, 59));
    }

    #[test]
    fn test_field_matches_range() {
        assert!(field_matches("1-5", 1, 0, 6));
        assert!(field_matches("1-5", 3, 0, 6));
        assert!(field_matches("1-5", 5, 0, 6));
        assert!(!field_matches("1-5", 0, 0, 6));
        assert!(!field_matches("1-5", 6, 0, 6));
    }

    #[test]
    fn test_field_matches_step() {
        // */5 on minute field: 0, 5, 10, 15, ...
        assert!(field_matches("*/5", 0, 0, 59));
        assert!(field_matches("*/5", 5, 0, 59));
        assert!(field_matches("*/5", 55, 0, 59));
        assert!(!field_matches("*/5", 3, 0, 59));
        assert!(!field_matches("*/5", 59, 0, 59));
    }

    #[test]
    fn test_field_matches_range_step() {
        // 1-10/3 → 1, 4, 7, 10
        assert!(field_matches("1-10/3", 1, 0, 59));
        assert!(field_matches("1-10/3", 4, 0, 59));
        assert!(field_matches("1-10/3", 7, 0, 59));
        assert!(field_matches("1-10/3", 10, 0, 59));
        assert!(!field_matches("1-10/3", 2, 0, 59));
        assert!(!field_matches("1-10/3", 11, 0, 59));
    }

    #[test]
    fn test_field_matches_comma() {
        assert!(field_matches("0,15,30,45", 0, 0, 59));
        assert!(field_matches("0,15,30,45", 15, 0, 59));
        assert!(field_matches("0,15,30,45", 45, 0, 59));
        assert!(!field_matches("0,15,30,45", 10, 0, 59));
    }

    #[test]
    fn test_field_matches_mixed_comma_range() {
        // "1-3,7,10-12"
        assert!(field_matches("1-3,7,10-12", 1, 0, 31));
        assert!(field_matches("1-3,7,10-12", 2, 0, 31));
        assert!(field_matches("1-3,7,10-12", 3, 0, 31));
        assert!(field_matches("1-3,7,10-12", 7, 0, 31));
        assert!(field_matches("1-3,7,10-12", 11, 0, 31));
        assert!(!field_matches("1-3,7,10-12", 5, 0, 31));
        assert!(!field_matches("1-3,7,10-12", 13, 0, 31));
    }
}
