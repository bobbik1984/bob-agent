//! Native F1 Direct Model Provider (BYOK) adapter.
//!
//! - Stores and reads API keys exclusively through the OS vault (`device_vault`
//!   DPAPI on Windows, `android_vault` AndroidKeyStore AES-256-GCM on Android).
//! - Plaintext API keys never enter SQLite, logs, or renderer responses.
//! - Supports `openai_compatible`, `anthropic_messages`, and `gemini_generate_content`.
use crate::{now, with_app, AppState, RunnerState};
use bob_kernel::{
    direct_provider::{
        DirectChatTurn, DirectExecutionOutcome, DirectExecutionPrepared, DirectProviderProfile,
        DirectProviderProfileInput,
    },
    local_execution::LocalExecution,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, Runtime, State};

const DEFAULT_SLOT: &str = "provider-key-default";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const EMBEDDED_MODEL_REGISTRY: &str = include_str!("../resources/model_providers.json");
const TOOL_KEY_IDS: [&str; 2] = ["TAVILY_API_KEY", "TINYFISH_API_KEY"];

pub(crate) fn provider_slot(provider_id: &str) -> String {
    format!("provider-key-{}", provider_id.trim())
}

pub(crate) fn remote_node_token_slot(node_id: &str) -> String {
    format!("remote-node-token-{}", node_id.trim())
}

fn merge_registry_with_embedded_defaults(mut disk_val: Value, embedded_val: &Value) -> Value {
    let Some(emb_providers) = embedded_val.get("providers").and_then(Value::as_array) else {
        return disk_val;
    };
    let Some(disk_providers) = disk_val.get_mut("providers").and_then(Value::as_array_mut) else {
        return embedded_val.clone();
    };
    for emb_p in emb_providers {
        let Some(pid) = emb_p.get("id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(disk_p) = disk_providers
            .iter_mut()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
        {
            if let (Some(emb_models), Some(disk_models)) = (
                emb_p.get("models").and_then(Value::as_array),
                disk_p.get_mut("models").and_then(Value::as_array_mut),
            ) {
                let mut merged_models = Vec::new();
                for em in emb_models {
                    if let Some(em_id) = em.get("id").and_then(Value::as_str) {
                        if let Some(existing) = disk_models
                            .iter()
                            .find(|dm| dm.get("id").and_then(Value::as_str) == Some(em_id))
                        {
                            let mut updated = existing.clone();
                            if let Some(name) = em.get("name") {
                                updated["name"] = name.clone();
                            }
                            if em.get("default").and_then(Value::as_bool) == Some(true) {
                                updated["default"] = json!(true);
                                updated["visible"] = json!(true);
                            } else if let Some(obj) = updated.as_object_mut() {
                                obj.remove("default");
                            }
                            if em.get("visible").and_then(Value::as_bool) == Some(true) {
                                updated["visible"] = json!(true);
                            }
                            merged_models.push(updated);
                        } else {
                            merged_models.push(em.clone());
                        }
                    }
                }
                for dm in disk_models.iter() {
                    if let Some(dm_id) = dm.get("id").and_then(Value::as_str) {
                        if !merged_models
                            .iter()
                            .any(|m| m.get("id").and_then(Value::as_str) == Some(dm_id))
                        {
                            let mut extra = dm.clone();
                            if let Some(obj) = extra.as_object_mut() {
                                obj.remove("default");
                            }
                            merged_models.push(extra);
                        }
                    }
                }
                disk_p["models"] = json!(merged_models);
            }
        } else {
            disk_providers.push(emb_p.clone());
        }
    }
    disk_val["$schema_version"] = embedded_val
        .get("$schema_version")
        .cloned()
        .unwrap_or_else(|| json!(4));
    disk_val["last_updated"] = embedded_val
        .get("last_updated")
        .cloned()
        .unwrap_or_else(|| json!("2026-10-30"));
    disk_val
}

fn load_or_init_registry(data_dir: &std::path::Path) -> Value {
    let path = data_dir.join("model_providers.json");
    let default_val: Value = serde_json::from_str(EMBEDDED_MODEL_REGISTRY)
        .unwrap_or_else(|_| json!({ "$schema_version": 4, "providers": [] }));
    let embedded_ver = default_val
        .get("$schema_version")
        .and_then(Value::as_i64)
        .unwrap_or(4);
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if val.get("providers").and_then(Value::as_array).is_some() {
                let disk_ver = val
                    .get("$schema_version")
                    .and_then(Value::as_i64)
                    .unwrap_or(1);
                if disk_ver < embedded_ver {
                    let merged = merge_registry_with_embedded_defaults(val, &default_val);
                    let _ = save_registry_file(data_dir, &merged);
                    return merged;
                }
                return val;
            }
        }
    }
    let _ = std::fs::write(&path, EMBEDDED_MODEL_REGISTRY);
    default_val
}

fn save_registry_file(data_dir: &std::path::Path, registry: &Value) -> Result<(), &'static str> {
    let path = data_dir.join("model_providers.json");
    let raw = serde_json::to_string_pretty(registry).map_err(|_| "REGISTRY_SAVE_FAILED")?;
    std::fs::write(path, raw).map_err(|_| "REGISTRY_SAVE_FAILED")
}

fn default_hub_state() -> Value {
    json!({
        "activeRoles": {
            "main": "deepseek::deepseek-v4-flash",
            "clerk": "doubao::doubao-seed-1-6-flash-250828",
            "vision": "gemini::gemini-2.5-flash"
        },
        "providerVariants": {},
        "customModels": [],
        "remoteNodes": [],
        "credentialTimestamps": {},
        "updatedAt": 0
    })
}

fn load_hub_state(data_dir: &std::path::Path) -> Value {
    let path = data_dir.join("model_hub_state.json");
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if val.is_object() {
                return val;
            }
        }
    }
    let def = default_hub_state();
    let _ = save_hub_state(data_dir, &def);
    def
}

fn save_hub_state(data_dir: &std::path::Path, hub: &Value) -> Result<(), &'static str> {
    let path = data_dir.join("model_hub_state.json");
    let raw = serde_json::to_string_pretty(hub).map_err(|_| "HUB_STATE_SAVE_FAILED")?;
    std::fs::write(path, raw).map_err(|_| "HUB_STATE_SAVE_FAILED")
}

fn infer_provider_protocol(provider_id: &str, base_url: &str) -> &'static str {
    let pid = provider_id.trim().to_ascii_lowercase();
    let url = base_url.trim().to_ascii_lowercase();
    if pid == "anthropic" || url.contains("api.anthropic.com") {
        "anthropic_messages"
    } else if (pid == "gemini" || url.contains("generativelanguage.googleapis.com"))
        && !url.contains("/openai")
    {
        "gemini_generate_content"
    } else {
        "openai_compatible"
    }
}

fn resolve_provider_base_url(provider: &Value, variants: &Value) -> String {
    let pid = provider.get("id").and_then(Value::as_str).unwrap_or("");
    let default_url = provider
        .get("base_url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let selected_variant = variants
        .get(pid)
        .and_then(Value::as_str)
        .unwrap_or("default");
    if selected_variant != "default" {
        if let Some(variant_url) = provider
            .get("base_url_variants")
            .and_then(|v| v.get(selected_variant))
            .and_then(Value::as_str)
        {
            if !variant_url.trim().is_empty() {
                return variant_url.to_owned();
            }
        }
    }
    default_url
}

async fn build_model_hub_snapshot_and_sync<R: Runtime>(
    host: &AppHandle<R>,
    state: &State<'_, AppState>,
) -> Result<Value, &'static str> {
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;

    let registry = load_or_init_registry(&data_dir);
    let mut hub_state = load_hub_state(&data_dir);
    let providers = registry
        .get("providers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut provider_keys = serde_json::Map::new();
    for prov in &providers {
        let pid = prov.get("id").and_then(Value::as_str).unwrap_or("");
        if pid.is_empty() || pid == "offline" {
            continue;
        }
        let slot = provider_slot(pid);
        let configured = read_api_key(host, &slot)
            .await
            .is_ok_and(|k| !k.trim().is_empty());
        provider_keys.insert(pid.to_owned(), json!(configured));
    }

    let mut tool_keys = serde_json::Map::new();
    for tid in TOOL_KEY_IDS {
        let slot = provider_slot(tid);
        let configured = read_api_key(host, &slot)
            .await
            .is_ok_and(|k| !k.trim().is_empty());
        tool_keys.insert(tid.to_owned(), json!(configured));
    }

    let mut pool = Vec::new();
    for prov in &providers {
        let pid = prov.get("id").and_then(Value::as_str).unwrap_or("");
        if pid.is_empty() || pid == "offline" {
            continue;
        }
        let pname = prov.get("name").and_then(Value::as_str).unwrap_or(pid);
        let models = prov
            .get("models")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for m in models {
            if m.get("visible").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let mid = m.get("id").and_then(Value::as_str).unwrap_or("");
            if mid.is_empty() {
                continue;
            }
            let mname = m.get("name").and_then(Value::as_str).unwrap_or(mid);
            let vision = m.get("vision").and_then(Value::as_bool).unwrap_or(false);
            let discovered = m.get("discovered").and_then(Value::as_bool).unwrap_or(false);
            let is_default = m.get("default").and_then(Value::as_bool).unwrap_or(false);
            pool.push(json!({
                "id": format!("{pid}::{mid}"),
                "provider": pid,
                "providerName": pname,
                "modelId": mid,
                "displayName": mname,
                "vision": vision,
                "discovered": discovered,
                "default": is_default,
            }));
        }
    }

    let custom_models = hub_state
        .get("customModels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for cm in &custom_models {
        let cid = cm.get("id").and_then(Value::as_str).unwrap_or("");
        let mid = cm.get("modelId").and_then(Value::as_str).unwrap_or(cid);
        let dname = cm.get("displayName").and_then(Value::as_str).unwrap_or(mid);
        let base_url = cm.get("baseUrl").and_then(Value::as_str).unwrap_or("");
        if !cid.is_empty() {
            let slot = provider_slot(&format!("custom-{cid}"));
            let has_k = read_api_key(host, &slot)
                .await
                .is_ok_and(|k| !k.trim().is_empty());
            provider_keys.insert(format!("custom-{cid}"), json!(has_k));
            pool.push(json!({
                "id": format!("custom-{cid}::{mid}"),
                "provider": format!("custom-{cid}"),
                "providerName": dname,
                "modelId": mid,
                "displayName": dname,
                "baseUrl": base_url,
                "vision": false,
                "discovered": false,
                "default": true,
            }));
        }
    }

    // If the currently assigned main model's provider has no API key configured,
    // but another provider DOES have an API key configured, auto-select that provider's default model.
    let main_entry_id = hub_state["activeRoles"]["main"]
        .as_str()
        .unwrap_or("deepseek::deepseek-v4-flash")
        .to_owned();
    let main_prov = main_entry_id.split("::").next().unwrap_or("deepseek");
    let main_has_key = provider_keys
        .get(main_prov)
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let effective_main_id = if main_has_key {
        main_entry_id
    } else if let Some((configured_pid, _)) = provider_keys
        .iter()
        .find(|(_, v)| v.as_bool() == Some(true))
    {
        let fallback_entry = pool
            .iter()
            .find(|m| {
                m["provider"].as_str() == Some(configured_pid.as_str())
                    && m["default"].as_bool() == Some(true)
            })
            .or_else(|| {
                pool.iter()
                    .find(|m| m["provider"].as_str() == Some(configured_pid.as_str()))
            })
            .and_then(|m| m["id"].as_str())
            .map(str::to_owned);
        if let Some(ref fb) = fallback_entry {
            hub_state["activeRoles"]["main"] = json!(fb);
            let _ = save_hub_state(&data_dir, &hub_state);
            fb.clone()
        } else {
            main_entry_id
        }
    } else {
        main_entry_id
    };

    // Sync effective main model into bob-kernel's default DirectProviderProfile
    // ONLY when at least one provider key is configured in ModelHub (so manual test profiles aren't clobbered).
    let any_hub_key_configured = provider_keys.values().any(|v| v.as_bool() == Some(true));
    if any_hub_key_configured {
        let (eff_prov, eff_model) = effective_main_id
            .split_once("::")
            .unwrap_or(("deepseek", "deepseek-v4-flash"));
        let variants = hub_state
            .get("providerVariants")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let (label, base_url, slot) = if let Some(cid) = eff_prov.strip_prefix("custom-") {
            let cm = custom_models
                .iter()
                .find(|c| c["id"].as_str() == Some(cid));
            let dname = cm
                .and_then(|c| c["displayName"].as_str())
                .unwrap_or("Custom")
                .to_owned();
            let burl = cm
                .and_then(|c| c["baseUrl"].as_str())
                .unwrap_or("https://api.openai.com/v1")
                .to_owned();
            (dname, burl, provider_slot(eff_prov))
        } else {
            let prov_val = providers
                .iter()
                .find(|p| p["id"].as_str() == Some(eff_prov));
            let pname = prov_val
                .and_then(|p| p["name"].as_str())
                .unwrap_or(eff_prov)
                .to_owned();
            let burl = prov_val
                .map(|p| resolve_provider_base_url(p, &variants))
                .unwrap_or_else(|| "https://api.deepseek.com".to_owned());
            (pname, burl, provider_slot(eff_prov))
        };
        let protocol = infer_provider_protocol(eff_prov, &base_url).to_owned();
        let key_configured = provider_keys
            .get(eff_prov)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let input = DirectProviderProfileInput {
            id: "default".to_owned(),
            provider_kind: protocol,
            label,
            base_url,
            model_id: eff_model.to_owned(),
            api_key_slot: slot,
            api_key_configured: key_configured,
            enabled: true,
        };
        let _ = with_app(&state.0, |app| {
            app.upsert_direct_provider_profile(&input, now()?)
                .map_err(|_| "PROVIDER_SAVE_FAILED")
        });
    }

    let mut remote_nodes = Vec::new();
    if let Some(raw_nodes) = hub_state.get("remoteNodes").and_then(Value::as_array) {
        for node in raw_nodes {
            let nid = node.get("id").and_then(Value::as_str).unwrap_or("");
            if nid.is_empty() {
                continue;
            }
            let slot = remote_node_token_slot(nid);
            let token_val = read_api_key(host, &slot).await.unwrap_or_default();
            let mut item = node.clone();
            if let Some(obj) = item.as_object_mut() {
                obj.remove("token");
                obj.insert(
                    "hasToken".to_owned(),
                    json!(!token_val.trim().is_empty()),
                );
            }
            remote_nodes.push(item);
        }
    }

    Ok(json!({
        "registry": registry,
        "pool": pool,
        "totalAvailableCount": pool.len(),
        "activeRoles": hub_state["activeRoles"],
        "providerKeys": provider_keys,
        "toolKeys": tool_keys,
        "providerVariants": hub_state["providerVariants"],
        "customModels": custom_models,
        "remoteNodes": remote_nodes,
    }))
}

pub(crate) async fn store_api_key<R: Runtime>(
    app: &AppHandle<R>,
    slot: &str,
    api_key: &str,
) -> Result<(), &'static str> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() || trimmed.len() > 8192 {
        return Err("INVALID_API_KEY");
    }
    #[cfg(target_os = "android")]
    {
        crate::android_vault::store_secret(app, slot, trimmed.as_bytes().to_vec()).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        std::fs::create_dir_all(&dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        crate::device_vault::store_secret(&dir, slot, trimmed.as_bytes())
            .map_err(|_| "VAULT_STORE_FAILED")
    }
}

