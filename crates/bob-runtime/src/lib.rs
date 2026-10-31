//! Platform-neutral runtime services shared between Tauri (`bob-app`) and the
//! headless/self-hosted node (`bob-node`).
//!
//! Responsibilities (`XP-01`, `XP-02`, `XP-03`):
//! - `SecretStore` trait + `FileVaultSecretStore` (0600 permissions on Unix,
//!   never exporting raw keys to sync bundles or UI snapshots).
//! - `HttpClient` trait + `DefaultHttpClient` (loopback HTTP + HTTPS via curl).
//! - `RuntimeCapabilityManifest` reporting explicit host capabilities.
//! - `ModelHubRuntime` managing `model_providers.json`, `model_hub_state.json`,
//!   `online_cache.json`, BYOK execution, and Online Client API v1 calls.

pub mod vision;
pub mod dsh_session;
use bob_kernel::app::LocalApp;
use bob_kernel::direct_provider::{
    DirectChatTurn, DirectProviderProfile, DirectProviderProfileInput,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_SLOT: &str = "provider-key-default";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const TOOL_KEY_IDS: [&str; 2] = ["TAVILY_API_KEY", "TINYFISH_API_KEY"];
const EMBEDDED_MODEL_REGISTRY: &str =
    include_str!("../../../apps/bob-app/src-tauri/resources/model_providers.json");

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCapabilityManifest {
    pub camera_scan: bool,
    pub local_process_runner: bool,
    pub tls_home_host: bool,
    pub os_secret_store: bool,
    pub browser_mode: bool,
}

impl RuntimeCapabilityManifest {
    pub fn headless_node() -> Self {
        Self {
            camera_scan: false,
            local_process_runner: true,
            tls_home_host: true,
            os_secret_store: true,
            browser_mode: true,
        }
    }
}

pub trait SecretStore: Send + Sync {
    fn get(&self, slot: &str) -> Result<String, &'static str>;
    fn set(&self, slot: &str, secret: &str) -> Result<(), &'static str>;
    fn clear(&self, slot: &str) -> Result<(), &'static str>;
    fn has(&self, slot: &str) -> bool {
        self.get(slot).is_ok_and(|v| !v.trim().is_empty())
    }
}

pub struct FileVaultSecretStore {
    vault_dir: PathBuf,
    master_key: [u8; 32],
}

impl FileVaultSecretStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let vault_dir = data_dir.join("vault");
        std::fs::create_dir_all(&vault_dir).map_err(|_| "SECRET_STORE_UNAVAILABLE")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&vault_dir, std::fs::Permissions::from_mode(0o700));
        }
        let seed_path = vault_dir.join(".vault_seed");
        let seed = if let Ok(bytes) = std::fs::read(&seed_path) {
            bytes
        } else {
            let generated = format!("bob-vault-seed-{}-{}", now_millis(), std::process::id())
                .into_bytes();
            let _ = std::fs::write(&seed_path, &generated);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&seed_path, std::fs::Permissions::from_mode(0o600));
            }
            generated
        };
        let mut hasher = Sha256::new();
        hasher.update(&seed);
        hasher.update(b"bob-runtime-file-vault-v1");
        let digest = hasher.finalize();
        let mut master_key = [0u8; 32];
        master_key.copy_from_slice(&digest[..32]);
        Ok(Self {
            vault_dir,
            master_key,
        })
    }

    fn slot_path(&self, slot: &str) -> PathBuf {
        let safe: String = slot
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.vault_dir.join(format!("{safe}.bin"))
    }

    fn xor_transform(&self, input: &[u8], slot: &str) -> Vec<u8> {
        let mut stream_hasher = Sha256::new();
        stream_hasher.update(self.master_key);
        stream_hasher.update(slot.as_bytes());
        let stream_key = stream_hasher.finalize();
        input
            .iter()
            .enumerate()
            .map(|(idx, b)| b ^ stream_key[idx % stream_key.len()])
            .collect()
    }
}

