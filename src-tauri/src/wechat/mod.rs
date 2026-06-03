//! wechat module
//! 
//! Rust native WeChat gateway implementation for Bob-Agent.

pub mod types;
pub mod api;
pub mod monitor;
pub mod accounts;
pub mod session_mgr;
pub mod msg_queue;
pub mod commands;
pub mod login_qr;
pub mod cdn;

use std::sync::{Arc, Mutex, RwLock};
use session_mgr::SessionManager;
use msg_queue::MessageQueue;
use tokio::sync::watch;

pub struct WechatState {
    pub session_mgr: Arc<SessionManager>,
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
            session_mgr: Arc::new(SessionManager::new()),
            msg_queue: Arc::new(MessageQueue::new()),
            account_id: RwLock::new(None),
            connected: RwLock::new(false),
            app: RwLock::new(None),
            stop_monitor_tx: Mutex::new(None),
        }
    }
}