pub(crate) async fn read_api_key<R: Runtime>(
    app: &AppHandle<R>,
    slot: &str,
) -> Result<String, &'static str> {
    #[cfg(target_os = "android")]
    {
        let bytes = crate::android_vault::read_secret(app, slot)
            .await?
            .ok_or("API_KEY_NOT_CONFIGURED")?;
        String::from_utf8(bytes).map_err(|_| "VAULT_INVALID_DATA")
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        let bytes = crate::device_vault::read_secret(&dir, slot)
            .map_err(|_| "API_KEY_NOT_CONFIGURED")?;
        String::from_utf8(bytes).map_err(|_| "VAULT_INVALID_DATA")
    }
}

pub(crate) async fn clear_api_key<R: Runtime>(
    app: &AppHandle<R>,
    slot: &str,
) -> Result<(), &'static str> {
    #[cfg(target_os = "android")]
    {
        crate::android_vault::clear_secret(app, slot).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        crate::device_vault::clear_secret(&dir, slot).map_err(|_| "VAULT_CLEAR_FAILED")
    }
}

#[derive(Debug)]
pub(crate) struct ProviderRequestSpec {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug)]
pub(crate) struct ProviderReplyParsed {
    pub text: String,
    pub reported_model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

pub(crate) fn build_provider_request(
    provider_kind: &str,
    base_url: &str,
    model_id: &str,
    api_key: &str,
    turns: &[DirectChatTurn],
    max_tokens: u32,
) -> Result<ProviderRequestSpec, &'static str> {
    let base = base_url.trim().trim_end_matches('/');
    let model = model_id.trim();
    let key = api_key.trim();
    if base.is_empty() || model.is_empty() || key.is_empty() || turns.is_empty() {
        return Err("INVALID_PROVIDER_CONFIG");
    }
    match provider_kind {
        "openai_compatible" => {
            let url = if base.ends_with("/chat/completions") {
                base.to_owned()
            } else {
                format!("{base}/chat/completions")
            };
            let messages: Vec<Value> = turns
                .iter()
                .map(|t| {
                    json!({
                        "role": if t.role == "assistant" { "assistant" } else { "user" },
                        "content": t.content
                    })
                })
                .collect();
            let body = serde_json::to_string(&json!({
                "model": model,
                "messages": messages,
                "max_tokens": max_tokens,
            }))
            .map_err(|_| "INVALID_PROVIDER_CONFIG")?;
            Ok(ProviderRequestSpec {
                url,
                headers: vec![
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                    ("Authorization".to_owned(), format!("Bearer {key}")),
                ],
                body,
            })
        }
        "anthropic_messages" => {
            let url = if base.ends_with("/messages") {
                base.to_owned()
            } else if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            };
            let messages: Vec<Value> = turns
                .iter()
                .map(|t| {
                    json!({
                        "role": if t.role == "assistant" { "assistant" } else { "user" },
                        "content": t.content
                    })
                })
                .collect();
            let body = serde_json::to_string(&json!({
                "model": model,
                "max_tokens": max_tokens,
                "messages": messages,
            }))
            .map_err(|_| "INVALID_PROVIDER_CONFIG")?;
            Ok(ProviderRequestSpec {
                url,
                headers: vec![
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                    ("x-api-key".to_owned(), key.to_owned()),
                    ("anthropic-version".to_owned(), "2023-06-01".to_owned()),
                ],
                body,
            })
        }
        "gemini_generate_content" => {
            let url = if base.contains(":generateContent") {
                base.to_owned()
            } else if base.ends_with("/models") {
                format!("{base}/{model}:generateContent")
            } else if base.contains("/v1beta") || base.contains("/v1") {
                format!("{base}/models/{model}:generateContent")
            } else {
                format!("{base}/v1beta/models/{model}:generateContent")
            };
            let contents: Vec<Value> = turns
                .iter()
                .map(|t| {
                    json!({
                        "role": if t.role == "assistant" { "model" } else { "user" },
                        "parts": [{ "text": t.content }]
                    })
                })
                .collect();
            let body = serde_json::to_string(&json!({
                "contents": contents,
                "generationConfig": {
                    "maxOutputTokens": max_tokens
                }
            }))
            .map_err(|_| "INVALID_PROVIDER_CONFIG")?;
            Ok(ProviderRequestSpec {
                url,
                headers: vec![
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                    ("x-goog-api-key".to_owned(), key.to_owned()),
                ],
                body,
            })
        }
        _ => Err("INVALID_PROVIDER_CONFIG"),
    }
}

pub(crate) fn parse_provider_response(
    provider_kind: &str,
    status: u16,
    raw_body: &str,
) -> Result<ProviderReplyParsed, String> {
    if !(200..300).contains(&status) {
        let code = match status {
            401 | 403 => "auth_failed",
            404 => "model_not_found",
            429 => "rate_limited",
            _ => "network_error",
        };
        return Err(format!("{code}:HTTP_{status}"));
    }
    let parsed: Value =
        serde_json::from_str(raw_body).map_err(|_| "invalid_json_response".to_owned())?;
    match provider_kind {
        "openai_compatible" => {
            let text = parsed["choices"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|c| c["message"]["content"].as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "empty_provider_reply".to_owned())?
                .to_owned();
            let reported_model = parsed["model"].as_str().map(str::to_owned);
            let input_tokens = parsed["usage"]["prompt_tokens"].as_i64();
            let output_tokens = parsed["usage"]["completion_tokens"].as_i64();
            Ok(ProviderReplyParsed {
                text,
                reported_model,
                input_tokens,
                output_tokens,
            })
        }
        "anthropic_messages" => {
            let text = parsed["content"]
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .find_map(|part| part["text"].as_str())
                })
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "empty_provider_reply".to_owned())?
                .to_owned();
            let reported_model = parsed["model"].as_str().map(str::to_owned);
            let input_tokens = parsed["usage"]["input_tokens"].as_i64();
            let output_tokens = parsed["usage"]["output_tokens"].as_i64();
            Ok(ProviderReplyParsed {
                text,
                reported_model,
                input_tokens,
                output_tokens,
            })
        }
        "gemini_generate_content" => {
            let text = parsed["candidates"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|c| c["content"]["parts"].as_array())
                .and_then(|parts| parts.iter().find_map(|p| p["text"].as_str()))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "empty_provider_reply".to_owned())?
                .to_owned();
            let reported_model = parsed["modelVersion"].as_str().map(str::to_owned);
            let input_tokens = parsed["usageMetadata"]["promptTokenCount"].as_i64();
            let output_tokens = parsed["usageMetadata"]["candidatesTokenCount"].as_i64();
            Ok(ProviderReplyParsed {
                text,
                reported_model,
                input_tokens,
                output_tokens,
            })
        }
        _ => Err("invalid_config".to_owned()),
    }
}

fn send_loopback_http(
    spec: &ProviderRequestSpec,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<(u16, String), String> {
    let rest = spec
        .url
        .strip_prefix("http://")
        .ok_or_else(|| "invalid_config".to_owned())?;
    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let normalized_hp = host_port.replace("localhost:", "127.0.0.1:");
    let addr: SocketAddr = normalized_hp
        .parse()
        .map_err(|_| "invalid_config".to_owned())?;
    if !addr.ip().is_loopback() {
        return Err("invalid_config".to_owned());
    }
    if cancel.load(Ordering::SeqCst) {
        return Err("cancelled".to_owned());
    }
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|_| "network_error".to_owned())?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = stream.set_write_timeout(Some(timeout));

    let body_bytes = spec.body.as_bytes();
    let mut req_head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body_bytes.len()
    );
    for (k, v) in &spec.headers {
        req_head.push_str(&format!("{k}: {v}\r\n"));
    }
    req_head.push_str("\r\n");
    stream
        .write_all(req_head.as_bytes())
        .and_then(|_| stream.write_all(body_bytes))
        .and_then(|_| stream.flush())
        .map_err(|_| "network_error".to_owned())?;

    let deadline = Instant::now() + timeout;
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_owned());
        }
        if Instant::now() >= deadline {
            return Err("network_error:timeout".to_owned());
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > MAX_RESPONSE_BYTES {
                    return Err("network_error:response_too_large".to_owned());
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e)
                if !raw.is_empty()
                    && (e.kind() == std::io::ErrorKind::ConnectionReset
                        || e.kind() == std::io::ErrorKind::ConnectionAborted) =>
            {
                break;
            }
            Err(_) => return Err("network_error".to_owned()),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))
        .ok_or_else(|| "network_error:malformed_http".to_owned())?;
    let status_line = head.lines().next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "network_error:malformed_status".to_owned())?;
    Ok((status, body.to_owned()))
}

