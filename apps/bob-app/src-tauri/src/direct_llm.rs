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

fn load_or_init_registry(data_dir: &std::path::Path) -> Value {
    let path = data_dir.join("model_providers.json");
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if val.get("providers").and_then(Value::as_array).is_some() {
                return val;
            }
        }
    }
    let default_val: Value = serde_json::from_str(EMBEDDED_MODEL_REGISTRY)
        .unwrap_or_else(|_| json!({ "$schema_version": 3, "providers": [] }));
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
        "customModels": []
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

    Ok(json!({
        "registry": registry,
        "pool": pool,
        "totalAvailableCount": pool.len(),
        "activeRoles": hub_state["activeRoles"],
        "providerKeys": provider_keys,
        "toolKeys": tool_keys,
        "providerVariants": hub_state["providerVariants"],
        "customModels": custom_models,
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
    if clear_key.unwrap_or(false) {
        clear_api_key(&host, &slot).await?;
    } else if let Some(ref k) = api_key {
        let trimmed = k.trim();
        if trimmed.is_empty() {
            clear_api_key(&host, &slot).await?;
        } else {
            store_api_key(&host, &slot, trimmed).await?;
        }
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
    let cid = format!("cm-{}", now()?);
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
    }));
    hub["customModels"] = json!(list);
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
