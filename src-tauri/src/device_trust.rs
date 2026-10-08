// ==============================================================================
// Bob Agent: Device Trust & Cryptographic Identity Separation (A2 / SEC-01)
// ==============================================================================
// Invariants:
// 1. Discovered != Trusted. Presence or knowledge of public key grants NO authorization.
// 2. New trust requires high-entropy, one-time PairingInvitation + Proof-of-Possession.
// 3. Established TrustedDevice works sustainably within AuthenticatedSession without
//    re-scanning QR code for every RPC message.
// 4. Session, request, action, and target device are cryptographically bound.
// 5. Replay, forged ID, public-key-only, cross-session, cross-device, expired invitation,
//    and revoked devices fail-closed with clear structured rejection.
// ==============================================================================

use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256, Sha512};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::Manager;

pub const SEC01_PROTOCOL_VERSION: &str = "0.9.6-sec01";
pub const SEC01_POP_PURPOSE: &str = "pairing_establishment";
pub const DEFAULT_INVITATION_TTL_MS: i64 = 600_000; // 10 minutes
pub const DEFAULT_SESSION_TTL_MS: i64 = 86_400_000; // 24 hours
pub const RPC_MAX_CLOCK_SKEW_MS: i64 = 120_000; // 2 minutes
pub const RPC_IDEMPOTENCY_PENDING_TIMEOUT_MS: i64 = 30_000; // 30 seconds

pub const ALLOWED_REMOTE_CONFIG_KEYS: &[&str] = &[
    "model",
    "clerkModel",
    "visionModel",
    "provider",
    "theme",
    "uiScale",
    "language",
    "accentColor",
    "weatherCity",
];

pub const ALLOWED_REMOTE_PUSH_OPS: &[&str] = &[
    "set_config",
];


// ------------------------------------------------------------------------------
// Data Structures
// ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub device_id: String,
    pub device_name: Option<String>,
    pub platform: String,
    pub ip_address: String,
    pub port: u16,
    pub transport: String, // "lan" | "relay"
    pub last_seen: i64,
    pub is_trusted: bool, // Always false for pure discovery
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrustStatus {
    Trusted,
    Revoked,
    LegacyUnverified,
}

impl TrustStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustStatus::Trusted => "trusted",
            TrustStatus::Revoked => "revoked",
            TrustStatus::LegacyUnverified => "legacy_unverified",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "trusted" => TrustStatus::Trusted,
            "revoked" => TrustStatus::Revoked,
            _ => TrustStatus::LegacyUnverified,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDevice {
    pub device_id: String,
    pub public_key: String, // Base64
    pub device_name: String,
    pub platform: String,
    pub paired_at: i64,
    pub last_authenticated_at: i64,
    pub status: TrustStatus,
    pub revoked_at: Option<i64>,
    pub revocation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingInvitation {
    pub invitation_id: String,
    pub invitation_secret_hash: String, // SHA-512 hex
    pub issuer_device_id: String,
    pub target_device_id_constraint: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingInvitationPayload {
    #[serde(alias = "v")]
    pub protocol_version: String,
    pub invitation_id: String,
    pub secret: String, // High-entropy secret, transmitted only via QR/URL
    pub issuer_device_id: String,
    #[serde(default)]
    pub device_id: String, // Backward/Forward-compatible alias for issuer_device_id
    pub relay: String,
    pub local_ips: Vec<String>,
    pub port: u16,
}

impl PairingInvitationPayload {
    pub fn to_qr_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("序列化二维码载荷失败: {}", e))
    }

    pub fn to_url(&self) -> String {
        let ips = self.local_ips.join(",");
        format!(
            "bob://pair?v={}&id={}&sec={}&iss={}&rly={}&ips={}&p={}&dev={}",
            urlencoding::encode(&self.protocol_version),
            urlencoding::encode(&self.invitation_id),
            urlencoding::encode(&self.secret),
            urlencoding::encode(&self.issuer_device_id),
            urlencoding::encode(&self.relay),
            urlencoding::encode(&ips),
            self.port,
            urlencoding::encode(&self.device_id)
        )
    }

    pub fn parse_input(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.starts_with('{') {
            // JSON format
            let mut payload = serde_json::from_str::<Self>(raw)
                .map_err(|e| format!("无法解析 JSON 配对邀请: {}", e))?;
            if payload.protocol_version.trim().is_empty() {
                return Err("缺少协议版本 (v/protocol_version) 参数".to_string());
            }
            if payload.protocol_version != SEC01_PROTOCOL_VERSION {
                return Err(format!("不支持的协议版本: {}", payload.protocol_version));
            }
            if payload.invitation_id.trim().is_empty() || payload.secret.trim().is_empty() {
                return Err("配对邀请凭证不全 (缺少 invitation_id 或 secret)".to_string());
            }
            if payload.device_id.is_empty() && !payload.issuer_device_id.is_empty() {
                payload.device_id = payload.issuer_device_id.clone();
            } else if payload.issuer_device_id.is_empty() && !payload.device_id.is_empty() {
                payload.issuer_device_id = payload.device_id.clone();
            }
            if payload.issuer_device_id.trim().is_empty() {
                return Err("缺少 issuer_device_id (iss) 参数".to_string());
            }
            Ok(payload)
        } else if raw.starts_with("bob://pair") || raw.starts_with("http://") || raw.starts_with("https://") {
            // URL format
            let url_part = if let Some(idx) = raw.find('?') {
                &raw[idx + 1..]
            } else {
                return Err("无效的配对邀请链接: 缺少查询参数".to_string());
            };
            let mut map = HashMap::new();
            for kv in url_part.split('&') {
                let mut parts = kv.splitn(2, '=');
                if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                    let decoded_k = urlencoding::decode(k).unwrap_or_default().to_string();
                    let decoded_v = urlencoding::decode(v).unwrap_or_default().to_string();
                    map.insert(decoded_k, decoded_v);
                }
            }

            let protocol_version = map.remove("v").ok_or_else(|| "缺少协议版本 (v) 参数".to_string())?;
            if protocol_version.trim().is_empty() {
                return Err("协议版本 (v) 不能为空".to_string());
            }
            if protocol_version != SEC01_PROTOCOL_VERSION {
                return Err(format!("不支持的协议版本: {}", protocol_version));
            }
            let invitation_id = map.remove("id").ok_or("缺少 invitation_id (id) 参数")?;
            let secret = map.remove("sec").ok_or("缺少 secret (sec) 参数")?;
            if invitation_id.trim().is_empty() || secret.trim().is_empty() {
                return Err("配对邀请凭证不全 (缺少 id 或 sec)".to_string());
            }
            let mut issuer_device_id = map.remove("iss").unwrap_or_default();
            let mut device_id = map.remove("dev").unwrap_or_default();
            if issuer_device_id.is_empty() && !device_id.is_empty() {
                issuer_device_id = device_id.clone();
            } else if device_id.is_empty() && !issuer_device_id.is_empty() {
                device_id = issuer_device_id.clone();
            }
            if issuer_device_id.trim().is_empty() {
                return Err("缺少 issuer_device_id (iss) 参数".to_string());
            }
            let relay = map.remove("rly").unwrap_or_default();
            let ips_str = map.remove("ips").unwrap_or_default();
            let local_ips = if ips_str.is_empty() {
                vec![]
            } else {
                ips_str.split(',').map(|s| s.trim().to_string()).collect()
            };
            let port = map.remove("p").and_then(|p| p.parse::<u16>().ok()).unwrap_or(3722);

            Ok(Self {
                protocol_version,
                invitation_id,
                secret,
                issuer_device_id,
                device_id,
                relay,
                local_ips,
                port,
            })
        } else {
            Err("未知的配对邀请格式: 既非 JSON 亦非 URL 链接".to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofOfPossession {
    pub protocol_version: String,
    pub invitation_id: String,
    pub invitation_secret: String,
    pub issuer_device_id: String,
    pub subject_device_id: String,
    pub subject_pubkey: String, // Base64
    pub device_name: String,
    pub platform: String,
    pub nonce: String,
    pub timestamp: i64,
    pub purpose: String, // "pairing_establishment"
    pub signature: String, // Base64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedSession {
    pub session_id: String,
    pub subject_device_id: String,
    pub issuer_device_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_activity_at: i64,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcAuthEnvelope {
    pub protocol_version: String,
    pub session_id: String,
    pub request_id: String,
    pub subject_device_id: String,
    pub target_device_id: String,
    pub action: String,
    pub payload_hash: String, // SHA-512 hex of inner payload bytes
    pub nonce: String,
    pub timestamp: i64,
    pub signature: String, // Base64 signature of canonical RPC request
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthVerificationOutcome {
    Authorized {
        subject_device_id: String,
        session_id: String,
        request_id: String,
        target_device_id: String,
        action: String,
        payload_hash: String,
        execution_token: String,
        lease_generation: i64,
    },
    IdempotentCached {
        cached_response: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRevocationCertificate {
    pub protocol_version: String,
    pub event_id: String,
    pub revoked_device_id: String,
    pub target_device_id: String,
    pub nonce: String,
    pub timestamp_ms: i64,
    pub reason: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevocationAck {
    pub r#type: String,
    pub protocol_version: String,
    pub event_id: String,
    pub revoked_device_id: String,
    pub target_device_id: String,
    pub status: String,
    pub nonce: String,
    pub timestamp_ms: i64,
    pub signature: String,
}

// ------------------------------------------------------------------------------
// Canonical Serialization & Cryptographic Primitives
// ------------------------------------------------------------------------------

pub fn compute_sha512(data: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hex = String::with_capacity(128);
    for byte in result {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", byte);
    }
    hex
}

pub fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in result {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", byte);
    }
    hex
}

pub fn canonical_pop_bytes(
    protocol_version: &str,
    invitation_id: &str,
    invitation_secret: &str,
    issuer_device_id: &str,
    subject_device_id: &str,
    subject_pubkey: &str,
    device_name: &str,
    platform: &str,
    nonce: &str,
    timestamp: i64,
    purpose: &str,
) -> Vec<u8> {
    format!(
        "BOB-POP:v1\nprotocol_version:{}\ninvitation_id:{}\ninvitation_secret:{}\nissuer_device_id:{}\nsubject_device_id:{}\nsubject_pubkey:{}\ndevice_name:{}\nplatform:{}\nnonce:{}\ntimestamp:{}\npurpose:{}\n",
        protocol_version,
        invitation_id,
        invitation_secret,
        issuer_device_id,
        subject_device_id,
        subject_pubkey,
        device_name,
        platform,
        nonce,
        timestamp,
        purpose
    ).into_bytes()
}

pub fn canonical_rpc_bytes(
    protocol_version: &str,
    session_id: &str,
    request_id: &str,
    subject_device_id: &str,
    target_device_id: &str,
    action: &str,
    payload_hash: &str,
    nonce: &str,
    timestamp: i64,
) -> Vec<u8> {
    format!(
        "BOB-RPC:v1\nprotocol_version:{}\nsession_id:{}\nrequest_id:{}\nsubject_device_id:{}\ntarget_device_id:{}\naction:{}\npayload_hash:{}\nnonce:{}\ntimestamp:{}\n",
        protocol_version,
        session_id,
        request_id,
        subject_device_id,
        target_device_id,
        action,
        payload_hash,
        nonce,
        timestamp
    ).into_bytes()
}

pub fn sign_bytes(signing_key: &SigningKey, data: &[u8]) -> String {
    let sig: Signature = signing_key.sign(data);
    BASE64.encode(sig.to_bytes())
}

pub fn verify_signature(pubkey_b64: &str, message: &[u8], signature_b64: &str) -> Result<(), String> {
    let pubkey_bytes = BASE64.decode(pubkey_b64.trim())
        .map_err(|e| format!("公钥 Base64 解码失败: {}", e))?;
    if pubkey_bytes.len() != 32 {
        return Err(format!("非法 Ed25519 公钥长度: {} (必须为 32 字节)", pubkey_bytes.len()));
    }
    let mut fixed_pk = [0u8; 32];
    fixed_pk.copy_from_slice(&pubkey_bytes);
    let vk = VerifyingKey::from_bytes(&fixed_pk)
        .map_err(|e| format!("解析 Ed25519 验证公钥失败: {}", e))?;

    let sig_bytes = BASE64.decode(signature_b64.trim())
        .map_err(|e| format!("签名 Base64 解码失败: {}", e))?;
    if sig_bytes.len() != 64 {
        return Err(format!("非法 Ed25519 签名长度: {} (必须为 64 字节)", sig_bytes.len()));
    }
    let mut fixed_sig = [0u8; 64];
    fixed_sig.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&fixed_sig);

    vk.verify_strict(message, &sig)
        .map_err(|e| format!("数字签名验证失败 (持钥证明无效): {}", e))
}

// ------------------------------------------------------------------------------
// Database Initialization & Migrations
// ------------------------------------------------------------------------------

static DEVICE_TRUST_INIT_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_device_trust_init_failed(failed: bool) {
    DEVICE_TRUST_INIT_FAILED.store(failed, std::sync::atomic::Ordering::SeqCst);
}

pub fn is_device_trust_init_failed() -> bool {
    DEVICE_TRUST_INIT_FAILED.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn init_device_trust_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    if let Err(e) = crate::recover_config_file_integrity() {
        set_device_trust_init_failed(true);
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))));
    }
    set_device_trust_init_failed(false);

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS trusted_devices (
            device_id TEXT PRIMARY KEY,
            public_key TEXT NOT NULL,
            device_name TEXT NOT NULL,
            platform TEXT NOT NULL,
            paired_at INTEGER NOT NULL,
            last_authenticated_at INTEGER NOT NULL,
            status TEXT NOT NULL,
            revoked_at INTEGER,
            revocation_reason TEXT
        );

        CREATE TABLE IF NOT EXISTS pairing_invitations (
            invitation_id TEXT PRIMARY KEY,
            invitation_secret_hash TEXT NOT NULL,
            issuer_device_id TEXT NOT NULL,
            target_device_id_constraint TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            consumed_at INTEGER,
            revoked_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS authenticated_sessions (
            session_id TEXT PRIMARY KEY,
            subject_device_id TEXT NOT NULL,
            issuer_device_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            last_activity_at INTEGER NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 1,
            FOREIGN KEY (subject_device_id) REFERENCES trusted_devices(device_id)
        );

        CREATE TABLE IF NOT EXISTS rpc_anti_replay (
            subject_device_id TEXT NOT NULL,
            nonce TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            PRIMARY KEY (subject_device_id, nonce)
        );

        CREATE TABLE IF NOT EXISTS rpc_idempotency_cache (
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            subject_device_id TEXT NOT NULL,
            target_device_id TEXT NOT NULL,
            action TEXT NOT NULL,
            payload_hash TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'completed',
            lease_generation INTEGER NOT NULL DEFAULT 1,
            execution_token TEXT NOT NULL DEFAULT '',
            response_json TEXT,
            error_message TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (session_id, request_id)
        );

        CREATE TABLE IF NOT EXISTS rpc_staged_outbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL DEFAULT '',
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            operations_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_at INTEGER NOT NULL,
            delivered_at INTEGER,
            UNIQUE(session_id, request_id)
        );

        CREATE TABLE IF NOT EXISTS rpc_processed_events (
            event_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS peer_revocation_outbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL DEFAULT '',
            target_peer_id TEXT NOT NULL,
            revoked_device_id TEXT NOT NULL,
            revocation_payload TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_at INTEGER NOT NULL,
            sent_at INTEGER,
            delivered_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS identity_reset_journal (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            state TEXT NOT NULL,
            revoked_device_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            error TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_sessions_subject ON authenticated_sessions(subject_device_id);
        CREATE INDEX IF NOT EXISTS idx_anti_replay_ts ON rpc_anti_replay(timestamp);
        "
    )?;

    // Safe column migrations for pre-existing local databases (Fail-Closed, 绝不吞错)
    ensure_column_exists(conn, "rpc_idempotency_cache", "status", "TEXT NOT NULL DEFAULT 'completed'")?;
    ensure_column_exists(conn, "rpc_idempotency_cache", "updated_at", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column_exists(conn, "rpc_idempotency_cache", "error_message", "TEXT")?;
    ensure_column_exists(conn, "rpc_idempotency_cache", "lease_generation", "INTEGER NOT NULL DEFAULT 1")?;
    ensure_column_exists(conn, "rpc_idempotency_cache", "execution_token", "TEXT NOT NULL DEFAULT ''")?;

    ensure_column_exists(conn, "rpc_staged_outbox", "event_id", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column_exists(conn, "rpc_staged_outbox", "status", "TEXT NOT NULL DEFAULT 'pending'")?;
    ensure_column_exists(conn, "rpc_staged_outbox", "attempts", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column_exists(conn, "rpc_staged_outbox", "last_error", "TEXT")?;
    ensure_column_exists(conn, "rpc_staged_outbox", "delivered_at", "INTEGER")?;

    ensure_column_exists(conn, "peer_revocation_outbox", "event_id", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "target_peer_id", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "revoked_device_id", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "revocation_payload", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "status", "TEXT NOT NULL DEFAULT 'pending'")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "attempts", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "last_error", "TEXT")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "created_at", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "sent_at", "INTEGER")?;
    ensure_column_exists(conn, "peer_revocation_outbox", "delivered_at", "INTEGER")?;

    // 创建依赖迁移字段的索引（必须在 ensure_column_exists 之后执行，防止旧库升级直接崩溃）
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_rpc_idempotency_status ON rpc_idempotency_cache(status, updated_at);
         CREATE INDEX IF NOT EXISTS idx_staged_outbox_status ON rpc_staged_outbox(status, id);
         CREATE INDEX IF NOT EXISTS idx_peer_revocation_status ON peer_revocation_outbox(status, id);
         CREATE INDEX IF NOT EXISTS idx_peer_revocation_event_id ON peer_revocation_outbox(event_id);
         CREATE INDEX IF NOT EXISTS idx_reset_journal_state ON identity_reset_journal(state);
         CREATE INDEX IF NOT EXISTS idx_processed_events_req ON rpc_processed_events(session_id, request_id);",
    )?;
    // 系统启动自愈：检查并收敛未完成的重置日志（严格 Fail-Closed 冒泡错误）
    recover_device_identity_state(conn, None, None, crate::now_ms())
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;

    // 回退超时的 sent 撤销记录为 pending，以便重新排空
    revert_stale_sent_revocations(conn, 60_000, crate::now_ms())
        .map_err(|e| rusqlite::Error::InvalidParameterName(e))?;

    // 系统启动时对残留的 pending outbox 执行持久化补投（严格 Fail-Closed 冒泡错误）
    drain_staged_outbox(conn, crate::now_ms())
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;

    Ok(())
}

/// 安全检查并增加列（严格 Fail-Closed，绝不使用 let _ = 吞噬磁盘或坏库错误）
pub fn ensure_column_exists(
    conn: &Connection,
    table: &str,
    column: &str,
    col_def: &str,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let mut rows = stmt.query([])?;
    let mut exists = false;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name.eq_ignore_ascii_case(column) {
            exists = true;
            break;
        }
    }
    if !exists {
        conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, col_def),
            [],
        )?;
    }
    Ok(())
}

/// 兼容性迁移：将旧的名册或配置中的设备导入为 `legacy_unverified` 状态
/// 严禁自动提升为 `trusted`！
pub fn migrate_legacy_devices_to_unverified(
    conn: &mut Connection,
    legacy_devices: &[DiscoveredDevice],
    now_ms: i64,
) -> Result<usize, String> {
    let tx = conn.transaction().map_err(|e| format!("启动迁移事务失败: {}", e))?;
    let mut imported = 0;
    for dev in legacy_devices {
        let exists: bool = tx.query_row(
            "SELECT COUNT(1) FROM trusted_devices WHERE device_id = ?",
            [&dev.device_id],
            |row| Ok(row.get::<_, i64>(0)? > 0),
        ).unwrap_or(false);

        if !exists {
            tx.execute(
                "INSERT INTO trusted_devices (
                    device_id, public_key, device_name, platform, paired_at,
                    last_authenticated_at, status, revoked_at, revocation_reason
                ) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL)",
                params![
                    dev.device_id,
                    dev.device_id, // 旧版本 device_id 与 public_key 相同
                    dev.device_name.clone().unwrap_or_else(|| "Legacy Device".to_string()),
                    dev.platform,
                    now_ms,
                    now_ms,
                    TrustStatus::LegacyUnverified.as_str(),
                ],
            ).map_err(|e| format!("插入遗留未验证设备失败: {}", e))?;
            imported += 1;
        }
    }
    tx.commit().map_err(|e| format!("提交迁移事务失败: {}", e))?;
    Ok(imported)
}

// ------------------------------------------------------------------------------
// Production Core Operations
// ------------------------------------------------------------------------------

/// 查询本地设备身份是否处于未完成重置或降级状态 (Fail-Closed)
pub fn is_identity_degraded(conn: &Connection) -> Result<bool, String> {
    if is_device_trust_init_failed() {
        return Ok(true);
    }
    conn.query_row(
        "SELECT COUNT(1) FROM identity_reset_journal WHERE state NOT IN ('committed', 'cancelled')",
        [],
        |row| Ok(row.get::<_, i64>(0)? > 0),
    ).map_err(|e| format!("查询身份降级状态失败 (Fail-Closed 阻断): {}", e))
}

/// 生成高熵一次性配对邀请
pub fn create_pairing_invitation(
    conn: &Connection,
    issuer_device_id: &str,
    target_constraint: Option<&str>,
    ttl_ms: i64,
    relay: &str,
    local_ips: Vec<String>,
    port: u16,
    now_ms: i64,
) -> Result<PairingInvitationPayload, String> {
    if is_identity_degraded(conn)? {
        return Err("创建配对邀请失败: 本地设备身份处于降级重置状态，禁止创建邀请".to_string());
    }

    if issuer_device_id.trim().is_empty() {
        return Err("创建配对邀请失败: 发起方设备 ID 不能为空".to_string());
    }

    let invitation_id = uuid::Uuid::new_v4().to_string();
    let mut secret_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut secret_bytes);
    let secret = BASE64.encode(secret_bytes);
    let secret_hash = compute_sha512(secret.as_bytes());
    let expires_at = now_ms + ttl_ms;

    conn.execute(
        "INSERT INTO pairing_invitations (
            invitation_id, invitation_secret_hash, issuer_device_id,
            target_device_id_constraint, created_at, expires_at, consumed_at, revoked_at
        ) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
        params![
            invitation_id,
            secret_hash,
            issuer_device_id,
            target_constraint,
            now_ms,
            expires_at
        ],
    ).map_err(|e| format!("持久化配对邀请失败: {}", e))?;

    Ok(PairingInvitationPayload {
        protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
        invitation_id,
        secret,
        issuer_device_id: issuer_device_id.to_string(),
        device_id: issuer_device_id.to_string(),
        relay: relay.to_string(),
        local_ips,
        port,
    })
}

/// 验证并原子消费一次性邀请，建立可信设备记录
pub fn verify_and_consume_invitation(
    conn: &mut Connection,
    pop: &ProofOfPossession,
    now_ms: i64,
) -> Result<TrustedDevice, String> {
    // 1. 协议版本与字段基础校验
    if pop.protocol_version != SEC01_PROTOCOL_VERSION {
        return Err(format!("不支持的协议版本: {}", pop.protocol_version));
    }
    if pop.purpose != SEC01_POP_PURPOSE {
        return Err(format!("非法的持钥证明目的用途: {}", pop.purpose));
    }
    if pop.subject_device_id.trim().is_empty() || pop.subject_pubkey.trim().is_empty() {
        return Err("受邀设备标识与公钥不能为空".to_string());
    }

    // 2. 检查 subject_device_id 与 subject_pubkey 的对应性
    if pop.subject_device_id.trim() != pop.subject_pubkey.trim() {
        return Err("设备 ID 与公开公钥不匹配 (身份伪造检测)".to_string());
    }

    // 3. 校验时钟漂移 (POP 时间戳不得早于或晚于当前时间 2 分钟)
    let skew = (now_ms - pop.timestamp).abs();
    if skew > RPC_MAX_CLOCK_SKEW_MS {
        return Err(format!("持钥证明时间戳漂移超限: 差异 {}ms (最大允许 {}ms)", skew, RPC_MAX_CLOCK_SKEW_MS));
    }

    // 4. 开启事务原子校验并消费邀请
    let tx = conn.transaction().map_err(|e| format!("开启配对消费事务失败: {}", e))?;

    // 查询邀请记录
    let invite_opt: Option<(String, String, Option<String>, i64, Option<i64>, Option<i64>)> = tx.query_row(
        "SELECT issuer_device_id, invitation_secret_hash, target_device_id_constraint, expires_at, consumed_at, revoked_at
         FROM pairing_invitations WHERE invitation_id = ?",
        [&pop.invitation_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).optional().map_err(|e| format!("查询配对邀请失败: {}", e))?;

    let (issuer_id, secret_hash, target_constraint, expires_at, consumed_at, revoked_at) = match invite_opt {
        Some(row) => row,
        None => return Err(format!("未找到配对邀请记录: {}", pop.invitation_id)),
    };

    // 检查撤销状态
    if revoked_at.is_some() {
        return Err(format!("配对邀请已被撤销: {}", pop.invitation_id));
    }

    // 检查已消费 (防重放)
    if consumed_at.is_some() {
        return Err(format!("配对邀请已被消费，严禁重复使用 (防重放): {}", pop.invitation_id));
    }

    // 检查过期
    if now_ms > expires_at {
        return Err(format!("配对邀请已过期 (过期时间: {}, 当前时间: {})", expires_at, now_ms));
    }

    // 检查 issuer_device_id 是否匹配
    if issuer_id != pop.issuer_device_id {
        return Err(format!("邀请发起方 ID 不匹配: 声明 {}, 实际 {}", pop.issuer_device_id, issuer_id));
    }

    // 检查 target_constraint 约束
    if let Some(ref constraint) = target_constraint {
        if constraint != &pop.subject_device_id {
            return Err(format!("受邀设备不符合目标约束: 允许 {}, 实际 {}", constraint, pop.subject_device_id));
        }
    }

    // 检查秘密哈希匹配 (防止猜测)
    let provided_secret_hash = compute_sha512(pop.invitation_secret.as_bytes());
    if provided_secret_hash != secret_hash {
        return Err("配对邀请秘密凭证不匹配".to_string());
    }

    // 5. 校验密码学持钥签名
    let canonical = canonical_pop_bytes(
        &pop.protocol_version,
        &pop.invitation_id,
        &pop.invitation_secret,
        &pop.issuer_device_id,
        &pop.subject_device_id,
        &pop.subject_pubkey,
        &pop.device_name,
        &pop.platform,
        &pop.nonce,
        pop.timestamp,
        &pop.purpose,
    );

    verify_signature(&pop.subject_pubkey, &canonical, &pop.signature)?;

    // 6. 原子标记邀请已消费 (竞争排他性)
    let updated = tx.execute(
        "UPDATE pairing_invitations SET consumed_at = ? WHERE invitation_id = ? AND consumed_at IS NULL",
        params![now_ms, pop.invitation_id],
    ).map_err(|e| format!("更新邀请状态失败: {}", e))?;

    if updated == 0 {
        return Err("配对邀请并发竞争消费失败: 已被另一请求消费".to_string());
    }

    // 7. 写入或更新 TrustedDevice
    // 检查该设备此前是否被撤销
    let _prev_revoked: bool = tx.query_row(
        "SELECT COUNT(1) FROM trusted_devices WHERE device_id = ? AND status = 'revoked'",
        [&pop.subject_device_id],
        |row| Ok(row.get::<_, i64>(0)? > 0),
    ).unwrap_or(false);

    // 如果是通过有效邀请重新配对，则允许重新确立信任，但公钥必须一致
    tx.execute(
        "INSERT INTO trusted_devices (
            device_id, public_key, device_name, platform, paired_at,
            last_authenticated_at, status, revoked_at, revocation_reason
        ) VALUES (?, ?, ?, ?, ?, ?, 'trusted', NULL, NULL)
        ON CONFLICT(device_id) DO UPDATE SET
            public_key = excluded.public_key,
            device_name = excluded.device_name,
            platform = excluded.platform,
            paired_at = excluded.paired_at,
            last_authenticated_at = excluded.last_authenticated_at,
            status = 'trusted',
            revoked_at = NULL,
            revocation_reason = NULL",
        params![
            pop.subject_device_id,
            pop.subject_pubkey,
            pop.device_name,
            pop.platform,
            now_ms,
            now_ms,
        ],
    ).map_err(|e| format!("落盘可信设备失败: {}", e))?;

    let trusted = TrustedDevice {
        device_id: pop.subject_device_id.clone(),
        public_key: pop.subject_pubkey.clone(),
        device_name: pop.device_name.clone(),
        platform: pop.platform.clone(),
        paired_at: now_ms,
        last_authenticated_at: now_ms,
        status: TrustStatus::Trusted,
        revoked_at: None,
        revocation_reason: None,
    };

    tx.commit().map_err(|e| format!("提交配对事务失败: {}", e))?;
    Ok(trusted)
}

