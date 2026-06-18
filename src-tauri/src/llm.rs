use futures_util::StreamExt;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

// ── 模型注册表路径 ─────────────────────────────────────
fn get_registry_path() -> PathBuf {
    super::get_data_dir().join("model_providers.json")
}

/// 从外部 model_providers.json 读取注册表
pub(crate) fn read_registry() -> Value {
    let path = get_registry_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(json) = serde_json::from_str(&data) {
            return json;
        }
    }
    // fallback: 返回空注册表
    json!({ "$schema_version": 1, "providers": [] })
}

/// 写入注册表
pub(crate) fn write_registry(registry: &Value) {
    let path = get_registry_path();
    if let Ok(data) = serde_json::to_string_pretty(registry) {
        let _ = std::fs::write(path, data);
    }
}

/// 合并内置默认模型与现有配置，按默认模板顺序重构，并迁移可见性状态
fn merge_registry_with_defaults(existing: &mut Value, defaults: &Value) -> bool {
    let mut modified = false;
    let default_version = defaults.get("$schema_version").and_then(|v| v.as_u64()).unwrap_or(1);
    let existing_version = existing.get("$schema_version").and_then(|v| v.as_u64()).unwrap_or(0);

    let default_providers = match defaults.get("providers").and_then(|v| v.as_array()) {
        Some(p) => p,
        None => return false,
    };

    // 构建推荐模型快速查找表: provider_id -> HashSet<model_id>
    let mut default_model_ids: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();
    for dp in default_providers {
        let pid = dp.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if let Some(models) = dp.get("models").and_then(|v| v.as_array()) {
            let ids: std::collections::HashSet<String> = models.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect();
            default_model_ids.insert(pid, ids);
        }
    }

    let mut existing_providers = existing.get("providers")
        .and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut merged_providers: Vec<Value> = Vec::new();

    for def_provider in default_providers {
        let def_id = def_provider.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if def_id.is_empty() { continue; }

        let existing_prov_idx = existing_providers.iter().position(|p| {
            p.get("id").and_then(|v| v.as_str()) == Some(def_id)
        });

        match existing_prov_idx {
            None => {
                merged_providers.push(def_provider.clone());
                modified = true;
                log::info!("Merged new default provider '{}'", def_id);
            }
            Some(idx) => {
                let mut ext_provider = existing_providers.remove(idx);

                for key in &["base_url", "supports_model_list", "base_url_variants", "auth_type"] {
                    if let Some(val) = def_provider.get(*key) {
                        if ext_provider.get(*key) != Some(val) {
                            ext_provider[key.to_string()] = val.clone();
                            modified = true;
                        }
                    }
                }

                let def_models = match def_provider.get("models").and_then(|v| v.as_array()) {
                    Some(m) => m,
                    None => { merged_providers.push(ext_provider); continue; }
                };

                if ext_provider.get("models").is_none() || !ext_provider["models"].is_array() {
                    ext_provider["models"] = json!([]);
                    modified = true;
                }
                let ext_models = ext_provider["models"].as_array_mut().unwrap();

                // 迁移清理：移除刚才误加的旧版本/无版本号模型
                if def_id == "qwen" {
                    let before_len = ext_models.len();
                    ext_models.retain(|m| {
                        let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        !["qwen-max", "qwen-plus", "qwen-turbo", "qwen2.5-max"].contains(&id)
                    });
                    if ext_models.len() < before_len {
                        modified = true;
                        log::info!("Cleaned up mistakenly added Qwen 2.5/legacy models");
                    }
                }

                // 追加默认列表中有但本地没有的新模型
                for def_model in def_models {
                    let dmid = def_model.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if dmid.is_empty() { continue; }
                    let exists = ext_models.iter().any(|m| m.get("id").and_then(|v| v.as_str()) == Some(dmid));
                    if !exists {
                        let mut m = def_model.clone();
                        if m.get("visible").is_none() {
                            if let Some(o) = m.as_object_mut() { o.insert("visible".to_string(), json!(true)); }
                        }
                        ext_models.push(m);
                        modified = true;
                        log::info!("Merged default model '{}' into provider '{}'", dmid, def_id);
                    }
                }

                // 可见性迁移：不在推荐列表中且没有 visible 字段的旧模型 -> hidden
                let curated = default_model_ids.get(def_id);
                for model in ext_models.iter_mut() {
                    if model.get("visible").is_some() { continue; }
                    let mid = model.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let is_curated = curated.map_or(false, |ids| ids.contains(&mid));
                    if let Some(obj) = model.as_object_mut() {
                        obj.insert("visible".to_string(), json!(is_curated));
                        if !is_curated {
                            log::info!("Auto-hiding non-curated model '{}' in '{}'", mid, def_id);
                        }
                        modified = true;
                    }
                }

                merged_providers.push(ext_provider);
            }
        }
    }

    // 用户自定义供应商追加在最后
    for custom_prov in existing_providers {
        merged_providers.push(custom_prov);
        modified = true;
    }

    if existing.get("providers").and_then(|v| v.as_array()) != Some(&merged_providers) {
        existing["providers"] = json!(merged_providers);
        modified = true;
    }
    if existing_version < default_version {
        existing["$schema_version"] = json!(default_version);
        modified = true;
    }
    modified
}

/// 初始化模型注册表：将内嵌默认值与本地文件合并
pub fn init_model_registry(_app: &tauri::AppHandle) {
    let dest = get_registry_path();
    let default_json = include_str!("../resources/model_providers.json");
    let default_registry: Value = match serde_json::from_str(default_json) {
        Ok(v) => v,
        Err(e) => { log::error!("Failed to parse embedded model registry: {}", e); return; }
    };
    if dest.exists() {
        log::info!("Model registry exists at {:?}, merging defaults", dest);
        let mut existing = read_registry();
        if merge_registry_with_defaults(&mut existing, &default_registry) {
            write_registry(&existing);
            log::info!("Model registry merged with new defaults");
        }
        return;
    }
    match std::fs::write(&dest, default_json) {
        Ok(_) => log::info!("Model registry initialized from embedded default"),
        Err(e) => log::warn!("Failed to write model registry: {}", e),
    }
}

/// 判断一个 model_id 是否为聊天/文本生成类模型
fn is_chat_model(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
    if id.starts_with("ft:") { return false; }
    // 精确匹配黑名单
    let exact_blocklist = ["davinci-002", "babbage-002"];
    if exact_blocklist.contains(&id.as_str()) { return false; }
    // 关键词黑名单
    let blocklist = [
        "embed", "embedding",
        "whisper", "tts", "audio", "speech",
        "dall-e", "dalle", "image", "stable-diffusion",
        "video", "sora",
        "moderation",
        "text-davinci", "text-curie", "text-babbage", "text-ada",
        "code-davinci", "code-cushman",
        "instruct",
        "realtime",
        "transcription", "translation",
        "search", "codex",
    ];
    for kw in &blocklist {
        if id.contains(kw) { return false; }
    }
    true
}

pub async fn refresh_models_on_startup() {
    let registry = read_registry();
    let config = super::read_config();
    let api_keys = config.get("apiKeys").cloned().unwrap_or(json!({}));
    
    let providers = match registry.get("providers").and_then(|v| v.as_array()) {
        Some(p) => p.clone(),
        None => return,
    };
    
    let mut updated = false;
    let mut new_registry = registry.clone();
    
    for (idx, provider) in providers.iter().enumerate() {
        let supports = provider.get("supports_model_list").and_then(|v| v.as_bool()).unwrap_or(false);
        if !supports { continue; }
        
        let provider_id = provider.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let api_key = api_keys.get(provider_id).and_then(|v| v.as_str()).unwrap_or("");
        if api_key.is_empty() { continue; }
        
        // Determine base_url, considering user's variant selection
        let base_url = resolve_provider_base_url(provider, &config);
        
        let models_url = format!("{}/models", base_url);
        log::info!("Refreshing models for provider '{}' from {}", provider_id, models_url);
        
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build() {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        let resp = match client.get(&models_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send().await {
            Ok(r) if r.status().is_success() => r,
            _ => {
                log::info!("Failed to refresh models for '{}', using cached list", provider_id);
                continue;
            }
        };
        
        let data: Value = match resp.json().await {
            Ok(d) => d,
            Err(_) => continue,
        };
        
        // Parse /v1/models response (OpenAI format: { data: [{ id: "...", ... }] })
        if let Some(model_list) = data.get("data").and_then(|v| v.as_array()) {
            let existing_models = provider.get("models").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let mut new_models: Vec<Value> = Vec::new();
            
            for api_model in model_list {
                let model_id = match api_model.get("id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => continue,
                };
                
                // Skip non-chat models (embedding, tts, image, video, etc.)
                if !is_chat_model(model_id) { continue; }
                
                // Try to find existing model entry to preserve pricing/vision/default info
                if let Some(existing) = existing_models.iter().find(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id)) {
                    new_models.push(existing.clone());
                } else {
                    // New model discovered — 自动发现默认 visible: false
                    new_models.push(json!({
                        "id": model_id,
                        "name": model_id,
                        "vision": false,
                        "visible": false,
                        "pricing": { "input": 0.0, "output": 0.0 }
                    }));
                }
            }
            
            // Only update if we got meaningful results (at least 1 model)
            if !new_models.is_empty() {
                // Preserve models that were in the old list but not returned by API
                // (they might be valid models not listed by the endpoint)
                for existing in &existing_models {
                    let eid = existing.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if !new_models.iter().any(|m| m.get("id").and_then(|v| v.as_str()) == Some(eid)) {
                        new_models.push(existing.clone());
                    }
                }
                
                if let Some(providers_arr) = new_registry.get_mut("providers").and_then(|v| v.as_array_mut()) {
                    if let Some(p) = providers_arr.get_mut(idx) {
                        p["models"] = json!(new_models);
                        updated = true;
                        log::info!("Updated models for '{}': {} models", provider_id, new_models.len());
                    }
                }
            }
        }
    }
    
    if updated {
        // Update last_updated timestamp
        new_registry["last_updated"] = json!(chrono::Local::now().format("%Y-%m-%d").to_string());
        write_registry(&new_registry);
        log::info!("Model registry updated and saved");
    }
}

/// Tauri command: 手动刷新指定供应商的模型列表
pub async fn refresh_models_for_provider(provider_id: String) -> Value {
    let registry = read_registry();
    let config = super::read_config();
    let api_keys = config.get("apiKeys").cloned().unwrap_or(json!({}));
    
    let api_key = match api_keys.get(&provider_id).and_then(|v| v.as_str()) {
        Some(k) if !k.is_empty() => k.to_string(),
        _ => return json!({ "error": format!("未配置 {} 的 API Key", provider_id) }),
    };
    
    let providers = match registry.get("providers").and_then(|v| v.as_array()) {
        Some(p) => p,
        None => return json!({ "error": "注册表格式错误" }),
    };
    
    let (idx, provider) = match providers.iter().enumerate().find(|(_, p)| p.get("id").and_then(|v| v.as_str()) == Some(&provider_id)) {
        Some(found) => found,
        None => return json!({ "error": format!("未找到供应商: {}", provider_id) }),
    };
    
    let supports = provider.get("supports_model_list").and_then(|v| v.as_bool()).unwrap_or(false);
    if !supports {
        return json!({ "error": format!("供应商 {} 不支持 /v1/models 查询", provider_id) });
    }
    
    let base_url = resolve_provider_base_url(provider, &config);
    let models_url = format!("{}/models", base_url);
    
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("HTTP client error: {}", e) }),
    };
    
    let resp = match client.get(&models_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return json!({ "error": format!("API 返回 {}", r.status()) }),
        Err(e) => return json!({ "error": format!("请求失败: {}", e) }),
    };
    
    let data: Value = match resp.json().await {
        Ok(d) => d,
        Err(e) => return json!({ "error": format!("解析失败: {}", e) }),
    };
    
    if let Some(model_list) = data.get("data").and_then(|v| v.as_array()) {
        let existing_models = provider.get("models").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut new_models: Vec<Value> = Vec::new();
        
        for api_model in model_list {
            let model_id = match api_model.get("id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => continue,
            };
            if !is_chat_model(model_id) { continue; }
            
            if let Some(existing) = existing_models.iter().find(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id)) {
                new_models.push(existing.clone());
            } else {
                // 手动刷新发现的新模型也默认 visible: false
                new_models.push(json!({
                    "id": model_id,
                    "name": model_id,
                    "vision": false,
                    "visible": false,
                    "pricing": { "input": 0.0, "output": 0.0 }
                }));
            }
        }
        
        for existing in &existing_models {
            let eid = existing.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if !new_models.iter().any(|m| m.get("id").and_then(|v| v.as_str()) == Some(eid)) {
                new_models.push(existing.clone());
            }
        }
        
        let mut new_registry = registry.clone();
        if let Some(providers_arr) = new_registry.get_mut("providers").and_then(|v| v.as_array_mut()) {
            if let Some(p) = providers_arr.get_mut(idx) {
                p["models"] = json!(&new_models);
            }
        }
        new_registry["last_updated"] = json!(chrono::Local::now().format("%Y-%m-%d").to_string());
        write_registry(&new_registry);
        
        json!({ "ok": true, "models_count": new_models.len() })
    } else {
        json!({ "error": "API 返回格式不匹配" })
    }
}

