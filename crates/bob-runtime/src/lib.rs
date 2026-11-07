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

pub mod agy_directory;
pub mod node_pairing;
pub mod project_context;
pub mod workspace_context;
pub mod vision;
pub mod dsh_session;
pub mod dsh_transport;
pub mod claude_harness;
pub mod capability_borrowing;
pub mod subagent_pool;
pub mod context_capsule;
pub mod claude_rpc;
pub mod harness_auth;
pub mod runtime_rpc;
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

pub(crate) const DEFAULT_SLOT: &str = "provider-key-default";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
pub(crate) const TOOL_KEY_IDS: [&str; 4] = ["tavily", "tinyfish", "TAVILY_API_KEY", "TINYFISH_API_KEY"];
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

fn generate_entropy_seed(label: &str) -> Vec<u8> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut hasher = Sha256::new();
    hasher.update(b"bob-vault-csprng-v2:");
    hasher.update(label.as_bytes());
    hasher.update(now_millis().to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(seq.to_le_bytes());
    let stack_addr = (&seq as *const u64) as usize;
    hasher.update(stack_addr.to_le_bytes());
    if let Ok(nanos) = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()) {
        hasher.update(nanos.to_le_bytes());
    }
    #[cfg(unix)]
    {
        if let Ok(mut urandom) = std::fs::File::open("/dev/urandom") {
            let mut os_bytes = [0u8; 32];
            if urandom.read_exact(&mut os_bytes).is_ok() {
                hasher.update(os_bytes);
            }
        }
    }
    #[cfg(windows)]
    {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        for round in 0..4u64 {
            let s = RandomState::new();
            let mut h = s.build_hasher();
            h.write_u64(round ^ seq);
            hasher.update(h.finish().to_le_bytes());
        }
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("v2:{hex}").into_bytes()
}

fn derive_master_key(seed: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(seed);
    hasher.update(b"bob-runtime-file-vault-v1");
    let digest = hasher.finalize();
    let mut master_key = [0u8; 32];
    master_key.copy_from_slice(&digest[..32]);
    master_key
}

fn set_secure_file_permissions(path: &Path, is_dir: bool) -> Result<(), &'static str> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if is_dir { 0o700 } else { 0o600 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|_| "SECRET_PERMISSION_SET_FAILED")?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, is_dir);
    }
    Ok(())
}

pub struct FileVaultSecretStore {
    vault_dir: PathBuf,
    master_key: [u8; 32],
}

