/// OS Keychain 安全密钥存储模块
///
/// 使用操作系统的凭据管理器（Windows DPAPI / macOS Keychain / Linux Secret Service）
/// 安全存储 API Key，取代明文 config.json 存储。
///
/// 架构：
///   - config.json 中只保留 `"apiKeys": {"deepseek": "vaulted", ...}` 标记
///   - 真实密钥存入 OS Keychain，service = "com.bob.agent", user = provider_id
///   - 所有外部调用者（llm.rs, outbox.rs）通过本模块的公开接口操作密钥

const SERVICE_NAME: &str = "com.bob.agent";

/// 将 API Key 存入 OS Keychain
pub fn store_key(provider: &str, api_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, provider)
        .map_err(|e| format!("Keychain entry 创建失败: {}", e))?;
    entry.set_password(api_key)
        .map_err(|e| format!("Keychain 写入失败 (provider={}): {}", provider, e))
}

/// 从 OS Keychain 读取 API Key
pub fn load_key(provider: &str) -> Option<String> {
    let entry = keyring::Entry::new(SERVICE_NAME, provider).ok()?;
    entry.get_password().ok()
}

/// 从 OS Keychain 删除 API Key
pub fn delete_key(provider: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, provider)
        .map_err(|e| format!("Keychain entry 创建失败: {}", e))?;
    // 如果 key 本来就不存在，不算错误
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Keychain 删除失败 (provider={}): {}", provider, e)),
    }
}

/// 启动时一次性迁移：将 config.json 中的明文 API Key 迁移到 OS Keychain
/// 迁移成功后将原始明文替换为 "vaulted" 标记
pub fn migrate_plaintext_keys() {
    let config = super::read_config();

    // 收集需要迁移的 provider → key 对
    let mut to_migrate: Vec<(String, String)> = Vec::new();

    if let Some(api_keys) = config.get("apiKeys").and_then(|v| v.as_object()) {
        for (provider, value) in api_keys {
            if let Some(key_str) = value.as_str() {
                // 跳过已经迁移过的标记
                if key_str == "vaulted" || key_str.is_empty() {
                    continue;
                }
                to_migrate.push((provider.clone(), key_str.to_string()));
            }
        }
    }

    // 兼容旧版单一 apiKey 字段
    if let Some(legacy_key) = config.get("apiKey").and_then(|v| v.as_str()) {
        if !legacy_key.is_empty() && legacy_key != "vaulted" {
            if let Some(provider) = config.get("provider").and_then(|v| v.as_str()) {
                // 只有当 apiKeys 中还没有这个 provider 时才迁移
                let already_handled = to_migrate.iter().any(|(p, _)| p == provider);
                if !already_handled {
                    to_migrate.push((provider.to_string(), legacy_key.to_string()));
                }
            }
        }
    }

    if to_migrate.is_empty() {
        return;
    }

    log::info!("Keychain migration: found {} plaintext keys to migrate", to_migrate.len());

    let mut config = super::read_config();
    let mut migrated_count = 0;

    for (provider, key) in &to_migrate {
        match store_key(provider, key) {
            Ok(()) => {
                // 迁移成功，将 config.json 中的明文替换为 "vaulted"
                if let Some(api_keys) = config.get_mut("apiKeys").and_then(|v| v.as_object_mut()) {
                    api_keys.insert(provider.clone(), serde_json::json!("vaulted"));
                }
                migrated_count += 1;
                log::info!("Keychain migration: {} migrated successfully", provider);
            }
            Err(e) => {
                // 迁移失败，保留明文（降级），不要让用户丢失密钥
                log::warn!("Keychain migration failed for {}: {} — keeping plaintext", provider, e);
            }
        }
    }

    // 清理旧的单一 apiKey 字段
    if let Some(cfg_obj) = config.as_object_mut() {
        if cfg_obj.get("apiKey").and_then(|v| v.as_str()).map_or(false, |s| !s.is_empty() && s != "vaulted") {
            cfg_obj.insert("apiKey".to_string(), serde_json::json!(""));
        }
    }

    if migrated_count > 0 {
        super::write_config(&config);
        log::info!("Keychain migration complete: {}/{} keys migrated", migrated_count, to_migrate.len());
    }
}

/// 自定义模型的 API Key 存储（带前缀区分）
pub fn store_custom_model_key(model_id: &str, api_key: &str) -> Result<(), String> {
    let user = format!("custom_model:{}", model_id);
    let entry = keyring::Entry::new(SERVICE_NAME, &user)
        .map_err(|e| format!("Keychain entry 创建失败: {}", e))?;
    entry.set_password(api_key)
        .map_err(|e| format!("Keychain 写入失败 (model={}): {}", model_id, e))
}

/// 自定义模型的 API Key 读取
pub fn load_custom_model_key(model_id: &str) -> Option<String> {
    let user = format!("custom_model:{}", model_id);
    let entry = keyring::Entry::new(SERVICE_NAME, &user).ok()?;
    entry.get_password().ok()
}

/// 自定义模型的 API Key 删除
pub fn delete_custom_model_key(model_id: &str) -> Result<(), String> {
    let user = format!("custom_model:{}", model_id);
    let entry = keyring::Entry::new(SERVICE_NAME, &user)
        .map_err(|e| format!("Keychain entry 创建失败: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Keychain 删除失败 (model={}): {}", model_id, e)),
    }
}
