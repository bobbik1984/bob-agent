//! http_api.rs — Bob-Agent 本地 HTTP 服务
//!
//! 监听 127.0.0.1:3721，暴露以下端点供 wechat-bot-bridge 调用：
//!
//!   POST /v1/chat              — SSE 流式对话（含 Tool Calling）
//!   GET  /v1/conversations     — 最近 N 条会话列表
//!   GET  /v1/health            — 健康检查

use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Listener};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

// ═══════════════════════════════════════════════════════════
// 共享应用状态
// ═══════════════════════════════════════════════════════════

/// 传递给每个 axum handler 的全局状态
#[derive(Clone)]
pub struct ApiState {
    pub app: AppHandle,
}

// ═══════════════════════════════════════════════════════════
// 请求 / 响应类型
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// 用户消息文本
    pub message: String,
    /// 要继续的会话 ID；None = 新建会话
    pub conversation_id: Option<String>,
    /// 消息来源渠道，例如 "wechat" | "desktop"
    pub from_channel: Option<String>,
    /// 微信用户 wxid（仅 from_channel = "wechat" 时有意义）
    pub from_user: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConversationSummary {
    id: String,
    title: String,
    updated_at: i64,
}

// ═══════════════════════════════════════════════════════════
// 数据库辅助函数（不依赖 Tauri State 锁，直接获取连接）
// ═══════════════════════════════════════════════════════════

/// 获取数据库连接（只读操作）
fn open_db(_app: &AppHandle) -> Option<Connection> {
    let data_dir = crate::get_data_dir();
    let db_path = data_dir.join("bob.db");
    Connection::open(db_path).ok()
}

/// 创建新会话，返回会话 ID
fn create_conversation(conn: &Connection, title: &str) -> Option<String> {
    let id = format!("conv-{}", crate::now_ms());
    let ts = crate::now_ms();
    conn.execute(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, '', ?3, ?4)",
        params![id, title, ts, ts],
    ).ok()?;
    Some(id)
}

/// 追加消息到指定会话
fn append_message(conn: &Connection, conversation_id: &str, role: &str, content: &str) {
    let ts = crate::now_ms();
    let _ = conn.execute(
        "INSERT INTO messages (conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![conversation_id, role, content, ts],
    );
    let preview: String = content.chars().take(20).collect();
    let _ = conn.execute(
        "UPDATE conversations SET last_message = ?1, last_role = ?2, updated_at = ?3 WHERE id = ?4",
        params![preview, role, ts, conversation_id],
    );
}

/// 加载会话历史消息（按时间升序）
fn load_history(conn: &Connection, conversation_id: &str) -> Vec<Value> {
    let mut stmt = match conn.prepare(
        "SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map(params![conversation_id], |row| {
        Ok(json!({
            "role": row.get::<_, String>(0)?,
            "content": row.get::<_, String>(1)?,
        }))
    });
    match rows {
        Ok(r) => r.filter_map(|x| x.ok()).collect(),
        Err(_) => vec![],
    }
}

/// 获取最近 N 条会话
fn get_recent_conversations(conn: &Connection, limit: usize) -> Vec<ConversationSummary> {
    let mut stmt = match conn.prepare(
        "SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC LIMIT ?1"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(ConversationSummary {
            id: row.get(0)?,
            title: row.get(1)?,
            updated_at: row.get(2)?,
        })
    });
    match rows {
        Ok(r) => r.filter_map(|x| x.ok()).collect(),
        Err(_) => vec![],
    }
}

// ═══════════════════════════════════════════════════════════
// Handler: POST /v1/chat  — SSE 流式对话
// ═══════════════════════════════════════════════════════════