#[cfg(not(target_os = "android"))]
fn send_desktop_curl_https(
    spec: &ProviderRequestSpec,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<(u16, String), String> {
    use std::process::{Command, Stdio};
    if cancel.load(Ordering::SeqCst) {
        return Err("cancelled".to_owned());
    }
    let curl_bin = if cfg!(windows) && std::path::Path::new(r"C:\Windows\System32\curl.exe").is_file() {
        r"C:\Windows\System32\curl.exe"
    } else {
        "curl"
    };
    let max_secs = timeout.as_secs().max(5).to_string();
    let escape_curl = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config_stdin = String::new();
    config_stdin.push_str(&format!("url = \"{}\"\n", escape_curl(&spec.url)));
    config_stdin.push_str("request = \"POST\"\n");
    for (k, v) in &spec.headers {
        config_stdin.push_str(&format!("header = \"{}: {}\"\n", escape_curl(k), escape_curl(v)));
    }
    config_stdin.push_str(&format!("data = \"{}\"\n", escape_curl(&spec.body)));

    let mut child = Command::new(curl_bin)
        .args([
            "--silent",
            "--show-error",
            "--no-buffer",
            "--max-time",
            &max_secs,
            "--write-out",
            "\n__BOB_HTTP_STATUS__:%{http_code}",
            "-K",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "network_error:curl_unavailable".to_owned())?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config_stdin.as_bytes());
    }

    let deadline = Instant::now() + timeout + Duration::from_secs(2);
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".to_owned());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("network_error:timeout".to_owned());
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => return Err("network_error".to_owned()),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "network_error".to_owned())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some((body, status_str)) = stdout.rsplit_once("\n__BOB_HTTP_STATUS__:") {
        if let Ok(code) = status_str.trim().parse::<u16>() {
            if code > 0 {
                return Ok((code, body.to_owned()));
            }
        }
    }
    Err("network_error".to_owned())
}

pub(crate) async fn execute_http_spec<R: Runtime>(
    app: Option<&AppHandle<R>>,
    spec: ProviderRequestSpec,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
) -> Result<(u16, String), String> {
    if spec.url.starts_with("http://127.0.0.1") || spec.url.starts_with("http://localhost") {
        return tauri::async_runtime::spawn_blocking(move || {
            send_loopback_http(&spec, &cancel, timeout)
        })
        .await
        .map_err(|_| "network_error".to_owned())?;
    }
    #[cfg(target_os = "android")]
    {
        let host = app.ok_or_else(|| "network_error".to_owned())?;
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_owned());
        }
        let resp = crate::android_vault::https_request(
            host,
            crate::android_vault::AndroidHttpRequest {
                url: spec.url,
                method: "POST".to_owned(),
                headers: spec.headers,
                body: Some(spec.body),
                timeout_ms: timeout.as_millis().min(120_000) as u32,
            },
        )
        .await
        .map_err(|_| "network_error".to_owned())?;
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_owned());
        }
        Ok((resp.status, resp.body))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        tauri::async_runtime::spawn_blocking(move || {
            send_desktop_curl_https(&spec, &cancel, timeout)
        })
        .await
        .map_err(|_| "network_error".to_owned())?
    }
}

#[cfg(not(target_os = "android"))]
fn send_desktop_curl_get(
    url: &str,
    headers: &[(String, String)],
    timeout: Duration,
) -> Result<(u16, String), String> {
    use std::process::{Command, Stdio};
    let curl_bin = if cfg!(windows) && std::path::Path::new(r"C:\Windows\System32\curl.exe").is_file() {
        r"C:\Windows\System32\curl.exe"
    } else {
        "curl"
    };
    let max_secs = timeout.as_secs().max(5).to_string();
    let escape_curl = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config_stdin = String::new();
    config_stdin.push_str(&format!("url = \"{}\"\n", escape_curl(url)));
    config_stdin.push_str("request = \"GET\"\n");
    for (k, v) in headers {
        config_stdin.push_str(&format!("header = \"{}: {}\"\n", escape_curl(k), escape_curl(v)));
    }

    let mut child = Command::new(curl_bin)
        .args([
            "--silent",
            "--show-error",
            "--no-buffer",
            "--max-time",
            &max_secs,
            "--write-out",
            "\n__BOB_HTTP_STATUS__:%{http_code}",
            "-K",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "network_error:curl_unavailable".to_owned())?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config_stdin.as_bytes());
    }

    let output = child
        .wait_with_output()
        .map_err(|_| "network_error".to_owned())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some((body, status_str)) = stdout.rsplit_once("\n__BOB_HTTP_STATUS__:") {
        if let Ok(code) = status_str.trim().parse::<u16>() {
            if code > 0 {
                return Ok((code, body.to_owned()));
            }
        }
    }
    Err("network_error".to_owned())
}

fn send_loopback_http_get(
    url: &str,
    headers: &[(String, String)],
    timeout: Duration,
) -> Result<(u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "invalid_config".to_owned())?;
    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let normalized_hp = host_port.replace("localhost:", "127.0.0.1:");
    let addr: SocketAddr = normalized_hp
        .parse()
        .map_err(|_| "invalid_config".to_owned())?;
    if !addr.ip().is_loopback() {
        return Err("invalid_config".to_owned());
    }
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|_| "network_error".to_owned())?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = stream.set_write_timeout(Some(timeout));
    let mut req_head = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n"
    );
    for (k, v) in headers {
        req_head.push_str(&format!("{k}: {v}\r\n"));
    }
    req_head.push_str("\r\n");
    stream
        .write_all(req_head.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|_| "network_error".to_owned())?;

    let deadline = Instant::now() + timeout;
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if Instant::now() >= deadline {
            return Err("network_error:timeout".to_owned());
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > MAX_RESPONSE_BYTES {
                    return Err("network_error:response_too_large".to_owned());
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e)
                if !raw.is_empty()
                    && (e.kind() == std::io::ErrorKind::ConnectionReset
                        || e.kind() == std::io::ErrorKind::ConnectionAborted) =>
            {
                break;
            }
            Err(_) => return Err("network_error".to_owned()),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))
        .ok_or_else(|| "network_error:malformed_http".to_owned())?;
    let status_line = head.lines().next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "network_error:malformed_status".to_owned())?;
    Ok((status, body.to_owned()))
}

pub(crate) async fn execute_http_get<R: Runtime>(
    app: Option<&AppHandle<R>>,
    url: String,
    headers: Vec<(String, String)>,
    timeout: Duration,
) -> Result<(u16, String), String> {
    if url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost") {
        return tauri::async_runtime::spawn_blocking(move || {
            send_loopback_http_get(&url, &headers, timeout)
        })
        .await
        .map_err(|_| "network_error".to_owned())?;
    }
    #[cfg(target_os = "android")]
    {
        let host = app.ok_or_else(|| "network_error".to_owned())?;
        let resp = crate::android_vault::https_request(
            host,
            crate::android_vault::AndroidHttpRequest {
                url,
                method: "GET".to_owned(),
                headers,
                body: None,
                timeout_ms: timeout.as_millis().min(30_000) as u32,
            },
        )
        .await
        .map_err(|_| "network_error".to_owned())?;
        Ok((resp.status, resp.body))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        tauri::async_runtime::spawn_blocking(move || send_desktop_curl_get(&url, &headers, timeout))
            .await
            .map_err(|_| "network_error".to_owned())?
    }
}

fn is_chat_model_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    let skip_patterns = [
        "embed",
        "rerank",
        "tts",
        "whisper",
        "audio",
        "realtime",
        "moderation",
        "dall-e",
        "wanx",
        "cogview",
        "cogvideo",
        "paraformer",
        "cosyvoice",
        "sambert",
        "babbage",
        "davinci",
        "canary",
        "guard",
        "text-embedding",
    ];
    for p in skip_patterns {
        if lower.contains(p) {
            return false;
        }
    }
    if lower.starts_with("ft:") {
        return false;
    }
    true
}

fn is_high_signal_flagship_model(provider_id: &str, model_id: &str) -> bool {
    let lower = model_id.to_ascii_lowercase();
    match provider_id {
        "qwen" => {
            (lower.starts_with("qwen3.")
                || lower.starts_with("qwen4")
                || lower.starts_with("qwen-max")
                || lower.starts_with("qwen-plus"))
                && (lower.contains("-max")
                    || lower.contains("-flash")
                    || lower.contains("-plus")
                    || lower.contains("-coder"))
                && !lower.contains("preview")
                && !lower.contains("audio")
        }
        "openrouter" => {
            (lower.starts_with("deepseek/deepseek-v4")
                || lower.starts_with("moonshotai/kimi-k3")
                || lower.starts_with("qwen/qwen3.8")
                || lower.starts_with("google/gemini-3.8")
                || lower.starts_with("google/gemini-4")
                || lower.starts_with("anthropic/claude-fable")
                || lower.starts_with("anthropic/claude-sonnet-5")
                || lower.starts_with("openai/gpt-5.6"))
                && !lower.contains(":free")
                && !lower.contains("extended")
        }
        "modelscope" => {
            lower.contains("deepseek-v4")
                || lower.contains("kimi-k3")
                || lower.contains("qwen3.8")
                || lower.contains("glm-5.3")
        }
        _ => true,
    }
}

fn prettify_discovered_model_name(provider_id: &str, model_id: &str) -> String {
    let raw = model_id.strip_prefix("models/").unwrap_or(model_id);
    if provider_id == "deepseek" && raw.starts_with("deepseek-") {
        let rest = raw.trim_start_matches("deepseek-");
        return format!("DeepSeek {}", rest.replace('-', " ").to_uppercase());
    }
    if provider_id == "kimi" && raw.starts_with("kimi-") {
        let rest = raw.trim_start_matches("kimi-");
        return format!("Kimi {}", rest.to_uppercase());
    }
    raw.to_owned()
}

#[tauri::command]
pub(crate) async fn app_provider_get(
    state: State<'_, AppState>,
) -> Result<Option<DirectProviderProfile>, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.direct_provider_profile()
                .map_err(|_| "PROVIDER_READ_FAILED")
        })
    })
    .await
    .map_err(|_| "PROVIDER_READ_FAILED")?
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn app_provider_save<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    provider_kind: String,
    label: String,
    base_url: String,
    model_id: String,
    api_key: Option<String>,
    clear_key: Option<bool>,
    enabled: bool,
) -> Result<DirectProviderProfile, &'static str> {
    let existing = with_app(&state.0, |app| {
        app.direct_provider_profile()
            .map_err(|_| "PROVIDER_READ_FAILED")
    })?;
    let slot = existing
        .as_ref()
        .map(|p| p.api_key_slot.clone())
        .unwrap_or_else(|| DEFAULT_SLOT.to_owned());

    let key_configured = if clear_key.unwrap_or(false) {
        clear_api_key(&host, &slot).await?;
        false
    } else if let Some(ref k) = api_key {
        if !k.trim().is_empty() {
            store_api_key(&host, &slot, k).await?;
            true
        } else {
            existing.as_ref().is_some_and(|p| p.api_key_configured)
        }
    } else {
        existing.as_ref().is_some_and(|p| p.api_key_configured)
    };

    let input = DirectProviderProfileInput {
        id: "default".to_owned(),
        provider_kind,
        label,
        base_url,
        model_id,
        api_key_slot: slot,
        api_key_configured: key_configured,
        enabled,
    };
    let app_state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&app_state, |app| {
            app.upsert_direct_provider_profile(&input, now()?)
                .map_err(|_| "PROVIDER_SAVE_FAILED")
        })
    })
    .await
    .map_err(|_| "PROVIDER_SAVE_FAILED")?
}

