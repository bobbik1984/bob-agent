//! file_share.rs — 基于 Token 的文件分享服务
//!
//! 核心能力：
//!   - 为任意本地文件生成一个带随机 token 的 HTTP 下载链接
//!   - 链接有 TTL（默认 24h），过期自动清理
//!   - 供 LLM 工具 `share_file` 和微信大文件降级使用
//!
//! 下载端点由 http_api.rs 路由注册：GET /v1/dl/:token

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

// ═══════════════════════════════════════════════════════════
// 数据结构
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFileEntry {
    /// 本地文件绝对路径
    pub path: PathBuf,
    /// 文件显示名（下载时的 Content-Disposition filename）
    pub display_name: String,
    /// 文件大小 (bytes)
    pub size: u64,
    /// 创建时间 (unix ms)
    pub created_at: u64,
    /// 过期时间 (unix ms)，0 = 永不过期
    pub expires_at: u64,
}

/// 全局文件分享注册表：token → SharedFileEntry
static SHARE_REGISTRY: LazyLock<Arc<Mutex<HashMap<String, SharedFileEntry>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

// ═══════════════════════════════════════════════════════════
// 公共 API
// ═══════════════════════════════════════════════════════════

/// 生成随机 token (URL-safe, 16 字节 hex = 32 chars)
fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 获取当前毫秒时间戳
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 注册一个文件用于分享，返回 (token, 完整下载 URL)
///
/// - `file_path`: 本地文件绝对路径
/// - `ttl_hours`: 链接有效时长（小时），0 = 永不过期
/// - `base_url`: HTTP 服务的公网前缀，如 "https://bob.example.com" 或 "http://127.0.0.1:3721"
pub fn register_shared_file(
    file_path: &str,
    ttl_hours: u64,
    base_url: &str,
) -> Result<(String, String), String> {
    let path = Path::new(file_path);
    if !path.exists() || !path.is_file() {
        return Err(format!("文件不存在或不是普通文件: {}", file_path));
    }

    let meta = std::fs::metadata(path)
        .map_err(|e| format!("无法读取文件元数据: {}", e))?;

    let display_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    let token = generate_token();
    let created = now_ms();
    let expires = if ttl_hours > 0 {
        created + ttl_hours * 3600 * 1000
    } else {
        0
    };

    let entry = SharedFileEntry {
        path: path.to_path_buf(),
        display_name: display_name.clone(),
        size: meta.len(),
        created_at: created,
        expires_at: expires,
    };

    let url = format!(
        "{}/v1/dl/{}",
        base_url.trim_end_matches('/'),
        token
    );

    let mut registry = SHARE_REGISTRY.lock().unwrap();
    registry.insert(token.clone(), entry);
    log::info!(
        "[file_share] registered: {} → token={} size={}MB ttl={}h",
        display_name,
        &token[..8],
        meta.len() as f64 / 1024.0 / 1024.0,
        ttl_hours
    );

    Ok((token, url))
}

/// 根据 token 查找文件（同时清理过期条目）
pub fn lookup_shared_file(token: &str) -> Option<SharedFileEntry> {
    let mut registry = SHARE_REGISTRY.lock().unwrap();
    let now = now_ms();

    // 惰性清理过期条目
    registry.retain(|_, entry| entry.expires_at == 0 || entry.expires_at > now);

    registry.get(token).cloned()
}

/// 列出当前所有活跃的分享（调试/管理用）
pub fn list_active_shares() -> Vec<(String, SharedFileEntry)> {
    let mut registry = SHARE_REGISTRY.lock().unwrap();
    let now = now_ms();
    registry.retain(|_, entry| entry.expires_at == 0 || entry.expires_at > now);
    registry.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// 撤销分享
pub fn revoke_share(token: &str) -> bool {
    let mut registry = SHARE_REGISTRY.lock().unwrap();
    registry.remove(token).is_some()
}

/// 从 settings 表中读取 file_share_base_url，如果没有则使用默认值
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let local_addr = socket.local_addr().ok()?;
    Some(local_addr.ip().to_string())
}

/// 从 settings 表中读取 file_share_base_url，如果没有则使用局域网IP
pub fn get_base_url() -> String {
    let data_dir = crate::get_data_dir();
    let db_path = data_dir.join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        if let Ok(val) = conn.query_row(
            "SELECT value FROM settings WHERE key = 'file_share_base_url'",
            [],
            |row| row.get::<_, String>(0),
        ) {
            if !val.trim().is_empty() {
                return val.trim().to_string();
            }
        }
    }
    
    // 如果没有配置，尝试获取本地局域网 IP
    if let Some(ip) = get_local_ip() {
        format!("http://{}:3722", ip)
    } else {
        "http://127.0.0.1:3722".to_string()
    }
}
