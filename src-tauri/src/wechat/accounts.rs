use crate::get_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

/// Normalize account ID: replace `@` and `.` with `-` so it's safe for filenames
/// and consistent across save / load paths.
pub fn normalize_account_id(raw: &str) -> String {
    raw.replace(['@', '.'], "-")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WechatAccountData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResolvedWechatAccount {
    pub account_id: String,
    pub base_url: String,
    pub token: Option<String>,
    pub enabled: bool,
    pub configured: bool,
}

pub fn get_wechat_dir() -> PathBuf {
    let mut path = get_data_dir();
    path.push("wechat");
    fs::create_dir_all(&path).unwrap_or_default();
    path
}

fn get_accounts_dir() -> PathBuf {
    let mut path = get_wechat_dir();
    path.push("accounts");
    fs::create_dir_all(&path).unwrap_or_default();
    path
}

fn resolve_account_path(account_id: &str) -> PathBuf {
    get_accounts_dir().join(format!("{}.json", account_id))
}

fn resolve_account_index_path() -> PathBuf {
    get_wechat_dir().join("accounts.json")
}

fn resolve_sync_buf_path(account_id: &str) -> PathBuf {
    get_accounts_dir().join(format!("{}.sync.json", account_id))
}

pub fn list_wechat_account_ids() -> Vec<String> {
    let path = resolve_account_index_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&data) {
            return parsed;
        }
    }
    Vec::new()
}

pub fn register_wechat_account_id(account_id: &str) {
    let id = normalize_account_id(account_id);
    let mut ids = list_wechat_account_ids();
    if !ids.contains(&id) {
        ids.push(id);
        if let Ok(json) = serde_json::to_string_pretty(&ids) {
            let _ = fs::write(resolve_account_index_path(), json);
        }
    }
}

pub fn unregister_wechat_account_id(account_id: &str) {
    let normalized = normalize_account_id(account_id);
    let ids = list_wechat_account_ids();
    let new_ids: Vec<String> = ids.into_iter().filter(|id| id != &normalized).collect();
    if let Ok(json) = serde_json::to_string_pretty(&new_ids) {
        let _ = fs::write(resolve_account_index_path(), json);
    }
}

pub fn clear_stale_accounts_for_user_id(current_account_id: &str, user_id: &str) {
    if user_id.trim().is_empty() {
        return;
    }
    let all_ids = list_wechat_account_ids();
    for id in all_ids {
        if id == current_account_id {
            continue;
        }
        if let Some(data) = load_wechat_account(&id) {
            if let Some(uid) = data.user_id {
                if uid.trim() == user_id.trim() {
                    log::info!("clear_stale_accounts_for_user_id: removing stale account={}", id);
                    clear_wechat_account(&id);
                    unregister_wechat_account_id(&id);
                }
            }
        }
    }
}

pub fn load_wechat_account(account_id: &str) -> Option<WechatAccountData> {
    let path = resolve_account_path(account_id);
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(parsed) = serde_json::from_str(&data) {
            return Some(parsed);
        }
    }
    None
}

pub fn save_wechat_account(account_id: &str, update: WechatAccountData) {
    let id = normalize_account_id(account_id);
    let mut existing = load_wechat_account(&id).unwrap_or_default();
    
    if let Some(token) = update.token {
        let trimmed = token.trim().to_string();
        if !trimmed.is_empty() {
            existing.token = Some(trimmed);
            existing.saved_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }
    if let Some(base_url) = update.base_url {
        let trimmed = base_url.trim().to_string();
        if !trimmed.is_empty() {
            existing.base_url = Some(trimmed);
        }
    }
    if let Some(user_id) = update.user_id {
        let trimmed = user_id.trim().to_string();
        if !trimmed.is_empty() {
            existing.user_id = Some(trimmed);
        } else {
            existing.user_id = None;
        }
    }
    existing.account_id = Some(id.clone());

    let path = resolve_account_path(&id);
    if let Ok(json) = serde_json::to_string_pretty(&existing) {
        let _ = fs::write(path, json);
    }
}

pub fn clear_wechat_account(account_id: &str) {
    let id = normalize_account_id(account_id);
    let dir = get_accounts_dir();
    let _ = fs::remove_file(dir.join(format!("{}.json", id)));
    let _ = fs::remove_file(dir.join(format!("{}.sync.json", id)));
}

pub fn load_sync_buf(account_id: &str) -> Option<String> {
    let path = resolve_sync_buf_path(account_id);
    if let Ok(data) = fs::read_to_string(&path) {
        // The buf could be stored as a raw string or JSON. Let's just store it as raw string.
        return Some(data);
    }
    None
}

pub fn save_sync_buf(account_id: &str, buf: &str) {
    let path = resolve_sync_buf_path(account_id);
    let _ = fs::write(path, buf);
}

pub fn resolve_wechat_account(account_id: Option<&str>) -> Result<ResolvedWechatAccount, String> {
    let mut raw = account_id.unwrap_or("").trim().to_string();
    if raw.is_empty() {
        if let Some(def_id) = get_default_account_id() {
            raw = def_id;
        } else {
            return Err("wechat: accountId is required and no default account found".to_string());
        }
    }
    let id = normalize_account_id(&raw);

    let account_data = load_wechat_account(&id);
    let token = account_data.as_ref().and_then(|a| a.token.clone());
    let state_base_url = account_data.as_ref().and_then(|a| a.base_url.clone()).unwrap_or_else(|| "".to_string());

    Ok(ResolvedWechatAccount {
        account_id: id,
        base_url: if state_base_url.is_empty() { DEFAULT_BASE_URL.to_string() } else { state_base_url },
        configured: token.is_some(),
        token,
        enabled: true,
    })
}

/// Helper to get the most recently used / only configured account
pub fn get_default_account_id() -> Option<String> {
    let ids = list_wechat_account_ids();
    if let Some(id) = ids.last() {
        return Some(id.clone());
    }
    None
}
