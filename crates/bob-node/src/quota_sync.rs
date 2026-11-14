//! Automatic incremental quota checkpoint synchronization to an external asset cockpit.
//! Configured strictly via environment variables (`BOB_QUOTA_CHECKPOINT_URL` and optional
//! `BOB_QUOTA_CHECKPOINT_TOKEN`) with zero hardcoded external endpoints.

use bob_runtime::reqwest;
use serde_json::{json, Map, Value};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PROVIDERS: [&str; 3] = ["codex", "antigravity", "claude"];

#[derive(Default)]
struct QuotaSyncState {
    last_pushed_fingerprint: Option<String>,
    in_flight: bool,
    last_synced_at: Option<u64>,
    last_recorded_count: u64,
    last_error: Option<String>,
}

struct InFlightGuard;

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut st) = sync_state().lock() {
            st.in_flight = false;
        }
    }
}

fn sync_state() -> &'static Mutex<QuotaSyncState> {
    static STATE: OnceLock<Mutex<QuotaSyncState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(QuotaSyncState::default()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn checkpoint_url() -> Option<String> {
    std::env::var("BOB_QUOTA_CHECKPOINT_URL")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn checkpoint_token() -> Option<String> {
    std::env::var("BOB_QUOTA_CHECKPOINT_TOKEN")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn pick_window<'a>(provider: &str, windows: &'a [Value]) -> Option<&'a Value> {
    let valid: Vec<&Value> = windows
        .iter()
        .filter(|w| {
            w.get("expired").and_then(Value::as_bool) != Some(true)
                && w.get("remaining_percent")
                    .and_then(Value::as_f64)
                    .is_some_and(|pct| pct.is_finite() && (0.0..=100.0).contains(&pct))
        })
        .collect();
    if valid.is_empty() {
        return None;
    }

    let preferred_id = if provider == "antigravity" {
        "gemini-weekly"
    } else {
        "weekly"
    };
    if let Some(found) = valid
        .iter()
        .copied()
        .find(|w| w.get("id").and_then(Value::as_str) == Some(preferred_id))
    {
        return Some(found);
    }
    if let Some(found) = valid.iter().copied().find(|w| {
        matches!(
            w.get("id").and_then(Value::as_str),
            Some("weekly" | "gemini-weekly" | "7d" | "3p-weekly")
        )
    }) {
        return Some(found);
    }
    valid
        .iter()
        .copied()
        .max_by_key(|w| w.get("resets_at").and_then(Value::as_u64).unwrap_or(0))
        .or_else(|| valid.first().copied())
}

/// Extract normalized provider weekly quota payload and a stable deduplication fingerprint.
pub fn extract_checkpoint_request(subscription_quotas: &Value) -> Option<(String, Value)> {
    let obj = subscription_quotas.as_object()?;
    let mut providers = Map::new();
    let mut fp_parts = Vec::new();

    for provider in PROVIDERS {
        let Some(item) = obj.get(provider) else {
            continue;
        };
        if item.get("status").and_then(Value::as_str) != Some("available") {
            continue;
        }
        let Some(windows) = item.get("windows").and_then(Value::as_array) else {
            continue;
        };
        let Some(win) = pick_window(provider, windows) else {
            continue;
        };
        let Some(remaining) = win.get("remaining_percent").and_then(Value::as_f64) else {
            continue;
        };
        let tenths = (remaining * 10.0).round() as i64;
        let pct = (tenths as f64) / 10.0;
        let resets_at = win.get("resets_at").and_then(Value::as_u64);
        let observed_at = item.get("observed_at").and_then(Value::as_u64);
        let source_node = item
            .get("source_node")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .chars()
            .take(64)
            .collect::<String>();

        fp_parts.push(format!("{provider}:{tenths}:{}", resets_at.unwrap_or(0)));
        providers.insert(
            provider.to_owned(),
            json!({
                "remaining_percent": pct,
                "resets_at": resets_at,
                "observed_at": observed_at,
                "source_node": source_node,
            }),
        );
    }

    if providers.is_empty() {
        return None;
    }

    Some((
        fp_parts.join("|"),
        json!({
            "source": "bob-node",
            "providers": providers,
        }),
    ))
}

/// Return public checkpoint sync status (never exposes configured URL or token).
pub fn status_snapshot() -> Value {
    let configured = checkpoint_url().is_some();
    let st = sync_state().lock().ok();
    match st {
        Some(guard) => json!({
            "configured": configured,
            "lastSyncedAt": guard.last_synced_at,
            "lastRecordedCount": guard.last_recorded_count,
            "inFlight": guard.in_flight,
            "lastError": guard.last_error,
        }),
        None => json!({
            "configured": configured,
            "lastSyncedAt": Value::Null,
            "lastRecordedCount": 0,
            "inFlight": false,
            "lastError": "LOCK_UNAVAILABLE",
        }),
    }
}

fn push_and_record(
    url: &str,
    token: Option<&str>,
    fingerprint: String,
    payload: Value,
) -> Option<u64> {
    let _guard = InFlightGuard;
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            if let Ok(mut st) = sync_state().lock() {
                st.last_error = Some("QUOTA_SYNC_CLIENT_ERROR".to_owned());
            }
            return None;
        }
    };

    let mut req = client.post(url).json(&payload);
    if let Some(tok) = token {
        req = req.bearer_auth(tok);
    }

    match req.send() {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                let body: Value = resp.json().unwrap_or_else(|_| json!({}));
                let recorded = body
                    .get("recordedCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                if let Ok(mut st) = sync_state().lock() {
                    st.last_pushed_fingerprint = Some(fingerprint);
                    st.last_synced_at = Some(now_secs());
                    st.last_recorded_count = recorded;
                    st.last_error = None;
                }
                Some(recorded)
            } else {
                if let Ok(mut st) = sync_state().lock() {
                    st.last_error = Some(format!("HTTP_{}", status.as_u16()));
                }
                None
            }
        }
        Err(_) => {
            if let Ok(mut st) = sync_state().lock() {
                st.last_error = Some("QUOTA_SYNC_REQUEST_FAILED".to_owned());
            }
            None
        }
    }
}

