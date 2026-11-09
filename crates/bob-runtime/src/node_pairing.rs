//! Self-contained Node & Web Host QR Pairing (`bob://pair?...` / `bob-node-pair:1:...`),
//! terminal ANSI QR code rendering, and bidirectional topology synchronization
//! (Mobile Broker <-> Work Nodes <-> Web Hosts).

use crate::{now_millis, provider_slot, remote_node_token_slot, ModelHubRuntime, DEFAULT_SLOT, TOOL_KEY_IDS};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use bob_kernel::app::LocalApp;
use bob_kernel::personal_ops::PeerSyncBundle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::time::Duration;

pub const NODE_PAIR_PREFIX: &str = "bob-node-pair:1:";
pub const NODE_PAIR_URI_PREFIX: &str = "bob://pair?";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodePairingInvitation {
    pub v: u8,
    pub role: String,       // "node" | "web_host"
    pub node_id: String,    // e.g. "node-vps3"
    pub label: String,      // human-readable host label
    pub base_url: String,   // e.g. "http://100.64.0.3:53020" or "https://bob-online.bobbik.org"
    pub pairing_id: String, // e.g. "pair_1793..."
    pub user_code: String,  // 6-digit code
    pub expires_at: i64,
    #[serde(default)]
    pub harnesses: Vec<String>,
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn url_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn is_node_pairing_code(raw: &str) -> bool {
    let trimmed = raw.trim();
    trimmed.starts_with(NODE_PAIR_URI_PREFIX) || trimmed.starts_with(NODE_PAIR_PREFIX)
}

impl NodePairingInvitation {
    /// Encode as a compact `bob://pair?...` URI suitable for low-version terminal QR codes.
    pub fn encode_qr_code(&self) -> String {
        let mut parts = vec![
            format!("u={}", url_encode(&self.base_url)),
            format!("c={}", url_encode(&self.user_code)),
            format!("p={}", url_encode(&self.pairing_id)),
            format!("n={}", url_encode(&self.label)),
            format!("id={}", url_encode(&self.node_id)),
            format!("r={}", url_encode(&self.role)),
            format!("x={}", self.expires_at),
        ];
        if !self.harnesses.is_empty() {
            parts.push(format!("h={}", url_encode(&self.harnesses.join(","))));
        }
        format!("{NODE_PAIR_URI_PREFIX}{}", parts.join("&"))
    }

    /// Encode as a `bob-node-pair:1:<base64url>` payload.
    pub fn encode_base64_code(&self) -> Result<String, &'static str> {
        let raw = serde_json::to_vec(self).map_err(|_| "PAIRING_CODE_INVALID")?;
        Ok(format!("{NODE_PAIR_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw)))
    }

    pub fn decode_qr_code(raw: &str, now_ms: i64) -> Result<Self, &'static str> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.len() > 4096 {
            return Err("PAIRING_CODE_INVALID");
        }
        let inv = if let Some(query) = trimmed.strip_prefix(NODE_PAIR_URI_PREFIX) {
            let mut base_url = String::new();
            let mut user_code = String::new();
            let mut pairing_id = String::new();
            let mut label = String::new();
            let mut node_id = String::new();
            let mut role = "node".to_owned();
            let mut expires_at = now_ms + 600_000;
            let mut harnesses = Vec::new();
            for kv in query.split('&') {
                let Some((k, v)) = kv.split_once('=') else {
                    continue;
                };
                let val = url_decode(v);
                match k {
                    "u" | "url" => base_url = val,
                    "c" | "code" => user_code = val,
                    "p" | "pairingId" => pairing_id = val,
                    "n" | "label" => label = val,
                    "id" | "nodeId" => node_id = val,
                    "r" | "role" => role = val,
                    "x" | "expiresAt" => expires_at = val.parse().unwrap_or(expires_at),
                    "h" => {
                        harnesses = val
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect()
                    }
                    _ => {}
                }
            }
            let clean_url = base_url.trim().trim_end_matches('/').to_owned();
            if clean_url.is_empty() || user_code.trim().is_empty() {
                return Err("PAIRING_CODE_INVALID");
            }
            if node_id.trim().is_empty() {
                node_id = compute_node_id(&clean_url);
            }
            if label.trim().is_empty() {
                label = clean_url.clone();
            }
            Self {
                v: 1,
                role,
                node_id,
                label,
                base_url: clean_url,
                pairing_id,
                user_code,
                expires_at,
                harnesses,
            }
        } else if let Some(b64) = trimmed.strip_prefix(NODE_PAIR_PREFIX) {
            let bytes = URL_SAFE_NO_PAD
                .decode(b64)
                .map_err(|_| "PAIRING_CODE_INVALID")?;
            serde_json::from_slice::<Self>(&bytes).map_err(|_| "PAIRING_CODE_INVALID")?
        } else {
            return Err("PAIRING_CODE_INVALID");
        };

        if inv.base_url.is_empty() || inv.user_code.is_empty() {
            return Err("PAIRING_CODE_INVALID");
        }
        if inv.expires_at <= now_ms {
            return Err("PAIRING_INVITATION_EXPIRED");
        }
        Ok(inv)
    }
}

pub fn compute_node_id(base_url: &str) -> String {
    let burl = base_url.trim().trim_end_matches('/');
    let mut h = 0u64;
    for b in burl.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    format!("node-{h:x}")
}

pub fn detect_lan_or_tailscale_ip() -> String {
    if let Ok(ip) = std::env::var("BOB_NODE_ADVERTISE_IP") {
        let trimmed = ip.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    // Use UDP socket routing table lookup (no packets sent over the wire)
    for target in ["100.100.100.100:80", "192.168.1.1:80", "8.8.8.8:80"] {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(target).is_ok() {
                if let Ok(local) = sock.local_addr() {
                    let ip = local.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() {
                        return ip.to_string();
                    }
                }
            }
        }
    }
    "127.0.0.1".to_owned()
}

pub fn load_device_harnesses_raw(data_dir: &Path) -> Vec<Value> {
    let path = data_dir.join("device_harnesses.json");
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(arr) = serde_json::from_slice::<Vec<Value>>(&bytes) {
            return arr;
        }
    }
    Vec::new()
}

pub fn save_device_harnesses_raw(data_dir: &Path, items: &[Value]) -> Result<(), &'static str> {
    let path = data_dir.join("device_harnesses.json");
    let raw = serde_json::to_string_pretty(items).map_err(|_| "HARNESS_CONFIG_SAVE_FAILED")?;
    fs::write(path, raw).map_err(|_| "HARNESS_CONFIG_SAVE_FAILED")?;
    // Clear cached directory files so new grants take effect immediately
    if let Ok(entries) = fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("agy-directory-") && name.ends_with(".json") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
    Ok(())
}

