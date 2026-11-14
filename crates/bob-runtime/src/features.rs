//! Pure-Rust Zero-Node Feature Package Sandbox Host, 3-Minute Trial & 5-Version Rollback,
//! Custom Skills and Remote MCP Connector.
//!
//! Enforces:
//! - Zero Node.js runtime: 100% pure Rust pipeline execution.
//! - Sandboxed feature package store with per-package 5-version history.
//! - 3-minute trial sandbox (`status: "trial"`, `trial_expires_at = now + 180_000`).
//! - Automatic crash/restart & timeout & runtime error rollback (`reconcile_trials`).
//! - Storage quota: KV storage strictly <= 8192 UTF-8 bytes (`FEATURE_STORAGE_LIMIT_EXCEEDED`).
//! - Declarative safe routes: `static`, `http_json`, `http_text`, `storage_patch`.
//! - SSRF-safe public HTTP fetching through `crate::net_guard::fetch_public_text`.
//! - Custom skills store (`skills_store.json`, max 20, <= 4000 chars instruction).
//! - Remote HTTPS MCP connector store (`mcp_store.json`, strict https:// only, SettingsChange risk).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const MAX_FEATURE_STORAGE_BYTES: usize = 8192;
pub const TRIAL_DURATION_MILLIS: i64 = 3 * 60 * 1000; // 3 minutes
pub const MAX_FEATURE_VERSIONS: usize = 5;
pub const MAX_ACTIVE_SKILLS: usize = 20;
pub const MAX_SKILL_INSTRUCTION_CHARS: usize = 4000;
pub const SYSTEM_PROMPT_SKILLS_BUDGET_CHARS: usize = 1500;

pub const WIDGET_SANDBOX_PROMPT_SPEC: &str = "\
[Widget Sandbox Specification (feature_create)]\n\
1. Sandbox Isolation: Widgets run inside `<iframe sandbox=\"allow-scripts\" referrerpolicy=\"no-referrer\">`. NEVER reference external CDNs (`<script src=\"http...\">`, `<link href=\"http...\">`, or `@import url(\"http...\")`). All HTML, CSS, inline SVG icons, and vanilla JS MUST be 100% self-contained inline.\n\
2. Theme Adaptation & Visual Discipline: NEVER use hardcoded `#hex`, `rgb()`, or `hsl()` colors, and NEVER use decorative emoji. Exclusively style with host-injected CSS semantic variables: `var(--bg-primary)`, `var(--bg-secondary)`, `var(--surface)`, `var(--line)`, `var(--text-primary)`, `var(--text-secondary)`, `var(--accent-primary)`, `var(--color-success)`, `var(--color-warning)`, `var(--color-error)`.\n\
3. Zero-Flicker Host Bridge (`window.bob`): Render frame 0 synchronously from `window.bob.state` and `window.bob.storage`. Subscribe to live updates via `window.bob.onState(state => ...)` and `window.bob.onTheme(vars => ...)`. Persist widget state (`<= 8192` UTF-8 bytes) via `await window.bob.save(nextObj)` and invoke declarative backend routes via `await window.bob.call(routeName, payload)`.";

fn extract_html_attr_value<'a>(tag_body: &'a str, attr_name: &str) -> Option<&'a str> {
    let lower = tag_body.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let target = attr_name.as_bytes();
    let mut i = 0;
    while i + target.len() <= bytes.len() {
        let prev_ok = i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b'/' || bytes[i - 1] == b'"' || bytes[i - 1] == b'\'';
        if prev_ok && &bytes[i..i + target.len()] == target {
            let mut j = i + target.len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= bytes.len() {
                    return None;
                }
                let quote = bytes[j];
                if quote == b'"' || quote == b'\'' {
                    let start = j + 1;
                    let mut end = start;
                    while end < bytes.len() && bytes[end] != quote {
                        end += 1;
                    }
                    return tag_body.get(start..end);
                } else {
                    let start = j;
                    let mut end = start;
                    while end < bytes.len() && !bytes[end].is_ascii_whitespace() && bytes[end] != b'>' {
                        end += 1;
                    }
                    return tag_body.get(start..end);
                }
            }
        }
        i += 1;
    }
    None
}

fn is_external_http_ref(raw: &str) -> bool {
    let trimmed = raw.trim().to_ascii_lowercase();
    trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("//")
}

/// Deterministically validates sandboxed widget HTML before installation/upgrade.
/// Rejects external HTTP/HTTPS/protocol-relative `<script src=...>` and `<link href=...>`
/// as well as CSS `@import` external URLs.
pub fn validate_widget_sandbox_html(html: &str) -> Result<(), &'static str> {
    let bytes = html.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'<' {
            if html[idx..].starts_with("<!--") {
                if let Some(end_comment) = html[idx + 4..].find("-->") {
                    idx += 4 + end_comment + 3;
                    continue;
                } else {
                    break;
                }
            }
            let mut close = idx + 1;
            let mut in_quote: Option<u8> = None;
            while close < bytes.len() {
                let c = bytes[close];
                if let Some(q) = in_quote {
                    if c == q {
                        in_quote = None;
                    }
                } else if c == b'"' || c == b'\'' {
                    in_quote = Some(c);
                } else if c == b'>' {
                    break;
                }
                close += 1;
            }
            if let Some(tag_inner) = html.get(idx + 1..close) {
                let trimmed = tag_inner.trim_start();
                if !trimmed.starts_with('/') && !trimmed.starts_with('!') {
                    let tag_name_end = trimmed
                        .find(|c: char| c.is_ascii_whitespace() || c == '/')
                        .unwrap_or(trimmed.len());
                    let tag_name = trimmed[..tag_name_end].to_ascii_lowercase();
                    let attrs = &trimmed[tag_name_end..];
                    if tag_name == "script" {
                        if let Some(src) = extract_html_attr_value(attrs, "src") {
                            if is_external_http_ref(src) {
                                return Err("FEATURE_WIDGET_EXTERNAL_CDN_FORBIDDEN");
                            }
                        }
                    } else if tag_name == "link" {
                        if let Some(href) = extract_html_attr_value(attrs, "href") {
                            if is_external_http_ref(href) {
                                return Err("FEATURE_WIDGET_EXTERNAL_CDN_FORBIDDEN");
                            }
                        }
                    }
                }
            }
            idx = close.saturating_add(1);
        } else {
            idx += 1;
        }
    }

    let lower = html.to_ascii_lowercase();
    let mut search_pos = 0;
    while let Some(rel) = lower[search_pos..].find("@import") {
        let after = lower[search_pos + rel + 7..].trim_start();
        let target = if let Some(rest) = after.strip_prefix("url(") {
            rest.trim_start()
        } else {
            after
        };
        let unquoted = target.trim_start_matches(['"', '\'']);
        if is_external_http_ref(unquoted) {
            return Err("FEATURE_WIDGET_EXTERNAL_CDN_FORBIDDEN");
        }
        search_pos += rel + 7;
    }

    Ok(())
}