impl FileVaultSecretStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let vault_dir = data_dir.join("vault");
        std::fs::create_dir_all(&vault_dir).map_err(|_| "SECRET_STORE_UNAVAILABLE")?;
        set_secure_file_permissions(&vault_dir, true)?;
        let seed_path = vault_dir.join(".vault_seed");
        let bak_seed_path = vault_dir.join(".vault_seed.bak");
        if !seed_path.is_file() && bak_seed_path.is_file() {
            std::fs::rename(&bak_seed_path, &seed_path).map_err(|_| "SECRET_STORE_UNAVAILABLE")?;
        }
        let seed = if let Ok(bytes) = std::fs::read(&seed_path) {
            if bytes.is_empty() {
                return Err("SECRET_SEED_CORRUPT");
            }
            bytes
        } else {
            let generated = generate_entropy_seed("master-seed");
            let tmp_seed = vault_dir.join(".vault_seed.tmp");
            std::fs::write(&tmp_seed, &generated).map_err(|_| "SECRET_STORE_UNAVAILABLE")?;
            set_secure_file_permissions(&tmp_seed, false)?;
            std::fs::rename(&tmp_seed, &seed_path).map_err(|_| "SECRET_STORE_UNAVAILABLE")?;
            generated
        };
        let master_key = derive_master_key(&seed);
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

    fn legacy_xor_transform(master_key: &[u8; 32], input: &[u8], slot: &str) -> Vec<u8> {
        let mut stream_hasher = Sha256::new();
        stream_hasher.update(master_key);
        stream_hasher.update(slot.as_bytes());
        let stream_key = stream_hasher.finalize();
        input
            .iter()
            .enumerate()
            .map(|(idx, b)| b ^ stream_key[idx % stream_key.len()])
            .collect()
    }

    fn v2_stream_transform(
        master_key: &[u8; 32],
        slot: &str,
        nonce: &str,
        input: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        for (block_idx, chunk) in input.chunks(32).enumerate() {
            let mut h = Sha256::new();
            h.update(master_key);
            h.update(b":v2-stream:");
            h.update(slot.as_bytes());
            h.update(b":");
            h.update(nonce.as_bytes());
            h.update((block_idx as u64).to_le_bytes());
            let block_key = h.finalize();
            for (i, &b) in chunk.iter().enumerate() {
                out.push(b ^ block_key[i]);
            }
        }
        out
    }

    fn v2_mac_hex(master_key: &[u8; 32], slot: &str, nonce: &str, cipher_hex: &str) -> String {
        let mut h = Sha256::new();
        h.update(master_key);
        h.update(b":v2-mac:");
        h.update(slot.as_bytes());
        h.update(b":");
        h.update(nonce.as_bytes());
        h.update(b":");
        h.update(cipher_hex.as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    fn encode_v2_slot(master_key: &[u8; 32], slot: &str, plaintext: &[u8]) -> Vec<u8> {
        let nonce_seed = generate_entropy_seed(slot);
        let nonce_hex = String::from_utf8_lossy(&nonce_seed)
            .trim_start_matches("v2:")
            .chars()
            .take(32)
            .collect::<String>();
        let cipher = Self::v2_stream_transform(master_key, slot, &nonce_hex, plaintext);
        let cipher_hex: String = cipher.iter().map(|b| format!("{b:02x}")).collect();
        let mac_hex = Self::v2_mac_hex(master_key, slot, &nonce_hex, &cipher_hex);
        format!("v2:{nonce_hex}:{mac_hex}:{cipher_hex}").into_bytes()
    }

    fn decode_slot_bytes(
        master_key: &[u8; 32],
        slot: &str,
        encrypted: &[u8],
    ) -> Result<String, &'static str> {
        if encrypted.starts_with(b"v2:") {
            let text = std::str::from_utf8(encrypted).map_err(|_| "PROVIDER_KEY_CORRUPT")?;
            let parts: Vec<&str> = text.split(':').collect();
            if parts.len() != 4 || parts[0] != "v2" || parts[1].is_empty() || parts[3].is_empty() {
                return Err("PROVIDER_KEY_CORRUPT");
            }
            let nonce_hex = parts[1];
            let mac_hex = parts[2];
            let cipher_hex = parts[3];
            let expected_mac = Self::v2_mac_hex(master_key, slot, nonce_hex, cipher_hex);
            if mac_hex != expected_mac || cipher_hex.len() % 2 != 0 {
                return Err("PROVIDER_KEY_CORRUPT");
            }
            let mut cipher_bytes = Vec::with_capacity(cipher_hex.len() / 2);
            for idx in (0..cipher_hex.len()).step_by(2) {
                let b = u8::from_str_radix(&cipher_hex[idx..idx + 2], 16)
                    .map_err(|_| "PROVIDER_KEY_CORRUPT")?;
                cipher_bytes.push(b);
            }
            let plain = Self::v2_stream_transform(master_key, slot, nonce_hex, &cipher_bytes);
            let val = String::from_utf8(plain).map_err(|_| "PROVIDER_KEY_CORRUPT")?;
            if val.trim().is_empty() {
                return Err("PROVIDER_KEY_MISSING");
            }
            return Ok(val);
        }
        // Legacy v1 compatibility path: existing slots remain 100% readable
        let plain = Self::legacy_xor_transform(master_key, encrypted, slot);
        let value = String::from_utf8(plain).map_err(|_| "PROVIDER_KEY_CORRUPT")?;
        if value.trim().is_empty() || value.chars().any(|c| c == '\0') {
            return Err("PROVIDER_KEY_CORRUPT");
        }
        Ok(value)
    }

    /// Atomically rotate `.vault_seed` and re-encrypt all existing slots without losing any key.
    /// If any slot is corrupt before rotation, aborts cleanly without modifying the vault.
    pub fn rotate_master_seed(&mut self) -> Result<usize, &'static str> {
        let mut decrypted_slots: Vec<(String, PathBuf, String)> = Vec::new();
        for entry in std::fs::read_dir(&self.vault_dir).map_err(|_| "SECRET_STORE_UNAVAILABLE")? {
            let entry = entry.map_err(|_| "SECRET_STORE_UNAVAILABLE")?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("bin") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("VAULT_ROTATION_ABORTED_CORRUPT_SLOT")?
                .to_owned();
            let raw = std::fs::read(&path).map_err(|_| "VAULT_ROTATION_ABORTED_CORRUPT_SLOT")?;
            let plain = Self::decode_slot_bytes(&self.master_key, &stem, &raw)
                .map_err(|_| "VAULT_ROTATION_ABORTED_CORRUPT_SLOT")?;
            decrypted_slots.push((stem, path, plain));
        }

        let seed_path = self.vault_dir.join(".vault_seed");
        let bak_seed_path = self.vault_dir.join(".vault_seed.bak");
        std::fs::copy(&seed_path, &bak_seed_path).map_err(|_| "VAULT_ROTATION_BACKUP_FAILED")?;

        let new_seed = generate_entropy_seed("rotated-master-seed");
        let new_master_key = derive_master_key(&new_seed);

        for (slot, path, plain) in &decrypted_slots {
            let bak_slot = path.with_extension("bin.bak");
            let _ = std::fs::copy(path, &bak_slot);
            let encoded = Self::encode_v2_slot(&new_master_key, slot, plain.as_bytes());
            let tmp_slot = path.with_extension("bin.tmp");
            if std::fs::write(&tmp_slot, &encoded).is_err()
                || set_secure_file_permissions(&tmp_slot, false).is_err()
                || std::fs::rename(&tmp_slot, path).is_err()
            {
                let _ = std::fs::copy(&bak_seed_path, &seed_path);
                return Err("VAULT_ROTATION_WRITE_FAILED");
            }
        }

        let tmp_seed = self.vault_dir.join(".vault_seed.tmp");
        std::fs::write(&tmp_seed, &new_seed).map_err(|_| "VAULT_ROTATION_WRITE_FAILED")?;
        set_secure_file_permissions(&tmp_seed, false)?;
        std::fs::rename(&tmp_seed, &seed_path).map_err(|_| "VAULT_ROTATION_WRITE_FAILED")?;
        self.master_key = new_master_key;

        for (_, path, _) in &decrypted_slots {
            let _ = std::fs::remove_file(path.with_extension("bin.bak"));
        }
        let _ = std::fs::remove_file(&bak_seed_path);
        Ok(decrypted_slots.len())
    }
}