/// Normalize a `deviceHarnesses` item so both camelCase (`connectionId`, `sshTarget`, `enabledHarnesses`)
/// and snake_case (`connection_id`, `ssh_target`, `enabled_harnesses`) keys are present for cross-runtime compatibility.
pub fn normalize_harness_item(item: &Value) -> Option<Value> {
    let cid = item
        .get("connectionId")
        .or_else(|| item.get("connection_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_owned();
    let label = item
        .get("label")
        .or_else(|| item.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&cid)
        .to_owned();
    let ssh_target = item
        .get("sshTarget")
        .or_else(|| item.get("ssh_target"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_owned();
    let mut enabled_harnesses: Vec<String> = item
        .get("enabledHarnesses")
        .or_else(|| item.get("enabled_harnesses"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if enabled_harnesses.is_empty() {
        enabled_harnesses = vec![
            "dsh".to_owned(),
            "agy".to_owned(),
            "claude".to_owned(),
            "codex".to_owned(),
            "pi".to_owned(),
            "cp".to_owned(),
        ];
    }
    let notes = item
        .get("notes")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let updated_at = item
        .get("updatedAt")
        .or_else(|| item.get("updated_at"))
        .and_then(Value::as_i64)
        .unwrap_or_else(now_millis);
    Some(json!({
        "connectionId": cid,
        "connection_id": cid,
        "label": label,
        "sshTarget": ssh_target,
        "ssh_target": ssh_target,
        "enabledHarnesses": enabled_harnesses,
        "enabled_harnesses": enabled_harnesses,
        "notes": notes,
        "updatedAt": updated_at,
        "updated_at": updated_at,
    }))
}

/// Export local topology (`remoteNodes` with tokens optional for broker push + `deviceHarnesses`).
pub fn export_local_topology(
    data_dir: &Path,
    runtime: &ModelHubRuntime,
    include_tokens: bool,
) -> Value {
    export_local_topology_with_app(data_dir, None, runtime, include_tokens)
}

/// The host's synthetic `local` connection is usable only on that host.
pub fn is_host_loopback_alias(node: &Value) -> bool {
    if node["id"] != "local" {
        return false;
    }
    let url = node["baseUrl"].as_str().or_else(|| node["url"].as_str());
    url.and_then(|value| reqwest::Url::parse(value).ok()).is_some_and(|url| {
        url.host_str().is_some_and(|host| {
            host.trim_end_matches('.').eq_ignore_ascii_case("localhost")
                || host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
        })
    })
}

/// Export full local topology + knowledge bundle (`remoteNodes`, `deviceHarnesses`, `modelHub`,
/// `projects`, `projectSessionLinks`, and `peerSyncBundle`).
pub fn export_local_topology_with_app(
    data_dir: &Path,
    app: Option<&LocalApp>,
    runtime: &ModelHubRuntime,
    include_tokens: bool,
) -> Value {
    let hub = runtime.load_hub_state();
    let mut nodes = hub
        .get("remoteNodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    nodes.retain(|node| !is_host_loopback_alias(node));
    for node in &mut nodes {
        if let Some(obj) = node.as_object_mut() {
            let nid = obj
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let url_val = obj
                .get("url")
                .or_else(|| obj.get("baseUrl"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let name_val = obj
                .get("name")
                .or_else(|| obj.get("label"))
                .and_then(Value::as_str)
                .unwrap_or(&url_val)
                .to_owned();
            obj.insert("url".to_owned(), json!(url_val));
            obj.insert("baseUrl".to_owned(), json!(url_val));
            obj.insert("name".to_owned(), json!(name_val));
            obj.insert("label".to_owned(), json!(name_val));
            let slot = remote_node_token_slot(&nid);
            let tok = runtime.secrets.get(&slot).unwrap_or_default();
            if include_tokens && !tok.trim().is_empty() {
                obj.insert("token".to_owned(), json!(tok.trim()));
            } else {
                obj.remove("token");
            }
            obj.insert("hasToken".to_owned(), json!(!tok.trim().is_empty()));
        }
    }
    let harnesses: Vec<Value> = load_device_harnesses_raw(data_dir)
        .into_iter()
        .filter_map(|item| normalize_harness_item(&item))
        .collect();

    let mut provider_keys = serde_json::Map::new();
    if include_tokens {
        let registry = runtime.load_or_init_registry();
        if let Some(providers) = registry.get("providers").and_then(Value::as_array) {
            for p in providers {
                if let Some(pid) = p.get("id").and_then(Value::as_str) {
                    if let Ok(k) = runtime.secrets.get(&provider_slot(pid)) {
                        if !k.trim().is_empty() {
                            provider_keys.insert(pid.to_owned(), json!(k.trim()));
                        }
                    }
                }
            }
        }
        for tool_id in TOOL_KEY_IDS {
            if let Ok(k) = runtime.secrets.get(&provider_slot(tool_id)) {
                if !k.trim().is_empty() {
                    provider_keys.insert(tool_id.to_owned(), json!(k.trim()));
                }
            }
        }
        if let Ok(k) = runtime.secrets.get(DEFAULT_SLOT) {
            if !k.trim().is_empty() {
                provider_keys.insert("__default__".to_owned(), json!(k.trim()));
            }
        }
    }

    let model_hub = json!({
        "activeRoles": hub.get("activeRoles").cloned().unwrap_or_else(|| json!({})),
        "providerVariants": hub.get("providerVariants").cloned().unwrap_or_else(|| json!({})),
        "customModels": hub.get("customModels").cloned().unwrap_or_else(|| json!([])),
        "providerKeys": Value::Object(provider_keys),
    });

    let mut out = json!({
        "remoteNodes": nodes,
        "deviceHarnesses": harnesses,
        "modelHub": model_hub,
        "updatedAt": now_millis(),
    });

    if let Some(app_ref) = app {
        if let Ok(pl) = app_ref.export_projects_and_links() {
            if let Some(obj) = out.as_object_mut() {
                obj.insert(
                    "projects".to_owned(),
                    pl.get("projects").cloned().unwrap_or_else(|| json!([])),
                );
                obj.insert(
                    "projectSessionLinks".to_owned(),
                    pl.get("projectSessionLinks")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                );
            }
        }
        if let Ok(bundle) = app_ref.export_peer_sync_bundle(
            &format!("topo-sync:{}", now_millis()),
            String::new(),
        ) {
            if let Ok(bundle_val) = serde_json::to_value(bundle) {
                if let Some(obj) = out.as_object_mut() {
                    obj.insert("peerSyncBundle".to_owned(), bundle_val);
                }
            }
        }
    }

    out
}

/// Merge incoming topology + knowledge (`remoteNodes`, `deviceHarnesses`, `modelHub`, `projects`,
/// `projectSessionLinks`, and `peerSyncBundle`) into `data_dir`, `LocalApp`, and `ModelHubRuntime`.
pub fn merge_topology(
    data_dir: &Path,
    app: &mut LocalApp,
    runtime: &ModelHubRuntime,
    incoming: &Value,
) -> Result<Value, &'static str> {
    let now = now_millis();
    if let Some(inc_nodes) = incoming.get("remoteNodes").and_then(Value::as_array) {
        for node in inc_nodes {
            if is_host_loopback_alias(node) {
                continue;
            }
            let url = node
                .get("url")
                .or_else(|| node.get("baseUrl"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .trim_end_matches('/');
            if url.is_empty() {
                continue;
            }
            let id = node
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| compute_node_id(url));
            let name = node
                .get("name")
                .or_else(|| node.get("label"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(url);
            let token = node
                .get("token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let remark = node.get("remark").and_then(Value::as_str);
            let enabled = node.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let _ = runtime.save_remote_node_for_device(
                app,
                Some(id),
                name,
                url,
                token,
                false,
                remark,
                enabled,
                node.get("deviceId").and_then(Value::as_str),
            )?;
        }
    }

    if let Some(inc_harnesses) = incoming.get("deviceHarnesses").and_then(Value::as_array) {
        if !inc_harnesses.is_empty() {
            let mut local_harnesses: Vec<Value> = load_device_harnesses_raw(data_dir)
                .into_iter()
                .filter_map(|item| normalize_harness_item(&item))
                .collect();
            let mut changed = false;
            for raw_item in inc_harnesses {
                let Some(item) = normalize_harness_item(raw_item) else {
                    continue;
                };
                let cid = item["connectionId"].as_str().unwrap_or("");
                let ssh_target = item["sshTarget"].as_str().unwrap_or("");
                // Never overwrite a local self-grant with a remote "local" grant
                if (cid == "local" || ssh_target == "local")
                    && local_harnesses.iter().any(|x| {
                        x.get("connectionId")
                            .or_else(|| x.get("connection_id"))
                            .and_then(Value::as_str)
                            == Some(cid)
                    })
                {
                    continue;
                }
                if let Some(existing) = local_harnesses.iter_mut().find(|x| {
                    x.get("connectionId")
                        .or_else(|| x.get("connection_id"))
                        .and_then(Value::as_str)
                        == Some(cid)
                }) {
                    *existing = item;
                    changed = true;
                } else {
                    local_harnesses.push(item);
                    changed = true;
                }
            }
            if changed {
                save_device_harnesses_raw(data_dir, &local_harnesses)?;
            }
        }
    }

    if let Some(mh) = incoming.get("modelHub").and_then(Value::as_object) {
        let mut hub = runtime.load_hub_state();
        let mut hub_changed = false;
        if let Some(roles) = mh.get("activeRoles").and_then(Value::as_object) {
            if !hub["activeRoles"].is_object() {
                hub["activeRoles"] = json!({});
            }
            if let Some(local_roles) = hub["activeRoles"].as_object_mut() {
                for (k, v) in roles {
                    if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                        let cur_empty = local_roles
                            .get(k)
                            .and_then(Value::as_str)
                            .map(|c| c.trim().is_empty())
                            .unwrap_or(true);
                        if cur_empty {
                            local_roles.insert(k.clone(), json!(s));
                            hub_changed = true;
                        }
                    }
                }
            }
        }
        if let Some(variants) = mh.get("providerVariants").and_then(Value::as_object) {
            if !hub["providerVariants"].is_object() {
                hub["providerVariants"] = json!({});
            }
            if let Some(local_vars) = hub["providerVariants"].as_object_mut() {
                for (k, v) in variants {
                    if !local_vars.contains_key(k) {
                        local_vars.insert(k.clone(), v.clone());
                        hub_changed = true;
                    }
                }
            }
        }
        if let Some(custom_models) = mh.get("customModels").and_then(Value::as_array) {
            if !hub["customModels"].is_array() {
                hub["customModels"] = json!([]);
            }
            if let Some(local_custom) = hub["customModels"].as_array_mut() {
                for cm in custom_models {
                    let cid = cm.get("id").and_then(Value::as_str).unwrap_or("");
                    if !cid.is_empty()
                        && !local_custom
                            .iter()
                            .any(|x| x.get("id").and_then(Value::as_str) == Some(cid))
                    {
                        local_custom.push(cm.clone());
                        hub_changed = true;
                    }
                }
            }
        }
        if hub_changed {
            let _ = runtime.save_hub_state(&hub);
        }
        if let Some(pkeys) = mh.get("providerKeys").and_then(Value::as_object) {
            for (pid, kval) in pkeys {
                if let Some(secret) = kval.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    let slot = if pid == "__default__" {
                        DEFAULT_SLOT.to_owned()
                    } else {
                        provider_slot(pid)
                    };
                    let existing = runtime.secrets.get(&slot).unwrap_or_default();
                    if existing.trim().is_empty() {
                        let _ = runtime.secrets.set(&slot, secret);
                    }
                }
            }
        }
    }

    if incoming.get("projects").is_some() || incoming.get("projectSessionLinks").is_some() {
        app.merge_projects_and_links(incoming, now).map_err(|_| "PROJECT_LINK_SYNC_FAILED")?;
    }

    if let Some(bundle_val) = incoming.get("peerSyncBundle").filter(|value| !value.is_null()) {
        let bundle = serde_json::from_value::<PeerSyncBundle>(bundle_val.clone())
            .map_err(|_| "HOME_PEER_SYNC_APPLY_FAILED")?;
        app.apply_peer_sync_bundle(&bundle, now).map_err(|_| "HOME_PEER_SYNC_APPLY_FAILED")?;
    }

    let _ = runtime.build_snapshot_and_sync(app)?;
    let mut full = export_local_topology_with_app(data_dir, Some(app), runtime, true);
    if let Some(obj) = full.as_object_mut() {
        obj.insert("status".to_owned(), json!("synced"));
        obj.insert("syncedAt".to_owned(), json!(now));
    }
    Ok(full)
}

/// Push local topology + knowledge to an already-paired `bob-node` / Web Host and return its merged payload.
pub fn sync_topology_with_peer_url(
    base_url: &str,
    token: Option<&str>,
    local_topology: &Value,
) -> Result<Value, &'static str> {
    let base = base_url.trim().trim_end_matches('/');
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err("PAIRING_HOME_UNREACHABLE");
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "PAIRING_NETWORK_UNAVAILABLE")?;
    let sync_url = format!("{base}/api/client/v1/topology/sync");
    let mut req = client.post(&sync_url).json(local_topology);
    if let Some(tok) = token.map(str::trim).filter(|s| !s.is_empty()) {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }
    let resp = req.send().map_err(|_| "PAIRING_HOME_UNREACHABLE")?;
    if !resp.status().is_success() {
        return Err("PAIRING_HOME_UNREACHABLE");
    }
    resp.json::<Value>().map_err(|_| "PAIRING_CODE_INVALID")
}

/// Execute HTTP pairing code exchange + bidirectional topology sync with a target `bob-node` / Web Host.
pub fn exchange_and_sync_with_peer(
    inv: &NodePairingInvitation,
    device_name: &str,
    local_topology: &Value,
) -> Result<Value, &'static str> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| "PAIRING_NETWORK_UNAVAILABLE")?;

    let base = inv.base_url.trim().trim_end_matches('/');
    let exchange_url = if inv.pairing_id.is_empty() {
        format!("{base}/api/client/v1/pairings/exchange")
    } else {
        format!("{base}/api/client/v1/pairings/{}/exchange", inv.pairing_id)
    };

    let ex_resp = client
        .post(&exchange_url)
        .json(&json!({
            "code": inv.user_code,
            "user_code": inv.user_code,
            "device_name": device_name,
        }))
        .send()
        .map_err(|_| "PAIRING_HOME_UNREACHABLE")?;

    if ex_resp.status().as_u16() == 410 {
        return Err("PAIRING_INVITATION_EXPIRED");
    }
    if !ex_resp.status().is_success() {
        return Err("PAIRING_CODE_INVALID");
    }
    let ex_json: Value = ex_resp.json().map_err(|_| "PAIRING_CODE_INVALID")?;
    let token = ex_json
        .get("access_token")
        .or_else(|| ex_json.get("token"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("PAIRING_CODE_INVALID")?
        .to_owned();

    // Push local topology (including remoteNodes + deviceHarnesses) to peer and receive peer's merged topology
    let sync_url = format!("{base}/api/client/v1/topology/sync");
    let synced_topology = if ex_json.get("topology_sync").and_then(Value::as_bool) == Some(false) {
        json!({})
    } else { client
        .post(&sync_url)
        .header("Authorization", format!("Bearer {token}"))
        .json(local_topology)
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.json::<Value>().ok())
        .unwrap_or_else(|| json!({})) };

    Ok(json!({
        "nodeId": ex_json.get("node_id").and_then(Value::as_str).unwrap_or(&inv.node_id),
        "label": inv.label,
        "baseUrl": base,
        "role": inv.role,
        "accessToken": token,
        "credentialId": ex_json.get("credential_id").cloned().unwrap_or(Value::Null),
        "expiresAt": ex_json.get("expires_at").cloned().unwrap_or(Value::Null),
        "syncedTopology": synced_topology,
    }))
}

/// Forward a DSH/Multi-Harness RPC (`app_dsh_list`, `app_dsh_page`, `app_dsh_prompt`, `app_dsh_receipt`, `app_dsh_preview`)
/// over HTTP to a remote `bob-node` endpoint, querying its `"local"` harnesses.
/// Discover protocol without sending either kind of credential. Never guess from a hostname.
pub fn is_bob_node_endpoint(endpoint: &str) -> Result<bool, &'static str> {
    let url = reqwest::Url::parse(endpoint).map_err(|_| "DSH_ADDRESS_INVALID")?;
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err("DSH_ADDRESS_INVALID");
    }
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(10))
        .build().map_err(|_| "DSH_UNAVAILABLE")?
        .get(format!("{}/api/node/v1/info", endpoint.trim_end_matches('/')))
        .send().map_err(|_| "DSH_UNAVAILABLE")?;
    if matches!(response.status().as_u16(), 401 | 404) { return Ok(false); }
    if !response.status().is_success() { return Err("DSH_UNAVAILABLE"); }
    let body: Value = response.json().map_err(|_| "DSH_PROTOCOL")?;
    Ok(body["protocol"] == "bob-node-v1")
}

/// A paired node owns its local tool authentication. Do not retry another peer.
pub fn route_paired_node_rpc(runtime: &ModelHubRuntime, connection: &str, command: &str, args: &Value) -> Result<Option<Value>, &'static str> {
    if connection == "local" { return Ok(None); }
    let hub = runtime.load_hub_state();
    let node = hub["remoteNodes"].as_array().and_then(|nodes| nodes.iter().find(|n| n["id"] == connection && n["enabled"].as_bool().unwrap_or(true))).ok_or("DSH_CONNECTION_NOT_FOUND")?;
    let endpoint = node["url"].as_str().or_else(|| node["baseUrl"].as_str()).ok_or("DSH_ADDRESS_INVALID")?;
    if !is_bob_node_endpoint(endpoint)? { return Ok(None); }
    let token = runtime.secrets.get(&remote_node_token_slot(connection)).ok();
    forward_dsh_rpc_to_endpoint(endpoint, token.as_deref(), connection, command, args).map(Some)
}

pub fn forward_dsh_rpc_to_endpoint(
    raw_url: &str,
    token: Option<&str>,
    connection_id: &str,
    command: &str,
    args: &Value,
) -> Result<Value, &'static str> {
    forward_dsh_rpc_to_endpoint_with_target(
        raw_url,
        token,
        connection_id,
        "local",
        command,
        args,
    )
}

/// Forward a DSH/Multi-Harness RPC over HTTP to a remote `bob-node` / Web Host endpoint,
/// requesting `target_connection_id` on that peer and stamping returned records with `connection_id`.
fn forwarded_rpc_timeout_secs(command: &str, args: &Value) -> u64 {
    if command=="app_dsh_preview" && crate::agy_directory::preview_host_timeout_secs(args["path"].as_str().unwrap_or(""))>35 {
        // Native conversion <=110s; DSH can additionally read history, stat,
        // download and re-stat before rendering. Keep the outer budget larger.
        240
    } else {35}
}

fn forwarded_preview_error(body: &Value) -> &'static str {
    match body["error"].as_str().unwrap_or("") {
        "DSH_DOCUMENT_RENDERER_UNAVAILABLE"=>"DSH_DOCUMENT_RENDERER_UNAVAILABLE",
        "DSH_DOCUMENT_SANDBOX_UNAVAILABLE"=>"DSH_DOCUMENT_SANDBOX_UNAVAILABLE",
        "DSH_DOCUMENT_RENDER_TIMEOUT"=>"DSH_DOCUMENT_RENDER_TIMEOUT",
        "DSH_DOCUMENT_RENDER_FAILED"=>"DSH_DOCUMENT_RENDER_FAILED",
        "DSH_DOCUMENT_PAGE_LIMIT"=>"DSH_DOCUMENT_PAGE_LIMIT",
        "DSH_FILE_TOO_LARGE"=>"DSH_FILE_TOO_LARGE",
        "DSH_FILE_TIMEOUT"=>"DSH_FILE_TIMEOUT",
        "DSH_FILE_CHANGED"=>"DSH_FILE_CHANGED",
        "DSH_FILE_NOT_REFERENCED"=>"DSH_FILE_NOT_REFERENCED",
        "DSH_FILE_OUTSIDE_WORKSPACE"=>"DSH_FILE_OUTSIDE_WORKSPACE",
        "DSH_FILE_PATH_REJECTED"=>"DSH_FILE_PATH_REJECTED",
        "DSH_FILE_NOT_FOUND"=>"DSH_FILE_NOT_FOUND",
        "DSH_FILE_TYPE_UNSUPPORTED"=>"DSH_FILE_TYPE_UNSUPPORTED",
        _=>"DSH_UNAVAILABLE",
    }
}

pub fn forward_dsh_rpc_to_endpoint_with_target(
    raw_url: &str,
    token: Option<&str>,
    connection_id: &str,
    target_connection_id: &str,
    command: &str,
    args: &Value,
) -> Result<Value, &'static str> {
    let raw_url = raw_url.trim().trim_end_matches('/');
    if !raw_url.starts_with("http://") && !raw_url.starts_with("https://") {
        return Err("DSH_ADDRESS_INVALID");
    }
    let mut forwarded_args = args.clone();
    if let Some(obj) = forwarded_args.as_object_mut() {
        obj.insert("connectionId".to_owned(), json!(target_connection_id));
        obj.insert("_relayed".to_owned(), json!(true));
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(forwarded_rpc_timeout_secs(command,args)))
        .build()
        .map_err(|_| "DSH_UNAVAILABLE")?;
    let mut req = client
        .post(format!("{raw_url}/api/rpc/{command}"))
        .json(&forwarded_args);
    if let Some(tok) = token.map(str::trim).filter(|s| !s.is_empty()) {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }
    let resp = req.send().map_err(|e|if command=="app_dsh_preview" && e.is_timeout() {"DSH_FILE_TIMEOUT"} else {"DSH_UNAVAILABLE"})?;
    let success=resp.status().is_success();
    if !success && command!="app_dsh_preview" {return Err("DSH_UNAVAILABLE");}
    let body: Value = resp.json().map_err(|_| if success {"DSH_PROTOCOL"} else {"DSH_UNAVAILABLE"})?;
    if !success {return Err(forwarded_preview_error(&body));}
    let mut result = if let Some(ok) = body.get("ok").and_then(Value::as_bool) {
        if !ok {
            return Err(if command=="app_dsh_preview" {forwarded_preview_error(&body)} else {"DSH_UNAVAILABLE"});
        }
        body.get("result").cloned().unwrap_or(Value::Null)
    } else {
        body
    };
    if command == "app_dsh_list" {
        if let Some(arr) = result.as_array_mut() {
            for item in arr {
                if let Some(obj) = item.as_object_mut() {
                    obj.insert("connectionId".to_owned(), json!(connection_id));
                }
            }
        }
    } else if matches!(command, "app_native_setup" | "app_dsh_create" | "app_dsh_cancel" | "app_dsh_live" | "app_dsh_page" | "app_dsh_history") {
        if let Some(addr) = result.get_mut("address").and_then(Value::as_object_mut) {
            addr.insert("connection_id".to_owned(), json!(connection_id));
        }
    } else if command == "app_dsh_preview" {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("connection_id".to_owned(), json!(connection_id));
        }
    }
    Ok(result)
}

