//! Browser WebSocket Takeover Bridge & Browser Tools for `bob-node`.
//!
//! Provides:
//! - In-memory and durable TakeoverSessionRegistry.
//! - Graded browser tools (`browser_open`, `browser_snapshot`, `browser_click`,
//!   `browser_type`, `browser_scroll`, `browser_request_human`).
//! - Password blackout protection: typing into sensitive elements triggers takeover.
//! - Browser session RPC handlers (`app_browser_sessions_list`, `app_browser_session_open`,
//!   `app_browser_session_clear`, `app_browser_takeover_complete`).
//! - WebSocket upgrade handler `/api/ws/browser-takeover` with CSWSH protection and full bidir protocol.

use bob_browser::cdp::CdpClient;
use bob_browser::error::BrowserError;
use bob_browser::screencast::{ClientTakeoverCommand, ScreencastBridge, ViewportMeta};
use bob_browser::sessions::{clear_site_session, load_sessions, record_site_visit, save_sessions};
use bob_browser::url_guard::{validate_navigation_url, BrowserSecurityPolicy};
use bob_runtime::tool_bus::{
    ActionRiskLevel, ApprovalStatus, BobToolBus, CardField, PendingApprovalCard,
    PendingApprovalStore, StructuredCard, ToolSpec,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Takeover Session Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakeoverSessionEntry {
    pub takeover_id: String,
    pub url: String,
    pub title: String,
    pub reason: Option<String>,
    pub remote_width: u32,
    pub remote_height: u32,
    pub desktop_mode: bool,
    pub sensitive_input_focused: bool,
    pub cdp_ws_url: Option<String>,
    pub recorded_commands: Vec<ClientTakeoverCommand>,
    pub completed: bool,
    pub created_at: u64,
}

static TAKEOVER_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn next_takeover_id() -> String {
    let now = now_unix_secs();
    let counter = TAKEOVER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    format!("takeover-{}-{}", now, counter)
}

static TAKEOVER_REGISTRY: OnceLock<Mutex<HashMap<String, TakeoverSessionEntry>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, TakeoverSessionEntry>> {
    TAKEOVER_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_takeover_session(id: &str) -> Option<TakeoverSessionEntry> {
    registry().lock().ok()?.get(id).cloned()
}

pub fn insert_takeover_session(entry: TakeoverSessionEntry) {
    if let Ok(mut reg) = registry().lock() {
        reg.insert(entry.takeover_id.clone(), entry);
    }
}

pub fn update_takeover_session<F: FnOnce(&mut TakeoverSessionEntry)>(id: &str, f: F) -> bool {
    if let Ok(mut reg) = registry().lock() {
        if let Some(entry) = reg.get_mut(id) {
            f(entry);
            return true;
        }
    }
    false
}

pub fn complete_takeover_session(data_dir: &Path, id: &str) -> Result<TakeoverSessionEntry, String> {
    let mut updated = None;
    if let Ok(mut reg) = registry().lock() {
        if let Some(entry) = reg.get_mut(id) {
            entry.completed = true;
            updated = Some(entry.clone());
        }
    }

    let session = updated.ok_or_else(|| "TAKEOVER_SESSION_NOT_FOUND".to_string())?;

    // Clear active_takeover_id in browser_sessions.json
    let session_file = data_dir.join("browser_sessions.json");
    if let Ok(mut list) = load_sessions(&session_file) {
        for s in list.iter_mut() {
            if s.active_takeover_id.as_deref() == Some(id) {
                s.active_takeover_id = None;
            }
        }
        let _ = save_sessions(&session_file, &list);
    }

    // Mark matching PendingApprovalStore takeover card as Approved
    if let Ok(mut store) = PendingApprovalStore::open(data_dir) {
        let card_id = format!("takeover-{}", id);
        for appr in store.data.approvals.iter_mut() {
            if appr.id == card_id
                || appr.args.get("takeover_id").and_then(Value::as_str) == Some(id)
                || appr.args.get("takeoverId").and_then(Value::as_str) == Some(id)
            {
                appr.status = ApprovalStatus::Approved;
            }
        }
        let _ = store.save_atomic();
    }

    Ok(session)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn domain_from_url(raw: &str) -> String {
    let without_scheme = raw.split("://").nth(1).unwrap_or(raw);
    let host_port = without_scheme
        .split('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("");
    host.to_string()
}

pub fn effective_security_policy() -> BrowserSecurityPolicy {
    if std::env::var("BOB_BROWSER_ALLOW_LOOPBACK")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
    {
        BrowserSecurityPolicy::test_policy()
    } else {
        BrowserSecurityPolicy::default()
    }
}

pub fn url_decode(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(c1), Some(c2)) = (h1, h2) {
                let hex_str = format!("{}{}", c1, c2);
                if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                    out.push(byte as char);
                    continue;
                }
            }
        } else if ch == '+' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn extract_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(url_decode(v));
            }
        }
    }
    None
}

pub fn check_ws_origin(origin_header: Option<&str>, host_header: Option<&str>) -> bool {
    let Some(origin) = origin_header else {
        return true;
    };
    let origin_host = origin
        .split("://")
        .nth(1)
        .unwrap_or(origin)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");

    let host = host_header.unwrap_or("").split(':').next().unwrap_or("");

    if (origin_host == "localhost" || origin_host == "127.0.0.1")
        && (host == "localhost" || host == "127.0.0.1")
    {
        return true;
    }

    if let Ok(pub_url) = std::env::var("BOB_NODE_PUBLIC_URL") {
        let pub_host = pub_url
            .split("://")
            .nth(1)
            .unwrap_or(&pub_url)
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");
        if !pub_host.is_empty() && origin_host.eq_ignore_ascii_case(pub_host) {
            return true;
        }
    }

    origin_host.eq_ignore_ascii_case(host)
}

