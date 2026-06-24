use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::accounts::get_wechat_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub conv_id: Option<String>,
    pub state: String, // "chat" | "selecting"
    #[serde(skip_serializing)]
    pub selecting_expiry: Option<u64>,
    #[serde(skip_serializing)]
    pub pending_list: Option<Vec<ConversationMeta>>,
}

pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, SessionState>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        };
        mgr.load();
        mgr
    }

    fn get_sessions_path() -> PathBuf {
        get_wechat_dir().join("sessions.json")
    }

    fn load(&mut self) {
        if let Ok(data) = fs::read_to_string(Self::get_sessions_path()) {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, SessionState>>(&data) {
                let mut sessions = self.sessions.lock().unwrap();
                *sessions = parsed;
                log::info!("[session-mgr] Loaded {} sessions", sessions.len());
            }
        }
    }

    fn save(&self) {
        let sessions = self.sessions.lock().unwrap();
        if let Ok(json) = serde_json::to_string_pretty(&*sessions) {
            let _ = fs::write(Self::get_sessions_path(), json);
        }
    }

    pub fn set_selecting(&self, wxid: &str, list: Vec<ConversationMeta>) {
        let mut sessions = self.sessions.lock().unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let mut state = sessions.get(wxid).cloned().unwrap_or_else(|| SessionState {
            conv_id: None,
            state: "chat".to_string(),
            selecting_expiry: None,
            pending_list: None,
        });
        state.state = "selecting".to_string();
        state.pending_list = Some(list);
        state.selecting_expiry = Some(now + 60_000); // 60 seconds TTL
        sessions.insert(wxid.to_string(), state);
        drop(sessions);
        self.save();
    }

    pub fn is_selecting(&self, wxid: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(wxid) {
            if state.state != "selecting" {
                return false;
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
            if let Some(expiry) = state.selecting_expiry {
                if now > expiry {
                    state.state = "chat".to_string();
                    state.pending_list = None;
                    state.selecting_expiry = None;
                    drop(sessions);
                    self.save();
                    return false;
                }
            }
            return true;
        }
        false
    }

    pub fn get_pending_list(&self, wxid: &str) -> Option<Vec<ConversationMeta>> {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(wxid).and_then(|s| s.pending_list.clone())
    }

    pub fn cancel_selecting(&self, wxid: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(wxid) {
            state.state = "chat".to_string();
            state.pending_list = None;
            state.selecting_expiry = None;
            drop(sessions);
            self.save();
        }
    }

    pub fn bind_session(&self, wxid: &str, conv_id: Option<String>) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(wxid.to_string(), SessionState {
            conv_id,
            state: "chat".to_string(),
            selecting_expiry: None,
            pending_list: None,
        });
        drop(sessions);
        self.save();
    }

    pub fn get_conv_id(&self, wxid: &str) -> Option<String> {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(wxid).and_then(|s| s.conv_id.clone())
    }

    pub fn get_all_wxids(&self) -> Vec<String> {
        let sessions = self.sessions.lock().unwrap();
        sessions.keys().cloned().collect()
    }
}