/// 解析供应商的 base_url（考虑 variants 和用户选择）
fn resolve_provider_base_url(provider: &Value, config: &Value) -> String {
    let base = provider.get("base_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let provider_id = provider.get("id").and_then(|v| v.as_str()).unwrap_or("");
    
    // Check if user has selected a variant
    let variant_key = format!("providerVariant_{}", provider_id);
    if let Some(variant) = config.get(&variant_key).and_then(|v| v.as_str()) {
        if !variant.is_empty() && variant != "default" {
            // Check for explicit api_base_{variant} field (e.g. api_base_coding)
            let explicit_key = format!("api_base_{}", variant);
            if let Some(explicit_url) = provider.get(&explicit_key).and_then(|v| v.as_str()) {
                return explicit_url.to_string();
            }
            
            if let Some(variants) = provider.get("base_url_variants").and_then(|v| v.as_object()) {
                if let Some(variant_url) = variants.get(variant).and_then(|v| v.as_str()) {
                    return variant_url.to_string();
                }
            }
        }
    }
    
    base
}

// ── 供应商默认模型 (从注册表动态读取) ───────────────────
fn get_default_model(provider: &str) -> String {
    let registry = read_registry();
    if let Some(providers) = registry.get("providers").and_then(|v| v.as_array()) {
        if let Some(p) = providers.iter().find(|p| p.get("id").and_then(|v| v.as_str()) == Some(provider)) {
            if let Some(models) = p.get("models").and_then(|v| v.as_array()) {
                // Find the model marked as default
                if let Some(default_model) = models.iter().find(|m| m.get("default").and_then(|v| v.as_bool()).unwrap_or(false)) {
                    return default_model.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                }
                // Fallback: first model
                if let Some(first) = models.first() {
                    return first.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                }
            }
        }
    }
    String::new()
}

// ── ModelHub API ─────────────────────────────────────────

pub fn get_models(provider_opt: Option<String>) -> Value {
    let provider = provider_opt.unwrap_or_else(|| {
        super::read_config().get("provider").and_then(|v| v.as_str()).unwrap_or("deepseek").to_string()
    });
    let pool = get_model_pool();
    if let Some(arr) = pool.as_array() {
        let filtered: Vec<Value> = arr.iter().filter(|m| {
            m.get("provider").and_then(|v| v.as_str()) == Some(provider.as_str())
        }).cloned().collect();
        return json!(filtered);
    }
    json!([])
}

pub fn get_model_pool() -> Value {
    let registry = read_registry();
    let mut pool: Vec<Value> = Vec::new();
    
    if let Some(providers) = registry.get("providers").and_then(|v| v.as_array()) {
        for provider in providers {
            let provider_id = provider.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let provider_name = provider.get("name").and_then(|v| v.as_str()).unwrap_or("");
            
            if let Some(models) = provider.get("models").and_then(|v| v.as_array()) {
                for model in models {
                    let model_id = model.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let mut model_name = model.get("name").and_then(|v| v.as_str()).unwrap_or(model_id);
                    if model_name.trim().is_empty() {
                        model_name = model_id;
                    }
                    let vision = model.get("vision").and_then(|v| v.as_bool()).unwrap_or(false);
                    let is_default = model.get("default").and_then(|v| v.as_bool()).unwrap_or(false);
                    let visible = model.get("visible").and_then(|v| v.as_bool()).unwrap_or(true);
                    let pricing = model.get("pricing").cloned().unwrap_or(json!({ "input": 0.0, "output": 0.0 }));
                    
                    if !visible {
                        continue;
                    }

                    let mut entry = json!({
                        "id": model_id,
                        "modelId": model_id,
                        "displayName": model_name,
                        "label": model_name,
                        "provider": provider_id,
                        "providerName": provider_name,
                        "vision": vision,
                        "pricing": pricing
                    });
                    
                    if is_default {
                        entry["default"] = json!(true);
                    }
                    
                    pool.push(entry);
                }
            }
        }
    }
    
    // 从配置中读取 customModels 并追加
    if let Some(custom_models) = super::read_config().get("customModels").and_then(|v| v.as_array()) {
        for cm in custom_models {
            pool.push(cm.clone());
        }
    }
    
    json!(pool)
}

pub fn get_active_models() -> Value {
    let config = super::read_config();
    let main = config.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let clerk = config.get("clerkModel").and_then(|v| v.as_str()).unwrap_or("").to_string();
    json!({ "main": main, "clerk": clerk })
}

pub fn assign_model_role(model_id: String, role: String) -> Value {
    let mut config = super::read_config();
    if let Some(obj) = config.as_object_mut() {
        if role == "main" {
            obj.insert("model".to_string(), json!(model_id));
        } else if role == "clerk" {
            obj.insert("clerkModel".to_string(), json!(model_id));
        }
    }
    super::write_config(&config);
    json!({ "ok": true })
}

pub fn get_api_keys() -> Value {
    let config = super::read_config();
    let mut keys = serde_json::Map::new();

    // 直接从 config.json 读取所有 API Key（明文存储）
    if let Some(api_keys) = config.get("apiKeys").and_then(|v| v.as_object()) {
        for (provider, value) in api_keys {
            if let Some(key_str) = value.as_str() {
                if !key_str.is_empty() && key_str != "vaulted" {
                    keys.insert(provider.clone(), json!(key_str));
                }
                // 兼容旧的 "vaulted" 标记：跳过，让用户重新输入
            }
        }
    }

    // 兼容旧的单一 apiKey 字段
    if keys.is_empty() {
        if let Some(legacy_provider) = config.get("provider").and_then(|v| v.as_str()) {
            if let Some(legacy_key) = config.get("apiKey").and_then(|v| v.as_str()) {
                if !legacy_key.is_empty() && legacy_key != "vaulted" {
                    keys.insert(legacy_provider.to_string(), json!(legacy_key));
                }
            }
        }
    }

    json!(keys)
}

pub fn set_api_key(provider_id: String, api_key: String) -> Value {
    let mut config = super::read_config();

    // 确保 apiKeys 对象存在
    if let Some(cfg_obj) = config.as_object_mut() {
        if !cfg_obj.contains_key("apiKeys") {
            cfg_obj.insert("apiKeys".to_string(), json!({}));
        }
    }

    if api_key.is_empty() {
        // 空字符串 = 删除该 Key
        if let Some(api_keys) = config.get_mut("apiKeys").and_then(|v| v.as_object_mut()) {
            api_keys.remove(&provider_id);
        }
    } else {
        // 直接明文写入 config.json
        if let Some(api_keys) = config.get_mut("apiKeys").and_then(|v| v.as_object_mut()) {
            api_keys.insert(provider_id.clone(), json!(api_key));
        }
    }

    super::write_config(&config);
    log::info!("set_api_key: provider={} saved to config.json", provider_id);
    json!({ "ok": true })
}

pub fn add_custom_model(model_id: String, display_name: String, provider: String, base_url: String, api_key: String) -> Value {
    let mut config = super::read_config();
    let mut custom_models = config.get("customModels").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    
    // 如果存在同名的，先移除
    custom_models.retain(|m| m.get("id").and_then(|v| v.as_str()) != Some(&model_id));
    
    // API Key 直接明文存入 config.json
    let model = json!({
        "id": model_id.clone(),
        "modelId": model_id,
        "displayName": display_name,
        "label": display_name,
        "provider": provider.clone(),
        "providerName": provider,
        "baseUrl": base_url,
        "apiKey": api_key,
        "vision": true,
        "pricing": { "input": 0.0, "output": 0.0 }
    });
    
    custom_models.push(model);
    
    if let Some(cfg_obj) = config.as_object_mut() {
        cfg_obj.insert("customModels".to_string(), json!(custom_models));
    }
    
    super::write_config(&config);
    json!({ "ok": true })
}

pub fn remove_custom_model(model_id: String) -> Value {
    let mut config = super::read_config();
    if let Some(custom_models) = config.get("customModels").and_then(|v| v.as_array()) {
        let mut new_models = custom_models.clone();
        new_models.retain(|m| m.get("id").and_then(|v| v.as_str()) != Some(&model_id));
        if let Some(cfg_obj) = config.as_object_mut() {
            cfg_obj.insert("customModels".to_string(), json!(new_models));
        }
        super::write_config(&config);
    }
    json!({ "ok": true })
}

/// 从 config.json 中读取 LLM 配置，包含动态 provider 解析
pub(crate) fn read_llm_config_for_model(model_id: &str) -> (String, String, String, String) {
    let config = super::read_config();
    
    // 1. 根据 model_id 从全局模型池反查 provider
    let pool = get_model_pool();
    let mut dynamic_provider = String::new();
    let mut custom_base_url = None;
    let mut custom_api_key = None;
    
    if let Some(arr) = pool.as_array() {
        if let Some(model_info) = arr.iter().find(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id)) {
            if let Some(p) = model_info.get("provider").and_then(|v| v.as_str()) {
                dynamic_provider = p.to_string();
            }
            if let Some(url) = model_info.get("baseUrl").and_then(|v| v.as_str()) {
                custom_base_url = Some(url.to_string());
            }
            // 自定义模型的 apiKey：直接从 config.json 读取明文
            if let Some(key_str) = model_info.get("apiKey").and_then(|v| v.as_str()) {
                if !key_str.is_empty() && key_str != "vaulted" {
                    custom_api_key = Some(key_str.to_string());
                }
            }
        }
    }
    
    // 如果没查到，降级使用全局配置中的 provider
    let provider = if !dynamic_provider.is_empty() {
        dynamic_provider
    } else {
        config.get("provider").and_then(|v| v.as_str()).unwrap_or("deepseek").to_string()
    };
    
    // ── Vertex AI 特殊处理 ──────────────────────────────────
    if provider == "vertex_ai" {
        let cred_path = super::gcp_auth::get_gcp_credential_path();
        if !cred_path.exists() {
            return (provider, String::new(), model_id.to_string(), String::new());
        }
        // 读取 project_id，构建动态 URL
        let project_id = config.get("gcpProjectId").and_then(|v| v.as_str()).unwrap_or("");
        let region = config.get("gcpVertexRegion").and_then(|v| v.as_str()).unwrap_or("us-central1");
        let base_url = super::gcp_auth::build_vertex_gemini_url(project_id, region);
        // 返回特殊标记 "__GCP_TOKEN__"，在 stream_internal 中动态换取真实 Token
        return (provider, "__GCP_TOKEN__".to_string(), model_id.to_string(), base_url);
    }
    
    // 2. 从 apiKeys 对象中获取对应 provider 的 Key (优先使用自定义模型自带的 apiKey 和 baseUrl)
    let api_keys_map = get_api_keys();
    let api_key = custom_api_key.unwrap_or_else(|| {
        api_keys_map.get(&provider).and_then(|v| v.as_str()).unwrap_or("").to_string()
    });
    let mut base_url = custom_base_url.unwrap_or_else(|| {
        config.get("baseURL").and_then(|v| v.as_str()).unwrap_or("").to_string()
    });
    
    // 从注册表动态解析 provider 的官方 base_url，替代硬编码
    let registry = read_registry();
    let config_for_variant = super::read_config();
    let official_base_url = registry.get("providers").and_then(|v| v.as_array())
        .and_then(|providers| {
            providers.iter()
                .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(provider.as_str()))
                .map(|p| resolve_provider_base_url(p, &config_for_variant))
        })
        .unwrap_or_default();
    
    // 判断当前 base_url 是否为用户自定义的代理（非官方域名）
    let is_custom_proxy = !base_url.is_empty() && !official_base_url.is_empty() && {
        // 提取域名部分进行比较
        let base_domain = base_url.split("//").nth(1).unwrap_or("").split('/').next().unwrap_or("");
        let official_domain = official_base_url.split("//").nth(1).unwrap_or("").split('/').next().unwrap_or("");
        base_domain != official_domain
    };
    
    if !is_custom_proxy {
        if !official_base_url.is_empty() {
            base_url = official_base_url;
        } else if base_url.is_empty() {
            base_url = "https://api.openai.com/v1".to_string();
        }
    }
    // else: 用户配置了非官方的自定义代理 URL，保持不变
    
    (provider, api_key, model_id.to_string(), base_url)
}