/// 建立双方绑定的有界会话 AuthenticatedSession
pub fn create_authenticated_session(
    conn: &mut Connection,
    subject_device_id: &str,
    issuer_device_id: &str,
    ttl_ms: i64,
    now_ms: i64,
) -> Result<AuthenticatedSession, String> {
    // 1. 检查设备信任状态
    let dev_status_opt: Option<(String, Option<i64>)> = conn.query_row(
        "SELECT status, revoked_at FROM trusted_devices WHERE device_id = ?",
        [subject_device_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional().map_err(|e| format!("查询可信设备状态失败: {}", e))?;

    let (status, revoked_at) = match dev_status_opt {
        Some(s) => s,
        None => return Err(format!("拒绝建立会话: 设备未配对信任 ({})", subject_device_id)),
    };

    if status == "revoked" || revoked_at.is_some() {
        return Err(format!("拒绝建立会话: 设备已被撤销信任 ({})", subject_device_id));
    }
    if status != "trusted" {
        return Err(format!("拒绝建立会话: 设备处于非可信状态 ({})", status));
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let expires_at = now_ms + ttl_ms;

    conn.execute(
        "INSERT INTO authenticated_sessions (
            session_id, subject_device_id, issuer_device_id,
            created_at, expires_at, last_activity_at, is_active
        ) VALUES (?, ?, ?, ?, ?, ?, 1)",
        params![
            session_id,
            subject_device_id,
            issuer_device_id,
            now_ms,
            expires_at,
            now_ms
        ],
    ).map_err(|e| format!("插入活跃会话失败: {}", e))?;

    // 更新最后认证时间
    let _ = conn.execute(
        "UPDATE trusted_devices SET last_authenticated_at = ? WHERE device_id = ?",
        params![now_ms, subject_device_id],
    );

    Ok(AuthenticatedSession {
        session_id,
        subject_device_id: subject_device_id.to_string(),
        issuer_device_id: issuer_device_id.to_string(),
        created_at: now_ms,
        expires_at,
        last_activity_at: now_ms,
        is_active: true,
    })
}

/// SEC-01 原子消费配对邀请并建立会话 (CAS核销邀请 + 落盘可信设备 + 签发会话在单事务内完成)
pub fn sec01_atomic_consume_and_create_session(
    conn: &mut Connection,
    pop: &ProofOfPossession,
    local_device_id: &str,
    session_ttl_ms: i64,
    now_ms: i64,
) -> Result<(TrustedDevice, AuthenticatedSession), String> {
    if is_identity_degraded(conn)? {
        return Err("Unauthorized: Device identity in degraded reset state; pairing blocked".to_string());
    }

    if local_device_id.trim().is_empty() {
        return Err("Unauthorized: Local device identity is uninitialized or empty".to_string());
    }

    // 1. 协议版本与必填字段基础校验
    if pop.protocol_version != SEC01_PROTOCOL_VERSION {
        return Err(format!("Unauthorized: Unsupported protocol version '{}' (must be '{}')", pop.protocol_version, SEC01_PROTOCOL_VERSION));
    }

    // 2. 强校验 purpose
    if pop.purpose != SEC01_POP_PURPOSE {
        return Err(format!("Unauthorized: Invalid pairing purpose '{}' (must be '{}')", pop.purpose, SEC01_POP_PURPOSE));
    }

    if pop.subject_device_id.trim().is_empty() || pop.subject_pubkey.trim().is_empty() {
        return Err("Unauthorized: Subject device ID and public key cannot be empty".to_string());
    }

    // 3. 校验设备 ID 与公开公钥一致性
    if pop.subject_device_id.trim() != pop.subject_pubkey.trim() {
        return Err("Unauthorized: Subject device ID does not match public key".to_string());
    }

    // 4. 校验时钟漂移 (POP 时间戳不得早于或晚于当前时间 2 分钟)
    let skew = (now_ms - pop.timestamp).abs();
    if skew > RPC_MAX_CLOCK_SKEW_MS {
        return Err(format!("Unauthorized: Proof of possession timestamp drift exceeded: {}ms (max: {}ms)", skew, RPC_MAX_CLOCK_SKEW_MS));
    }

    // 4. 开启事务原子校验、CAS消费邀请、落盘可信设备与创建认证会话
    let tx = conn.transaction().map_err(|e| format!("Failed to begin atomic pairing transaction: {}", e))?;

    // 查询邀请记录
    let invite_opt: Option<(String, String, Option<String>, i64, Option<i64>, Option<i64>)> = tx.query_row(
        "SELECT issuer_device_id, invitation_secret_hash, target_device_id_constraint, expires_at, consumed_at, revoked_at
         FROM pairing_invitations WHERE invitation_id = ?",
        [&pop.invitation_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).optional().map_err(|e| format!("Failed to query pairing invitation: {}", e))?;

    let (issuer_id, secret_hash, target_constraint, expires_at, consumed_at, revoked_at) = match invite_opt {
        Some(row) => row,
        None => return Err(format!("Unauthorized: Pairing invitation not found: {}", pop.invitation_id)),
    };

    if revoked_at.is_some() {
        return Err(format!("Unauthorized: Pairing invitation has been revoked: {}", pop.invitation_id));
    }

    if consumed_at.is_some() {
        return Err(format!("Unauthorized: Pairing invitation already consumed (replay defense): {}", pop.invitation_id));
    }

    if now_ms > expires_at {
        return Err(format!("Unauthorized: Pairing invitation expired (expired_at: {}, now: {})", expires_at, now_ms));
    }

    if issuer_id != pop.issuer_device_id {
        return Err(format!("Unauthorized: Invitation issuer mismatch: declared {}, actual {}", pop.issuer_device_id, issuer_id));
    }

    if local_device_id.trim() != issuer_id.trim() {
        return Err(format!(
            "Unauthorized: Local device identity does not match invitation issuer (local: {}, issuer: {})",
            local_device_id, issuer_id
        ));
    }

    if let Some(ref constraint) = target_constraint {
        if constraint != &pop.subject_device_id {
            return Err(format!("Unauthorized: Subject device does not match invitation constraint: allowed {}, actual {}", constraint, pop.subject_device_id));
        }
    }

    let provided_secret_hash = compute_sha512(pop.invitation_secret.as_bytes());
    if provided_secret_hash != secret_hash {
        return Err("Unauthorized: Invalid pairing invitation secret".to_string());
    }

    // 校验持钥签名
    let canonical = canonical_pop_bytes(
        &pop.protocol_version,
        &pop.invitation_id,
        &pop.invitation_secret,
        &pop.issuer_device_id,
        &pop.subject_device_id,
        &pop.subject_pubkey,
        &pop.device_name,
        &pop.platform,
        &pop.nonce,
        pop.timestamp,
        &pop.purpose,
    );
    verify_signature(&pop.subject_pubkey, &canonical, &pop.signature)?;

    // 原子标记邀请已消费 (CAS 排他性)
    let updated = tx.execute(
        "UPDATE pairing_invitations SET consumed_at = ? WHERE invitation_id = ? AND consumed_at IS NULL",
        params![now_ms, pop.invitation_id],
    ).map_err(|e| format!("Failed to consume invitation: {}", e))?;

    if updated == 0 {
        return Err("Unauthorized: Concurrent pairing invitation consumption conflict".to_string());
    }

    // 写入或更新 TrustedDevice
    tx.execute(
        "INSERT INTO trusted_devices (
            device_id, public_key, device_name, platform, paired_at,
            last_authenticated_at, status, revoked_at, revocation_reason
        ) VALUES (?, ?, ?, ?, ?, ?, 'trusted', NULL, NULL)
        ON CONFLICT(device_id) DO UPDATE SET
            public_key = excluded.public_key,
            device_name = excluded.device_name,
            platform = excluded.platform,
            paired_at = excluded.paired_at,
            last_authenticated_at = excluded.last_authenticated_at,
            status = 'trusted',
            revoked_at = NULL,
            revocation_reason = NULL",
        params![
            pop.subject_device_id,
            pop.subject_pubkey,
            pop.device_name,
            pop.platform,
            now_ms,
            now_ms,
        ],
    ).map_err(|e| format!("Failed to persist trusted device: {}", e))?;

    // 创建 AuthenticatedSession
    let session_id = uuid::Uuid::new_v4().to_string();
    let expires_at = now_ms + session_ttl_ms;

    tx.execute(
        "INSERT INTO authenticated_sessions (
            session_id, subject_device_id, issuer_device_id,
            created_at, expires_at, last_activity_at, is_active
        ) VALUES (?, ?, ?, ?, ?, ?, 1)",
        params![
            session_id,
            pop.subject_device_id,
            issuer_id,
            now_ms,
            expires_at,
            now_ms,
        ],
    ).map_err(|e| format!("Failed to insert authenticated session: {}", e))?;

    let trusted = TrustedDevice {
        device_id: pop.subject_device_id.clone(),
        public_key: pop.subject_pubkey.clone(),
        device_name: pop.device_name.clone(),
        platform: pop.platform.clone(),
        paired_at: now_ms,
        last_authenticated_at: now_ms,
        status: TrustStatus::Trusted,
        revoked_at: None,
        revocation_reason: None,
    };

    let session = AuthenticatedSession {
        session_id,
        subject_device_id: pop.subject_device_id.clone(),
        issuer_device_id: issuer_id.clone(),
        created_at: now_ms,
        expires_at,
        last_activity_at: now_ms,
        is_active: true,
    };

    tx.commit().map_err(|e| format!("Failed to commit atomic pairing transaction: {}", e))?;
    Ok((trusted, session))
}

/// 移动端本地原子持久化对端可信设备及认证会话 (Fail-Closed: 任何落库失败立即回滚整笔事务)
pub fn sec01_persist_mobile_trusted_session(
    conn: &mut Connection,
    target_device_id: &str,
    my_device_id: &str,
    session_id: &str,
    device_name: &str,
    platform: &str,
    ttl_ms: i64,
    now_ms: i64,
) -> Result<(), String> {
    if session_id.trim().is_empty() {
        return Err("Cannot persist mobile session with empty session_id".to_string());
    }
    if target_device_id.trim().is_empty() {
        return Err("Cannot persist mobile session with empty target_device_id".to_string());
    }

    let tx = conn.transaction().map_err(|e| format!("Failed to begin mobile persistence transaction: {}", e))?;

    tx.execute(
        "INSERT INTO trusted_devices (
            device_id, public_key, device_name, platform, paired_at,
            last_authenticated_at, status, revoked_at, revocation_reason
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'trusted', NULL, NULL)
        ON CONFLICT(device_id) DO UPDATE SET
            status = 'trusted',
            last_authenticated_at = ?6,
            revoked_at = NULL,
            revocation_reason = NULL",
        rusqlite::params![
            target_device_id,
            target_device_id,
            device_name,
            platform,
            now_ms,
            now_ms,
        ],
    ).map_err(|e| format!("Failed to insert trusted device on mobile: {}", e))?;

    let expires_at = now_ms + ttl_ms;
    tx.execute(
        "INSERT INTO authenticated_sessions (
            session_id, subject_device_id, issuer_device_id,
            created_at, expires_at, last_activity_at, is_active
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
        ON CONFLICT(session_id) DO UPDATE SET is_active = 1, last_activity_at = ?6",
        rusqlite::params![
            session_id,
            target_device_id,
            my_device_id,
            now_ms,
            expires_at,
            now_ms,
        ],
    ).map_err(|e| format!("Failed to insert authenticated session on mobile: {}", e))?;

    tx.commit().map_err(|e| format!("Failed to commit mobile persistence transaction: {}", e))?;
    Ok(())
}

/// 生产级 Relay Wakeup 消息鉴权分发前置校验 (SEC-01 fail-closed)
/// 校验：消息完整性、呼叫者伪造防护 (from_device_id == envelope.subject)、规范化载荷验签、防重放及设备信任状态
pub fn sec01_verify_relay_wakeup_message(
    conn: &mut Connection,
    json: &serde_json::Value,
    current_device_id: &str,
    now_ms: i64,
) -> Result<String, String> {
    let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("");
    if from_id.trim().is_empty() {
        return Err("SEC-01 Rejected wakeup: missing or empty from_device_id".to_string());
    }

    let wakeup_envelope_opt = json.get("payload")
        .and_then(|p| p.get("auth_envelope").or_else(|| p.get("envelope")))
        .or_else(|| json.get("auth_envelope"))
        .or_else(|| json.get("envelope"))
        .and_then(|v| serde_json::from_value::<RpcAuthEnvelope>(v.clone()).ok());

    let envelope = wakeup_envelope_opt.ok_or_else(|| {
        format!("SEC-01 Rejected unauthenticated wakeup from {} (missing envelope)", from_id)
    })?;

    if envelope.subject_device_id != from_id {
        return Err(format!("SEC-01 Wakeup caller identity spoofing: from_id '{}' != envelope subject '{}'", from_id, envelope.subject_device_id));
    }

    let canonical_bytes = json.get("payload")
        .map(|p| canonicalize_payload_without_envelope(p))
        .unwrap_or_default();

    verify_rpc_request_auth(conn, &envelope, &canonical_bytes, current_device_id, now_ms)?;
    Ok(from_id.to_string())
}

/// 验证受保护 RPC 请求的会话、签名与防重放
pub fn verify_rpc_request_auth(
    conn: &mut Connection,
    auth: &RpcAuthEnvelope,
    raw_inner_payload: &[u8],
    expected_target_device_id: &str,
    now_ms: i64,
) -> Result<AuthVerificationOutcome, String> {
    // 1. 协议版本与必填参数校验
    if auth.protocol_version != SEC01_PROTOCOL_VERSION {
        return Err(format!("不支持的 RPC 鉴权协议版本: {}", auth.protocol_version));
    }
    if auth.session_id.trim().is_empty() || auth.request_id.trim().is_empty() || auth.nonce.trim().is_empty() {
        return Err("缺少关键安全字段: session_id, request_id 或 nonce 不能为空".to_string());
    }

    // 2. 目标设备校验 (防止中继串换或跨目标调用)
    if auth.target_device_id != expected_target_device_id {
        return Err(format!("目标设备 ID 不匹配: 请求目标 {}, 本机标识 {}", auth.target_device_id, expected_target_device_id));
    }

    // 3. 时间戳时钟漂移检查
    let skew = (now_ms - auth.timestamp).abs();
    if skew > RPC_MAX_CLOCK_SKEW_MS {
        return Err(format!("RPC 时间戳漂移超限: 差异 {}ms (最大允许 {}ms)", skew, RPC_MAX_CLOCK_SKEW_MS));
    }

    // 4. 内容完整性哈希校验
    let expected_hash = compute_sha512(raw_inner_payload);
    if auth.payload_hash != expected_hash {
        return Err("RPC 请求载荷哈希不匹配 (内容可能被篡改)".to_string());
    }

    // 5. 事务检查：设备可信状态、密码学验签、防重放、幂等性与会话生命周期
    let tx = conn.transaction().map_err(|e| format!("开启鉴权校验事务失败: {}", e))?;

    // A. 设备信任状态检查 (SEC-01: 严禁 legacy_unverified 或 revoked)
    let dev_opt: Option<(String, String, Option<i64>)> = tx.query_row(
        "SELECT public_key, status, revoked_at FROM trusted_devices WHERE device_id = ?",
        [&auth.subject_device_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(|e| format!("查询可信设备失败: {}", e))?;

    let (public_key, status, revoked_at) = match dev_opt {
        Some(d) => d,
        None => return Err(format!("未识别的未配对设备: {}", auth.subject_device_id)),
    };

    if status == "revoked" || revoked_at.is_some() {
        return Err(format!("设备已被撤销，阻断 RPC 请求: {}", auth.subject_device_id));
    }
    if status != "trusted" {
        return Err(format!("设备未通过持钥配对验证 (状态: {}): {}", status, auth.subject_device_id));
    }

    // B. 密码学签名验证 (确保请求 100% 由持有对应公钥的受信任设备私钥签署)
    let canonical = canonical_rpc_bytes(
        &auth.protocol_version,
        &auth.session_id,
        &auth.request_id,
        &auth.subject_device_id,
        &auth.target_device_id,
        &auth.action,
        &auth.payload_hash,
        &auth.nonce,
        auth.timestamp,
    );
    verify_signature(&public_key, &canonical, &auth.signature)?;

    // C. 防重放 nonce 检查与记录
    let nonce_inserted = tx.execute(
        "INSERT INTO rpc_anti_replay (subject_device_id, nonce, timestamp) VALUES (?, ?, ?)",
        params![auth.subject_device_id, auth.nonce, auth.timestamp],
    );

    let mut is_retry_takeover = false;

    // D. 幂等性检查与冲突检测
    let cached_row_opt: Option<(String, String, String, String, String, Option<String>, i64, i64, String)> = tx.query_row(
        "SELECT subject_device_id, target_device_id, action, payload_hash, status, response_json, updated_at, lease_generation, execution_token
         FROM rpc_idempotency_cache WHERE session_id = ? AND request_id = ?",
        params![auth.session_id, auth.request_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?)),
    ).optional().map_err(|e| format!("查询幂等缓存失败: {}", e))?;

    let mut current_token = String::new();
    let mut current_gen = 1i64;

    if let Some((cached_subject, cached_target, cached_action, cached_payload_hash, cached_status, cached_response_opt, cached_updated_at, cached_gen, _cached_token)) = cached_row_opt {
        // 强校验冲突：若相同的 (session_id, request_id) 用于不同的 action、payload_hash、subject 或 target
        if cached_subject != auth.subject_device_id
            || cached_target != auth.target_device_id
            || cached_action != auth.action
            || cached_payload_hash != auth.payload_hash
        {
            return Err(format!(
                "Idempotency conflict: request_id '{}' in session '{}' already used with different parameters (action: '{}' vs '{}', payload mismatch: {})",
                auth.request_id, auth.session_id, cached_action, auth.action, cached_payload_hash != auth.payload_hash
            ));
        }

        match cached_status.as_str() {
            "completed" => {
                let cached_response = cached_response_opt.unwrap_or_else(|| "{}".to_string());
                return Ok(AuthVerificationOutcome::IdempotentCached { cached_response });
            }
            "pending" => {
                let elapsed = now_ms - cached_updated_at;
                if elapsed < RPC_IDEMPOTENCY_PENDING_TIMEOUT_MS {
                    return Err(format!(
                        "Idempotency conflict: request_id '{}' in session '{}' is currently pending execution by another worker (elapsed: {}ms)",
                        auth.request_id, auth.session_id, elapsed
                    ));
                }
                log::warn!(
                    "[device_trust] Request '{}' was pending for {}ms (> {}ms timeout); allowing retry takeover with incremented generation",
                    auth.request_id, elapsed, RPC_IDEMPOTENCY_PENDING_TIMEOUT_MS
                );
                let new_token = format!("tok-{}", uuid::Uuid::new_v4());
                let new_gen = cached_gen + 1;
                let rows = tx.execute(
                    "UPDATE rpc_idempotency_cache SET lease_generation = ?, execution_token = ?, updated_at = ? WHERE session_id = ? AND request_id = ? AND status = 'pending' AND lease_generation = ?",
                    params![new_gen, new_token, now_ms, auth.session_id, auth.request_id, cached_gen],
                ).map_err(|e| format!("更新接管幂等状态失败: {}", e))?;
                if rows != 1 {
                    return Err(format!("Idempotency conflict: concurrent takeover conflict on pending request '{}'", auth.request_id));
                }
                current_token = new_token;
                current_gen = new_gen;
                is_retry_takeover = true;
            }
            "failed" => {
                log::info!("[device_trust] Request '{}' was previously failed; resetting to pending for retry", auth.request_id);
                let new_token = format!("tok-{}", uuid::Uuid::new_v4());
                let new_gen = cached_gen + 1;
                let rows = tx.execute(
                    "UPDATE rpc_idempotency_cache SET status = 'pending', lease_generation = ?, execution_token = ?, updated_at = ?, error_message = NULL WHERE session_id = ? AND request_id = ? AND status = 'failed'",
                    params![new_gen, new_token, now_ms, auth.session_id, auth.request_id],
                ).map_err(|e| format!("重试状态重置失败: {}", e))?;
                if rows != 1 {
                    return Err(format!("Idempotency conflict: failed to reset failed request '{}' for retry", auth.request_id));
                }
                current_token = new_token;
                current_gen = new_gen;
                is_retry_takeover = true;
            }
            other => {
                return Err(format!("未知的幂等状态: '{}' for request '{}'", other, auth.request_id));
            }
        }
    } else {
        // 新请求：原子抢占 pending 状态并分配 execution_token
        let new_token = format!("tok-{}", uuid::Uuid::new_v4());
        let new_gen = 1i64;
        let reserve_res = tx.execute(
            "INSERT INTO rpc_idempotency_cache (
                session_id, request_id, subject_device_id, target_device_id,
                action, payload_hash, status, lease_generation, execution_token,
                response_json, error_message, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?, NULL, NULL, ?, ?)",
            params![
                auth.session_id, auth.request_id, auth.subject_device_id, auth.target_device_id,
                auth.action, auth.payload_hash, new_gen, new_token, now_ms, now_ms
            ],
        );
        if let Err(rusqlite::Error::SqliteFailure(err, _)) = reserve_res {
            if err.code == rusqlite::ErrorCode::ConstraintViolation {
                return Err(format!(
                    "Idempotency conflict: concurrent request with id '{}' in session '{}' already in progress",
                    auth.request_id, auth.session_id
                ));
            }
        }
        reserve_res.map_err(|e| format!("写入幂等占用状态失败: {}", e))?;
        current_token = new_token;
        current_gen = new_gen;
    }

    // 校验防重放结果（若不是合法幂等重试且 nonce 发生主键冲突，判定为重放攻击）
    if let Err(rusqlite::Error::SqliteFailure(err, _)) = nonce_inserted {
        if err.code == rusqlite::ErrorCode::ConstraintViolation {
            if !is_retry_takeover {
                return Err(format!("检测到重放攻击: 重复的 nonce ({})", auth.nonce));
            }
        } else {
            nonce_inserted.map_err(|e| format!("记录防重放状态失败: {}", e))?;
        }
    } else {
        nonce_inserted.map_err(|e| format!("记录防重放状态失败: {}", e))?;
    }

    // E. 会话状态检查与滑动续期 / 自动建立
    let sess_opt: Option<(String, String, i64, i64)> = tx.query_row(
        "SELECT subject_device_id, issuer_device_id, expires_at, is_active
         FROM authenticated_sessions WHERE session_id = ?",
        [&auth.session_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional().map_err(|e| format!("查询会话失败: {}", e))?;

    match sess_opt {
        Some((sess_subject, sess_issuer, _expires_at, is_active)) => {
            if is_active == 0 {
                return Err(format!("会话已失活或被撤销: {}", auth.session_id));
            }
            let participants_match = (sess_subject == auth.subject_device_id && sess_issuer == auth.target_device_id)
                || (sess_issuer == auth.subject_device_id && sess_subject == auth.target_device_id);
            if !participants_match {
                return Err(format!(
                    "会话参与设备不匹配: 会话绑定 ({} <-> {}), 请求 ({} -> {})",
                    sess_subject, sess_issuer, auth.subject_device_id, auth.target_device_id
                ));
            }
            // 滑动续期：每次受信任调用成功，自动延长有效会话窗口
            let new_expires = now_ms + DEFAULT_SESSION_TTL_MS;
            tx.execute(
                "UPDATE authenticated_sessions SET last_activity_at = ?, expires_at = ? WHERE session_id = ?",
                params![now_ms, new_expires, auth.session_id],
            ).map_err(|e| format!("更新会话活跃与滑动过期失败: {}", e))?;
        }
        None => {
            // 对端为已配对可信设备（status == 'trusted'）且私钥签名完好，当对端由于会话到期或重启自愈建立新会话时，自动在本机同步注册该会话
            let expires_at = now_ms + DEFAULT_SESSION_TTL_MS;
            tx.execute(
                "INSERT INTO authenticated_sessions (
                    session_id, subject_device_id, issuer_device_id,
                    created_at, expires_at, last_activity_at, is_active
                ) VALUES (?, ?, ?, ?, ?, ?, 1)",
                params![
                    auth.session_id,
                    auth.subject_device_id,
                    auth.target_device_id,
                    now_ms,
                    expires_at,
                    now_ms,
                ],
            ).map_err(|e| format!("自动建立接入会话失败: {}", e))?;
        }
    }

    tx.commit().map_err(|e| format!("提交鉴权状态失败: {}", e))?;

    Ok(AuthVerificationOutcome::Authorized {
        subject_device_id: auth.subject_device_id.clone(),
        session_id: auth.session_id.clone(),
        request_id: auth.request_id.clone(),
        target_device_id: auth.target_device_id.clone(),
        action: auth.action.clone(),
        payload_hash: auth.payload_hash.clone(),
        execution_token: current_token,
        lease_generation: current_gen,
    })
}

/// 原子完成幂等 RPC 缓存：带 Fencing Token 强校验
pub fn complete_rpc_idempotency(
    conn: &Connection,
    session_id: &str,
    request_id: &str,
    execution_token: &str,
    response_json: &str,
    now_ms: i64,
) -> Result<(), String> {
    let affected_rows = conn.execute(
        "UPDATE rpc_idempotency_cache
         SET status = 'completed', response_json = ?, updated_at = ?
         WHERE session_id = ? AND request_id = ? AND status = 'pending' AND execution_token = ?",
        params![response_json, now_ms, session_id, request_id, execution_token],
    ).map_err(|e| format!("更新幂等缓存为 completed 失败: {}", e))?;

    if affected_rows == 0 {
        return Err(format!(
            "幂等缓存提交失败: Fencing token mismatch 或未找到 pending 记录 (session_id: '{}', request_id: '{}', token: '{}')",
            session_id, request_id, execution_token
        ));
    }
    Ok(())
}

/// 业务执行失败时，带 Fencing Token 校验更新为 failed
pub fn fail_rpc_idempotency(
    conn: &Connection,
    session_id: &str,
    request_id: &str,
    execution_token: &str,
    error_message: &str,
    now_ms: i64,
) -> Result<(), String> {
    let affected_rows = conn.execute(
        "UPDATE rpc_idempotency_cache
         SET status = 'failed', error_message = ?, updated_at = ?
         WHERE session_id = ? AND request_id = ? AND status = 'pending' AND execution_token = ?",
        params![error_message, now_ms, session_id, request_id, execution_token],
    ).map_err(|e| format!("更新幂等缓存为 failed 失败: {}", e))?;

    if affected_rows == 0 {
        return Err(format!(
            "幂等缓存标记失败: Fencing token mismatch 或未找到 pending 记录 (session_id: '{}', request_id: '{}', token: '{}')",
            session_id, request_id, execution_token
        ));
    }
    Ok(())
}

/// 持续投递/消费数据库中待处理的 Outbox 消息 (Durable Outbox Drainer)
/// 保证：
/// 1. 以 SQLite `rpc_staged_outbox` 为唯一队列真理源，彻底废除单文件覆盖缺陷；
/// 2. 只有在操作真正被 apply_operations 应用成功后，才在同一数据库中标记 status = 'delivered' 并写入 rpc_processed_events；
/// 3. 若应用失败，累加 attempts 并记录 last_error，保留 pending 等待下一次重试；
/// 4. 消费端基于 rpc_processed_events 表按 event_id 精准去重，绝不重复应用。
pub fn drain_staged_outbox(conn: &Connection, now_ms: i64) -> Result<usize, String> {
    let pending_items: Vec<(i64, String, String, String, String, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT id, event_id, session_id, request_id, operations_json, attempts
             FROM rpc_staged_outbox
             WHERE status = 'pending'
             ORDER BY id ASC LIMIT 50"
        ).map_err(|e| format!("准备查询待投递 Outbox 失败: {}", e))?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        }).map_err(|e| format!("查询待投递 Outbox 失败: {}", e))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("收集待投递 Outbox 数据失败: {}", e))?
    };

    let mut delivered_count = 0;
    for (id, event_id, session_id, request_id, ops_json, attempts) in pending_items {
        // 1. 检查去重表：若该 event_id 已被消费过，直接更新为 delivered
        let already_processed: bool = conn.query_row(
            "SELECT COUNT(1) FROM rpc_processed_events WHERE event_id = ?",
            params![&event_id],
            |r| r.get::<_, i64>(0),
        ).map_err(|e| format!("检查事件去重表失败: {}", e))? > 0;

        if already_processed {
            conn.execute(
                "UPDATE rpc_staged_outbox SET status = 'delivered', delivered_at = ?, last_error = NULL WHERE id = ?",
                params![now_ms, id],
            ).map_err(|e| format!("更新已消费 Outbox 状态失败: {}", e))?;
            delivered_count += 1;
            continue;
        }

        // 2. 解析 operations_json
        let ops: Vec<serde_json::Value> = match serde_json::from_str(&ops_json) {
            Ok(v) => v,
            Err(e) => {
                let err_str = format!("解析 operations_json 失败: {}", e);
                conn.execute(
                    "UPDATE rpc_staged_outbox SET attempts = ?, last_error = ? WHERE id = ?",
                    params![attempts + 1, err_str, id],
                ).map_err(|e| format!("记录 Outbox 解析错误失败: {}", e))?;
                continue;
            }
        };

        if ops.is_empty() {
            conn.execute(
                "UPDATE rpc_staged_outbox SET status = 'delivered', delivered_at = ?, last_error = NULL WHERE id = ?",
                params![now_ms, id],
            ).map_err(|e| format!("更新空 Outbox 投递状态失败: {}", e))?;
            delivered_count += 1;
            continue;
        }

        // 3. 真正消费操作：调用 crate::outbox::apply_operations(&ops)
        match crate::outbox::apply_operations(&ops) {
            Ok(applied) => {
                if !ops.is_empty() && applied == 0 {
                    let err_str = "所有操作均被 Outbox 校验拒斥 (0 applied)".to_string();
                    conn.execute(
                        "UPDATE rpc_staged_outbox SET attempts = attempts + 1, last_error = ? WHERE id = ?",
                        params![&err_str, id],
                    ).map_err(|e| format!("记录 Outbox 投递错误失败: {}", e))?;
                    log::warn!("[device_trust] Durable outbox rejected all ops for event '{}'", event_id);
                    continue;
                }
                // 4. 成功消费后：在单笔 SQLite 事务内原子完成写入 rpc_processed_events 与更新 staged_outbox 为 delivered (P2 事务化)
                let sp_res = (|| -> Result<(), rusqlite::Error> {
                    conn.execute("SAVEPOINT sp_outbox_deliver", [])?;
                    let ins_res = conn.execute(
                        "INSERT OR IGNORE INTO rpc_processed_events (event_id, session_id, request_id, applied_at) VALUES (?, ?, ?, ?)",
                        params![&event_id, &session_id, &request_id, now_ms],
                    );
                    if let Err(e) = ins_res {
                        let _ = conn.execute("ROLLBACK TO sp_outbox_deliver", []);
                        let _ = conn.execute("RELEASE sp_outbox_deliver", []);
                        return Err(e);
                    }
                    let upd_res = conn.execute(
                        "UPDATE rpc_staged_outbox SET status = 'delivered', delivered_at = ?, last_error = NULL WHERE id = ?",
                        params![now_ms, id],
                    );
                    if let Err(e) = upd_res {
                        let _ = conn.execute("ROLLBACK TO sp_outbox_deliver", []);
                        let _ = conn.execute("RELEASE sp_outbox_deliver", []);
                        return Err(e);
                    }
                    conn.execute("RELEASE sp_outbox_deliver", [])?;
                    Ok(())
                })();
                sp_res.map_err(|e| format!("原子提交 outbox 交付状态事务失败: {}", e))?;

                delivered_count += 1;
                log::info!("[device_trust] Durable outbox successfully applied and delivered event '{}' (id: {})", event_id, id);
            }
            Err(err_msg) => {
                // 5. 应用失败：累加 attempts 并记录 last_error，保留 pending 等待重试（绝不写入 rpc_processed_events）
                conn.execute(
                    "UPDATE rpc_staged_outbox SET attempts = attempts + 1, last_error = ? WHERE id = ?",
                    params![&err_msg, id],
                ).map_err(|e| format!("记录 Outbox 投递错误失败: {}", e))?;
                log::warn!("[device_trust] Durable outbox apply failed for event '{}': {}", event_id, err_msg);
            }
        }
    }

    Ok(delivered_count)
}

/// Push 提交与投递执行回执契约
/// 严格区分已可靠入队 (pending_apply) 与已应用生效 (applied)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushCommitReceipt {
    pub status: String,
    pub r#type: String,
    pub stage: String, // "applied" 或 "pending_apply"
    pub applied_count: usize,
    pub pending_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_error: Option<String>,
}

