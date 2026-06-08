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
                    let model_name = model.get("name").and_then(|v| v.as_str()).unwrap_or(model_id);
                    let vision = model.get("vision").and_then(|v| v.as_bool()).unwrap_or(false);
                    let is_default = model.get("default").and_then(|v| v.as_bool()).unwrap_or(false);
                    let pricing = model.get("pricing").cloned().unwrap_or(json!({ "input": 0.0, "output": 0.0 }));
                    
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
// Tool Calling 引擎 (T-903/T-904)
// ═══════════════════════════════════════════════════════════

/// 内部通用流式处理 — 支持 Tool Calling 循环
async fn stream_internal(
    app: AppHandle,
    messages: Vec<Value>,
    conv_id: Option<String>,
    from_user: Option<String>,
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
    let base_url = custom_base_url;

    if base_url.is_empty() {
        return json!({ "error": format!("未知的供应商: {}，请检查模型配置", provider) });
    }

    let model_id = if model_override.is_empty() {
        get_default_model(&provider)
    } else {
        model_override.clone()
    };

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
- **list_skills**: 查看可用的专业分析框架\n\
- **read_skill**: 加载某个技能的详细指南（加载后请严格遵循其工作流程）\n\
- **read_model_registry**: 读取当前 AI 模型注册表（查看/对比供应商模型列表）\n\
- **test_model_endpoint**: 测试某个模型 ID 的 API 连通性（验证模型是否可用）\n\
- **update_model_registry**: 更新指定供应商的模型列表（必须先 test 验证）\n\
\n\
当用户提到文件路径时，请主动调用 read_file 读取；当用户要求分析/规划时，先 read_skill 加载对应框架。\n\
当用户提出事实性问题时，请先调用 brain_search 检索知识库。\n\
{}\n\
{}\n\
{}\n\
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
            os_info, current_dir, wxid_info, skills_summary, memory_summary, wiki_status
        );

        full_messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }
    full_messages.extend(messages);

    // 3. 获取工具 Schema
    let tool_schemas = super::tools::get_tool_schemas();

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

        // DeepSeek 专有参数
        if provider == "deepseek" {
            body["thinking"] = json!({ "type": "enabled" });
            body["reasoning_effort"] = json!("low");
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
        struct PendingToolCall { id: String, name: String, arguments: String }
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
                                                id: String::new(), name: String::new(), arguments: String::new(),
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
        if has_tool_calls && !pending_tool_calls.is_empty() {
            // 构建 assistant 消息 (含 tool_calls)
            let tc_json: Vec<Value> = pending_tool_calls.iter().map(|tc| json!({
                "id": tc.id, "type": "function",
                "function": { "name": tc.name, "arguments": tc.arguments }
            })).collect();

            let mut assistant_msg = json!({ "role": "assistant", "tool_calls": tc_json });
            if !content.is_empty() { assistant_msg["content"] = json!(content); }
            // DeepSeek thinking 模式要求回传 reasoning_content，否则下一轮请求会 400
            if !thinking_content.is_empty() {
                assistant_msg["reasoning_content"] = json!(thinking_content);
            }
            full_messages.push(assistant_msg);

            // ── 并行执行工具 (tokio::join_all) ──────────────
            let from_user_for_tools = from_user.clone();
            let tool_futures: Vec<_> = pending_tool_calls.iter().map(|tc| {
                let app_clone = app.clone();
                let name = tc.name.clone();
                let args_str = tc.arguments.clone();
                let fu = from_user_for_tools.clone();
                async move {
                    let args: Value = serde_json::from_str(&args_str).unwrap_or(json!({}));
                    let result = super::tools::execute_tool(&app_clone, &name, &args, fu.as_deref()).await;
                    (name, result)
                }
            }).collect();

            // 发射所有 tool_start 事件
            for tc in &pending_tool_calls {
                let _ = app.emit("llm:chunk", json!({ "type": "tool_start", "name": tc.name, "conv_id": &conv_id_for_emit }));
            }

            // 并行等待所有工具完成
            let results = futures_util::future::join_all(tool_futures).await;

            // 按顺序推送结果到 messages
            for (i, (name, result)) in results.into_iter().enumerate() {
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
            tool_calls_total += pending_tool_calls.len() as i64;
            rounds_completed = round + 1;

            log::info!("Tool Calling round {}: executed {} tools", round + 1, pending_tool_calls.len());

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

pub async fn stream_chat(app: AppHandle, messages: Vec<Value>, conv_id: Option<String>, from_user: Option<String>) -> Value {
    stream_internal(app, messages, conv_id, from_user).await
}



pub async fn stream_vision(app: AppHandle, mut messages: Vec<Value>, image_base64: String, conv_id: Option<String>) -> Value {
    if let Some(last) = messages.last_mut() {
        if let Some(obj) = last.as_object_mut() {
            let text_content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("请分析这张图片");
            
            obj.insert("content".to_string(), json!([
                { "type": "text", "text": text_content },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", image_base64),
                        "detail": "auto"
                    }
                }
            ]));
        }
    }
    stream_internal(app, messages, conv_id, None).await
}

// ═══════════════════════════════════════════════════════════
// T-1305: 聊天就绪校验 (fail-open, 纯本地检测)
// ═══════════════════════════════════════════════════════════

/// 校验聊天所需的配置是否完整 (provider + model + apiKey)
/// 不做网络探测，仅检查本地配置文件
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