// ═══════════════════════════════════════════════════════════
// 技能摘要生成 (注入 System Prompt)
// ═══════════════════════════════════════════════════════════

/// 构建可用技能的简要列表，供 System Prompt 使用
fn build_skills_summary() -> String {
    let config = super::read_config();
    let bundled_dir = config.get("bundledSkillsDir").and_then(|v| v.as_str()).map(|s| std::path::Path::new(s).to_path_buf());
    let external_dir = config.get("externalSkillsDir").and_then(|v| v.as_str()).map(|s| std::path::Path::new(s).to_path_buf());

    let mut skills_map = std::collections::HashMap::new();

    let mut load_from_dir = |dir_opt: Option<&std::path::PathBuf>| {
        if let Some(dir_path) = dir_opt {
            if dir_path.exists() && dir_path.is_dir() {
                if let Ok(entries) = std::fs::read_dir(dir_path) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if !p.is_dir() { continue; }
                        let skill_md = p.join("SKILL.md");
                        if !skill_md.exists() { continue; }

                        let folder = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                        if let Ok(content) = std::fs::read_to_string(&skill_md) {
                            let (name, desc) = super::tools::parse_skill_frontmatter(&content, &folder);
                            let short_desc: String = desc.chars().take(80).collect();
                            skills_map.insert(folder.clone(), format!("- **{}** ({}): {}", name, folder, short_desc));
                        }
                    }
                }
            }
        }
    };

    // 先加载内置，再加载外部（外部覆盖内置同名技能）
    load_from_dir(bundled_dir.as_ref());
    load_from_dir(external_dir.as_ref());

    if skills_map.is_empty() { return String::new(); }

    let mut lines: Vec<String> = Vec::new();
    lines.push("\n## 可用技能库（可通过 read_skill 加载详细说明）".to_string());
    for summary in skills_map.values() {
        lines.push(summary.clone());
    }

    if lines.len() <= 1 { return String::new(); }
    lines.join("\n")
}

// ═══════════════════════════════════════════════════════════
// 记忆引擎摘要生成 (Tier 1 & 2)
// ═══════════════════════════════════════════════════════════

fn build_memory_summary() -> String {
    let data_dir = super::get_data_dir();
    let memory_dir = data_dir.join("memory");
    let soul_path = memory_dir.join("SOUL.md");
    
    let mut lines = Vec::new();

    // 1. 注入 Tier 1: 灵魂
    if soul_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&soul_path) {
            lines.push("\n## 你的记忆系统 (Tier 1: 灵魂)".to_string());
            lines.push(content);
        }
    }

    // 2. 注入 Tier 2: 短期记忆 (最近的 session)
    let sessions_dir = memory_dir.join("sessions");
    if sessions_dir.exists() {
        let mut sessions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().map_or(false, |ext| ext == "json" || ext == "md") {
                    if let Ok(meta) = std::fs::metadata(&p) {
                        if let Ok(modified) = meta.modified() {
                            sessions.push((p, modified));
                        }
                    }
                }
            }
        }
        
        // 按时间倒序，取最近的 3 个
        sessions.sort_by(|a, b| b.1.cmp(&a.1));
        if !sessions.is_empty() {
            lines.push("\n## 短期记忆 (Tier 2: 最近对话摘要)".to_string());
            for (p, _) in sessions.into_iter().take(3) {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if p.extension().map_or(false, |ext| ext == "json") {
                        if let Ok(v) = serde_json::from_str::<Value>(&content) {
                            if let Some(user_topics) = v.get("userTopics").and_then(|t| t.as_array()) {
                                let topics: Vec<String> = user_topics.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect();
                                lines.push(format!("- 讨论过的主题: {}", topics.join(", ")));
                            }
                        }
                    } else {
                        lines.push(content);
                    }
                }
            }
        }
    }

    lines.push("\n## 知识库检索 (Tier 3: 长期记忆)".to_string());
    lines.push("你有权限使用 brain_search 检索知识库中的文档和过往学到的知识。".to_string());
    lines.push("注意：你现在拥有自动自进化记忆系统。当用户告知你一条重要的架构原则、个人偏好，或你认为非常有必要长期记住的知识时，请在你的回复末尾悄悄加上一个不可见的标记 `<|mem|>`。你依然只需要正常回复用户，但必须带上这个标记。后台会自动提取并永久保存，**绝不要尝试手动调用 write_file 去保存记忆！**".to_string());

    lines.join("\n")
}

/// 构建 Wiki 知识库状态概览，注入 System Prompt
fn build_wiki_status() -> String {
    let wiki_dir = super::get_wiki_dir();
    let index_path = wiki_dir.join("index.md");

    if !index_path.exists() {
        return "\n## 知识库状态\n知识库为空。当用户拖入文件夹时，引导他们使用知识库构建功能。".to_string();
    }

    let mut lines = Vec::new();
    lines.push("\n## 知识库目录概览".to_string());

    if let Ok(content) = std::fs::read_to_string(&index_path) {
        // 截取前 2000 字符作为快速概览
        let preview: String = content.chars().take(2000).collect();
        lines.push(preview);
        if content.chars().count() > 2000 {
            lines.push("\n... (目录已截断，请使用 brain_search 或 read_file 获取详细信息)".to_string());
        }
    }

    lines.push("\n当用户提出事实性问题时，请先调用 brain_search 检索 Wiki 知识库。如果命中，用 read_file 加载完整页面后再回答，并引用来源路径。".to_string());

    lines.join("\n")
}

// ═══════════════════════════════════════════════════════════
// T-1411: 上下文分级压缩 (Context Tiering)
// ═══════════════════════════════════════════════════════════

use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::Lazy;