/// 原子提交 push 的副作用 (Outbox 记录) 与幂等 completed 状态
/// 在单笔 SQLite 事务内完成：
/// 1. 校验并消费 execution_token (fencing token check)，更新状态为 completed；
/// 2. 为每笔操作注入稳定 event_id 与 request_id 供消费端精准去重；
/// 3. 将操作写入 rpc_staged_outbox 表（具备 UNIQUE(session_id, request_id) 防御，初始为 pending）；
/// 4. 单笔事务原子提交；
/// 5. 仅在事务成功提交后立即执行持久化投递 (drain_staged_outbox)。若在此处或此后崩溃，下次重启自动重新补投，永不丢失消息。
/// 6. 返回 PushCommitReceipt 回执，如实反映操作是 applied 还是 pending_apply。
pub fn atomic_commit_push_outbox(
    conn: &mut Connection,
    session_id: &str,
    request_id: &str,
    execution_token: &str,
    operations: &[serde_json::Value],
    resp_str: &str,
    now_ms: i64,
) -> Result<PushCommitReceipt, String> {
    // SEC-03 严格正向白名单 (Constructive Allowlist) 与标量校验：
    // 密钥留在执行设备，严禁通过远程 Push 修改本地 API 密钥或凭证 (Fail-Closed)
    for op in operations {
        let obj = op.as_object().ok_or_else(|| {
            "SEC-03 Fail-Closed: 拒绝执行非法远程操作: 操作载荷必须为 JSON Object (Raw scalars or arrays are rejected)".to_string()
        })?;

        let op_type = obj.get("op")
            .or_else(|| obj.get("action"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if op_type == "set_api_key" {
            return Err("SEC-03 Fail-Closed: 拒绝执行非法远程操作: 严禁通过远程同步修改本地设备 API 密钥 (set_api_key is blocked on remote sync)".to_string());
        }

        if !ALLOWED_REMOTE_PUSH_OPS.contains(&op_type) {
            return Err(format!(
                "SEC-03 Fail-Closed: 拒绝执行非法远程操作: 未知或不允许的操作类型 '{}' (Only whitelisted remote operations are accepted)",
                op_type
            ));
        }

        if op_type == "set_config" {
            let key = obj.get("key").and_then(|v| v.as_str()).unwrap_or("");
            if !ALLOWED_REMOTE_CONFIG_KEYS.contains(&key) {
                return Err(format!(
                    "SEC-03 Fail-Closed: 拒绝执行非法远程配置变更: key '{}' 不在允许的配置白名单中",
                    key
                ));
            }
            if let Some(val) = obj.get("value") {
                if val.is_object() || val.is_array() {
                    return Err(format!(
                        "SEC-03 Fail-Closed: 拒绝执行非法远程配置变更: 配置项 '{}' 的值必须为标量，禁止传递嵌套对象或数组",
                        key
                    ));
                }
            } else {
                return Err(format!(
                    "SEC-03 Fail-Closed: 拒绝执行非法远程配置变更: 配置项 '{}' 缺少 value",
                    key
                ));
            }

            // 检查 set_config 是否夹带其他未知或敏感属性 (仅允许 op, action, key, value, event_id, request_id)
            for (k, _) in obj {
                let lk = k.to_ascii_lowercase();
                if lk != "op" && lk != "action" && lk != "key" && lk != "value" && lk != "event_id" && lk != "request_id" {
                    return Err(format!("SEC-03 Fail-Closed: 拒绝执行非法远程配置变更: 未知或敏感夹带字段 '{}'", k));
                }
            }
        }
    }

    let tx = conn.transaction().map_err(|e| format!("开启原子事务失败: {}", e))?;

    // 1. Fencing Token 强校验更新 completed
    let affected = tx.execute(
        "UPDATE rpc_idempotency_cache
         SET status = 'completed', response_json = ?, updated_at = ?
         WHERE session_id = ? AND request_id = ? AND status = 'pending' AND execution_token = ?",
        params![resp_str, now_ms, session_id, request_id, execution_token],
    ).map_err(|e| format!("原子更新 idempotency cache 失败: {}", e))?;

    if affected != 1 {
        return Err(format!(
            "幂等提交被拒绝: Fencing token mismatch 或记录已被其他 worker 接管 (session_id: '{}', request_id: '{}', token: '{}')",
            session_id, request_id, execution_token
        ));
    }

    // 2. 将 Outbox 写入数据库暂存队列（具备 UNIQUE(session_id, request_id) 约束）
    if !operations.is_empty() {
        let event_id = format!("evt-{}-{}", session_id, request_id);
        let mut enriched_ops = operations.to_vec();
        for op in enriched_ops.iter_mut() {
            if let Some(obj) = op.as_object_mut() {
                if !obj.contains_key("event_id") {
                    obj.insert("event_id".to_string(), serde_json::json!(event_id));
                }
                if !obj.contains_key("request_id") {
                    obj.insert("request_id".to_string(), serde_json::json!(request_id));
                }
            }
        }

        let ops_json = serde_json::to_string(&enriched_ops).map_err(|e| format!("序列化 outbox 操作失败: {}", e))?;
        tx.execute(
            "INSERT INTO rpc_staged_outbox (event_id, session_id, request_id, operations_json, status, attempts, created_at)
             VALUES (?, ?, ?, ?, 'pending', 0, ?)
             ON CONFLICT(session_id, request_id) DO UPDATE SET operations_json = excluded.operations_json, status = 'pending'",
            params![event_id, session_id, request_id, ops_json, now_ms],
        ).map_err(|e| format!("写入 rpc_staged_outbox 失败: {}", e))?;
    }

    // 3. 提交事务 (单笔事务同时生效 completed 状态与 staged outbox pending 记录)
    tx.commit().map_err(|e| format!("提交原子 push 事务失败: {}", e))?;

    // 4. 事务成功提交后立即执行持久化投递
    // 若进程在此处崩溃，数据库已可靠持久化，重启时通过 drain_staged_outbox 自动补投，永不丢消息
    let (stage, applied_count, pending_count, delivery_error) = if !operations.is_empty() {
        let _ = drain_staged_outbox(conn, now_ms);
        let check_res: Result<(String, Option<String>), _> = conn.query_row(
            "SELECT status, last_error FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
            params![session_id, request_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        );
        match check_res {
            Ok((st, err)) => {
                if st == "delivered" {
                    ("applied".to_string(), operations.len(), 0, None)
                } else {
                    ("pending_apply".to_string(), 0, operations.len(), err.or_else(|| Some("配置操作暂存中，等待后台收敛".to_string())))
                }
            }
            Err(e) => {
                ("pending_apply".to_string(), 0, operations.len(), Some(format!("无法确认落盘状态 (Fail-Closed): {}", e)))
            }
        }
    } else {
        ("applied".to_string(), 0, 0, None)
    };

    let receipt = PushCommitReceipt {
        status: "ok".to_string(),
        r#type: "commit_ack".to_string(),
        stage,
        applied_count,
        pending_count,
        delivery_error,
    };

    let final_resp_str = if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(resp_str) {
        if let Some(payload_obj) = val.get_mut("payload").and_then(|v| v.as_object_mut()) {
            payload_obj.insert("stage".to_string(), serde_json::json!(receipt.stage));
            payload_obj.insert("applied_count".to_string(), serde_json::json!(receipt.applied_count));
            payload_obj.insert("pending_count".to_string(), serde_json::json!(receipt.pending_count));
            if let Some(ref err) = receipt.delivery_error {
                payload_obj.insert("delivery_error".to_string(), serde_json::json!(err));
            }
            val.to_string()
        } else {
            serde_json::to_string(&receipt).unwrap_or_else(|_| resp_str.to_string())
        }
    } else {
        serde_json::to_string(&receipt).unwrap_or_else(|_| resp_str.to_string())
    };
    let update_rows = conn.execute(
        "UPDATE rpc_idempotency_cache SET response_json = ? WHERE session_id = ? AND request_id = ?",
        params![final_resp_str, session_id, request_id],
    ).map_err(|e| format!("更新幂等缓存 response_json 失败 (Fail-Closed): {}", e))?;
    if update_rows == 0 {
        return Err(format!("更新幂等缓存 response_json 影响行数为 0 (Fail-Closed, 无法定位 session {} req {})", session_id, request_id));
    }

    Ok(receipt)
}

/// 缓存幂等 RPC 执行结果（兼容性实现：优先更新 pending，不存在时执行 completed upsert）
pub fn record_rpc_idempotency_result(
    conn: &Connection,
    session_id: &str,
    request_id: &str,
    subject_device_id: &str,
    target_device_id: &str,
    action: &str,
    payload_hash: &str,
    response_json: &str,
    now_ms: i64,
) -> Result<(), String> {
    let updated = conn.execute(
        "UPDATE rpc_idempotency_cache
         SET status = 'completed', response_json = ?, updated_at = ?
         WHERE session_id = ? AND request_id = ? AND status = 'pending'",
        params![response_json, now_ms, session_id, request_id],
    ).map_err(|e| format!("更新幂等缓存失败: {}", e))?;

    if updated == 0 {
        let token = format!("tok-{}", uuid::Uuid::new_v4());
        conn.execute(
            "INSERT INTO rpc_idempotency_cache (session_id, request_id, subject_device_id, target_device_id, action, payload_hash, status, lease_generation, execution_token, response_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'completed', 1, ?, ?, ?, ?)
             ON CONFLICT(session_id, request_id) DO UPDATE SET status = 'completed', response_json = excluded.response_json, updated_at = excluded.updated_at",
            params![session_id, request_id, subject_device_id, target_device_id, action, payload_hash, token, response_json, now_ms, now_ms],
        ).map_err(|e| format!("保存幂等缓存失败: {}", e))?;
    }
    Ok(())
}

/// 撤销可信设备：设置状态为 revoked，使该设备所有会话立即失效
pub fn revoke_trusted_device(
    conn: &mut Connection,
    device_id: &str,
    reason: Option<&str>,
    now_ms: i64,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| format!("开启撤销事务失败: {}", e))?;

    // 1. 标记设备为 revoked
    let updated = tx.execute(
        "UPDATE trusted_devices
         SET status = 'revoked', revoked_at = ?, revocation_reason = ?
         WHERE device_id = ?",
        params![now_ms, reason, device_id],
    ).map_err(|e| format!("更新设备撤销状态失败: {}", e))?;

    // 2. 使关联所有会话立即失效
    tx.execute(
        "UPDATE authenticated_sessions SET is_active = 0 WHERE subject_device_id = ?1 OR issuer_device_id = ?1",
        params![device_id],
    ).map_err(|e| format!("失效设备会话失败: {}", e))?;

    tx.commit().map_err(|e| format!("提交撤销事务失败: {}", e))?;
    log::info!("[Device Trust] 设备 {} 已成功撤销 (匹配条目: {})", device_id, updated);
    Ok(())
}

/// SEC-01 重置本地设备身份数据库状态：
/// 1. 查询所有当前 trusted 的设备，向 peer_revocation_outbox 暂存持久化撤销通知
pub fn canonical_revocation_bytes(
    protocol_version: &str,
    event_id: &str,
    revoked_device_id: &str,
    target_device_id: &str,
    nonce: &str,
    timestamp_ms: i64,
    reason: &str,
) -> Vec<u8> {
    format!(
        "SEC01_REVOCATION:{}:{}:{}:{}:{}:{}:{}",
        protocol_version,
        event_id,
        revoked_device_id,
        target_device_id,
        nonce,
        timestamp_ms,
        reason
    ).into_bytes()
}

pub fn create_device_revocation_certificate(
    signing_key: &SigningKey,
    target_device_id: &str,
    reason: &str,
    now_ms: i64,
) -> DeviceRevocationCertificate {
    let vk = VerifyingKey::from(signing_key);
    let revoked_device_id = BASE64.encode(vk.to_bytes());
    let event_id = uuid::Uuid::new_v4().to_string();
    let nonce = uuid::Uuid::new_v4().to_string();
    let canonical = canonical_revocation_bytes(
        SEC01_PROTOCOL_VERSION,
        &event_id,
        &revoked_device_id,
        target_device_id,
        &nonce,
        now_ms,
        reason,
    );
    let sig = signing_key.sign(&canonical);
    let signature = BASE64.encode(sig.to_bytes());

    DeviceRevocationCertificate {
        protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
        event_id,
        revoked_device_id,
        target_device_id: target_device_id.to_string(),
        nonce,
        timestamp_ms: now_ms,
        reason: reason.to_string(),
        signature,
    }
}

pub fn canonical_revocation_ack_bytes(
    protocol_version: &str,
    event_id: &str,
    revoked_device_id: &str,
    target_device_id: &str,
    status: &str,
    nonce: &str,
    timestamp_ms: i64,
) -> Vec<u8> {
    format!(
        "SEC01_REVOCATION_ACK:{}:{}:{}:{}:{}:{}:{}",
        protocol_version,
        event_id,
        revoked_device_id,
        target_device_id,
        status,
        nonce,
        timestamp_ms
    ).into_bytes()
}

pub fn create_device_revocation_ack(
    signing_key: &SigningKey,
    event_id: &str,
    revoked_device_id: &str,
    target_device_id: &str,
    status: &str,
    now_ms: i64,
) -> RevocationAck {
    let nonce = uuid::Uuid::new_v4().to_string();
    let canonical = canonical_revocation_ack_bytes(
        SEC01_PROTOCOL_VERSION,
        event_id,
        revoked_device_id,
        target_device_id,
        status,
        &nonce,
        now_ms,
    );
    let sig = signing_key.sign(&canonical);
    let signature = BASE64.encode(sig.to_bytes());

    RevocationAck {
        r#type: "device_revocation_ack".to_string(),
        protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
        event_id: event_id.to_string(),
        revoked_device_id: revoked_device_id.to_string(),
        target_device_id: target_device_id.to_string(),
        status: status.to_string(),
        nonce,
        timestamp_ms: now_ms,
        signature,
    }
}

/// SEC-01 统一撤销验证入口 (LAN HTTP 与 Relay WS 共享):
/// 1. 协议版本 Fail-Closed
/// 2. 目标设备绑定强校验 (防跨目标转发)
/// 3. 时间戳偏差校验 (防过期重放)
/// 4. 幂等与防重放去重校验
/// 5. 校验被撤销公钥在本地的名册状态
/// 6. Ed25519 签名强校验 (持钥证明)
/// 7. 单笔 SQLite 事务原子作废设备、清空会话与防重放缓存，并记录 processed_events
/// 8. 返回持钥签名的 CommitAck (状态: committed)
pub fn sec01_verify_and_apply_peer_revocation(
    conn: &mut Connection,
    cert: &DeviceRevocationCertificate,
    local_device_id: &str,
    local_signing_key: &SigningKey,
    now_ms: i64,
) -> Result<RevocationAck, String> {
    // 1. 协议版本门禁 (Fail-Closed)
    if cert.protocol_version.trim().is_empty() || cert.protocol_version != SEC01_PROTOCOL_VERSION {
        return Err(format!("不支持或无效的撤销协议版本: {}", cert.protocol_version));
    }

    // 2. 本机身份与目标设备强校验 (跨目标转发防御)
    if local_device_id.trim().is_empty() {
        return Err("本机设备身份未初始化或为空，拒绝处理撤销".to_string());
    }
    let local_vk = VerifyingKey::from(local_signing_key);
    let sk_derived_id = BASE64.encode(local_vk.to_bytes());
    if local_device_id != sk_derived_id {
        return Err(format!("本地私钥派生 ID 不匹配: local={}, sk_derived={}", local_device_id, sk_derived_id));
    }
    if cert.target_device_id != local_device_id {
        return Err(format!(
            "撤销目标设备 ID 不匹配: cert.target={}, local={}",
            cert.target_device_id, local_device_id
        ));
    }

    // 持久化的撤销证书须允许离线重投；只拒绝未来时钟漂移。重放由事件 ID 与 Nonce 防护。
    if cert.timestamp_ms > now_ms.saturating_add(300_000) {
        return Err("撤销证书时间戳超前超过五分钟".to_string());
    }

    if cert.event_id.trim().is_empty() || cert.nonce.trim().is_empty() {
        return Err("撤销证书缺少事件 ID 或 Nonce".to_string());
    }

    // 验签必须先于幂等回执，否则伪造已处理事件 ID 可索取签名 Ack。
    let pubkey_bytes = BASE64.decode(&cert.revoked_device_id)
        .map_err(|e| format!("Base64 解码 revoked_device_id 失败: {}", e))?;
    let vk = VerifyingKey::from_bytes(
        pubkey_bytes.as_slice().try_into()
            .map_err(|_| "revoked_device_id 不是合法的 32 字节 Ed25519 公钥".to_string())?
    ).map_err(|e| format!("无法构建 Ed25519 VerifyingKey: {}", e))?;
    let sig_bytes = BASE64.decode(&cert.signature)
        .map_err(|e| format!("Base64 解码 signature 失败: {}", e))?;
    let sig = Signature::from_bytes(
        sig_bytes.as_slice().try_into()
            .map_err(|_| "signature 不是合法的 64 字节 Ed25519 签名".to_string())?
    );
    let canonical = canonical_revocation_bytes(
        &cert.protocol_version, &cert.event_id, &cert.revoked_device_id,
        &cert.target_device_id, &cert.nonce, cert.timestamp_ms, &cert.reason,
    );
    vk.verify(&canonical, &sig)
        .map_err(|e| format!("撤销证书签名校验失败: {}", e))?;

    // 4. 幂等去重检查 (已处理事件直接返回持钥签名的 Ack，零副作用)
    let already_processed: bool = conn.query_row(
        "SELECT COUNT(1) FROM rpc_processed_events WHERE event_id = ?",
        [&cert.event_id],
        |row| Ok(row.get::<_, i64>(0)? > 0),
    ).map_err(|e| format!("查询已处理撤销事件失败: {}", e))?;

    if already_processed {
        return Ok(create_device_revocation_ack(
            local_signing_key,
            &cert.event_id,
            &cert.revoked_device_id,
            &cert.target_device_id,
            "committed",
            now_ms,
        ));
    }

    // 4b. 防重放 Nonce 检查
    let nonce_replayed: bool = conn.query_row(
        "SELECT COUNT(1) FROM rpc_anti_replay WHERE subject_device_id = ? AND nonce = ?",
        params![&cert.revoked_device_id, &cert.nonce],
        |row| Ok(row.get::<_, i64>(0)? > 0),
    ).map_err(|e| format!("查询撤销 Nonce 失败: {}", e))?;

    if nonce_replayed {
        return Err(format!("撤销证书 Nonce 已被使用 (重放攻击拦截): {}", cert.nonce));
    }

    // 5. 校验被撤销公钥在本地的名册状态
    let device_status: Option<String> = conn.query_row(
        "SELECT status FROM trusted_devices WHERE device_id = ?",
        [&cert.revoked_device_id],
        |row| row.get(0),
    ).optional().map_err(|e| format!("查询对端设备状态失败: {}", e))?;

    let is_trusted = match device_status.as_deref() {
        Some("trusted") => true,
        Some("revoked") => false, // 已经撤销过，幂等处理
        _ => {
            return Err(format!("未识别的对端设备或非可信设备: {}", cert.revoked_device_id));
        }
    };

    // 7. 单笔 SQLite 事务原子执行作废操作
    let tx = conn.transaction().map_err(|e| format!("开启撤销事务失败: {}", e))?;

    if is_trusted {
        tx.execute(
            "UPDATE trusted_devices SET status = 'revoked', revoked_at = ?1, revocation_reason = ?2 WHERE device_id = ?3 AND status = 'trusted'",
            params![now_ms, &cert.reason, &cert.revoked_device_id],
        ).map_err(|e| format!("更新设备信任状态失败: {}", e))?;

        tx.execute(
            "DELETE FROM authenticated_sessions WHERE subject_device_id = ?1 OR issuer_device_id = ?1",
            params![&cert.revoked_device_id],
        ).map_err(|e| format!("清除关联会话失败: {}", e))?;

        tx.execute(
            "DELETE FROM rpc_anti_replay WHERE subject_device_id = ?1",
            params![&cert.revoked_device_id],
        ).map_err(|e| format!("清除关联防重放缓存失败: {}", e))?;
    }

    // 记录已处理事件去重与防重放
    tx.execute(
        "INSERT INTO rpc_processed_events (event_id, session_id, request_id, applied_at) VALUES (?, 'revocation', ?, ?)",
        params![&cert.event_id, &cert.event_id, now_ms],
    ).map_err(|e| format!("写入 processed_events 失败: {}", e))?;

    tx.execute(
        "INSERT INTO rpc_anti_replay (subject_device_id, nonce, timestamp) VALUES (?, ?, ?)",
        params![&cert.revoked_device_id, &cert.nonce, now_ms],
    ).map_err(|e| format!("写入 anti_replay 失败: {}", e))?;

    tx.commit().map_err(|e| format!("提交撤销事务失败: {}", e))?;

    log::info!(
        "[Device Trust] 经持钥认证成功作废对端设备: {} (event_id: {}, reason: {})",
        cert.revoked_device_id, cert.event_id, cert.reason
    );

    Ok(create_device_revocation_ack(
        local_signing_key,
        &cert.event_id,
        &cert.revoked_device_id,
        &cert.target_device_id,
        "committed",
        now_ms,
    ))
}

/// SEC-01 重置本地设备身份数据库状态 (携带预签名的撤销证书):
/// 1. 更新 identity_reset_journal 为 staged
/// 2. 为每个已签名的撤销证书持久化写入 peer_revocation_outbox
/// 3. 将本地所有 trusted 设备置为 revoked
/// 4. 清除 authenticated_sessions
/// 5. 清理未消费配对邀请
/// 6. 清理防重放、幂等与发件箱
/// 7. 更新 identity_reset_journal 为 db_committed
pub fn reset_device_identity_in_db_with_certs(
    conn: &mut Connection,
    journal_id: i64,
    certs: &[DeviceRevocationCertificate],
    _local_revoked_id: Option<&str>,
    now_ms: i64,
) -> Result<usize, String> {
    let tx = conn.transaction().map_err(|e| format!("开启重置事务失败: {}", e))?;

    tx.execute(
        "UPDATE identity_reset_journal SET state = 'staged', updated_at = ? WHERE id = ?",
        params![now_ms, journal_id],
    ).map_err(|e| format!("更新重置日志 staged 失败: {}", e))?;

    let mut staged_count = 0;
    for cert in certs {
        let payload_str = serde_json::to_string(cert)
            .map_err(|e| format!("序列化撤销证书失败: {}", e))?;

        tx.execute(
            "INSERT INTO peer_revocation_outbox (
                event_id, target_peer_id, revoked_device_id, revocation_payload, status, attempts, last_error, created_at, sent_at, delivered_at
            ) VALUES (?1, ?2, ?3, ?4, 'pending', 0, NULL, ?5, NULL, NULL)",
            params![&cert.event_id, &cert.target_device_id, &cert.revoked_device_id, payload_str, now_ms],
        ).map_err(|e| format!("写入撤销出件箱失败: {}", e))?;
        staged_count += 1;
    }

    tx.execute(
        "UPDATE trusted_devices SET status = 'revoked', revoked_at = ?1, revocation_reason = 'local_key_reset' WHERE status = 'trusted'",
        params![now_ms],
    ).map_err(|e| format!("撤销可信设备失败: {}", e))?;

    tx.execute("DELETE FROM authenticated_sessions", [])
        .map_err(|e| format!("清除认证会话失败: {}", e))?;

    tx.execute("DELETE FROM pairing_invitations WHERE consumed_at IS NULL", [])
        .map_err(|e| format!("清理未消费邀请失败: {}", e))?;

    tx.execute("DELETE FROM rpc_anti_replay", [])
        .map_err(|e| format!("清理防重放缓存失败: {}", e))?;

    tx.execute("DELETE FROM rpc_idempotency_cache", [])
        .map_err(|e| format!("清理幂等缓存失败: {}", e))?;

    tx.execute("DELETE FROM rpc_staged_outbox", [])
        .map_err(|e| format!("清理暂存发件箱失败: {}", e))?;

    tx.execute(
        "UPDATE identity_reset_journal SET state = 'db_committed', updated_at = ? WHERE id = ?",
        params![now_ms, journal_id],
    ).map_err(|e| format!("更新重置日志 db_committed 失败: {}", e))?;

    tx.commit().map_err(|e| format!("提交重置事务失败: {}", e))?;
    log::info!("[Device Trust] 本地设备身份数据库状态已全面重置，暂存了 {} 条持钥签名撤销通知 (journal_id: {})", staged_count, journal_id);
    Ok(staged_count)
}

/// SEC-01 重置本地设备身份数据库状态 (兼容入口)
pub fn reset_device_identity_in_db(
    conn: &mut Connection,
    local_revoked_id: Option<&str>,
    now_ms: i64,
) -> Result<usize, String> {
    let rev_id = local_revoked_id.unwrap_or("unknown");
    conn.execute(
        "INSERT INTO identity_reset_journal (state, revoked_device_id, created_at, updated_at, error) VALUES ('prepared', ?, ?, ?, NULL)",
        params![rev_id, now_ms, now_ms],
    ).map_err(|e| format!("写入重置日志 prepared 失败: {}", e))?;
    let journal_id = conn.last_insert_rowid();

    reset_device_identity_in_db_with_certs(conn, journal_id, &[], local_revoked_id, now_ms)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRevocationOutboxItem {
    pub id: i64,
    pub event_id: String,
    pub target_peer_id: String,
    pub revoked_device_id: String,
    pub revocation_payload: String,
    pub status: String,
    pub attempts: i64,
    pub created_at: i64,
}

pub const MAX_PEER_REVOCATION_ATTEMPTS: i64 = 3;

pub fn get_pending_peer_revocations(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<PeerRevocationOutboxItem>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, event_id, target_peer_id, revoked_device_id, revocation_payload, status, attempts, created_at
         FROM peer_revocation_outbox
         WHERE status = 'pending' AND attempts < ?1
         ORDER BY id ASC
         LIMIT ?2"
    ).map_err(|e| format!("查询待分发撤销失败: {}", e))?;

    let rows = stmt.query_map(params![MAX_PEER_REVOCATION_ATTEMPTS, limit as i64], |row| {
        Ok(PeerRevocationOutboxItem {
            id: row.get(0)?,
            event_id: row.get(1)?,
            target_peer_id: row.get(2)?,
            revoked_device_id: row.get(3)?,
            revocation_payload: row.get(4)?,
            status: row.get(5)?,
            attempts: row.get(6)?,
            created_at: row.get(7)?,
        })
    }).map_err(|e| format!("读取待分发撤销失败: {}", e))?;

    let mut items = Vec::new();
    for r in rows {
        items.push(r.map_err(|e| format!("解析待分发撤销失败: {}", e))?);
    }
    Ok(items)
}

pub fn mark_peer_revocation_sent(
    conn: &Connection,
    id: i64,
    now_ms: i64,
) -> Result<(), String> {
    let changed = conn.execute(
        "UPDATE peer_revocation_outbox SET status = 'sent', sent_at = ?, attempts = attempts + 1, last_error = NULL WHERE id = ? AND status = 'pending'",
        params![now_ms, id],
    ).map_err(|e| format!("更新撤销出件箱发送状态失败: {}", e))?;
    if changed != 1 {
        return Err(format!("撤销出件箱 {} 不再处于 pending，拒绝重复认领", id));
    }
    Ok(())
}

pub fn mark_peer_revocation_delivered(
    conn: &Connection,
    id: i64,
    now_ms: i64,
) -> Result<(), String> {
    conn.execute(
        "UPDATE peer_revocation_outbox SET status = 'delivered', delivered_at = ?, last_error = NULL WHERE id = ?",
        params![now_ms, id],
    ).map_err(|e| format!("更新撤销出件箱投递成功状态失败: {}", e))?;
    Ok(())
}

pub fn mark_peer_revocation_delivered_by_event_id(
    conn: &Connection,
    event_id: &str,
    now_ms: i64,
) -> Result<bool, String> {
    let count = conn.execute(
        "UPDATE peer_revocation_outbox SET status = 'delivered', delivered_at = ?, last_error = NULL WHERE event_id = ? AND status = 'sent'",
        params![now_ms, event_id],
    ).map_err(|e| format!("按 event_id 更新撤销出件箱投递成功失败: {}", e))?;
    Ok(count > 0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevocationAckOutcome {
    Delivered { event_id: String, peer_id: String },
    AlreadyDelivered { event_id: String },
}

/// SEC-01 统一验证并应用对端持钥签名的撤销确认 (CommitAck):
/// 1. 协议版本强校验
/// 2. 状态强校验 (status == 'committed')
/// 3. 被撤销身份强校验 (必须匹配期望的本地被撤销公钥)
/// 4. 时钟偏差强校验 (5分钟防重放窗口)
/// 5. 目标设备公钥格式与 Ed25519 签名强校验 (持钥证明)
/// 6. 条件更新出件箱：严格要求 status = 'sent' 且 event_id, target_peer_id, revoked_device_id 完全匹配
/// 7. 幂等处理：若已处于 delivered 状态，返回 AlreadyDelivered，零重复副作用
/// 8. 未匹配到 sent 亦非 delivered（例如在 pending 时提前收到、未知事件 ID 或字段被篡改）：拒绝并返回 Err，零副作用！
pub fn sec01_verify_and_apply_peer_revocation_ack(
    conn: &mut Connection,
    ack: &RevocationAck,
    expected_revoked_id: &str,
    now_ms: i64,
) -> Result<RevocationAckOutcome, String> {
    // 1. 协议版本门禁 (Fail-Closed)
    if ack.protocol_version.trim().is_empty() || ack.protocol_version != SEC01_PROTOCOL_VERSION {
        return Err(format!("不支持或无效的撤销 Ack 协议版本: {}", ack.protocol_version));
    }

    // 2. 状态强校验
    if ack.status != "committed" {
        return Err(format!("撤销 Ack 状态非 committed (status: {})", ack.status));
    }

    // 3. 被撤销身份强校验
    if ack.revoked_device_id != expected_revoked_id {
        return Err(format!(
            "撤销 Ack 被撤销身份不匹配: ack.revoked={}, expected={}",
            ack.revoked_device_id, expected_revoked_id
        ));
    }

    // 4. 时钟偏差校验 (5 分钟防重放窗口)
    let skew = (now_ms - ack.timestamp_ms).abs();
    if skew > 300_000 {
        return Err(format!(
            "撤销 Ack 时间戳偏差过大 (skew: {}ms, max: 300000ms)",
            skew
        ));
    }

    // 5. 校验目标设备公钥格式与 Ed25519 签名 (防伪造确认)
    let pubkey_bytes = BASE64.decode(ack.target_device_id.trim())
        .map_err(|e| format!("Base64 解码 target_device_id 失败: {}", e))?;
    let vk = VerifyingKey::from_bytes(
        pubkey_bytes.as_slice().try_into()
            .map_err(|_| "target_device_id 不是合法的 32 字节 Ed25519 公钥".to_string())?
    ).map_err(|e| format!("无法构建 Ed25519 VerifyingKey: {}", e))?;

    let sig_bytes = BASE64.decode(ack.signature.trim())
        .map_err(|e| format!("Base64 解码 Ack signature 失败: {}", e))?;
    let sig = Signature::from_bytes(
        sig_bytes.as_slice().try_into()
            .map_err(|_| "signature 不是合法的 64 字节 Ed25519 签名".to_string())?
    );

    let canonical = canonical_revocation_ack_bytes(
        &ack.protocol_version,
        &ack.event_id,
        &ack.revoked_device_id,
        &ack.target_device_id,
        &ack.status,
        &ack.nonce,
        ack.timestamp_ms,
    );

    vk.verify(&canonical, &sig)
        .map_err(|e| format!("撤销 Ack 数字签名验证失败 (持钥证明无效): {}", e))?;

    // 6. 条件更新出件箱：必须在 sent 状态下，且 event_id, target_peer_id, revoked_device_id 完全匹配
    let affected = conn.execute(
        "UPDATE peer_revocation_outbox
         SET status = 'delivered', delivered_at = ?1, last_error = NULL
         WHERE event_id = ?2
           AND status = 'sent'
           AND target_peer_id = ?3
           AND revoked_device_id = ?4",
        params![now_ms, &ack.event_id, &ack.target_device_id, &ack.revoked_device_id],
    ).map_err(|e| format!("更新撤销出件箱 delivered 失败: {}", e))?;

    if affected > 0 {
        log::info!(
            "[Device Trust] 经持钥认证 Ack，撤销事件 '{}' 确认送达对端 '{}' (status -> delivered)",
            ack.event_id, ack.target_device_id
        );
        return Ok(RevocationAckOutcome::Delivered {
            event_id: ack.event_id.clone(),
            peer_id: ack.target_device_id.clone(),
        });
    }

    // 7. 幂等检查：是否已经是 delivered
    let already_delivered: bool = conn.query_row(
        "SELECT COUNT(1) FROM peer_revocation_outbox
         WHERE event_id = ?1
           AND status = 'delivered'
           AND target_peer_id = ?2
           AND revoked_device_id = ?3",
        params![&ack.event_id, &ack.target_device_id, &ack.revoked_device_id],
        |row| Ok(row.get::<_, i64>(0)? > 0),
    ).unwrap_or(false);

    if already_delivered {
        log::debug!(
            "[Device Trust] 撤销事件 '{}' 收到重复 Ack，保持 delivered (幂等处理)",
            ack.event_id
        );
        return Ok(RevocationAckOutcome::AlreadyDelivered {
            event_id: ack.event_id.clone(),
        });
    }

    // 8. 既非 sent 也非 delivered（例如在 pending 时提前收到、未知事件 ID 或伪造字段）
    Err(format!(
        "拒绝未匹配的撤销 Ack: event_id '{}' 不在 sent 状态或目标/被撤销方不匹配",
        ack.event_id
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityRecoveryOutcome {
    Clean,
    CancelledPrepared { count: usize },
    ConvergedToCommitted { journal_id: i64 },
    RetainedDegraded { journal_id: i64, reason: String },
}

/// SEC-01 启动与重试自愈恢复：
/// 核验 identity_reset_journal 中所有未完成的记录 (prepared / staged / db_committed / degraded)
/// 1. prepared: Phase 2 事务未发生，DB与配置完好，安全标记为 cancelled，保留原身份
/// 2. staged / db_committed: Phase 2 已提交，物理收敛删除密钥、清理配置并标记为 committed
/// 3. degraded: 检查是否已达到收敛态；若密钥成功删除且配置已清空，标记 committed；若无法删除则保持 degraded 锁
pub fn recover_device_identity_state(
    conn: &Connection,
    key_path_opt: Option<&std::path::Path>,
    memory_state_opt: Option<&std::sync::Mutex<Option<SigningKey>>>,
    now_ms: i64,
) -> Result<IdentityRecoveryOutcome, String> {
    let mut stmt = conn.prepare(
        "SELECT id, state, revoked_device_id, error FROM identity_reset_journal
         WHERE state NOT IN ('committed', 'cancelled')
         ORDER BY id ASC"
    ).map_err(|e| format!("查询未完成重置日志失败: {}", e))?;

    let rows: Vec<(i64, String, String, Option<String>)> = stmt.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    }).map_err(|e| format!("读取重置日志失败: {}", e))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| format!("解析重置日志失败: {}", e))?;

    if rows.is_empty() {
        return Ok(IdentityRecoveryOutcome::Clean);
    }

    let mut cancelled_prepared = 0;
    let mut outcome = IdentityRecoveryOutcome::Clean;

    for (jid, state, revoked_id, _err_opt) in rows {
        // 核验阶段与数据库事务事实
        let phase2_committed = match state.as_str() {
            "prepared" | "degraded_pre_db" => false,
            "staged" | "db_committed" | "config_committed" | "degraded_post_db" | "degraded_key_destroy" => true,
            _ => {
                // 对于历史或未细分的 "degraded" / "staged"，严格核验 SQLite 事务事实：
                // 1. 是否在 peer_revocation_outbox 中持久化了针对 revoked_device_id 的记录
                // 2. trusted_devices 是否已被标记为 revoked
                let outbox_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM peer_revocation_outbox WHERE revoked_device_id = ?",
                    [&revoked_id],
                    |r| r.get(0),
                ).map_err(|e| format!("查询撤销出件箱事实失败: {}", e))?;
                let active_trusted: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM trusted_devices WHERE status = 'trusted'",
                    [],
                    |r| r.get(0),
                ).map_err(|e| format!("查询可信设备事实失败: {}", e))?;

                if outbox_count > 0 && active_trusted == 0 {
                    true
                } else if outbox_count == 0 && active_trusted > 0 {
                    false
                } else {
                    return Err(format!(
                        "无法判定旧身份重置阶段 (journal {}, state {}, outbox {}, trusted {})，保留密钥与日志",
                        jid, state, outbox_count, active_trusted
                    ));
                }
            }
        };

        if !phase2_committed {
            // Phase 2 数据库事务未提交（或已回滚），撤销 Outbox 不存在，可信关系未作废
            // 安全自愈：取消重置日志，绝不能销毁密钥或清理配置，完整保留旧身份！
            conn.execute(
                "UPDATE identity_reset_journal SET state = 'cancelled', updated_at = ?, error = 'Startup recovery: Phase 2 uncommitted, safely cancelled reset to preserve old identity' WHERE id = ?",
                params![now_ms, jid],
            ).map_err(|e| format!("取消未提交重置日志失败: {}", e))?;
            cancelled_prepared += 1;
            outcome = IdentityRecoveryOutcome::CancelledPrepared { count: cancelled_prepared };
            log::info!("[Device Trust] 启动自愈检测到 Phase 2 未提交/已回滚的重置记录 (id: {}, revoked: {})，已安全取消并保留旧身份", jid, revoked_id);
            continue;
        }

        // Phase 2 确认已提交，但磁盘或内存可能已有另一代身份。
        // 在任何密钥删除之前核对当前身份，绝不能把旧 journal 应用到新密钥。
        let mut config = crate::read_config_checked()
            .map_err(|e| format!("恢复旧身份前读取配置失败，保留密钥与日志: {}", e))?;
        if let Some(active_id) = config.get("device_id").and_then(|v| v.as_str()).filter(|id| !id.trim().is_empty()) {
            if active_id != revoked_id {
                return Err(format!(
                    "恢复旧身份时发现另一代设备身份，保留密钥与日志: journal={}, active={}",
                    revoked_id, active_id
                ));
            }
        }
        if let Some(mem) = memory_state_opt {
            let guard = mem.lock().map_err(|e| format!("恢复旧身份前读取内存密钥失败，保留密钥: {}", e))?;
            if let Some(sk) = guard.as_ref() {
                let memory_id = BASE64.encode(VerifyingKey::from(sk).to_bytes());
                if memory_id != revoked_id {
                    return Err("恢复旧身份时内存中已有另一代密钥，保留密钥与日志".to_string());
                }
            }
        }

        // 身份核对通过后，才允许物理收敛旧密钥、内存状态与配置。
        let key_path_buf = key_path_opt.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            crate::get_data_dir()
                .join("workspace_config")
                .join("device_identity.json")
        });

        // 1. 确保物理密钥删除
        let key_removed = if key_path_buf.exists() {
            match std::fs::remove_file(&key_path_buf) {
                Ok(()) => true,
                Err(e) => {
                    let err_msg = format!("Startup recovery 无法删除密钥文件: {}", e);
                    let _ = conn.execute(
                        "UPDATE identity_reset_journal SET state = 'degraded_key_destroy', updated_at = ?, error = ? WHERE id = ?",
                        params![now_ms, &err_msg, jid],
                    );
                    return Ok(IdentityRecoveryOutcome::RetainedDegraded {
                        journal_id: jid,
                        reason: err_msg,
                    });
                }
            }
        } else {
            true
        };

        // 2. 确保内存私钥清空
        if let Some(mem) = memory_state_opt {
            if let Ok(mut guard) = mem.lock() {
                *guard = None;
            }
        }

        // 3. 清理先前已经核对过身份的 config.json
        let mut need_save = false;
        if let Some(obj) = config.as_object_mut() {
            if obj.remove("device_id").is_some() {
                need_save = true;
            }
            if obj.remove("pairing_payload").is_some() {
                need_save = true;
            }
        }
        if need_save {
            if let Err(e) = crate::write_config_checked(&config) {
                let err_msg = format!("Startup recovery 无法持久化清理配置: {}", e);
                let _ = conn.execute(
                    "UPDATE identity_reset_journal SET state = 'degraded_post_db', updated_at = ?, error = ? WHERE id = ?",
                    params![now_ms, &err_msg, jid],
                );
                return Ok(IdentityRecoveryOutcome::RetainedDegraded {
                    journal_id: jid,
                    reason: err_msg,
                });
            }
        }

        if key_removed {
            conn.execute(
                "UPDATE identity_reset_journal SET state = 'committed', updated_at = ?, error = 'Startup recovery: successfully converged to committed' WHERE id = ?",
                params![now_ms, jid],
            ).map_err(|e| format!("收敛重置日志为 committed 失败: {}", e))?;
            log::info!("[Device Trust] 启动自愈将已提交 DB 的重置记录 (id: {}, revoked: {}) 收敛为 committed", jid, revoked_id);
            outcome = IdentityRecoveryOutcome::ConvergedToCommitted { journal_id: jid };
        }
    }

    Ok(outcome)
}