impl SecretStore for FileVaultSecretStore {
    fn get(&self, slot: &str) -> Result<String, &'static str> {
        let path = self.slot_path(slot);
        let encrypted = std::fs::read(path).map_err(|_| "PROVIDER_KEY_MISSING")?;
        Self::decode_slot_bytes(&self.master_key, slot, &encrypted)
    }

    fn set(&self, slot: &str, secret: &str) -> Result<(), &'static str> {
        let trimmed = secret.trim();
        if trimmed.is_empty() {
            return self.clear(slot);
        }
        let path = self.slot_path(slot);
        let safe_slot = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(slot);
        let encoded = Self::encode_v2_slot(&self.master_key, safe_slot, trimmed.as_bytes());
        let tmp_path = path.with_extension("bin.tmp");
        std::fs::write(&tmp_path, encoded).map_err(|_| "SECRET_SAVE_FAILED")?;
        set_secure_file_permissions(&tmp_path, false)?;
        std::fs::rename(&tmp_path, &path).map_err(|_| "SECRET_SAVE_FAILED")?;
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
                "main": "deepseek::deepseek-v4.1-flash-1017",
                "flash": "deepseek::deepseek-v4.1-flash-1017"
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
        for (short_id, env_id) in [("tavily", "TAVILY_API_KEY"), ("tinyfish", "TINYFISH_API_KEY")] {
            let configured = self.secrets.has(&provider_slot(short_id))
                || self.secrets.has(&provider_slot(env_id));
            tool_keys.insert(short_id.to_owned(), json!(configured));
            tool_keys.insert(env_id.to_owned(), json!(configured));
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

        if !hub_state["activeRoles"].is_object() {
            hub_state["activeRoles"] = json!({});
        }
        let main_empty = hub_state
            .get("activeRoles")
            .and_then(|r| r.get("main"))
            .and_then(Value::as_str)
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
        if main_empty {
            hub_state["activeRoles"]["main"] = json!("deepseek::deepseek-v4.1-flash-1017");
        }

        let flash_empty = hub_state
            .get("activeRoles")
            .and_then(|r| r.get("flash"))
            .and_then(Value::as_str)
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
        if flash_empty {
            let clerk_val = hub_state["activeRoles"]
                .get("clerk")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if !clerk_val.is_empty() {
                hub_state["activeRoles"]["flash"] = json!(clerk_val);
            } else {
                hub_state["activeRoles"]["flash"] = json!("deepseek::deepseek-v4.1-flash-1017");
            }
        }

        let main_role = hub_state
            .get("activeRoles")
            .and_then(|r| r.get("main"))
            .and_then(Value::as_str)
            .unwrap_or("deepseek::deepseek-v4.1-flash-1017")
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
        let subscription_quotas = subscription_quota::read(&self.data_dir);
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
            "subscriptionQuotas": subscription_quotas,
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
        let alias_slot = match pid {
            "tavily" => Some(provider_slot("TAVILY_API_KEY")),
            "TAVILY_API_KEY" => Some(provider_slot("tavily")),
            "tinyfish" => Some(provider_slot("TINYFISH_API_KEY")),
            "TINYFISH_API_KEY" => Some(provider_slot("tinyfish")),
            _ => None,
        };
        if clear_key {
            self.secrets.clear(&slot)?;
            if let Some(ref a) = alias_slot {
                let _ = self.secrets.clear(a);
            }
        } else if let Some(raw) = api_key {
            if raw.trim().is_empty() {
                self.secrets.clear(&slot)?;
                if let Some(ref a) = alias_slot {
                    let _ = self.secrets.clear(a);
                }
            } else {
                self.secrets.set(&slot, raw.trim())?;
                if let Some(ref a) = alias_slot {
                    let _ = self.secrets.set(a, raw.trim());
                }
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
        let r = match role.trim() {
            "clerk" => "flash",
            other => other,
        };
        hub["activeRoles"][r] = json!(model_ref.trim());
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

    pub fn dsh_client(&self, id: &str) -> Result<dsh_transport::DshClient, &'static str> {
        let hub=self.load_hub_state();
        let node=hub["remoteNodes"].as_array().and_then(|items|items.iter().find(|n|n["id"]==id && n["enabled"]==true)).ok_or("DSH_CONNECTION_NOT_FOUND")?;
        let url=node["url"].as_str().ok_or("DSH_ADDRESS_INVALID")?;
        // Optional operator-only candidate reference: one connection, no credential copying.
        let token=if std::env::var("BOB_DSH_CREDENTIAL_CONNECTION").ok().as_deref()==Some(id){
            let source=std::env::var_os("BOB_DSH_CREDENTIAL_SOURCE").ok_or("SECRET_STORE_UNAVAILABLE")?;
            let vault_dir=PathBuf::from(source).join("vault");
            let seed=std::fs::read(vault_dir.join(".vault_seed")).map_err(|_|"SECRET_STORE_UNAVAILABLE")?;
            let mut hasher=Sha256::new();hasher.update(seed);hasher.update(b"bob-runtime-file-vault-v1");
            let master_key:[u8;32]=hasher.finalize().into();
            FileVaultSecretStore{vault_dir,master_key}.get(&remote_node_token_slot(id)).ok()
        }else{self.secrets.get(&remote_node_token_slot(id)).ok()};
        let log_source=node["logPath"].as_str().map(PathBuf::from).or_else(||std::env::var_os("BOB_DSH_LOG_PATH").map(PathBuf::from));
        if token.is_none() && log_source.is_none() && dsh_transport::load_cached_cookie(&self.data_dir, url).is_none() {
            return Err("SECRET_NOT_CONFIGURED");
        }
        dsh_transport::DshClient::connect_or_recover(url, token.as_deref(), &self.data_dir, log_source.as_deref())
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
        self.save_remote_node_for_device(app, id, name, url, token, clear_token, remark, enabled, None)
    }

    pub fn save_remote_node_for_device(
        &self,
        app: &mut LocalApp,
        id: Option<String>,
        name: &str,
        url: &str,
        token: Option<&str>,
        clear_token: bool,
        remark: Option<&str>,
        enabled: bool,
        device_id: Option<&str>,
    ) -> Result<Value, &'static str> {
        let mut hub = self.load_hub_state();
        let old_nodes = hub.get("remoteNodes").and_then(Value::as_array);
        let same_address = old_nodes.and_then(|nodes| nodes.iter().find(|n| {
            n.get("baseUrl").or_else(|| n.get("url")).and_then(Value::as_str)
                .is_some_and(|u| u.trim().trim_end_matches('/') == url.trim().trim_end_matches('/'))
        })).and_then(|n| n["id"].as_str()).map(str::to_owned);
        let nid = id.filter(|s| !s.trim().is_empty()).or(same_address)
            .unwrap_or_else(|| format!("node-{}", now_millis()));
        let existing_device = old_nodes.and_then(|nodes| nodes.iter().find(|n| n["id"].as_str() == Some(&nid)))
            .and_then(|n| n["deviceId"].as_str());
        let resolved_device = device_id.filter(|s| !s.trim().is_empty())
            .or(existing_device).unwrap_or(&nid).trim().to_owned();
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
        let mut entry = list.iter().find(|n| n["id"].as_str() == Some(&nid))
            .cloned().unwrap_or_else(|| json!({}));
        let saved_remark = remark.map(str::to_owned)
            .or_else(|| entry["remark"].as_str().map(str::to_owned)).unwrap_or_default();
        let update = json!({
            "id": nid,
            "deviceId": resolved_device,
            "name": name.trim(),
            "label": name.trim(),
            "url": url.trim(),
            "baseUrl": url.trim(),
            "remark": saved_remark.trim(),
            "enabled": enabled,
            "updatedAt": now_millis()
        });
        let fields = entry.as_object_mut().ok_or("INVALID_REMOTE_NODE")?;
        fields.remove("token");
        fields.remove("accessToken");
        fields.extend(update.as_object().unwrap().clone());
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

    pub fn resolve_profile_for_model_ref(
        &self,
        model_ref: &str,
    ) -> Result<DirectProviderProfile, &'static str> {
        let (pid, mid) = model_ref.split_once("::").ok_or("INVALID_MODEL_REF")?;
        let registry = self.load_or_init_registry();
        let hub_state = self.load_hub_state();
        let providers = registry
            .get("providers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let custom_models = hub_state
            .get("customModels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let variants = hub_state
            .get("providerVariants")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let mut resolved_base_url = "https://api.deepseek.com/v1".to_owned();
        let mut resolved_protocol = "openai_compatible".to_owned();
        let mut resolved_label = pid.to_owned();
        let mut resolved_slot = provider_slot(pid);
        let mut key_present = false;

        if let Some(cm) = custom_models
            .iter()
            .find(|c| c.get("id").and_then(Value::as_str) == Some(pid))
        {
            resolved_base_url = cm
                .get("baseUrl")
                .and_then(Value::as_str)
                .unwrap_or("https://api.openai.com/v1")
                .to_owned();
            resolved_label = cm
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(pid)
                .to_owned();
            resolved_protocol =
                infer_provider_protocol(pid, &resolved_base_url).to_owned();
            resolved_slot = provider_slot(pid);
            key_present = self.secrets.has(&resolved_slot);
        } else if let Some(prov) = providers
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
        {
            resolved_base_url = resolve_provider_base_url(prov, &variants);
            resolved_label = prov
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(pid)
                .to_owned();
            resolved_protocol =
                infer_provider_protocol(pid, &resolved_base_url).to_owned();
            resolved_slot = provider_slot(pid);
            key_present = self.secrets.has(&resolved_slot);
        }

        if !key_present && self.secrets.has(DEFAULT_SLOT) {
            resolved_slot = DEFAULT_SLOT.to_owned();
            key_present = true;
        }

        if !key_present {
            return Err("PROVIDER_KEY_NOT_CONFIGURED");
        }

        Ok(DirectProviderProfile {
            id: format!("role-{pid}-{mid}"),
            provider_kind: resolved_protocol,
            label: resolved_label,
            base_url: resolved_base_url,
            model_id: mid.to_owned(),
            api_key_slot: resolved_slot,
            api_key_configured: true,
            enabled: true,
            last_check_status: "ok".to_owned(),
            last_check_reason: String::new(),
            last_checked_at: Some(now_millis()),
            updated_at: now_millis(),
        })
    }

    pub fn resolve_role_profile(&self, role: &str) -> Result<DirectProviderProfile, &'static str> {
        let hub = self.load_hub_state();
        let target_role = if role == "clerk" { "flash" } else { role };
        let mut model_ref = hub["activeRoles"]
            .get(target_role)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if model_ref.is_empty() && target_role == "flash" {
            model_ref = hub["activeRoles"]
                .get("clerk")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
        }
        if model_ref.is_empty() {
            model_ref = hub["activeRoles"]
                .get("main")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
        }
        if model_ref.is_empty() {
            model_ref = if target_role == "flash" {
                "deepseek::deepseek-v4.1-flash-1017"
            } else {
                "deepseek::deepseek-v4.1-flash-1017"
            };
        }
        self.resolve_profile_for_model_ref(model_ref)
    }

    pub fn try_load_knowledge_feed() -> Option<Value> {
        let mut candidates = Vec::new();
        if let Ok(dir) = std::env::var("BOB_CLOUD_SYNC_DIR") {
            candidates.push(PathBuf::from(&dir).join("registry").join("models.json"));
            candidates.push(PathBuf::from(&dir).join("models.json"));
            candidates.push(PathBuf::from(&dir).join("registry").join("unified_model_registry.json"));
        }
        if let Ok(ws) = std::env::var("GEMINI_WORKSPACE") {
            candidates.push(PathBuf::from(&ws).join("cloud_sync").join("registry").join("models.json"));
            candidates.push(PathBuf::from(&ws).join("cloud_sync").join("registry").join("unified_model_registry.json"));
        }
        if let Ok(home) = std::env::var("HOME") {
            let ref_dir = PathBuf::from(&home)
                .join("common")
                .join("knowledge")
                .join("skills")
                .join("model-registry")
                .join("references");
            candidates.push(ref_dir.join("models.json"));
            candidates.push(ref_dir.join("unified_model_registry.json"));
        }
        candidates.push(PathBuf::from("cloud_sync/registry/models.json"));
        candidates.push(PathBuf::from("../cloud_sync/registry/models.json"));
        candidates.push(PathBuf::from("../../cloud_sync/registry/models.json"));
        candidates.push(PathBuf::from("cloud_sync/registry/unified_model_registry.json"));

        for p in candidates {
            if p.is_file() {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                        return Some(val);
                    }
                }
            }
        }
        None
    }

    pub fn get_tool_secret(&self, short_id: &str, env_id: &str) -> Option<String> {
        self.secrets
            .get(&provider_slot(short_id))
            .or_else(|_| self.secrets.get(&provider_slot(env_id)))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    pub fn discover_and_curate_models(
        &self,
        app: &mut LocalApp,
        target_provider_id: Option<&str>,
    ) -> Result<Value, &'static str> {
        let mut registry = self.load_or_init_registry();
        let mut hub = self.load_hub_state();
        let variants = hub
            .get("providerVariants")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let feed_opt = Self::try_load_knowledge_feed();
        let tavily_key_opt = self.get_tool_secret("tavily", "TAVILY_API_KEY");
        let judge_profile_opt = self
            .resolve_role_profile("main")
            .or_else(|_| self.resolve_role_profile("flash"))
            .ok();

        let mut any_updated = false;
        let mut hub_updated = false;
        let mut targets = Vec::new();
        if let Some(providers) = registry.get("providers").and_then(Value::as_array) {
            for p in providers {
                let pid = p.get("id").and_then(Value::as_str).unwrap_or("");
                if pid.is_empty() || pid == "offline" || pid == "vertex_ai" {
                    continue;
                }
                if let Some(target) = target_provider_id {
                    if pid != target {
                        continue;
                    }
                }
                let pname = p.get("name").and_then(Value::as_str).unwrap_or(pid).to_owned();
                let slot = provider_slot(pid);
                let api_key = self.secrets.get(&slot).unwrap_or_default().trim().to_owned();
                let base_url = resolve_provider_base_url(p, &variants);
                let existing_ids: Vec<String> = p
                    .get("models")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                targets.push((pid.to_owned(), pname, base_url, api_key, existing_ids));
            }
        }

        for (pid, pname, base_url, api_key, existing_ids) in targets {
            let mut candidate_ids: Vec<String> = Vec::new();

            // 1. Live /models probe if provider API key is configured
            if !api_key.is_empty() && !base_url.is_empty() {
                let base_trimmed = base_url.trim_end_matches('/');
                let auth_header = format!("Bearer {api_key}");
                let (models_url, headers) = if pid == "gemini"
                    || (base_trimmed.contains("generativelanguage.googleapis.com")
                        && !base_trimmed.contains("/openai"))
                {
                    (
                        format!("{base_trimmed}/models?key={api_key}"),
                        vec![("Accept", "application/json")],
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
                            ("x-api-key", api_key.as_str()),
                            ("anthropic-version", "2023-06-01"),
                            ("Accept", "application/json"),
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
                            ("Authorization", auth_header.as_str()),
                            ("Accept", "application/json"),
                        ],
                    )
                };

                if let Ok(body) = get_https_curl(&models_url, &headers) {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&body) {
                        let items = parsed
                            .get("data")
                            .and_then(Value::as_array)
                            .or_else(|| parsed.get("models").and_then(Value::as_array));
                        if let Some(remote_list) = items {
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
                                let clean = if pid == "gemini" {
                                    raw_id.strip_prefix("models/").unwrap_or(raw_id)
                                } else {
                                    raw_id
                                };
                                if !candidate_ids.iter().any(|x| x == clean) {
                                    candidate_ids.push(clean.to_owned());
                                }
                            }
                        }
                    }
                }
            }

            // 2. Merge with existing catalog models & optional knowledge feed so dated snapshots (like -1017) are preserved
            for eid in &existing_ids {
                if is_chat_model_id(eid) && !candidate_ids.iter().any(|x| x == eid) {
                    candidate_ids.push(eid.clone());
                }
            }
            if let Some(ref feed) = feed_opt {
                let feed_key = if pid == "gemini" { "google" } else { pid.as_str() };
                if let Some(feed_models) = feed.get(feed_key).and_then(|fp| fp.get("models")).and_then(Value::as_object) {
                    for mid in feed_models.keys() {
                        if !candidate_ids.iter().any(|x| x == mid) {
                            candidate_ids.push(mid.clone());
                        }
                    }
                }
            }

            if candidate_ids.is_empty() {
                continue;
            }

            // Only run live LLM + Tavily Scout Skill for providers the user has configured (or explicitly targeted)
            let should_scout = !api_key.is_empty() || target_provider_id == Some(pid.as_str());
            if !should_scout {
                continue;
            }

            let mut chosen_main: Option<String> = None;
            let mut chosen_flash: Option<String> = None;

            // For DeepSeek: Official benchmark certification proves v4.1 flash 1017 outperforms v4 pro, so only 1 model is needed
            if pid == "deepseek" {
                let best = candidate_ids
                    .iter()
                    .find(|id| id.contains("4.1") && id.contains("1017"))
                    .cloned()
                    .or_else(|| candidate_ids.iter().find(|id| id.contains("4.1")).cloned())
                    .unwrap_or_else(|| "deepseek-v4.1-flash-1017".to_string());
                chosen_main = Some(best.clone());
                chosen_flash = Some(best);
            }

            // 3. Built-in Model Scout Skill: Query Tavily Web Search if key is available
            let search_context = if pid != "deepseek" && tavily_key_opt.is_some() {
                query_tavily_search(
                    tavily_key_opt.as_ref().unwrap(),
                    &format!("{pname} ({pid}) latest LLM API model release benchmark ranking 2026"),
                )
                .unwrap_or_default()
            } else {
                String::new()
            };

            if pid != "deepseek" && judge_profile_opt.is_some() {
                let prof = judge_profile_opt.as_ref().unwrap();
                let prompt = format!(
                    "You are the built-in `model-scout` skill for an AI Agent workbench.\nProvider: {pname} ({pid})\nCandidate Model IDs: {:?}\nLatest Web Search Intelligence:\n{}\n\nSelection Rules:\n1. Pick 'main': The single newest, highest-capability flagship/pro/reasoning model for complex coding and architecture.\n2. Pick 'flash': The single newest, fastest high-efficiency flash/mini/haiku model for daily tasks and routing.\n3. CRITICAL VERSION & SNAPSHOT RULE: Higher version numbers (e.g. v4.1 > v4.0, 3.8 > 3.5, 2.5 > 2.0) and newer date-suffixed snapshots (e.g. `deepseek-v4.1-flash-1017` > `deepseek-v4.1-flash` > `deepseek-flash`; `260215` > `250828`) ALWAYS win over older or generic undated aliases.\n4. Both selected IDs MUST come from the Candidate Model IDs list above and MUST be distinct if at least 2 models exist.\nReturn ONLY raw JSON (no markdown fences):\n{{\"main\": {{\"id\": \"...\", \"vision\": true}}, \"flash\": {{\"id\": \"...\", \"vision\": true}}}}",
                    &candidate_ids[..candidate_ids.len().min(80)],
                    if search_context.is_empty() { "(No external search configured; apply strict version & date-suffix rules)" } else { &search_context }
                );
                if let Ok(reply) = self.invoke_direct_model(prof, &[], &prompt) {
                    let trimmed = reply.trim();
                    let clean_json = if let Some(s) = trimmed.strip_prefix("```json") {
                        s.strip_suffix("```").unwrap_or(s).trim()
                    } else if let Some(s) = trimmed.strip_prefix("```") {
                        s.strip_suffix("```").unwrap_or(s).trim()
                    } else {
                        trimmed
                    };
                    if let Ok(v) = serde_json::from_str::<Value>(clean_json) {
                        chosen_main = v["main"]["id"]
                            .as_str()
                            .map(str::to_owned)
                            .filter(|id| candidate_ids.iter().any(|c| c == id));
                        chosen_flash = v["flash"]["id"]
                            .as_str()
                            .map(str::to_owned)
                            .filter(|id| candidate_ids.iter().any(|c| c == id));
                    }
                }
            }

            // Deterministic version-aware fallback heuristic
            if chosen_flash.is_none() {
                chosen_flash = pick_best_tier_model(&candidate_ids, true, None);
            }
            if chosen_main.is_none() {
                chosen_main = pick_best_tier_model(&candidate_ids, false, chosen_flash.as_deref());
            }

            if chosen_main.is_none() && chosen_flash.is_none() {
                continue;
            }

            if let Some(providers) = registry.get_mut("providers").and_then(Value::as_array_mut) {
                if let Some(prov) = providers.iter_mut().find(|p| p.get("id").and_then(Value::as_str) == Some(&pid)) {
                    if let Some(models) = prov.get_mut("models").and_then(Value::as_array_mut) {
                        // Ensure chosen_main and chosen_flash exist in `models`
                        for (role_id_opt, is_default) in [(&chosen_main, true), (&chosen_flash, false)] {
                            if let Some(sel_id) = role_id_opt {
                                if !models.iter().any(|m| m.get("id").and_then(Value::as_str) == Some(sel_id.as_str())) {
                                    models.insert(0, json!({
                                        "id": sel_id,
                                        "name": sel_id,
                                        "visible": true,
                                        "vision": true,
                                        "discovered": true,
                                        "default": is_default,
                                        "pricing": { "input": 0.0, "output": 0.0 }
                                    }));
                                }
                            }
                        }

                        for m in models.iter_mut() {
                            let mid = m.get("id").and_then(Value::as_str).unwrap_or("");
                            let is_curated = chosen_main.as_deref() == Some(mid)
                                || chosen_flash.as_deref() == Some(mid);
                            m["visible"] = json!(is_curated);
                            if is_curated {
                                m["vision"] = json!(true);
                            }
                        }
                        any_updated = true;
                    }
                }
            }

            // If activeRoles currently uses this provider, upgrade to the newly scouted main/flash
            if hub["activeRoles"].is_object() {
                let current_main = hub["activeRoles"].get("main").and_then(Value::as_str).unwrap_or("");
                if current_main.starts_with(&format!("{pid}::")) {
                    if let Some(ref cm) = chosen_main {
                        hub["activeRoles"]["main"] = json!(format!("{pid}::{cm}"));
                        hub_updated = true;
                    }
                }
                let current_flash = hub["activeRoles"].get("flash").and_then(Value::as_str).unwrap_or("");
                if current_flash.starts_with(&format!("{pid}::")) {
                    if let Some(ref cf) = chosen_flash {
                        hub["activeRoles"]["flash"] = json!(format!("{pid}::{cf}"));
                        hub_updated = true;
                    }
                }
            }
        }

        if any_updated {
            let _ = self.save_registry(&registry);
        }
        if hub_updated {
            let _ = self.save_hub_state(&hub);
        }
        self.build_snapshot_and_sync(app)
    }
}