/// 摘要缓存: conversation_id → (messages_hash, summary_text)
/// messages_hash 用于检测消息列表是否变化，变了则需要重新压缩
static CONTEXT_SUMMARY_CACHE: Lazy<Mutex<HashMap<String, (u64, String)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 用牛马模型做非流式单轮对话（通用工具函数）
/// 失败时返回 None，不会 panic
async fn call_clerk_oneshot(system_prompt: &str, user_prompt: &str, max_tokens: u32) -> Option<String> {
    let config = super::read_config();
    let clerk_model = config.get("clerkModel").and_then(|v| v.as_str()).unwrap_or("");
    if clerk_model.is_empty() { return None; }

    let (provider, api_key, model_id, base_url) = read_llm_config_for_model(clerk_model);
    if api_key.is_empty() || base_url.is_empty() { return None; }

    let url = format!("{}/chat/completions", base_url);
    let body = json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "max_tokens": max_tokens,
        "stream": false,
        "temperature": 0.2
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;

    // GCP Token 动态获取
    let final_key = if provider == "google" || api_key == "__GCP_TOKEN__" {
        let cred_path = super::gcp_auth::get_gcp_credential_path();
        super::gcp_auth::GcpTokenManager::from_file(&cred_path).ok()?
            .get_access_token().await.ok()?
    } else {
        api_key
    };

    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", final_key))
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() { return None; }

    let resp_json: Value = resp.json().await.ok()?;
    resp_json.pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// 粗略估算消息的 Token 数（CJK 内容: ~2 chars/token, Latin: ~4 chars/token）
fn estimate_tokens(messages: &[Value]) -> usize {
    messages.iter()
        .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
        .map(|s| {
            // 粗略估算: CJK 字符占比高时用 2 chars/token, 否则 4
            let cjk_count = s.chars().filter(|c| *c > '\u{2E80}').count();
            let total = s.chars().count().max(1);
            let ratio = cjk_count as f32 / total as f32;
            let chars_per_token = if ratio > 0.3 { 2.0 } else { 4.0 };
            (total as f32 / chars_per_token).ceil() as usize
        })
        .sum()
}

/// 计算消息列表的简易哈希指纹（用于缓存失效判断）
fn hash_messages(messages: &[Value]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for m in messages {
        if let Some(c) = m.get("content").and_then(|v| v.as_str()) {
            // 只取前 100 字符做指纹，足够判断是否变化
            let snippet: String = c.chars().take(100).collect();
            snippet.hash(&mut hasher);
        }
    }
    messages.len().hash(&mut hasher);
    hasher.finish()
}

/// 对消息列表应用三层分级压缩:
///   - 活跃层 (最近 6 轮 = 12 条 user/assistant): 原样保留
///   - 摘要层 (7~20 轮): 由牛马模型压缩为摘要段落
///   - 废弃层 (20 轮以上): 直接丢弃，不进入上下文
///
/// 返回压缩后的消息列表（system 消息保留在最前面）
async fn apply_context_tiering(messages: Vec<Value>, conv_id: &str) -> Vec<Value> {
    // 分离 system 消息和对话消息
    let mut system_msgs: Vec<Value> = Vec::new();
    let mut dialog_msgs: Vec<Value> = Vec::new();

    for msg in &messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "system" {
            system_msgs.push(msg.clone());
        } else {
            dialog_msgs.push(msg.clone());
        }
    }

    // 计算对话轮次 (一个 user + 一个 assistant = 1 轮)
    let user_count = dialog_msgs.iter()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .count();

    // 活跃窗口: 最近 6 轮 (约 12 条消息)
    const ACTIVE_ROUNDS: usize = 6;
    // 摘要窗口上限: 20 轮
    const SUMMARY_MAX_ROUNDS: usize = 20;
    // Token 触发阈值: 对话消息超过此值才启动压缩
    const TOKEN_THRESHOLD: usize = 3000;

    // 不够 6 轮或 Token 未超阈值 → 不压缩
    if user_count <= ACTIVE_ROUNDS || estimate_tokens(&dialog_msgs) < TOKEN_THRESHOLD {
        return messages;
    }

    // 找到活跃层的起始位置: 从尾部倒数 ACTIVE_ROUNDS 个 user 消息
    let mut active_start_idx = dialog_msgs.len();
    let mut user_seen = 0;
    for (i, msg) in dialog_msgs.iter().enumerate().rev() {
        if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
            user_seen += 1;
            if user_seen == ACTIVE_ROUNDS {
                active_start_idx = i;
                break;
            }
        }
    }

    let active_msgs = dialog_msgs[active_start_idx..].to_vec();
    let older_msgs = &dialog_msgs[..active_start_idx];

    // 废弃层: 超过 SUMMARY_MAX_ROUNDS 轮的老消息直接丢弃
    let mut summary_msgs: Vec<&Value> = Vec::new();
    let mut summary_user_count = 0;
    for msg in older_msgs.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
            summary_user_count += 1;
            if summary_user_count > SUMMARY_MAX_ROUNDS - ACTIVE_ROUNDS {
                break;
            }
        }
        summary_msgs.push(msg);
    }
    summary_msgs.reverse();

    if summary_msgs.is_empty() {
        // 没有需要摘要的消息
        let mut result = system_msgs;
        result.extend(active_msgs);
        return result;
    }

    // 检查缓存: 如果摘要消息没变化，直接复用缓存
    let msgs_hash = hash_messages(older_msgs);
    if let Ok(cache) = CONTEXT_SUMMARY_CACHE.lock() {
        if let Some((cached_hash, cached_summary)) = cache.get(conv_id) {
            if *cached_hash == msgs_hash {
                let mut result = system_msgs;
                result.push(json!({
                    "role": "system",
                    "content": format!("[早期对话摘要]\n{}", cached_summary)
                }));
                result.extend(active_msgs);
                log::info!("T-1411: context tiering cache hit for conv {}", conv_id);
                return result;
            }
        }
    }

    // 构建待压缩内容
    // T-1411-b: 检测用户否决模式，排除被否决的 assistant 回复
    let rejection_patterns = ["不要", "换一个", "不行", "算了", "不用了", "不对", "错了", "重新", "别这样"];
    let mut skip_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (i, msg) in summary_msgs.iter().enumerate() {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "user" {
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            let is_rejection = rejection_patterns.iter().any(|p| content.contains(p));
            if is_rejection && i > 0 {
                // 跳过前一条 assistant 消息（被否决的方案）
                let prev_role = summary_msgs[i - 1].get("role").and_then(|r| r.as_str()).unwrap_or("");
                if prev_role == "assistant" {
                    skip_indices.insert(i - 1);
                    log::debug!("T-1411-b: skipping rejected proposal at index {}", i - 1);
                }
            }
        }
    }

    let mut compress_input = String::new();
    for (i, msg) in summary_msgs.iter().enumerate() {
        if skip_indices.contains(&i) { continue; }
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("?");
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        // 每条消息最多取 300 字符
        let snippet: String = content.chars().take(300).collect();
        let ellipsis = if content.chars().count() > 300 { "..." } else { "" };
        compress_input.push_str(&format!("[{}] {}{}\n", role, snippet, ellipsis));
    }

    // 调用牛马模型压缩
    let summary = call_clerk_oneshot(
        "你是一个对话历史压缩引擎。将用户提供的多轮对话记录压缩为简洁的摘要段落。\
         保留：关键决策、用户偏好、事实性结论、待办承诺。\
         丢弃：试探性讨论、被否决的方案、重复内容。\
         输出纯文本，不超过 200 字。",
        &compress_input,
        300
    ).await;

    match summary {
        Some(text) if !text.is_empty() => {
            // 写入缓存
            if let Ok(mut cache) = CONTEXT_SUMMARY_CACHE.lock() {
                cache.insert(conv_id.to_string(), (msgs_hash, text.clone()));
                // 缓存上限: 最多保留 20 个对话的摘要
                if cache.len() > 20 {
                    let oldest_key = cache.keys().next().cloned();
                    if let Some(k) = oldest_key { cache.remove(&k); }
                }
            }

            let discarded_count = active_start_idx - summary_msgs.len();
            log::info!(
                "T-1411: compressed {} msgs into summary for conv {} (discarded {} oldest msgs)",
                summary_msgs.len(), conv_id, discarded_count
            );

            let mut result = system_msgs;
            result.push(json!({
                "role": "system",
                "content": format!("[早期对话摘要]\n{}", text)
            }));
            result.extend(active_msgs);
            result
        }
        _ => {
            // 压缩失败，降级: 保留系统消息 + 活跃消息，丢弃老消息
            log::warn!("T-1411: clerk compression failed for conv {}, falling back to truncation", conv_id);
            let mut result = system_msgs;
            result.extend(active_msgs);
            result
        }
    }
}

// ═══════════════════════════════════════════════════════════
// T-1412-b: 即时纠错检测 (Correction Detection)
// ═══════════════════════════════════════════════════════════

/// 检测对话中用户是否纠正了 Bob 的错误认知，如果发现纠正：
/// 1. 降低相关旧记忆的 confidence
/// 2. 通过日志记录纠正事件（新记忆由正常的 session summarize 流程生成）
fn detect_and_apply_corrections(messages: &[Value], conv_id: &str) {
    let correction_patterns = [
        "不对", "错了", "其实是", "应该是", "不是这样",
        "你记错了", "不准确", "纠正一下", "更正",
    ];

    // 提取用户最后 3 条消息，检测是否包含纠正模式
    let recent_user_msgs: Vec<&str> = messages.iter().rev()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .take(3)
        .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
        .collect();

    let mut corrections_found = 0u32;
    for user_msg in &recent_user_msgs {
        let has_correction = correction_patterns.iter().any(|p| user_msg.contains(p));
        if !has_correction { continue; }

        corrections_found += 1;

        // 提取纠正内容中的关键词（用于匹配旧记忆）
        // 简易策略: 取纠正语句中的名词短语（>= 2 字的非停用词片段）
        let keywords: Vec<&str> = user_msg.split(|c: char| !c.is_alphanumeric() && c < '\u{2E80}')
            .filter(|w| w.len() >= 4 || w.chars().count() >= 2) // 2字中文或4字英文
            .take(5)
            .collect();

        if keywords.is_empty() { continue; }

        // 扫描 memory/sessions/ 目录中的 JSON 文件，查找匹配的旧记忆
        let sessions_dir = super::get_data_dir().join("memory").join("sessions");
        if !sessions_dir.exists() { continue; }

        let entries: Vec<std::path::PathBuf> = match std::fs::read_dir(&sessions_dir) {
            Ok(rd) => rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |ext| ext == "json"))
                .collect(),
            Err(_) => continue,
        };

        for path in entries {
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // 检查 userTopics 是否包含纠正相关的关键词
            let has_match = keywords.iter().any(|kw| content.contains(kw));
            if !has_match { continue; }

            // 降低该记忆的 confidence
            if let Ok(mut session) = serde_json::from_str::<Value>(&content) {
                let old_confidence = session.get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.8);

                if let Some(obj) = session.as_object_mut() {
                    obj.insert("confidence".to_string(), serde_json::json!(
                        (old_confidence * 0.5).max(0.0) // 被纠正后 confidence 减半
                    ));
                    obj.insert("source".to_string(), serde_json::json!("corrected"));
                    obj.insert("correctedBy".to_string(), serde_json::json!(conv_id));

                    if let Ok(data) = serde_json::to_string_pretty(&session) {
                        let _ = std::fs::write(&path, data);
                        log::info!(
                            "T-1412-b: corrected memory {:?} (confidence: {:.2} → {:.2})",
                            path.file_name().unwrap_or_default(),
                            old_confidence,
                            old_confidence * 0.5
                        );
                    }
                }
            }
        }
    }

    if corrections_found > 0 {
        log::info!("T-1412-b: detected {} correction(s) in conv {}", corrections_found, conv_id);
    }
}

// ═══════════════════════════════════════════════════════════
// Tool Calling 引擎 (T-903/T-904)
// ═══════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════
// T-1431: 任务复杂度感知路由 (Complexity-Aware Routing)
// ═══════════════════════════════════════════════════════════

/// 复杂度等级
#[derive(Debug, Clone, Copy)]
enum Complexity {
    /// 简单闲聊/查询 (score 0-30): 用默认模型
    Simple,
    /// 中等任务 (score 31-65): 用默认模型
    Medium,
    /// 复杂推理/多步任务 (score 66-100): 升级到 thinkModel
    Complex,
}

