//! Unified Node Harness Auth Maintenance & Lifecycle Adapter (`Phase B`).
//!
//! Standardizes the 5 internal node adapter actions across all 7 tools
//! (`dsh`, `agy`, `codex`, `claude`, `pi`, `hermes`, `openclaw`) per
//! `docs/wiki/harness-auth.md` and `BOB-D-010`:
//! 1. `discover_tool`: Probe binary/service presence, version, and profile/data roots.
//! 2. `check_auth`: Verify authentication with zero model token consumption, strictly
//!    distinguishing `network_unreachable` from `auth_required` / `pairing_required` / `scope_insufficient`.
//! 3. `recover_auth`: Execute deterministic non-interactive credential recovery where supported
//!    (DSH Cookie reuse -> bounded log startup token exchange; Hermes profile key reload; OpenClaw device token).
//! 4. `begin_login`: Return official login/pairing flow instructions when interactive auth is required,
//!    never copying or smuggling raw user credentials across machines.
//! 5. `verify_capabilities`: Separate documented support, host observations, and Bob
//!    integration evidence. Legacy support booleans remain unknown until verified.

use crate::{dsh_transport, now_millis};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const SUPPORTED_TOOLS: [&str; 7] = [
    "dsh",
    "agy",
    "codex",
    "claude",
    "pi",
    "hermes",
    "openclaw",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessAuthReport {
    pub connection_id: String,
    pub tool: String,
    pub action: String,
    pub installed: bool,
    pub auth_state: String,
    pub daemon_mode: String,
    pub anti_wrapping_compliant: bool,
    pub login_hint: Option<String>,
    pub capabilities: Value,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolAuthConfig {
    pub connection_id: String,
    pub tool: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub log_path: Option<PathBuf>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub device_token_present: Option<bool>,
    #[serde(default)]
    pub pairing_approved: Option<bool>,
    #[serde(default)]
    pub scopes_granted: Option<bool>,
    #[serde(default)]
    pub installed: Option<bool>,
    #[serde(default)]
    pub auth_cached: Option<bool>,
}

fn load_tool_configs(data_dir: &Path) -> Vec<ToolAuthConfig> {
    let path = data_dir.join("harness-auth-configs.json");
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    if bytes.len() > 131_072 {
        return Vec::new();
    }
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn find_config(data_dir: &Path, connection_id: &str, tool: &str) -> ToolAuthConfig {
    load_tool_configs(data_dir)
        .into_iter()
        .find(|c| c.connection_id == connection_id && c.tool == tool)
        .unwrap_or(ToolAuthConfig {
            connection_id: connection_id.to_owned(),
            tool: tool.to_owned(),
            ..Default::default()
        })
}

fn capabilities_for(tool: &str) -> Value {
    let preferred = match tool {
        "dsh" => "native_service",
        "agy" => "acp_or_stream_json",
        "codex" => "app_server_stdio",
        "claude" => "structured_cli",
        "pi" => "stdio_rpc",
        "hermes" => "http_sse",
        "openclaw" => "native_gateway",
        _ => "unknown",
    };
    json!({
        "schema_version": 1,
        "preferred_transport": preferred,
        "documented_support": {"stream_events": true, "basis": "harness-auth/6", "checked_on": "2026-11-04"},
        "observed_support": {
            "list_sessions": "unknown", "read_history": "unknown", "resume_chat": "unknown",
            "stream_events": "unknown", "cancel_execution": "unknown", "read_artifacts": "unknown", "approvals": "unknown"
        },
        "bob_integration": "unverified",
        "supports_stream": null, "supports_resume": null, "supports_directory": null,
        "compliance_status": "not_assessed"
    })
}

fn native_report(data: &Path, connection: &str, tool: &str, discovery_only: bool) -> HarnessAuthReport {
    let mut report = HarnessAuthReport {
        connection_id: connection.to_owned(), tool: tool.to_owned(),
        action: if discovery_only { "discover_tool" } else { "check_auth" }.to_owned(),
        installed: false, auth_state: "unknown".to_owned(), daemon_mode: "unknown".to_owned(),
        anti_wrapping_compliant: false, login_hint: None,
        capabilities: capabilities_for(tool), observed_at: now_millis(),
    };
    let config = find_config(data, connection, tool);
    if config.profile.as_deref().is_some_and(|p| !p.is_empty() && p != "default") {
        // Never probe another profile while presenting it as the requested identity.
        report.capabilities["probe"] = json!({"reason": "HARNESS_PROFILE_NOT_SUPPORTED", "installed_state": "unknown"});
        return report;
    }
    match crate::agy_directory::probe_native_harness(data, connection, tool, discovery_only) {
        Ok(observed) => {
            report.installed = observed["installed"].as_bool().unwrap_or(false);
            report.auth_state = observed["auth_state"].as_str().unwrap_or("unknown").to_owned();
            report.daemon_mode = observed["observed_transport"].as_str().unwrap_or("unknown").to_owned();
            if report.auth_state == "login_required" {
                report.login_hint = Some(official_login_hint(tool, None));
            }
            report.capabilities["observed_support"] = observed["capabilities"].clone();
            report.capabilities["probe"] = observed;
            // A probe verifies the upstream endpoint, not the chat UI/send path.
            report.capabilities["bob_integration"] = json!("probe_only");
        }
        Err(reason) => {
            report.capabilities["probe"] = json!({"reason": reason, "installed_state": "unknown"});
        }
    }
    report
}

fn official_login_hint(tool: &str, profile: Option<&str>) -> String {
    match tool {
        "dsh" => "Ensure DSH Host is running and provide its HTTPS endpoint or controlled startup log path for token-to-cookie exchange.".to_owned(),
        "agy" => "Run the official `agy` CLI under the same OS user on the node to complete interactive login; Bob never copies AGY credentials across machines.".to_owned(),
        "codex" => "Run `codex login` on the node via official CLI; Bob reuses the native local session state without extracting tokens.".to_owned(),
        "claude" => "Run `claude` on the node under the target OS user to complete official login or configure ANTHROPIC_API_KEY in the host environment.".to_owned(),
        "pi" => "Launch `pi` on the node to configure model provider credentials or `/login`; Bob communicates via `pi --mode rpc` stdio.".to_owned(),
        "hermes" => format!(
            "Configure `API_SERVER_KEY` in the Hermes profile (`{}`) on the node and reload the adapter.",
            profile.unwrap_or("default")
        ),
        "openclaw" => "Complete OpenClaw gateway device pairing and approve `operator.read` / `operator.write` scopes on the gateway host.".to_owned(),
        _ => "Unsupported tool".to_owned(),
    }
}

pub fn discover_tool(
    data_dir: &Path,
    connection_id: &str,
    tool: &str,
) -> Result<HarnessAuthReport, &'static str> {
    if !SUPPORTED_TOOLS.contains(&tool) {
        return Err("HARNESS_TOOL_UNSUPPORTED");
    }
    if ["pi", "codex", "claude", "agy"].contains(&tool) {
        return Ok(native_report(data_dir, connection_id, tool, true));
    }
    let cfg = find_config(data_dir, connection_id, tool);
    let installed = cfg.installed.unwrap_or_else(|| match tool {
        "dsh" => cfg.endpoint.is_some(),
        "agy" => data_dir.join("device_harnesses.json").exists(),
        "claude" => data_dir.join("claude-harness-grants.json").exists(),
        _ => false,
    });
    Ok(HarnessAuthReport {
        connection_id: connection_id.to_owned(),
        tool: tool.to_owned(),
        action: "discover_tool".to_owned(),
        installed,
        auth_state: if installed { "discovered" } else { "unconfigured" }.to_owned(),
        daemon_mode: "unknown".to_owned(),
        anti_wrapping_compliant: false,
        login_hint: if installed {
            None
        } else {
            Some(official_login_hint(tool, cfg.profile.as_deref()))
        },
        capabilities: capabilities_for(tool),
        observed_at: now_millis(),
    })
}

pub fn check_auth(
    data_dir: &Path,
    connection_id: &str,
    tool: &str,
) -> Result<HarnessAuthReport, &'static str> {
    if !SUPPORTED_TOOLS.contains(&tool) {
        return Err("HARNESS_TOOL_UNSUPPORTED");
    }
    if ["pi", "codex", "claude", "agy"].contains(&tool) {
        return Ok(native_report(data_dir, connection_id, tool, false));
    }
    let cfg = find_config(data_dir, connection_id, tool);
    let (installed, auth_state, hint) = match tool {
        "dsh" => {
            if let Some(endpoint) = cfg.endpoint.as_deref() {
                if let Some(cookie) = dsh_transport::load_cached_cookie(data_dir, endpoint) {
                    match dsh_transport::DshClient::from_cookie(endpoint, &cookie)
                        .and_then(|c| c.check_auth())
                    {
                        Ok(()) => (true, "session_reused", None),
                        Err("DSH_HISTORY_TIMEOUT" | "DSH_HISTORY_UNAVAILABLE" | "DSH_UNAVAILABLE") => {
                            // Network unreachable: preserve cached Cookie, report network_unreachable!
                            (true, "network_unreachable", None)
                        }
                        Err(_) => (
                            true,
                            "auth_required",
                            Some(official_login_hint("dsh", None)),
                        ),
                    }
                } else {
                    (
                        true,
                        "auth_required",
                        Some(official_login_hint("dsh", None)),
                    )
                }
            } else {
                (false, "unconfigured", Some(official_login_hint("dsh", None)))
            }
        }
        "openclaw" => {
            let installed = cfg.installed.unwrap_or(cfg.endpoint.is_some());
            if !installed {
                (false, "unconfigured", Some(official_login_hint("openclaw", None)))
            } else if cfg.device_token_present == Some(false) {
                (
                    true,
                    "auth_required",
                    Some("OpenClaw gateway token or device token is missing or invalid.".to_owned()),
                )
            } else if cfg.pairing_approved == Some(false) {
                (
                    true,
                    "pairing_required",
                    Some("OpenClaw device pairing is pending or revoked; approve pairing on gateway.".to_owned()),
                )
            } else if cfg.scopes_granted == Some(false) {
                (
                    true,
                    "scope_insufficient",
                    Some("OpenClaw device token lacks required operator scopes.".to_owned()),
                )
            } else if cfg.auth_cached == Some(true) {
                (true, "unknown", None)
            } else {
                (true, "auth_required", Some(official_login_hint("openclaw", None)))
            }
        }
        "hermes" => {
            let installed = cfg.installed.unwrap_or(cfg.endpoint.is_some());
            let has_key = cfg
                .token
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
                || cfg
                    .api_key_env
                    .as_deref()
                    .and_then(|k| std::env::var(k).ok())
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);
            if !installed {
                (
                    false,
                    "unconfigured",
                    Some(official_login_hint("hermes", cfg.profile.as_deref())),
                )
            } else if has_key {
                (true, "unknown", None)
            } else {
                (
                    true,
                    "auth_required",
                    Some(official_login_hint("hermes", cfg.profile.as_deref())),
                )
            }
        }
        _ => return Err("HARNESS_TOOL_UNSUPPORTED"),
    };

    Ok(HarnessAuthReport {
        connection_id: connection_id.to_owned(),
        tool: tool.to_owned(),
        action: "check_auth".to_owned(),
        installed,
        auth_state: auth_state.to_owned(),
        daemon_mode: "unknown".to_owned(),
        anti_wrapping_compliant: false,
        login_hint: hint,
        capabilities: capabilities_for(tool),
        observed_at: now_millis(),
    })
}