#[tauri::command]
pub(crate) async fn app_provider_test<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<DirectProviderProfile, &'static str> {
    let profile = with_app(&state.0, |app| {
        app.direct_provider_profile()
            .map_err(|_| "PROVIDER_READ_FAILED")
    })?
    .ok_or("PROVIDER_UNCONFIGURED")?;

    if !profile.api_key_configured {
        return with_app(&state.0, |app| {
            app.record_direct_provider_check(
                &profile.id,
                "auth_failed",
                "API_KEY_NOT_CONFIGURED",
                now()?,
            )
            .map_err(|_| "PROVIDER_CHECK_FAILED")
        });
    }

    let api_key = match read_api_key(&host, &profile.api_key_slot).await {
        Ok(k) => k,
        Err(reason) => {
            return with_app(&state.0, |app| {
                app.record_direct_provider_check(&profile.id, "auth_failed", reason, now()?)
                    .map_err(|_| "PROVIDER_CHECK_FAILED")
            });
        }
    };

    let probe_turn = vec![DirectChatTurn {
        message_id: "probe".to_owned(),
        role: "user".to_owned(),
        content: "Reply with OK.".to_owned(),
    }];
    let spec = match build_provider_request(
        &profile.provider_kind,
        &profile.base_url,
        &profile.model_id,
        &api_key,
        &probe_turn,
        16,
    ) {
        Ok(s) => s,
        Err(reason) => {
            return with_app(&state.0, |app| {
                app.record_direct_provider_check(&profile.id, "invalid_config", reason, now()?)
                    .map_err(|_| "PROVIDER_CHECK_FAILED")
            });
        }
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let outcome = execute_http_spec(Some(&host), spec, cancel, Duration::from_secs(20)).await;
    let (check_status, check_reason) = match outcome {
        Ok((http_status, body)) => match parse_provider_response(
            &profile.provider_kind,
            http_status,
            &body,
        ) {
            Ok(parsed) => (
                "ok",
                parsed
                    .reported_model
                    .unwrap_or_else(|| profile.model_id.clone()),
            ),
            Err(err) => {
                let status_code = if err.starts_with("auth_failed") {
                    "auth_failed"
                } else if err.starts_with("model_not_found") {
                    "model_not_found"
                } else if err.starts_with("rate_limited") {
                    "rate_limited"
                } else {
                    "network_error"
                };
                (status_code, err)
            }
        },
        Err(err) => ("network_error", err),
    };

    with_app(&state.0, |app| {
        app.record_direct_provider_check(&profile.id, check_status, &check_reason, now()?)
            .map_err(|_| "PROVIDER_CHECK_FAILED")
    })
}

#[tauri::command]
pub(crate) async fn app_route_get(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.conversation_execution_route(&session_id)
                .map_err(|_| "ROUTE_READ_FAILED")
        })
    })
    .await
    .map_err(|_| "ROUTE_READ_FAILED")?
}

#[tauri::command]
pub(crate) async fn app_route_set(
    state: State<'_, AppState>,
    session_id: String,
    route: String,
) -> Result<String, &'static str> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_app(&state, |app| {
            app.set_conversation_execution_route(&session_id, &route, now()?)
                .map_err(|_| "ROUTE_SAVE_FAILED")
        })
    })
    .await
    .map_err(|_| "ROUTE_SAVE_FAILED")?
}

#[tauri::command]
pub(crate) async fn app_model_hub_get<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Value, &'static str> {
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_provider_key_set<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    provider_id: String,
    api_key: Option<String>,
    clear_key: Option<bool>,
) -> Result<Value, &'static str> {
    let pid = provider_id.trim();
    if pid.is_empty() {
        return Err("INVALID_PROVIDER_ID");
    }
    let slot = provider_slot(pid);
    let ts = now().unwrap_or(1);
    let mut cleared = false;
    if clear_key.unwrap_or(false) {
        clear_api_key(&host, &slot).await?;
        cleared = true;
    } else if let Some(ref k) = api_key {
        let trimmed = k.trim();
        if trimmed.is_empty() {
            clear_api_key(&host, &slot).await?;
            cleared = true;
        } else {
            store_api_key(&host, &slot, trimmed).await?;
        }
    }
    if let Ok(data_dir) = host.path().app_data_dir() {
        let mut hub = load_hub_state(&data_dir);
        if !hub.get("credentialTimestamps").is_some_and(Value::is_object) {
            hub["credentialTimestamps"] = json!({});
        }
        if cleared {
            if let Some(obj) = hub["credentialTimestamps"].as_object_mut() {
                obj.remove(&slot);
            }
        } else {
            hub["credentialTimestamps"][&slot] = json!(ts);
        }
        hub["updatedAt"] = json!(ts);
        let _ = save_hub_state(&data_dir, &hub);
    }
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_model_role_assign<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    role: String,
    model_entry_id: String,
) -> Result<Value, &'static str> {
    let r = role.trim();
    if !matches!(r, "main" | "clerk" | "vision") {
        return Err("INVALID_MODEL_ROLE");
    }
    let mid = model_entry_id.trim();
    if mid.is_empty() || !mid.contains("::") {
        return Err("INVALID_MODEL_ID");
    }
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut hub = load_hub_state(&data_dir);
    hub["activeRoles"][r] = json!(mid);
    hub["updatedAt"] = json!(now().unwrap_or(1));
    save_hub_state(&data_dir, &hub)?;

    if r == "main" {
        let registry = load_or_init_registry(&data_dir);
        let providers = registry
            .get("providers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (eff_prov, eff_model) = mid
            .split_once("::")
            .unwrap_or(("deepseek", "deepseek-v4-flash"));
        let variants = hub
            .get("providerVariants")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let custom_models = hub
            .get("customModels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (label, base_url, slot) = if let Some(cid) = eff_prov.strip_prefix("custom-") {
            let cm = custom_models
                .iter()
                .find(|c| c["id"].as_str() == Some(cid));
            let dname = cm
                .and_then(|c| c["displayName"].as_str())
                .unwrap_or("Custom")
                .to_owned();
            let burl = cm
                .and_then(|c| c["baseUrl"].as_str())
                .unwrap_or("https://api.openai.com/v1")
                .to_owned();
            (dname, burl, provider_slot(eff_prov))
        } else {
            let prov_val = providers
                .iter()
                .find(|p| p["id"].as_str() == Some(eff_prov));
            let pname = prov_val
                .and_then(|p| p["name"].as_str())
                .unwrap_or(eff_prov)
                .to_owned();
            let burl = prov_val
                .map(|p| resolve_provider_base_url(p, &variants))
                .unwrap_or_else(|| "https://api.deepseek.com".to_owned());
            (pname, burl, provider_slot(eff_prov))
        };
        let protocol = infer_provider_protocol(eff_prov, &base_url).to_owned();
        let key_configured = read_api_key(&host, &slot)
            .await
            .is_ok_and(|k| !k.trim().is_empty());
        let input = DirectProviderProfileInput {
            id: "default".to_owned(),
            provider_kind: protocol,
            label,
            base_url,
            model_id: eff_model.to_owned(),
            api_key_slot: slot,
            api_key_configured: key_configured,
            enabled: true,
        };
        let _ = with_app(&state.0, |app| {
            app.upsert_direct_provider_profile(&input, now()?)
                .map_err(|_| "PROVIDER_SAVE_FAILED")
        });
    }

    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_provider_variant_set<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    provider_id: String,
    variant: String,
) -> Result<Value, &'static str> {
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut hub = load_hub_state(&data_dir);
    hub["providerVariants"][provider_id.trim()] = json!(variant.trim());
    hub["updatedAt"] = json!(now().unwrap_or(1));
    save_hub_state(&data_dir, &hub)?;
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_custom_model_save<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
    model_id: String,
    base_url: String,
    api_key: String,
) -> Result<Value, &'static str> {
    let dname = name.trim();
    let mid = if model_id.trim().is_empty() {
        dname
    } else {
        model_id.trim()
    };
    let burl = base_url.trim();
    let key = api_key.trim();
    if dname.is_empty() || burl.is_empty() || key.is_empty() {
        return Err("INVALID_CUSTOM_MODEL");
    }
    let ts = now()?;
    let cid = format!("cm-{ts}");
    let slot = provider_slot(&format!("custom-{cid}"));
    store_api_key(&host, &slot, key).await?;

    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut hub = load_hub_state(&data_dir);
    let mut list = hub
        .get("customModels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    list.push(json!({
        "id": cid,
        "displayName": dname,
        "modelId": mid,
        "baseUrl": burl,
        "updatedAt": ts,
    }));
    hub["customModels"] = json!(list);
    if !hub.get("credentialTimestamps").is_some_and(Value::is_object) {
        hub["credentialTimestamps"] = json!({});
    }
    hub["credentialTimestamps"][&slot] = json!(ts);
    hub["updatedAt"] = json!(ts);
    save_hub_state(&data_dir, &hub)?;
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_custom_model_delete<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, &'static str> {
    let cid = id.trim();
    let slot = provider_slot(&format!("custom-{cid}"));
    let _ = clear_api_key(&host, &slot).await;
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut hub = load_hub_state(&data_dir);
    let mut list = hub
        .get("customModels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    list.retain(|item| item["id"].as_str() != Some(cid));
    hub["customModels"] = json!(list);
    if let Some(obj) = hub
        .get_mut("credentialTimestamps")
        .and_then(Value::as_object_mut)
    {
        obj.remove(&slot);
    }
    hub["updatedAt"] = json!(now().unwrap_or(1));
    save_hub_state(&data_dir, &hub)?;
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn app_remote_node_save<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    id: Option<String>,
    label: Option<String>,
    base_url: String,
    token: Option<String>,
    clear_token: Option<bool>,
    selected_project_id: Option<String>,
    enabled: Option<bool>,
) -> Result<Value, &'static str> {
    let burl = base_url.trim().trim_end_matches('/');
    if burl.is_empty() {
        return Err("INVALID_REMOTE_NODE_URL");
    }
    let ts = now()?;
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut hub = load_hub_state(&data_dir);
    let mut nodes = hub
        .get("remoteNodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let resolved_id = id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            nodes
                .iter()
                .find(|n| n["baseUrl"].as_str() == Some(burl))
                .and_then(|n| n["id"].as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            let mut h = 0u64;
            for b in burl.bytes() {
                h = h.wrapping_mul(31).wrapping_add(b as u64);
            }
            format!("node-{h:x}")
        });

    let slot = remote_node_token_slot(&resolved_id);
    if clear_token.unwrap_or(false) {
        let _ = clear_api_key(&host, &slot).await;
        if let Some(obj) = hub
            .get_mut("credentialTimestamps")
            .and_then(Value::as_object_mut)
        {
            obj.remove(&slot);
        }
    } else if let Some(ref raw_token) = token {
        let trimmed = raw_token.trim();
        if !trimmed.is_empty() {
            store_api_key(&host, &slot, trimmed).await?;
            if !hub.get("credentialTimestamps").is_some_and(Value::is_object) {
                hub["credentialTimestamps"] = json!({});
            }
            hub["credentialTimestamps"][&slot] = json!(ts);
        } else {
            let _ = clear_api_key(&host, &slot).await;
            if let Some(obj) = hub
                .get_mut("credentialTimestamps")
                .and_then(Value::as_object_mut)
            {
                obj.remove(&slot);
            }
        }
    }

    let node_label = label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(burl)
        .to_owned();
    let entry = json!({
        "id": resolved_id,
        "label": node_label,
        "baseUrl": burl,
        "selectedProjectId": selected_project_id.unwrap_or_default(),
        "enabled": enabled.unwrap_or(true),
        "updatedAt": ts,
    });
    if let Some(existing) = nodes
        .iter_mut()
        .find(|n| n["id"].as_str() == Some(&resolved_id) || n["baseUrl"].as_str() == Some(burl))
    {
        *existing = entry;
    } else {
        nodes.push(entry);
    }
    hub["remoteNodes"] = json!(nodes);
    hub["updatedAt"] = json!(ts);
    save_hub_state(&data_dir, &hub)?;
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_remote_node_delete<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, &'static str> {
    let nid = id.trim();
    if nid.is_empty() {
        return Err("INVALID_REMOTE_NODE_ID");
    }
    let slot = remote_node_token_slot(nid);
    let _ = clear_api_key(&host, &slot).await;
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut hub = load_hub_state(&data_dir);
    let mut nodes = hub
        .get("remoteNodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    nodes.retain(|n| n["id"].as_str() != Some(nid));
    hub["remoteNodes"] = json!(nodes);
    if let Some(obj) = hub
        .get_mut("credentialTimestamps")
        .and_then(Value::as_object_mut)
    {
        obj.remove(&slot);
    }
    hub["updatedAt"] = json!(now().unwrap_or(1));
    save_hub_state(&data_dir, &hub)?;
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_registry_save<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    registry: Option<Value>,
) -> Result<Value, &'static str> {
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    match registry {
        Some(reg) if reg.get("providers").and_then(Value::as_array).is_some() => {
            save_registry_file(&data_dir, &reg)?;
        }
        _ => {
            let def: Value = serde_json::from_str(EMBEDDED_MODEL_REGISTRY)
                .map_err(|_| "REGISTRY_RESET_FAILED")?;
            save_registry_file(&data_dir, &def)?;
        }
    }
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_model_visibility_set<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    provider_id: String,
    model_id: String,
    visible: bool,
) -> Result<Value, &'static str> {
    let pid = provider_id.trim();
    let mid = model_id.trim();
    if pid.is_empty() || mid.is_empty() {
        return Err("INVALID_MODEL_ID");
    }
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut registry = load_or_init_registry(&data_dir);
    if let Some(providers) = registry.get_mut("providers").and_then(Value::as_array_mut) {
        if let Some(prov) = providers
            .iter_mut()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
        {
            if let Some(models) = prov.get_mut("models").and_then(Value::as_array_mut) {
                if let Some(m) = models
                    .iter_mut()
                    .find(|m| m.get("id").and_then(Value::as_str) == Some(mid))
                {
                    m["visible"] = json!(visible);
                }
            }
        }
    }
    save_registry_file(&data_dir, &registry)?;
    build_model_hub_snapshot_and_sync(&host, &state).await
}

