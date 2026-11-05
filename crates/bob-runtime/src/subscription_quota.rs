//! Sanitized local quota snapshots produced by authorized provider adapters.
use serde_json::{json, Value};
use std::{path::Path, time::{SystemTime, UNIX_EPOCH}};

pub fn read(data_dir: &Path) -> Value {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let path = data_dir.join("subscription-quota-cache.json");
    let snapshot = std::fs::metadata(&path).ok().filter(|m| m.len() <= 65536)
        .and_then(|_| std::fs::read(&path).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null);
    sanitize(&snapshot, now)
}

fn sanitize(snapshot: &Value, now: u64) -> Value {
    let mut result = json!({});
    for provider in ["codex", "antigravity", "claude"] {
        let unavailable = json!({"status":"unavailable", "reason":"PROVIDER_QUOTA_NOT_CONNECTED", "windows":[]});
        if snapshot["version"] != 1 { result[provider] = unavailable; continue; }
        let item = &snapshot["providers"][provider];
        let Some(observed) = item["observed_at"].as_u64().filter(|v| *v > 0 && *v <= now + 60) else {
            result[provider] = unavailable; continue;
        };
        let kind = item["source_kind"].as_str().unwrap_or("");
        if !matches!((provider, kind), ("codex", "codex_app_server") | ("antigravity", "official_statusline") | ("claude", "official_statusline")) {
            result[provider] = unavailable; continue;
        }
        let windows: Vec<Value> = item["windows"].as_array().into_iter().flatten().take(12).filter_map(|w| {
            let id = w["id"].as_str()?;
            if id.is_empty() || id.len() > 64 { return None; }
            let remaining = w["remaining_percent"].as_f64()?;
            if !remaining.is_finite() || !(0.0..=100.0).contains(&remaining) { return None; }
            let reset = w["resets_at"].as_u64().filter(|n| *n > 0);
            Some(json!({"id":id,"remaining_percent":remaining,"resets_at":reset,"expired":reset.is_some_and(|v| v <= now)}))
        }).collect();
        if windows.is_empty() { result[provider] = unavailable; continue; }
        let stale = now.saturating_sub(observed) > 900 || item["last_error"].as_str().is_some()
            || windows.iter().any(|w| w["expired"] == true);
        result[provider] = json!({"status":if stale {"stale"} else {"available"},
            "observed_at":observed,"source_kind":kind,
            "source_node":item["source_node"].as_str().unwrap_or("").chars().take(64).collect::<String>(),
            "windows":windows});
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value { json!({"version":1,"providers":{"codex":{"observed_at":1000,"source_kind":"codex_app_server","source_node":"fixture","secret":"must not escape","windows":[{"id":"weekly","remaining_percent":98.0,"resets_at":5000}]}}}) }
    #[test] fn quota_cache_missing_and_malformed_are_unavailable() {
        assert_eq!(sanitize(&Value::Null, 1000)["codex"]["status"], "unavailable");
        let mut s=sample();s["providers"]["codex"]["windows"][0]["remaining_percent"]=json!(101);
        assert_eq!(sanitize(&s,1000)["codex"]["status"],"unavailable");
    }
    #[test] fn quota_cache_is_sanitized_and_never_refreshes_observation_time() {
        let fresh=sanitize(&sample(),1100);assert_eq!(fresh["codex"]["status"],"available");
        assert!(fresh["codex"].get("secret").is_none());
        let old=sanitize(&sample(),2000);assert_eq!(old["codex"]["status"],"stale");assert_eq!(old["codex"]["observed_at"],1000);
        let expired=sanitize(&sample(),5001);assert_eq!(expired["codex"]["windows"][0]["expired"],true);
    }
    #[test] fn quota_cache_future_or_untrusted_source_is_rejected() {
        let mut s=sample();s["providers"]["codex"]["observed_at"]=json!(99999);
        assert_eq!(sanitize(&s,1000)["codex"]["status"],"unavailable");
        let mut s=sample();s["providers"]["codex"]["source_kind"]=json!("terminal_scraping");
        assert_eq!(sanitize(&s,1000)["codex"]["status"],"unavailable");
    }
}