pub fn is_valid_feature_name(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 48 {
        return false;
    }
    let mut chars = trimmed.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' && c != '-' {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum FeatureRouteSpec {
    #[serde(rename = "static", alias = "template")]
    Static {
        #[serde(default)]
        template: Value,
        #[serde(default)]
        cache_result: Option<bool>,
    },
    #[serde(rename = "http_json", alias = "json")]
    HttpJson {
        url: String,
        #[serde(default)]
        json_pointer: Option<String>,
        #[serde(default)]
        field: Option<String>,
        #[serde(default)]
        cache_result: Option<bool>,
    },
    #[serde(rename = "http_text", alias = "text")]
    HttpText {
        url: String,
        #[serde(default)]
        cache_result: Option<bool>,
    },
    #[serde(rename = "storage_patch", alias = "patch")]
    StoragePatch {
        #[serde(default)]
        patch: Option<Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureToolSpec {
    pub name: String,
    pub description: String,
    #[serde(default = "default_tool_schema")]
    pub input_schema: Value,
    pub route: String,
    #[serde(default)]
    pub risk_level: Option<String>,
}

fn default_tool_schema() -> Value {
    json!({ "type": "object", "properties": {} })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureSnapshot {
    pub version: u32,
    pub title: String,
    pub description: String,
    pub status: String,
    pub enabled: bool,
    pub has_widget: bool,
    pub widget_html: Option<String>,
    pub cached_state: Value,
    pub storage: Value,
    pub routes: HashMap<String, FeatureRouteSpec>,
    #[serde(default)]
    pub tools: Vec<FeatureToolSpec>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeaturePackage {
    pub name: String,
    pub title: String,
    pub description: String,
    pub status: String, // "trial" | "active" | "disabled"
    pub enabled: bool,  // status != "disabled"
    pub version: u32,
    pub trial_expires_at: Option<i64>,
    pub has_widget: bool,
    pub widget_html: Option<String>,
    pub cached_state: Value,
    pub storage: Value,
    #[serde(default)]
    pub routes: HashMap<String, FeatureRouteSpec>,
    #[serde(default)]
    pub tools: Vec<FeatureToolSpec>,
    #[serde(default)]
    pub consecutive_errors: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_reason: Option<String>,
    #[serde(default)]
    pub versions: Vec<FeatureSnapshot>,
}

impl FeaturePackage {
    pub fn snapshot(&self) -> FeatureSnapshot {
        FeatureSnapshot {
            version: self.version,
            title: self.title.clone(),
            description: self.description.clone(),
            status: self.status.clone(),
            enabled: self.enabled,
            has_widget: self.has_widget,
            widget_html: self.widget_html.clone(),
            cached_state: self.cached_state.clone(),
            storage: self.storage.clone(),
            routes: self.routes.clone(),
            tools: self.tools.clone(),
            updated_at: self.updated_at,
        }
    }

    pub fn restore_from_snapshot(&mut self, snap: FeatureSnapshot, now_ms: i64) {
        self.version = snap.version;
        self.title = snap.title;
        self.description = snap.description;
        self.status = "active".to_string();
        self.enabled = snap.enabled;
        self.trial_expires_at = None;
        self.has_widget = snap.has_widget;
        self.widget_html = snap.widget_html;
        self.cached_state = snap.cached_state;
        self.storage = snap.storage;
        self.routes = snap.routes;
        self.tools = snap.tools;
        self.consecutive_errors = 0;
        self.last_error = None;
        self.updated_at = now_ms;
    }
}

pub type FeaturePackageManager = FeatureStore;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FeatureStoreData {
    pub packages: HashMap<String, FeaturePackage>,
}

#[derive(Debug, Clone)]
pub struct FeatureStore {
    file_path: PathBuf,
    data: FeatureStoreData,
}

impl FeatureStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        Self::open_at(data_dir, crate::now_millis())
    }

    pub fn open_at(data_dir: &Path, now: i64) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("features_store.json");

        let data = if file_path.is_file() {
            let content = std::fs::read_to_string(&file_path).map_err(|_| "FEATURES_READ_FAILED")?;
            serde_json::from_str::<FeatureStoreData>(&content).unwrap_or_default()
        } else {
            FeatureStoreData::default()
        };

        let mut store = Self { file_path, data };
        if store.reconcile_trials(now) {
            let _ = store.save_atomic();
        }
        Ok(store)
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "FEATURES_SERIALIZATION_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes()).map_err(|_| "FEATURES_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "FEATURES_WRITE_FAILED")?;
        Ok(())
    }

    pub fn reconcile_trials(&mut self, now_ms: i64) -> bool {
        let mut modified = false;
        let mut to_remove = Vec::new();

        for (name, pkg) in self.data.packages.iter_mut() {
            if pkg.status == "trial" {
                let expired = pkg.trial_expires_at.map(|exp| now_ms >= exp).unwrap_or(false);
                let had_error = pkg.consecutive_errors >= 1;

                if expired || had_error {
                    modified = true;
                    if let Some(snapshot) = pkg.versions.pop() {
                        pkg.restore_from_snapshot(snapshot, now_ms);
                        pkg.rollback_reason = Some(if had_error {
                            "TRIAL_ERROR_AUTO_ROLLBACK".to_string()
                        } else {
                            "TRIAL_EXPIRED".to_string()
                        });
                    } else {
                        to_remove.push(name.clone());
                    }
                }
            }
        }

        for name in to_remove {
            self.data.packages.remove(&name);
            modified = true;
        }

        modified
    }

    pub fn list(&mut self, now_ms: i64) -> Vec<FeaturePackage> {
        if self.reconcile_trials(now_ms) {
            let _ = self.save_atomic();
        }
        let mut list: Vec<FeaturePackage> = self.data.packages.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn get(&mut self, name: &str, now_ms: i64) -> Option<&FeaturePackage> {
        if self.reconcile_trials(now_ms) {
            let _ = self.save_atomic();
        }
        self.data.packages.get(name)
    }

    pub fn create_or_update(
        &mut self,
        name: &str,
        title: &str,
        description: &str,
        widget_html: Option<String>,
        routes: HashMap<String, FeatureRouteSpec>,
        tools: Vec<FeatureToolSpec>,
        initial_storage: Option<Value>,
        now_ms: i64,
    ) -> Result<FeaturePackage, &'static str> {
        if !is_valid_feature_name(name) {
            return Err("INVALID_FEATURE_NAME");
        }

        if let Some(ref html) = widget_html {
            if !html.trim().is_empty() {
                validate_widget_sandbox_html(html)?;
            }
        }

        let storage_val = initial_storage.unwrap_or_else(|| json!({}));
        let storage_str =
            serde_json::to_string(&storage_val).map_err(|_| "STORAGE_SERIALIZATION_FAILED")?;
        if storage_str.len() > MAX_FEATURE_STORAGE_BYTES {
            return Err("FEATURE_STORAGE_LIMIT_EXCEEDED");
        }

        let has_widget = widget_html
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let trial_expires_at = Some(now_ms + TRIAL_DURATION_MILLIS);

        if let Some(existing) = self.data.packages.get_mut(name) {
            // Snapshot current version before updating
            let snap = existing.snapshot();
            existing.versions.push(snap);
            if existing.versions.len() > MAX_FEATURE_VERSIONS {
                existing.versions.remove(0);
            }
            existing.version += 1;
            existing.title = title.to_string();
            existing.description = description.to_string();
            existing.status = "trial".to_string();
            existing.enabled = true;
            existing.trial_expires_at = trial_expires_at;
            existing.has_widget = has_widget;
            existing.widget_html = widget_html;
            existing.routes = routes;
            existing.tools = tools;
            existing.consecutive_errors = 0;
            existing.last_error = None;
            existing.updated_at = now_ms;
            existing.rollback_reason = None;
            if !storage_val.is_null() && storage_val != json!({}) {
                existing.storage = storage_val;
            }
            let res = existing.clone();
            self.save_atomic()?;
            Ok(res)
        } else {
            let pkg = FeaturePackage {
                name: name.to_string(),
                title: title.to_string(),
                description: description.to_string(),
                status: "trial".to_string(),
                enabled: true,
                version: 1,
                trial_expires_at,
                has_widget,
                widget_html,
                cached_state: json!({}),
                storage: storage_val,
                routes,
                tools,
                consecutive_errors: 0,
                last_error: None,
                updated_at: now_ms,
                rollback_reason: None,
                versions: Vec::new(),
            };
            self.data.packages.insert(name.to_string(), pkg.clone());
            self.save_atomic()?;
            Ok(pkg)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn install_or_upgrade(
        &mut self,
        name: &str,
        title: &str,
        description: &str,
        widget_html: Option<String>,
        routes: HashMap<String, FeatureRouteSpec>,
        tools: Vec<FeatureToolSpec>,
        initial_storage: Option<Value>,
        now_ms: i64,
    ) -> Result<FeaturePackage, &'static str> {
        self.create_or_update(
            name,
            title,
            description,
            widget_html,
            routes,
            tools,
            initial_storage,
            now_ms,
        )
    }

    pub fn keep_feature(&mut self, name: &str, now_ms: i64) -> Result<FeaturePackage, &'static str> {
        let pkg = self.data.packages.get_mut(name).ok_or("FEATURE_NOT_FOUND")?;
        pkg.status = "active".to_string();
        pkg.enabled = true;
        pkg.trial_expires_at = None;
        pkg.consecutive_errors = 0;
        pkg.last_error = None;
        pkg.updated_at = now_ms;
        let res = pkg.clone();
        self.save_atomic()?;
        Ok(res)
    }

    pub fn rollback_feature(
        &mut self,
        name: &str,
        target_version: Option<u32>,
        now_ms: i64,
    ) -> Result<Option<FeaturePackage>, &'static str> {
        let pkg = self.data.packages.get_mut(name).ok_or("FEATURE_NOT_FOUND")?;

        if let Some(target) = target_version {
            if let Some(pos) = pkg.versions.iter().position(|s| s.version == target) {
                let snap = pkg.versions.remove(pos);
                pkg.versions.truncate(pos);
                pkg.restore_from_snapshot(snap, now_ms);
                let res = pkg.clone();
                self.save_atomic()?;
                Ok(Some(res))
            } else if pkg.version == target {
                Ok(Some(pkg.clone()))
            } else {
                Err("TARGET_VERSION_NOT_FOUND")
            }
        } else if let Some(snap) = pkg.versions.pop() {
            pkg.restore_from_snapshot(snap, now_ms);
            let res = pkg.clone();
            self.save_atomic()?;
            Ok(Some(res))
        } else {
            self.data.packages.remove(name);
            self.save_atomic()?;
            Ok(None)
        }
    }

    pub fn toggle_feature(
        &mut self,
        name: &str,
        enabled: bool,
        now_ms: i64,
    ) -> Result<FeaturePackage, &'static str> {
        let pkg = self.data.packages.get_mut(name).ok_or("FEATURE_NOT_FOUND")?;
        pkg.enabled = enabled;
        if !enabled {
            pkg.status = "disabled".to_string();
        } else if pkg.status == "disabled" {
            pkg.status = "active".to_string();
        }
        pkg.updated_at = now_ms;
        let res = pkg.clone();
        self.save_atomic()?;
        Ok(res)
    }

    pub fn save_storage(
        &mut self,
        name: &str,
        storage: Value,
        now_ms: i64,
    ) -> Result<Value, &'static str> {
        let raw = serde_json::to_string(&storage).map_err(|_| "STORAGE_SERIALIZATION_FAILED")?;
        if raw.len() > MAX_FEATURE_STORAGE_BYTES {
            return Err("FEATURE_STORAGE_LIMIT_EXCEEDED");
        }
        let pkg = self.data.packages.get_mut(name).ok_or("FEATURE_NOT_FOUND")?;
        pkg.storage = storage.clone();
        pkg.updated_at = now_ms;
        self.save_atomic()?;
        Ok(storage)
    }

    pub fn record_error_and_rollback_if_trial(&mut self, name: &str, now_ms: i64) -> bool {
        if let Some(pkg) = self.data.packages.get_mut(name) {
            pkg.consecutive_errors += 1;
            if pkg.status == "trial" {
                if let Some(snap) = pkg.versions.pop() {
                    pkg.restore_from_snapshot(snap, now_ms);
                    pkg.rollback_reason = Some("TRIAL_ERROR_AUTO_ROLLBACK".to_string());
                } else {
                    self.data.packages.remove(name);
                }
                let _ = self.save_atomic();
                return true;
            }
        }
        false
    }

    pub fn rollback(
        &mut self,
        name: &str,
        target_version: Option<u32>,
    ) -> Result<Option<FeaturePackage>, &'static str> {
        self.rollback_feature(name, target_version, crate::now_millis())
    }

    /// Records a frontend/sandbox widget runtime error (`last_error`) on `name`.
    /// If `name` is currently in `"trial"` status, automatically rolls back to the immediately
    /// preceding working version (or disables cleanly if v1) and returns recovery telemetry.
    pub fn report_runtime_error(
        &mut self,
        name: &str,
        error: &str,
        now_ms: i64,
    ) -> Result<Value, &'static str> {
        let pkg = self.data.packages.get_mut(name).ok_or("FEATURE_NOT_FOUND")?;
        let err_msg = error.trim();
        let clean_err = if err_msg.is_empty() {
            "WIDGET_RUNTIME_ERROR"
        } else {
            err_msg
        };

        if pkg.status == "trial" {
            if let Some(snap) = pkg.versions.pop() {
                pkg.restore_from_snapshot(snap, now_ms);
                pkg.last_error = Some(clean_err.to_string());
                pkg.rollback_reason = Some("TRIAL_RUNTIME_ERROR_AUTO_ROLLBACK".to_string());
                let restored_version = pkg.version;
                let restored_status = pkg.status.clone();
                self.save_atomic()?;
                Ok(json!({
                    "name": name,
                    "recovered": true,
                    "action": "rolled_back",
                    "version": restored_version,
                    "status": restored_status,
                    "error": clean_err,
                }))
            } else {
                pkg.status = "disabled".to_string();
                pkg.enabled = false;
                pkg.trial_expires_at = None;
                pkg.consecutive_errors += 1;
                pkg.last_error = Some(clean_err.to_string());
                pkg.rollback_reason = Some("TRIAL_RUNTIME_ERROR_AUTO_DISABLED".to_string());
                pkg.updated_at = now_ms;
                let disabled_version = pkg.version;
                self.save_atomic()?;
                Ok(json!({
                    "name": name,
                    "recovered": true,
                    "action": "rolled_back",
                    "version": disabled_version,
                    "status": "disabled",
                    "error": clean_err,
                }))
            }
        } else {
            pkg.consecutive_errors += 1;
            pkg.last_error = Some(clean_err.to_string());
            pkg.updated_at = now_ms;
            let ver = pkg.version;
            let st = pkg.status.clone();
            self.save_atomic()?;
            Ok(json!({
                "name": name,
                "recovered": true,
                "action": "recorded",
                "version": ver,
                "status": st,
                "error": clean_err,
            }))
        }
    }
}

