use rusqlite::{params, Connection};
use serde_json::Value;
use std::sync::Mutex;
use tauri::State;

pub struct DbState(pub Mutex<Connection>);

pub fn init_db(data_dir: &std::path::Path) -> Connection {
    let db_path = data_dir.join("bob.db");

    let db_backup = data_dir.join("bob.db.bak");

    // ==========================================
    // 1. 无感自检与自愈 (Self-Healing)
    // ==========================================
    if db_path.exists() {
        let is_healthy = match Connection::open(&db_path) {
            Ok(probe) => {
                let mut healthy = false;
                if let Ok(mut stmt) = probe.prepare("PRAGMA quick_check;") {
                    if let Ok(mut rows) = stmt.query([]) {
                        if let Ok(Some(row)) = rows.next() {
                            let result: String = row.get(0).unwrap_or_default();
                            if result.to_lowercase() == "ok" {
                                healthy = true;
                            }
                        }
                    }
                }
                // 显式释放 probe，解除 SQLite 文件占用锁
                drop(probe);
                healthy
            }
            Err(_) => false,
        };

        if is_healthy {
            // 健康：更新冷备份文件，保证备份池 100% 纯净
            let _ = std::fs::copy(&db_path, &db_backup);
        } else {
            // 损坏：尝试从备份安全回滚
            log::warn!(
                "T-1304: Database corruption detected during startup. Attempting self-healing..."
            );
            if db_backup.exists() {
                if std::fs::copy(&db_backup, &db_path).is_err() {
                    // 如果连恢复都失败，干脆重命名坏库，触发降级创建空库
                    let _ = std::fs::rename(&db_path, data_dir.join("bob.db.corrupted"));
                }
            } else {
                // 没有备份可用，直接重命名坏库隔离
                let _ = std::fs::rename(&db_path, data_dir.join("bob.db.corrupted"));
            }
        }
    }

    // ==========================================
    // 2. 正式开辟全局连接
    // ==========================================
    let conn = Connection::open(&db_path)
        .expect("Failed to open SQLite database even after self-healing attempts");

    conn.execute_batch(
        "
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
    ",
    )
    .expect("Failed to initialize database tables");

    // 启用 WAL 模式（并发读写性能更佳）
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .unwrap_or_default();
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .unwrap_or_default();

    // Phase 2 迁移：messages 表新增 from_channel 列（已存在则忽略）
    conn.execute_batch("ALTER TABLE messages ADD COLUMN from_channel TEXT DEFAULT 'desktop';")
        .unwrap_or_default();

    // ==========================================
    // T-2335 同步协议基建
    // ==========================================
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sync_tombstones (
            table_name TEXT NOT NULL,
            record_key TEXT NOT NULL,
            deleted_at INTEGER NOT NULL,
            PRIMARY KEY (table_name, record_key)
        );
        CREATE TABLE IF NOT EXISTS sync_conflicts (
            id TEXT PRIMARY KEY,
            table_name TEXT NOT NULL,
            local_id TEXT NOT NULL,
            remote_id TEXT NOT NULL,
            status TEXT DEFAULT 'pending',
            created_at INTEGER NOT NULL
        );
        "
    ).unwrap_or_default();

    conn.execute_batch("ALTER TABLE messages ADD COLUMN sync_id TEXT;").unwrap_or_default();
    
    let my_device_id = crate::read_config().get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    conn.execute(
        "UPDATE messages SET sync_id = ?1 || '-' || created_at || '-' || id WHERE sync_id IS NULL",
        params![my_device_id]
    ).unwrap_or_default();
    
    conn.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_sync ON messages(sync_id);").unwrap_or_default();

    // 初始化日程表
    crate::calendar::init_events_table(&conn);

    // 初始化 Cron 调度表 (T-1211)
    crate::scheduler::init_cron_table(&conn);

    // LLM-Wiki 知识库全文搜索索引 (FTS5)
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS wiki_fts USING fts5(
            file_name,
            source_path,
            wiki_path,
            summary,
            keywords,
            category,
            indexed_at
        );
    ",
    )
    .unwrap_or_default();

    // P0-2: 笔记专用全文搜索索引 (FTS5)
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
            note_path,
            title,
            content,
            tags,
            tokenize='unicode61'
        );
    ",
    )
    .unwrap_or_default();

    conn.execute_batch("
        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            content,
            content=messages,
            content_rowid=id
        );

        -- 自动同步触发器
        CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
        END;
        CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.id, old.content);
        END;
        CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE OF content ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.id, old.content);
            INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
        END;
    ").unwrap_or_default();

    // 回填存量消息到 FTS 索引
    conn.execute_batch(
        "
        INSERT OR IGNORE INTO messages_fts(rowid, content)
        SELECT id, content FROM messages;
    ",
    )
    .unwrap_or_default();

    // ── 进化引擎: 零成本遥测记录 ──────────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS session_observations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            model_used TEXT DEFAULT '',
            tool_calls_count INTEGER DEFAULT 0,
            tool_failures INTEGER DEFAULT 0,
            total_rounds INTEGER DEFAULT 0,
            duration_ms INTEGER DEFAULT 0,
            tokens_in INTEGER DEFAULT 0,
            tokens_out INTEGER DEFAULT 0,
            stop_reason TEXT DEFAULT '',
            created_at INTEGER NOT NULL
        );
    ",
    )
    .unwrap_or_default();

    // ── 进化引擎: 做梦日志 ──────────────────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS evolution_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            dream_type TEXT NOT NULL DEFAULT 'daily_catchup',
            facts_extracted INTEGER DEFAULT 0,
            stale_cleaned INTEGER DEFAULT 0,
            memories_merged INTEGER DEFAULT 0,
            soul_refined INTEGER DEFAULT 0,
            report_text TEXT DEFAULT '',
            soul_hash TEXT DEFAULT '',
            created_at INTEGER NOT NULL
        );
    ",
    )
    .unwrap_or_default();

    // ── M17: 知识图谱 ──────────────────────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS kg_nodes (
            id          TEXT PRIMARY KEY,
            label       TEXT NOT NULL,
            node_type   TEXT NOT NULL DEFAULT 'concept',
            summary     TEXT DEFAULT '',
            source      TEXT DEFAULT '',
            metadata    TEXT DEFAULT '{}',
            created_at  TEXT DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS kg_edges (
            source_id   TEXT NOT NULL,
            target_id   TEXT NOT NULL,
            relation    TEXT NOT NULL DEFAULT 'related_to',
            confidence  REAL DEFAULT 0.8,
            created_at  TEXT DEFAULT (datetime('now')),
            PRIMARY KEY (source_id, target_id, relation)
        );
        CREATE INDEX IF NOT EXISTS idx_kg_edges_source ON kg_edges(source_id);
        CREATE INDEX IF NOT EXISTS idx_kg_edges_target ON kg_edges(target_id);
        
        CREATE TABLE IF NOT EXISTS kg_source_batches (
            batch_id    TEXT PRIMARY KEY,
            folder_name TEXT NOT NULL,
            folder_path TEXT NOT NULL,
            file_count  INTEGER DEFAULT 0,
            created_at  TEXT DEFAULT (datetime('now')),
            status      TEXT DEFAULT 'active'
        );
    ",
    )
    .unwrap_or_default();

    let _ = conn.execute(
        "ALTER TABLE kg_nodes ADD COLUMN source_batches TEXT DEFAULT '[]'",
        [],
    );
    let _ = conn.execute("ALTER TABLE kg_nodes ADD COLUMN updated_at INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE kg_edges ADD COLUMN updated_at INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE events ADD COLUMN updated_at INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE cron_jobs ADD COLUMN updated_at INTEGER DEFAULT 0", []);

    // ── 目標 19: Goal Mode V2 执行错误记录 ────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS execution_errors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conv_id TEXT,
            goal_description TEXT,
            tool_name TEXT NOT NULL,
            error_type TEXT NOT NULL,
            error_message TEXT NOT NULL,
            context_summary TEXT,
            analyzed INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_exec_errors_analyzed ON execution_errors(analyzed);
        CREATE INDEX IF NOT EXISTS idx_exec_errors_created ON execution_errors(created_at);
    ",
    )
    .unwrap_or_default();

    // 数据迁移: 标准化知识图谱节点类型 (修复中英文混杂的问题)
    conn.execute_batch("
        UPDATE kg_nodes SET node_type = 'concept' WHERE node_type IN ('Concept', '概念', '名词');
        UPDATE kg_nodes SET node_type = 'project' WHERE node_type IN ('Project', '项目');
        UPDATE kg_nodes SET node_type = 'file'    WHERE node_type IN ('File', '文件');
        UPDATE kg_nodes SET node_type = 'tag'     WHERE node_type IN ('Tag', '标签');
        UPDATE kg_nodes SET node_type = 'person'  WHERE node_type IN ('Person', '人物', '人名', 'author', '作者');
        UPDATE kg_nodes SET node_type = 'topic'   WHERE node_type IN ('Topic', '主题');
        UPDATE kg_nodes SET node_type = 'entity'  WHERE node_type IN ('Entity', '实体');
        UPDATE kg_nodes SET node_type = 'organization' WHERE node_type IN ('Organization', '组织', '机构', '公司');
        UPDATE kg_nodes SET node_type = 'location' WHERE node_type IN ('Location', '地点', '位置');
        UPDATE kg_nodes SET node_type = 'event'   WHERE node_type IN ('Event', '事件');
        UPDATE kg_nodes SET node_type = 'technology' WHERE node_type IN ('Technology', '技术');
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
         FROM conversations ORDER BY updated_at DESC",
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
    let id = ulid::Ulid::new().to_string();
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
        },
    )
    .ok()
}