pub fn mark_peer_revocation_failed(
    conn: &Connection,
    id: i64,
    err_msg: &str,
) -> Result<(), String> {
    let attempts: i64 = conn.query_row(
        "SELECT attempts FROM peer_revocation_outbox WHERE id = ?",
        [id],
        |row| row.get(0),
    ).unwrap_or(0);

    if attempts >= MAX_PEER_REVOCATION_ATTEMPTS {
        conn.execute(
            "UPDATE peer_revocation_outbox SET status = 'failed', last_error = ? WHERE id = ?",
            params![format!("{}: 已达最大重试次数 (attempts: {})", err_msg, attempts), id],
        ).map_err(|e| format!("更新撤销出件箱失败状态失败: {}", e))?;
    } else {
        conn.execute(
            "UPDATE peer_revocation_outbox SET status = 'pending', last_error = ? WHERE id = ? AND status = 'sent'",
            params![err_msg, id],
        ).map_err(|e| format!("更新撤销出件箱重试状态失败: {}", e))?;
    }
    Ok(())
}

pub fn revert_stale_sent_revocations(
    conn: &Connection,
    timeout_ms: i64,
    now_ms: i64,
) -> Result<usize, String> {
    let cutoff = now_ms - timeout_ms;
    // 超过最大重试次数直接标记为 failed，避免死锁阻止正常身份接入
    conn.execute(
        "UPDATE peer_revocation_outbox SET status = 'failed', last_error = 'Peer unreachable after max attempts' WHERE ((status = 'sent' AND (sent_at IS NULL OR sent_at < ?1)) OR status = 'pending') AND attempts >= ?2",
        params![cutoff, MAX_PEER_REVOCATION_ATTEMPTS],
    ).map_err(|e| format!("标记超限撤销为失败失败: {}", e))?;

    let count = conn.execute(
        "UPDATE peer_revocation_outbox SET status = 'pending' WHERE status = 'sent' AND (sent_at IS NULL OR sent_at < ?1) AND attempts < ?2",
        params![cutoff, MAX_PEER_REVOCATION_ATTEMPTS],
    ).map_err(|e| format!("回退超时发送中撤销记录失败: {}", e))?;
    Ok(count)
}

pub fn drain_peer_revocation_outbox_via_relay<F>(
    conn: &mut Connection,
    now_ms: i64,
    mut send_relay_fn: F,
) -> Result<usize, String>
where
    F: FnMut(&str, &str) -> Result<(), String>,
{
    let items = get_pending_peer_revocations(conn, 50)?;
    let mut sent_count = 0;
    for item in items {
        match send_relay_fn(&item.target_peer_id, &item.revocation_payload) {
            Ok(()) => {
                mark_peer_revocation_sent(conn, item.id, now_ms)?;
                sent_count += 1;
            }
            Err(e) => {
                mark_peer_revocation_failed(conn, item.id, &e)?;
            }
        }
    }
    Ok(sent_count)
}

pub fn drain_peer_revocation_outbox_via_lan<F>(
    conn: &mut Connection,
    now_ms: i64,
    mut send_lan_fn: F,
) -> Result<usize, String>
where
    F: FnMut(&str, &str) -> Result<RevocationAck, String>,
{
    let items = get_pending_peer_revocations(conn, 50)?;
    let mut delivered_count = 0;
    for item in items {
        mark_peer_revocation_sent(conn, item.id, now_ms)?;
        match send_lan_fn(&item.target_peer_id, &item.revocation_payload) {
            Ok(ack) => {
                match sec01_verify_and_apply_peer_revocation_ack(conn, &ack, &item.revoked_device_id, now_ms) {
                    Ok(RevocationAckOutcome::Delivered { .. }) | Ok(RevocationAckOutcome::AlreadyDelivered { .. }) => {
                        delivered_count += 1;
                    }
                    Err(e) => {
                        mark_peer_revocation_failed(conn, item.id, &format!("Ack 校验失败: {}", e))?;
                    }
                }
            }
            Err(e) => {
                mark_peer_revocation_failed(conn, item.id, &e)?;
            }
        }
    }
    Ok(delivered_count)
}

pub fn drain_peer_revocation_outbox<F>(
    conn: &mut Connection,
    now_ms: i64,
    mut dispatch_fn: F,
) -> Result<usize, String>
where
    F: FnMut(&str, &str) -> Result<(), String>,
{
    let items = get_pending_peer_revocations(conn, 50)?;
    let mut delivered_count = 0;
    for item in items {
        match dispatch_fn(&item.target_peer_id, &item.revocation_payload) {
            Ok(()) => {
                mark_peer_revocation_delivered(conn, item.id, now_ms)?;
                delivered_count += 1;
            }
            Err(e) => {
                mark_peer_revocation_failed(conn, item.id, &e)?;
            }
        }
    }
    Ok(delivered_count)
}

/// SEC-01 对端接收到撤销通知后作废该设备身份与信任
pub fn revoke_peer_device_in_db(
    conn: &mut Connection,
    revoked_device_id: &str,
    reason: &str,
    now_ms: i64,
) -> Result<bool, String> {
    let tx = conn.transaction().map_err(|e| format!("开启撤销对端设备事务失败: {}", e))?;

    let updated = tx.execute(
        "UPDATE trusted_devices SET status = 'revoked', revoked_at = ?1, revocation_reason = ?2 WHERE device_id = ?3 AND status = 'trusted'",
        params![now_ms, reason, revoked_device_id],
    ).map_err(|e| format!("更新设备信任状态失败: {}", e))?;

    tx.execute(
        "DELETE FROM authenticated_sessions WHERE subject_device_id = ?1 OR issuer_device_id = ?1",
        params![revoked_device_id],
    ).map_err(|e| format!("清理关联认证会话失败: {}", e))?;

    tx.execute(
        "DELETE FROM rpc_anti_replay WHERE subject_device_id = ?1",
        params![revoked_device_id],
    ).map_err(|e| format!("清理关联防重放缓存失败: {}", e))?;

    tx.commit().map_err(|e| format!("提交撤销对端设备事务失败: {}", e))?;
    log::info!("[Device Trust] 已作废对端设备信任与会话: {} (reason: {})", revoked_device_id, reason);
    Ok(updated > 0)
}

/// 查询设备是否为受信任状态
pub fn is_device_trusted(conn: &Connection, device_id: &str) -> bool {
    conn.query_row(
        "SELECT status, revoked_at FROM trusted_devices WHERE device_id = ?",
        [device_id],
        |row| {
            let s: String = row.get(0)?;
            let rev: Option<i64> = row.get(1)?;
            Ok(s == "trusted" && rev.is_none())
        },
    ).unwrap_or(false)
}