/// Forward a DSH/Multi-Harness RPC (`app_dsh_list`, `app_dsh_page`, `app_dsh_prompt`, `app_dsh_receipt`, `app_dsh_preview`)
/// over HTTP to a remote `bob-node` instance registered in `remoteNodes`.
/// First tries the target node's own URL (`target_connection_id = "local"` unless the target itself is a `web_host`).
/// If the target node URL only hosts a raw DSH server (not `bob-node`) and this request wasn't already relayed,
/// falls back to any paired `web_host` / `bob-node` peer in `remoteNodes` with `target_connection_id = connection_id`.
pub fn forward_dsh_rpc_to_remote_node(
    runtime: &ModelHubRuntime,
    connection_id: &str,
    command: &str,
    args: &Value,
) -> Result<Value, &'static str> {
    if matches!(command,"app_native_setup" | "app_dsh_create") {return Err("DSH_CREATE_UNCONFIRMED");}
    if command == "app_dsh_cancel" {return Err("DSH_CANCEL_UNCONFIRMED");}
    let already_relayed = args
        .get("_relayed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let hub = runtime.load_hub_state();
    let nodes = hub["remoteNodes"]
        .as_array()
        .ok_or("DSH_CONNECTION_NOT_FOUND")?;

    if let Some(node) = nodes.iter().find(|n| {
        n["id"].as_str() == Some(connection_id)
            && n.get("enabled").and_then(Value::as_bool).unwrap_or(true)
    }) {
        let raw_url = node
            .get("url")
            .or_else(|| node.get("baseUrl"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let remark = node.get("remark").and_then(Value::as_str).unwrap_or("");
        let token = runtime
            .secrets
            .get(&remote_node_token_slot(connection_id))
            .ok();
        let target_cid = if remark == "web_host" {
            "local"
        } else {
            "local"
        };
        if let Ok(res) = forward_dsh_rpc_to_endpoint_with_target(
            raw_url,
            token.as_deref(),
            connection_id,
            target_cid,
            command,
            args,
        ) {
            return Ok(res);
        }
    }

    // A send with an unknown receipt must never be replayed through another peer.
    if already_relayed || command == "app_dsh_prompt" {
        return Err("DSH_UNAVAILABLE");
    }

    // Fallback: relay through any paired `web_host` (or other `bob-node`) peer in `remoteNodes`
    let mut candidates: Vec<&Value> = nodes
        .iter()
        .filter(|n| {
            n["id"].as_str() != Some(connection_id)
                && n.get("enabled").and_then(Value::as_bool).unwrap_or(true)
        })
        .collect();
    candidates.sort_by_key(|n| {
        let is_web_host = n.get("remark").and_then(Value::as_str) == Some("web_host")
            || n.get("url")
                .or_else(|| n.get("baseUrl"))
                .and_then(Value::as_str)
                .map(|u| u.contains("bob-online"))
                .unwrap_or(false);
        if is_web_host { 0 } else { 1 }
    });

    for peer in candidates {
        let Some(peer_id) = peer.get("id").and_then(Value::as_str) else {
            continue;
        };
        let peer_url = peer
            .get("url")
            .or_else(|| peer.get("baseUrl"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let peer_tok = runtime
            .secrets
            .get(&remote_node_token_slot(peer_id))
            .ok();
        if let Ok(res) = forward_dsh_rpc_to_endpoint_with_target(
            peer_url,
            peer_tok.as_deref(),
            connection_id,
            connection_id,
            command,
            args,
        ) {
            return Ok(res);
        }
    }

    Err("DSH_UNAVAILABLE")
}

// ============================================================================
// Pure-Rust ISO/IEC 18004 QR Code (Byte mode, EC Level L, Versions 1..10)
// and Unicode Half-Block Terminal Renderer
// ============================================================================

const VERSION_L_SPECS: [(usize, usize, usize); 10] = [
    // (total_data_codewords, ec_codewords_per_block, num_blocks)
    (19, 7, 1),   // v1: 21x21
    (34, 10, 1),  // v2: 25x25
    (55, 15, 1),  // v3: 29x29
    (80, 20, 1),  // v4: 33x33
    (108, 26, 1), // v5: 37x37
    (136, 18, 2), // v6: 41x41
    (156, 20, 2), // v7: 45x45
    (194, 24, 2), // v8: 49x49
    (232, 30, 2), // v9: 53x53
    (274, 18, 4), // v10: 57x57 (2 blocks of 68 + 2 blocks of 69 = 274)
];

fn gf_mul(mut x: u8, mut y: u8) -> u8 {
    let mut r = 0u8;
    for _ in 0..8 {
        if (y & 1) != 0 {
            r ^= x;
        }
        let hi = x & 0x80;
        x <<= 1;
        if hi != 0 {
            x ^= 0x1D;
        }
        y >>= 1;
    }
    r
}

fn rs_generator_poly(degree: usize) -> Vec<u8> {
    let mut poly = vec![1u8];
    let mut root = 1u8;
    for _ in 0..degree {
        let mut next = vec![0u8; poly.len() + 1];
        for (i, &c) in poly.iter().enumerate() {
            next[i] ^= c;
            next[i + 1] ^= gf_mul(c, root);
        }
        poly = next;
        root = gf_mul(root, 2);
    }
    poly
}

fn rs_remainder(data: &[u8], degree: usize) -> Vec<u8> {
    let gen = rs_generator_poly(degree);
    let mut rem = vec![0u8; degree];
    for &b in data {
        let factor = b ^ rem[0];
        rem.remove(0);
        rem.push(0);
        for (i, &g) in gen.iter().skip(1).enumerate() {
            rem[i] ^= gf_mul(g, factor);
        }
    }
    rem
}

fn alignment_coords(version: usize) -> &'static [usize] {
    match version {
        1 => &[],
        2 => &[6, 18],
        3 => &[6, 22],
        4 => &[6, 26],
        5 => &[6, 30],
        6 => &[6, 34],
        7 => &[6, 22, 38],
        8 => &[6, 24, 42],
        9 => &[6, 26, 46],
        10 => &[6, 28, 50],
        _ => &[],
    }
}

pub fn encode_qr_matrix(payload: &str) -> Result<(usize, Vec<Vec<bool>>), &'static str> {
    let bytes = payload.as_bytes();
    let mut ver = 0usize;
    for (idx, &(data_cw, _, _)) in VERSION_L_SPECS.iter().enumerate() {
        let v = idx + 1;
        let len_bits = if v <= 9 { 8 } else { 16 };
        let max_bytes = (data_cw * 8 - 4 - len_bits) / 8;
        if bytes.len() <= max_bytes {
            ver = v;
            break;
        }
    }
    if ver == 0 {
        return Err("QR_PAYLOAD_TOO_LARGE");
    }
    let (total_data_cw, ec_cw, num_blocks) = VERSION_L_SPECS[ver - 1];
    let mut bits: Vec<bool> = Vec::with_capacity(total_data_cw * 8);
    let push_bits = |bits: &mut Vec<bool>, val: usize, count: usize| {
        for i in (0..count).rev() {
            bits.push(((val >> i) & 1) != 0);
        }
    };
    push_bits(&mut bits, 0b0100, 4); // Byte mode
    push_bits(&mut bits, bytes.len(), if ver <= 9 { 8 } else { 16 });
    for &b in bytes {
        push_bits(&mut bits, b as usize, 8);
    }
    let max_bits = total_data_cw * 8;
    for _ in 0..4.min(max_bits.saturating_sub(bits.len())) {
        bits.push(false);
    }
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut data_codewords = Vec::with_capacity(total_data_cw);
    for chunk in bits.chunks(8) {
        let mut b = 0u8;
        for &bit in chunk {
            b = (b << 1) | (bit as u8);
        }
        data_codewords.push(b);
    }
    let pads = [0xECu8, 0x11u8];
    let mut pad_idx = 0;
    while data_codewords.len() < total_data_cw {
        data_codewords.push(pads[pad_idx % 2]);
        pad_idx += 1;
    }

    // Split into blocks and compute Reed-Solomon EC codewords
    let short_block_len = total_data_cw / num_blocks;
    let num_long_blocks = total_data_cw % num_blocks;
    let num_short_blocks = num_blocks - num_long_blocks;
    let mut data_blocks = Vec::with_capacity(num_blocks);
    let mut ec_blocks = Vec::with_capacity(num_blocks);
    let mut offset = 0;
    for b in 0..num_blocks {
        let blen = if b < num_short_blocks {
            short_block_len
        } else {
            short_block_len + 1
        };
        let slice = &data_codewords[offset..offset + blen];
        offset += blen;
        data_blocks.push(slice.to_vec());
        ec_blocks.push(rs_remainder(slice, ec_cw));
    }

    let mut final_codewords = Vec::with_capacity(total_data_cw + ec_cw * num_blocks);
    let max_data_len = if num_long_blocks > 0 {
        short_block_len + 1
    } else {
        short_block_len
    };
    for i in 0..max_data_len {
        for blk in &data_blocks {
            if i < blk.len() {
                final_codewords.push(blk[i]);
            }
        }
    }
    for i in 0..ec_cw {
        for blk in &ec_blocks {
            final_codewords.push(blk[i]);
        }
    }

    let size = 17 + 4 * ver;
    let mut modules = vec![vec![false; size]; size];
    let mut is_func = vec![vec![false; size]; size];

    let mut set_func = |r: usize, c: usize, val: bool| {
        modules[r][c] = val;
        is_func[r][c] = true;
    };

    // Timing patterns
    for i in 0..size {
        set_func(6, i, i % 2 == 0);
        set_func(i, 6, i % 2 == 0);
    }

    // Finder patterns + separators
    for &(fr, fc) in &[(0isize, 0isize), (0, (size - 7) as isize), ((size - 7) as isize, 0)] {
        for dr in -1..=7 {
            for dc in -1..=7 {
                let r = fr + dr;
                let c = fc + dc;
                if r >= 0 && r < size as isize && c >= 0 && c < size as isize {
                    let in_box = (0..=6).contains(&dr) && (0..=6).contains(&dc);
                    let dark = in_box
                        && (dr == 0
                            || dr == 6
                            || dc == 0
                            || dc == 6
                            || ((2..=4).contains(&dr) && (2..=4).contains(&dc)));
                    set_func(r as usize, c as usize, dark);
                }
            }
        }
    }

    // Alignment patterns
    let align = alignment_coords(ver);
    for &ar in align {
        for &ac in align {
            if (ar == 6 && ac == 6)
                || (ar == 6 && ac == size - 7)
                || (ar == size - 7 && ac == 6)
            {
                continue;
            }
            for dr in -2isize..=2 {
                for dc in -2isize..=2 {
                    let r = (ar as isize + dr) as usize;
                    let c = (ac as isize + dc) as usize;
                    let dark = dr.abs().max(dc.abs()) != 1;
                    set_func(r, c, dark);
                }
            }
        }
    }

    // Reserve format information areas + dark module
    for i in 0..9 {
        if i != 6 {
            set_func(8, i, false);
            set_func(i, 8, false);
        }
    }
    for i in 0..8 {
        set_func(8, size - 1 - i, false);
        set_func(size - 1 - i, 8, false);
    }
    set_func(size - 8, 8, true);

    // Reserve version info if ver >= 7
    if ver >= 7 {
        let mut rem = ver;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        let ver_bits = (ver << 12) | rem;
        for i in 0..18 {
            let bit = ((ver_bits >> i) & 1) != 0;
            let a = size - 11 + (i % 3);
            let b = i / 3;
            set_func(a, b, bit);
            set_func(b, a, bit);
        }
    }

    // Place data bits in zigzag order
    let mut bit_idx = 0usize;
    let total_bits = final_codewords.len() * 8;
    let mut right = (size - 1) as isize;
    while right >= 1 {
        if right == 6 {
            right -= 1;
        }
        let upward = ((right + 1) & 2) == 0;
        for v in 0..size {
            let row = if upward { size - 1 - v } else { v };
            for j in 0..2 {
                let col = (right - j) as usize;
                if !is_func[row][col] {
                    let mut dark = if bit_idx < total_bits {
                        let cw = final_codewords[bit_idx / 8];
                        ((cw >> (7 - (bit_idx % 8))) & 1) != 0
                    } else {
                        false
                    };
                    bit_idx += 1;
                    // Apply Mask 0: (row + col) % 2 == 0
                    if (row + col) % 2 == 0 {
                        dark = !dark;
                    }
                    modules[row][col] = dark;
                }
            }
        }
        right -= 2;
    }

    // Format info for EC Level L (01) + Mask 0 (000) -> 0x77C4
    let fmt_bits: u16 = 0x77C4;
    for i in 0..=5 {
        modules[8][i] = ((fmt_bits >> i) & 1) != 0;
    }
    modules[8][7] = ((fmt_bits >> 6) & 1) != 0;
    modules[8][8] = ((fmt_bits >> 7) & 1) != 0;
    modules[7][8] = ((fmt_bits >> 8) & 1) != 0;
    for i in 9..15 {
        modules[14 - i][8] = ((fmt_bits >> i) & 1) != 0;
    }
    for i in 0..8 {
        modules[size - 1 - i][8] = ((fmt_bits >> i) & 1) != 0;
    }
    for i in 8..15 {
        modules[8][size - 15 + i] = ((fmt_bits >> i) & 1) != 0;
    }
    modules[size - 8][8] = true;

    Ok((size, modules))
}

/// Render a QR code string as a compact Unicode half-block string for terminal display.
/// Uses light background with dark modules (`█` = light, ` ` = dark, `▀`/`▄` = half-blocks)
/// so phone cameras can reliably scan it on dark terminal backgrounds.
pub fn render_terminal_qr(payload: &str) -> Result<String, &'static str> {
    let (size, modules) = encode_qr_matrix(payload)?;
    let quiet = 2isize;
    let total = size as isize + quiet * 2;
    let is_dark = |r: isize, c: isize| -> bool {
        if r < quiet || c < quiet || r >= quiet + size as isize || c >= quiet + size as isize {
            false
        } else {
            modules[(r - quiet) as usize][(c - quiet) as usize]
        }
    };

    let mut out = String::new();
    let mut r = 0isize;
    while r < total {
        out.push_str("  ");
        for c in 0..total {
            let top_dark = is_dark(r, c);
            let bot_dark = if r + 1 < total {
                is_dark(r + 1, c)
            } else {
                false
            };
            // On a dark terminal, foreground glyphs are bright (white) and spaces are dark (black):
            // QR quiet zone & light modules = bright block, QR dark modules = space
            let ch = match (top_dark, bot_dark) {
                (false, false) => '█',
                (false, true) => '▀',
                (true, false) => '▄',
                (true, true) => ' ',
            };
            out.push(ch);
        }
        out.push('\n');
        r += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsh_preview_forwarding_preserves_document_errors_without_echoing_details() {
        use std::io::{Read,Write};
        for (code,expected) in [("DSH_DOCUMENT_RENDERER_UNAVAILABLE","DSH_DOCUMENT_RENDERER_UNAVAILABLE"),("DSH_DOCUMENT_RENDER_TIMEOUT","DSH_DOCUMENT_RENDER_TIMEOUT"),("private upstream details","DSH_UNAVAILABLE")] {
            let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint=format!("http://{}",listener.local_addr().unwrap());
            let worker=std::thread::spawn(move || {
                let (mut stream,_)=listener.accept().unwrap();let mut buf=[0u8;4096];let _=stream.read(&mut buf).unwrap();
                let body=json!({"ok":false,"error":code}).to_string();
                write!(stream,"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            });
            let got=forward_dsh_rpc_to_endpoint_with_target(&endpoint,None,"visible","local","app_dsh_preview",&json!({"path":"report.pdf"}));
            assert_eq!(got,Err(expected));worker.join().unwrap();
        }
        assert!(crate::agy_directory::preview_host_timeout_secs("report.PDF")>45+10+40);
        assert!(forwarded_rpc_timeout_secs("app_dsh_preview",&json!({"path":"slides.pptx"}))>110+20+20+30+20);
        assert_eq!(forwarded_rpc_timeout_secs("app_dsh_preview",&json!({"path":"photo.png"})),35);
        assert_eq!(forwarded_rpc_timeout_secs("app_dsh_history",&json!({"path":"report.pdf"})),35);
    }

    #[test]
    fn node_protocol_discovery_is_credential_free_and_fail_closed() {
        use std::io::{Read, Write};
        for (status, body, expected) in [
            ("200 OK", r#"{"protocol":"bob-node-v1"}"#, Ok(true)),
            ("401 Unauthorized", "{}", Ok(false)),
            ("404 Not Found", "{}", Ok(false)),
            ("503 Unavailable", "{}", Err("DSH_UNAVAILABLE")),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let count = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..count]).to_lowercase();
                assert!(request.starts_with("get /api/node/v1/info "));
                assert!(!request.contains("authorization:") && !request.contains("token="));
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            assert_eq!(is_bob_node_endpoint(&endpoint), expected);
            worker.join().unwrap();
        }
        assert_eq!(is_bob_node_endpoint("https://user:secret@example.invalid"), Err("DSH_ADDRESS_INVALID"));
    }

    #[test]
    fn node_pairing_invitation_roundtrips_compact_uri_and_base64() {
        let now = 1_793_720_000_000i64;
        let inv = NodePairingInvitation {
            v: 1,
            role: "node".to_owned(),
            node_id: "node-vps3".to_owned(),
            label: "VPS3 Worker".to_owned(),
            base_url: "http://100.64.0.3:53020".to_owned(),
            pairing_id: "pair_1".to_owned(),
            user_code: "654321".to_owned(),
            expires_at: now + 600_000,
            harnesses: vec!["claude".to_owned(), "pi".to_owned(), "codex".to_owned()],
        };
        let uri = inv.encode_qr_code();
        assert!(is_node_pairing_code(&uri));
        let decoded = NodePairingInvitation::decode_qr_code(&uri, now).unwrap();
        assert_eq!(decoded, inv);

        let b64 = inv.encode_base64_code().unwrap();
        assert!(is_node_pairing_code(&b64));
        let decoded_b64 = NodePairingInvitation::decode_qr_code(&b64, now).unwrap();
        assert_eq!(decoded_b64, inv);

        // Expired invitation fails closed
        assert_eq!(
            NodePairingInvitation::decode_qr_code(&uri, now + 600_001),
            Err("PAIRING_INVITATION_EXPIRED")
        );
    }

    #[test]
    fn terminal_qr_renders_valid_compact_matrix() {
        let sample = "bob://pair?u=http%3A%2F%2F100.64.0.3%3A53020&c=654321&p=pair_1&n=VPS3&r=node&x=1793720600000";
        let rendered = render_terminal_qr(sample).unwrap();
        let lines: Vec<&str> = rendered.lines().collect();
        assert!(lines.len() >= 15 && lines.len() <= 35);
        assert!(rendered.contains('█') && rendered.contains('▀'));
    }

    #[test]
    fn topology_does_not_share_the_hosts_loopback_alias() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = LocalApp::open(&dir.path().join("kernel.sqlite")).unwrap();
        let runtime = ModelHubRuntime::with_file_vault(dir.path().to_path_buf()).unwrap();
        let alias = json!({"id":"local","url":"http://[::1]:53020","name":"Host only"});
        let remote = json!({"id":"remote","url":"https://example.invalid","name":"Remote"});
        let mut hub = runtime.load_hub_state();
        hub["remoteNodes"] = json!([alias.clone(), remote.clone()]);
        runtime.save_hub_state(&hub).unwrap();
        let exported = export_local_topology_with_app(dir.path(), Some(&app), &runtime, true);
        assert_eq!(exported["remoteNodes"].as_array().unwrap().len(), 1);
        assert_eq!(exported["remoteNodes"][0]["id"], "remote");
        assert_eq!(runtime.load_hub_state()["remoteNodes"].as_array().unwrap().len(), 2);

        hub["remoteNodes"] = json!([]);
        runtime.save_hub_state(&hub).unwrap();
        merge_topology(dir.path(), &mut app, &runtime, &json!({"remoteNodes":[alias,remote]})).unwrap();
        assert_eq!(runtime.load_hub_state()["remoteNodes"][0]["id"], "remote");
        assert_eq!(runtime.load_hub_state()["remoteNodes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn bidirectional_merge_topology_syncs_tokens_harnesses_projects_and_knowledge() {
        let base_tmp = std::env::temp_dir().join(format!("bob-topo-sync-{}", now_millis()));
        let host_dir = base_tmp.join("host");
        let phone_dir = base_tmp.join("phone");
        let _ = fs::create_dir_all(&host_dir);
        let _ = fs::create_dir_all(&phone_dir);

        let mut host_app = LocalApp::open(&host_dir.join("bob.db")).unwrap();
        let host_rt = ModelHubRuntime::with_file_vault(host_dir.clone()).unwrap();
        host_rt
            .save_remote_node(
                &mut host_app,
                Some("node-x1t".to_owned()),
                "X1T",
                "https://x1t-dsh.bobbik.org",
                Some("x1t-secret-token"),
                false,
                Some("node"),
                true,
            )
            .unwrap();
        save_device_harnesses_raw(
            &host_dir,
            &[json!({
                "connectionId": "node-x1t",
                "label": "X1T",
                "sshTarget": "x1t",
                "enabledHarnesses": ["dsh", "agy", "claude"],
            })],
        )
        .unwrap();
        host_app
            .create_project("proj-winter", "Winter Shelter", 1000)
            .unwrap();
        host_app
            .set_project_session_link(
                "node-x1t",
                "claude",
                "claude-jsonl:7863969d",
                Some("proj-winter"),
                0,
                1000,
            )
            .unwrap();
        host_app
            .upsert_note(
                "note-1",
                Some("proj-winter"),
                None,
                "Architecture Wiki",
                "# Shared Knowledge",
                &["wiki".to_owned()],
                1000,
            )
            .unwrap();

        let mut phone_app = LocalApp::open(&phone_dir.join("bob.db")).unwrap();
        let phone_rt = ModelHubRuntime::with_file_vault(phone_dir.clone()).unwrap();
        phone_app
            .upsert_note(
                "note-phone",
                None,
                None,
                "Phone Memo",
                "Captured on mobile",
                &["memo".to_owned()],
                1100,
            )
            .unwrap();

        // Phone exports its local topology + knowledge and sends to Host
        let phone_payload =
            export_local_topology_with_app(&phone_dir, Some(&phone_app), &phone_rt, true);
        let host_merged_reply =
            merge_topology(&host_dir, &mut host_app, &host_rt, &phone_payload).unwrap();

        // Host now has Phone's note
        let host_notes = host_app.list_notes(None).unwrap();
        assert!(host_notes.iter().any(|n| n.id == "note-phone"));

        // Phone merges Host's reply
        let _ = merge_topology(&phone_dir, &mut phone_app, &phone_rt, &host_merged_reply).unwrap();

        // Phone now has X1T's secret token, harness grant (both camelCase and snake_case), project, session link, and wiki note!
        let phone_x1t_tok = phone_rt
            .secrets
            .get(&remote_node_token_slot("node-x1t"))
            .unwrap();
        assert_eq!(phone_x1t_tok, "x1t-secret-token");

        let phone_harnesses = load_device_harnesses_raw(&phone_dir);
        assert!(phone_harnesses.iter().any(|h| {
            h.get("connectionId").and_then(Value::as_str) == Some("node-x1t")
                && h.get("connection_id").and_then(Value::as_str) == Some("node-x1t")
        }));

        let phone_links_val = phone_app.project_session_links().unwrap();
        let phone_links = phone_links_val["links"].as_array().unwrap();
        assert_eq!(phone_links.len(), 1);
        assert_eq!(
            phone_links[0].get("project_id").and_then(Value::as_str),
            Some("proj-winter")
        );
        assert_eq!(
            phone_links[0].get("session_id").and_then(Value::as_str),
            Some("claude-jsonl:7863969d")
        );

        let phone_notes = phone_app.list_notes(None).unwrap();
        assert!(phone_notes.iter().any(|n| n.id == "note-1"));

        let _ = fs::remove_dir_all(&base_tmp);
    }
}
