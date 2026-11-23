use crate::cdp::CdpClient;
use crate::error::{BrowserError, BrowserResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiteSession {
    pub domain: String,
    pub label: String,
    #[serde(alias = "lastUsedAt")]
    pub last_used_at: u64,
    #[serde(alias = "cookiesCount")]
    pub cookies_count: usize,
    #[serde(skip_serializing_if = "Option::is_none", alias = "activeTakeoverId")]
    pub active_takeover_id: Option<String>,
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load_sessions(file_path: &Path) -> BrowserResult<Vec<SiteSession>> {
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(file_path)?;
    let sessions: Vec<SiteSession> = serde_json::from_str(&data)?;
    Ok(sessions)
}

pub fn save_sessions(file_path: &Path, sessions: &[SiteSession]) -> BrowserResult<()> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(sessions)?;
    fs::write(file_path, data)?;
    Ok(())
}

pub fn record_site_visit(
    file_path: &Path,
    domain: &str,
    label: &str,
    cookies_count: usize,
    takeover_id: Option<String>,
) -> BrowserResult<()> {
    let mut sessions = load_sessions(file_path)?;
    let now = now_unix_secs();

    if let Some(existing) = sessions.iter_mut().find(|s| s.domain == domain) {
        existing.last_used_at = now;
        existing.cookies_count = cookies_count;
        if !label.is_empty() {
            existing.label = label.to_string();
        }
        if takeover_id.is_some() {
            existing.active_takeover_id = takeover_id;
        }
    } else {
        sessions.push(SiteSession {
            domain: domain.to_string(),
            label: if label.is_empty() {
                domain.to_string()
            } else {
                label.to_string()
            },
            last_used_at: now,
            cookies_count,
            active_takeover_id: takeover_id,
        });
    }

    save_sessions(file_path, &sessions)?;
    Ok(())
}

/// Clears cookies for a specific domain or all cookies if domain == "*" via CDP.
pub fn clear_domain_cookies(cdp: &CdpClient, domain: &str) -> BrowserResult<usize> {
    if domain == "*" {
        // Clear all cookies and storage
        let _ = cdp.send_command("Network.clearBrowserCookies", None, Duration::from_secs(5));
        let _ = cdp.send_command(
            "Storage.clearDataForOrigin",
            Some(json!({
                "origin": "*",
                "storageTypes": "all"
            })),
            Duration::from_secs(5),
        );
        return Ok(0);
    }

    // Clear specific domain cookies
    let res = cdp.send_command("Network.getAllCookies", None, Duration::from_secs(5))?;
    let mut deleted_count = 0;

    if let Some(cookies_arr) = res.get("cookies").and_then(|v| v.as_array()) {
        for c in cookies_arr {
            let c_domain = c.get("domain").and_then(|v| v.as_str()).unwrap_or("");
            let c_name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let c_path = c.get("path").and_then(|v| v.as_str()).unwrap_or("/");

            let clean_c_domain = c_domain.trim_start_matches('.');
            let clean_target_domain = domain.trim_start_matches('.');

            if clean_c_domain == clean_target_domain
                || clean_c_domain.ends_with(&format!(".{}", clean_target_domain))
                || clean_target_domain.ends_with(&format!(".{}", clean_c_domain))
            {
                let params = json!({
                    "name": c_name,
                    "domain": c_domain,
                    "path": c_path,
                });
                let _ = cdp.send_command("Network.deleteCookies", Some(params), Duration::from_secs(3));
                deleted_count += 1;
            }
        }
    }

    let origin = if domain.starts_with("http://") || domain.starts_with("https://") {
        domain.to_string()
    } else {
        format!("https://{}", domain)
    };

    let _ = cdp.send_command(
        "Storage.clearDataForOrigin",
        Some(json!({
            "origin": origin,
            "storageTypes": "cookies,local_storage"
        })),
        Duration::from_secs(5),
    );

    Ok(deleted_count)
}

/// Clears session entry and triggers CDP cookie clearing if client is provided.
pub fn clear_site_session(
    file_path: &Path,
    cdp: Option<&CdpClient>,
    domain: &str,
) -> BrowserResult<()> {
    if let Some(client) = cdp {
        let _ = clear_domain_cookies(client, domain);
    }

    let mut sessions = load_sessions(file_path)?;
    if domain == "*" {
        sessions.clear();
    } else {
        sessions.retain(|s| s.domain != domain && s.domain != domain.trim_start_matches('.'));
    }

    save_sessions(file_path, &sessions)?;
    Ok(())
}

/// Security baseline check: strictly forbids exporting or downloading the raw Chromium SQLite cookie database.
pub fn assert_profile_download_forbidden(path: &Path) -> BrowserResult<()> {
    let p_str = path.to_string_lossy().to_lowercase();
    if p_str.contains("cookies")
        || p_str.contains("web data")
        || p_str.contains("login data")
        || p_str.contains("browser_profile")
        || p_str.contains("network/cookies")
    {
        return Err(BrowserError::RawProfileExportForbidden);
    }
    Ok(())
}
