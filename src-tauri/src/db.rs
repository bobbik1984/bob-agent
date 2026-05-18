use rusqlite::{Connection, params};
use serde_json::Value;
use std::sync::Mutex;
use tauri::State;

pub struct DbState(pub Mutex<Connection>);

pub fn init_db(data_dir: &std::path::Path) -> Connection {
    let db_path = data_dir.join("bob.db");
    let conn = Connection::open(&db_path)
        .expect("Failed to open SQLite database");

    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '新对话',
            model TEXT DEFAULT '',
            cost REAL DEFAULT 0.0,
            last_message TEXT,
            last_role TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            image_base64 TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id);
    ").expect("Failed to initialize database tables");

    // 启用 WAL 模式（并发读写性能更佳）
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap_or_default();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap_or_default();

    // Phase 2 迁移：messages 表新增 from_channel 列（已存在则忽略）
    conn.execute_batch(
        "ALTER TABLE messages ADD COLUMN from_channel TEXT DEFAULT 'desktop';"
    ).unwrap_or_default();

    // 初始化日程表
    crate::calendar::init_events_table(&conn);

    // LLM-Wiki 知识库全文搜索索引 (FTS5)
    conn.execute_batch("
        CREATE VIRTUAL TABLE IF NOT EXISTS wiki_fts USING fts5(
            file_name,
            source_path,
            wiki_path,
            summary,
            keywords,
            category,
            indexed_at
        );
    ").unwrap_or_default();

    conn
}

#[tauri::command]
pub fn db_conversations(db: State<DbState>) -> Vec<Value> {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare(
        "SELECT id, title, model, cost, last_message, last_role, created_at, updated_at
         FROM conversations ORDER BY updated_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "model": row.get::<_, String>(2).unwrap_or_default(),
            "cost": row.get::<_, f64>(3).unwrap_or(0.0),
            "last_message": row.get::<_, Option<String>>(4).unwrap_or(None),
            "last_role": row.get::<_, Option<String>>(5).unwrap_or(None),
            "created_at": row.get::<_, i64>(6)?,
            "updated_at": row.get::<_, i64>(7)?,
        }))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

#[tauri::command]
pub fn db_conversation_create(title: String, model: String, db: State<DbState>) -> Value {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return serde_json::json!({ "error": "数据库锁失败" }),
    };
    let id = format!("conv-{}", crate::now_ms());
    let ts = crate::now_ms();
    if let Err(e) = conn.execute(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, title, model, ts, ts],
    ) {
        return serde_json::json!({ "error": format!("创建对话失败: {}", e) });
    }
    serde_json::json!({
        "id": id,
        "title": title,
        "model": model,
        "cost": 0.0,
        "last_message": null,
        "last_role": null,
        "created_at": ts,
        "updated_at": ts,
    })
}

#[tauri::command]
pub fn db_conversation_get(id: String, db: State<DbState>) -> Option<Value> {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return None,
    };
    conn.query_row(
        "SELECT id, title, model, cost, created_at, updated_at FROM conversations WHERE id = ?1",
        params![id],
        |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "model": row.get::<_, String>(2).unwrap_or_default(),
                "cost": row.get::<_, f64>(3).unwrap_or(0.0),
                "created_at": row.get::<_, i64>(4)?,
                "updated_at": row.get::<_, i64>(5)?,
            }))
        }
    ).ok()
}

#[tauri::command]
pub fn db_conversation_delete(id: String, db: State<DbState>) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    // 先删消息，再删对话
    conn.execute("DELETE FROM messages WHERE conversation_id = ?1", params![id]).unwrap_or(0);
    conn.execute("DELETE FROM conversations WHERE id = ?1", params![id]).unwrap_or(0);
    true
}

#[tauri::command]
pub fn db_conversation_rename(id: String, title: String, db: State<DbState>) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.execute(
        "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, crate::now_ms(), id],
    ).unwrap_or(0);
    true
}

#[tauri::command]
pub fn db_conversation_update_cost(id: String, cost: f64, db: State<DbState>) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.execute(
        "UPDATE conversations SET cost = ?1 WHERE id = ?2",
        params![cost, id],
    ).unwrap_or(0);
    true
}

#[tauri::command]
pub fn db_messages(conversation_id: String, db: State<DbState>) -> Vec<Value> {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare(
        "SELECT id, role, content, image_base64, created_at
         FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map(params![conversation_id], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "role": row.get::<_, String>(1)?,
            "content": row.get::<_, String>(2)?,
            "image_base64": row.get::<_, Option<String>>(3).unwrap_or(None),
            "created_at": row.get::<_, i64>(4)?,
        }))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

#[tauri::command]
pub fn db_message_add(
    conversation_id: String, role: String, content: String,
    image_base64: Option<String>, db: State<DbState>
) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let ts = crate::now_ms();

    conn.execute(
        "INSERT INTO messages (conversation_id, role, content, image_base64, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![conversation_id, role, content, image_base64, ts],
    ).unwrap_or(0);

    // 更新对话的最后消息和时间戳
    let preview: String = content.chars().take(20).collect();
    conn.execute(
        "UPDATE conversations SET last_message = ?1, last_role = ?2, updated_at = ?3 WHERE id = ?4",
        params![preview, role, ts, conversation_id],
    ).unwrap_or(0);

    true
}

#[tauri::command]
pub fn system_factory_reset(db: State<DbState>) -> bool {
    // 1. 清空数据库
    if let Ok(conn) = db.0.lock() {
        let _ = conn.execute_batch("DELETE FROM messages; DELETE FROM conversations;");
    }
    // 2. 删除配置文件
    let config_path = crate::get_config_path();
    let _ = std::fs::remove_file(config_path);
    true
}