#[tauri::command]
pub(crate) async fn app_models_discover<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    provider_id: Option<String>,
) -> Result<Value, &'static str> {
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut registry = load_or_init_registry(&data_dir);
    let hub = load_hub_state(&data_dir);
    let variants = hub
        .get("providerVariants")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let filter_pid = provider_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let mut targets = Vec::new();
    if let Some(providers) = registry.get("providers").and_then(Value::as_array) {
        for p in providers {
            let pid = p.get("id").and_then(Value::as_str).unwrap_or("");
            if pid.is_empty() || pid == "offline" || pid == "vertex_ai" {
                continue;
            }
            if let Some(target_pid) = filter_pid {
                if pid != target_pid {
                    continue;
                }
            }
            let slot = provider_slot(pid);
            let Ok(api_key) = read_api_key(&host, &slot).await else {
                continue;
            };
            let trimmed_key = api_key.trim().to_owned();
            if trimmed_key.is_empty() {
                continue;
            }
            let base_url = resolve_provider_base_url(p, &variants);
            if base_url.is_empty() {
                continue;
            }
            targets.push((pid.to_owned(), base_url, trimmed_key));
        }
    }

    let mut any_updated = false;
    for (pid, base_url, api_key) in targets {
        let base_trimmed = base_url.trim_end_matches('/');
        let (models_url, headers) = if pid == "gemini"
            || (base_trimmed.contains("generativelanguage.googleapis.com")
                && !base_trimmed.contains("/openai"))
        {
            (
                format!("{base_trimmed}/models?key={api_key}"),
                vec![("Accept".to_owned(), "application/json".to_owned())],
            )
        } else if pid == "anthropic" || base_trimmed.contains("api.anthropic.com") {
            let url = if base_trimmed.ends_with("/v1") {
                format!("{base_trimmed}/models")
            } else {
                format!("{base_trimmed}/v1/models")
            };
            (
                url,
                vec![
                    ("x-api-key".to_owned(), api_key),
                    ("anthropic-version".to_owned(), "2023-06-01".to_owned()),
                    ("Accept".to_owned(), "application/json".to_owned()),
                ],
            )
        } else {
            let url = if base_trimmed.ends_with("/v1")
                || base_trimmed.contains("/compatible-mode/")
                || base_trimmed.contains("/api/v3")
            {
                format!("{base_trimmed}/models")
            } else {
                format!("{base_trimmed}/v1/models")
            };
            (
                url,
                vec![
                    ("Authorization".to_owned(), format!("Bearer {api_key}")),
                    ("Accept".to_owned(), "application/json".to_owned()),
                ],
            )
        };

        let Ok((status, body)) =
            execute_http_get(Some(&host), models_url, headers, Duration::from_secs(10)).await
        else {
            continue;
        };
        if !(200..300).contains(&status) {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        let items = parsed
            .get("data")
            .and_then(Value::as_array)
            .or_else(|| parsed.get("models").and_then(Value::as_array));
        let Some(remote_list) = items else {
            continue;
        };

        let is_catalog_heavy = matches!(pid.as_str(), "qwen" | "openrouter" | "modelscope");
        let max_auto_visible = if is_catalog_heavy { 4usize } else { 8usize };

        if let Some(providers) = registry.get_mut("providers").and_then(Value::as_array_mut) {
            if let Some(prov) = providers
                .iter_mut()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(pid.as_str()))
            {
                if let Some(models) = prov.get_mut("models").and_then(Value::as_array_mut) {
                    let mut auto_promoted = 0usize;
                    let mut newly_visible = Vec::new();
                    let mut newly_hidden = Vec::new();
                    for rm in remote_list {
                        let raw_id = rm
                            .get("id")
                            .or_else(|| rm.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim();
                        if raw_id.is_empty() || !is_chat_model_id(raw_id) {
                            continue;
                        }
                        let clean_id = if pid == "gemini" {
                            raw_id.strip_prefix("models/").unwrap_or(raw_id)
                        } else {
                            raw_id
                        };
                        if models.iter().any(|m| {
                            let existing_id = m.get("id").and_then(Value::as_str).unwrap_or("");
                            existing_id == clean_id || existing_id == raw_id
                        }) {
                            continue;
                        }
                        let display_name = rm
                            .get("display_name")
                            .or_else(|| rm.get("displayName"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .unwrap_or_else(|| prettify_discovered_model_name(&pid, clean_id));

                        let should_be_visible = if is_catalog_heavy {
                            auto_promoted < max_auto_visible
                                && is_high_signal_flagship_model(&pid, clean_id)
                        } else {
                            auto_promoted < max_auto_visible
                        };

                        if should_be_visible {
                            auto_promoted += 1;
                            newly_visible.push(json!({
                                "id": clean_id,
                                "name": display_name,
                                "visible": true,
                                "discovered": true,
                                "vision": clean_id.contains("vl") || clean_id.contains("vision") || clean_id.contains("k3") || clean_id.contains("v4"),
                                "pricing": { "input": 0.0, "output": 0.0 }
                            }));
                        } else {
                            newly_hidden.push(json!({
                                "id": clean_id,
                                "name": display_name,
                                "visible": false,
                                "discovered": true,
                                "vision": false,
                                "pricing": { "input": 0.0, "output": 0.0 }
                            }));
                        }
                    }
                    if !newly_visible.is_empty() || !newly_hidden.is_empty() {
                        any_updated = true;
                        let mut combined = newly_visible;
                        combined.append(models);
                        combined.extend(newly_hidden);
                        *models = combined;
                    }
                }
            }
        }
    }

    if any_updated {
        let _ = save_registry_file(&data_dir, &registry);
    }
    build_model_hub_snapshot_and_sync(&host, &state).await
}

fn known_credential_slots(data_dir: &std::path::Path, hub: &Value) -> Vec<String> {
    let mut slots = vec![DEFAULT_SLOT.to_owned()];
    let registry = load_or_init_registry(data_dir);
    if let Some(providers) = registry.get("providers").and_then(Value::as_array) {
        for p in providers {
            if let Some(pid) = p.get("id").and_then(Value::as_str) {
                if !pid.is_empty() && pid != "offline" {
                    slots.push(provider_slot(pid));
                }
            }
        }
    }
    for tid in TOOL_KEY_IDS {
        slots.push(provider_slot(tid));
    }
    if let Some(cms) = hub.get("customModels").and_then(Value::as_array) {
        for cm in cms {
            if let Some(cid) = cm.get("id").and_then(Value::as_str) {
                if !cid.is_empty() {
                    slots.push(provider_slot(&format!("custom-{cid}")));
                }
            }
        }
    }
    if let Some(nodes) = hub.get("remoteNodes").and_then(Value::as_array) {
        for n in nodes {
            if let Some(nid) = n.get("id").and_then(Value::as_str) {
                if !nid.is_empty() {
                    slots.push(remote_node_token_slot(nid));
                }
            }
        }
    }
    if let Some(ts_map) = hub.get("credentialTimestamps").and_then(Value::as_object) {
        for k in ts_map.keys() {
            if !slots.contains(k) {
                slots.push(k.clone());
            }
        }
    }
    slots
}

fn sanitize_remote_nodes_metadata(nodes: &Value) -> Value {
    let Some(arr) = nodes.as_array() else {
        return json!([]);
    };
    let sanitized: Vec<Value> = arr
        .iter()
        .filter_map(|n| {
            let mut copy = n.clone();
            let obj = copy.as_object_mut()?;
            obj.remove("token");
            Some(copy)
        })
        .collect();
    json!(sanitized)
}

pub(crate) fn export_shared_config_from_dir(data_dir: &std::path::Path) -> String {
    let hub = load_hub_state(data_dir);
    let remote_nodes = hub
        .get("remoteNodes")
        .map(sanitize_remote_nodes_metadata)
        .unwrap_or_else(|| json!([]));

    // S0 Trust Boundary: Vault secrets (API keys and Online bearer tokens) are never
    // exported in PeerSync shared_config_json. Only whitelisted non-secret metadata is synced.
    serde_json::to_string(&json!({
        "updatedAt": hub.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
        "activeRoles": hub.get("activeRoles").cloned().unwrap_or_else(|| default_hub_state()["activeRoles"].clone()),
        "providerVariants": hub.get("providerVariants").cloned().unwrap_or_else(|| json!({})),
        "customModels": hub.get("customModels").cloned().unwrap_or_else(|| json!([])),
        "remoteNodes": remote_nodes,
        "credentials": {},
    }))
    .unwrap_or_default()
}

pub(crate) async fn export_shared_config_for_sync<R: Runtime>(host: &AppHandle<R>) -> String {
    let Ok(data_dir) = host.path().app_data_dir() else {
        return String::new();
    };
    export_shared_config_from_dir(&data_dir)
}

fn merge_hub_metadata(
    data_dir: &std::path::Path,
    hub: &mut Value,
    incoming: &Value,
    configured_slots: &std::collections::HashSet<String>,
) -> Option<DirectProviderProfileInput> {
    let local_ts = hub.get("updatedAt").and_then(Value::as_i64).unwrap_or(0);
    let incoming_ts = incoming
        .get("updatedAt")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    // Merge customModels by id
    if let Some(inc_cms) = incoming.get("customModels").and_then(Value::as_array) {
        let mut local_cms = hub
            .get("customModels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for cm in inc_cms {
            let Some(cid) = cm.get("id").and_then(Value::as_str) else {
                continue;
            };
            if let Some(existing) = local_cms
                .iter_mut()
                .find(|x| x.get("id").and_then(Value::as_str) == Some(cid))
            {
                let ex_ts = existing
                    .get("updatedAt")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let in_ts = cm.get("updatedAt").and_then(Value::as_i64).unwrap_or(1);
                if in_ts >= ex_ts {
                    *existing = cm.clone();
                }
            } else {
                local_cms.push(cm.clone());
            }
        }
        hub["customModels"] = json!(local_cms);
    }

    // Merge remoteNodes by id or baseUrl (stripping any raw token fields)
    if let Some(inc_nodes) = incoming.get("remoteNodes").and_then(Value::as_array) {
        let mut local_nodes = hub
            .get("remoteNodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for raw_node in inc_nodes {
            let mut node = raw_node.clone();
            if let Some(obj) = node.as_object_mut() {
                obj.remove("token");
            }
            let nid = node.get("id").and_then(Value::as_str).unwrap_or("");
            let burl = node.get("baseUrl").and_then(Value::as_str).unwrap_or("");
            if nid.is_empty() && burl.is_empty() {
                continue;
            }
            if let Some(existing) = local_nodes.iter_mut().find(|x| {
                (!nid.is_empty() && x.get("id").and_then(Value::as_str) == Some(nid))
                    || (!burl.is_empty() && x.get("baseUrl").and_then(Value::as_str) == Some(burl))
            }) {
                let ex_ts = existing
                    .get("updatedAt")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let in_ts = node.get("updatedAt").and_then(Value::as_i64).unwrap_or(1);
                if in_ts >= ex_ts {
                    *existing = node;
                }
            } else {
                local_nodes.push(node);
            }
        }
        hub["remoteNodes"] = json!(local_nodes);
    }

    // Merge providerVariants
    if let Some(inc_vars) = incoming.get("providerVariants").and_then(Value::as_object) {
        if !hub.get("providerVariants").is_some_and(Value::is_object) {
            hub["providerVariants"] = json!({});
        }
        if let Some(local_vars) = hub["providerVariants"].as_object_mut() {
            for (k, v) in inc_vars {
                if incoming_ts >= local_ts || !local_vars.contains_key(k) {
                    local_vars.insert(k.clone(), v.clone());
                }
            }
        }
    }

    // Merge activeRoles
    if let Some(inc_roles) = incoming.get("activeRoles").and_then(Value::as_object) {
        if !hub.get("activeRoles").is_some_and(Value::is_object) {
            hub["activeRoles"] = default_hub_state()["activeRoles"].clone();
        }
        for role in ["main", "clerk", "vision"] {
            if let Some(inc_val) = inc_roles.get(role).and_then(Value::as_str) {
                if inc_val.contains("::") {
                    let cur_val = hub["activeRoles"][role].as_str().unwrap_or("");
                    let cur_prov = cur_val.split("::").next().unwrap_or("");
                    let inc_prov = inc_val.split("::").next().unwrap_or("");
                    let cur_has_key = configured_slots.contains(&provider_slot(cur_prov));
                    let inc_has_key = configured_slots.contains(&provider_slot(inc_prov));
                    if incoming_ts >= local_ts || (!cur_has_key && inc_has_key) {
                        hub["activeRoles"][role] = json!(inc_val);
                    }
                }
            }
        }
    }

    if incoming_ts > local_ts {
        hub["updatedAt"] = json!(incoming_ts);
    }
    let _ = save_hub_state(data_dir, hub);

    // Sync effective main role into LocalApp DirectProviderProfile
    let main_entry_id = hub["activeRoles"]["main"]
        .as_str()
        .unwrap_or("deepseek::deepseek-v4-flash")
        .to_owned();
    let (eff_prov, eff_model) = main_entry_id
        .split_once("::")
        .unwrap_or(("deepseek", "deepseek-v4-flash"));
    let slot = provider_slot(eff_prov);
    if configured_slots.contains(&slot) {
        let registry = load_or_init_registry(data_dir);
        let providers = registry
            .get("providers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let variants = hub
            .get("providerVariants")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let custom_models = hub
            .get("customModels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (label, base_url) = if let Some(cid) = eff_prov.strip_prefix("custom-") {
            let cm = custom_models
                .iter()
                .find(|c| c["id"].as_str() == Some(cid));
            let dname = cm
                .and_then(|c| c["displayName"].as_str())
                .unwrap_or("Custom")
                .to_owned();
            let burl = cm
                .and_then(|c| c["baseUrl"].as_str())
                .unwrap_or("https://api.openai.com/v1")
                .to_owned();
            (dname, burl)
        } else {
            let prov_val = providers
                .iter()
                .find(|p| p["id"].as_str() == Some(eff_prov));
            let pname = prov_val
                .and_then(|p| p["name"].as_str())
                .unwrap_or(eff_prov)
                .to_owned();
            let burl = prov_val
                .map(|p| resolve_provider_base_url(p, &variants))
                .unwrap_or_else(|| "https://api.deepseek.com".to_owned());
            (pname, burl)
        };
        let protocol = infer_provider_protocol(eff_prov, &base_url).to_owned();
        let input = DirectProviderProfileInput {
            id: "default".to_owned(),
            provider_kind: protocol,
            label,
            base_url,
            model_id: eff_model.to_owned(),
            api_key_slot: slot,
            api_key_configured: true,
            enabled: true,
        };
        return Some(input);
    }
    None
}

pub(crate) fn merge_shared_config_with_app(
    data_dir: &std::path::Path,
    app: &mut bob_kernel::app::LocalApp,
    incoming_json: &str,
) {
    if incoming_json.trim().is_empty() {
        return;
    }
    let Ok(incoming) = serde_json::from_str::<Value>(incoming_json) else {
        return;
    };
    let _ = std::fs::create_dir_all(data_dir);
    let mut hub = load_hub_state(data_dir);
    if !hub.get("credentialTimestamps").is_some_and(Value::is_object) {
        hub["credentialTimestamps"] = json!({});
    }

    // S0 Trust Boundary: Incoming PeerSync payloads cannot overwrite or inject raw secrets
    // into the local device vault; only whitelisted non-secret metadata is merged.
    let mut configured_slots = std::collections::HashSet::new();
    #[cfg(not(target_os = "android"))]
    for slot in known_credential_slots(data_dir, &hub) {
        if crate::device_vault::read_secret(data_dir, &slot)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .is_some_and(|s| !s.trim().is_empty())
        {
            configured_slots.insert(slot);
        }
    }

    if let Some(input) = merge_hub_metadata(data_dir, &mut hub, &incoming, &configured_slots) {
        if let Ok(ts) = now() {
            let _ = app.upsert_direct_provider_profile(&input, ts);
        }
    }
}

pub(crate) async fn merge_shared_config_for_host<R: Runtime>(
    host: &AppHandle<R>,
    owner: &Arc<std::sync::Mutex<Option<bob_kernel::app::LocalApp>>>,
    incoming_json: &str,
) {
    if incoming_json.trim().is_empty() {
        return;
    }
    let Ok(incoming) = serde_json::from_str::<Value>(incoming_json) else {
        return;
    };
    let Ok(data_dir) = host.path().app_data_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&data_dir);
    let mut hub = load_hub_state(&data_dir);
    if !hub.get("credentialTimestamps").is_some_and(Value::is_object) {
        hub["credentialTimestamps"] = json!({});
    }

    // S0 Trust Boundary: Incoming PeerSync payloads cannot overwrite or inject raw secrets
    // into the local device vault; only whitelisted non-secret metadata is merged.
    let mut configured_slots = std::collections::HashSet::new();
    for slot in known_credential_slots(&data_dir, &hub) {
        if read_api_key(host, &slot)
            .await
            .is_ok_and(|s| !s.trim().is_empty())
        {
            configured_slots.insert(slot);
        }
    }

    if let Some(input) = merge_hub_metadata(&data_dir, &mut hub, &incoming, &configured_slots) {
        let _ = with_app(owner, |app| {
            app.upsert_direct_provider_profile(&input, now()?)
                .map_err(|_| "PROVIDER_SAVE_FAILED")
        });
    }
}

pub(crate) async fn execute_direct<R: Runtime>(
    host: AppHandle<R>,
    state: &State<'_, AppState>,
    runner: &State<'_, RunnerState>,
    prepared: DirectExecutionPrepared,
) -> Result<LocalExecution, &'static str> {
    if !prepared.can_start {
        return with_app(&state.0, |app| {
            app.local_executions(&prepared.conversation_id)
                .map_err(|_| "EXECUTION_READ_FAILED")?
                .into_iter()
                .find(|e| e.id == prepared.execution_id)
                .ok_or("EXECUTION_NOT_FOUND")
        });
    }

    let api_key = match read_api_key(&host, &prepared.api_key_slot).await {
        Ok(k) => k,
        Err(reason) => {
            return with_app(&state.0, |app| {
                app.finish_direct_execution(
                    &prepared.execution_id,
                    DirectExecutionOutcome::Failed {
                        reason: reason.to_owned(),
                    },
                    now()?,
                )
                .map_err(|_| "RESULT_SAVE_UNCONFIRMED")
            });
        }
    };

    let spec = match build_provider_request(
        &prepared.provider_kind,
        &prepared.base_url,
        &prepared.model_id,
        &api_key,
        &prepared.messages,
        4096,
    ) {
        Ok(s) => s,
        Err(reason) => {
            return with_app(&state.0, |app| {
                app.finish_direct_execution(
                    &prepared.execution_id,
                    DirectExecutionOutcome::Failed {
                        reason: reason.to_owned(),
                    },
                    now()?,
                )
                .map_err(|_| "RESULT_SAVE_UNCONFIRMED")
            });
        }
    };

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = runner.active.lock().map_err(|_| "RUNNER_UNAVAILABLE")?;
        active.insert(prepared.execution_id.clone(), cancel.clone());
    }

    let initial_view = with_app(&state.0, |app| {
        app.local_executions(&prepared.conversation_id)
            .map_err(|_| "EXECUTION_READ_FAILED")?
            .into_iter()
            .find(|e| e.id == prepared.execution_id)
            .ok_or("EXECUTION_NOT_FOUND")
    })?;

    let owner = state.0.clone();
    let runners = runner.active.clone();
    let exec_id = prepared.execution_id.clone();
    let provider_kind = prepared.provider_kind.clone();
    let host_clone = host.clone();

    tauri::async_runtime::spawn(async move {
        let http_res =
            execute_http_spec(Some(&host_clone), spec, cancel.clone(), Duration::from_secs(60))
                .await;
        let outcome = if cancel.load(Ordering::SeqCst) {
            DirectExecutionOutcome::Cancelled {
                reason: "cancelled".to_owned(),
            }
        } else {
            match http_res {
                Ok((status, body)) => match parse_provider_response(&provider_kind, status, &body) {
                    Ok(parsed) => DirectExecutionOutcome::Completed {
                        text: parsed.text,
                        provider_reported_model: parsed.reported_model,
                        input_tokens: parsed.input_tokens,
                        output_tokens: parsed.output_tokens,
                    },
                    Err(reason) => DirectExecutionOutcome::Failed { reason },
                },
                Err(reason) => {
                    if reason == "cancelled" {
                        DirectExecutionOutcome::Cancelled { reason }
                    } else {
                        DirectExecutionOutcome::Failed { reason }
                    }
                }
            }
        };
        let _ = with_app(&owner, |app| {
            app.finish_direct_execution(&exec_id, outcome, now()?)
                .map_err(|_| "RESULT_SAVE_UNCONFIRMED")
        });
        if let Ok(mut active) = runners.lock() {
            active.remove(&exec_id);
        }
    });

    Ok(initial_view)
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16).unwrap_or(0) as u8;
        let lo = (bytes[i + 1] as char).to_digit(16).unwrap_or(0) as u8;
        out.push((hi << 4) | lo);
        i += 2;
    }
    out
}