fn pick_best_tier_model(candidates: &[String], want_flash: bool, exclude: Option<&str>) -> Option<String> {
    let mut best: Option<(i64, String)> = None;
    for id in candidates {
        if Some(id.as_str()) == exclude {
            continue;
        }
        let l = id.to_ascii_lowercase();
        let is_flash_like = l.contains("flash") || l.contains("mini") || l.contains("haiku") || l.contains("lite");
        let is_main_like = l.contains("pro") || l.contains("max") || l.contains("sonnet") || l.contains("opus") || l.contains("plus") || l == "gpt-4o";
        let mut score: i64 = 0;
        if want_flash {
            if is_flash_like { score += 1000; }
        } else if is_main_like && !is_flash_like {
            score += 1000;
        }
        // Prefer higher version numbers & date snapshots (e.g. v4.1, -1017, -260215)
        if l.contains("4.1") || l.contains("4-5") || l.contains("3.8") || l.contains("5.3") || l.contains("2.5") || l.contains("2-0") {
            score += 300;
        }
        if l.contains("1017") || l.contains("2602") || l.contains("2508") || l.contains("2025") || l.contains("2026") {
            score += 200;
        }
        if l.contains("lite") && want_flash {
            score -= 50; // Prefer standard flash over flash-lite
        }
        if best.as_ref().map_or(true, |(b_score, _)| score > *b_score) {
            best = Some((score, id.clone()));
        }
    }
    best.map(|(_, id)| id)
}

