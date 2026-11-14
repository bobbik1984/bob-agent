//! First-Visit 3-Step Graphical Onboarding & Single-Use 6-Digit Pairing Authority (`WP-E1.3`).
//!
//! Enforces:
//! - Zero-secret read-only `/api/node/v1/onboarding/status` discovery (`needsOnboarding` + dual presets).
//! - CSPRNG 6-digit single-use bootstrap pairing code with 30-minute TTL, constant-time verification,
//!   exponential backoff rate limiting (`PAIRING_RATE_LIMITED`), and anti-front-running lock
//!   (`ONBOARDING_ALREADY_CLAIMED`).
//! - Owner-authenticated Step 2 provider connectivity test + automatic `(clerk, maker)` dual-role binding.
//! - Owner-authenticated Step 3 optional persona writing (`SOUL.md`) + completion receipt.

use crate::{node_access, NodeContext};
use bob_runtime::ModelHubRuntime;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const ONBOARDING_COMPLETED_FILE: &str = "onboarding_completed.json";
pub const ONBOARDING_IN_PROGRESS_FILE: &str = "onboarding_in_progress.json";
pub const BOOTSTRAP_CODE_TTL_MS: i64 = 30 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct BootstrapPairingState {
    pub code: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub failed_attempts: u32,
    pub locked_until: i64,
    pub consumed: bool,
}

fn bootstrap_store() -> &'static Mutex<HashMap<PathBuf, BootstrapPairingState>> {
    static STORE: OnceLock<Mutex<HashMap<PathBuf, BootstrapPairingState>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn canonical_key(data_dir: &Path) -> PathBuf {
    std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf())
}

fn generate_six_digit_code() -> Result<String, &'static str> {
    let rng = SystemRandom::new();
    // Rejection sampling over 0..1_000_000_000 (< u32::MAX) for uniform 6-digit distribution 100000..=999999
    let bound: u32 = 900_000;
    let threshold: u32 = u32::MAX - (u32::MAX % bound);
    loop {
        let mut bytes = [0u8; 4];
        rng.fill(&mut bytes).map_err(|_| "ONBOARDING_RNG_FAILED")?;
        let raw = u32::from_le_bytes(bytes);
        if raw < threshold {
            let num = 100_000 + (raw % bound);
            return Ok(format!("{num:06}"));
        }
    }
}

fn constant_time_eq_6(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != 6 || bb.len() != 6 {
        return false;
    }
    let mut diff: u8 = 0;
    for idx in 0..6 {
        diff |= ab[idx] ^ bb[idx];
    }
    diff == 0
}

fn normalize_code(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
}