pub(crate) fn compute_node_id_for_origin(origin: &str) -> String {
    let burl = origin.trim().trim_end_matches('/');
    let mut h = 0u64;
    for b in burl.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    format!("node-{h:x}")
}

fn resolve_node_id(hub: &Value, origin: &str) -> String {
    let burl = origin.trim().trim_end_matches('/');
    if let Some(nodes) = hub.get("remoteNodes").and_then(Value::as_array) {
        if let Some(id) = nodes
            .iter()
            .find(|n| n["baseUrl"].as_str() == Some(burl))
            .and_then(|n| n["id"].as_str())
        {
            if !id.trim().is_empty() {
                return id.to_owned();
            }
        }
    }
    compute_node_id_for_origin(burl)
}

fn load_online_cache(data_dir: &std::path::Path) -> Value {
    let path = data_dir.join("online_cache.json");
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if val.is_object() {
                return val;
            }
        }
    }
    json!({})
}

fn save_online_cache(data_dir: &std::path::Path, cache: &Value) {
    let path = data_dir.join("online_cache.json");
    if let Ok(raw) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(path, raw);
    }
}

#[tauri::command]
pub(crate) async fn app_online_begin_pairing<R: Runtime>(
    host: AppHandle<R>,
    origin: String,
    client_instance_id: String,
    client_name: Option<String>,
) -> Result<Value, String> {
    let burl = origin.trim().trim_end_matches('/').to_owned();
    if burl.is_empty() {
        return Err("ONLINE_ORIGIN_REQUIRED".to_owned());
    }
    let ts = now().map_err(str::to_owned)?;
    let seed = format!(
        "bob-pkce:{}:{}:{}:{}",
        burl,
        client_instance_id,
        ts,
        std::process::id()
    );
    let verifier_bytes = hex_to_bytes(&bob_kernel::outbox::digest(seed.as_bytes()));
    let verifier = base64url_encode(&verifier_bytes);
    let challenge_bytes = hex_to_bytes(&bob_kernel::outbox::digest(verifier.as_bytes()));
    let challenge = base64url_encode(&challenge_bytes);

    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE".to_owned())?;
    std::fs::create_dir_all(&data_dir).map_err(|_| "LOCAL_STORE_UNAVAILABLE".to_owned())?;
    let mut hub = load_hub_state(&data_dir);
    if !hub.get("onlinePendingPairings").is_some_and(Value::is_object) {
        hub["onlinePendingPairings"] = json!({});
    }
    hub["onlinePendingPairings"][&burl] = json!({
        "verifier": verifier,
        "clientInstanceId": client_instance_id,
        "createdAt": ts
    });
    let _ = save_hub_state(&data_dir, &hub);

    let req_body = serde_json::to_string(&json!({
        "client_instance_id": client_instance_id,
        "client_name": client_name.unwrap_or_else(|| "Bob Unified Client".to_owned()),
        "protocol_version": "bob-core-client/1.0",
        "challenge_method": "S256",
        "challenge": challenge,
    }))
    .map_err(|_| "INVALID_JSON".to_owned())?;
    let spec = ProviderRequestSpec {
        url: format!("{burl}/api/client/v1/pairings"),
        headers: vec![
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Accept".to_owned(), "application/json".to_owned()),
        ],
        body: req_body,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let (status, raw) = execute_http_spec(Some(&host), spec, cancel, Duration::from_secs(15)).await?;
    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    if !(200..300).contains(&status) {
        let code = parsed["code"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("HTTP_{status}"));
        return Err(code);
    }
    if let Some(pid) = parsed.get("pairing_id").and_then(Value::as_str) {
        hub["onlinePendingPairings"][&burl]["pairingId"] = json!(pid);
        let _ = save_hub_state(&data_dir, &hub);
    }
    Ok(json!({
        "pairing_id": parsed.get("pairing_id").cloned().unwrap_or(Value::Null),
        "user_code": parsed.get("user_code").cloned().unwrap_or(Value::Null),
        "verification_uri": parsed.get("verification_uri").cloned().unwrap_or(Value::Null),
        "expires_at": parsed.get("expires_at").cloned().unwrap_or(Value::Null),
        "poll_interval_ms": parsed.get("poll_interval_ms").cloned().unwrap_or_else(|| json!(5000)),
        "origin": burl,
        "client_instance_id": client_instance_id,
    }))
}

#[tauri::command]
pub(crate) async fn app_online_exchange_pairing<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    origin: String,
    pairing_id: String,
    client_instance_id: String,
    verifier: Option<String>,
) -> Result<Value, String> {
    let burl = origin.trim().trim_end_matches('/').to_owned();
    let pid = pairing_id.trim().to_owned();
    if burl.is_empty() || pid.is_empty() {
        return Err("INVALID_ONLINE_PAIRING".to_owned());
    }
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE".to_owned())?;
    let mut hub = load_hub_state(&data_dir);
    let resolved_verifier = verifier
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            hub.get("onlinePendingPairings")
                .and_then(|m| m.get(&burl))
                .and_then(|e| e.get("verifier"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| "ONLINE_VERIFIER_MISSING".to_owned())?;

    let req_body = serde_json::to_string(&json!({
        "client_instance_id": client_instance_id,
        "verifier": resolved_verifier,
    }))
    .map_err(|_| "INVALID_JSON".to_owned())?;
    let spec = ProviderRequestSpec {
        url: format!("{burl}/api/client/v1/pairings/{pid}/exchange"),
        headers: vec![
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Accept".to_owned(), "application/json".to_owned()),
        ],
        body: req_body,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let (status, raw) = execute_http_spec(Some(&host), spec, cancel, Duration::from_secs(15)).await?;
    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    if status == 202 || parsed.get("status").and_then(Value::as_str) == Some("pending") {
        return Ok(json!({
            "pending": true,
            "status": "pending",
            "poll_interval_ms": parsed.get("poll_interval_ms").cloned().unwrap_or_else(|| json!(5000)),
        }));
    }
    if !(200..300).contains(&status) {
        let code = parsed["code"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("HTTP_{status}"));
        return Err(code);
    }
    let token = parsed
        .get("access_token")
        .or_else(|| parsed.get("bearer_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "ONLINE_TOKEN_MISSING".to_owned())?;

    let node_id = resolve_node_id(&hub, &burl);
    let slot = remote_node_token_slot(&node_id);
    store_api_key(&host, &slot, token)
        .await
        .map_err(str::to_owned)?;
    let ts = now().unwrap_or(1);
    if !hub.get("credentialTimestamps").is_some_and(Value::is_object) {
        hub["credentialTimestamps"] = json!({});
    }
    hub["credentialTimestamps"][&slot] = json!(ts);

    let mut nodes = hub
        .get("remoteNodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let entry = json!({
        "id": node_id,
        "label": burl,
        "baseUrl": burl,
        "credentialId": parsed.get("credential_id").cloned().unwrap_or(Value::Null),
        "principalId": parsed.get("principal_id").cloned().unwrap_or(Value::Null),
        "expiresAt": parsed.get("expires_at").cloned().unwrap_or(Value::Null),
        "enabled": true,
        "updatedAt": ts,
    });
    if let Some(existing) = nodes
        .iter_mut()
        .find(|n| n["id"].as_str() == Some(&node_id) || n["baseUrl"].as_str() == Some(&burl))
    {
        *existing = entry;
    } else {
        nodes.push(entry);
    }
    hub["remoteNodes"] = json!(nodes);
    if let Some(obj) = hub
        .get_mut("onlinePendingPairings")
        .and_then(Value::as_object_mut)
    {
        obj.remove(&burl);
    }
    let _ = save_hub_state(&data_dir, &hub);
    let _ = build_model_hub_snapshot_and_sync(&host, &state).await;

    Ok(json!({
        "pending": false,
        "connected": true,
        "hasToken": true,
        "credential_id": parsed.get("credential_id").cloned().unwrap_or(Value::Null),
        "principal_id": parsed.get("principal_id").cloned().unwrap_or(Value::Null),
        "expires_at": parsed.get("expires_at").cloned().unwrap_or(Value::Null),
        "grants": parsed.get("grants").cloned().unwrap_or_else(|| json!([])),
    }))
}

#[tauri::command]
pub(crate) async fn app_online_sync<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    origin: String,
    project_id: Option<String>,
    after: Option<i64>,
) -> Result<Value, String> {
    let burl = origin.trim().trim_end_matches('/').to_owned();
    if burl.is_empty() {
        return Err("ONLINE_ORIGIN_REQUIRED".to_owned());
    }
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE".to_owned())?;
    let hub = load_hub_state(&data_dir);
    let node_id = resolve_node_id(&hub, &burl);
    let slot = remote_node_token_slot(&node_id);
    let token = read_api_key(&host, &slot)
        .await
        .map_err(|_| "ONLINE_TOKEN_NOT_CONFIGURED".to_owned())?;
    let headers = vec![
        ("Authorization".to_owned(), format!("Bearer {}", token.trim())),
        ("Accept".to_owned(), "application/json".to_owned()),
    ];

    let mut cache = load_online_cache(&data_dir);
    let caps_res = execute_http_get(
        Some(&host),
        format!("{burl}/api/client/v1/capabilities"),
        headers.clone(),
        Duration::from_secs(12),
    )
    .await;

    let (caps_status, caps_raw) = match caps_res {
        Ok(pair) => pair,
        Err(net_err) => {
            if let Some(cached) = cache.get(&burl) {
                let mut fallback = cached.clone();
                if let Some(obj) = fallback.as_object_mut() {
                    obj.insert("offlineCached".to_owned(), json!(true));
                    obj.insert("syncError".to_owned(), json!(net_err));
                }
                return Ok(fallback);
            }
            return Err(net_err);
        }
    };
    if caps_status == 401 || caps_status == 403 {
        return Err("CLIENT_CREDENTIAL_INVALID".to_owned());
    }
    let caps: Value = serde_json::from_str(&caps_raw).unwrap_or_else(|_| json!({}));
    if !(200..300).contains(&caps_status) {
        let code = caps["code"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("HTTP_{caps_status}"));
        if let Some(cached) = cache.get(&burl) {
            let mut fallback = cached.clone();
            if let Some(obj) = fallback.as_object_mut() {
                obj.insert("offlineCached".to_owned(), json!(true));
                obj.insert("syncError".to_owned(), json!(code));
            }
            return Ok(fallback);
        }
        return Err(code);
    }

    let mut all_projects = Vec::new();
    let mut proj_cursor: Option<String> = None;
    for _ in 0..5 {
        let url = match proj_cursor {
            Some(ref c) => format!("{burl}/api/client/v1/projects?cursor={c}"),
            None => format!("{burl}/api/client/v1/projects"),
        };
        let Ok((st, body)) =
            execute_http_get(Some(&host), url, headers.clone(), Duration::from_secs(12)).await
        else {
            break;
        };
        if !(200..300).contains(&st) {
            break;
        }
        let Ok(page) = serde_json::from_str::<Value>(&body) else {
            break;
        };
        if let Some(items) = page.get("items").and_then(Value::as_array) {
            all_projects.extend(items.iter().cloned());
        }
        if page.get("has_more").and_then(Value::as_bool) == Some(true) {
            proj_cursor = page
                .get("next_cursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if proj_cursor.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    let selected_pid = project_id
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            all_projects
                .first()
                .and_then(|p| p.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });

    let mut tasks_list = Vec::new();
    let mut messages_list = Vec::new();
    let mut messages_next_after = after.unwrap_or(0);
    let mut messages_has_more = false;
    let mut task_snapshots = serde_json::Map::new();

    if let Some(ref pid) = selected_pid {
        if let Ok((st, body)) = execute_http_get(
            Some(&host),
            format!("{burl}/api/client/v1/projects/{pid}/tasks"),
            headers.clone(),
            Duration::from_secs(12),
        )
        .await
        {
            if (200..300).contains(&st) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&body) {
                    if let Some(items) = parsed.get("items").and_then(Value::as_array) {
                        tasks_list = items.clone();
                    }
                }
            }
        }

        let after_val = after.unwrap_or(0).max(0);
        if let Ok((st, body)) = execute_http_get(
            Some(&host),
            format!("{burl}/api/client/v1/projects/{pid}/messages?after={after_val}"),
            headers.clone(),
            Duration::from_secs(12),
        )
        .await
        {
            if (200..300).contains(&st) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&body) {
                    if let Some(items) = parsed.get("items").and_then(Value::as_array) {
                        messages_list = items.clone();
                    }
                    messages_next_after = parsed
                        .get("next_after")
                        .and_then(Value::as_i64)
                        .unwrap_or(after_val);
                    messages_has_more = parsed
                        .get("has_more")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                }
            }
        }

        for t in tasks_list.iter().take(8) {
            if let Some(tid) = t.get("id").and_then(Value::as_str) {
                if let Ok((st, body)) = execute_http_get(
                    Some(&host),
                    format!("{burl}/api/client/v1/tasks/{tid}/snapshot"),
                    headers.clone(),
                    Duration::from_secs(10),
                )
                .await
                {
                    if (200..300).contains(&st) {
                        if let Ok(snap_val) = serde_json::from_str::<Value>(&body) {
                            task_snapshots.insert(tid.to_owned(), snap_val);
                        }
                    }
                }
            }
        }

        // Persist Online sessions into bob-kernel SQLite online_provider_sessions
        let ts = now().unwrap_or(1);
        let proj_title = all_projects
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(pid.as_str()))
            .and_then(|p| p.get("title").or_else(|| p.get("name")))
            .and_then(Value::as_str)
            .unwrap_or(pid.as_str())
            .to_owned();
        let summary_snippet = messages_list
            .last()
            .and_then(|m| m.get("text").or_else(|| m.get("content")))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let tasks_for_db = tasks_list.clone();
        let node_id_clone = node_id.clone();
        let burl_clone = burl.clone();
        let pid_clone = pid.clone();
        let msg_count = messages_list.len() as i64;
        let _ = with_app(&state.0, |app| {
            if tasks_for_db.is_empty() {
                let _ = app.upsert_online_provider_session(
                    &node_id_clone,
                    &burl_clone,
                    "bob-online",
                    &pid_clone,
                    &format!("/online/{pid_clone}"),
                    &proj_title,
                    &summary_snippet,
                    msg_count,
                    false,
                    true,
                    ts,
                );
            } else {
                for task_item in &tasks_for_db {
                    let tid = task_item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or(&pid_clone);
                    let ttitle = task_item
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or(&proj_title);
                    let tstatus = task_item
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("observed");
                    let write_active = matches!(tstatus, "running" | "dispatching" | "pending");
                    let _ = app.upsert_online_provider_session(
                        &node_id_clone,
                        &burl_clone,
                        "bob-online",
                        tid,
                        &format!("/online/{pid_clone}"),
                        ttitle,
                        &summary_snippet,
                        msg_count,
                        write_active,
                        !write_active,
                        ts,
                    );
                }
            }
            Ok(())
        });
    }

    let payload = json!({
        "connected": true,
        "offlineCached": false,
        "syncError": Value::Null,
        "capabilities": caps,
        "projects": all_projects,
        "selectedProjectId": selected_pid,
        "tasks": tasks_list,
        "messages": messages_list,
        "messagesNextAfter": messages_next_after,
        "messagesHasMore": messages_has_more,
        "taskSnapshots": task_snapshots,
    });
    cache[&burl] = payload.clone();
    save_online_cache(&data_dir, &cache);
    Ok(payload)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn app_online_submit_operation<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    origin: String,
    operation_kind: String,
    project_id: String,
    task_id: Option<String>,
    prompt: Option<String>,
    parent_version_id: Option<String>,
    request_id: Option<String>,
) -> Result<Value, String> {
    let burl = origin.trim().trim_end_matches('/').to_owned();
    let pid = project_id.trim().to_owned();
    if burl.is_empty() || pid.is_empty() {
        return Err("INVALID_ONLINE_OPERATION".to_owned());
    }
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE".to_owned())?;
    let hub = load_hub_state(&data_dir);
    let node_id = resolve_node_id(&hub, &burl);
    let slot = remote_node_token_slot(&node_id);
    let token = read_api_key(&host, &slot)
        .await
        .map_err(|_| "ONLINE_TOKEN_NOT_CONFIGURED".to_owned())?;

    let req_id = request_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("op-{}", now().unwrap_or(1)));
    let body_json = json!({
        "request_id": req_id,
        "kind": operation_kind,
        "project_id": pid,
        "task_id": task_id,
        "prompt": prompt,
        "parent_version_id": parent_version_id,
    });
    let spec = ProviderRequestSpec {
        url: format!("{burl}/api/client/v1/operations"),
        headers: vec![
            ("Authorization".to_owned(), format!("Bearer {}", token.trim())),
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Accept".to_owned(), "application/json".to_owned()),
        ],
        body: serde_json::to_string(&body_json).map_err(|_| "INVALID_JSON".to_owned())?,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let (status, raw) = execute_http_spec(Some(&host), spec, cancel, Duration::from_secs(30)).await?;
    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    if !(200..300).contains(&status) {
        let code = parsed["code"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("HTTP_{status}"));
        return Err(code);
    }

    // Bind or update local continuation conversation in bob-kernel if applicable
    let _ = with_app(&state.0, |app| {
        if let Ok(sessions) = app.online_provider_sessions() {
            let target_native = task_id.as_deref().unwrap_or(&pid);
            if let Some(sess) = sessions
                .iter()
                .find(|s| s.provider_node_id == node_id && s.native_session_id == target_native)
            {
                let conv_id = format!("conv-online-{}", &bob_kernel::outbox::digest(sess.id.as_bytes())[..12]);
                let _ = app.bind_online_provider_session(
                    &sess.id,
                    &conv_id,
                    &node_id,
                    "bob-online",
                    &sess.project_path,
                    true,
                    now().unwrap_or(1),
                );
            }
        }
        Ok(())
    });

    Ok(parsed)
}