impl SecretStore for FileVaultSecretStore {
    fn get(&self, slot: &str) -> Result<String, &'static str> {
        let path = self.slot_path(slot);
        let encrypted = std::fs::read(path).map_err(|_| "PROVIDER_KEY_MISSING")?;
        let plain = self.xor_transform(&encrypted, slot);
        let value = String::from_utf8(plain).map_err(|_| "PROVIDER_KEY_CORRUPT")?;
        if value.trim().is_empty() {
            return Err("PROVIDER_KEY_MISSING");
        }
        Ok(value)
    }

    fn set(&self, slot: &str, secret: &str) -> Result<(), &'static str> {
        let trimmed = secret.trim();
        if trimmed.is_empty() {
            return self.clear(slot);
        }
        let path = self.slot_path(slot);
        let encrypted = self.xor_transform(trimmed.as_bytes(), slot);
        std::fs::write(&path, encrypted).map_err(|_| "SECRET_SAVE_FAILED")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn clear(&self, slot: &str) -> Result<(), &'static str> {
        let path = self.slot_path(slot);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("SECRET_CLEAR_FAILED"),
        }
    }
}

pub fn provider_slot(provider_id: &str) -> String {
    format!("provider-key-{}", provider_id.trim())
}

pub fn remote_node_token_slot(node_id: &str) -> String {
    format!("remote-node-token-{}", node_id.trim())
}

pub struct ModelHubRuntime {
    pub data_dir: PathBuf,
    pub secrets: Box<dyn SecretStore>,
}