/// 纯 Rust 复杂度估算器 — 不调用 LLM，零延迟
/// 从用户最后一条消息中提取信号，综合评分
fn estimate_complexity(messages: &[Value]) -> (Complexity, u32) {
    // 提取用户最后一条消息
    let last_user_msg = messages.iter().rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .and_then(|m| m.get("content").and_then(|c| c.as_str()))
        .unwrap_or("");

    if last_user_msg.is_empty() {
        return (Complexity::Simple, 0);
    }

    let mut score: u32 = 0;

    // 信号 1: 消息长度 (长消息通常意味着复杂需求)
    let char_count = last_user_msg.chars().count();
    score += match char_count {
        0..=50 => 0,
        51..=200 => 10,
        201..=500 => 20,
        _ => 30,
    };

    // 信号 2: 代码块存在 (技术任务)
    if last_user_msg.contains("```") {
        score += 15;
    }

    // 信号 3: 多步骤指示词
    let multi_step_keywords = [
        "然后", "接着", "首先", "其次", "最后",
        "第一步", "第二步", "步骤",
        "分析", "对比", "比较",
        "帮我写", "帮我实现", "帮我设计",
        "重构", "优化", "修改",
        "then", "after that", "step by step",
        "implement", "refactor", "design",
    ];
    let keyword_hits: u32 = multi_step_keywords.iter()
        .filter(|kw| last_user_msg.contains(*kw))
        .count() as u32;
    score += (keyword_hits * 8).min(25);

    // 信号 4: 推理/分析关键词
    let reasoning_keywords = [
        "为什么", "怎么办", "如何", "解释", "原因",
        "推导", "证明", "评估", "权衡", "取舍",
        "why", "how to", "explain", "evaluate",
        "trade-off", "pros and cons",
    ];
    let reasoning_hits: u32 = reasoning_keywords.iter()
        .filter(|kw| last_user_msg.contains(*kw))
        .count() as u32;
    score += (reasoning_hits * 5).min(15);

    // 信号 5: 对话深度 (多轮对话意味着问题可能在深入)
    let total_user_msgs = messages.iter()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .count();
    if total_user_msgs >= 5 {
        score += 10;
    }

    score = score.min(100);

    let complexity = match score {
        0..=30 => Complexity::Simple,
        31..=65 => Complexity::Medium,
        _ => Complexity::Complex,
    };

    (complexity, score)
}