#[tauri::command]
pub(crate) async fn app_online_revoke<R: Runtime>(
    host: AppHandle<R>,
    state: State<'_, AppState>,
    origin: String,
    local_only: Option<bool>,
) -> Result<Value, String> {
    let burl = origin.trim().trim_end_matches('/').to_owned();
    let data_dir = host
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE".to_owned())?;
    let mut hub = load_hub_state(&data_dir);
    let node_id = resolve_node_id(&hub, &burl);
    let slot = remote_node_token_slot(&node_id);

    let mut remote_revoked = false;
    let mut revoke_error: Option<String> = None;
    if !local_only.unwrap_or(false) && !burl.is_empty() {
        if let Ok(token) = read_api_key(&host, &slot).await {
            if !token.trim().is_empty() {
                let spec = ProviderRequestSpec {
                    url: format!("{burl}/api/client/v1/credentials/self/revoke"),
                    headers: vec![
                        ("Authorization".to_owned(), format!("Bearer {}", token.trim())),
                        ("Content-Type".to_owned(), "application/json".to_owned()),
                        ("Accept".to_owned(), "application/json".to_owned()),
                    ],
                    body: "{}".to_owned(),
                };
                let cancel = Arc::new(AtomicBool::new(false));
                match execute_http_spec(Some(&host), spec, cancel, Duration::from_secs(10)).await {
                    Ok((st, _)) if (200..300).contains(&st) => {
                        remote_revoked = true;
                    }
                    Ok((st, raw)) => {
                        let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
                        revoke_error = Some(
                            parsed["code"]
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| format!("HTTP_{st}")),
                        );
                    }
                    Err(e) => {
                        revoke_error = Some(e);
                    }
                }
            }
        }
    }

    let _ = clear_api_key(&host, &slot).await;
    if let Some(obj) = hub
        .get_mut("credentialTimestamps")
        .and_then(Value::as_object_mut)
    {
        obj.remove(&slot);
    }
    if let Some(nodes) = hub.get_mut("remoteNodes").and_then(Value::as_array_mut) {
        for n in nodes.iter_mut() {
            if n["id"].as_str() == Some(&node_id) || n["baseUrl"].as_str() == Some(&burl) {
                n["enabled"] = json!(false);
                n["updatedAt"] = json!(now().unwrap_or(1));
            }
        }
    }
    let _ = save_hub_state(&data_dir, &hub);
    let _ = build_model_hub_snapshot_and_sync(&host, &state).await;

    Ok(json!({
        "localCleared": true,
        "remoteRevoked": remote_revoked,
        "revokeError": revoke_error,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bob_kernel::app::LocalApp;
    use std::net::TcpListener;

    #[test]
    fn loopback_http_executes_openai_anthropic_and_gemini_protocols_end_to_end() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            for i in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                let response_json = match i {
                    0 => {
                        assert!(req.contains("Authorization: Bearer sk-test-openai"));
                        json!({
                            "model": "gpt-4o-mini-2024-07-18",
                            "choices": [{"message": {"role": "assistant", "content": "OpenAI直连回复正常"}}],
                            "usage": {"prompt_tokens": 12, "completion_tokens": 8}
                        })
                    }
                    1 => {
                        assert!(req.contains("x-api-key: sk-test-anthropic"));
                        json!({
                            "model": "claude-3-5-sonnet-20241022",
                            "content": [{"type": "text", "text": "Anthropic直连回复正常"}],
                            "usage": {"input_tokens": 15, "output_tokens": 9}
                        })
                    }
                    _ => {
                        assert!(req.contains("x-goog-api-key: sk-test-gemini"));
                        json!({
                            "modelVersion": "gemini-2.5-flash",
                            "candidates": [{"content": {"parts": [{"text": "Gemini直连回复正常"}]}}],
                            "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 7}
                        })
                    }
                };
                let body = serde_json::to_string(&response_json).unwrap();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).unwrap();
                stream.flush().unwrap();
                let _ = stream.shutdown(std::net::Shutdown::Write);
            }
        });

        let dir = std::env::temp_dir().join(format!(
            "bob-direct-llm-test-{}-{}",
            std::process::id(),
            now().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = LocalApp::open(&dir.join("kernel.sqlite")).unwrap();
        let session = app.default_session().unwrap();

        let cases = [
            ("openai_compatible", "sk-test-openai", "OpenAI直连回复正常"),
            ("anthropic_messages", "sk-test-anthropic", "Anthropic直连回复正常"),
            ("gemini_generate_content", "sk-test-gemini", "Gemini直连回复正常"),
        ];

        for (idx, (kind, key, expected_reply)) in cases.iter().enumerate() {
            app.upsert_direct_provider_profile(
                &DirectProviderProfileInput {
                    id: "default".into(),
                    provider_kind: (*kind).into(),
                    label: (*kind).into(),
                    base_url: format!("http://{addr}/v1"),
                    model_id: "test-model".into(),
                    api_key_slot: DEFAULT_SLOT.into(),
                    api_key_configured: true,
                    enabled: true,
                },
                100 + idx as i64 * 10,
            )
            .unwrap();

            let req_id = format!("req-{idx}");
            let msg_id = format!("msg-{idx}");
            app.accept(&req_id, &msg_id, &session, format!("测试 {kind}"), 101 + idx as i64 * 10)
                .unwrap();
            let prep = app
                .prepare_direct_execution(&req_id, 102 + idx as i64 * 10)
                .unwrap();
            let spec = build_provider_request(
                &prep.provider_kind,
                &prep.base_url,
                &prep.model_id,
                key,
                &prep.messages,
                256,
            )
            .unwrap();
            let cancel = AtomicBool::new(false);
            let (status, raw_body) =
                send_loopback_http(&spec, &cancel, Duration::from_secs(5)).unwrap();
            let parsed = parse_provider_response(kind, status, &raw_body).unwrap();
            assert_eq!(parsed.text, *expected_reply);
            let exec = app
                .finish_direct_execution(
                    &prep.execution_id,
                    DirectExecutionOutcome::Completed {
                        text: parsed.text,
                        provider_reported_model: parsed.reported_model,
                        input_tokens: parsed.input_tokens,
                        output_tokens: parsed.output_tokens,
                    },
                    103 + idx as i64 * 10,
                )
                .unwrap();
            assert_eq!(exec.status, "completed");
        }

        server.join().unwrap();
        let history = app.history(&session).unwrap();
        assert_eq!(history.len(), 6);
        assert_eq!(history[1].content, "OpenAI直连回复正常");
        assert_eq!(history[3].content, "Anthropic直连回复正常");
        assert_eq!(history[5].content, "Gemini直连回复正常");
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[tauri::command]
pub(crate) async fn app_capture_vision_target<R:Runtime>(host:AppHandle<R>)->Result<Value,&'static str>{
 let dir=host.path().app_data_dir().map_err(|_|"VISION_NOT_CONFIGURED")?;
 let target=bob_runtime::vision::resolve(&load_or_init_registry(&dir),&load_hub_state(&dir))?;
 let _=read_api_key(&host,&provider_slot(&target.provider)).await?;
 serde_json::to_value(target).map_err(|_|"VISION_NOT_CONFIGURED")
}
#[tauri::command]
pub(crate) async fn app_capture_recognize<R:Runtime>(host:AppHandle<R>,state:State<'_,AppState>,media_id:String,model_ref:String)->Result<Value,&'static str>{
 let dir=host.path().app_data_dir().map_err(|_|"VISION_NOT_CONFIGURED")?;
 let target=bob_runtime::vision::resolve(&load_or_init_registry(&dir),&load_hub_state(&dir))?;
 if target.model_ref!=model_ref{return Err("VISION_TARGET_CHANGED");}
 let image=with_app(&state.0,|app|app.capture_media(&media_id).map_err(|_|"CAPTURE_IMAGE_MISSING"))?;
 let key=read_api_key(&host,&provider_slot(&target.provider)).await?;
 let request=bob_runtime::vision::request(&target,&key,&image)?;
 let mut headers=request.headers;
 headers.push(("Content-Type".into(),"application/json".into()));
 let spec=ProviderRequestSpec{url:request.url,headers,body:request.body};
 let (status,raw)=execute_http_spec(Some(&host),spec,Arc::new(AtomicBool::new(false)),Duration::from_secs(60)).await.map_err(|_|"VISION_REQUEST_FAILED")?;
 if !(200..300).contains(&status){return Err("VISION_REQUEST_FAILED");}
 let reply=parse_provider_response(&target.protocol,status,&raw).map_err(|_|"VISION_INVALID_RESPONSE")?;
 Ok(json!({"text":reply.text,"requestedModel":target.model_ref,"reportedModel":reply.reported_model}))
}
