// ═══════════════════════════════════════════════════════════
// R2/R3 工具确认状态管理
// 
// 当 LLM 请求执行 R2/R3 风险等级工具时，暂停执行，
// 向前端发送确认事件，等待用户通过此模块回传决定。
// ═══════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;

/// 存储待确认的工具请求 (request_id -> oneshot sender)
pub struct ToolConfirmState {
    pub pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl ToolConfirmState {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }
}

/// 前端调用此命令回传确认结果
#[tauri::command]
pub fn tool_confirm_response(
    request_id: String,
    approved: bool,
    state: tauri::State<'_, ToolConfirmState>,
) -> bool {
    if let Ok(mut map) = state.pending.lock() {
        if let Some(sender) = map.remove(&request_id) {
            let _ = sender.send(approved);
            return true;
        }
    }
    false
}