/// 内部通用流式处理 — 支持 Tool Calling 循环
async fn stream_internal(
    app: AppHandle,
    messages: Vec<Value>,
    conv_id: Option<String>,
    from_user: Option<String>,
    global_file_access: bool,
    agent_mode: String,
) -> Value {
    // conv_id 用于标记 llm:chunk 事件属于哪个会话，防止跨会话串流
    let conv_id_for_emit = conv_id.clone().unwrap_or_default();
    // 1. 读取 LLM 配置
    let config = super::read_config();
    let config_model_id = config.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (provider, mut api_key, model_override, custom_base_url) = read_llm_config_for_model(&config_model_id);

    // ── Vertex AI: 动态换取 GCP Access Token ──────────────
    if api_key == "__GCP_TOKEN__" {
        let cred_path = super::gcp_auth::get_gcp_credential_path();
        match super::gcp_auth::GcpTokenManager::from_file(&cred_path) {
            Ok(manager) => {
                match manager.get_access_token().await {
                    Ok(token) => { api_key = token; }
                    Err(e) => {
                        return json!({ "error": format!("GCP Token 获取失败: {}", e) });
                    }
                }
            }
            Err(e) => {
                return json!({ "error": format!("GCP 凭证加载失败: {}，请在设置中重新上传凭证文件", e) });
            }
        }
    }

    if api_key.is_empty() && provider != "offline" {
        return json!({ "error": "API Key 未配置或被清空，请前往设置检查" });
    }

    // 使用 read_llm_config_for_model 返回的智能路由 base_url（已包含按 provider 自动匹配官方域名的逻辑）
    if custom_base_url.is_empty() {
        return json!({ "error": format!("未知的供应商: {}，请检查模型配置", provider) });
    }

    let mut model_id = if model_override.is_empty() {
        get_default_model(&provider)
    } else {
        model_override.clone()
    };

    // ── T-1431: 复杂度感知路由 ──────────────────────────────
    // 纯 Rust 启发式估算，零 LLM 开销。如果任务复杂且配置了 thinkModel，自动升级
    let mut provider = provider;
    let (complexity, complexity_score) = estimate_complexity(&messages);
    let mut routed_to_think = false;
    // T-1431-b: 配置开关 (默认 true)
    let auto_upgrade_enabled = config.get("autoModelUpgrade")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if auto_upgrade_enabled {
        if let Complexity::Complex = complexity {
            let think_model = config.get("thinkModel").and_then(|v| v.as_str()).unwrap_or("");
            if !think_model.is_empty() && think_model != config_model_id {
                let (tp, tk, tm, tb) = read_llm_config_for_model(think_model);
                if !tk.is_empty() && !tb.is_empty() {
                    log::info!(
                        "T-1431: complexity={} (score={}), upgrading {} → {}",
                        "Complex", complexity_score, model_id, tm
                    );
                    provider = tp;
                    api_key = if tk == "__GCP_TOKEN__" {
                        // 对 GCP thinkModel 也要动态换 token
                        let cred_path = super::gcp_auth::get_gcp_credential_path();
                        match super::gcp_auth::GcpTokenManager::from_file(&cred_path) {
                            Ok(mgr) => match mgr.get_access_token().await {
                                Ok(token) => token,
                                Err(_) => api_key, // 降级回原 key
                            },
                            Err(_) => api_key,
                        }
                    } else {
                        tk
                    };
                    model_id = tm.clone();
                    routed_to_think = true;

                    // T-1431-b: 通知前端模型已升级，前端可显示淡色提示
                    let _ = app.emit("llm:model-routed", json!({
                        "from": config_model_id,
                        "to": tm,
                        "reason": "complexity",
                        "score": complexity_score,
                        "conv_id": &conv_id_for_emit
                    }));
                }
            }
        }
    }

    // 重新计算 base_url（如果路由到 thinkModel，使用其 base_url）
    let base_url = if routed_to_think {
        let think_model = config.get("thinkModel").and_then(|v| v.as_str()).unwrap_or("");
        let (_, _, _, tb) = read_llm_config_for_model(think_model);
        tb
    } else {
        custom_base_url
    };

    if !routed_to_think {
        log::debug!("T-1431: complexity score={}, using default model {}", complexity_score, model_id);
    }

    // 2. 构建系统提示词 + 消息
    let has_system = messages.iter().any(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"));
    let mut full_messages: Vec<Value> = Vec::new();
    if !has_system {
        let current_dir = std::env::current_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "未知".to_string());
        let os_info = std::env::consts::OS;
        let skills_summary = build_skills_summary();
        let memory_summary = build_memory_summary();
        let wiki_status = build_wiki_status();
        let wxid_info = if let Some(u) = &from_user {
            format!("当前对话的微信用户 ID (wxid) 是: {}\n", u)
        } else {
            String::new()
        };

        let agent_mode_info = if agent_mode == "yolo" {
            "\n## 工作模式：干活模式 (YOLO)\n你当前处于高度授权的“干活模式”。你可以大胆使用文件操作等工具完成用户的请求，无需反复向用户确认。\n"
        } else {
            ""
        };

        let system_prompt = format!(
            "你是 Bob，一个友善、专业的桌面 AI 私人助手，由 Tauri (Rust) 和 Vue 3 构建。\n\
你当前运行在用户的本地计算机上。\n\
当前操作系统: {}\n\
当前工作目录 (CWD): {}\n\
{}\n\
请用中文回答用户的问题，并记住你是一个拥有本机访问能力的桌面助手，而不是一个受限的云端网页服务。\n\
\n\
## 工具调用能力\n\
你拥有以下工具，可以主动调用来完成用户的请求：\n\
- **read_file**: 读取用户磁盘上的文本文件\n\
- **list_dir**: 浏览目录内容，帮用户找文件\n\
- **fetch_url**: 抓取网页内容\n\
- **web_search**: 搜索互联网获取实时信息（Tavily/TinyFish 双引擎）\n\
- **write_file**: 将内容写入 wiki/ 目录保存知识\n\
- **append_file**: 向 wiki/index.md 或 wiki/log.md 追加内容\n\
- **brain_search**: 检索你维护的 Wiki 知识库\n\
- **list_skills**: 查看可用的专业分析框架
- **read_skill**: 加载某个技能的详细指南（加载后请严格遵循其工作流程）
- **read_model_registry**: 读取当前 AI 模型注册表（查看/对比供应商模型列表）
- **test_model_endpoint**: 测试某个模型 ID 的 API 连通性（验证模型是否可用）
- **update_model_registry**: 更新指定供应商的模型列表（必须先 test 验证）
- **create_directory**: 创建新文件夹（仅在干活模式可用）
- **move_file**: 移动文件或文件夹（仅在干活模式可用）
- **copy_file**: 复制文件（仅在干活模式可用）
- **delete_file**: 安全删除文件或目录到系统回收站（仅在干活模式可用）
- **rename_file**: 重命名文件或目录（仅在干活模式可用）
{}
## 文档输出能力
你能够为用户生成并导出专业级别的文档，请在以下场景主动调用对应的导出工具：
- **export_html**: 生成精美排版的 HTML 分析报告/周报。这是你的**首选和主力**文档输出方式。用户可以通过浏览器原生的打印功能(Ctrl+P)将其完美导出为 PDF (已适配 @media print 分页规则)。
- **export_xlsx**: 当用户让你\"提取这些数据\"、\"整理成表格发我\"时，生成结构化的 Excel 表格。
- **export_docx**: 当用户需要一份正式的文字文档（带标题层级）时，生成 Word 文件。
- **export_pptx**: 当用户要求生成 PPT 时调用。请注意你需要先通过 read_skill 加载 mckinsey-designer 等排版技能生成对应的 Storyboard JSON。

当用户提到文件路径时，请主动调用 read_file 读取；当用户要求分析/规划时，先 read_skill 加载对应框架。
当用户提出事实性问题时，请先调用 brain_search 检索知识库。

## 格式规范：文件路径与图片显示
**重要**：当你在回复中提到任何本地文件路径时，必须使用 markdown 链接格式，使路径可被用户点击打开：
- 正确格式：`[文件名](file:///C:/path/to/file.ext)`（注意使用正斜杠）
- 错误格式：直接写 `C:\\path\\to\\file.ext`（用户无法点击）
- 示例：文件已保存到 [report.html](file:///C:/Users/xm_bo/Desktop/Bob-Exports/report.html)

**展示本地图片（极其重要）**：如果用户要求你发送、展示或查看一张本地图片，你**绝对可以**直接在聊天窗口中显示它！
你只需使用 Markdown 的图片语法加上绝对路径即可（前端已支持直接渲染）：
- 图片显示语法：`![图片描述](file:///C:/path/to/image.png)`
- 绝对不要回答“我是一个文本模型无法发送图片”或“我无法直接在对话窗口插入图片”。你完全可以！只要输出上述语法，图片就会直接展示在对话流中。
{}
{}
{}
## 自主配置能力\n\
当用户要求你帮忙配置 API Key、切换模型、修改主题等系统设置时，请在回复末尾输出 bob-config 代码块：\n\
\n\
```bob-config\n\
{{\\\"op\\\": \\\"set_api_key\\\", \\\"provider\\\": \\\"openai\\\", \\\"value\\\": \\\"sk-xxxx\\\"}}\n\
```\n\
\n\
支持的操作：\n\
- set_api_key: provider 可选 deepseek, openai, qwen, doubao, zhipu, kimi, minimax, tavily, tinyfish\n\
- set_config: key 可选 model, clerkModel, theme, uiScale, language, workspaceDir\n\
\n\
如果用户发送了包含 API Key 的文件或文本，请提取密钥并使用上述格式帮用户配置好。\n\
\n\
## 行动项捕获\n\
在对话过程中，如果你发现用户提到了需要做的事情（待办、提醒、日程、承诺、计划），\n\
请在回复的最末尾附加一个 bob-action-items JSON 代码块来提取它们。格式如下：\n\
\n\
```bob-action-items\n\
[{{\\\"title\\\": \\\"事项标题\\\", \\\"type\\\": \\\"todo\\\", \\\"date\\\": \\\"YYYY-MM-DD\\\"}}]\n\
```\n\
\n\
规则：\n\
- type 可选 todo 或 event\n\
- date 可以为 null（如果没有明确时间）\n\
- 只在确实检测到行动项时才输出此代码块，不要强行捕获\n\
- 不要在普通问答、闲聊中输出此代码块",
            os_info, current_dir, wxid_info, agent_mode_info, skills_summary, memory_summary, wiki_status
        );

        full_messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }
    full_messages.extend(messages);

    // T-1411: 上下文分级压缩 — 如果对话过长，压缩旧消息为摘要
    let tiering_conv_id = conv_id.clone().unwrap_or_default();
    if !tiering_conv_id.is_empty() {
        full_messages = apply_context_tiering(full_messages, &tiering_conv_id).await;
    }

    // 3. 获取工具 Schema
    let tool_schemas = super::tools::get_tool_schemas_with_mcp().await;

    // 4. 构建 HTTP 客户端
    let url = format!("{}/chat/completions", base_url);
    let client = if provider == "offline" {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    } else {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    };

    // ═══════════════════════════════════════════════════════════
    // Tool Calling 循环 (最多 MAX_TOOL_ROUNDS 轮)
    // ═══════════════════════════════════════════════════════════
    const MAX_TOOL_ROUNDS: usize = 5;
    let mut final_content = String::new();
    let mut final_thinking = String::new();
    let mut final_usage: Option<Value> = None;

    // ── 进化引擎: 遥测计数器 ──────────────────────────────
    let start_time = std::time::Instant::now();
    let mut tool_calls_total: i64 = 0;
    let mut tool_failures_total: i64 = 0;
    let mut rounds_completed: usize = 0;

    // ── T-1401: 循环熔断器 ──────────────────────────────────
    let mut tool_tracker = super::tools::ToolCallTracker::new();

    // ── 工具结果缓存 (会话级) ────────────────────────────────
    // 避免同一对话中重复读取同一文件/目录，节省 Token 和时间
    let mut tool_cache: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    let cacheable_tools: std::collections::HashSet<&str> = [
        "read_file", "list_dir", "list_skills", "read_skill", "system_time",
    ].iter().copied().collect();
    let cache_invalidating_tools: std::collections::HashSet<&str> = [
        "write_file", "append_file", "create_directory", "move_file",
        "copy_file", "delete_file", "rename_file",
    ].iter().copied().collect();

    for round in 0..=MAX_TOOL_ROUNDS {
        // 构建请求体
        let mut body = json!({
            "model": model_id,
            "messages": full_messages,
            "stream": true,
        });

        // Minimax 和本地 offline 模型可能不支持 stream_options，会导致 400 报错
        if provider != "minimax" && provider != "offline" {
            body["stream_options"] = json!({ "include_usage": true });
        }

        // 添加工具定义（最后一轮不带工具，防止无限循环）
        if round < MAX_TOOL_ROUNDS && !tool_schemas.is_empty() {
            body["tools"] = json!(tool_schemas);
        }

        // 检查是否包含图片 (T-1422 DeepSeek Vision 兼容)
        let mut has_image = false;
        for msg in &full_messages {
            if let Some(content) = msg.get("content") {
                if let Some(arr) = content.as_array() {
                    for item in arr {
                        if item.get("type").and_then(|v| v.as_str()) == Some("image_url") {
                            has_image = true;
                            break;
                        }
                    }
                }
            }
            if has_image { break; }
        }

        // DeepSeek 专有参数
        if provider == "deepseek" {
            // 如果包含图片，则不传入 thinking 参数，否则 DeepSeek 的 API 会尝试使用 Reasoner schema 验证导致报错 (unknown variant 'image_url', expected 'text')
            if !has_image {
                body["thinking"] = json!({ "type": "enabled" });
                body["reasoning_effort"] = json!("low");
            }
            body["max_completion_tokens"] = json!(16384);
        } else {
            body["max_tokens"] = json!(8192);
        }

        // 发送请求
        let response = match client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return json!({ "error": format!("网络请求失败: {}", e) }),
        };

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            let safe_text: String = text.chars().take(200).collect();
            let msg = match status {
                401 => "API Key 无效，请在设置中检查".to_string(),
                429 => "请求太频繁，请稍后再试".to_string(),
                _ => format!("API 错误 ({}): {}", status, safe_text),
            };
            return json!({ "error": msg });
        }

        // ── SSE 流式解析（含 Tool Calls 增量累加）────────
        let mut content = String::new();
        let mut thinking_content = String::new();
        let mut usage_data: Option<Value> = None;
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        // T-904: Tool call 增量累加器
        struct PendingToolCall { id: String, name: String, arguments: String, thought_signature: String }
        let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
        let mut has_tool_calls = false;

        // <think> 标签解析状态机 (MiniMax/Qwen 等通过 OpenAI 兼容接口返回思考链)
        let mut inside_think_tag = false;
        let mut think_tag_buffer = String::new();

        // ── IPC 降压器 (Debounce Buffer) ────────────────
        // 将高频的逐 Token emit 合并为每 30ms 一次批量发送，
        // 削减约 80% 的跨进程序列化开销，防止前端渲染线程卡顿或 OOM 白屏。
        let mut text_emit_buf = String::new();
        let mut thinking_emit_buf = String::new();
        let mut last_emit_time = std::time::Instant::now();
        const EMIT_INTERVAL_MS: u128 = 30;
        const EMIT_BUFFER_SIZE: usize = 4;

        while let Some(chunk_result) = stream.next().await {
            let bytes = match chunk_result {
                Ok(b) => b,
                Err(_) => break,
            };
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" { continue; }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        // 提取 usage
                        if let Some(usage) = parsed.get("usage") {
                            if usage.is_object() && usage.get("total_tokens").is_some() {
                                usage_data = Some(usage.clone());
                            }
                        }

                        if let Some(choice) = parsed.pointer("/choices/0") {
                            // 检查 finish_reason
                            if let Some(fr) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                                if fr == "tool_calls" { has_tool_calls = true; }
                            }

                            if let Some(delta) = choice.get("delta") {
                                // 思考内容 (DeepSeek)
                                if let Some(reasoning) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                                    if !reasoning.is_empty() {
                                        thinking_content.push_str(reasoning);
                                        thinking_emit_buf.push_str(reasoning);
                                    }
                                }
                                // 正文内容（含 <think> 标签解析）
                                if let Some(c) = delta.get("content").and_then(|v| v.as_str()) {
                                    if !c.is_empty() {
                                        // 将当前 chunk 追加到 tag buffer 做标签解析
                                        think_tag_buffer.push_str(c);

                                        // 循环提取 <think> / </think> 边界
                                        loop {
                                            if inside_think_tag {
                                                // 在 <think> 块内部，等待 </think>
                                                if let Some(end_pos) = think_tag_buffer.find("</think>") {
                                                    let thinking_chunk = &think_tag_buffer[..end_pos];
                                                    if !thinking_chunk.is_empty() {
                                                        thinking_content.push_str(thinking_chunk);
                                                        thinking_emit_buf.push_str(thinking_chunk);
                                                    }
                                                    think_tag_buffer = think_tag_buffer[end_pos + 8..].to_string(); // 8 = "</think>".len()
                                                    inside_think_tag = false;
                                                } else {
                                                    // 还没收到闭合标签，全部作为思考内容输出
                                                    let thinking_chunk = think_tag_buffer.clone();
                                                    if !thinking_chunk.is_empty() {
                                                        thinking_content.push_str(&thinking_chunk);
                                                        thinking_emit_buf.push_str(&thinking_chunk);
                                                    }
                                                    think_tag_buffer.clear();
                                                    break;
                                                }
                                            } else {
                                                // 在正文区域，等待 <think>
                                                if let Some(start_pos) = think_tag_buffer.find("<think>") {
                                                    let text_chunk = &think_tag_buffer[..start_pos];
                                                    if !text_chunk.is_empty() {
                                                        content.push_str(text_chunk);
                                                        text_emit_buf.push_str(text_chunk);
                                                    }
                                                    think_tag_buffer = think_tag_buffer[start_pos + 7..].to_string(); // 7 = "<think>".len()
                                                    inside_think_tag = true;
                                                } else {
                                                    // 没有更多标签，全部作为正文输出
                                                    // 但保留尾部可能的不完整标签 (如 "<thi")
                                                    let mut safe_len = if think_tag_buffer.len() > 7 {
                                                        think_tag_buffer.len() - 7
                                                    } else {
                                                        0
                                                    };
                                                    // 回退到最近的 UTF-8 字符边界，防止中文等多字节字符被从中间切开导致 panic
                                                    while safe_len > 0 && !think_tag_buffer.is_char_boundary(safe_len) {
                                                        safe_len -= 1;
                                                    }
                                                    if safe_len > 0 {
                                                        let text_chunk = &think_tag_buffer[..safe_len];
                                                        content.push_str(text_chunk);
                                                        text_emit_buf.push_str(text_chunk);
                                                        think_tag_buffer = think_tag_buffer[safe_len..].to_string();
                                                    }
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                // T-904: Tool calls 增量解析
                                if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                                    for tc in tcs {
                                        let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                        while pending_tool_calls.len() <= idx {
                                            pending_tool_calls.push(PendingToolCall {
                                                id: String::new(), name: String::new(), arguments: String::new(), thought_signature: String::new()
                                            });
                                        }
                                        if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                            pending_tool_calls[idx].id = id.to_string();
                                        }
                                        if let Some(func) = tc.get("function") {
                                            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                                pending_tool_calls[idx].name = name.to_string();
                                            }
                                            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                                pending_tool_calls[idx].arguments.push_str(args);
                                            }
                                        }
                                        // 提取 Google 的 thought_signature (Gemini 3.5+ 需要在后续请求带上)
                                        if let Some(extra) = tc.get("extra_content") {
                                            if let Some(google) = extra.get("google") {
                                                if let Some(sig) = google.get("thought_signature").and_then(|v| v.as_str()) {
                                                    pending_tool_calls[idx].thought_signature = sig.to_string();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── IPC 降压: 定时/定量刷新 ────────────────
            let elapsed = last_emit_time.elapsed().as_millis();
            if elapsed >= EMIT_INTERVAL_MS || text_emit_buf.len() >= EMIT_BUFFER_SIZE || thinking_emit_buf.len() >= EMIT_BUFFER_SIZE {
                if !thinking_emit_buf.is_empty() {
                    let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": &thinking_emit_buf, "conv_id": &conv_id_for_emit }));
                    thinking_emit_buf.clear();
                }
                if !text_emit_buf.is_empty() {
                    let _ = app.emit("llm:chunk", json!({ "type": "text", "content": &text_emit_buf, "conv_id": &conv_id_for_emit }));
                    text_emit_buf.clear();
                }
                last_emit_time = std::time::Instant::now();
            }
        }

        // ── 流结束: 刷出所有 buffer 残余 ────────────────
        if !thinking_emit_buf.is_empty() {
            let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": &thinking_emit_buf, "conv_id": &conv_id_for_emit }));
            thinking_emit_buf.clear();
        }
        if !text_emit_buf.is_empty() {
            let _ = app.emit("llm:chunk", json!({ "type": "text", "content": &text_emit_buf, "conv_id": &conv_id_for_emit }));
            text_emit_buf.clear();
        }

        // 流结束后，将 think_tag_buffer 中残留内容全部输出为正文
        if !think_tag_buffer.is_empty() {
            if inside_think_tag {
                // 残留在 <think> 块中的内容，归入思考
                thinking_content.push_str(&think_tag_buffer);
                let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": &think_tag_buffer, "conv_id": &conv_id_for_emit }));
            } else {
                content.push_str(&think_tag_buffer);
                let _ = app.emit("llm:chunk", json!({ "type": "text", "content": &think_tag_buffer, "conv_id": &conv_id_for_emit }));
            }
            think_tag_buffer.clear();
        }

        // ── 防御性检测: DSML/内部标记泄漏 ────────────────
        // 某些供应商（DeepSeek/MiniMax）偶发性地将内部 tool_calls 标记
        // 作为普通文本返回（如 <｜｜DSML｜｜tool_calls>），而非结构化 tool_calls。
        // 检测到后自动清除脏内容并重试本轮请求。
        let dsml_leaked = content.contains('\u{FF5C}') // 全角竖线 ｜
            || content.contains("DSML")
            || content.contains("<|tool_calls|>")
            || content.contains("<|invoke");

        if dsml_leaked && !has_tool_calls {
            let mut extracted = false;
            let invoke_markers = ["<｜｜DSML｜｜invoke name=\"", "<|invoke name=\""];
            
            for marker in invoke_markers {
                let mut start_idx = 0;
                while let Some(idx) = content[start_idx..].find(marker) {
                    let invoke_start = start_idx + idx;
                    let name_start = invoke_start + marker.len();
                    if let Some(name_end_offset) = content[name_start..].find("\">") {
                        let name_end = name_start + name_end_offset;
                        let tool_name = &content[name_start..name_end];
                        
                        let mut args_map = serde_json::Map::new();
                        let mut param_start_idx = name_end + 2;
                        
                        let param_markers = ["<｜｜DSML｜｜parameter name=\"", "<|parameter name=\""];
                        let param_end_markers = ["</｜｜DSML｜｜parameter>", "</|parameter>"];
                        
                        loop {
                            let mut min_pos = usize::MAX;
                            let mut best_marker_idx = 0;
                            for (i, p_marker) in param_markers.iter().enumerate() {
                                if let Some(pos) = content[param_start_idx..].find(p_marker) {
                                    if pos < min_pos {
                                        min_pos = pos;
                                        best_marker_idx = i;
                                    }
                                }
                            }
                            if min_pos == usize::MAX { break; }
                            
                            // 检查是否在找 parameter 之前先遇到了 invoke 的结束标签
                            if let Some(invoke_end) = content[param_start_idx..].find("</") {
                                if invoke_end < min_pos && content[param_start_idx+invoke_end..].contains("invoke>") {
                                    break;
                                }
                            }
                            
                            let p_start = param_start_idx + min_pos;
                            let p_name_start = p_start + param_markers[best_marker_idx].len();
                            if let Some(p_name_end_offset) = content[p_name_start..].find("\"") {
                                let p_name_end = p_name_start + p_name_end_offset;
                                let param_name = &content[p_name_start..p_name_end];
                                
                                if let Some(val_start_offset) = content[p_name_end..].find(">") {
                                    let val_start = p_name_end + val_start_offset + 1;
                                    if let Some(val_end_offset) = content[val_start..].find(param_end_markers[best_marker_idx]) {
                                        let val_end = val_start + val_end_offset;
                                        let param_value = &content[val_start..val_end];
                                        args_map.insert(param_name.to_string(), json!(param_value));
                                        param_start_idx = val_end + param_end_markers[best_marker_idx].len();
                                        continue;
                                    }
                                }
                            }
                            break;
                        }
                        
                        pending_tool_calls.push(PendingToolCall {
                            id: format!("call_{}", super::now_ms() + pending_tool_calls.len() as i64),
                            name: tool_name.to_string(),
                            arguments: serde_json::to_string(&args_map).unwrap_or_default(),
                            thought_signature: String::new(),
                        });
                        has_tool_calls = true;
                        extracted = true;
                        start_idx = param_start_idx;
                    } else {
                        break;
                    }
                }
            }

            if extracted {
                log::info!("Successfully parsed DSML internal format into standard tool calls (round {})", round + 1);
                // 如果成功解析，把文本里的 DSML 标签清掉，让用户界面看起来干净
                let clean_content = content.split("<｜").next().unwrap_or(&content).to_string();
                let clean_content = clean_content.split("<|").next().unwrap_or(&clean_content).to_string();
                content.clear();
                content.push_str(&clean_content);
                // 触发前端清屏并重发干净文本
                let _ = app.emit("llm:chunk", json!({ "type": "clear", "conv_id": &conv_id_for_emit }));
                if !content.is_empty() {
                    let _ = app.emit("llm:chunk", json!({ "type": "text", "content": &content, "conv_id": &conv_id_for_emit }));
                }
            } else if round < MAX_TOOL_ROUNDS {
                // 解析失败，退回到重试逻辑
                log::warn!("DSML token leakage detected but parsing failed (round {}), retrying...", round + 1);
                let _ = app.emit("llm:chunk", json!({ "type": "clear", "conv_id": &conv_id_for_emit }));
                let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": "\n[检测到模型输出异常，自动重试中...]\n", "conv_id": &conv_id_for_emit }));
                full_messages.push(json!({
                    "role": "system",
                    "content": "请使用标准的 function calling JSON 格式调用工具。不要在回复文本中直接输出工具调用标记。"
                }));
                continue;
            }
        }

        // ── 判断: Tool Call 还是最终回复 ────────────────
        if !pending_tool_calls.is_empty() {
            has_tool_calls = true;
        }
        if has_tool_calls && !pending_tool_calls.is_empty() {
            // 构建 assistant 消息 (含 tool_calls)
            let tc_json: Vec<Value> = pending_tool_calls.iter().map(|tc| {
                let mut call_obj = json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments }
                });
                if !tc.thought_signature.is_empty() {
                    call_obj["extra_content"] = json!({
                        "google": { "thought_signature": tc.thought_signature }
                    });
                }
                call_obj
            }).collect();

            let mut assistant_msg = json!({ "role": "assistant", "tool_calls": tc_json });
            if !content.is_empty() { assistant_msg["content"] = json!(content); }
            // DeepSeek thinking 模式要求回传 reasoning_content，否则下一轮请求会 400
            if !thinking_content.is_empty() {
                assistant_msg["reasoning_content"] = json!(thinking_content);
            }
            full_messages.push(assistant_msg);

            // ── T-1401: 熔断器预检 + 并行执行工具 ──────────────
            let from_user_for_tools = from_user.clone();

            // 先对每个工具做熔断检查，分为可执行和被熔断两组
            let mut executable: Vec<(usize, Value)> = Vec::new(); // (index, parsed_args)
            let mut circuit_broken: Vec<(usize, String)> = Vec::new(); // (index, error_msg)

            for (i, tc) in pending_tool_calls.iter().enumerate() {
                let args: Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
                match tool_tracker.check(&tc.name, &args) {
                    Ok(()) => executable.push((i, args)),
                    Err(reason) => {
                        log::warn!("Circuit breaker tripped for tool '{}': {}", tc.name, reason);
                        circuit_broken.push((i, reason));
                    }
                }
            }

            // 构建可执行工具的 futures（含缓存命中检测）
            let mut cached_results: Vec<(usize, String, Value, Value)> = Vec::new();
            let mut uncached: Vec<(usize, Value)> = Vec::new();

            for (i, args) in &executable {
                let name = &pending_tool_calls[*i].name;
                if cacheable_tools.contains(name.as_str()) {
                    let cache_key = format!("{}:{}", name, serde_json::to_string(args).unwrap_or_default());
                    if let Some(cached) = tool_cache.get(&cache_key) {
                        log::info!("Tool cache HIT: {} ({})", name, cache_key.chars().take(60).collect::<String>());
                        cached_results.push((*i, name.clone(), args.clone(), cached.clone()));
                        continue;
                    }
                }
                uncached.push((*i, args.clone()));
            }

            let tool_futures: Vec<_> = uncached.iter().map(|(i, args)| {
                let app_clone = app.clone();
                let name = pending_tool_calls[*i].name.clone();
                let args = args.clone();
                let fu = from_user_for_tools.clone();
                let idx = *i;
                async move {
                    let result = super::tools::execute_tool(&app_clone, &name, &args, fu.as_deref(), global_file_access).await;
                    (idx, name, args, result)
                }
            }).collect();

            // 发射所有 tool_start 事件（包括被熔断的，前端需要显示状态）
            for tc in &pending_tool_calls {
                let _ = app.emit("llm:chunk", json!({ "type": "tool_start", "name": tc.name, "conv_id": &conv_id_for_emit }));
            }

            // 并行等待可执行的工具完成
            let exec_results = futures_util::future::join_all(tool_futures).await;

            // 合并执行结果、缓存命中结果和熔断结果，按原始索引排序
            let mut all_results: Vec<(usize, String, Value)> = Vec::new();

            for (idx, name, args, result) in exec_results {
                // 写入缓存（仅可缓存工具 + 无错误结果）
                if cacheable_tools.contains(name.as_str()) && result.get("error").is_none() {
                    let cache_key = format!("{}:{}", name, serde_json::to_string(&args).unwrap_or_default());
                    tool_cache.insert(cache_key, result.clone());
                }
                // 写操作清空缓存（文件可能已变更）
                if cache_invalidating_tools.contains(name.as_str()) {
                    tool_cache.clear();
                }
                // 记录到熔断追踪器
                tool_tracker.record(&name, &args);
                all_results.push((idx, name, result));
            }

            // 缓存命中的结果直接加入
            for (idx, name, args, result) in cached_results {
                tool_tracker.record(&name, &args);
                all_results.push((idx, name, result));
            }

            for (idx, reason) in circuit_broken {
                let name = pending_tool_calls[idx].name.clone();
                all_results.push((idx, name, json!({ "error": reason })));
            }

            // 按原始索引排序，确保 tool_call_id 对应正确
            all_results.sort_by_key(|(idx, _, _)| *idx);

            // 按顺序推送结果到 messages
            for (i, (_, name, result)) in all_results.iter().enumerate() {
                // 进化引擎: 检测工具失败
                if result.get("error").is_some() {
                    tool_failures_total += 1;
                }

                let result_str = serde_json::to_string_pretty(&result).unwrap_or_default();

                // 截断过长结果（防 context window 爆炸），使用 char 边界安全截断
                let truncated = if result_str.len() > 8000 {
                    let mut end = 8000;
                    while end > 0 && !result_str.is_char_boundary(end) { end -= 1; }
                    format!("{}...\n[结果被截断，共 {} 字符]", &result_str[..end], result_str.len())
                } else {
                    result_str
                };

                let _ = app.emit("llm:chunk", json!({ "type": "tool_end", "name": name, "result": truncated, "conv_id": &conv_id_for_emit }));

                full_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": pending_tool_calls[i].id,
                    "content": truncated
                }));
            }

            // ── 进化引擎: 累计工具调用统计 ──────────────
            tool_calls_total = tool_tracker.total() as i64;
            rounds_completed = round + 1;

            log::info!("Tool Calling round {}: executed {} tools (total: {}/{})",
                round + 1, pending_tool_calls.len(), tool_tracker.total(), 15);

            // ── Restatement 注意力重申 (round >= 1) ──────────────────
            // 利用大模型 U 型注意力曲线：对上下文首尾关注度最高、中间最弱。
            // 在第 2 轮起，于 messages 尾部注入 system 消息重申任务约束，
            // 防止模型在消化大量工具结果后"失焦"忘记原始需求。
            if round >= 1 {
                // 提取用户最后一条消息作为任务锚点
                let user_request_hint = full_messages.iter().rev()
                    .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                    .and_then(|m| m.get("content").and_then(|c| c.as_str()))
                    .map(|s| {
                        let chars: String = s.chars().take(200).collect();
                        chars
                    })
                    .unwrap_or_default();

                let restatement = format!(
                    "[注意力重申] 你已经执行了 {} 轮工具调用。请基于以上所有工具返回的结果，\
                    综合回答用户的原始请求。用户的请求是：「{}」\n\
                    请严格遵守你的人设和回复风格规范。不要遗漏关键信息，也不要重复调用已经成功获取结果的工具。",
                    round + 1,
                    user_request_hint
                );

                full_messages.push(json!({
                    "role": "system",
                    "content": restatement
                }));

                log::info!("Restatement injected at round {}", round + 1);
            }

            continue; // 回到循环顶部，将工具结果发回 LLM
        }

        // 没有 tool calls — 这是最终文本回复
        rounds_completed = round;
        final_content = content;
        final_thinking = thinking_content;
        final_usage = usage_data;
        break;
    }

    // 发送完成事件
    let _ = app.emit("llm:chunk", json!({ "type": "done", "content": "", "conv_id": &conv_id_for_emit }));

    // ── 进化引擎: 零成本遥测 ────────────────────────────────
    // 纯 Rust 计数器，不调用 LLM，不阻塞响应
    {
        let stop = if final_content.is_empty() && final_thinking.is_empty() {
            "empty_response".to_string()
        } else {
            "completed".to_string()
        };

        // 从 usage 中提取 token 计数
        let (toks_in, toks_out) = final_usage.as_ref()
            .map(|u| {
                let i = u.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                let o = u.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                (i, o)
            })
            .unwrap_or((0, 0));

        let obs = super::evolution::ObservationRecord {
            conversation_id: conv_id_for_emit.clone(),
            model_used: model_id.clone(),
            tool_calls_count: tool_calls_total,
            tool_failures: tool_failures_total,
            total_rounds: rounds_completed as i64,
            duration_ms: start_time.elapsed().as_millis() as i64,
            tokens_in: toks_in,
            tokens_out: toks_out,
            stop_reason: stop,
        };
        super::evolution::capture_observation(&obs);
    }

    // ── 进化引擎: 异步知识提取 ──────────────────────────────
    // tokio::spawn 后台执行，完全不阻塞主线程返回
    {
        let extract_app = app.clone();
        let mut extract_messages = full_messages.clone();
        // 追加当前最终回复，让进化引擎能看见 <|mem|> 标记和完整答复
        extract_messages.push(json!({
            "role": "assistant",
            "content": final_content.clone()
        }));
        let extract_conv_id = conv_id_for_emit.clone();
        let extract_rounds = rounds_completed as i64;
        tokio::spawn(async move {
            super::evolution::extract_learned_facts(
                extract_app, extract_messages, extract_conv_id, extract_rounds
            ).await;
        });
    }

    // ── T-1412-b: 即时纠错检测 (后台, 不阻塞) ────────────────
    // 检测用户是否在对话中纠正了 Bob 的错误认知
    {
        let correction_msgs = full_messages.clone();
        let correction_conv_id = conv_id_for_emit.clone();
        tokio::spawn(async move {
            detect_and_apply_corrections(&correction_msgs, &correction_conv_id);
        });
    }

    let mut pricing = json!({ "input": 0.0, "output": 0.0 });
    let pool = get_model_pool();
    if let Some(arr) = pool.as_array() {
        if let Some(m) = arr.iter().find(|x| x.get("modelId").and_then(|v| v.as_str()) == Some(model_id.as_str())) {
            if let Some(p) = m.get("pricing") {
                pricing = p.clone();
            }
        }
    }

    json!({
        "content": final_content,
        "thinking": if final_thinking.is_empty() { Value::Null } else { Value::String(final_thinking) },
        "usage": final_usage,
        "pricing": pricing,
        "model": model_id,
    })
}

