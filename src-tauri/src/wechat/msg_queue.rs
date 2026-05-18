use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use log::error;

use super::types::WeixinMessage;
use super::WechatState; // we will define this in mod.rs

pub struct MessageQueue {
    queues: Mutex<HashMap<String, mpsc::UnboundedSender<WeixinMessage>>>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            queues: Mutex::new(HashMap::new()),
        }
    }

    pub fn enqueue(&self, wxid: String, msg: WeixinMessage, state: Arc<WechatState>) {
        let mut queues = self.queues.lock().unwrap();
        
        if let Some(tx) = queues.get(&wxid) {
            if tx.send(msg.clone()).is_ok() {
                return;
            }
            // If send fails, the receiver is dropped, we'll recreate it.
        }

        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(msg).unwrap(); // should not fail
        queues.insert(wxid.clone(), tx);

        let wxid_clone = wxid.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                // Call commands routing
                if let Err(e) = super::commands::process_message(msg, state.clone()).await {
                    error!("[msg_queue] wxid={} task error: {}", wxid_clone, e);
                }
            }
        });
    }
}
