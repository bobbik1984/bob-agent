use std::sync::Arc;
use std::time::Duration;
use log::{debug, error, info, warn};
use tokio::time::sleep;
use tokio::sync::watch;

use super::api::WechatApi;
use super::accounts::{load_sync_buf, save_sync_buf, resolve_wechat_account, get_default_account_id};
use super::types::{GetUpdatesReq, WeixinMessage};
use super::WechatState;

const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
const BACKOFF_DELAY_MS: u64 = 30_000;
const RETRY_DELAY_MS: u64 = 2_000;
const SESSION_EXPIRED_ERRCODE: i32 = -14;

/// Start the WeChat monitor loop. Called once on app startup and after QR login.
/// If a monitor is already running, it is stopped first.
pub fn start_monitor(state: Arc<WechatState>) {
    // Stop any existing monitor
    {
        let mut tx_lock = state.stop_monitor_tx.lock().unwrap();
        if let Some(tx) = tx_lock.take() {
            let _ = tx.send(true);
        }
    }

    // Determine which account to use
    let account_id = {
        let configured = state.account_id.read().unwrap().clone();
        configured.or_else(|| get_default_account_id())
    };

    let account_id = match account_id {
        Some(id) => id,
        None => {
            info!("[wechat] No account configured, monitor not started. Waiting for QR login.");
            *state.connected.write().unwrap() = false;
            return;
        }
    };

    let account = match resolve_wechat_account(Some(&account_id)) {
        Ok(acc) if acc.configured => acc,
        _ => {
            warn!("[wechat] Account {} not configured (no token), monitor not started.", account_id);
            *state.connected.write().unwrap() = false;
            return;
        }
    };

    // Write back the resolved account_id
    *state.account_id.write().unwrap() = Some(account_id.clone());

    let (stop_tx, stop_rx) = watch::channel(false);
    {
        let mut tx_lock = state.stop_monitor_tx.lock().unwrap();
        *tx_lock = Some(stop_tx);
    }

    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<WeixinMessage>();

    // Spawn the inbound message consumer — routes to per-user serial queues
    let state_clone = state.clone();
    tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            let wxid = msg.from_user_id.clone().unwrap_or_default();
            if wxid.is_empty() {
                continue;
            }
            state_clone.msg_queue.enqueue(wxid, msg, state_clone.clone());
        }
    });

    // Spawn the long-poll monitor
    let base_url = account.base_url.clone();
    let token = account.token.clone();
    tokio::spawn(async move {
        monitor_weixin_provider(account_id, base_url, token, stop_rx, msg_tx).await;
    });

    *state.connected.write().unwrap() = true;
    info!("[wechat] Monitor started");
}

#[derive(Clone)]
pub struct MonitorStatus {
    pub account_id: String,
    pub last_event_at: u64,
    pub last_inbound_at: u64,
}

pub async fn monitor_weixin_provider(
    account_id: String,
    base_url: String,
    token: Option<String>,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
    message_tx: tokio::sync::mpsc::UnboundedSender<WeixinMessage>,
) {
    info!("Weixin monitor started (account={})", account_id);
    
    let api = WechatApi::new(base_url, token);
    let mut get_updates_buf = load_sync_buf(&account_id).unwrap_or_default();
    
    if !get_updates_buf.is_empty() {
        debug!("Using previous get_updates_buf ({} bytes)", get_updates_buf.len());
    } else {
        info!("No previous get_updates_buf found, starting fresh");
    }

    let mut next_timeout_ms = DEFAULT_LONG_POLL_TIMEOUT_MS;
    let mut consecutive_failures = 0;

    loop {
        if *stop_rx.borrow() {
            info!("Monitor stopped (aborted)");
            break;
        }

        let req = GetUpdatesReq {
            sync_buf: None, // deprecated
            get_updates_buf: Some(get_updates_buf.clone()),
            base_info: None,
        };

        let result = tokio::select! {
            res = api.get_updates(req, next_timeout_ms) => res,
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    info!("Monitor stopped (aborted during get_updates)");
                    break;
                }
                continue;
            }
        };

        match result {
            Ok(resp) => {
                if let Some(to) = resp.longpolling_timeout_ms {
                    if to > 0 {
                        next_timeout_ms = to as u64;
                    }
                }

                let ret = resp.ret.unwrap_or(0);
                let errcode = resp.errcode.unwrap_or(0);

                if ret != 0 || errcode != 0 {
                    let is_session_expired = ret == SESSION_EXPIRED_ERRCODE || errcode == SESSION_EXPIRED_ERRCODE;
                    if is_session_expired {
                        error!("getUpdates: session expired, please re-login via QR");
                        // Wait for a long time or stop since token is invalid
                        consecutive_failures = 0;
                        sleep(Duration::from_secs(600)).await;
                        continue;
                    }

                    consecutive_failures += 1;
                    error!("getUpdates failed: ret={} errcode={} errmsg={:?} ({}/{})", 
                        ret, errcode, resp.errmsg, consecutive_failures, MAX_CONSECUTIVE_FAILURES);

                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        error!("getUpdates: {} consecutive failures, backing off", MAX_CONSECUTIVE_FAILURES);
                        consecutive_failures = 0;
                        sleep(Duration::from_millis(BACKOFF_DELAY_MS)).await;
                    } else {
                        sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                    continue;
                }

                // Success
                consecutive_failures = 0;
                
                if let Some(new_buf) = resp.get_updates_buf {
                    if !new_buf.is_empty() {
                        save_sync_buf(&account_id, &new_buf);
                        get_updates_buf = new_buf;
                    }
                }

                if let Some(msgs) = resp.msgs {
                    for msg in msgs {
                        if let Err(e) = message_tx.send(msg) {
                            error!("Failed to send message to processor queue: {}", e);
                        }
                    }
                }
            }
            Err(err) => {
                consecutive_failures += 1;
                error!("getUpdates error ({}/{}): {}", consecutive_failures, MAX_CONSECUTIVE_FAILURES, err);
                
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    consecutive_failures = 0;
                    sleep(Duration::from_millis(BACKOFF_DELAY_MS)).await;
                } else {
                    sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                }
            }
        }
    }
}