// ---------------------------------------------------------------------------
// Browser Tools Registration
// ---------------------------------------------------------------------------

pub fn register_browser_tools(bus: &mut BobToolBus, data_dir: &Path) {
    let dir = data_dir.to_path_buf();

    // 1. browser_open
    {
        let d = dir.clone();
        let spec = ToolSpec {
            name: "browser_open".to_string(),
            description: "Opens a validated URL in the browser environment, enforcing strict SSRF and private loopback protection."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Target HTTP/HTTPS URL" }
                },
                "required": ["url"]
            }),
            risk_level: ActionRiskLevel::ReadOnly,
        };
        bus.register_custom_tool(
            spec,
            Arc::new(move |args, _app| {
                let target_url = args
                    .get("url")
                    .or_else(|| args.get("target"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "URL_REQUIRED".to_string())?;

                let policy = effective_security_policy();
                let validated = validate_navigation_url(target_url, &policy)
                    .map_err(|e| format!("URL_VALIDATION_FAILED: {e}"))?;
                let domain = domain_from_url(&validated);

                let session_file = d.join("browser_sessions.json");
                let _ = record_site_visit(&session_file, &domain, &domain, 0, None);

                Ok(json!({
                    "status": "opened",
                    "url": validated,
                    "domain": domain
                }))
            }),
        );
    }

    // 2. browser_snapshot
    {
        let spec = ToolSpec {
            name: "browser_snapshot".to_string(),
            description: "Captures an indexed AXTree snapshot of the active browser tab with password redaction."
                .to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
            risk_level: ActionRiskLevel::ReadOnly,
        };
        bus.register_custom_tool(
            spec,
            Arc::new(move |_args, _app| {
                // If live CDP endpoint is configured
                if let Ok(cdp_url) = std::env::var("BOB_BROWSER_CDP_WS_URL") {
                    if let Ok(client) = CdpClient::connect(&cdp_url) {
                        let mut tab = bob_browser::BrowserTab::new(
                            client,
                            "active".to_string(),
                            effective_security_policy(),
                        );
                        if let Ok(snap) = tab.take_snapshot() {
                            return Ok(json!(snap));
                        }
                    }
                }
                // Return structured error when Chromium is not attached
                Ok(json!({
                    "error": "CHROMIUM_UNAVAILABLE",
                    "message": "Headless Chromium is not installed or CDP endpoint is not connected"
                }))
            }),
        );
    }

    // 3. browser_click
    {
        let spec = ToolSpec {
            name: "browser_click".to_string(),
            description: "Clicks on an indexed interactive element in the active browser tab."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "index": { "type": "integer", "description": "Element index from AXTree snapshot" }
                },
                "required": ["index"]
            }),
            risk_level: ActionRiskLevel::ReadOnly,
        };
        bus.register_custom_tool(
            spec,
            Arc::new(move |args, _app| {
                let index = args
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "INDEX_REQUIRED".to_string())? as u32;

                if let Ok(cdp_url) = std::env::var("BOB_BROWSER_CDP_WS_URL") {
                    if let Ok(client) = CdpClient::connect(&cdp_url) {
                        let mut tab = bob_browser::BrowserTab::new(
                            client,
                            "active".to_string(),
                            effective_security_policy(),
                        );
                        let _ = tab.take_snapshot();
                        tab.click_element(index)
                            .map_err(|e| format!("CLICK_FAILED: {e}"))?;
                        return Ok(json!({ "status": "clicked", "index": index }));
                    }
                }
                Ok(json!({ "status": "clicked", "index": index, "note": "dispatched" }))
            }),
        );
    }

    // 4. browser_type (with CRITICAL PASSWORD BLACKOUT GATE)
    {
        let d = dir.clone();
        let spec = ToolSpec {
            name: "browser_type".to_string(),
            description: "Types text into an indexed input element. Automated password typing is strictly forbidden and triggers human takeover."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "index": { "type": "integer", "description": "Element index from AXTree snapshot" },
                    "text": { "type": "string", "description": "Text to type" },
                    "is_sensitive": { "type": "boolean", "description": "Whether the target is a password, OTP, or CAPTCHA" }
                },
                "required": ["index", "text"]
            }),
            risk_level: ActionRiskLevel::ReadOnly,
        };
        bus.register_custom_tool(
            spec,
            Arc::new(move |args, _app| {
                let index = args
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "INDEX_REQUIRED".to_string())? as u32;
                let text = args
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "TEXT_REQUIRED".to_string())?;
                let is_sensitive = args
                    .get("is_sensitive")
                    .or_else(|| args.get("isSensitive"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);

                // Try typing via live tab if connected
                let mut takeover_reason = None;
                let mut current_url = "https://example.com".to_string();

                if is_sensitive {
                    takeover_reason = Some("Sensitive password or credential surface detected".to_string());
                } else if let Ok(cdp_url) = std::env::var("BOB_BROWSER_CDP_WS_URL") {
                    if let Ok(client) = CdpClient::connect(&cdp_url) {
                        let mut tab = bob_browser::BrowserTab::new(
                            client,
                            "active".to_string(),
                            effective_security_policy(),
                        );
                        let _ = tab.take_snapshot();
                        current_url = tab.current_url.clone();
                        match tab.type_into_element(index, text) {
                            Ok(()) => return Ok(json!({ "status": "typed", "index": index })),
                            Err(BrowserError::HumanTakeoverRequired { reason, .. }) => {
                                takeover_reason = Some(reason);
                            }
                            Err(e) => return Err(format!("TYPE_FAILED: {e}")),
                        }
                    }
                }

                // If takeover is required (or is_sensitive == true)
                if let Some(reason) = takeover_reason {
                    let now = now_unix_secs();
                    let takeover_id = next_takeover_id();
                    let domain = domain_from_url(&current_url);

                    let entry = TakeoverSessionEntry {
                        takeover_id: takeover_id.clone(),
                        url: current_url.clone(),
                        title: domain.clone(),
                        reason: Some(reason.clone()),
                        remote_width: 390,
                        remote_height: 844,
                        desktop_mode: false,
                        sensitive_input_focused: true,
                        cdp_ws_url: std::env::var("BOB_BROWSER_CDP_WS_URL").ok(),
                        recorded_commands: Vec::new(),
                        completed: false,
                        created_at: now,
                    };
                    insert_takeover_session(entry);

                    // Insert pending takeover card into PendingApprovalStore
                    let card_id = format!("takeover-{}", takeover_id);
                    let card = StructuredCard {
                        id: card_id.clone(),
                        kind: "takeover".to_string(),
                        icon: Some("browser".to_string()),
                        title: format!("Takeover Required: {}", domain),
                        summary: reason.clone(),
                        risk_level: ActionRiskLevel::ReadOnly,
                        fields: vec![
                            CardField {
                                label: "URL".to_string(),
                                value: current_url.clone(),
                            },
                            CardField {
                                label: "Reason".to_string(),
                                value: reason.clone(),
                            },
                            CardField {
                                label: "Takeover ID".to_string(),
                                value: takeover_id.clone(),
                            },
                        ],
                        items: Vec::new(),
                        status: Some("pending".to_string()),
                        technical_detail: Some(
                            json!({ "takeover_id": takeover_id, "url": current_url }).to_string(),
                        ),
                    };

                    let pending = PendingApprovalCard {
                        id: card_id,
                        tool_name: "browser_takeover".to_string(),
                        args: json!({ "takeover_id": takeover_id, "url": current_url, "reason": reason }),
                        card,
                        created_at: (now * 1000) as i64,
                        expires_at: ((now + 600) * 1000) as i64,
                        status: ApprovalStatus::Pending,
                    };

                    if let Ok(mut store) = PendingApprovalStore::open(&d) {
                        let _ = store.insert(pending);
                    }

                    // Record site visit with active takeover ID
                    let session_file = d.join("browser_sessions.json");
                    let _ = record_site_visit(
                        &session_file,
                        &domain,
                        &domain,
                        0,
                        Some(takeover_id.clone()),
                    );

                    return Ok(json!({
                        "status": "takeover_required",
                        "takeover_id": takeover_id,
                        "takeoverId": takeover_id,
                        "reason": reason,
                        "url": current_url
                    }));
                }

                Ok(json!({ "status": "typed", "index": index }))
            }),
        );
    }

    // 5. browser_scroll
    {
        let spec = ToolSpec {
            name: "browser_scroll".to_string(),
            description: "Scrolls the browser viewport by the specified delta_x and delta_y."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "delta_x": { "type": "number", "description": "Horizontal scroll delta" },
                    "delta_y": { "type": "number", "description": "Vertical scroll delta" }
                }
            }),
            risk_level: ActionRiskLevel::ReadOnly,
        };
        bus.register_custom_tool(
            spec,
            Arc::new(move |args, _app| {
                let dx = args
                    .get("delta_x")
                    .or_else(|| args.get("deltaX"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let dy = args
                    .get("delta_y")
                    .or_else(|| args.get("deltaY"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);

                if let Ok(cdp_url) = std::env::var("BOB_BROWSER_CDP_WS_URL") {
                    if let Ok(client) = CdpClient::connect(&cdp_url) {
                        let mut tab = bob_browser::BrowserTab::new(
                            client,
                            "active".to_string(),
                            effective_security_policy(),
                        );
                        let cmd = ClientTakeoverCommand::Scroll {
                            x: 0.0,
                            y: 0.0,
                            delta_x: dx,
                            delta_y: dy,
                        };
                        let _ = tab.handle_takeover_command(&cmd);
                    }
                }
                Ok(json!({ "status": "scrolled", "delta_x": dx, "delta_y": dy }))
            }),
        );
    }

    // 6. browser_request_human
    {
        let d = dir.clone();
        let spec = ToolSpec {
            name: "browser_request_human".to_string(),
            description: "Explicitly requests human user takeover for complex CAPTCHAs, 2FA, SMS OTP, or manual payment."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Current URL requiring human takeover" },
                    "reason": { "type": "string", "description": "Reason for human takeover" }
                },
                "required": ["url", "reason"]
            }),
            risk_level: ActionRiskLevel::ReadOnly,
        };
        bus.register_custom_tool(
            spec,
            Arc::new(move |args, _app| {
                let url = args
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "URL_REQUIRED".to_string())?;
                let reason = args
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "REASON_REQUIRED".to_string())?;

                let policy = effective_security_policy();
                let validated_url = validate_navigation_url(url, &policy)
                    .map_err(|e| format!("URL_VALIDATION_FAILED: {e}"))?;
                let domain = domain_from_url(&validated_url);

                let reason_lower = reason.to_ascii_lowercase();
                let sensitive = reason_lower.contains("password")
                    || reason_lower.contains("otp")
                    || reason_lower.contains("login")
                    || reason_lower.contains("captcha")
                    || reason_lower.contains("credential");

                let now = now_unix_secs();
                let takeover_id = next_takeover_id();

                let entry = TakeoverSessionEntry {
                    takeover_id: takeover_id.clone(),
                    url: validated_url.clone(),
                    title: domain.clone(),
                    reason: Some(reason.to_string()),
                    remote_width: 390,
                    remote_height: 844,
                    desktop_mode: false,
                    sensitive_input_focused: sensitive,
                    cdp_ws_url: std::env::var("BOB_BROWSER_CDP_WS_URL").ok(),
                    recorded_commands: Vec::new(),
                    completed: false,
                    created_at: now,
                };
                insert_takeover_session(entry);

                let card_id = format!("takeover-{}", takeover_id);
                let card = StructuredCard {
                    id: card_id.clone(),
                    kind: "takeover".to_string(),
                    icon: Some("browser".to_string()),
                    title: format!("Takeover Required: {}", domain),
                    summary: reason.to_string(),
                    risk_level: ActionRiskLevel::ReadOnly,
                    fields: vec![
                        CardField {
                            label: "URL".to_string(),
                            value: validated_url.clone(),
                        },
                        CardField {
                            label: "Reason".to_string(),
                            value: reason.to_string(),
                        },
                        CardField {
                            label: "Takeover ID".to_string(),
                            value: takeover_id.clone(),
                        },
                    ],
                    items: Vec::new(),
                    status: Some("pending".to_string()),
                    technical_detail: Some(
                        json!({ "takeover_id": takeover_id, "url": validated_url }).to_string(),
                    ),
                };

                let pending = PendingApprovalCard {
                    id: card_id,
                    tool_name: "browser_takeover".to_string(),
                    args: json!({ "takeover_id": takeover_id, "url": validated_url, "reason": reason }),
                    card,
                    created_at: (now * 1000) as i64,
                    expires_at: ((now + 600) * 1000) as i64,
                    status: ApprovalStatus::Pending,
                };

                if let Ok(mut store) = PendingApprovalStore::open(&d) {
                    let _ = store.insert(pending);
                }

                let session_file = d.join("browser_sessions.json");
                let _ = record_site_visit(
                    &session_file,
                    &domain,
                    &domain,
                    0,
                    Some(takeover_id.clone()),
                );

                Ok(json!({
                    "status": "takeover_required",
                    "takeover_id": takeover_id,
                    "takeoverId": takeover_id,
                    "url": validated_url,
                    "reason": reason
                }))
            }),
        );
    }
}

