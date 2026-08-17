//! Connector 基础设施
//!
//! 提供 OAuth 回调 HTTP 服务器、Token 持久化、公共 Connector trait。
//! 所有原生连接器（Google、Feishu、Outlook）都基于此模块构建。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// 连接器凭证状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorStatus {
    pub name: String,
    #[serde(rename = "type")]
    pub connector_type: String,
    pub status: String, // "connected" | "disconnected" | "expired"
    pub connected_at: Option<i64>,
}

/// 通用凭证存储结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredCredentials {
    /// OAuth access token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// OAuth refresh token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Token 过期时间 (Unix timestamp seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// 飞书 App ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// 飞书 App Secret
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_secret: Option<String>,
    /// 飞书 tenant_access_token（运行时自动获取）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_access_token: Option<String>,
    /// Azure / Google client_id (嵌入到应用中)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Azure / Google client_secret
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// 连接时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<i64>,
    /// 额外字段
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

// ═══════════════════════════════════════════════════════════
// Token 持久化
// ═══════════════════════════════════════════════════════════

/// 获取 tokens 目录路径
fn tokens_dir() -> PathBuf {
    let data_dir = super::get_data_dir();
    let dir = data_dir.join("tokens");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 保存凭证
pub fn save_credentials(name: &str, creds: &StoredCredentials) -> Result<(), String> {
    let path = tokens_dir().join(format!("{}.json", name));
    let data = serde_json::to_string_pretty(creds).map_err(|e| e.to_string())?;
    std::fs::write(&path, data.as_bytes())
        .map_err(|e| format!("Failed to save credentials for '{}': {}", name, e))?;
    log::info!("[Connector] Saved credentials for '{}'", name);
    Ok(())
}

/// 加载凭证
pub fn load_credentials(name: &str) -> Option<StoredCredentials> {
    let path = tokens_dir().join(format!("{}.json", name));
    if !path.exists() {
        return None;
    }
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

/// 删除凭证
pub fn remove_credentials(name: &str) -> Result<(), String> {
    let path = tokens_dir().join(format!("{}.json", name));
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove credentials for '{}': {}", name, e))?;
    }
    log::info!("[Connector] Removed credentials for '{}'", name);
    Ok(())
}

/// 获取凭证状态
fn get_connector_status(name: &str, connector_type: &str) -> ConnectorStatus {
    match load_credentials(name) {
        Some(creds) => {
            // 检查是否过期
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let status = if let Some(expires_at) = creds.expires_at {
                if expires_at < now {
                    "expired"
                } else {
                    "connected"
                }
            } else {
                "connected"
            };

            ConnectorStatus {
                name: name.to_string(),
                connector_type: connector_type.to_string(),
                status: status.to_string(),
                connected_at: creds.connected_at,
            }
        }
        None => ConnectorStatus {
            name: name.to_string(),
            connector_type: connector_type.to_string(),
            status: "disconnected".to_string(),
            connected_at: None,
        },
    }
}

// ═══════════════════════════════════════════════════════════
// OAuth 回调服务器
// ═══════════════════════════════════════════════════════════

use std::sync::Mutex as StdMutex;
use tokio::sync::oneshot;

/// 全局 OAuth 回调等待器
static OAUTH_WAITER: std::sync::OnceLock<StdMutex<Option<oneshot::Sender<String>>>> =
    std::sync::OnceLock::new();

fn get_oauth_waiter() -> &'static StdMutex<Option<oneshot::Sender<String>>> {
    OAUTH_WAITER.get_or_init(|| StdMutex::new(None))
}

/// 启动 OAuth 回调监听
/// 返回 (redirect_uri, code_receiver)
pub fn prepare_oauth_callback() -> (String, oneshot::Receiver<String>) {
    let (tx, rx) = oneshot::channel();
    {
        let mut waiter = get_oauth_waiter().lock().unwrap();
        *waiter = Some(tx);
    }
    let redirect_uri = "http://127.0.0.1:19823/oauth/callback".to_string();
    (redirect_uri, rx)
}

/// 启动 OAuth 回调 HTTP 服务器（如果尚未启动）
pub fn start_oauth_server() {
    use std::sync::Once;
    static STARTED: Once = Once::new();

    STARTED.call_once(|| {
        tokio::spawn(async {
            let app = axum::Router::new().route(
                "/oauth/callback",
                axum::routing::get(oauth_callback_handler),
            );

            let listener = match tokio::net::TcpListener::bind("127.0.0.1:19823").await {
                Ok(l) => l,
                Err(e) => {
                    log::error!("[Connector] Failed to bind OAuth callback server: {}", e);
                    return;
                }
            };
            log::info!("[Connector] OAuth callback server listening on http://127.0.0.1:19823");
            let _ = axum::serve(listener, app).await;
        });
    });
}