/// Non-blocking quota checkpoint trigger used during quota reads / topology sync.
pub fn maybe_sync_in_background(subscription_quotas: &Value) {
    let Some(url) = checkpoint_url() else {
        return;
    };
    let Some((fingerprint, payload)) = extract_checkpoint_request(subscription_quotas) else {
        return;
    };
    {
        let Ok(mut st) = sync_state().lock() else {
            return;
        };
        if st.in_flight || st.last_pushed_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            return;
        }
        st.in_flight = true;
    }
    let token = checkpoint_token();
    std::thread::spawn(move || {
        let _ = push_and_record(&url, token.as_deref(), fingerprint, payload);
    });
}

/// Synchronous checkpoint execution invoked explicitly via `app_quota_checkpoint_sync`.
pub fn sync_now(subscription_quotas: &Value) -> Value {
    let Some(url) = checkpoint_url() else {
        return json!({
            "configured": false,
            "synced": false,
            "reason": "QUOTA_CHECKPOINT_UNCONFIGURED",
            "status": status_snapshot(),
        });
    };
    let Some((fingerprint, payload)) = extract_checkpoint_request(subscription_quotas) else {
        return json!({
            "configured": true,
            "synced": false,
            "reason": "NO_AVAILABLE_QUOTAS",
            "status": status_snapshot(),
        });
    };
    {
        let Ok(mut st) = sync_state().lock() else {
            return json!({
                "configured": true,
                "synced": false,
                "reason": "LOCK_UNAVAILABLE",
                "status": status_snapshot(),
            });
        };
        if st.in_flight {
            return json!({
                "configured": true,
                "synced": false,
                "reason": "SYNC_IN_FLIGHT",
                "status": status_snapshot(),
            });
        }
        st.in_flight = true;
    }
    let token = checkpoint_token();
    let recorded = push_and_record(&url, token.as_deref(), fingerprint, payload);
    json!({
        "configured": true,
        "synced": recorded.is_some(),
        "recordedCount": recorded.unwrap_or(0),
        "status": status_snapshot(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_picks_weekly_windows_and_filters_unavailable_and_stale() {
        let quotas = json!({
            "codex": {
                "status": "available",
                "observed_at": 1700000000u64,
                "source_node": "vps2",
                "windows": [
                    { "id": "five_hour", "remaining_percent": 50.0, "resets_at": 1700018000u64, "expired": false },
                    { "id": "weekly", "remaining_percent": 82.44, "resets_at": 1700600000u64, "expired": false }
                ]
            },
            "antigravity": {
                "status": "available",
                "observed_at": 1700000050u64,
                "source_node": "local",
                "windows": [
                    { "id": "gemini-5h", "remaining_percent": 30.0, "resets_at": 1700018000u64, "expired": false },
                    { "id": "gemini-weekly", "remaining_percent": 95.0, "resets_at": 1700550000u64, "expired": false }
                ]
            },
            "claude": {
                "status": "stale",
                "observed_at": 1699990000u64,
                "windows": [
                    { "id": "weekly", "remaining_percent": 61.0, "resets_at": 1700500000u64, "expired": false }
                ]
            }
        });

        let (fp, payload) = extract_checkpoint_request(&quotas).expect("should extract available");
        assert_eq!(
            fp,
            "codex:824:1700600000|antigravity:950:1700550000"
        );
        assert_eq!(payload["providers"]["codex"]["remaining_percent"], 82.4);
        assert_eq!(payload["providers"]["antigravity"]["remaining_percent"], 95.0);
        assert!(payload["providers"].get("claude").is_none());
    }

    #[test]
    fn extract_falls_back_to_7d_then_largest_reset_and_ignores_expired() {
        let quotas = json!({
            "codex": {
                "status": "available",
                "observed_at": 1700000000u64,
                "windows": [
                    { "id": "weekly", "remaining_percent": 10.0, "resets_at": 1699999000u64, "expired": true },
                    { "id": "7d", "remaining_percent": 77.5, "resets_at": 1700600000u64, "expired": false }
                ]
            },
            "claude": {
                "status": "unavailable",
                "windows": []
            }
        });

        let (fp, payload) = extract_checkpoint_request(&quotas).expect("should pick 7d fallback");
        assert_eq!(fp, "codex:775:1700600000");
        assert_eq!(payload["providers"]["codex"]["remaining_percent"], 77.5);
    }

    #[test]
    fn sync_now_without_env_config_is_safe_noop() {
        let quotas = json!({
            "codex": {
                "status": "available",
                "observed_at": 1700000000u64,
                "windows": [{ "id": "weekly", "remaining_percent": 90.0, "resets_at": 1700600000u64, "expired": false }]
            }
        });
        let res = sync_now(&quotas);
        assert_eq!(res["configured"], false);
        assert_eq!(res["synced"], false);
    }
}
