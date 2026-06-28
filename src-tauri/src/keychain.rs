/// OS Keychain 模块 — 已停用
///
/// API Key 现在直接明文存储在 AppData/bob-agent/config.json 中。
/// 本模块保留为空壳，避免破坏 Cargo 编译依赖。
///
/// 如需重新启用 OS Keychain，恢复此文件中的实现代码即可。

/// 将 API Key 存入 OS Keychain（已停用，直接返回 Ok）
pub fn store_key(_provider: &str, _api_key: &str) -> Result<(), String> {
    Ok(())
}

/// 从 OS Keychain 读取 API Key（已停用，直接返回 None）
pub fn load_key(_provider: &str) -> Option<String> {
    None
}

/// 从 OS Keychain 删除 API Key（已停用，直接返回 Ok）
pub fn delete_key(_provider: &str) -> Result<(), String> {
    Ok(())
}

/// 启动时迁移明文 Key 到 Keychain（已停用，无操作）
pub fn migrate_plaintext_keys() {
    // 不再需要迁移；所有 Key 直接存在 config.json
}

/// 自定义模型 API Key 存入（已停用）
pub fn store_custom_model_key(_model_id: &str, _api_key: &str) -> Result<(), String> {
    Ok(())
}

/// 自定义模型 API Key 读取（已停用）
pub fn load_custom_model_key(_model_id: &str) -> Option<String> {
    None
}

/// 自定义模型 API Key 删除（已停用）
pub fn delete_custom_model_key(_model_id: &str) -> Result<(), String> {
    Ok(())
}