pub async fn stream_chat(app: AppHandle, messages: Vec<Value>, conv_id: Option<String>, from_user: Option<String>, global_file_access: bool, agent_mode: String) -> Value {
    stream_internal(app, messages, conv_id, from_user, global_file_access, agent_mode).await
}



pub async fn stream_vision(app: AppHandle, mut messages: Vec<Value>, image_base64s: Vec<String>, conv_id: Option<String>, global_file_access: bool, agent_mode: String) -> Value {
    if let Some(last) = messages.last_mut() {
        if let Some(obj) = last.as_object_mut() {
            let text_content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("请分析图片").to_string();
            
            let mut content_array = vec![json!({ "type": "text", "text": text_content })];
            
            for b64 in image_base64s {
                content_array.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", b64),
                        "detail": "auto"
                    }
                }));
            }
            
            obj.insert("content".to_string(), Value::Array(content_array));
        }
    }
    stream_internal(app, messages, conv_id, None, global_file_access, agent_mode).await
}

// ═══════════════════════════════════════════════════════════
// T-1305: 聊天就绪校验 (fail-open, 纯本地检测)
// ═══════════════════════════════════════════════════════════

/// 校验聊天所需的配置是否完整 (provider + model + apiKey)
/// 不做网络探测，仅检查本地配置文件
#[tauri::command]
pub async fn system_auto_rename_conversation(
    conversation_id: String,
    db: tauri::State<'_, crate::db::DbState>
) -> Result<String, String> {
    let config = super::read_config();
    let clerk_model = config.get("clerkModel").and_then(|v| v.as_str()).unwrap_or("");
    if clerk_model.is_empty() {
        return Err("No clerkModel configured".into());
    }

    let mut messages_json = Vec::new();
    if let Ok(conn) = db.0.lock() {
        let mut stmt = conn.prepare("SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC LIMIT 4").map_err(|e| e.to_string())?;
        let rows = stmt.query_map(rusqlite::params![conversation_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| e.to_string())?;
        for row in rows {
            if let Ok((role, content)) = row {
                if !content.starts_with("__rename__") {
                    messages_json.push(serde_json::json!({
                        "role": role,
                        "content": content
                    }));
                }
            }
        }
    }

    if messages_json.is_empty() {
        return Err("No messages found".into());
    }

    messages_json.push(serde_json::json!({
        "role": "user",
        "content": "根据以上的对话内容，提取一个极简的标题（2-6个字，不要标点符号，不要书名号）。只输出标题内容即可。"
    }));

    let (provider, base_url, api_key, actual_model) = read_llm_config_for_model(clerk_model);
    if base_url.is_empty() || api_key.is_empty() {
        return Err("clerkModel config incomplete".into());
    }

    let url = if provider == "anthropic" {
        format!("{}/messages", base_url)
    } else {
        format!("{}/chat/completions", base_url)
    };

    let body = serde_json::json!({
        "model": actual_model,
        "messages": messages_json,
        "max_tokens": 20,
        "temperature": 0.1
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let final_api_key = if provider == "google" {
        let cred_path = crate::gcp_auth::get_gcp_credential_path();
        match crate::gcp_auth::GcpTokenManager::from_file(&cred_path) {
            Ok(manager) => match manager.get_access_token().await {
                Ok(token) => token,
                Err(e) => return Err(format!("GCP token fetch failed: {}", e)),
            },
            Err(e) => return Err(format!("GCP cred load failed: {}", e)),
        }
    } else {
        api_key
    };

    let req = client.post(&url).header("Content-Type", "application/json");
    let req = if provider == "anthropic" {
        req.header("x-api-key", final_api_key)
           .header("anthropic-version", "2023-06-01")
    } else {
        req.header("Authorization", format!("Bearer {}", final_api_key))
    };

    let resp = req.json(&body).send().await.map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("API error: {}", resp.status()));
    }

    let resp_json: Value = resp.json().await.map_err(|e| e.to_string())?;
    
    let mut title = if provider == "anthropic" {
        resp_json.get("content")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        resp_json.get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string()
    };

    title = title.replace("\"", "").replace("'", "").replace("《", "").replace("》", "").replace("\n", "").trim().to_string();
    if title.is_empty() {
        return Err("Generated title is empty".into());
    }

    if let Ok(conn) = db.0.lock() {
        let _ = conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![title, crate::now_ms(), conversation_id]
        );
    }

    Ok(title)
}

