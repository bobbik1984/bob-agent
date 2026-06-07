//! GCP Service Account 鉴权引擎
//!
//! 实现 Google Cloud Platform 的 Service Account JSON → JWT → Access Token 完整鉴权流。
//! 用于 Vertex AI 上的 Gemini / Claude 模型调用。
//!
//! 流程：
//! 1. 解析用户上传的 Service Account JSON 文件
//! 2. 用 private_key (RSA) 签署一个 JWT (RS256)
//! 3. 拿 JWT 去 https://oauth2.googleapis.com/token 交换 Access Token
//! 4. 缓存 Token，过期前 5 分钟自动刷新

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

/// Google OAuth2 token endpoint
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// Vertex AI 所需的 OAuth scope
const VERTEX_AI_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
/// Token 提前刷新的缓冲时间（秒）
const TOKEN_REFRESH_BUFFER_SECS: i64 = 300; // 5 分钟

// ═══════════════════════════════════════════════════════════
// Service Account JSON 结构
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceAccountInfo {
    #[serde(rename = "type")]
    pub account_type: String,
    pub project_id: String,
    pub private_key_id: String,
    pub private_key: String,
    pub client_email: String,
    pub client_id: String,
    pub auth_uri: Option<String>,
    pub token_uri: Option<String>,
}

impl ServiceAccountInfo {
    /// 从 JSON 文件解析 Service Account 信息
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("无法读取凭证文件: {}", e))?;
        let info: ServiceAccountInfo = serde_json::from_str(&content)
            .map_err(|e| format!("凭证文件格式无效: {}", e))?;
        
        if info.account_type != "service_account" {
            return Err(format!("凭证类型必须是 service_account，当前为: {}", info.account_type));
        }
        if info.private_key.is_empty() {
            return Err("凭证文件缺少 private_key 字段".to_string());
        }
        if info.client_email.is_empty() {
            return Err("凭证文件缺少 client_email 字段".to_string());
        }
        
        Ok(info)
    }

    /// 从 JSON 字符串解析（用于验证）
    pub fn from_json_str(json_str: &str) -> Result<Self, String> {
        let info: ServiceAccountInfo = serde_json::from_str(json_str)
            .map_err(|e| format!("JSON 解析失败: {}", e))?;
        if info.account_type != "service_account" {
            return Err(format!("凭证类型必须是 service_account，当前为: {}", info.account_type));
        }
        Ok(info)
    }
}

// ═══════════════════════════════════════════════════════════
// JWT 构建与签名
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,   // client_email
    scope: String, // OAuth scope
    aud: String,   // token endpoint
    iat: i64,      // issued at
    exp: i64,      // expiration
}

/// 使用 Service Account 的 private_key 签署 JWT (RS256)
fn create_signed_jwt(sa: &ServiceAccountInfo) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp();
    
    let claims = JwtClaims {
        iss: sa.client_email.clone(),
        scope: VERTEX_AI_SCOPE.to_string(),
        aud: sa.token_uri.as_deref().unwrap_or(GOOGLE_TOKEN_URL).to_string(),
        iat: now,
        exp: now + 3600, // 1 小时有效
    };

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
        .map_err(|e| format!("RSA 私钥解析失败: {}", e))?;
    
    jsonwebtoken::encode(&header, &claims, &key)
        .map_err(|e| format!("JWT 签名失败: {}", e))
}

// ═══════════════════════════════════════════════════════════
// Token 缓存与自动刷新
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: i64, // Unix timestamp
}

impl CachedToken {
    fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now >= (self.expires_at - TOKEN_REFRESH_BUFFER_SECS)
    }
}

/// GCP Token 管理器 — 线程安全，支持并发读取与自动刷新
pub struct GcpTokenManager {
    service_account: ServiceAccountInfo,
    cached_token: RwLock<Option<CachedToken>>,
}

impl GcpTokenManager {
    /// 从 Service Account JSON 文件创建 Token 管理器
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let sa = ServiceAccountInfo::from_file(path)?;
        Ok(Self {
            service_account: sa,
            cached_token: RwLock::new(None),
        })
    }

    /// 从 ServiceAccountInfo 直接创建
    pub fn from_info(info: ServiceAccountInfo) -> Self {
        Self {
            service_account: info,
            cached_token: RwLock::new(None),
        }
    }

    /// 获取 project_id
    pub fn project_id(&self) -> &str {
        &self.service_account.project_id
    }

    /// 获取 client_email（用于 UI 显示）
    pub fn client_email(&self) -> &str {
        &self.service_account.client_email
    }

    /// 获取有效的 Access Token（自动刷新过期 Token）
    pub async fn get_access_token(&self) -> Result<String, String> {
        // 1. 尝试读取缓存
        {
            let cached = self.cached_token.read().await;
            if let Some(token) = cached.as_ref() {
                if !token.is_expired() {
                    return Ok(token.access_token.clone());
                }
            }
        }

        // 2. 缓存过期或不存在，刷新 Token
        let new_token = self.refresh_token().await?;
        
        // 3. 写入缓存
        {
            let mut cached = self.cached_token.write().await;
            *cached = Some(new_token.clone());
        }

        Ok(new_token.access_token)
    }

    /// 向 Google OAuth2 端点请求新的 Access Token
    async fn refresh_token(&self) -> Result<CachedToken, String> {
        let jwt = create_signed_jwt(&self.service_account)?;
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| format!("HTTP 客户端创建失败: {}", e))?;

        let token_url = self.service_account.token_uri.as_deref().unwrap_or(GOOGLE_TOKEN_URL);
        
        let form_body = format!(
            "grant_type={}&assertion={}",
            urlencoding::encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
            urlencoding::encode(&jwt),
        );

        let resp = client
            .post(token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await
            .map_err(|e| format!("Token 请求网络错误: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Token 请求失败 (HTTP {}): {}", status, body));
        }

        let data: Value = resp.json().await
            .map_err(|e| format!("Token 响应解析失败: {}", e))?;

        let access_token = data.get("access_token")
            .and_then(|v| v.as_str())
            .ok_or("Token 响应中缺少 access_token 字段")?
            .to_string();

        let expires_in = data.get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(3600);

        let now = chrono::Utc::now().timestamp();

        log::info!("GCP Access Token 刷新成功 (有效期 {}s, project: {})", 
            expires_in, self.service_account.project_id);

        Ok(CachedToken {
            access_token,
            expires_at: now + expires_in,
        })
    }
}