pub fn recover_auth(
    data_dir: &Path,
    connection_id: &str,
    tool: &str,
) -> Result<HarnessAuthReport, &'static str> {
    if !SUPPORTED_TOOLS.contains(&tool) {
        return Err("HARNESS_TOOL_UNSUPPORTED");
    }
    let cfg = find_config(data_dir, connection_id, tool);
    match tool {
        "dsh" => {
            let Some(endpoint) = cfg.endpoint.as_deref() else {
                return check_auth(data_dir, connection_id, tool);
            };
            // Step 1-2: Check existing saved session Cookie first
            let initial = check_auth(data_dir, connection_id, tool)?;
            if initial.auth_state == "session_reused" || initial.auth_state == "network_unreachable" {
                return Ok(HarnessAuthReport {
                    action: "recover_auth".to_owned(),
                    ..initial
                });
            }
            // Step 3: Deterministically extract token from configured token or bounded log tail
            let candidate_token = if let Some(tok) = cfg.token.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(tok.trim().to_owned())
            } else if let Some(log_path) = cfg.log_path.as_deref() {
                dsh_transport::extract_startup_token_from_log_tail(log_path, 64 * 1024).ok()
            } else {
                None
            };
            let Some(token) = candidate_token else {
                return Ok(HarnessAuthReport {
                    action: "recover_auth".to_owned(),
                    auth_state: "auth_required".to_owned(),
                    login_hint: Some(official_login_hint("dsh", None)),
                    ..initial
                });
            };
            match dsh_transport::DshClient::connect(endpoint, &token) {
                Ok(client) => {
                    let _ = dsh_transport::save_cached_cookie_atomic(data_dir, endpoint, client.cookie());
                    Ok(HarnessAuthReport {
                        action: "recover_auth".to_owned(),
                        installed: true,
                        auth_state: "token_recovered".to_owned(),
                        login_hint: None,
                        ..initial
                    })
                }
                Err("DSH_UNAVAILABLE" | "DSH_HISTORY_TIMEOUT" | "DSH_HISTORY_UNAVAILABLE") => {
                    Ok(HarnessAuthReport {
                        action: "recover_auth".to_owned(),
                        installed: true,
                        auth_state: "network_unreachable".to_owned(),
                        ..initial
                    })
                }
                Err(_) => Ok(HarnessAuthReport {
                    action: "recover_auth".to_owned(),
                    installed: true,
                    auth_state: "auth_required".to_owned(),
                    login_hint: Some(official_login_hint("dsh", None)),
                    ..initial
                }),
            }
        }
        "hermes" => {
            let mut report = check_auth(data_dir, connection_id, tool)?;
            report.action = "recover_auth".to_owned();
            if report.auth_state == "authenticated" {
                report.auth_state = "token_recovered".to_owned();
            }
            Ok(report)
        }
        _ => {
            // AGY, Codex, Claude Code, Pi, OpenClaw do not fabricate or scrape tokens;
            // they either reuse official local state or direct the user to begin_login.
            let mut report = check_auth(data_dir, connection_id, tool)?;
            report.action = "recover_auth".to_owned();
            Ok(report)
        }
    }
}