#[tauri::command]
pub fn system_validate_chat_ready() -> Value {
    let config = super::read_config();

    // 1. 检查是否有任何 API Key
    let api_keys = get_api_keys();
    let has_any_key = api_keys.as_object()
        .map(|m| m.values().any(|v| {
            v.as_str().map_or(false, |s| !s.is_empty())
        }))
        .unwrap_or(false);

    if !has_any_key {
        return json!({
            "ready": false,
            "reason": "no_api_key",
            "message": "未配置 API Key，请前往设置添加"
        });
    }

    // 2. 检查模型配置
    let model_id = config.get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if model_id.is_empty() {
        // 没有显式选择模型，但如果有 API Key 则可能使用默认模型
        // 这是 warning 级别，不阻断
        return json!({
            "ready": true,
            "reason": "no_model_selected",
            "message": "未选择主模型，将使用默认模型"
        });
    }

    // 3. 验证选择的模型有对应的 API Key
    let (provider, api_key, _, base_url) = read_llm_config_for_model(&model_id);
    if api_key.is_empty() && provider != "offline" {
        return json!({
            "ready": false,
            "reason": "model_no_key",
            "message": format!("模型 {} 对应的供应商 {} 未配置 API Key", model_id, provider)
        });
    }

    if base_url.is_empty() {
        return json!({
            "ready": false,
            "reason": "no_base_url",
            "message": format!("供应商 {} 缺少 API 地址配置", provider)
        });
    }

    json!({
        "ready": true,
        "reason": "ok",
        "message": ""
    })
}