impl ModelHubRuntime {
    pub fn new(data_dir: PathBuf, secrets: Box<dyn SecretStore>) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        Self { data_dir, secrets }
    }

    pub fn with_file_vault(data_dir: PathBuf) -> Result<Self, &'static str> {
        let secrets = FileVaultSecretStore::open(&data_dir)?;
        Ok(Self::new(data_dir, Box::new(secrets)))
    }

    pub fn load_or_init_registry(&self) -> Value {
        let path = self.data_dir.join("model_providers.json");
        let default_val: Value = serde_json::from_str(EMBEDDED_MODEL_REGISTRY)
            .unwrap_or_else(|_| json!({ "$schema_version": 4, "providers": [] }));
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if val.get("providers").and_then(Value::as_array).is_some() {
                    return val;
                }
            }
        }
        let _ = std::fs::write(&path, EMBEDDED_MODEL_REGISTRY);
        default_val
    }

    pub fn save_registry(&self, registry: &Value) -> Result<(), &'static str> {
        let path = self.data_dir.join("model_providers.json");
        let raw = serde_json::to_string_pretty(registry).map_err(|_| "REGISTRY_SAVE_FAILED")?;
        std::fs::write(path, raw).map_err(|_| "REGISTRY_SAVE_FAILED")
    }

    pub fn load_hub_state(&self) -> Value {
        let path = self.data_dir.join("model_hub_state.json");
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if val.is_object() {
                    return val;
                }
            }
        }
        let def = json!({
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
        });
        let _ = self.save_hub_state(&def);
        def
    }

    pub fn save_hub_state(&self, hub: &Value) -> Result<(), &'static str> {
        let path = self.data_dir.join("model_hub_state.json");
        let raw = serde_json::to_string_pretty(hub).map_err(|_| "HUB_STATE_SAVE_FAILED")?;
        std::fs::write(path, raw).map_err(|_| "HUB_STATE_SAVE_FAILED")
    }

    pub fn build_snapshot_and_sync(&self, app: &mut LocalApp) -> Result<Value, &'static str> {
        let registry = self.load_or_init_registry();
        let mut hub_state = self.load_hub_state();
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
            let configured = self.secrets.has(&slot);
            provider_keys.insert(pid.to_owned(), json!(configured));
        }

        let mut tool_keys = serde_json::Map::new();
        for tid in TOOL_KEY_IDS {
            let slot = provider_slot(tid);
            let configured = self.secrets.has(&slot);
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
            if cid.is_empty() {
                continue;
            }
            let slot = provider_slot(cid);
            let configured = self.secrets.has(&slot);
            provider_keys.insert(cid.to_owned(), json!(configured));
            let model_name = cm
                .get("modelName")
                .and_then(Value::as_str)
                .unwrap_or("custom-model");
            let display_name = cm.get("name").and_then(Value::as_str).unwrap_or(model_name);
            let vision = cm.get("vision").and_then(Value::as_bool).unwrap_or(false);
            pool.push(json!({
                "id": format!("{cid}::{model_name}"),
                "provider": cid,
                "providerName": display_name,
                "modelId": model_name,
                "displayName": display_name,
                "vision": vision,
                "custom": true,
            }));
        }

        let mut remote_nodes = hub_state
            .get("remoteNodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for node in &mut remote_nodes {
            if let Some(obj) = node.as_object_mut() {
                let nid = obj
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let slot = remote_node_token_slot(&nid);
                let token_val = self.secrets.get(&slot).unwrap_or_default();
                obj.remove("token");
                obj.insert("hasToken".to_owned(), json!(!token_val.trim().is_empty()));
            }
        }
        hub_state["remoteNodes"] = json!(remote_nodes);

        let main_role = hub_state
            .get("activeRoles")
            .and_then(|r| r.get("main"))
            .and_then(Value::as_str)
            .unwrap_or("deepseek::deepseek-v4-flash")
            .to_owned();
        let (active_pid, active_mid) = main_role
            .split_once("::")
            .unwrap_or(("deepseek", "deepseek-v4-flash"));

        let variants = hub_state
            .get("providerVariants")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let mut resolved_base_url = "https://api.deepseek.com/v1".to_owned();
        let mut resolved_protocol = "openai_compatible".to_owned();
        let mut resolved_label = active_pid.to_owned();
        let mut resolved_slot = provider_slot(active_pid);
        let mut key_present = false;

        if let Some(cm) = custom_models
            .iter()
            .find(|c| c.get("id").and_then(Value::as_str) == Some(active_pid))
        {
            resolved_base_url = cm
                .get("baseUrl")
                .and_then(Value::as_str)
                .unwrap_or("https://api.openai.com/v1")
                .to_owned();
            resolved_label = cm
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(active_pid)
                .to_owned();
            resolved_protocol =
                infer_provider_protocol(active_pid, &resolved_base_url).to_owned();
            resolved_slot = provider_slot(active_pid);
            key_present = provider_keys
                .get(active_pid)
                .and_then(Value::as_bool)
                .unwrap_or(false);
        } else if let Some(prov) = providers
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(active_pid))
        {
            resolved_base_url = resolve_provider_base_url(prov, &variants);
            resolved_label = prov
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(active_pid)
                .to_owned();
            resolved_protocol =
                infer_provider_protocol(active_pid, &resolved_base_url).to_owned();
            resolved_slot = provider_slot(active_pid);
            key_present = provider_keys
                .get(active_pid)
                .and_then(Value::as_bool)
                .unwrap_or(false);
        }

        if !key_present && self.secrets.has(DEFAULT_SLOT) {
            resolved_slot = DEFAULT_SLOT.to_owned();
            key_present = true;
        }

        let _ = app.upsert_direct_provider_profile(
            &DirectProviderProfileInput {
                id: "default".to_owned(),
                provider_kind: resolved_protocol,
                label: resolved_label,
                base_url: resolved_base_url,
                model_id: active_mid.to_owned(),
                api_key_slot: resolved_slot,
                api_key_configured: key_present,
                enabled: key_present,
            },
            now_millis(),
        );

        let pool_len = pool.len();
        Ok(json!({
            "registry": registry,
            "pool": pool,
            "totalAvailableCount": pool_len,
            "activeRoles": hub_state["activeRoles"],
            "providerKeys": provider_keys,
            "toolKeys": tool_keys,
            "providerVariants": hub_state["providerVariants"],
            "customModels": custom_models,
            "remoteNodes": hub_state["remoteNodes"],
        }))
    }

    pub fn set_provider_key(
        &self,
        app: &mut LocalApp,
        provider_id: &str,
        api_key: Option<&str>,
        clear_key: bool,
    ) -> Result<Value, &'static str> {
        let pid = provider_id.trim();
        if pid.is_empty() {
            return Err("PROVIDER_ID_REQUIRED");
        }
        let slot = provider_slot(pid);
        if clear_key {
            self.secrets.clear(&slot)?;
        } else if let Some(raw) = api_key {
            if raw.trim().is_empty() {
                self.secrets.clear(&slot)?;
            } else {
                self.secrets.set(&slot, raw.trim())?;
            }
        }
        let mut hub = self.load_hub_state();
        let stamp = now_millis();
        if !hub["credentialTimestamps"].is_object() {
            hub["credentialTimestamps"] = json!({});
        }
        hub["credentialTimestamps"][pid] = json!(stamp);
        hub["updatedAt"] = json!(stamp);
        self.save_hub_state(&hub)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn assign_role(
        &self,
        app: &mut LocalApp,
        role: &str,
        model_ref: &str,
    ) -> Result<Value, &'static str> {
        let mut hub = self.load_hub_state();
        if !hub["activeRoles"].is_object() {
            hub["activeRoles"] = json!({});
        }
        hub["activeRoles"][role.trim()] = json!(model_ref.trim());
        hub["updatedAt"] = json!(now_millis());
        self.save_hub_state(&hub)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn set_provider_variant(
        &self,
        app: &mut LocalApp,
        provider_id: &str,
        variant: &str,
    ) -> Result<Value, &'static str> {
        let mut hub = self.load_hub_state();
        if !hub["providerVariants"].is_object() {
            hub["providerVariants"] = json!({});
        }
        hub["providerVariants"][provider_id.trim()] = json!(variant.trim());
        hub["updatedAt"] = json!(now_millis());
        self.save_hub_state(&hub)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn save_custom_model(
        &self,
        app: &mut LocalApp,
        id: Option<String>,
        name: &str,
        model_name: &str,
        base_url: &str,
        api_key: Option<&str>,
        vision: bool,
    ) -> Result<Value, &'static str> {
        let mut hub = self.load_hub_state();
        let cid = id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("custom-{}", now_millis()));
        if let Some(raw) = api_key {
            if !raw.trim().is_empty() {
                self.secrets.set(&provider_slot(&cid), raw.trim())?;
            }
        }
        let mut list = hub
            .get("customModels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let entry = json!({
            "id": cid,
            "name": name.trim(),
            "modelName": model_name.trim(),
            "baseUrl": base_url.trim(),
            "vision": vision
        });
        if let Some(existing) = list
            .iter_mut()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(&cid))
        {
            *existing = entry;
        } else {
            list.push(entry);
        }
        hub["customModels"] = json!(list);
        hub["updatedAt"] = json!(now_millis());
        self.save_hub_state(&hub)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn delete_custom_model(
        &self,
        app: &mut LocalApp,
        id: &str,
    ) -> Result<Value, &'static str> {
        let mut hub = self.load_hub_state();
        let _ = self.secrets.clear(&provider_slot(id));
        let mut list = hub
            .get("customModels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        list.retain(|item| item.get("id").and_then(Value::as_str) != Some(id.trim()));
        hub["customModels"] = json!(list);
        hub["updatedAt"] = json!(now_millis());
        self.save_hub_state(&hub)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn save_remote_node(
        &self,
        app: &mut LocalApp,
        id: Option<String>,
        name: &str,
        url: &str,
        token: Option<&str>,
        clear_token: bool,
        remark: Option<&str>,
        enabled: bool,
    ) -> Result<Value, &'static str> {
        let mut hub = self.load_hub_state();
        let nid = id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("node-{}", now_millis()));
        let slot = remote_node_token_slot(&nid);
        if clear_token {
            let _ = self.secrets.clear(&slot);
        } else if let Some(tok) = token {
            if tok.trim().is_empty() {
                let _ = self.secrets.clear(&slot);
            } else {
                self.secrets.set(&slot, tok.trim())?;
            }
        }
        let mut list = hub
            .get("remoteNodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let entry = json!({
            "id": nid,
            "name": name.trim(),
            "url": url.trim(),
            "remark": remark.unwrap_or("").trim(),
            "enabled": enabled,
            "updatedAt": now_millis()
        });
        if let Some(existing) = list
            .iter_mut()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(&nid))
        {
            *existing = entry;
        } else {
            list.push(entry);
        }
        hub["remoteNodes"] = json!(list);
        hub["updatedAt"] = json!(now_millis());
        self.save_hub_state(&hub)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn delete_remote_node(
        &self,
        app: &mut LocalApp,
        id: &str,
    ) -> Result<Value, &'static str> {
        let mut hub = self.load_hub_state();
        let _ = self.secrets.clear(&remote_node_token_slot(id));
        let mut list = hub
            .get("remoteNodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        list.retain(|item| item.get("id").and_then(Value::as_str) != Some(id.trim()));
        hub["remoteNodes"] = json!(list);
        hub["updatedAt"] = json!(now_millis());
        self.save_hub_state(&hub)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn set_model_visibility(
        &self,
        app: &mut LocalApp,
        provider_id: &str,
        model_id: &str,
        visible: bool,
    ) -> Result<Value, &'static str> {
        let mut registry = self.load_or_init_registry();
        if let Some(providers) = registry.get_mut("providers").and_then(Value::as_array_mut) {
            if let Some(prov) = providers
                .iter_mut()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(provider_id.trim()))
            {
                if let Some(models) = prov.get_mut("models").and_then(Value::as_array_mut) {
                    if let Some(model) = models
                        .iter_mut()
                        .find(|m| m.get("id").and_then(Value::as_str) == Some(model_id.trim()))
                    {
                        model["visible"] = json!(visible);
                    }
                }
            }
        }
        self.save_registry(&registry)?;
        self.build_snapshot_and_sync(app)
    }

    pub fn invoke_direct_model(
        &self,
        profile: &DirectProviderProfile,
        history: &[DirectChatTurn],
        prompt: &str,
    ) -> Result<String, &'static str> {
        let api_key = self.secrets.get(&profile.api_key_slot)?;
        invoke_provider_http(profile, &api_key, history, prompt)
    }
}

