//! wechat module
//!
//! Rust native WeChat gateway implementation for Bob-Agent.

pub mod accounts;
pub mod api;
pub mod cdn;
pub mod commands;
pub mod login_qr;
pub mod monitor;
pub mod msg_queue;
pub mod types;

use msg_queue::MessageQueue;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::watch;

pub struct WechatState {
    pub msg_queue: Arc<MessageQueue>,
    pub account_id: RwLock<Option<String>>,
    pub connected: RwLock<bool>,
    pub app: RwLock<Option<tauri::AppHandle>>,
    // Sender used to stop the monitor loop when disconnected
    pub stop_monitor_tx: Mutex<Option<watch::Sender<bool>>>,
}

impl WechatState {
    pub fn new() -> Self {
        Self {
            msg_queue: Arc::new(MessageQueue::new()),
            account_id: RwLock::new(None),
            connected: RwLock::new(false),
            app: RwLock::new(None),
            stop_monitor_tx: Mutex::new(None),
        }
    }
}