pub fn execute_feature_call(data_dir: &Path, args: &Value) -> Result<Value, &'static str> {
    let now = args
        .get("now")
        .and_then(Value::as_i64)
        .unwrap_or_else(crate::now_millis);
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or("FEATURE_NAME_REQUIRED")?;
    let route = args
        .get("route")
        .and_then(Value::as_str)
        .ok_or("FEATURE_ROUTE_REQUIRED")?;
    let payload = args.get("payload").cloned().unwrap_or_else(|| json!({}));

    let (route_spec, is_trial, mut storage, mut cached_state) = {
        let mut store = FeatureStore::open(data_dir)?;
        let pkg = store.get(name, now).ok_or("FEATURE_NOT_FOUND")?;
        if !pkg.enabled || pkg.status == "disabled" {
            return Err("FEATURE_DISABLED");
        }
        let rspec = pkg
            .routes
            .get(route)
            .ok_or("FEATURE_ROUTE_NOT_FOUND")?
            .clone();
        (
            rspec,
            pkg.status == "trial",
            pkg.storage.clone(),
            pkg.cached_state.clone(),
        )
    };

    let res = match route_spec {
        FeatureRouteSpec::Static {
            ref template,
            cache_result,
        } => {
            let mut out = template.clone();
            if let Some(obj) = out.as_object_mut() {
                if let Some(p_obj) = payload.as_object() {
                    for (k, v) in p_obj {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
            if cache_result.unwrap_or(true) {
                cached_state = out.clone();
            }
            Ok((out, cached_state, storage))
        }
        FeatureRouteSpec::HttpJson {
            ref url,
            ref json_pointer,
            ref field,
            cache_result,
        } => {
            let mut final_url = url.clone();
            if let Some(obj) = payload.as_object() {
                for (k, v) in obj {
                    let placeholder = format!("{{{k}}}");
                    if final_url.contains(&placeholder) {
                        let val_str = if let Some(s) = v.as_str() {
                            s.to_string()
                        } else {
                            v.to_string()
                        };
                        final_url = final_url.replace(&placeholder, &val_str);
                    }
                }
            }

            let fetch_res = crate::net_guard::fetch_public_text(&final_url, 64 * 1024, 5_000);
            match fetch_res {
                Ok(resp) => {
                    let parsed: Value =
                        serde_json::from_str(&resp.text).map_err(|_| "INVALID_JSON_RESPONSE")?;
                    let data = if let Some(ptr) = json_pointer {
                        parsed.pointer(ptr).cloned().unwrap_or(Value::Null)
                    } else if let Some(f) = field {
                        parsed.get(f).cloned().unwrap_or(Value::Null)
                    } else {
                        parsed
                    };
                    if cache_result.unwrap_or(true) {
                        cached_state = data.clone();
                    }
                    Ok((data, cached_state, storage))
                }
                Err(err) => Err(err),
            }
        }
        FeatureRouteSpec::HttpText {
            ref url,
            cache_result,
        } => {
            let mut final_url = url.clone();
            if let Some(obj) = payload.as_object() {
                for (k, v) in obj {
                    let placeholder = format!("{{{k}}}");
                    if final_url.contains(&placeholder) {
                        let val_str = if let Some(s) = v.as_str() {
                            s.to_string()
                        } else {
                            v.to_string()
                        };
                        final_url = final_url.replace(&placeholder, &val_str);
                    }
                }
            }

            let fetch_res = crate::net_guard::fetch_public_text(&final_url, 64 * 1024, 5_000);
            match fetch_res {
                Ok(resp) => {
                    let data = json!({ "text": resp.text, "status": resp.status });
                    if cache_result.unwrap_or(true) {
                        cached_state = data.clone();
                    }
                    Ok((data, cached_state, storage))
                }
                Err(err) => Err(err),
            }
        }
        FeatureRouteSpec::StoragePatch { ref patch } => {
            let patch_val = patch.as_ref().unwrap_or(&payload);
            if let Some(obj) = patch_val.as_object() {
                if let Some(target) = storage.as_object_mut() {
                    for (k, v) in obj {
                        target.insert(k.clone(), v.clone());
                    }
                } else {
                    storage = patch_val.clone();
                }
            }
            let raw =
                serde_json::to_string(&storage).map_err(|_| "STORAGE_SERIALIZATION_FAILED")?;
            if raw.len() > MAX_FEATURE_STORAGE_BYTES {
                Err("FEATURE_STORAGE_LIMIT_EXCEEDED")
            } else {
                cached_state = storage.clone();
                Ok((storage.clone(), cached_state, storage))
            }
        }
    };

    match res {
        Ok((data, new_cached, new_storage)) => {
            let mut store = FeatureStore::open(data_dir)?;
            if let Some(pkg) = store.data.packages.get_mut(name) {
                pkg.cached_state = new_cached.clone();
                pkg.storage = new_storage;
                pkg.consecutive_errors = 0;
                pkg.updated_at = now;
                let _ = store.save_atomic();
            }
            Ok(json!({
                "ok": true,
                "data": data,
                "cached_state": new_cached,
            }))
        }
        Err(err) => {
            if is_trial {
                let mut store = FeatureStore::open(data_dir)?;
                store.record_error_and_rollback_if_trial(name, now);
            }
            Err(err)
        }
    }
}

// ---------------------------------------------------------------------------
// Custom Skills System
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomSkill {
    pub name: String,
    pub title: String,
    pub instructions: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SkillsStoreData {
    pub skills: HashMap<String, CustomSkill>,
}

#[derive(Debug, Clone)]
pub struct SkillStore {
    file_path: PathBuf,
    data: SkillsStoreData,
}

impl SkillStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("skills_store.json");
        let data = if file_path.is_file() {
            let content = std::fs::read_to_string(&file_path).map_err(|_| "SKILLS_READ_FAILED")?;
            serde_json::from_str::<SkillsStoreData>(&content).unwrap_or_default()
        } else {
            SkillsStoreData::default()
        };
        Ok(Self { file_path, data })
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "SKILLS_SERIALIZATION_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes()).map_err(|_| "SKILLS_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "SKILLS_WRITE_FAILED")?;
        Ok(())
    }

    pub fn create_or_update(
        &mut self,
        name: &str,
        title: &str,
        instructions: &str,
        triggers: Vec<String>,
        now_ms: i64,
    ) -> Result<CustomSkill, &'static str> {
        if name.trim().is_empty() {
            return Err("SKILL_NAME_REQUIRED");
        }
        if instructions.chars().count() > MAX_SKILL_INSTRUCTION_CHARS {
            return Err("SKILL_INSTRUCTIONS_TOO_LONG");
        }

        if !self.data.skills.contains_key(name) {
            let active_count = self.data.skills.values().filter(|s| s.enabled).count();
            if active_count >= MAX_ACTIVE_SKILLS {
                return Err("MAX_SKILLS_EXCEEDED");
            }
        }

        let skill = CustomSkill {
            name: name.to_string(),
            title: title.to_string(),
            instructions: instructions.to_string(),
            triggers,
            enabled: true,
            created_at: self
                .data
                .skills
                .get(name)
                .map(|s| s.created_at)
                .unwrap_or(now_ms),
            updated_at: now_ms,
        };
        self.data.skills.insert(name.to_string(), skill.clone());
        self.save_atomic()?;
        Ok(skill)
    }

    pub fn list(&self) -> Vec<CustomSkill> {
        let mut list: Vec<CustomSkill> = self.data.skills.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn list_active(&self) -> Vec<CustomSkill> {
        let mut list: Vec<CustomSkill> = self
            .data
            .skills
            .values()
            .filter(|s| s.enabled)
            .cloned()
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

// ---------------------------------------------------------------------------
// Remote HTTPS MCP Connector Store
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct McpStoreData {
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone)]
pub struct McpStore {
    file_path: PathBuf,
    data: McpStoreData,
}

impl McpStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("mcp_store.json");
        let data = if file_path.is_file() {
            let content = std::fs::read_to_string(&file_path).map_err(|_| "MCP_READ_FAILED")?;
            serde_json::from_str::<McpStoreData>(&content).unwrap_or_default()
        } else {
            McpStoreData::default()
        };
        Ok(Self { file_path, data })
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "MCP_SERIALIZATION_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes()).map_err(|_| "MCP_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "MCP_WRITE_FAILED")?;
        Ok(())
    }

    pub fn add_server(
        &mut self,
        name: &str,
        raw_url: &str,
        description: Option<String>,
        now_ms: i64,
    ) -> Result<McpServerConfig, &'static str> {
        if name.trim().is_empty() {
            return Err("MCP_NAME_REQUIRED");
        }
        let validated = crate::net_guard::validate_public_http_url(raw_url)?;
        if validated.scheme != "https" {
            return Err("HTTPS_REQUIRED");
        }
        let _ =
            crate::net_guard::resolve_and_verify_public_host(&validated.host, validated.port)?;

        let srv = McpServerConfig {
            name: name.to_string(),
            url: validated.url,
            description,
            enabled: true,
            created_at: now_ms,
            updated_at: now_ms,
        };
        self.data.servers.insert(name.to_string(), srv.clone());
        self.save_atomic()?;
        Ok(srv)
    }

    pub fn list(&self) -> Vec<McpServerConfig> {
        let mut list: Vec<McpServerConfig> = self.data.servers.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

// ---------------------------------------------------------------------------
// Unit & Integration Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_bus::{ActionRiskLevel, BobToolBus};
    use tempfile::tempdir;

    #[test]
    fn test_feature_create_and_trial_expiry() {
        let dir = tempdir().unwrap();
        let mut store = FeatureStore::open(dir.path()).unwrap();
        let now = 1_000_000i64;

        let pkg = store
            .create_or_update(
                "pomodoro_timer",
                "Pomodoro Timer",
                "A clean 25m productivity timer",
                Some("<div id='timer'>25:00</div>".to_string()),
                HashMap::new(),
                Vec::new(),
                Some(json!({ "work_mins": 25 })),
                now,
            )
            .unwrap();

        assert_eq!(pkg.name, "pomodoro_timer");
        assert_eq!(pkg.status, "trial");
        assert!(pkg.enabled);
        assert_eq!(pkg.version, 1);
        assert_eq!(pkg.trial_expires_at, Some(now + 180_000));
        assert!(pkg.has_widget);
        assert_eq!(
            pkg.widget_html,
            Some("<div id='timer'>25:00</div>".to_string())
        );
        assert_eq!(pkg.storage["work_mins"], 25);

        // list() returns the package
        let list = store.list(now);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "pomodoro_timer");

        // keep_feature transitions to active
        let kept = store.keep_feature("pomodoro_timer", now + 10_000).unwrap();
        assert_eq!(kept.status, "active");
        assert_eq!(kept.trial_expires_at, None);
        assert!(kept.enabled);
    }

    #[test]
    fn test_updating_feature_retains_max_5_versions_and_rollbacks_step_by_step() {
        let dir = tempdir().unwrap();
        let mut store = FeatureStore::open(dir.path()).unwrap();
        let mut now = 1_000_000i64;

        // 1. Initial create: v1
        store
            .create_or_update(
                "weather_widget",
                "Weather v1",
                "Initial version",
                Some("<h1>Weather v1</h1>".to_string()),
                HashMap::new(),
                Vec::new(),
                None,
                now,
            )
            .unwrap();
        store.keep_feature("weather_widget", now).unwrap();

        // 2. Update 6 times (v2..v7)
        for i in 2..=7 {
            now += 10_000;
            let updated = store
                .create_or_update(
                    "weather_widget",
                    &format!("Weather v{i}"),
                    &format!("Updated version {i}"),
                    Some(format!("<h1>Weather v{i}</h1>")),
                    HashMap::new(),
                    Vec::new(),
                    None,
                    now,
                )
                .unwrap();
            assert_eq!(updated.version, i);
            store.keep_feature("weather_widget", now).unwrap();
        }

        let pkg = store.get("weather_widget", now).unwrap();
        assert_eq!(pkg.version, 7);
        // At most 5 historical snapshots are kept!
        assert_eq!(pkg.versions.len(), 5);
        let snapshot_versions: Vec<u32> = pkg.versions.iter().map(|s| s.version).collect();
        assert_eq!(snapshot_versions, vec![2, 3, 4, 5, 6]);

        // 3. Rollback step-by-step
        // Rollback 1: restores v6
        now += 10_000;
        let r1 = store
            .rollback_feature("weather_widget", None, now)
            .unwrap()
            .unwrap();
        assert_eq!(r1.version, 6);
        assert_eq!(r1.title, "Weather v6");

        // Rollback 2: restores v5
        now += 10_000;
        let r2 = store
            .rollback_feature("weather_widget", None, now)
            .unwrap()
            .unwrap();
        assert_eq!(r2.version, 5);

        // Rollback 3: restores v4
        now += 10_000;
        let r3 = store
            .rollback_feature("weather_widget", None, now)
            .unwrap()
            .unwrap();
        assert_eq!(r3.version, 4);

        // Rollback 4: restores v3
        now += 10_000;
        let r4 = store
            .rollback_feature("weather_widget", None, now)
            .unwrap()
            .unwrap();
        assert_eq!(r4.version, 3);

        // Rollback 5: restores v2
        now += 10_000;
        let r5 = store
            .rollback_feature("weather_widget", None, now)
            .unwrap()
            .unwrap();
        assert_eq!(r5.version, 2);

        // Rollback 6: no snapshots remain -> removes/disables package
        now += 10_000;
        let r6 = store
            .rollback_feature("weather_widget", None, now)
            .unwrap();
        assert!(r6.is_none());
        assert!(store.get("weather_widget", now).is_none());
    }

    #[test]
    fn test_trial_auto_rollback_timeout_restart_and_runtime_error() {
        let dir = tempdir().unwrap();
        let now = 1_000_000i64;

        // A. Trial timeout auto-rollback
        {
            let mut store = FeatureStore::open(dir.path()).unwrap();
            store
                .create_or_update(
                    "water_counter",
                    "Water Counter",
                    "Count glasses of water",
                    Some("<div>0 glasses</div>".to_string()),
                    HashMap::new(),
                    Vec::new(),
                    None,
                    now,
                )
                .unwrap();

            // At now + 179_000: still in trial
            assert!(store.get("water_counter", now + 179_000).is_some());
            assert_eq!(
                store.get("water_counter", now + 179_000).unwrap().status,
                "trial"
            );

            // At now + 181_000: trial expired -> auto-rollback!
            let expired_list = store.list(now + 181_000);
            assert!(expired_list.is_empty());
            assert!(store.get("water_counter", now + 181_000).is_none());
        }

        // B. Simulated server restart mid-trial:
        {
            let mut store1 = FeatureStore::open(dir.path()).unwrap();
            store1
                .create_or_update(
                    "habit_tracker",
                    "Habit Tracker",
                    "Daily habits",
                    Some("<div>Habits</div>".to_string()),
                    HashMap::new(),
                    Vec::new(),
                    None,
                    now,
                )
                .unwrap();
            assert!(store1.get("habit_tracker", now).is_some());
            // drop store1 to simulate node process exit
        }

        // Advance clock past trial expiration and restart node: FeatureStore::open automatically reconciles!
        {
            let mut store2 = FeatureStore::open(dir.path()).unwrap();
            let after_restart_list = store2.list(now + 200_000);
            assert!(after_restart_list.is_empty());
            assert!(store2.get("habit_tracker", now + 200_000).is_none());
        }

        // C. Runtime error during trial auto-rolls back:
        {
            let now = crate::now_millis();
            let mut store3 = FeatureStore::open(dir.path()).unwrap();
            let mut routes = HashMap::new();
            routes.insert(
                "failing_step".to_string(),
                FeatureRouteSpec::HttpJson {
                    url: "http://127.0.0.1:8787/blocked".to_string(),
                    json_pointer: None,
                    field: None,
                    cache_result: Some(true),
                },
            );

            store3
                .create_or_update(
                    "fragile_tool",
                    "Fragile Tool",
                    "A tool that errors",
                    None,
                    routes,
                    Vec::new(),
                    None,
                    now,
                )
                .unwrap();

            // Calling failing route triggers SSRF error and auto-rollback!
            let call_res = execute_feature_call(
                dir.path(),
                &json!({
                    "name": "fragile_tool",
                    "route": "failing_step",
                    "now": now,
                }),
            );
            assert!(call_res.is_err());

            // Check store: package was auto-rolled back because error occurred in trial!
            let mut check_store = FeatureStore::open(dir.path()).unwrap();
            assert!(check_store.get("fragile_tool", now).is_none());
        }
    }

    #[test]
    fn test_ssrf_protection_inside_feature_route() {
        let dir = tempdir().unwrap();
        let now = crate::now_millis();
        let mut store = FeatureStore::open(dir.path()).unwrap();

        // 1. Loopback attack
        let mut routes = HashMap::new();
        routes.insert(
            "attack_loopback".to_string(),
            FeatureRouteSpec::HttpJson {
                url: "http://127.0.0.1:8787/api/capabilities".to_string(),
                json_pointer: None,
                field: None,
                cache_result: None,
            },
        );
        routes.insert(
            "attack_metadata".to_string(),
            FeatureRouteSpec::HttpJson {
                url: "http://169.254.169.254/latest/meta-data".to_string(),
                json_pointer: None,
                field: None,
                cache_result: None,
            },
        );

        store
            .create_or_update(
                "ssrf_test_pkg",
                "SSRF Test",
                "Testing SSRF blocks",
                None,
                routes,
                Vec::new(),
                None,
                now,
            )
            .unwrap();

        // Attempt loopback
        let res1 = execute_feature_call(
            dir.path(),
            &json!({
                "name": "ssrf_test_pkg",
                "route": "attack_loopback",
                "now": now,
            }),
        );
        assert!(res1.is_err());
        assert_eq!(res1.unwrap_err(), "BLOCKED_PRIVATE_IP");

        // Package was auto-rolled back due to error in trial!
        let mut check_store = FeatureStore::open(dir.path()).unwrap();
        assert!(check_store.get("ssrf_test_pkg", now).is_none());

        // 2. Metadata attack
        let mut store2 = FeatureStore::open(dir.path()).unwrap();
        let mut routes2 = HashMap::new();
        routes2.insert(
            "attack_metadata".to_string(),
            FeatureRouteSpec::HttpJson {
                url: "http://169.254.169.254/latest/meta-data".to_string(),
                json_pointer: None,
                field: None,
                cache_result: None,
            },
        );
        store2
            .create_or_update(
                "meta_test_pkg",
                "Metadata Test",
                "Testing Metadata SSRF",
                None,
                routes2,
                Vec::new(),
                None,
                now,
            )
            .unwrap();

        let res2 = execute_feature_call(
            dir.path(),
            &json!({
                "name": "meta_test_pkg",
                "route": "attack_metadata",
                "now": now,
            }),
        );
        assert!(res2.is_err());
        assert_eq!(res2.unwrap_err(), "BLOCKED_PRIVATE_IP");
        let mut check_store2 = FeatureStore::open(dir.path()).unwrap();
        assert!(check_store2.get("meta_test_pkg", now).is_none());
    }

    #[test]
    fn test_storage_quota_enforcement() {
        let dir = tempdir().unwrap();
        let now = 1_000_000i64;
        let mut store = FeatureStore::open(dir.path()).unwrap();

        // Under 8192 bytes: succeeds
        let small_storage = json!({ "key": "a".repeat(100) });
        let pkg = store
            .create_or_update(
                "storage_test",
                "Storage Test",
                "Storage limit test",
                None,
                HashMap::new(),
                Vec::new(),
                Some(small_storage),
                now,
            )
            .unwrap();
        assert_eq!(pkg.name, "storage_test");

        // Updating storage under limit: succeeds
        let valid_storage = json!({ "data": "b".repeat(8000) });
        let save_ok = store.save_storage("storage_test", valid_storage, now);
        assert!(save_ok.is_ok());

        // Updating storage over 8192 bytes: fails with FEATURE_STORAGE_LIMIT_EXCEEDED
        let oversize_storage = json!({ "data": "c".repeat(8500) });
        let save_err = store.save_storage("storage_test", oversize_storage, now);
        assert_eq!(save_err.unwrap_err(), "FEATURE_STORAGE_LIMIT_EXCEEDED");
    }

    #[test]
    fn test_skills_store_prompt_injection_and_mcp_validation() {
        let dir = tempdir().unwrap();
        let now = 1_000_000i64;

        // 1. SkillStore
        let mut skill_store = SkillStore::open(dir.path()).unwrap();
        let skill = skill_store
            .create_or_update(
                "weekly_report",
                "Weekly Standup Report",
                "Always summarize tasks into 3 bullets: Done, Ongoing, Blockers.",
                vec!["weekly report".to_string(), "standup".to_string()],
                now,
            )
            .unwrap();
        assert_eq!(skill.name, "weekly_report");
        assert_eq!(skill_store.list().len(), 1);

        // 2. McpStore
        let mut mcp_store = McpStore::open(dir.path()).unwrap();

        // HTTP is rejected (HTTPS required)
        let http_err = mcp_store.add_server(
            "insecure_srv",
            "http://example.com/mcp",
            None,
            now,
        );
        assert_eq!(http_err.unwrap_err(), "HTTPS_REQUIRED");

        // Loopback HTTPS is rejected
        let loopback_err = mcp_store.add_server(
            "loopback_srv",
            "https://127.0.0.1:8443/mcp",
            None,
            now,
        );
        assert_eq!(loopback_err.unwrap_err(), "BLOCKED_PRIVATE_IP");

        // 3. BobToolBus registers tools with correct risk levels
        let bus = BobToolBus::with_approval_store(dir.path()).unwrap();
        let mcp_add_spec = bus.get_spec("mcp_add").unwrap();
        assert_eq!(mcp_add_spec.risk_level, ActionRiskLevel::SettingsChange);

        let feat_create_spec = bus.get_spec("feature_create").unwrap();
        assert_eq!(feat_create_spec.risk_level, ActionRiskLevel::ReadOnly);

        let skill_create_spec = bus.get_spec("skill_create").unwrap();
        assert_eq!(skill_create_spec.risk_level, ActionRiskLevel::ReadOnly);
    }

    #[test]
    fn test_wp_e1_4_office_widgets_trial_refinement_runtime_error_rollback_5v_cap_and_cdn_guard() {
        let dir = tempdir().unwrap();
        let mut now = 2_000_000i64;
        let mut mgr = FeaturePackageManager::open(dir.path()).unwrap();

        // 1. Realistic office widget #1: daily_focus_matrix (CSS variables + window.bob.save/call)
        let focus_v1_html = r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { background: var(--bg-primary); color: var(--text-primary); border: 1px solid var(--line); }
  .quadrant { background: var(--surface); color: var(--text-secondary); }
  button.primary { background: var(--accent-primary); color: var(--bg-primary); }
</style>
</head>
<body>
  <div id="matrix" class="quadrant"></div>
  <script>
    const initial = window.bob.storage || { urgent_important: [] };
    window.bob.onTheme(vars => { document.body.dataset.themeReady = "1"; });
    window.bob.onState(st => { document.getElementById("matrix").textContent = JSON.stringify(st); });
    async function addItem(text) {
      const next = { urgent_important: [...(initial.urgent_important || []), text] };
      await window.bob.save(next);
      await window.bob.call("sync_focus", next);
    }
  </script>
</body>
</html>"#;

        let mut focus_routes = HashMap::new();
        focus_routes.insert(
            "sync_focus".to_string(),
            FeatureRouteSpec::StoragePatch { patch: None },
        );

        let v1 = mgr
            .install_or_upgrade(
                "daily_focus_matrix",
                "Daily Focus Matrix",
                "Eisenhower quadrant planner v1",
                Some(focus_v1_html.to_string()),
                focus_routes.clone(),
                Vec::new(),
                Some(json!({ "urgent_important": ["Prepare board deck"] })),
                now,
            )
            .unwrap();
        assert_eq!(v1.version, 1);
        assert_eq!(v1.status, "trial");

        // v2 refinement within 3-minute trial window
        now += 30_000;
        let focus_v2_html = focus_v1_html.replace("quadrant", "quadrant-v2");
        let v2 = mgr
            .install_or_upgrade(
                "daily_focus_matrix",
                "Daily Focus Matrix v2",
                "Added keyboard shortcut support",
                Some(focus_v2_html.clone()),
                focus_routes.clone(),
                Vec::new(),
                None,
                now,
            )
            .unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.status, "trial");

        // Confirm Keep on v2
        now += 15_000;
        let kept_v2 = mgr.keep_feature("daily_focus_matrix", now).unwrap();
        assert_eq!(kept_v2.version, 2);
        assert_eq!(kept_v2.status, "active");

        // v3 buggy update enters trial, then frontend reports runtime error -> auto-rollback to v2!
        now += 20_000;
        let focus_v3_buggy = focus_v2_html.replace("addItem", "brokenReference()");
        let v3 = mgr
            .install_or_upgrade(
                "daily_focus_matrix",
                "Daily Focus Matrix v3",
                "Experimental drag handler",
                Some(focus_v3_buggy),
                focus_routes.clone(),
                Vec::new(),
                None,
                now,
            )
            .unwrap();
        assert_eq!(v3.version, 3);
        assert_eq!(v3.status, "trial");

        let recovery = crate::runtime_rpc::dispatch_runtime_rpc(
            dir.path(),
            "app_feature_report_error",
            &json!({
                "name": "daily_focus_matrix",
                "error": "ReferenceError: brokenReference is not defined",
                "now": now + 1_000
            }),
        )
        .unwrap();
        assert_eq!(recovery["recovered"], true);
        assert_eq!(recovery["action"], "rolled_back");
        assert_eq!(recovery["version"], 2);

        let mut reloaded = FeaturePackageManager::open_at(dir.path(), now + 2_000).unwrap();
        let restored = reloaded.get("daily_focus_matrix", now + 2_000).unwrap();
        assert_eq!(restored.version, 2);
        assert_eq!(restored.status, "active");
        assert_eq!(
            restored.last_error.as_deref(),
            Some("ReferenceError: brokenReference is not defined")
        );
        assert!(restored
            .widget_html
            .as_deref()
            .unwrap()
            .contains("quadrant-v2"));

        // 2. Realistic office widget #2: quote_margin_calc + 5-version cap + CDN rejection
        let calc_html = r#"<div style="background:var(--surface);color:var(--text-primary);border-color:var(--line)">
  <span id="margin" style="color:var(--color-success)">32%</span>
  <script>
    const s = window.bob.state || {};
    async function saveQuote(cost, price) {
      const margin = price > 0 ? ((price - cost) / price) : 0;
      await window.bob.save({ cost, price, margin });
      return await window.bob.call("compute_quote", { cost, price });
    }
  </script>