pub fn infer_provider_protocol(provider_id: &str, base_url: &str) -> &'static str {
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

pub fn resolve_provider_base_url(provider: &Value, variants: &Value) -> String {
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

pub fn invoke_provider_http(
    profile: &DirectProviderProfile,
    api_key: &str,
    history: &[DirectChatTurn],
    prompt: &str,
) -> Result<String, &'static str> {
    let base = profile.base_url.trim().trim_end_matches('/');
    let endpoint = match profile.provider_kind.as_str() {
        "anthropic_messages" => {
            if base.ends_with("/messages") {
                base.to_owned()
            } else if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        "gemini_generate_content" => {
            if base.contains(":generateContent") {
                base.to_owned()
            } else if base.ends_with("/v1beta") || base.ends_with("/v1") {
                format!("{base}/models/{}:generateContent", profile.model_id)
            } else {
                format!("{base}/v1beta/models/{}:generateContent", profile.model_id)
            }
        }
        _ => {
            if base.ends_with("/chat/completions") {
                base.to_owned()
            } else {
                format!("{base}/chat/completions")
            }
        }
    };

    let body = build_request_body(profile, history, prompt);
    let raw_response = if let Some(rest) = endpoint.strip_prefix("http://") {
        let (host_port, path) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, "/"),
        };
        let addr: SocketAddr = host_port.parse().map_err(|_| "PROVIDER_INVALID_URL")?;
        if !addr.ip().is_loopback() {
            return Err("PROVIDER_INSECURE_HTTP_FORBIDDEN");
        }
        post_loopback_http(addr, host_port, path, &profile.provider_kind, api_key, &body)?
    } else if endpoint.starts_with("https://") {
        post_https_curl(&endpoint, &profile.provider_kind, api_key, &body)?
    } else {
        return Err("PROVIDER_INVALID_URL");
    };

    extract_reply_text(&profile.provider_kind, &raw_response)
}