async fn handle_chat(
    State(state): State<ApiState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let app = state.app.clone();

    // ── 1. 打开数据库，决定 conversation_id ──────────────────
    let conn = match open_db(&app) {
        Some(c) => Arc::new(Mutex::new(c)),
        None => {
            // 返回一个立即完成的 SSE 错误流
            let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(1);
            let _ = tx.send(Ok(Event::default()
                .event("error")
                .data("{\"error\":\"数据库连接失败\"}"))).await;
            return Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default());
        }
    };

    let conversation_id = {
        let db = conn.lock().unwrap();
        match req.conversation_id.as_deref() {
            Some(id) if !id.is_empty() => {
                // 校验会话存在
                let exists: bool = db
                    .query_row("SELECT 1 FROM conversations WHERE id = ?1", params![id], |_| Ok(true))
                    .unwrap_or(false);
                if exists {
                    id.to_string()
                } else {
                    // ID 不存在时新建
                    let title: String = req.message.chars().take(20).collect();
                    create_conversation(&db, &title).unwrap_or_else(|| format!("conv-{}", crate::now_ms()))
                }
            }
            _ => {
                // 新建会话，标题取消息前 20 字
                let title: String = req.message.chars().take(20).collect();
                create_conversation(&db, &title).unwrap_or_else(|| format!("conv-{}", crate::now_ms()))
            }
        }
    };

    // ── 2. 加载历史并追加用户消息 ────────────────────────────
    let messages: Vec<Value> = {
        let db = conn.lock().unwrap();
        // 先写入用户消息
        append_message(&db, &conversation_id, "user", &req.message);
        // 再读取全量历史（含刚写入的用户消息）
        load_history(&db, &conversation_id)
    };

    let conv_id_clone = conversation_id.clone();
    let app_clone = app.clone();
    let conn_clone = conn.clone();

    // ── 3. 建立 SSE 通道 ──────────────────────────────────────
    // tx: Rust 侧写入 SSE 事件
    // rx: axum 包装成 SSE 流传给客户端
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);

    // ── 4. 后台 Task: 调用 LLM 并把 Tauri 事件桥接到 SSE ────
    tokio::spawn(async move {
        // 订阅 Tauri 的 llm:chunk 事件，转发给 SSE 客户端
        let tx_chunk = tx.clone();
        let conv_id_for_done = conv_id_clone.clone();

        // 用一个内部 mpsc 通道做 Tauri→SSE 桥
        let (bridge_tx, mut bridge_rx) = mpsc::channel::<Value>(64);
        let bridge_tx = Arc::new(bridge_tx);

        // 注册 Tauri 事件监听（在 LLM 调用开始前）
        let bridge_tx_for_listener = bridge_tx.clone();
        let listener_id = app_clone.listen("llm:chunk", move |event| {
            if let Ok(payload) = serde_json::from_str::<Value>(event.payload()) {
                let _ = bridge_tx_for_listener.try_send(payload);
            }
        });

        // 在独立 task 里把 bridge_rx 转发给 SSE tx
        let tx_forward = tx_chunk.clone();
        let forward_handle = tokio::spawn(async move {
            let mut full_text = String::new();
            while let Some(chunk) = bridge_rx.recv().await {
                let chunk_type = chunk.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match chunk_type {
                    "text" => {
                        if let Some(content) = chunk.get("content").and_then(|v| v.as_str()) {
                            full_text.push_str(content);
                            let event = Event::default()
                                .event("text")
                                .data(json!({ "content": content }).to_string());
                            let _ = tx_forward.send(Ok(event)).await;
                        }
                    }
                    "thinking" => {
                        if let Some(content) = chunk.get("content").and_then(|v| v.as_str()) {
                            let event = Event::default()
                                .event("thinking")
                                .data(json!({ "content": content }).to_string());
                            let _ = tx_forward.send(Ok(event)).await;
                        }
                    }
                    "tool_start" => {
                        let name = chunk.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let event = Event::default()
                            .event("tool_start")
                            .data(json!({ "name": name }).to_string());
                        let _ = tx_forward.send(Ok(event)).await;
                    }
                    "tool_end" => {
                        let name = chunk.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let event = Event::default()
                            .event("tool_end")
                            .data(json!({ "name": name }).to_string());
                        let _ = tx_forward.send(Ok(event)).await;
                    }
                    "done" => {
                        // LLM 完成，将完整文本和 conv_id 一起发给客户端
                        let event = Event::default()
                            .event("done")
                            .data(json!({
                                "conversation_id": conv_id_for_done,
                                "full_text": full_text
                            }).to_string());
                        let _ = tx_forward.send(Ok(event)).await;
                        break;
                    }
                    _ => {}
                }
            }
            full_text
        });

        // 调用 LLM（阻塞直到完成）
        let result = crate::llm::stream_chat(app_clone.clone(), messages).await;

        // 取消事件监听
        app_clone.unlisten(listener_id);

        // 等待 forward task 完成，获取完整文本
        let full_text = forward_handle.await.unwrap_or_default();

        // ── 5. 将 assistant 回复写入数据库 ──────────────────
        let assistant_content = if full_text.is_empty() {
            // 如果流没收到文本（可能发生错误），用 result 中的 content 字段
            result.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string()
        } else {
            full_text
        };

        if !assistant_content.is_empty() {
            let db = conn_clone.lock().unwrap();
            append_message(&db, &conv_id_clone, "assistant", &assistant_content);
        }

        // ── 6. 广播 remote:new-message 给桌面端 UI ──────────
        let _ = app_clone.emit("remote:new-message", json!({
            "conversation_id": conv_id_clone,
            "from_channel": req.from_channel.as_deref().unwrap_or("wechat"),
        }));
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
}

// ═══════════════════════════════════════════════════════════
// Handler: GET /v1/conversations — 最近会话列表
// ═══════════════════════════════════════════════════════════

async fn handle_get_conversations(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let conn = match open_db(&state.app) {
        Some(c) => c,
        None => return Json(json!({ "error": "数据库连接失败" })),
    };
    let list = get_recent_conversations(&conn, 10);
    let result: Vec<Value> = list
        .into_iter()
        .map(|c| json!({ "id": c.id, "title": c.title, "updated_at": c.updated_at }))
        .collect();
    Json(json!(result))
}

// ═══════════════════════════════════════════════════════════
// Handler: GET /v1/health
// ═══════════════════════════════════════════════════════════

async fn handle_health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "service": "bob-agent-api", "version": env!("CARGO_PKG_VERSION") }))
}

// ═══════════════════════════════════════════════════════════
// 路由组装 & 服务启动
// ═══════════════════════════════════════════════════════════

pub fn create_router(app: AppHandle) -> Router {
    let state = ApiState { app };
    Router::new()
        .route("/v1/chat", post(handle_chat))
        .route("/v1/conversations", get(handle_get_conversations))
        .route("/v1/health", get(handle_health))
        .with_state(state)
}

/// 在后台 Task 中启动 HTTP 服务，绑定 127.0.0.1:3721
pub fn start_http_server(app: AppHandle) {
    let router = create_router(app);
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:3721").await {
            Ok(l) => l,
            Err(e) => {
                log::error!("[http_api] 无法绑定 127.0.0.1:3721: {}", e);
                return;
            }
        };
        log::info!("[http_api] Bob HTTP API 启动成功，监听 127.0.0.1:3721");
        if let Err(e) = axum::serve(listener, router).await {
            log::error!("[http_api] 服务异常退出: {}", e);
        }
    });
}