fn query_tavily_search(api_key: &str, query: &str) -> Option<String> {
    let body = json!({
        "api_key": api_key,
        "query": query,
        "search_depth": "basic",
        "include_answer": true,
        "max_results": 4
    })
    .to_string();
    let raw = post_https_curl("https://api.tavily.com/search", "openai_compatible", api_key, &body).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    let mut summary = String::new();
    if let Some(ans) = val.get("answer").and_then(Value::as_str) {
        if !ans.trim().is_empty() {
            summary.push_str(ans.trim());
            summary.push('\n');
        }
    }
    if let Some(results) = val.get("results").and_then(Value::as_array) {
        for r in results.iter().take(4) {
            let title = r.get("title").and_then(Value::as_str).unwrap_or("");
            let content = r.get("content").and_then(Value::as_str).unwrap_or("");
            if !title.is_empty() || !content.is_empty() {
                summary.push_str(&format!("- {title}: {content}\n"));
            }
        }
    }
    if summary.trim().is_empty() {
        None
    } else {
        Some(summary)
    }
}

pub fn is_chat_model_id(raw_id: &str) -> bool {
    let lower = raw_id.to_ascii_lowercase();
    !lower.contains("embedding")
        && !lower.contains("tts")
        && !lower.contains("whisper")
        && !lower.contains("audio")
        && !lower.contains("dall-e")
        && !lower.contains("realtime")
        && !lower.contains("moderation")
}