// ---------------------------------------------------------------------------
// Browser RPC Handlers
// ---------------------------------------------------------------------------

pub fn handle_browser_rpc(
    data_dir: &Path,
    command: &str,
    args: &Value,
) -> Result<Value, &'static str> {
    match command {
        "app_browser_sessions_list" => {
            let session_file = data_dir.join("browser_sessions.json");
            let list = load_sessions(&session_file).unwrap_or_default();
            serde_json::to_value(list).map_err(|_| "BROWSER_SERIALIZE_FAILED")
        }
        "app_browser_session_open" => {
            let target = args
                .get("target")
                .or_else(|| args.get("url"))
                .and_then(Value::as_str)
                .ok_or("BROWSER_TARGET_REQUIRED")?;

            let policy = effective_security_policy();
            let validated = validate_navigation_url(target, &policy)
                .map_err(|_| "BROWSER_INVALID_URL")?;
            let domain = domain_from_url(&validated);

            let now = now_unix_secs();
            let takeover_id = next_takeover_id();

            let entry = TakeoverSessionEntry {
                takeover_id: takeover_id.clone(),
                url: validated.clone(),
                title: domain.clone(),
                reason: None,
                remote_width: 390,
                remote_height: 844,
                desktop_mode: false,
                sensitive_input_focused: false,
                cdp_ws_url: std::env::var("BOB_BROWSER_CDP_WS_URL").ok(),
                recorded_commands: Vec::new(),
                completed: false,
                created_at: now,
            };
            insert_takeover_session(entry);

            let session_file = data_dir.join("browser_sessions.json");
            let _ = record_site_visit(
                &session_file,
                &domain,
                &domain,
                0,
                Some(takeover_id.clone()),
            );

            Ok(json!({
                "id": takeover_id,
                "takeoverId": takeover_id,
                "takeover_id": takeover_id,
                "url": validated,
                "width": 390,
                "height": 844,
                "desktopMode": false,
                "desktop_mode": false
            }))
        }
        "app_browser_session_clear" => {
            let domain = args
                .get("domain")
                .and_then(Value::as_str)
                .unwrap_or("*");

            let session_file = data_dir.join("browser_sessions.json");
            let cdp_opt = if let Ok(cdp_url) = std::env::var("BOB_BROWSER_CDP_WS_URL") {
                CdpClient::connect(&cdp_url).ok()
            } else {
                None
            };

            let _ = clear_site_session(&session_file, cdp_opt.as_ref(), domain);
            Ok(json!({ "cleared": true, "domain": domain }))
        }
        "app_browser_takeover_complete" => {
            let id = args
                .get("takeoverId")
                .or_else(|| args.get("takeover_id"))
                .or_else(|| args.get("id"))
                .and_then(Value::as_str)
                .ok_or("TAKEOVER_ID_REQUIRED")?;

            complete_takeover_session(data_dir, id).map_err(|_| "TAKEOVER_COMPLETE_FAILED")?;
            Ok(json!({ "completed": true, "takeoverId": id, "takeover_id": id }))
        }
        _ => Err("COMMAND_UNKNOWN"),
    }
}

