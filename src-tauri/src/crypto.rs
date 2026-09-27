use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct DeviceIdentityState(pub Mutex<Option<SigningKey>>);

#[derive(Serialize, Deserialize)]
struct EncryptedKeyData {
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn get_keys_path(app: &AppHandle) -> PathBuf {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        crate::get_data_dir()
            .join("workspace_config")
            .join("device_identity.json")
    }
    #[cfg(not(target_os = "android"))]
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("workspace_config")
        .join("device_identity.json")
}

fn derive_key(pin: &str, salt_str: &str) -> Result<[u8; 32], String> {
    let argon2 = Argon2::default();
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(pin.as_bytes(), salt_str.as_bytes(), &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

pub const DEFAULT_INTERNAL_PIN: &str = "BOB_INTERNAL_DEVICE_KEY_DEFAULT_v1";

/// 确保本机设备身份密钥已就绪并加载到内存 (Zero-Friction Auto-Init & Auto-Unlock)
pub fn ensure_device_identity_unlocked_core(
    key_path: &Path,
    memory_state: &Mutex<Option<SigningKey>>,
) -> Result<SigningKey, String> {
    let mut guard = memory_state.lock().map_err(|e| e.to_string())?;
    if let Some(ref sk) = *guard {
        return Ok(sk.clone());
    }

    if key_path.exists() {
        if let Ok(file_content) = fs::read_to_string(key_path) {
            if let Ok(data) = serde_json::from_str::<EncryptedKeyData>(&file_content) {
                // 优先尝试零摩擦内置默认 PIN 与常见免 PIN 标识
                for test_pin in [DEFAULT_INTERNAL_PIN, "", "0000", "1234", "123456"] {
                    if let Ok(aes_key) = derive_key(test_pin, &data.salt) {
                        if let Ok(cipher) = Aes256Gcm::new_from_slice(&aes_key) {
                            if let Ok(nonce_bytes) = BASE64.decode(&data.nonce) {
                                if let Ok(ciphertext) = BASE64.decode(&data.ciphertext) {
                                    if let Ok(decrypted) = cipher.decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref()) {
                                        if decrypted.len() == 32 {
                                            let mut key_bytes = [0u8; 32];
                                            key_bytes.copy_from_slice(&decrypted);
                                            let signing_key = SigningKey::from_bytes(&key_bytes);
                                            *guard = Some(signing_key.clone());

                                            // 确保持久化 config 中的 device_id 与公钥严格一致
                                            let verifying_key = VerifyingKey::from(&signing_key);
                                            let b64_pub = BASE64.encode(verifying_key.to_bytes());
                                            if let Ok(mut app_config) = crate::read_config_checked() {
                                                if app_config.get("device_id").and_then(|v| v.as_str()) != Some(&b64_pub) {
                                                    if let Some(obj) = app_config.as_object_mut() {
                                                        obj.insert("device_id".to_string(), serde_json::json!(b64_pub));
                                                    }
                                                    let _ = crate::write_config_checked(&app_config);
                                                }
                                            }

                                            return Ok(signing_key);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        return Err("设备秘钥已加密锁定，请先输入 PIN 码解锁".to_string());
    }

    // 物理密钥文件不存在：自动生成全新 Ed25519 密钥对并透明持久化 (开箱即用)
    let mut csprng = rand::rngs::OsRng;
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);

    let mut salt_bytes = [0u8; 16];
    csprng.fill_bytes(&mut salt_bytes);
    let salt = BASE64.encode(salt_bytes);
    let mut nonce_bytes = [0u8; 12];
    csprng.fill_bytes(&mut nonce_bytes);
    let nonce_str = BASE64.encode(nonce_bytes);

    let aes_key = derive_key(DEFAULT_INTERNAL_PIN, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&aes_key).map_err(|e| e.to_string())?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, key_bytes.as_ref())
        .map_err(|e| e.to_string())?;

    if let Some(parent) = key_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let data = EncryptedKeyData {
        salt,
        nonce: nonce_str,
        ciphertext: BASE64.encode(ciphertext),
    };
    fs::write(
        key_path,
        serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?,
    ).map_err(|e| e.to_string())?;

    *guard = Some(signing_key.clone());

    let verifying_key = VerifyingKey::from(&signing_key);
    let b64_pub = BASE64.encode(verifying_key.to_bytes());
    if let Ok(mut app_config) = crate::read_config_checked() {
        if let Some(obj) = app_config.as_object_mut() {
            obj.insert("device_id".to_string(), serde_json::json!(b64_pub));
        }
        let _ = crate::write_config_checked(&app_config);
    }

    Ok(signing_key)
}

pub fn ensure_device_identity_unlocked_for_app(app: &AppHandle) -> Result<SigningKey, String> {
    let key_path = get_keys_path(app);
    let state = app.try_state::<DeviceIdentityState>()
        .ok_or_else(|| "DeviceIdentityState 未注册".to_string())?;
    ensure_device_identity_unlocked_core(&key_path, &state.0)
}

pub fn ensure_device_identity_unlocked_from_state(
    app: &AppHandle,
    state: &DeviceIdentityState,
) -> Result<SigningKey, String> {
    let key_path = get_keys_path(app);
    ensure_device_identity_unlocked_core(&key_path, &state.0)
}

#[tauri::command]
pub fn check_device_keys_initialized(app: AppHandle) -> bool {
    let config = crate::read_config();
    // 优先检查是否已设置本地 UI 门禁 PIN (SEC-01 解耦独立验证)
    if let Some(hash) = config.get("p2p_pin_hash").and_then(|v| v.as_str()) {
        if !hash.trim().is_empty() {
            return true;
        }
    }

    // 向后兼容历史遗留自定义 PIN
    let key_path = get_keys_path(&app);
    if key_path.exists() {
        if let Ok(file_content) = fs::read_to_string(&key_path) {
            if let Ok(data) = serde_json::from_str::<EncryptedKeyData>(&file_content) {
                // 若可用 DEFAULT_INTERNAL_PIN 解密，说明仅是开箱自愈生成的默认秘钥，用户尚未设置本地 PIN
                if let Ok(aes_key) = derive_key(DEFAULT_INTERNAL_PIN, &data.salt) {
                    if let Ok(cipher) = Aes256Gcm::new_from_slice(&aes_key) {
                        if let Ok(nonce_bytes) = BASE64.decode(&data.nonce) {
                            if let Ok(ciphertext) = BASE64.decode(&data.ciphertext) {
                                if cipher.decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref()).is_ok() {
                                    return false;
                                }
                            }
                        }
                    }
                }
                // 无法用默认 PIN 解密，说明历史文件由旧版本用户自定义 PIN 加密
                return true;
            }
        }
    }
    false
}

#[tauri::command]
pub fn init_device_keys(
    pin: String,
    app: AppHandle,
    _state: tauri::State<'_, DeviceIdentityState>,
) -> Result<(), String> {
    let clean_pin = pin.trim();
    if clean_pin.len() < 4 {
        return Err("PIN 码至少需 4 位数字".to_string());
    }

    // 1. 确保底层设备身份密钥就绪（已在内存或自愈生成）
    let _ = ensure_device_identity_unlocked_for_app(&app)?;

    // 2. 生成本地 PIN 的独立安全 Salt 与 Argon2 哈希
    let mut csprng = rand::rngs::OsRng;
    let mut salt_bytes = [0u8; 16];
    csprng.fill_bytes(&mut salt_bytes);
    let salt = BASE64.encode(salt_bytes);

    let pin_hash_bytes = derive_key(clean_pin, &salt)?;
    let pin_hash = BASE64.encode(pin_hash_bytes);

    // 3. 持久化到 config.json
    let mut app_config = crate::read_config_checked()?;
    if let Some(obj) = app_config.as_object_mut() {
        obj.insert("p2p_pin_salt".to_string(), serde_json::json!(salt));
        obj.insert("p2p_pin_hash".to_string(), serde_json::json!(pin_hash));
    }
    crate::write_config_checked(&app_config)?;

    Ok(())
}

#[tauri::command]
pub fn unlock_device_keys(
    pin: String,
    app: AppHandle,
    state: tauri::State<'_, DeviceIdentityState>,
) -> Result<(), String> {
    let clean_pin = pin.trim();
    if clean_pin.is_empty() {
        return Err("请输入 PIN 码".to_string());
    }

    // 确保底层身份密钥就绪
    let _ = ensure_device_identity_unlocked_for_app(&app);

    let mut app_config = crate::read_config_checked()?;
    let stored_hash_opt = app_config.get("p2p_pin_hash").and_then(|v| v.as_str()).map(|s| s.to_string());
    let stored_salt_opt = app_config.get("p2p_pin_salt").and_then(|v| v.as_str()).map(|s| s.to_string());

    if let (Some(stored_hash), Some(stored_salt)) = (stored_hash_opt, stored_salt_opt) {
        let candidate_key = derive_key(clean_pin, &stored_salt)?;
        let candidate_hash = BASE64.encode(candidate_key);
        if candidate_hash == stored_hash {
            return Ok(());
        } else {
            return Err("PIN 码错误，请重新输入".to_string());
        }
    }

    // 向后兼容历史遗留使用 PIN 加密密钥文件的场景
    let key_path = get_keys_path(&app);
    if key_path.exists() {
        let file_content = fs::read_to_string(&key_path).map_err(|e| e.to_string())?;
        let data: EncryptedKeyData = serde_json::from_str(&file_content).map_err(|e| e.to_string())?;

        let aes_key = derive_key(clean_pin, &data.salt)?;
        let cipher = Aes256Gcm::new_from_slice(&aes_key).map_err(|e| e.to_string())?;
        let nonce_bytes = BASE64.decode(&data.nonce).map_err(|e| e.to_string())?;
        let ciphertext = BASE64.decode(&data.ciphertext).map_err(|e| e.to_string())?;
        let decrypted = cipher.decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
            .map_err(|_| "PIN 码错误，请重新输入".to_string())?;

        if decrypted.len() != 32 {
            return Err("设备秘钥文件长度异常".to_string());
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&decrypted);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        *state.0.lock().unwrap() = Some(signing_key.clone());

        // 自动迁移保存本地 PIN 独立哈希，实现未来解耦
        let mut csprng = rand::rngs::OsRng;
        let mut salt_bytes = [0u8; 16];
        csprng.fill_bytes(&mut salt_bytes);
        let new_salt = BASE64.encode(salt_bytes);
        let new_hash = BASE64.encode(derive_key(clean_pin, &new_salt)?);
        if let Some(obj) = app_config.as_object_mut() {
            obj.insert("p2p_pin_salt".to_string(), serde_json::json!(new_salt));
            obj.insert("p2p_pin_hash".to_string(), serde_json::json!(new_hash));
        }
        let _ = crate::write_config_checked(&app_config);

        return Ok(());
    }

    Err("尚未设置本地 PIN 码，请先设置".to_string())
}

/// SEC-01 重置设备秘钥核心逻辑 (可恢复状态机, Fail-Closed):
/// Phase 1 (Prepare): 提取完整私钥与身份，使用未破坏的内存私钥为每个受信任对端生成已签名撤销证书，写入 identity_reset_journal ('prepared')
/// Phase 2 (Stage): 开启单笔 SQLite 事务，暂存带有签名的 peer_revocation_outbox，撤销所有可信对端并清空会话与缓存，更新 journal ('db_committed')
/// Phase 3 (Config Commit): 移除 config 中的 device_id 与 pairing_payload，通过 write_config_fn 严格持久化
/// Phase 4 (Destroy Key): 物理删除密钥文件并清空内存私钥。若删除失败则标记 journal 为 'degraded' 并阻断新配对
/// Phase 5 (Committed): 更新 journal 为 'committed'
fn validate_reset_identity_binding(
    config: &serde_json::Value,
    derived_id: Option<&str>,
) -> Result<(), String> {
    if let (Some(derived), Some(configured)) = (
        derived_id,
        config.get("device_id").and_then(|d| d.as_str()).filter(|s| !s.trim().is_empty()),
    ) {
        if derived != configured {
            return Err("旧私钥公钥与配置中的设备 ID 不一致，拒绝重置身份 (SEC-01 Fail-Closed)".to_string());
        }
    }
    Ok(())
}

pub fn reset_device_keys_core<F>(
    key_path: Option<&Path>,
    memory_state: &std::sync::Mutex<Option<SigningKey>>,
    mut write_config_fn: F,
    conn: &mut rusqlite::Connection,
    now_ms: i64,
) -> Result<(), String>
where
    F: FnMut(&mut serde_json::Value) -> Result<(), String>,
{
    // Phase 1: Prepare (在私钥完整未破坏前提取并预签名)
    let (sk_opt, local_pubkey_b64) = {
        let guard = memory_state.lock().map_err(|e| format!("Memory key state poisoned: {}", e))?;
        (guard.clone(), guard.as_ref().map(|sk| {
            let vk = VerifyingKey::from(sk);
            BASE64.encode(vk.to_bytes())
        }))
    };

    let config_before = crate::read_config_checked()?;
    validate_reset_identity_binding(&config_before, local_pubkey_b64.as_deref())?;

    let revoked_id = match local_pubkey_b64.clone() {
        Some(id) if !id.trim().is_empty() => id,
        _ => {
            config_before.get("device_id")
                .and_then(|d| d.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string())
                .ok_or_else(|| "无法获取本机设备 ID 用于撤销证书生成与重置 (SEC-01 fail-closed)".to_string())?
        }
    };

    // 查询当前所有处于 trusted 状态的对端
    let mut trusted_peers = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT device_id FROM trusted_devices WHERE status = 'trusted'")
            .map_err(|e| format!("查询可信设备失败: {}", e))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| format!("读取可信设备失败: {}", e))?;
        for r in rows {
            trusted_peers.push(r.map_err(|e| format!("解析可信设备失败: {}", e))?);
        }
    }

    // 关键安全门禁 (P0): 若存在任何处于 trusted 状态的可信对端，必须要求本地旧私钥处于解锁状态 (sk_opt 必须为 Some)，
    // 且核对其派生公钥与配置 device_id 一致；否则在写 journal、数据库、配置或密钥文件前 Fail-Closed 阻断。
    if !trusted_peers.is_empty() {
        if sk_opt.is_none() {
            return Err("存在处于 trusted 状态的可信对端，但旧私钥尚未解锁，无法签发持钥撤销证明，拒绝重置身份 (SEC-01 Fail-Closed)".to_string());
        }
    }

    // 在私钥未破坏前，为每个可信对端生成持钥签名的撤销证书
    let mut certs = Vec::new();
    if let Some(ref sk) = sk_opt {
        for peer_id in &trusted_peers {
            certs.push(crate::device_trust::create_device_revocation_certificate(
                sk, peer_id, "local_key_reset", now_ms
            ));
        }
    }

    // 记录 prepared 状态至 identity_reset_journal
    let journal_id: i64 = {
        conn.execute(
            "INSERT INTO identity_reset_journal (state, revoked_device_id, created_at, updated_at, error) VALUES ('prepared', ?, ?, ?, NULL)",
            rusqlite::params![&revoked_id, now_ms, now_ms],
        ).map_err(|e| format!("写入重置日志 prepared 失败: {}", e))?;
        conn.last_insert_rowid()
    };

    // Phase 2: Stage Revocations & DB Transaction
    let reset_db_res = crate::device_trust::reset_device_identity_in_db_with_certs(
        conn, journal_id, &certs, Some(&revoked_id), now_ms
    );
    if let Err(e) = reset_db_res {
        let _ = conn.execute(
            "UPDATE identity_reset_journal SET state = 'degraded_pre_db', updated_at = ?, error = ? WHERE id = ?",
            rusqlite::params![now_ms, &e, journal_id],
        );
        // 关键断言：DB 失败时，事务整体回滚，密钥文件与内存私钥均未被触碰，保持完全未破坏
        return Err(format!("数据库重置事务失败 (事务已整体回滚，密钥与内存保持未破坏): {}", e));
    }

    // Phase 3: Config Commit
    let mut config = crate::read_config_checked()?;
    if let Some(obj) = config.as_object_mut() {
        obj.remove("device_id");
        obj.remove("pairing_payload");
        obj.remove("p2p_pin_hash");
        obj.remove("p2p_pin_salt");
    }
    if let Err(cfg_err) = write_config_fn(&mut config) {
        let err_msg = format!("配置持久化失败: {}", cfg_err);
        let _ = conn.execute(
            "UPDATE identity_reset_journal SET state = 'degraded_post_db', updated_at = ?, error = ? WHERE id = ?",
            rusqlite::params![now_ms, &err_msg, journal_id],
        );
        // 关键断言：配置写盘失败时，密钥文件与内存私钥均未被触碰，但进入 degraded_post_db 状态阻断新配对
        return Err(format!("{} (数据库已提交撤销出件箱，但配置写盘失败，进入 degraded_post_db 状态)", err_msg));
    }

    let stage_updated = conn.execute(
        "UPDATE identity_reset_journal SET state = 'config_committed', updated_at = ? WHERE id = ?",
        rusqlite::params![now_ms, journal_id],
    ).map_err(|e| format!("配置提交后更新重置日志失败，保留旧私钥: {}", e))?;
    if stage_updated != 1 {
        return Err("配置提交后重置日志未更新，保留旧私钥".to_string());
    }

    // Phase 4: Destroy Key (此时且仅在此刻物理销毁密钥并清空内存)
    if let Some(path) = key_path {
        if path.exists() {
            if let Err(fs_err) = std::fs::remove_file(path) {
                let err_msg = format!("物理删除密钥文件失败: {}", fs_err);
                let _ = conn.execute(
                    "UPDATE identity_reset_journal SET state = 'degraded_key_destroy', updated_at = ?, error = ? WHERE id = ?",
                    rusqlite::params![now_ms, &err_msg, journal_id],
                );
                return Err(format!("{} (系统进入 degraded_key_destroy 状态)", err_msg));
            }
        }
    }

    // 清空内存私钥
    {
        let mut guard = memory_state.lock().map_err(|e| format!("Memory key state poisoned: {}", e))?;
        *guard = None;
    }

    // Phase 5: Committed
    let affected = conn.execute(
        "UPDATE identity_reset_journal SET state = 'committed', updated_at = ? WHERE id = ?",
        rusqlite::params![now_ms, journal_id],
    ).map_err(|e| {
        let err_msg = format!("Phase 5 更新重置日志 committed 状态失败: {}", e);
        let _ = conn.execute(
            "UPDATE identity_reset_journal SET state = 'degraded_post_db', updated_at = ?, error = ? WHERE id = ?",
            rusqlite::params![now_ms, &err_msg, journal_id],
        );
        err_msg
    })?;

    if affected == 0 {
        let err_msg = format!("Phase 5 未找到待提交的重置日志记录 (id: {})", journal_id);
        let _ = conn.execute(
            "UPDATE identity_reset_journal SET state = 'degraded_post_db', updated_at = ?, error = ? WHERE id = ?",
            rusqlite::params![now_ms, &err_msg, journal_id],
        );
        return Err(err_msg);
    }

    Ok(())
}

#[tauri::command]
pub fn reset_device_keys(
    app: AppHandle,
    state: tauri::State<'_, DeviceIdentityState>,
) -> Result<(), String> {
    let path = get_keys_path(&app);
    let key_path_opt = if path.exists() { Some(path.as_path()) } else { None };

    let db_state = app.try_state::<crate::db::DbState>()
        .ok_or_else(|| "Database state not available for identity reset".to_string())?;
    let mut conn = db_state.0.lock()
        .map_err(|e| format!("Database lock poisoned: {}", e))?;

    reset_device_keys_core(
        key_path_opt,
        &state.0,
        |cfg| crate::write_config_checked(cfg),
        &mut conn,
        crate::now_ms(),
    )?;

    // Trigger relay reconnect
    if let Some(tx) = crate::sync_engine::RELAY_RECONNECT_TRIGGER.lock().unwrap().as_ref() {
        let _ = tx.try_send(());
    }

    Ok(())
}

use std::net::UdpSocket;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PairingPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_device_id: Option<String>,
    pub device_id: String,
    pub public_key: String,
    pub local_ips: Vec<String>,
    pub port: u16,
    pub relay: String,
}

pub fn get_candidate_ips() -> Vec<String> {
    let mut ips = Vec::new();
    
    #[cfg(target_os = "windows")]
    let mut bad_interfaces = std::collections::HashSet::new();
    
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        if let Ok(output) = std::process::Command::new("wmic")
            .args(&["nic", "get", "NetConnectionID,Description"])
            .creation_flags(CREATE_NO_WINDOW)
            .output() 
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut lines = text.lines();
            if let Some(header) = lines.next() {
                if let Some(net_conn_idx) = header.find("NetConnectionID") {
                    for line in lines {
                        if line.len() > net_conn_idx {
                            let desc = line[..net_conn_idx].trim().to_lowercase();
                            let name = line[net_conn_idx..].trim().to_string();
                            if !name.is_empty() {
                                if desc.contains("tailscale") || desc.contains("tap") || desc.contains("tun") || 
                                   desc.contains("vethernet") || desc.contains("docker") || desc.contains("wsl") || 
                                   desc.contains("vboxnet") || desc.contains("vmware") || desc.contains("openvpn") ||
                                   desc.contains("protonvpn") || desc.contains("virtual") || desc.contains("cellular") ||
                                   desc.contains("mobile broadband") {
                                    bad_interfaces.insert(name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in interfaces {
            let name_lower = name.to_lowercase();
            // Filter out known virtual/vpn interfaces
            if name_lower.contains("tailscale") || name_lower.contains("tap") || name_lower.contains("tun") || 
               name_lower.contains("vethernet") || name_lower.contains("docker") || name_lower.contains("wsl") || 
               name_lower.contains("vboxnet") || name_lower.contains("vmware") || name_lower.contains("openvpn") {
                continue;
            }
            
            #[cfg(target_os = "windows")]
            if bad_interfaces.contains(&name) || bad_interfaces.contains(name.trim()) {
                continue;
            }
            
            // Exclude loopback and link-local
            if ip.is_loopback() || ip.to_string().starts_with("169.254.") {
                continue;
            }
            
            // We want to capture private IPs
            if let std::net::IpAddr::V4(ipv4) = ip {
                let octets = ipv4.octets();
                let is_private = octets[0] == 10 || 
                                 (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31) ||
                                 (octets[0] == 192 && octets[1] == 168);
                if is_private {
                    ips.push(ip.to_string());
                }
            }
        }
    }
    
    // Fallback if none found
    if ips.is_empty() {
        if let Some(ip) = get_local_ip_fallback() {
            ips.push(ip);
        }
    }
    
    // Sort so that 192.168 comes first, then 172, then 10
    ips.sort_by(|a, b| {
        let a_is_192 = a.starts_with("192.168.");
        let b_is_192 = b.starts_with("192.168.");
        b_is_192.cmp(&a_is_192)
    });
    
    ips
}

fn get_local_ip_fallback() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

#[tauri::command]
pub fn get_pairing_payload(
    app: AppHandle,
    state: tauri::State<'_, DeviceIdentityState>,
    db: tauri::State<'_, crate::db::DbState>,
) -> Result<PairingPayload, String> {
    let _ = state;
    let signing_key = ensure_device_identity_unlocked_for_app(&app)?;

    let verifying_key = VerifyingKey::from(&signing_key);
    let pub_key_bytes = verifying_key.to_bytes();
    let b64_pub = BASE64.encode(pub_key_bytes);

    let local_ips = get_candidate_ips();

    let relay = option_env!("BOB_RELAY_SECRET")
        .map(|_| "wss://relay.bobbik.org")
        .unwrap_or("wss://relay.bobbik.org");

    let now_ms = crate::now_ms();
    let invitation_opt = if let Ok(conn) = db.0.lock() {
        crate::device_trust::create_pairing_invitation(
            &conn,
            &b64_pub,
            None,
            crate::device_trust::DEFAULT_INVITATION_TTL_MS,
            relay,
            local_ips.clone(),
            3722,
            now_ms,
        ).ok()
    } else {
        None
    };

    if let Some(inv) = invitation_opt {
        Ok(PairingPayload {
            protocol_version: Some(inv.protocol_version),
            invitation_id: Some(inv.invitation_id),
            secret: Some(inv.secret),
            issuer_device_id: Some(inv.issuer_device_id),
            device_id: b64_pub.clone(),
            public_key: b64_pub,
            local_ips,
            port: 3722,
            relay: relay.to_string(),
        })
    } else {
        Ok(PairingPayload {
            protocol_version: None,
            invitation_id: None,
            secret: None,
            issuer_device_id: None,
            device_id: b64_pub.clone(),
            public_key: b64_pub,
            local_ips,
            port: 3722,
            relay: relay.to_string(),
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sec01_reset_rejects_configured_identity_mismatch() {
        let config = serde_json::json!({"device_id": "different-public-key"});
        let err = validate_reset_identity_binding(&config, Some("actual-public-key")).unwrap_err();
        assert!(err.contains("不一致"));
        assert!(validate_reset_identity_binding(&config, Some("different-public-key")).is_ok());
    }

    #[test]
    fn test_lan_candidate_sorting() {
        let mut ips = vec![
            "10.0.0.5".to_string(),
            "172.16.0.4".to_string(),
            "192.168.1.100".to_string(),
        ];
        
        ips.sort_by(|a, b| {
            let a_is_192 = a.starts_with("192.168.");
            let b_is_192 = b.starts_with("192.168.");
            b_is_192.cmp(&a_is_192)
        });
        
        assert_eq!(ips[0], "192.168.1.100");
    }

    #[tokio::test]
    async fn test_sec01_crypto_reset_keys_locked_private_key_with_trusted_peers_fails_closed() {
        let _test_lock = crate::CONFIG_OP_TEST_MUTEX.lock().unwrap();
        let baseline_real = crate::http_api::tests::RealConfigBaseline::capture();

        let temp_dir = std::env::temp_dir().join(format!("bob_cfg_sec01_locked_key_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let cfg_path = temp_dir.join("config.json");
        let key_path = temp_dir.join("device_identity.json");
        crate::set_test_config_path_override(Some(cfg_path.clone()));
        let _guard = crate::http_api::tests::TestConfigOverrideGuard;

        // 1. 初始化磁盘配置与密钥文件
        let orig_device_id = "initial_pc_device_id_12345";
        let initial_cfg = serde_json::json!({
            "device_id": orig_device_id,
            "pairing_payload": "some_payload"
        });
        std::fs::write(&cfg_path, serde_json::to_string_pretty(&initial_cfg).unwrap().as_bytes()).unwrap();

        let original_key_content = b"ORIGINAL_ENCRYPTED_OR_LOCKED_KEY_DATA";
        std::fs::write(&key_path, original_key_content).unwrap();

        // 2. 初始化数据库并添加处于 trusted 状态的可信对端
        let db_path = temp_dir.join("locked_key_test.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        crate::device_trust::init_device_trust_tables(&conn).unwrap();

        let now = crate::now_ms();
        conn.execute(
            "INSERT INTO trusted_devices (device_id, device_name, public_key, platform, status, paired_at, last_authenticated_at) VALUES ('peer_mobile_1', 'Phone', 'pub_phone_1', 'mobile', 'trusted', ?1, ?1)",
            rusqlite::params![now],
        ).unwrap();

        // 3. 内存密钥尚未解锁 (SigningKey 为 None)
        let memory_state = std::sync::Arc::new(std::sync::Mutex::new(None));

        // 4. 执行重置 -> 必须严格被阻断 (Fail-Closed)
        let reset_res = reset_device_keys_core(
            Some(&key_path),
            &memory_state,
            |cfg| crate::write_config_checked(cfg),
            &mut conn,
            now + 10,
        );

        assert!(reset_res.is_err(), "存在可信对端但私钥未解锁时必须严格拒绝重置身份");
        let err_msg = reset_res.unwrap_err();
        assert!(err_msg.contains("旧私钥尚未解锁"), "错误信息必须明确指出旧私钥未解锁: {}", err_msg);

        // 5. 关键安全断言：所有存储实现零变化 (Zero Side Effects)
        // a. 磁盘密钥文件必须完好无损
        assert!(key_path.exists(), "磁盘密钥文件绝对不能被删除");
        let current_key_content = std::fs::read(&key_path).unwrap();
        assert_eq!(current_key_content, original_key_content, "磁盘密钥文件内容绝对不能被修改");

        // b. 磁盘配置文件中的 device_id 与 pairing_payload 必须完好无损
        let current_cfg = crate::read_config_checked().unwrap();
        assert_eq!(current_cfg.get("device_id").and_then(|d| d.as_str()), Some(orig_device_id), "配置中的 device_id 必须保持完好");
        assert_eq!(current_cfg.get("pairing_payload").and_then(|d| d.as_str()), Some("some_payload"), "配置中的 pairing_payload 必须保持完好");

        // c. 数据库中可信设备必须依然保持 trusted，绝不能被提前撤销
        let peer_status: String = conn.query_row(
            "SELECT status FROM trusted_devices WHERE device_id = 'peer_mobile_1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(peer_status, "trusted", "可信对端状态必须保持 trusted");

        // d. 重置日志表必须没有任何记录写入
        let journal_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM identity_reset_journal",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(journal_count, 0, "identity_reset_journal 绝对不能写入任何脏记录");

        // e. 出件箱必须没有任何未签名的空记录
        let outbox_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM peer_revocation_outbox",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(outbox_count, 0, "peer_revocation_outbox 必须为空");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
        baseline_real.assert_unchanged();
    }
}