pub fn begin_login(
    data_dir: &Path,
    connection_id: &str,
    tool: &str,
) -> Result<HarnessAuthReport, &'static str> {
    if !SUPPORTED_TOOLS.contains(&tool) {
        return Err("HARNESS_TOOL_UNSUPPORTED");
    }
    let cfg = find_config(data_dir, connection_id, tool);
    let mut report = check_auth(data_dir, connection_id, tool)?;
    report.action = "begin_login".to_owned();
    report.login_hint = Some(official_login_hint(tool, cfg.profile.as_deref()));
    Ok(report)
}

pub fn verify_capabilities(
    data_dir: &Path,
    connection_id: &str,
    tool: &str,
) -> Result<HarnessAuthReport, &'static str> {
    let mut report = check_auth(data_dir, connection_id, tool)?;
    report.action = "verify_capabilities".to_owned();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_seven_tool_identifiers_without_claiming_verification() {
        let dir = tempfile::tempdir().unwrap();
        for tool in SUPPORTED_TOOLS {
            let disc = discover_tool(dir.path(), "node-1", tool).unwrap();
            assert_eq!(disc.tool, tool);
            assert!(!disc.anti_wrapping_compliant);
            let login = begin_login(dir.path(), "node-1", tool).unwrap();
            assert!(login.login_hint.is_some());
        }
        assert_eq!(
            discover_tool(dir.path(), "node-1", "unknown_cli").unwrap_err(),
            "HARNESS_TOOL_UNSUPPORTED"
        );
    }

    #[test]
    fn configuration_cannot_claim_live_native_auth_or_streaming() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("harness-auth-configs.json"),
            br#"[{"connectionId":"c1","tool":"codex","installed":true,"authCached":true}]"#).unwrap();
        let report = verify_capabilities(dir.path(), "c1", "codex").unwrap();
        assert_eq!(report.auth_state, "unknown");
        assert_eq!(report.daemon_mode, "unknown");
        assert_eq!(report.capabilities["probe"]["reason"], "HARNESS_HOST_GRANT_REQUIRED");
        assert!(report.capabilities["supports_stream"].is_null());
        assert_eq!(report.capabilities["preferred_transport"], "app_server_stdio");
        assert!(!report.anti_wrapping_compliant);
    }

    #[test]
    fn non_default_profile_cannot_silently_probe_the_default_identity() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("harness-auth-configs.json"),
            br#"[{"connectionId":"c1","tool":"pi","profile":"other"}]"#).unwrap();
        let report = check_auth(dir.path(), "c1", "pi").unwrap();
        assert_eq!(report.auth_state, "unknown");
        assert_eq!(report.capabilities["probe"]["reason"], "HARNESS_PROFILE_NOT_SUPPORTED");
    }

    #[test]
    fn a_connection_cannot_probe_an_ungranted_tool() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("device_harnesses.json"),
            br#"[{"connectionId":"c1","sshTarget":"local","harnesses":["agy"]}]"#).unwrap();
        let report = verify_capabilities(dir.path(), "c1", "codex").unwrap();
        assert_eq!(report.capabilities["probe"]["reason"], "HARNESS_TOOL_NOT_GRANTED");
    }

    #[test]
    fn openclaw_distinguishes_token_pairing_and_scope_states() {
        let dir = tempfile::tempdir().unwrap();
        let configs = json!([
            {
                "connectionId": "c1",
                "tool": "openclaw",
                "installed": true,
                "deviceTokenPresent": true,
                "pairingApproved": false,
                "scopesGranted": true
            },
            {
                "connectionId": "c2",
                "tool": "openclaw",
                "installed": true,
                "deviceTokenPresent": true,
                "pairingApproved": true,
                "scopesGranted": false
            },
            {
                "connectionId": "c3",
                "tool": "openclaw",
                "installed": true,
                "deviceTokenPresent": true,
                "pairingApproved": true,
                "scopesGranted": true,
                "authCached": true
            }
        ]);
        std::fs::write(
            dir.path().join("harness-auth-configs.json"),
            serde_json::to_vec(&configs).unwrap(),
        )
        .unwrap();

        let r1 = check_auth(dir.path(), "c1", "openclaw").unwrap();
        assert_eq!(r1.auth_state, "pairing_required");

        let r2 = check_auth(dir.path(), "c2", "openclaw").unwrap();
        assert_eq!(r2.auth_state, "scope_insufficient");

        let r3 = check_auth(dir.path(), "c3", "openclaw").unwrap();
        assert_eq!(r3.auth_state, "unknown");
    }

    #[test]
    fn dsh_network_unreachable_preserves_cached_cookie_without_invalidating() {
        let dir = tempfile::tempdir().unwrap();
        let endpoint = "https://127.0.0.1:1/";
        dsh_transport::save_cached_cookie_atomic(dir.path(), endpoint, "dsh-auth-test=valid_cookie_123").unwrap();
        let configs = json!([
            {
                "connectionId": "dsh-node",
                "tool": "dsh",
                "endpoint": endpoint,
                "installed": true
            }
        ]);
        std::fs::write(
            dir.path().join("harness-auth-configs.json"),
            serde_json::to_vec(&configs).unwrap(),
        )
        .unwrap();

        let report = recover_auth(dir.path(), "dsh-node", "dsh").unwrap();
        assert_eq!(report.auth_state, "network_unreachable");
        // Cookie remains intact on disk!
        assert_eq!(
            dsh_transport::load_cached_cookie(dir.path(), endpoint).as_deref(),
            Some("dsh-auth-test=valid_cookie_123")
        );
    }
}
