//! http_api.rs �?Bob.agent 本地 HTTP 服务
//!
//! 监听 127.0.0.1:3721，暴露以下端点供 wechat-bot-bridge 调用�?
//!
//!   POST /v1/chat              �?SSE 流式对话（含 Tool Calling�?
//!   GET  /v1/conversations     �?最�?N 条会话列�?
//!   GET  /v1/health            �?健康检�?

use axum::{
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Listener, Manager};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use ed25519_dalek::SigningKey;

// ══════════════════════════════════════════════════════════?
// 共享应用状?
// ══════════════════════════════════════════════════════════?

/// 传递给每个 axum handler 的全局状?
#[derive(Clone, Default)]
pub struct ApiState {
    pub app: Option<AppHandle>,
    pub db: Option<Arc<Mutex<Connection>>>,
    pub test_target_id: Option<String>,
    pub test_signing_key: Option<SigningKey>,
}

// ══════════════════════════════════════════════════════════?
// 请求 / 响应类型
// ══════════════════════════════════════════════════════════?

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// 用户消息文本
    pub message: String,
    /// 要继续的会话 ID；None = 新建会话
    pub conversation_id: Option<String>,
    /// 消息来源渠道，例�?"wechat" | "desktop"
    pub from_channel: Option<String>,
    /// 微信用户 wxid（仅 from_channel = "wechat" 时有意义�?
    pub from_user: Option<String>,
    /// 代理模式，例�?"auto" | "manual"
    pub agent_mode: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConversationSummary {
    id: String,
    title: String,
    updated_at: i64,
}

// ══════════════════════════════════════════════════════════�?
// 数据库辅助函数（不依�?Tauri State 锁，直接获取连接�?
// ══════════════════════════════════════════════════════════�?

/// 获取数据库连�?
pub fn open_db_for_app(_app: &AppHandle) -> Option<Connection> {
    let data_dir = crate::get_data_dir();
    let db_path = data_dir.join("bob.db");
    Connection::open(db_path).ok()
}

fn open_db(app: &AppHandle) -> Option<Connection> {
    open_db_for_app(app)
}

/// 获取本机设备 ID
pub fn get_local_device_id() -> String {
    let config = crate::read_config();
    config.get("device_id").and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// 获取本机设备 ID (严格校验配置完整性，Fail-Closed)
pub fn get_local_device_id_checked() -> Result<String, String> {
    let config = crate::read_config_checked()?;
    let id = config.get("device_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if id.is_empty() {
        return Err("SEC-01: 本机 device_id 为空或未配置".to_string());
    }
    Ok(id.to_string())
}

/// 解析本机设备 ID (优先从已解锁的 SigningKey 内存状态读取，缺失时回退至 config.json)
pub fn resolve_local_device_id(app_opt: Option<&AppHandle>) -> String {
    if let Some(app) = app_opt {
        if let Ok(sk) = crate::crypto::ensure_device_identity_unlocked_for_app(app) {
            let vk = ed25519_dalek::VerifyingKey::from(&sk);
            return base64::engine::general_purpose::STANDARD.encode(vk.to_bytes());
        }
    }
    get_local_device_id()
}

/// 解析本机设备 ID (优先从已解锁的 SigningKey 内存状态读取，缺失时从受保护的 checked 配置读取，Fail-Closed)
pub fn resolve_local_device_id_checked(app_opt: Option<&AppHandle>) -> Result<String, String> {
    if let Some(app) = app_opt {
        if let Ok(sk) = crate::crypto::ensure_device_identity_unlocked_for_app(app) {
            let vk = ed25519_dalek::VerifyingKey::from(&sk);
            return Ok(base64::engine::general_purpose::STANDARD.encode(vk.to_bytes()));
        }
    }
    get_local_device_id_checked()
}

/// 解析本机设备私钥 (优先从已解锁的 SigningKey 内存状态读取)
pub fn resolve_local_signing_key(app_opt: Option<&AppHandle>) -> Option<SigningKey> {
    if let Some(app) = app_opt {
        if let Ok(sk) = crate::crypto::ensure_device_identity_unlocked_for_app(app) {
            return Some(sk);
        }
    }
    None
}

/// 使用数据库连接执行闭包（优先使用 DbState 互斥锁，缺失时打开独立连接�?
pub fn with_db_mut<F, R>(app: &AppHandle, f: F) -> Result<R, String>
where
    F: FnOnce(&mut Connection) -> R,
{
    if let Some(db_state) = app.try_state::<crate::db::DbState>() {
        let mut guard = db_state.0.lock().map_err(|e| format!("DB lock error: {}", e))?;
        Ok(f(&mut *guard))
    } else if let Some(mut conn) = open_db_for_app(app) {
        Ok(f(&mut conn))
    } else {
        Err("Failed to open database connection".to_string())
    }
}

/// 使用 ApiState 提供的数据库连接执行闭包（优先使�?state.db 互斥锁，次�?state.app�?
pub fn with_api_state_db<F, R>(state: &ApiState, f: F) -> Result<R, String>
where
    F: FnOnce(&mut Connection) -> R,
{
    if let Some(db) = &state.db {
        let mut guard = db.lock().map_err(|e| format!("DB lock error: {}", e))?;
        return Ok(f(&mut *guard));
    }
    if let Some(app) = &state.app {
        return with_db_mut(app, f);
    }
    Err("Neither db nor app handle provided in ApiState".to_string())
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

/// 追加消息到指定会�?
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

/// 加载会话历史消息（按时间升序�?
fn load_history(conn: &Connection, conversation_id: &str) -> Vec<Value> {
    let mut stmt = match conn.prepare(
        "SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
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

/// 获取最�?N 条会�?
fn get_recent_conversations(conn: &Connection, limit: usize) -> Vec<ConversationSummary> {
    let mut stmt = match conn.prepare(
        "SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC LIMIT ?1",
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

// ══════════════════════════════════════════════════════════�?
// Handler: POST /v1/chat  �?SSE 流式对话
// ══════════════════════════════════════════════════════════�?

async fn handle_chat(
    State(state): State<ApiState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let app = match state.app.clone() {
        Some(a) => a,
        None => {
            let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(1);
            let _ = tx
                .send(Ok(Event::default()
                    .event("error")
                    .data("{\"error\":\"AppHandle not available\"}")))
                .await;
            return Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default());
        }
    };

    // ── 1. 打开数据库，决定 conversation_id ──────────────────
    let conn = match open_db(&app) {
        Some(c) => Arc::new(Mutex::new(c)),
        None => {
            // 返回一个立即完成的 SSE 错误�?
            let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(1);
            let _ = tx
                .send(Ok(Event::default()
                    .event("error")
                    .data("{\"error\":\"数据库连接失败\"}")))
                .await;
            return Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default());
        }
    };

    let conversation_id = {
        let db = conn.lock().unwrap();
        match req.conversation_id.as_deref() {
            Some(id) if !id.is_empty() => {
                // 校验会话存在
                let exists: bool = db
                    .query_row(
                        "SELECT 1 FROM conversations WHERE id = ?1",
                        params![id],
                        |_| Ok(true),
                    )
                    .unwrap_or(false);
                if exists {
                    id.to_string()
                } else {
                    // ID 不存在时新建
                    let title: String = req.message.chars().take(20).collect();
                    create_conversation(&db, &title)
                        .unwrap_or_else(|| format!("conv-{}", crate::now_ms()))
                }
            }
            _ => {
                // 新建会话，标题取消息�?20 �?
                let title: String = req.message.chars().take(20).collect();
                create_conversation(&db, &title)
                    .unwrap_or_else(|| format!("conv-{}", crate::now_ms()))
            }
        }
    };

    // ── 2. 加载历史并追加用户消�?────────────────────────────
    let messages: Vec<Value> = {
        let db = conn.lock().unwrap();
        // 先写入用户消�?
        append_message(&db, &conversation_id, "user", &req.message);
        // 再读取全量历史（含刚写入的用户消息）
        load_history(&db, &conversation_id)
    };

    let conv_id_clone = conversation_id.clone();
    let app_clone = app.clone();
    let conn_clone = conn.clone();

    // ── 3. 建立 SSE 通道 ──────────────────────────────────────
    // tx: Rust 侧写�?SSE 事件
    // rx: axum 包装�?SSE 流传给客户端
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);

    // ── 4. 后台 Task: 调用 LLM 并把 Tauri 事件桥接�?SSE ────
    tokio::spawn(async move {
        // 订阅 Tauri �?llm:chunk 事件，转发给 SSE 客户�?
        let tx_chunk = tx.clone();
        let conv_id_for_done = conv_id_clone.clone();

        // 用一个内�?mpsc 通道�?Tauri→SSE �?
        let (bridge_tx, mut bridge_rx) = mpsc::channel::<Value>(64);
        let bridge_tx = Arc::new(bridge_tx);

        // 注册 Tauri 事件监听（在 LLM 调用开始前�?
        let bridge_tx_for_listener = bridge_tx.clone();
        let listener_id = app_clone.listen("llm:chunk", move |event| {
            if let Ok(payload) = serde_json::from_str::<Value>(event.payload()) {
                let _ = bridge_tx_for_listener.try_send(payload);
            }
        });

        // 在独�?task 里把 bridge_rx 转发�?SSE tx
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
                        // LLM 完成，将完整文本�?conv_id 一起发给客户端
                        let event = Event::default().event("done").data(
                            json!({
                                "conversation_id": conv_id_for_done,
                                "full_text": full_text
                            })
                            .to_string(),
                        );
                        let _ = tx_forward.send(Ok(event)).await;
                        break;
                    }
                    _ => {}
                }
            }
            full_text
        });

        let agent_mode = req.agent_mode.unwrap_or_else(|| "default".to_string());
        // 调用 LLM（直接在此处流式返回，不需要先写库�?
        let result = crate::llm::stream_chat(
            app_clone.clone(),
            messages,
            Some(conv_id_clone.clone()),
            req.from_user.clone(),
            false,
            agent_mode,
        )
        .await;

        // 取消事件监听
        app_clone.unlisten(listener_id);

        // 等待 forward task 完成，获取完整文�?
        let full_text = forward_handle.await.unwrap_or_default();

        // ── 5. �?assistant 回复写入数据�?──────────────────
        let assistant_content = if full_text.is_empty() {
            // 如果流没收到文本（可能发生错误），用 result 中的 content 字段
            result
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            full_text
        };

        if !assistant_content.is_empty() {
            let db = conn_clone.lock().unwrap();
            append_message(&db, &conv_id_clone, "assistant", &assistant_content);
        }

        // ── 6. 广播 remote:new-message 给桌面端 UI ──────────
        let _ = app_clone.emit(
            "remote:new-message",
            json!({
                "conversation_id": conv_id_clone,
                "from_channel": req.from_channel.as_deref().unwrap_or("wechat"),
            }),
        );
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

// ══════════════════════════════════════════════════════════�?
// Handler: GET /v1/conversations �?最近会话列�?
// ══════════════════════════════════════════════════════════�?

async fn handle_get_conversations(State(state): State<ApiState>) -> impl IntoResponse {
    let list = match with_api_state_db(&state, |conn| get_recent_conversations(conn, 10)) {
        Ok(l) => l,
        Err(e) => return Json(json!({ "error": format!("数据库连接失�? {}", e) })),
    };
    let result: Vec<Value> = list
        .into_iter()
        .map(|c| json!({ "id": c.id, "title": c.title, "updated_at": c.updated_at }))
        .collect();
    Json(json!(result))
}

// ══════════════════════════════════════════════════════════�?
// Handler: GET /v1/health
// ══════════════════════════════════════════════════════════�?

async fn handle_health() -> impl IntoResponse {
    Json(
        json!({ "status": "ok", "service": "bob.agent-api", "version": env!("CARGO_PKG_VERSION") }),
    )
}

// ══════════════════════════════════════════════════════════�?
// Handler: GET /v1/file?path=...  �?本地文件服务
// ══════════════════════════════════════════════════════════�?

/// 通过 HTTP 提供本地文件，供前端 `<img>` / `<video>` 标签加载�?
/// �?Tauri 自定义协议更可靠，在 dev �?production 模式下均可用�?
async fn handle_file(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let path = match params.get("path") {
        Some(p) => p.clone(),
        None => {
            return axum::response::Response::builder()
                .status(400)
                .header("Access-Control-Allow-Origin", "*")
                .body(axum::body::Body::from("Missing 'path' query parameter"))
                .unwrap();
        }
    };

    let file_path = std::path::Path::new(&path);
    if !file_path.exists() || !file_path.is_file() {
        log::warn!("[http_api] /v1/file 404: {}", path);
        return axum::response::Response::builder()
            .status(404)
            .header("Access-Control-Allow-Origin", "*")
            .body(axum::body::Body::from("File not found"))
            .unwrap();
    }

    let mime = match file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("bmp") => "image/bmp",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    };

    match std::fs::read(file_path) {
        Ok(data) => {
            log::info!("[http_api] /v1/file 200: {} ({} bytes)", path, data.len());
            axum::response::Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "public, max-age=3600")
                .body(axum::body::Body::from(data))
                .unwrap()
        }
        Err(e) => {
            log::error!("[http_api] /v1/file 500: {} - {}", path, e);
            axum::response::Response::builder()
                .status(500)
                .header("Access-Control-Allow-Origin", "*")
                .body(axum::body::Body::from(format!("Read error: {}", e)))
                .unwrap()
        }
    }
}

// ══════════════════════════════════════════════════════════�?
// Handler: GET /v1/dl/:token  �?Token 式文件下载（大文件流式传输）
// ══════════════════════════════════════════════════════════�?

/// 通过分享 Token 下载文件�?
/// 支持 Range 请求（断点续传），以流式方式传输大文件，不会一次性加载进内存�?
async fn handle_download(
    axum::extract::Path(token): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // 1. 查找 token
    let entry = match crate::file_share::lookup_shared_file(&token) {
        Some(e) => e,
        None => {
            return axum::response::Response::builder()
                .status(404)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(axum::body::Body::from("链接无效或已过期"))
                .unwrap();
        }
    };

    // 2. 校验文件仍然存在
    let path = &entry.path;
    if !path.exists() || !path.is_file() {
        log::warn!(
            "[http_api] /v1/dl/{} 文件已被移动或删�? {:?}",
            &token[..8],
            path
        );
        return axum::response::Response::builder()
            .status(410) // Gone
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(axum::body::Body::from("File moved or deleted"))
            .unwrap();
    }

    // 3. MIME 类型推断
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("rar") => "application/x-rar-compressed",
        Some("7z") => "application/x-7z-compressed",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xls") => "application/vnd.ms-excel",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("ppt") => "application/vnd.ms-powerpoint",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("txt" | "log") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    };

    // 4. 流式读取文件
    let file_size = entry.size;
    let display_name = entry.display_name.clone();

    // 解析 Range 头（简单实现，只支�?bytes=start-end�?
    let range = headers.get("range").and_then(|v| v.to_str().ok());
    let (start, end, is_partial) = if let Some(range_str) = range {
        parse_range(range_str, file_size)
    } else {
        (0, file_size - 1, false)
    };

    let content_length = end - start + 1;

    // 打开文件�?seek 到起始位�?
    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(e) => {
            log::error!("[http_api] /v1/dl/{} open failed: {}", &token[..8], e);
            return axum::response::Response::builder()
                .status(500)
                .body(axum::body::Body::from(format!("文件读取失败: {}", e)))
                .unwrap();
        }
    };

    // 使用 tokio seek
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = file;
    if start > 0 {
        if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
            return axum::response::Response::builder()
                .status(500)
                .body(axum::body::Body::from(format!("Seek 失败: {}", e)))
                .unwrap();
        }
    }

    // 将文件包装为流式 Body�?4KB chunks�?
    let stream = async_stream::stream! {
        let mut remaining = content_length;
        let mut buf = vec![0u8; 65536]; // 64KB chunks
        loop {
            if remaining == 0 {
                break;
            }
            let to_read = std::cmp::min(remaining as usize, buf.len());
            match file.read(&mut buf[..to_read]).await {
                Ok(0) => break,
                Ok(n) => {
                    remaining -= n as u64;
                    yield Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => {
                    yield Err(e);
                    break;
                }
            }
        }
    };

    let body = axum::body::Body::from_stream(stream);

    let status = if is_partial { 206 } else { 200 };

    // URL 编码文件名（RFC 5987�?
    let encoded_name = urlencoding::encode(&display_name);

    log::info!(
        "[http_api] /v1/dl/{} {} {} bytes={}-{}/{} ({})",
        &token[..8],
        status,
        display_name,
        start,
        end,
        file_size,
        mime
    );

    let mut builder = axum::response::Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Content-Length", content_length.to_string())
        .header("Accept-Ranges", "bytes")
        .header(
            "Content-Disposition",
            format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                display_name, encoded_name
            ),
        )
        .header("Access-Control-Allow-Origin", "*");

    if is_partial {
        builder = builder.header(
            "Content-Range",
            format!("bytes {}-{}/{}", start, end, file_size),
        );
    }

    builder.body(body).unwrap()
}

/// 解析 HTTP Range 头，返回 (start, end, is_partial)
fn parse_range(range: &str, total: u64) -> (u64, u64, bool) {
    // 格式：bytes=start-end �?bytes=start- �?bytes=-suffix
    if let Some(spec) = range.strip_prefix("bytes=") {
        if let Some(dash) = spec.find('-') {
            let start_str = &spec[..dash];
            let end_str = &spec[dash + 1..];

            if start_str.is_empty() {
                // bytes=-500 �?最�?500 bytes
                if let Ok(suffix) = end_str.parse::<u64>() {
                    let start = total.saturating_sub(suffix);
                    return (start, total - 1, true);
                }
            } else if let Ok(start) = start_str.parse::<u64>() {
                let end = if end_str.is_empty() {
                    total - 1
                } else {
                    end_str.parse::<u64>().unwrap_or(total - 1).min(total - 1)
                };
                if start <= end && start < total {
                    return (start, end, true);
                }
            }
        }
    }
    (0, total - 1, false)
}

// ══════════════════════════════════════════════════════════�?
// Handler: GET /v1/sync  �?LAN Direct WebSocket Endpoint
// ══════════════════════════════════════════════════════════�?

async fn handle_sync_ws(
    State(state): State<ApiState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_authenticated_ws(socket, state, addr, headers))
}

async fn handle_authenticated_ws(
    mut socket: WebSocket,
    state: ApiState,
    addr: std::net::SocketAddr,
    headers: axum::http::HeaderMap,
) {
    use futures_util::{SinkExt, StreamExt};
    let now_ms = crate::now_ms();
    let local_id = state.test_target_id.clone().unwrap_or_else(get_local_device_id);

    // 1. 尝试从握�?Header 认证
    let header_auth = match with_api_state_db(&state, |conn| {
        verify_rest_request_auth_with_target(conn, &headers, b"", "ws_connect", &local_id, now_ms)
    }) {
        Ok(res) => res,
        Err(e) => Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e)),
    };

    let subject_device_id = match header_auth {
        Ok(crate::device_trust::AuthVerificationOutcome::Authorized { subject_device_id, .. }) => {
            Some(subject_device_id)
        }
        _ => {
            // 2. 握手 Header 未能通过时，强制在首包中等待并校验认证帧（限�?5 秒）
            let auth_res = tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(Ok(msg)) = socket.next().await {
                    if let Message::Text(text) = msg {
                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            if val.get("type").and_then(|v| v.as_str()) == Some("auth") {
                                if let Some(env_val) = val.get("envelope").or_else(|| val.get("auth_envelope")) {
                                    if let Ok(env) = serde_json::from_value::<crate::device_trust::RpcAuthEnvelope>(env_val.clone()) {
                                        if env.action != "ws_connect" && env.action != "sync" {
                                            return Err(format!("Action mismatch for websocket auth: expected ws_connect, got {}", env.action));
                                        }
                                        let target_id = state.test_target_id.clone().unwrap_or_else(get_local_device_id);
                                        let now = crate::now_ms();
                                        let verify_res = match with_api_state_db(&state, |conn| {
                                            crate::device_trust::verify_rpc_request_auth(conn, &env, b"", &target_id, now)
                                        }) {
                                            Ok(res) => res,
                                            Err(e) => Err(e),
                                        };
                                        match verify_res {
                                            Ok(crate::device_trust::AuthVerificationOutcome::Authorized { subject_device_id, .. }) => {
                                                return Ok(subject_device_id);
                                            }
                                            Ok(crate::device_trust::AuthVerificationOutcome::IdempotentCached { .. }) => {
                                                return Ok(env.subject_device_id);
                                            }
                                            Err(e) => return Err(format!("Auth verification failed: {}", e)),
                                        }
                                    }
                                }
                            }
                        }
                        return Err("First WebSocket frame must be an auth frame".to_string());
                    }
                }
                Err("Connection closed before authentication".to_string())
            }).await;

            match auth_res {
                Ok(Ok(device_id)) => Some(device_id),
                Ok(Err(e)) => {
                    log::warn!("[http_api] /v1/sync auth failed for {}: {}", addr, e);
                    let err_msg = serde_json::json!({
                        "type": "error",
                        "error": format!("Unauthorized: SEC-01 fail-closed: {}", e)
                    });
                    let _ = socket.send(Message::Text(err_msg.to_string().into())).await;
                    let _ = socket.close().await;
                    return;
                }
                Err(_) => {
                    log::warn!("[http_api] /v1/sync auth timed out for {}", addr);
                    let err_msg = serde_json::json!({
                        "type": "error",
                        "error": "Unauthorized: SEC-01 authentication timeout"
                    });
                    let _ = socket.send(Message::Text(err_msg.to_string().into())).await;
                    let _ = socket.close().await;
                    return;
                }
            }
        }
    };

    let authed_id = match subject_device_id {
        Some(id) => id,
        None => {
            let _ = socket.close().await;
            return;
        }
    };

    log::info!("[http_api] /v1/sync: Authenticated LAN WebSocket for device '{}' at {}", authed_id, addr);
    let welcome = serde_json::json!({
        "type": "auth_ok",
        "device_id": authed_id,
        "status": "authenticated"
    });
    let _ = socket.send(Message::Text(welcome.to_string().into())).await;

    let (mut sink, mut stream) = socket.split();
    while let Some(Ok(msg)) = stream.next().await {
        if let Message::Text(text) = msg {
            log::debug!("[http_api] /v1/sync received from authenticated {}: (len: {})", authed_id, text.len());
            let parsed: Result<Value, _> = serde_json::from_str(&text);
            match parsed {
                Ok(val) => {
                    let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match msg_type {
                        "ping" => {
                            let resp = serde_json::json!({
                                "type": "pong",
                                "timestamp": crate::now_ms()
                            });
                            let _ = sink.send(Message::Text(resp.to_string().into())).await;
                        }
                        "status" => {
                            let resp = serde_json::json!({
                                "type": "status_ok",
                                "device_id": authed_id,
                                "timestamp": crate::now_ms()
                            });
                            let _ = sink.send(Message::Text(resp.to_string().into())).await;
                        }
                        "sync_request" => {
                            let env_opt = val.get("envelope")
                                .or_else(|| val.get("auth_envelope"))
                                .and_then(|e| serde_json::from_value::<crate::device_trust::RpcAuthEnvelope>(e.clone()).ok());
                            if let Some(env) = env_opt {
                                // SEC-02: Connection-subject pinning (信封主体必须与已鉴权 WebSocket 连接主体严格一致)
                                if env.subject_device_id != authed_id {
                                    let resp = serde_json::json!({
                                        "type": "error",
                                        "message": format!("Unauthorized sync_request: Subject device mismatch (envelope claims '{}', but connection authenticated as '{}')", env.subject_device_id, authed_id)
                                    });
                                    let _ = sink.send(Message::Text(resp.to_string().into())).await;
                                    continue;
                                }
                                let local_target = state.test_target_id.clone().unwrap_or_else(get_local_device_id);
                                let now = crate::now_ms();
                                let raw_payload = val.get("payload")
                                    .map(|p| crate::device_trust::canonicalize_json_value(p))
                                    .unwrap_or_default();
                                let verify_res = with_api_state_db(&state, |conn| {
                                    crate::device_trust::verify_rpc_request_auth(conn, &env, &raw_payload, &local_target, now)
                                });
                                match verify_res {
                                    Ok(Ok(_outcome)) => {
                                        let resp = serde_json::json!({
                                            "type": "sync_response",
                                            "status": "ok",
                                            "request_id": env.request_id,
                                            "timestamp": now
                                        });
                                        let _ = sink.send(Message::Text(resp.to_string().into())).await;
                                    }
                                    Ok(Err(err)) => {
                                        let resp = serde_json::json!({
                                            "type": "error",
                                            "message": format!("Unauthorized sync_request: {}", err)
                                        });
                                        let _ = sink.send(Message::Text(resp.to_string().into())).await;
                                    }
                                    Err(err) => {
                                        let resp = serde_json::json!({
                                            "type": "error",
                                            "message": format!("Database error: {}", err)
                                        });
                                        let _ = sink.send(Message::Text(resp.to_string().into())).await;
                                    }
                                }
                            } else {
                                let resp = serde_json::json!({
                                    "type": "error",
                                    "message": "Missing envelope in sync_request"
                                });
                                let _ = sink.send(Message::Text(resp.to_string().into())).await;
                            }
                        }
                        _ => {
                            let resp = serde_json::json!({
                                "type": "error",
                                "message": "Unsupported frame or unauthorized action"
                            });
                            let _ = sink.send(Message::Text(resp.to_string().into())).await;
                        }
                    }
                }
                Err(_) => {
                    let resp = serde_json::json!({
                        "type": "error",
                        "message": "Malformed JSON frame"
                    });
                    let _ = sink.send(Message::Text(resp.to_string().into())).await;
                }
            }
        }
    }
    log::info!("[http_api] /v1/sync: Authenticated device '{}' disconnected.", authed_id);
}

// ════════════════════════════════════════════════════════════
// REST Sync Endpoints for Mobile Phase 3 (SEC-01 Cryptographic Integration)
// ════════════════════════════════════════════════════════════