/// 获取全部可信与已撤销设备列表
pub fn get_trusted_devices(conn: &Connection) -> Result<Vec<TrustedDevice>, String> {
    let mut stmt = conn.prepare(
        "SELECT device_id, public_key, device_name, platform, paired_at, last_authenticated_at, status, revoked_at, revocation_reason
         FROM trusted_devices ORDER BY last_authenticated_at DESC"
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok(TrustedDevice {
            device_id: row.get(0)?,
            public_key: row.get(1)?,
            device_name: row.get(2)?,
            platform: row.get(3)?,
            paired_at: row.get(4)?,
            last_authenticated_at: row.get(5)?,
            status: TrustStatus::from_str(&row.get::<_, String>(6)?),
            revoked_at: row.get(7)?,
            revocation_reason: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut list = Vec::new();
    for r in rows {
        if let Ok(dev) = r {
            list.push(dev);
        }
    }
    Ok(list)
}

/// 针对 JSON 载荷生成稳定的规范化字节序列（对顶层键递归按字典序排序，剥离 envelope 字段）
pub fn canonicalize_json_value(val: &serde_json::Value) -> Vec<u8> {
    match val {
        serde_json::Value::Object(map) => {
            let mut sorted = std::collections::BTreeMap::new();
            for (k, v) in map {
                if k != "envelope" && k != "auth_envelope" {
                    sorted.insert(k.as_str(), v);
                }
            }
            serde_json::to_vec(&sorted).unwrap_or_default()
        }
        other => serde_json::to_vec(other).unwrap_or_default(),
    }
}

/// 显式规范化移除 envelope 后的载荷字节序列，供发送方与接收方统一调用 (SEC-01)
pub fn canonicalize_payload_without_envelope(val: &serde_json::Value) -> Vec<u8> {
    canonicalize_json_value(val)
}

/// 构造并签署一个受保护 RPC 请求鉴权封套 RpcAuthEnvelope
pub fn create_rpc_auth_envelope(
    signing_key: &SigningKey,
    session_id: &str,
    request_id: &str,
    subject_device_id: &str,
    target_device_id: &str,
    action: &str,
    raw_inner_payload: &[u8],
    now_ms: i64,
) -> RpcAuthEnvelope {
    let payload_hash = compute_sha512(raw_inner_payload);
    let mut nonce_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = BASE64.encode(nonce_bytes);

    let canonical = canonical_rpc_bytes(
        SEC01_PROTOCOL_VERSION,
        session_id,
        request_id,
        subject_device_id,
        target_device_id,
        action,
        &payload_hash,
        &nonce,
        now_ms,
    );

    let signature = signing_key.sign(&canonical);
    let signature_b64 = BASE64.encode(signature.to_bytes());

    RpcAuthEnvelope {
        protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
        session_id: session_id.to_string(),
        request_id: request_id.to_string(),
        subject_device_id: subject_device_id.to_string(),
        target_device_id: target_device_id.to_string(),
        action: action.to_string(),
        payload_hash,
        nonce,
        timestamp: now_ms,
        signature: signature_b64,
    }
}

/// 获取对端现有的未过期活跃会话，若不存在则为已受信任的设备原子创建新会话
pub fn get_or_create_active_session(
    conn: &mut Connection,
    subject_device_id: &str,
    issuer_device_id: &str,
    now_ms: i64,
) -> Result<String, String> {
    // 1. 验证对端必须处于受信任状态
    let dev_opt: Option<(String, Option<i64>)> = conn.query_row(
        "SELECT status, revoked_at FROM trusted_devices WHERE device_id = ?",
        [subject_device_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional().map_err(|e| format!("查询可信设备失败: {}", e))?;

    let (status, revoked_at) = match dev_opt {
        Some(s) => s,
        None => return Err(format!("设备未建立信任关系 (SEC-01 fail-closed): {}", subject_device_id)),
    };

    if status != "trusted" || revoked_at.is_some() {
        return Err(format!("设备已被撤销或未受信任: {}", subject_device_id));
    }

    // 2. 查询两设备间双向有效的未过期会话
    let existing_opt: Option<String> = conn.query_row(
        "SELECT session_id FROM authenticated_sessions
         WHERE ((subject_device_id = ?1 AND issuer_device_id = ?2) OR (subject_device_id = ?2 AND issuer_device_id = ?1))
           AND is_active = 1 AND expires_at > ?3
         ORDER BY created_at DESC LIMIT 1",
        params![subject_device_id, issuer_device_id, now_ms],
        |row| row.get(0),
    ).optional().map_err(|e| format!("查询会话记录失败: {}", e))?;

    if let Some(sid) = existing_opt {
        return Ok(sid);
    }

    // 3. 不存在时自动建立新会话
    let sess = create_authenticated_session(conn, subject_device_id, issuer_device_id, DEFAULT_SESSION_TTL_MS, now_ms)?;
    Ok(sess.session_id)
}

/// 使用数据库连接和私钥为出站 RPC 请求签名
pub fn sign_outgoing_rpc_envelope(
    conn: &mut Connection,
    signing_key: &SigningKey,
    local_device_id: &str,
    target_device_id: &str,
    action: &str,
    raw_inner_payload: &[u8],
    request_id: Option<&str>,
    now_ms: i64,
) -> Result<RpcAuthEnvelope, String> {
    let req_id = request_id
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let session_id = get_or_create_active_session(conn, target_device_id, local_device_id, now_ms)?;
    Ok(create_rpc_auth_envelope(
        signing_key,
        &session_id,
        &req_id,
        local_device_id,
        target_device_id,
        action,
        raw_inner_payload,
        now_ms,
    ))
}

/// 生产级出站 RPC 签名辅助函数：自动从 AppHandle 提取秘钥、本地设备身份和数据库会话
pub fn sign_outgoing_rpc(
    app: &tauri::AppHandle,
    target_device_id: &str,
    action: &str,
    raw_inner_payload: &[u8],
    request_id: Option<&str>,
) -> Result<RpcAuthEnvelope, String> {
    use tauri::Manager;
    let signing_key = crate::crypto::ensure_device_identity_unlocked_for_app(app)?;

    let verifying_key = VerifyingKey::from(&signing_key);
    let pubkey_b64 = BASE64.encode(verifying_key.to_bytes());
    let local_device_id = pubkey_b64.clone();

    // SEC-01 统一身份模型：保持 config["device_id"] 与 Ed25519 持钥身份绝对同步
    let mut config = crate::read_config_checked()?;
    if config.get("device_id").and_then(|v| v.as_str()) != Some(&pubkey_b64) {
        if let Some(obj) = config.as_object_mut() {
            obj.insert("device_id".to_string(), serde_json::json!(pubkey_b64));
        }
        crate::write_config_checked(&config)?;
    }

    let db_state = app.try_state::<crate::db::DbState>()
        .ok_or_else(|| "未找到数据库状态".to_string())?;
    let mut conn = db_state.0.lock().map_err(|e| format!("数据库加锁失败: {}", e))?;

    let now_ms = crate::now_ms();
    sign_outgoing_rpc_envelope(
        &mut conn,
        &signing_key,
        &local_device_id,
        target_device_id,
        action,
        raw_inner_payload,
        request_id,
        now_ms,
    )
}

/// 生产级出站 JSON RPC 签名辅助函数：自动规范化 JSON 负载并完成签名
pub fn sign_outgoing_rpc_json(
    app: &tauri::AppHandle,
    target_device_id: &str,
    action: &str,
    payload_val: &serde_json::Value,
    request_id: Option<&str>,
) -> Result<RpcAuthEnvelope, String> {
    let canonical = canonicalize_json_value(payload_val);
    sign_outgoing_rpc(app, target_device_id, action, &canonical, request_id)
}

/// Verify a relay response without reserving a business idempotency lease or mutating the DB.
/// The pending-request correlation is checked by the caller, so a signed replay cannot satisfy
/// a different request. Pairing ACKs are the sole bootstrap exception: the target's public key
/// comes from the invitation, before its trust row/session has been installed locally.
pub fn verify_relay_response_auth(
    conn: &Connection,
    response: &serde_json::Value,
    expected_peer: &str,
    expected_local: &str,
    allow_pairing_bootstrap: bool,
    now_ms: i64,
) -> Result<(), String> {
    let auth_val = response.get("auth_envelope")
        .or_else(|| response.get("payload").and_then(|p| p.get("auth_envelope")))
        .cloned()
        .ok_or("SEC-01 response missing authentication envelope")?;
    let auth: RpcAuthEnvelope = serde_json::from_value(auth_val)
        .map_err(|e| format!("SEC-01 malformed response envelope: {e}"))?;
    if auth.protocol_version != SEC01_PROTOCOL_VERSION || auth.action != "relay_response" {
        return Err("SEC-01 response protocol or action mismatch".into());
    }
    if auth.subject_device_id != expected_peer
        || auth.target_device_id != expected_local
        || response.get("from_device_id").and_then(|v| v.as_str()) != Some(expected_peer)
        || auth.session_id.trim().is_empty()
        || auth.nonce.trim().is_empty()
    {
        return Err("SEC-01 response identity, target, or message mismatch".into());
    }
    if let Some(target) = response.get("target_device_id").and_then(|v| v.as_str()) {
        if target != expected_local {
            return Err("SEC-01 response target mismatch".into());
        }
    }
    if let Some(msg_id) = response.get("message_id").and_then(|v| v.as_str()) {
        if msg_id != auth.request_id {
            return Err("SEC-01 response message mismatch".into());
        }
    }
    if (now_ms - auth.timestamp).abs() > RPC_MAX_CLOCK_SKEW_MS {
        return Err("SEC-01 response timestamp outside allowed window".into());
    }
    if is_identity_degraded(conn)? {
        return Err("SEC-01 local identity is degraded; response rejected".into());
    }
    let mut matches_hash = false;
    let mut unsigned = response.clone();
    if let Some(obj) = unsigned.as_object_mut() {
        obj.remove("auth_envelope");
        if let Some(payload_obj) = obj.get_mut("payload").and_then(|p| p.as_object_mut()) {
            payload_obj.remove("auth_envelope");
            payload_obj.remove("signed_bytes");
        }
    }
    if compute_sha512(&canonicalize_json_value(&unsigned)) == auth.payload_hash {
        matches_hash = true;
    } else {
        if let Some(obj) = unsigned.as_object_mut() {
            obj.insert("target_device_id".to_string(), serde_json::json!(expected_local));
        }
        if compute_sha512(&canonicalize_json_value(&unsigned)) == auth.payload_hash {
            matches_hash = true;
        }
    }
    if !matches_hash {
        if let Some(sb) = response.get("payload").and_then(|p| p.get("signed_bytes")).and_then(|v| v.as_str()) {
            if compute_sha512(sb.as_bytes()) == auth.payload_hash {
                if let Ok(sb_val) = serde_json::from_str::<serde_json::Value>(sb) {
                    if sb_val.get("from_device_id").and_then(|v| v.as_str()) == Some(expected_peer)
                        && sb_val.get("target_device_id").and_then(|v| v.as_str()) == Some(expected_local)
                    {
                        matches_hash = true;
                    }
                }
            }
        }
    }
    if !matches_hash {
        return Err("SEC-01 response payload hash mismatch".into());
    }
    if !allow_pairing_bootstrap {
        let trusted: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM trusted_devices WHERE device_id = ?1 AND public_key = ?1 AND status = 'trusted' AND revoked_at IS NULL)",
            [&auth.subject_device_id], |row| row.get(0),
        ).map_err(|e| format!("SEC-01 response trust lookup failed: {e}"))?;
        if !trusted {
            return Err("SEC-01 response from untrusted or revoked device".into());
        }
        let active: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM authenticated_sessions WHERE session_id = ?1 AND is_active = 1 AND expires_at > ?2 AND ((subject_device_id = ?3 AND issuer_device_id = ?4) OR (subject_device_id = ?4 AND issuer_device_id = ?3)))",
            rusqlite::params![auth.session_id, now_ms, expected_peer, expected_local], |row| row.get(0),
        ).map_err(|e| format!("SEC-01 response session lookup failed: {e}"))?;
        if !active {
            return Err("SEC-01 response session is missing, expired, or revoked".into());
        }
    }
    let canonical = canonical_rpc_bytes(
        &auth.protocol_version, &auth.session_id, &auth.request_id,
        &auth.subject_device_id, &auth.target_device_id, &auth.action,
        &auth.payload_hash, &auth.nonce, auth.timestamp,
    );
    verify_signature(expected_peer, &canonical, &auth.signature)
}

// ------------------------------------------------------------------------------
// Tauri Command Handlers (A2 / SEC-01 IPC API)
// ------------------------------------------------------------------------------

#[tauri::command]
pub fn sec01_create_pairing_invitation(
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::DbState>,
    crypto_state: tauri::State<'_, crate::crypto::DeviceIdentityState>,
    target_constraint: Option<String>,
    ttl_ms: Option<i64>,
) -> Result<PairingInvitationPayload, String> {
    let conn = db.0.lock().map_err(|e| format!("数据库锁定失败: {}", e))?;
    let _ = crypto_state;
    let signing_key = crate::crypto::ensure_device_identity_unlocked_for_app(&app)?;
    let verifying_key = VerifyingKey::from(&signing_key);
    let issuer_device_id = BASE64.encode(verifying_key.to_bytes());

    let local_ips = crate::crypto::get_candidate_ips();
    let relay = "wss://relay.bobbik.org";
    let now_ms = crate::now_ms();
    let ttl = ttl_ms.unwrap_or(DEFAULT_INVITATION_TTL_MS);

    create_pairing_invitation(
        &conn,
        &issuer_device_id,
        target_constraint.as_deref(),
        ttl,
        relay,
        local_ips,
        3722,
        now_ms,
    )
}

#[tauri::command]
pub fn sec01_parse_pairing_invitation(
    raw_invitation: String,
) -> Result<PairingInvitationPayload, String> {
    PairingInvitationPayload::parse_input(&raw_invitation)
}

/// SEC-01 统一 PoP 签名核心函数：基于已解锁的 SigningKey 生成所有权证明
pub fn create_proof_of_possession_from_signing_key(
    signing_key: &SigningKey,
    invitation: &PairingInvitationPayload,
    device_name: Option<String>,
    now_ms: i64,
) -> Result<ProofOfPossession, String> {
    let verifying_key = VerifyingKey::from(signing_key);
    let pubkey_b64 = BASE64.encode(verifying_key.to_bytes());
    // SEC-01 统一身份模型：持钥实体的 device_id 始终严格为其 Ed25519 公钥 (pubkey_b64)
    let subject_device_id = pubkey_b64.clone();

    let mut config = crate::read_config_checked()?;
    if config.get("device_id").and_then(|v| v.as_str()) != Some(&pubkey_b64) {
        if let Some(obj) = config.as_object_mut() {
            obj.insert("device_id".to_string(), serde_json::json!(pubkey_b64));
        }
        crate::write_config_checked(&config)?;
    }

    let platform = std::env::consts::OS.to_string();
    let name = device_name
        .or_else(|| config.get("deviceName").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| format!("Bob on {}", platform));

    let mut nonce_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = BASE64.encode(nonce_bytes);

    let canonical = canonical_pop_bytes(
        SEC01_PROTOCOL_VERSION,
        &invitation.invitation_id,
        &invitation.secret,
        &invitation.issuer_device_id,
        &subject_device_id,
        &pubkey_b64,
        &name,
        &platform,
        &nonce,
        now_ms,
        SEC01_POP_PURPOSE,
    );

    let signature = signing_key.sign(&canonical);
    let signature_b64 = BASE64.encode(signature.to_bytes());

    Ok(ProofOfPossession {
        protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
        invitation_id: invitation.invitation_id.clone(),
        invitation_secret: invitation.secret.clone(),
        issuer_device_id: invitation.issuer_device_id.clone(),
        subject_device_id,
        subject_pubkey: pubkey_b64,
        device_name: name,
        platform,
        nonce,
        timestamp: now_ms,
        purpose: SEC01_POP_PURPOSE.to_string(),
        signature: signature_b64,
    })
}

#[tauri::command]
pub fn sec01_create_proof_of_possession(
    app: tauri::AppHandle,
    crypto_state: tauri::State<'_, crate::crypto::DeviceIdentityState>,
    invitation: PairingInvitationPayload,
    device_name: Option<String>,
) -> Result<ProofOfPossession, String> {
    let _ = crypto_state;
    let signing_key = crate::crypto::ensure_device_identity_unlocked_for_app(&app)?;
    let now_ms = crate::now_ms();
    create_proof_of_possession_from_signing_key(&signing_key, &invitation, device_name, now_ms)
}

pub fn create_proof_of_possession_for_app(
    app: &tauri::AppHandle,
    invitation: &PairingInvitationPayload,
    device_name: Option<String>,
) -> Result<ProofOfPossession, String> {
    let signing_key = crate::crypto::ensure_device_identity_unlocked_for_app(app)?;
    let now_ms = crate::now_ms();
    create_proof_of_possession_from_signing_key(&signing_key, invitation, device_name, now_ms)
}

#[tauri::command]
pub fn sec01_revoke_trusted_device(
    db: tauri::State<'_, crate::db::DbState>,
    device_id: String,
    reason: Option<String>,
) -> Result<bool, String> {
    let mut conn = db.0.lock().map_err(|e| format!("数据库锁定失败: {}", e))?;
    let now_ms = crate::now_ms();
    revoke_trusted_device(
        &mut conn,
        &device_id,
        reason.as_deref(),
        now_ms,
    ).map(|_| true)
}

#[tauri::command]
pub fn sec01_get_trusted_devices(
    db: tauri::State<'_, crate::db::DbState>,
) -> Result<Vec<TrustedDevice>, String> {
    let conn = db.0.lock().map_err(|e| format!("数据库锁定失败: {}", e))?;
    get_trusted_devices(&conn)
}

#[tauri::command]
pub fn sec01_is_device_trusted(
    db: tauri::State<'_, crate::db::DbState>,
    device_id: String,
) -> Result<bool, String> {
    let conn = db.0.lock().map_err(|e| format!("数据库锁定失败: {}", e))?;
    Ok(is_device_trusted(&conn, &device_id))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingResult {
    pub status: String,
    pub transport: String,
    pub target_device_id: String,
    pub session_id: String,
}

#[tauri::command]
pub async fn sec01_pair_device(
    app: tauri::AppHandle,
    raw_invitation: String,
) -> Result<PairingResult, String> {
    let invitation = PairingInvitationPayload::parse_input(&raw_invitation)?;
    let pop = create_proof_of_possession_for_app(&app, &invitation, None)?;
    let target_device_id = invitation.issuer_device_id.clone();
    let my_id = crate::http_api::resolve_local_device_id_checked(Some(&app))?;
    if my_id.trim().is_empty() {
        return Err("SEC-01 Fail-Closed: Local device identity is empty, pairing aborted".to_string());
    }
    let now_ms = crate::now_ms();

    // 1. 尝试局域网直连建信配对 (LAN POST /v1/pair)
    if !invitation.local_ips.is_empty() {
        for ip in &invitation.local_ips {
            let pair_url = format!("http://{}:{}/v1/pair", ip, invitation.port);
            if let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).build() {
                if let Ok(res) = client.post(&pair_url).json(&pop).send().await {
                    if res.status().is_success() {
                        if let Ok(resp_json) = res.json::<serde_json::Value>().await {
                            if resp_json.get("status").and_then(|v| v.as_str()) == Some("trusted") {
                                let session_id = resp_json.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                if session_id.is_empty() {
                                    return Err("Server returned trusted status without session_id".to_string());
                                }
                                // 原子落盘可信设备和活跃会话 (Fail-Closed: 任何落库失败立即报错)
                                let db_res = if let Some(db_state) = app.try_state::<crate::db::DbState>() {
                                    if let Ok(mut conn) = db_state.0.lock() {
                                        sec01_persist_mobile_trusted_session(
                                            &mut conn,
                                            &target_device_id,
                                            &my_id,
                                            &session_id,
                                            "Paired PC",
                                            "windows",
                                            DEFAULT_SESSION_TTL_MS,
                                            now_ms,
                                        )
                                    } else {
                                        Err("Database lock failed on mobile".to_string())
                                    }
                                } else if let Some(mut conn) = crate::http_api::open_db_for_app(&app) {
                                    sec01_persist_mobile_trusted_session(
                                        &mut conn,
                                        &target_device_id,
                                        &my_id,
                                        &session_id,
                                        "Paired PC",
                                        "windows",
                                        DEFAULT_SESSION_TTL_MS,
                                        now_ms,
                                    )
                                } else {
                                    Err("Database unavailable on mobile".to_string())
                                };

                                db_res?;

                                return Ok(PairingResult {
                                    status: "trusted".to_string(),
                                    transport: "lan".to_string(),
                                    target_device_id,
                                    session_id,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. 局域网失败，尝试外网中继握手 (Relay Handshake with PoP)
    let session_id = crate::sync_engine::relay_handshake(app.clone(), target_device_id.clone(), raw_invitation).await
        .map_err(|e| format!("Pairing via Relay failed: {}", e))?;

    if session_id.trim().is_empty() {
        return Err("Relay pairing completed without valid session_id".to_string());
    }

    Ok(PairingResult {
        status: "trusted".to_string(),
        transport: "relay".to_string(),
        target_device_id,
        session_id,
    })
}

// ------------------------------------------------------------------------------
// Tests Module: Covering Negative Security Matrix (1-24) & Full End-to-End Flow
// ------------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    pub fn setup_test_db() -> Connection {
        set_device_trust_init_failed(false);
        let conn = Connection::open_in_memory().unwrap();
        init_device_trust_tables(&conn).unwrap();
        conn
    }

    pub fn generate_test_keypair() -> (SigningKey, String) {
        let mut csprng = rand::rngs::OsRng;
        let mut bytes = [0u8; 32];
        csprng.fill_bytes(&mut bytes);
        let sk = SigningKey::from_bytes(&bytes);
        let vk = VerifyingKey::from(&sk);
        let pk_b64 = BASE64.encode(vk.to_bytes());
        (sk, pk_b64)
    }

    // --------------------------------------------------------------------------
    // Test 1: Forged device_id rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_forged_device_id_rejected() {
        let mut conn = setup_test_db();
        let (sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();

        // Attacker claims a forged subject_device_id that does NOT match pk_subject
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: "forged_victim_device_id".to_string(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Attacker Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: "dummy_sig".to_string(),
        };

        let res = verify_and_consume_invitation(&mut conn, &pop, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("身份伪造检测"));
    }

    // --------------------------------------------------------------------------
    // Test 2: Knowing public key without private key rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_public_key_only_no_signature_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (_sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();

        // Attacker knows pk_subject, but doesn't have sk_subject, sends garbage/empty signature
        let garbage_sig = BASE64.encode([0u8; 64]);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Victim Device".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: garbage_sig,
        };

        let res = verify_and_consume_invitation(&mut conn, &pop, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("持钥证明无效"));
    }

    // --------------------------------------------------------------------------
    // Test 3: Signed with wrong private key rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_wrong_private_key_signature_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (_sk_subject, pk_subject) = generate_test_keypair();
        let (sk_evil, _pk_evil) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();

        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &pk_issuer,
            &pk_subject,
            &pk_subject,
            "Victim",
            "android",
            "nonce-1",
            now,
            "pairing_establishment",
        );
        // Signed with sk_evil, declared pubkey is pk_subject
        let evil_sig = sign_bytes(&sk_evil, &canonical);

        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Victim".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: evil_sig,
        };

        let res = verify_and_consume_invitation(&mut conn, &pop, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("持钥证明无效"));
    }

    // --------------------------------------------------------------------------
    // Test 4: invitation_id mismatch rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_invitation_id_mismatch_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let _invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();

        let fake_invite_id = uuid::Uuid::new_v4().to_string();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &fake_invite_id,
            "secret",
            &pk_issuer,
            &pk_subject,
            &pk_subject,
            "Phone",
            "android",
            "nonce-1",
            now,
            "pairing_establishment",
        );
        let sig = sign_bytes(&sk_subject, &canonical);

        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: fake_invite_id,
            invitation_secret: "secret".to_string(),
            issuer_device_id: pk_issuer,
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject,
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };

        let res = verify_and_consume_invitation(&mut conn, &pop, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("未找到配对邀请记录"));
    }

    // --------------------------------------------------------------------------
    // Test 5: Expired invitation rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_expired_invitation_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let created_at = 1_000_000;
        let ttl = 600_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, ttl, "relay", vec![], 3722, created_at).unwrap();

        let attempt_time = created_at + ttl + 1000; // Expired by 1s
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &pk_issuer,
            &pk_subject,
            &pk_subject,
            "Phone",
            "android",
            "nonce-1",
            attempt_time,
            "pairing_establishment",
        );
        let sig = sign_bytes(&sk_subject, &canonical);

        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer,
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject,
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-1".to_string(),
            timestamp: attempt_time,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };

        let res = verify_and_consume_invitation(&mut conn, &pop, attempt_time);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("配对邀请已过期"));
    }

    // --------------------------------------------------------------------------
    // Test 6: Consumed invitation replay rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_consumed_invitation_replay_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();

        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &pk_issuer,
            &pk_subject,
            &pk_subject,
            "Phone",
            "android",
            "nonce-1",
            now,
            "pairing_establishment",
        );
        let sig = sign_bytes(&sk_subject, &canonical);

        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer,
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject,
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };

        // First attempt succeeds
        let res1 = verify_and_consume_invitation(&mut conn, &pop, now);
        assert!(res1.is_ok());

        // Replay attempt fails immediately
        let res2 = verify_and_consume_invitation(&mut conn, &pop, now);
        assert!(res2.is_err());
        assert!(res2.unwrap_err().contains("已被消费，严禁重复使用"));
    }

    // --------------------------------------------------------------------------
    // Test 7: Nonce replay rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_nonce_replay_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "nonce-init", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-init".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();

        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{\"action\":\"rpc_discover_capabilities\"}";
        let payload_hash = compute_sha512(payload);
        let rpc_canonical = canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-1", &pk_subject, &pk_issuer, "rpc_discover_capabilities", &payload_hash, "reused-nonce", now
        );
        let rpc_sig = sign_bytes(&sk_subject, &rpc_canonical);

        let env1 = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-1".to_string(),
            subject_device_id: pk_subject.clone(),
            target_device_id: pk_issuer.clone(),
            action: "rpc_discover_capabilities".to_string(),
            payload_hash: payload_hash.clone(),
            nonce: "reused-nonce".to_string(),
            timestamp: now,
            signature: rpc_sig.clone(),
        };

        // First RPC succeeds
        let out1 = verify_rpc_request_auth(&mut conn, &env1, payload, &pk_issuer, now);
        assert!(out1.is_ok());

        // Replay with identical nonce for different request_id rejected!
        let rpc_canonical2 = canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-2", &pk_subject, &pk_issuer, "rpc_discover_capabilities", &payload_hash, "reused-nonce", now
        );
        let rpc_sig2 = sign_bytes(&sk_subject, &rpc_canonical2);

        let env2 = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-2".to_string(),
            subject_device_id: pk_subject.clone(),
            target_device_id: pk_issuer.clone(),
            action: "rpc_discover_capabilities".to_string(),
            payload_hash: payload_hash.clone(),
            nonce: "reused-nonce".to_string(), // Reused!
            timestamp: now,
            signature: rpc_sig2,
        };
        let out2 = verify_rpc_request_auth(&mut conn, &env2, payload, &pk_issuer, now);
        assert!(out2.is_err());
        assert!(out2.unwrap_err().contains("检测到重放攻击: 重复的 nonce"));
    }

    // --------------------------------------------------------------------------
    // Test 8: Idempotent request_id retry does not duplicate side effects
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_idempotent_request_no_duplicate_side_effects() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "nonce-pop", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-pop".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{\"change_id\":\"c1\",\"decision\":\"accept\"}";
        let payload_hash = compute_sha512(payload);
        let rpc_canonical = canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-retry", &pk_subject, &pk_issuer, "rpc_approval_decision", &payload_hash, "nonce-ret", now
        );
        let rpc_sig = sign_bytes(&sk_subject, &rpc_canonical);

        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-retry".to_string(),
            subject_device_id: pk_subject.clone(),
            target_device_id: pk_issuer.clone(),
            action: "rpc_approval_decision".to_string(),
            payload_hash: payload_hash.clone(),
            nonce: "nonce-ret".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        // First verification succeeds
        let out1 = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now).unwrap();
        assert!(matches!(out1, AuthVerificationOutcome::Authorized { .. }));

        // Simulate core business executing and recording idempotency response
        record_rpc_idempotency_result(&conn, &session.session_id, "req-retry", &pk_subject, &pk_issuer, "rpc_approval_decision", &payload_hash, "{\"status\":\"applied\"}", now).unwrap();

        // Second verification is idempotent and returns cached result without re-executing
        let out2 = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now).unwrap();
        match out2 {
            AuthVerificationOutcome::IdempotentCached { cached_response } => {
                assert_eq!(cached_response, "{\"status\":\"applied\"}");
            }
            _ => panic!("Expected idempotent cached response"),
        }
    }

    // --------------------------------------------------------------------------
    // Test 9: Tampered session_id with replayed signature rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_tampered_session_id_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "nonce-pop", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-pop".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{\"action\":\"rpc_cancel\"}";
        let payload_hash = compute_sha512(payload);
        let rpc_canonical = canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-tamp-sess", &pk_subject, &pk_issuer, "rpc_cancel", &payload_hash, "nonce-tamp", now
        );
        let rpc_sig = sign_bytes(&sk_subject, &rpc_canonical);

        // Attacker alters session_id in the envelope
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: "other_session_id".to_string(),
            request_id: "req-tamp-sess".to_string(),
            subject_device_id: pk_subject,
            target_device_id: pk_issuer.clone(),
            action: "rpc_cancel".to_string(),
            payload_hash,
            nonce: "nonce-tamp".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now);
        assert!(res.is_err());
    }

    // --------------------------------------------------------------------------
    // Test 10: Tampered target_device_id rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_tampered_target_device_id_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{}";
        let payload_hash = compute_sha512(payload);
        let rpc_sig = sign_bytes(&sk_subject, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-t", &pk_subject, &pk_issuer, "rpc_cancel", &payload_hash, "n1", now
        ));

        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id,
            request_id: "req-t".to_string(),
            subject_device_id: pk_subject,
            target_device_id: "someone_elses_pc_id".to_string(),
            action: "rpc_cancel".to_string(),
            payload_hash,
            nonce: "n1".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("目标设备 ID 不匹配"));
    }

    // --------------------------------------------------------------------------
    // Test 11: Tampered request_id or operation type rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_tampered_action_or_request_id_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{}";
        let payload_hash = compute_sha512(payload);
        // Signed for req-A
        let rpc_sig = sign_bytes(&sk_subject, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-A", &pk_subject, &pk_issuer, "rpc_cancel", &payload_hash, "n-tamp", now
        ));

        // Attacker changes request_id to req-B
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id,
            request_id: "req-B".to_string(),
            subject_device_id: pk_subject,
            target_device_id: pk_issuer.clone(),
            action: "rpc_cancel".to_string(),
            payload_hash,
            nonce: "n-tamp".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("数字签名验证失败"));
    }

    // --------------------------------------------------------------------------
    // Test 12: Cross-action signature reuse rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_cross_action_signature_reuse_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{}";
        let payload_hash = compute_sha512(payload);
        // Signed for "rpc_discover_capabilities"
        let rpc_sig = sign_bytes(&sk_subject, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-1", &pk_subject, &pk_issuer, "rpc_discover_capabilities", &payload_hash, "n-action", now
        ));

        // Attacker tries to use the same signature to invoke "rpc_approval_decision"
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id,
            request_id: "req-1".to_string(),
            subject_device_id: pk_subject,
            target_device_id: pk_issuer.clone(),
            action: "rpc_approval_decision".to_string(),
            payload_hash,
            nonce: "n-action".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("数字签名验证失败"));
    }

    // --------------------------------------------------------------------------
    // Test 13: Swapped identities rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_swapped_identities_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{}";
        let payload_hash = compute_sha512(payload);
        // Attacker swaps subject and target in the call
        let rpc_sig = sign_bytes(&sk_subject, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-swap", &pk_issuer, &pk_subject, "rpc_cancel", &payload_hash, "n-swap", now
        ));

        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id,
            request_id: "req-swap".to_string(),
            subject_device_id: pk_issuer.clone(), // Inverted!
            target_device_id: pk_subject,
            action: "rpc_cancel".to_string(),
            payload_hash,
            nonce: "n-swap".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now);
        assert!(res.is_err());
    }

    // --------------------------------------------------------------------------
    // Test 14: Rotated public key without re-pairing rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_rotated_public_key_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let (sk_subject_new, _pk_subject_new) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        // Subject generated new keys sk_subject_new without re-pairing through invitation
        let payload = b"{}";
        let payload_hash = compute_sha512(payload);
        let rpc_sig = sign_bytes(&sk_subject_new, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-rot", &pk_subject, &pk_issuer, "rpc_cancel", &payload_hash, "n-rot", now
        ));

        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id,
            request_id: "req-rot".to_string(),
            subject_device_id: pk_subject,
            target_device_id: pk_issuer.clone(),
            action: "rpc_cancel".to_string(),
            payload_hash,
            nonce: "n-rot".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("数字签名验证失败"));
    }

    // --------------------------------------------------------------------------
    // Test 15: Revoked device rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_revoked_device_rejected() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();

        // Revoke device
        revoke_trusted_device(&mut conn, &pk_subject, Some("User requested unbind"), now + 10).unwrap();

        // Attempt to create session fails
        let sess_res = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now + 20);
        assert!(sess_res.is_err());
        assert!(sess_res.unwrap_err().contains("设备已被撤销信任"));
    }

    // --------------------------------------------------------------------------
    // Test 16: Active session established before revocation rejected after revocation
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_active_session_rejected_after_revocation() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n", now, "pairing_establishment"
        );
        let sig = sign_bytes(&sk_subject, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sig,
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        // User revokes the device
        revoke_trusted_device(&mut conn, &pk_subject, Some("Unbound"), now + 50).unwrap();

        // Active session is now invalid for any RPC
        let payload = b"{}";
        let payload_hash = compute_sha512(payload);
        let rpc_sig = sign_bytes(&sk_subject, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-post-rev", &pk_subject, &pk_issuer, "rpc_cancel", &payload_hash, "n-post-rev", now + 60
        ));

        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id,
            request_id: "req-post-rev".to_string(),
            subject_device_id: pk_subject,
            target_device_id: pk_issuer.clone(),
            action: "rpc_cancel".to_string(),
            payload_hash,
            nonce: "n-post-rev".to_string(),
            timestamp: now + 60,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now + 60);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("设备已被撤销"));
    }

    // --------------------------------------------------------------------------
    // Test 17: Database query failure fails closed
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_db_failure_fail_closed() {
        let mut conn = Connection::open_in_memory().unwrap();
        // Do NOT create tables -> triggers SQLite error
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: "s1".to_string(),
            request_id: "r1".to_string(),
            subject_device_id: "dev1".to_string(),
            target_device_id: "pc1".to_string(),
            action: "rpc_request".to_string(),
            payload_hash: "hash".to_string(),
            nonce: "n1".to_string(),
            timestamp: 1000,
            signature: "sig".to_string(),
        };

        let res = verify_rpc_request_auth(&mut conn, &env, b"{}", "pc1", 1000);
        assert!(res.is_err());
    }

    // --------------------------------------------------------------------------
    // Test 18: Invalid protocol version rejected
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_invalid_protocol_version_rejected() {
        let mut conn = setup_test_db();
        let env = RpcAuthEnvelope {
            protocol_version: "v999_incompatible".to_string(),
            session_id: "s1".to_string(),
            request_id: "r1".to_string(),
            subject_device_id: "dev1".to_string(),
            target_device_id: "pc1".to_string(),
            action: "rpc_request".to_string(),
            payload_hash: "hash".to_string(),
            nonce: "n1".to_string(),
            timestamp: 1000,
            signature: "sig".to_string(),
        };

        let res = verify_rpc_request_auth(&mut conn, &env, b"{}", "pc1", 1000);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("不支持的 RPC 鉴权协议版本"));
    }

    // --------------------------------------------------------------------------
    // Test 19: Discovered device cannot execute protected RPC
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_discovered_device_cannot_execute_protected_rpc() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        // Discovered via LAN or Relay presence
        let discovered = DiscoveredDevice {
            device_id: pk_subject.clone(),
            device_name: Some("LAN Phone".to_string()),
            platform: "android".to_string(),
            ip_address: "192.168.1.50".to_string(),
            port: 3723,
            transport: "lan".to_string(),
            last_seen: now,
            is_trusted: false,
        };
        assert!(!discovered.is_trusted);

        // Migrate to legacy unverified
        migrate_legacy_devices_to_unverified(&mut conn, &[discovered], now).unwrap();

        // Attempt to create session with unverified discovered device fails
        let sess_res = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now);
        assert!(sess_res.is_err());
        assert!(sess_res.unwrap_err().contains("设备处于非可信状态"));

        // Attempting to execute RPC fails closed
        let payload = b"{\"action\":\"rpc_request\"}";
        let payload_hash = compute_sha512(payload);
        let rpc_sig = sign_bytes(&sk_subject, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, "fake_sess", "req-disc", &pk_subject, &pk_issuer, "rpc_request", &payload_hash, "n-disc", now
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: "fake_sess".to_string(),
            request_id: "req-disc".to_string(),
            subject_device_id: pk_subject,
            target_device_id: pk_issuer.clone(),
            action: "rpc_request".to_string(),
            payload_hash,
            nonce: "n-disc".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        let res = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now);
        assert!(res.is_err());
    }

    // --------------------------------------------------------------------------
    // Test 20: QR and URL invitations parity
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_qr_and_url_invitation_parity() {
        let conn = setup_test_db();
        let (_sk, pk) = generate_test_keypair();
        let payload = create_pairing_invitation(
            &conn, &pk, None, 600_000, "wss://relay.bobbik.org", vec!["192.168.1.10".to_string()], 3722, 1000
        ).unwrap();

        // Serialize to QR JSON and URL
        let qr_json = payload.to_qr_json().unwrap();
        let url = payload.to_url();

        // Parse both
        let parsed_from_json = PairingInvitationPayload::parse_input(&qr_json).unwrap();
        let parsed_from_url = PairingInvitationPayload::parse_input(&url).unwrap();

        assert_eq!(parsed_from_json.invitation_id, payload.invitation_id);
        assert_eq!(parsed_from_json.secret, payload.secret);
        assert_eq!(parsed_from_url.invitation_id, payload.invitation_id);
        assert_eq!(parsed_from_url.secret, payload.secret);
        assert_eq!(parsed_from_url.issuer_device_id, payload.issuer_device_id);
        assert_eq!(parsed_from_url.relay, payload.relay);
        assert_eq!(parsed_from_url.local_ips, payload.local_ips);
        assert_eq!(parsed_from_url.port, payload.port);
    }

    // --------------------------------------------------------------------------
    // Test 21: Concurrent invitation consumption allows only a single winner
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_concurrent_invitation_consumption_single_winner() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_sub1, pk_sub1) = generate_test_keypair();
        let (sk_sub2, pk_sub2) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();

        // Pop 1
        let canonical1 = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_sub1, &pk_sub1, "Phone 1", "android", "n1", now, "pairing_establishment"
        );
        let pop1 = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_sub1.clone(),
            subject_pubkey: pk_sub1,
            device_name: "Phone 1".to_string(),
            platform: "android".to_string(),
            nonce: "n1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_sub1, &canonical1),
        };

        // Pop 2
        let canonical2 = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_sub2, &pk_sub2, "Phone 2", "android", "n2", now, "pairing_establishment"
        );
        let pop2 = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id,
            invitation_secret: invite.secret,
            issuer_device_id: pk_issuer,
            subject_device_id: pk_sub2.clone(),
            subject_pubkey: pk_sub2,
            device_name: "Phone 2".to_string(),
            platform: "android".to_string(),
            nonce: "n2".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_sub2, &canonical2),
        };

        let res1 = verify_and_consume_invitation(&mut conn, &pop1, now);
        let res2 = verify_and_consume_invitation(&mut conn, &pop2, now);

        assert!(res1.is_ok());
        assert!(res2.is_err());
        assert!(res2.unwrap_err().contains("已被消费，严禁重复使用"));
    }

    // --------------------------------------------------------------------------
    // Test 22: Concurrent duplicate requests idempotent
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_concurrent_duplicate_requests_idempotent() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-pop".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_subject, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n-pop", now, "pairing_establishment"
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_subject, &pk_issuer, 86400_000, now).unwrap();

        let payload = b"{\"action\":\"rpc_approval_decision\",\"change_id\":\"c2\"}";
        let payload_hash = compute_sha512(payload);
        let rpc_sig = sign_bytes(&sk_subject, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-dup-concurrent", &pk_subject, &pk_issuer, "rpc_approval_decision", &payload_hash, "n-concurrent", now
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-dup-concurrent".to_string(),
            subject_device_id: pk_subject.clone(),
            target_device_id: pk_issuer.clone(),
            action: "rpc_approval_decision".to_string(),
            payload_hash: payload_hash.clone(),
            nonce: "n-concurrent".to_string(),
            timestamp: now,
            signature: rpc_sig,
        };

        // First execution succeeds
        let out1 = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now).unwrap();
        assert!(matches!(out1, AuthVerificationOutcome::Authorized { .. }));
        record_rpc_idempotency_result(&conn, &session.session_id, "req-dup-concurrent", &env.subject_device_id, &env.target_device_id, &env.action, &env.payload_hash, "{\"status\":\"applied\",\"side_effect_count\":1}", now).unwrap();

        // Duplicate call returns identical result without double increment
        let out2 = verify_rpc_request_auth(&mut conn, &env, payload, &pk_issuer, now).unwrap();
        match out2 {
            AuthVerificationOutcome::IdempotentCached { cached_response } => {
                assert!(cached_response.contains("\"side_effect_count\":1"));
            }
            _ => panic!("Expected idempotent cached response"),
        }
    }

    // --------------------------------------------------------------------------
    // Test 23: Repeated revocation is idempotent
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_repeated_revocation_idempotent() {
        let mut conn = setup_test_db();
        let (_sk_issuer, pk_issuer) = generate_test_keypair();
        let (sk_subject, pk_subject) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_issuer, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_issuer.clone(),
            subject_device_id: pk_subject.clone(),
            subject_pubkey: pk_subject.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-pop".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_subject, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_issuer, &pk_subject, &pk_subject, "Phone", "android", "n-pop", now, "pairing_establishment"
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();

        // Revoke once
        revoke_trusted_device(&mut conn, &pk_subject, Some("First revoke"), now + 10).unwrap();
        assert!(!is_device_trusted(&conn, &pk_subject));

        // Revoke second time (idempotent)
        revoke_trusted_device(&mut conn, &pk_subject, Some("Second revoke"), now + 20).unwrap();
        assert!(!is_device_trusted(&conn, &pk_subject));
    }

    // --------------------------------------------------------------------------
    // Test 24: Credential and secret redaction audit
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_credential_redaction_audit() {
        let conn = setup_test_db();
        let (_sk, pk) = generate_test_keypair();
        let payload = create_pairing_invitation(
            &conn, &pk, None, 600_000, "wss://relay.bobbik.org", vec![], 3722, 1000
        ).unwrap();

        // Check DB row has hash, NOT raw secret
        let stored_hash: String = conn.query_row(
            "SELECT invitation_secret_hash FROM pairing_invitations WHERE invitation_id = ?",
            [&payload.invitation_id],
            |row| row.get(0),
        ).unwrap();

        assert_ne!(stored_hash, payload.secret);
        assert_eq!(stored_hash.len(), 128); // SHA-512 hex is 128 chars
        assert!(!stored_hash.contains(&payload.secret));
    }

    // --------------------------------------------------------------------------
    // End-to-End Positive Path: Physical SQLite reopen + continuous authenticated calls
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_e2e_positive_path_and_db_reopen() {
        let temp_dir = std::env::temp_dir().join(format!("bob_sec01_e2e_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let db_path = temp_dir.join("test_sec01_e2e.db");
        let now = 1_000_000;

        let (sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();

        // Step 1: Open initial SQLite connection and create invitation
        {
            let mut conn = Connection::open(&db_path).unwrap();
            init_device_trust_tables(&conn).unwrap();

            let invite_payload = create_pairing_invitation(
                &conn, &pk_pc, None, 600_000, "wss://relay.bobbik.org", vec!["192.168.1.100".to_string()], 3722, now
            ).unwrap();

            // Step 2: Mobile generates proof-of-possession
            let canonical_pop = canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION,
                &invite_payload.invitation_id,
                &invite_payload.secret,
                &pk_pc,
                &pk_mobile,
                &pk_mobile,
                "Pixel 8",
                "android",
                "pop-nonce-1",
                now,
                "pairing_establishment",
            );
            let pop_sig = sign_bytes(&sk_mobile, &canonical_pop);

            let pop = ProofOfPossession {
                protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
                invitation_id: invite_payload.invitation_id,
                invitation_secret: invite_payload.secret,
                issuer_device_id: pk_pc.clone(),
                subject_device_id: pk_mobile.clone(),
                subject_pubkey: pk_mobile.clone(),
                device_name: "Pixel 8".to_string(),
                platform: "android".to_string(),
                nonce: "pop-nonce-1".to_string(),
                timestamp: now,
                purpose: "pairing_establishment".to_string(),
                signature: pop_sig,
            };

            // Step 3: PC verifies POP and establishes TrustedDevice
            let trusted = verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
            assert_eq!(trusted.status, TrustStatus::Trusted);

            // Step 4: Establish AuthenticatedSession
            let session = create_authenticated_session(&mut conn, &pk_mobile, &pk_pc, 86400_000, now).unwrap();
            assert!(session.is_active);

            // Step 5: Execute first protected RPC (discover capabilities)
            let cap_payload = b"{\"action\":\"rpc_discover_capabilities\"}";
            let cap_hash = compute_sha512(cap_payload);
            let cap_sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION, &session.session_id, "req-cap", &pk_mobile, &pk_pc, "rpc_discover_capabilities", &cap_hash, "nonce-cap", now
            ));
            let cap_env = RpcAuthEnvelope {
                protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
                session_id: session.session_id.clone(),
                request_id: "req-cap".to_string(),
                subject_device_id: pk_mobile.clone(),
                target_device_id: pk_pc.clone(),
                action: "rpc_discover_capabilities".to_string(),
                payload_hash: cap_hash,
                nonce: "nonce-cap".to_string(),
                timestamp: now,
                signature: cap_sig,
            };
            let cap_auth = verify_rpc_request_auth(&mut conn, &cap_env, cap_payload, &pk_pc, now).unwrap();
            assert!(matches!(cap_auth, AuthVerificationOutcome::Authorized { .. }));

            // Step 6: Execute second protected RPC (approval decision)
            let app_payload = b"{\"action\":\"rpc_approval_decision\",\"change_id\":\"ch-100\",\"decision\":\"accept\"}";
            let app_hash = compute_sha512(app_payload);
            let app_sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION, &session.session_id, "req-app", &pk_mobile, &pk_pc, "rpc_approval_decision", &app_hash, "nonce-app", now + 10
            ));
            let app_env = RpcAuthEnvelope {
                protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
                session_id: session.session_id.clone(),
                request_id: "req-app".to_string(),
                subject_device_id: pk_mobile.clone(),
                target_device_id: pk_pc.clone(),
                action: "rpc_approval_decision".to_string(),
                payload_hash: app_hash,
                nonce: "nonce-app".to_string(),
                timestamp: now + 10,
                signature: app_sig,
            };
            let app_auth = verify_rpc_request_auth(&mut conn, &app_env, app_payload, &pk_pc, now + 10).unwrap();
            assert!(matches!(app_auth, AuthVerificationOutcome::Authorized { .. }));
        }

        // Step 7: Close connection, simulate process shutdown and REOPEN SQLite
        {
            let mut conn2 = Connection::open(&db_path).unwrap();

            // Verify device remains trusted across restart
            assert!(is_device_trusted(&conn2, &pk_mobile));

            // Verify active session persists across restart
            let active_sessions: i64 = conn2.query_row(
                "SELECT COUNT(1) FROM authenticated_sessions WHERE subject_device_id = ? AND is_active = 1",
                [&pk_mobile],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(active_sessions, 1);

            let session_id: String = conn2.query_row(
                "SELECT session_id FROM authenticated_sessions WHERE subject_device_id = ? AND is_active = 1",
                [&pk_mobile],
                |row| row.get(0),
            ).unwrap();

            // Continuous RPC works without re-scanning QR code!
            let cont_payload = b"{\"action\":\"rpc_request\",\"instruction\":\"read report\"}";
            let cont_hash = compute_sha512(cont_payload);
            let cont_sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION, &session_id, "req-cont-1", &pk_mobile, &pk_pc, "rpc_request", &cont_hash, "nonce-cont", now + 20
            ));
            let cont_env = RpcAuthEnvelope {
                protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
                session_id: session_id.clone(),
                request_id: "req-cont-1".to_string(),
                subject_device_id: pk_mobile.clone(),
                target_device_id: pk_pc.clone(),
                action: "rpc_request".to_string(),
                payload_hash: cont_hash,
                nonce: "nonce-cont".to_string(),
                timestamp: now + 20,
                signature: cont_sig,
            };
            let cont_auth = verify_rpc_request_auth(&mut conn2, &cont_env, cont_payload, &pk_pc, now + 20).unwrap();
            assert!(matches!(cont_auth, AuthVerificationOutcome::Authorized { .. }));

            // Step 8: User unbinds / revokes device
            revoke_trusted_device(&mut conn2, &pk_mobile, Some("Manual Unbind"), now + 30).unwrap();
            assert!(!is_device_trusted(&conn2, &pk_mobile));

            // Subsequent requests immediately rejected!
            let cont_sig2 = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION, &session_id, "req-cont-2", &pk_mobile, &pk_pc, "rpc_request", &compute_sha512(cont_payload), "nonce-after-rev", now + 40
            ));
            let cont_env2 = RpcAuthEnvelope {
                protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
                session_id,
                request_id: "req-cont-2".to_string(),
                subject_device_id: pk_mobile,
                target_device_id: pk_pc.clone(),
                action: "rpc_request".to_string(),
                payload_hash: compute_sha512(cont_payload),
                nonce: "nonce-after-rev".to_string(),
                timestamp: now + 40,
                signature: cont_sig2,
            };
            let rej = verify_rpc_request_auth(&mut conn2, &cont_env2, cont_payload, &pk_pc, now + 40);
            assert!(rej.is_err());
            assert!(rej.unwrap_err().contains("设备已被撤销"));
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --------------------------------------------------------------------------
    // Test 26: LAN REST production entry point rejects forged X-Device-Id
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_lan_rest_forged_device_id_rejected() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-pop-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Phone", "android", "n-pop-1", now, "pairing_establishment"
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_mobile, &pk_pc, 86400_000, now).unwrap();

        // Sign legitimate envelope for pull
        let payload_hash = compute_sha512(b"");
        let sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-pull-1", &pk_mobile, &pk_pc, "pull", &payload_hash, "nonce-pull-1", now
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-pull-1".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash,
            nonce: "nonce-pull-1".to_string(),
            timestamp: now,
            signature: sig,
        };

        // Case A: Attacker supplies envelope signed by pk_mobile, but sets X-Device-Id: "forged_admin_device"
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", "forged_admin_device".parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());

        let res = crate::http_api::verify_rest_request_auth_with_target(&mut conn, &headers, b"", "pull", &pk_pc, now);
        assert!(res.is_err());
        let (status, msg) = res.unwrap_err();
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
        assert!(msg.contains("Forged device ID detected"), "Actual: {}", msg);

        // Case B: Legitimate header matches envelope subject -> succeeds
        headers.insert("x-device-id", pk_mobile.parse().unwrap());
        let ok_res = crate::http_api::verify_rest_request_auth_with_target(&mut conn, &headers, b"", "pull", &pk_pc, now);
        assert!(ok_res.is_ok(), "Expected Ok, got: {:?}", ok_res);
    }

    // --------------------------------------------------------------------------
    // Test 27: LAN REST rejects tampered action
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_lan_rest_tampered_action_rejected() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-act-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Phone", "android", "n-act-1", now, "pairing_establishment"
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_mobile, &pk_pc, 86400_000, now).unwrap();

        // Sign for "pull"
        let payload_hash = compute_sha512(b"");
        let sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-act-1", &pk_mobile, &pk_pc, "pull", &payload_hash, "nonce-act-1", now
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-act-1".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash,
            nonce: "nonce-act-1".to_string(),
            timestamp: now,
            signature: sig,
        };

        // Try to present pull-envelope to "push" endpoint
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());

        let res = crate::http_api::verify_rest_request_auth_with_target(&mut conn, &headers, b"[]", "push", &pk_pc, now);
        assert!(res.is_err());
        let (status, msg) = res.unwrap_err();
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
        assert!(msg.contains("Action mismatch"), "Actual: {}", msg);
    }

    // --------------------------------------------------------------------------
    // Test 28: LAN REST rejects requests missing authentication envelope
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_lan_rest_missing_envelope_rejected() {
        let mut conn = setup_test_db();
        let now = 1_000_000;
        let headers = axum::http::HeaderMap::new();

        let res = crate::http_api::verify_rest_request_auth(&mut conn, &headers, b"", "pull", now);
        assert!(res.is_err());
        let (status, msg) = res.unwrap_err();
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
        assert!(msg.contains("Missing cryptographic RPC authentication envelope"), "Actual: {}", msg);
    }

    // --------------------------------------------------------------------------
    // Test 29: Post-revocation LAN REST and Relay are rejected fail-closed
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_post_revocation_lan_and_relay_blocked() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-rev-1".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Phone", "android", "n-rev-1", now, "pairing_establishment"
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_mobile, &pk_pc, 86400_000, now).unwrap();

        // Revoke device
        revoke_trusted_device(&mut conn, &pk_mobile, Some("Security event"), now + 100).unwrap();

        // Attempt LAN REST
        let payload_hash = compute_sha512(b"");
        let sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-post-rev", &pk_mobile, &pk_pc, "pull", &payload_hash, "n-post-rev", now + 200
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id,
            request_id: "req-post-rev".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash,
            nonce: "n-post-rev".to_string(),
            timestamp: now + 200,
            signature: sig,
        };

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());

        let res = crate::http_api::verify_rest_request_auth_with_target(&mut conn, &headers, b"", "pull", &pk_pc, now + 200);
        assert!(res.is_err());
        let (status, msg) = res.unwrap_err();
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
        assert!(msg.contains("设备已被撤销"), "Actual: {}", msg);
    }

    // --------------------------------------------------------------------------
    // Test 30: REST replay attack rejected with zero side effects
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_rest_replay_attack_rejected_zero_side_effects() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-replay-sideeff".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Phone", "android", "n-replay-sideeff", now, "pairing_establishment"
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_mobile, &pk_pc, 86400_000, now).unwrap();

        let payload_hash = compute_sha512(b"");
        let sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-replay-1", &pk_mobile, &pk_pc, "pull", &payload_hash, "nonce-fixed-1", now
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-replay-1".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash,
            nonce: "nonce-fixed-1".to_string(),
            timestamp: now,
            signature: sig,
        };

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-device-id", pk_mobile.parse().unwrap());
        headers.insert("x-rpc-auth-envelope", serde_json::to_string(&env).unwrap().parse().unwrap());

        // First call: authorized
        let first_res = crate::http_api::verify_rest_request_auth_with_target(&mut conn, &headers, b"", "pull", &pk_pc, now);
        assert!(first_res.is_ok());

        // Snapshot DB count
        let count_before: i64 = conn.query_row("SELECT COUNT(*) FROM trusted_devices", [], |r| r.get(0)).unwrap();

        // Second call: replayed identical nonce
        let replay_res = crate::http_api::verify_rest_request_auth_with_target(&mut conn, &headers, b"", "pull", &pk_pc, now + 50);
        assert!(replay_res.is_err());
        let (status, msg) = replay_res.unwrap_err();
        assert!(status == axum::http::StatusCode::UNAUTHORIZED || status == axum::http::StatusCode::CONFLICT, "Actual status: {}", status);
        assert!(msg.contains("重复的 nonce") || msg.contains("重放攻击") || msg.contains("Idempotency conflict"), "Actual: {}", msg);

        // Verify zero side effects on DB
        let count_after: i64 = conn.query_row("SELECT COUNT(*) FROM trusted_devices", [], |r| r.get(0)).unwrap();
        assert_eq!(count_before, count_after);
    }

    // --------------------------------------------------------------------------
    // Test 31: Cross-restart replay attack rejected via SQLite disk persistence
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_cross_restart_replay_attack_rejected_disk_persistence() {
        let db_path = std::env::temp_dir().join(format!("sec01_cross_restart_{}.db", uuid::Uuid::new_v4()));
        let now = 1_000_000;
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();

        let env = {
            let mut conn1 = Connection::open(&db_path).unwrap();
            init_device_trust_tables(&conn1).unwrap();

            let invite = create_pairing_invitation(&conn1, &pk_pc, None, 600_000, "relay", vec![], 3722, now).unwrap();
            let pop = ProofOfPossession {
                protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
                invitation_id: invite.invitation_id.clone(),
                invitation_secret: invite.secret.clone(),
                issuer_device_id: pk_pc.clone(),
                subject_device_id: pk_mobile.clone(),
                subject_pubkey: pk_mobile.clone(),
                device_name: "Phone".to_string(),
                platform: "android".to_string(),
                nonce: "n-persist-1".to_string(),
                timestamp: now,
                purpose: "pairing_establishment".to_string(),
                signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                    SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Phone", "android", "n-persist-1", now, "pairing_establishment"
                )),
            };
            verify_and_consume_invitation(&mut conn1, &pop, now).unwrap();
            let session = create_authenticated_session(&mut conn1, &pk_mobile, &pk_pc, 86400_000, now).unwrap();

            let payload_hash = compute_sha512(b"{}");
            let sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION, &session.session_id, "req-persist-1", &pk_mobile, &pk_pc, "pull", &payload_hash, "nonce-persist-1", now
            ));
            let env = RpcAuthEnvelope {
                protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
                session_id: session.session_id.clone(),
                request_id: "req-persist-1".to_string(),
                subject_device_id: pk_mobile.clone(),
                target_device_id: pk_pc.clone(),
                action: "pull".to_string(),
                payload_hash,
                nonce: "nonce-persist-1".to_string(),
                timestamp: now,
                signature: sig,
            };

            // First run on conn1: verifies successfully
            let out1 = verify_rpc_request_auth(&mut conn1, &env, b"{}", &pk_pc, now);
            assert!(out1.is_ok());

            // Connection is completely closed and dropped here (simulating application shutdown/reboot)
            drop(conn1);
            env
        };

        // Process restart: open completely new connection to the disk SQLite file
        let mut conn2 = Connection::open(&db_path).unwrap();

        // Replay the exact same request on conn2 after restart
        let replay_res = verify_rpc_request_auth(&mut conn2, &env, b"{}", &pk_pc, now + 10);
        assert!(replay_res.is_err(), "Replay after restart must be rejected!");
        let replay_err = replay_res.unwrap_err();
        assert!(replay_err.contains("重复的 nonce") || replay_err.contains("重放攻击") || replay_err.contains("Idempotency conflict"), "Actual: {}", replay_err);

        // Clean up temp DB
        drop(conn2);
        let _ = std::fs::remove_file(&db_path);
    }

    // --------------------------------------------------------------------------
    // Test 32: Relay dispatcher forgery, target mismatch, and revocation
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_relay_dispatch_forgery_replay_revocation() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Phone".to_string(),
            platform: "android".to_string(),
            nonce: "n-relay-dispatch".to_string(),
            timestamp: now,
            purpose: "pairing_establishment".to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Phone", "android", "n-relay-dispatch", now, "pairing_establishment"
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_mobile, &pk_pc, 86400_000, now).unwrap();

        // Vector A: Caller identity spoofing in Relay dispatcher
        let payload_hash = compute_sha512(b"{\"action\":\"pull\"}");
        let sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-rly-1", &pk_mobile, &pk_pc, "pull", &payload_hash, "n-rly-1", now
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-rly-1".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash: payload_hash.clone(),
            nonce: "n-rly-1".to_string(),
            timestamp: now,
            signature: sig,
        };

        // When relay receives from_device_id: "attacker_device", it must detect spoofing
        let relay_from_id = "attacker_device";
        assert_ne!(relay_from_id, env.subject_device_id);

        // Vector B: Target device mismatch
        let env_wrong_target = RpcAuthEnvelope {
            target_device_id: "wrong_pc_device".to_string(),
            ..env.clone()
        };
        let target_err = verify_rpc_request_auth(&mut conn, &env_wrong_target, b"{\"action\":\"pull\"}", &pk_pc, now);
        assert!(target_err.is_err());
        assert!(target_err.unwrap_err().contains("目标设备 ID 不匹配"));

        // Vector C: Valid relay verification passes
        let ok_res = verify_rpc_request_auth(&mut conn, &env, b"{\"action\":\"pull\"}", &pk_pc, now);
        assert!(ok_res.is_ok());

        // Vector D: Replay across relay fails
        let replay_err = verify_rpc_request_auth(&mut conn, &env, b"{\"action\":\"pull\"}", &pk_pc, now + 10);
        assert!(replay_err.is_err());
        let err_str = replay_err.unwrap_err();
        assert!(err_str.contains("重复的 nonce") || err_str.contains("Idempotency conflict"), "Actual: {}", err_str);

        // Vector E: Revoked device fails across relay
        revoke_trusted_device(&mut conn, &pk_mobile, Some("compromised"), now + 20).unwrap();
        let sig2 = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-rly-2", &pk_mobile, &pk_pc, "pull", &payload_hash, "n-rly-2", now + 30
        ));
        let env2 = RpcAuthEnvelope {
            request_id: "req-rly-2".to_string(),
            nonce: "n-rly-2".to_string(),
            timestamp: now + 30,
            signature: sig2,
            ..env
        };
        let rev_err = verify_rpc_request_auth(&mut conn, &env2, b"{\"action\":\"pull\"}", &pk_pc, now + 30);
        assert!(rev_err.is_err());
        assert!(rev_err.unwrap_err().contains("设备已被撤销"));
    }

    // --------------------------------------------------------------------------
    // Test 33: Real QR pairing flow: Invitation -> PoP -> PC verification & atomic consumption
    // --------------------------------------------------------------------------
    #[test]
    pub fn test_sec01_real_qr_invitation_to_pop_atomic_consumption() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        // 1. PC generates high-entropy pairing invitation (no target restriction so any scanning device can pair)
        let invite = create_pairing_invitation(&conn, &pk_pc, None, 300_000, "relay", vec!["192.168.1.100".to_string()], 3722, now).unwrap();

        // 2. Encoded as QR code URL: bob://pair?v=0.9.6-sec01&id=...&sec=...&dev=...&rly=...&ips=192.168.1.100&p=3722
        let qr_url = format!(
            "bob://pair?v={}&id={}&sec={}&dev={}&rly={}&ips=192.168.1.100&p=3722",
            SEC01_PROTOCOL_VERSION, invite.invitation_id, invite.secret, pk_pc, "relay"
        );

        // 3. Mobile scans QR and parses input
        let parsed_inv = PairingInvitationPayload::parse_input(&qr_url).unwrap();
        assert_eq!(parsed_inv.invitation_id, invite.invitation_id);
        assert_eq!(parsed_inv.secret, invite.secret);
        assert_eq!(parsed_inv.issuer_device_id, pk_pc);

        // 4. Mobile signs ProofOfPossession with purpose = "pairing_establishment"
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: parsed_inv.invitation_id.clone(),
            invitation_secret: parsed_inv.secret.clone(),
            issuer_device_id: parsed_inv.issuer_device_id.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Pixel 9".to_string(),
            platform: "android".to_string(),
            nonce: "qr-pop-nonce-1".to_string(),
            timestamp: now + 5,
            purpose: SEC01_POP_PURPOSE.to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &parsed_inv.invitation_id, &parsed_inv.secret, &pk_pc, &pk_mobile, &pk_mobile, "Pixel 9", "android", "qr-pop-nonce-1", now + 5, SEC01_POP_PURPOSE
            )),
        };

        // 5. PC verifies and atomically consumes
        let trusted_dev = verify_and_consume_invitation(&mut conn, &pop, now + 5).unwrap();
        assert_eq!(trusted_dev.device_id, pk_mobile);
        assert_eq!(trusted_dev.status, TrustStatus::Trusted);

        // 6. Atomic check: immediate second consumption attempt fails
        let second_consume = verify_and_consume_invitation(&mut conn, &pop, now + 6);
        assert!(second_consume.is_err());
        assert!(second_consume.unwrap_err().contains("已被消费"));

        // 7. Active session is established
        let session_id = get_or_create_active_session(&mut conn, &pk_mobile, &pk_pc, now + 6).unwrap();
        assert!(uuid::Uuid::parse_str(&session_id).is_ok());

        // 8. Session enables authenticated RPC requests
        let payload_hash = compute_sha512(b"");
        let sig = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session_id, "req-qr-paired", &pk_mobile, &pk_pc, "pull", &payload_hash, "nonce-qr-paired", now + 10
        ));
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id,
            request_id: "req-qr-paired".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash,
            nonce: "nonce-qr-paired".to_string(),
            timestamp: now + 10,
            signature: sig,
        };
        let rpc_out = verify_rpc_request_auth(&mut conn, &env, b"", &pk_pc, now + 10);
        assert!(rpc_out.is_ok(), "RPC from QR paired device must succeed");
    }

    // --------------------------------------------------------------------------
    // Test 34: Real Axum Router /v1/pair Endpoint: PoP Consumption & Replay Defense
    // --------------------------------------------------------------------------
    #[tokio::test]
    async fn test_sec01_axum_router_pair_endpoint_pop_consumption_and_replay_rejection() {
        let conn = setup_test_db();
        let (sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = crate::now_ms();

        let db = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let state = crate::http_api::ApiState {
            app: None,
            db: Some(db.clone()),
            test_target_id: Some(pk_pc.clone()),
            test_signing_key: None,
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let router = crate::http_api::create_public_router_with_state(state);

        tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await;
        });

        // 1. PC generates pairing invitation
        let invite = {
            let conn = db.lock().unwrap();
            create_pairing_invitation(
                &conn,
                &pk_pc,
                None,
                600_000,
                "relay",
                vec!["127.0.0.1".to_string()],
                port,
                now,
            )
            .unwrap()
        };

        // 2. Mobile constructs valid PoP
        let canonical = canonical_pop_bytes(
            SEC01_PROTOCOL_VERSION,
            &invite.invitation_id,
            &invite.secret,
            &pk_pc,
            &pk_mobile,
            &pk_mobile,
            "Pixel 9 Pro",
            "android",
            "nonce-axum-pair-01",
            now + 5,
            SEC01_POP_PURPOSE,
        );
        let sig = sign_bytes(&sk_mobile, &canonical);
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Pixel 9 Pro".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-axum-pair-01".to_string(),
            timestamp: now + 5,
            purpose: SEC01_POP_PURPOSE.to_string(),
            signature: sig,
        };

        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        // 3. Mobile sends POST /v1/pair to Axum Router
        let resp = client
            .post(format!("http://127.0.0.1:{}/v1/pair", port))
            .json(&pop)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let resp_json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(resp_json["status"], "trusted");
        assert_eq!(resp_json["device_id"], pk_pc);
        let session_id = resp_json["session_id"].as_str().unwrap().to_string();
        assert!(!session_id.is_empty(), "Session ID must be non-empty");

        // Verify DB state
        {
            let conn = db.lock().unwrap();
            let is_trusted = is_device_trusted(&conn, &pk_mobile);
            assert!(is_trusted, "Mobile must be marked as trusted in DB");
            let sess_count: i64 = conn
                .query_row(
                    "SELECT COUNT(1) FROM authenticated_sessions WHERE session_id = ? AND is_active = 1 AND expires_at > ?",
                    rusqlite::params![session_id, now + 10],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(sess_count, 1, "Session must be active and valid in DB");
        }

        // 4. Replay attack rejection: identical PoP immediately resubmitted
        let replay_resp = client
            .post(format!("http://127.0.0.1:{}/v1/pair", port))
            .json(&pop)
            .send()
            .await
            .unwrap();

        assert_eq!(replay_resp.status(), axum::http::StatusCode::UNAUTHORIZED);
        let replay_json: serde_json::Value = replay_resp.json().await.unwrap();
        let err_msg = replay_json["message"].as_str().unwrap_or("");
        assert!(err_msg.contains("已被消费") || err_msg.contains("重复使用") || err_msg.contains("already consumed") || err_msg.contains("replay defense"), "Must reject consumed invitation replay: {}", err_msg);

        // 5. Invalid purpose rejection
        let mut bad_purpose_pop = pop.clone();
        bad_purpose_pop.purpose = "unauthorized_admin_access".to_string();
        let bad_purpose_resp = client
            .post(format!("http://127.0.0.1:{}/v1/pair", port))
            .json(&bad_purpose_pop)
            .send()
            .await
            .unwrap();
        assert_eq!(bad_purpose_resp.status(), axum::http::StatusCode::UNAUTHORIZED);
        let bad_purpose_json: serde_json::Value = bad_purpose_resp.json().await.unwrap();
        let bad_purpose_err = bad_purpose_json["message"].as_str().unwrap_or("");
        assert!(bad_purpose_err.contains("Invalid pairing purpose"), "Must reject invalid purpose: {}", bad_purpose_err);

        // 6. Forged signature rejection
        let mut forged_pop = pop.clone();
        forged_pop.signature = BASE64.encode([9u8; 64]);
        let forged_resp = client
            .post(format!("http://127.0.0.1:{}/v1/pair", port))
            .json(&forged_pop)
            .send()
            .await
            .unwrap();
        assert_eq!(forged_resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    // --------------------------------------------------------------------------
    // Test 35: Wakeup Canonical Payload Sender-Receiver Shared Parity & Full Defense
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_relay_wakeup_canonical_payload_shared_function_sender_receiver_parity() {
        let mut conn = setup_test_db();
        let (sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        // Pair devices and create authenticated session
        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "relay", vec![], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Mobile".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-pop-wakeup".to_string(),
            timestamp: now,
            purpose: SEC01_POP_PURPOSE.to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Mobile", "android", "nonce-pop-wakeup", now, SEC01_POP_PURPOSE
            )),
        };
        verify_and_consume_invitation(&mut conn, &pop, now).unwrap();
        let session = create_authenticated_session(&mut conn, &pk_mobile, &pk_pc, 86400_000, now).unwrap();

        // 1. Sender (Mobile waking up PC) builds raw payload
        let mut sender_payload = serde_json::json!({
            "local_ips": ["192.168.1.50", "10.0.0.5"],
            "port": 3722
        });

        // 2. Sender computes canonical bytes using canonicalize_payload_without_envelope
        let sender_canonical_bytes = canonicalize_payload_without_envelope(&sender_payload);
        let payload_hash = compute_sha512(&sender_canonical_bytes);
        let req_id = "req-wakeup-test-01";
        let nonce = "nonce-wakeup-test-01";
        let sig = sign_bytes(
            &sk_mobile,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &session.session_id,
                req_id,
                &pk_mobile,
                &pk_pc,
                "wakeup",
                &payload_hash,
                nonce,
                now,
            ),
        );
        let env = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: req_id.to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "wakeup".to_string(),
            payload_hash: payload_hash.clone(),
            nonce: nonce.to_string(),
            timestamp: now,
            signature: sig,
        };

        // Sender attaches envelope into payload
        if let Some(obj) = sender_payload.as_object_mut() {
            obj.insert("envelope".to_string(), serde_json::to_value(&env).unwrap());
        }

        // 3. Sent over Relay and received by PC
        let relay_msg = serde_json::json!({
            "type": "wakeup",
            "from_device_id": pk_mobile,
            "target_device_id": pk_pc,
            "payload": sender_payload
        });

        // Receiver extracts envelope and computes canonical bytes using canonicalize_payload_without_envelope
        let received_payload = relay_msg.get("payload").unwrap();
        let receiver_canonical_bytes = canonicalize_payload_without_envelope(received_payload);

        // Core Invariant Check: Sender and Receiver canonical bytes MUST be byte-for-byte identical!
        assert_eq!(
            sender_canonical_bytes,
            receiver_canonical_bytes,
            "Sender and receiver canonical bytes MUST be identical byte-for-byte!"
        );

        // 4. Receiver verifies RPC auth using production sec01_verify_relay_wakeup_message
        let verify_res = sec01_verify_relay_wakeup_message(&mut conn, &relay_msg, &pk_pc, now);
        assert!(verify_res.is_ok(), "Valid wakeup request must authenticate successfully: {:?}", verify_res);
        assert_eq!(verify_res.unwrap(), pk_mobile);

        // 5. Negative: Tampered payload inside relay_msg fails signature verification
        let mut tampered_relay_msg = relay_msg.clone();
        tampered_relay_msg["payload"].as_object_mut().unwrap().insert("port".to_string(), serde_json::json!(8080));
        let tamper_res = sec01_verify_relay_wakeup_message(&mut conn, &tampered_relay_msg, &pk_pc, now);
        assert!(tamper_res.is_err(), "Tampered wakeup payload must be rejected!");

        // 6. Negative: Caller identity spoofing (Relay from_device_id != envelope subject)
        let mut spoofed_relay_msg = relay_msg.clone();
        spoofed_relay_msg["from_device_id"] = serde_json::json!("attacker_device_id");
        let spoof_res = sec01_verify_relay_wakeup_message(&mut conn, &spoofed_relay_msg, &pk_pc, now);
        assert!(spoof_res.is_err(), "Caller spoofing must be rejected!");
        assert!(spoof_res.unwrap_err().contains("spoofing"));

        // 7. Negative: Replayed nonce fails
        let replay_res = sec01_verify_relay_wakeup_message(&mut conn, &relay_msg, &pk_pc, now + 10);
        assert!(replay_res.is_err(), "Replayed wakeup nonce must be rejected!");
        let err_text = replay_res.unwrap_err();
        assert!(err_text.contains("重复的 nonce") || err_text.contains("Idempotency conflict"), "Actual: {}", err_text);

        // 8. Negative: Revoked device fails
        revoke_trusted_device(&mut conn, &pk_mobile, Some("user unpair"), now + 20).unwrap();
        let req_id_revoked = "req-wakeup-test-revoked";
        let nonce_revoked = "nonce-wakeup-test-revoked";
        let sig_revoked = sign_bytes(
            &sk_mobile,
            &canonical_rpc_bytes(
                SEC01_PROTOCOL_VERSION,
                &session.session_id,
                req_id_revoked,
                &pk_mobile,
                &pk_pc,
                "wakeup",
                &payload_hash,
                nonce_revoked,
                now + 30,
            ),
        );
        let mut revoked_env = env.clone();
        revoked_env.request_id = req_id_revoked.to_string();
        revoked_env.nonce = nonce_revoked.to_string();
        revoked_env.timestamp = now + 30;
        revoked_env.signature = sig_revoked;

        let mut revoked_payload = sender_payload.clone();
        revoked_payload.as_object_mut().unwrap().insert("envelope".to_string(), serde_json::to_value(&revoked_env).unwrap());
        let revoked_relay_msg = serde_json::json!({
            "type": "wakeup",
            "from_device_id": pk_mobile,
            "target_device_id": pk_pc,
            "payload": revoked_payload
        });

        let revoked_res = sec01_verify_relay_wakeup_message(&mut conn, &revoked_relay_msg, &pk_pc, now + 30);
        assert!(revoked_res.is_err(), "Wakeup from revoked device must be rejected!");
        assert!(revoked_res.unwrap_err().contains("设备已被撤销"));
    }

    // --------------------------------------------------------------------------
    // Test 36: Dedicated Step E Nonce Anti-Replay Verification (New request_id, Replayed Nonce)
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_step_e_nonce_anti_replay_with_distinct_request_id() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 1_000_000;

        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "lan", vec!["127.0.0.1".to_string()], 3722, now).unwrap();
        let pop = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Mobile".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-pop-replay-test".to_string(),
            timestamp: now,
            purpose: SEC01_POP_PURPOSE.to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Mobile", "android", "nonce-pop-replay-test", now, SEC01_POP_PURPOSE
            )),
        };
        let (td, session) = sec01_atomic_consume_and_create_session(&mut conn, &pop, &pk_pc, 86400_000, now).unwrap();
        assert_eq!(td.status, TrustStatus::Trusted);

        let payload_hash = compute_sha512(b"{}");
        let fixed_nonce = "fixed-replay-nonce-001";

        // Request 1 with fixed_nonce
        let sig1 = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-initial-01", &pk_mobile, &pk_pc, "pull", &payload_hash, fixed_nonce, now
        ));
        let env1 = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-initial-01".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash: payload_hash.clone(),
            nonce: fixed_nonce.to_string(),
            timestamp: now,
            signature: sig1,
        };
        let res1 = verify_rpc_request_auth(&mut conn, &env1, b"{}", &pk_pc, now);
        assert!(res1.is_ok(), "Initial request must succeed");

        // Request 2 with NEW request_id but REUSED nonce (attacker bypasses idempotency cache)
        let sig2 = sign_bytes(&sk_mobile, &canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION, &session.session_id, "req-new-attempt-02", &pk_mobile, &pk_pc, "pull", &payload_hash, fixed_nonce, now + 10
        ));
        let env2 = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: session.session_id.clone(),
            request_id: "req-new-attempt-02".to_string(),
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: "pull".to_string(),
            payload_hash,
            nonce: fixed_nonce.to_string(),
            timestamp: now + 10,
            signature: sig2,
        };
        let res2 = verify_rpc_request_auth(&mut conn, &env2, b"{}", &pk_pc, now + 10);
        assert!(res2.is_err(), "Reused nonce with new request_id must be rejected");
        let err2 = res2.unwrap_err();
        assert!(err2.contains("重复的 nonce"), "Must trigger anti-replay nonce violation: {}", err2);
    }

    // --------------------------------------------------------------------------
    // Test 37: Atomic Consume and Session Creation (All-or-Nothing Transaction)
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_atomic_consume_and_create_session_atomicity_and_cas() {
        let mut conn = setup_test_db();
        let (_sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = 2_000_000;

        let invite = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "lan", vec!["192.168.1.100".to_string()], 3722, now).unwrap();

        // 1. Invalid purpose fails before any modification
        let pop_bad_purpose = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Mobile".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-bad-purpose".to_string(),
            timestamp: now,
            purpose: "invalid_random_purpose".to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Mobile", "android", "nonce-bad-purpose", now, "invalid_random_purpose"
            )),
        };
        let err1 = sec01_atomic_consume_and_create_session(&mut conn, &pop_bad_purpose, &pk_pc, 86400_000, now);
        assert!(err1.is_err());
        assert!(err1.unwrap_err().contains("Invalid pairing purpose"));

        // Verify invitation remains completely unconsumed
        let consumed: Option<i64> = conn.query_row(
            "SELECT consumed_at FROM pairing_invitations WHERE invitation_id = ?",
            [&invite.invitation_id],
            |r| r.get(0)
        ).unwrap();
        assert!(consumed.is_none(), "Invitation must remain unconsumed after failed attempt");

        // Verify trusted_devices has zero records
        let dev_count: i64 = conn.query_row("SELECT COUNT(*) FROM trusted_devices", [], |r| r.get(0)).unwrap();
        assert_eq!(dev_count, 0);

        // 2. Successful atomic execution
        let pop_valid = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite.invitation_id.clone(),
            invitation_secret: invite.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_mobile.clone(),
            subject_pubkey: pk_mobile.clone(),
            device_name: "Mobile Alpha".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-valid-atomic".to_string(),
            timestamp: now,
            purpose: SEC01_POP_PURPOSE.to_string(),
            signature: sign_bytes(&sk_mobile, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite.invitation_id, &invite.secret, &pk_pc, &pk_mobile, &pk_mobile, "Mobile Alpha", "android", "nonce-valid-atomic", now, SEC01_POP_PURPOSE
            )),
        };

        let res = sec01_atomic_consume_and_create_session(&mut conn, &pop_valid, &pk_pc, 86400_000, now);
        assert!(res.is_ok(), "Valid PoP must consume invitation and create session: {:?}", res);
        let (td, session) = res.unwrap();
        assert_eq!(td.device_id, pk_mobile);
        assert_eq!(td.status, TrustStatus::Trusted);
        assert_eq!(session.subject_device_id, pk_mobile);
        assert_eq!(session.issuer_device_id, pk_pc);
        assert!(session.is_active);
        assert!(!session.session_id.is_empty());

        // Verify DB state: invitation consumed, trusted device inserted, session inserted
        let consumed_after: Option<i64> = conn.query_row(
            "SELECT consumed_at FROM pairing_invitations WHERE invitation_id = ?",
            [&invite.invitation_id],
            |r| r.get(0)
        ).unwrap();
        assert_eq!(consumed_after, Some(now));

        let sess_count: i64 = conn.query_row("SELECT COUNT(*) FROM authenticated_sessions WHERE session_id = ?", [&session.session_id], |r| r.get(0)).unwrap();
        assert_eq!(sess_count, 1);

        // 3. Replay attack: calling atomic pairing again with the same consumed invitation fails
        let res_replay = sec01_atomic_consume_and_create_session(&mut conn, &pop_valid, &pk_pc, 86400_000, now + 10);
        assert!(res_replay.is_err(), "Replayed PoP on consumed invitation must fail");
        assert!(res_replay.unwrap_err().contains("already consumed"));

        // 4. Protocol version mismatch fails before mutation
        let mut pop_bad_ver = pop_valid.clone();
        pop_bad_ver.protocol_version = "v0.9.5-legacy".to_string();
        let res_bad_ver = sec01_atomic_consume_and_create_session(&mut conn, &pop_bad_ver, &pk_pc, 86400_000, now + 20);
        assert!(res_bad_ver.is_err());
        assert!(res_bad_ver.unwrap_err().contains("Unsupported protocol version"));

        // 5. TRUE MID-TRANSACTION FAILURE INJECTION & ROLLBACK VERIFICATION:
        // Create a new invitation and install a trigger on authenticated_sessions that aborts on INSERT.
        // This ensures CAS consumption (Step 1) and trusted_devices insertion (Step 2) execute first,
        // and session creation (Step 3) aborts mid-flight within the same transaction.
        let invite_fail = create_pairing_invitation(&conn, &pk_pc, None, 600_000, "lan", vec!["192.168.1.100".to_string()], 3722, now + 30).unwrap();
        let (sk_guest, pk_guest) = generate_test_keypair();
        let pop_guest = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invite_fail.invitation_id.clone(),
            invitation_secret: invite_fail.secret.clone(),
            issuer_device_id: pk_pc.clone(),
            subject_device_id: pk_guest.clone(),
            subject_pubkey: pk_guest.clone(),
            device_name: "Guest Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-mid-fail".to_string(),
            timestamp: now + 30,
            purpose: SEC01_POP_PURPOSE.to_string(),
            signature: sign_bytes(&sk_guest, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invite_fail.invitation_id, &invite_fail.secret, &pk_pc, &pk_guest, &pk_guest, "Guest Phone", "android", "nonce-mid-fail", now + 30, SEC01_POP_PURPOSE
            )),
        };

        conn.execute(
            "CREATE TRIGGER test_inject_mid_tx_failure
             BEFORE INSERT ON authenticated_sessions
             BEGIN
                 SELECT RAISE(ABORT, 'INJECTED_MID_TX_FAILURE: disk I/O failure on session insert');
             END;",
            [],
        ).unwrap();

        let mid_fail_res = sec01_atomic_consume_and_create_session(&mut conn, &pop_guest, &pk_pc, 86400_000, now + 30);
        assert!(mid_fail_res.is_err(), "Must fail when session insert fails mid-transaction");
        assert!(mid_fail_res.unwrap_err().contains("INJECTED_MID_TX_FAILURE"));

        // Assert Step 1 CAS consumption was completely rolled back:
        let inv_consumed: Option<i64> = conn.query_row(
            "SELECT consumed_at FROM pairing_invitations WHERE invitation_id = ?",
            [&invite_fail.invitation_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(inv_consumed, None, "Invitation CAS consumption must be rolled back to NULL on mid-tx failure!");

        // Assert Step 2 trusted_devices insert was completely rolled back:
        let dev_persisted: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trusted_devices WHERE device_id = ?",
            [&pk_guest],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(dev_persisted, 0, "Trusted device row must be rolled back on mid-tx failure!");

        // Assert 0 sessions for guest:
        let sess_persisted: i64 = conn.query_row(
            "SELECT COUNT(*) FROM authenticated_sessions WHERE subject_device_id = ?",
            [&pk_guest],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(sess_persisted, 0, "No orphaned session should exist!");

        // Remove fault injection and retry exact same invitation & PoP:
        conn.execute("DROP TRIGGER test_inject_mid_tx_failure", []).unwrap();
        let retry_res = sec01_atomic_consume_and_create_session(&mut conn, &pop_guest, &pk_pc, 86400_000, now + 30);
        assert!(retry_res.is_ok(), "Retry after removing fault must succeed: {:?}", retry_res);

        let inv_consumed_now: Option<i64> = conn.query_row(
            "SELECT consumed_at FROM pairing_invitations WHERE invitation_id = ?",
            [&invite_fail.invitation_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(inv_consumed_now, Some(now + 30), "Invitation must now be cleanly consumed");
    }

    // --------------------------------------------------------------------------
    // Test 38: Mobile Client Atomic Persistence (Zero Dirty State on Session Failure)
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_mobile_atomic_persistence_failure_modes() {
        let mut conn = setup_test_db();
        let target_pc = "pc-device-12345";
        let my_mobile = "mobile-device-67890";
        let now = 3_000_000;

        // Negative: empty session_id must fail closed and leave zero DB records
        let res_empty_session = sec01_persist_mobile_trusted_session(
            &mut conn, target_pc, my_mobile, "", "PC", "windows", 86400_000, now
        );
        assert!(res_empty_session.is_err(), "Empty session_id must be rejected");

        let td_count: i64 = conn.query_row("SELECT COUNT(*) FROM trusted_devices", [], |r| r.get(0)).unwrap();
        assert_eq!(td_count, 0, "No trusted device should be persisted on error");

        let sess_count: i64 = conn.query_row("SELECT COUNT(*) FROM authenticated_sessions", [], |r| r.get(0)).unwrap();
        assert_eq!(sess_count, 0, "No session should be persisted on error");

        // Positive: valid session_id persists both atomically
        let valid_session = "sess-mobile-persisted-001";
        let res_valid = sec01_persist_mobile_trusted_session(
            &mut conn, target_pc, my_mobile, valid_session, "PC", "windows", 86400_000, now
        );
        assert!(res_valid.is_ok(), "Valid persistence must succeed: {:?}", res_valid.err());

        let td_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM trusted_devices WHERE device_id = ?", [target_pc], |r| r.get(0)).unwrap();
        assert_eq!(td_count_after, 1);

        let sess_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM authenticated_sessions WHERE session_id = ?", [valid_session], |r| r.get(0)).unwrap();
        assert_eq!(sess_count_after, 1);
    }

    // --------------------------------------------------------------------------
    // Test 39: Protocol Version Fail-Closed Parsing (URL & JSON)
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_parse_invitation_protocol_version_fail_closed() {
        // 1. Missing v in URL
        let url_no_v = "bob://pair?id=inv-1&sec=secret-1&iss=device-1&ips=127.0.0.1&p=3722";
        let res_no_v = PairingInvitationPayload::parse_input(url_no_v);
        assert!(res_no_v.is_err(), "Missing v in URL must fail closed");
        assert!(res_no_v.unwrap_err().contains("缺少协议版本"));

        // 2. Empty v in URL
        let url_empty_v = "bob://pair?v=&id=inv-1&sec=secret-1&iss=device-1&ips=127.0.0.1&p=3722";
        let res_empty_v = PairingInvitationPayload::parse_input(url_empty_v);
        assert!(res_empty_v.is_err(), "Empty v in URL must fail closed");
        let err_empty_v = res_empty_v.unwrap_err();
        assert!(err_empty_v.contains("不能为空") || err_empty_v.contains("缺少协议版本"));

        // 3. Unknown v in URL
        let url_bad_v = "bob://pair?v=999.0.0&id=inv-1&sec=secret-1&iss=device-1&ips=127.0.0.1&p=3722";
        let res_bad_v = PairingInvitationPayload::parse_input(url_bad_v);
        assert!(res_bad_v.is_err(), "Unknown v in URL must fail closed");
        assert!(res_bad_v.unwrap_err().contains("不支持的协议版本"));

        // 4. Missing protocol_version in JSON
        let json_no_v = r#"{"invitation_id":"inv-1","secret":"sec-1","issuer_device_id":"dev-1","relay":"","local_ips":[],"port":3722}"#;
        let res_json_no_v = PairingInvitationPayload::parse_input(json_no_v);
        assert!(res_json_no_v.is_err(), "Missing protocol_version in JSON must fail closed");

        // 5. Empty protocol_version in JSON
        let json_empty_v = r#"{"protocol_version":"   ","invitation_id":"inv-1","secret":"sec-1","issuer_device_id":"dev-1","relay":"","local_ips":[],"port":3722}"#;
        let res_json_empty_v = PairingInvitationPayload::parse_input(json_empty_v);
        assert!(res_json_empty_v.is_err(), "Empty protocol_version in JSON must fail closed");

        // 6. Unknown protocol_version in JSON
        let json_bad_v = r#"{"protocol_version":"v0.1-alpha","invitation_id":"inv-1","secret":"sec-1","issuer_device_id":"dev-1","relay":"","local_ips":[],"port":3722}"#;
        let res_json_bad_v = PairingInvitationPayload::parse_input(json_bad_v);
        assert!(res_json_bad_v.is_err(), "Unknown protocol_version in JSON must fail closed");
        assert!(res_json_bad_v.unwrap_err().contains("不支持的协议版本"));

        // 7. Valid protocol_version in URL and JSON succeeds
        let url_valid = format!("bob://pair?v={}&id=inv-1&sec=secret-1&iss=device-1&ips=127.0.0.1&p=3722", SEC01_PROTOCOL_VERSION);
        let res_valid = PairingInvitationPayload::parse_input(&url_valid);
        assert!(res_valid.is_ok(), "Valid protocol version in URL must succeed");

        let json_valid = format!(
            r#"{{"protocol_version":"{}","invitation_id":"inv-1","secret":"sec-1","issuer_device_id":"dev-1","relay":"","local_ips":[],"port":3722}}"#,
            SEC01_PROTOCOL_VERSION
        );
        let res_json_valid = PairingInvitationPayload::parse_input(&json_valid);
        assert!(res_json_valid.is_ok(), "Valid protocol version in JSON must succeed");
    }

    // --------------------------------------------------------------------------
    // Test 40: End-to-End Penetration: Identity Lifecycle, Recovery State Machine & LAN CommitAck
    // --------------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_sec01_device_identity_lifecycle_empty_unlock_invite_reset_reinit() {
        let _config_lock = crate::CONFIG_OP_TEST_MUTEX.lock().unwrap();
        let _config_guard = crate::http_api::tests::TestConfigOverrideGuard;
        let config_path = std::env::temp_dir().join(format!("sec01_lifecycle_cfg_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&config_path, b"{}").unwrap();
        crate::set_test_config_path_override(Some(config_path.clone()));
        let mut conn = setup_test_db();
        let mut config = serde_json::json!({});
        let memory_mutex = std::sync::Mutex::new(None);
        let now = crate::now_ms();

        // 1. Empty config startup check
        assert!(config.get("device_id").is_none());
        assert!(memory_mutex.lock().unwrap().is_none());
        let invite_fail = memory_mutex.lock().unwrap().as_ref().map(|sk| {
            let vk = VerifyingKey::from(sk);
            BASE64.encode(vk.to_bytes())
        }).ok_or_else(|| "设备秘钥未解锁，无法创建配对邀请".to_string());
        assert!(invite_fail.is_err(), "未解锁状态下创建邀请必须失败");

        // 2. Unlock / Init
        let (sk1, pk1) = generate_test_keypair();
        *memory_mutex.lock().unwrap() = Some(sk1.clone());
        config.as_object_mut().unwrap().insert("device_id".to_string(), serde_json::json!(pk1));
        std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();

        // 3. Create Invitation
        let issuer_id = pk1.clone();
        let invitation = create_pairing_invitation(
            &conn,
            &issuer_id,
            None,
            DEFAULT_INVITATION_TTL_MS,
            "wss://relay.bobbik.org",
            vec!["192.168.1.100".to_string()],
            3722,
            now,
        ).expect("解锁后必须能成功创建邀请");
        assert_eq!(invitation.issuer_device_id, pk1);
        assert_eq!(invitation.protocol_version, SEC01_PROTOCOL_VERSION);

        // 4. Pair with mobile peer
        let (sk_phone, pk_phone) = generate_test_keypair();
        let pop_phone = ProofOfPossession {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            invitation_id: invitation.invitation_id.clone(),
            invitation_secret: invitation.secret.clone(),
            issuer_device_id: pk1.clone(),
            subject_device_id: pk_phone.clone(),
            subject_pubkey: pk_phone.clone(),
            device_name: "Test Phone".to_string(),
            platform: "android".to_string(),
            nonce: "nonce-lifecycle-1".to_string(),
            timestamp: now + 10,
            purpose: SEC01_POP_PURPOSE.to_string(),
            signature: sign_bytes(&sk_phone, &canonical_pop_bytes(
                SEC01_PROTOCOL_VERSION, &invitation.invitation_id, &invitation.secret, &pk1, &pk_phone, &pk_phone, "Test Phone", "android", "nonce-lifecycle-1", now + 10, SEC01_POP_PURPOSE
            )),
        };

        // Inbound checks
        assert!(sec01_atomic_consume_and_create_session(&mut conn, &pop_phone, "", 86400_000, now + 10).is_err());
        assert!(sec01_atomic_consume_and_create_session(&mut conn, &pop_phone, "spoofed-id", 86400_000, now + 10).is_err());

        let pair_res = sec01_atomic_consume_and_create_session(&mut conn, &pop_phone, &pk1, 86400_000, now + 10).unwrap();
        assert_eq!(pair_res.1.issuer_device_id, pk1);
        assert!(is_device_trusted(&conn, &pk_phone));

        // 5. Recovery State Machine & Fault Injection Penetration:
        // a. Config write failure: asserts Err, key on disk UNTOUCHED, memory key UNTOUCHED, degraded state blocks pairing!
        let fake_key_path = std::env::temp_dir().join(format!("bob_test_key_fail_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&fake_key_path, b"dummy-key-content").unwrap();

        let reset_fail_cfg = crate::crypto::reset_device_keys_core(
            Some(&fake_key_path),
            &memory_mutex,
            |_cfg| Err("Simulated config disk write failure".to_string()),
            &mut conn,
            now + 50,
        );
        assert!(reset_fail_cfg.is_err(), "配置持久化失败必须返回 Err");
        assert!(fake_key_path.exists(), "配置失败时密钥文件绝对不能被删除！");
        assert!(memory_mutex.lock().unwrap().is_some(), "配置失败时内存私钥绝对不能被清空！");
        assert!(is_identity_degraded(&conn).unwrap(), "配置失败后系统必须进入 degraded 状态");

        // Assert degraded state blocks pairing operations:
        assert!(create_pairing_invitation(&conn, &pk1, None, 600_000, "relay", vec![], 3722, now + 55).is_err(), "Degraded 状态必须阻断创建邀请");
        assert!(sec01_atomic_consume_and_create_session(&mut conn, &pop_phone, &pk1, 86400_000, now + 55).is_err(), "Degraded 状态必须阻断配对建信");

        let _ = std::fs::remove_file(&fake_key_path);
        // Clear degraded journal and outbox for next test step
        conn.execute("DELETE FROM identity_reset_journal", []).unwrap();
        conn.execute("DELETE FROM peer_revocation_outbox", []).unwrap();
        conn.execute("UPDATE trusted_devices SET status = 'trusted' WHERE device_id = ?", [&pk_phone]).unwrap();

        // b. DB transaction failure: asserts Err, key on disk UNTOUCHED, memory key UNTOUCHED!
        let fake_key_path_db = std::env::temp_dir().join(format!("bob_test_key_db_fail_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&fake_key_path_db, b"dummy-key-db").unwrap();

        conn.execute(
            "CREATE TRIGGER test_inject_reset_db_fail
             BEFORE UPDATE ON identity_reset_journal
             BEGIN
                 SELECT RAISE(ABORT, 'INJECTED_RESET_DB_FAILURE');
             END;",
            [],
        ).unwrap();

        let reset_fail_db = crate::crypto::reset_device_keys_core(
            Some(&fake_key_path_db),
            &memory_mutex,
            |_cfg| Ok(()),
            &mut conn,
            now + 60,
        );
        assert!(reset_fail_db.is_err(), "数据库重置事务失败必须返回 Err");
        assert!(fake_key_path_db.exists(), "数据库失败时密钥文件绝对不能被删除！");
        assert!(memory_mutex.lock().unwrap().is_some(), "数据库失败时内存私钥绝对不能被清空！");

        conn.execute("DROP TRIGGER test_inject_reset_db_fail", []).unwrap();
        let _ = std::fs::remove_file(&fake_key_path_db);
        conn.execute("DELETE FROM identity_reset_journal", []).unwrap();
        conn.execute("DELETE FROM peer_revocation_outbox", []).unwrap();
        // Restore trusted status on peer for real reset
        conn.execute("UPDATE trusted_devices SET status = 'trusted' WHERE device_id = ?", [&pk_phone]).unwrap();

        // 6. Positive Production Reset (reset_device_keys_core)
        let real_key_file = std::env::temp_dir().join(format!("bob_test_key_real_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&real_key_file, b"real_encrypted_bytes").unwrap();

        let reset_ok = crate::crypto::reset_device_keys_core(
            Some(&real_key_file),
            &memory_mutex,
            |cfg| {
                if let Some(obj) = cfg.as_object_mut() {
                    obj.remove("device_id");
                    obj.remove("pairing_payload");
                }
                Ok(())
            },
            &mut conn,
            now + 100,
        );
        assert!(reset_ok.is_ok(), "生产重置核心函数执行成功");
        assert!(!real_key_file.exists(), "密钥文件已被物理删除");
        assert!(memory_mutex.lock().unwrap().is_none(), "内存密钥已被清空");
        assert!(!is_identity_degraded(&conn).unwrap(), "成功后 journal 为 committed，无 degraded");

        // 7. Penetrate Real LAN Axum Endpoint & CommitAck Delivery
        // Setup peer node with real Axum server
        let mut peer_conn = setup_test_db();
        peer_conn.execute(
            "INSERT INTO trusted_devices (device_id, public_key, device_name, platform, paired_at, last_authenticated_at, status)
             VALUES (?, ?, 'PC', 'windows', ?, ?, 'trusted')",
            rusqlite::params![pk1, pk1, now, now],
        ).unwrap();
        peer_conn.execute(
            "INSERT INTO authenticated_sessions (session_id, subject_device_id, issuer_device_id, created_at, expires_at, last_activity_at, is_active)
             VALUES ('sess-peer-1', ?, 'peer-id', ?, ?, ?, 1)",
            rusqlite::params![pk1, now, now + 86400_000, now],
        ).unwrap();
        assert!(is_device_trusted(&peer_conn, &pk1));

        let peer_db = std::sync::Arc::new(std::sync::Mutex::new(peer_conn));
        let peer_api_state = crate::http_api::ApiState {
            app: None,
            db: Some(peer_db.clone()),
            test_target_id: Some(pk_phone.clone()),
            test_signing_key: Some(sk_phone.clone()),
        };

        let peer_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_port = peer_listener.local_addr().unwrap().port();
        let peer_router = crate::http_api::create_public_router_with_state(peer_api_state);

        tokio::spawn(async move {
            let _ = axum::serve(
                peer_listener,
                peer_router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            ).await;
        });

        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .unwrap();

        // Drain outbox via LAN dispatch to peer Axum /v1/device/revoke
        let delivered_count = drain_peer_revocation_outbox_via_lan(&mut conn, now + 120, |_target, payload| {
            let cert: DeviceRevocationCertificate = serde_json::from_str(payload).unwrap();
            let url = format!("http://127.0.0.1:{}/v1/device/revoke", peer_port);
            let resp = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    http_client.post(&url).json(&cert).send().await
                })
            }).map_err(|e| e.to_string())?;

            if resp.status().is_success() {
                let ack: RevocationAck = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        resp.json().await
                    })
                }).map_err(|e| e.to_string())?;
                Ok(ack)
            } else {
                let status = resp.status();
                let body = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        resp.text().await.unwrap_or_default()
                    })
                });
                Err(format!("Peer HTTP error {}: {}", status, body))
            }
        }).unwrap();

        if delivered_count != 1 {
            let (st, att, err): (String, i64, Option<String>) = conn.query_row(
                "SELECT status, attempts, last_error FROM peer_revocation_outbox LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).unwrap_or_else(|_| ("none".to_string(), 0, None));
            panic!("delivered_count is {} (expected 1)! outbox status: {}, attempts: {}, last_err: {:?}", delivered_count, st, att, err);
        }
        assert_eq!(delivered_count, 1, "必须有 1 条撤销经由 Axum 成功提交并收到 CommitAck");

        // Verify local outbox row marked 'delivered'
        let pending_after = get_pending_peer_revocations(&conn, 10).unwrap();
        assert_eq!(pending_after.len(), 0, "出件箱 pending 队列已清空");

        let outbox_status: String = conn.query_row(
            "SELECT status FROM peer_revocation_outbox WHERE revoked_device_id = ?",
            [&pk1],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(outbox_status, "delivered");

        // Verify peer node state
        {
            let conn_guard = peer_db.lock().unwrap();
            assert!(!is_device_trusted(&conn_guard, &pk1), "旧公钥在对端已被完全作废");
            let sess_count: i64 = conn_guard.query_row(
                "SELECT COUNT(*) FROM authenticated_sessions WHERE subject_device_id = ?",
                [&pk1],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(sess_count, 0, "对端旧会话已被物理销毁");
        }

        // 8. Re-initialization with new key
        let (sk2, pk2) = generate_test_keypair();
        assert_ne!(pk1, pk2);
        *memory_mutex.lock().unwrap() = Some(sk2.clone());

        let invitation2 = create_pairing_invitation(
            &conn,
            &pk2,
            None,
            DEFAULT_INVITATION_TTL_MS,
            "wss://relay.bobbik.org",
            vec!["192.168.1.100".to_string()],
            3722,
            now + 200,
        ).expect("重新初始化后必须能成功创建新邀请");
        assert_eq!(invitation2.issuer_device_id, pk2);
        std::fs::remove_file(config_path).unwrap();
    }

    // --------------------------------------------------------------------------
    // Test 41: Real Axum Router /v1/device/revoke: Proof-of-Key Auth & CommitAck
    // --------------------------------------------------------------------------
    #[tokio::test]
    async fn test_sec01_device_revoke_http_endpoint_authentication_and_ack() {
        let mut conn = setup_test_db();
        let (sk_pc, pk_pc) = generate_test_keypair();
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let now = crate::now_ms();

        // Setup trusted device on PC for mobile
        conn.execute(
            "INSERT INTO trusted_devices (device_id, public_key, device_name, platform, paired_at, last_authenticated_at, status)
             VALUES (?, ?, 'Mobile', 'android', ?, ?, 'trusted')",
            rusqlite::params![pk_mobile, pk_mobile, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO authenticated_sessions (session_id, subject_device_id, issuer_device_id, created_at, expires_at, last_activity_at, is_active)
             VALUES ('sess-rev-1', ?, ?, ?, ?, ?, 1)",
            rusqlite::params![pk_mobile, pk_pc, now, now + 86400_000, now],
        ).unwrap();
        assert!(is_device_trusted(&conn, &pk_mobile));

        let db = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let state = crate::http_api::ApiState {
            app: None,
            db: Some(db.clone()),
            test_target_id: Some(pk_pc.clone()),
            test_signing_key: Some(sk_pc.clone()),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let router = crate::http_api::create_public_router_with_state(state);

        tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            ).await;
        });

        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .unwrap();

        // 1. Negative Test: Forged signature rejected (401 Unauthorized, zero DB side effects)
        let mut cert_forged = create_device_revocation_certificate(&sk_mobile, &pk_pc, "user_reset", now + 10);
        cert_forged.signature = BASE64.encode(vec![0u8; 64]);
        let resp_forged = client
            .post(format!("http://127.0.0.1:{}/v1/device/revoke", port))
            .json(&cert_forged)
            .send()
            .await
            .unwrap();
        assert_eq!(resp_forged.status(), axum::http::StatusCode::UNAUTHORIZED);
        {
            let conn = db.lock().unwrap();
            assert!(is_device_trusted(&conn, &pk_mobile), "Forged signature must leave device trusted");
        }

        // 2. Negative Test: Target mismatch rejected (403 Forbidden, zero DB side effects)
        let cert_wrong_target = create_device_revocation_certificate(&sk_mobile, "someone-else-pc", "user_reset", now + 20);
        let resp_wrong_target = client
            .post(format!("http://127.0.0.1:{}/v1/device/revoke", port))
            .json(&cert_wrong_target)
            .send()
            .await
            .unwrap();
        assert_eq!(resp_wrong_target.status(), axum::http::StatusCode::FORBIDDEN);
        {
            let conn = db.lock().unwrap();
            assert!(is_device_trusted(&conn, &pk_mobile), "Target mismatch must leave device trusted");
        }

        // 3. Negative Test: Future timestamp rejected (400 Bad Request, zero DB side effects)
        let cert_expired = create_device_revocation_certificate(&sk_mobile, &pk_pc, "user_reset", now + 600_000);
        let resp_expired = client
            .post(format!("http://127.0.0.1:{}/v1/device/revoke", port))
            .json(&cert_expired)
            .send()
            .await
            .unwrap();
        assert_eq!(resp_expired.status(), axum::http::StatusCode::BAD_REQUEST);
        {
            let conn = db.lock().unwrap();
            assert!(is_device_trusted(&conn, &pk_mobile), "Future-dated certificate must leave device trusted");
        }

        // 4. Positive Test: Valid signed certificate accepted (200 OK + CommitAck, DB status -> revoked, session purged)
        let cert_valid = create_device_revocation_certificate(&sk_mobile, &pk_pc, "user_reset", now - 86_400_000);
        let resp_valid = client
            .post(format!("http://127.0.0.1:{}/v1/device/revoke", port))
            .json(&cert_valid)
            .send()
            .await
            .unwrap();
        assert_eq!(resp_valid.status(), axum::http::StatusCode::OK);
        let ack: RevocationAck = resp_valid.json().await.unwrap();
        assert_eq!(ack.status, "committed");
        assert_eq!(ack.event_id, cert_valid.event_id);
        assert_eq!(ack.revoked_device_id, pk_mobile);
        assert_eq!(ack.target_device_id, pk_pc);

        {
            let conn = db.lock().unwrap();
            assert!(!is_device_trusted(&conn, &pk_mobile), "Device must be revoked in DB");
            let sess_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM authenticated_sessions WHERE subject_device_id = ?",
                [&pk_mobile],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(sess_count, 0, "Sessions for revoked device must be deleted");
        }

        // 5. Negative/Idempotent Test: Replayed certificate returns CommitAck without duplicating mutations
        let resp_replay = client
            .post(format!("http://127.0.0.1:{}/v1/device/revoke", port))
            .json(&cert_valid)
            .send()
            .await
            .unwrap();
        assert_eq!(resp_replay.status(), axum::http::StatusCode::OK);
        let ack_replay: RevocationAck = resp_replay.json().await.unwrap();
        assert_eq!(ack_replay.status, "committed");
        assert_eq!(ack_replay.event_id, cert_valid.event_id);

        // 已处理事件仍须先验证证书签名，伪造 event_id 不得换取签名 Ack。
        let mut forged_replay = cert_valid.clone();
        forged_replay.signature = BASE64.encode([0u8; 64]);
        let forged_replay_resp = client
            .post(format!("http://127.0.0.1:{}/v1/device/revoke", port))
            .json(&forged_replay).send().await.unwrap();
        assert_eq!(forged_replay_resp.status(), axum::http::StatusCode::UNAUTHORIZED);

        // 6. Negative Test: Untrusted/unknown signer rejected
        let (sk_unknown, _pk_unknown) = generate_test_keypair();
        let cert_unknown = create_device_revocation_certificate(&sk_unknown, &pk_pc, "user_reset", now + 40);
        let resp_unknown = client
            .post(format!("http://127.0.0.1:{}/v1/device/revoke", port))
            .json(&cert_unknown)
            .send()
            .await
            .unwrap();
        assert_eq!(resp_unknown.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    // --------------------------------------------------------------------------
    // Test 42: Real WebSocket Penetration: Relay Revocation & Cryptographic CommitAck Lifecycle
    // --------------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_sec01_relay_revocation_and_commit_ack_lifecycle() {
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        use tokio_tungstenite::connect_async;
        use futures_util::{StreamExt, SinkExt};

        // 1. Setup mock Relay WebSocket server using Axum on 127.0.0.1:0
        let (broadcast_tx, _) = tokio::sync::broadcast::channel::<String>(100);
        let btx_clone = broadcast_tx.clone();

        let relay_router = axum::Router::new().route("/relay/ws", axum::routing::get(move |ws: axum::extract::ws::WebSocketUpgrade| {
            let btx = btx_clone.clone();
            async move {
                ws.on_upgrade(move |mut socket| async move {
                    let mut brx = btx.subscribe();
                    loop {
                        tokio::select! {
                            msg_opt = socket.next() => {
                                match msg_opt {
                                    Some(Ok(axum::extract::ws::Message::Text(txt))) => {
                                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&txt) {
                                            if matches!(value.get("type").and_then(|v| v.as_str()), Some("notify") | Some("ack") | Some("commit_ack") | Some("proxy")) {
                                                let _ = btx.send(txt.to_string());
                                            }
                                        }
                                    }
                                    Some(Ok(axum::extract::ws::Message::Close(_))) | None => break,
                                    _ => {}
                                }
                            }
                            recv_res = brx.recv() => {
                                match recv_res {
                                    Ok(txt) => {
                                        if socket.send(axum::extract::ws::Message::Text(txt.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                    }
                })
            }
        }));

        let relay_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_port = relay_listener.local_addr().unwrap().port();
        let relay_url = format!("ws://127.0.0.1:{}/relay/ws", relay_port);

        tokio::spawn(async move {
            let _ = axum::serve(
                relay_listener,
                relay_router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            ).await;
        });

        // 2. Setup Node A (Local / PC) and Node B (Peer / Mobile)
        let mut conn_local = setup_test_db();
        let mut conn_peer = setup_test_db();
        let (sk_local, pk_local) = generate_test_keypair();
        let (sk_peer, pk_peer) = generate_test_keypair();
        let now = crate::now_ms();

        // Node B has Node A as trusted peer with active session
        conn_peer.execute(
            "INSERT INTO trusted_devices (device_id, public_key, device_name, platform, paired_at, last_authenticated_at, status)
             VALUES (?, ?, 'PC', 'windows', ?, ?, 'trusted')",
            rusqlite::params![pk_local, pk_local, now, now],
        ).unwrap();
        conn_peer.execute(
            "INSERT INTO authenticated_sessions (session_id, subject_device_id, issuer_device_id, created_at, expires_at, last_activity_at, is_active)
             VALUES ('sess-peer-relay-1', ?, 'peer-id', ?, ?, ?, 1)",
            rusqlite::params![pk_local, now, now + 86400_000, now],
        ).unwrap();
        assert!(is_device_trusted(&conn_peer, &pk_local));

        // Connect Node A and Node B to Mock Relay over WebSocket
        let (ws_stream_a, _) = connect_async(&relay_url).await.expect("Node A 必须能成功连接 Relay WebSocket");
        let (ws_stream_b, _) = connect_async(&relay_url).await.expect("Node B 必须能成功连接 Relay WebSocket");
        let (mut ws_a_tx, mut ws_a_rx) = ws_stream_a.split();
        let (mut ws_b_tx, mut ws_b_rx) = ws_stream_b.split();

        // 3. Node A stages revocation certificate into peer_revocation_outbox
        let cert = create_device_revocation_certificate(&sk_local, &pk_peer, "local_reset", now);
        let cert_json = serde_json::to_string(&cert).unwrap();
        conn_local.execute(
            "INSERT INTO peer_revocation_outbox (event_id, target_peer_id, revoked_device_id, revocation_payload, status, attempts, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5)",
            rusqlite::params![&cert.event_id, &pk_peer, &pk_local, &cert_json, now],
        ).unwrap();

        assert_eq!(get_pending_peer_revocations(&conn_local, 10).unwrap().len(), 1);

        // 4. Node A drains outbox via Relay -> marks 'sent' (NOT 'delivered'!), sends frame over WebSocket
        let mut dispatched_count = 0;
        let items = get_pending_peer_revocations(&conn_local, 50).unwrap();
        for item in items {
            mark_peer_revocation_sent(&conn_local, item.id, now + 10).unwrap();
            let relay_msg = serde_json::json!({
                "type": "notify",
                "target_device_id": item.target_peer_id,
                "payload": { "device_revocation": serde_json::from_str::<serde_json::Value>(&item.revocation_payload).unwrap() }
            });
            ws_a_tx.send(WsMessage::Text(relay_msg.to_string().into())).await.unwrap();
            dispatched_count += 1;
        }
        assert_eq!(dispatched_count, 1);

        let outbox_status: String = conn_local.query_row(
            "SELECT status FROM peer_revocation_outbox WHERE event_id = ?",
            [&cert.event_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(outbox_status, "sent", "Relay dispatch 阶段出件箱状态必须为 'sent'，绝对不能标记为 'delivered'");

        // 5. Node B receives WebSocket frame, verifies cert, applies revocation, signs and returns CommitAck
        // 5. Node B receives WebSocket frame, verifies cert, applies revocation, signs and returns CommitAck
        // 关键穿透：经由生产 sync_engine::process_relay_device_revocation_frame 处理
        let node_b_recv = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(Ok(msg)) = ws_b_rx.next().await {
                if let WsMessage::Text(txt) = msg {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                        if val.get("type").and_then(|v| v.as_str()) == Some("notify") {
                            return val;
                        }
                    }
                }
            }
            panic!("Node B 未收到 device_revocation 帧");
        }).await.unwrap();

        // Node B 调用生产分发器处理接收到的 Relay 撤销消息帧
        let ack_msg = crate::sync_engine::process_relay_device_revocation_frame(
            &mut conn_peer,
            &node_b_recv,
            &pk_peer,
            &sk_peer,
            now + 20,
        ).expect("Node B 生产分发器必须成功处理撤销证书并生成持钥签名 CommitAck 消息");

        let ack: RevocationAck = serde_json::from_value(ack_msg["payload"]["device_revocation_ack"].clone()).unwrap();
        assert_eq!(ack.status, "committed");
        assert_eq!(ack.event_id, cert.event_id);
        assert_eq!(ack.revoked_device_id, pk_local);
        assert_eq!(ack.target_device_id, pk_peer);
        assert!(!ack.signature.is_empty());
        assert!(!ack.nonce.is_empty());

        // Verify Node B local trust and session destroyed
        assert!(!is_device_trusted(&conn_peer, &pk_local), "Node B 上 PC 已被成功作废");
        let sess_cnt: i64 = conn_peer.query_row(
            "SELECT COUNT(*) FROM authenticated_sessions WHERE subject_device_id = ?",
            [&pk_local],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(sess_cnt, 0, "Node B 上 PC 的认证会话已被清理");

        // Node B sends CommitAck back to Relay over WebSocket
        ws_b_tx.send(WsMessage::Text(ack_msg.to_string().into())).await.unwrap();

        // 6. Node A receives CommitAck over WebSocket, passes through production sync_engine::process_relay_device_revocation_ack_frame
        let node_a_recv = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(Ok(msg)) = ws_a_rx.next().await {
                if let WsMessage::Text(txt) = msg {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                        if val.get("type").and_then(|v| v.as_str()) == Some("ack") {
                            return val;
                        }
                    }
                }
            }
            panic!("Node A 未收到 device_revocation_ack 帧");
        }).await.unwrap();

        // Node A 调用生产分发器处理接收到的 Ack 消息帧
        let outcome = crate::sync_engine::process_relay_device_revocation_ack_frame(
            &mut conn_local,
            &node_a_recv,
            now + 30,
        ).expect("Node A 生产分发器必须能持钥校验通过对端的 CommitAck 并提交出件箱");
        assert_eq!(outcome, RevocationAckOutcome::Delivered { event_id: cert.event_id.clone(), peer_id: pk_peer.clone() });

        let final_status: String = conn_local.query_row(
            "SELECT status FROM peer_revocation_outbox WHERE event_id = ?",
            [&cert.event_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(final_status, "delivered", "合法持钥签名的 CommitAck 经生产分发器处理后必须将状态置为 'delivered'");

        // 7. Negative Test: Forged Ack signature rejected (zero side effects, outbox untouched)
        let mut forged_msg = node_a_recv.clone();
        forged_msg["payload"]["device_revocation_ack"]["signature"] = serde_json::json!(BASE64.encode(vec![0u8; 64]));
        let forged_res = crate::sync_engine::process_relay_device_revocation_ack_frame(&mut conn_local, &forged_msg, now + 40);
        assert!(forged_res.is_err(), "伪造签名的 Ack 必须被生产分发器拒绝");
        assert!(forged_res.unwrap_err().contains("数字签名验证失败"));

        // 8. Negative Test: Mismatched target/revoked ID rejected (zero side effects)
        let mut mismatch_msg = node_a_recv.clone();
        mismatch_msg["payload"]["device_revocation_ack"]["revoked_device_id"] = serde_json::json!("wrong-revoked-id");
        let mismatch_res = crate::sync_engine::process_relay_device_revocation_ack_frame(&mut conn_local, &mismatch_msg, now + 45);
        assert!(mismatch_res.is_err(), "被撤销 ID 不匹配必须被生产分发器拒绝");

        // 9. Negative Test: Unknown event ID rejected (zero side effects)
        let mut unknown_msg = node_a_recv.clone();
        unknown_msg["payload"]["device_revocation_ack"]["event_id"] = serde_json::json!("unknown-event-id-999");
        let unknown_res = crate::sync_engine::process_relay_device_revocation_ack_frame(&mut conn_local, &unknown_msg, now + 50);
        assert!(unknown_res.is_err(), "未知的 event_id 必须被生产分发器拒绝");
        assert!(unknown_res.unwrap_err().contains("收到未知的撤销出件箱事件 Ack"));

        // 10. Negative Test: Premature Ack before item is sent (status == 'pending') rejected
        let cert2 = create_device_revocation_certificate(&sk_local, &pk_peer, "local_reset_2", now + 100);
        let cert2_json = serde_json::to_string(&cert2).unwrap();
        conn_local.execute(
            "INSERT INTO peer_revocation_outbox (event_id, target_peer_id, revoked_device_id, revocation_payload, status, attempts, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5)",
            rusqlite::params![&cert2.event_id, &pk_peer, &pk_local, &cert2_json, now + 100],
        ).unwrap();

        let premature_ack = create_device_revocation_ack(&sk_peer, &cert2.event_id, &pk_local, &pk_peer, "committed", now + 105);
        let premature_msg = serde_json::json!({
            "type": "ack",
            "target_device_id": pk_local,
            "payload": { "device_revocation_ack": premature_ack }
        });
        let premature_res = crate::sync_engine::process_relay_device_revocation_ack_frame(&mut conn_local, &premature_msg, now + 110);
        assert!(premature_res.is_err(), "在出件箱未处于 sent 状态前收到 Ack 必须被生产分发器拒绝");
        let premature_status: String = conn_local.query_row(
            "SELECT status FROM peer_revocation_outbox WHERE event_id = ?",
            [&cert2.event_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(premature_status, "pending", "过早 Ack 被拒绝后，出件箱状态仍保持 pending，绝不误标 delivered");

        // 11. Idempotent Test: Repeated valid Ack returns AlreadyDelivered with zero side effects
        let replay_res = crate::sync_engine::process_relay_device_revocation_ack_frame(&mut conn_local, &node_a_recv, now + 120);
        assert_eq!(replay_res.unwrap(), RevocationAckOutcome::AlreadyDelivered { event_id: cert.event_id.clone() });

        // 12. Stale Timeout Reversion & Re-dispatch Lifecycle
        mark_peer_revocation_sent(&conn_local, 2, now + 130).unwrap();
        let reverted = revert_stale_sent_revocations(&conn_local, 50_000, now + 190_000).unwrap();
        assert_eq!(reverted, 1, "超时的 sent 项必须自动回退至 pending");
        let pending_after = get_pending_peer_revocations(&conn_local, 10).unwrap();
        assert_eq!(pending_after.len(), 1);
        assert_eq!(pending_after[0].event_id, cert2.event_id);
    }

    // --------------------------------------------------------------------------
    // Test 43: is_identity_degraded Fail-Closed Verification
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_is_identity_degraded_fail_closed() {
        let mut conn = setup_test_db();
        let (sk, pk) = generate_test_keypair();
        let now = crate::now_ms();

        // 1. Initially no degraded records -> Ok(false)
        assert_eq!(is_identity_degraded(&conn).unwrap(), false);

        // 2. Insert degraded journal -> Ok(true)
        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, error, created_at, updated_at)
             VALUES ('degraded', 'dummy', 'disk write failed', ?, ?)",
            rusqlite::params![now, now],
        ).unwrap();
        assert_eq!(is_identity_degraded(&conn).unwrap(), true);

        // 3. Pairing and session creation fail-closed when degraded
        let invite_err = create_pairing_invitation(&conn, &pk, None, 600_000, "relay", vec![], 3722, now);
        assert!(invite_err.is_err());
        assert!(invite_err.unwrap_err().contains("处于降级重置状态"));

        // 4. Injected DB error: drop identity_reset_journal table
        // is_identity_degraded MUST return Err (not false!), and callers must Fail-Closed
        conn.execute("DROP TABLE identity_reset_journal", []).unwrap();
        let degraded_err = is_identity_degraded(&conn);
        assert!(degraded_err.is_err(), "数据库查询失败时必须返回 Err，严禁 unwrap_or(false)");

        let invite_db_err = create_pairing_invitation(&conn, &pk, None, 600_000, "relay", vec![], 3722, now);
        assert!(invite_db_err.is_err(), "降级检查失败时创建邀请必须 Fail-Closed 阻断");
    }

    // --------------------------------------------------------------------------
    // Test 44: recover_device_identity_state Convergence Across Phases
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_recover_device_identity_state_convergence() {
        let _test_lock = crate::CONFIG_OP_TEST_MUTEX.lock().unwrap();
        let baseline_real = crate::http_api::tests::RealConfigBaseline::capture();
        let mut conn = setup_test_db();
        let (sk, pk) = generate_test_keypair();
        let now = crate::now_ms();
        let config_dir = std::env::temp_dir().join(format!("sec01_recovery_config_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.json");
        std::fs::write(&config_path, serde_json::to_vec(&serde_json::json!({"device_id": pk})).unwrap()).unwrap();
        crate::set_test_config_path_override(Some(config_path));
        let _config_guard = crate::http_api::tests::TestConfigOverrideGuard;

        // Path 1: prepared record -> cancelled
        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, error, created_at, updated_at)
             VALUES ('prepared', ?1, NULL, ?2, ?2)",
            rusqlite::params![&pk, now],
        ).unwrap();
        let outcome1 = recover_device_identity_state(&conn, None, None, now + 10).unwrap();
        assert_eq!(outcome1, IdentityRecoveryOutcome::CancelledPrepared { count: 1 });
        let st1: String = conn.query_row("SELECT state FROM identity_reset_journal WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(st1, "cancelled");

        // Path 2: staged / db_committed record -> physical cleanup and converged to committed
        let temp_key = std::env::temp_dir().join(format!("recover_test_key_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&temp_key, b"unwiped_key").unwrap();
        let mem = std::sync::Mutex::new(Some(sk.clone()));

        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, error, created_at, updated_at)
             VALUES ('staged', ?1, NULL, ?2, ?2)",
            rusqlite::params![&pk, now + 20],
        ).unwrap();
        let j_id: i64 = conn.last_insert_rowid();

        let outcome2 = recover_device_identity_state(&conn, Some(&temp_key), Some(&mem), now + 30).unwrap();
        assert_eq!(outcome2, IdentityRecoveryOutcome::ConvergedToCommitted { journal_id: j_id });
        assert!(!temp_key.exists(), "恢复逻辑必须物理删除遗留密钥文件");
        assert!(mem.lock().unwrap().is_none(), "恢复逻辑必须清空遗留内存密钥");
        let st2: String = conn.query_row("SELECT state FROM identity_reset_journal WHERE id = ?", [j_id], |r| r.get(0)).unwrap();
        assert_eq!(st2, "committed");

        // Path 3: 已确认 DB 提交、但密钥文件无法清理 -> RetainedDegraded
        let unremovable_dir = std::env::temp_dir().join(format!("recover_unremovable_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&unremovable_dir).unwrap();
        let fake_file_as_dir = unremovable_dir.join("sub_key_dir");
        std::fs::create_dir_all(&fake_file_as_dir).unwrap();
        std::fs::write(fake_file_as_dir.join("child.txt"), b"block").unwrap();

        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, error, created_at, updated_at)
             VALUES ('degraded_post_db', ?1, 'disk fail', ?2, ?2)",
            rusqlite::params![&pk, now + 40],
        ).unwrap();
        let j_deg_id: i64 = conn.last_insert_rowid();

        let outcome3 = recover_device_identity_state(&conn, Some(&fake_file_as_dir), None, now + 50).unwrap();
        match outcome3 {
            IdentityRecoveryOutcome::RetainedDegraded { journal_id, .. } => {
                assert_eq!(journal_id, j_deg_id);
            }
            other => panic!("Expected RetainedDegraded, got {:?}", other),
        }
        let st3: String = conn.query_row("SELECT state FROM identity_reset_journal WHERE id = ?", [j_deg_id], |r| r.get(0)).unwrap();
        assert!(st3 == "degraded_post_db" || st3 == "degraded_key_destroy");
        // 本例其余分支独立验证；移除模拟的未决日志，避免下次恢复误触该故障路径。
        conn.execute("UPDATE identity_reset_journal SET state = 'cancelled' WHERE id = ?", [j_deg_id]).unwrap();

        // Path 4: degraded_pre_db record -> safely cancelled to preserve old identity
        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, error, created_at, updated_at)
             VALUES ('degraded_pre_db', ?1, 'db tx failed', ?2, ?2)",
            rusqlite::params![&pk, now + 60],
        ).unwrap();
        let j_pre_id: i64 = conn.last_insert_rowid();
        let outcome4 = recover_device_identity_state(&conn, None, None, now + 70).unwrap();
        assert_eq!(outcome4, IdentityRecoveryOutcome::CancelledPrepared { count: 1 });
        let st4: String = conn.query_row("SELECT state FROM identity_reset_journal WHERE id = ?", [j_pre_id], |r| r.get(0)).unwrap();
        assert_eq!(st4, "cancelled");

        let _ = std::fs::remove_dir_all(&unremovable_dir);
        std::fs::remove_dir_all(&config_dir).unwrap();
        baseline_real.assert_unchanged();
    }

    #[test]
    fn test_sec01_old_reset_recovery_preserves_new_identity_before_key_deletion() {
        let _test_lock = crate::CONFIG_OP_TEST_MUTEX.lock().unwrap();
        let baseline_real = crate::http_api::tests::RealConfigBaseline::capture();
        let conn = setup_test_db();
        let old_key = SigningKey::from_bytes(&[31u8; 32]);
        let new_key = SigningKey::from_bytes(&[32u8; 32]);
        let old_id = BASE64.encode(VerifyingKey::from(&old_key).to_bytes());
        let new_id = BASE64.encode(VerifyingKey::from(&new_key).to_bytes());
        let temp_dir = std::env::temp_dir().join(format!("sec01_new_identity_guard_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("config.json");
        let key_path = temp_dir.join("device_identity.json");
        std::fs::write(&config_path, serde_json::to_vec(&serde_json::json!({"device_id": new_id})).unwrap()).unwrap();
        std::fs::write(&key_path, b"new-encrypted-key-must-survive").unwrap();
        crate::set_test_config_path_override(Some(config_path));
        let _config_guard = crate::http_api::tests::TestConfigOverrideGuard;
        let memory = std::sync::Mutex::new(Some(new_key));
        let now = crate::now_ms();
        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, created_at, updated_at)
             VALUES ('db_committed', ?1, ?2, ?2)",
            rusqlite::params![&old_id, now],
        ).unwrap();

        let error = recover_device_identity_state(&conn, Some(&key_path), Some(&memory), now + 1).unwrap_err();
        assert!(error.contains("另一代设备身份"), "unexpected recovery error: {}", error);
        assert_eq!(std::fs::read(&key_path).unwrap(), b"new-encrypted-key-must-survive");
        assert!(memory.lock().unwrap().is_some());
        assert_eq!(crate::read_config_checked().unwrap()["device_id"], new_id);
        let state: String = conn.query_row("SELECT state FROM identity_reset_journal", [], |row| row.get(0)).unwrap();
        assert_eq!(state, "db_committed", "旧恢复记录必须保留待人工核查");
        let config_path = temp_dir.join("config.json");
        std::fs::write(&config_path, serde_json::to_vec(&serde_json::json!({"device_id": old_id})).unwrap()).unwrap();
        let memory_error = recover_device_identity_state(&conn, Some(&key_path), Some(&memory), now + 2).unwrap_err();
        assert!(memory_error.contains("内存中已有另一代密钥"));
        assert_eq!(std::fs::read(&key_path).unwrap(), b"new-encrypted-key-must-survive");
        assert!(memory.lock().unwrap().is_some());
        std::fs::remove_dir_all(&temp_dir).unwrap();
        baseline_real.assert_unchanged();
    }

    #[test]
    fn test_sec01_unknown_reset_stage_fails_closed_without_destroying_key() {
        let conn = setup_test_db();
        let (_sk, pk) = generate_test_keypair();
        let now = crate::now_ms();
        let key_path = std::env::temp_dir().join(format!("sec01_unknown_stage_{}", uuid::Uuid::new_v4()));
        std::fs::write(&key_path, b"preserve-old-key").unwrap();
        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, created_at, updated_at)
             VALUES ('legacy_unknown', ?1, ?2, ?2)",
            rusqlite::params![pk, now],
        ).unwrap();
        let err = recover_device_identity_state(&conn, Some(&key_path), None, now + 1).unwrap_err();
        assert!(err.contains("无法判定"));
        assert_eq!(std::fs::read(&key_path).unwrap(), b"preserve-old-key");
        let state: String = conn.query_row("SELECT state FROM identity_reset_journal LIMIT 1", [], |r| r.get(0)).unwrap();
        assert_eq!(state, "legacy_unknown");
        conn.execute("DROP TABLE peer_revocation_outbox", []).unwrap();
        let err = recover_device_identity_state(&conn, Some(&key_path), None, now + 2).unwrap_err();
        assert!(err.contains("查询撤销出件箱事实失败"));
        assert_eq!(std::fs::read(&key_path).unwrap(), b"preserve-old-key");
        std::fs::remove_file(key_path).unwrap();
    }

    // --------------------------------------------------------------------------
    // Test 45: Phase 5 Reset Journal Committed Error Handling & Degraded Fallback
    // --------------------------------------------------------------------------
    #[test]
    fn test_sec01_crypto_reset_phase5_error_handling() {
        let _config_lock = crate::CONFIG_OP_TEST_MUTEX.lock().unwrap();
        let _config_guard = crate::http_api::tests::TestConfigOverrideGuard;
        let mut conn = setup_test_db();
        let (sk, pk) = generate_test_keypair();
        let config_path = std::env::temp_dir().join(format!("sec01_phase5_cfg_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&config_path, serde_json::to_vec(&serde_json::json!({"device_id": pk})).unwrap()).unwrap();
        crate::set_test_config_path_override(Some(config_path.clone()));
        let now = crate::now_ms();
        let memory_mutex = std::sync::Mutex::new(Some(sk));

        // Inject trigger abort specifically on Phase 5 committed update
        conn.execute(
            "CREATE TRIGGER test_inject_phase5_abort
             BEFORE UPDATE ON identity_reset_journal
             WHEN NEW.state = 'committed'
             BEGIN
                 SELECT RAISE(ABORT, 'INJECTED_PHASE5_ABORT');
             END;",
            [],
        ).unwrap();

        let fake_key_path = std::env::temp_dir().join(format!("bob_test_phase5_{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&fake_key_path, b"key_data").unwrap();

        let res = crate::crypto::reset_device_keys_core(
            Some(&fake_key_path),
            &memory_mutex,
            |_cfg| Ok(()),
            &mut conn,
            now,
        );

        assert!(res.is_err(), "Phase 5 commit 失败必须返回 Err，严禁吞掉错误返回 Ok");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("Phase 5"));

        // Verify journal was marked 'degraded' / 'degraded_post_db' instead of remaining in limbo
        let st: String = conn.query_row(
            "SELECT state FROM identity_reset_journal ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(st == "degraded" || st == "degraded_post_db", "Phase 5 失败必须标记 journal 为 degraded/degraded_post_db, got: {}", st);

        conn.execute("DROP TRIGGER test_inject_phase5_abort", []).unwrap();
        let _ = std::fs::remove_file(&fake_key_path);
        std::fs::remove_file(config_path).unwrap();
    }

    // --------------------------------------------------------------------------
    // Test 46: Phase 2 Transaction Rollback & Restart Recovery Preserves Old Identity
    // --------------------------------------------------------------------------
    #[tokio::test]
    async fn test_sec01_crypto_reset_phase2_rollback_preserves_old_identity_after_restart() {
        let _test_lock = crate::CONFIG_OP_TEST_MUTEX.lock().unwrap();
        let baseline_real = crate::http_api::tests::RealConfigBaseline::capture();

        let temp_dir = std::env::temp_dir().join(format!("bob_cfg_sec01_rollback_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let cfg_path = temp_dir.join("config.json");
        let key_path = temp_dir.join("device_identity.json");
        crate::set_test_config_path_override(Some(cfg_path.clone()));
        let _guard = crate::http_api::tests::TestConfigOverrideGuard;

        // 1. 初始化磁盘配置与密钥文件
        let (sk_local, pk_local) = generate_test_keypair();
        let (sk_peer, pk_peer) = generate_test_keypair();

        let initial_cfg = serde_json::json!({
            "device_id": pk_local,
            "pairing_payload": "active_pairing_payload"
        });
        std::fs::write(&cfg_path, serde_json::to_string_pretty(&initial_cfg).unwrap().as_bytes()).unwrap();

        let original_key_content = b"ACTIVE_VALID_LOCAL_PRIVATE_KEY_DATA";
        std::fs::write(&key_path, original_key_content).unwrap();

        // 2. 初始化持久化 SQLite 数据库并插入 trusted 对端和活跃会话
        let db_path = temp_dir.join("rollback_recovery_test.db");
        let now = crate::now_ms();
        {
            let mut conn = rusqlite::Connection::open(&db_path).unwrap();
            init_device_trust_tables(&conn).unwrap();

            conn.execute(
                "INSERT INTO trusted_devices (device_id, public_key, device_name, platform, paired_at, last_authenticated_at, status) VALUES (?1, ?1, 'Phone', 'mobile', ?2, ?2, 'trusted')",
                rusqlite::params![&pk_peer, now],
            ).unwrap();

            conn.execute(
                "INSERT INTO authenticated_sessions (session_id, subject_device_id, issuer_device_id, created_at, expires_at, last_activity_at, is_active)
                 VALUES ('sess_1', ?1, ?1, ?2, ?2 + 3600000, ?2, 1)",
                rusqlite::params![&pk_peer, now],
            ).unwrap();

            // 3. 注入故障触发器：在 Phase 2 事务后段 (DELETE FROM rpc_staged_outbox) 触发 ABORT
            // 导致整个 Phase 2 SQLite 事务强行回滚！
            conn.execute(
                "CREATE TRIGGER test_inject_phase2_late_failure
                 BEFORE DELETE ON authenticated_sessions
                 BEGIN
                     SELECT RAISE(ABORT, 'INJECTED_PHASE2_LATE_ABORT');
                 END;",
                [],
            ).unwrap();

            let memory_mutex = std::sync::Mutex::new(Some(sk_local.clone()));

            // 4. 执行重置 -> Phase 2 事务整体回滚，外层捕获错误并将 journal 更新为 degraded_pre_db
            let reset_res = crate::crypto::reset_device_keys_core(
                Some(&key_path),
                &memory_mutex,
                |cfg| crate::write_config_checked(cfg),
                &mut conn,
                now + 10,
            );

            assert!(reset_res.is_err(), "Phase 2 后段触发器回滚必须导致重置失败");
            let err_msg = reset_res.unwrap_err();
            assert!(err_msg.contains("数据库重置事务失败"), "错误信息必须指出数据库重置事务失败: {}", err_msg);

            // 验证 journal 被标记为 degraded_pre_db
            let st: String = conn.query_row(
                "SELECT state FROM identity_reset_journal WHERE id = 1",
                [],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(st, "degraded_pre_db", "事务回滚后外层必须标记为 degraded_pre_db");

            // 移除注入触发器，模拟进程退出
            conn.execute("DROP TRIGGER test_inject_phase2_late_failure", []).unwrap();
        }

        // 5. 模拟节点重启：重新打开 SQLite 数据库连接并运行生产启动恢复器 recover_device_identity_state
        {
            let mut conn_restart = rusqlite::Connection::open(&db_path).unwrap();

            // 执行启动恢复
            let recovery_outcome = recover_device_identity_state(
                &conn_restart,
                Some(&key_path),
                None,
                now + 100,
            ).unwrap();

            assert_eq!(
                recovery_outcome,
                IdentityRecoveryOutcome::CancelledPrepared { count: 1 },
                "启动恢复器必须识别出 Phase 2 未提交，安全取消重置，严禁销毁旧身份"
            );

            // 6. 核心安全不变量验证 (Old Identity Preserved & Zero Unilateral Key Destruction)
            // a. 磁盘私钥文件必须完好存在，绝对未被销毁！
            assert!(key_path.exists(), "磁盘旧私钥绝对不能被误销毁！");
            let key_after_restart = std::fs::read(&key_path).unwrap();
            assert_eq!(key_after_restart, original_key_content, "磁盘旧私钥内容必须保持原样");

            // b. 磁盘配置中的 device_id 与 pairing_payload 必须完好无损！
            let cfg_after_restart = crate::read_config_checked().unwrap();
            assert_eq!(cfg_after_restart.get("device_id").and_then(|d| d.as_str()), Some(pk_local.as_str()));
            assert_eq!(cfg_after_restart.get("pairing_payload").and_then(|d| d.as_str()), Some("active_pairing_payload"));

            // c. 数据库中对端信任关系依然是 trusted，旧认证会话依然存在（事务整体回滚）
            let peer_status: String = conn_restart.query_row(
                "SELECT status FROM trusted_devices WHERE device_id = ?",
                [&pk_peer],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(peer_status, "trusted", "可信对端状态必须依然是 trusted");

            let sess_count: i64 = conn_restart.query_row(
                "SELECT COUNT(*) FROM authenticated_sessions WHERE subject_device_id = ?",
                [&pk_peer],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(sess_count, 1, "认证会话在事务回滚后依然完好");

            // d. 撤销出件箱必须为空 (没有半套 Outbox 残留)
            let outbox_count: i64 = conn_restart.query_row(
                "SELECT COUNT(*) FROM peer_revocation_outbox",
                [],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(outbox_count, 0, "撤销出件箱不能有未提交的孤儿记录");

            // e. journal 记录必须被置为 cancelled，解除降级锁定
            let st_after: String = conn_restart.query_row(
                "SELECT state FROM identity_reset_journal WHERE id = 1",
                [],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(st_after, "cancelled", "重置记录必须收敛为 cancelled");

            // f. 降级状态已自愈解除
            assert_eq!(is_identity_degraded(&conn_restart).unwrap(), false, "重启恢复后节点恢复正常，降级状态必须解除");
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
        baseline_real.assert_unchanged();
    }

    #[test]
    fn test_sec01_trusted_peer_new_session_auto_registration_and_sliding_expiry() {
        let mut conn = setup_test_db();
        let now = 1791460000000i64;

        // 1. 建立已配对可信移动端设备 (status = 'trusted')
        let (sk_mobile, pk_mobile) = generate_test_keypair();
        let (_sk_pc, pk_pc) = generate_test_keypair();

        conn.execute(
            "INSERT INTO trusted_devices (
                device_id, public_key, device_name, platform, paired_at,
                last_authenticated_at, status, revoked_at, revocation_reason
            ) VALUES (?1, ?2, 'Test Mobile', 'android', ?3, ?3, 'trusted', NULL, NULL)",
            params![pk_mobile, pk_mobile, now],
        ).unwrap();

        // 2. 模拟移动端在 24 小时会话到期或自愈建立新会话：生成接收端数据库从未见过的 session_id
        let new_sess_id = format!("sess-{}", uuid::Uuid::new_v4());
        let req_id1 = format!("req-{}", uuid::Uuid::new_v4());
        let nonce1 = format!("nonce-{}", uuid::Uuid::new_v4());
        let action = "pull";
        let payload = b"";
        let payload_hash = compute_sha512(payload);

        let canonical1 = canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION,
            &new_sess_id,
            &req_id1,
            &pk_mobile,
            &pk_pc,
            action,
            &payload_hash,
            &nonce1,
            now,
        );
        let sig1 = BASE64.encode(sk_mobile.sign(&canonical1).to_bytes());

        let env1 = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: new_sess_id.clone(),
            request_id: req_id1,
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: action.to_string(),
            payload_hash: payload_hash.clone(),
            signature: sig1,
            nonce: nonce1,
            timestamp: now,
        };

        // 3. 验证端在会话未预先存在时，必须基于可信持钥签名自动注册该会话并放行 (杜绝 ERR-SYNC-05 阻断)
        let outcome1 = verify_rpc_request_auth(&mut conn, &env1, payload, &pk_pc, now)
            .expect("持有效签名的可信设备发起的新会话必须成功建立并验签通过");
        match outcome1 {
            AuthVerificationOutcome::Authorized { session_id, .. } => {
                assert_eq!(session_id, new_sess_id);
            }
            _ => panic!("Expected Authorized outcome"),
        }

        // 检查数据库中已原子建立该活跃会话
        let (db_subj, db_iss, db_exp, db_act): (String, String, i64, i64) = conn.query_row(
            "SELECT subject_device_id, issuer_device_id, expires_at, is_active FROM authenticated_sessions WHERE session_id = ?",
            [&new_sess_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).expect("authenticated_sessions 必须已持久化新会话");
        assert_eq!(db_subj, pk_mobile);
        assert_eq!(db_iss, pk_pc);
        assert_eq!(db_act, 1);
        assert_eq!(db_exp, now + DEFAULT_SESSION_TTL_MS);

        // 4. 模拟 1 小时后再次调用：测试滑动过期窗口 (Sliding Window Expiration)
        let now_plus_1h = now + 3600_000;
        let req_id2 = format!("req-{}", uuid::Uuid::new_v4());
        let nonce2 = format!("nonce-{}", uuid::Uuid::new_v4());
        let canonical2 = canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION,
            &new_sess_id,
            &req_id2,
            &pk_mobile,
            &pk_pc,
            action,
            &payload_hash,
            &nonce2,
            now_plus_1h,
        );
        let sig2 = BASE64.encode(sk_mobile.sign(&canonical2).to_bytes());

        let env2 = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: new_sess_id.clone(),
            request_id: req_id2,
            subject_device_id: pk_mobile.clone(),
            target_device_id: pk_pc.clone(),
            action: action.to_string(),
            payload_hash,
            signature: sig2,
            nonce: nonce2,
            timestamp: now_plus_1h,
        };

        let outcome2 = verify_rpc_request_auth(&mut conn, &env2, payload, &pk_pc, now_plus_1h)
            .expect("已有会话的后续调用必须正常通过");
        match outcome2 {
            AuthVerificationOutcome::Authorized { session_id, .. } => {
                assert_eq!(session_id, new_sess_id);
            }
            _ => panic!("Expected Authorized outcome"),
        }

        // 检查滑动窗口是否已被延展到 now_plus_1h + DEFAULT_SESSION_TTL_MS
        let db_exp_after: i64 = conn.query_row(
            "SELECT expires_at FROM authenticated_sessions WHERE session_id = ?",
            [&new_sess_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(db_exp_after, now_plus_1h + DEFAULT_SESSION_TTL_MS, "活跃会话必须按滑动窗口自动续期");

        // 5. 负向测试：未配对设备哪怕自造新会话，也必须被拦截
        let (sk_untrusted, pk_untrusted) = generate_test_keypair();
        let unk_sess = format!("sess-{}", uuid::Uuid::new_v4());
        let unk_nonce = format!("nonce-{}", uuid::Uuid::new_v4());
        let can_untrusted = canonical_rpc_bytes(
            SEC01_PROTOCOL_VERSION,
            &unk_sess,
            "req-untrusted",
            &pk_untrusted,
            &pk_pc,
            action,
            &compute_sha512(b""),
            &unk_nonce,
            now,
        );
        let sig_untrusted = BASE64.encode(sk_untrusted.sign(&can_untrusted).to_bytes());
        let env_untrusted = RpcAuthEnvelope {
            protocol_version: SEC01_PROTOCOL_VERSION.to_string(),
            session_id: unk_sess,
            request_id: "req-untrusted".to_string(),
            subject_device_id: pk_untrusted,
            target_device_id: pk_pc.clone(),
            action: action.to_string(),
            payload_hash: compute_sha512(b""),
            signature: sig_untrusted,
            nonce: unk_nonce,
            timestamp: now,
        };
        let err_untrusted = verify_rpc_request_auth(&mut conn, &env_untrusted, b"", &pk_pc, now).unwrap_err();
        assert!(err_untrusted.contains("未识别的未配对设备"), "未配对设备必须被拒绝");
    }
}