</div>"#;

        let mut calc_routes = HashMap::new();
        calc_routes.insert(
            "compute_quote".to_string(),
            FeatureRouteSpec::Static {
                template: json!({ "currency": "USD", "approved_tier": "standard" }),
                cache_result: Some(true),
            },
        );

        for step in 1..=7 {
            now += 5_000;
            let pkg = reloaded
                .install_or_upgrade(
                    "quote_margin_calc",
                    &format!("Quote Margin Calculator v{step}"),
                    "Instant B2B gross margin calculator",
                    Some(format!("{calc_html}<!-- rev {step} -->")),
                    calc_routes.clone(),
                    Vec::new(),
                    Some(json!({ "cost": 68, "price": 100 })),
                    now,
                )
                .unwrap();
            assert_eq!(pkg.version, step);
            reloaded.keep_feature("quote_margin_calc", now).unwrap();
        }
        let calc_pkg = reloaded.get("quote_margin_calc", now).unwrap();
        assert_eq!(calc_pkg.version, 7);
        assert_eq!(calc_pkg.versions.len(), MAX_FEATURE_VERSIONS); // Strictly capped at 5!

        // External CDN <script src="https://cdn.example.com/..."> MUST be deterministically rejected
        let cdn_script_err = reloaded
            .install_or_upgrade(
                "quote_margin_calc",
                "CDN Script Attempt",
                "Should fail validation",
                Some(r#"<script src="https://cdn.example.com/chart.min.js"></script>"#.to_string()),
                HashMap::new(),
                Vec::new(),
                None,
                now + 1_000,
            )
            .unwrap_err();
        assert_eq!(cdn_script_err, "FEATURE_WIDGET_EXTERNAL_CDN_FORBIDDEN");

        let cdn_link_err = validate_widget_sandbox_html(
            r#"<link rel="stylesheet" href="https://cdn.example.com/reset.css"><div>Hi</div>"#,
        )
        .unwrap_err();
        assert_eq!(cdn_link_err, "FEATURE_WIDGET_EXTERNAL_CDN_FORBIDDEN");

        let cdn_import_err = validate_widget_sandbox_html(
            r#"<style>@import url("https://cdn.example.com/inter.css");</style>"#,
        )
        .unwrap_err();
        assert_eq!(cdn_import_err, "FEATURE_WIDGET_EXTERNAL_CDN_FORBIDDEN");
    }
}