fn build_request_body(
    profile: &DirectProviderProfile,
    history: &[DirectChatTurn],
    prompt: &str,
) -> String {
    let mut turns: Vec<DirectChatTurn> = history.to_vec();
    if turns.last().map(|t| t.content.as_str()) != Some(prompt) {
        turns.push(DirectChatTurn {
            message_id: "prompt-turn".to_owned(),
            role: "user".to_owned(),
            content: prompt.to_owned(),
        });
    }
    match profile.provider_kind.as_str() {
        "anthropic_messages" => {
            let messages: Vec<Value> = turns
                .into_iter()
                .map(|t| {
                    let role = if t.role == "assistant" {
                        "assistant"
                    } else {
                        "user"
                    };
                    json!({ "role": role, "content": t.content })
                })
                .collect();
            json!({
                "model": profile.model_id,
                "max_tokens": 4096,
                "temperature": 0.3,
                "messages": messages,
            })
            .to_string()
        }
        "gemini_generate_content" => {
            let contents: Vec<Value> = turns
                .into_iter()
                .map(|t| {
                    let role = if t.role == "assistant" {
                        "model"
                    } else {
                        "user"
                    };
                    json!({ "role": role, "parts": [{ "text": t.content }] })
                })
                .collect();
            json!({
                "contents": contents,
                "generationConfig": {
                    "temperature": 0.3,
                    "maxOutputTokens": 4096,
                }
            })
            .to_string()
        }
        _ => {
            let messages: Vec<Value> = turns
                .into_iter()
                .map(|t| json!({ "role": t.role, "content": t.content }))
                .collect();
            json!({
                "model": profile.model_id,
                "messages": messages,
                "temperature": 0.3,
                "max_tokens": 4096,
                "stream": false,
            })
            .to_string()
        }
    }
}

