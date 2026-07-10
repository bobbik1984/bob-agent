use rusqlite::params;
use serde_json::{json, Value};

/// T-605: 日程管理引擎 — 基于 SQLite 的事件/待办系统
///
/// 数据表: events (id, title, type, status, date, start_time, end_time, description, created_at)
/// 前端消费者: InboxView.vue, WeekTimeline.vue, TodoList.vue, ChatView.vue

/// 初始化 events 表（在 init_db 中调用）
pub fn init_events_table(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            type TEXT NOT NULL DEFAULT 'event',
            status TEXT NOT NULL DEFAULT 'pending',
            date TEXT,
            start_time TEXT,
            end_time TEXT,
            description TEXT DEFAULT '',
            created_at INTEGER NOT NULL
        );
    ",
    )
    .unwrap_or_default();

    // T-1307: 向 last_notified 迁移（数据库兼容）
    conn.execute_batch(
        "
        ALTER TABLE events ADD COLUMN last_notified INTEGER DEFAULT 0;
    ",
    )
    .unwrap_or_default();

    conn.execute_batch(
        "
        ALTER TABLE events ADD COLUMN completed_at INTEGER DEFAULT 0;
    ",
    )
    .unwrap_or_default();
}

/// 列出所有事件和待办
#[tauri::command]
pub fn system_list_events(db: tauri::State<'_, crate::db::DbState>) -> Vec<Value> {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut stmt = match conn.prepare(
        "SELECT id, title, type, status, date, start_time, end_time, description, created_at, completed_at
         FROM events ORDER BY created_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "type": row.get::<_, String>(2)?,
            "status": row.get::<_, String>(3)?,
            "date": row.get::<_, Option<String>>(4).unwrap_or(None),
            "start_time": row.get::<_, Option<String>>(5).unwrap_or(None),
            "end_time": row.get::<_, Option<String>>(6).unwrap_or(None),
            "description": row.get::<_, Option<String>>(7).unwrap_or(None),
            "created_at": row.get::<_, i64>(8)?,
            "completed_at": row.get::<_, Option<i64>>(9).unwrap_or(Some(0)).unwrap_or(0),
        }))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

/// 从自然语言文本解析为事件结构（V1 简易版: 关键词匹配）
#[tauri::command]
pub fn system_parse_event(text: String) -> Value {
    let lower = text.to_lowercase();

    // 简单判断类型
    let event_type = if lower.contains("待办") || lower.contains("todo") || lower.contains("任务")
    {
        "todo"
    } else {
        "event"
    };

    // 提取标题（取前 50 个字符）
    let title: String = text.chars().take(50).collect();

    // 尝试从文本中提取日期（简单正则不引入额外依赖）
    let today = chrono_like_today();

    json!({
        "title": title,
        "type": event_type,
        "status": "pending",
        "date": today,
        "startTime": null,
        "endTime": null,
        "description": text,
    })
}

/// 确认并保存事件到数据库
#[tauri::command]
pub fn system_confirm_event(event: Value, db: tauri::State<'_, crate::db::DbState>) -> Value {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({ "ok": false, "error": "数据库锁失败" }),
    };

    let id = format!("evt-{}", super::now_ms());
    let title = event.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let etype = event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("event");
    let status = event
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending");
    let date = event.get("date").and_then(|v| v.as_str());
    let start_time = event.get("startTime").and_then(|v| v.as_str());
    let end_time = event.get("endTime").and_then(|v| v.as_str());
    let description = event
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match conn.execute(
        "INSERT INTO events (id, title, type, status, date, start_time, end_time, description, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, title, etype, status, date, start_time, end_time, description, super::now_ms()],
    ) {
        Ok(_) => json!({ "ok": true, "id": id }),
        Err(e) => json!({ "ok": false, "error": format!("{}", e) }),
    }
}

/// 删除事件
#[tauri::command]
pub fn system_delete_event(id: String, db: tauri::State<'_, crate::db::DbState>) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let rows = conn.execute("DELETE FROM events WHERE id = ?1", params![&id])
        .unwrap_or(0);
    if rows > 0 {
        let ts = crate::now_ms();
        let _ = conn.execute("INSERT OR REPLACE INTO sync_tombstones (table_name, record_key, deleted_at) VALUES ('events', ?1, ?2)", params![id, ts]);
    }
    true
}

#[tauri::command]
pub fn system_update_event_description(
    id: String,
    description: String,
    db: tauri::State<'_, crate::db::DbState>,
) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.execute(
        "UPDATE events SET description = ?1 WHERE id = ?2",
        params![description, id],
    )
    .is_ok()
}

/// 更新事件状态（pending/done/cancelled）
#[tauri::command]
pub fn system_update_event_status(
    id: String,
    status: String,
    db: tauri::State<'_, crate::db::DbState>,
) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let completed_at = if status == "done" {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    } else {
        0
    };

    conn.execute(
        "UPDATE events SET status = ?1, completed_at = ?2 WHERE id = ?3",
        params![status, completed_at, id],
    )
    .unwrap_or(0);
    true
}

/// 更新事件时间（拖拽调整）
#[tauri::command]
pub fn system_update_event_time(
    id: String,
    start_time: String,
    end_time: String,
    db: tauri::State<'_, crate::db::DbState>,
) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.execute(
        "UPDATE events SET start_time = ?1, end_time = ?2 WHERE id = ?3",
        params![start_time, end_time, id],
    )
    .unwrap_or(0);
    true
}

/// 简易日期生成（避免引入 chrono 依赖）
fn chrono_like_today() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // 简单计算: Unix timestamp to YYYY-MM-DD (UTC)
    let days = now / 86400;
    let mut y = 1970i64;
    let mut remaining = days as i64;

    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let months_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 1;
    for &md in &months_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }

    format!("{:04}-{:02}-{:02}", y, m, remaining + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// T-1307: 供后台定时器调用的内部接口，获取今天尚未提醒过的待办/事件
pub fn get_due_todos_for_scheduler(conn: &rusqlite::Connection) -> Vec<(String, String)> {
    let today = chrono_like_today();

    // 把 today 转换为整数以便和 last_notified 比较 (2026-06-06 -> 20260606)
    let today_int: i64 = today.replace("-", "").parse().unwrap_or(0);

    let mut stmt = match conn.prepare(
        "SELECT id, title FROM events 
         WHERE date = ?1 
         AND status != 'completed' 
         AND last_notified != ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map(params![today, today_int], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

/// T-1307: 标记指定事件在今天已提醒
pub fn mark_todo_notified(conn: &rusqlite::Connection, id: &str) {
    let today = chrono_like_today();
    let today_int: i64 = today.replace("-", "").parse().unwrap_or(0);

    let _ = conn.execute(
        "UPDATE events SET last_notified = ?1 WHERE id = ?2",
        params![today_int, id],
    );
}