pub fn verify_rest_request_auth(
    conn: &mut Connection,
    headers: &axum::http::HeaderMap,
    body_bytes: &[u8],
    expected_action: &str,
    now_ms: i64,
) -> Result<crate::device_trust::AuthVerificationOutcome, (axum::http::StatusCode, String)> {
    let local_id = get_local_device_id();
    verify_rest_request_auth_with_target(conn, headers, body_bytes, expected_action, &local_id, now_ms)
}

pub fn verify_rest_request_auth_with_target(
    conn: &mut Connection,
    headers: &axum::http::HeaderMap,
    body_bytes: &[u8],
    expected_action: &str,
    expected_target_id: &str,
    now_ms: i64,
) -> Result<crate::device_trust::AuthVerificationOutcome, (axum::http::StatusCode, String)> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    // 1. 尝试�?Header 中解�?RpcAuthEnvelope
    let mut envelope_opt: Option<crate::device_trust::RpcAuthEnvelope> = None;
    let header_val = headers
        .get("x-rpc-auth-envelope")
        .or_else(|| headers.get("x-auth-envelope"))
        .or_else(|| headers.get("authorization"))
        .and_then(|v| v.to_str().ok());

    if let Some(mut h_str) = header_val {
        if let Some(stripped) = h_str.strip_prefix("Bearer ") {
            h_str = stripped.trim();
        }
        if h_str.starts_with('{') {
            if let Ok(env) = serde_json::from_str::<crate::device_trust::RpcAuthEnvelope>(h_str) {
                envelope_opt = Some(env);
            }
        } else if let Ok(decoded) = BASE64.decode(h_str.trim().as_bytes()) {
            if let Ok(env) = serde_json::from_slice::<crate::device_trust::RpcAuthEnvelope>(&decoded) {
                envelope_opt = Some(env);
            }
        }
    }

    let is_from_header = envelope_opt.is_some();

    // 2. �?Header 未携带，尝试�?JSON Body 中提�?
    if envelope_opt.is_none() && !body_bytes.is_empty() {
        if let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(body_bytes) {
            if let Some(env_val) = map.get("auth_envelope").or_else(|| map.get("envelope")) {
                if let Ok(env) = serde_json::from_value::<crate::device_trust::RpcAuthEnvelope>(env_val.clone()) {
                    envelope_opt = Some(env);
                }
            }
        }
    }

    let envelope = match envelope_opt {
        Some(env) => env,
        None => {
            return Err((
                axum::http::StatusCode::UNAUTHORIZED,
                "Unauthorized: Missing cryptographic RPC authentication envelope (SEC-01 fail-closed)".to_string(),
            ));
        }
    };

    // 3. 校验调用者身份声明一致�?(防伪�?X-Device-Id: 必须与信封签名主体一�?
    if let Some(claimed_dev) = headers.get("x-device-id").and_then(|v| v.to_str().ok()) {
        if claimed_dev.trim() != envelope.subject_device_id {
            return Err((
                axum::http::StatusCode::FORBIDDEN,
                format!(
                    "Forbidden: Forged device ID detected. X-Device-Id '{}' does not match envelope subject '{}'",
                    claimed_dev.trim(), envelope.subject_device_id
                ),
            ));
        }
    }

    // 4. 校验动作绑定 (防止�?Action 挪用)
    if envelope.action != expected_action {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            format!(
                "Forbidden: Action mismatch (expected '{}', envelope signed for '{}')",
                expected_action, envelope.action
            ),
        ));
    }

    // 5. 校验目标设备绑定 (防止跨目标重�?
    if !expected_target_id.is_empty() && envelope.target_device_id != expected_target_id {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            format!(
                "Forbidden: Target device ID mismatch (expected '{}', got '{}')",
                expected_target_id, envelope.target_device_id
            ),
        ));
    }

    // 6. 确定参与验签的有效载荷字�?
    let payload_bytes: Vec<u8> = if is_from_header {
        if body_bytes.is_empty() {
            if envelope.payload_hash == crate::device_trust::compute_sha512(b"") {
                b"".to_vec()
            } else {
                b"{}".to_vec()
            }
        } else {
            body_bytes.to_vec()
        }
    } else {
        // Envelope 来自 Body，提取除�?envelope 字段后的载荷
        if let Ok(mut val) = serde_json::from_slice::<Value>(body_bytes) {
            if let Some(obj) = val.as_object_mut() {
                obj.remove("auth_envelope");
                obj.remove("envelope");
                serde_json::to_vec(&val).unwrap_or_default()
            } else {
                body_bytes.to_vec()
            }
        } else {
            body_bytes.to_vec()
        }
    };

    // 7. 密码学全要素校验（会话、签名、重放、撤销�?
    let target_check_id = if expected_target_id.is_empty() {
        &envelope.target_device_id
    } else {
        expected_target_id
    };

    match crate::device_trust::verify_rpc_request_auth(
        conn,
        &envelope,
        &payload_bytes,
        target_check_id,
        now_ms,
    ) {
        Ok(outcome) => Ok(outcome),
        Err(err) => {
            let status = if err.contains("Idempotency conflict") {
                axum::http::StatusCode::CONFLICT
            } else {
                axum::http::StatusCode::UNAUTHORIZED
            };
            Err((
                status,
                format!("Unauthorized: SEC-01 verification failed: {}", err),
            ))
        }
    }
}

async fn handle_sync_pull(
    axum::extract::State(state): axum::extract::State<ApiState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let now_ms = crate::now_ms();
    let local_id = state.test_target_id.clone().unwrap_or_else(get_local_device_id);
    let auth_outcome = match with_api_state_db(&state, |conn| {
        verify_rest_request_auth_with_target(conn, &headers, b"", "pull", &local_id, now_ms)
    }) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err((status, msg))) => {
            log::warn!("[http_api] Rejected unauthorized LAN sync pull from {}: {}", addr, msg);
            return (
                status,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": msg
                })),
            ).into_response();
        }
        Err(e) => {
            log::error!("[http_api] Database error during sync pull auth from {}: {}", addr, e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": e
                })),
            ).into_response();
        }
    };

    let (subject_device_id, session_id, request_id, _target_device_id, _action, _payload_hash, execution_token) = match auth_outcome {
        crate::device_trust::AuthVerificationOutcome::IdempotentCached { cached_response } => {
            log::info!("[http_api] Returning idempotent cached sync pull response for {}", addr);
            return (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::from_str::<Value>(&cached_response).unwrap_or_else(|_| {
                    serde_json::json!({ "status": "ok", "cached": cached_response })
                })),
            ).into_response();
        }
        crate::device_trust::AuthVerificationOutcome::Authorized {
            subject_device_id,
            session_id,
            request_id,
            target_device_id,
            action,
            payload_hash,
            execution_token,
            ..
        } => (subject_device_id, session_id, request_id, target_device_id, action, payload_hash, execution_token),
    };

    let platform = headers.get("x-platform").and_then(|v| v.to_str().ok()).unwrap_or("mobile");
    let device_name = headers.get("x-device-name").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    if let Some(app) = &state.app {
        if let Err(e) = crate::sync_engine::register_authenticated_device(app, &subject_device_id, platform, device_name, addr) {
            log::error!("[http_api] register_authenticated_device failed: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("SEC-01 Fail-Closed: Device registration failed: {}", e)
                })),
            ).into_response();
        }
    }

    // Export full sync schema (config + SQLite rows)
    let sync_data_res: Result<crate::sync_engine::SyncData, String> = if let Some(app) = &state.app {
        crate::sync_engine::export_sync_data(app, 0, false)
    } else if let Some(db) = &state.db {
        match db.lock() {
            Ok(conn) => crate::sync_engine::export_sync_data_from_conn(&conn, 0, false),
            Err(e) => Err(format!("Failed to lock test db: {}", e)),
        }
    } else {
        Err("No database available".to_string())
    };

    let sync_data = match sync_data_res {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to export sync data: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": e
                })),
            ).into_response();
        }
    };

    let resp_payload = serde_json::json!({
        "status": "ok",
        "data": sync_data,
        "timestamp": chrono::Utc::now().timestamp()
    });
    let resp_str = resp_payload.to_string();

    // 生产真实落盘幂等缓存（带 execution_token 强校验）
    let commit_res = with_api_state_db(&state, |conn| {
        crate::device_trust::complete_rpc_idempotency(
            conn,
            &session_id,
            &request_id,
            &execution_token,
            &resp_str,
            crate::now_ms(),
        )
    }).and_then(|r| r);
    if let Err(e) = commit_res {
        log::error!("[http_api] Failed to commit idempotency cache in sync_pull: {}", e);
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to commit idempotency cache: {}", e)
            })),
        ).into_response();
    }

    (
        axum::http::StatusCode::OK,
        axum::Json(resp_payload),
    ).into_response()
}

async fn handle_sync_push(
    axum::extract::State(state): axum::extract::State<ApiState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let now_ms = crate::now_ms();
    let local_id = state.test_target_id.clone().unwrap_or_else(get_local_device_id);
    let auth_outcome = match with_api_state_db(&state, |conn| {
        verify_rest_request_auth_with_target(conn, &headers, &body, "push", &local_id, now_ms)
    }) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err((status, msg))) => {
            log::warn!("[http_api] Rejected unauthorized LAN sync push from {}: {}", addr, msg);
            return (
                status,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": msg
                })),
            ).into_response();
        }
        Err(e) => {
            log::error!("[http_api] Database error during sync push auth from {}: {}", addr, e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": e
                })),
            ).into_response();
        }
    };

    let (subject_device_id, session_id, request_id, _target_device_id, _action, _payload_hash, execution_token) = match auth_outcome {
        crate::device_trust::AuthVerificationOutcome::IdempotentCached { cached_response } => {
            log::info!("[http_api] Returning idempotent cached sync push response for {}", addr);
            return (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::from_str::<Value>(&cached_response).unwrap_or_else(|_| {
                    serde_json::json!({ "status": "ok" })
                })),
            ).into_response();
        }
        crate::device_trust::AuthVerificationOutcome::Authorized {
            subject_device_id,
            session_id,
            request_id,
            target_device_id,
            action,
            payload_hash,
            execution_token,
            ..
        } => (subject_device_id, session_id, request_id, target_device_id, action, payload_hash, execution_token),
    };

    let platform = headers.get("x-platform").and_then(|v| v.to_str().ok()).unwrap_or("mobile");
    let device_name = headers.get("x-device-name").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    if let Some(app) = &state.app {
        if let Err(e) = crate::sync_engine::register_authenticated_device(app, &subject_device_id, platform, device_name, addr) {
            log::error!("[http_api] register_authenticated_device failed in handle_sync_push: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("SEC-01 Fail-Closed: Device registration failed: {}", e)
                })),
            ).into_response();
        }
    }

    // 严格解析 push 操作列表：支持顶层数组，或显式包含 ops/data 数组的包装对象 (SEC-03 Fail-Closed)
    let ops: Vec<Value> = if let Ok(arr) = serde_json::from_slice::<Vec<Value>>(&body) {
        arr
    } else if let Ok(val) = serde_json::from_slice::<Value>(&body) {
        if let Some(arr) = val.get("ops").and_then(|v| v.as_array()) {
            arr.clone()
        } else if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
            arr.clone()
        } else {
            // 合法 JSON 但格式不符（既不是数组，也没有 ops/data 数组）
            log::warn!("[http_api] Rejected push with unrecognized JSON payload structure from {}", subject_device_id);
            if let Err(fail_err) = with_api_state_db(&state, |conn| {
                crate::device_trust::fail_rpc_idempotency(
                    conn,
                    &session_id,
                    &request_id,
                    &execution_token,
                    "SEC-03 Fail-Closed: 无法识别的 push 载荷格式，必须为操作数组或包含 ops/data 数组的对象",
                    crate::now_ms(),
                )
            }).and_then(|r| r) {
                log::warn!("[http_api] Failed to mark idempotency record failed: {}", fail_err);
            }
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": "SEC-03 Fail-Closed: 无法识别的 push 载荷格式，必须为操作数组或包含 ops/data 数组的对象"
                })),
            ).into_response();
        }
    } else {
        // 非法 JSON 格式
        log::warn!("[http_api] Rejected push with malformed JSON from {}", subject_device_id);
        if let Err(fail_err) = with_api_state_db(&state, |conn| {
            crate::device_trust::fail_rpc_idempotency(
                conn,
                &session_id,
                &request_id,
                &execution_token,
                "SEC-03 Fail-Closed: 载荷不是合法的 JSON 数据",
                crate::now_ms(),
            )
        }).and_then(|r| r) {
            log::warn!("[http_api] Failed to mark idempotency record failed: {}", fail_err);
        }
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "status": "error",
                "message": "SEC-03 Fail-Closed: 载荷不是合法的 JSON 数据"
            })),
        ).into_response();
    };

    log::info!("[http_api] Pushing {} operations from {} to PC outbox atomically", ops.len(), subject_device_id);

    let initial_resp_payload = serde_json::json!({
        "status": "ok",
        "type": "commit_ack"
    });
    let initial_resp_str = initial_resp_payload.to_string();

    // 生产真实原子落盘：同一事务内 Fencing 校验、状态更新为 completed、暂存 outbox 并执行投递
    let commit_res = with_api_state_db(&state, |conn| {
        crate::device_trust::atomic_commit_push_outbox(
            conn,
            &session_id,
            &request_id,
            &execution_token,
            &ops,
            &initial_resp_str,
            crate::now_ms(),
        )
    }).and_then(|r| r);

    let receipt = match commit_res {
        Ok(r) => r,
        Err(e) => {
            log::error!("[http_api] Failed to commit atomic push outbox in sync_push: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Failed to commit idempotency cache: {}", e)
                })),
            ).into_response();
        }
    };

    (
        axum::http::StatusCode::OK,
        axum::Json(receipt),
    ).into_response()
}

async fn handle_sync_push_db(
    axum::extract::State(state): axum::extract::State<ApiState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let now_ms = crate::now_ms();
    let local_id = state.test_target_id.clone().unwrap_or_else(get_local_device_id);
    let auth_outcome = match with_api_state_db(&state, |conn| {
        verify_rest_request_auth_with_target(conn, &headers, &body, "push_db", &local_id, now_ms)
    }) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err((status, msg))) => {
            log::warn!("[http_api] Rejected unauthorized LAN sync push_db from {}: {}", addr, msg);
            return (
                status,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": msg
                })),
            ).into_response();
        }
        Err(e) => {
            log::error!("[http_api] Database error during sync push_db auth from {}: {}", addr, e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": e
                })),
            ).into_response();
        }
    };

    let (subject_device_id, session_id, request_id, _target_device_id, _action, _payload_hash, execution_token) = match auth_outcome {
        crate::device_trust::AuthVerificationOutcome::IdempotentCached { cached_response } => {
            log::info!("[http_api] Returning idempotent cached sync push_db response for {}", addr);
            return (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::from_str::<Value>(&cached_response).unwrap_or_else(|_| {
                    serde_json::json!({ "status": "ok" })
                })),
            ).into_response();
        }
        crate::device_trust::AuthVerificationOutcome::Authorized {
            subject_device_id,
            session_id,
            request_id,
            target_device_id,
            action,
            payload_hash,
            execution_token,
            ..
        } => (subject_device_id, session_id, request_id, target_device_id, action, payload_hash, execution_token),
    };

    let platform = headers.get("x-platform").and_then(|v| v.to_str().ok()).unwrap_or("mobile");
    let device_name = headers.get("x-device-name").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    if let Some(app) = &state.app {
        if let Err(e) = crate::sync_engine::register_authenticated_device(app, &subject_device_id, platform, device_name, addr) {
            log::error!("[http_api] register_authenticated_device failed in handle_sync_push_db: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("SEC-01 Fail-Closed: Device registration failed: {}", e)
                })),
            ).into_response();
        }
    }

    let sync_data: crate::sync_engine::SyncData = match serde_json::from_slice(&body) {
        Ok(d) => d,
        Err(e) => {
            log::error!("[http_api] Malformed SyncData from {}: {}", subject_device_id, e);
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Malformed SyncData: {}", e)
                })),
            ).into_response();
        }
    };

    log::info!("[http_api] Importing mobile push_db data from authenticated peer {}", subject_device_id);
    let resp_payload = serde_json::json!({
        "status": "ok",
        "type": "commit_ack"
    });
    let resp_str = resp_payload.to_string();
    let now_ms = crate::now_ms();

    let idempotency_info = crate::sync_engine::IdempotencyCommitInfo {
        session_id: &session_id,
        request_id: &request_id,
        execution_token: &execution_token,
        response_json: &resp_str,
        now_ms,
    };

    let import_res = with_api_state_db(&state, |conn| {
        crate::sync_engine::import_sync_data_to_conn_atomic(
            conn,
            &sync_data,
            0,
            Some(idempotency_info),
        )
    }).and_then(|r| r);

    match import_res {
        Ok(outcome) => {
            (
                axum::http::StatusCode::OK,
                axum::Json(outcome.receipt),
            ).into_response()
        }
        Err(e) => {
            log::error!("[http_api] Failed to import mobile push_db data: {}", e);
            if let Err(fail_err) = with_api_state_db(&state, |conn| {
                crate::device_trust::fail_rpc_idempotency(
                    conn,
                    &session_id,
                    &request_id,
                    &execution_token,
                    &e,
                    crate::now_ms(),
                )
            }).and_then(|r| r) {
                log::warn!("[http_api] Failed to mark idempotency record failed: {}", fail_err);
            }
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "status": "error", "message": e })),
            ).into_response()
        }
    }
}

async fn handle_lan_rpc(
    axum::extract::State(state): axum::extract::State<ApiState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let now_ms = crate::now_ms();
    let local_id = state.test_target_id.clone().unwrap_or_else(get_local_device_id);

    // 1. 解析请求体 JSON
    let body_json: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Invalid JSON body: {}", e)
                })),
            ).into_response();
        }
    };

    let action = match body_json.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": "Missing 'action' field in body"
                })),
            ).into_response();
        }
    };

    // 2. SEC-01 签名与凭证校验 (verify_rest_request_auth_with_target)
    let auth_outcome = match with_api_state_db(&state, |conn| {
        verify_rest_request_auth_with_target(conn, &headers, &body, action, &local_id, now_ms)
    }) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err((status, msg))) => {
            log::warn!("[http_api] Rejected unauthorized LAN RPC ({}) from {}: {}", action, addr, msg);
            return (
                status,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": msg
                })),
            ).into_response();
        }
        Err(e) => {
            log::error!("[http_api] Database error during LAN RPC auth from {}: {}", addr, e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": e
                })),
            ).into_response();
        }
    };

    // 3. 处理幂等缓存
    let (subject_device_id, session_id, request_id, execution_token) = match auth_outcome {
        crate::device_trust::AuthVerificationOutcome::IdempotentCached { cached_response } => {
            log::info!("[http_api] Returning idempotent cached LAN RPC response for {}", addr);
            return (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::from_str::<serde_json::Value>(&cached_response).unwrap_or_else(|_| {
                    serde_json::json!({ "status": "ok" })
                })),
            ).into_response();
        }
        crate::device_trust::AuthVerificationOutcome::Authorized {
            subject_device_id,
            session_id,
            request_id,
            execution_token,
            ..
        } => (subject_device_id, session_id, request_id, execution_token),
    };

    // 4. 自动注册设备活跃 IP 与传输通道
    let platform = headers.get("x-platform").and_then(|v| v.to_str().ok()).unwrap_or("mobile");
    let device_name = headers.get("x-device-name").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    if let Some(app) = &state.app {
        let _ = crate::sync_engine::register_authenticated_device(app, &subject_device_id, platform, device_name, addr);
        crate::sync_engine::update_device_last_transport(app, &subject_device_id, &addr.ip().to_string(), "lan");
    }

    // 5. 根据 action 分发执行业务
    let exec_result: Result<serde_json::Value, String> = match action {
        "rpc_request" => {
            if let Some(app) = &state.app {
                crate::sync_engine::execute_rpc_instruction_core(app, &subject_device_id, &body_json).await
            } else {
                Err("AppHandle unavailable for RPC execution".to_string())
            }
        }
        "rpc_approval" | "rpc_approval_decision" => {
            let change_id = body_json.get("change_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let decision = body_json.get("decision").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let from_id = subject_device_id.clone();
            let outcome_res = with_api_state_db(&state, |conn| {
                crate::sync_engine::process_approval_decision_core(
                    conn,
                    &change_id,
                    &decision,
                    &format!("remote:{}", from_id),
                )
            });
            match outcome_res {
                Ok(outcome_val) => {
                    if outcome_val.get("status").and_then(|s| s.as_str()) == Some("applied") {
                        let _ = crate::sync_history::record_activity(
                            crate::sync_protocol::DiagnosticStatus::Success,
                            Some(crate::sync_protocol::TransportKind::Lan),
                            Some(from_id),
                            &format!("LAN直连已批准并真实应用修改: {}", change_id),
                            Some("RPC-APPLY-OK".to_string()),
                        );
                    } else if outcome_val.get("status").and_then(|s| s.as_str()) == Some("rejected") {
                        let _ = crate::sync_history::record_activity(
                            crate::sync_protocol::DiagnosticStatus::Skipped,
                            Some(crate::sync_protocol::TransportKind::Lan),
                            Some(from_id),
                            &format!("LAN直连已拒绝修改提案: {}", change_id),
                            Some("RPC-REJECT".to_string()),
                        );
                    }
                    Ok(serde_json::json!({
                        "action": "rpc_response",
                        "request_id": request_id,
                        "outcome": outcome_val
                    }))
                }
                Err(e) => Err(e),
            }
        }
        "rpc_cancel" => {
            let cancel_req_id = body_json.get("request_id").and_then(|v| v.as_str()).unwrap_or(&request_id).to_string();
            let (_cancel_outcome, is_confirmed, cancel_status, cancel_error_msg) =
                crate::sync_engine::cancel_active_rpc_task_core(&cancel_req_id, tokio::time::Duration::from_secs(5)).await;

            if is_confirmed {
                let _ = with_api_state_db(&state, |conn| {
                    if let Ok(cancelled) = crate::sync_engine::cancel_staged_changes_by_request(conn, &cancel_req_id) {
                        if let Ok(mut staged) = crate::sync_engine::STAGED_CHANGES.lock() {
                            for c in &cancelled {
                                staged.remove(&c.change_id);
                            }
                        }
                    }
                    let pid = crate::work_core::repository::ensure_personal_workspace(conn).unwrap_or_else(|_| "project_personal_inbox".to_string());
                    let _ = crate::work_core::repository::record_work_event(
                        conn,
                        &pid,
                        None,
                        "remote.task.cancelled",
                        &format!("remote:{}", subject_device_id),
                        &serde_json::json!({
                            "requestId": cancel_req_id,
                            "fromDevice": subject_device_id,
                            "reason": "User cancelled from mobile"
                        }),
                        Some(&format!("event_rpc_cancel_{}", cancel_req_id)),
                    );
                    if let Ok(agg) = crate::work_core::repository::get_project_aggregate(conn, &pid) {
                        let _ = crate::work_core::snapshot::write_project_snapshot(&agg);
                    }
                    Ok::<(), String>(())
                });
                let _ = crate::sync_history::record_activity(
                    crate::sync_protocol::DiagnosticStatus::Skipped,
                    Some(crate::sync_protocol::TransportKind::Lan),
                    Some(subject_device_id.clone()),
                    &format!("LAN直连任务已被移动端取消: {}", cancel_req_id),
                    Some("RPC-CANCEL-ACK".to_string()),
                );
            }

            let mut payload_data = serde_json::json!({
                "action": "rpc_cancel_ack",
                "request_id": cancel_req_id,
                "status": cancel_status,
                "confirmed": is_confirmed
            });
            if let Some(err) = cancel_error_msg {
                payload_data["error"] = serde_json::Value::String(err);
            }
            Ok(payload_data)
        }
        "rpc_discover_capabilities" => {
            if let Some(app) = &state.app {
                let snapshot = crate::capability::CapabilitySnapshot::capture(app, false, true);
                let (safe_models, default_model) = crate::capability::get_safe_model_pool_for_remote();
                match crate::read_config_checked() {
                    Ok(config) => {
                        let device_name = config.get("device_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("本机")
                            .to_string();
                        if let Some(local_device_id) = config.get("device_id")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.trim().is_empty())
                        {
                            Ok(serde_json::json!({
                                "action": "rpc_capabilities_response",
                                "request_id": request_id,
                                "status": "success",
                                "device_id": local_device_id,
                                "device_name": device_name,
                                "platform": snapshot.platform,
                                "file_scope": snapshot.file_scope,
                                "capabilities": snapshot.capabilities,
                                "available_models": safe_models,
                                "default_model": default_model,
                                "timestamp": crate::now_ms()
                            }))
                        } else {
                            Err("SEC-01 Fail-Closed: 本机缺少 device_id".to_string())
                        }
                    }
                    Err(e) => Err(format!("SEC-01 Fail-Closed: 无法读取配置: {}", e)),
                }
            } else {
                Err("AppHandle unavailable for capabilities discovery".to_string())
            }
        }
        other => Err(format!("Unsupported RPC action: {}", other)),
    };

    // 6. 依据执行结果提交幂等缓存状态并返回 HTTP 响应
    match exec_result {
        Ok(resp_payload) => {
            let resp_str = resp_payload.to_string();
            let _ = with_api_state_db(&state, |conn| {
                crate::device_trust::complete_rpc_idempotency(
                    conn,
                    &session_id,
                    &request_id,
                    &execution_token,
                    &resp_str,
                    crate::now_ms(),
                )
            });
            (axum::http::StatusCode::OK, axum::Json(resp_payload)).into_response()
        }
        Err(err_msg) => {
            let err_payload = serde_json::json!({
                "action": "rpc_response",
                "request_id": request_id,
                "status": "error",
                "error": err_msg
            });
            let err_str = err_payload.to_string();
            let _ = with_api_state_db(&state, |conn| {
                crate::device_trust::complete_rpc_idempotency(
                    conn,
                    &session_id,
                    &request_id,
                    &execution_token,
                    &err_str,
                    crate::now_ms(),
                )
            });
            (axum::http::StatusCode::OK, axum::Json(err_payload)).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PairRequestBody {
    Wrapped { pop: crate::device_trust::ProofOfPossession },
    Direct(crate::device_trust::ProofOfPossession),
}

async fn handle_pair(
    axum::extract::State(state): axum::extract::State<ApiState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    axum::extract::Json(body): axum::extract::Json<PairRequestBody>,
) -> impl IntoResponse {
    let pop = match body {
        PairRequestBody::Wrapped { pop } => pop,
        PairRequestBody::Direct(pop) => pop,
    };
    let now_ms = crate::now_ms();
    let local_id = state.test_target_id.clone().unwrap_or_else(|| {
        resolve_local_device_id(state.app.as_ref())
    });

    if local_id.trim().is_empty() {
        log::warn!("[http_api] Rejected pair request from {}: Local device identity is uninitialized", addr);
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "status": "error",
                "message": "Unauthorized: Local device identity is uninitialized"
            })),
        ).into_response();
    }

    // SEC-01: 校验建信目的必须�?"pairing_establishment"
    if pop.purpose != crate::device_trust::SEC01_POP_PURPOSE {
        log::warn!("[http_api] Rejected pair request with invalid purpose '{}' from {}", pop.purpose, addr);
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "status": "error",
                "message": format!("Unauthorized: Invalid pairing purpose '{}'", pop.purpose)
            })),
        ).into_response();
    }

    // 原子核销配对邀请、落盘可信设备并建立会话 (单一 SQLite 事务，Fail-Closed)
    let atomic_res = with_api_state_db(&state, |conn| {
        crate::device_trust::sec01_atomic_consume_and_create_session(
            conn,
            &pop,
            &local_id,
            crate::device_trust::DEFAULT_SESSION_TTL_MS,
            now_ms,
        )
    });

    let (trusted_device, session) = match atomic_res {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => {
            log::warn!("[http_api] Pairing verification failed from {}: {}", addr, e);
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Unauthorized: Pairing verification failed: {}", e)
                })),
            ).into_response();
        }
        Err(e) => {
            log::error!("[http_api] Database error during pair from {}: {}", addr, e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": e
                })),
            ).into_response();
        }
    };

    // 注册到在线设备名册
    if let Some(app) = &state.app {
        if let Err(e) = crate::sync_engine::register_authenticated_device(app, &trusted_device.device_id, &trusted_device.platform, Some(trusted_device.device_name.clone()), addr) {
            log::error!("[http_api] register_authenticated_device failed during LAN pair: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("SEC-01 Fail-Closed: LAN pair device registration failed: {}", e)
                })),
            ).into_response();
        }
    }

    log::info!("[http_api] SEC-01 LAN Pair established with {} ({})", trusted_device.device_id, trusted_device.device_name);

    (
        axum::http::StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "trusted",
            "device_id": local_id,
            "session_id": session.session_id,
            "device_name": trusted_device.device_name,
        })),
    ).into_response()
}