#[tauri::command]
pub fn db_conversation_delete(id: String, db: State<DbState>) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    // 先删消息，再删对话
    conn.execute(
        "DELETE FROM messages WHERE conversation_id = ?1",
        params![&id],
    )
    .unwrap_or(0);
    
    let rows = conn.execute("DELETE FROM conversations WHERE id = ?1", params![&id])
        .unwrap_or(0);
    
    if rows > 0 {
        let ts = crate::now_ms();
        conn.execute(
            "INSERT OR REPLACE INTO sync_tombstones (table_name, record_key, deleted_at) VALUES ('conversations', ?1, ?2)", 
            params![id, ts]
        ).unwrap_or(0);
    }
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
    )
    .unwrap_or(0);
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
    )
    .unwrap_or(0);
    true
}

#[tauri::command]
pub fn db_messages(conversation_id: String, db: State<DbState>) -> Vec<Value> {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare(
        "SELECT id, role, content, image_base64, created_at, from_channel
         FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
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
            "from_channel": row.get::<_, Option<String>>(5).unwrap_or(None),
        }))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

#[tauri::command]
pub fn db_message_add(
    conversation_id: String,
    role: String,
    content: String,
    image_base64: Option<String>,
    db: State<DbState>,
) -> bool {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let ts = crate::now_ms();
    let config = crate::read_config();
    let my_device_id = config.get("device_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let uuid_part: String = uuid::Uuid::new_v4().to_string().chars().take(8).collect();
    let sync_id = format!("{}-{}-{}", my_device_id, ts, uuid_part);

    conn.execute(
        "INSERT INTO messages (conversation_id, role, content, image_base64, created_at, sync_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![conversation_id, role, content, image_base64, ts, sync_id],
    )
    .unwrap_or(0);

    // 更新对话的最后消息和时间戳
    let preview: String = content.chars().take(20).collect();
    conn.execute(
        "UPDATE conversations SET last_message = ?1, last_role = ?2, updated_at = ?3 WHERE id = ?4",
        params![preview, role, ts, conversation_id],
    )
    .unwrap_or(0);

    true
}

#[tauri::command]
pub fn db_search_messages(query: String, db: State<DbState>) -> Vec<Value> {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let search_query = query.trim().to_string();
    if search_query.is_empty() {
        return vec![];
    }
    let mut stmt = match conn.prepare(
        "SELECT m.id, m.conversation_id, c.title as conv_title,
                snippet(messages_fts, 0, '<mark>', '</mark>', '...', 32) as snippet,
                m.created_at
         FROM messages_fts fts
         JOIN messages m ON m.id = fts.rowid
         JOIN conversations c ON c.id = m.conversation_id
         WHERE messages_fts MATCH ?1
         ORDER BY rank
         LIMIT 30",
    ) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("FTS search error: {}", e);
            return vec![];
        }
    };

    let rows = match stmt.query_map(params![search_query], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "conversation_id": row.get::<_, String>(1)?,
            "conv_title": row.get::<_, String>(2)?,
            "snippet": row.get::<_, String>(3)?,
            "created_at": row.get::<_, i64>(4)?,
        }))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