fn get_https_curl(
    url: &str,
    headers: &[(&str, &str)],
) -> Result<String, &'static str> {
    let curl_bin = if cfg!(windows) { "curl.exe" } else { "curl" };
    let mut cmd = std::process::Command::new(curl_bin);
    cmd.args(["-sS", "--max-time", "15", "-X", "GET", url]);
    for (k, v) in headers {
        cmd.args(["-H", &format!("{k}: {v}")]);
    }
    let output = cmd.output().map_err(|_| "PROVIDER_UNREACHABLE")?;
    if !output.status.success() {
        return Err("PROVIDER_UNREACHABLE");
    }
    String::from_utf8(output.stdout).map_err(|_| "PROVIDER_INVALID_RESPONSE")
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
            let clean_mid = profile.model_id.strip_prefix("models/").unwrap_or(&profile.model_id);
            if base.contains(":generateContent") {
                base.to_owned()
            } else if base.ends_with("/v1beta") || base.ends_with("/v1") {
                format!("{base}/models/{clean_mid}:generateContent")
            } else {
                format!("{base}/v1beta/models/{clean_mid}:generateContent")
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
            let wire_model = if profile.base_url.contains("api.deepseek.com") {
                if profile.model_id.contains("reasoner") || profile.model_id.contains("-r1") || profile.model_id.contains("-r2") {
                    "deepseek-reasoner"
                } else if profile.model_id.starts_with("deepseek-") {
                    "deepseek-chat"
                } else {
                    profile.model_id.as_str()
                }
            } else if profile.base_url.contains("dashscope.aliyuncs.com") && profile.model_id.starts_with("qwen3.") {
                if profile.model_id.contains("flash") {
                    "qwen-plus"
                } else {
                    "qwen-max"
                }
            } else {
                profile.model_id.as_str()
            };
            json!({
                "model": wire_model,
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
    cmd.args(["--data-binary", "@-"]);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().map_err(|_| "PROVIDER_UNREACHABLE")?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(body.as_bytes());
    }
    let output = child.wait_with_output().map_err(|_| "PROVIDER_UNREACHABLE")?;
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
    fn subscription_quotas_do_not_fabricate_data() {
        let quotas = probe_subscription_quotas_cached();
        for provider in ["codex", "antigravity", "claude"] {
            assert_eq!(quotas[provider]["status"], "unavailable");
            assert!(quotas[provider].get("remaining_weekly_pct").is_none());
            assert!(quotas[provider].get("reset_time_pct").is_none());
        }
    }

    #[test]
    fn device_paths_preserve_tokens_ids_and_membership() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = LocalApp::open(&dir.path().join("kernel.sqlite")).unwrap();
        let runtime = ModelHubRuntime::with_file_vault(dir.path().to_path_buf()).unwrap();
        runtime.save_remote_node_for_device(&mut app, Some("public".into()), "Desktop",
            "https://node.example", Some("public-token"), false, None, true, Some("machine")).unwrap();
        runtime.save_remote_node_for_device(&mut app, Some("vpn".into()), "Desktop VPN",
            "http://node.internal", Some("vpn-token"), false, None, true, Some("machine")).unwrap();
        let mut saved = runtime.load_hub_state();
        saved["remoteNodes"][1]["transportHint"] = json!("keep");
        saved["remoteNodes"][1]["token"] = json!("legacy-plaintext");
        runtime.save_hub_state(&saved).unwrap();
        // Legacy callers must retain explicit membership and keep credential slots separate.
        let snap = runtime.save_remote_node(&mut app, Some("vpn".into()), "Renamed",
            "http://node.internal", None, false, None, true).unwrap();
        let nodes = snap["remoteNodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n["deviceId"] == "machine"));
        assert_eq!(nodes[1]["transportHint"], "keep");
        assert!(nodes[1].get("token").is_none());
        assert_eq!(runtime.secrets.get(&remote_node_token_slot("public")).unwrap(), "public-token");
        assert_eq!(runtime.secrets.get(&remote_node_token_slot("vpn")).unwrap(), "vpn-token");
        let snap = runtime.save_remote_node(&mut app, None, "Updated",
            "https://node.example/", None, false, None, true).unwrap();
        assert_eq!(snap["remoteNodes"].as_array().unwrap().len(), 2);
        node_pairing::merge_topology(dir.path(), &mut app, &runtime, &json!({
            "remoteNodes": [{"id":"imported", "deviceId":"machine", "label":"Alias",
                "url":"http://alias.internal", "enabled":true}]
        })).unwrap();
        let hub = runtime.load_hub_state();
        let imported = hub["remoteNodes"].as_array().unwrap().iter()
            .find(|n| n["id"] == "imported").unwrap();
        assert_eq!(imported["deviceId"], "machine");
    }

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

/// No authenticated provider quota feed is configured yet. Local token/step counts
/// and another process's session logs cannot establish the user's subscription quota.
pub fn probe_subscription_quotas_cached() -> Value {
    json!({
        "codex": { "status": "unavailable", "reason": "PROVIDER_QUOTA_NOT_CONNECTED" },
        "antigravity": { "status": "unavailable", "reason": "PROVIDER_QUOTA_NOT_CONNECTED" },
        "claude": { "status": "unavailable", "reason": "PROVIDER_QUOTA_NOT_CONNECTED" }
    })
}

pub mod assistant;

pub mod subscription_quota;

mod dsh_live;