async fn handle_device_revoke(
    axum::extract::State(state): axum::extract::State<ApiState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    axum::extract::Json(cert): axum::extract::Json<crate::device_trust::DeviceRevocationCertificate>,
) -> impl IntoResponse {
    let now_ms = crate::now_ms();
    log::info!(
        "[http_api] Received authenticated device revocation from {}: revoked_id={}, target_id={}, event_id={}",
        addr, cert.revoked_device_id, cert.target_device_id, cert.event_id
    );

    let local_signing_key = state.test_signing_key.clone().or_else(|| {
        resolve_local_signing_key(state.app.as_ref())
    });
    let local_sk = match local_signing_key {
        Some(sk) => sk,
        None => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "error": "Local device private key is locked or uninitialized; cannot issue signed CommitAck"
                })),
            ).into_response();
        }
    };

    let local_device_id = state.test_target_id.clone().unwrap_or_else(|| {
        let vk = ed25519_dalek::VerifyingKey::from(&local_sk);
        base64::engine::general_purpose::STANDARD.encode(vk.to_bytes())
    });
    if local_device_id.trim().is_empty() {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "status": "error",
                "error": "Local device identity uninitialized or empty"
            })),
        ).into_response();
    }

    let res = with_api_state_db(&state, |conn| {
        crate::device_trust::sec01_verify_and_apply_peer_revocation(conn, &cert, &local_device_id, &local_sk, now_ms)
    });

    match res {
        Ok(Ok(ack)) => {
            (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::to_value(ack).unwrap_or(serde_json::json!({ "status": "committed" }))),
            ).into_response()
        }
        Ok(Err(err_msg)) => {
            log::warn!("[http_api] Revocation verification rejected: {}", err_msg);
            let status = if err_msg.contains("不匹配") || err_msg.contains("target") {
                axum::http::StatusCode::FORBIDDEN
            } else if err_msg.contains("签名") || err_msg.contains("signature") || err_msg.contains("Nonce") {
                axum::http::StatusCode::UNAUTHORIZED
            } else {
                axum::http::StatusCode::BAD_REQUEST
            };
            (
                status,
                axum::Json(serde_json::json!({
                    "status": "rejected",
                    "error": err_msg
                })),
            ).into_response()
        }
        Err(e) => {
            log::error!("[http_api] Database state error during revocation: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "error": format!("Database state unavailable: {}", e)
                })),
            ).into_response()
        }
    }
}

// ══════════════════════════════════════════════════════════?
// 路由组装 & 服务启动
// ══════════════════════════════════════════════════════════?

pub fn create_router(app: AppHandle) -> Router {
    let state = ApiState {
        app: Some(app),
        db: None,
        test_target_id: None,
        test_signing_key: None,
    };
    Router::new()
        .route("/v1/chat", post(handle_chat))
        .route("/v1/conversations", get(handle_get_conversations))
        .route("/v1/health", get(handle_health))
        .route("/v1/file", get(handle_file))
        .route("/v1/dl/{token}", get(handle_download))
        .with_state(state)
}