pub fn onboarding_disabled_by_env() -> bool {
    std::env::var("BOB_DISABLE_ONBOARDING")
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Returns true when a node is already established behind an external reverse-proxy auth gate
/// (e.g., Caddy `common_auth` on `bob-online.bobbik.org`) where `node_access.sqlite` is unused
/// and onboarding was not started via `ONBOARDING_IN_PROGRESS_FILE`.
/// NOTE: Do NOT check `model_hub_state.json` or `online_work_store.json` here because
/// `NodeContext::open` / `load_hub_state` can create default copies in a brand-new tempdir.
pub fn is_established_external_auth_node(
    data_dir: &Path,
    runtime: Option<&ModelHubRuntime>,
) -> bool {
    if node_access::enabled(data_dir) || data_dir.join(ONBOARDING_IN_PROGRESS_FILE).is_file() {
        return false;
    }
    has_provider_configured(data_dir, runtime)
        || data_dir.join("device_harnesses.json").is_file()
        || data_dir.join("bot-grants.json").is_file()
}

/// Ensures an uninitialized node has an active in-memory 6-digit single-use pairing code.
pub fn ensure_bootstrap_code(data_dir: &Path, now: i64) -> Option<String> {
    if onboarding_disabled_by_env()
        || node_access::enabled(data_dir)
        || data_dir.join(ONBOARDING_COMPLETED_FILE).is_file()
        || is_established_external_auth_node(data_dir, None)
    {
        return None;
    }
    let _ = std::fs::create_dir_all(data_dir);
    let key = canonical_key(data_dir);
    let mut map = bootstrap_store().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = map.get(&key) {
        if !existing.consumed && now < existing.expires_at {
            return Some(existing.code.clone());
        }
    }
    let code = generate_six_digit_code().ok()?;
    map.insert(
        key,
        BootstrapPairingState {
            code: code.clone(),
            created_at: now,
            expires_at: now + BOOTSTRAP_CODE_TTL_MS,
            failed_attempts: 0,
            locked_until: 0,
            consumed: false,
        },
    );
    Some(code)
}

/// Test/diagnostic helper to override expiry on an existing bootstrap code.
#[cfg(test)]
pub fn force_expire_bootstrap_code_for_test(data_dir: &Path, expired_at: i64) {
    let key = canonical_key(data_dir);
    let mut map = bootstrap_store().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = map.get_mut(&key) {
        entry.expires_at = expired_at;
    }
}

pub fn peek_bootstrap_code(data_dir: &Path, now: i64) -> Option<String> {
    if onboarding_disabled_by_env()
        || node_access::enabled(data_dir)
        || data_dir.join(ONBOARDING_COMPLETED_FILE).is_file()
        || is_established_external_auth_node(data_dir, None)
    {
        return None;
    }
    let key = canonical_key(data_dir);
    let map = bootstrap_store().lock().unwrap_or_else(|e| e.into_inner());
    map.get(&key)
        .filter(|s| !s.consumed && now < s.expires_at)
        .map(|s| s.code.clone())
}

pub fn has_provider_configured(data_dir: &Path, runtime: Option<&ModelHubRuntime>) -> bool {
    if let Some(rt) = runtime {
        return rt.has_any_provider_key();
    }
    ModelHubRuntime::with_file_vault(data_dir.to_path_buf())
        .map(|rt| rt.has_any_provider_key())
        .unwrap_or(false)
}

pub fn needs_onboarding(data_dir: &Path, runtime: Option<&ModelHubRuntime>) -> bool {
    if onboarding_disabled_by_env()
        || data_dir.join(ONBOARDING_COMPLETED_FILE).is_file()
        || is_established_external_auth_node(data_dir, runtime)
    {
        return false;
    }
    let password_configured = node_access::enabled(data_dir);
    if !password_configured {
        return true;
    }
    let provider_configured = has_provider_configured(data_dir, runtime);
    if !provider_configured {
        return true;
    }
    // If Step 1 & Step 2 happened inside onboarding, keep wizard open for Step 3 until completed/skipped.
    // Pre-existing configured nodes do not have ONBOARDING_IN_PROGRESS_FILE, so they return false.
    data_dir.join(ONBOARDING_IN_PROGRESS_FILE).is_file()
}

pub fn status_json(data_dir: &Path, runtime: Option<&ModelHubRuntime>, now: i64) -> Value {
    let password_configured = node_access::enabled(data_dir);
    let provider_configured = has_provider_configured(data_dir, runtime);
    let external_auth_established =
        is_established_external_auth_node(data_dir, runtime) || onboarding_disabled_by_env();
    let needs = needs_onboarding(data_dir, runtime);
    let pairing_code_required = !password_configured && !external_auth_established && needs;

    if pairing_code_required {
        let _ = ensure_bootstrap_code(data_dir, now);
    }

    let (expires_at, retry_after_sec) = {
        let key = canonical_key(data_dir);
        let map = bootstrap_store().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = map.get(&key) {
            let locked_sec = if now < entry.locked_until {
                ((entry.locked_until - now + 999) / 1000).max(1)
            } else {
                0
            };
            let exp = if !entry.consumed {
                Some(entry.expires_at)
            } else {
                None
            };
            (exp, locked_sec)
        } else {
            (None, 0)
        }
    };

    json!({
        "ok": true,
        "needsOnboarding": needs,
        "needs_onboarding": needs,
        "passwordConfigured": password_configured,
        "password_configured": password_configured,
        "providerConfigured": provider_configured,
        "provider_configured": provider_configured,
        "pairingCodeRequired": pairing_code_required,
        "pairing_code_required": pairing_code_required,
        "pairingExpiresAt": expires_at,
        "pairing_expires_at": expires_at,
        "lockedRetryAfterSec": retry_after_sec,
        "locked_retry_after_sec": retry_after_sec,
        "presets": bob_runtime::model_presets::list_dual_presets_json(),
    })
}

/// Step 1: Claim uninitialized node using single-use 6-digit pairing code and set owner password.
pub fn claim_node(data_dir: &Path, body: &Value, now: i64) -> (u16, Value) {
    if onboarding_disabled_by_env()
        || node_access::enabled(data_dir)
        || data_dir.join(ONBOARDING_COMPLETED_FILE).is_file()
        || is_established_external_auth_node(data_dir, None)
    {
        return (
            403,
            json!({
                "ok": false,
                "error": "ONBOARDING_ALREADY_CLAIMED",
                "message": "Node owner has already been claimed"
            }),
        );
    }

    let raw_code = body
        .get("pairingCode")
        .or_else(|| body.get("pairing_code"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let submitted_code = normalize_code(raw_code);

    let password = body
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("");

    let client_name = body
        .get("clientName")
        .or_else(|| body.get("client_name"))
        .and_then(Value::as_str)
        .unwrap_or("Bob Web Client");

    let key = canonical_key(data_dir);
    let mut map = bootstrap_store().lock().unwrap_or_else(|e| e.into_inner());

    let Some(entry) = map.get_mut(&key) else {
        return (
            403,
            json!({
                "ok": false,
                "error": "PAIRING_CODE_INVALID",
                "message": "Bootstrap pairing code is not active"
            }),
        );
    };

    if entry.consumed {
        return (
            403,
            json!({
                "ok": false,
                "error": "ONBOARDING_ALREADY_CLAIMED",
                "message": "Bootstrap pairing code has already been used"
            }),
        );
    }

    if now < entry.locked_until {
        let retry_sec = ((entry.locked_until - now + 999) / 1000).max(1);
        return (
            429,
            json!({
                "ok": false,
                "error": "PAIRING_RATE_LIMITED",
                "retryAfterSec": retry_sec,
                "retry_after_sec": retry_sec,
                "message": "Too many failed pairing attempts; please wait before retrying"
            }),
        );
    }

    if now >= entry.expires_at {
        return (
            403,
            json!({
                "ok": false,
                "error": "PAIRING_CODE_EXPIRED",
                "message": "Bootstrap pairing code has expired; restart bob-node to generate a fresh code"
            }),
        );
    }

    if !constant_time_eq_6(&submitted_code, &entry.code) {
        entry.failed_attempts = entry.failed_attempts.saturating_add(1);
        if entry.failed_attempts >= 3 {
            let shift = (entry.failed_attempts - 3).min(5);
            let backoff_ms = (15_000_i64.saturating_mul(1_i64 << shift)).min(300_000);
            entry.locked_until = now + backoff_ms;
            let retry_sec = ((backoff_ms + 999) / 1000).max(1);
            return (
                429,
                json!({
                    "ok": false,
                    "error": "PAIRING_RATE_LIMITED",
                    "retryAfterSec": retry_sec,
                    "retry_after_sec": retry_sec,
                    "message": "Too many invalid pairing code attempts"
                }),
            );
        }
        return (
            403,
            json!({
                "ok": false,
                "error": "PAIRING_CODE_INVALID",
                "message": "Invalid 6-digit pairing code"
            }),
        );
    }

    if password.len() < 12 {
        return (
            400,
            json!({
                "ok": false,
                "error": "PASSWORD_TOO_SHORT",
                "message": "Owner password must be at least 12 characters"
            }),
        );
    }
    if password.len() > 1024 {
        return (
            400,
            json!({
                "ok": false,
                "error": "NODE_PASSWORD_LENGTH",
                "message": "Owner password exceeds maximum length"
            }),
        );
    }

    // Consume single-use code atomically while holding bootstrap_store lock
    entry.consumed = true;

    if let Err(err) = node_access::initialize(data_dir, password) {
        if err == "NODE_ALREADY_CONFIGURED" {
            return (
                403,
                json!({
                    "ok": false,
                    "error": "ONBOARDING_ALREADY_CLAIMED",
                    "message": "Node owner has already been claimed"
                }),
            );
        }
        return (
            500,
            json!({
                "ok": false,
                "error": err,
                "message": "Failed to initialize node owner credentials"
            }),
        );
    }

    let _ = write_private_json(
        &data_dir.join(ONBOARDING_IN_PROGRESS_FILE),
        &json!({ "claimedAt": now }),
    );

    match node_access::issue_owner_token(data_dir, client_name, now) {
        Ok(tok) => {
            let secret = tok["access_token"].as_str().unwrap_or("").to_owned();
            let expires_at = tok["expires_at"].as_i64().unwrap_or(now + 3_600_000);
            let cred_id = tok["credential_id"].as_str().unwrap_or("").to_owned();
            (
                200,
                json!({
                    "ok": true,
                    "accessToken": secret,
                    "access_token": secret,
                    "credentialId": cred_id,
                    "credential_id": cred_id,
                    "expiresAt": expires_at,
                    "expires_at": expires_at,
                    "scope": "owner"
                }),
            )
        }
        Err(err) => (
            500,
            json!({
                "ok": false,
                "error": err,
                "message": "Owner initialized, please sign in with your password"
            }),
        ),
    }
}

fn verify_owner(data_dir: &Path, auth_header: Option<&str>, now: i64) -> Result<(), (u16, Value)> {
    match node_access::authorize(data_dir, auth_header, now) {
        Ok(ref scope) if scope == "owner" => Ok(()),
        Ok(_) => Err((
            403,
            json!({
                "ok": false,
                "error": "NODE_OWNER_REQUIRED",
                "message": "Owner session token required"
            }),
        )),
        Err(e) => Err((
            401,
            json!({
                "ok": false,
                "error": e,
                "message": "Authentication required"
            }),
        )),
    }
}

/// Step 2: Test connectivity (unless explicitly skipped) and persist API key + Clerk/Maker dual role preset.
pub fn configure_provider(
    state: &Arc<Mutex<NodeContext>>,
    data_dir: &Path,
    auth_header: Option<&str>,
    body: &Value,
    now: i64,
) -> (u16, Value) {
    if let Err(resp) = verify_owner(data_dir, auth_header, now) {
        return resp;
    }

    let provider_id = body
        .get("providerId")
        .or_else(|| body.get("provider_id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let api_key = body
        .get("apiKey")
        .or_else(|| body.get("api_key"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let base_url = body
        .get("baseUrl")
        .or_else(|| body.get("base_url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let clerk_model = body
        .get("clerkModel")
        .or_else(|| body.get("clerk_model"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let maker_model = body
        .get("makerModel")
        .or_else(|| body.get("maker_model"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let skip_connectivity = body
        .get("skipConnectivityTest")
        .or_else(|| body.get("skip_connectivity_test"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if provider_id.is_empty() {
        return (
            400,
            json!({
                "ok": false,
                "error": "PROVIDER_ID_REQUIRED",
                "message": "Provider ID is required"
            }),
        );
    }
    if api_key.is_empty() {
        return (
            400,
            json!({
                "ok": false,
                "error": "API_KEY_REQUIRED",
                "message": "API key is required"
            }),
        );
    }

    if !skip_connectivity {
        if let Err(err_msg) = bob_runtime::model_presets::probe_provider_connectivity(
            &provider_id,
            base_url.as_deref(),
            &api_key,
        ) {
            let code = if err_msg.starts_with("PROVIDER_AUTH_FAILED") {
                "PROVIDER_AUTH_FAILED"
            } else {
                "PROVIDER_CONNECTIVITY_FAILED"
            };
            return (
                400,
                json!({
                    "ok": false,
                    "error": code,
                    "message": err_msg
                }),
            );
        }
    }

    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(_) => {
            return (
                500,
                json!({
                    "ok": false,
                    "error": "NODE_LOCK_POISONED",
                    "message": "Internal node lock poisoned"
                }),
            );
        }
    };

    let ctx: &mut NodeContext = &mut *guard;
    match ctx.runtime.apply_provider_dual_preset_with_base_url(
        &mut ctx.app,
        &provider_id,
        Some(&api_key),
        base_url.as_deref(),
        clerk_model.as_deref(),
        maker_model.as_deref(),
    ) {
        Ok(snapshot) => {
            let active_roles = snapshot.get("activeRoles").cloned().unwrap_or_else(|| json!({}));
            let resolved_clerk = active_roles
                .get("clerk")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let resolved_maker = active_roles
                .get("maker")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            (
                200,
                json!({
                    "ok": true,
                    "providerId": provider_id,
                    "clerkRole": resolved_clerk,
                    "makerRole": resolved_maker,
                    "connectivityVerified": !skip_connectivity
                }),
            )
        }
        Err(code) => (
            400,
            json!({
                "ok": false,
                "error": code,
                "message": code
            }),
        ),
    }
}

fn sanitize_persona_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(64)
        .collect()
}

fn write_private_json(path: &Path, val: &Value) -> Result<(), &'static str> {
    let raw = serde_json::to_vec_pretty(val).map_err(|_| "ONBOARDING_SERIALIZE_FAILED")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &raw).map_err(|_| "ONBOARDING_WRITE_FAILED")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path).map_err(|_| "ONBOARDING_WRITE_FAILED")?;
    Ok(())
}

/// Step 3: Optionally persist assistant persona name to `SOUL.md` and mark onboarding completed.
pub fn complete_persona(
    data_dir: &Path,
    auth_header: Option<&str>,
    body: &Value,
    now: i64,
) -> (u16, Value) {
    if let Err(resp) = verify_owner(data_dir, auth_header, now) {
        return resp;
    }

    let raw_name = body
        .get("assistantName")
        .or_else(|| body.get("assistant_name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let safe_name = sanitize_persona_name(raw_name);

    if !safe_name.is_empty() {
        let soul_path = data_dir.join("SOUL.md");
        let soul_content = format!(
            "# Assistant Persona\n\nYour name is **{safe_name}**. Address yourself naturally as {safe_name} when appropriate, while remaining calm, concise, trustworthy, and helpful.\n"
        );
        let tmp_soul = data_dir.join("SOUL.md.tmp");
        if std::fs::write(&tmp_soul, soul_content.as_bytes()).is_err()
            || std::fs::rename(&tmp_soul, &soul_path).is_err()
        {
            return (
                500,
                json!({
                    "ok": false,
                    "error": "SOUL_WRITE_FAILED",
                    "message": "Failed to persist SOUL.md"
                }),
            );
        }
    }

    if let Err(e) = write_private_json(
        &data_dir.join(ONBOARDING_COMPLETED_FILE),
        &json!({
            "completedAt": now,
            "assistantName": if safe_name.is_empty() { Value::Null } else { json!(safe_name) }
        }),
    ) {
        return (
            500,
            json!({
                "ok": false,
                "error": e,
                "message": "Failed to persist onboarding completion receipt"
            }),
        );
    }

    let _ = std::fs::remove_file(data_dir.join(ONBOARDING_IN_PROGRESS_FILE));

    (
        200,
        json!({
            "ok": true,
            "assistantName": safe_name,
            "needsOnboarding": false
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{spawn_node_server, BobNodeConfig};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};

    fn http_json(
        addr: SocketAddr,
        method: &str,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
    ) -> (u16, Value) {
        let mut stream = TcpStream::connect(addr).expect("connect to bob-node");
        let payload = body
            .map(|b| serde_json::to_string(b).unwrap())
            .unwrap_or_default();
        let auth_line = bearer
            .map(|tok| format!("Authorization: Bearer {tok}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{auth_line}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        let (hdr, raw_body) = resp.split_once("\r\n\r\n").unwrap();
        let status: u16 = hdr
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let parsed = serde_json::from_str::<Value>(raw_body)
            .unwrap_or_else(|_| json!({ "raw": raw_body }));
        (status, parsed)
    }

    #[test]
    fn test_wp_e1_3_three_step_onboarding_pairing_rate_limit_frontrun_probe_and_soul_e2e() {
        // Spawn mock LLM /v1/models endpoint to validate Step 2 real connectivity test
        let models_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let models_addr = models_listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..4 {
                let Ok((mut sock, _)) = models_listener.accept() else {
                    break;
                };
                let mut reader = BufReader::new(sock.try_clone().unwrap());
                let mut auth_val = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some((k, v)) = trimmed.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("authorization") {
                            auth_val = v.trim().to_owned();
                        }
                    }
                }
                if auth_val == "Bearer sk-valid-onboarding-key" {
                    let body = r#"{"object":"list","data":[{"id":"deepseek-v4.1-flash-1017"},{"id":"deepseek-v4-pro"}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes());
                } else {
                    let body = r#"{"error":{"message":"Invalid API key"}}"#;
                    let resp = format!(
                        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes());
                }
                let _ = sock.flush();
            }
        });

        // Part A: Brute-force backoff protection on pairing code
        let dir_rate = tempfile::tempdir().unwrap();
        let srv_rate = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir_rate.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();
        let real_code_a = srv_rate
            .bootstrap_pairing_code()
            .expect("fresh node must generate 6-digit bootstrap code");
        assert_eq!(real_code_a.len(), 6);
        let wrong_code = if real_code_a == "000000" {
            "111111"
        } else {
            "000000"
        };

        let (s1, r1) = http_json(
            srv_rate.local_addr,
            "POST",
            "/api/node/v1/onboarding/claim",
            None,
            Some(&json!({ "pairingCode": wrong_code, "password": "ValidOwnerPassword123!" })),
        );
        assert_eq!(s1, 403);
        assert_eq!(r1["error"], "PAIRING_CODE_INVALID");

        let (s2, r2) = http_json(
            srv_rate.local_addr,
            "POST",
            "/api/node/v1/onboarding/claim",
            None,
            Some(&json!({ "pairingCode": wrong_code, "password": "ValidOwnerPassword123!" })),
        );
        assert_eq!(s2, 403);
        assert_eq!(r2["error"], "PAIRING_CODE_INVALID");

        let (s3, r3) = http_json(
            srv_rate.local_addr,
            "POST",
            "/api/node/v1/onboarding/claim",
            None,
            Some(&json!({ "pairingCode": wrong_code, "password": "ValidOwnerPassword123!" })),
        );
        assert_eq!(s3, 429);
        assert_eq!(r3["error"], "PAIRING_RATE_LIMITED");
        assert!(r3["retryAfterSec"].as_i64().unwrap_or(0) >= 15);
        srv_rate.shutdown();

        // Part B: Full 3-Step E2E Onboarding + Anti-Front-Running + Zero-Secret Status
        let dir = tempfile::tempdir().unwrap();
        let server = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        let boot_code = server
            .bootstrap_pairing_code()
            .expect("bootstrap code present");
        assert_eq!(boot_code.len(), 6);

        let (cap_status, cap_json) =
            http_json(server.local_addr, "GET", "/api/capabilities", None, None);
        assert_eq!(cap_status, 200);
        assert_eq!(cap_json["needsOnboarding"], true);

        let (st_status, st_json) = http_json(
            server.local_addr,
            "GET",
            "/api/node/v1/onboarding/status",
            None,
            None,
        );
        assert_eq!(st_status, 200);
        assert_eq!(st_json["needsOnboarding"], true);
        assert_eq!(st_json["passwordConfigured"], false);
        assert_eq!(st_json["providerConfigured"], false);
        assert_eq!(st_json["pairingCodeRequired"], true);
        assert_eq!(st_json["presets"].as_array().unwrap().len(), 10);
        let raw_status_dump = serde_json::to_string(&st_json).unwrap();
        assert!(
            !raw_status_dump.contains(&boot_code),
            "Public /onboarding/status must NEVER leak the 6-digit pairing code"
        );

        // Short password rejected without consuming bootstrap code
        let (short_st, short_res) = http_json(
            server.local_addr,
            "POST",
            "/api/node/v1/onboarding/claim",
            None,
            Some(&json!({ "pairingCode": boot_code, "password": "short" })),
        );
        assert_eq!(short_st, 400);
        assert_eq!(short_res["error"], "PASSWORD_TOO_SHORT");

        // Step 1 valid claim -> returns owner session token
        let (claim_st, claim_res) = http_json(
            server.local_addr,
            "POST",
            "/api/node/v1/onboarding/claim",
            None,
            Some(&json!({
                "pairingCode": format!("{}-{}", &boot_code[..3], &boot_code[3..]),
                "password": "AdminSecurePassword2026!",
                "clientName": "E2E Onboarding Browser"
            })),
        );
        assert_eq!(claim_st, 200);
        assert_eq!(claim_res["ok"], true);
        let owner_token = claim_res["accessToken"]
            .as_str()
            .expect("owner session token returned")
            .to_owned();
        assert_eq!(owner_token.len(), 64);

        // Anti-front-running: any subsequent claim attempt is immediately rejected with 403 ONBOARDING_ALREADY_CLAIMED
        let (dup_st, dup_res) = http_json(
            server.local_addr,
            "POST",
            "/api/node/v1/onboarding/claim",
            None,
            Some(&json!({
                "pairingCode": boot_code,
                "password": "AttackerPassword9999!"
            })),
        );
        assert_eq!(dup_st, 403);
        assert_eq!(dup_res["error"], "ONBOARDING_ALREADY_CLAIMED");

        // Mid-onboarding unauthenticated status check still works (200) and shows Step 2 pending
        let (mid_st, mid_json) = http_json(
            server.local_addr,
            "GET",
            "/api/node/v1/onboarding/status",
            None,
            None,
        );
        assert_eq!(mid_st, 200);
        assert_eq!(mid_json["needsOnboarding"], true);
        assert_eq!(mid_json["passwordConfigured"], true);
        assert_eq!(mid_json["providerConfigured"], false);

        // Step 2 without token -> 401
        let (unauth_st, _) = http_json(
            server.local_addr,
            "POST",
            "/api/node/v1/onboarding/provider",
            None,
            Some(&json!({ "providerId": "deepseek", "apiKey": "sk-valid-onboarding-key" })),
        );
        assert_eq!(unauth_st, 401);

        // Step 2 with invalid provider API key -> 400 PROVIDER_AUTH_FAILED
        let (bad_key_st, bad_key_res) = http_json(
            server.local_addr,
            "POST",
            "/api/node/v1/onboarding/provider",
            Some(&owner_token),
            Some(&json!({
                "providerId": "deepseek",
                "apiKey": "sk-wrong-key",
                "baseUrl": format!("http://{models_addr}/v1")
            })),
        );
        assert_eq!(bad_key_st, 400);
        assert_eq!(bad_key_res["error"], "PROVIDER_AUTH_FAILED");

        // Step 2 with valid API key -> connectivity verified & Clerk/Maker auto-paired
        let (prov_st, prov_res) = http_json(
            server.local_addr,
            "POST",
            "/api/node/v1/onboarding/provider",
            Some(&owner_token),
            Some(&json!({
                "providerId": "deepseek",
                "apiKey": "sk-valid-onboarding-key",
                "baseUrl": format!("http://{models_addr}/v1")
            })),
        );
        assert_eq!(prov_st, 200);
        assert_eq!(prov_res["ok"], true);
        assert_eq!(prov_res["connectivityVerified"], true);
        assert_eq!(
            prov_res["clerkRole"],
            "deepseek::deepseek-v4.1-flash-1017"
        );
        assert_eq!(prov_res["makerRole"], "deepseek::deepseek-v4-pro");

        // Step 3: Set optional persona -> writes SOUL.md and clears needsOnboarding
        let (pers_st, pers_res) = http_json(
            server.local_addr,
            "POST",
            "/api/node/v1/onboarding/persona",
            Some(&owner_token),
            Some(&json!({ "assistantName": "Jarvis-Prime" })),
        );
        assert_eq!(pers_st, 200);
        assert_eq!(pers_res["ok"], true);
        assert_eq!(pers_res["needsOnboarding"], false);

        let soul_md = std::fs::read_to_string(dir.path().join("SOUL.md"))
            .expect("SOUL.md must be written in data_dir");
        assert!(soul_md.contains("Jarvis-Prime"));

        let (done_st, done_json) = http_json(
            server.local_addr,
            "GET",
            "/api/node/v1/onboarding/status",
            None,
            None,
        );
        assert_eq!(done_st, 200);
        assert_eq!(done_json["needsOnboarding"], false);
        assert_eq!(done_json["passwordConfigured"], true);
        assert_eq!(done_json["providerConfigured"], true);
        assert!(!raw_status_dump.contains("sk-valid-onboarding-key"));

        server.shutdown();
    }

    #[test]
    fn test_established_external_auth_node_skips_onboarding_and_blocks_claim() {
        let dir_provider = tempfile::tempdir().unwrap();
        let mut app = bob_kernel::app::LocalApp::open(&dir_provider.path().join("kernel.sqlite")).unwrap();
        let rt = ModelHubRuntime::with_file_vault(dir_provider.path().to_path_buf()).unwrap();
        rt.apply_provider_dual_preset_with_base_url(
            &mut app,
            "deepseek",
            Some("sk-existing-provider-key"),
            None,
            None,
            None,
        )
        .unwrap();
        drop(rt);
        drop(app);

        let srv_provider = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir_provider.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();
        assert!(
            srv_provider.bootstrap_pairing_code().is_none(),
            "established external-auth node must never emit a 6-digit pairing code"
        );
        let (st, status) = http_json(
            srv_provider.local_addr,
            "GET",
            "/api/node/v1/onboarding/status",
            None,
            None,
        );
        assert_eq!(st, 200);
        assert_eq!(status["needsOnboarding"], false);
        assert_eq!(status["pairingCodeRequired"], false);

        let (claim_st, claim_res) = http_json(
            srv_provider.local_addr,
            "POST",
            "/api/node/v1/onboarding/claim",
            None,
            Some(&json!({ "pairingCode": "123456", "password": "AttemptOverride123!" })),
        );
        assert_eq!(claim_st, 403);
        assert_eq!(claim_res["error"], "ONBOARDING_ALREADY_CLAIMED");
        assert!(
            !dir_provider.path().join("node_access.sqlite").exists(),
            "claim must not create node_access.sqlite on an established external-auth node"
        );
        srv_provider.shutdown();

        let dir_harnesses = tempfile::tempdir().unwrap();
        std::fs::write(dir_harnesses.path().join("device_harnesses.json"), "{}").unwrap();
        let srv_harnesses = spawn_node_server(BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: dir_harnesses.path().to_path_buf(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();
        assert!(srv_harnesses.bootstrap_pairing_code().is_none());
        let (hst, hstatus) = http_json(
            srv_harnesses.local_addr,
            "GET",
            "/api/node/v1/onboarding/status",
            None,
            None,
        );
        assert_eq!(hst, 200);
        assert_eq!(hstatus["needsOnboarding"], false);
        assert_eq!(hstatus["pairingCodeRequired"], false);
        srv_harnesses.shutdown();
    }
}
