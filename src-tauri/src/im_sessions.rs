use lazy_static::lazy_static;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

lazy_static! {
    pub static ref SESSION_MANAGER: Arc<SessionManager> = Arc::new(SessionManager::new());
}

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
        crate::get_data_dir().join("im_sessions.json")
    }

    fn open_db() -> Option<Connection> {
        let db_path = crate::get_data_dir().join("bob.db");
        Connection::open(db_path).ok()
    }

    fn load(&mut self) {
        if let Ok(data) = fs::read_to_string(Self::get_sessions_path()) {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, SessionState>>(&data) {
                let mut sessions = self.sessions.lock().unwrap();
                *sessions = parsed;
                log::info!("[im-sessions] Loaded {} sessions", sessions.len());
            }
        }
    }

    fn save(&self) {
        let sessions = self.sessions.lock().unwrap();
        if let Ok(json) = serde_json::to_string_pretty(&*sessions) {
            let _ = fs::write(Self::get_sessions_path(), json);
        }
    }

    pub fn set_selecting(&self, user_id: &str, list: Vec<ConversationMeta>) {
        let mut sessions = self.sessions.lock().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let mut state = sessions
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| SessionState {
                conv_id: None,
                state: "chat".to_string(),
                selecting_expiry: None,
                pending_list: None,
            });
        state.state = "selecting".to_string();
        state.pending_list = Some(list);
        state.selecting_expiry = Some(now + 60_000); // 60 seconds TTL
        sessions.insert(user_id.to_string(), state);
        drop(sessions);
        self.save();
    }

    pub fn is_selecting(&self, user_id: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(user_id) {
            if state.state != "selecting" {
                return false;
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
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

    pub fn get_pending_list(&self, user_id: &str) -> Option<Vec<ConversationMeta>> {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(user_id).and_then(|s| s.pending_list.clone())
    }

    pub fn cancel_selecting(&self, user_id: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(user_id) {
            state.state = "chat".to_string();
            state.pending_list = None;
            state.selecting_expiry = None;
            drop(sessions);
            self.save();
        }
    }

    pub fn bind_session(&self, user_id: &str, conv_id: Option<String>) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(
            user_id.to_string(),
            SessionState {
                conv_id,
                state: "chat".to_string(),
                selecting_expiry: None,
                pending_list: None,
            },
        );
        drop(sessions);
        self.save();
    }

    pub fn get_conv_id(&self, user_id: &str) -> Option<String> {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(user_id).and_then(|s| s.conv_id.clone())
    }

    pub fn get_all_user_ids(&self) -> Vec<String> {
        let sessions = self.sessions.lock().unwrap();
        sessions.keys().cloned().collect()
    }
}

// 帮助函数：格式化时间戳
fn format_relative_time(ts_ms: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let diff = now - ts_ms;
    if diff < 60_000 {
        "刚刚".to_string()
    } else if diff < 3600_000 {
        format!("{}分钟前", diff / 60_000)
    } else if diff < 86400_000 {
        format!("{}小时前", diff / 3600_000)
    } else {
        format!("{}天前", diff / 86400_000)
    }
}

/// 处理系统级 IM 指令（如果返回 Some，代表已被拦截，IM 服务应直接回复此字符串并终止大模型处理）
pub fn handle_im_command(user_id: &str, text: &str) -> Option<String> {
    let text = text.trim();

    if text == "/help" {
        return Some("指令菜单：\n/sessions — 列出历史会话供切换\n/new      — 开启全新会话\n/status   — 查看当前会话状态\n/help     — 显示帮助\n\n直接发送文字即与 Bob 对话 💬".to_string());
    }

    if text == "/new" {
        SESSION_MANAGER.bind_session(user_id, None);
        return Some("✅ 已开启全新会话, 接下来的消息将进入新对话。".to_string());
    }

    if text == "/status" {
        if let Some(conv_id) = SESSION_MANAGER.get_conv_id(user_id) {
            let short = if conv_id.len() > 8 {
                &conv_id[..8]
            } else {
                &conv_id
            };
            return Some(format!("当前会话：{}…（发送 /sessions 可切换）", short));
        } else {
            return Some("当前无活跃会话（下条消息将自动新建）".to_string());
        }
    }

    if text == "/sessions" || text == "/chat" || text == "/list" {
        let conn = match SessionManager::open_db() {
            Some(c) => c,
            None => return Some("❌ 数据库连接失败".to_string()),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC LIMIT 8",
        ) {
            Ok(s) => s,
            Err(_) => return Some("❌ 数据库查询失败".to_string()),
        };

        let rows = stmt.query_map([], |row| {
            Ok(ConversationMeta {
                id: row.get(0)?,
                title: row.get(1)?,
                updated_at: row.get(2)?,
            })
        });

        let mut list = Vec::new();
        if let Ok(iter) = rows {
            for row in iter.flatten() {
                list.push(row);
            }
        }

        if list.is_empty() {
            return Some("暂无历史会话，直接发消息开始吧 💬".to_string());
        }

        SESSION_MANAGER.set_selecting(user_id, list.clone());
        let mut reply = "📋 请回复序号切换会话（60 秒内有效）：\n".to_string();
        for (i, c) in list.iter().enumerate() {
            let title = if c.title.is_empty() {
                "未命名对话"
            } else {
                &c.title
            };
            let time_label = format_relative_time(c.updated_at);
            reply.push_str(&format!("[{}] {} ({}）\n", i + 1, title, time_label));
        }
        reply.push_str("[0] 开启全新会话");
        return Some(reply);
    }

    if SESSION_MANAGER.is_selecting(user_id) {
        if let Ok(idx) = text.parse::<usize>() {
            if idx == 0 {
                SESSION_MANAGER.bind_session(user_id, None);
                return Some("✅ 已开启全新会话。".to_string());
            }
            if let Some(list) = SESSION_MANAGER.get_pending_list(user_id) {
                if idx > 0 && idx <= list.len() {
                    let target = &list[idx - 1];
                    SESSION_MANAGER.bind_session(user_id, Some(target.id.clone()));
                    SESSION_MANAGER.cancel_selecting(user_id);
                    let title = if target.title.is_empty() {
                        "未命名对话"
                    } else {
                        &target.title
                    };
                    return Some(format!("✅ 已切换至「{}」，继续上下文吧。", title));
                } else {
                    return Some(format!(
                        "❌ 序号无效，请回复 1-{} 之间的数字，或发送 /sessions 重新列出。",
                        list.len()
                    ));
                }
            }
        } else {
            // Not a number, exit selecting mode
            SESSION_MANAGER.cancel_selecting(user_id);
        }
    }

    None // 不是系统指令，放行给 LLM
}

/// 获取用户当前的会话 ID，如果不存在，且指定了需要自动新建，则生成一个全新的 UUID 会话 ID。
pub fn get_or_create_conv_id(user_id: &str) -> String {
    if let Some(id) = SESSION_MANAGER.get_conv_id(user_id) {
        // 验证该 ID 在数据库中是否依然存在
        if let Some(conn) = SessionManager::open_db() {
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM conversations WHERE id = ?1",
                    params![&id],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if exists {
                return id;
            }
        }
    }
    // 如果没有，或者数据库已被删除，新建
    let new_id = format!("conv-{}", crate::now_ms());
    SESSION_MANAGER.bind_session(user_id, Some(new_id.clone()));

    // 初始化空对话入库
    if let Some(conn) = SessionManager::open_db() {
        let title = "未命名对话";
        let ts = crate::now_ms();
        let _ = conn.execute(
            "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, '', ?3, ?4)",
            params![new_id, title, ts, ts],
        );
    }

    new_id
}