// ═══════════════════════════════════════════════════════════
// 凭证文件管理（存储/读取/删除）
// ═══════════════════════════════════════════════════════════

/// 获取凭证存储目录
fn get_credentials_dir() -> PathBuf {
    let dir = super::get_data_dir().join("credentials");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 获取 GCP 凭证文件路径
pub fn get_gcp_credential_path() -> PathBuf {
    get_credentials_dir().join("gcp_service_account.json")
}

/// 保存 GCP 凭证文件（从用户选择的源路径复制到安全目录）
pub fn save_gcp_credential(source_path: &str) -> Result<Value, String> {
    let source = Path::new(source_path);
    if !source.exists() {
        return Err("源文件不存在".to_string());
    }

    // 读取并验证 JSON 格式
    let content = std::fs::read_to_string(source)
        .map_err(|e| format!("无法读取文件: {}", e))?;
    let sa = ServiceAccountInfo::from_json_str(&content)?;

    // 复制到安全目录
    let dest = get_gcp_credential_path();
    std::fs::write(&dest, &content)
        .map_err(|e| format!("无法保存凭证文件: {}", e))?;

    // 在 config.json 中记录路径
    let mut config = super::read_config();
    if let Some(obj) = config.as_object_mut() {
        obj.insert("gcpCredentialPath".to_string(), json!(dest.to_string_lossy().to_string()));
        obj.insert("gcpProjectId".to_string(), json!(sa.project_id));
        obj.insert("gcpClientEmail".to_string(), json!(sa.client_email));
    }
    super::write_config(&config);

    log::info!("GCP 凭证已保存: project={}, email={}", sa.project_id, sa.client_email);

    Ok(json!({
        "ok": true,
        "project_id": sa.project_id,
        "client_email": sa.client_email,
    }))
}

/// 删除 GCP 凭证
pub fn remove_gcp_credential() -> Result<Value, String> {
    let path = get_gcp_credential_path();
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("无法删除凭证文件: {}", e))?;
    }

    // 清除 config 中的记录
    let mut config = super::read_config();
    if let Some(obj) = config.as_object_mut() {
        obj.remove("gcpCredentialPath");
        obj.remove("gcpProjectId");
        obj.remove("gcpClientEmail");
    }
    super::write_config(&config);

    log::info!("GCP 凭证已删除");
    Ok(json!({ "ok": true }))
}

/// 获取当前 GCP 凭证状态
pub fn get_gcp_credential_status() -> Value {
    let config = super::read_config();
    let path = get_gcp_credential_path();
    
    if path.exists() {
        let project_id = config.get("gcpProjectId").and_then(|v| v.as_str()).unwrap_or("未知");
        let client_email = config.get("gcpClientEmail").and_then(|v| v.as_str()).unwrap_or("未知");
        json!({
            "configured": true,
            "project_id": project_id,
            "client_email": client_email,
        })
    } else {
        json!({ "configured": false })
    }
}

/// 测试 GCP 凭证连通性 — 尝试获取一次 Access Token
pub async fn test_gcp_credential() -> Result<Value, String> {
    let path = get_gcp_credential_path();
    if !path.exists() {
        return Err("未配置 GCP 凭证文件".to_string());
    }

    let manager = GcpTokenManager::from_file(&path)?;
    let token = manager.get_access_token().await?;

    // Token 成功获取，只显示前 10 个字符
    let preview: String = token.chars().take(10).collect();

    Ok(json!({
        "ok": true,
        "project_id": manager.project_id(),
        "token_preview": format!("{}...", preview),
    }))
}

// ═══════════════════════════════════════════════════════════
// Vertex AI URL 构建
// ═══════════════════════════════════════════════════════════

/// 为 Vertex AI 上的 Gemini 模型构建 OpenAI 兼容的 Chat Completions URL
/// Vertex AI 提供的 OpenAI 兼容端点格式：
/// https://{region}-aiplatform.googleapis.com/v1beta1/projects/{project_id}/locations/{region}/endpoints/openapi/chat/completions
pub fn build_vertex_gemini_url(project_id: &str, region: &str) -> String {
    format!(
        "https://{}-aiplatform.googleapis.com/v1beta1/projects/{}/locations/{}/endpoints/openapi",
        region, project_id, region
    )
}