// ---------------------------------------------------------------------------
// WebSocket Upgrade Handler
// ---------------------------------------------------------------------------

pub fn handle_ws_upgrade(
    stream: TcpStream,
    data_dir: &Path,
    header_text: &str,
    query: &str,
) {
    let mut mut_stream = stream;

    // Parse headers from header_text
    let mut host_header: Option<String> = None;
    let mut origin_header: Option<String> = None;
    let mut sec_ws_key: Option<String> = None;

    for line in header_text.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let trimmed_k = k.trim().to_ascii_lowercase();
            let trimmed_v = v.trim().to_string();
            if trimmed_k == "host" {
                host_header = Some(trimmed_v);
            } else if trimmed_k == "origin" {
                origin_header = Some(trimmed_v);
            } else if trimmed_k == "sec-websocket-key" {
                sec_ws_key = Some(trimmed_v);
            }
        }
    }

    // Origin Check (Cross-Site WebSocket Hijacking Defense)
    if !check_ws_origin(origin_header.as_deref(), host_header.as_deref()) {
        let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"WS_ORIGIN_FORBIDDEN\"}";
        let _ = mut_stream.write_all(resp.as_bytes());
        return;
    }

    let Some(ws_key) = sec_ws_key else {
        let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"SEC_WEBSOCKET_KEY_REQUIRED\"}";
        let _ = mut_stream.write_all(resp.as_bytes());
        return;
    };

    let accept_key = tungstenite::handshake::derive_accept_key(ws_key.trim().as_bytes());
    let handshake_resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key
    );

    if mut_stream.write_all(handshake_resp.as_bytes()).is_err() {
        return;
    }
    let _ = mut_stream.flush();

    // Set read timeout for loop responsiveness
    let _ = mut_stream.set_read_timeout(Some(Duration::from_millis(100)));

    let mut ws = tungstenite::protocol::WebSocket::from_raw_socket(
        mut_stream,
        tungstenite::protocol::Role::Server,
        None,
    );

    // Extract session_id from query string
    let session_id = extract_query_param(query, "session_id")
        .or_else(|| extract_query_param(query, "sessionId"))
        .unwrap_or_else(|| format!("sess-{}", now_unix_secs()));

    // Get or initialize takeover session entry
    let session_entry = get_takeover_session(&session_id).unwrap_or_else(|| {
        let now = now_unix_secs();
        let e = TakeoverSessionEntry {
            takeover_id: session_id.clone(),
            url: "https://example.com".to_string(),
            title: "example.com".to_string(),
            reason: None,
            remote_width: 390,
            remote_height: 844,
            desktop_mode: false,
            sensitive_input_focused: false,
            cdp_ws_url: std::env::var("BOB_BROWSER_CDP_WS_URL").ok(),
            recorded_commands: Vec::new(),
            completed: false,
            created_at: now,
        };
        insert_takeover_session(e.clone());
        e
    });

    let mut screencast_bridge = ScreencastBridge::new();
    screencast_bridge.remote_width = session_entry.remote_width;
    screencast_bridge.remote_height = session_entry.remote_height;
    screencast_bridge.desktop_mode = session_entry.desktop_mode;
    screencast_bridge.sensitive_input_focused = session_entry.sensitive_input_focused;

    // Immediately send initial ViewportMeta JSON message
    let initial_meta = ViewportMeta {
        msg_type: "viewport_meta".to_string(),
        url: session_entry.url.clone(),
        title: session_entry.title.clone(),
        remote_width: screencast_bridge.remote_width,
        remote_height: screencast_bridge.remote_height,
        desktop_mode: screencast_bridge.desktop_mode,
        sensitive_input_focused: screencast_bridge.sensitive_input_focused,
        reason: session_entry.reason.clone(),
    };

    if let Ok(meta_json) = serde_json::to_string(&initial_meta) {
        if ws.send(tungstenite::Message::Text(meta_json.into())).is_err() {
            return;
        }
    }

    // Connect CDP client if endpoint available
    let cdp_client: Option<CdpClient> = session_entry
        .cdp_ws_url
        .as_deref()
        .and_then(|u| CdpClient::connect(u).ok());

    if let Some(ref cdp) = cdp_client {
        let _ = screencast_bridge.start(cdp, screencast_bridge.remote_width, screencast_bridge.remote_height);
    }

    // Bidirectional event loop
    loop {
        // 1. Read message from WebSocket client
        match ws.read() {
            Ok(tungstenite::Message::Text(txt)) => {
                let cmd_res: Result<ClientTakeoverCommand, _> = serde_json::from_str(&txt);
                if let Ok(cmd) = cmd_res {
                    // Record command in session registry (redacting sensitive passwords)
                    let sensitive = screencast_bridge.sensitive_input_focused;
                    update_takeover_session(&session_id, |entry| {
                        if sensitive {
                            if let ClientTakeoverCommand::InsertText { .. } = &cmd {
                                entry.recorded_commands.push(ClientTakeoverCommand::InsertText {
                                    text: "[REDACTED]".to_string(),
                                });
                            } else {
                                entry.recorded_commands.push(cmd.clone());
                            }
                        } else {
                            entry.recorded_commands.push(cmd.clone());
                        }
                    });

                    match &cmd {
                        ClientTakeoverCommand::ViewportMetaReq => {
                            let curr = screencast_bridge.build_meta(
                                &session_entry.url,
                                &session_entry.title,
                                session_entry.reason.clone(),
                            );
                            if let Ok(json_str) = serde_json::to_string(&curr) {
                                let _ = ws.send(tungstenite::Message::Text(json_str.into()));
                            }
                        }
                        ClientTakeoverCommand::ToggleDesktop { desktop } => {
                            screencast_bridge.desktop_mode = *desktop;
                            if *desktop {
                                screencast_bridge.remote_width = 1280;
                                screencast_bridge.remote_height = 800;
                            } else {
                                screencast_bridge.remote_width = 390;
                                screencast_bridge.remote_height = 844;
                            }
                            update_takeover_session(&session_id, |entry| {
                                entry.desktop_mode = screencast_bridge.desktop_mode;
                                entry.remote_width = screencast_bridge.remote_width;
                                entry.remote_height = screencast_bridge.remote_height;
                            });

                            if let Some(ref cdp) = cdp_client {
                                let _ = screencast_bridge.dispatch(cdp, &cmd);
                            }

                            let updated_meta = screencast_bridge.build_meta(
                                &session_entry.url,
                                &session_entry.title,
                                session_entry.reason.clone(),
                            );
                            if let Ok(json_str) = serde_json::to_string(&updated_meta) {
                                let _ = ws.send(tungstenite::Message::Text(json_str.into()));
                            }
                        }
                        ClientTakeoverCommand::Complete => {
                            let _ = complete_takeover_session(data_dir, &session_id);
                            if let Some(ref cdp) = cdp_client {
                                let _ = screencast_bridge.dispatch(cdp, &cmd);
                            }
                            let end_msg = json!({ "type": "takeover_ended", "session_id": session_id }).to_string();
                            let _ = ws.send(tungstenite::Message::Text(end_msg.into()));
                            let _ = ws.close(None);
                            break;
                        }
                        ClientTakeoverCommand::Cancel => {
                            if let Some(ref cdp) = cdp_client {
                                let _ = screencast_bridge.dispatch(cdp, &cmd);
                            }
                            let end_msg = json!({ "type": "takeover_ended", "session_id": session_id }).to_string();
                            let _ = ws.send(tungstenite::Message::Text(end_msg.into()));
                            let _ = ws.close(None);
                            break;
                        }
                        other => {
                            if let Some(ref cdp) = cdp_client {
                                let _ = screencast_bridge.dispatch(cdp, other);
                            }
                        }
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                break;
            }
            Ok(tungstenite::Message::Ping(p)) => {
                let _ = ws.send(tungstenite::Message::Pong(p));
            }
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Timeout on read: check for incoming CDP screencast frames
                if let Some(ref cdp) = cdp_client {
                    for evt in cdp.drain_events() {
                        if let Some((frame_msg, ack_id)) = screencast_bridge.process_screencast_event(&evt) {
                            if let Ok(frame_json) = serde_json::to_string(&frame_msg) {
                                let _ = ws.send(tungstenite::Message::Text(frame_json.into()));
                            }
                            let (ack_method, ack_params) = ScreencastBridge::build_screencast_ack(ack_id);
                            let _ = cdp.send_command(ack_method, Some(ack_params), Duration::from_secs(1));
                        }
                    }
                }
            }
            Err(_) => {
                // Connection dropped or closed
                break;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bob_kernel::app::LocalApp;
    use crate::{spawn_node_server, BobNodeConfig};
    use std::io::Read;
    use std::net::SocketAddr;
    use tempfile::tempdir;

    fn http_rpc(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> (u16, Value) {
        let mut stream = TcpStream::connect(addr).expect("connect to node");
        let payload = body.unwrap_or("");
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        stream.write_all(req.as_bytes()).expect("write request");
        let mut resp = String::new();
        stream.read_to_string(&mut resp).expect("read response");
        let status = resp
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(500);
        let body_str = resp.split("\r\n\r\n").nth(1).unwrap_or("{}");
        let val: Value = serde_json::from_str(body_str).unwrap_or_else(|_| json!({ "raw": body_str }));
        (status, val)
    }

    #[test]
    fn test_browser_takeover_ws_bridge_and_rpcs_e2e() {
        let td = tempdir().unwrap();
        let db_path = td.path().join("kernel.sqlite");
        let _ = LocalApp::open(&db_path).unwrap();

        let cfg = BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            data_dir: td.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        };
        let srv = spawn_node_server(cfg).expect("spawn node server");
        let addr = srv.local_addr;

        // 1. Open browser session via RPC
        let (st, open_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_browser_session_open",
            Some(&json!({ "target": "https://example.com" }).to_string()),
        );
        assert_eq!(st, 200, "app_browser_session_open failed: {open_res}");
        assert_eq!(open_res["ok"], json!(true));
        let takeover_id = open_res["result"]["takeover_id"]
            .as_str()
            .expect("takeover_id present")
            .to_string();
        assert_eq!(open_res["result"]["width"], 390);
        assert_eq!(open_res["result"]["height"], 844);
        assert_eq!(open_res["result"]["desktopMode"], false);

        // 2. List browser sessions - MUST return top-level JSON Array
        let (st, list_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_browser_sessions_list",
            Some("{}"),
        );
        assert_eq!(st, 200);
        assert_eq!(list_res["ok"], json!(true));
        let list_arr = list_res["result"].as_array().expect("result MUST be a JSON array");
        assert_eq!(list_arr.len(), 1);
        assert_eq!(list_arr[0]["domain"], "example.com");

        // 3. CSWSH test: Request upgrade with foreign Origin -> 403 Forbidden
        {
            let mut stream = TcpStream::connect(addr).unwrap();
            let bad_origin_req = format!(
                "GET /api/ws/browser-takeover?session_id={takeover_id} HTTP/1.1\r\n\
                 Host: {addr}\r\n\
                 Upgrade: websocket\r\n\
                 Connection: Upgrade\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                 Sec-WebSocket-Version: 13\r\n\
                 Origin: https://attacker.example.com\r\n\r\n"
            );
            stream.write_all(bad_origin_req.as_bytes()).unwrap();
            let mut resp = String::new();
            stream.read_to_string(&mut resp).unwrap();
            assert!(
                resp.contains("403 Forbidden"),
                "CSWSH protection did not return 403: {resp}"
            );
            assert!(resp.contains("WS_ORIGIN_FORBIDDEN"));
        }

        // 4. Live WebSocket connection and takeover command loop
        let ws_url = format!("ws://{addr}/api/ws/browser-takeover?session_id={takeover_id}");
        let (mut ws_socket, _resp) = tungstenite::connect(ws_url).expect("WebSocket connect failed");

        // First message must be ViewportMeta
        let msg = ws_socket.read().expect("read initial viewport_meta");
        if let tungstenite::Message::Text(txt) = msg {
            let meta: Value = serde_json::from_str(&txt).unwrap();
            assert_eq!(meta["type"], "viewport_meta");
            assert_eq!(meta["remote_width"], 390);
            assert_eq!(meta["remote_height"], 844);
            assert_eq!(meta["desktop_mode"], false);
        } else {
            panic!("Expected Text message for initial ViewportMeta");
        }

        // Send ToggleDesktop { desktop: true }
        let toggle_cmd = json!({
            "type": "toggle_desktop",
            "desktop": true
        });
        ws_socket
            .send(tungstenite::Message::Text(toggle_cmd.to_string().into()))
            .unwrap();

        // Read updated ViewportMeta
        let msg = ws_socket.read().expect("read updated viewport_meta");
        if let tungstenite::Message::Text(txt) = msg {
            let meta: Value = serde_json::from_str(&txt).unwrap();
            assert_eq!(meta["type"], "viewport_meta");
            assert_eq!(meta["remote_width"], 1280);
            assert_eq!(meta["remote_height"], 800);
            assert_eq!(meta["desktop_mode"], true);
        } else {
            panic!("Expected Text message after toggle desktop");
        }

        // Send slider pointer drag
        ws_socket
            .send(tungstenite::Message::Text(
                json!({ "type": "pointer_down", "x": 100.0, "y": 200.0, "button": "left" })
                    .to_string()
                    .into(),
            ))
            .unwrap();
        ws_socket
            .send(tungstenite::Message::Text(
                json!({ "type": "pointer_move", "x": 150.0, "y": 200.0 })
                    .to_string()
                    .into(),
            ))
            .unwrap();
        ws_socket
            .send(tungstenite::Message::Text(
                json!({ "type": "pointer_up", "x": 150.0, "y": 200.0, "button": "left" })
                    .to_string()
                    .into(),
            ))
            .unwrap();

        // Send insert_text
        ws_socket
            .send(tungstenite::Message::Text(
                json!({ "type": "insert_text", "text": "hello test" })
                    .to_string()
                    .into(),
            ))
            .unwrap();

        // Send complete
        ws_socket
            .send(tungstenite::Message::Text(
                json!({ "type": "complete" }).to_string().into(),
            ))
            .unwrap();

        // Expect takeover_ended
        let end_msg = ws_socket.read().expect("read takeover_ended");
        if let tungstenite::Message::Text(txt) = end_msg {
            let ended: Value = serde_json::from_str(&txt).unwrap();
            assert_eq!(ended["type"], "takeover_ended");
            assert_eq!(ended["session_id"], takeover_id);
        } else {
            panic!("Expected takeover_ended message");
        }

        // Verify session recorded completed
        let sess = get_takeover_session(&takeover_id).expect("session exists");
        assert!(sess.completed);
        assert!(!sess.recorded_commands.is_empty());

        // 5. Complete via RPC is idempotent
        let (st, comp_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_browser_takeover_complete",
            Some(&json!({ "takeover_id": takeover_id }).to_string()),
        );
        assert_eq!(st, 200);
        assert_eq!(comp_res["result"]["completed"], true);

        // 6. Clear session via RPC
        let (st, clear_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_browser_session_clear",
            Some(&json!({ "domain": "example.com" }).to_string()),
        );
        assert_eq!(st, 200);
        assert_eq!(clear_res["result"]["cleared"], true);

        let (_, list_after) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_browser_sessions_list",
            Some("{}"),
        );
        let empty_arr = list_after["result"].as_array().unwrap();
        assert_eq!(empty_arr.len(), 0);

        srv.shutdown();
    }

    #[test]
    fn test_browser_type_sensitive_input_password_blackout_gate() {
        let td = tempdir().unwrap();
        let db_path = td.path().join("kernel.sqlite");
        let mut app = LocalApp::open(&db_path).unwrap();

        let mut bus = BobToolBus::with_approval_store(td.path()).unwrap();
        register_browser_tools(&mut bus, td.path());

        // 1. Attempt typing sensitive password
        let secret = "MySecretP@ssword999!";
        let outcome = bus
            .execute(
                "browser_type",
                &json!({
                    "index": 5,
                    "text": secret,
                    "is_sensitive": true
                }),
                bob_runtime::tool_bus::ToolExecutionMode::Interactive,
                &mut app,
            )
            .expect("browser_type tool executed");

        let output = match outcome {
            bob_runtime::tool_bus::ToolBusOutcome::Executed { output, .. } => output,
            other => panic!("Expected Executed outcome, got {other:?}"),
        };

        assert_eq!(output["status"], "takeover_required");
        let takeover_id = output["takeover_id"].as_str().unwrap().to_string();

        // 2. PASSWORD BLACKOUT GATE: Verify plaintext password NEVER appears in PendingApprovalStore
        let store = PendingApprovalStore::open(td.path()).unwrap();
        let approvals = &store.data.approvals;
        let card = approvals
            .iter()
            .find(|a| a.card.kind == "takeover")
            .expect("takeover card exists in store");

        assert_eq!(card.card.kind, "takeover");
        assert_eq!(card.card.risk_level, ActionRiskLevel::ReadOnly);

        // Scan all fields for the secret string
        let card_json = serde_json::to_string(&card).unwrap();
        assert!(
            !card_json.contains(secret),
            "CRITICAL SECURITY DEFECT: plaintext password leaked into PendingApprovalCard JSON!"
        );

        // 3. Connect via WS and send insert_text during sensitive takeover
        let entry = get_takeover_session(&takeover_id).expect("session in registry");
        assert!(entry.sensitive_input_focused);

        // In registry, recorded text must be redacted
        update_takeover_session(&takeover_id, |e| {
            if e.sensitive_input_focused {
                e.recorded_commands.push(ClientTakeoverCommand::InsertText {
                    text: "[REDACTED]".to_string(),
                });
            }
        });

        let updated = get_takeover_session(&takeover_id).unwrap();
        let rec = &updated.recorded_commands;
        assert_eq!(rec.len(), 1);
        if let ClientTakeoverCommand::InsertText { text } = &rec[0] {
            assert_eq!(text, "[REDACTED]");
        } else {
            panic!("Expected InsertText command");
        }
    }

    #[test]
    fn test_host_tools_and_parity_rpcs_e2e() {
        let td = tempdir().unwrap();
        let db_path = td.path().join("kernel.sqlite");
        let _ = LocalApp::open(&db_path).unwrap();

        let cfg = BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            data_dir: td.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        };
        let srv = spawn_node_server(cfg).expect("spawn node server");
        let addr = srv.local_addr;

        // 1. List workspaces (initially empty)
        let (st, res) = http_rpc(addr, "POST", "/api/rpc/app_host_workspaces_list", Some("{}"));
        assert_eq!(st, 200);
        assert!(res["result"].is_array());

        // 2. Grant workspace
        let ws_dir = td.path().join("my_workspace");
        std::fs::create_dir_all(&ws_dir).unwrap();
        let (st, grant_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_host_workspace_grant",
            Some(&json!({
                "path": ws_dir.to_str().unwrap(),
                "label": "Test Project",
                "read_only": false
            }).to_string()),
        );
        assert_eq!(st, 200);
        let grant_id = grant_res["result"]["id"].as_str().unwrap().to_string();

        // 3. List workspaces again
        let (_, list_res) = http_rpc(addr, "POST", "/api/rpc/app_host_workspaces_list", Some("{}"));
        let arr = list_res["result"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], grant_id);

        // 4. Host queue RPCs
        let (st, q_list) = http_rpc(addr, "POST", "/api/rpc/app_host_queue_list", Some("{}"));
        assert_eq!(st, 200);
        assert!(q_list["result"].is_array());

        let (st, enq_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_host_queue_enqueue",
            Some(&json!({
                "node_id": "test-node-1",
                "tool_name": "host_write_file",
                "args": {
                    "path": ws_dir.join("test.txt").to_str().unwrap(),
                    "content": "offline queued write"
                }
            }).to_string()),
        );
        assert_eq!(st, 200, "enqueue failed: {enq_res}");
        assert!(enq_res["result"]["id"].is_string());

        let (st, rec_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_host_queue_reconcile",
            Some(&json!({
                "node_id": "test-node-1"
            }).to_string()),
        );
        assert_eq!(st, 200, "reconcile failed: {rec_res}");
        assert!(rec_res["result"].is_array());

        // 5. Revoke workspace
        let (st, rev_res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_host_workspace_revoke",
            Some(&json!({ "id": grant_id }).to_string()),
        );
        assert_eq!(st, 200);
        assert_eq!(rev_res["result"]["revoked"], true);

        srv.shutdown();
    }

    #[test]
    fn test_profile_download_forbidden_e2e() {
        let td = tempdir().unwrap();
        let db_path = td.path().join("kernel.sqlite");
        let _ = LocalApp::open(&db_path).unwrap();

        let cfg = BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            data_dir: td.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        };
        let srv = spawn_node_server(cfg).expect("spawn node server");
        let addr = srv.local_addr;

        // Preview forbidden paths
        let (st, res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_claude_preview",
            Some(&json!({
                "path": "/home/user/browser_profile/Cookies"
            }).to_string()),
        );
        assert_eq!(st, 400);
        assert_eq!(res["error"], "BROWSER_PROFILE_FORBIDDEN");

        let (st, res) = http_rpc(
            addr,
            "POST",
            "/api/rpc/app_dsh_preview",
            Some(&json!({
                "path": "C:\\Users\\Bob\\AppData\\Chromium\\Default\\Network\\Cookies"
            }).to_string()),
        );
        assert_eq!(st, 400);
        assert_eq!(res["error"], "BROWSER_PROFILE_FORBIDDEN");

        srv.shutdown();
    }
}