/// OAuth 回调处理器
async fn oauth_callback_handler(
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> axum::response::Html<String> {
    let code = params.get("code").cloned().unwrap_or_default();

    if !code.is_empty() {
        let mut waiter = get_oauth_waiter().lock().unwrap();
        if let Some(tx) = waiter.take() {
            let _ = tx.send(code);
        }
    }

    axum::response::Html(
        "<html><body style='background:#1a1a2e;color:#eee;display:flex;align-items:center;justify-content:center;height:100vh;font-family:system-ui'>\
         <div style='text-align:center'><h2>✅ 授权成功</h2><p>请返回 Bob 应用</p><script>setTimeout(()=>window.close(),2000)</script></div>\
         </body></html>".to_string()
    )
}

// ═══════════════════════════════════════════════════════════
// IPC 命令
// ═══════════════════════════════════════════════════════════

/// 列出所有连接器状态
#[tauri::command]
pub async fn connector_list() -> Value {
    let connectors = vec![
        get_connector_status("google", "oauth"),
        get_connector_status("lark", "app_token"),
        get_connector_status("outlook", "oauth"),
    ];
    json!(connectors)
}

/// 启动 OAuth 授权流程
#[tauri::command]
pub async fn connector_start_oauth(name: String) -> Value {
    start_oauth_server();

    match name.as_str() {
        "google" => super::google_calendar::start_google_oauth().await,
        "outlook" => {
            // Outlook OAuth 暂未实现
            json!({"error": "Outlook OAuth not yet implemented"})
        }
        _ => json!({"error": format!("Unknown OAuth connector: {}", name)}),
    }
}

/// 保存连接器凭证（非 OAuth 类型，如飞书 App ID/Secret）
#[tauri::command]
pub async fn connector_save_credentials(name: String, credentials: Value) -> Value {
    match name.as_str() {
        "google" => {
            let file_path = credentials
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if file_path.is_empty() {
                return json!({"error": "file_path is required for google credentials"});
            }

            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => {
                    return json!({"error": format!("Failed to read credentials file: {}", e)})
                }
            };

            let parsed: Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => return json!({"error": format!("Invalid JSON: {}", e)}),
            };

            let (client_id, client_secret) = if let Some(installed) = parsed.get("installed") {
                (
                    installed.get("client_id").and_then(|v| v.as_str()),
                    installed.get("client_secret").and_then(|v| v.as_str()),
                )
            } else if let Some(web) = parsed.get("web") {
                (
                    web.get("client_id").and_then(|v| v.as_str()),
                    web.get("client_secret").and_then(|v| v.as_str()),
                )
            } else {
                (None, None)
            };

            if let (Some(id), Some(secret)) = (client_id, client_secret) {
                let creds = StoredCredentials {
                    client_id: Some(id.to_string()),
                    client_secret: Some(secret.to_string()),
                    ..Default::default()
                };
                match save_credentials("google", &creds) {
                    Ok(()) => json!({"ok": true}),
                    Err(e) => json!({"error": e}),
                }
            } else {
                json!({"error": "Invalid credentials.json format (missing installed/web client_id or client_secret)"})
            }
        }
        "lark" => {
            let app_id = credentials
                .get("app_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let app_secret = credentials
                .get("app_secret")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if app_id.is_empty() || app_secret.is_empty() {
                return json!({"error": "app_id and app_secret are required"});
            }

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let creds = StoredCredentials {
                app_id: Some(app_id),
                app_secret: Some(app_secret),
                connected_at: Some(now),
                ..Default::default()
            };

            match save_credentials("lark", &creds) {
                Ok(()) => {
                    // 尝试获取 tenant_access_token 验证凭证
                    match super::lark::refresh_tenant_token().await {
                        Ok(_) => json!({"ok": true, "message": "飞书连接成功"}),
                        Err(e) => {
                            // 凭证无效，清除
                            let _ = remove_credentials("lark");
                            json!({"error": format!("飞书凭证验证失败: {}", e)})
                        }
                    }
                }
                Err(e) => json!({"error": e}),
            }
        }
        _ => json!({"error": format!("Unknown credential connector: {}", name)}),
    }
}

/// 断开连接器
#[tauri::command]
pub async fn connector_disconnect(name: String) -> Value {
    match remove_credentials(&name) {
        Ok(()) => json!({"ok": true}),
        Err(e) => json!({"error": e}),
    }
}