// ═══════════════════════════════════════════════════════════
// 目标 19: Goal Mode V2 — 执行错误记录与查询
// ═══════════════════════════════════════════════════════════

/// 记录一条执行错误到 execution_errors 表
pub fn log_execution_error(
    conn: &Connection,
    conv_id: Option<&str>,
    goal: &str,
    tool_name: &str,
    error_type: &str,
    error_msg: &str,
    context: Option<&str>,
) -> Result<i64, String> {
    let ts = crate::now_ms();
    conn.execute(
        "INSERT INTO execution_errors (conv_id, goal_description, tool_name, error_type, error_message, context_summary, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![conv_id, goal, tool_name, error_type, error_msg, context, ts],
    ).map_err(|e| format!("log_execution_error: {}", e))?;
    Ok(conn.last_insert_rowid())
}

/// 查询未被 Dream Engine 分析过的错误记录
pub fn get_unanalyzed_errors(conn: &Connection, since_hours: u64) -> Vec<Value> {
    let cutoff = crate::now_ms() as i64 - (since_hours as i64 * 3_600_000);
    let mut stmt = match conn.prepare(
        "SELECT id, conv_id, goal_description, tool_name, error_type, error_message, context_summary, created_at
         FROM execution_errors
         WHERE analyzed = 0 AND created_at > ?1
         ORDER BY created_at DESC
         LIMIT 100"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map(params![cutoff], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "conv_id": row.get::<_, Option<String>>(1).unwrap_or(None),
            "goal": row.get::<_, Option<String>>(2).unwrap_or(None),
            "tool_name": row.get::<_, String>(3)?,
            "error_type": row.get::<_, String>(4)?,
            "error_message": row.get::<_, String>(5)?,
            "context": row.get::<_, Option<String>>(6).unwrap_or(None),
            "created_at": row.get::<_, i64>(7)?,
        }))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

/// 标记指定错误记录为已分析（Dream Engine 处理后调用）
pub fn mark_errors_analyzed(conn: &Connection, ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    for id in ids {
        conn.execute(
            "UPDATE execution_errors SET analyzed = 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| format!("mark_errors_analyzed: {}", e))?;
    }
    Ok(())
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

// ═══════════════════════════════════════════════════════════
// 同步协议 - 辅助函数
// ═══════════════════════════════════════════════════════════
pub fn db_delete_with_tombstone(
    conn: &Connection,
    table_name: &str,
    record_key: &str,
    delete_query: &str,
    params: &[&dyn rusqlite::ToSql]
) -> Result<usize, String> {
    let rows_deleted = conn.execute(delete_query, params).map_err(|e| e.to_string())?;
    if rows_deleted > 0 {
        let ts = crate::now_ms();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO sync_tombstones (table_name, record_key, deleted_at) VALUES (?1, ?2, ?3)",
            params![table_name, record_key, ts]
        );
    }
    Ok(rows_deleted)
}