/// 在后�?Task 中启�?HTTP 服务，绑�?127.0.0.1:3721
///
/// 使用 socket2 创建不可继承�?TCP socket，防�?WebView2 / MCP 等子进程
/// 继承 socket handle 导致端口在主进程退出后仍被幽灵占用�?
pub fn start_http_server(app: AppHandle) {
    let router = create_router(app.clone());
    tauri::async_runtime::spawn(async move {
        let listener = match create_non_inheritable_listener("127.0.0.1:3721") {
            Ok(l) => l,
            Err(e) => {
                log::error!("[http_api] 无法绑定 127.0.0.1:3721: {}", e);
                return;
            }
        };
        log::info!("[http_api] Bob HTTP API 启动成功，监�?127.0.0.1:3721 (non-inheritable)");
        if let Err(e) = axum::serve(listener, router).await {
            log::error!("[http_api] 服务异常退�? {}", e);
        }
    });

    // 启动一个专门用于外网下载的 0.0.0.0:3722 服务，仅暴露下载路由和局域网同步路由
    let public_router = create_public_router(app.clone());

    tauri::async_runtime::spawn(async move {
        let listener = match create_non_inheritable_listener("0.0.0.0:3722") {
            Ok(l) => l,
            Err(e) => {
                log::error!("[http_api] 无法绑定 0.0.0.0:3722: {}", e);
                return;
            }
        };
        log::info!("[http_api] Bob Public Download API 启动成功，监?0.0.0.0:3722");
        if let Err(e) = axum::serve(listener, public_router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await {
            log::error!("[http_api] 公共服务异常退? {}", e);
        }
    });
}

pub fn create_public_router(app: AppHandle) -> Router {
    create_public_router_with_state(ApiState {
        app: Some(app),
        db: None,
        test_target_id: None,
        test_signing_key: None,
    })
}

pub fn create_public_router_with_state(public_state: ApiState) -> Router {
    Router::new()
        .route("/v1/health", get(handle_health))
        .route("/v1/dl/{token}", get(handle_download))
        .route("/v1/pair", post(handle_pair))
        .route("/v1/device/revoke", post(handle_device_revoke))
        .route("/v1/sync", get(handle_sync_ws))
        .route("/v1/sync/pull", get(handle_sync_pull))
        .route("/v1/sync/push", post(handle_sync_push))
        .route("/v1/sync/push_db", post(handle_sync_push_db))
        .route("/v1/rpc", post(handle_lan_rpc))
        .with_state(public_state)
}

/// 使用 socket2 创建一个不可继承的 TCP 监听器�?
/// �?Windows 上，这会通过 SetHandleInformation 清除 HANDLE_FLAG_INHERIT�?
/// 确保子进程（WebView2、MCP node.exe 等）不会继承�?socket handle�?
fn create_non_inheritable_listener(addr: &str) -> Result<tokio::net::TcpListener, String> {
    use socket2::{Domain, Protocol, Socket, Type};

    let addr: std::net::SocketAddr = addr.parse().map_err(|e| format!("地址解析失败: {}", e))?;

    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| format!("创建 socket 失败: {}", e))?;

    // 关键：设�?socket 为不可继承（Windows 上清�?HANDLE_FLAG_INHERIT�?
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::io::AsRawSocket;
        unsafe {
            // SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) �?清除继承标志
            windows_sys::Win32::Foundation::SetHandleInformation(
                socket.as_raw_socket() as _,
                windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT,
                0,
            );
        }
    }

    socket
        .set_reuse_address(true)
        .map_err(|e| format!("SO_REUSEADDR 失败: {}", e))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| format!("非阻塞设置失�? {}", e))?;
    socket
        .bind(&addr.into())
        .map_err(|e| format!("绑定失败: {}", e))?;
    socket
        .listen(128)
        .map_err(|e| format!("listen 失败: {}", e))?;

    let std_listener: std::net::TcpListener = socket.into();
    tokio::net::TcpListener::from_std(std_listener)
        .map_err(|e| format!("转换�?tokio listener 失败: {}", e))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use crate::device_trust::tests::{generate_test_keypair, setup_test_db};
    use crate::device_trust::*;
    use ed25519_dalek::SigningKey;
    use std::sync::Arc;
    use axum::extract::{ConnectInfo, State};
    use axum::response::IntoResponse;
    use std::path::{Path, PathBuf};
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::fs;

    fn empty_sync_data() -> crate::sync_engine::SyncData {
        crate::sync_engine::SyncData {
            config: serde_json::json!({}),
            settings: vec![],
            conversations: vec![],
            messages: vec![],
            events: vec![],
            captures: vec![],
            cron_jobs: vec![],
            kg_nodes: vec![],
            kg_edges: vec![],
            wiki_fts: vec![],
            tombstones: vec![],
            notes: vec![],
        }
    }

    struct TestContext {
        db: Arc<Mutex<Connection>>,
        state: ApiState,
        addr: std::net::SocketAddr,
        target_device_id: String,
        target_sk: SigningKey,
        mobile_device_id: String,
        mobile_sk: SigningKey,
        session_id: String,
    }

    fn setup_test_context() -> TestContext {
        let mut conn = setup_test_db();
        let (target_sk, target_device_id) = generate_test_keypair();
        let (mobile_sk, mobile_device_id) = generate_test_keypair();
        let now = crate::now_ms();

        // 建立真实配对邀请与可信设备记录
        let invite = create_pairing_invitation(
            &conn,
            &target_device_id,
            None,
            600_000,
            "relay",
            vec![],
            3722,
            now,
        )
        .unwrap();

        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &target_device_id,
            &mobile_device_id,
            &mobile_device_id,
            "Phone",
            "android",
            "n-test",
            now,
            "pairing_establishment",
        );
        let sig = sign_bytes(&mobile_sk, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: target_device_id.clone(),
            subject_device_id: mobile_device_id.clone(),
            subject_pubkey: mobile_device_id.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-test".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(
            &mut conn,
            &mobile_device_id,
            &target_device_id,
            86400_000,
            now,
        )
        .unwrap();

        let db = Arc::new(Mutex::new(conn));
        let state = ApiState {
            app: None,
            db: Some(db.clone()),
            test_target_id: Some(target_device_id.clone()),
            test_signing_key: Some(target_sk.clone()),
        };
        let addr = "192.168.1.100:54321".parse().unwrap();

        TestContext {
            db,
            state,
            addr,
            target_device_id,
            target_sk,
            mobile_device_id,
            mobile_sk,
            session_id: session.session_id,
        }
    }

    fn make_test_envelope(
        ctx: &TestContext,
        action: &str,
        request_id: &str,
        payload: &[u8],
        now: i64,
    ) -> RpcAuthEnvelope {
        let payload_hash = compute_sha512(payload);
        let nonce = format!("nonce-{}", uuid::Uuid::new_v4());
        let sig = sign_bytes(
            &ctx.mobile_sk,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &ctx.session_id,
                request_id,
                &ctx.mobile_device_id,
                &ctx.target_device_id,
                action,
                &payload_hash,
                &nonce,
                now,
            ),
        );
        RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: ctx.session_id.clone(),
            request_id: request_id.to_string(),
            subject_device_id: ctx.mobile_device_id.clone(),
            target_device_id: ctx.target_device_id.clone(),
            action: action.to_string(),
            payload_hash,
            nonce,
            timestamp: now,
            signature: sig,
        }
    }

    // --------------------------------------------------------------------------
    // 1. Production Axum handle_sync_pull: Positive & Idempotent Cache
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_axum_sync_pull_positive_and_idempotent_cache() {
        let ctx = setup_test_context();
        let now = crate::now_ms();
        let req_id = "req-e2e-pull-001";
        let env = make_test_envelope(&ctx, "pull", req_id, b"", now);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());

        // 首次请求：调用真实生产 Axum Handler handle_sync_pull
        let resp = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers.clone()).await.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let resp_val: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(resp_val["status"], "ok");
        assert!(resp_val.get("data").is_some());

        // 验证数据库真实落盘了 rpc_idempotency_cache 记录且状态为 completed
        {
            let conn = ctx.db.lock().unwrap();
            let (cached_resp, action, subject, target, p_hash, status): (String, String, String, String, String, String) = conn
                .query_row(
                    "SELECT response_json, action, subject_device_id, target_device_id, payload_hash, status
                     FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                    rusqlite::params![ctx.session_id, req_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                )
                .expect("rpc_idempotency_cache 必须落盘首次执行结果");

            assert_eq!(action, "pull");
            assert_eq!(subject, ctx.mobile_device_id);
            assert_eq!(target, ctx.target_device_id);
            assert_eq!(p_hash, compute_sha512(b""));
            assert_eq!(status, "completed");
            assert!(cached_resp.contains("\"status\":\"ok\""));
        }

        // 二次重放完全相同请求：必须直接从 rpc_idempotency_cache 返回，状态码保持 200
        let resp2 = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers).await.into_response();
        assert_eq!(resp2.status(), axum::http::StatusCode::OK);
        let body_bytes2 = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        let resp2_val: Value = serde_json::from_slice(&body_bytes2).unwrap();
        assert_eq!(resp2_val["status"], "ok");

        // 幂等表记录数仍严格为 1
        {
            let conn = ctx.db.lock().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(1) FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                    rusqlite::params![ctx.session_id, req_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1);
        }
    }

    // --------------------------------------------------------------------------
    // 2. Production Axum handle_sync_push: Side-effect & Idempotent Cache
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_axum_sync_push_positive_and_side_effect() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_cfg_push_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        let _ = std::fs::write(&test_cfg_path, "{}");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;

        let ctx = setup_test_context();
        let now = crate::now_ms();
        let req_id = "req-e2e-push-001";
        let push_ops = serde_json::json!([
            {"op": "set_config", "key": "theme", "value": "dark"}
        ]);
        let body_bytes = serde_json::to_vec(&push_ops).unwrap();
        let env = make_test_envelope(&ctx, "push", req_id, &body_bytes, now);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        // 首次真实调用 handle_sync_push
        let resp = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers.clone(), body_bytes.clone().into()).await.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let resp_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let resp_val: Value = serde_json::from_slice(&resp_bytes).unwrap();
        assert_eq!(resp_val["status"], "ok");

        // 验证幂等表落盘且状态为 completed
        {
            let conn = ctx.db.lock().unwrap();
            let (action, status): (String, String) = conn
                .query_row(
                    "SELECT action, status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                    rusqlite::params![ctx.session_id, req_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .expect("Push 成功后必须落盘 rpc_idempotency_cache");
            assert_eq!(action, "push");
            assert_eq!(status, "completed");
        }

        // 重复发送完全相同的 POST 请求：返回命中幂等缓存的 200 OK
        let resp2 = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers, body_bytes.into()).await.into_response();
        assert_eq!(resp2.status(), axum::http::StatusCode::OK);
        let resp2_bytes = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        let resp2_val: Value = serde_json::from_slice(&resp2_bytes).unwrap();
        assert_eq!(resp2_val["status"], "ok");
    }

    // --------------------------------------------------------------------------
    // 3. Production Axum handle_sync_push_db: Positive & Idempotent Cache
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_axum_sync_push_db_positive_and_idempotent_cache() {
        let ctx = setup_test_context();
        let now = crate::now_ms();
        let req_id = "req-e2e-pushdb-001";
        let sync_data = empty_sync_data();
        let body_bytes = serde_json::to_vec(&sync_data).unwrap();
        let env = make_test_envelope(&ctx, "push_db", req_id, &body_bytes, now);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        let resp = handle_sync_push_db(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers.clone(), body_bytes.clone().into()).await.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let resp_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let resp_val: Value = serde_json::from_slice(&resp_bytes).unwrap();
        assert_eq!(resp_val["status"], "ok");

        // 验证幂等表落盘 action = "push_db" 且 status = "completed"
        {
            let conn = ctx.db.lock().unwrap();
            let (action, status): (String, String) = conn
                .query_row(
                    "SELECT action, status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                    rusqlite::params![ctx.session_id, req_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .expect("push_db 成功后必须落盘 rpc_idempotency_cache");
            assert_eq!(action, "push_db");
            assert_eq!(status, "completed");
        }

        // 二次重试命中缓存
        let resp2 = handle_sync_push_db(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers, body_bytes.into()).await.into_response();
        assert_eq!(resp2.status(), axum::http::StatusCode::OK);
        let resp2_bytes = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        let resp2_val: Value = serde_json::from_slice(&resp2_bytes).unwrap();
        assert_eq!(resp2_val["status"], "ok");
    }

    // --------------------------------------------------------------------------
    // 4. Production Axum Idempotency Conflict returns 409 Conflict
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_axum_idempotency_conflict_returns_409() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_cfg_conflict_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        let _ = std::fs::write(&test_cfg_path, "{}");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;

        let ctx = setup_test_context();
        let now = crate::now_ms();
        let shared_req_id = "req-conflict-shared-001";

        // 第一步：客户端以 action="pull" 发起请求并成功落盘缓存
        let env_pull = make_test_envelope(&ctx, "pull", shared_req_id, b"", now);
        let mut headers_pull = axum::http::HeaderMap::new();
        headers_pull.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_pull.insert("x-rpc-auth-envelope", serde_json::to_string(&env_pull).unwrap().parse().unwrap());

        let resp1 = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_pull.clone()).await.into_response();
        assert_eq!(resp1.status(), axum::http::StatusCode::OK);

        // 第二步：客户端非法复用同一 request_id 发送 action="push"，但缓存中记录的是 pull
        let push_ops = serde_json::json!([]);
        let push_bytes = serde_json::to_vec(&push_ops).unwrap();
        let env_push_conflicting = make_test_envelope(&ctx, "push", shared_req_id, &push_bytes, now + 10);
        let mut headers_push = axum::http::HeaderMap::new();
        headers_push.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_push.insert("x-rpc-auth-envelope", serde_json::to_string(&env_push_conflicting).unwrap().parse().unwrap());
        headers_push.insert("content-type", "application/json".parse().unwrap());

        let resp2 = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_push, push_bytes.into()).await.into_response();

        // 必须返回 409 CONFLICT，绝不能返回历史 pull 的旧响应
        assert_eq!(resp2.status(), axum::http::StatusCode::CONFLICT);
        let resp2_bytes = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        let err_body: Value = serde_json::from_slice(&resp2_bytes).unwrap();
        let msg = err_body["message"].as_str().unwrap_or("");
        assert!(msg.contains("Idempotency conflict"), "Error must state idempotency conflict: {}", msg);

        // 第三步：测试同 action ("push") 但不同 payload_hash 的 409 冲突
        let push_req_id = "req-conflict-push-001";
        let push_1 = serde_json::to_vec(&serde_json::json!([{"op": "set_config", "key": "theme", "value": "light"}])).unwrap();
        let env_push_1 = make_test_envelope(&ctx, "push", push_req_id, &push_1, now + 20);
        let mut headers_push_1 = axum::http::HeaderMap::new();
        headers_push_1.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_push_1.insert("x-rpc-auth-envelope", serde_json::to_string(&env_push_1).unwrap().parse().unwrap());
        headers_push_1.insert("content-type", "application/json".parse().unwrap());

        let resp_push_1 = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_push_1, push_1.into()).await.into_response();
        assert_eq!(resp_push_1.status(), axum::http::StatusCode::OK);

        // 同一 push_req_id，相同 action="push"，但 payload 变为 push_2
        let push_2 = serde_json::to_vec(&serde_json::json!([{"op": "set_config", "key": "theme", "value": "dark"}])).unwrap();
        let env_push_2 = make_test_envelope(&ctx, "push", push_req_id, &push_2, now + 30);
        let mut headers_push_2 = axum::http::HeaderMap::new();
        headers_push_2.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_push_2.insert("x-rpc-auth-envelope", serde_json::to_string(&env_push_2).unwrap().parse().unwrap());
        headers_push_2.insert("content-type", "application/json".parse().unwrap());

        let resp_push_2 = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_push_2, push_2.into()).await.into_response();
        assert_eq!(resp_push_2.status(), axum::http::StatusCode::CONFLICT);
        let resp_push_2_bytes = axum::body::to_bytes(resp_push_2.into_body(), usize::MAX).await.unwrap();
        let err_body_3: Value = serde_json::from_slice(&resp_push_2_bytes).unwrap();
        let msg_3 = err_body_3["message"].as_str().unwrap_or("");
        assert!(msg_3.contains("Idempotency conflict"), "Payload mismatch must trigger conflict: {}", msg_3);
    }

    // --------------------------------------------------------------------------
    // 5. Pre-side-effect Failure allows subsequent Retry (No Cache Pollution)
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_axum_pre_side_effect_failure_allows_retry() {
        let ctx = setup_test_context();
        let now = crate::now_ms();

        // 场景 A: 缺失信封 -> 401 UNAUTHORIZED
        let headers_empty = axum::http::HeaderMap::new();
        let resp_no_env = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_empty).await.into_response();
        assert_eq!(resp_no_env.status(), axum::http::StatusCode::UNAUTHORIZED);

        // 场景 B: 冒充 X-Device-Id -> 403 FORBIDDEN
        let env_valid = make_test_envelope(&ctx, "pull", "req-retry-001", b"", now);
        let mut headers_forged = axum::http::HeaderMap::new();
        headers_forged.insert("x-device-id", "attacker_device_id".parse().unwrap());
        headers_forged.insert("x-rpc-auth-envelope", serde_json::to_string(&env_valid).unwrap().parse().unwrap());

        let resp_forged = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_forged).await.into_response();
        assert_eq!(resp_forged.status(), axum::http::StatusCode::FORBIDDEN);

        // 场景 C: 篡改签名 -> 401 UNAUTHORIZED
        let mut env_bad_sig = env_valid.clone();
        env_bad_sig.signature = BASE64.encode([0u8; 64]);
        let mut headers_bad_sig = axum::http::HeaderMap::new();
        headers_bad_sig.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_bad_sig.insert("x-rpc-auth-envelope", serde_json::to_string(&env_bad_sig).unwrap().parse().unwrap());

        let resp_bad_sig = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_bad_sig).await.into_response();
        assert_eq!(resp_bad_sig.status(), axum::http::StatusCode::UNAUTHORIZED);

        // 校验：以上所有副作用前鉴权失败均未污染 idempotency_cache
        {
            let conn = ctx.db.lock().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(1) FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                    rusqlite::params![ctx.session_id, "req-retry-001"],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "Authentication failure must not be written into idempotency cache");
        }

        // 场景 D: 客户端修正身份与签名后再次发起请求 -> 成功执行并落盘
        let mut headers_ok = axum::http::HeaderMap::new();
        headers_ok.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_ok.insert("x-rpc-auth-envelope", serde_json::to_string(&env_valid).unwrap().parse().unwrap());

        let resp_ok = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_ok).await.into_response();
        assert_eq!(resp_ok.status(), axum::http::StatusCode::OK);

        // 成功后幂等表有且仅有 1 条
        {
            let conn = ctx.db.lock().unwrap();
            let (count, status): (i64, String) = conn
                .query_row(
                    "SELECT COUNT(1), status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                    rusqlite::params![ctx.session_id, "req-retry-001"],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(count, 1);
            assert_eq!(status, "completed");
        }
    }

    // --------------------------------------------------------------------------
    // Test 1: Worker A lease > 30s, Worker B takes over; side effect executed only once
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_fencing_worker_a_slow_worker_b_takeover_single_side_effect() {
        let ctx = setup_test_context();
        let now = crate::now_ms();
        let req_id = "req-fence-slow-001";
        let ops = vec![serde_json::json!({"op": "set_config", "key": "theme", "value": "system"})];
        let payload_bytes = serde_json::to_vec(&ops).unwrap();
        let env_a = make_test_envelope(&ctx, "push", req_id, &payload_bytes, now - 35_000);

        // 1. Worker A 开始处理（时间为 35 秒前），获取 execution_token_A，lease_generation = 1
        let (token_a, gen_a) = {
            let mut conn = ctx.db.lock().unwrap();
            let auth_a = verify_rpc_request_auth(&mut conn, &env_a, &payload_bytes, &ctx.target_device_id, now - 35_000).unwrap();
            match auth_a {
                crate::device_trust::AuthVerificationOutcome::Authorized { execution_token, lease_generation, .. } => {
                    (execution_token, lease_generation)
                }
                _ => panic!("Worker A auth must succeed and acquire lease"),
            }
        };
        assert_eq!(gen_a, 1);

        // 2. 模拟 Worker A 运行缓慢（> 30 秒超时），Worker B 到达并触发超时接管
        let env_b = make_test_envelope(&ctx, "push", req_id, &payload_bytes, now);
        let (token_b, gen_b) = {
            let mut conn = ctx.db.lock().unwrap();
            let auth_b = verify_rpc_request_auth(&mut conn, &env_b, &payload_bytes, &ctx.target_device_id, now).unwrap();
            match auth_b {
                crate::device_trust::AuthVerificationOutcome::Authorized { execution_token, lease_generation, .. } => {
                    (execution_token, lease_generation)
                }
                _ => panic!("Worker B auth takeover must succeed"),
            }
        };
        assert_eq!(gen_b, 2);
        assert_ne!(token_a, token_b, "Worker B must receive a new execution token");

        // 3. Worker B 执行副作用并调用 atomic_commit_push_outbox：提交成功
        {
            let mut conn = ctx.db.lock().unwrap();
            let commit_b = crate::device_trust::atomic_commit_push_outbox(
                &mut conn,
                &ctx.session_id,
                req_id,
                &token_b,
                &ops,
                "{\"status\":\"ok\"}",
                now,
            );
            assert!(commit_b.is_ok(), "Worker B commit must succeed: {:?}", commit_b);
        }

        // 4. Worker A 终于醒来，尝试使用已经过期的 token_a 进行提交：必须被 Fencing Token 拦截拒绝！
        {
            let mut conn = ctx.db.lock().unwrap();
            let commit_a = crate::device_trust::atomic_commit_push_outbox(
                &mut conn,
                &ctx.session_id,
                req_id,
                &token_a,
                &ops,
                "{\"status\":\"ok\"}",
                now,
            );
            assert!(commit_a.is_err(), "Worker A commit with stale token must be rejected by fencing check");
            let err_msg = commit_a.unwrap_err();
            assert!(err_msg.contains("Fencing token mismatch") || err_msg.contains("接管"), "Error must mention fencing token mismatch: {}", err_msg);
        }

        // 5. 校验数据库：rpc_staged_outbox 中此请求的副作用记录严格为 1 条，绝无重复副作用！
        {
            let conn = ctx.db.lock().unwrap();
            let outbox_count: i64 = conn.query_row(
                "SELECT COUNT(1) FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(outbox_count, 1, "Side-effect in staged outbox must exist exactly once");

            let cache_status: String = conn.query_row(
                "SELECT status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(cache_status, "completed");
        }
    }

    // --------------------------------------------------------------------------
    // Test 2: Worker A side effect staged, crash before completed; retry executes side effect only once
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_crash_after_side_effect_rollback_and_retry_no_duplicate() {
        let ctx = setup_test_context();
        let now = crate::now_ms();
        let req_id = "req-crash-rollback-001";
        let ops = vec![serde_json::json!({"op": "set_config", "key": "language", "value": "en-US"})];
        let payload_bytes = serde_json::to_vec(&ops).unwrap();
        let env = make_test_envelope(&ctx, "push", req_id, &payload_bytes, now);

        // 1. Worker A 鉴权成功占用 pending，分配 token_a
        let (_token_a, _) = {
            let mut conn = ctx.db.lock().unwrap();
            let auth = verify_rpc_request_auth(&mut conn, &env, &payload_bytes, &ctx.target_device_id, now).unwrap();
            match auth {
                crate::device_trust::AuthVerificationOutcome::Authorized { execution_token, lease_generation, .. } => {
                    (execution_token, lease_generation)
                }
                _ => panic!("Auth must succeed"),
            }
        };

        // 2. 模拟 Worker A 在写入副作用事务中途发生崩溃（事务未提交回滚）
        {
            let mut conn = ctx.db.lock().unwrap();
            let tx = conn.transaction().unwrap();
            // 在事务内暂存副作用，但在 commit 前回滚模拟崩溃
            let ops_json = serde_json::to_string(&ops).unwrap();
            tx.execute(
                "INSERT INTO rpc_staged_outbox (session_id, request_id, operations_json, created_at) VALUES (?, ?, ?, ?)",
                rusqlite::params![ctx.session_id, req_id, ops_json, now],
            ).unwrap();
            // 事务显式 rollback，模拟崩溃
            tx.rollback().unwrap();
        }

        // 校验：此时 rpc_staged_outbox 中为 0 条，数据库处于清洁状态
        {
            let conn = ctx.db.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT COUNT(1) FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(count, 0, "No staged outbox should persist after rollback/crash");
        }

        // 3. 30 秒超时后重试（模拟服务重启或客户端重发），Worker B 接管并执行原子提交
        let retry_time = now + 35_000;
        let env_retry = make_test_envelope(&ctx, "push", req_id, &payload_bytes, retry_time);
        let token_b = {
            let mut conn = ctx.db.lock().unwrap();
            let auth_retry = verify_rpc_request_auth(&mut conn, &env_retry, &payload_bytes, &ctx.target_device_id, retry_time).unwrap();
            match auth_retry {
                crate::device_trust::AuthVerificationOutcome::Authorized { execution_token, .. } => execution_token,
                _ => panic!("Retry takeover must succeed"),
            }
        };

        // Worker B 调用 atomic_commit_push_outbox 原子提交
        {
            let mut conn = ctx.db.lock().unwrap();
            let commit_res = crate::device_trust::atomic_commit_push_outbox(
                &mut conn,
                &ctx.session_id,
                req_id,
                &token_b,
                &ops,
                "{\"status\":\"ok\"}",
                retry_time,
            );
            assert!(commit_res.is_ok(), "Worker B atomic commit must succeed");
        }

        // 4. 校验：重试后数据库中 staged outbox 严格为 1 条，status 为 completed
        {
            let conn = ctx.db.lock().unwrap();
            let outbox_count: i64 = conn.query_row(
                "SELECT COUNT(1) FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(outbox_count, 1, "Exactly one outbox record must exist after crash recovery");

            let (status, resp_json): (String, String) = conn.query_row(
                "SELECT status, response_json FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).unwrap();
            assert_eq!(status, "completed");
            assert!(resp_json.contains("\"status\":\"ok\""));
        }
    }

    // --------------------------------------------------------------------------
    // Test 3: Worker B takes over, Worker A attempts commit with old token; rejected by fencing check
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_fencing_stale_token_rejected_after_takeover() {
        let ctx = setup_test_context();
        let now = crate::now_ms();
        let req_id = "req-fence-stale-001";
        let env_a = make_test_envelope(&ctx, "pull", req_id, b"", now - 35_000);

        // Worker A 抢占租约
        let token_a = {
            let mut conn = ctx.db.lock().unwrap();
            match verify_rpc_request_auth(&mut conn, &env_a, b"", &ctx.target_device_id, now - 35_000).unwrap() {
                crate::device_trust::AuthVerificationOutcome::Authorized { execution_token, .. } => execution_token,
                _ => panic!("Expected authorized"),
            }
        };

        // Worker B 接管租约
        let env_b = make_test_envelope(&ctx, "pull", req_id, b"", now);
        let token_b = {
            let mut conn = ctx.db.lock().unwrap();
            match verify_rpc_request_auth(&mut conn, &env_b, b"", &ctx.target_device_id, now).unwrap() {
                crate::device_trust::AuthVerificationOutcome::Authorized { execution_token, .. } => execution_token,
                _ => panic!("Expected authorized"),
            }
        };
        assert_ne!(token_a, token_b);

        // Worker A 尝试以旧 token 完成 idempotency cache
        {
            let conn = ctx.db.lock().unwrap();
            let res_a = crate::device_trust::complete_rpc_idempotency(
                &conn,
                &ctx.session_id,
                req_id,
                &token_a,
                "{\"from\":\"worker_a\"}",
                now,
            );
            assert!(res_a.is_err(), "Worker A stale token must fail complete_rpc_idempotency");
            let err = res_a.unwrap_err();
            assert!(err.contains("Fencing token mismatch"), "Error must state Fencing token mismatch: {}", err);
        }

        // Worker B 以有效 token 完成
        {
            let conn = ctx.db.lock().unwrap();
            let res_b = crate::device_trust::complete_rpc_idempotency(
                &conn,
                &ctx.session_id,
                req_id,
                &token_b,
                "{\"from\":\"worker_b\"}",
                now,
            );
            assert!(res_b.is_ok(), "Worker B valid token must succeed");
        }

        // 校验缓存内容为 Worker B 的输出，而非 Worker A
        {
            let conn = ctx.db.lock().unwrap();
            let resp: String = conn.query_row(
                "SELECT response_json FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert!(resp.contains("\"from\":\"worker_b\""), "Cache must belong to Worker B: {}", resp);
        }
    }

    // --------------------------------------------------------------------------
    // Test 4: Two concurrent handlers with same request ID; outbox table has only 1 item
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_concurrent_handlers_same_request_id_single_outbox_item() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_cfg_conc_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        let _ = std::fs::write(&test_cfg_path, "{}");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;

        let ctx = setup_test_context();
        let now = crate::now_ms();
        let ops = serde_json::json!([
            {"op": "set_config", "key": "uiScale", "value": 1.2}
        ]);
        let body_bytes = serde_json::to_vec(&ops).unwrap();

        // 场景 A: 校验并发抢占窗口（当请求 1 正处于 pending 状态尚未提交时）
        // 并发携带相同 (session_id, request_id) 的 Handler 必被前置拦截为 409 CONFLICT
        {
            let req_pending_id = "req-conc-pending-001";
            let env_pending = make_test_envelope(&ctx, "push", req_pending_id, &body_bytes, now);
            // 抢占 pending
            {
                let mut conn = ctx.db.lock().unwrap();
                verify_rpc_request_auth(&mut conn, &env_pending, &body_bytes, &ctx.target_device_id, now).unwrap();
            }
            // 并发请求到达 Axum Handler：必须返回 409 CONFLICT，绝不触碰业务逻辑
            let mut h_pend = axum::http::HeaderMap::new();
            h_pend.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
            h_pend.insert("x-rpc-auth-envelope", serde_json::to_string(&env_pending).unwrap().parse().unwrap());
            h_pend.insert("content-type", "application/json".parse().unwrap());

            let resp_conflict = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), h_pend, body_bytes.clone().into()).await.into_response();
            assert_eq!(resp_conflict.status(), axum::http::StatusCode::CONFLICT);
        }

        // 场景 B: 两个 Handler 真实并发执行同一 request_id
        let req_id = "req-conc-push-001";
        let env = make_test_envelope(&ctx, "push", req_id, &body_bytes, now);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        let h1 = headers.clone();
        let b1 = body_bytes.clone();
        let s1 = ctx.state.clone();
        let addr1 = ctx.addr;

        let h2 = headers.clone();
        let b2 = body_bytes.clone();
        let s2 = ctx.state.clone();
        let addr2 = ctx.addr;

        let cfg1 = test_cfg_path.clone();
        let fut1 = tokio::spawn(async move {
            crate::set_test_config_path_override(Some(cfg1));
            handle_sync_push(State(s1), ConnectInfo(addr1), h1, b1.into()).await.into_response()
        });
        let cfg2 = test_cfg_path.clone();
        let fut2 = tokio::spawn(async move {
            crate::set_test_config_path_override(Some(cfg2));
            handle_sync_push(State(s2), ConnectInfo(addr2), h2, b2.into()).await.into_response()
        });

        let (res1, res2) = tokio::join!(fut1, fut2);
        let resp1 = res1.unwrap();
        let resp2 = res2.unwrap();

        // 至少一个成功（200 OK）；另一个要么在 pending 期间被拦截（409 CONFLICT），要么命中已完成的幂等缓存（200 OK）
        assert!(resp1.status() == axum::http::StatusCode::OK || resp2.status() == axum::http::StatusCode::OK);
        assert!(
            resp1.status() == axum::http::StatusCode::CONFLICT
                || resp2.status() == axum::http::StatusCode::CONFLICT
                || (resp1.status() == axum::http::StatusCode::OK && resp2.status() == axum::http::StatusCode::OK)
        );

        // 关键核心不变量：无论两并发请求调度时序如何，rpc_staged_outbox 中该请求的副作用严格有且仅有 1 条！
        {
            let conn = ctx.db.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT COUNT(1) FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(count, 1, "Concurrent race must result in exactly 1 staged outbox item");
        }
    }

    // --------------------------------------------------------------------------
    // Test 5: Real WebSocket connection via tokio_tungstenite testing allowlist and close behavior
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_real_websocket_router_connection_allowlist_and_close() {
        use futures_util::{SinkExt, StreamExt};
        let ctx = setup_test_context();
        let now = crate::now_ms();

        // 1. 真实绑定 TCP 监听器并启动 Axum 路由服务
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let router = create_public_router_with_state(ctx.state.clone());

        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
        });

        // 2. 通过 tokio_tungstenite 建立真实的 TCP/WebSocket 连接
        let ws_url = format!("ws://{}/v1/sync", local_addr);
        let (mut ws_stream, _response) = tokio_tungstenite::connect_async(&ws_url).await.expect("Real WebSocket TCP connect must succeed");

        // 3. 发送首包认证帧（action = ws_connect）
        let env_ws = make_test_envelope(&ctx, "ws_connect", "req-ws-e2e-001", b"", now);
        let auth_frame = serde_json::json!({
            "type": "auth",
            "envelope": env_ws
        });
        ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(auth_frame.to_string().into())).await.unwrap();

        // 读取响应：期望获得 auth_ok
        let msg = ws_stream.next().await.expect("Expected message from server").expect("Valid frame");
        let txt = match msg {
            tokio_tungstenite::tungstenite::Message::Text(ref s) => s.as_str(),
            other => panic!("Expected text frame, got {:?}", other),
        };
        let val: Value = serde_json::from_str(txt).unwrap();
        assert_eq!(val["type"], "auth_ok");
        assert_eq!(val["device_id"], ctx.mobile_device_id);

        // 4. 测试允许列表帧 1: ping -> 期望收到 pong
        let ping_frame = serde_json::json!({ "type": "ping" });
        ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(ping_frame.to_string().into())).await.unwrap();
        let pong_msg = ws_stream.next().await.unwrap().unwrap();
        let pong_txt = match pong_msg {
            tokio_tungstenite::tungstenite::Message::Text(ref s) => s.as_str(),
            other => panic!("Expected text frame, got {:?}", other),
        };
        let pong_val: Value = serde_json::from_str(pong_txt).unwrap();
        assert_eq!(pong_val["type"], "pong");

        // 5. 测试允许列表帧 2: status -> 期望收到 status_ok
        let status_frame = serde_json::json!({ "type": "status" });
        ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(status_frame.to_string().into())).await.unwrap();
        let status_msg = ws_stream.next().await.unwrap().unwrap();
        let status_txt = match status_msg {
            tokio_tungstenite::tungstenite::Message::Text(ref s) => s.as_str(),
            other => panic!("Expected text frame, got {:?}", other),
        };
        let status_val: Value = serde_json::from_str(status_txt).unwrap();
        assert_eq!(status_val["type"], "status_ok");
        assert_eq!(status_val["device_id"], ctx.mobile_device_id);

        // 6. 测试未授权/非标准帧: arbitrary_cmd -> 期望收到 error 帧
        let illegal_frame = serde_json::json!({
            "type": "arbitrary_cmd",
            "command": "rm -rf /"
        });
        ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(illegal_frame.to_string().into())).await.unwrap();
        let err_msg = ws_stream.next().await.unwrap().unwrap();
        let err_txt = match err_msg {
            tokio_tungstenite::tungstenite::Message::Text(ref s) => s.as_str(),
            other => panic!("Expected text frame, got {:?}", other),
        };
        let err_val: Value = serde_json::from_str(err_txt).unwrap();
        assert_eq!(err_val["type"], "error");
        let err_text = err_val["message"].as_str().unwrap_or("");
        assert!(err_text.contains("Unsupported frame"), "Must reject unsupported frame: {}", err_text);

        // 7. 测试正常断开连接：发送 Close 帧并验证连接干净关闭
        ws_stream.close(None).await.unwrap();
        let next_msg = ws_stream.next().await;
        if let Some(Ok(m)) = next_msg {
            assert!(m.is_close(), "Trailing frame if any must be Close frame");
        }
    }

    // --------------------------------------------------------------------------
    // Test 6: Injected completed write failure via trigger calling real Handler, asserting HTTP 500
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_axum_handler_injected_completed_failure_returns_http_500() {
        let ctx = setup_test_context();
        let now = crate::now_ms();
        let req_id = "req-inj-500-001";
        let env = make_test_envelope(&ctx, "pull", req_id, b"", now);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());

        // 关键：在数据库中注入 SQLite TRIGGER，在更新 status = 'completed' 时抛出 ABORT
        {
            let conn = ctx.db.lock().unwrap();
            conn.execute(
                "CREATE TRIGGER trigger_fail_idempotency_completed
                 BEFORE UPDATE OF status ON rpc_idempotency_cache
                 WHEN NEW.status = 'completed'
                 BEGIN
                     SELECT RAISE(ABORT, 'injected disk failure on idempotency completed');
                 END;",
                [],
            ).unwrap();
        }

        // 调用真实生产 Axum Handler handle_sync_pull
        let resp = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers).await.into_response();

        // 证明：真实 Handler 绝对不静默吞错 (严禁 let _ =)，必须诚实返回 HTTP 500
        assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);

        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let err_body: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(err_body["status"], "error");
        let msg = err_body["message"].as_str().unwrap_or("");
        assert!(msg.contains("injected disk failure on idempotency completed"), "HTTP 500 response must carry trigger failure detail: {}", msg);
    }

    // --------------------------------------------------------------------------
    // Test 7: Relay push fencing and crash window protection
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_e2e_relay_push_fencing_and_crash_window_protection() {
        let mut conn = setup_test_db();
        let (_target_sk, target_device_id) = generate_test_keypair();
        let (mobile_sk, mobile_device_id) = generate_test_keypair();
        let now = crate::now_ms();

        // 配对并建立会话
        let invite = create_pairing_invitation(
            &conn,
            &target_device_id,
            None,
            600_000,
            "relay",
            vec![],
            3722,
            now,
        ).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &target_device_id,
            &mobile_device_id,
            &mobile_device_id,
            "Phone",
            "android",
            "n-relay-fence",
            now,
            "pairing_establishment",
        );
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: target_device_id.clone(),
            subject_device_id: mobile_device_id.clone(),
            subject_pubkey: mobile_device_id.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-relay-fence".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&mobile_sk, &canonical),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(
            &mut conn,
            &mobile_device_id,
            &target_device_id,
            86400_000,
            now,
        ).unwrap();

        let req_id = "req-relay-fence-push-001";
        let ops = serde_json::json!([
            {"op": "set_config", "key": "accentColor", "value": "#1890ff"}
        ]);
        let raw_ops = crate::device_trust::canonicalize_json_value(&ops);
        let payload_hash = compute_sha512(&raw_ops);
        let sig = sign_bytes(
            &mobile_sk,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &session.session_id,
                req_id,
                &mobile_device_id,
                &target_device_id,
                "push",
                &payload_hash,
                "n-relay-push-1",
                now,
            ),
        );
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: req_id.to_string(),
            subject_device_id: mobile_device_id.clone(),
            target_device_id: target_device_id.clone(),
            action: "push".to_string(),
            payload_hash,
            nonce: "n-relay-push-1".to_string(),
            timestamp: now,
            signature: sig,
        };
        let relay_msg = serde_json::json!({
            "type": "proxy",
            "from_device_id": mobile_device_id,
            "target_device_id": target_device_id,
            "payload": {
                "action": "push",
                "data": ops,
                "envelope": env
            }
        });

        // 1. 首次调度：成功执行 push 并通过 atomic_commit_push_outbox 原子提交
        let res1 = crate::sync_engine::dispatch_relay_proxy_message(
            &mut conn,
            &relay_msg,
            &target_device_id,
            now,
        );
        assert!(res1.is_ok(), "First relay push must succeed: {:?}", res1);
        let val1 = res1.unwrap();
        assert_eq!(val1["payload"]["action"], "commit_ack");
        assert_eq!(val1["payload"]["status"], "committed");

        // 校验数据库：staged outbox 有且仅有 1 条
        let outbox_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
            rusqlite::params![session.session_id, req_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(outbox_count, 1, "Relay push must insert exactly 1 staged outbox item");

        // 2. 重放同一 push 消息：直接返回幂等缓存的 commit_ack，不新增 outbox
        let res2 = crate::sync_engine::dispatch_relay_proxy_message(
            &mut conn,
            &relay_msg,
            &target_device_id,
            now,
        );
        assert!(res2.is_ok(), "Second relay push must hit idempotent cache: {:?}", res2);
        let val2 = res2.unwrap();
        assert_eq!(val2["payload"]["action"], "commit_ack");

        let outbox_count2: i64 = conn.query_row(
            "SELECT COUNT(1) FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
            rusqlite::params![session.session_id, req_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(outbox_count2, 1, "Staged outbox count must remain 1 after idempotent replay");
    }

    // --------------------------------------------------------------------------
    // Relay Idempotency Conflict & Action Mismatch
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_e2e_relay_dispatcher_idempotency_and_conflict() {
        let mut conn = setup_test_db();
        let (_target_sk, target_device_id) = generate_test_keypair();
        let (mobile_sk, mobile_device_id) = generate_test_keypair();
        let now = crate::now_ms();

        // 配对并建立会话
        let invite = create_pairing_invitation(
            &conn,
            &target_device_id,
            None,
            600_000,
            "relay",
            vec![],
            3722,
            now,
        ).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &target_device_id,
            &mobile_device_id,
            &mobile_device_id,
            "Phone",
            "android",
            "n-relay-test",
            now,
            "pairing_establishment",
        );
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: target_device_id.clone(),
            subject_device_id: mobile_device_id.clone(),
            subject_pubkey: mobile_device_id.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-relay-test".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&mobile_sk, &canonical),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(
            &mut conn,
            &mobile_device_id,
            &target_device_id,
            86400_000,
            now,
        ).unwrap();

        // 构造 pull 消息
        let req_id = "req-relay-pull-001";
        let payload_hash = compute_sha512(b"{\"action\":\"pull\"}");
        let sig = sign_bytes(
            &mobile_sk,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &session.session_id,
                req_id,
                &mobile_device_id,
                &target_device_id,
                "pull",
                &payload_hash,
                "n-relay-p1",
                now,
            ),
        );
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: req_id.to_string(),
            subject_device_id: mobile_device_id.clone(),
            target_device_id: target_device_id.clone(),
            action: "pull".to_string(),
            payload_hash,
            nonce: "n-relay-p1".to_string(),
            timestamp: now,
            signature: sig,
        };

        let relay_msg = serde_json::json!({
            "type": "proxy",
            "from_device_id": mobile_device_id,
            "target_device_id": target_device_id,
            "payload": {
                "action": "pull",
                "envelope": env
            }
        });

        // 首次调度：成功执行 pull 并落盘幂等缓存
        let res1 = crate::sync_engine::dispatch_relay_proxy_message(
            &mut conn,
            &relay_msg,
            &target_device_id,
            now,
        );
        assert!(res1.is_ok(), "First relay pull must succeed: {:?}", res1);
        let val1 = res1.unwrap();
        assert_eq!(val1["payload"]["action"], "pull_response");

        // 冲突检测：复用同一 request_id 发起 push 动作
        let push_payload_hash = compute_sha512(b"{\"action\":\"push\"}");
        let push_sig = sign_bytes(
            &mobile_sk,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &session.session_id,
                req_id,
                &mobile_device_id,
                &target_device_id,
                "push",
                &push_payload_hash,
                "n-relay-conflict",
                now + 10,
            ),
        );
        let env_conflict = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: req_id.to_string(),
            subject_device_id: mobile_device_id.clone(),
            target_device_id: target_device_id.clone(),
            action: "push".to_string(),
            payload_hash: push_payload_hash,
            nonce: "n-relay-conflict".to_string(),
            timestamp: now + 10,
            signature: push_sig,
        };
        let conflict_relay_msg = serde_json::json!({
            "type": "proxy",
            "from_device_id": mobile_device_id,
            "target_device_id": target_device_id,
            "payload": {
                "action": "push",
                "envelope": env_conflict
            }
        });

        let conflict_res = crate::sync_engine::dispatch_relay_proxy_message(
            &mut conn,
            &conflict_relay_msg,
            &target_device_id,
            now + 10,
        );
        assert!(conflict_res.is_err(), "Conflicting relay message must be rejected");
        let err_val = conflict_res.unwrap_err();
        assert_eq!(err_val["payload"]["error_type"], "conflict");
        let err_text = err_val["payload"]["error"].as_str().unwrap_or("");
        assert!(err_text.contains("Idempotency conflict"), "Error must specify conflict: {}", err_text);
    }

    // --------------------------------------------------------------------------
    // Test: Durable Outbox crash recovery after SQLite commit
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_durable_outbox_multi_event_no_overwrite_and_delivered_on_reconcile() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_cfg_durable_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        let _ = std::fs::write(&test_cfg_path, "{}");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;

        let ctx = setup_test_context();
        let now = crate::now_ms();

        // 构造两个独立的待投递事件 A 与 B，分别修改不同的配置项
        let req_id_a = "req-durable-multi-001";
        let ops_a = serde_json::json!([
            {"op": "set_config", "key": "theme", "value": "dark"}
        ]);
        let body_bytes_a = serde_json::to_vec(&ops_a).unwrap();
        let env_a = make_test_envelope(&ctx, "push", req_id_a, &body_bytes_a, now);
        let event_id_a = format!("evt-{}-{}", ctx.session_id, req_id_a);

        let req_id_b = "req-durable-multi-002";
        let ops_b = serde_json::json!([
            {"op": "set_config", "key": "uiScale", "value": 1.25}
        ]);
        let body_bytes_b = serde_json::to_vec(&ops_b).unwrap();
        let _env_b = make_test_envelope(&ctx, "push", req_id_b, &body_bytes_b, now + 10);
        let event_id_b = format!("evt-{}-{}", ctx.session_id, req_id_b);

        let resp_payload = serde_json::json!({
            "status": "ok",
            "type": "commit_ack"
        });
        let resp_str = resp_payload.to_string();

        // 1. 模拟在 SQLite commit 之后、尚未消费调谐前进程崩溃：
        // 数据库中 idempotency_cache 已经是 completed，rpc_staged_outbox 存有 A、B 两笔 pending 记录
        {
            let conn = ctx.db.lock().unwrap();
            conn.execute(
                "INSERT INTO rpc_idempotency_cache
                 (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
                 VALUES (?, ?, 'push', ?, ?, ?, 'completed', ?, ?, ?, 1, 'tok-crash-a')",
                rusqlite::params![
                    ctx.session_id,
                    req_id_a,
                    ctx.mobile_device_id,
                    ctx.target_device_id,
                    compute_sha512(&body_bytes_a),
                    resp_str,
                    now,
                    now,
                ],
            ).unwrap();

            conn.execute(
                "INSERT INTO rpc_staged_outbox (event_id, session_id, request_id, operations_json, status, attempts, created_at)
                 VALUES (?, ?, ?, ?, 'pending', 0, ?)",
                rusqlite::params![event_id_a, ctx.session_id, req_id_a, serde_json::to_string(&ops_a).unwrap(), now],
            ).unwrap();

            conn.execute(
                "INSERT INTO rpc_idempotency_cache
                 (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
                 VALUES (?, ?, 'push', ?, ?, ?, 'completed', ?, ?, ?, 1, 'tok-crash-b')",
                rusqlite::params![
                    ctx.session_id,
                    req_id_b,
                    ctx.mobile_device_id,
                    ctx.target_device_id,
                    compute_sha512(&body_bytes_b),
                    resp_str,
                    now + 10,
                    now + 10,
                ],
            ).unwrap();

            conn.execute(
                "INSERT INTO rpc_staged_outbox (event_id, session_id, request_id, operations_json, status, attempts, created_at)
                 VALUES (?, ?, ?, ?, 'pending', 0, ?)",
                rusqlite::params![event_id_b, ctx.session_id, req_id_b, serde_json::to_string(&ops_b).unwrap(), now + 10],
            ).unwrap();
        }

        // 2. 客户端发起重试请求：必须命中 completed 缓存返回 200 OK，且数据库 staged outbox 不重复插入
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env_a).unwrap().parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        let retry_resp = handle_sync_push(
            State(ctx.state.clone()),
            ConnectInfo(ctx.addr),
            headers,
            body_bytes_a.into(),
        ).await.into_response();
        assert_eq!(retry_resp.status(), axum::http::StatusCode::OK);

        {
            let conn = ctx.db.lock().unwrap();
            let count_a: i64 = conn.query_row(
                "SELECT COUNT(1) FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id_a],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(count_a, 1, "Staged outbox must not duplicate on retry");
        }

        // 3. 执行 drain_staged_outbox 进行批量消费
        let delivered_count = {
            let conn = ctx.db.lock().unwrap();
            drain_staged_outbox(&conn, now + 1000).expect("Drain staged outbox must succeed")
        };
        // 关键断言：2 个待投递事件全部被处理并交付，绝不互相覆盖！
        assert_eq!(delivered_count, 2, "Both pending outbox events must be delivered without overwrite");

        // 4. 验证数据库状态：两个事件在 rpc_staged_outbox 中均转为 delivered
        {
            let conn = ctx.db.lock().unwrap();
            let status_a: String = conn.query_row(
                "SELECT status FROM rpc_staged_outbox WHERE event_id = ?",
                rusqlite::params![event_id_a],
                |r| r.get(0),
            ).unwrap();
            let status_b: String = conn.query_row(
                "SELECT status FROM rpc_staged_outbox WHERE event_id = ?",
                rusqlite::params![event_id_b],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(status_a, "delivered");
            assert_eq!(status_b, "delivered");

            // 验证 rpc_processed_events 去重表内真实持久化记录了两笔消费记录
            let processed_count: i64 = conn.query_row(
                "SELECT COUNT(1) FROM rpc_processed_events WHERE event_id IN (?, ?)",
                rusqlite::params![event_id_a, event_id_b],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(processed_count, 2, "Both events must be recorded in rpc_processed_events");
        }

        // 5. 关键安全断言：业务配置真正生效且两者全部存在（证明不是仅写覆盖文件，且两笔修改均完整物化）
        let final_config = crate::read_config();
        assert_eq!(final_config.get("theme").and_then(|v| v.as_str()), Some("dark"), "Event A theme=dark must be applied");
        assert_eq!(final_config.get("uiScale").and_then(|v| v.as_f64()), Some(1.25), "Event B uiScale=1.25 must be applied");

        // 6. 重复执行 drain：因为已在 rpc_processed_events 中，再次调用不会重复应用
        let re_delivered = {
            let conn = ctx.db.lock().unwrap();
            drain_staged_outbox(&conn, now + 2000).expect("Subsequent drain must succeed")
        };
        assert_eq!(re_delivered, 0, "No pending events to deliver on subsequent drain");
    }

    // --------------------------------------------------------------------------
    // Test: push_db atomic business write and idempotency on physical SQLite reopen
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_push_db_atomic_business_write_and_idempotency_physical_reopen() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_atomic_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let db_path = temp_dir.join("bob.db");

        // 1. 在物理磁盘建立并初始化 SQLite 数据库
        let mut conn = crate::db::init_db(&temp_dir);
        crate::device_trust::init_device_trust_tables(&conn).unwrap();

        let (_target_sk, target_device_id) = generate_test_keypair();
        let (mobile_sk, mobile_device_id) = generate_test_keypair();
        let now = crate::now_ms();

        // 建立配对与会话
        let invite = create_pairing_invitation(&conn, &target_device_id, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &target_device_id,
            &mobile_device_id,
            &mobile_device_id,
            "Phone",
            "android",
            "nonce-phys",
            now,
            "pairing_establishment",
        );
        let sig = sign_bytes(&mobile_sk, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: target_device_id.clone(),
            subject_device_id: mobile_device_id.clone(),
            subject_pubkey: mobile_device_id.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-phys".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &mobile_device_id, &target_device_id, 86400_000, now).unwrap();

        let req_id = "req-atomic-pushdb-001";
        let execution_token = "tok-atomic-work-01";

        // 预插入 pending 状态的幂等记录
        conn.execute(
            "INSERT INTO rpc_idempotency_cache
             (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push_db', ?, ?, 'dummy_hash', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![session.session_id, req_id, mobile_device_id, target_device_id, now, now, execution_token],
        ).unwrap();

        let initial_config = crate::read_config();
        let test_theme = format!("emerald-theme-{}", uuid::Uuid::new_v4());
        let test_lang = "en-US";

        // 构造包含全量业务表的待同步数据 (conversations, events, kg_nodes, 以及配置变更)
        let mut sync_data = empty_sync_data();
        sync_data.config = serde_json::json!({
            "theme": test_theme,
            "language": test_lang,
        });
        sync_data.conversations.push(serde_json::json!({
            "id": "conv-test-atomic-1",
            "title": "Chat Atomic 1",
            "model": "gpt-4",
            "cost": 0.0,
            "last_message": "hello",
            "last_role": "user",
            "created_at": now,
            "updated_at": now,
        }));
        sync_data.events.push(serde_json::json!({
            "id": "evt-test-atomic-1",
            "title": "Meeting Atomic 1",
            "type": "event",
            "status": "pending",
            "date": "2026-09-22",
            "start_time": "10:00",
            "end_time": "11:00",
            "description": "Atomic Sync Event",
            "created_at": now,
            "updated_at": now,
            "completed_at": 0,
            "linked_ticket_id": null,
        }));
        sync_data.kg_nodes.push(serde_json::json!({
            "id": "node-test-atomic-1",
            "label": "Atomic Knowledge Node",
            "node_type": "concept",
            "summary": "Atomic Sync Test Summary",
            "source": "manual",
            "metadata": "{}",
            "created_at": "2026-09-22T00:00:00Z",
        }));

        // ----------------------------------------------------------------------
        // Phase 1: 故障注入回滚 — 模拟执行 token 校验失败导致事务回滚
        // ----------------------------------------------------------------------
        let fail_info = crate::sync_engine::IdempotencyCommitInfo {
            session_id: &session.session_id,
            request_id: req_id,
            execution_token: "stale-wrong-token", // 错误的 token
            response_json: "{\"status\":\"ok\"}",
            now_ms: now,
        };
        let fail_res = crate::sync_engine::import_sync_data_to_conn_atomic(&mut conn, &sync_data, 0, Some(fail_info));
        assert!(fail_res.is_err(), "Mismatched execution token must cause transaction rollback");

        // 关键安全断言：事务回滚后，物理 config.json 绝不受到任何污染！
        let rolled_back_config = crate::read_config();
        assert_ne!(
            rolled_back_config.get("theme").and_then(|v| v.as_str()),
            Some(test_theme.as_str()),
            "config.json must NOT be updated when transaction rolls back"
        );
        assert_eq!(
            rolled_back_config,
            initial_config,
            "config.json must remain identical to initial state after rollback"
        );

        // 物理关闭数据库连接并从磁盘物理重开探查
        drop(conn);
        {
            let conn2 = rusqlite::Connection::open(&db_path).unwrap();
            let conv_count: i64 = conn2.query_row("SELECT COUNT(*) FROM conversations WHERE id = 'conv-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(conv_count, 0, "conversations must have 0 records after rollback (zero dirty writes)");
            let event_count: i64 = conn2.query_row("SELECT COUNT(*) FROM events WHERE id = 'evt-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(event_count, 0, "events must have 0 records after rollback (zero dirty writes)");
            let node_count: i64 = conn2.query_row("SELECT COUNT(*) FROM kg_nodes WHERE id = 'node-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(node_count, 0, "kg_nodes must have 0 records after rollback (zero dirty writes)");

            let idem_status: String = conn2.query_row(
                "SELECT status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                rusqlite::params![session.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(idem_status, "pending", "Idempotency cache must still be pending after rollback");
        }

        // ----------------------------------------------------------------------
        // Phase 2: 原子提交成功 — 单事务内业务表写入 + completed 提交 + 配置成功物化
        // ----------------------------------------------------------------------
        let mut conn2 = rusqlite::Connection::open(&db_path).unwrap();
        let ok_info = crate::sync_engine::IdempotencyCommitInfo {
            session_id: &session.session_id,
            request_id: req_id,
            execution_token, // 正确的 token
            response_json: "{\"status\":\"ok\",\"type\":\"commit_ack\"}",
            now_ms: now,
        };
        let ok_res = crate::sync_engine::import_sync_data_to_conn_atomic(&mut conn2, &sync_data, 0, Some(ok_info));
        assert!(ok_res.is_ok(), "Atomic import and idempotency commit must succeed: {:?}", ok_res);

        // 关键断言：事务提交成功后，物理 config.json 成功物化写入
        let committed_config = crate::read_config();
        assert_eq!(
            committed_config.get("theme").and_then(|v| v.as_str()),
            Some(test_theme.as_str()),
            "config.json must be updated after atomic transaction commit"
        );
        assert_eq!(
            committed_config.get("language").and_then(|v| v.as_str()),
            Some(test_lang),
            "config.json language must be updated after atomic transaction commit"
        );

        // 再次物理关闭连接并重新打开磁盘数据库探查
        drop(conn2);
        {
            let conn3 = rusqlite::Connection::open(&db_path).unwrap();
            let conv_count: i64 = conn3.query_row("SELECT COUNT(*) FROM conversations WHERE id = 'conv-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(conv_count, 1, "conversations must have exactly 1 record after atomic commit");
            let event_count: i64 = conn3.query_row("SELECT COUNT(*) FROM events WHERE id = 'evt-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(event_count, 1, "events must have exactly 1 record after atomic commit");
            let node_count: i64 = conn3.query_row("SELECT COUNT(*) FROM kg_nodes WHERE id = 'node-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(node_count, 1, "kg_nodes must have exactly 1 record after atomic commit");

            let idem_status: String = conn3.query_row(
                "SELECT status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                rusqlite::params![session.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(idem_status, "completed", "Idempotency cache must be completed atomically with business writes");
        }

        // ----------------------------------------------------------------------
        // Phase 3: 重试命中 completed 缓存，业务表记录数严格保持为 1
        // ----------------------------------------------------------------------
        {
            let conn4 = rusqlite::Connection::open(&db_path).unwrap();
            let cached_resp: String = conn4.query_row(
                "SELECT response_json FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ? AND status = 'completed'",
                rusqlite::params![session.session_id, req_id],
                |r| r.get(0),
            ).unwrap();
            assert!(cached_resp.contains("commit_ack"));

            let conv_count: i64 = conn4.query_row("SELECT COUNT(*) FROM conversations WHERE id = 'conv-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(conv_count, 1, "conversations count must stay 1 on retry");
            let event_count: i64 = conn4.query_row("SELECT COUNT(*) FROM events WHERE id = 'evt-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(event_count, 1, "events count must stay 1 on retry");
            let node_count: i64 = conn4.query_row("SELECT COUNT(*) FROM kg_nodes WHERE id = 'node-test-atomic-1'", [], |r| r.get(0)).unwrap();
            assert_eq!(node_count, 1, "kg_nodes count must stay 1 on retry");
        }
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --------------------------------------------------------------------------
    // Test: Schema migration fail-closed behavior and PRAGMA validation
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_schema_migration_fail_closed_and_pragmas() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE test_mig (id INTEGER PRIMARY KEY);").unwrap();

        // 1. Column 不存在：成功执行 ALTER TABLE ADD COLUMN
        let res1 = ensure_column_exists(&conn, "test_mig", "new_col", "TEXT NOT NULL DEFAULT 'val'");
        assert!(res1.is_ok(), "Adding missing column must succeed");

        // 验证 PRAGMA table_info 中确实存在新字段
        let col_exists: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(test_mig)").unwrap();
            let mut rows = stmt.query([]).unwrap();
            let mut found = false;
            while let Some(row) = rows.next().unwrap() {
                let name: String = row.get(1).unwrap();
                if name.eq_ignore_ascii_case("new_col") {
                    found = true;
                    break;
                }
            }
            found
        };
        assert!(col_exists, "new_col must exist in table_info");

        // 2. 幂等性：再次调用必须成功且无副作用
        let res2 = ensure_column_exists(&conn, "test_mig", "new_col", "TEXT NOT NULL DEFAULT 'val'");
        assert!(res2.is_ok(), "Idempotent re-run must succeed without error");

        // 3. Fail-Closed：非法列定义（如 ALTER TABLE ADD COLUMN 不允许 PRIMARY KEY）必须返回 Err，严禁吞错
        let res_bad_def = ensure_column_exists(&conn, "test_mig", "another_col", "INTEGER PRIMARY KEY");
        assert!(res_bad_def.is_err(), "Invalid column definition (PRIMARY KEY in ALTER TABLE) must return Err (fail-closed)");

        // 4. Fail-Closed：不存在的表执行 ALTER TABLE 必须失败返回 Err
        let res_bad_tbl = ensure_column_exists(&conn, "non_existent_table", "col1", "TEXT");
        assert!(res_bad_tbl.is_err(), "Non-existent table ALTER TABLE must fail closed");
    }

    // --------------------------------------------------------------------------
    // Test: Legacy database schema without status field upgrades cleanly
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_legacy_schema_migration_without_status_upgrades_cleanly() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();

        // 1. 创建真实旧版本表结构（不含 status、execution_token、attempts 等新列，且无 rpc_processed_events）
        conn.execute_batch(
            "CREATE TABLE authenticated_sessions (
                session_id TEXT PRIMARY KEY,
                subject_device_id TEXT NOT NULL,
                target_device_id TEXT NOT NULL,
                shared_key_enc TEXT NOT NULL,
                established_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE rpc_anti_replay (
                nonce TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE rpc_idempotency_cache (
                session_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                subject_device_id TEXT NOT NULL,
                target_device_id TEXT NOT NULL,
                action TEXT NOT NULL,
                payload_hash TEXT NOT NULL,
                response_json TEXT,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (session_id, request_id)
            );

            CREATE TABLE rpc_staged_outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                operations_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(session_id, request_id)
            );
            "
        ).unwrap();

        // 2. 调用完整的 init_device_trust_tables
        // 在修复前：会因 CREATE INDEX 引用不存在的 status 列而直接崩溃
        // 修复后：先补齐列再建索引，能够平滑升级成功
        let init_res = crate::device_trust::init_device_trust_tables(&conn);
        assert!(init_res.is_ok(), "Legacy database migration must succeed cleanly without errors: {:?}", init_res);

        // 3. 校验 PRAGMA table_info 确认所有升级列均已就绪
        let check_col = |tbl: &str, col: &str| -> bool {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", tbl)).unwrap();
            let mut rows = stmt.query([]).unwrap();
            let mut found = false;
            while let Some(row) = rows.next().unwrap() {
                let name: String = row.get(1).unwrap();
                if name.eq_ignore_ascii_case(col) {
                    found = true;
                    break;
                }
            }
            found
        };

        assert!(check_col("rpc_idempotency_cache", "status"));
        assert!(check_col("rpc_idempotency_cache", "updated_at"));
        assert!(check_col("rpc_idempotency_cache", "error_message"));
        assert!(check_col("rpc_idempotency_cache", "lease_generation"));
        assert!(check_col("rpc_idempotency_cache", "execution_token"));

        assert!(check_col("rpc_staged_outbox", "event_id"));
        assert!(check_col("rpc_staged_outbox", "status"));
        assert!(check_col("rpc_staged_outbox", "attempts"));
        assert!(check_col("rpc_staged_outbox", "last_error"));
        assert!(check_col("rpc_staged_outbox", "delivered_at"));

        // 4. 校验 rpc_processed_events 表存在
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name='rpc_processed_events'",
            [],
            |r| r.get::<_, i64>(0),
        ).unwrap() > 0;
        assert!(table_exists, "rpc_processed_events table must be created");

        // 5. 校验索引存在
        let index_exists = |idx_name: &str| -> bool {
            conn.query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type='index' AND name = ?",
                rusqlite::params![idx_name],
                |r| r.get::<_, i64>(0),
            ).unwrap() > 0
        };
        assert!(index_exists("idx_rpc_idempotency_status"), "idx_rpc_idempotency_status must exist");
        assert!(index_exists("idx_staged_outbox_status"), "idx_staged_outbox_status must exist");
        assert!(index_exists("idx_processed_events_req"), "idx_processed_events_req must exist");

        // 6. 验证升级后的数据库可正常进行 Durable Outbox 操作
        let now = crate::now_ms();
        conn.execute(
            "INSERT INTO rpc_staged_outbox (event_id, session_id, request_id, operations_json, status, attempts, created_at)
             VALUES ('evt-mig-1', 'sess-mig-1', 'req-mig-1', '[]', 'pending', 0, ?)",
            rusqlite::params![now],
        ).unwrap();

        let delivered = crate::device_trust::drain_staged_outbox(&conn, now).unwrap();
        assert_eq!(delivered, 1, "drain_staged_outbox must work on upgraded legacy database");
    }

    #[derive(Debug)]
    pub struct RealConfigBaseline {
        pub path: PathBuf,
        pub pending_path: PathBuf,
        pub backup_path: PathBuf,
        pub canonical_exists: bool,
        pub canonical_hash: Option<String>,
        pub pending_exists: bool,
        pub pending_hash: Option<String>,
        pub backup_exists: bool,
        pub backup_hash: Option<String>,
    }

    impl RealConfigBaseline {
        pub fn capture() -> Self {
            let path = crate::get_data_dir().join("config.json");
            let parent = path.parent().unwrap();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            let pending_path = parent.join(format!("{}.pending", file_name));
            let backup_path = parent.join(format!("{}.backup", file_name));

            let hash_file = |p: &Path| -> Option<String> {
                if p.exists() {
                    let mut file = std::fs::File::open(p).unwrap();
                    let mut hasher = Sha256::new();
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = file.read(&mut buf) {
                        if n == 0 { break; }
                        hasher.update(&buf[..n]);
                    }
                    Some(hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>())
                } else {
                    None
                }
            };

            Self {
                canonical_exists: path.exists(),
                canonical_hash: hash_file(&path),
                pending_exists: pending_path.exists(),
                pending_hash: hash_file(&pending_path),
                backup_exists: backup_path.exists(),
                backup_hash: hash_file(&backup_path),
                path,
                pending_path,
                backup_path,
            }
        }

        pub fn assert_unchanged(&self) {
            let hash_file = |p: &Path| -> Option<String> {
                if p.exists() {
                    let mut file = std::fs::File::open(p).unwrap();
                    let mut hasher = Sha256::new();
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = file.read(&mut buf) {
                        if n == 0 { break; }
                        hasher.update(&buf[..n]);
                    }
                    Some(hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>())
                } else {
                    None
                }
            };

            assert_eq!(self.path.exists(), self.canonical_exists, "Real canonical existence must be untouched");
            assert_eq!(hash_file(&self.path), self.canonical_hash, "Real canonical SHA-256 must be untouched");
            assert_eq!(self.pending_path.exists(), self.pending_exists, "Real pending existence must be untouched");
            assert_eq!(hash_file(&self.pending_path), self.pending_hash, "Real pending SHA-256 must be untouched");
            assert_eq!(self.backup_path.exists(), self.backup_exists, "Real backup existence must be untouched");
            assert_eq!(hash_file(&self.backup_path), self.backup_hash, "Real backup SHA-256 must be untouched");
        }
    }

    pub struct TestConfigOverrideGuard;
    impl Drop for TestConfigOverrideGuard {
        fn drop(&mut self) {
            crate::set_test_config_path_override(None);
            crate::set_inject_config_write_error(false);
            crate::set_inject_sync_dir_error(false);
            crate::set_config_write_failure_stage(crate::ConfigWriteFailureStage::None);
            crate::device_trust::set_device_trust_init_failed(false);
        }
    }

    // --------------------------------------------------------------------------
    // Test A: 配置写入失败被拒止且 Outbox 保持 pending (Fail-Closed)
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_e2e_config_write_failure_keeps_outbox_pending_and_omits_processed_event() {
        let _test_lock = crate::lock_config_test_mutex();
        let baseline_real = RealConfigBaseline::capture();

        let temp_dir = std::env::temp_dir().join(format!("bob_test_cfg_fail_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;

        let conn = crate::db::init_db(&temp_dir);
        crate::device_trust::init_device_trust_tables(&conn).unwrap();

        let now = crate::now_ms();
        let target_theme = format!("ruby-failure-test-{}", uuid::Uuid::new_v4());
        let event_id = "evt-write-fail-001";
        let session_id = "sess-write-fail-001";
        let request_id = "req-write-fail-001";
        let ops = serde_json::json!([
            {
                "op": "set_config",
                "key": "theme",
                "value": target_theme,
            }
        ]);
        let ops_str = serde_json::to_string(&ops).unwrap();

        // 1. 插入 pending 状态的配置写入 Outbox 事件
        conn.execute(
            "INSERT INTO rpc_staged_outbox (event_id, session_id, request_id, operations_json, status, attempts, created_at)
             VALUES (?, ?, ?, ?, 'pending', 0, ?)",
            rusqlite::params![event_id, session_id, request_id, ops_str, now],
        ).unwrap();

        // 2. 注入配置写入故障 (模拟磁盘写保护、I/O 错误或权限受限)
        crate::set_inject_config_write_error(true);

        // 3. 执行 drain_staged_outbox 进行消费
        let delivered_count = crate::device_trust::drain_staged_outbox(&conn, now + 1000)
            .expect("drain_staged_outbox must gracefully record failure and not crash");
        assert_eq!(delivered_count, 0, "Failed config write must deliver 0 events");

        // 4. 关键安全断言：
        // (1) rpc_staged_outbox 保持 pending，attempts 增加，记录了 last_error
        let (status, attempts, last_err): (String, i64, Option<String>) = conn.query_row(
            "SELECT status, attempts, last_error FROM rpc_staged_outbox WHERE event_id = ?",
            rusqlite::params![event_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(status, "pending", "Outbox status must remain pending on write failure");
        assert_eq!(attempts, 1, "Attempts must be incremented to 1");
        assert!(last_err.is_some(), "last_error must be recorded");
        assert!(last_err.unwrap().contains("simulated config write failure"), "last_error must record failure details");

        // (2) rpc_processed_events 表绝不能含有该事件记录
        let processed_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM rpc_processed_events WHERE event_id = ?",
            rusqlite::params![event_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(processed_count, 0, "Failed write must NOT be recorded in rpc_processed_events");

        // (3) 测试 config.json 绝不能被标记或更新为新 theme
        let current_cfg = crate::read_config();
        assert_ne!(
            current_cfg.get("theme").and_then(|v| v.as_str()),
            Some(target_theme.as_str()),
            "config.json must NOT be updated when write fails"
        );

        // 5. 故障恢复：恢复写能力并再次执行 drain_staged_outbox
        crate::set_inject_config_write_error(false);

        let recovered_count = crate::device_trust::drain_staged_outbox(&conn, now + 2000)
            .expect("Subsequent drain after fault clearance must succeed");
        assert_eq!(recovered_count, 1, "Cleared fault must allow delivering pending event");

        // 6. 验证恢复后状态闭环：
        let (status2, attempts2, last_err2): (String, i64, Option<String>) = conn.query_row(
            "SELECT status, attempts, last_error FROM rpc_staged_outbox WHERE event_id = ?",
            rusqlite::params![event_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(status2, "delivered", "Outbox status must now be delivered");
        assert_eq!(attempts2, 1, "Attempts remains preserved");
        assert!(last_err2.is_none(), "last_error must be cleared on successful delivery");

        let processed_count2: i64 = conn.query_row(
            "SELECT COUNT(1) FROM rpc_processed_events WHERE event_id = ?",
            rusqlite::params![event_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(processed_count2, 1, "Event must now be recorded in rpc_processed_events");

        let current_cfg2 = crate::read_config();
        assert_eq!(
            current_cfg2.get("theme").and_then(|v| v.as_str()),
            Some(target_theme.as_str()),
            "config.json must be successfully updated after fault cleared"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        baseline_real.assert_unchanged();
    }

    // --------------------------------------------------------------------------
    // 16. SEC-03 同步载荷脱敏：导出数据严格剥离本地 API 密钥、自定义模型密钥与 Settings Bot Token
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_export_sync_data_strips_credentials() {
        let _test_lock = crate::lock_config_test_mutex();
        let baseline_real = RealConfigBaseline::capture();

        let temp_dir = std::env::temp_dir().join(format!("bob_cfg_sec03_export_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let cfg_path = temp_dir.join("config.json");
        crate::set_test_config_path_override(Some(cfg_path.clone()));
        let _guard = TestConfigOverrideGuard;

        let sensitive_cfg = serde_json::json!({
            "device_id": "test-dev-1",
            "theme": "dark",
            "model": "deepseek-chat",
            "apiKeys": {
                "deepseek": "sk-sensitive-12345678",
                "openai": "sk-sensitive-87654321"
            },
            "customModels": [
                {
                    "id": "my-custom-model",
                    "name": "Custom Model 1",
                    "provider": "openai",
                    "model": "gpt-4-turbo",
                    "apiKey": "sk-custom-secret-key",
                    "baseUrl": "https://api.custom.com",
                    "nested_secret": { "token": "leak" },
                    "unknown_field": "drop_me"
                }
            ],
            "mcpServers": {
                "fetch": {
                    "command": "node",
                    "env": { "GITHUB_TOKEN": "ghp_secret123" }
                }
            },
            "pairing_payload": { "token": "pairing-secret-token" },
            "signing_key": "raw_private_signing_key_secret",
            "relay_secret": "relay_password_secret",
            "attacker_injected_unknown_field": "untrusted_value",
            "nested_credentials": {
                "gcp_token": "secret_token_123"
            }
        });
        std::fs::write(&cfg_path, serde_json::to_vec(&sensitive_cfg).unwrap()).unwrap();

        let db_path = temp_dir.join("test_settings.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('discord_token', 'bot-discord-secret')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('telegram_token', 'bot-telegram-secret')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('wechat_auth_key', 'wechat-secret')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('last_sync_ts', '1700000000')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('theme', 'dark')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('unknown_plugin_field', 'leak_attempt')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('session_bearer_token', 'bearer_leak')", []).unwrap();

        let sync_data = crate::sync_engine::export_sync_data_from_conn(&conn, 0, false).expect("export_sync_data_from_conn must succeed");

        // 验证正向白名单：config 中的敏感项与未知字段被 100% 过滤
        let exported_cfg = sync_data.config;
        assert_eq!(exported_cfg.get("apiKeys"), None, "apiKeys must be stripped");
        assert_eq!(exported_cfg.get("mcpServers"), None, "mcpServers must be stripped");
        assert_eq!(exported_cfg.get("pairing_payload"), None, "pairing_payload must be stripped");
        assert_eq!(exported_cfg.get("signing_key"), None, "signing_key must be stripped");
        assert_eq!(exported_cfg.get("relay_secret"), None, "relay_secret must be stripped");
        assert_eq!(exported_cfg.get("attacker_injected_unknown_field"), None, "unknown fields must be filtered by allowlist");
        assert_eq!(exported_cfg.get("nested_credentials"), None, "nested credentials must be filtered by allowlist");

        // 验证 customModels 正向白名单：仅提取 id, name, provider, model，剥离 apiKey 与未知嵌套字段
        if let Some(custom_models) = exported_cfg.get("customModels").and_then(|v| v.as_array()) {
            for m in custom_models {
                assert_eq!(m.get("apiKey"), None, "customModels[].apiKey must be stripped");
                assert_eq!(m.get("nested_secret"), None, "customModels[].nested_secret must be stripped");
                assert_eq!(m.get("unknown_field"), None, "customModels[].unknown_field must be stripped");
                assert_eq!(m.get("id").and_then(|v| v.as_str()), Some("my-custom-model"));
                assert_eq!(m.get("name").and_then(|v| v.as_str()), Some("Custom Model 1"));
                assert_eq!(m.get("provider").and_then(|v| v.as_str()), Some("openai"));
                assert_eq!(m.get("model").and_then(|v| v.as_str()), Some("gpt-4-turbo"));
            }
        }

        // 验证白名单内安全偏好仍然保留
        assert_eq!(exported_cfg.get("theme").and_then(|v| v.as_str()), Some("dark"));
        assert_eq!(exported_cfg.get("model").and_then(|v| v.as_str()), Some("deepseek-chat"));

        // 验证 settings 表仅导出明确允许的白名单 key
        let setting_keys: Vec<&str> = sync_data.settings.iter()
            .filter_map(|r| r.get("key").and_then(|v| v.as_str()))
            .collect();
        assert_eq!(setting_keys.len(), 2, "Settings must strictly contain only allowlisted keys");
        assert!(setting_keys.contains(&"last_sync_ts"));
        assert!(setting_keys.contains(&"theme"));
        assert!(!setting_keys.contains(&"discord_token"));
        assert!(!setting_keys.contains(&"telegram_token"));
        assert!(!setting_keys.contains(&"wechat_auth_key"));
        assert!(!setting_keys.contains(&"unknown_plugin_field"));
        assert!(!setting_keys.contains(&"session_bearer_token"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
        baseline_real.assert_unchanged();
    }

    // --------------------------------------------------------------------------
    // 17. SEC-03 跨端配置合并绝不接受对端 API Key，严格保证密钥留在本地执行设备
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_merge_synced_config_refuses_remote_api_keys() {
        let local_cfg = serde_json::json!({
            "theme": "dark",
            "model": "deepseek-chat",
            "apiKeys": {
                "deepseek": "local-sk-12345678"
            }
        });

        let remote_cfg = serde_json::json!({
            "theme": "light",
            "model": "qwen-max",
            "apiKeys": {
                "deepseek": "attacker-sk-overwritten",
                "openai": "remote-sk-87654321"
            }
        });

        let merged = crate::sync_engine::merge_synced_config(&local_cfg, &remote_cfg);

        // SEC-03: 严禁从对端合并 apiKeys
        assert_eq!(
            merged.get("apiKeys").and_then(|v| v.get("deepseek")).and_then(|v| v.as_str()),
            Some("local-sk-12345678"),
            "Local apiKey must not be overwritten"
        );
        assert_eq!(
            merged.get("apiKeys").and_then(|v| v.get("openai")),
            None,
            "Remote new apiKey must not be merged"
        );

        // 白名单安全偏好设置允许合并
        assert_eq!(merged.get("theme").and_then(|v| v.as_str()), Some("light"));
        assert_eq!(merged.get("model").and_then(|v| v.as_str()), Some("qwen-max"));
    }

    // --------------------------------------------------------------------------
    // 18. SEC-03 原子 Push 队列拒绝执行远程 set_api_key 与敏感 set_config (Fail-Closed)
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_push_outbox_rejects_credential_modification_fail_closed() {
        let ctx = setup_test_context();
        let mut conn = ctx.db.lock().unwrap();

        // 尝试推入 set_api_key 操作
        let mal_ops = vec![serde_json::json!({
            "op": "set_api_key",
            "provider": "openai",
            "value": "sk-attacker-injected"
        })];

        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn,
            &ctx.session_id,
            "req-mal-001",
            "token-001",
            &mal_ops,
            "{\"status\":\"ok\"}",
            crate::now_ms(),
        );

        assert!(res.is_err(), "atomic_commit_push_outbox must reject set_api_key fail-closed");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("SEC-03 Fail-Closed"), "Error must contain SEC-03 Fail-Closed: {}", err_msg);
        assert!(err_msg.contains("set_api_key is blocked"), "Error must explain set_api_key is blocked: {}", err_msg);

        // 尝试推入包含敏感 token 的 set_config 操作
        let mal_cfg_ops = vec![serde_json::json!({
            "op": "set_config",
            "key": "telegram_token",
            "value": "injected-token"
        })];

        let res_cfg = crate::device_trust::atomic_commit_push_outbox(
            &mut conn,
            &ctx.session_id,
            "req-mal-002",
            "token-002",
            &mal_cfg_ops,
            "{\"status\":\"ok\"}",
            crate::now_ms(),
        );

        assert!(res_cfg.is_err(), "atomic_commit_push_outbox must reject sensitive set_config fail-closed");
        let err_cfg = res_cfg.unwrap_err();
        assert!(err_cfg.contains("SEC-03 Fail-Closed"), "Error must contain SEC-03 Fail-Closed: {}", err_cfg);
    }

    // --------------------------------------------------------------------------
    // 19. SEC-02 LAN REST 接口全维度安全边界审计 (正向 + 身份伪造/目标越权/动作挪用/缺信封/载荷篡改)
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec02_lan_rest_sync_endpoints_security_boundary() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_cfg_lan_sec_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        let _ = std::fs::write(&test_cfg_path, "{}");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;

        let ctx = setup_test_context();
        let now = crate::now_ms();

        // 1. Positive: 正确签名的 pull 请求 -> 200 OK
        let env_pull_ok = make_test_envelope(&ctx, "pull", "req-sec02-pull-ok", b"", now);
        let mut headers_ok = axum::http::HeaderMap::new();
        headers_ok.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_ok.insert("x-auth-envelope", serde_json::to_string(&env_pull_ok).unwrap().parse().unwrap());
        let resp = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_ok).await.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        // 2. Negative: 冒充 X-Device-Id (调用者身份声明与信封签名主体不一致) -> 403 FORBIDDEN
        let env_pull_forged = make_test_envelope(&ctx, "pull", "req-sec02-pull-forged", b"", now + 1);
        let mut headers_forged = axum::http::HeaderMap::new();
        headers_forged.insert("x-device-id", "attacker_fake_device_id".parse().unwrap());
        headers_forged.insert("x-auth-envelope", serde_json::to_string(&env_pull_forged).unwrap().parse().unwrap());
        let resp_forged = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_forged).await.into_response();
        assert_eq!(resp_forged.status(), axum::http::StatusCode::FORBIDDEN);

        // 3. Negative: 目标设备不匹配 (信封 target_device_id 指向第三方设备) -> 403 FORBIDDEN
        let mut env_wrong_target = env_pull_ok.clone();
        env_wrong_target.request_id = "req-sec02-pull-wrong-target".to_string();
        env_wrong_target.target_device_id = "other_pc_device_id".to_string();
        env_wrong_target.signature = sign_bytes(
            &ctx.mobile_sk,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &ctx.session_id,
                &env_wrong_target.request_id,
                &ctx.mobile_device_id,
                "other_pc_device_id",
                "pull",
                &compute_sha512(b""),
                &env_wrong_target.nonce,
                now + 2,
            ),
        );
        let mut headers_wrong_target = axum::http::HeaderMap::new();
        headers_wrong_target.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_wrong_target.insert("x-auth-envelope", serde_json::to_string(&env_wrong_target).unwrap().parse().unwrap());
        let resp_target = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_wrong_target).await.into_response();
        assert_eq!(resp_target.status(), axum::http::StatusCode::FORBIDDEN);

        // 4. Negative: 动作挪用 (为 push 签署的信封挪用于 pull 接口) -> 403 FORBIDDEN
        let env_push_action = make_test_envelope(&ctx, "push", "req-sec02-action-mismatch", b"", now + 3);
        let mut headers_action = axum::http::HeaderMap::new();
        headers_action.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_action.insert("x-auth-envelope", serde_json::to_string(&env_push_action).unwrap().parse().unwrap());
        let resp_action = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_action).await.into_response();
        assert_eq!(resp_action.status(), axum::http::StatusCode::FORBIDDEN);

        // 5. Negative: 缺失信封 (未携带任何签名凭据) -> 401 UNAUTHORIZED
        let headers_missing = axum::http::HeaderMap::new();
        let resp_missing = handle_sync_pull(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_missing).await.into_response();
        assert_eq!(resp_missing.status(), axum::http::StatusCode::UNAUTHORIZED);

        // 6. Positive: 正确签名的 push 请求 -> 200 OK
        let push_ops = serde_json::json!([{"op": "set_config", "key": "theme", "value": "light"}]);
        let push_bytes = serde_json::to_vec(&push_ops).unwrap();
        let env_push_ok = make_test_envelope(&ctx, "push", "req-sec02-push-ok", &push_bytes, now + 4);
        let mut headers_push_ok = axum::http::HeaderMap::new();
        headers_push_ok.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_push_ok.insert("x-auth-envelope", serde_json::to_string(&env_push_ok).unwrap().parse().unwrap());
        headers_push_ok.insert("content-type", "application/json".parse().unwrap());
        let resp_push = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_push_ok, push_bytes.clone().into()).await.into_response();
        assert_eq!(resp_push.status(), axum::http::StatusCode::OK);

        // 7. Negative: 载荷篡改 (Body 在信封签名后被恶意修改) -> 401 UNAUTHORIZED
        let tampered_bytes = serde_json::to_vec(&serde_json::json!([{"op": "set_config", "key": "theme", "value": "tampered"}])).unwrap();
        let mut headers_tampered = axum::http::HeaderMap::new();
        headers_tampered.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_tampered.insert("x-auth-envelope", serde_json::to_string(&env_push_ok).unwrap().parse().unwrap());
        headers_tampered.insert("content-type", "application/json".parse().unwrap());
        let resp_tampered = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_tampered, tampered_bytes.into()).await.into_response();
        assert_eq!(resp_tampered.status(), axum::http::StatusCode::UNAUTHORIZED);

        // 8. Negative: push 载荷中包含 set_api_key -> 触发 SEC-03 拒斥
        let mal_key_ops = serde_json::json!([{"op": "set_api_key", "provider": "deepseek", "value": "sk-injected"}]);
        let mal_key_bytes = serde_json::to_vec(&mal_key_ops).unwrap();
        let env_mal_key = make_test_envelope(&ctx, "push", "req-sec02-push-mal-key", &mal_key_bytes, now + 5);
        let mut headers_mal_key = axum::http::HeaderMap::new();
        headers_mal_key.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_mal_key.insert("x-auth-envelope", serde_json::to_string(&env_mal_key).unwrap().parse().unwrap());
        headers_mal_key.insert("content-type", "application/json".parse().unwrap());
        let resp_mal = handle_sync_push(State(ctx.state.clone()), ConnectInfo(ctx.addr), headers_mal_key, mal_key_bytes.into()).await.into_response();
        assert_eq!(resp_mal.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let resp_mal_bytes = axum::body::to_bytes(resp_mal.into_body(), usize::MAX).await.unwrap();
        let resp_mal_val: Value = serde_json::from_slice(&resp_mal_bytes).unwrap();
        let err_text = resp_mal_val["message"].as_str().unwrap_or("");
        assert!(err_text.contains("SEC-03 Fail-Closed"), "Push containing set_api_key must be rejected with SEC-03 Fail-Closed, got: {}", err_text);
    }

    // --------------------------------------------------------------------------
    // 20. SEC-02 WebSocket /v1/sync 连接绑定强校验 (Connection-Subject Pinning 防跨设备伪造)
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec02_ws_sync_request_connection_subject_pinning() {
        use futures_util::{SinkExt, StreamExt};
        let ctx = setup_test_context();
        let now = crate::now_ms();

        // 启动本地 Axum 测试监听器
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let router = create_public_router_with_state(ctx.state.clone());
        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
        });

        // 准备 Device A (合法认证的已配对设备) 的 WebSocket 认证 Header
        let ws_auth_env_a = make_test_envelope(&ctx, "ws_connect", "ws-connect-001", b"", now);
        let ws_auth_env_str = serde_json::to_string(&ws_auth_env_a).unwrap();

        let ws_url = format!("ws://{}/v1/sync", local_addr);
        let req = tokio_tungstenite::tungstenite::handshake::client::Request::builder()
            .uri(&ws_url)
            .header("Host", local_addr.to_string())
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("Sec-WebSocket-Version", "13")
            .header("X-Device-Id", &ctx.mobile_device_id)
            .header("X-Auth-Envelope", &ws_auth_env_str)
            .body(())
            .unwrap();

        let (mut ws_stream, _) = tokio_tungstenite::connect_async(req).await.expect("WebSocket connection with Device A must succeed");

        // 消费初始的 welcome auth_ok 帧
        let welcome_msg = ws_stream.next().await.expect("Expected welcome frame").unwrap();
        if let tokio_tungstenite::tungstenite::protocol::Message::Text(w_str) = welcome_msg {
            let w_json: Value = serde_json::from_str(&w_str).unwrap();
            assert_eq!(w_json["type"], "auth_ok");
        }

        // 构造由另一个伪造设备 Device B 签署的 sync_request 信封
        let (dev_b_sk, dev_b_id) = generate_test_keypair();
        let nonce_b = format!("nonce-{}", uuid::Uuid::new_v4());
        let sig_b = sign_bytes(
            &dev_b_sk,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &ctx.session_id,
                "req-b-sync-001",
                &dev_b_id,
                &ctx.target_device_id,
                "sync",
                &compute_sha512(b""),
                &nonce_b,
                now + 10,
            ),
        );
        let env_b = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: ctx.session_id.clone(),
            request_id: "req-b-sync-001".to_string(),
            subject_device_id: dev_b_id.clone(),
            target_device_id: ctx.target_device_id.clone(),
            action: "sync".to_string(),
            payload_hash: compute_sha512(b""),
            nonce: nonce_b,
            timestamp: now + 10,
            signature: sig_b,
        };

        // 通过 Device A 的 WebSocket 发送 Device B 的信封
        let frame = serde_json::json!({
            "type": "sync_request",
            "envelope": env_b,
            "payload": {}
        });
        ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(frame.to_string().into())).await.unwrap();

        // 接收响应：必须返回 Subject device mismatch 错误
        let msg = ws_stream.next().await.expect("Expected response frame").unwrap();
        if let tokio_tungstenite::tungstenite::protocol::Message::Text(resp_str) = msg {
            let resp_json: Value = serde_json::from_str(&resp_str).unwrap();
            assert_eq!(resp_json["type"], "error");
            let err_msg = resp_json["message"].as_str().unwrap_or("");
            assert!(err_msg.contains("Subject device mismatch"), "Must reject mismatched envelope subject, got: {}", err_msg);
        } else {
            panic!("Expected text frame, got: {:?}", msg);
        }
    }


    // --------------------------------------------------------------------------
    // 21. SEC-03 原子 Push 队列正向白名单与负向测试 (拒绝未知操作、未知键、嵌套凭据字典)
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_atomic_commit_push_outbox_positive_allowlist_negative_tests() {
        let ctx = setup_test_context();
        let mut conn = ctx.db.lock().unwrap();
        let now = crate::now_ms();

        // 1. 非对象操作载荷 -> 必须 Fail-Closed 拒绝
        let raw_scalar_ops = vec![serde_json::json!("just_a_string")];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-001", "tok-001", &raw_scalar_ops, "{}", now,
        );
        assert!(res.is_err(), "Raw scalar op must be rejected");
        assert!(res.unwrap_err().contains("SEC-03 Fail-Closed"));

        // 2. 未知操作类型 -> 必须 Fail-Closed 拒绝
        let unknown_op = vec![serde_json::json!({"op": "malicious_eval", "code": "process.exit()"})];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-002", "tok-002", &unknown_op, "{}", now,
        );
        assert!(res.is_err(), "Unknown op must be rejected");
        assert!(res.unwrap_err().contains("未知或不允许的操作类型"));

        // 3. set_config 包含不在白名单中的未知键 -> 必须 Fail-Closed 拒绝
        let unknown_key_op = vec![serde_json::json!({"op": "set_config", "key": "arbitrary_custom_url", "value": "http://evil.com"})];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-003", "tok-003", &unknown_key_op, "{}", now,
        );
        assert!(res.is_err(), "Unknown config key must be rejected");
        assert!(res.unwrap_err().contains("不在允许的配置白名单中"));

        // 4. set_config 尝试传递嵌套对象（嵌套凭据字典） -> 必须 Fail-Closed 拒绝
        let nested_obj_op = vec![serde_json::json!({
            "op": "set_config",
            "key": "theme",
            "value": { "nested_token": "sk-smuggled-secret" }
        })];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-004", "tok-004", &nested_obj_op, "{}", now,
        );
        assert!(res.is_err(), "Nested object in set_config must be rejected");
        assert!(res.unwrap_err().contains("必须为标量"));

        // 5. set_config 尝试传递嵌套数组 -> 必须 Fail-Closed 拒绝
        let nested_arr_op = vec![serde_json::json!({
            "op": "set_config",
            "key": "theme",
            "value": ["token_in_array"]
        })];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-005", "tok-005", &nested_arr_op, "{}", now,
        );
        assert!(res.is_err(), "Nested array in set_config must be rejected");
        assert!(res.unwrap_err().contains("必须为标量"));

        // 6. create_item 等非白名单操作 (即使伪装正常) -> 必须 Fail-Closed 拒绝
        let create_item_op = vec![serde_json::json!({
            "action": "create_item",
            "item_id": "it-safe-1"
        })];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-006", "tok-006", &create_item_op, "{}", now,
        );
        assert!(res.is_err(), "create_item must be rejected as non-whitelisted op");
        assert!(res.unwrap_err().contains("未知或不允许的操作类型"));

        // 7. add / delete 等非白名单操作 -> 必须 Fail-Closed 拒绝
        let add_op = vec![serde_json::json!({
            "op": "add",
            "item_id": "it-safe-2"
        })];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-007", "tok-007", &add_op, "{}", now,
        );
        assert!(res.is_err(), "add must be rejected as non-whitelisted op");
        assert!(res.unwrap_err().contains("未知或不允许的操作类型"));

        // 8. set_config 尝试伪装携带敏感 token/secret 字段 -> 必须 Fail-Closed 拒绝
        let smuggled_cred_op = vec![serde_json::json!({
            "op": "set_config",
            "key": "theme",
            "value": "dark",
            "telegram_token": "smuggled_secret_bot_token"
        })];
        let res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, "req-neg-008", "tok-008", &smuggled_cred_op, "{}", now,
        );
        assert!(res.is_err(), "Smuggled credential in set_config must be rejected");
        assert!(res.unwrap_err().contains("未知或敏感夹带字段"));
    }

    // --------------------------------------------------------------------------
    // 22. SEC-03 异常非对象配置阻断策略与合并安全保障 (Fail-Closed Guarantees)
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_malformed_config_fail_closed_guarantees() {
        let temp_dir = std::env::temp_dir().join(format!("bob_malformed_cfg_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);

        // 1. read_config_checked_at 遇到非对象 JSON 必须 Fail-Closed 拒绝
        let test_cases = vec![
            ("string.json", r#""just a string""#),
            ("array.json", "[1, 2, 3]"),
            ("number.json", "12345"),
            ("boolean.json", "true"),
        ];
        for (fname, raw_val) in test_cases {
            let p = temp_dir.join(fname);
            std::fs::write(&p, raw_val).unwrap();
            let res = crate::read_config_checked_at(&p);
            assert!(res.is_err(), "Non-object config {} must be rejected", fname);
            assert!(res.unwrap_err().contains("SEC-01/03 Fail-Closed"));
        }

        // 2. merge_synced_config 遇 local 为非对象时，必须拒绝合并，绝不返回 remote
        let malformed_local = serde_json::json!("corrupted_string_config");
        let remote_with_keys = serde_json::json!({
            "theme": "dark",
            "apiKeys": { "openai": "sk-attacker-smuggled" },
            "custom_secret": "leak"
        });
        let merged_when_local_malformed = crate::sync_engine::merge_synced_config(&malformed_local, &remote_with_keys);
        assert_eq!(merged_when_local_malformed, malformed_local, "Must keep local unchanged and NEVER return remote");

        // 3. merge_synced_config 遇 remote 为非对象时，保持 local 原样
        let valid_local = serde_json::json!({ "theme": "light", "model": "gpt-4" });
        let malformed_remote = serde_json::json!(["array_instead_of_object"]);
        let merged_when_remote_malformed = crate::sync_engine::merge_synced_config(&valid_local, &malformed_remote);
        assert_eq!(merged_when_remote_malformed, valid_local);

        // 4. merge_synced_config 严格只合并白名单键，忽略未知键和敏感键
        let remote_mixed = serde_json::json!({
            "theme": "dark",
            "model": "qwen-max",
            "apiKeys": { "deepseek": "sk-leak" },
            "arbitrary_untrusted_key": "injected",
            "nested_obj": { "key": "val" }
        });
        let merged_clean = crate::sync_engine::merge_synced_config(&valid_local, &remote_mixed);
        assert_eq!(merged_clean.get("theme").and_then(|v| v.as_str()), Some("dark"));
        assert_eq!(merged_clean.get("model").and_then(|v| v.as_str()), Some("qwen-max"));
        assert_eq!(merged_clean.get("apiKeys"), None);
        assert_eq!(merged_clean.get("arbitrary_untrusted_key"), None);
        assert_eq!(merged_clean.get("nested_obj"), None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --------------------------------------------------------------------------
    // 23. SEC-02 LAN Outbox 推送失败保留与重试测试 ( Durability & Fail-Retention )
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec02_lan_outbox_push_failure_retention_and_retry() {
        use std::sync::atomic::{AtomicU8, Ordering};
        use axum::routing::post;
        use axum::extract::State;
        use axum::response::IntoResponse;

        let temp_dir = std::env::temp_dir().join(format!("bob_lan_outbox_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let outbox_path = temp_dir.join("bob_mobile_outbox.json");

        // 共享状态：1 = 500, 2 = 401, 3 = 200 without commit_ack, 4 = 200 with commit_ack
        let server_mode = Arc::new(AtomicU8::new(1));
        let mode_clone = server_mode.clone();

        let app = axum::Router::new()
            .route("/v1/sync/push", post(|State(m): State<Arc<AtomicU8>>| async move {
                match m.load(Ordering::SeqCst) {
                    1 => (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        axum::Json(serde_json::json!({ "status": "error", "message": "Simulated database write error" }))
                    ).into_response(),
                    2 => (
                        axum::http::StatusCode::UNAUTHORIZED,
                        axum::Json(serde_json::json!({ "status": "error", "message": "Invalid auth envelope" }))
                    ).into_response(),
                    3 => (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({ "status": "error", "message": "Queue temporarily busy" }))
                    ).into_response(),
                    4 => (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({ "status": "ok", "type": "commit_ack" }))
                    ).into_response(),
                    _ => (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({ "status": "ok", "type": "commit_ack", "stage": "applied" }))
                    ).into_response(),
                }
            }))
            .with_state(mode_clone);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let push_url = format!("http://127.0.0.1:{}/v1/sync/push", port);
        let push_bytes = serde_json::to_vec(&serde_json::json!([
            {"op": "set_config", "key": "theme", "value": "dark"}
        ])).unwrap();

        // 写入初始本地 outbox 文件
        std::fs::write(&outbox_path, &push_bytes).unwrap();
        assert!(outbox_path.exists(), "Outbox file must exist initially");

        // 场景 A: 远端返回 HTTP 500 -> 失败，本地 outbox 必须完整保留
        server_mode.store(1, Ordering::SeqCst);
        let res_500 = crate::sync_engine::execute_lan_outbox_push(
            &client, &push_url, "dev-mob-1", "android", "Phone", "env-test", push_bytes.clone(), &outbox_path,
        ).await;
        assert!(res_500.is_err(), "HTTP 500 must return error");
        assert!(outbox_path.exists(), "Outbox file MUST be retained after HTTP 500 failure!");

        // 场景 B: 远端返回 HTTP 401 -> 失败，本地 outbox 必须完整保留
        server_mode.store(2, Ordering::SeqCst);
        let res_401 = crate::sync_engine::execute_lan_outbox_push(
            &client, &push_url, "dev-mob-1", "android", "Phone", "env-test", push_bytes.clone(), &outbox_path,
        ).await;
        assert!(res_401.is_err(), "HTTP 401 must return error");
        assert!(outbox_path.exists(), "Outbox file MUST be retained after HTTP 401 failure!");

        // 场景 C: 远端返回 HTTP 200 但没有 status: ok 回执 -> 失败，本地 outbox 必须完整保留
        server_mode.store(3, Ordering::SeqCst);
        let res_no_ack = crate::sync_engine::execute_lan_outbox_push(
            &client, &push_url, "dev-mob-1", "android", "Phone", "env-test", push_bytes.clone(), &outbox_path,
        ).await;
        assert!(res_no_ack.is_err(), "Missing commit receipt must return error");
        assert!(outbox_path.exists(), "Outbox file MUST be retained when commit receipt is missing!");

        // 场景 D: 远端返回 HTTP 200 带有 status: ok 回执但缺失 stage -> 必须 Fail-Closed 拒斥，本地 outbox 必须完整保留！
        server_mode.store(4, Ordering::SeqCst);
        let res_no_stage = crate::sync_engine::execute_lan_outbox_push(
            &client, &push_url, "dev-mob-1", "android", "Phone", "env-test", push_bytes.clone(), &outbox_path,
        ).await;
        assert!(res_no_stage.is_err(), "Missing stage in receipt must fail-closed: {:?}", res_no_stage);
        assert!(outbox_path.exists(), "Outbox file MUST be retained when stage is missing from receipt!");

        // 场景 E: 重试成功，远端返回 HTTP 200 且明确带有 stage: applied 提交回执
        // 唯有此时，本地 outbox 才能被安全删除！
        server_mode.store(5, Ordering::SeqCst);
        let res_ok = crate::sync_engine::execute_lan_outbox_push(
            &client, &push_url, "dev-mob-1", "android", "Phone", "env-test", push_bytes, &outbox_path,
        ).await;
        assert!(res_ok.is_ok(), "Retry with 200 OK and stage: applied commit receipt must succeed: {:?}", res_ok);
        assert!(!outbox_path.exists(), "Outbox file MUST be deleted only after confirmed applied commit receipt!");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --------------------------------------------------------------------------
    // 24. SEC-02 LAN push_db 失败检测与回执验证 (HTTP 500 / 401 / 无回执阻断)
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec02_lan_push_db_failure_and_receipt_verification() {
        use std::sync::atomic::{AtomicU8, Ordering};
        use axum::routing::post;
        use axum::extract::State;
        use axum::response::IntoResponse;

        let server_mode = Arc::new(AtomicU8::new(1));
        let mode_clone = server_mode.clone();

        let app = axum::Router::new()
            .route("/v1/sync/push_db", post(|State(m): State<Arc<AtomicU8>>| async move {
                match m.load(Ordering::SeqCst) {
                    1 => (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        axum::Json(serde_json::json!({ "status": "error", "message": "Database transaction failed" }))
                    ).into_response(),
                    2 => (
                        axum::http::StatusCode::UNAUTHORIZED,
                        axum::Json(serde_json::json!({ "status": "error", "message": "Invalid token" }))
                    ).into_response(),
                    3 => (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({ "status": "pending_processing" }))
                    ).into_response(),
                    4 => (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({ "status": "ok", "type": "commit_ack" }))
                    ).into_response(),
                    _ => (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({ "status": "ok", "type": "commit_ack", "stage": "applied" }))
                    ).into_response(),
                }
            }))
            .with_state(mode_clone);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let push_db_url = format!("http://127.0.0.1:{}/v1/sync/push_db", port);
        let push_db_bytes = serde_json::to_vec(&empty_sync_data()).unwrap();

        // 场景 A: HTTP 500 -> 必须返回 Err
        server_mode.store(1, Ordering::SeqCst);
        let res_500 = crate::sync_engine::execute_lan_push_db(
            &client, &push_db_url, "dev-mob-1", "android", "Phone", "env-test", push_db_bytes.clone(),
        ).await;
        assert!(res_500.is_err(), "HTTP 500 must return error");
        assert!(res_500.unwrap_err().contains("500"));

        // 场景 B: HTTP 401 -> 必须返回 Err
        server_mode.store(2, Ordering::SeqCst);
        let res_401 = crate::sync_engine::execute_lan_push_db(
            &client, &push_db_url, "dev-mob-1", "android", "Phone", "env-test", push_db_bytes.clone(),
        ).await;
        assert!(res_401.is_err(), "HTTP 401 must return error");
        assert!(res_401.unwrap_err().contains("401"));

        // 场景 C: HTTP 200 但无 status: ok 回执 -> 必须返回 Err
        server_mode.store(3, Ordering::SeqCst);
        let res_no_receipt = crate::sync_engine::execute_lan_push_db(
            &client, &push_db_url, "dev-mob-1", "android", "Phone", "env-test", push_db_bytes.clone(),
        ).await;
        assert!(res_no_receipt.is_err(), "Missing commit receipt must return error");
        assert!(res_no_receipt.unwrap_err().contains("缺少提交回执"));

        // 场景 D: HTTP 200 带有 status: ok 但缺少 stage -> 必须 Fail-Closed 拒斥
        server_mode.store(4, Ordering::SeqCst);
        let res_no_stage = crate::sync_engine::execute_lan_push_db(
            &client, &push_db_url, "dev-mob-1", "android", "Phone", "env-test", push_db_bytes.clone(),
        ).await;
        assert!(res_no_stage.is_err(), "Missing stage must return error");
        assert!(res_no_stage.unwrap_err().contains("缺少 stage 回执字段"));

        // 场景 E: HTTP 200 带 status: ok 且 stage: applied 提交回执 -> 成功
        server_mode.store(5, Ordering::SeqCst);
        let res_ok = crate::sync_engine::execute_lan_push_db(
            &client, &push_db_url, "dev-mob-1", "android", "Phone", "env-test", push_db_bytes,
        ).await;
        assert!(res_ok.is_ok(), "Confirmed applied commit receipt must succeed: {:?}", res_ok);
    }

    // --------------------------------------------------------------------------
    // 25. SEC-03 远程 Push 契约统一与端到端真实投递执行验证 (E2E Delivery & Reconciliation)
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_push_outbox_delivery_e2e_reconciliation() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_outbox_reconcile_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let ctx = setup_test_context();
        let now = crate::now_ms();
        let mut conn = ctx.db.lock().unwrap();

        // 1. 验证白名单操作 set_config 在 atomic_commit_push_outbox 中通过并完成实际投递
        let req_id_1 = "req-e2e-delivery-001";
        let tok_1 = "tok-delivery-001";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push', ?, ?, 'h1', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_1, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_1],
        ).unwrap();

        let ops_1 = vec![serde_json::json!({
            "op": "set_config",
            "key": "theme",
            "value": "dark"
        })];

        let commit_res = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id_1, tok_1, &ops_1, "{\"status\":\"ok\"}", now,
        );
        assert!(commit_res.is_ok(), "Valid set_config must commit and deliver: {:?}", commit_res);

        // 验证 rpc_staged_outbox 状态转为 delivered
        let event_id_1 = format!("evt-{}-{}", ctx.session_id, req_id_1);
        let status_1: String = conn.query_row(
            "SELECT status FROM rpc_staged_outbox WHERE event_id = ?",
            rusqlite::params![event_id_1],
            |r| r.get(0),
        ).expect("staged outbox row must exist");
        assert_eq!(status_1, "delivered", "Valid op must transition to delivered");

        // 验证 config.json 真实更新了对应键值
        let current_cfg = crate::read_config_checked().unwrap_or_else(|_| serde_json::json!({}));
        assert_eq!(current_cfg.get("theme").and_then(|v| v.as_str()), Some("dark"), "Config theme must be updated to dark");

        // 2. 验证非白名单操作 (如 create_item) 在前置校验被彻底阻断，不进入投递
        let req_id_2 = "req-e2e-delivery-002";
        let tok_2 = "tok-delivery-002";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push', ?, ?, 'h2', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_2, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_2],
        ).unwrap();

        let ops_2 = vec![serde_json::json!({
            "op": "create_item",
            "item_id": "malicious-1"
        })];

        let commit_res_2 = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id_2, tok_2, &ops_2, "{\"status\":\"ok\"}", now,
        );
        assert!(commit_res_2.is_err(), "create_item must be rejected at ingress");
        assert!(commit_res_2.unwrap_err().contains("未知或不允许的操作类型"));

        // 3. 验证防御深度：若暂存队列中存在非法操作，drain_staged_outbox 绝不会将其标为 delivered
        let event_id_bad = "evt-test-bad-op";
        let bad_ops = vec![serde_json::json!({"op": "non_existent_op_type"})];
        conn.execute(
            "INSERT INTO rpc_staged_outbox (event_id, session_id, request_id, operations_json, status, attempts, created_at)
             VALUES (?, 'sess-bad', 'req-bad', ?, 'pending', 0, ?)",
            rusqlite::params![event_id_bad, serde_json::to_string(&bad_ops).unwrap(), now],
        ).unwrap();

        let drain_res = crate::device_trust::drain_staged_outbox(&conn, now);
        assert!(drain_res.is_ok());

        // 检查该行记录：status 必须依然是 pending，且 attempts > 0，last_error 包含拒绝信息
        let (bad_status, bad_attempts, bad_error): (String, i64, Option<String>) = conn.query_row(
            "SELECT status, attempts, last_error FROM rpc_staged_outbox WHERE event_id = ?",
            rusqlite::params![event_id_bad],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(bad_status, "pending", "Rejected ops must NOT be marked delivered!");
        assert!(bad_attempts > 0, "Attempts must be incremented on rejection");
        assert!(bad_error.is_some(), "Error must be recorded for rejected ops");
    }

    // --------------------------------------------------------------------------
    // 26. SEC-03 接收端格式不符载荷拒斥与发送端 Outbox 保留 (Fail-Closed)
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec03_push_unrecognized_payload_rejection_and_outbox_retention() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_cfg_unrecog_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        let _ = std::fs::write(&test_cfg_path, "{}");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;

        let ctx = setup_test_context();
        let now = crate::now_ms();

        let req_id = "req-unrecog-001";

        // Case 1: 合法 JSON Object 但格式不符（包含未知字段，缺失 ops/data 数组）
        let unrecog_body = serde_json::json!({
            "unrecognized_field": "some_value",
            "foo": "bar"
        });
        let unrecog_bytes = serde_json::to_vec(&unrecog_body).unwrap();
        let env_1 = make_test_envelope(&ctx, "push", req_id, &unrecog_bytes, now);
        let mut headers_1 = axum::http::HeaderMap::new();
        headers_1.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_1.insert("x-rpc-auth-envelope", serde_json::to_string(&env_1).unwrap().parse().unwrap());

        let resp_1 = handle_sync_push(
            State(ctx.state.clone()),
            ConnectInfo(ctx.addr),
            headers_1,
            unrecog_bytes.clone().into(),
        ).await.into_response();

        assert_eq!(resp_1.status(), axum::http::StatusCode::BAD_REQUEST, "Unrecognized JSON payload must return HTTP 400");
        let body_1 = axum::body::to_bytes(resp_1.into_body(), 1024 * 1024).await.unwrap();
        let json_1: serde_json::Value = serde_json::from_slice(&body_1).unwrap();
        assert_eq!(json_1.get("status").and_then(|v| v.as_str()), Some("error"));
        assert!(json_1.get("message").and_then(|v| v.as_str()).unwrap().contains("无法识别的 push 载荷格式"));

        // Case 2: 包装对象中 ops 存在但不是数组（如 ops 为字符串）
        let bad_ops_type = serde_json::json!({ "ops": "not_an_array" });
        let bad_ops_bytes = serde_json::to_vec(&bad_ops_type).unwrap();
        let env_2 = make_test_envelope(&ctx, "push", "req-unrecog-002", &bad_ops_bytes, now);
        let mut headers_2 = axum::http::HeaderMap::new();
        headers_2.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_2.insert("x-rpc-auth-envelope", serde_json::to_string(&env_2).unwrap().parse().unwrap());

        let resp_2 = handle_sync_push(
            State(ctx.state.clone()),
            ConnectInfo(ctx.addr),
            headers_2,
            bad_ops_bytes.into(),
        ).await.into_response();
        assert_eq!(resp_2.status(), axum::http::StatusCode::BAD_REQUEST);

        // Case 3: 包装对象字段名为 operations 而非 ops/data
        let wrong_field = serde_json::json!({ "operations": [{"op": "set_config", "key": "theme", "value": "dark"}] });
        let wrong_field_bytes = serde_json::to_vec(&wrong_field).unwrap();
        let env_3 = make_test_envelope(&ctx, "push", "req-unrecog-003", &wrong_field_bytes, now);
        let mut headers_3 = axum::http::HeaderMap::new();
        headers_3.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_3.insert("x-rpc-auth-envelope", serde_json::to_string(&env_3).unwrap().parse().unwrap());

        let resp_3 = handle_sync_push(
            State(ctx.state.clone()),
            ConnectInfo(ctx.addr),
            headers_3,
            wrong_field_bytes.into(),
        ).await.into_response();
        assert_eq!(resp_3.status(), axum::http::StatusCode::BAD_REQUEST);

        // Case 4: 完全非法畸变字节串 (Malformed JSON)
        let malformed_bytes = b"not a json at all {{{".to_vec();
        let env_4 = make_test_envelope(&ctx, "push", "req-unrecog-004", &malformed_bytes, now);
        let mut headers_4 = axum::http::HeaderMap::new();
        headers_4.insert("x-device-id", ctx.mobile_device_id.parse().unwrap());
        headers_4.insert("x-rpc-auth-envelope", serde_json::to_string(&env_4).unwrap().parse().unwrap());

        let resp_4 = handle_sync_push(
            State(ctx.state.clone()),
            ConnectInfo(ctx.addr),
            headers_4,
            malformed_bytes.into(),
        ).await.into_response();
        assert_eq!(resp_4.status(), axum::http::StatusCode::BAD_REQUEST);

        // 验证发送端：调用 execute_lan_outbox_push 发送不符载荷时，本地 outbox 绝不删除
        let temp_dir = std::env::temp_dir().join(format!("bob_test_unrecog_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let outbox_path = temp_dir.join("test_outbox.json");
        std::fs::write(&outbox_path, &unrecog_bytes).unwrap();
        assert!(outbox_path.exists());

        let client = reqwest::Client::new();
        // 启动轻量 local receiver 模拟 HTTP 400 返回
        let app_mock = axum::Router::new()
            .route("/v1/sync/push", axum::routing::post(|| async {
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({ "status": "error", "message": "Invalid format" }))
                ).into_response()
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app_mock).await.unwrap();
        });

        let push_url = format!("http://127.0.0.1:{}/v1/sync/push", port);
        let send_res = crate::sync_engine::execute_lan_outbox_push(
            &client, &push_url, "dev-test", "android", "Phone", "env-test", unrecog_bytes, &outbox_path,
        ).await;

        assert!(send_res.is_err(), "execute_lan_outbox_push must fail on HTTP 400");
        assert!(outbox_path.exists(), "Local outbox file MUST be retained when push is rejected for unrecognized format!");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --------------------------------------------------------------------------
    // 27. “提交回执”状态契约验证：已应用 (applied) 与 待应用 (pending_apply)
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec03_push_commit_receipt_delivery_status_contract() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_receipt_contract_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        let _ = std::fs::write(&test_cfg_path, "{}");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let ctx = setup_test_context();
        let mut conn = ctx.db.lock().unwrap();
        let now = crate::now_ms();

        let req_id_applied = "req-receipt-applied-001";
        let tok_applied = "tok-receipt-applied-001";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push', ?, ?, 'h1', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_applied, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_applied],
        ).unwrap();

        let ops = vec![serde_json::json!({
            "op": "set_config",
            "key": "theme",
            "value": "dark"
        })];

        // 正常情况下（磁盘可写）：返回 stage == "applied"
        let receipt_applied = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id_applied, tok_applied, &ops, "{\"status\":\"ok\"}", now,
        ).expect("commit must succeed");

        assert_eq!(receipt_applied.status, "ok");
        assert_eq!(receipt_applied.stage, "applied", "Must be marked applied on successful disk write");
        assert_eq!(receipt_applied.applied_count, 1);
        assert_eq!(receipt_applied.pending_count, 0);
        assert!(receipt_applied.delivery_error.is_none());

        // 验证幂等表落盘的 response_json 同样严格反映 stage == "applied"
        let cached_resp: String = conn.query_row(
            "SELECT response_json FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
            rusqlite::params![ctx.session_id, req_id_applied],
            |r| r.get(0),
        ).unwrap();
        assert!(cached_resp.contains("\"stage\":\"applied\""));
    }

    // --------------------------------------------------------------------------
    // 28. 磁盘写入失败后的 pending 状态与重试收敛全生命周期
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec03_disk_failure_pending_status_and_retry_convergence() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_disk_fail_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let ctx = setup_test_context();
        let mut conn = ctx.db.lock().unwrap();
        let now = crate::now_ms();

        let req_id_retry = "req-retry-lifecycle-001";
        let tok_retry = "tok-retry-lifecycle-001";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push', ?, ?, 'h-retry', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_retry, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_retry],
        ).unwrap();

        let ops = vec![serde_json::json!({
            "op": "set_config",
            "key": "accentColor",
            "value": "#10b981"
        })];

        // ══════════════════════════════════════════════════════════════════════
        // 阶段 1: 故障注入 — 模拟磁盘写入失败 (EIO / 只读)
        // ══════════════════════════════════════════════════════════════════════
        crate::set_inject_config_write_error(true);

        let receipt_pending = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id_retry, tok_retry, &ops, "{\"status\":\"ok\"}", now,
        ).expect("commit to durable staged outbox must succeed even if apply fails");

        // 状态契约验证：回执必须明确声明为 pending_apply，绝不可谎报 applied
        assert_eq!(receipt_pending.status, "ok");
        assert_eq!(receipt_pending.stage, "pending_apply", "Failed disk write must result in pending_apply receipt");
        assert_eq!(receipt_pending.applied_count, 0);
        assert_eq!(receipt_pending.pending_count, 1);
        assert!(receipt_pending.delivery_error.is_some(), "Delivery error must be present in receipt");
        assert!(receipt_pending.delivery_error.as_ref().unwrap().contains("Fault injection"));

        // 数据库持久化断言：操作已安全入队，status 为 pending，attempts 递增，last_error 已记录
        let (db_status, db_attempts, db_err): (String, i64, Option<String>) = conn.query_row(
            "SELECT status, attempts, last_error FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
            rusqlite::params![ctx.session_id, req_id_retry],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(db_status, "pending", "Failed operation must remain pending in durable outbox");
        assert_eq!(db_attempts, 1, "Attempts must be incremented to 1");
        assert!(db_err.unwrap().contains("Fault injection"));

        // 发送端验证：调用 execute_lan_outbox_push 接收到 pending_apply 时，本地 outbox 绝不删除
        let temp_dir = std::env::temp_dir().join(format!("bob_test_pending_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let outbox_path = temp_dir.join("test_outbox.json");
        std::fs::write(&outbox_path, b"[{\"op\":\"set_config\"}]").unwrap();

        let client = reqwest::Client::new();
        let app_mock_pending = axum::Router::new()
            .route("/v1/sync/push", axum::routing::post(|| async {
                (
                    axum::http::StatusCode::OK,
                    axum::Json(serde_json::json!({
                        "status": "ok",
                        "type": "commit_ack",
                        "stage": "pending_apply",
                        "applied_count": 0,
                        "pending_count": 1,
                        "delivery_error": "Disk write failed"
                    }))
                ).into_response()
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app_mock_pending).await.unwrap();
        });

        let push_url = format!("http://127.0.0.1:{}/v1/sync/push", port);
        let send_res = crate::sync_engine::execute_lan_outbox_push(
            &client, &push_url, "dev-test", "android", "Phone", "env-test", b"[]".to_vec(), &outbox_path,
        ).await;

        assert!(send_res.is_ok());
        assert_eq!(send_res.unwrap(), crate::sync_engine::PushDeliveryOutcome::PendingApply(Some("Disk write failed".to_string())));
        assert!(outbox_path.exists(), "Local outbox MUST be retained when receiver acknowledges as pending_apply!");

        // ══════════════════════════════════════════════════════════════════════
        // 阶段 2: 故障消除与重试收敛 (Retry Convergence)
        // ══════════════════════════════════════════════════════════════════════
        crate::set_inject_config_write_error(false);

        // 重新调用 drain_staged_outbox 触发投递收敛
        let convergence_count = crate::device_trust::drain_staged_outbox(&conn, now + 1000).expect("drain must succeed after fault cleared");
        assert_eq!(convergence_count, 1, "Exactly 1 pending operation must be delivered and converged");

        // 验证数据库状态流转：转换为 delivered，去重表写入
        let (conv_status, conv_err): (String, Option<String>) = conn.query_row(
            "SELECT status, last_error FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
            rusqlite::params![ctx.session_id, req_id_retry],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(conv_status, "delivered", "Pending op must converge to delivered");
        assert!(conv_err.is_none(), "last_error must be cleared on successful delivery");

        let event_id = format!("evt-{}-{}", ctx.session_id, req_id_retry);
        let processed_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM rpc_processed_events WHERE event_id = ?",
            rusqlite::params![&event_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(processed_count, 1, "rpc_processed_events must record converged event");

        // 验证物理 config.json 真实生效
        let final_cfg = crate::read_config_checked().unwrap_or_else(|_| serde_json::json!({}));
        assert_eq!(final_cfg.get("accentColor").and_then(|v| v.as_str()), Some("#10b981"), "Config must converge to updated accentColor");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --------------------------------------------------------------------------
    // 29. push_db 路径下的配置差异投递回执契约验证
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec03_push_db_config_delivery_status_and_idempotency_receipt() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_push_db_rcpt_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let ctx = setup_test_context();
        let mut conn = ctx.db.lock().unwrap();
        let now = crate::now_ms();

        let req_id = "req-push-db-receipt-001";
        let exec_token = "tok-push-db-receipt-001";

        let mut sync_data = empty_sync_data();
        sync_data.config = serde_json::json!({
            "language": "zh-CN",
            "uiScale": 1.2
        });

        let idem_info = crate::sync_engine::IdempotencyCommitInfo {
            session_id: &ctx.session_id,
            request_id: req_id,
            execution_token: exec_token,
            response_json: "{\"status\":\"ok\"}",
            now_ms: now,
        };

        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push_db', ?, ?, 'h-pdb', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id, ctx.mobile_device_id, ctx.target_device_id, now, now, exec_token],
        ).unwrap();

        let outcome = crate::sync_engine::import_sync_data_to_conn_atomic(
            &mut conn, &sync_data, 0, Some(idem_info),
        ).expect("import_sync_data_to_conn_atomic must succeed");

        assert_eq!(outcome.receipt.status, "ok");
        assert_eq!(outcome.receipt.stage, "applied", "push_db config diff must be applied");

        let cached_json: String = conn.query_row(
            "SELECT response_json FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
            rusqlite::params![ctx.session_id, req_id],
            |r| r.get(0),
        ).unwrap();
        let cached_val: serde_json::Value = serde_json::from_str(&cached_json).unwrap();
        assert_eq!(cached_val.get("stage").and_then(|v| v.as_str()), Some("applied"));
    }

    // --------------------------------------------------------------------------
    // 30. 查询投递状态失败时 Fail-Closed 返回 pending_apply (绝不虚报 applied)
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_delivery_status_query_failure_returns_pending_apply() {
        let ctx = setup_test_context();
        let mut conn = ctx.db.lock().unwrap();
        let now = crate::now_ms();

        // 1. push 路径：添加临时触发器在 INSERT INTO rpc_staged_outbox 之后立即删除该行，模拟在 commit 后查询状态找不到记录 (QueryReturnedNoRows)
        let req_id_1 = "req-stat-fail-001";
        let tok_1 = "tok-stat-fail-001";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push', ?, ?, 'h-sf1', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_1, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_1],
        ).unwrap();

        let ops_1 = vec![serde_json::json!({
            "op": "set_config",
            "key": "theme",
            "value": "dark"
        })];

        conn.execute_batch("CREATE TEMP TRIGGER trigger_drop_staged AFTER INSERT ON rpc_staged_outbox BEGIN DELETE FROM rpc_staged_outbox WHERE event_id = NEW.event_id; END;").unwrap();

        let receipt_1 = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id_1, tok_1, &ops_1, "{\"status\":\"ok\"}", now,
        );

        conn.execute_batch("DROP TRIGGER trigger_drop_staged;").unwrap();

        // 当无法核实磁盘状态时，必须按失败关闭处理，报告 pending_apply，绝不能在无法核实磁盘状态时宣称 applied
        assert!(receipt_1.is_ok(), "atomic_commit_push_outbox must survive query error: {:?}", receipt_1);
        let rec = receipt_1.unwrap();
        assert_eq!(rec.stage, "pending_apply", "Must fail-closed to pending_apply when status cannot be verified");
        assert!(rec.delivery_error.is_some(), "Must carry delivery_error details");
        assert!(rec.delivery_error.unwrap().contains("无法确认落盘状态"));

        // 2. push_db 路径：同样模拟查询失败
        let req_id_2 = "req-stat-fail-002";
        let tok_2 = "tok-stat-fail-002";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push_db', ?, ?, 'h-sf2', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_2, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_2],
        ).unwrap();

        let mut sync_data = empty_sync_data();
        sync_data.config = serde_json::json!({ "theme": "dark" });
        let idem_info = crate::sync_engine::IdempotencyCommitInfo {
            session_id: &ctx.session_id,
            request_id: req_id_2,
            execution_token: tok_2,
            response_json: "{\"status\":\"ok\"}",
            now_ms: now,
        };

        conn.execute_batch("CREATE TEMP TRIGGER trigger_drop_staged_pdb AFTER INSERT ON rpc_staged_outbox BEGIN DELETE FROM rpc_staged_outbox WHERE event_id = NEW.event_id; END;").unwrap();
        let outcome_2 = crate::sync_engine::import_sync_data_to_conn_atomic(
            &mut conn, &sync_data, 0, Some(idem_info),
        );
        conn.execute_batch("DROP TRIGGER trigger_drop_staged_pdb;").unwrap();

        assert!(outcome_2.is_ok());
        let rec_2 = outcome_2.unwrap().receipt;
        assert_eq!(rec_2.stage, "pending_apply", "push_db must also fail-closed to pending_apply when query fails");
        assert!(rec_2.delivery_error.unwrap().contains("无法确认配置落盘状态"));
    }

    // --------------------------------------------------------------------------
    // 31. 幂等缓存更新失败拦截 (Fail-Closed) 与重放完整 stage 回执
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec03_idempotency_cache_update_failure_and_replay_stage() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir = std::env::temp_dir().join(format!("bob_test_idem_replay_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_cfg_path = temp_dir.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let ctx = setup_test_context();
        let mut conn = ctx.db.lock().unwrap();
        let now = crate::now_ms();

        let req_id = "req-idem-fail-001";
        let tok = "tok-idem-fail-001";

        let ops = vec![serde_json::json!({
            "op": "set_config",
            "key": "theme",
            "value": "dark"
        })];

        // 1. 如果 rpc_idempotency_cache 中不存在对应记录，第一步 fencing 校验拒绝
        let res_no_row = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id, tok, &ops, "{\"status\":\"ok\"}", now,
        );
        assert!(res_no_row.is_err(), "Must fail closed if idempotency cache row does not exist");
        assert!(res_no_row.unwrap_err().contains("幂等提交被拒绝"));

        // 2. 如果在事务提交后、第四步更新 response_json 时记录被删除或影响行数为0，必须 Fail-Closed 报错
        let req_id_2 = "req-idem-fail-002";
        let tok_2 = "tok-idem-fail-002";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push', ?, ?, 'h-idem2', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_2, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_2],
        ).unwrap();

        // 创建触发器：在第一步 status 更新为 completed 时立即删除该行，使第四步更新 response_json 遭遇 0 行
        conn.execute_batch("CREATE TEMP TRIGGER trigger_del_idem AFTER UPDATE OF status ON rpc_idempotency_cache BEGIN DELETE FROM rpc_idempotency_cache WHERE session_id = NEW.session_id AND request_id = NEW.request_id; END;").unwrap();

        let res_zero_rows = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id_2, tok_2, &ops, "{\"status\":\"ok\"}", now,
        );
        conn.execute_batch("DROP TRIGGER trigger_del_idem;").unwrap();

        assert!(res_zero_rows.is_err(), "Must fail closed if idempotency response update affects 0 rows");
        assert!(res_zero_rows.unwrap_err().contains("更新幂等缓存 response_json 影响行数为 0"));

        // 3. 正常流程：插入 pending 记录后正常提交，验证更新成功与缓存持久化
        let req_id_ok = "req-idem-ok-003";
        let tok_ok = "tok-idem-ok-003";
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, action, subject_device_id, target_device_id, payload_hash, status, response_json, created_at, updated_at, lease_generation, execution_token)
             VALUES (?, ?, 'push', ?, ?, 'h-idem3', 'pending', '', ?, ?, 1, ?)",
            rusqlite::params![ctx.session_id, req_id_ok, ctx.mobile_device_id, ctx.target_device_id, now, now, tok_ok],
        ).unwrap();

        let res_ok = crate::device_trust::atomic_commit_push_outbox(
            &mut conn, &ctx.session_id, req_id_ok, tok_ok, &ops, "{\"status\":\"ok\"}", now,
        );
        assert!(res_ok.is_ok());

        // 4. 验证幂等缓存中落盘的 response_json 必须包含 stage: applied
        let cached_json: String = conn.query_row(
            "SELECT response_json FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
            rusqlite::params![ctx.session_id, req_id_ok],
            |r| r.get(0),
        ).unwrap();
        let cached_val: serde_json::Value = serde_json::from_str(&cached_json).unwrap();
        assert_eq!(cached_val.get("stage").and_then(|v| v.as_str()), Some("applied"));

        // 5. 发送端消费该重放回执，必须正确解析为 PushDeliveryOutcome::Applied
        let outcome = crate::sync_engine::parse_push_commit_receipt(&cached_val).unwrap();
        assert_eq!(outcome, crate::sync_engine::PushDeliveryOutcome::Applied);
    }

    // --------------------------------------------------------------------------
    // 32. 生产 Relay Push 接收端穿透测试：写入失败拦截 (Fail-Closed)、暂存 pending_apply 与恢复收敛
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec03_relay_production_push_receiver_failure_pending_and_recovery() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir_cfg = std::env::temp_dir().join(format!("bob_test_relay_push_cfg_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir_cfg);
        let test_cfg_path = temp_dir_cfg.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let ctx = setup_test_context();
        let dispatch_ctx = crate::sync_engine::RelayDispatchContext::for_test_with_signer(
            Some(ctx.db.clone()),
            None,
            ctx.target_sk.clone(),
            ctx.target_device_id.clone(),
        );
        let (tx_mpsc, mut rx_mpsc) = tokio::sync::mpsc::channel::<tokio_tungstenite::tungstenite::protocol::Message>(32);

        // ══════════════════════════════════════════════════════════════════════
        // 场景 A: 恶意/越权操作被拦截 (Fail-Closed)
        // 试图通过 Relay push 修改远程 API 密钥 -> 必须失败、更新幂等缓存为 failed、返回签名 error 帧，绝不发送 commit_ack
        // ══════════════════════════════════════════════════════════════════════
        let malicious_ops = serde_json::json!([
            {
                "op": "set_api_key",
                "provider": "openai",
                "key": "sk-malicious-leak"
            }
        ]);
        let mal_bytes = serde_json::to_vec(&malicious_ops).unwrap();
        let now_a = crate::now_ms();
        let req_id_a = "req-relay-push-fail-001";
        let env_a = make_test_envelope(&ctx, "push", req_id_a, &mal_bytes, now_a);
        let msg_a = serde_json::json!({
            "type": "proxy",
            "from_device_id": ctx.mobile_device_id,
            "target_device_id": ctx.target_device_id,
            "message_id": format!("msg-{}", uuid::Uuid::new_v4()),
            "trace_id": format!("trace-{}", uuid::Uuid::new_v4()),
            "protocol_version": crate::sync_protocol::SYNC_PROTOCOL_VERSION,
            "payload": {
                "action": "push",
                "data": malicious_ops,
                "auth_envelope": env_a
            }
        });

        let res_a = crate::sync_engine::dispatch_inbound_relay_message_core(&dispatch_ctx, &msg_a, &tx_mpsc).await;
        assert!(res_a.is_err(), "Relay push modifying API key must fail-closed");
        assert!(res_a.unwrap_err().contains("set_api_key is blocked on remote sync"));

        // 接收端通道收到 signed error 帧，绝不可是 commit_ack
        let err_frame = rx_mpsc.try_recv().expect("Must have sent signed error response");
        let err_text = match err_frame {
            tokio_tungstenite::tungstenite::protocol::Message::Text(t) => t.to_string(),
            _ => panic!("Expected text frame"),
        };
        let err_json: serde_json::Value = serde_json::from_str(&err_text).unwrap();
        assert_eq!(err_json.get("type").and_then(|v| v.as_str()), Some("proxy"));
        let err_action = err_json.get("payload").and_then(|p| p.get("action")).and_then(|v| v.as_str());
        assert_eq!(err_action, Some("error"));
        assert_ne!(err_action, Some("commit_ack"));

        // 幂等缓存状态必须被标记为 failed
        {
            let conn = ctx.db.lock().unwrap();
            let status: String = conn.query_row(
                "SELECT status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id_a],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(status, "failed", "Rejected relay push must fail idempotency");
        }

        // ══════════════════════════════════════════════════════════════════════
        // 场景 B: 磁盘写入失败 (故障注入) -> stage 必须为 pending_apply，发送端保留待同步数据
        // ══════════════════════════════════════════════════════════════════════
        let temp_dir = std::env::temp_dir().join(format!("bob_relay_e2e_push_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let client_outbox_path = temp_dir.join("mobile_outbox.json");
        std::fs::write(&client_outbox_path, b"[{\"op\":\"set_config\",\"key\":\"theme\",\"value\":\"dark\"}]").unwrap();
        assert!(client_outbox_path.exists());

        crate::set_inject_config_write_error(true);

        let valid_ops = serde_json::json!([
            {
                "op": "set_config",
                "key": "theme",
                "value": "dark"
            }
        ]);
        let valid_bytes = serde_json::to_vec(&valid_ops).unwrap();
        let now_b = crate::now_ms();
        let req_id_b = "req-relay-push-pending-002";
        let env_b = make_test_envelope(&ctx, "push", req_id_b, &valid_bytes, now_b);
        let msg_b = serde_json::json!({
            "type": "proxy",
            "from_device_id": ctx.mobile_device_id,
            "target_device_id": ctx.target_device_id,
            "message_id": format!("msg-{}", uuid::Uuid::new_v4()),
            "trace_id": format!("trace-{}", uuid::Uuid::new_v4()),
            "protocol_version": crate::sync_protocol::SYNC_PROTOCOL_VERSION,
            "payload": {
                "action": "push",
                "data": valid_ops,
                "auth_envelope": env_b
            }
        });

        let res_b = crate::sync_engine::dispatch_inbound_relay_message_core(&dispatch_ctx, &msg_b, &tx_mpsc).await;
        assert!(res_b.is_ok(), "Relay push must complete and return receipt even on pending disk apply");

        // 提取通道中的 commit_ack 帧
        let ack_frame = rx_mpsc.try_recv().expect("Receiver must send commit_ack");
        let ack_text = match ack_frame {
            tokio_tungstenite::tungstenite::protocol::Message::Text(t) => t.to_string(),
            _ => panic!("Expected text frame"),
        };
        let ack_json: serde_json::Value = serde_json::from_str(&ack_text).unwrap();
        let payload = ack_json.get("payload").unwrap();
        assert_eq!(payload.get("status").and_then(|v| v.as_str()), Some("committed"));
        assert_eq!(payload.get("stage").and_then(|v| v.as_str()), Some("pending_apply"));
        assert_eq!(payload.get("applied_count").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(payload.get("pending_count").and_then(|v| v.as_u64()), Some(1));
        assert!(payload.get("delivery_error").is_some());
        assert!(payload["delivery_error"].as_str().unwrap().contains("Fault injection"));

        // 发送端消费此回执：必须解析为 PendingApply，且 client_outbox_path 绝不被删除
        let parsed_outcome = crate::sync_engine::parse_push_commit_receipt(&ack_json).unwrap();
        match parsed_outcome {
            crate::sync_engine::PushDeliveryOutcome::PendingApply(ref err) => {
                assert!(err.as_ref().unwrap().contains("Fault injection"));
                // 模拟发送端逻辑：保留待同步数据，等待重试收敛
                assert!(client_outbox_path.exists(), "Client outbox MUST be retained when relay receipt is pending_apply!");
            }
            _ => panic!("Expected PendingApply outcome"),
        }

        // 清理通道中的 legacy_ack
        let _ = rx_mpsc.try_recv();

        // ══════════════════════════════════════════════════════════════════════
        // 场景 C: 故障解除后后台收敛与后续 Push 达到 Applied，发送端安全删除 outbox
        // ══════════════════════════════════════════════════════════════════════
        crate::set_inject_config_write_error(false);

        // 触发后台 drain_staged_outbox，验证原暂存任务收敛
        {
            let conn = ctx.db.lock().unwrap();
            let _ = crate::device_trust::drain_staged_outbox(&conn, crate::now_ms());
            let status: String = conn.query_row(
                "SELECT status FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id_b],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(status, "delivered", "Staged outbox must converge to delivered after recovery");
        }

        // 再次发送新 Push 操作
        let new_ops = serde_json::json!([
            {
                "op": "set_config",
                "key": "accentColor",
                "value": "#3b82f6"
            }
        ]);
        let new_bytes = serde_json::to_vec(&new_ops).unwrap();
        let now_c = crate::now_ms();
        let req_id_c = "req-relay-push-applied-003";
        let env_c = make_test_envelope(&ctx, "push", req_id_c, &new_bytes, now_c);
        let msg_c = serde_json::json!({
            "type": "proxy",
            "from_device_id": ctx.mobile_device_id,
            "target_device_id": ctx.target_device_id,
            "message_id": format!("msg-{}", uuid::Uuid::new_v4()),
            "trace_id": format!("trace-{}", uuid::Uuid::new_v4()),
            "protocol_version": crate::sync_protocol::SYNC_PROTOCOL_VERSION,
            "payload": {
                "action": "push",
                "data": new_ops,
                "auth_envelope": env_c
            }
        });

        let res_c = crate::sync_engine::dispatch_inbound_relay_message_core(&dispatch_ctx, &msg_c, &tx_mpsc).await;
        assert!(res_c.is_ok());

        let ack_frame_c = rx_mpsc.try_recv().expect("Receiver must send commit_ack");
        let ack_text_c = match ack_frame_c {
            tokio_tungstenite::tungstenite::protocol::Message::Text(t) => t.to_string(),
            _ => panic!("Expected text frame"),
        };
        let ack_json_c: serde_json::Value = serde_json::from_str(&ack_text_c).unwrap();
        let payload_c = ack_json_c.get("payload").unwrap();
        assert_eq!(payload_c.get("stage").and_then(|v| v.as_str()), Some("applied"));
        assert_eq!(payload_c.get("applied_count").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(payload_c.get("pending_count").and_then(|v| v.as_u64()), Some(0));

        // 发送端消费此 applied 回执：唯有此时才安全删除 outbox
        let parsed_c = crate::sync_engine::parse_push_commit_receipt(&ack_json_c).unwrap();
        assert_eq!(parsed_c, crate::sync_engine::PushDeliveryOutcome::Applied);
        if parsed_c == crate::sync_engine::PushDeliveryOutcome::Applied {
            std::fs::remove_file(&client_outbox_path).unwrap();
        }
        assert!(!client_outbox_path.exists(), "Client outbox MUST be deleted once receipt is applied");

        crate::set_inject_config_write_error(false);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --------------------------------------------------------------------------
    // 33. 生产 Relay Push_DB 接收端穿透测试：写入失败拦截 (Fail-Closed)、暂存 pending_apply 与恢复收敛
    // --------------------------------------------------------------------------
    #[tokio::test]
    pub async fn test_sec03_relay_production_push_db_receiver_failure_pending_and_recovery() {
        let _test_lock = crate::lock_config_test_mutex();
        let temp_dir_cfg = std::env::temp_dir().join(format!("bob_test_relay_pushdb_cfg_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir_cfg);
        let test_cfg_path = temp_dir_cfg.join("config.json");
        crate::set_test_config_path_override(Some(test_cfg_path.clone()));
        let _override_guard = TestConfigOverrideGuard;
        let ctx = setup_test_context();
        let dispatch_ctx = crate::sync_engine::RelayDispatchContext::for_test_with_signer(
            Some(ctx.db.clone()),
            None,
            ctx.target_sk.clone(),
            ctx.target_device_id.clone(),
        );
        let (tx_mpsc, mut rx_mpsc) = tokio::sync::mpsc::channel::<tokio_tungstenite::tungstenite::protocol::Message>(32);

        // ══════════════════════════════════════════════════════════════════════
        // 场景 A: 非法/畸形 push_db 载荷被拦截 (Fail-Closed)
        // 远端同步配置为非法非对象 -> 必须拒斥、更新幂等状态为 failed、返回签名 error 帧，绝不发送 commit_ack
        // ══════════════════════════════════════════════════════════════════════
        let mut mal_sync_data = empty_sync_data();
        mal_sync_data.config = serde_json::json!("malformed_non_object_config");
        let mal_data_val = serde_json::to_value(&mal_sync_data).unwrap();
        let mal_bytes = serde_json::to_vec(&mal_data_val).unwrap();
        let now_a = crate::now_ms();
        let req_id_a = "req-relay-push-db-fail-001";
        let env_a = make_test_envelope(&ctx, "push_db", req_id_a, &mal_bytes, now_a);
        let msg_a = serde_json::json!({
            "type": "proxy",
            "from_device_id": ctx.mobile_device_id,
            "target_device_id": ctx.target_device_id,
            "message_id": format!("msg-{}", uuid::Uuid::new_v4()),
            "trace_id": format!("trace-{}", uuid::Uuid::new_v4()),
            "protocol_version": crate::sync_protocol::SYNC_PROTOCOL_VERSION,
            "payload": {
                "action": "push_db",
                "data": mal_data_val,
                "auth_envelope": env_a
            }
        });

        let res_a = crate::sync_engine::dispatch_inbound_relay_message_core(&dispatch_ctx, &msg_a, &tx_mpsc).await;
        assert!(res_a.is_err(), "Relay push_db with non-object config must fail-closed");
        assert!(res_a.unwrap_err().contains("远端同步配置必须为 JSON Object"));

        // 验证通道收到 signed error 帧
        let err_frame = rx_mpsc.try_recv().expect("Must have sent signed error response");
        let err_text = match err_frame {
            tokio_tungstenite::tungstenite::protocol::Message::Text(t) => t.to_string(),
            _ => panic!("Expected text frame"),
        };
        let err_json: serde_json::Value = serde_json::from_str(&err_text).unwrap();
        assert_eq!(err_json.get("type").and_then(|v| v.as_str()), Some("proxy"));
        let err_action = err_json.get("payload").and_then(|p| p.get("action")).and_then(|v| v.as_str());
        assert_eq!(err_action, Some("error"));

        // 幂等缓存状态必须为 failed
        {
            let conn = ctx.db.lock().unwrap();
            let status: String = conn.query_row(
                "SELECT status FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id_a],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(status, "failed", "Rejected relay push_db must fail idempotency");
        }

        // ══════════════════════════════════════════════════════════════════════
        // 场景 B: 磁盘写入故障 (故障注入) -> stage 必须为 pending_apply，绝不可宣称 applied
        // ══════════════════════════════════════════════════════════════════════
        crate::set_inject_config_write_error(true);

        let mut valid_sync_data = empty_sync_data();
        valid_sync_data.config = serde_json::json!({
            "accentColor": "#ff00aa",
            "uiScale": 2.5
        });
        let valid_data_val = serde_json::to_value(&valid_sync_data).unwrap();
        let valid_bytes = serde_json::to_vec(&valid_data_val).unwrap();
        let now_b = crate::now_ms();
        let req_id_b = "req-relay-push-db-pending-002";
        let env_b = make_test_envelope(&ctx, "push_db", req_id_b, &valid_bytes, now_b);
        let msg_b = serde_json::json!({
            "type": "proxy",
            "from_device_id": ctx.mobile_device_id,
            "target_device_id": ctx.target_device_id,
            "message_id": format!("msg-{}", uuid::Uuid::new_v4()),
            "trace_id": format!("trace-{}", uuid::Uuid::new_v4()),
            "protocol_version": crate::sync_protocol::SYNC_PROTOCOL_VERSION,
            "payload": {
                "action": "push_db",
                "data": valid_data_val,
                "auth_envelope": env_b
            }
        });

        let res_b = crate::sync_engine::dispatch_inbound_relay_message_core(&dispatch_ctx, &msg_b, &tx_mpsc).await;
        assert!(res_b.is_ok(), "Relay push_db must succeed and return pending_apply receipt");

        let ack_frame = rx_mpsc.try_recv().expect("Receiver must send commit_ack");
        let ack_text = match ack_frame {
            tokio_tungstenite::tungstenite::protocol::Message::Text(t) => t.to_string(),
            _ => panic!("Expected text frame"),
        };
        let ack_json: serde_json::Value = serde_json::from_str(&ack_text).unwrap();
        let payload = ack_json.get("payload").unwrap();
        assert_eq!(payload.get("status").and_then(|v| v.as_str()), Some("committed"));
        assert_eq!(payload.get("stage").and_then(|v| v.as_str()), Some("pending_apply"));
        assert_eq!(payload.get("applied_count").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(payload.get("pending_count").and_then(|v| v.as_u64()), Some(2));
        assert!(payload.get("delivery_error").is_some());

        // 发送端解析回执验证
        let parsed_b = crate::sync_engine::parse_push_commit_receipt(&ack_json).unwrap();
        assert!(matches!(parsed_b, crate::sync_engine::PushDeliveryOutcome::PendingApply(_)));

        // 验证接收端历史/诊断活动记录：必须记录 Pending，严禁误报 Success 或 "同步完成"
        let runs_b = crate::sync_history::get_sync_runs().unwrap();
        assert!(!runs_b.is_empty(), "Receiver must record an activity run");
        assert_eq!(runs_b[0].status, crate::sync_protocol::DiagnosticStatus::Pending, "Pending apply receiver activity must have status Pending");
        assert_ne!(runs_b[0].status, crate::sync_protocol::DiagnosticStatus::Success, "Pending apply receiver activity must NEVER be Success");
        assert_ne!(runs_b[0].summary.as_deref(), Some("同步完成"), "Pending apply receiver activity must NEVER claim 同步完成");
        assert!(runs_b[0].summary.as_deref().unwrap().contains("待应用") || runs_b[0].summary.as_deref().unwrap().contains("暂存"));

        // 清理通道中的 legacy_ack
        let _ = rx_mpsc.try_recv();

        // ══════════════════════════════════════════════════════════════════════
        // 场景 C: 故障解除后后台收敛与后续 Push_DB 达到 Applied
        // ══════════════════════════════════════════════════════════════════════
        crate::set_inject_config_write_error(false);

        // 触发后台 drain_staged_outbox，验证原暂存任务收敛
        {
            let conn = ctx.db.lock().unwrap();
            let _ = crate::device_trust::drain_staged_outbox(&conn, crate::now_ms());
            let status: String = conn.query_row(
                "SELECT status FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
                rusqlite::params![ctx.session_id, req_id_b],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(status, "delivered");
        }

        // 发送新的 push_db
        let mut new_sync_data = empty_sync_data();
        new_sync_data.config = serde_json::json!({
            "accentColor": "#00ffaa",
            "uiScale": 1.5
        });
        let new_data_val = serde_json::to_value(&new_sync_data).unwrap();
        let new_bytes = serde_json::to_vec(&new_data_val).unwrap();
        let now_c = crate::now_ms();
        let req_id_c = "req-relay-push-db-applied-003";
        let env_c = make_test_envelope(&ctx, "push_db", req_id_c, &new_bytes, now_c);
        let msg_c = serde_json::json!({
            "type": "proxy",
            "from_device_id": ctx.mobile_device_id,
            "target_device_id": ctx.target_device_id,
            "message_id": format!("msg-{}", uuid::Uuid::new_v4()),
            "trace_id": format!("trace-{}", uuid::Uuid::new_v4()),
            "protocol_version": crate::sync_protocol::SYNC_PROTOCOL_VERSION,
            "payload": {
                "action": "push_db",
                "data": new_data_val,
                "auth_envelope": env_c
            }
        });

        let res_c = crate::sync_engine::dispatch_inbound_relay_message_core(&dispatch_ctx, &msg_c, &tx_mpsc).await;
        assert!(res_c.is_ok());

        let ack_frame_c = rx_mpsc.try_recv().expect("Receiver must send commit_ack");
        let ack_text_c = match ack_frame_c {
            tokio_tungstenite::tungstenite::protocol::Message::Text(t) => t.to_string(),
            _ => panic!("Expected text frame"),
        };
        let ack_json_c: serde_json::Value = serde_json::from_str(&ack_text_c).unwrap();
        let payload_c = ack_json_c.get("payload").unwrap();
        assert_eq!(payload_c.get("stage").and_then(|v| v.as_str()), Some("applied"));
        assert_eq!(payload_c.get("applied_count").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(payload_c.get("pending_count").and_then(|v| v.as_u64()), Some(0));

        let parsed_c = crate::sync_engine::parse_push_commit_receipt(&ack_json_c).unwrap();
        assert_eq!(parsed_c, crate::sync_engine::PushDeliveryOutcome::Applied);

        // 验证故障恢复后接收端历史/诊断活动记录：正常报告 Success
        let runs_c = crate::sync_history::get_sync_runs().unwrap();
        assert!(!runs_c.is_empty(), "Receiver must record an activity run for scenario C");
        assert_eq!(runs_c[0].status, crate::sync_protocol::DiagnosticStatus::Success, "Recovered applied receiver activity must have status Success");
        assert_eq!(runs_c[0].summary.as_deref(), Some("已通过 Relay 接收并应用移动端数据"));

        crate::set_inject_config_write_error(false);
    }
}