fn post_loopback_http(
    addr: SocketAddr,
    host_header: &str,
    path: &str,
    protocol: &str,
    api_key: &str,
    body: &str,
) -> Result<String, &'static str> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|_| "PROVIDER_UNREACHABLE")?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let auth_header = match protocol {
        "anthropic_messages" => format!("x-api-key: {api_key}\r\nanthropic-version: 2023-06-01"),
        "gemini_generate_content" => format!("x-goog-api-key: {api_key}"),
        _ => format!("Authorization: Bearer {api_key}"),
    };
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_header}\r\nContent-Type: application/json\r\n{auth_header}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|_| "PROVIDER_WRITE_FAILED")?;
    let mut buf = Vec::new();
    let start = Instant::now();
    let mut chunk = [0u8; 4096];
    loop {
        if start.elapsed() > Duration::from_secs(60) {
            return Err("PROVIDER_TIMEOUT");
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > MAX_RESPONSE_BYTES {
                    return Err("PROVIDER_RESPONSE_TOO_LARGE");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(_) => return Err("PROVIDER_READ_FAILED"),
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let (headers, payload) = text
        .split_once("\r\n\r\n")
        .ok_or("PROVIDER_INVALID_RESPONSE")?;
    let status_line = headers.lines().next().unwrap_or("");
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    if status_code == 401 || status_code == 403 {
        return Err("PROVIDER_AUTH_FAILED");
    }
    if status_code == 429 {
        return Err("PROVIDER_RATE_LIMITED");
    }
    if !(200..300).contains(&status_code) {
        return Err("PROVIDER_HTTP_ERROR");
    }
    Ok(payload.to_owned())
}

fn post_https_curl(
    endpoint: &str,
    protocol: &str,
    api_key: &str,
    body: &str,
) -> Result<String, &'static str> {
    let curl_bin = if cfg!(windows) { "curl.exe" } else { "curl" };
    let mut cmd = std::process::Command::new(curl_bin);
    cmd.args([
        "-sS",
        "--max-time",
        "60",
        "-X",
        "POST",
        endpoint,
        "-H",
        "Content-Type: application/json",
    ]);
    match protocol {
        "anthropic_messages" => {
            cmd.args([
                "-H",
                &format!("x-api-key: {api_key}"),
                "-H",
                "anthropic-version: 2023-06-01",
            ]);
        }
        "gemini_generate_content" => {
            cmd.args(["-H", &format!("x-goog-api-key: {api_key}")]);
        }
        _ => {
            cmd.args(["-H", &format!("Authorization: Bearer {api_key}")]);
        }
    }
    cmd.args(["--data-raw", body]);
    let output = cmd.output().map_err(|_| "PROVIDER_UNREACHABLE")?;
    if !output.status.success() {
        return Err("PROVIDER_UNREACHABLE");
    }
    String::from_utf8(output.stdout).map_err(|_| "PROVIDER_INVALID_RESPONSE")
}

fn extract_reply_text(protocol: &str, raw_json: &str) -> Result<String, &'static str> {
    let parsed: Value =
        serde_json::from_str(raw_json).map_err(|_| "PROVIDER_INVALID_RESPONSE")?;
    if parsed.get("error").is_some() {
        return Err("PROVIDER_HTTP_ERROR");
    }
    let reply = match protocol {
        "anthropic_messages" => parsed
            .get("content")
            .and_then(Value::as_array)
            .and_then(|arr| {
                let mut joined = String::new();
                for item in arr {
                    if let Some(t) = item.get("text").and_then(Value::as_str) {
                        joined.push_str(t);
                    }
                }
                if joined.trim().is_empty() {
                    None
                } else {
                    Some(joined)
                }
            }),
        "gemini_generate_content" => parsed
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(Value::as_array)
            .and_then(|parts| {
                let mut joined = String::new();
                for p in parts {
                    if let Some(t) = p.get("text").and_then(Value::as_str) {
                        joined.push_str(t);
                    }
                }
                if joined.trim().is_empty() {
                    None
                } else {
                    Some(joined)
                }
            }),
        _ => parsed
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    let text = reply.ok_or("PROVIDER_EMPTY_REPLY")?;
    if text.trim().is_empty() {
        return Err("PROVIDER_EMPTY_REPLY");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_vault_secret_store_roundtrip_and_redaction() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut app = LocalApp::open(&dir.path().join("kernel.sqlite")).expect("open kernel");
        let runtime = ModelHubRuntime::with_file_vault(dir.path().to_path_buf()).expect("runtime");

        let snap = runtime
            .set_provider_key(&mut app, "deepseek", Some("sk-secret-key-999"), false)
            .expect("set provider key");
        assert_eq!(snap["providerKeys"]["deepseek"], json!(true));
        let dump = serde_json::to_string(&snap).unwrap();
        assert!(
            !dump.contains("sk-secret-key-999"),
            "snapshot must never leak raw secret"
        );

        let snap2 = runtime
            .save_remote_node(
                &mut app,
                Some("node-1".to_owned()),
                "Online VPS",
                "http://127.0.0.1:53020",
                Some("tok-super-secret"),
                false,
                Some("VPS2"),
                true,
            )
            .expect("save remote node");
        let dump2 = serde_json::to_string(&snap2).unwrap();
        assert!(
            !dump2.contains("tok-super-secret"),
            "remote node snapshot must never leak bearer token"
        );
        let nodes = snap2["remoteNodes"].as_array().unwrap();
        assert_eq!(nodes[0]["hasToken"], json!(true));
        assert!(nodes[0].get("token").is_none());

        let snap3 = runtime
            .save_remote_node(
                &mut app,
                Some("node-1".to_owned()),
                "Online VPS",
                "http://127.0.0.1:53020",
                None,
                true,
                Some("VPS2"),
                true,
            )
            .expect("clear remote node token");
        let nodes3 = snap3["remoteNodes"].as_array().unwrap();
        assert_eq!(nodes3[0]["hasToken"], json!(false));
    }
}
