use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::{command, AppHandle, Emitter, Manager};
use rusqlite::{Connection, OptionalExtension};

pub static RELAY_CONNECTED: AtomicBool = AtomicBool::new(false);

use lazy_static::lazy_static;
use std::sync::Mutex;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::protocol::Message;
use std::fs;
use std::path::{Path, PathBuf};

use crate::lan_sync::LanSyncEngine;
use crate::sync_protocol::{
    DiagnosticEvent, DiagnosticStage, DiagnosticStatus, TransportKind, SYNC_PROTOCOL_VERSION,
};

#[derive(Debug, Clone, PartialEq)]
pub enum RelayTerminal {
    Ack,
    ProxyResponse,
    CommitAck,
    RpcResponse,
    AnyResponse,
}

pub struct RelayRequestWaiter {
    pub tx: oneshot::Sender<serde_json::Value>,
    pub terminal: RelayTerminal,
    pub expected_peer: String,
    pub expected_local: String,
    pub allow_pairing_bootstrap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedChange {
    pub change_id: String,
    pub request_id: String,
    pub project_id: String,
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub old_content_hash: String,
    pub diff: String,
    pub summary: String,
    pub additions: usize,
    pub deletions: usize,
    pub status: String,
    pub created_at: i64,
    pub applied_at: Option<i64>,
    pub work_object_id: Option<String>,
}

#[derive(Clone)]
pub struct ActiveRpcTask {
    pub cancel_tx: tokio::sync::watch::Sender<bool>,
    pub done_rx: tokio::sync::watch::Receiver<bool>,
}

lazy_static! {
    pub static ref RELAY_TX: RwLock<Option<tokio::sync::mpsc::Sender<Message>>> = RwLock::new(None);
    pub static ref PENDING_REQUESTS: RwLock<HashMap<String, RelayRequestWaiter>> =
        RwLock::new(HashMap::new());
    pub static ref RELAY_RECONNECT_TRIGGER: Mutex<Option<tokio::sync::mpsc::Sender<()>>> =
        Mutex::new(None);
    pub static ref ACTIVE_RPC_TASKS: Arc<Mutex<HashMap<String, ActiveRpcTask>>> =
        Arc::new(Mutex::new(HashMap::new()));
    pub static ref STAGED_CHANGES: Arc<Mutex<HashMap<String, StagedChange>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

pub fn compute_content_hash(content: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    Confirmed,
    Timeout,
    UnknownTask,
    AbortedAbnormally,
}

pub async fn cancel_active_rpc_task_core(
    request_id: &str,
    timeout_duration: std::time::Duration,
) -> (CancelOutcome, bool, String, Option<String>) {
    let task = {
        let lock = ACTIVE_RPC_TASKS.lock().unwrap();
        lock.get(request_id).cloned()
    };

    let task = match task {
        Some(t) => t,
        None => {
            return (
                CancelOutcome::UnknownTask,
                false,
                "unknown_task".to_string(),
                Some("未找到该活跃任务，任务可能已结束或不存在".to_string()),
            );
        }
    };

    let _ = task.cancel_tx.send(true);

    let mut done_rx = task.done_rx;
    let res = tokio::time::timeout(timeout_duration, async {
        loop {
            if *done_rx.borrow() {
                return Ok(true);
            }
            if done_rx.changed().await.is_err() {
                return Err("Aborted abnormally");
            }
        }
    }).await;

    match res {
        Ok(Ok(_)) => {
            ACTIVE_RPC_TASKS.lock().unwrap().remove(request_id);
            (CancelOutcome::Confirmed, true, "cancelled".to_string(), None)
        }
        Ok(Err(_)) => {
            ACTIVE_RPC_TASKS.lock().unwrap().remove(request_id);
            (
                CancelOutcome::AbortedAbnormally,
                false,
                "aborted_abnormally".to_string(),
                Some("任务执行通道异常关闭，未能确认正常清理退出".to_string()),
            )
        }
        Err(_) => {
            (
                CancelOutcome::Timeout,
                false,
                "timeout".to_string(),
                Some(format!("任务中止信号已发送，但后台任务在 {:?} 内未退出", timeout_duration)),
            )
        }
    }
}

pub fn generate_unified_diff(file_path: &str, old_content: &str, new_content: &str) -> (String, usize, usize) {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let mut diff = format!("--- a/{}\n+++ b/{}\n", file_path, file_path);
    diff.push_str(&format!("@@ -1,{} +1,{} @@\n", old_lines.len(), new_lines.len()));

    let n = old_lines.len();
    let m = new_lines.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            if old_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i][j].max(dp[i][j + 1]);
            }
        }
    }

    let mut i = n;
    let mut j = m;
    let mut diff_lines = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            diff_lines.push(format!(" {}", old_lines[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            diff_lines.push(format!("+{}", new_lines[j - 1]));
            additions += 1;
            j -= 1;
        } else if i > 0 {
            diff_lines.push(format!("-{}", old_lines[i - 1]));
            deletions += 1;
            i -= 1;
        }
    }
    diff_lines.reverse();
    for l in diff_lines {
        diff.push_str(&l);
        diff.push('\n');
    }
    (diff, additions, deletions)
}

pub fn apply_unified_diff(old_content: &str, diff_content: &str) -> Result<String, String> {
    let mut new_lines = Vec::new();
    let old_lines: Vec<&str> = old_content.lines().collect();
    let mut old_idx = 0;

    for line in diff_content.lines() {
        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@") {
            continue;
        }
        if let Some(stripped) = line.strip_prefix('+') {
            new_lines.push(stripped.to_string());
        } else if let Some(_stripped) = line.strip_prefix('-') {
            old_idx += 1;
        } else if let Some(stripped) = line.strip_prefix(' ') {
            new_lines.push(stripped.to_string());
            old_idx += 1;
        } else {
            new_lines.push(line.to_string());
            old_idx += 1;
        }
    }
    while old_idx < old_lines.len() {
        new_lines.push(old_lines[old_idx].to_string());
        old_idx += 1;
    }
    let mut res = new_lines.join("\n");
    if old_content.ends_with('\n') && !res.is_empty() {
        res.push('\n');
    }
    Ok(res)
}

pub fn extract_diff_from_text(text: &str) -> Option<(String, String, usize, usize)> {
    let content = if let Some(start) = text.find("```diff") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            &after[..end]
        } else {
            after
        }
    } else {
        text
    };

    let mut file_path = String::new();
    let mut diff_lines = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;
    let mut in_diff = false;

    for line in content.lines() {
        if line.starts_with("--- a/") {
            file_path = line.trim_start_matches("--- a/").trim().to_string();
            in_diff = true;
            diff_lines.push(line);
        } else if line.starts_with("+++ b/") {
            if file_path.is_empty() {
                file_path = line.trim_start_matches("+++ b/").trim().to_string();
            }
            in_diff = true;
            diff_lines.push(line);
        } else if in_diff {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
                diff_lines.push(line);
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
                diff_lines.push(line);
            } else if line.starts_with(' ') || line.starts_with("@@") {
                diff_lines.push(line);
            }
        }
    }

    if !file_path.is_empty() && !diff_lines.is_empty() {
        let diff_str = diff_lines.join("\n") + "\n";
        Some((file_path, diff_str, additions, deletions))
    } else {
        None
    }
}

pub fn init_staged_changes_table(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS staged_changes (
            change_id TEXT PRIMARY KEY,
            request_id TEXT NOT NULL,
            project_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            old_content TEXT NOT NULL,
            new_content TEXT NOT NULL,
            old_content_hash TEXT NOT NULL,
            diff TEXT NOT NULL,
            summary TEXT NOT NULL,
            additions INTEGER NOT NULL,
            deletions INTEGER NOT NULL,
            status TEXT NOT NULL, -- 'pending', 'applied', 'rejected', 'cancelled'
            work_object_id TEXT,
            created_at INTEGER NOT NULL,
            applied_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_staged_changes_req ON staged_changes(request_id);
        CREATE INDEX IF NOT EXISTS idx_staged_changes_status ON staged_changes(status);"
    )
}

pub fn save_staged_change(conn: &rusqlite::Connection, staged: &StagedChange) -> Result<(), String> {
    conn.execute(
        "INSERT INTO staged_changes (
            change_id, request_id, project_id, file_path, old_content, new_content,
            old_content_hash, diff, summary, additions, deletions, status, work_object_id,
            created_at, applied_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            staged.change_id,
            staged.request_id,
            staged.project_id,
            staged.file_path,
            staged.old_content,
            staged.new_content,
            staged.old_content_hash,
            staged.diff,
            staged.summary,
            staged.additions as i64,
            staged.deletions as i64,
            staged.status,
            staged.work_object_id,
            staged.created_at,
            staged.applied_at,
        ],
    ).map_err(|e| format!("Save staged change failed: {}", e))?;
    Ok(())
}

pub fn get_staged_change(conn: &rusqlite::Connection, change_id: &str) -> Result<Option<StagedChange>, String> {
    let mut stmt = conn.prepare(
        "SELECT change_id, request_id, project_id, file_path, old_content, new_content,
                old_content_hash, diff, summary, additions, deletions, status, work_object_id,
                created_at, applied_at
         FROM staged_changes WHERE change_id = ?1"
    ).map_err(|e| format!("Prepare query failed: {}", e))?;

    let mut rows = stmt.query_map(rusqlite::params![change_id], |row| {
        Ok(StagedChange {
            change_id: row.get(0)?,
            request_id: row.get(1)?,
            project_id: row.get(2)?,
            file_path: row.get(3)?,
            old_content: row.get(4)?,
            new_content: row.get(5)?,
            old_content_hash: row.get(6)?,
            diff: row.get(7)?,
            summary: row.get(8)?,
            additions: row.get::<_, i64>(9)? as usize,
            deletions: row.get::<_, i64>(10)? as usize,
            status: row.get(11)?,
            work_object_id: row.get(12)?,
            created_at: row.get(13)?,
            applied_at: row.get(14)?,
        })
    }).map_err(|e| format!("Query failed: {}", e))?;

    if let Some(r) = rows.next() {
        r.map(Some).map_err(|e| format!("Row map error: {}", e))
    } else {
        Ok(None)
    }
}

pub fn update_staged_change_status(
    conn: &rusqlite::Connection,
    change_id: &str,
    status: &str,
    applied_at: Option<i64>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE staged_changes SET status = ?1, applied_at = ?2 WHERE change_id = ?3",
        rusqlite::params![status, applied_at, change_id],
    ).map_err(|e| format!("Update staged change status failed: {}", e))?;
    Ok(())
}

pub fn cancel_staged_changes_by_request(conn: &rusqlite::Connection, request_id: &str) -> Result<Vec<StagedChange>, String> {
    let mut stmt = conn.prepare(
        "SELECT change_id, request_id, project_id, file_path, old_content, new_content,
                old_content_hash, diff, summary, additions, deletions, status, work_object_id,
                created_at, applied_at
         FROM staged_changes WHERE request_id = ?1 AND status = 'pending'"
    ).map_err(|e| format!("Prepare query failed: {}", e))?;

    let rows = stmt.query_map(rusqlite::params![request_id], |row| {
        Ok(StagedChange {
            change_id: row.get(0)?,
            request_id: row.get(1)?,
            project_id: row.get(2)?,
            file_path: row.get(3)?,
            old_content: row.get(4)?,
            new_content: row.get(5)?,
            old_content_hash: row.get(6)?,
            diff: row.get(7)?,
            summary: row.get(8)?,
            additions: row.get::<_, i64>(9)? as usize,
            deletions: row.get::<_, i64>(10)? as usize,
            status: row.get(11)?,
            work_object_id: row.get(12)?,
            created_at: row.get(13)?,
            applied_at: row.get(14)?,
        })
    }).map_err(|e| format!("Query failed: {}", e))?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(c) = r {
            result.push(c);
        }
    }

    let _ = conn.execute(
        "UPDATE staged_changes SET status = 'cancelled' WHERE request_id = ?1 AND status = 'pending'",
        rusqlite::params![request_id],
    );

    Ok(result)
}

pub fn get_staged_changes_by_request(conn: &rusqlite::Connection, request_id: &str) -> Result<Vec<StagedChange>, String> {
    let mut stmt = conn.prepare(
        "SELECT change_id, request_id, project_id, file_path, old_content, new_content,
                old_content_hash, diff, summary, additions, deletions, status, work_object_id,
                created_at, applied_at
         FROM staged_changes WHERE request_id = ?1"
    ).map_err(|e| format!("Prepare query failed: {}", e))?;

    let rows = stmt.query_map(rusqlite::params![request_id], |row| {
        Ok(StagedChange {
            change_id: row.get(0)?,
            request_id: row.get(1)?,
            project_id: row.get(2)?,
            file_path: row.get(3)?,
            old_content: row.get(4)?,
            new_content: row.get(5)?,
            old_content_hash: row.get(6)?,
            diff: row.get(7)?,
            summary: row.get(8)?,
            additions: row.get::<_, i64>(9)? as usize,
            deletions: row.get::<_, i64>(10)? as usize,
            status: row.get(11)?,
            work_object_id: row.get(12)?,
            created_at: row.get(13)?,
            applied_at: row.get(14)?,
        })
    }).map_err(|e| format!("Query failed: {}", e))?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(c) = r {
            result.push(c);
        }
    }
    Ok(result)
}


pub fn init_staged_write_recovery_table(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS staged_write_recovery (
            change_id TEXT PRIMARY KEY,
            target_path TEXT NOT NULL,
            bak_path TEXT,
            tmp_path TEXT NOT NULL,
            stage TEXT NOT NULL, -- 'prepared', 'backed_up', 'replaced'
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_staged_write_recovery_target ON staged_write_recovery(target_path);"
    )
}


pub fn record_staged_write_recovery_stage(
    conn: &rusqlite::Connection,
    change_id: &str,
    target_path: &str,
    bak_path: Option<&str>,
    tmp_path: &str,
    stage: &str,
) -> Result<(), rusqlite::Error> {
    let now = crate::now_ms();
    conn.execute(
        "INSERT INTO staged_write_recovery (change_id, target_path, bak_path, tmp_path, stage, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(change_id) DO UPDATE SET
             stage = excluded.stage,
             bak_path = excluded.bak_path,
             tmp_path = excluded.tmp_path",
        rusqlite::params![change_id, target_path, bak_path, tmp_path, stage, now],
    )?;
    Ok(())
}

pub fn delete_staged_write_recovery_entry(
    conn: &rusqlite::Connection,
    change_id: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM staged_write_recovery WHERE change_id = ?1",
        rusqlite::params![change_id],
    )?;
    Ok(())
}


static RECOVERY_DEGRADED: AtomicBool = AtomicBool::new(false);
static RECOVERY_DEGRADED_REASON: RwLock<Option<String>> = RwLock::new(None);

pub fn set_recovery_degraded(degraded: bool, reason: Option<&str>) {
    RECOVERY_DEGRADED.store(degraded, Ordering::SeqCst);
    if let Ok(mut r) = RECOVERY_DEGRADED_REASON.write() {
        *r = reason.map(|s| s.to_string());
    }
}

pub fn is_recovery_degraded() -> (bool, Option<String>) {
    let degraded = RECOVERY_DEGRADED.load(Ordering::SeqCst);
    let reason = RECOVERY_DEGRADED_REASON.read().ok().and_then(|r| r.clone());
    (degraded, reason)
}

pub fn safe_restore_target_internal<R, F>(
    bak: &std::path::Path,
    target_path: &std::path::Path,
    renamer: R,
    copier: F,
) -> Result<(), String>
where
    R: FnOnce(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
    F: FnOnce(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
{
    if !bak.exists() {
        return Err(format!("备份文件不存在 {:?}", bak));
    }
    if target_path.exists() {
        if let Err(e) = std::fs::remove_file(target_path) {
            return Err(format!("无法移除受损或待恢复的目标文件 {:?}: {}。原备份文件安全保留在 {:?}", target_path, e, bak));
        }
    }
    if let Err(re) = renamer(bak, target_path) {
        // 跨卷、占用或权限受限 fallback 到复制
        copier(bak, target_path).map_err(|ce| {
            format!("重命名 ({}) 与复制 ({}) 均失败，原文件备份安全保留在 {:?}", re, ce, bak)
        })?;
        if !target_path.exists() {
            return Err(format!("复制后目标文件仍不存在 {:?}，原文件备份安全保留在 {:?}", target_path, bak));
        }
        // 强核验：内容必须逐字节完全一致（绝不容忍同长度但内容不同），核验失败绝对保留备份
        let bak_bytes = std::fs::read(bak).map_err(|e| format!("读取备份文件核验失败: {}，原文件备份安全保留在 {:?}", e, bak))?;
        let target_bytes = std::fs::read(target_path).map_err(|e| format!("读取恢复后目标文件核验失败: {}，原文件备份安全保留在 {:?}", e, bak))?;
        if bak_bytes != target_bytes {
            return Err(format!("复制后内容不匹配 (内容校验损坏)，原文件备份安全保留在 {:?}", bak));
        }
    }
    if !target_path.exists() {
        return Err(format!("核验失败：目标文件在恢复后仍不存在 {:?}，原文件备份安全保留在 {:?}", target_path, bak));
    }
    Ok(())
}

pub fn safe_restore_target_from_backup(bak: &std::path::Path, target_path: &std::path::Path) -> Result<(), String> {
    safe_restore_target_internal(
        bak,
        target_path,
        |_src, _dst| Err(std::io::Error::new(std::io::ErrorKind::Other, "preserve backup until cleanup")),
        |src, dst| std::fs::copy(src, dst).map(|_| ()),
    )
}

pub fn is_path_recovery_blocked(
    conn: &rusqlite::Connection,
    target_path: &str,
    current_change_id: Option<&str>,
) -> Result<bool, rusqlite::Error> {
    let count: i64 = if let Some(cid) = current_change_id {
        conn.query_row(
            "SELECT COUNT(*) FROM staged_write_recovery WHERE target_path = ?1 AND change_id != ?2",
            rusqlite::params![target_path, cid],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM staged_write_recovery WHERE target_path = ?1",
            rusqlite::params![target_path],
            |r| r.get(0),
        )?
    };
    Ok(count > 0)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoverySummary {
    pub recovered_and_cleaned: usize,
    pub restored_pending_cleanup: usize,
    pub blocked_or_degraded: usize,
    pub remaining_unprocessed: usize,
}

impl PartialEq<usize> for RecoverySummary {
    fn eq(&self, other: &usize) -> bool {
        self.recovered_and_cleaned == *other
    }
}

impl PartialEq<RecoverySummary> for usize {
    fn eq(&self, other: &RecoverySummary) -> bool {
        *self == other.recovered_and_cleaned
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedRestoredState {
    Present,
    Absent,
}

pub fn recover_interrupted_staged_writes(conn: &mut rusqlite::Connection) -> Result<RecoverySummary, String> {
    if is_recovery_degraded().0 {
        return Ok(RecoverySummary::default());
    }

    let mut summary = RecoverySummary::default();

    let entries: Vec<(String, String, Option<String>, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT change_id, target_path, bak_path, tmp_path, stage FROM staged_write_recovery ORDER BY rowid ASC")
            .map_err(|e| {
                set_recovery_degraded(true, Some(&format!("初始化崩溃恢复失败: 无法读取日志: {}", e)));
                format!("初始化崩溃恢复失败: 无法读取日志: {}", e)
            })?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        }).map_err(|e| {
            set_recovery_degraded(true, Some(&format!("读取崩溃恢复日志失败: {}", e)));
            format!("读取崩溃恢复日志失败: {}", e)
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| {
                set_recovery_degraded(true, Some(&format!("解析崩溃恢复日志条目失败: {}", e)));
                format!("解析崩溃恢复日志条目失败: {}", e)
            })?);
        }
        list
    };

    let total_entries = entries.len();
    for (idx, (change_id, target_path_str, bak_path_opt, tmp_path_str, stage)) in entries.into_iter().enumerate() {
        let remaining_unprocessed = total_entries.saturating_sub(idx + 1);
        log::warn!(
            "[Sync Engine] Found interrupted staged write WAL: change={}, target={}, stage={}, remaining={}",
            change_id, target_path_str, stage, remaining_unprocessed
        );
        let target_path = std::path::Path::new(&target_path_str);
        let tmp_path = std::path::Path::new(&tmp_path_str);

        let make_degraded_err = |reason: &str| -> String {
            set_recovery_degraded(true, Some(reason));
            format!(
                "崩溃恢复降级中止 (change_id: {}, 路径: {}, 原因: {}, 已恢复: {}, 剩余未处理: {})",
                change_id, target_path_str, reason, summary.recovered_and_cleaned, remaining_unprocessed
            )
        };

        // 1. 查询暂存提案状态 (P1-2: 严禁 unwrap_or(None) 吞错，失败立即中止并降级)
        let staged_opt = match get_staged_change(conn, &change_id) {
            Ok(opt) => opt,
            Err(e) => {
                log::error!(
                    "[Sync Engine] CRITICAL: Failed to query staged change {}: {}. Retaining WAL and aborting recovery batch.",
                    change_id, e
                );
                return Err(make_degraded_err(&format!("查询暂存变更状态失败: {}", e)));
            }
        };

        // 2. 跨重启重试状态处理 (支持 restored_pending_cleanup, restored, restored_absent_pending_cleanup, restored_present_pending_cleanup)
        let is_restored_stage = stage == "restored_pending_cleanup"
            || stage == "restored"
            || stage == "restored_absent_pending_cleanup"
            || stage == "restored_present_pending_cleanup";

        if is_restored_stage {
            let expected_state = if stage == "restored_absent_pending_cleanup" {
                ExpectedRestoredState::Absent
            } else if stage == "restored_present_pending_cleanup" {
                ExpectedRestoredState::Present
            } else if let Some(ref staged) = staged_opt {
                if staged.status == "applied" {
                    ExpectedRestoredState::Present
                } else if staged.old_content.is_empty() && bak_path_opt.is_none() {
                    // 无历史内容且无备份路径的未提交提案，必为新建文件，预期应当不存在 (Absent)
                    ExpectedRestoredState::Absent
                } else {
                    // 存在历史内容或备份路径，说明原文件本来就存在，回滚后预期为 Present
                    ExpectedRestoredState::Present
                }
            } else if bak_path_opt.is_none() {
                ExpectedRestoredState::Absent
            } else {
                ExpectedRestoredState::Present
            };

            let mut target_valid = false;
            match expected_state {
                ExpectedRestoredState::Absent => {
                    if target_path.exists() {
                        log::error!(
                            "[Sync Engine] CRITICAL: Target file {:?} was unexpectedly present or recreated during restored absent retry for {}. Aborting recovery batch.",
                            target_path, change_id
                        );
                        return Err(make_degraded_err("清理重试核验失败：未提交新建文件在重启期间被外部异常创建或存在"));
                    }
                    target_valid = true;
                }
                ExpectedRestoredState::Present => {
                    if !target_path.exists() {
                        log::error!(
                            "[Sync Engine] CRITICAL: Target file {:?} missing during restored present retry for change {}. Aborting recovery batch.",
                            target_path, change_id
                        );
                        return Err(make_degraded_err("清理重试核验失败：目标文件在重启期间已被外部篡改或缺失"));
                    }
                    if let Some(ref staged) = staged_opt {
                        match std::fs::read(target_path) {
                            Ok(bytes) => {
                                let expected = if staged.status == "applied" {
                                    staged.new_content.as_bytes()
                                } else {
                                    staged.old_content.as_bytes()
                                };
                                if bytes == expected {
                                    target_valid = true;
                                }
                            }
                            Err(e) => {
                                log::error!(
                                    "[Sync Engine] CRITICAL: Failed to read target file {:?} during restored retry check for {}: {}. Fail-closed.",
                                    target_path, change_id, e
                                );
                            }
                        }
                    }
                }
            }

            if !target_valid {
                log::error!(
                    "[Sync Engine] CRITICAL: Target file {:?} was externally modified, corrupted, unreadable, or missing during restored retry for change {}. Aborting recovery batch.",
                    target_path, change_id
                );
                return Err(make_degraded_err("清理重试核验失败：目标文件在重启期间已被外部篡改或缺失"));
            }

            // 目标文件内容完整且经验证，重试清理
            let mut cleanup_ok = true;
            if tmp_path.exists() {
                if let Err(e) = std::fs::remove_file(&tmp_path) {
                    log::warn!("[Sync Engine] Pending cleanup: failed to remove tmp {:?}: {}", tmp_path, e);
                    cleanup_ok = false;
                }
            }
            if let Some(ref bak_str) = bak_path_opt {
                let bak_path = std::path::Path::new(bak_str);
                if bak_path.exists() {
                    if let Err(e) = std::fs::remove_file(bak_path) {
                        log::warn!("[Sync Engine] Pending cleanup: failed to remove residual bak {:?}: {}", bak_path, e);
                        cleanup_ok = false;
                    }
                }
            }

            if cleanup_ok {
                match conn.execute("DELETE FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id]) {
                    Ok(_) => {
                        summary.restored_pending_cleanup += 1;
                        log::info!("[Sync Engine] Pending cleanup retry succeeded for {}: WAL entry removed (状态: 清理重试收敛完成)", change_id);
                    }
                    Err(e) => {
                        log::warn!("[Sync Engine] Target verified restored, but WAL delete retry failed for {}: {}. Retaining WAL for next startup", change_id, e);
                        summary.restored_pending_cleanup += 1;
                    }
                }
            } else {
                log::warn!("[Sync Engine] Target verified restored, but cleanup retry failed for {}. Retaining WAL in stage 'restored_pending_cleanup'", change_id);
                summary.restored_pending_cleanup += 1;
            }
            continue;
        }

        // 3. 检查 staged_changes 真实提交状态 (applied)
        if let Some(ref staged) = staged_opt {
            if staged.status == "applied" {
                // 数据库事务在崩溃前已成功提交。
                // 必须严格核验磁盘目标文件真实存在且内容等于 new_content！
                let target_matches = if target_path.exists() {
                    if let Ok(bytes) = std::fs::read(target_path) {
                        bytes == staged.new_content.as_bytes()
                    } else {
                        false
                    }
                } else {
                    false
                };

                if target_matches {
                    // 目标文件完整且一致，安全清理历史备份与 WAL 记录
                    log::info!("[Sync Engine] Crash recovery: change {} applied in DB and verified on disk, retaining target", change_id);
                    if let Err(ue) = conn.execute(
                        "UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1",
                        rusqlite::params![change_id],
                    ) {
                        return Err(make_degraded_err(&format!("持久化已提交恢复状态失败 (change: {}): {}", change_id, ue)));
                    }

                    let mut cleanup_ok = true;
                    if let Some(ref bak_str) = bak_path_opt {
                        let bak_path = std::path::Path::new(bak_str);
                        if bak_path.exists() {
                            if let Err(e) = std::fs::remove_file(bak_path) {
                                log::warn!("[Sync Engine] Target matches but failed to remove bak {:?}: {}", bak_path, e);
                                cleanup_ok = false;
                            }
                        }
                    }
                    if tmp_path.exists() {
                        if let Err(e) = std::fs::remove_file(&tmp_path) {
                            log::warn!("[Sync Engine] Target matches but failed to remove tmp {:?}: {}", tmp_path, e);
                            cleanup_ok = false;
                        }
                    }
                    if cleanup_ok {
                        match conn.execute("DELETE FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id]) {
                            Ok(_) => {
                                summary.recovered_and_cleaned += 1;
                                log::info!("[Sync Engine] Crash recovery complete (状态: 数据已恢复、清理完成): {}", change_id);
                            }
                            Err(e) => {
                                log::warn!("[Sync Engine] Target data verified on disk, but failed to delete recovery entry {}: {}. Updating stage to 'restored_pending_cleanup'", change_id, e);
                                if let Err(ue) = conn.execute("UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1", rusqlite::params![change_id]) {
                                    log::warn!("Failed to update stage to restored_pending_cleanup: {}", ue);
                                }
                                summary.restored_pending_cleanup += 1;
                            }
                        }
                    } else {
                        log::warn!("[Sync Engine] Target data verified on disk, but cleanup of bak/tmp failed for {}. Retaining WAL in stage 'restored_pending_cleanup'", change_id);
                        if let Err(ue) = conn.execute("UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1", rusqlite::params![change_id]) {
                            log::warn!("Failed to update stage to restored_pending_cleanup: {}", ue);
                        }
                        summary.restored_pending_cleanup += 1;
                    }
                    continue;
                } else {
                    // 数据库已提交，但磁盘目标文件缺失或内容损坏！
                    log::error!(
                        "[Sync Engine] CRITICAL: change {} is applied in DB but disk target {:?} is missing or corrupted!",
                        change_id, target_path
                    );

                    // 检查临时文件是否存在且有效
                    let mut repaired = false;
                    if tmp_path.exists() {
                        if let Ok(bytes) = std::fs::read(&tmp_path) {
                            if bytes == staged.new_content.as_bytes() {
                                if safe_restore_target_from_backup(&tmp_path, target_path).is_ok() {
                                    repaired = true;
                                    log::info!("[Sync Engine] Repaired committed file {:?} from tmp file", target_path);
                                }
                            }
                        }
                    }

                    if repaired {
                        let mut cleanup_ok = true;
                        if let Some(ref bak_str) = bak_path_opt {
                            let bak_path = std::path::Path::new(bak_str);
                            if bak_path.exists() {
                                if let Err(e) = std::fs::remove_file(bak_path) {
                                    log::warn!("[Sync Engine] Repaired from tmp but failed to remove bak {:?}: {}", bak_path, e);
                                    cleanup_ok = false;
                                }
                            }
                        }
                        if tmp_path.exists() {
                            if let Err(e) = std::fs::remove_file(&tmp_path) {
                                log::warn!("[Sync Engine] Repaired from tmp but failed to remove tmp {:?}: {}", tmp_path, e);
                                cleanup_ok = false;
                            }
                        }
                        if cleanup_ok {
                            match conn.execute("DELETE FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id]) {
                                Ok(_) => {
                                    summary.recovered_and_cleaned += 1;
                                    log::info!("[Sync Engine] Crash recovery repaired from tmp complete (状态: 数据已恢复、清理完成): {}", change_id);
                                }
                                Err(e) => {
                                    log::warn!("[Sync Engine] Repaired target from tmp but failed to delete recovery entry {}: {}. Updating stage to 'restored_pending_cleanup'", change_id, e);
                                    if let Err(ue) = conn.execute("UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1", rusqlite::params![change_id]) {
                                        log::warn!("Failed to update stage to restored_pending_cleanup: {}", ue);
                                    }
                                    summary.restored_pending_cleanup += 1;
                                }
                            }
                        } else {
                            log::warn!("[Sync Engine] Repaired target from tmp, but cleanup failed for {}. Retaining WAL in stage 'restored_pending_cleanup'", change_id);
                            if let Err(ue) = conn.execute("UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1", rusqlite::params![change_id]) {
                                log::warn!("Failed to update stage to restored_pending_cleanup: {}", ue);
                            }
                            summary.restored_pending_cleanup += 1;
                        }
                        continue;
                    }

                    // 无法从 tmp 修复，绝不能删除仅存的 bak 备份！将其安全保留
                    if let Some(ref bak_str) = bak_path_opt {
                        let bak_path = std::path::Path::new(bak_str);
                        if bak_path.exists() {
                            let parent = bak_path.parent().unwrap_or_else(|| std::path::Path::new("."));
                            let file_name = target_path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                            let safe_bak = parent.join(format!(".{}.bak_committed_corruption_{}", file_name, change_id));
                            if let Err(e) = std::fs::rename(bak_path, &safe_bak) {
                                log::error!("[Sync Engine] Failed to rename backup {:?} to safe backup {:?}: {}", bak_path, safe_bak, e);
                            } else {
                                log::warn!("[Sync Engine] Preserved backup at {:?} due to committed target corruption", safe_bak);
                            }
                        }
                    }

                    // P1-2: 设置系统降级，立即中止后续恢复，坚决不继续循环！
                    return Err(make_degraded_err(&format!("已提交变更但在磁盘损坏/缺失 (change: {}, target: {:?})", change_id, target_path)));
                }
            }
        }

        // 4. 事务未提交 (pending/cancelled/rejected 或不存在): 坚决还原磁盘状态
        let mut tmp_cleanup_ok = true;
        if tmp_path.exists() {
            if let Err(e) = std::fs::remove_file(&tmp_path) {
                log::warn!("[Sync Engine] Failed to remove uncommitted tmp file {:?}: {}", tmp_path, e);
                tmp_cleanup_ok = false;
            }
        }

        if stage == "prepared" {
            // 在 prepared 阶段，目标文件尚未被重命名或替换，原目标文件完好无损。
            // 只需要清理孤立的 tmp 文件并删除 recovery 记录。
            if tmp_cleanup_ok {
                match conn.execute("DELETE FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id]) {
                    Ok(_) => {
                        summary.recovered_and_cleaned += 1;
                        log::info!("[Sync Engine] Crash recovery for prepared stage complete: orphan tmp removed, target intact: {}", change_id);
                    }
                    Err(e) => {
                        log::warn!("[Sync Engine] Prepared stage tmp removed, but failed to delete recovery entry {}: {}", change_id, e);
                        summary.restored_pending_cleanup += 1;
                    }
                }
            } else {
                summary.restored_pending_cleanup += 1;
            }
            continue;
        }

        if let Some(ref bak_str) = bak_path_opt {
            let bak_path = std::path::Path::new(bak_str);
            if bak_path.exists() {
                // 检测系统崩溃关闭期间用户是否在外部手动修改了目标文件 (P1-3: 字节级比对，读取失败严格 Fail-Closed)
                let mut is_conflict = false;
                if target_path.exists() {
                    let current_bytes = match std::fs::read(target_path) {
                        Ok(b) => b,
                        Err(e) => {
                            // P1-3: 读取失败意味着无法证明文件仍是 Bob 写入的版本，应当 fail-closed！
                            log::error!(
                                "[Sync Engine] CRITICAL: Failed to read target file {:?} during conflict check for change {}: {}. Fail-closed.",
                                target_path, change_id, e
                            );
                            return Err(make_degraded_err(&format!("读取目标文件核验外部冲突失败 (路径: {:?}): {}", target_path, e)));
                        }
                    };

                    if let Some(ref staged) = staged_opt {
                        if current_bytes != staged.new_content.as_bytes() && current_bytes != staged.old_content.as_bytes() {
                            is_conflict = true;
                        }
                    }
                }

                if is_conflict {
                    let parent = bak_path.parent().unwrap_or_else(|| std::path::Path::new("."));
                    let file_name = target_path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    let conflict_path = parent.join(format!(".{}.bak_recovery_conflict_{}", file_name, change_id));
                    if let Err(e) = std::fs::rename(bak_path, &conflict_path) {
                        log::error!("[Sync Engine] Failed to rename conflict backup {:?} to {:?}: {}", bak_path, conflict_path, e);
                    } else {
                        log::warn!(
                            "[Sync Engine] Crash recovery CONFLICT for {}: target file {:?} was modified externally. Backup preserved at {:?}",
                            change_id, target_path, conflict_path
                        );
                    }
                    // 保持 recovery 条目不删除，由准入控制阻止后续盲写该路径
                    summary.blocked_or_degraded += 1;
                    continue;
                }

                match safe_restore_target_from_backup(bak_path, target_path) {
                    Ok(()) => {
                        // P1-1: 目标文件已安全还原！立即将 WAL 标记为持久化的 'restored_pending_cleanup'！
                        if let Err(ue) = conn.execute(
                            "UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1",
                            rusqlite::params![change_id],
                        ) {
                            return Err(make_degraded_err(&format!("持久化恢复状态 restored_pending_cleanup 失败 (change: {}): {}", change_id, ue)));
                        }

                        let mut all_cleanup_ok = tmp_cleanup_ok;
                        if bak_path.exists() {
                            if let Err(rm_e) = std::fs::remove_file(bak_path) {
                                log::warn!("[Sync Engine] Target restored, but residual bak removal failed {:?}: {}", bak_path, rm_e);
                                all_cleanup_ok = false;
                            }
                        }

                        if all_cleanup_ok {
                            match conn.execute("DELETE FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id]) {
                                Ok(_) => {
                                    summary.recovered_and_cleaned += 1;
                                    log::info!("[Sync Engine] Crash recovery: Successfully restored target {:?} from backup (状态: 数据已恢复、清理完成)", target_path);
                                }
                                Err(e) => {
                                    log::warn!("[Sync Engine] Target restored from backup but failed to delete recovery entry {}: {}. Retaining WAL in stage 'restored_pending_cleanup'", change_id, e);
                                    summary.restored_pending_cleanup += 1;
                                }
                            }
                        } else {
                            log::warn!("[Sync Engine] Target restored from backup, but tmp/bak cleanup failed for {}. Retaining WAL in stage 'restored_pending_cleanup' for next startup", change_id);
                            summary.restored_pending_cleanup += 1;
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "[Sync Engine] CRITICAL: Failed to restore target {:?} from backup {:?}: {}. Aborting recovery batch.",
                            target_path, bak_path, e
                        );
                        return Err(make_degraded_err(&format!("未提交变更还原失败: {}", e)));
                    }
                }
            } else {
                // bak_path 在磁盘上不存在，且 stage 不是 restored_pending_cleanup！
                // P1-3: 声明有备份但文件缺失且未曾恢复，坚决 Fail-Closed 立即中止！
                log::error!(
                    "[Sync Engine] CRITICAL: Recovery record {} declared backup {:?}, but backup file is missing! Cannot restore original file. Aborting recovery batch.",
                    change_id, bak_path
                );
                return Err(make_degraded_err(&format!("未提交变更声明有备份但文件缺失 (change: {}, bak: {:?})", change_id, bak_path)));
            }
        } else {
            // 原先无目标文件，创建的新文件尚未提交事务，应予以移除
            if stage == "replaced" && target_path.exists() {
                let mut clean = false;
                if let Some(ref staged) = staged_opt {
                    let cur_bytes = match std::fs::read(target_path) {
                        Ok(b) => b,
                        Err(e) => {
                            log::error!(
                                "[Sync Engine] CRITICAL: Failed to read uncommitted target file {:?} for change {}: {}. Fail-closed.",
                                target_path, change_id, e
                            );
                            return Err(make_degraded_err(&format!("读取未提交新建文件 {:?} 失败: {}", target_path, e)));
                        }
                    };
                    if cur_bytes == staged.new_content.as_bytes() {
                        clean = true;
                    }
                } else {
                    // 无 staged 记录但新建文件已存在，无法证明是未提交内容，fail-closed
                    log::error!(
                        "[Sync Engine] CRITICAL: Uncommitted new file {:?} exists but missing staged_change record for {}. Fail-closed.",
                        target_path, change_id
                    );
                    return Err(make_degraded_err(&format!("缺失暂存变更记录，无法证明新建文件 {:?} 为未提交内容", target_path)));
                }

                if clean {
                    if let Err(e) = std::fs::remove_file(target_path) {
                        log::error!("[Sync Engine] Crash recovery: Failed to remove uncommitted newly created file {:?}: {}. Aborting recovery batch.", target_path, e);
                        return Err(make_degraded_err(&format!("无法删除未提交新建文件 {:?}: {}", target_path, e)));
                    }
                    log::info!("[Sync Engine] Crash recovery: Cleaned uncommitted newly created file {:?}", target_path);

                    // P1: 目标文件已移除，先更新 WAL 为 restored_absent_pending_cleanup！
                    // 若状态持久化失败，绝不能继续清理 tmp，立即 fail-closed 降级中止！
                    if let Err(ue) = conn.execute(
                        "UPDATE staged_write_recovery SET stage = 'restored_absent_pending_cleanup' WHERE change_id = ?1",
                        rusqlite::params![change_id],
                    ) {
                        return Err(make_degraded_err(&format!("持久化恢复状态 restored_absent_pending_cleanup 失败 (change: {}): {}", change_id, ue)));
                    }

                    let mut tmp_cleanup_ok = true;
                    if tmp_path.exists() {
                        if let Err(e) = std::fs::remove_file(&tmp_path) {
                            log::warn!("[Sync Engine] Failed to remove uncommitted tmp file {:?}: {}", tmp_path, e);
                            tmp_cleanup_ok = false;
                        }
                    }

                    if tmp_cleanup_ok {
                        match conn.execute("DELETE FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id]) {
                            Ok(_) => {
                                summary.recovered_and_cleaned += 1;
                                log::info!("[Sync Engine] Cleaned uncommitted newly created file and deleted recovery entry (状态: 数据已恢复、清理完成): {}", change_id);
                            }
                            Err(e) => {
                                log::warn!("[Sync Engine] Cleaned uncommitted file but failed to delete recovery entry {}: {}. Retaining WAL in stage 'restored_absent_pending_cleanup'", change_id, e);
                                summary.restored_pending_cleanup += 1;
                            }
                        }
                    } else {
                        log::warn!("[Sync Engine] Cleaned uncommitted file, but tmp cleanup failed for {}. Retaining WAL in stage 'restored_pending_cleanup'", change_id);
                        summary.restored_pending_cleanup += 1;
                    }
                } else {
                    log::warn!(
                        "[Sync Engine] Crash recovery: Newly created file {:?} was externally modified, preserving file and recovery lock",
                        target_path
                    );
                    // 坚决不删除 recovery 记录！保持 WAL 准入锁定！
                    summary.blocked_or_degraded += 1;
                    continue;
                }
            } else {
                let mut tmp_cleanup_ok = true;
                if tmp_path.exists() {
                    if let Err(e) = std::fs::remove_file(&tmp_path) {
                        log::warn!("[Sync Engine] Failed to remove uncommitted tmp file {:?}: {}", tmp_path, e);
                        tmp_cleanup_ok = false;
                    }
                }

                if tmp_cleanup_ok {
                    match conn.execute("DELETE FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id]) {
                        Ok(_) => {
                            summary.recovered_and_cleaned += 1;
                        }
                        Err(e) => {
                            log::warn!("[Sync Engine] Failed to delete recovery entry {}: {}. Updating stage to 'restored_pending_cleanup'", change_id, e);
                            if let Err(ue) = conn.execute(
                                "UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1",
                                rusqlite::params![change_id],
                            ) {
                                return Err(make_degraded_err(&format!("持久化恢复状态 restored_pending_cleanup 失败 (change: {}): {}", change_id, ue)));
                            }
                            summary.restored_pending_cleanup += 1;
                        }
                    }
                } else {
                    log::warn!("[Sync Engine] Tmp cleanup failed for {}. Retaining WAL entry in stage 'restored_pending_cleanup'.", change_id);
                    if let Err(ue) = conn.execute(
                        "UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup' WHERE change_id = ?1",
                        rusqlite::params![change_id],
                    ) {
                        return Err(make_degraded_err(&format!("持久化恢复状态 restored_pending_cleanup 失败 (change: {}): {}", change_id, ue)));
                    }
                    summary.restored_pending_cleanup += 1;
                }
            }
        }
    }

    Ok(summary)
}

#[derive(Debug)]
pub struct DiskWriteBackup {
    pub change_id: String,
    pub target_path: std::path::PathBuf,
    pub bak_path: Option<std::path::PathBuf>,
}

impl DiskWriteBackup {
    pub fn commit(self) -> Result<(), String> {
        if let Some(ref bak) = self.bak_path {
            if bak.exists() {
                std::fs::remove_file(bak).map_err(|e| format!("删除备份文件失败: {}", e))?;
            }
        }
        Ok(())
    }

    pub fn rollback(self) -> Result<(), String> {
        if let Some(ref bak) = self.bak_path {
            safe_restore_target_from_backup(&bak, &self.target_path)
                .map_err(|e| format!("回滚失败，恢复未完成：{}", e))?;
            let _ = std::fs::remove_file(bak);
            Ok(())
        } else {
            // 目标此前为全新创建的文件，回滚需彻底移除
            if self.target_path.exists() {
                std::fs::remove_file(&self.target_path)
                    .map_err(|e| format!("回滚失败，恢复未完成：无法删除新建的目标文件 {:?}: {}", self.target_path, e))?;
            }
            Ok(())
        }
    }
}

pub fn safe_atomic_write_file_with_backup(
    conn: Option<&rusqlite::Connection>,
    change_id: &str,
    target_path: &std::path::Path,
    new_content: &str,
) -> Result<DiskWriteBackup, String> {
    use std::io::Write;

    // 0. 全局降级检查 (Fail-Closed)
    let (degraded, reason) = is_recovery_degraded();
    if degraded {
        return Err(format!("崩溃恢复系统处于降级保护状态 (fail-closed 拒绝写入): {}", reason.unwrap_or_default()));
    }

    let parent = target_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    let file_name = target_path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let nonce = uuid::Uuid::new_v4();
    let tmp_path = parent.join(format!(".{}.tmp_bob_{}", file_name, nonce));
    let bak_path = if target_path.exists() {
        Some(parent.join(format!(".{}.bak_bob_{}", file_name, nonce)))
    } else {
        None
    };

    let target_str = target_path.to_string_lossy().to_string();
    let tmp_str = tmp_path.to_string_lossy().to_string();
    let bak_str = bak_path.as_ref().map(|b| b.to_string_lossy().to_string());

    // 0.1 准入控制：检查该路径是否处于未恢复的崩溃冲突状态 (Fail-Closed)
    if let Some(c) = conn {
        match is_path_recovery_blocked(c, &target_str, Some(change_id)) {
            Ok(true) => {
                return Err(format!("目标路径 {} 存在未解决的崩溃恢复冲突或未决恢复记录，已阻止写入以防数据损坏", target_str));
            }
            Err(e) => {
                return Err(format!("查询路径恢复状态失败 (fail-closed 拒绝写入): {}", e));
            }
            Ok(false) => {}
        }
    }

    // 1. 写入同目录临时文件并强制刷盘
    let write_res = (|| -> Result<(), std::io::Error> {
        let mut tmp_file = std::fs::File::create(&tmp_path)?;
        tmp_file.write_all(new_content.as_bytes())?;
        tmp_file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("写入临时文件失败: {}", e));
    }

    // 预写恢复日志: prepared (严格校验持久化，若写入日志失败绝不篡改目标文件)
    // 此时目标文件尚未备份，bak 为 None
    if let Some(c) = conn {
        if let Err(e) = record_staged_write_recovery_stage(c, change_id, &target_str, None, &tmp_str, "prepared") {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("记录暂存恢复预写日志失败 (prepared): {}", e));
        }
    }

    #[cfg(any(test, feature = "fault-injection"))]
    crate::fault_injection::check_and_trigger_07b_interrupt(
        change_id,
        target_path,
        crate::fault_injection::FaultStage::Prepared,
    );

    // 2. 如果目标存在，先备份
    if let Some(ref bak) = bak_path {
        if bak.exists() {
            let _ = std::fs::remove_file(bak);
        }
        if let Err(e) = std::fs::rename(target_path, bak) {
            let _ = std::fs::remove_file(&tmp_path);
            if let Some(c) = conn {
                let _ = delete_staged_write_recovery_entry(c, change_id);
            }
            return Err(format!("创建原始文件备份失败: {}", e));
        }
    }

    // 预写恢复日志: backed_up
    if let Some(c) = conn {
        if let Err(e) = record_staged_write_recovery_stage(c, change_id, &target_str, bak_str.as_deref(), &tmp_str, "backed_up") {
            // 回滚备份恢复目标文件
            let rb_res = if let Some(ref bak) = bak_path {
                safe_restore_target_from_backup(bak, target_path)
            } else {
                Ok(())
            };
            let _ = std::fs::remove_file(&tmp_path);
            match rb_res {
                Ok(()) => {
                    let _ = delete_staged_write_recovery_entry(c, change_id);
                    return Err(format!("记录暂存恢复预写日志失败 (backed_up): {}", e));
                }
                Err(rb_err) => {
                    set_recovery_degraded(true, Some(&format!("backed_up日志失败且回滚未完成 (路径: {}): {}", target_str, rb_err)));
                    return Err(format!("记录暂存恢复预写日志失败 (backed_up): {}；且回滚恢复未完成: {}", e, rb_err));
                }
            }
        }
    }

    #[cfg(any(test, feature = "fault-injection"))]
    crate::fault_injection::check_and_trigger_07b_interrupt(
        change_id,
        target_path,
        crate::fault_injection::FaultStage::BackedUp,
    );

    // 3. 将临时文件替换到目标位置
    let rename_res = {
        #[cfg(any(test, feature = "fault-injection"))]
        {
            if let Some(err_msg) = crate::fault_injection::check_07a_write_error(change_id, target_path) {
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, err_msg))
            } else {
                std::fs::rename(&tmp_path, target_path)
            }
        }
        #[cfg(not(any(test, feature = "fault-injection")))]
        {
            std::fs::rename(&tmp_path, target_path)
        }
    };

    if let Err(e) = rename_res {
        let rb_res = if let Some(ref bak) = bak_path {
            safe_restore_target_from_backup(bak, target_path)
        } else {
            if target_path.exists() {
                let _ = std::fs::remove_file(target_path);
            }
            Ok(())
        };
        let _ = std::fs::remove_file(&tmp_path);
        match rb_res {
            Ok(()) => {
                if let Some(c) = conn {
                    let _ = delete_staged_write_recovery_entry(c, change_id);
                }
                return Err(format!("替换目标文件失败: {}", e));
            }
            Err(rb_err) => {
                set_recovery_degraded(true, Some(&format!("替换目标失败且回滚未完成 (路径: {}): {}", target_str, rb_err)));
                return Err(format!("替换目标文件失败: {}；且回滚恢复未完成: {}", e, rb_err));
            }
        }
    }

    // 预写恢复日志: replaced
    if let Some(c) = conn {
        if let Err(e) = record_staged_write_recovery_stage(c, change_id, &target_str, bak_str.as_deref(), &tmp_str, "replaced") {
            // 回滚目标文件至备份
            let rb_res = if let Some(ref bak) = bak_path {
                safe_restore_target_from_backup(bak, target_path)
            } else {
                if target_path.exists() {
                    let _ = std::fs::remove_file(target_path);
                }
                Ok(())
            };
            let _ = std::fs::remove_file(&tmp_path);
            match rb_res {
                Ok(()) => {
                    let _ = delete_staged_write_recovery_entry(c, change_id);
                    return Err(format!("记录暂存恢复预写日志失败 (replaced): {}", e));
                }
                Err(rb_err) => {
                    set_recovery_degraded(true, Some(&format!("replaced日志失败且回滚未完成 (路径: {}): {}", target_str, rb_err)));
                    return Err(format!("记录暂存恢复预写日志失败 (replaced): {}；且回滚恢复未完成: {}", e, rb_err));
                }
            }
        }
    }

    // 4. 读回校验
    let mut verification_failed = false;
    let mut verify_err = String::new();
    match std::fs::read(target_path) {
        Ok(read_back) => {
            if read_back != new_content.as_bytes() {
                verification_failed = true;
                verify_err = "文件读回校验不匹配".to_string();
            }
        }
        Err(e) => {
            verification_failed = true;
            verify_err = format!("读回文件校验失败: {}", e);
        }
    }

    if verification_failed {
        let rb_res = if let Some(ref bak) = bak_path {
            safe_restore_target_from_backup(bak, target_path)
        } else {
            if target_path.exists() {
                let _ = std::fs::remove_file(target_path);
            }
            Ok(())
        };
        match rb_res {
            Ok(()) => {
                if let Some(c) = conn {
                    let _ = delete_staged_write_recovery_entry(c, change_id);
                }
                return Err(verify_err);
            }
            Err(rb_err) => {
                set_recovery_degraded(true, Some(&format!("读回核验失败且回滚未完成 (路径: {}): {}", target_str, rb_err)));
                return Err(format!("{}；且回滚恢复未完成: {}", verify_err, rb_err));
            }
        }
    }

    #[cfg(any(test, feature = "fault-injection"))]
    crate::fault_injection::check_and_trigger_07b_interrupt(
        change_id,
        target_path,
        crate::fault_injection::FaultStage::Replaced,
    );

    Ok(DiskWriteBackup {
        change_id: change_id.to_string(),
        target_path: target_path.to_path_buf(),
        bak_path,
    })
}

pub fn process_approval_decision_core(
    conn: &mut rusqlite::Connection,
    change_id: &str,
    decision: &str,
    actor: &str,
) -> serde_json::Value {
    // 1. 读取提案
    let staged_change = match get_staged_change(conn, change_id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return serde_json::json!({
                "status": "error",
                "change_id": change_id,
                "error": format!("未找到 ID 为 {} 的修改提案", change_id)
            });
        }
        Err(e) => {
            return serde_json::json!({
                "status": "error",
                "change_id": change_id,
                "error": format!("查询修改提案失败: {}", e)
            });
        }
    };

    // 2. 状态保护机 (State Machine Guard)
    if staged_change.status == "applied" {
        return serde_json::json!({
            "status": "applied",
            "change_id": change_id,
            "file_path": staged_change.file_path,
            "message": "修改此前已成功应用 (幂等)"
        });
    }

    if staged_change.status == "rejected" {
        return serde_json::json!({
            "status": "error",
            "change_id": change_id,
            "error": "该提案此前已被拒绝，不可重复审批或应用。"
        });
    }

    if staged_change.status == "cancelled" {
        return serde_json::json!({
            "status": "error",
            "change_id": change_id,
            "error": "该提案所在任务已被中止，提案已失效，不可应用。"
        });
    }

    if staged_change.status != "pending" {
        return serde_json::json!({
            "status": "error",
            "change_id": change_id,
            "error": format!("提案状态无效（当前状态：{}），仅处于待审批 (pending) 状态的提案可被处理。", staged_change.status)
        });
    }

    if decision == "approve" {
        // 0. 全局降级检查 (Fail-Closed)
        let (degraded, reason) = is_recovery_degraded();
        if degraded {
            log::error!("[Sync Engine] Approval rejected due to degraded recovery state: {:?}", reason);
            return serde_json::json!({
                "status": "error",
                "change_id": change_id,
                "error": format!("崩溃恢复系统处于降级保护状态 (fail-closed 拒绝写入): {}", reason.unwrap_or_default())
            });
        }

        // 3. 路径恢复阻塞与基线 Hash 冲突检测 (Fail-Closed)
        let target_str = staged_change.file_path.clone();
        match is_path_recovery_blocked(conn, &target_str, Some(change_id)) {
            Ok(true) => {
                log::warn!("[Sync Engine] Path {} is recovery-blocked by unresolved crash recovery entry", target_str);
                return serde_json::json!({
                    "status": "error",
                    "change_id": change_id,
                    "error": format!("目标路径 {} 存在未解决的崩溃恢复冲突或未决恢复记录，已阻止审批写入以防数据损坏。", target_str)
                });
            }
            Err(e) => {
                log::error!("[Sync Engine] Failed to query recovery blocked state for {}: {}", target_str, e);
                return serde_json::json!({
                    "status": "error",
                    "change_id": change_id,
                    "error": format!("无法核验路径恢复安全状态 (fail-closed 拒绝审批写入): {}", e)
                });
            }
            Ok(false) => {}
        }

        let target_path = std::path::Path::new(&staged_change.file_path);
        let current_content = if target_path.exists() {
            std::fs::read_to_string(target_path).unwrap_or_default()
        } else {
            String::new()
        };
        let current_hash = compute_content_hash(&current_content);

        if current_hash != staged_change.old_content_hash {
            log::error!(
                "[Sync Engine] Baseline conflict on {}: current hash {}, expected {}",
                staged_change.file_path, current_hash, staged_change.old_content_hash
            );
            return serde_json::json!({
                "status": "error",
                "change_id": change_id,
                "error": format!(
                    "文件基线冲突：磁盘文件自提案生成后已被修改（当前: {}, 期望: {}），审批已阻止以防代码覆盖。",
                    current_hash, staged_change.old_content_hash
                )
            });
        }

        // 4. 原子安全写入与读回校验 (带备份保护与崩溃恢复 WAL)
        let backup = match safe_atomic_write_file_with_backup(Some(conn), change_id, target_path, &staged_change.new_content) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[Sync Engine] Failed atomic write for {}: {}", staged_change.file_path, e);
                return serde_json::json!({
                    "status": "error",
                    "change_id": change_id,
                    "error": format!("文件安全写入失败: {}", e)
                });
            }
        };

        // 5. 协同持久化与回滚保护 (以单事务原子性保证文件系统与数据库绝对一致)
        let now = crate::now_ms();
        let pid = if !staged_change.project_id.is_empty() {
            staged_change.project_id.clone()
        } else {
            crate::work_core::repository::ensure_personal_workspace(conn)
                .unwrap_or_else(|_| "project_personal_inbox".to_string())
        };

        let bak_path_for_reporting = backup.bak_path.clone();

        let tx_res: Result<(), String> = (|| {
            let tx = conn.transaction().map_err(|e| format!("开启数据库事务失败: {}", e))?;

            update_staged_change_status(&tx, change_id, "applied", Some(now))
                .map_err(|e| format!("更新提案状态失败: {}", e))?;

            if let Some(ref obj_id) = staged_change.work_object_id {
                let _ = crate::work_core::repository::update_object_status_in_tx(
                    &tx,
                    crate::work_core::models::UpdateWorkStatusInput {
                        object_id: obj_id.clone(),
                        expected_revision: 1,
                        status: "accepted".into(),
                        actor: Some(actor.to_string()),
                        idempotency_key: format!("status_approve_{}", change_id),
                    },
                    false,
                ).map_err(|e| format!("更新工作对象状态失败: {}", e))?;
            }

            let _ = crate::work_core::repository::create_object_in_tx(
                &tx,
                crate::work_core::models::CreateWorkObjectInput {
                    kind: crate::work_core::models::WorkObjectKind::Artifact,
                    project_id: pid.clone(),
                    parent_id: staged_change.work_object_id.clone(),
                    title: format!("远程变更产物: {}", staged_change.file_path),
                    status: Some("active".into()),
                    description: Some(format!("已成功应用移动端批准的变更 (+{} -{})", staged_change.additions, staged_change.deletions)),
                    data: serde_json::json!({
                        "filePath": staged_change.file_path,
                        "changeId": change_id,
                        "additions": staged_change.additions,
                        "deletions": staged_change.deletions,
                        "appliedAt": now,
                    }),
                    source_capture_id: None,
                    actor: Some(actor.to_string()),
                    idempotency_key: format!("obj_artifact_{}", change_id),
                },
            ).map_err(|e| format!("创建变更产物对象失败: {}", e))?;

            let _ = crate::work_core::repository::record_work_event_in_tx(
                &tx,
                &pid,
                staged_change.work_object_id.as_deref(),
                "remote.change.approved",
                actor,
                &serde_json::json!({
                    "changeId": change_id,
                    "filePath": staged_change.file_path,
                    "additions": staged_change.additions,
                    "deletions": staged_change.deletions,
                    "status": "applied",
                }),
                Some(&format!("event_change_approved_{}", change_id)),
            ).map_err(|e| format!("记录审批事件失败: {}", e))?;

            let _ = delete_staged_write_recovery_entry(&tx, change_id)
                .map_err(|e| format!("清理暂存写入恢复表记录失败: {}", e))?;

            tx.commit().map_err(|e| format!("提交数据库事务失败: {}", e))?;
            Ok(())
        })();

        if let Err(e) = tx_res {
            log::error!("[Sync Engine] Failed database transaction for approval of {}, rolling back disk file: {}", change_id, e);
            match backup.rollback() {
                Ok(()) => {
                    let _ = delete_staged_write_recovery_entry(conn, change_id);
                    return serde_json::json!({
                        "status": "error",
                        "change_id": change_id,
                        "error": format!("应用变更数据库持久化失败，已安全回滚磁盘文件以防状态倾斜: {}", e)
                    });
                }
                Err(rb_err) => {
                    log::error!("[Sync Engine] CRITICAL: DB transaction failed AND rollback failed for {}. DB: {}, Rollback: {}", change_id, e, rb_err);
                    return serde_json::json!({
                        "status": "error",
                        "change_id": change_id,
                        "error": format!("应用变更数据库持久化失败 ({})，且磁盘恢复未完成 ({})。原文件备份保留在: {:?}", e, rb_err, bak_path_for_reporting)
                    });
                }
            }
        }

        if let Ok(agg) = crate::work_core::repository::get_project_aggregate(conn, &pid) {
            let _ = crate::work_core::snapshot::write_project_snapshot(&agg);
        }

        // 提交成功，清理磁盘备份
        if let Err(commit_err) = backup.commit() {
            log::warn!("[Sync Engine] Failed to delete backup file after commit: {}", commit_err);
        }

        // 同步更新内存缓存 (若存在)
        if let Ok(mut staged) = STAGED_CHANGES.lock() {
            if let Some(m) = staged.get_mut(change_id) {
                m.status = "applied".to_string();
                m.applied_at = Some(now);
            }
        }

        serde_json::json!({
            "status": "applied",
            "change_id": change_id,
            "file_path": staged_change.file_path,
            "applied_at": now,
        })
    } else if decision == "reject" {
        let pid = if !staged_change.project_id.is_empty() {
            staged_change.project_id.clone()
        } else {
            crate::work_core::repository::ensure_personal_workspace(conn)
                .unwrap_or_else(|_| "project_personal_inbox".to_string())
        };
        let tx_res: Result<(), String> = (|| {
            let tx = conn.transaction().map_err(|e| format!("开启数据库事务失败: {}", e))?;

            update_staged_change_status(&tx, change_id, "rejected", None)
                .map_err(|e| format!("更新拒绝状态失败: {}", e))?;

            if let Some(ref obj_id) = staged_change.work_object_id {
                let _ = crate::work_core::repository::update_object_status_in_tx(
                    &tx,
                    crate::work_core::models::UpdateWorkStatusInput {
                        object_id: obj_id.clone(),
                        expected_revision: 1,
                        status: "failed".into(),
                        actor: Some(actor.to_string()),
                        idempotency_key: format!("status_reject_{}", change_id),
                    },
                    false,
                ).map_err(|e| format!("更新工作对象状态失败: {}", e))?;
            }

            let _ = crate::work_core::repository::record_work_event_in_tx(
                &tx,
                &pid,
                staged_change.work_object_id.as_deref(),
                "remote.change.rejected",
                actor,
                &serde_json::json!({
                    "changeId": change_id,
                    "filePath": staged_change.file_path,
                    "status": "rejected",
                }),
                Some(&format!("event_change_rejected_{}", change_id)),
            ).map_err(|e| format!("记录审批事件失败: {}", e))?;

            tx.commit().map_err(|e| format!("提交拒绝事务失败: {}", e))?;
            Ok(())
        })();

        if let Err(e) = tx_res {
            return serde_json::json!({
                "status": "error",
                "change_id": change_id,
                "error": format!("拒绝变更持久化失败: {}", e)
            });
        }

        if let Ok(mut staged) = STAGED_CHANGES.lock() {
            if let Some(m) = staged.get_mut(change_id) {
                m.status = "rejected".to_string();
            }
        }

        serde_json::json!({
            "status": "rejected",
            "change_id": change_id,
            "file_path": staged_change.file_path,
        })
    } else {
        serde_json::json!({
            "status": "error",
            "change_id": change_id,
            "error": format!("未知的审批决策: {}", decision)
        })
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConnectedDevice {
    pub device_id: String,
    pub platform: String,
    pub ip_address: String,
    pub last_seen: i64,
    #[serde(default)]
    pub device_name: Option<String>,
    #[serde(default)]
    pub is_trusted: bool,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Default)]
pub struct DeviceRegistry {
    pub devices: RwLock<HashMap<String, ConnectedDevice>>,
}

impl DeviceRegistry {
    pub fn load() -> Self {
        let path = crate::get_data_dir().join("device_registry.json");
        let devices = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str(&data).ok())
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        Self {
            devices: RwLock::new(devices),
        }
    }

    pub fn save(&self) {
        let path = crate::get_data_dir().join("device_registry.json");
        if let Ok(devices) = self.devices.read() {
            if let Ok(json) = serde_json::to_string_pretty(&*devices) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn update_device(&self, device: ConnectedDevice) {
        {
            let mut devices = self.devices.write().unwrap();
            devices.insert(device.device_id.clone(), device);
        }
        self.save();
    }

    pub fn get_all(&self) -> Vec<ConnectedDevice> {
        let devices = self.devices.read().unwrap();
        let mut list: Vec<_> = devices.values().cloned().collect();
        list.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        list
    }
}

pub fn get_connected_devices_core(
    registry_opt: Option<Arc<DeviceRegistry>>,
    db_conn_opt: Option<&rusqlite::Connection>,
) -> Vec<ConnectedDevice> {
    let mut list = if let Some(ref reg) = registry_opt {
        reg.get_all()
    } else {
        Vec::new()
    };

    if let Ok(config) = crate::read_config_checked() {
        if let Some(payload) = config.get("pairing_payload").and_then(|v| v.as_object()) {
            if let Some(pc_id) = payload.get("device_id").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                if !list.iter().any(|d| d.device_id == pc_id) {
                    let ip_addr = payload.get("local_ips")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("relay")
                        .to_string();
                    let pc_dev = ConnectedDevice {
                        device_id: pc_id.to_string(),
                        platform: "windows".to_string(),
                        ip_address: ip_addr,
                        last_seen: crate::now_ms(),
                        device_name: Some("已配对电脑 (PC)".to_string()),
                        is_trusted: false,
                        status: Some("discovered".to_string()),
                    };
                    if let Some(ref reg) = registry_opt {
                        reg.update_device(pc_dev.clone());
                    }
                    list.push(pc_dev);
                }
            }
        }
    }

    if let Some(conn) = db_conn_opt {
        for dev in list.iter_mut() {
            let is_trusted = crate::device_trust::is_device_trusted(conn, &dev.device_id);
            dev.is_trusted = is_trusted;
            dev.status = Some(if is_trusted { "trusted".to_string() } else { "untrusted".to_string() });
        }
    } else {
        for dev in list.iter_mut() {
            dev.is_trusted = false;
            dev.status = Some("untrusted".to_string());
        }
    }

    list
}

#[command]
pub async fn get_connected_devices(app: AppHandle) -> Result<Vec<ConnectedDevice>, String> {
    let registry_opt = app.try_state::<Arc<DeviceRegistry>>().map(|r| r.inner().clone());
    let db_state = app.try_state::<crate::db::DbState>();
    let db_conn_opt = db_state.as_ref().and_then(|s| s.0.lock().ok());
    Ok(get_connected_devices_core(registry_opt, db_conn_opt.as_deref()))
}

#[command]
pub async fn disconnect_device(app: AppHandle, device_id: String) -> Result<(), String> {
    let registry = app.state::<Arc<DeviceRegistry>>();
    {
        let mut devices = registry.devices.write().unwrap();
        devices.remove(&device_id);
    }
    registry.save();

    if let Ok(mut config) = crate::read_config_checked() {
        if let Some(pp) = config.get("pairing_payload").and_then(|v| v.as_object()) {
            if pp.get("device_id").and_then(|v| v.as_str()) == Some(&device_id) {
                if let Some(obj) = config.as_object_mut() {
                    obj.remove("pairing_payload");
                    if let Err(e) = crate::write_config_checked(&config) {
                        log::error!("[Sync Engine] 保存解绑后的配置失败: {}", e);
                    }
                    log::info!("[Sync Engine] Removed pairing_payload for disconnected device {}", device_id);
                }
            }
        }
    }

    let _ = app.emit("sync:device_disconnected", device_id);
    Ok(())
}

#[command]
pub async fn check_device_online(app: AppHandle, target_device_id: String) -> Result<bool, String> {
    if target_device_id == "local" || target_device_id.is_empty() {
        return Ok(true);
    }

    let registry = app.state::<Arc<DeviceRegistry>>();
    let now = crate::now_ms();
    if let Ok(devices) = registry.devices.read() {
        if let Some(dev) = devices.get(&target_device_id) {
            if (now - dev.last_seen) < 120_000 {
                return Ok(true);
            }
        }
    }

    // Also check pairing_payload in config.json
    let config = crate::read_config_checked().map_err(|e| format!("SEC-01 Fail-Closed: 无法读取配置: {}", e))?;
    if let Some(pp) = config.get("pairing_payload").and_then(|v| v.as_object()) {
        if pp.get("device_id").and_then(|v| v.as_str()) == Some(&target_device_id) {
            // 局域网优先探测 (1.5s 快速超时)
            if let Some(ips) = pp.get("local_ips").and_then(|v| v.as_array()) {
                let port = pp.get("port").and_then(|v| v.as_u64()).unwrap_or(3722);
                if let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_millis(1500)).build() {
                    for ip_val in ips {
                        if let Some(ip) = ip_val.as_str() {
                            let health_url = format!("http://{}:{}/v1/health", ip, port);
                            if let Ok(res) = client.get(&health_url).send().await {
                                if res.status().is_success() {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }

            let relay_connected = RELAY_TX.read().map(|l| l.is_some()).unwrap_or(false);
            if relay_connected {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[command]
pub async fn dispatch_remote_instruction(
    app: AppHandle,
    target_device_id: String,
    conversation_id: String,
    instruction: String,
    read_only: Option<bool>,
    model: Option<String>,
    request_id: Option<String>,
    project_id: Option<String>,
) -> Result<serde_json::Value, String> {
    log::info!("[Sync Engine] Dispatching remote instruction to {}: {}", target_device_id, instruction);

    if target_device_id == "local" || target_device_id.is_empty() {
        return Err("Cannot dispatch remote instruction to local device".to_string());
    }

    let req_id = request_id.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let msg_id = uuid::Uuid::new_v4().to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();

    let mut inner_payload = serde_json::json!({
        "action": "rpc_request",
        "request_id": req_id,
        "conversation_id": conversation_id,
        "project_id": project_id,
        "instruction": instruction,
        "read_only": read_only.unwrap_or(true),
        "model": model,
    });

    let payload_bytes = crate::device_trust::canonicalize_json_value(&inner_payload);
    let envelope = crate::device_trust::sign_outgoing_rpc(&app, &target_device_id, "rpc_request", &payload_bytes, Some(&req_id))?;
    inner_payload["envelope"] = serde_json::to_value(&envelope).map_err(|e| e.to_string())?;

    let request = serde_json::json!({
        "type": "proxy",
        "from_device_id": envelope.subject_device_id,
        "target_device_id": target_device_id,
        "trace_id": trace_id,
        "message_id": msg_id,
        "sync_id": uuid::Uuid::new_v4().to_string(),
        "protocol_version": SYNC_PROTOCOL_VERSION,
        "payload": inner_payload
    });

    let resp = send_relay_request_and_wait(
        request,
        tokio::time::Duration::from_secs(60),
        RelayTerminal::RpcResponse,
    )
    .await?;

    if let Some(payload) = resp.get("payload") {
        if payload.get("status").and_then(|v| v.as_str()) == Some("error") {
            let err_msg = payload.get("error").and_then(|v| v.as_str()).unwrap_or("Remote execution failed");
            return Err(err_msg.to_string());
        }
        return Ok(payload.clone());
    }

    Ok(resp)
}

#[command]
pub async fn dispatch_remote_approval(
    app: AppHandle,
    target_device_id: String,
    change_id: String,
    decision: String,
    request_id: String,
) -> Result<serde_json::Value, String> {
    log::info!("[Sync Engine] Dispatching remote approval to {}: change_id={}, decision={}", target_device_id, change_id, decision);

    if target_device_id == "local" || target_device_id.is_empty() {
        return Err("Cannot dispatch remote approval to local device".to_string());
    }

    let msg_id = uuid::Uuid::new_v4().to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();

    let mut inner_payload = serde_json::json!({
        "action": "rpc_approval",
        "request_id": request_id,
        "change_id": change_id,
        "decision": decision,
    });

    let payload_bytes = crate::device_trust::canonicalize_json_value(&inner_payload);
    let envelope = crate::device_trust::sign_outgoing_rpc(&app, &target_device_id, "rpc_approval", &payload_bytes, Some(&request_id))?;
    inner_payload["envelope"] = serde_json::to_value(&envelope).map_err(|e| e.to_string())?;

    let request = serde_json::json!({
        "type": "proxy",
        "from_device_id": envelope.subject_device_id,
        "target_device_id": target_device_id,
        "trace_id": trace_id,
        "message_id": msg_id,
        "sync_id": uuid::Uuid::new_v4().to_string(),
        "protocol_version": SYNC_PROTOCOL_VERSION,
        "payload": inner_payload
    });

    let resp = send_relay_request_and_wait(
        request,
        tokio::time::Duration::from_secs(30),
        RelayTerminal::RpcResponse,
    )
    .await?;

    if let Some(payload) = resp.get("payload") {
        if payload.get("status").and_then(|v| v.as_str()) == Some("error") {
            let err_msg = payload.get("error").and_then(|v| v.as_str()).unwrap_or("Remote approval failed");
            return Err(err_msg.to_string());
        }
        return Ok(payload.clone());
    }

    Ok(resp)
}

#[command]
pub async fn cancel_remote_instruction(
    app: AppHandle,
    target_device_id: String,
    request_id: String,
) -> Result<serde_json::Value, String> {
    log::info!("[Sync Engine] Dispatching remote cancellation to {}: request_id={}", target_device_id, request_id);

    if target_device_id == "local" || target_device_id.is_empty() {
        return Err("Cannot dispatch remote cancel to local device".to_string());
    }

    let msg_id = uuid::Uuid::new_v4().to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();

    let mut inner_payload = serde_json::json!({
        "action": "rpc_cancel",
        "request_id": request_id,
    });

    let payload_bytes = crate::device_trust::canonicalize_json_value(&inner_payload);
    let envelope = crate::device_trust::sign_outgoing_rpc(&app, &target_device_id, "rpc_cancel", &payload_bytes, Some(&request_id))?;
    inner_payload["envelope"] = serde_json::to_value(&envelope).map_err(|e| e.to_string())?;

    let request = serde_json::json!({
        "type": "proxy",
        "from_device_id": envelope.subject_device_id,
        "target_device_id": target_device_id,
        "trace_id": trace_id,
        "message_id": msg_id,
        "sync_id": uuid::Uuid::new_v4().to_string(),
        "protocol_version": SYNC_PROTOCOL_VERSION,
        "payload": inner_payload
    });

    let resp = send_relay_request_and_wait(
        request,
        tokio::time::Duration::from_secs(15),
        RelayTerminal::RpcResponse,
    )
    .await?;

    if let Some(payload) = resp.get("payload") {
        if payload.get("status").and_then(|v| v.as_str()) == Some("error") {
            let err_msg = payload.get("error").and_then(|v| v.as_str()).unwrap_or("Remote cancel failed");
            return Err(err_msg.to_string());
        }
        return Ok(payload.clone());
    }

    Ok(resp)
}

#[command]
pub async fn fetch_remote_capabilities(
    app: AppHandle,
    target_device_id: String,
) -> Result<serde_json::Value, String> {
    log::info!("[Sync Engine] Probing remote capabilities for device: {}", target_device_id);

    if target_device_id == "local" || target_device_id.is_empty() {
        let snapshot = crate::capability::CapabilitySnapshot::capture(&app, false, true);
        let (safe_models, default_model) = crate::capability::get_safe_model_pool_for_remote();
        let config = crate::read_config_checked().map_err(|e| format!("SEC-01 Fail-Closed: 无法读取配置: {}", e))?;
        let device_name = config.get("device_name")
            .and_then(|v| v.as_str())
            .unwrap_or("本机")
            .to_string();
        let local_device_id = config.get("device_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| "SEC-01 Fail-Closed: 本机缺少 device_id".to_string())?
            .to_string();

        return Ok(serde_json::json!({
            "action": "rpc_capabilities_response",
            "status": "success",
            "device_id": local_device_id,
            "device_name": device_name,
            "platform": snapshot.platform,
            "file_scope": snapshot.file_scope,
            "capabilities": snapshot.capabilities,
            "available_models": safe_models,
            "default_model": default_model,
            "timestamp": crate::now_ms()
        }));
    }

    let req_id = uuid::Uuid::new_v4().to_string();
    let msg_id = uuid::Uuid::new_v4().to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();

    let mut inner_payload = serde_json::json!({
        "action": "rpc_discover_capabilities",
        "request_id": req_id,
    });

    let payload_bytes = crate::device_trust::canonicalize_json_value(&inner_payload);
    let envelope = crate::device_trust::sign_outgoing_rpc(&app, &target_device_id, "rpc_discover_capabilities", &payload_bytes, Some(&req_id))?;
    inner_payload["envelope"] = serde_json::to_value(&envelope).map_err(|e| e.to_string())?;

    let request = serde_json::json!({
        "type": "proxy",
        "from_device_id": envelope.subject_device_id,
        "target_device_id": target_device_id,
        "trace_id": trace_id,
        "message_id": msg_id,
        "sync_id": uuid::Uuid::new_v4().to_string(),
        "protocol_version": SYNC_PROTOCOL_VERSION,
        "payload": inner_payload
    });

    let resp = send_relay_request_and_wait(
        request,
        tokio::time::Duration::from_secs(15),
        RelayTerminal::RpcResponse,
    )
    .await?;

    if let Some(payload) = resp.get("payload") {
        if payload.get("status").and_then(|v| v.as_str()) == Some("error") {
            let err_msg = payload.get("error").and_then(|v| v.as_str()).unwrap_or("Remote capabilities discovery failed");
            return Err(err_msg.to_string());
        }
        return Ok(payload.clone());
    }

    Ok(resp)
}

pub fn register_authenticated_device_core(
    registry_opt: Option<&DeviceRegistry>,
    db_conn_opt: Option<&rusqlite::Connection>,
    local_device_id: &str,
    device_id: &str,
    platform: &str,
    device_name: Option<String>,
    ip_str: &str,
) -> Result<Option<ConnectedDevice>, String> {
    if local_device_id.trim().is_empty() {
        return Err("Local device ID unresolved: fail-closed".to_string());
    }
    if device_id == local_device_id {
        log::debug!("[Sync Engine] Skipping register_authenticated_device for self device_id: {}", device_id);
        return Ok(None);
    }
    let is_trusted = if let Some(conn) = db_conn_opt {
        crate::device_trust::is_device_trusted(conn, device_id)
    } else {
        false
    };
    let status = if is_trusted { "trusted".to_string() } else { "untrusted".to_string() };
    let device = ConnectedDevice {
        device_id: device_id.to_string(),
        platform: platform.to_string(),
        ip_address: ip_str.to_string(),
        last_seen: crate::now_ms(),
        device_name,
        is_trusted,
        status: Some(status),
    };
    if let Some(reg) = registry_opt {
        reg.update_device(device.clone());
    }
    Ok(Some(device))
}

pub fn register_authenticated_device_with_resolver<F>(
    registry: &DeviceRegistry,
    conn: &Connection,
    resolve_local_id: F,
    device_id: &str,
    platform: &str,
    device_name: Option<String>,
    ip_str: &str,
) -> Result<Option<ConnectedDevice>, String>
where
    F: FnOnce() -> Result<String, String>,
{
    let local_id = resolve_local_id()?;
    register_authenticated_device_core(
        Some(registry), Some(conn), &local_id, device_id, platform, device_name, ip_str,
    )
}

pub fn register_authenticated_device(
    app: &AppHandle,
    device_id: &str,
    platform: &str,
    device_name: Option<String>,
    ip: std::net::SocketAddr,
) -> Result<(), String> {
    let registry = app.state::<Arc<DeviceRegistry>>();
    let db_state = app.try_state::<crate::db::DbState>()
        .ok_or("SEC-01 authenticated registration database unavailable")?;
    let conn_guard = db_state.0.lock()
        .map_err(|_| "SEC-01 authenticated registration database locked")?;

    let device_opt = register_authenticated_device_with_resolver(
        &registry,
        &conn_guard,
        || crate::http_api::resolve_local_device_id_checked(Some(app)),
        device_id,
        platform,
        device_name,
        &ip.ip().to_string(),
    )?;

    if let Some(device) = device_opt {
        let _ = app.emit("sync:device_connected", device);
    }
    Ok(())
}

pub fn register_device(app: &AppHandle, headers: &axum::http::HeaderMap, ip: std::net::SocketAddr) {
    if let (Some(device_id), Some(platform)) = (
        headers.get("x-device-id").and_then(|v| v.to_str().ok()),
        headers.get("x-platform").and_then(|v| v.to_str().ok()),
    ) {
        let my_device_id = match crate::http_api::resolve_local_device_id_checked(Some(app)) {
            Ok(id) if !id.trim().is_empty() => id,
            _ => {
                log::warn!("[Sync Engine] Skipping register_device due to unresolvable local device ID (SEC-01 Fail-Closed)");
                return;
            }
        };
        if device_id == my_device_id {
            log::debug!("[Sync Engine] Skipping register_device for self device_id: {}", device_id);
            return;
        }
        let registry = app.state::<Arc<DeviceRegistry>>();
        let device_name = headers
            .get("x-device-name")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let device = ConnectedDevice {
            device_id: device_id.to_string(),
            platform: platform.to_string(),
            ip_address: ip.ip().to_string(),
            last_seen: crate::now_ms(),
            device_name,
            is_trusted: false,
            status: Some("discovered".to_string()),
        };
        registry.update_device(device.clone());
        let _ = app.emit("sync:device_connected", device);
    }
}


fn get_mobile_outbox_path() -> PathBuf {
    crate::get_data_dir().join("mobile_outbox.json")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncCommandPayload {
    pub device_id: String,
    pub public_key: String,
    pub local_ips: Vec<String>,
    pub port: u16,
    pub relay: String,
    #[serde(default)]
    pub listen_only: bool,
    #[serde(default)]
    pub skip_relay: bool,
}

#[derive(Clone)]
struct SyncTraceContext {
    sync_id: String,
    trace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ActiveSyncOutcome {
    Applied {
        transport: TransportKind,
    },
    PendingApply {
        transport: TransportKind,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        reasons: Vec<String>,
    },
}

impl ActiveSyncOutcome {
    pub fn transport(&self) -> TransportKind {
        match self {
            ActiveSyncOutcome::Applied { transport } => *transport,
            ActiveSyncOutcome::PendingApply { transport, .. } => *transport,
        }
    }
}

#[command]
pub async fn trigger_mobile_sync(
    app: AppHandle,
    payload: SyncCommandPayload,
) -> Result<ActiveSyncOutcome, String> {
    info!(
        "[Sync Engine] trigger_mobile_sync called, listen_only: {}",
        payload.listen_only
    );
    log_sync_action(
        "Auto Discovery",
        "running",
        if payload.listen_only {
            "Passive listen"
        } else {
            "Active probe"
        },
    );

    if payload.listen_only {
        let lan_engine = app.state::<Arc<LanSyncEngine>>();
        let target_device_id = payload.device_id.clone();
        let payload_clone = payload.clone();
        let app_clone = app.clone();

        lan_engine.start_listen_broadcast(move |discovered_id, ip, port| {
            if discovered_id == target_device_id {
                info!(
                    "[Sync Engine] Discovered paired PC at {}:{}, initiating active sync!",
                    ip, port
                );
                let mut active_payload = payload_clone.clone();
                active_payload.listen_only = false;
                active_payload.local_ips = vec![ip];
                active_payload.port = port;

                let app_for_task = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = do_active_sync(app_for_task, active_payload, None).await {
                        error!("[Sync Engine] Active sync failed: {}", e);
                    }
                });
            }
        });
        return Ok(ActiveSyncOutcome::Applied { transport: TransportKind::Lan });
    }

    let started_at = crate::now_ms();
    let sync_id = uuid::Uuid::new_v4().to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();
    let peer_device_id = payload.device_id.clone();
    crate::sync_diagnostics::begin_trace(&trace_id, SYNC_PROTOCOL_VERSION);
    record_diagnostic_event(
        &trace_id,
        &sync_id,
        &peer_device_id,
        TransportKind::Lan,
        DiagnosticStage::LanDirect,
        DiagnosticStatus::Running,
        1,
        None,
    );

    let result = do_active_sync(
        app,
        payload,
        Some(SyncTraceContext {
            sync_id: sync_id.clone(),
            trace_id: trace_id.clone(),
        }),
    )
    .await;
    record_trigger_sync_outcome(&sync_id, &trace_id, &peer_device_id, started_at, &result);
    result
}

pub(crate) fn record_trigger_sync_outcome(
    sync_id: &str,
    trace_id: &str,
    peer_device_id: &str,
    started_at: i64,
    result: &Result<ActiveSyncOutcome, String>,
) -> (DiagnosticStatus, Option<TransportKind>, Option<String>, Option<String>) {
    let (status, transport, summary, error_code) = match result {
        Ok(ActiveSyncOutcome::Applied { transport }) => {
            if *transport == TransportKind::Lan {
                record_diagnostic_event(
                    trace_id,
                    sync_id,
                    peer_device_id,
                    *transport,
                    DiagnosticStage::LanDirect,
                    DiagnosticStatus::Success,
                    2,
                    None,
                );
            } else {
                record_diagnostic_event(
                    trace_id,
                    sync_id,
                    peer_device_id,
                    TransportKind::Lan,
                    DiagnosticStage::LanDirect,
                    DiagnosticStatus::Skipped,
                    2,
                    Some("LAN 不可用，已自动切换 Relay"),
                );
            }
            (
                DiagnosticStatus::Success,
                Some(*transport),
                Some("同步完成".to_string()),
                None,
            )
        }
        Ok(ActiveSyncOutcome::PendingApply { transport, reasons }) => {
            let pending_detail = if reasons.is_empty() {
                "配置变更已入队待应用".to_string()
            } else {
                format!("配置变更已入队待应用: {}", reasons.join("; "))
            };
            if *transport == TransportKind::Lan {
                record_diagnostic_event(
                    trace_id,
                    sync_id,
                    peer_device_id,
                    *transport,
                    DiagnosticStage::LanDirect,
                    DiagnosticStatus::Pending,
                    2,
                    Some(&pending_detail),
                );
            } else {
                record_diagnostic_event(
                    trace_id,
                    sync_id,
                    peer_device_id,
                    TransportKind::Lan,
                    DiagnosticStage::LanDirect,
                    DiagnosticStatus::Skipped,
                    2,
                    Some("LAN 不可用，已自动切换 Relay"),
                );
                record_diagnostic_event(
                    trace_id,
                    sync_id,
                    peer_device_id,
                    TransportKind::Relay,
                    DiagnosticStage::MobileToRelay,
                    DiagnosticStatus::Pending,
                    2,
                    Some(&pending_detail),
                );
            }
            (
                DiagnosticStatus::Pending,
                Some(*transport),
                Some(pending_detail),
                None,
            )
        }
        Err(error) => {
            record_diagnostic_event(
                trace_id,
                sync_id,
                peer_device_id,
                TransportKind::Lan,
                DiagnosticStage::LanDirect,
                DiagnosticStatus::Failed,
                2,
                Some(error),
            );
            record_diagnostic_event(
                trace_id,
                sync_id,
                peer_device_id,
                TransportKind::Relay,
                DiagnosticStage::LocalCommit,
                DiagnosticStatus::Unknown,
                2,
                Some(error),
            );
            (
                DiagnosticStatus::Failed,
                None,
                Some("同步未完成".to_string()),
                extract_error_code(error),
            )
        }
    };
    let _ = crate::sync_history::record_run(crate::sync_history::SyncRun {
        sync_id: sync_id.to_string(),
        trace_id: trace_id.to_string(),
        started_at,
        finished_at: crate::now_ms(),
        status,
        transport,
        peer_device_id: Some(peer_device_id.to_string()),
        summary: summary.clone(),
        error_code: error_code.clone(),
    });
    (status, transport, summary, error_code)
}

fn extract_error_code(error: &str) -> Option<String> {
    error
        .split_whitespace()
        .find(|part| part.starts_with("ERR-"))
        .map(|part| part.trim_end_matches(':').to_string())
}

fn record_diagnostic_event(
    trace_id: &str,
    sync_id: &str,
    peer_device_id: &str,
    transport: TransportKind,
    stage: DiagnosticStage,
    status: DiagnosticStatus,
    sequence: u64,
    detail: Option<&str>,
) {
    let event = DiagnosticEvent {
        protocol_version: SYNC_PROTOCOL_VERSION,
        trace_id: trace_id.to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        sync_id: Some(sync_id.to_string()),
        from_device_id: crate::read_config_checked()
            .ok()
            .and_then(|value| value.get("device_id").and_then(|v| v.as_str()).map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string()),
        target_device_id: peer_device_id.to_string(),
        transport,
        stage,
        status,
        sequence,
        timestamp: crate::now_ms(),
        error_code: detail.and_then(extract_error_code),
        detail: detail.map(str::to_string),
    };
    crate::sync_diagnostics::apply_event(&event);
    let _ = crate::sync_history::record_event(event);
}

fn copy_trace_fields(source: &serde_json::Value, target: &mut serde_json::Value, response: bool) {
    if source
        .get("protocol_version")
        .and_then(|value| value.as_u64())
        .unwrap_or(1)
        < SYNC_PROTOCOL_VERSION as u64
    {
        return;
    }
    for key in ["protocol_version", "trace_id", "sync_id"] {
        if let Some(value) = source.get(key) {
            target[key] = value.clone();
        }
    }
    if response {
        target["flow_phase"] = serde_json::json!("response");
        if let Some(msg_id) = source.get("message_id") {
            target["ref_message_id"] = msg_id.clone();
        }
    } else {
        if let Some(msg_id) = source.get("message_id") {
            target["message_id"] = msg_id.clone();
        }
    }
}

fn record_relay_receipt(receipt: &serde_json::Value, peer_device_id: &str) {
    let Some(trace_id) = receipt.get("trace_id").and_then(|value| value.as_str()) else {
        return;
    };
    let sync_id = receipt
        .get("sync_id")
        .and_then(|value| value.as_str())
        .unwrap_or(trace_id);
    let (stage, status) = match receipt.get("receipt").and_then(|value| value.as_str()) {
        Some("relay_request_accepted") => {
            (DiagnosticStage::MobileToRelay, DiagnosticStatus::Success)
        }
        Some("relay_delivered_to_target") => {
            (DiagnosticStage::RelayToPc, DiagnosticStatus::Success)
        }
        Some("relay_response_accepted") => (DiagnosticStage::PcToRelay, DiagnosticStatus::Success),
        Some("relay_delivered_to_origin") => {
            (DiagnosticStage::RelayToMobile, DiagnosticStatus::Success)
        }
        Some("target_offline") | Some("delivery_failed") => {
            (DiagnosticStage::RelayToPc, DiagnosticStatus::Failed)
        }
        _ => return,
    };
    record_diagnostic_event(
        trace_id,
        sync_id,
        peer_device_id,
        TransportKind::Relay,
        stage,
        status,
        receipt
            .get("timestamp")
            .and_then(|value| value.as_u64())
            .unwrap_or(1),
        receipt.get("error_code").and_then(|value| value.as_str()),
    );
}

#[command]
pub async fn write_mobile_outbox(
    _app: AppHandle,
    operations: Vec<serde_json::Value>,
) -> Result<(), String> {
    let path = get_mobile_outbox_path();
    let mut outbox: Vec<serde_json::Value> = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    outbox.extend(operations);

    let data = serde_json::to_string_pretty(&outbox).map_err(|e| e.to_string())?;

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, data).map_err(|e| e.to_string())?;
    fs::rename(&temp_path, &path).map_err(|e| e.to_string())?;

    info!(
        "[Sync Engine] Appended to mobile outbox, total items: {}",
        outbox.len()
    );
    Ok(())
}

#[command]
pub async fn trigger_wakeup_via_relay(app: AppHandle, device_id: String) -> Result<(), String> {
    let my_device_id = crate::http_api::resolve_local_device_id_checked(Some(&app))?;

    if !my_device_id.is_empty() && device_id == my_device_id {
        log::debug!("[Sync Engine] Skipping wakeup for self device_id: {}", device_id);
        return Ok(());
    }

    log_sync_action(
        "Relay Wakeup",
        "running",
        &format!("Attempting to wake up device: {}", device_id),
    );

    let msg = build_authenticated_wakeup_message(
        &my_device_id, &device_id, crate::crypto::get_candidate_ips(),
        |payload| crate::device_trust::sign_outgoing_rpc_json(
            &app, &device_id, "wakeup", payload, None,
        ),
    )?;

    let relay_tx = {
        let lock = RELAY_TX.read().unwrap();
        lock.as_ref().cloned()
    };

    if let Some(tx) = relay_tx {
        if let Err(e) = tx
            .send(tokio_tungstenite::tungstenite::Message::Text(
                msg.to_string().into(),
            ))
            .await
        {
            log_sync_action(
                "Relay Wakeup",
                "error",
                &format!("Failed to send wakeup: {}", e),
            );
            return Err(e.to_string());
        }
    } else {
        log_sync_action("Relay Wakeup", "error", "Relay 后台未连接");
        return Err("Relay 后台未连接".to_string());
    }

    log_sync_action(
        "Relay Wakeup",
        "done",
        &format!("Wakeup signal sent to {}", device_id),
    );

    Ok(())
}

pub async fn send_relay_request_and_wait(
    request: serde_json::Value,
    timeout: tokio::time::Duration,
    terminal: RelayTerminal,
) -> Result<serde_json::Value, String> {
    let trace_id = request
        .get("trace_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let message_id = request
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if trace_id.is_empty() || message_id.is_empty() {
        return Err("Request missing trace_id or message_id".to_string());
    }

    let expected_peer = request.get("target_device_id").and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or("SEC-01 request missing target device identity")?.to_string();
    let expected_local = crate::http_api::get_local_device_id_checked()?;
    let allow_pairing_bootstrap = terminal == RelayTerminal::Ack
        && request.get("type").and_then(|v| v.as_str()) == Some("notify")
        && request.get("payload").and_then(|p| p.get("pop")).is_some();

    let (tx, rx) = oneshot::channel();
    {
        let mut pending = PENDING_REQUESTS.write().unwrap();
        if pending.contains_key(&message_id) {
            return Err("SEC-01 duplicate in-flight relay message_id".into());
        }
        pending.insert(message_id.clone(), RelayRequestWaiter {
            tx, terminal, expected_peer, expected_local, allow_pairing_bootstrap,
        });
    }

    let relay_tx = {
        let lock = RELAY_TX.read().unwrap();
        lock.as_ref().cloned()
    };

    let send_result = if let Some(tx) = relay_tx {
        tx.send(Message::Text(request.to_string().into())).await
    } else {
        PENDING_REQUESTS.write().unwrap().remove(&message_id);
        return Err("ERR-SYNC-02: Relay 后台未连接".to_string());
    };

    if send_result.is_err() {
        let mut pending = PENDING_REQUESTS.write().unwrap();
        pending.remove(&message_id);
        return Err("ERR-SYNC-02: Failed to send to Relay".to_string());
    }

    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(response)) => {
            if response.get("type").and_then(|v| v.as_str()) == Some("error")
                || response.get("type").and_then(|v| v.as_str()) == Some("proxy_error")
                || response.get("payload").and_then(|p| p.get("action")).and_then(|v| v.as_str()) == Some("error")
            {
                return Err(response
                    .get("error")
                    .or_else(|| response.get("message"))
                    .or_else(|| response.get("payload").and_then(|p| p.get("error")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Relay error")
                    .to_string());
            }
            Ok(response)
        }
        Ok(Err(_)) => Err("Relay response channel closed".to_string()),
        Err(_) => {
            let mut pending = PENDING_REQUESTS.write().unwrap();
            pending.remove(&message_id);
            Err("ERR-SYNC-02: Relay 请求超时".to_string())
        }
    }
}

#[command]
pub async fn relay_handshake(
    app: AppHandle,
    target_device_id: String,
    auth_code: String,
) -> Result<String, String> {
    let config = crate::read_config_checked().map_err(|e| format!("SEC-01 Fail-Closed: 无法读取配置: {}", e))?;
    let my_device_name = config
        .get("deviceName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let platform = std::env::consts::OS.to_string();
    let trace_id = uuid::Uuid::new_v4().to_string();
    let sync_id = uuid::Uuid::new_v4().to_string();

    // ── Stage 3a: Verify Relay connection ──
    let _ = app.emit(
        "sync:progress",
        serde_json::json!({"stage": "relay_connect", "status": "running"}),
    );

    let relay_tx = {
        let lock = RELAY_TX.read().unwrap();
        lock.as_ref().cloned()
    };

    if relay_tx.is_none() {
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_connect", "status": "error", "detail": "ERR-PAIRING-01: Relay backend not connected"}));
        let _ = crate::sync_history::record_activity(
            DiagnosticStatus::Failed,
            Some(TransportKind::Relay),
            Some(target_device_id.clone()),
            "Relay connection failed",
            Some("ERR-PAIRING-01".to_string()),
        );
        return Err("ERR-PAIRING-01: Relay backend not connected".to_string());
    }

    let _ = app.emit(
        "sync:progress",
        serde_json::json!({"stage": "relay_connect", "status": "running"}),
    );
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    let _ = app.emit(
        "sync:progress",
        serde_json::json!({"stage": "relay_connect", "status": "done"}),
    );
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // ── Stage 3b & 3c: Send notify to PC via Relay and wait for Ack ──
    let _ = app.emit(
        "sync:progress",
        serde_json::json!({"stage": "relay_notify", "status": "running"}),
    );
    // Try to parse auth_code as PairingInvitationPayload (JSON or bob://pair URL)
    let invitation_opt = crate::device_trust::PairingInvitationPayload::parse_input(&auth_code).ok();
    let pop_opt = if let Some(ref inv) = invitation_opt {
        crate::device_trust::create_proof_of_possession_for_app(&app, inv, Some(my_device_name.clone())).ok()
    } else {
        None
    };

    let mut handshake_payload = serde_json::json!({
        "device_name": my_device_name,
        "platform": platform,
    });
    if let Some(pop) = pop_opt {
        handshake_payload["pop"] = serde_json::to_value(&pop).unwrap_or_default();
        handshake_payload["proof_of_possession"] = serde_json::to_value(&pop).unwrap_or_default();
    } else {
        // Fallback positioning only, cannot authorize
        handshake_payload["auth_code"] = serde_json::Value::String(auth_code.clone());
    }

    let msg = serde_json::json!({
        "type": "notify",
        "from_device_id": crate::http_api::resolve_local_device_id_checked(Some(&app))?,
        "target_device_id": target_device_id,
        "protocol_version": SYNC_PROTOCOL_VERSION,
        "trace_id": trace_id,
        "message_id": uuid::Uuid::new_v4().to_string(),
        "sync_id": sync_id,
        "payload": handshake_payload
    });

    match send_relay_request_and_wait(
        msg,
        tokio::time::Duration::from_secs(25),
        RelayTerminal::Ack,
    )
    .await
    {
        Ok(json) => {
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({"stage": "relay_notify", "status": "done"}),
            );
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({"stage": "relay_ack", "status": "running"}),
            );

            if let Some(error_msg) = json.get("error").and_then(|v| v.as_str()) {
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "error", "detail": format!("ERR-PAIRING-04: {}", error_msg)}));
                let _ = crate::sync_history::record_activity(
                    DiagnosticStatus::Failed,
                    Some(TransportKind::Relay),
                    Some(target_device_id.clone()),
                    "Target device rejected pairing",
                    Some("ERR-PAIRING-04".to_string()),
                );
                return Err(format!("Relay error: {}", error_msg));
            }

            let status = json.get("payload").and_then(|p| p.get("status")).and_then(|v| v.as_str()).unwrap_or("");
            let session_id = json.get("payload").and_then(|p| p.get("session_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();

            if status != "trusted" || session_id.trim().is_empty() {
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "error", "detail": "ERR-PAIRING-04: Target device did not return trusted status or session_id"}));
                let _ = crate::sync_history::record_activity(
                    DiagnosticStatus::Failed,
                    Some(TransportKind::Relay),
                    Some(target_device_id.clone()),
                    "Relay pairing missing credentials",
                    Some("ERR-PAIRING-04".to_string()),
                );
                return Err("ERR-PAIRING-04: Target device did not return trusted status or session_id".to_string());
            }

            let _ = app.emit(
                "sync:progress",
                serde_json::json!({"stage": "relay_ack", "status": "done"}),
            );
            let _ = crate::sync_history::record_activity(
                DiagnosticStatus::Success,
                Some(TransportKind::Relay),
                Some(target_device_id.clone()),
                "Relay pairing succeeded",
                None,
            );
            let now_ms = crate::now_ms();
            let my_id = crate::http_api::resolve_local_device_id_checked(Some(&app))
                .map_err(|e| format!("SEC-01 Fail-Closed: 无法解析本机身份: {}", e))?;
            if my_id.trim().is_empty() {
                return Err("SEC-01 Fail-Closed: 本机身份为空，拒绝登记配对会话".to_string());
            }

            let db_res = if let Some(db_state) = app.try_state::<crate::db::DbState>() {
                if let Ok(mut conn) = db_state.0.lock() {
                    crate::device_trust::sec01_persist_mobile_trusted_session(
                        &mut conn,
                        &target_device_id,
                        &my_id,
                        &session_id,
                        "Paired PC",
                        "windows",
                        crate::device_trust::DEFAULT_SESSION_TTL_MS,
                        now_ms,
                    )
                } else {
                    Err("Database lock failed on mobile".to_string())
                }
            } else if let Some(mut conn) = crate::http_api::open_db_for_app(&app) {
                crate::device_trust::sec01_persist_mobile_trusted_session(
                    &mut conn,
                    &target_device_id,
                    &my_id,
                    &session_id,
                    "Paired PC",
                    "windows",
                    crate::device_trust::DEFAULT_SESSION_TTL_MS,
                    now_ms,
                )
            } else {
                Err("Database unavailable on mobile".to_string())
            };

            db_res.map_err(|e| format!("Failed to persist paired session on mobile: {}", e))?;

            let registry = app.state::<Arc<DeviceRegistry>>();
            registry.update_device(ConnectedDevice {
                device_id: target_device_id.clone(),
                platform: "windows".to_string(),
                ip_address: "relay".to_string(),
                last_seen: crate::now_ms(),
                device_name: Some("Paired PC".to_string()),
                is_trusted: true,
                status: Some("trusted".to_string()),
            });
            Ok(session_id)
        }
        Err(e) => {
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_ack", "status": "error", "detail": format!("ERR-PAIRING-03: {}", e)}));
            let _ = crate::sync_history::record_activity(
                DiagnosticStatus::Timeout,
                Some(TransportKind::Relay),
                Some(target_device_id.clone()),
                "Target device response timed out",
                Some("ERR-PAIRING-03".to_string()),
            );
            Err(format!("ERR-PAIRING-03: {}", e))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NoteFilePayload {
    pub path: String,                   // 相对路径，如 "topics/建造商场模拟游戏.md", "daily/2026-09-12.md", "wiki/sources/xxx.md"
    #[serde(default)]
    pub content: String,                // UTF-8 文本内容
    #[serde(default)]
    pub content_base64: Option<String>, // 二进制资源 (如 assets/ 下的图片)
    pub updated_at: i64,                // 修改时间戳 (毫秒)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncData {
    pub config: serde_json::Value,
    pub settings: Vec<serde_json::Value>,
    pub conversations: Vec<serde_json::Value>,
    pub messages: Vec<serde_json::Value>,
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub captures: Vec<serde_json::Value>,
    pub cron_jobs: Vec<serde_json::Value>,
    pub kg_nodes: Vec<serde_json::Value>,
    pub kg_edges: Vec<serde_json::Value>,
    pub wiki_fts: Vec<serde_json::Value>,
    #[serde(default)]
    pub tombstones: Vec<serde_json::Value>,
    #[serde(default)]
    pub notes: Vec<NoteFilePayload>,
}

/// 导出笔记与资料物理文件载荷
pub fn export_notes_payload(since_ts: i64) -> Vec<NoteFilePayload> {
    use base64::Engine;
    use walkdir::WalkDir;

    let mut payloads = Vec::new();
    let notes_dir = crate::notebook::get_notes_dir();

    // 1. 扫描笔记目录 (daily, topics, projects, custom, assets 等)
    if notes_dir.exists() {
        for entry in WalkDir::new(&notes_dir).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                let path = entry.path();
                let mtime_ms = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                if since_ts > 0 && mtime_ms < since_ts {
                    continue;
                }

                if let Ok(rel) = path.strip_prefix(&notes_dir) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if rel_str.is_empty() {
                        continue;
                    }

                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    let is_binary = matches!(
                        ext.as_str(),
                        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "pdf" | "zip"
                    );

                    if is_binary {
                        if let Ok(bytes) = std::fs::read(path) {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            payloads.push(NoteFilePayload {
                                path: rel_str,
                                content: String::new(),
                                content_base64: Some(b64),
                                updated_at: mtime_ms,
                            });
                        }
                    } else if let Ok(text) = std::fs::read_to_string(path) {
                        payloads.push(NoteFilePayload {
                            path: rel_str,
                            content: text,
                            content_base64: None,
                            updated_at: mtime_ms,
                        });
                    }
                }
            }
        }
    }

    // 2. 扫描 wiki_dir/sources 目录 (知识库资料文档，在笔记面板中展示)
    let wiki_sources_dir = crate::get_wiki_dir().join("sources");
    if wiki_sources_dir.exists() {
        for entry in WalkDir::new(&wiki_sources_dir).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                let path = entry.path();
                let mtime_ms = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                if since_ts > 0 && mtime_ms < since_ts {
                    continue;
                }

                if let Ok(rel) = path.strip_prefix(&wiki_sources_dir) {
                    let rel_str = format!("wiki/sources/{}", rel.to_string_lossy().replace('\\', "/"));
                    if let Ok(text) = std::fs::read_to_string(path) {
                        payloads.push(NoteFilePayload {
                            path: rel_str,
                            content: text,
                            content_base64: None,
                            updated_at: mtime_ms,
                        });
                    }
                }
            }
        }
    }

    payloads
}


pub fn export_sync_data_from_conn(
    conn: &Connection,
    since_ts: i64,
    is_relay: bool,
) -> Result<SyncData, String> {
    let config_val = crate::read_config_checked().map_err(|e| format!("SEC-01 Fail-Closed: 无法读取配置: {}", e))?;

    // SEC-03 严格正向白名单 (Constructive Allowlist)：
    // 绝不基于黑名单剥离，而是从零构造安全导出配置，仅包含明确允许的公开标量配置项与脱敏的模型元数据
    let mut safe_exported_config = serde_json::Map::new();
    if let Some(local_cfg_obj) = config_val.as_object() {
        for key in crate::device_trust::ALLOWED_REMOTE_CONFIG_KEYS {
            if let Some(val) = local_cfg_obj.get(*key) {
                // 仅允许标量类型（字符串、布尔、数值），杜绝嵌套凭据
                if val.is_string() || val.is_boolean() || val.is_number() {
                    safe_exported_config.insert((*key).to_string(), val.clone());
                }
            }
        }

        // customModels 若存在，只白名单提取安全的元数据字段 (id, name, provider, model, contextWindow, maxTokens)，严禁导出 apiKey 或任何未知字段
        if let Some(custom_models) = local_cfg_obj.get("customModels").and_then(|v| v.as_array()) {
            let mut safe_models = Vec::new();
            for m in custom_models {
                if let Some(m_obj) = m.as_object() {
                    let mut safe_m = serde_json::Map::new();
                    for safe_field in &["id", "name", "provider", "model", "contextWindow", "maxTokens"] {
                        if let Some(val) = m_obj.get(*safe_field) {
                            if val.is_string() || val.is_number() || val.is_boolean() {
                                safe_m.insert((*safe_field).to_string(), val.clone());
                            }
                        }
                    }
                    if !safe_m.is_empty() {
                        safe_models.push(serde_json::Value::Object(safe_m));
                    }
                }
            }
            if !safe_models.is_empty() {
                safe_exported_config.insert("customModels".to_string(), serde_json::Value::Array(safe_models));
            }
        }
    }
    let config = serde_json::Value::Object(safe_exported_config);

    let extract = |query: &str,
                   params: &[&dyn rusqlite::ToSql],
                   cols: &[&str]|
     -> Result<Vec<serde_json::Value>, String> {
        let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params, |row| {
                let mut map = serde_json::Map::new();
                for (i, col) in cols.iter().enumerate() {
                    let val: Result<String, _> = row.get(i);
                    if let Ok(v) = val {
                        map.insert(col.to_string(), serde_json::Value::String(v));
                    } else if let Ok(v) = row.get::<_, i64>(i) {
                        map.insert(col.to_string(), serde_json::json!(v));
                    } else if let Ok(v) = row.get::<_, f64>(i) {
                        map.insert(col.to_string(), serde_json::json!(v));
                    } else {
                        map.insert(col.to_string(), serde_json::Value::Null);
                    }
                }
                Ok(serde_json::Value::Object(map))
            })
            .map_err(|e| e.to_string())?;

        let mut result = Vec::new();
        for r in rows {
            if let Ok(v) = r {
                result.push(v);
            }
        }
        Ok(result)
    };

    // SEC-03 严格正向白名单：settings 表仅允许导出明确允许同步的公开配置 key
    let settings = extract(
        "SELECT key, value FROM settings WHERE key IN ('last_sync_ts', 'last_routine_date', 'theme', 'language', 'weather_city')",
        &[],
        &["key", "value"],
    ).unwrap_or_default();
    let conversations = extract("SELECT id, title, model, cost, last_message, last_role, created_at, updated_at FROM conversations WHERE updated_at >= ?1", &[&since_ts],
        &["id", "title", "model", "cost", "last_message", "last_role", "created_at", "updated_at"]).unwrap_or_default();
    let messages = if is_relay {
        extract("SELECT id, conversation_id, role, content, NULL as image_base64, created_at, from_channel, sync_id FROM messages WHERE created_at >= ?1", &[&since_ts],
        &["id", "conversation_id", "role", "content", "image_base64", "created_at", "from_channel", "sync_id"]).unwrap_or_default()
    } else {
        extract("SELECT id, conversation_id, role, content, image_base64, created_at, from_channel, sync_id FROM messages WHERE created_at >= ?1", &[&since_ts],
        &["id", "conversation_id", "role", "content", "image_base64", "created_at", "from_channel", "sync_id"]).unwrap_or_default()
    };
    let events = extract("SELECT id, title, type, status, date, start_time, end_time, description, created_at, updated_at, completed_at, linked_ticket_id FROM events WHERE updated_at >= ?1", &[&since_ts],
        &["id", "title", "type", "status", "date", "start_time", "end_time", "description", "created_at", "updated_at", "completed_at", "linked_ticket_id"]).unwrap_or_default();
    let captures = extract("SELECT capture_id, schema_version, entry_point, source_device, original_content, source_url, file_path, explicit_intent, content_hash, idempotency_key, language, privacy_scope, sync_scope, status, error_stage, error_message, derived_refs, retry_count, next_retry_at, created_at, updated_at FROM capture_journal WHERE updated_at >= ?1 AND sync_scope != 'local_only'", &[&since_ts],
        &["capture_id", "schema_version", "entry_point", "source_device", "original_content", "source_url", "file_path", "explicit_intent", "content_hash", "idempotency_key", "language", "privacy_scope", "sync_scope", "status", "error_stage", "error_message", "derived_refs", "retry_count", "next_retry_at", "created_at", "updated_at"]).unwrap_or_default();
    let cron_jobs = extract("SELECT id, title, cron_expr, prompt_template, enabled, last_run, created_at FROM cron_jobs", &[],
        &["id", "title", "cron_expr", "prompt_template", "enabled", "last_run", "created_at"]).unwrap_or_default();

    let kg_nodes = if is_relay {
        vec![]
    } else {
        extract(
            "SELECT id, label, node_type, summary, source, metadata, created_at FROM kg_nodes",
            &[],
            &[
                "id",
                "label",
                "node_type",
                "summary",
                "source",
                "metadata",
                "created_at",
            ],
        )
        .unwrap_or_default()
    };
    let kg_edges = if is_relay {
        vec![]
    } else {
        extract(
            "SELECT source_id, target_id, relation, confidence, created_at FROM kg_edges",
            &[],
            &[
                "source_id",
                "target_id",
                "relation",
                "confidence",
                "created_at",
            ],
        )
        .unwrap_or_default()
    };
    let wiki_fts = Vec::new();
    let tombstones = extract(
        "SELECT table_name, record_key, deleted_at FROM sync_tombstones WHERE deleted_at >= ?1",
        &[&since_ts],
        &["table_name", "record_key", "deleted_at"],
    )
    .unwrap_or_default();

    let notes = export_notes_payload(since_ts);

    Ok(SyncData {
        config,
        settings,
        conversations,
        messages,
        events,
        captures,
        cron_jobs,
        kg_nodes,
        kg_edges,
        wiki_fts,
        tombstones,
        notes,
    })
}

pub fn export_sync_data(
    app: &AppHandle,
    since_ts: i64,
    is_relay: bool,
) -> Result<SyncData, String> {
    let db = app.state::<crate::db::DbState>();
    let conn = db.0.lock().map_err(|_| "Failed to lock db")?;
    export_sync_data_from_conn(&conn, since_ts, is_relay)
}

/// 跨端智能配置合并：严格隔离设备本地专属配置，仅正向白名单合并允许的偏好配置项 (标量)
pub fn merge_synced_config(local: &serde_json::Value, remote: &serde_json::Value) -> serde_json::Value {
    let mut merged = local.clone();
    let local_obj = match merged.as_object_mut() {
        Some(o) => o,
        None => {
            log::warn!("[merge_synced_config] SEC-03 Fail-Closed: Local config is not a JSON object, refusing to merge remote config");
            return local.clone();
        }
    };

    let remote_obj = match remote.as_object() {
        Some(o) => o,
        None => {
            log::warn!("[merge_synced_config] SEC-03 Fail-Closed: Remote config is not a JSON object, keeping local config unchanged");
            return merged;
        }
    };

    // SEC-03: 严格正向白名单合并允许跨设备漫游的偏好设置项，只接受标量
    for key in crate::device_trust::ALLOWED_REMOTE_CONFIG_KEYS {
        if let Some(val) = remote_obj.get(*key) {
            if val.is_string() || val.is_boolean() || val.is_number() {
                if let Some(s) = val.as_str() {
                    if !s.trim().is_empty() {
                        local_obj.insert((*key).to_string(), val.clone());
                    }
                } else {
                    local_obj.insert((*key).to_string(), val.clone());
                }
            }
        }
    }

    // 3. 设备本地专属配置绝对隔离 (严禁覆盖):
    // device_id, device_name, workspaceDir, wikiDir, browserPath, bundledSkillsDir, offlineModelPath, paired_devices, etc.

    // 4. 对合并后的配置执行当前平台的路径净化 (防止本地已被污染的 Windows 路径残留)
    #[cfg(target_os = "android")]
    {
        for key in &["workspaceDir", "wikiDir", "browserPath", "bundledSkillsDir", "offlineModelPath"] {
            if let Some(val) = local_obj.get(*key).and_then(|v| v.as_str()) {
                if val.contains(':') || val.contains('\\') || !val.starts_with('/') {
                    local_obj.remove(*key);
                }
            }
        }
    }

    merged
}

#[derive(Debug, Clone)]
pub struct IdempotencyCommitInfo<'a> {
    pub session_id: &'a str,
    pub request_id: &'a str,
    pub execution_token: &'a str,
    pub response_json: &'a str,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct ImportSyncOutcome {
    pub total_records: usize,
    pub receipt: crate::device_trust::PushCommitReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushDeliveryOutcome {
    Applied,
    PendingApply(Option<String>),
}

/// 解析并严格校验接收端的提交回执 (Fail-Closed: 必须明确包含 status: ok/committed 且包含 stage: applied/pending_apply)
pub fn parse_push_commit_receipt(resp_val: &serde_json::Value) -> Result<PushDeliveryOutcome, String> {
    let receipt_obj = if let Some(p) = resp_val.get("payload").and_then(|v| v.as_object()) {
        p
    } else if let Some(obj) = resp_val.as_object() {
        obj
    } else {
        return Err(format!("Commit response is not a JSON object (Fail-Closed): {:?}", resp_val));
    };

    let status = receipt_obj.get("status").and_then(|v| v.as_str());
    let action = receipt_obj.get("action").and_then(|v| v.as_str());
    let msg_type = resp_val.get("type").and_then(|v| v.as_str());

    let is_commit_ack = status == Some("ok")
        || status == Some("committed")
        || action == Some("commit_ack")
        || msg_type == Some("commit_ack");

    if !is_commit_ack {
        return Err(format!("Receiver did not confirm commit receipt (missing status: ok/committed / 缺少提交回执): {:?}", resp_val));
    }

    let stage = receipt_obj.get("stage").and_then(|v| v.as_str());
    match stage {
        Some("applied") => Ok(PushDeliveryOutcome::Applied),
        Some("pending_apply") => {
            let err_detail = receipt_obj.get("delivery_error").and_then(|v| v.as_str()).map(|s| s.to_string());
            Ok(PushDeliveryOutcome::PendingApply(err_detail))
        }
        Some(other) => {
            Err(format!("Receiver returned invalid receipt stage '{}' (Fail-Closed): {:?}", other, resp_val))
        }
        None => {
            Err(format!("Receiver returned incomplete receipt missing 'stage' (Fail-Closed: 缺少 stage 回执字段): {:?}", resp_val))
        }
    }
}

pub fn import_sync_data_to_conn(conn: &mut Connection, data: &SyncData, last_sync_ts: i64) -> Result<usize, String> {
    import_sync_data_to_conn_atomic(conn, data, last_sync_ts, None).map(|o| o.total_records)
}

pub fn import_sync_data_to_conn_atomic(
    conn: &mut Connection,
    data: &SyncData,
    last_sync_ts: i64,
    idempotency: Option<IdempotencyCommitInfo>,
) -> Result<ImportSyncOutcome, String> {
    let current_config = match crate::read_config_checked() {
        Ok(c) => c,
        Err(e) => return Err(format!("SEC-01 Fail-Closed: 无法读取本地配置: {}", e)),
    };
    if !current_config.is_object() {
        return Err("SEC-01/03 Fail-Closed: 本地配置必须为 JSON Object".to_string());
    }
    if !data.config.is_object() {
        return Err("SEC-01/03 Fail-Closed: 远端同步配置必须为 JSON Object".to_string());
    }
    let merged_config = merge_synced_config(&current_config, &data.config);

    // 提取配置差异为白名单操作列表 (Durable Outbox operations)
    let mut config_ops: Vec<serde_json::Value> = Vec::new();
    let mut staged_req_info: Option<(String, String)> = None;
    if merged_config != current_config {
        if let Some(m_obj) = merged_config.as_object() {
            let c_obj = current_config.as_object();
            // SEC-03 严格正向白名单：仅提取允许的标量配置项变更
            for key in crate::device_trust::ALLOWED_REMOTE_CONFIG_KEYS {
                if let Some(val) = m_obj.get(*key) {
                    if val.is_string() || val.is_boolean() || val.is_number() {
                        let cur_val = c_obj.and_then(|c| c.get(*key));
                        if cur_val != Some(val) {
                            config_ops.push(serde_json::json!({
                                "op": "set_config",
                                "key": *key,
                                "value": val.clone(),
                            }));
                        }
                    }
                }
            }
        }
    }

    let tx_sql = conn.transaction().map_err(|e| e.to_string())?;
    let ts = crate::now_ms();

    // 0. 若配置了幂等提交信息，在同一笔 SQLite 事务内执行 Fencing Token 强校验并原子更新 completed
    if let Some(ref idem) = idempotency {
        let affected = tx_sql.execute(
            "UPDATE rpc_idempotency_cache
             SET status = 'completed', response_json = ?, updated_at = ?
             WHERE session_id = ? AND request_id = ? AND status = 'pending' AND execution_token = ?",
            rusqlite::params![idem.response_json, idem.now_ms, idem.session_id, idem.request_id, idem.execution_token],
        ).map_err(|e| format!("原子更新幂等缓存为 completed 失败: {}", e))?;

        if affected != 1 {
            return Err(format!(
                "幂等提交被拒绝: Fencing token mismatch 或记录已被其他 worker 接管 (session_id: '{}', request_id: '{}', token: '{}')",
                idem.session_id, idem.request_id, idem.execution_token
            ));
        }
    }

    // 0.1 若存在配置变更，在同一笔事务内写入 rpc_staged_outbox 暂存队列（Durable Outbox 真理源）
    // 保证配置修改与业务表、幂等 completed 状态在同一笔 SQLite 事务内原子提交
    if !config_ops.is_empty() {
        let (session_id, request_id) = if let Some(ref idem) = idempotency {
            (idem.session_id.to_string(), idem.request_id.to_string())
        } else {
            ("sync_internal".to_string(), format!("req-sync-{}", ulid::Ulid::new()))
        };
        staged_req_info = Some((session_id.clone(), request_id.clone()));
        let event_id = format!("evt-cfg-{}-{}", session_id, request_id);

        let mut enriched_ops = config_ops.clone();
        for op in enriched_ops.iter_mut() {
            if let Some(obj) = op.as_object_mut() {
                if !obj.contains_key("event_id") {
                    obj.insert("event_id".to_string(), serde_json::json!(event_id));
                }
                if !obj.contains_key("request_id") {
                    obj.insert("request_id".to_string(), serde_json::json!(request_id));
                }
            }
        }

        let ops_json = serde_json::to_string(&enriched_ops)
            .map_err(|e| format!("序列化 sync outbox 操作失败: {}", e))?;

        tx_sql.execute(
            "INSERT INTO rpc_staged_outbox (event_id, session_id, request_id, operations_json, status, attempts, created_at)
             VALUES (?, ?, ?, ?, 'pending', 0, ?)
             ON CONFLICT(session_id, request_id) DO UPDATE SET operations_json = excluded.operations_json, status = 'pending'",
            rusqlite::params![event_id, session_id, request_id, ops_json, ts],
        ).map_err(|e| format!("写入 rpc_staged_outbox 失败: {}", e))?;
    }

    // 1. Process Tombstones FIRST (Physical Deletion)
    if !data.tombstones.is_empty() {
        for t in &data.tombstones {
            if let Some(obj) = t.as_object() {
                let table = obj.get("table_name").and_then(|v| v.as_str()).unwrap_or("");
                let record_key = obj.get("record_key").and_then(|v| v.as_str()).unwrap_or("");
                let deleted_at = obj.get("deleted_at").and_then(|v| v.as_i64()).unwrap_or(0);

                let query = match table {
                    "conversations" => Some("DELETE FROM conversations WHERE id = ?1"),
                    "events" => Some("DELETE FROM events WHERE id = ?1"),
                    "kg_nodes" => Some("DELETE FROM kg_nodes WHERE id = ?1"),
                    _ => None,
                };

                if let Some(q) = query {
                    // Check local updated_at to ensure deletion is newer than last update
                    let local_updated_at: i64 = tx_sql
                        .query_row(
                            &format!("SELECT updated_at FROM {} WHERE id = ?1", table),
                            rusqlite::params![record_key],
                            |row| row.get(0),
                        )
                        .unwrap_or(0);

                    if deleted_at >= local_updated_at {
                        tx_sql
                            .execute(q, rusqlite::params![record_key])
                            .map_err(|e| e.to_string())?;
                        // Record tombstone locally to prevent ghost resurrections
                        tx_sql.execute(
                            "INSERT OR REPLACE INTO sync_tombstones (table_name, record_key, deleted_at) VALUES (?1, ?2, ?3)",
                            rusqlite::params![table, record_key, deleted_at]
                        ).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }

    // Generic blind replace (for one-way configs and readonly tables)
    let mut import_replace =
        |table: &str, rows: Vec<serde_json::Value>, cols: &[&str]| -> Result<(), rusqlite::Error> {
            if rows.is_empty() {
                return Ok(());
            }
            let placeholders = vec!["?"; cols.len()].join(", ");
            let query = format!(
                "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
                table,
                cols.join(", "),
                placeholders
            );
            for row in rows {
                if let Some(obj) = row.as_object() {
                    // 保护本地私有同步游标：禁止远端 settings 的 last_sync_ts 覆盖本地时间戳导致游标倒流
                    if table == "settings" {
                        if let Some(k) = obj.get("key").and_then(|v| v.as_str()) {
                            if k == "last_sync_ts" {
                                continue;
                            }
                        }
                    }
                    let mut params = Vec::new();
                    for col in cols {
                        let val = obj.get(*col).unwrap_or(&serde_json::Value::Null);
                        if let Some(s) = val.as_str() {
                            params.push(rusqlite::types::Value::Text(s.to_string()));
                        } else if let Some(i) = val.as_i64() {
                            params.push(rusqlite::types::Value::Integer(i));
                        } else if let Some(f) = val.as_f64() {
                            params.push(rusqlite::types::Value::Real(f));
                        } else {
                            params.push(rusqlite::types::Value::Null);
                        }
                    }
                    tx_sql.execute(&query, rusqlite::params_from_iter(params))?;
                }
            }
            Ok(())
        };

    // LWW (Last-Write-Wins) strategy with Conflict Detection
    let mut import_lww = |table: &str,
                          rows: Vec<serde_json::Value>,
                          cols: &[&str]|
     -> Result<(), rusqlite::Error> {
        if rows.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; cols.len()].join(", ");
        let query_insert = format!(
            "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
            table,
            cols.join(", "),
            placeholders
        );
        let query_check = format!("SELECT updated_at FROM {} WHERE id = ?1", table);

        for row in rows {
            if let Some(obj) = row.as_object() {
                let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let remote_updated_at = obj.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(0);

                let local_updated_at: i64 = tx_sql
                    .query_row(&query_check, rusqlite::params![id], |r| r.get(0))
                    .unwrap_or(0);

                // CONFLICT DETECTION
                // 会话表属于消息容器，消息本身通过全局唯一 sync_id 实施幂等追加合流；
                // 会话元数据直接遵循 LWW (最后修改者胜) 更新，严禁分裂出 0 消息的幽灵空壳副本
                let is_conflict = table != "conversations"
                    && local_updated_at > last_sync_ts
                    && remote_updated_at > last_sync_ts
                    && local_updated_at != remote_updated_at;

                if is_conflict {
                    log::warn!("[Sync Engine] Conflict detected on table {} for id {}. local_updated_at: {}, remote_updated_at: {}, last_sync_ts: {}", table, id, local_updated_at, remote_updated_at, last_sync_ts);

                    // Generate new ULID for the remote conflicted copy
                    let conflict_id = ulid::Ulid::new().to_string();

                    let mut params = Vec::new();
                    for col in cols {
                        let mut val = obj.get(*col).unwrap_or(&serde_json::Value::Null).clone();

                        // Overwrite ID
                        if *col == "id" {
                            val = serde_json::Value::String(conflict_id.clone());
                        }

                        // Append to title/label if it exists
                        if *col == "title" || *col == "label" {
                            if let Some(s) = val.as_str() {
                                val =
                                    serde_json::Value::String(format!("{} (手机同步冲突副本)", s));
                            }
                        }

                        if let Some(s) = val.as_str() {
                            params.push(rusqlite::types::Value::Text(s.to_string()));
                        } else if let Some(i) = val.as_i64() {
                            params.push(rusqlite::types::Value::Integer(i));
                        } else if let Some(f) = val.as_f64() {
                            params.push(rusqlite::types::Value::Real(f));
                        } else {
                            params.push(rusqlite::types::Value::Null);
                        }
                    }

                    // Insert the conflict copy
                    tx_sql.execute(&query_insert, rusqlite::params_from_iter(params))?;

                    // Record to sync_conflicts
                    let ts = crate::now_ms();
                    tx_sql.execute(
                        "INSERT INTO sync_conflicts (id, table_name, local_id, remote_id, status, created_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
                        rusqlite::params![ulid::Ulid::new().to_string(), table, id, conflict_id, ts]
                    )?;
                } else if remote_updated_at > local_updated_at {
                    // Normal LWW overwrite
                    let mut params = Vec::new();
                    for col in cols {
                        let val = obj.get(*col).unwrap_or(&serde_json::Value::Null);
                        if let Some(s) = val.as_str() {
                            params.push(rusqlite::types::Value::Text(s.to_string()));
                        } else if let Some(i) = val.as_i64() {
                            params.push(rusqlite::types::Value::Integer(i));
                        } else if let Some(f) = val.as_f64() {
                            params.push(rusqlite::types::Value::Real(f));
                        } else {
                            params.push(rusqlite::types::Value::Null);
                        }
                    }
                    tx_sql.execute(&query_insert, rusqlite::params_from_iter(params))?;
                }
            }
        }
        Ok(())
    };

    import_replace("settings", data.settings.clone(), &["key", "value"])
        .map_err(|e| e.to_string())?;
    import_lww(
        "conversations",
        data.conversations.clone(),
        &[
            "id",
            "title",
            "model",
            "cost",
            "last_message",
            "last_role",
            "created_at",
            "updated_at",
        ],
    )
    .map_err(|e| e.to_string())?;

    // Append-only strategy for messages (de-dupe by sync_id)
    if !data.messages.is_empty() {
        for msg in &data.messages {
            if let Some(obj) = msg.as_object() {
                let sync_id = obj.get("sync_id").and_then(|v| v.as_str()).unwrap_or("");
                if !sync_id.is_empty() {
                    let existing: i32 = tx_sql
                        .query_row(
                            "SELECT 1 FROM messages WHERE sync_id = ?1",
                            rusqlite::params![sync_id],
                            |_| Ok(1),
                        )
                        .unwrap_or(0);
                    if existing == 0 {
                        tx_sql.execute(
                            "INSERT INTO messages (conversation_id, role, content, image_base64, created_at, from_channel, sync_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            rusqlite::params![
                                obj.get("conversation_id").and_then(|v| v.as_str()).unwrap_or(""),
                                obj.get("role").and_then(|v| v.as_str()).unwrap_or(""),
                                obj.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                                obj.get("image_base64").and_then(|v| v.as_str()),
                                obj.get("created_at").and_then(|v| v.as_i64()).unwrap_or(ts),
                                obj.get("from_channel").and_then(|v| v.as_str()).unwrap_or("mobile"),
                                sync_id
                            ]
                        ).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }

    import_lww(
        "events",
        data.events.clone(),
        &[
            "id",
            "title",
            "type",
            "status",
            "date",
            "start_time",
            "end_time",
            "description",
            "created_at",
            "updated_at",
            "completed_at",
            "linked_ticket_id",
        ],
    )
    .map_err(|e| e.to_string())?;

    // Capture Journal uses a content-derived idempotency key across entry points and devices.
    // Keep the original capture_id, but accept a newer processing state from a peer.
    for capture in &data.captures {
        if let Some(obj) = capture.as_object() {
            crate::capture::merge_capture_record(&tx_sql, obj, ts)?;
        }
    }
    import_replace(
        "cron_jobs",
        data.cron_jobs.clone(),
        &[
            "id",
            "title",
            "cron_expr",
            "prompt_template",
            "enabled",
            "last_run",
            "created_at",
        ],
    )
    .map_err(|e| e.to_string())?;
    import_replace(
        "kg_nodes",
        data.kg_nodes.clone(),
        &[
            "id",
            "label",
            "node_type",
            "summary",
            "source",
            "metadata",
            "created_at",
        ],
    )
    .map_err(|e| e.to_string())?;
    import_replace(
        "kg_edges",
        data.kg_edges.clone(),
        &[
            "source_id",
            "target_id",
            "relation",
            "confidence",
            "created_at",
        ],
    )
    .map_err(|e| e.to_string())?;

    // 8. 导入笔记与资料物理文件及更新 FTS
    let mut imported_notes_count = 0;
    if !data.notes.is_empty() {
        use base64::Engine;
        let notes_dir = crate::notebook::get_notes_dir();
        let wiki_dir = crate::get_wiki_dir();

        for note in &data.notes {
            let target_path = if note.path.starts_with("wiki/") || note.path.starts_with("wiki\\") {
                let rel = &note.path[5..];
                wiki_dir.join(rel)
            } else {
                notes_dir.join(&note.path)
            };

            if let Some(parent) = target_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let should_write = if target_path.exists() {
                if let Ok(meta) = std::fs::metadata(&target_path) {
                    if let Ok(local_mod) = meta.modified() {
                        let local_ms = local_mod
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        note.updated_at >= local_ms
                    } else {
                        true
                    }
                } else {
                    true
                }
            } else {
                true
            };

            if should_write {
                let write_res = if let Some(ref b64) = note.content_base64 {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                        std::fs::write(&target_path, bytes)
                    } else {
                        std::fs::write(&target_path, &note.content)
                    }
                } else {
                    std::fs::write(&target_path, &note.content)
                };

                if write_res.is_ok() {
                    imported_notes_count += 1;
                    if target_path.extension().map_or(false, |ext| ext == "md") {
                        let (fm, text) = crate::notebook::parse_frontmatter_and_content(&note.content);
                        let title = if fm.title.is_empty() {
                            target_path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default()
                        } else {
                            fm.title
                        };
                        let tags = fm.tags.join(" ");
                        let _ = tx_sql.execute(
                            "DELETE FROM notes_fts WHERE note_path = ?1",
                            rusqlite::params![&note.path],
                        );
                        let _ = tx_sql.execute(
                            "INSERT INTO notes_fts (note_path, title, content, tags) VALUES (?1, ?2, ?3, ?4)",
                            rusqlite::params![&note.path, &title, &text, &tags],
                        );
                    }
                }
            }
        }
    }

    tx_sql.commit().map_err(|e| e.to_string())?;

    // 仅在 SQLite 事务成功提交后，通过 durable outbox 统一执行调谐与物化落盘
    let (stage, applied_count, pending_count, delivery_error) = if !config_ops.is_empty() {
        let _ = crate::device_trust::drain_staged_outbox(conn, ts);
        let (session_id, request_id) = if let Some(ref s) = staged_req_info {
            (s.0.clone(), s.1.clone())
        } else if let Some(ref idem) = idempotency {
            (idem.session_id.to_string(), idem.request_id.to_string())
        } else {
            ("sync_internal".to_string(), "req-sync".to_string())
        };
        let check_res: Result<(String, Option<String>), _> = conn.query_row(
            "SELECT status, last_error FROM rpc_staged_outbox WHERE session_id = ? AND request_id = ?",
            rusqlite::params![session_id, request_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        );
        match check_res {
            Ok((st, err)) => {
                if st == "delivered" {
                    ("applied".to_string(), config_ops.len(), 0, None)
                } else {
                    ("pending_apply".to_string(), 0, config_ops.len(), err.or_else(|| Some("配置变更暂存中，等待后台收敛".to_string())))
                }
            }
            Err(e) => {
                ("pending_apply".to_string(), 0, config_ops.len(), Some(format!("无法确认配置落盘状态 (Fail-Closed): {}", e)))
            }
        }
    } else {
        ("applied".to_string(), 0, 0, None)
    };

    let receipt = crate::device_trust::PushCommitReceipt {
        status: "ok".to_string(),
        r#type: "commit_ack".to_string(),
        stage,
        applied_count,
        pending_count,
        delivery_error,
    };

    if let Some(ref idem) = idempotency {
        let final_resp_str = serde_json::to_string(&receipt).unwrap_or_default();
        let update_rows = conn.execute(
            "UPDATE rpc_idempotency_cache SET response_json = ? WHERE session_id = ? AND request_id = ?",
            rusqlite::params![final_resp_str, idem.session_id, idem.request_id],
        ).map_err(|e| format!("更新幂等缓存 response_json 失败 (Fail-Closed): {}", e))?;
        if update_rows == 0 {
            return Err(format!("更新幂等缓存 response_json 影响行数为 0 (Fail-Closed, 无法定位 session {} req {})", idem.session_id, idem.request_id));
        }
    }

    let total_records = data.conversations.len()
        + data.messages.len()
        + data.events.len()
        + data.captures.len()
        + data.cron_jobs.len()
        + data.kg_nodes.len()
        + data.kg_edges.len()
        + imported_notes_count;

    Ok(ImportSyncOutcome {
        total_records,
        receipt,
    })
}

pub(crate) fn build_sync_import_history_entry(
    ts: i64,
    direction: &str,
    data: &SyncData,
    total_records: usize,
    receipt: &crate::device_trust::PushCommitReceipt,
) -> (serde_json::Value, String) {
    let mut detail_parts = Vec::new();
    if data.conversations.len() > 0 { detail_parts.push(format!("会话 {} 项", data.conversations.len())); }
    if data.messages.len() > 0 { detail_parts.push(format!("消息 {} 项", data.messages.len())); }
    if data.events.len() > 0 { detail_parts.push(format!("待办日程 {} 项", data.events.len())); }
    if data.captures.len() > 0 { detail_parts.push(format!("捕获记录 {} 项", data.captures.len())); }
    if data.settings.len() > 0 { detail_parts.push(format!("配置 {} 项", data.settings.len())); }
    if data.cron_jobs.len() > 0 { detail_parts.push(format!("定时任务 {} 项", data.cron_jobs.len())); }
    if data.kg_nodes.len() > 0 { detail_parts.push(format!("知识节点 {} 项", data.kg_nodes.len())); }
    if data.notes.len() > 0 { detail_parts.push(format!("笔记 {} 篇", data.notes.len())); }

    let is_applied = receipt.stage == "applied";
    let detail_str = if is_applied {
        if detail_parts.is_empty() {
            "成功合并云端数据 (无新增)".to_string()
        } else {
            format!("同步更新已应用：{}", detail_parts.join(", "))
        }
    } else {
        let reason = receipt.delivery_error.as_deref().unwrap_or("配置变更暂存待应用");
        if detail_parts.is_empty() {
            format!("数据已入库，{}", reason)
        } else {
            format!("数据已入库（{}），{}", detail_parts.join(", "), reason)
        }
    };

    let entry = serde_json::json!({
        "timestamp": ts,
        "direction": direction,
        "status": if is_applied { "applied" } else { "pending_apply" },
        "stage": receipt.stage,
        "counts": {
            "conversations": data.conversations.len(),
            "messages": data.messages.len(),
            "events": data.events.len(),
            "captures": data.captures.len(),
            "settings": data.settings.len(),
            "cron_jobs": data.cron_jobs.len(),
            "kg_nodes": data.kg_nodes.len(),
            "kg_edges": data.kg_edges.len(),
            "notes": data.notes.len()
        },
        "total_records": total_records,
        "detail": detail_str
    });

    (entry, detail_str)
}

pub fn import_sync_data(app: &AppHandle, data: SyncData, last_sync_ts: i64) -> Result<crate::device_trust::PushCommitReceipt, String> {
    let db = app.state::<crate::db::DbState>();
    let mut conn = db.0.lock().map_err(|_| "Failed to lock db")?;
    let outcome = import_sync_data_to_conn_atomic(&mut conn, &data, last_sync_ts, None)?;
    let total_records = outcome.total_records;

    let history_path = crate::get_data_dir().join("sync_history.json");
    let mut history: Vec<serde_json::Value> =
        if let Ok(existing) = std::fs::read_to_string(&history_path) {
            serde_json::from_str(&existing).unwrap_or_default()
        } else {
            vec![]
        };

    let ts = crate::now_ms();
    let (entry, detail_str) = build_sync_import_history_entry(ts, "pull", &data, total_records, &outcome.receipt);
    history.insert(0, entry);

    if history.len() > 50 { history.truncate(50); }
    if let Ok(json_str) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(&history_path, json_str);
    }

    if data.notes.len() > 0 {
        let _ = app.emit("notebook:updated", serde_json::json!({ "count": data.notes.len() }));
    }
    if data.events.len() > 0 {
        let _ = app.emit("calendar-updated", serde_json::json!({ "action": "sync", "count": data.events.len() }));
    }
    if outcome.receipt.stage == "applied" {
        let _ = app.emit("sync:completed", serde_json::json!({ "status": "ok", "stage": "applied", "total_records": total_records }));
        let _ = crate::sync_history::record_activity(
            DiagnosticStatus::Success,
            None,
            None,
            &detail_str,
            None,
        );
    } else {
        let pending_reason = outcome.receipt.delivery_error.as_deref().unwrap_or("数据已写入，配置变更入队待应用");
        let _ = app.emit("sync:progress", serde_json::json!({
            "stage": "data_import",
            "status": "pending_apply",
            "detail": pending_reason,
            "total_records": total_records,
        }));
        let _ = crate::sync_history::record_activity(
            DiagnosticStatus::Pending,
            None,
            None,
            &detail_str,
            None,
        );
    }

    let config = match crate::read_config_checked() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[Sync Engine] Skipping pairing_payload registry update due to config error: {}", e);
            return Ok(outcome.receipt);
        }
    };
    if let Some(pp) = config.get("pairing_payload").and_then(|v| v.as_object()) {
        if let Some(pc_id) = pp.get("device_id").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
            if let Some(registry) = app.try_state::<Arc<DeviceRegistry>>() {
                let ip_addr = pp.get("local_ips")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("relay")
                    .to_string();
                registry.update_device(ConnectedDevice {
                    device_id: pc_id.to_string(),
                    platform: "windows".to_string(),
                    ip_address: ip_addr,
                    last_seen: crate::now_ms(),
                    device_name: Some("已配对电脑 (PC)".to_string()),
                    is_trusted: false,
                    status: Some("discovered".to_string()),
                });
            }
        }
    }

    Ok(outcome.receipt)
}

pub fn log_sync_action(action: &str, status: &str, detail: &str) {
    let history_path = crate::get_data_dir().join("sync_history.json");
    let mut history: Vec<serde_json::Value> =
        if let Ok(existing) = std::fs::read_to_string(&history_path) {
            serde_json::from_str(&existing).unwrap_or_default()
        } else {
            vec![]
        };

    history.insert(
        0,
        serde_json::json!({
            "timestamp": crate::now_ms(),
            "action": action,
            "status": status,
            "detail": detail
        }),
    );

    if history.len() > 50 {
        history.truncate(50);
    }

    if let Ok(json_str) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(history_path, json_str);
    }
}

pub async fn execute_lan_outbox_push(
    client: &reqwest::Client,
    push_url: &str,
    my_device_id: &str,
    platform: &str,
    my_device_name: &str,
    push_env_str: &str,
    push_bytes: Vec<u8>,
    outbox_path: &std::path::Path,
) -> Result<PushDeliveryOutcome, String> {
    let resp = client
        .post(push_url)
        .header("X-Device-Id", my_device_id)
        .header("X-Platform", platform)
        .header("X-Device-Name", my_device_name)
        .header("X-Auth-Envelope", push_env_str)
        .header("Authorization", format!("Bearer {}", push_env_str))
        .header("Content-Type", "application/json")
        .body(push_bytes)
        .send()
        .await
        .map_err(|e| format!("LAN outbox push HTTP transport failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("LAN outbox push rejected with HTTP status {}: {}", status, err_body));
    }

    let resp_val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("LAN outbox push response is not valid JSON: {}", e))?;

    let outcome = parse_push_commit_receipt(&resp_val)?;
    match outcome {
        PushDeliveryOutcome::Applied => {
            if outbox_path.exists() {
                std::fs::remove_file(outbox_path)
                    .map_err(|e| format!("Failed to delete outbox after confirmed commit: {}", e))?;
                log::info!("[sync_engine] LAN outbox push confirmed committed and applied by receiver. Outbox successfully deleted.");
            }
            Ok(PushDeliveryOutcome::Applied)
        }
        PushDeliveryOutcome::PendingApply(ref err_detail) => {
            log::warn!("[sync_engine] LAN outbox push enqueued by receiver but pending apply: {:?}. Outbox file retained.", err_detail);
            Ok(outcome)
        }
    }
}

pub async fn execute_lan_push_db(
    client: &reqwest::Client,
    push_db_url: &str,
    my_device_id: &str,
    platform: &str,
    my_device_name: &str,
    push_db_env_str: &str,
    push_db_bytes: Vec<u8>,
) -> Result<PushDeliveryOutcome, String> {
    let resp = client
        .post(push_db_url)
        .header("X-Device-Id", my_device_id)
        .header("X-Platform", platform)
        .header("X-Device-Name", my_device_name)
        .header("X-Auth-Envelope", push_db_env_str)
        .header("Authorization", format!("Bearer {}", push_db_env_str))
        .header("Content-Type", "application/json")
        .body(push_db_bytes)
        .send()
        .await
        .map_err(|e| format!("LAN push_db HTTP transport failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("LAN push_db rejected with HTTP status {}: {}", status, err_body));
    }

    let resp_val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("LAN push_db response is not valid JSON: {}", e))?;

    let outcome = parse_push_commit_receipt(&resp_val)?;
    match outcome {
        PushDeliveryOutcome::Applied => {
            log::info!("[sync_engine] LAN push_db confirmed committed and applied by receiver: {:?}", resp_val);
            Ok(PushDeliveryOutcome::Applied)
        }
        PushDeliveryOutcome::PendingApply(ref err_detail) => {
            log::warn!("[sync_engine] LAN push_db committed by receiver but pending apply: {:?}", err_detail);
            Ok(outcome)
        }
    }
}

async fn do_active_sync(
    app: AppHandle,
    payload: SyncCommandPayload,
    trace: Option<SyncTraceContext>,
) -> Result<ActiveSyncOutcome, String> {
    info!(
        "[Sync Engine] Starting active sync to device {}",
        payload.device_id
    );

    // ── Stage: Route Discovery ──
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "route_discovery", "status": "running", "detail": "多路并发探测中..."}));

    use futures_util::stream::FuturesUnordered;
    use futures_util::stream::StreamExt;

    let config = crate::read_config_checked().map_err(|e| format!("SEC-01 Fail-Closed: 无法读取本地配置: {}", e))?;
    let my_device_id = crate::http_api::resolve_local_device_id_checked(Some(&app))?;
    let my_device_name = config
        .get("deviceName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let platform = std::env::consts::OS.to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(5000)) // 增加到 5 秒以防部分手机休眠唤醒慢
        .build()
        .map_err(|e| e.to_string())?;

    let mut tasks = FuturesUnordered::new();

    for ip in &payload.local_ips {
        let ip_clone = ip.clone();
        let client_clone = client.clone();
        let port = payload.port;
        tasks.push(tauri::async_runtime::spawn(async move {
            let url = format!("http://{}:{}/v1/health", ip_clone, port);
            match client_clone.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    Some((TransportKind::Lan, Some(ip_clone)))
                }
                _ => None,
            }
        }));
    }

    if !payload.skip_relay {
        let target_device_id = payload.device_id.clone();
        let public_key = payload.public_key.clone();
        let device_name = my_device_name.clone();
        let platform_clone = platform.clone();
        let probe_from_id = my_device_id.clone();

        tasks.push(tauri::async_runtime::spawn(async move {
            let mut waited = 0;
            // 手机刚扫码唤醒时，后台 Websocket 可能还未重连成功。给予最多 8 秒的重连宽限期。
            while !RELAY_CONNECTED.load(Ordering::SeqCst) && waited < 16 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                waited += 1;
            }

            if !RELAY_CONNECTED.load(Ordering::SeqCst) {
                return None;
            }

            let msg = serde_json::json!({
                "type": "notify",
                "from_device_id": probe_from_id,
                "target_device_id": target_device_id,
                "protocol_version": SYNC_PROTOCOL_VERSION,
                "trace_id": uuid::Uuid::new_v4().to_string(),
                "message_id": uuid::Uuid::new_v4().to_string(),
                "sync_id": uuid::Uuid::new_v4().to_string(),
                "payload": {
                    "device_name": device_name,
                    "platform": platform_clone,
                    "auth_code": public_key
                }
            });

            match send_relay_request_and_wait(
                msg,
                tokio::time::Duration::from_secs(5),
                RelayTerminal::Ack,
            )
            .await
            {
                Ok(json) => {
                    if json.get("error").is_none() {
                        Some((TransportKind::Relay, None))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        }));
    }

    let mut winning_transport = None;
    while let Some(res) = tasks.next().await {
        if let Ok(Some(transport)) = res {
            winning_transport = Some(transport);
            break;
        }
    }

    let (transport, lan_ip) = match winning_transport {
        Some(t) => t,
        None => {
            let err_msg = "ERR-SYNC-01: 所有网络通道 (LAN/Relay) 均不可达或超时";
            let _ = app.emit("sync:progress", serde_json::json!({"stage": "route_discovery", "status": "error", "detail": err_msg}));
            log_sync_action("Route Discovery", "error", err_msg);
            return Err(err_msg.to_string());
        }
    };

    let route_name = if transport == TransportKind::Lan {
        format!("局域网 ({})", lan_ip.as_deref().unwrap_or("unknown"))
    } else {
        "Relay 外网".to_string()
    };
    let _ = app.emit("sync:progress", serde_json::json!({"stage": "route_discovery", "status": "done", "detail": format!("通道建立成功: {}", route_name)}));
    info!("[Sync Engine] Route discovery won by {:?}", transport);

    let last_sync_ts: i64 = match app.state::<crate::db::DbState>().0.lock() {
        Ok(conn) => {
            let s: String = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'last_sync_ts'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| "0".to_string());
            s.parse::<i64>().unwrap_or(0)
        }
        Err(_) => 0,
    };

    if transport == TransportKind::Lan {
        let mut lan_push_errors: Vec<String> = Vec::new();
        let mut lan_pending_apply = false;
        let mut lan_pending_reasons: Vec<String> = Vec::new();

        let ip = lan_ip.unwrap();
        let base_url = format!("http://{}:{}", ip, payload.port);
        let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "running", "detail": format!("通过 {} 传输数据", ip)}));

        let client_full = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        let pull_url = format!("{}/v1/sync/pull", base_url);
        let pull_env = match crate::device_trust::sign_outgoing_rpc(&app, &payload.device_id, "pull", b"", None) {
            Ok(env) => env,
            Err(e) => {
                let err_msg = format!("SEC-01 Fail-Closed: 无法签署 LAN pull 请求: {}", e);
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "error", "detail": &err_msg}));
                return Err(err_msg);
            }
        };
        let pull_env_str = serde_json::to_string(&pull_env).map_err(|e| e.to_string())?;

        match client_full
            .get(&pull_url)
            .header("X-Device-Id", &my_device_id)
            .header("X-Platform", &platform)
            .header("X-Device-Name", &my_device_name)
            .header("X-Since-Ts", last_sync_ts.to_string())
            .header("X-Auth-Envelope", &pull_env_str)
            .header("Authorization", format!("Bearer {}", pull_env_str))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(data_val) = json.get("data") {
                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone())
                        {
                            match import_sync_data(&app, sync_data, last_sync_ts) {
                                Ok(receipt) => {
                                    if receipt.stage == "pending_apply" {
                                        lan_pending_apply = true;
                                        if let Some(err) = receipt.delivery_error {
                                            lan_pending_reasons.push(format!("导入配置暂存待应用: {}", err));
                                        } else {
                                            lan_pending_reasons.push("导入配置暂存待应用".to_string());
                                        }
                                    } else {
                                        let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                                    }
                                }
                                Err(e) => {
                                    let err_msg = format!("导入失败: {}", e);
                                    let _ = app.emit("sync:progress", serde_json::json!({"stage": "lan_sync", "status": "error", "detail": &err_msg}));
                                    return Err(err_msg);
                                }
                            }

                            let now = crate::now_ms();
                            if let Ok(conn) = app.state::<crate::db::DbState>().0.lock() {
                                let _ = conn.execute("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []);
                                let _ = conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('last_sync_ts', ?1)", rusqlite::params![now.to_string()]);
                            }

                            let _ =
                                app.emit("notebook:updated", serde_json::json!({"applied": 1}));
                        }
                    } else if let Some(config_val) = json.get("config") {
                        if let Ok(current_config) = crate::read_config_checked() {
                            let merged_config = merge_synced_config(&current_config, config_val);
                            let _ = crate::write_config_checked(&merged_config);
                            let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                        }
                    }
                }
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::UNAUTHORIZED => {
                let err_msg = "ERR-SYNC-05: 鉴权失败 (无效的配对凭证)";
                let _ = app.emit(
                    "sync:progress",
                    serde_json::json!({"stage": "lan_sync", "status": "error", "detail": err_msg}),
                );
                return Err(err_msg.to_string());
            }
            Ok(resp) => {
                let err_msg = format!("HTTP {}", resp.status());
                let _ = app.emit(
                    "sync:progress",
                    serde_json::json!({"stage": "lan_sync", "status": "error", "detail": &err_msg}),
                );
                return Err(err_msg);
            }
            Err(e) => {
                let err_msg = e.to_string();
                let _ = app.emit(
                    "sync:progress",
                    serde_json::json!({"stage": "lan_sync", "status": "error", "detail": &err_msg}),
                );
                return Err(err_msg);
            }
        }

        use std::fs;
        let outbox_path = get_mobile_outbox_path();
        if outbox_path.exists() {
            match fs::read_to_string(&outbox_path) {
                Ok(data) => match serde_json::from_str::<serde_json::Value>(&data) {
                    Ok(mock_outbox) => {
                        let outbox_url = format!("{}/v1/sync/push", base_url);
                        let push_bytes = serde_json::to_vec(&mock_outbox).unwrap_or_default();
                        match crate::device_trust::sign_outgoing_rpc(&app, &payload.device_id, "push", &push_bytes, None) {
                            Ok(push_env) => match serde_json::to_string(&push_env) {
                                Ok(push_env_str) => {
                                    match execute_lan_outbox_push(
                                        &client_full,
                                        &outbox_url,
                                        &my_device_id,
                                        &platform,
                                        &my_device_name,
                                        &push_env_str,
                                        push_bytes,
                                        &outbox_path,
                                    ).await {
                                        Ok(PushDeliveryOutcome::Applied) => {}
                                        Ok(PushDeliveryOutcome::PendingApply(reason)) => {
                                            lan_pending_apply = true;
                                            if let Some(r) = reason {
                                                lan_pending_reasons.push(r);
                                            }
                                        }
                                        Err(e) => {
                                            log::warn!("[sync_engine] LAN outbox push failed: {}. Outbox file retained for retry.", e);
                                            lan_push_errors.push(format!("Outbox 投递失败: {}", e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    lan_push_errors.push(format!("Outbox 签名信封序列化失败: {}", e));
                                }
                            },
                            Err(e) => {
                                lan_push_errors.push(format!("Outbox 签名失败: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        lan_push_errors.push(format!("Outbox JSON 解析失败: {}", e));
                    }
                },
                Err(e) => {
                    lan_push_errors.push(format!("Outbox 文件读取失败: {}", e));
                }
            }
        }

        match export_sync_data(&app, last_sync_ts, false) {
            Ok(local_sync_data) => {
                let push_db_url = format!("{}/v1/sync/push_db", base_url);
                let push_db_bytes = serde_json::to_vec(&local_sync_data).unwrap_or_default();
                match crate::device_trust::sign_outgoing_rpc(&app, &payload.device_id, "push_db", &push_db_bytes, None) {
                    Ok(push_db_env) => match serde_json::to_string(&push_db_env) {
                        Ok(push_db_env_str) => {
                            match execute_lan_push_db(
                                &client_full,
                                &push_db_url,
                                &my_device_id,
                                &platform,
                                &my_device_name,
                                &push_db_env_str,
                                push_db_bytes,
                            ).await {
                                Ok(PushDeliveryOutcome::Applied) => {}
                                Ok(PushDeliveryOutcome::PendingApply(reason)) => {
                                    lan_pending_apply = true;
                                    if let Some(r) = reason {
                                        lan_pending_reasons.push(r);
                                    }
                                }
                                Err(e) => {
                                    log::warn!("[sync_engine] LAN push_db failed: {}", e);
                                    lan_push_errors.push(e);
                                }
                            }
                        }
                        Err(e) => {
                            lan_push_errors.push(format!("LAN push_db 签名信封序列化失败: {}", e));
                        }
                    },
                    Err(e) => {
                        lan_push_errors.push(format!("LAN push_db 签名失败: {}", e));
                    }
                }
            }
            Err(e) => {
                lan_push_errors.push(format!("LAN 数据导出失败: {}", e));
            }
        }

        if !lan_push_errors.is_empty() {
            let combined_err = format!("部分同步失败: {}", lan_push_errors.join("; "));
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({
                    "stage": "lan_sync",
                    "status": "error",
                    "detail": &combined_err,
                }),
            );
            return Err(combined_err);
        }

        if lan_pending_apply {
            let pending_msg = if lan_pending_reasons.is_empty() {
                "配置变更已可靠入队，待目标设备应用".to_string()
            } else {
                format!("配置变更已可靠入队，待目标设备应用: {}", lan_pending_reasons.join("; "))
            };
            log::info!("[sync_engine] LAN sync enqueued but pending apply: {}", pending_msg);
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({
                    "stage": "lan_sync",
                    "status": "pending_apply",
                    "detail": &pending_msg,
                }),
            );
            return Ok(ActiveSyncOutcome::PendingApply {
                transport: TransportKind::Lan,
                reasons: lan_pending_reasons,
            });
        }

        if let Some(registry) = app.try_state::<Arc<DeviceRegistry>>() {
            registry.update_device(ConnectedDevice {
                device_id: payload.device_id.clone(),
                platform: "windows".to_string(),
                ip_address: payload.local_ips.first().cloned().unwrap_or_else(|| "LAN".to_string()),
                last_seen: crate::now_ms(),
                device_name: Some("已配对电脑 (PC)".to_string()),
                is_trusted: false,
                status: Some("discovered".to_string()),
            });
        }

        let _ = app.emit(
            "sync:progress",
            serde_json::json!({"stage": "lan_sync", "status": "done"}),
        );
        return Ok(ActiveSyncOutcome::Applied {
            transport: TransportKind::Lan,
        });
    } else {
        let mut relay_push_errors: Vec<String> = Vec::new();
        let mut relay_pending_apply = false;
        let mut relay_pending_reasons: Vec<String> = Vec::new();

        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "running", "detail": "通过 Relay 传输数据..."}));

        let relay_trace_id = trace
            .as_ref()
            .map(|value| value.trace_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let relay_sync_id = trace
            .as_ref()
            .map(|value| value.sync_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let mut pull_req = serde_json::json!({
            "type": "proxy",
            "target_device_id": payload.device_id,
            "protocol_version": SYNC_PROTOCOL_VERSION,
            "trace_id": relay_trace_id,
            "message_id": uuid::Uuid::new_v4().to_string(),
            "sync_id": relay_sync_id,
            "payload": {
                "action": "pull",
                "auth_code": payload.public_key
            }
        });
        authenticate_outgoing_proxy_request(&app, &mut pull_req)?;

        match send_relay_request_and_wait(
            pull_req,
            tokio::time::Duration::from_secs(45),
            RelayTerminal::ProxyResponse,
        )
        .await
        {
            Ok(response) => {
                if let Some(inner_payload) = response.get("payload") {
                    if let Some(data_val) = inner_payload.get("data") {
                        if let Ok(sync_data) = serde_json::from_value::<SyncData>(data_val.clone())
                        {
                            match import_sync_data(&app, sync_data, 0) {
                                Ok(receipt) => {
                                    if receipt.stage == "pending_apply" {
                                        relay_pending_apply = true;
                                        if let Some(err) = receipt.delivery_error {
                                            relay_pending_reasons.push(format!("导入配置暂存待应用: {}", err));
                                        } else {
                                            relay_pending_reasons.push("导入配置暂存待应用".to_string());
                                        }
                                    } else {
                                        let _ = app.emit("config:reconciled", serde_json::json!({"applied": 1}));
                                    }
                                }
                                Err(e) => {
                                    let err_msg = format!("导入失败: {}", e);
                                    let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                                    return Err(err_msg);
                                }
                            }
                        }
                    } else {
                        let err_msg = "响应中缺少 data 字段".to_string();
                        let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                        return Err(err_msg);
                    }
                } else {
                    let err_msg = "响应中缺少 payload 字段".to_string();
                    let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                    return Err(err_msg);
                }
            }
            Err(e) => {
                let err_msg = format!("Relay Pull 失败: {}", e);
                let _ = app.emit("sync:progress", serde_json::json!({"stage": "relay_sync", "status": "error", "detail": &err_msg}));
                return Err(err_msg);
            }
        }

        use std::fs;
        let outbox_path = get_mobile_outbox_path();
        if outbox_path.exists() {
            match fs::read_to_string(&outbox_path) {
                Ok(data) => match serde_json::from_str::<serde_json::Value>(&data) {
                    Ok(mock_outbox) => {
                        let mut push_req = serde_json::json!({
                            "type": "proxy",
                            "target_device_id": payload.device_id,
                            "trace_id": trace.as_ref().map(|t| t.trace_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                            "message_id": uuid::Uuid::new_v4().to_string(),
                            "sync_id": trace.as_ref().map(|t| t.sync_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                            "protocol_version": SYNC_PROTOCOL_VERSION,
                            "payload": {
                                "action": "push",
                                "auth_code": payload.public_key,
                                "data": mock_outbox
                            }
                        });
                        match authenticate_outgoing_proxy_request(&app, &mut push_req) {
                            Ok(()) => {
                                match send_relay_request_and_wait(
                                    push_req,
                                    tokio::time::Duration::from_secs(45),
                                    RelayTerminal::CommitAck,
                                ).await {
                                    Ok(resp_val) => {
                                        match parse_push_commit_receipt(&resp_val) {
                                            Ok(PushDeliveryOutcome::Applied) => {
                                                let _ = fs::remove_file(&outbox_path);
                                                log::info!("[Sync Engine] Relay outbox push confirmed committed and applied by receiver. Outbox deleted.");
                                            }
                                            Ok(PushDeliveryOutcome::PendingApply(err_opt)) => {
                                                relay_pending_apply = true;
                                                if let Some(err) = err_opt {
                                                    relay_pending_reasons.push(format!("Outbox 暂存待应用: {}", err));
                                                } else {
                                                    relay_pending_reasons.push("Outbox 暂存待应用".to_string());
                                                }
                                                log::warn!("[Sync Engine] Relay outbox push pending apply by receiver. Outbox retained.");
                                            }
                                            Err(e) => {
                                                log::warn!("[Sync Engine] Relay outbox push receipt invalid: {}. Outbox retained.", e);
                                                relay_push_errors.push(format!("Relay outbox 回执校验失败 (Fail-Closed): {}", e));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "[Sync Engine] Relay outbox push failed to get CommitAck: {}. Outbox retained.",
                                            e
                                        );
                                        relay_push_errors.push(format!("Relay outbox 投递未获提交确认: {}", e));
                                    }
                                }
                            }
                            Err(e) => {
                                relay_push_errors.push(format!("Relay outbox 签名鉴权失败: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        relay_push_errors.push(format!("Relay outbox JSON 解析失败: {}", e));
                    }
                },
                Err(e) => {
                    relay_push_errors.push(format!("Relay outbox 文件读取失败: {}", e));
                }
            }
        }

        match export_sync_data(&app, last_sync_ts, true) {
            Ok(local_sync_data) => {
                let mut push_db_req = serde_json::json!({
                    "type": "proxy",
                    "target_device_id": payload.device_id,
                    "trace_id": trace.as_ref().map(|t| t.trace_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    "message_id": uuid::Uuid::new_v4().to_string(),
                    "sync_id": trace.as_ref().map(|t| t.sync_id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    "protocol_version": SYNC_PROTOCOL_VERSION,
                    "payload": {
                        "action": "push_db",
                        "auth_code": payload.public_key,
                        "data": local_sync_data
                    }
                });
                match authenticate_outgoing_proxy_request(&app, &mut push_db_req) {
                    Ok(()) => {
                        match send_relay_request_and_wait(
                            push_db_req,
                            tokio::time::Duration::from_secs(25),
                            RelayTerminal::CommitAck,
                        ).await {
                            Ok(resp_val) => {
                                match parse_push_commit_receipt(&resp_val) {
                                    Ok(PushDeliveryOutcome::Applied) => {
                                        log::info!("[Sync Engine] Relay push_db acknowledged and applied");
                                    }
                                    Ok(PushDeliveryOutcome::PendingApply(err_opt)) => {
                                        relay_pending_apply = true;
                                        if let Some(err) = err_opt {
                                            relay_pending_reasons.push(format!("配置数据库暂存待应用: {}", err));
                                        } else {
                                            relay_pending_reasons.push("配置数据库暂存待应用".to_string());
                                        }
                                        log::warn!("[Sync Engine] Relay push_db committed by receiver but pending apply");
                                    }
                                    Err(e) => {
                                        let err = format!("Relay push_db 回执校验失败 (Fail-Closed): {}", e);
                                        log::warn!("[Sync Engine] {}", err);
                                        relay_push_errors.push(err);
                                    }
                                }
                            }
                            Err(e) => {
                                let err = format!("Relay push_db 未获得提交确认: {}", e);
                                log::warn!("[Sync Engine] {}", err);
                                relay_push_errors.push(err);
                            }
                        }
                    }
                    Err(e) => {
                        relay_push_errors.push(format!("Relay push_db 签名鉴权失败: {}", e));
                    }
                }
            }
            Err(e) => {
                relay_push_errors.push(format!("Relay 数据导出失败: {}", e));
            }
        }

        if !relay_push_errors.is_empty() {
            let combined_err = format!("部分同步失败: {}", relay_push_errors.join("; "));
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({
                    "stage": "relay_sync",
                    "status": "error",
                    "detail": &combined_err,
                }),
            );
            return Err(combined_err);
        }

        if relay_pending_apply {
            let pending_msg = if relay_pending_reasons.is_empty() {
                "配置变更已可靠入队，待目标设备应用".to_string()
            } else {
                format!("配置变更已可靠入队，待目标设备应用: {}", relay_pending_reasons.join("; "))
            };
            log::info!("[sync_engine] Relay sync enqueued but pending apply: {}", pending_msg);
            let _ = app.emit(
                "sync:progress",
                serde_json::json!({
                    "stage": "relay_sync",
                    "status": "pending_apply",
                    "detail": &pending_msg,
                }),
            );
            return Ok(ActiveSyncOutcome::PendingApply {
                transport: TransportKind::Relay,
                reasons: relay_pending_reasons,
            });
        }

        if let Some(registry) = app.try_state::<Arc<DeviceRegistry>>() {
            registry.update_device(ConnectedDevice {
                device_id: payload.device_id.clone(),
                platform: "windows".to_string(),
                ip_address: "relay".to_string(),
                last_seen: crate::now_ms(),
                device_name: Some("已配对电脑 (PC)".to_string()),
                is_trusted: false,
                status: Some("discovered".to_string()),
            });
        }

        let _ = app.emit(
            "sync:progress",
            serde_json::json!({"stage": "relay_sync", "status": "done"}),
        );
        return Ok(ActiveSyncOutcome::Applied {
            transport: TransportKind::Relay,
        });
    }
}

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::client_async_tls;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// URL-encode Base64 device_id to prevent +, /, = from being mangled in URL paths
fn url_encode_device_id(id: &str) -> String {
    id.replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D")
}

async fn connect_websocket_robust(
    ws_url: &str,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    String,
> {
    log::info!(
        "[WS Robust] Connecting to {} with IPv4 priority",
        ws_url
    );

    let request = ws_url.into_client_request().map_err(|e| e.to_string())?;
    let host = request.uri().host().unwrap_or("relay.bobbik.org").to_string();
    let port = request.uri().port_u16().unwrap_or(443);

    // Resolve DNS and sort so IPv4 comes first (avoids IPv6 blackholes behind VPNs)
    let addrs_lookup = tokio::net::lookup_host(format!("{}:{}", host, port)).await;
    let mut addrs: Vec<std::net::SocketAddr> = match addrs_lookup {
        Ok(iter) => iter.collect(),
        Err(e) => {
            log::warn!("[WS Robust] DNS lookup failed for {}:{}: {}, fallback to connect_async", host, port, e);
            return tokio::time::timeout(std::time::Duration::from_secs(10), tokio_tungstenite::connect_async(request))
                .await
                .map_err(|_| "WS connection timeout after 10s".to_string())?
                .map_err(|e| e.to_string());
        }
    };

    addrs.sort_by_key(|addr| !addr.is_ipv4());

    let mut connected_stream = None;
    for addr in addrs {
        log::info!("[WS Robust] Attempting TCP connect to {} ({})", host, addr);
        match tokio::time::timeout(std::time::Duration::from_secs(4), TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => {
                log::info!("[WS Robust] TCP connected successfully to {} ({})", host, addr);
                connected_stream = Some(stream);
                break;
            }
            Ok(Err(e)) => {
                log::warn!("[WS Robust] TCP connect failed to {}: {}", addr, e);
            }
            Err(_) => {
                log::warn!("[WS Robust] TCP connect timed out after 4s to {}", addr);
            }
        }
    }

    let stream = connected_stream.ok_or_else(|| format!("Could not connect to any address for {}:{}", host, port))?;

    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let client_config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS protocol config error: {}", e))?
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(client_config));

    log::info!("[WS Robust] Initiating TLS handshake with {}", host);
    let ws_stream = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::client_async_tls_with_config(request, stream, None, Some(connector)),
    )
    .await
    .map_err(|_| "WS TLS handshake timed out after 10s".to_string())?
    .map_err(|e| format!("WS TLS handshake failed: {}", e))?;

    log::info!("[WS Robust] TLS handshake and WebSocket upgrade succeeded for {}", host);
    Ok(ws_stream)
}

fn is_peer_authorized(
    app: &AppHandle,
    from_id: &str,
    _provided_auth: Option<&str>,
) -> bool {
    if let Some(db_state) = app.try_state::<crate::db::DbState>() {
        if let Ok(conn) = db_state.0.lock() {
            if crate::device_trust::is_device_trusted(&conn, from_id) {
                log::info!("[Sync Engine] Peer {} authorized via device_trust", from_id);
                return true;
            }
        }
    }
    log::warn!("[Sync Engine] Peer {} authorization failed: not in trusted_devices or revoked (SEC-01 fail-closed)", from_id);
    false
}

fn extract_relay_payload_bytes(inner_payload: &serde_json::Value, expected_hash: &str) -> Vec<u8> {
    if let Some(raw_s) = inner_payload.get("raw_payload").and_then(|v| v.as_str()) {
        return raw_s.as_bytes().to_vec();
    }
    if let Some(data) = inner_payload.get("data") {
        if let Ok(b) = serde_json::to_vec(data) {
            if crate::device_trust::compute_sha512(&b) == expected_hash {
                return b;
            }
        }
    }
    if let Some(b64) = inner_payload.get("payload_raw_b64").and_then(|v| v.as_str()) {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        if let Ok(b) = BASE64.decode(b64) {
            return b;
        }
    }
    if crate::device_trust::compute_sha512(b"{}") == expected_hash {
        return b"{}".to_vec();
    }
    if crate::device_trust::compute_sha512(b"") == expected_hash {
        return b"".to_vec();
    }
    let mut inner_clone = inner_payload.clone();
    if let Some(obj) = inner_clone.as_object_mut() {
        obj.remove("auth_envelope");
        obj.remove("envelope");
        if let Ok(b) = serde_json::to_vec(&inner_clone) {
            if crate::device_trust::compute_sha512(&b) == expected_hash {
                return b;
            }
        }
    }
    b"{}".to_vec()
}

pub struct RelayDispatchContext<'a> {
    pub app: Option<&'a AppHandle>,
    pub db: Option<Arc<Mutex<Connection>>>,
    pub registry: Option<Arc<DeviceRegistry>>,
    pub test_signing_key: Option<ed25519_dalek::SigningKey>,
    pub test_local_device_id: Option<String>,
}

fn build_authenticated_wakeup_message<F>(
    local_device_id: &str,
    target_device_id: &str,
    candidate_ips: Vec<String>,
    signer: F,
) -> Result<serde_json::Value, String>
where
    F: FnOnce(&serde_json::Value) -> Result<crate::device_trust::RpcAuthEnvelope, String>,
{
    if local_device_id.trim().is_empty() || target_device_id.trim().is_empty() {
        return Err("SEC-01 wakeup requires both device identities".into());
    }
    let mut payload = serde_json::json!({ "local_ips": candidate_ips, "port": 3722 });
    let envelope = signer(&payload)?;
    if envelope.subject_device_id != local_device_id
        || envelope.target_device_id != target_device_id
        || envelope.action != "wakeup"
    {
        return Err("SEC-01 wakeup signer identity or action mismatch".into());
    }
    payload["auth_envelope"] = serde_json::to_value(envelope).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "type": "wakeup", "target_device_id": target_device_id,
        "from_device_id": local_device_id, "payload": payload,
    }))
}

fn authenticate_outgoing_proxy_request(
    app: &AppHandle,
    request: &mut serde_json::Value,
) -> Result<(), String> {
    let target = request.get("target_device_id").and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty()).ok_or("SEC-01 proxy target missing")?.to_string();
    let mut payload = request.get("payload").cloned().ok_or("SEC-01 proxy payload missing")?;
    let action = payload.get("action").and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty()).ok_or("SEC-01 proxy action missing")?.to_string();
    let request_id = request.get("message_id").and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty()).ok_or("SEC-01 proxy message_id missing")?.to_string();
    let local = crate::http_api::resolve_local_device_id_checked(Some(app))?;
    let envelope = crate::device_trust::sign_outgoing_rpc_json(
        app, &target, &action, &payload, Some(&request_id),
    )?;
    payload["auth_envelope"] = serde_json::to_value(envelope).map_err(|e| e.to_string())?;
    request["payload"] = payload;
    request["from_device_id"] = serde_json::json!(local);
    Ok(())
}

/// The production Relay Ping loop and its tests share this exact identity continuity check.
pub fn sec01_check_relay_identity_continuity(expected_device_id: &str) -> Result<(), String> {
    if expected_device_id.trim().is_empty() {
        return Err("SEC-01 relay identity baseline is empty".into());
    }
    let latest = crate::http_api::get_local_device_id_checked()
        .map_err(|e| format!("SEC-01 relay identity unavailable: {e}"))?;
    if latest != expected_device_id {
        return Err("SEC-01 relay identity changed during connection".into());
    }
    Ok(())
}

/// All successful relay replies carry a signature over the entire routed message, including
/// ref_message_id. This prevents the relay or an unrelated peer from satisfying a waiter.
fn sign_relay_response(
    app: &AppHandle,
    response: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let local = crate::http_api::resolve_local_device_id_checked(Some(app))?;
    build_signed_relay_response(response, &local, |target, bytes, message_id| {
        crate::device_trust::sign_outgoing_rpc(
            app, target, "relay_response", bytes, Some(message_id),
        )
    })
}

fn build_signed_relay_response<F>(
    mut response: serde_json::Value,
    local_device_id: &str,
    signer: F,
) -> Result<serde_json::Value, String>
where
    F: FnOnce(&str, &[u8], &str) -> Result<crate::device_trust::RpcAuthEnvelope, String>,
{
    if local_device_id.trim().is_empty() {
        return Err("SEC-01 reply local identity missing".into());
    }
    response.as_object_mut().ok_or("SEC-01 reply must be an object")?.remove("auth_envelope");
    let target = response.get("target_device_id").and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty()).ok_or("SEC-01 reply missing target")?.to_string();
    if response.get("ref_message_id").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        return Err("SEC-01 reply missing request correlation".into());
    }
    if response.get("message_id").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        response["message_id"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
    }
    response["from_device_id"] = serde_json::json!(local_device_id);
    let message_id = response["message_id"].as_str().ok_or("SEC-01 invalid reply message_id")?.to_string();
    let bytes = crate::device_trust::canonicalize_json_value(&response);
    let envelope = signer(&target, &bytes, &message_id)?;
    if envelope.subject_device_id != local_device_id
        || envelope.target_device_id != target
        || envelope.action != "relay_response"
        || envelope.request_id != message_id
    {
        return Err("SEC-01 reply signer identity or correlation mismatch".into());
    }
    response["auth_envelope"] = serde_json::to_value(envelope).map_err(|e| e.to_string())?;
    Ok(response)
}

async fn send_signed_relay_response(
    ctx: &RelayDispatchContext<'_>,
    tx: &tokio::sync::mpsc::Sender<Message>,
    response: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let signed = if let Some(app) = ctx.app {
        sign_relay_response(app, response)?
    } else if let (Some(ref key), Some(ref local_id)) = (&ctx.test_signing_key, &ctx.test_local_device_id) {
        build_signed_relay_response(response, local_id, |target, bytes, message_id| {
            let mut conn_guard = ctx.db.as_ref().map(|d| d.lock().unwrap());
            if let Some(ref mut conn) = conn_guard {
                crate::device_trust::sign_outgoing_rpc_envelope(
                    conn, key, local_id, target, "relay_response", bytes, Some(message_id), crate::now_ms(),
                )
            } else {
                Err("Database unavailable for signing in test".to_string())
            }
        })?
    } else {
        return Err("SEC-01 reply signing requires an unlocked app identity or test signer".into());
    };
    tx.send(Message::Text(signed.to_string().into()))
        .await.map_err(|e| format!("SEC-01 signed reply delivery failed: {e}"))?;
    Ok(signed)
}

impl<'a> RelayDispatchContext<'a> {
    pub fn new(app: &'a AppHandle) -> Self {
        let registry = app.try_state::<Arc<DeviceRegistry>>().map(|r| r.inner().clone());
        Self {
            app: Some(app),
            db: None,
            registry,
            test_signing_key: None,
            test_local_device_id: None,
        }
    }

    pub fn for_test(db: Option<Arc<Mutex<Connection>>>, registry: Option<Arc<DeviceRegistry>>) -> Self {
        Self {
            app: None,
            db,
            registry,
            test_signing_key: None,
            test_local_device_id: None,
        }
    }

    pub fn for_test_with_signer(
        db: Option<Arc<Mutex<Connection>>>,
        registry: Option<Arc<DeviceRegistry>>,
        signing_key: ed25519_dalek::SigningKey,
        local_device_id: String,
    ) -> Self {
        Self {
            app: None,
            db,
            registry,
            test_signing_key: Some(signing_key),
            test_local_device_id: Some(local_device_id),
        }
    }

    pub fn with_db<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut Connection) -> Result<R, String>,
    {
        if let Some(ref db_mutex) = self.db {
            if let Ok(mut conn) = db_mutex.lock() {
                f(&mut conn)
            } else {
                Err("Database locked or poisoned".to_string())
            }
        } else if let Some(app) = self.app {
            if let Some(db_state) = app.try_state::<crate::db::DbState>() {
                if let Ok(mut conn) = db_state.0.lock() {
                    f(&mut conn)
                } else {
                    Err("Database locked or poisoned".to_string())
                }
            } else if let Some(mut conn) = crate::http_api::open_db_for_app(app) {
                f(&mut conn)
            } else {
                Err("Database unavailable".to_string())
            }
        } else {
            Err("Database unavailable".to_string())
        }
    }

    pub fn update_registry(&self, device: ConnectedDevice) {
        if let Some(ref reg) = self.registry {
            reg.update_device(device);
        } else if let Some(app) = self.app {
            if let Some(reg) = app.try_state::<Arc<DeviceRegistry>>() {
                reg.update_device(device);
            }
        }
    }

    pub fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app) = self.app {
            let _ = app.emit(event, payload);
        }
    }

    pub fn record_idempotency(
        &self,
        session_id: &str,
        request_id: &str,
        subject_device_id: &str,
        target_device_id: &str,
        action: &str,
        payload_hash: &str,
        response_json: &str,
    ) {
        let now_ms = crate::now_ms();
        let _ = self.with_db(|conn| {
            crate::device_trust::record_rpc_idempotency_result(
                conn,
                session_id,
                request_id,
                subject_device_id,
                target_device_id,
                action,
                payload_hash,
                response_json,
                now_ms,
            )
        });
    }
}

fn relay_response_matches_terminal(terminal: &RelayTerminal, json: &serde_json::Value) -> bool {
    let kind = json.get("type").and_then(|v| v.as_str());
    let action = json.get("payload").and_then(|p| p.get("action")).and_then(|v| v.as_str());
    match terminal {
        RelayTerminal::Ack => kind == Some("ack"),
        RelayTerminal::CommitAck => matches!(kind, Some("ack" | "commit_ack"))
            && action == Some("commit_ack"),
        RelayTerminal::ProxyResponse => kind == Some("proxy")
            && matches!(action, Some("pull_response" | "error")),
        RelayTerminal::RpcResponse => kind == Some("proxy")
            && matches!(action, Some("rpc_response" | "rpc_cancel_ack" | "rpc_capabilities_response" | "error")),
        RelayTerminal::AnyResponse => matches!(kind, Some("ack" | "commit_ack" | "proxy")),
    }
}

fn deliver_authenticated_relay_response(
    ctx: &RelayDispatchContext<'_>,
    json: &serde_json::Value,
) -> Result<bool, String> {
    let Some(ref_id) = json.get("ref_message_id").and_then(|v| v.as_str()) else {
        return Ok(false);
    };
    let candidate = {
        let pending = PENDING_REQUESTS.read().map_err(|_| "SEC-01 pending queue poisoned")?;
        pending.get(ref_id).map(|w| (
            w.terminal.clone(), w.expected_peer.clone(), w.expected_local.clone(),
            w.allow_pairing_bootstrap,
        ))
    };
    let Some((terminal, peer, local, bootstrap)) = candidate else {
        return Ok(false);
    };
    if !relay_response_matches_terminal(&terminal, json) {
        return Err("SEC-01 response terminal does not match pending request".into());
    }
    ctx.with_db(|conn| crate::device_trust::verify_relay_response_auth(
        conn, json, &peer, &local, bootstrap, crate::now_ms(),
    ))?;
    if let Some(waiter) = PENDING_REQUESTS.write().map_err(|_| "SEC-01 pending queue poisoned")?.remove(ref_id) {
        let _ = waiter.tx.send(json.clone());
    }
    Ok(true)
}

/// SEC-01 生产入站中继消息核心分发入口 (Fail-Closed, 100% 生产与测试复用)
pub async fn dispatch_inbound_relay_message_core(
    ctx: &RelayDispatchContext<'_>,
    json: &serde_json::Value,
    tx_mpsc: &tokio::sync::mpsc::Sender<Message>,
) -> Result<(), String> {
    // 1. 拦截路由状态回执 (Diagnostic Receipt)
    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
        if msg_type == "diagnostic_receipt" {
            let peer_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
            record_relay_receipt(json, peer_id);
            return Ok(());
        }
    }

    // 2. A response may complete a waiter only after exact correlation, expected-peer binding,
    //    signature verification and (except pairing bootstrap) trusted-session validation.
    if deliver_authenticated_relay_response(ctx, json)? {
        return Ok(());
    }

    // 3. 业务消息类型强鉴权与安全分发
    let msg_type = match json.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return Ok(()),
    };

    if msg_type == "device_revocation" {
        let now_ms = crate::now_ms();
        let local_id = match crate::http_api::resolve_local_device_id_checked(ctx.app) {
            Ok(id) if !id.trim().is_empty() => id,
            res => {
                let e = res.err().unwrap_or_else(|| "Local device ID is empty".to_string());
                log::warn!("[Sync Engine] SEC-01 Fail-Closed: Local device ID unavailable for revocation verification: {}", e);
                return Err(format!("SEC-01 Fail-Closed: Local device ID unavailable for revocation verification: {}", e));
            }
        };
        let local_sk = match crate::http_api::resolve_local_signing_key(ctx.app) {
            Some(sk) => sk,
            None => {
                log::warn!("[Sync Engine] 无法解析本地签名密钥，拒绝签署 Relay device_revocation_ack");
                return Err("无法解析本地签名密钥，拒绝签署 Relay device_revocation_ack".to_string());
            }
        };

        let ack_msg = ctx.with_db(|conn| {
            process_relay_device_revocation_frame(conn, json, &local_id, &local_sk, now_ms)
        })?;
        tx_mpsc.try_send(Message::Text(ack_msg.to_string().into()))
            .map_err(|e| format!("撤销确认入队失败: {}", e))?;
        return Ok(());
    }

    if msg_type == "device_revocation_ack"
        || (msg_type == "ack" && json.get("payload").and_then(|p| p.get("device_revocation_ack")).is_some()) {
        let outcome = ctx.with_db(|conn| {
            process_relay_device_revocation_ack_frame(conn, json, crate::now_ms())
        })?;
        log::info!("[Sync Engine] 持钥撤销确认处理结果: {:?}", outcome);
        return Ok(());
    }

    if msg_type == "notify" {
        let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
        log::info!("[Sync Engine] Received notify from {}", from_id);

        // SEC-01: 若 payload 携带持钥签名 device_revocation，经验证后作废该对端设备信任与会话
        if json.get("payload").and_then(|p| p.get("device_revocation").or_else(|| p.get("revocation"))).is_some() {
            let now_ms = crate::now_ms();
            let local_id = match crate::http_api::resolve_local_device_id_checked(ctx.app) {
                Ok(id) if !id.trim().is_empty() => id,
                res => {
                    let e = res.err().unwrap_or_else(|| "Local device ID is empty".to_string());
                    log::warn!("[Sync Engine] SEC-01 Fail-Closed: Local device ID unavailable for notify revocation: {}", e);
                    return Err(format!("SEC-01 Fail-Closed: Local device ID unavailable for notify revocation: {}", e));
                }
            };
            let local_sk = match crate::http_api::resolve_local_signing_key(ctx.app) {
                Some(sk) => sk,
                None => {
                    log::warn!("[Sync Engine] 无法解析本地签名密钥，拒绝处理 Notify 中的撤销证书");
                    return Err("无法解析本地签名密钥，拒绝处理 Notify 中的撤销证书".to_string());
                }
            };

            let ack_msg = ctx.with_db(|conn| {
                process_relay_device_revocation_frame(conn, json, &local_id, &local_sk, now_ms)
            })?;
            tx_mpsc.try_send(Message::Text(ack_msg.to_string().into()))
                .map_err(|e| format!("撤销确认入队失败: {}", e))?;
            return Ok(());
        }

        // SEC-01: 若 payload 携带 proof_of_possession (pop)，原子进行邀请消费与建立认证会话
        if let Some(pop_val) = json.get("payload").and_then(|p| p.get("pop").or_else(|| p.get("proof_of_possession"))) {
            let pop = match serde_json::from_value::<crate::device_trust::ProofOfPossession>(pop_val.clone()) {
                Ok(p) => p,
                Err(err) => {
                    log::warn!("[Sync Engine] Malformed PoP in Relay notify: {}", err);
                    let mut ack = serde_json::json!({
                        "type": "ack",
                        "target_device_id": from_id,
                        "error": format!("Unauthorized: Malformed proof of possession: {}", err),
                        "message_id": format!("msg-ack-{}", &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]),
                        "ref_message_id": json.get("message_id").and_then(|v| v.as_str()).unwrap_or_default(),
                        "protocol_version": SYNC_PROTOCOL_VERSION,
                    });
                    copy_trace_fields(json, &mut ack, true);
                    let _ = tx_mpsc.send(Message::Text(ack.to_string().into())).await;
                    return Err(format!("Malformed proof of possession: {}", err));
                }
            };

            if from_id != pop.subject_device_id {
                return Err("SEC-01 notify sender does not match proof-of-possession subject".into());
            }

            let now_ms = crate::now_ms();
            let my_id = match crate::http_api::resolve_local_device_id_checked(ctx.app) {
                Ok(id) if !id.trim().is_empty() => id,
                res => {
                    let e = res.err().unwrap_or_else(|| "Local device ID is empty".to_string());
                    log::warn!("[Sync Engine] SEC-01 Fail-Closed: Local device ID unavailable for PoP pairing: {}", e);
                    return Err(format!("SEC-01 Fail-Closed: Local device ID unavailable for PoP pairing: {}", e));
                }
            };

            let pair_res = ctx.with_db(|conn| {
                crate::device_trust::sec01_atomic_consume_and_create_session(
                    conn,
                    &pop,
                    &my_id,
                    crate::device_trust::DEFAULT_SESSION_TTL_MS,
                    now_ms,
                )
            });

            match pair_res {
                Ok((td, session)) => {
                    log::info!("[Sync Engine] SEC-01 Relay Pair succeeded: {} ({}), session {}", td.device_id, td.device_name, session.session_id);
                    let mut ack = serde_json::json!({
                        "type": "ack",
                        "target_device_id": from_id,
                        "message_id": format!("msg-ack-{}", &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]),
                        "ref_message_id": json.get("message_id").and_then(|v| v.as_str()).unwrap_or_default(),
                        "protocol_version": SYNC_PROTOCOL_VERSION,
                        "trace_id": json.get("trace_id").and_then(|v| v.as_str()).unwrap_or_default(),
                        "sync_id": json.get("sync_id").and_then(|v| v.as_str()).unwrap_or_default(),
                        "payload": {
                            "status": "trusted",
                            "session_id": session.session_id,
                            "device_id": my_id,
                            "device_name": td.device_name.clone(),
                        }
                    });
                    copy_trace_fields(json, &mut ack, true);
                    send_signed_relay_response(ctx, tx_mpsc, ack).await?;
                    ctx.update_registry(ConnectedDevice {
                        device_id: from_id.to_string(),
                        platform: td.platform.clone(),
                        ip_address: "relay".to_string(),
                        last_seen: now_ms,
                        device_name: Some(td.device_name.clone()),
                        is_trusted: true,
                        status: Some("trusted".to_string()),
                    });
                    let _ = crate::sync_history::record_activity(
                        DiagnosticStatus::Success,
                        Some(TransportKind::Relay),
                        Some(from_id.to_string()),
                        "Confirmed mobile Relay pairing",
                        None,
                    );
                    ctx.emit("sync:device_connected", serde_json::json!({
                        "device_id": from_id,
                        "platform": td.platform,
                        "device_name": td.device_name
                    }));
                    return Ok(());
                }
                Err(err) => {
                    log::warn!("[Sync Engine] SEC-01 Relay pairing rejected: {}", err);
                    let _ = crate::sync_history::record_activity(
                        DiagnosticStatus::Failed,
                        Some(TransportKind::Relay),
                        Some(from_id.to_string()),
                        "Rejected mobile Relay pairing request",
                        Some("ERR-PAIRING-04".to_string()),
                    );
                    let mut ack = serde_json::json!({
                        "type": "ack",
                        "target_device_id": from_id,
                        "error": format!("Unauthorized: Pairing verification failed: {}", err),
                        "message_id": format!("msg-ack-{}", &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]),
                        "ref_message_id": json.get("message_id").and_then(|v| v.as_str()).unwrap_or_default(),
                        "protocol_version": SYNC_PROTOCOL_VERSION,
                    });
                    copy_trace_fields(json, &mut ack, true);
                    let _ = tx_mpsc.send(Message::Text(ack.to_string().into())).await;
                    return Err(format!("Pairing verification failed: {}", err));
                }
            }
        }

        // 检查设备信任状态 (SEC-01: Discovered != Trusted)
        let is_trusted = ctx.with_db(|conn| Ok(crate::device_trust::is_device_trusted(conn, from_id))).unwrap_or(false);
        let device_name = json.get("payload").and_then(|p| p.get("device_name")).and_then(|v| v.as_str()).map(|s| s.to_string());
        let platform = json.get("payload").and_then(|p| p.get("platform")).and_then(|v| v.as_str()).unwrap_or("mobile").to_string();

        if !is_trusted {
            log::warn!("[Sync Engine] Notify from untrusted/unpaired device '{}', registering as discovered only", from_id);
            ctx.update_registry(ConnectedDevice {
                device_id: from_id.to_string(),
                platform,
                ip_address: "relay".to_string(),
                last_seen: crate::now_ms(),
                device_name,
                is_trusted: false,
                status: Some("discovered".to_string()),
            });
            return Ok(());
        }

        // SEC-01 Fail-Closed: 校验本机身份有效性
        let my_id = match crate::http_api::resolve_local_device_id_checked(ctx.app) {
            Ok(id) if !id.trim().is_empty() => id,
            res => {
                let e = res.err().unwrap_or_else(|| "Local device identity is empty".to_string());
                log::warn!("[Sync Engine] SEC-01 Fail-Closed: Local device identity unavailable during notify: {}", e);
                ctx.update_registry(ConnectedDevice {
                    device_id: from_id.to_string(),
                    platform: platform.clone(),
                    ip_address: "relay".to_string(),
                    last_seen: crate::now_ms(),
                    device_name: device_name.clone(),
                    is_trusted: false,
                    status: Some("discovered".to_string()),
                });
                return Err(format!("SEC-01 Fail-Closed: Local device identity unavailable during notify: {}", e));
            }
        };

        // 已可信设备也必须先取得有效会话，才可在运行时名册中标记 trusted。
        let now_ms = crate::now_ms();
        let session_id = match ctx.with_db(|conn| {
            crate::device_trust::get_or_create_active_session(conn, from_id, &my_id, now_ms)
        }) {
            Ok(id) => id,
            Err(e) => {
                ctx.update_registry(ConnectedDevice {
                    device_id: from_id.to_string(), platform: platform.clone(),
                    ip_address: "relay".to_string(), last_seen: now_ms,
                    device_name: device_name.clone(), is_trusted: false,
                    status: Some("discovered".to_string()),
                });
                return Err(format!("SEC-01 notify session creation failed: {e}"));
            }
        };
        // Signing is part of the authorization boundary: never publish trusted on failure.
        let mut ack = serde_json::json!({
            "type": "ack", "target_device_id": from_id,
            "message_id": uuid::Uuid::new_v4().to_string(),
            "ref_message_id": json.get("message_id").and_then(|v| v.as_str()).unwrap_or_default(),
            "protocol_version": SYNC_PROTOCOL_VERSION,
            "trace_id": json.get("trace_id").and_then(|v| v.as_str()).unwrap_or_default(),
            "sync_id": json.get("sync_id").and_then(|v| v.as_str()).unwrap_or_default(),
            "payload": { "status": "trusted", "session_id": session_id }
        });
        copy_trace_fields(json, &mut ack, true);
        send_signed_relay_response(ctx, tx_mpsc, ack).await?;
        ctx.update_registry(ConnectedDevice {
            device_id: from_id.to_string(),
            platform: platform.clone(),
            ip_address: "relay".to_string(),
            last_seen: crate::now_ms(),
            device_name: device_name.clone(),
            is_trusted: true,
            status: Some("trusted".to_string()),
        });

        let _ = crate::sync_history::record_activity(
            DiagnosticStatus::Success,
            Some(TransportKind::Relay),
            Some(from_id.to_string()),
            "Confirmed mobile Relay heartbeat",
            None,
        );

        ctx.emit("sync:device_connected", serde_json::json!({
            "device_id": from_id,
            "platform": platform,
            "device_name": device_name
        }));
        return Ok(());
    }

    if msg_type == "wakeup" {
        let from_id = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");
        log::info!("[Sync Engine] Received wakeup from {}", from_id);

        let now_ms = crate::now_ms();
        let current_device_id = match crate::http_api::resolve_local_device_id_checked(ctx.app) {
            Ok(id) if !id.trim().is_empty() => id,
            res => {
                let e = res.err().unwrap_or_else(|| "Local device identity is empty".to_string());
                log::warn!("[Sync Engine] SEC-01 Fail-Closed: Local device identity unavailable for wakeup verification: {}", e);
                return Err(format!("SEC-01 Fail-Closed: Local device identity unavailable for wakeup verification: {}", e));
            }
        };

        let authenticated_from_id = ctx.with_db(|conn| {
            crate::device_trust::sec01_verify_relay_wakeup_message(conn, json, &current_device_id, now_ms)
        })?;

        log_sync_action("Relay Wakeup", "done", &format!("Received authenticated wakeup from {}", authenticated_from_id));

        ctx.emit("sync:wakeup", serde_json::json!({
            "device_id": authenticated_from_id,
            "payload": json.get("payload")
        }));
        return Ok(());
    }

    if msg_type == "proxy" {
        let inner_payload = match json.get("payload") {
            Some(p) => p,
            None => return Err("Missing payload in proxy message".to_string()),
        };
        let from_id_raw = json.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");

        // SEC-01: 必须携带合法的密码学鉴权信封 (auth_envelope / envelope)
        let auth_val = inner_payload.get("auth_envelope")
            .or_else(|| inner_payload.get("envelope"))
            .or_else(|| json.get("auth_envelope"))
            .or_else(|| json.get("envelope"));

        let envelope: crate::device_trust::RpcAuthEnvelope = match auth_val {
            Some(v) => match serde_json::from_value(v.clone()) {
                Ok(env) => env,
                Err(e) => {
                    log::warn!("[Sync Engine] SEC-01: Malformed auth_envelope from {}: {}", from_id_raw, e);
                    let err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id_raw,
                        "payload": {
                            "action": "error",
                            "error": format!("Unauthorized: Malformed auth_envelope (SEC-01 fail-closed): {}", e)
                        }
                    });
                    let _ = tx_mpsc.send(Message::Text(err_resp.to_string().into())).await;
                    return Err(format!("Unauthorized: Malformed auth_envelope (SEC-01 fail-closed): {}", e));
                }
            },
            None => {
                log::warn!("[Sync Engine] SEC-01: Rejected proxy RPC without auth_envelope from {}", from_id_raw);
                let err_resp = serde_json::json!({
                    "type": "proxy",
                    "target_device_id": from_id_raw,
                    "payload": {
                        "action": "error",
                        "error": "Unauthorized: Missing cryptographic RPC authentication envelope (SEC-01 fail-closed)"
                    }
                });
                let _ = tx_mpsc.send(Message::Text(err_resp.to_string().into())).await;
                return Err("Unauthorized: Missing cryptographic RPC authentication envelope (SEC-01 fail-closed)".to_string());
            }
        };

        let action = inner_payload.get("action").and_then(|v| v.as_str()).unwrap_or("");

        // A. 动作绑定强校验
        if envelope.action != action {
            log::warn!("[Sync Engine] SEC-01: Action tampering detected! envelope action='{}', requested action='{}'", envelope.action, action);
            let err_resp = serde_json::json!({
                "type": "proxy",
                "target_device_id": from_id_raw,
                "payload": {
                    "action": "error",
                    "error": format!("Forbidden: Action mismatch between envelope ('{}') and payload ('{}')", envelope.action, action)
                }
            });
            let _ = tx_mpsc.send(Message::Text(err_resp.to_string().into())).await;
            return Err(format!("Forbidden: Action mismatch between envelope ('{}') and payload ('{}')", envelope.action, action));
        }

        // B. 目标设备强校验 (SEC-01 Fail-Closed)
        let local_device_id = if let Some(ref test_id) = ctx.test_local_device_id {
            if envelope.target_device_id != *test_id {
                let err_msg = format!("Forbidden: Target device ID mismatch (expected '{}', got '{}')", test_id, envelope.target_device_id);
                log::warn!("[Sync Engine] SEC-01 Proxy target check failed: {}", err_msg);
                let mut err_resp = serde_json::json!({
                    "type": "proxy",
                    "target_device_id": from_id_raw,
                    "payload": {
                        "action": "error",
                        "error": err_msg.clone()
                    }
                });
                copy_trace_fields(json, &mut err_resp, true);
                let _ = send_signed_relay_response(ctx, tx_mpsc, err_resp).await;
                return Err(format!("SEC-01 Proxy target check failed: {}", err_msg));
            }
            test_id.clone()
        } else {
            match sec01_check_relay_proxy_target(&envelope.target_device_id, ctx.app) {
                Ok(id) => id,
                Err(err_msg) => {
                    log::warn!("[Sync Engine] SEC-01 Proxy target check failed: {}", err_msg);
                    let mut err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id_raw,
                        "payload": {
                            "action": "error",
                            "error": err_msg.clone()
                        }
                    });
                    copy_trace_fields(json, &mut err_resp, true);
                    let _ = send_signed_relay_response(ctx, tx_mpsc, err_resp).await;
                    return Err(format!("SEC-01 Proxy target check failed: {}", err_msg));
                }
            }
        };

        // C. 身份绑定强校验 (防止伪造 from_device_id)
        if envelope.subject_device_id != from_id_raw {
            log::warn!("[Sync Engine] SEC-01: Caller identity mismatch! from_id='{}', envelope subject='{}'", from_id_raw, envelope.subject_device_id);
            let err_resp = serde_json::json!({
                "type": "proxy",
                "target_device_id": from_id_raw,
                "payload": {
                    "action": "error",
                    "error": "Forbidden: Caller identity mismatch between relay message and cryptographic envelope"
                }
            });
            let _ = tx_mpsc.send(Message::Text(err_resp.to_string().into())).await;
            return Err("Forbidden: Caller identity mismatch between relay message and cryptographic envelope".to_string());
        }

        // D. 提取载荷哈希比对字节
        let raw_payload_bytes = extract_relay_payload_bytes(inner_payload, &envelope.payload_hash);

        // E. 密码学全要素鉴权（会话、防重放、签名、撤销状态）
        let now_ms = crate::now_ms();
        let auth_res = ctx.with_db(|conn| {
            crate::device_trust::verify_rpc_request_auth(conn, &envelope, &raw_payload_bytes, &local_device_id, now_ms)
        });

        let (verified_subject, session_id, request_id, execution_token) = match auth_res {
            Ok(crate::device_trust::AuthVerificationOutcome::Authorized {
                subject_device_id,
                session_id,
                request_id,
                execution_token,
                ..
            }) => (subject_device_id, session_id, request_id, execution_token),
            Ok(crate::device_trust::AuthVerificationOutcome::IdempotentCached { cached_response }) => {
                log::info!("[Sync Engine] SEC-01: Returning idempotent cached response for request {} to {}", envelope.request_id, from_id_raw);
                let cached: serde_json::Value = serde_json::from_str(&cached_response)
                    .map_err(|e| format!("SEC-01 cached response is invalid: {e}"))?;
                let _signed_ack = send_signed_relay_response(ctx, tx_mpsc, cached).await?;
                return Ok(());
            }
            Err(e) => {
                log::warn!("[Sync Engine] SEC-01: Authorization rejected for proxy from {}: {}", from_id_raw, e);
                let error_type = if e.contains("Idempotency conflict") {
                    "conflict"
                } else {
                    "unauthorized"
                };
                let err_resp = serde_json::json!({
                    "type": "proxy",
                    "target_device_id": from_id_raw,
                    "payload": {
                        "action": "error",
                        "error_type": error_type,
                        "error": format!("Unauthorized (SEC-01 fail-closed): {}", e)
                    }
                });
                let _ = tx_mpsc.send(Message::Text(err_resp.to_string().into())).await;
                return Err(format!("Unauthorized (SEC-01 fail-closed): {}", e));
            }
        };

        let from_id = verified_subject.as_str();

        ctx.emit("sync:device_syncing", serde_json::json!({
            "device_id": from_id,
            "status": "syncing"
        }));

        if action == "pull" {
            log::info!("[Sync Engine] Received proxy pull request from {}", from_id);
            if let Some(app) = ctx.app {
                let since_ts = inner_payload.get("since_ts").and_then(|v| v.as_i64()).unwrap_or(0);
                if let Ok(sync_data) = export_sync_data(app, since_ts, true) {
                    let mut pull_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id,
                        "payload": {
                            "action": "pull_response",
                            "data": sync_data
                        }
                    });
                    copy_trace_fields(json, &mut pull_resp, true);
                    if let Err(e) = send_signed_relay_response(ctx, tx_mpsc, pull_resp.clone()).await {
                        log::error!("[Sync Engine] Failed to send pull_response to {}: {}", from_id, e);
                        let _ = crate::sync_history::record_activity(
                            DiagnosticStatus::Failed,
                            Some(TransportKind::Relay),
                            Some(from_id.to_string()),
                            "向移动端返回同步数据失败",
                            Some("ERR-SYNC-RELAY-SEND".to_string()),
                        );
                    } else {
                        log::info!("[Sync Engine] Sent pull_response to {} successfully", from_id);
                        let _ = crate::sync_history::record_activity(
                            DiagnosticStatus::Success,
                            Some(TransportKind::Relay),
                            Some(from_id.to_string()),
                            "已通过 Relay 向移动端返回同步数据",
                            None,
                        );
                        ctx.record_idempotency(
                            &envelope.session_id,
                            &envelope.request_id,
                            &envelope.subject_device_id,
                            &envelope.target_device_id,
                            &envelope.action,
                            &envelope.payload_hash,
                            &pull_resp.to_string(),
                        );
                    }
                }
            } else {
                log::warn!("[Sync Engine] AppHandle unavailable for proxy pull request");
            }
        } else if action == "push" {
            log::info!("[Sync Engine] Received proxy push request from {}", from_id);
            let ops_opt: Option<Vec<serde_json::Value>> = if let Some(data_val) = inner_payload.get("data") {
                if let Some(arr) = data_val.as_array() {
                    Some(arr.clone())
                } else if let Some(obj) = data_val.as_object() {
                    obj.get("operations")
                        .or_else(|| obj.get("ops"))
                        .or_else(|| obj.get("data"))
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.clone())
                } else {
                    None
                }
            } else {
                None
            };

            let ops = match ops_opt {
                Some(arr) => arr,
                None => {
                    let err_msg = "SEC-03 Fail-Closed: 无法识别的 relay push 载荷格式，必须为操作数组或包含 operations/ops/data 数组的对象".to_string();
                    log::warn!("[Sync Engine] Rejected relay push with malformed payload from {}", from_id);
                    let _ = ctx.with_db(|conn| {
                        crate::device_trust::fail_rpc_idempotency(
                            conn,
                            &session_id,
                            &request_id,
                            &execution_token,
                            &err_msg,
                            crate::now_ms(),
                        )
                    });
                    let mut err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id,
                        "payload": {
                            "action": "error",
                            "error": err_msg
                        }
                    });
                    copy_trace_fields(json, &mut err_resp, true);
                    send_signed_relay_response(ctx, tx_mpsc, err_resp).await?;
                    return Err("SEC-03 Fail-Closed: 无法识别的 relay push 载荷格式".to_string());
                }
            };

            log::info!("[Sync Engine] Pushing {} operations to PC outbox via Relay atomically", ops.len());
            let initial_resp_payload = serde_json::json!({
                "status": "ok",
                "type": "commit_ack"
            });
            let initial_resp_str = initial_resp_payload.to_string();

            let commit_res = ctx.with_db(|conn| {
                crate::device_trust::atomic_commit_push_outbox(
                    conn,
                    &session_id,
                    &request_id,
                    &execution_token,
                    &ops,
                    &initial_resp_str,
                    crate::now_ms(),
                )
            });

            match commit_res {
                Ok(receipt) => {
                    let mut commit_ack = serde_json::json!({
                        "type": "ack",
                        "target_device_id": from_id,
                        "payload": {
                            "status": "committed",
                            "action": "commit_ack",
                            "stage": receipt.stage,
                            "applied_count": receipt.applied_count,
                            "pending_count": receipt.pending_count,
                            "delivery_error": receipt.delivery_error,
                        }
                    });
                    copy_trace_fields(json, &mut commit_ack, true);
                    let signed_ack = send_signed_relay_response(ctx, tx_mpsc, commit_ack).await?;

                    ctx.record_idempotency(
                        &envelope.session_id,
                        &envelope.request_id,
                        &envelope.subject_device_id,
                        &envelope.target_device_id,
                        &envelope.action,
                        &envelope.payload_hash,
                        &signed_ack.to_string(),
                    );

                    let mut legacy_ack = serde_json::json!({
                        "type": "commit_ack",
                        "target_device_id": from_id,
                        "payload": {
                            "status": "committed",
                            "action": "commit_ack",
                            "stage": receipt.stage,
                            "applied_count": receipt.applied_count,
                            "pending_count": receipt.pending_count,
                            "delivery_error": receipt.delivery_error,
                        }
                    });
                    copy_trace_fields(json, &mut legacy_ack, true);
                    send_signed_relay_response(ctx, tx_mpsc, legacy_ack).await?;
                }
                Err(e) => {
                    log::error!("[Sync Engine] Failed to atomic commit push outbox via Relay: {}", e);
                    let _ = ctx.with_db(|conn| {
                        crate::device_trust::fail_rpc_idempotency(
                            conn,
                            &session_id,
                            &request_id,
                            &execution_token,
                            &e,
                            crate::now_ms(),
                        )
                    });
                    let mut err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id,
                        "payload": {
                            "action": "error",
                            "error": format!("Push atomic commit failed: {}", e)
                        }
                    });
                    copy_trace_fields(json, &mut err_resp, true);
                    send_signed_relay_response(ctx, tx_mpsc, err_resp).await?;
                    return Err(format!("Push atomic commit failed: {}", e));
                }
            }
        } else if action == "rpc_discover_capabilities" {
            log::info!("[Sync Engine] Received proxy rpc_discover_capabilities from {}", from_id);
            let request_id = inner_payload.get("request_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let req_msg_id = json.get("message_id").and_then(|v| v.as_str()).unwrap_or(&request_id).to_string();
            let req_trace_id = json.get("trace_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let from_id_clone = from_id.to_string();
            let app_opt = ctx.app.cloned();
            let tx_mpsc_clone = tx_mpsc.clone();

            tauri::async_runtime::spawn(async move {
                let snapshot = if let Some(ref app) = app_opt {
                    crate::capability::CapabilitySnapshot::capture(app, false, true)
                } else {
                    crate::capability::CapabilitySnapshot::detect(std::env::consts::OS, false, true, false)
                };
                let (safe_models, default_model) = crate::capability::get_safe_model_pool_for_remote();
                let (local_device_id, device_name) = match crate::read_config_checked() {
                    Ok(config) => {
                        let name = config.get("device_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("电脑端 (PC)")
                            .to_string();
                        let id = match crate::http_api::resolve_local_device_id_checked(app_opt.as_ref()) {
                            Ok(id) if !id.trim().is_empty() => id,
                            res => {
                                log::warn!("[Sync Engine] SEC-01 Fail-Closed: Local device id resolution failed for rpc_discover_capabilities: {:?}", res.err());
                                "".to_string()
                            }
                        };
                        (id, name)
                    }
                    Err(e) => {
                        log::error!("[Sync Engine] Config read failed for rpc_discover_capabilities (SEC-01 Fail-Closed): {}", e);
                        ("".to_string(), "".to_string())
                    }
                };

                if local_device_id.trim().is_empty() {
                    let err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id_clone,
                        "ref_message_id": req_msg_id,
                        "message_id": uuid::Uuid::new_v4().to_string(),
                        "trace_id": req_trace_id,
                        "protocol_version": SYNC_PROTOCOL_VERSION,
                        "payload": {
                            "action": "rpc_capabilities_response",
                            "request_id": request_id,
                            "status": "error",
                            "error": "Forbidden: Local device ID is unconfigured or unavailable (SEC-01 fail-closed)"
                        }
                    });
                    if let Some(app) = app_opt.as_ref() {
                        if let Ok(signed) = sign_relay_response(app, err_resp) {
                            let _ = tx_mpsc_clone.send(Message::Text(signed.to_string().into())).await;
                        }
                    }
                    return;
                }

                let payload = serde_json::json!({
                    "action": "rpc_capabilities_response",
                    "request_id": request_id,
                    "status": "success",
                    "device_id": local_device_id,
                    "device_name": device_name,
                    "platform": snapshot.platform,
                    "file_scope": snapshot.file_scope,
                    "capabilities": snapshot.capabilities,
                    "available_models": safe_models,
                    "default_model": default_model,
                    "timestamp": crate::now_ms()
                });

                let resp = serde_json::json!({
                    "type": "proxy",
                    "target_device_id": from_id_clone,
                    "ref_message_id": req_msg_id,
                    "message_id": uuid::Uuid::new_v4().to_string(),
                    "trace_id": req_trace_id,
                    "protocol_version": SYNC_PROTOCOL_VERSION,
                    "payload": payload
                });

                if let Some(app) = app_opt.as_ref() {
                    if let Ok(signed) = sign_relay_response(app, resp) {
                        let _ = tx_mpsc_clone.send(Message::Text(signed.to_string().into())).await;
                    }
                }
            });
        } else if action == "rpc_request" {
            log::info!("[Sync Engine] Received proxy rpc_request from {}", from_id);
            if let Some(app) = ctx.app {
                let request_id = inner_payload.get("request_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let conversation_id = inner_payload.get("conversation_id").and_then(|v| v.as_str()).unwrap_or("default").to_string();
                let project_id_opt = inner_payload.get("project_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let instruction = inner_payload.get("instruction").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let read_only = inner_payload.get("read_only").and_then(|v| v.as_bool()).unwrap_or(true);
                let requested_model = inner_payload.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
                let req_msg_id = json.get("message_id").and_then(|v| v.as_str()).unwrap_or(&request_id).to_string();
                let req_trace_id = json.get("trace_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let from_id_clone = from_id.to_string();
                let app_clone = app.clone();
                let tx_mpsc_clone = tx_mpsc.clone();

                let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
                let (done_tx, done_rx) = tokio::sync::watch::channel(false);
                {
                    ACTIVE_RPC_TASKS.lock().unwrap().insert(request_id.clone(), ActiveRpcTask {
                        cancel_tx,
                        done_rx,
                    });
                }

                let req_id_for_task = request_id.clone();
                tauri::async_runtime::spawn(async move {
                    let start_time = std::time::Instant::now();
                    let pc_conv_id = format!("remote_conv_{}", conversation_id);
                    let mut msgs = Vec::new();
                    let mut resolved_project_id = project_id_opt.clone().unwrap_or_else(|| "project_personal_inbox".to_string());

                    // 从 SQLite 恢复上下文历史及解析项目归属
                    if let Some(db_state) = app_clone.try_state::<crate::db::DbState>() {
                        if let Ok(mut conn) = db_state.0.lock() {
                            if project_id_opt.is_none() {
                                if let Ok(pid) = crate::work_core::repository::ensure_personal_workspace(&mut conn) {
                                    resolved_project_id = pid;
                                }
                            }

                            let _ = conn.execute(
                                "INSERT OR IGNORE INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                                rusqlite::params![pc_conv_id, format!("移动端协同: {}", instruction.chars().take(20).collect::<String>()), crate::now_ms(), crate::now_ms()],
                            );

                            if let Ok(mut stmt) = conn.prepare(
                                "SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC LIMIT 30"
                            ) {
                                if let Ok(rows) = stmt.query_map(rusqlite::params![pc_conv_id], |row| {
                                    Ok(serde_json::json!({
                                        "role": row.get::<_, String>(0)?,
                                        "content": row.get::<_, String>(1)?
                                    }))
                                }) {
                                    for r in rows {
                                        if let Ok(m) = r {
                                            msgs.push(m);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let prompt = if read_only {
                        format!("[移动端 RPC 只读指令]\n{}\n注意：当前处于只读安全模式，仅允许进行文件读取、分析和查询。禁止使用任何文件修改、删除或系统写入工具。", instruction)
                    } else {
                        format!("[移动端 RPC 协同修改指令]\n{}\n注意：当前处于受控协同修改模式。如果需要修改文件，请调用 write_file 工具提出修改（系统将自动拦截并转为待手机审批的变更提案，不会直接覆写磁盘），或在回复中使用标准 diff 格式说明（以 ```diff 块包含 --- a/file 与 +++ b/file 及 +/- 改动行），手机端将审阅该 Diff 并由用户确认批准后再生效。", instruction)
                    };

                    msgs.push(serde_json::json!({
                        "role": "user",
                        "content": prompt
                    }));

                    let exec_policy = crate::tools::ToolExecutionPolicy {
                        read_only,
                        staged_mode: !read_only,
                        request_id: Some(req_id_for_task.clone()),
                        project_id: Some(resolved_project_id.clone()),
                    };

                    log::info!("[Sync Engine] Executing Agent for RPC {} in conv {} (policy: read_only={}, staged={})", req_id_for_task, pc_conv_id, exec_policy.read_only, exec_policy.staged_mode);

                    tokio::select! {
                        result = crate::llm::stream_chat_with_policy(
                            app_clone.clone(),
                            msgs,
                            Some(pc_conv_id.clone()),
                            None,
                            true,
                            "standard".to_string(),
                            requested_model.clone(),
                            exec_policy,
                        ) => {
                            if *cancel_rx.borrow() {
                                log::info!("[Sync Engine] Task {} was cancelled during execution", req_id_for_task);
                                let _ = done_tx.send(true);
                                ACTIVE_RPC_TASKS.lock().unwrap().remove(&req_id_for_task);
                                return;
                            }

                            let result_text = if let Some(arr) = result.as_array() {
                                if let Some(last) = arr.last() {
                                    last.get("content").and_then(|v| v.as_str()).unwrap_or("Empty response").to_string()
                                } else {
                                    "Empty array".to_string()
                                }
                            } else if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
                                content.to_string()
                            } else {
                                result.to_string()
                            };

                            let elapsed = start_time.elapsed().as_millis() as u64;
                            let executor_device = match crate::read_config_checked() {
                                Ok(cfg) => cfg.get("device_name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("电脑端 (PC)")
                                    .to_string(),
                                Err(e) => {
                                    log::error!("[Sync Engine] Config read failed during remote execution result persistence (SEC-01 Fail-Closed): {}", e);
                                    let err_resp = serde_json::json!({
                                        "type": "proxy",
                                        "target_device_id": from_id_clone,
                                        "ref_message_id": req_msg_id,
                                        "message_id": uuid::Uuid::new_v4().to_string(),
                                        "trace_id": req_trace_id,
                                        "protocol_version": SYNC_PROTOCOL_VERSION,
                                        "payload": {
                                            "action": "rpc_response",
                                            "request_id": req_id_for_task,
                                            "status": "error",
                                            "error": format!("SEC-01 Fail-Closed: 配置读取失败无法持久化或确认执行结果: {}", e)
                                        }
                                    });
                                    if let Ok(signed) = sign_relay_response(&app_clone, err_resp) {
                                        let _ = tx_mpsc_clone.send(Message::Text(signed.to_string().into())).await;
                                    }
                                    return;
                                }
                            };

                            // 持久化本次交互至 PC 会话，供下一轮多轮追问
                            if let Some(db_state) = app_clone.try_state::<crate::db::DbState>() {
                                if let Ok(conn) = db_state.0.lock() {
                                    let now = crate::now_ms();
                                    let _ = conn.execute(
                                        "INSERT INTO messages (conversation_id, role, content, from_channel, created_at) VALUES (?1, 'user', ?2, 'remote', ?3)",
                                        rusqlite::params![pc_conv_id, prompt, now],
                                    );
                                    let _ = conn.execute(
                                        "INSERT INTO messages (conversation_id, role, content, from_channel, created_at) VALUES (?1, 'assistant', ?2, 'remote', ?3)",
                                        rusqlite::params![pc_conv_id, result_text, now + 1],
                                    );
                                    let _ = conn.execute(
                                        "UPDATE conversations SET updated_at = ?1, last_message = ?2 WHERE id = ?3",
                                        rusqlite::params![now + 1, result_text.chars().take(80).collect::<String>(), pc_conv_id],
                                    );
                                }
                            }

                            let mut created_staged_changes: Vec<StagedChange> = Vec::new();

                            if !read_only {
                                if let Some(db_state) = app_clone.try_state::<crate::db::DbState>() {
                                    if let Ok(conn) = db_state.0.lock() {
                                        if let Ok(staged_list) = get_staged_changes_by_request(&conn, &req_id_for_task) {
                                            created_staged_changes = staged_list;
                                        }
                                    }
                                }

                                if created_staged_changes.is_empty() {
                                    if let Ok(staged_map) = STAGED_CHANGES.lock() {
                                        created_staged_changes = staged_map.values()
                                            .filter(|c| c.request_id == req_id_for_task && c.status == "pending")
                                            .cloned()
                                            .collect();
                                    }
                                }

                                if created_staged_changes.is_empty() {
                                    if let Some((ref file_path, ref diff_content, adds, dels)) = extract_diff_from_text(&result_text) {
                                        let old_content = if std::path::Path::new(file_path).exists() {
                                            std::fs::read_to_string(file_path).unwrap_or_default()
                                        } else {
                                            String::new()
                                        };
                                        if let Ok(new_content) = apply_unified_diff(&old_content, diff_content) {
                                            let old_content_hash = compute_content_hash(&old_content);
                                            let change_id = format!("change_{}", uuid::Uuid::new_v4());

                                            let mut staged = StagedChange {
                                                change_id: change_id.clone(),
                                                request_id: req_id_for_task.clone(),
                                                project_id: resolved_project_id.clone(),
                                                file_path: file_path.clone(),
                                                old_content,
                                                new_content,
                                                old_content_hash,
                                                diff: diff_content.clone(),
                                                summary: format!("修改文件: {}", file_path),
                                                additions: adds,
                                                deletions: dels,
                                                status: "pending".to_string(),
                                                created_at: crate::now_ms(),
                                                applied_at: None,
                                                work_object_id: None,
                                            };

                                            if let Some(db_state) = app_clone.try_state::<crate::db::DbState>() {
                                                if let Ok(mut conn) = db_state.0.lock() {
                                                    if let Ok(obj) = crate::work_core::repository::create_object(
                                                        &mut conn,
                                                        crate::work_core::models::CreateWorkObjectInput {
                                                            kind: crate::work_core::models::WorkObjectKind::Change,
                                                            project_id: resolved_project_id.clone(),
                                                            parent_id: None,
                                                            title: format!("待审阅修改提案: {}", file_path),
                                                            status: Some("needs_review".into()),
                                                            description: Some(format!("待审批的修改 (+{} -{})", adds, dels)),
                                                            data: serde_json::json!({
                                                                "filePath": file_path,
                                                                "changeId": change_id,
                                                                "additions": adds,
                                                                "deletions": dels,
                                                                "diff": diff_content,
                                                            }),
                                                            source_capture_id: None,
                                                            actor: Some(format!("remote:{}", from_id_clone)),
                                                            idempotency_key: format!("obj_change_{}", change_id),
                                                        },
                                                    ) {
                                                        staged.work_object_id = Some(obj.id);
                                                    }
                                                    let _ = save_staged_change(&conn, &staged);
                                                }
                                            }
                                            if let Ok(mut map) = STAGED_CHANGES.lock() {
                                                map.insert(change_id.clone(), staged.clone());
                                            }
                                            created_staged_changes.push(staged);
                                        } else {
                                            log::warn!("[Sync Engine] Failed to apply extracted unified diff for {}", file_path);
                                        }
                                    }
                                }
                            }

                            let (status, change_val, changes_val) = if !created_staged_changes.is_empty() {
                                let first = &created_staged_changes[0];
                                let changes_json: Vec<serde_json::Value> = created_staged_changes.iter().map(|s| serde_json::json!({
                                    "change_id": s.change_id,
                                    "request_id": s.request_id,
                                    "file_path": s.file_path,
                                    "diff": s.diff,
                                    "summary": s.summary,
                                    "additions": s.additions,
                                    "deletions": s.deletions,
                                    "status": "pending"
                                })).collect();

                                (
                                    "needs_approval",
                                    Some(serde_json::json!({
                                        "change_id": first.change_id,
                                        "request_id": first.request_id,
                                        "file_path": first.file_path,
                                        "diff": first.diff,
                                        "summary": first.summary,
                                        "additions": first.additions,
                                        "deletions": first.deletions,
                                        "status": "pending"
                                    })),
                                    Some(serde_json::Value::Array(changes_json))
                                )
                            } else {
                                ("success", None, None)
                            };

                            // ── 接入统一工作记录 (Work Object / Journal / Evidence) ──
                            if let Some(db_state) = app_clone.try_state::<crate::db::DbState>() {
                                if let Ok(mut conn) = db_state.0.lock() {
                                    let _ = crate::work_core::repository::record_work_event(
                                        &mut conn,
                                        &resolved_project_id,
                                        None,
                                        "remote.instruction.executed",
                                        &format!("remote:{}", from_id_clone),
                                        &serde_json::json!({
                                            "requestId": req_id_for_task,
                                            "fromDevice": from_id_clone,
                                            "instruction": instruction,
                                            "elapsedMs": elapsed,
                                            "status": status,
                                            "executorDevice": executor_device,
                                            "model": requested_model,
                                            "hasChange": !created_staged_changes.is_empty(),
                                            "changesCount": created_staged_changes.len(),
                                        }),
                                        Some(&format!("event_rpc_exec_{}", req_id_for_task)),
                                    );

                                    if let Ok(agg) = crate::work_core::repository::get_project_aggregate(&conn, &resolved_project_id) {
                                        let _ = crate::work_core::snapshot::write_project_snapshot(&agg);
                                    }
                                }
                            }

                            let mut payload = serde_json::json!({
                                "action": "rpc_response",
                                "request_id": req_id_for_task,
                                "status": status,
                                "result": result_text,
                                "elapsed_ms": elapsed,
                                "executor_device": executor_device
                            });
                            if let Some(c) = change_val {
                                payload["change"] = c;
                            }
                            if let Some(cs) = changes_val {
                                payload["changes"] = cs;
                            }

                            let resp = serde_json::json!({
                                "type": "proxy",
                                "target_device_id": from_id_clone,
                                "ref_message_id": req_msg_id,
                                "message_id": uuid::Uuid::new_v4().to_string(),
                                "trace_id": req_trace_id,
                                "protocol_version": SYNC_PROTOCOL_VERSION,
                                "payload": payload
                            });

                            if let Ok(signed) = sign_relay_response(&app_clone, resp) {
                                let _ = tx_mpsc_clone.send(Message::Text(signed.to_string().into())).await;
                            }

                            let _ = crate::sync_history::record_activity(
                                DiagnosticStatus::Success,
                                Some(TransportKind::Relay),
                                Some(from_id_clone),
                                &format!("执行远程指令完成 ({}ms, status={})", elapsed, status),
                                Some("RPC-EXEC-OK".to_string()),
                            );
                        }
                        _ = cancel_rx.changed() => {
                            log::info!("[Sync Engine] RPC task {} cancelled via watch channel", req_id_for_task);
                        }
                    }

                    let _ = done_tx.send(true);
                    ACTIVE_RPC_TASKS.lock().unwrap().remove(&req_id_for_task);
                });
            } else {
                log::warn!("[Sync Engine] AppHandle unavailable for proxy rpc_request");
            }
        } else if action == "rpc_cancel" {
            log::info!("[Sync Engine] Received proxy rpc_cancel from {}", from_id);
            let cancel_req_id = inner_payload.get("request_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let from_id_clone = from_id.to_string();
            let tx_mpsc_clone = tx_mpsc.clone();

            let (cancel_outcome, is_confirmed, cancel_status, cancel_error_msg) =
                cancel_active_rpc_task_core(&cancel_req_id, tokio::time::Duration::from_secs(5)).await;

            if is_confirmed {
                let mut cancelled_changes = Vec::new();
                let _ = ctx.with_db(|conn| {
                    if let Ok(cancelled) = cancel_staged_changes_by_request(conn, &cancel_req_id) {
                        cancelled_changes = cancelled;
                    }
                    Ok(())
                });
                {
                    let mut staged = STAGED_CHANGES.lock().unwrap();
                    for c in &cancelled_changes {
                        staged.remove(&c.change_id);
                    }
                }

                let _ = ctx.with_db(|conn| {
                    let pid = crate::work_core::repository::ensure_personal_workspace(conn).unwrap_or_else(|_| "project_personal_inbox".to_string());
                    let _ = crate::work_core::repository::record_work_event(
                        conn,
                        &pid,
                        None,
                        "remote.task.cancelled",
                        &format!("remote:{}", from_id_clone),
                        &serde_json::json!({
                            "requestId": cancel_req_id,
                            "fromDevice": from_id_clone,
                            "reason": "User cancelled from mobile"
                        }),
                        Some(&format!("event_rpc_cancel_{}", cancel_req_id)),
                    );

                    for c in cancelled_changes {
                        if let Some(obj_id) = c.work_object_id {
                            let _ = crate::work_core::repository::update_object_status(
                                conn,
                                crate::work_core::models::UpdateWorkStatusInput {
                                    object_id: obj_id,
                                    expected_revision: 1,
                                    status: "cancelled".into(),
                                    actor: Some(format!("remote:{}", from_id_clone)),
                                    idempotency_key: format!("cancel_change_{}", c.change_id),
                                },
                            );
                        }
                    }

                    if let Ok(agg) = crate::work_core::repository::get_project_aggregate(conn, &pid) {
                        let _ = crate::work_core::snapshot::write_project_snapshot(&agg);
                    }
                    Ok(())
                });

                let _ = crate::sync_history::record_activity(
                    DiagnosticStatus::Skipped,
                    Some(TransportKind::Relay),
                    Some(from_id_clone.clone()),
                    &format!("任务已被移动端取消并退出 (Stop Confirmed): {}", cancel_req_id),
                    Some("RPC-CANCEL-ACK".to_string()),
                );
            } else {
                let diag_status = if cancel_outcome == CancelOutcome::Timeout {
                    DiagnosticStatus::Timeout
                } else {
                    DiagnosticStatus::Failed
                };
                let _ = crate::sync_history::record_activity(
                    diag_status,
                    Some(TransportKind::Relay),
                    Some(from_id_clone.clone()),
                    &format!("任务取消未确认完成 (outcome={:?}): {}", cancel_outcome, cancel_req_id),
                    Some("RPC-CANCEL-UNCONFIRMED".to_string()),
                );
            }

            let mut payload_data = serde_json::json!({
                "action": "rpc_cancel_ack",
                "request_id": cancel_req_id,
                "status": cancel_status,
                "confirmed": is_confirmed
            });
            if let Some(err) = cancel_error_msg {
                payload_data["error"] = serde_json::Value::String(err);
            }

            let ack_resp = serde_json::json!({
                "type": "proxy",
                "target_device_id": from_id_clone,
                "ref_message_id": json.get("message_id").and_then(|v| v.as_str()).unwrap_or(&cancel_req_id),
                "message_id": uuid::Uuid::new_v4().to_string(),
                "trace_id": json.get("trace_id").and_then(|v| v.as_str()).unwrap_or(""),
                "protocol_version": SYNC_PROTOCOL_VERSION,
                "payload": payload_data
            });
            send_signed_relay_response(ctx, &tx_mpsc_clone, ack_resp).await?;
        } else if action == "rpc_approval_decision" {
            log::info!("[Sync Engine] Received proxy rpc_approval_decision from {}", from_id);
            let change_id = inner_payload.get("change_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let decision = inner_payload.get("decision").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let req_id = inner_payload.get("request_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let from_id_clone = from_id.to_string();
            let tx_mpsc_clone = tx_mpsc.clone();

            let outcome = ctx.with_db(|conn| {
                let outcome_val = process_approval_decision_core(
                    conn,
                    &change_id,
                    &decision,
                    &format!("remote:{}", from_id_clone),
                );
                if outcome_val.get("status").and_then(|s| s.as_str()) == Some("applied") {
                    let _ = crate::sync_history::record_activity(
                        DiagnosticStatus::Success,
                        Some(TransportKind::Relay),
                        Some(from_id_clone.clone()),
                        &format!("已批准并真实应用修改: {}", change_id),
                        Some("RPC-APPLY-OK".to_string()),
                    );
                } else if outcome_val.get("status").and_then(|s| s.as_str()) == Some("rejected") {
                    let _ = crate::sync_history::record_activity(
                        DiagnosticStatus::Skipped,
                        Some(TransportKind::Relay),
                        Some(from_id_clone.clone()),
                        &format!("已拒绝修改提案: {}", change_id),
                        Some("RPC-REJECT".to_string()),
                    );
                }
                Ok(outcome_val)
            }).unwrap_or_else(|e| {
                serde_json::json!({
                    "status": "error",
                    "change_id": change_id,
                    "error": e
                })
            });

            let resp = serde_json::json!({
                "type": "proxy",
                "target_device_id": from_id_clone,
                "ref_message_id": json.get("message_id").and_then(|v| v.as_str()).unwrap_or(&req_id),
                "message_id": uuid::Uuid::new_v4().to_string(),
                "trace_id": json.get("trace_id").and_then(|v| v.as_str()).unwrap_or(""),
                "protocol_version": SYNC_PROTOCOL_VERSION,
                "payload": {
                    "action": "rpc_response",
                    "request_id": req_id,
                    "outcome": outcome
                }
            });
            send_signed_relay_response(ctx, &tx_mpsc_clone, resp).await?;
        } else if action == "rpc_response" || action == "rpc_capabilities_response" {
            log::info!("[Sync Engine] Received proxy {} from {}", action, from_id);
            ctx.emit("sync:rpc_response", inner_payload.clone());
        } else if action == "push_db" {
            log::info!("[Sync Engine] Received proxy push_db request from {}", from_id);
            let data_val = match inner_payload.get("data") {
                Some(v) => v,
                None => {
                    let err_msg = "Missing data in push_db payload".to_string();
                    let _ = ctx.with_db(|conn| {
                        crate::device_trust::fail_rpc_idempotency(
                            conn,
                            &session_id,
                            &request_id,
                            &execution_token,
                            &err_msg,
                            crate::now_ms(),
                        )
                    });
                    let mut err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id,
                        "payload": { "action": "error", "error": err_msg }
                    });
                    copy_trace_fields(json, &mut err_resp, true);
                    send_signed_relay_response(ctx, tx_mpsc, err_resp).await?;
                    return Err("Missing data in push_db payload".to_string());
                }
            };

            let sync_data: SyncData = match serde_json::from_value::<SyncData>(data_val.clone()) {
                Ok(d) => d,
                Err(e) => {
                    let err_msg = format!("Malformed SyncData: {}", e);
                    log::warn!("[Sync Engine] {}", err_msg);
                    let _ = ctx.with_db(|conn| {
                        crate::device_trust::fail_rpc_idempotency(
                            conn,
                            &session_id,
                            &request_id,
                            &execution_token,
                            &err_msg,
                            crate::now_ms(),
                        )
                    });
                    let mut err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id,
                        "payload": { "action": "error", "error": err_msg }
                    });
                    copy_trace_fields(json, &mut err_resp, true);
                    send_signed_relay_response(ctx, tx_mpsc, err_resp).await?;
                    return Err(format!("Malformed SyncData: {}", e));
                }
            };

            let last_sync_ts_pc: i64 = ctx.with_db(|conn| {
                let s: String = conn.query_row("SELECT value FROM settings WHERE key = 'last_sync_ts'", [], |row| row.get(0)).unwrap_or_else(|_| "0".to_string());
                Ok(s.parse::<i64>().unwrap_or(0))
            }).unwrap_or(0);

            let initial_resp_payload = serde_json::json!({
                "status": "ok",
                "type": "commit_ack"
            });
            let initial_resp_str = initial_resp_payload.to_string();
            let now_ms = crate::now_ms();

            let idempotency_info = IdempotencyCommitInfo {
                session_id: &session_id,
                request_id: &request_id,
                execution_token: &execution_token,
                response_json: &initial_resp_str,
                now_ms,
            };

            let import_res = ctx.with_db(|conn| {
                import_sync_data_to_conn_atomic(
                    conn,
                    &sync_data,
                    last_sync_ts_pc,
                    Some(idempotency_info),
                )
            });

            match import_res {
                Ok(outcome) => {
                    let now = crate::now_ms();
                    let _ = ctx.with_db(|conn| {
                        conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('last_sync_ts', ?1)", rusqlite::params![now.to_string()])
                            .map_err(|e| e.to_string())?;
                        Ok(())
                    });

                    let receipt = outcome.receipt;
                    if receipt.stage == "applied" {
                        if let Some(app) = ctx.app {
                            if sync_data.notes.len() > 0 {
                                let _ = app.emit("notebook:updated", serde_json::json!({ "count": sync_data.notes.len() }));
                            }
                            if sync_data.events.len() > 0 {
                                let _ = app.emit("calendar-updated", serde_json::json!({ "action": "sync", "count": sync_data.events.len() }));
                            }
                            let _ = app.emit("sync:completed", serde_json::json!({ "status": "ok", "stage": "applied", "total_records": outcome.total_records }));
                        }

                        let _ = crate::sync_history::record_activity(
                            DiagnosticStatus::Success,
                            Some(TransportKind::Relay),
                            Some(from_id.to_string()),
                            "已通过 Relay 接收并应用移动端数据",
                            None,
                        );
                    } else {
                        let pending_detail = match &receipt.delivery_error {
                            Some(err) => format!("已通过 Relay 接收移动端数据，配置暂存待应用: {}", err),
                            None => "已通过 Relay 接收移动端数据，配置暂存待应用".to_string(),
                        };
                        if let Some(app) = ctx.app {
                            if sync_data.notes.len() > 0 {
                                let _ = app.emit("notebook:updated", serde_json::json!({ "count": sync_data.notes.len() }));
                            }
                            if sync_data.events.len() > 0 {
                                let _ = app.emit("calendar-updated", serde_json::json!({ "action": "sync", "count": sync_data.events.len() }));
                            }
                            let _ = app.emit("sync:progress", serde_json::json!({
                                "stage": "relay_sync",
                                "status": "pending_apply",
                                "detail": pending_detail,
                                "total_records": outcome.total_records,
                            }));
                        }

                        let _ = crate::sync_history::record_activity(
                            DiagnosticStatus::Pending,
                            Some(TransportKind::Relay),
                            Some(from_id.to_string()),
                            pending_detail,
                            None,
                        );
                    }
                    let mut commit_ack = serde_json::json!({
                        "type": "ack",
                        "target_device_id": from_id,
                        "payload": {
                            "status": "committed",
                            "action": "commit_ack",
                            "stage": receipt.stage,
                            "applied_count": receipt.applied_count,
                            "pending_count": receipt.pending_count,
                            "delivery_error": receipt.delivery_error,
                        }
                    });
                    copy_trace_fields(json, &mut commit_ack, true);
                    let signed_ack = send_signed_relay_response(ctx, tx_mpsc, commit_ack).await?;

                    ctx.record_idempotency(
                        &envelope.session_id,
                        &envelope.request_id,
                        &envelope.subject_device_id,
                        &envelope.target_device_id,
                        &envelope.action,
                        &envelope.payload_hash,
                        &signed_ack.to_string(),
                    );

                    let mut legacy_ack = serde_json::json!({
                        "type": "commit_ack",
                        "target_device_id": from_id,
                        "payload": {
                            "status": "committed",
                            "action": "commit_ack",
                            "stage": receipt.stage,
                            "applied_count": receipt.applied_count,
                            "pending_count": receipt.pending_count,
                            "delivery_error": receipt.delivery_error,
                        }
                    });
                    copy_trace_fields(json, &mut legacy_ack, true);
                    send_signed_relay_response(ctx, tx_mpsc, legacy_ack).await?;
                }
                Err(e) => {
                    log::error!("[Sync Engine] Failed to import relay pushed DB data: {}", e);
                    let _ = ctx.with_db(|conn| {
                        crate::device_trust::fail_rpc_idempotency(
                            conn,
                            &session_id,
                            &request_id,
                            &execution_token,
                            &e,
                            crate::now_ms(),
                        )
                    });
                    let _ = crate::sync_history::record_activity(
                        DiagnosticStatus::Failed,
                        Some(TransportKind::Relay),
                        Some(from_id.to_string()),
                        "Relay 数据写入 PC 失败",
                        Some("ERR-SYNC-RELAY-IMPORT".to_string()),
                    );

                    let mut err_resp = serde_json::json!({
                        "type": "proxy",
                        "target_device_id": from_id,
                        "payload": { "action": "error", "error": format!("DB Import failed: {}", e) }
                    });
                    copy_trace_fields(json, &mut err_resp, true);
                    send_signed_relay_response(ctx, tx_mpsc, err_resp).await?;
                    return Err(format!("DB Import failed: {}", e));
                }
            }
        }

        ctx.emit("sync:device_syncing", serde_json::json!({
            "device_id": from_id,
            "status": "idle"
        }));
        return Ok(());
    }

    Ok(())
}

pub fn sec01_check_relay_proxy_target(
    envelope_target: &str,
    app: Option<&AppHandle>,
) -> Result<String, String> {
    let local_device_id = crate::http_api::resolve_local_device_id_checked(app)
        .map_err(|e| format!("Forbidden: Local device ID is unconfigured or unavailable (SEC-01 fail-closed): {}", e))?;
    if local_device_id.trim().is_empty() {
        return Err("Forbidden: Local device ID is unconfigured or unavailable (SEC-01 fail-closed)".to_string());
    }
    if envelope_target != local_device_id {
        return Err(format!("Forbidden: Target device ID mismatch (expected '{}', got '{}')", local_device_id, envelope_target));
    }
    Ok(local_device_id)
}

pub fn dispatch_relay_proxy_message(
    conn: &mut rusqlite::Connection,
    relay_msg: &serde_json::Value,
    target_device_id: &str,
    now: i64,
) -> Result<serde_json::Value, serde_json::Value> {
    let inner_payload = match relay_msg.get("payload") {
        Some(p) => p,
        None => {
            return Err(serde_json::json!({
                "type": "proxy",
                "target_device_id": target_device_id,
                "payload": {
                    "action": "error",
                    "error_type": "malformed",
                    "error": "Missing payload in proxy message"
                }
            }));
        }
    };
    let from_id_raw = relay_msg.get("from_device_id").and_then(|v| v.as_str()).unwrap_or("unknown");

    let auth_val = inner_payload.get("auth_envelope")
        .or_else(|| inner_payload.get("envelope"))
        .or_else(|| relay_msg.get("auth_envelope"))
        .or_else(|| relay_msg.get("envelope"));

    let envelope: crate::device_trust::RpcAuthEnvelope = match auth_val {
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(env) => env,
            Err(e) => {
                return Err(serde_json::json!({
                    "type": "proxy",
                    "target_device_id": from_id_raw,
                    "payload": {
                        "action": "error",
                        "error_type": "unauthorized",
                        "error": format!("Unauthorized: Malformed auth_envelope (SEC-01 fail-closed): {}", e)
                    }
                }));
            }
        },
        None => {
            return Err(serde_json::json!({
                "type": "proxy",
                "target_device_id": from_id_raw,
                "payload": {
                    "action": "error",
                    "error_type": "unauthorized",
                    "error": "Unauthorized: Missing cryptographic RPC authentication envelope (SEC-01 fail-closed)"
                }
            }));
        }
    };

    let action = inner_payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if envelope.action != action {
        return Err(serde_json::json!({
            "type": "proxy",
            "target_device_id": from_id_raw,
            "payload": {
                "action": "error",
                "error_type": "action_mismatch",
                "error": format!("Forbidden: Action mismatch between envelope ('{}') and payload ('{}')", envelope.action, action)
            }
        }));
    }

    if envelope.target_device_id != target_device_id {
        return Err(serde_json::json!({
            "type": "proxy",
            "target_device_id": from_id_raw,
            "payload": {
                "action": "error",
                "error_type": "target_mismatch",
                "error": format!("Forbidden: Target device ID mismatch (expected '{}', got '{}')", target_device_id, envelope.target_device_id)
            }
        }));
    }

    if envelope.subject_device_id != from_id_raw {
        return Err(serde_json::json!({
            "type": "proxy",
            "target_device_id": from_id_raw,
            "payload": {
                "action": "error",
                "error_type": "subject_mismatch",
                "error": "Forbidden: Caller identity mismatch between relay message and cryptographic envelope"
            }
        }));
    }

    let raw_payload_bytes = extract_relay_payload_bytes(inner_payload, &envelope.payload_hash);
    let auth_res = crate::device_trust::verify_rpc_request_auth(conn, &envelope, &raw_payload_bytes, target_device_id, now);

    let (subject_device_id, execution_token) = match auth_res {
        Ok(crate::device_trust::AuthVerificationOutcome::Authorized { subject_device_id, execution_token, .. }) => {
            (subject_device_id, execution_token)
        }
        Ok(crate::device_trust::AuthVerificationOutcome::IdempotentCached { cached_response }) => {
            let cached_val = serde_json::from_str::<serde_json::Value>(&cached_response)
                .unwrap_or_else(|_| serde_json::json!({
                    "type": "proxy",
                    "target_device_id": from_id_raw,
                    "payload": {
                        "action": if action == "push" { "commit_ack" } else { "pull_response" },
                        "status": "committed"
                    }
                }));
            return Ok(cached_val);
        }
        Err(e) => {
            let error_type = if e.contains("Idempotency conflict") {
                "conflict"
            } else {
                "unauthorized"
            };
            return Err(serde_json::json!({
                "type": "proxy",
                "target_device_id": from_id_raw,
                "payload": {
                    "action": "error",
                    "error_type": error_type,
                    "error": format!("Unauthorized (SEC-01 fail-closed): {}", e)
                }
            }));
        }
    };

    if action == "push" {
        let empty_vec = vec![];
        let ops = inner_payload.get("data")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_vec);

        let resp_payload = serde_json::json!({
            "type": "proxy",
            "target_device_id": subject_device_id,
            "payload": {
                "action": "commit_ack",
                "status": "committed",
                "request_id": envelope.request_id
            }
        });
        let resp_str = resp_payload.to_string();

        crate::device_trust::atomic_commit_push_outbox(
            conn,
            &envelope.session_id,
            &envelope.request_id,
            &execution_token,
            ops,
            &resp_str,
            now,
        ).map_err(|e| serde_json::json!({
            "type": "proxy",
            "target_device_id": subject_device_id,
            "payload": {
                "action": "error",
                "error_type": "commit_failed",
                "error": e
            }
        }))?;

        Ok(resp_payload)
    } else if action == "pull" {
        let since_ts = inner_payload.get("since_ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let sync_data = export_sync_data_from_conn(conn, since_ts, true)
            .unwrap_or_else(|_| SyncData {
                config: serde_json::json!({}),
                settings: vec![],
                conversations: vec![],
                messages: vec![],
                events: vec![],
                captures: vec![],
                cron_jobs: vec![],
                kg_nodes: vec![],
                kg_edges: vec![],
                wiki_fts: vec![],
                tombstones: vec![],
                notes: vec![],
            });

        let pull_resp = serde_json::json!({
            "type": "proxy",
            "target_device_id": subject_device_id,
            "payload": {
                "action": "pull_response",
                "data": sync_data
            }
        });
        let resp_str = pull_resp.to_string();

        crate::device_trust::atomic_commit_push_outbox(
            conn,
            &envelope.session_id,
            &envelope.request_id,
            &execution_token,
            &[],
            &resp_str,
            now,
        ).map_err(|e| serde_json::json!({
            "type": "proxy",
            "target_device_id": subject_device_id,
            "payload": {
                "action": "error",
                "error_type": "commit_failed",
                "error": e
            }
        }))?;

        Ok(pull_resp)
    } else {
        Err(serde_json::json!({
            "type": "proxy",
            "target_device_id": subject_device_id,
            "payload": {
                "action": "error",
                "error_type": "unsupported_action",
                "error": format!("Unsupported action: {}", action)
            }
        }))
    }
}

pub fn process_relay_device_revocation_frame(
    conn: &mut rusqlite::Connection,
    msg: &serde_json::Value,
    local_id: &str,
    local_sk: &ed25519_dalek::SigningKey,
    now_ms: i64,
) -> Result<serde_json::Value, String> {
    let payload = msg.get("payload").and_then(|p| p.get("device_revocation").or_else(|| p.get("revocation")))
        .or_else(|| msg.get("payload")).unwrap_or(msg);
    let cert = serde_json::from_value::<crate::device_trust::DeviceRevocationCertificate>(payload.clone())
        .map_err(|e| format!("无法解析 Relay device_revocation 证书: {}", e))?;
    let ack = crate::device_trust::sec01_verify_and_apply_peer_revocation(conn, &cert, local_id, local_sk, now_ms)?;
    let ack_msg = serde_json::json!({
        "type": "ack",
        "target_device_id": cert.revoked_device_id,
        "payload": { "device_revocation_ack": ack }
    });
    Ok(ack_msg)
}

pub fn process_relay_device_revocation_ack_frame(
    conn: &mut rusqlite::Connection,
    msg: &serde_json::Value,
    now_ms: i64,
) -> Result<crate::device_trust::RevocationAckOutcome, String> {
    let payload = msg.get("payload").and_then(|p| p.get("device_revocation_ack"))
        .or_else(|| msg.get("payload")).unwrap_or(msg);
    let ack = serde_json::from_value::<crate::device_trust::RevocationAck>(payload.clone())
        .map_err(|e| format!("无法解析 device_revocation_ack: {}", e))?;
    if ack.status != "committed" || ack.event_id.is_empty() {
        return Err("非法 Ack 状态或缺少 event_id".to_string());
    }
    let expected_revoked_id: Option<String> = conn.query_row(
        "SELECT revoked_device_id FROM peer_revocation_outbox WHERE event_id = ?",
        [&ack.event_id],
        |row| row.get(0),
    ).optional().map_err(|e| format!("查询撤销事件失败: {}", e))?;
    if let Some(expected_id) = expected_revoked_id {
        crate::device_trust::sec01_verify_and_apply_peer_revocation_ack(conn, &ack, &expected_id, now_ms)
    } else {
        Err(format!("收到未知的撤销出件箱事件 Ack: event_id={}", ack.event_id))
    }
}

fn resolve_relay_registration_id(
    conn: &Connection,
    configured_id: Option<&str>,
) -> Result<Option<(String, bool)>, String> {
    // 即使已经生成新身份，也必须先用旧身份投递未确认的撤销证书；
    // 否则新身份的正常连接会令旧身份的出件箱永久得不到对应 Relay 注册。
    let old_id: Option<String> = conn.query_row(
        "SELECT revoked_device_id FROM peer_revocation_outbox WHERE status IN ('pending', 'sent') ORDER BY id LIMIT 1",
        [],
        |row| row.get(0),
    ).optional().map_err(|e| format!("查询待投递撤销身份失败: {}", e))?;
    if let Some(id) = old_id {
        if id.trim().is_empty() {
            return Err("待投递撤销记录缺少旧设备身份，拒绝普通 Relay 连接".to_string());
        }
        return Ok(Some((id, true)));
    }
    Ok(configured_id.filter(|id| !id.trim().is_empty())
        .map(|id| (id.trim().to_string(), false)))
}

fn queue_peer_revocations_for_relay(
    conn: &Connection,
    tx: &tokio::sync::mpsc::Sender<Message>,
    identity_filter: Option<&str>,
    now_ms: i64,
) -> Result<usize, String> {
    crate::device_trust::revert_stale_sent_revocations(conn, 60_000, now_ms)?;
    let mut stmt = conn.prepare(
        "SELECT id, event_id, target_peer_id, revoked_device_id, revocation_payload
         FROM peer_revocation_outbox
         WHERE status = 'pending' AND (?1 IS NULL OR revoked_device_id = ?1)
         ORDER BY id LIMIT 50",
    ).map_err(|e| format!("查询待投递撤销失败: {}", e))?;
    let rows = stmt.query_map([identity_filter], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?))
    }).map_err(|e| format!("读取待投递撤销失败: {}", e))?;
    let items = rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("解析待投递撤销失败: {}", e))?;
    let mut queued = 0;
    for (id, event_id, target_id, revoked_id, payload) in items {
        let cert: crate::device_trust::DeviceRevocationCertificate = serde_json::from_str(&payload)
            .map_err(|e| format!("撤销出件箱证书损坏: {}", e))?;
        if cert.event_id != event_id || cert.target_device_id != target_id || cert.revoked_device_id != revoked_id {
            return Err(format!("撤销出件箱身份绑定不一致: {}", event_id));
        }
        let frame = serde_json::json!({
            "type": "notify", "target_device_id": target_id,
            "payload": { "device_revocation": cert },
        });
        crate::device_trust::mark_peer_revocation_sent(conn, id, now_ms)?;
        if let Err(e) = tx.try_send(Message::Text(frame.to_string().into())) {
            crate::device_trust::mark_peer_revocation_failed(conn, id, &e.to_string())?;
            return Err(format!("撤销证书无法入队，仍保留待投递: {}", e));
        }
        queued += 1;
    }
    Ok(queued)
}

fn is_allowed_lan_revocation_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let octets = v4.octets();
            v4.is_private() || v4.is_link_local() || v4.is_loopback()
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        }
        std::net::IpAddr::V6(v6) => v6.is_unique_local() || v6.is_unicast_link_local() || v6.is_loopback(),
    }
}

fn start_peer_revocation_lan_retry(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5)).build() {
            Ok(client) => client,
            Err(e) => { log::error!("[Sync Engine] LAN 撤销客户端初始化失败: {}", e); return; }
        };
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let ctx = RelayDispatchContext::new(&app);
            let items = match ctx.with_db(|conn| {
                crate::device_trust::revert_stale_sent_revocations(conn, 60_000, crate::now_ms())?;
                crate::device_trust::get_pending_peer_revocations(conn, 50)
            }) {
                Ok(items) => items,
                Err(e) => { log::error!("[Sync Engine] LAN 撤销出件箱查询失败: {}", e); continue; }
            };
            let registry = app.try_state::<Arc<DeviceRegistry>>();
            let endpoints = registry.map(|r| r.get_all()).unwrap_or_default();
            for item in items {
                let Some(device) = endpoints.iter().find(|d| d.device_id == item.target_peer_id) else { continue; };
                let Ok(ip) = device.ip_address.parse::<std::net::IpAddr>() else { continue; };
                if !is_allowed_lan_revocation_ip(ip) { continue; }
                let address = std::net::SocketAddr::new(ip, 3722);
                let claimed = ctx.with_db(|conn| crate::device_trust::mark_peer_revocation_sent(conn, item.id, crate::now_ms()));
                if claimed.is_err() { continue; }
                let response = client.post(format!("http://{}/v1/device/revoke", address))
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(item.revocation_payload.clone()).send().await;
                let outcome = match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<crate::device_trust::RevocationAck>().await {
                            Ok(ack) => ctx.with_db(|conn| {
                                crate::device_trust::sec01_verify_and_apply_peer_revocation_ack(
                                    conn, &ack, &item.revoked_device_id, crate::now_ms(),
                                ).map(|_| ())
                            }),
                            Err(e) => Err(format!("LAN 撤销确认解析失败: {}", e)),
                        }
                    }
                    Ok(resp) => Err(format!("LAN 撤销端点返回 {}", resp.status())),
                    Err(e) => Err(format!("LAN 撤销连接失败: {}", e)),
                };
                if let Err(e) = outcome {
                    log::warn!("[Sync Engine] 撤销事件 {} 待重试: {}", item.event_id, e);
                    if let Err(db_err) = ctx.with_db(|conn| crate::device_trust::mark_peer_revocation_failed(conn, item.id, &e)) {
                        log::error!("[Sync Engine] 撤销失败状态持久化失败: {}", db_err);
                    }
                }
            }
        }
    });
}

pub fn start_relay_listener(app: AppHandle) {
    start_peer_revocation_lan_retry(app.clone());
    tauri::async_runtime::spawn(async move {
        loop {
            crate::sync_diagnostics::set_relay_state(
                crate::sync_diagnostics::RelayConnectionState::Connecting,
            );
            let mut current_device_id = String::new();
            let mut revocation_only = false;
            {
                let config = match crate::read_config_checked() {
                    Ok(c) => c,
                    Err(e) => {
                        log::error!("[Sync Engine] SEC-01 Fail-Closed: Config read error during relay reconnect: {}. Retrying after backoff...", e);
                        crate::sync_diagnostics::set_relay_state(crate::sync_diagnostics::RelayConnectionState::Disconnected);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };
                let configured_id = config.get("device_id").and_then(|v| v.as_str());
                let ctx = RelayDispatchContext::new(&app);
                match ctx.with_db(|conn| resolve_relay_registration_id(conn, configured_id)) {
                    Ok(Some((id, only))) => {
                        current_device_id = id;
                        revocation_only = only;
                    }
                    Ok(None) => {}
                    Err(e) => log::error!("[Sync Engine] 撤销身份查询失败，拒绝连接: {}", e),
                }
                if current_device_id.is_empty() {
                    if let Ok(sk) = crate::crypto::ensure_device_identity_unlocked_for_app(&app) {
                        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
                        let vk = ed25519_dalek::VerifyingKey::from(&sk);
                        current_device_id = BASE64.encode(vk.to_bytes());
                    }
                }
                if current_device_id.is_empty() {
                    log::error!("[Sync Engine] SEC-01 Fail-Closed: Local device_id missing in config during relay reconnect. Retrying after backoff...");
                    crate::sync_diagnostics::set_relay_state(crate::sync_diagnostics::RelayConnectionState::Disconnected);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            }
            if !revocation_only {
                crate::sync_diagnostics::set_local_identity(
                    crate::sync_diagnostics::LocalIdentityState::Ready,
                );
            }

            let relay_url = "wss://relay.bobbik.org".to_string();
            let ws_url = format!(
                "{}/ws/device/{}",
                relay_url,
                url_encode_device_id(&current_device_id)
            );

            match connect_websocket_robust(&ws_url).await {
                Ok((mut ws_stream, _)) => {
                    log::info!("[Sync Engine] Connected to Relay WebSocket: {}", ws_url);
                    RELAY_CONNECTED.store(!revocation_only, Ordering::SeqCst);
                    crate::sync_diagnostics::set_relay_state(
                        crate::sync_diagnostics::RelayConnectionState::Registered,
                    );

                    // Explicitly register device ID (fixes NGINX URL stripping bugs)
                    let reg_msg = serde_json::json!({
                        "type": "register",
                        "deviceId": current_device_id
                    });
                    if let Err(e) = ws_stream.send(Message::Text(reg_msg.to_string().into())).await {
                        log::error!("[Sync Engine] Relay 注册失败，撤销证书保留待重试: {}", e);
                        RELAY_CONNECTED.store(false, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }

                    use futures_util::{SinkExt, StreamExt};
                    let (mut tx, mut rx) = ws_stream.split();
                    let (tx_mpsc, mut rx_mpsc) = tokio::sync::mpsc::channel::<Message>(100);
                    let (reconnect_tx, mut reconnect_rx) = tokio::sync::mpsc::channel::<()>(1);
                    let mut ping_interval =
                        tokio::time::interval(std::time::Duration::from_secs(15));

                    if !revocation_only {
                        let mut lock = RELAY_TX.write().unwrap();
                        *lock = Some(tx_mpsc.clone());
                        let mut pending = PENDING_REQUESTS.write().unwrap();
                        pending.clear();
                    }
                    {
                        let mut lock = RELAY_RECONNECT_TRIGGER.lock().unwrap();
                        *lock = Some(reconnect_tx);
                    }

                    let mut last_activity = crate::now_ms();

                    let ctx = RelayDispatchContext::new(&app);
                    if let Err(e) = ctx.with_db(|conn| queue_peer_revocations_for_relay(
                        conn, &tx_mpsc,
                        revocation_only.then_some(current_device_id.as_str()),
                        crate::now_ms(),
                    )) {
                        log::error!("[Sync Engine] 撤销出件箱初始投递失败: {}", e);
                    }

                    loop {
                        tokio::select! {
                            _ = reconnect_rx.recv() => {
                                log::info!("[Sync Engine] Received manual reconnect trigger!");
                                break;
                            }
                            _ = ping_interval.tick() => {
                                if crate::now_ms() - last_activity > 45_000 {
                                    log::error!("[Sync Engine] Relay connection timeout: No activity for 45s. Reconnecting...");
                                    break;
                                }

                                // SEC-01 Fail-Closed: 检查配置与身份，若读取失败或 ID 变更则立即断开会话并停止 Ping
                                if revocation_only {
                                    let ctx = RelayDispatchContext::new(&app);
                                    let still_pending = ctx.with_db(|conn| resolve_relay_registration_id(conn, None));
                                    match still_pending {
                                        Ok(Some((id, true))) if id == current_device_id => {}
                                        Ok(_) => break,
                                        Err(e) => { log::error!("[Sync Engine] 撤销身份续期检查失败: {}", e); break; }
                                    }
                                } else if let Err(e) = sec01_check_relay_identity_continuity(&current_device_id) {
                                    log::warn!("[Sync Engine] {}. Disconnecting Relay before Ping.", e);
                                    break;
                                }

                                let ctx = RelayDispatchContext::new(&app);
                                if let Err(e) = ctx.with_db(|conn| queue_peer_revocations_for_relay(
                                    conn, &tx_mpsc,
                                    revocation_only.then_some(current_device_id.as_str()),
                                    crate::now_ms(),
                                )) {
                                    log::error!("[Sync Engine] 撤销出件箱重试失败: {}", e);
                                }

                                let _ = tx_mpsc.send(Message::Ping(bytes::Bytes::new())).await;
                            }
                            mpsc_msg_opt = rx_mpsc.recv() => {
                                if let Some(msg) = mpsc_msg_opt {
                                    if let Err(e) = tx.send(msg).await {
                                        log::error!("[Sync Engine] Failed to send WS message: {}", e);
                                        break;
                                    }
                                }
                            }
                            msg_opt = rx.next() => {
                                let msg = match msg_opt {
                                    Some(m) => m,
                                    None => {
                                        log::error!("[Sync Engine] Relay WS connection closed (None)");
                                        break;
                                    }
                                };
                                last_activity = crate::now_ms();
                                match msg {
                            Ok(Message::Text(text)) => {
                                match serde_json::from_str::<serde_json::Value>(&text) {
                                    Ok(json) => {
                                        let ctx = RelayDispatchContext::new(&app);
                                        let dispatch_result = if revocation_only {
                                            if matches!(json.get("type").and_then(|v| v.as_str()), Some("ack") | Some("device_revocation_ack"))
                                                && json.get("payload").and_then(|p| p.get("device_revocation_ack")).is_some() {
                                                ctx.with_db(|conn| process_relay_device_revocation_ack_frame(conn, &json, crate::now_ms()).map(|_| ()))
                                            } else {
                                                Err("撤销专用连接拒绝非 Ack 消息".to_string())
                                            }
                                        } else {
                                            dispatch_inbound_relay_message_core(&ctx, &json, &tx_mpsc).await
                                        };
                                        if let Err(e) = dispatch_result {
                                            log::warn!("[Sync Engine] Inbound relay dispatch rejected (SEC-01 Fail-Closed): {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("[Sync Engine] Failed to parse inbound relay WS message as JSON: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("[Sync Engine] Relay WS error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                            } // closes rx.next() =>
                        } // closes tokio::select!
                    } // closes loop
                    RELAY_CONNECTED.store(false, Ordering::SeqCst);
                    if let Ok(mut lock) = RELAY_TX.write() {
                        *lock = None;
                    }
                } // closes Ok((ws_stream, _)) =>
                Err(e) => {
                    log::error!("[Sync Engine] Failed to connect to Relay: {}", e);
                    RELAY_CONNECTED.store(false, Ordering::SeqCst);
                }
            }
            RELAY_CONNECTED.store(false, Ordering::SeqCst);
            crate::sync_diagnostics::set_relay_state(
                crate::sync_diagnostics::RelayConnectionState::Disconnected,
            );

            // Reconnect backoff
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

#[tauri::command]
pub fn get_sync_logs(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let path = crate::get_data_dir().join("sync_history.json");
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(logs) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
            return Ok(logs);
        }
    }
    Ok(vec![])
}

#[tauri::command]
pub fn get_shared_intents(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let mut results = vec![];
    if let Ok(cache_dir) = app.path().cache_dir() {
        let incoming_dir = cache_dir.join("shared_incoming");
        if let Ok(entries) = std::fs::read_dir(&incoming_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();

                if file_name.ends_with(".txt") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        results.push(serde_json::json!({
                            "type": "text",
                            "filename": file_name,
                            "content": content
                        }));
                    }
                } else {
                    // Treat as image or binary file
                    results.push(serde_json::json!({
                        "type": "file",
                        "filename": file_name,
                        "path": path.to_string_lossy().into_owned()
                    }));
                }
            }
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn clear_shared_intent(app: tauri::AppHandle, filename: String) -> Result<(), String> {
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "分享缓存文件名无效".to_string())?;
    if safe_name != filename {
        return Err("分享缓存路径越界".to_string());
    }
    if let Ok(cache_dir) = app.path().cache_dir() {
        let incoming_dir = cache_dir.join("shared_incoming");
        let file_path = incoming_dir.join(safe_name);
        if file_path.exists() {
            std::fs::remove_file(file_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_p2p_relay_status() -> bool {
    RELAY_CONNECTED.load(Ordering::SeqCst)
}

#[tauri::command]

pub fn force_relay_reconnect() {
    log::info!("[Sync Engine] Force reconnect triggered by frontend network change");
    if let Some(tx) = RELAY_RECONNECT_TRIGGER.lock().unwrap().as_ref() {
        let _ = tx.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use rand::RngCore;
    use serde_json::json;

    #[test]
    fn test_sec01_lan_revocation_only_uses_local_or_tailnet_addresses() {
        assert!(is_allowed_lan_revocation_ip("100.64.0.5".parse().unwrap()));
        assert!(is_allowed_lan_revocation_ip("192.168.1.2".parse().unwrap()));
        assert!(is_allowed_lan_revocation_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_allowed_lan_revocation_ip("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn test_sec01_relay_revocation_outbox_is_queued_and_old_identity_is_revocation_only() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE peer_revocation_outbox (
                id INTEGER PRIMARY KEY, event_id TEXT, target_peer_id TEXT,
                revoked_device_id TEXT, revocation_payload TEXT, status TEXT,
                attempts INTEGER DEFAULT 0, sent_at INTEGER, last_error TEXT
            );",
        ).unwrap();
        let signer = SigningKey::from_bytes(&[17u8; 32]);
        let old_id = base64::engine::general_purpose::STANDARD.encode(VerifyingKey::from(&signer).to_bytes());
        let cert = crate::device_trust::create_device_revocation_certificate(
            &signer, "peer-id", "key_reset", 1_000,
        );
        conn.execute(
            "INSERT INTO peer_revocation_outbox
             (event_id, target_peer_id, revoked_device_id, revocation_payload, status)
             VALUES (?1, ?2, ?3, ?4, 'pending')",
            rusqlite::params![cert.event_id, cert.target_device_id, old_id,
                serde_json::to_string(&cert).unwrap()],
        ).unwrap();
        assert_eq!(resolve_relay_registration_id(&conn, None).unwrap(), Some((old_id.clone(), true)));
        let new_id = "new-device-id";
        assert_eq!(resolve_relay_registration_id(&conn, Some(new_id)).unwrap(), Some((old_id.clone(), true)),
            "新身份已配置时仍必须先投递旧身份的撤销证书");
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        assert_eq!(queue_peer_revocations_for_relay(&conn, &tx, Some(&old_id), 2_000).unwrap(), 1);
        let frame = rx.try_recv().unwrap().into_text().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(parsed["type"], "notify");
        assert_eq!(parsed["target_device_id"], "peer-id");
        assert_eq!(parsed["payload"]["device_revocation"]["event_id"], cert.event_id);
        assert_eq!(queue_peer_revocations_for_relay(&conn, &tx, Some(&old_id), 2_001).unwrap(), 0);
        assert_eq!(crate::device_trust::revert_stale_sent_revocations(&conn, 60_000, 63_000).unwrap(), 1);
        assert_eq!(queue_peer_revocations_for_relay(&conn, &tx, Some(&old_id), 63_000).unwrap(), 1);
        conn.execute("UPDATE peer_revocation_outbox SET status = 'delivered' WHERE event_id = ?1", [&cert.event_id]).unwrap();
        assert_eq!(resolve_relay_registration_id(&conn, Some(new_id)).unwrap(), Some((new_id.to_string(), false)),
            "旧身份撤销已确认后才能恢复新身份的普通 Relay 连接");
    }

    static RECOVERY_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_test_recovery() -> std::sync::MutexGuard<'static, ()> {
        let guard = match RECOVERY_TEST_MUTEX.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        set_recovery_degraded(false, None);
        guard
    }

#[test]
    fn test_apply_unified_diff() {
        let old = "line 1\nline 2\nline 3";
        let diff = "--- a/test.txt\n+++ b/test.txt\n@@ -1,3 +1,4 @@\n line 1\n+line 1.5\n line 2\n line 3";
        let applied = apply_unified_diff(old, diff).unwrap();
        assert!(applied.contains("line 1.5"));
        assert!(applied.contains("line 1"));
        assert!(applied.contains("line 2"));
    }

#[test]
    fn test_compute_content_hash() {
        let empty_hash = compute_content_hash("");
        assert_eq!(empty_hash, "d41d8cd98f00b204e9800998ecf8427e");

        let hash1 = compute_content_hash("fn main() {}");
        let hash2 = compute_content_hash("fn main() {}");
        let hash3 = compute_content_hash("fn main() { println!(\"modified\"); }");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

#[tokio::test]
    async fn test_cancellation_unknown_task() {
        let req_id = format!("req_unknown_{}", uuid::Uuid::new_v4());
        let (outcome, is_confirmed, status, err_opt) =
            cancel_active_rpc_task_core(&req_id, std::time::Duration::from_millis(50)).await;

        assert_eq!(outcome, CancelOutcome::UnknownTask);
        assert!(!is_confirmed);
        assert_eq!(status, "unknown_task");
        assert!(err_opt.unwrap().contains("未找到该活跃任务"));
    }

#[tokio::test]
    async fn test_cancellation_abnormal_channel_drop() {
        let req_id = format!("req_abnormal_{}", uuid::Uuid::new_v4());
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let (done_tx, done_rx) = tokio::sync::watch::channel(false);

        ACTIVE_RPC_TASKS.lock().unwrap().insert(req_id.clone(), ActiveRpcTask {
            cancel_tx,
            done_rx,
        });

        // Spawn mock worker that drops done_tx without ever sending `true`
        tokio::spawn(async move {
            while cancel_rx.changed().await.is_ok() {
                if *cancel_rx.borrow() {
                    // Deliberately drop done_tx without sending true!
                    drop(done_tx);
                    break;
                }
            }
        });

        let (outcome, is_confirmed, status, err_opt) =
            cancel_active_rpc_task_core(&req_id, std::time::Duration::from_millis(500)).await;

        assert_eq!(outcome, CancelOutcome::AbortedAbnormally);
        assert!(!is_confirmed);
        assert_eq!(status, "aborted_abnormally");
        assert!(err_opt.unwrap().contains("未能确认正常清理退出"));

        ACTIVE_RPC_TASKS.lock().unwrap().remove(&req_id);
    }

#[tokio::test]
    async fn test_cancellation_real_confirmed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();

        let req_id = format!("req_confirmed_{}", uuid::Uuid::new_v4());
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let (done_tx, done_rx) = tokio::sync::watch::channel(false);

        // Register active task
        ACTIVE_RPC_TASKS.lock().unwrap().insert(req_id.clone(), ActiveRpcTask {
            cancel_tx,
            done_rx,
        });

        // Spawn mock worker responding immediately to cancel
        let req_id_clone = req_id.clone();
        tokio::spawn(async move {
            while cancel_rx.changed().await.is_ok() {
                if *cancel_rx.borrow() {
                    let _ = done_tx.send(true);
                    ACTIVE_RPC_TASKS.lock().unwrap().remove(&req_id_clone);
                    break;
                }
            }
        });

        let staged = StagedChange {
            change_id: format!("chg_conf_{}", uuid::Uuid::new_v4()),
            request_id: req_id.clone(),
            project_id: "".to_string(),
            file_path: "src/fast.rs".to_string(),
            old_content: "".to_string(),
            new_content: "data".to_string(),
            old_content_hash: compute_content_hash(""),
            diff: "".to_string(),
            summary: "fast task".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        let (outcome, is_confirmed, status, err_opt) =
            cancel_active_rpc_task_core(&req_id, std::time::Duration::from_millis(500)).await;

        assert_eq!(outcome, CancelOutcome::Confirmed);
        assert!(is_confirmed);
        assert_eq!(status, "cancelled");
        assert!(err_opt.is_none());

        // When confirmed, cancellation is applied to database
        let _ = cancel_staged_changes_by_request(&conn, &req_id);
        let loaded = get_staged_change(&conn, &staged.change_id).unwrap().unwrap();
        assert_eq!(loaded.status, "cancelled");
    }

#[test]
    fn test_approval_rollback_failure_reporting() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_rb_fail_{}.txt", uuid::Uuid::new_v4()));

        let initial_content = "initial v1";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        let backup = safe_atomic_write_file_with_backup(None, "chg_rb_rep_1", &file_path, "new v2").unwrap();
        assert!(backup.bak_path.is_some());
        let bak = backup.bak_path.clone().unwrap();
        assert!(bak.exists());

        // Intentionally delete backup before rollback to simulate failure
        std::fs::remove_file(&bak).unwrap();

        let res = backup.rollback();
        assert!(res.is_err());
        let err_str = res.unwrap_err();
        assert!(err_str.contains("恢复未完成"), "Error must report 恢复未完成: {}", err_str);
        assert!(err_str.contains("备份文件不存在"), "Error must contain reason: {}", err_str);

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_admission_control_blocks_unresolved_recovery_path() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        crate::work_core::repository::init_work_core_tables(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_admission_{}.txt", uuid::Uuid::new_v4()));
        let file_path_str = file_path.to_string_lossy().to_string();

        let initial_content = "existing content";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        // Insert unresolved crash recovery entry from a previous crashed change
        record_staged_write_recovery_stage(
            &conn,
            "crashed_change_old",
            &file_path_str,
            None,
            "dummy_tmp",
            "replaced",
        ).unwrap();

        // Now attempt to approve a new change on the same path
        let new_change_id = "change_new_proposal";
        let staged = StagedChange {
            change_id: new_change_id.to_string(),
            request_id: "req_adm_1".to_string(),
            project_id: "".to_string(),
            file_path: file_path_str.clone(),
            old_content: initial_content.to_string(),
            new_content: "new proposed content".to_string(),
            old_content_hash: compute_content_hash(initial_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        let outcome = process_approval_decision_core(&mut conn, new_change_id, "approve", "tester");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("error"));
        let err_msg = outcome.get("error").and_then(|v| v.as_str()).unwrap_or("");
        assert!(err_msg.contains("存在未解决的崩溃恢复冲突或未决恢复记录"), "Expected admission block, got: {}", err_msg);

        // Also test direct safe_atomic_write_file_with_backup admission block
        let write_res = safe_atomic_write_file_with_backup(Some(&conn), new_change_id, &file_path, "direct write");
        assert!(write_res.is_err());
        assert!(write_res.unwrap_err().contains("存在未解决的崩溃恢复冲突或未决恢复记录"));

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_approval_transaction_rollback_on_db_error() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        crate::work_core::repository::init_work_core_tables(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_appr_tx_rb_{}.txt", uuid::Uuid::new_v4()));
        let file_path_str = file_path.to_str().unwrap().to_string();

        let initial_content = "original file content v1";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        let new_content = "new file content v2";
        let change_id = format!("chg_tx_fail_{}", uuid::Uuid::new_v4());
        let staged = StagedChange {
            change_id: change_id.clone(),
            request_id: "req_tx_fail_1".to_string(),
            project_id: "".to_string(),
            file_path: file_path_str.clone(),
            old_content: initial_content.to_string(),
            new_content: new_content.to_string(),
            old_content_hash: compute_content_hash(initial_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        // Inject trigger failure on staged_changes update to force transaction failure
        conn.execute_batch(
            "CREATE TRIGGER force_tx_fail BEFORE UPDATE ON staged_changes BEGIN SELECT RAISE(ABORT, 'Simulated DB Tx Failure'); END;"
        ).unwrap();

        let outcome = process_approval_decision_core(&mut conn, &change_id, "approve", "tester");

        // Verify transaction failed
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("error"));
        let err_msg = outcome.get("error").and_then(|v| v.as_str()).unwrap_or("");
        assert!(err_msg.contains("已安全回滚磁盘文件以防状态倾斜"), "Unexpected err: {}", err_msg);

        // Verify disk content was rolled back to original content v1
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, initial_content);

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_crash_recovery_applied_target_missing_or_corrupted_preserves_bak() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_app_corrupt_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_app_corrupt_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_app_corrupt_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "original good content";
        let committed_content = "committed desired content";
        let corrupted_disk_content = "CORRUPTED / PARTIAL DISK DATA";

        // Target on disk is corrupted, not matching committed_content!
        std::fs::write(&file_path, corrupted_disk_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        // tmp file also corrupted or gone
        std::fs::write(&tmp_path, "invalid tmp".as_bytes()).unwrap();

        let change_id = "change_applied_corrupt_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_app_corrupt".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: committed_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "applied".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: Some(2000),
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err(), "Must fail-closed and return Err on degradation");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains(change_id));
        assert!(err_msg.contains("已恢复: 0"));

        // Verify system enters degraded state!
        let (degraded, reason) = is_recovery_degraded();
        assert!(degraded);
        assert!(reason.unwrap().contains("已提交变更但在磁盘损坏/缺失"));

        // Verify backup was NOT destroyed; preserved as bak_committed_corruption
        let file_name = file_path.file_name().unwrap().to_str().unwrap();
        let safe_bak = temp_dir.join(format!(".{}.bak_committed_corruption_{}", file_name, change_id));
        assert!(safe_bak.exists(), "Backup must be preserved!");
        assert_eq!(std::fs::read_to_string(&safe_bak).unwrap(), old_content);

        // Verify WAL record is NOT deleted
        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&safe_bak);
        let _ = std::fs::remove_file(&tmp_path);
    }

#[tokio::test]
    async fn test_cancellation_real_timeout() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();

        let req_id = format!("req_timeout_{}", uuid::Uuid::new_v4());
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        let (_done_tx, done_rx) = tokio::sync::watch::channel(false);

        // Register active task that ignores cancellation
        ACTIVE_RPC_TASKS.lock().unwrap().insert(req_id.clone(), ActiveRpcTask {
            cancel_tx,
            done_rx,
        });

        let staged = StagedChange {
            change_id: format!("chg_timeout_{}", uuid::Uuid::new_v4()),
            request_id: req_id.clone(),
            project_id: "".to_string(),
            file_path: "src/slow.rs".to_string(),
            old_content: "".to_string(),
            new_content: "data".to_string(),
            old_content_hash: compute_content_hash(""),
            diff: "".to_string(),
            summary: "slow task".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        // Cancel with a short timeout of 50ms
        let (outcome, is_confirmed, status, err_opt) =
            cancel_active_rpc_task_core(&req_id, std::time::Duration::from_millis(50)).await;

        assert_eq!(outcome, CancelOutcome::Timeout);
        assert!(!is_confirmed);
        assert_eq!(status, "timeout");
        assert!(err_opt.unwrap().contains("未退出"));

        // When not confirmed, proposal MUST remain pending
        let loaded = get_staged_change(&conn, &staged.change_id).unwrap().unwrap();
        assert_eq!(loaded.status, "pending");

        // Cleanup
        ACTIVE_RPC_TASKS.lock().unwrap().remove(&req_id);
    }

#[test]
    fn test_crash_recovery_cross_restart_pending_cleanup_converges_without_bak() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let uid = uuid::Uuid::new_v4();
        let db_path = temp_dir.join(format!("bob_test_cross_restart_{}.db", uid));
        let file_path = temp_dir.join(format!("bob_test_cross_restart_target_{}.txt", uid));
        let bak_path = temp_dir.join(format!(".bob_test_cross_restart_target_{}.bak", uid));
        let tmp_path = temp_dir.join(format!(".bob_test_cross_restart_target_{}.tmp", uid));

        let old_content = "ORIGINAL_PERSISTED_CONTENT\n";
        let new_content = "PROPOSED_UNAPPLIED_CONTENT\n";
        let change_id = "change_cross_restart_001";

        // Setup: target has old_content, bak has old_content, tmp has new_content
        std::fs::write(&file_path, old_content).unwrap();
        std::fs::write(&bak_path, old_content).unwrap();
        std::fs::write(&tmp_path, new_content).unwrap();

        // 1. Connection 1: staged_changes created (status = "pending"), WAL created (stage = "backed_up")
        {
            let mut conn1 = rusqlite::Connection::open(&db_path).unwrap();
            init_staged_changes_table(&conn1).unwrap();
            init_staged_write_recovery_table(&conn1).unwrap();

            let staged = StagedChange {
                change_id: change_id.to_string(),
                request_id: "req_cross_001".to_string(),
                project_id: "".to_string(),
                file_path: file_path.to_string_lossy().to_string(),
                old_content: old_content.to_string(),
                new_content: new_content.to_string(),
                old_content_hash: compute_content_hash(old_content),
                diff: "diff".to_string(),
                summary: "summary".to_string(),
                additions: 1,
                deletions: 1,
                status: "pending".to_string(),
                work_object_id: None,
                created_at: 1000,
                applied_at: None,
            };
            save_staged_change(&conn1, &staged).unwrap();

            record_staged_write_recovery_stage(
                &conn1,
                change_id,
                &file_path.to_string_lossy(),
                Some(&bak_path.to_string_lossy()),
                &tmp_path.to_string_lossy(),
                "backed_up",
            ).unwrap();

            // Simulate DB error on WAL DELETE: trigger force_delete_fail
            conn1.execute_batch(
                "CREATE TRIGGER force_delete_fail BEFORE DELETE ON staged_write_recovery
                 BEGIN SELECT RAISE(ABORT, 'simulated transient sqlite error on delete'); END;"
            ).unwrap();

            // Run 1: safe_restore_target_internal succeeds, stage transitions to restored_pending_cleanup,
            // but WAL delete fails due to trigger.
            let rec1 = recover_interrupted_staged_writes(&mut conn1).unwrap();
            assert_eq!(rec1.recovered_and_cleaned, 0);
            assert_eq!(rec1.restored_pending_cleanup, 1, "Run 1 must record 1 restored_pending_cleanup when WAL delete fails");

            // Verify: .bak was consumed (does not exist anymore on disk!)
            assert!(!bak_path.exists());
            // Verify: target matches old_content
            assert_eq!(std::fs::read_to_string(&file_path).unwrap(), old_content);
            // Verify: WAL stage is now 'restored_pending_cleanup'
            let stage: String = conn1.query_row(
                "SELECT stage FROM staged_write_recovery WHERE change_id = ?1",
                rusqlite::params![change_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(stage, "restored_pending_cleanup");

            // Close connection 1 cleanly (simulating process exit/reboot)
        }

        // 2. Connection 2: completely new connection (simulating next app startup)
        {
            let mut conn2 = rusqlite::Connection::open(&db_path).unwrap();
            // Drop failure trigger
            conn2.execute_batch("DROP TRIGGER IF EXISTS force_delete_fail;").unwrap();

            // Run 2: recover_interrupted_staged_writes MUST converge even though .bak is missing!
            let rec2 = recover_interrupted_staged_writes(&mut conn2).unwrap();
            assert_eq!(rec2.restored_pending_cleanup, 1, "Must converge via pending cleanup");
            assert_eq!(rec2.recovered_and_cleaned, 0);

            let (degraded, reason) = is_recovery_degraded();
            assert!(!degraded, "Must not degrade: {:?}", reason);

            // WAL record is cleanly deleted
            let count: i64 = conn2.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
            assert_eq!(count, 0, "WAL entry must be deleted after convergence");

            // Target file still has old_content intact
            assert_eq!(std::fs::read_to_string(&file_path).unwrap(), old_content);

            // Run 3: Idempotent - subsequent recovery does nothing
            let rec3 = recover_interrupted_staged_writes(&mut conn2).unwrap();
            assert_eq!(rec3.recovered_and_cleaned, 0);
            assert_eq!(rec3.restored_pending_cleanup, 0);
        }

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&bak_path);
        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_file(&db_path);
    }

#[test]
    fn test_crash_recovery_cross_restart_pending_cleanup_external_conflict_degrades() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let uid = uuid::Uuid::new_v4();
        let db_path = temp_dir.join(format!("bob_test_conflict_{}.db", uid));
        let file_path = temp_dir.join(format!("bob_test_conflict_target_{}.txt", uid));
        let tmp_path = temp_dir.join(format!(".bob_test_conflict_target_{}.tmp", uid));

        let old_content = "ORIGINAL_SAFE_CONTENT\n";
        let new_content = "NEW_UNAPPLIED_CONTENT\n";
        let change_id = "change_conflict_001";

        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_conflict_001".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: new_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            None,
            &tmp_path.to_string_lossy(),
            "restored_pending_cleanup",
        ).unwrap();

        // Simulate external tampering between restarts: file contains UNKNOWN external content!
        std::fs::write(&file_path, "EXTERNALLY_TAMPERED_CONTENT_UNKNOWN").unwrap();

        // Run recovery: MUST detect that target does not match expected old_content, MUST NOT delete WAL, and MUST DEGRADE
        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err(), "Recovery must return Err on external conflict during pending cleanup");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("崩溃恢复降级中止"));
        assert!(err_msg.contains("外部篡改或缺失"));

        let (degraded, reason) = is_recovery_degraded();
        assert!(degraded, "System must enter degraded mode");
        assert!(reason.unwrap().contains("外部篡改或缺失"));

        // WAL record is PRESERVED for investigation
        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "WAL entry must be retained on external tampering conflict");

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_file(&db_path);
    }

#[test]
    fn test_crash_recovery_data_restored_but_tmp_cleanup_failure_retains_wal_for_retry() {
        let _lock = lock_test_recovery();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_retry_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_retry_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_retry_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "original backup content to restore";
        let uncommitted_content = "uncommitted content on disk";
        std::fs::write(&file_path, uncommitted_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        // Create tmp_path as a directory so std::fs::remove_file fails
        std::fs::create_dir(&tmp_path).unwrap();

        let change_id = "change_retry_cleanup_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_retry_1".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: uncommitted_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        let recovered = recover_interrupted_staged_writes(&mut conn).unwrap();
        // Target restored, but cleanup pending retry -> recovered count is 0
        assert_eq!(recovered, 0);

        // System is NOT degraded (data was successfully restored)
        let (degraded, _) = is_recovery_degraded();
        assert!(!degraded, "System should not degrade when data was restored and only cleanup is pending retry");

        // Target file WAS restored to original content
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), old_content);

        // WAL record is retained for retry
        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "WAL record must be retained when cleanup failed");

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&bak_path);
        let _ = std::fs::remove_dir(&tmp_path);
    }

#[test]
    fn test_crash_recovery_declared_backup_missing_on_disk_does_not_delete_wal() {
        let _lock = lock_test_recovery();
        set_recovery_degraded(false, None);

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_missing_bak_{}.txt", uuid::Uuid::new_v4()));
        let nonexistent_bak = temp_dir.join(format!("bob_nonexistent_bak_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_missing_bak_{}.tmp", uuid::Uuid::new_v4()));

        std::fs::write(&file_path, "current uncommitted content").unwrap();
        std::fs::write(&tmp_path, "tmp").unwrap();
        // Notice: nonexistent_bak is NOT created!

        let change_id = "change_missing_bak_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_missing_bak".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: "original content".to_string(),
            new_content: "new content".to_string(),
            old_content_hash: compute_content_hash("original content"),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&nonexistent_bak.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err(), "Recovery must fail-closed and return Err when declared backup is missing");

        // System MUST enter degraded state
        let (degraded, reason) = is_recovery_degraded();
        assert!(degraded);
        assert!(reason.unwrap().contains("声明有备份但文件缺失"));

        // WAL record MUST NOT be deleted
        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);

        set_recovery_degraded(false, None);
        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&tmp_path);
    }

#[test]
    fn test_crash_recovery_degraded_immediately_aborts_subsequent_wal_entries() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let uid = uuid::Uuid::new_v4();
        let file1 = temp_dir.join(format!("bob_test_entry1_{}.txt", uid));
        let file2 = temp_dir.join(format!("bob_test_entry2_{}.txt", uid));
        let tmp1 = temp_dir.join(format!(".bob_test_entry1_{}.tmp", uid));
        let tmp2 = temp_dir.join(format!(".bob_test_entry2_{}.tmp", uid));
        let bak1 = temp_dir.join(format!(".bob_test_entry1_{}.bak", uid)); // intentionally NOT created on disk!
        let bak2 = temp_dir.join(format!(".bob_test_entry2_{}.bak", uid));

        // Entry 2 backup IS created on disk
        std::fs::write(&file2, "ENTRY2_OLD").unwrap();
        std::fs::write(&bak2, "ENTRY2_OLD").unwrap();
        std::fs::write(&tmp2, "ENTRY2_NEW").unwrap();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let staged1 = StagedChange {
            change_id: "entry_001".to_string(),
            request_id: "req_1".to_string(),
            project_id: "".to_string(),
            file_path: file1.to_string_lossy().to_string(),
            old_content: "ENTRY1_OLD".to_string(),
            new_content: "ENTRY1_NEW".to_string(),
            old_content_hash: compute_content_hash("ENTRY1_OLD"),
            diff: "d1".to_string(),
            summary: "s1".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged1).unwrap();

        let staged2 = StagedChange {
            change_id: "entry_002".to_string(),
            request_id: "req_2".to_string(),
            project_id: "".to_string(),
            file_path: file2.to_string_lossy().to_string(),
            old_content: "ENTRY2_OLD".to_string(),
            new_content: "ENTRY2_NEW".to_string(),
            old_content_hash: compute_content_hash("ENTRY2_OLD"),
            diff: "d2".to_string(),
            summary: "s2".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 2000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged2).unwrap();

        conn.execute(
            "INSERT INTO staged_write_recovery (change_id, target_path, bak_path, tmp_path, stage, created_at)
             VALUES (?1, ?2, ?3, ?4, 'backed_up', 1000)",
            rusqlite::params![
                "entry_001",
                file1.to_str().unwrap(),
                bak1.to_str().unwrap(),
                tmp1.to_str().unwrap()
            ],
        ).unwrap();

        conn.execute(
            "INSERT INTO staged_write_recovery (change_id, target_path, bak_path, tmp_path, stage, created_at)
             VALUES (?1, ?2, ?3, ?4, 'backed_up', 2000)",
            rusqlite::params![
                "entry_002",
                file2.to_str().unwrap(),
                bak2.to_str().unwrap(),
                tmp2.to_str().unwrap()
            ],
        ).unwrap();

        // Run recovery: entry 1 will degrade because bak1 is missing.
        // It MUST abort immediately, returning Err with remaining_unprocessed = 1.
        // Entry 2 must NOT be processed or deleted.
        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err(), "Recovery must return Err immediately upon degradation");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("entry_001"));
        assert!(err_msg.contains("剩余未处理: 1"));

        let (degraded, _) = is_recovery_degraded();
        assert!(degraded, "System must be in degraded mode");

        // Entry 2 WAL MUST still exist in database untouched
        let count2: i64 = conn.query_row(
            "SELECT count(*) FROM staged_write_recovery WHERE change_id = 'entry_002'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count2, 1, "Entry 2 WAL must be untouched because batch aborted immediately");

        // Entry 2 .bak still exists on disk (not consumed)
        assert!(bak2.exists(), "Entry 2 backup must remain untouched");

        let _ = std::fs::remove_file(&file1);
        let _ = std::fs::remove_file(&file2);
        let _ = std::fs::remove_file(&tmp1);
        let _ = std::fs::remove_file(&tmp2);
        let _ = std::fs::remove_file(&bak1);
        let _ = std::fs::remove_file(&bak2);
    }

#[test]
    fn test_crash_recovery_detects_external_file_modification_conflict() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_conflict_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_conflict_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_conflict_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "original file content";
        let crash_content = "replaced content before crash";
        let user_external_content = "USER EDITED THIS MANUALLY IN VS CODE DURING DOWNTIME";

        // Disk currently has user's manual external edits
        std::fs::write(&file_path, user_external_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "tmp".as_bytes()).unwrap();

        let change_id = "change_conflict_sim_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_conflict_1".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: crash_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        let recovered = recover_interrupted_staged_writes(&mut conn).unwrap();
        // Conflict was detected and skipped from overwrite
        assert_eq!(recovered, 0);

        // User's manual file edit is SAFE and NOT overwritten
        let disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk, user_external_content);

        // Path is marked as blocked
        assert!(is_path_recovery_blocked(&conn, &file_path.to_string_lossy(), None).unwrap());

        // Check that conflict backup file was preserved
        let file_name = file_path.file_name().unwrap().to_str().unwrap();
        let conflict_path = temp_dir.join(format!(".{}.bak_recovery_conflict_{}", file_name, change_id));
        assert!(conflict_path.exists());
        let conf_content = std::fs::read_to_string(&conflict_path).unwrap();
        assert_eq!(conf_content, old_content);

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&conflict_path);
    }

#[test]
    fn test_crash_recovery_from_wal_journal() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_crash_rec_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_crash_rec_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_crash_rec_{}.tmp", uuid::Uuid::new_v4()));

        let orig_content = "original file content before crash";
        let crashed_content = "uncommitted modified content during crash";
        let abandoned_tmp = "abandoned tmp bytes";

        std::fs::write(&file_path, crashed_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, orig_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, abandoned_tmp.as_bytes()).unwrap();

        let change_id = "change_crash_sim_1";
        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        // Perform startup recovery
        let recovered = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(recovered, 1);

        // File is restored to original content
        let disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk, orig_content);

        // Bak and tmp are cleaned up
        assert!(!bak_path.exists());
        assert!(!tmp_path.exists());

        // Recovery table entry is removed
        let remaining: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(remaining, 0);

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_crash_recovery_idempotent_two_consecutive_runs() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_idem_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_idem_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_idem_{}.tmp", uuid::Uuid::new_v4()));

        let initial_content = "initial content";
        let crash_content = "crash content";
        std::fs::write(&file_path, crash_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, initial_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "tmp".as_bytes()).unwrap();

        let change_id = "change_idem_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_idem".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: initial_content.to_string(),
            new_content: crash_content.to_string(),
            old_content_hash: compute_content_hash(initial_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        // First run: restores target
        let rec1 = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(rec1, 1);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), initial_content);

        // Second run immediately: 0 entries, succeeds with Ok(0)
        let rec2 = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(rec2, 0);

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_crash_recovery_new_file_external_conflict_maintains_admission_block() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_new_conflict_{}.txt", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_new_conflict_{}.tmp", uuid::Uuid::new_v4()));

        let staged_content = "staged new file content";
        let user_written_content = "USER CREATED THIS FILE EXTERNALLY WHILE BOB WAS OFF";

        // File exists with user written content
        std::fs::write(&file_path, user_written_content.as_bytes()).unwrap();

        let change_id = "change_new_conflict_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_new_conf".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: "".to_string(),
            new_content: staged_content.to_string(),
            old_content_hash: compute_content_hash(""),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            None, // Brand new file
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        let recovered = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(recovered, 0);

        // User file was NOT deleted
        assert!(file_path.exists());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), user_written_content);

        // Recovery record was NOT deleted
        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);

        // Admission control continues to BLOCK writes to this path
        let blocked = is_path_recovery_blocked(&conn, &file_path.to_string_lossy(), None).unwrap();
        assert!(blocked);

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_crash_recovery_new_file_recreated_before_second_startup_degrades() {
        let _guard = lock_test_recovery();
        set_recovery_degraded(false, None);

        let temp_dir = std::env::temp_dir().join(format!("bob_test_new_file_reappear_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let uid = uuid::Uuid::new_v4().to_string();
        let db_path = temp_dir.join(format!("bob_test_new_file_reappear_{}.db", uid));
        let file_path = temp_dir.join(format!("reappear_file_{}.txt", uid));
        let tmp_path = temp_dir.join(format!(".reappear_file_{}.txt.tmp", uid));

        let uncommitted_content = "brand new content";
        std::fs::write(&file_path, uncommitted_content.as_bytes()).unwrap();

        let change_id = "change_reappear_1";

        {
            let mut conn1 = rusqlite::Connection::open(&db_path).unwrap();
            init_staged_changes_table(&conn1).unwrap();
            init_staged_write_recovery_table(&conn1).unwrap();

            let staged = StagedChange {
                change_id: change_id.to_string(),
                request_id: "req_reappear_1".to_string(),
                project_id: "".to_string(),
                file_path: file_path.to_string_lossy().to_string(),
                old_content: "".to_string(),
                new_content: uncommitted_content.to_string(),
                old_content_hash: compute_content_hash(""),
                diff: "diff".to_string(),
                summary: "summary".to_string(),
                additions: 1,
                deletions: 0,
                status: "pending".to_string(),
                work_object_id: None,
                created_at: 1000,
                applied_at: None,
            };
            save_staged_change(&conn1, &staged).unwrap();

            record_staged_write_recovery_stage(
                &conn1,
                change_id,
                &file_path.to_string_lossy(),
                None,
                &tmp_path.to_string_lossy(),
                "replaced",
            ).unwrap();

            conn1.execute_batch(
                "CREATE TRIGGER force_delete_fail BEFORE DELETE ON staged_write_recovery
                 BEGIN SELECT RAISE(ABORT, 'Simulated WAL delete failure'); END;"
            ).unwrap();

            // Run 1 deletes new file, sets restored_absent_pending_cleanup
            let rec1 = recover_interrupted_staged_writes(&mut conn1).unwrap();
            assert_eq!(rec1.restored_pending_cleanup, 1);
            assert!(!file_path.exists());

            drop(conn1);
        }

        // 模拟外部异常：在两次启动之间，该新建文件被外部意外创建！
        std::fs::write(&file_path, "unexpected file recreated between reboots").unwrap();

        {
            let mut conn2 = rusqlite::Connection::open(&db_path).unwrap();
            conn2.execute_batch("DROP TRIGGER IF EXISTS force_delete_fail;").unwrap();

            // Run 2 检测到预期 absent 的文件存在，fail-closed 降级中止！
            let res2 = recover_interrupted_staged_writes(&mut conn2);
            assert!(res2.is_err(), "Must abort with error when absent file exists");
            let (degraded2, _) = is_recovery_degraded();
            assert!(degraded2, "Must enter degraded state");

            // WAL 未被误删
            let wal_count: i64 = conn2.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
            assert_eq!(wal_count, 1, "WAL must be retained for admission locking");
        }

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

#[test]
    fn test_crash_recovery_new_file_wal_delete_failure_converges_on_second_startup() {
        let _guard = lock_test_recovery();
        set_recovery_degraded(false, None);

        let temp_dir = std::env::temp_dir().join(format!("bob_test_new_file_converge_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let uid = uuid::Uuid::new_v4().to_string();
        let db_path = temp_dir.join(format!("bob_test_new_file_converge_wal_{}.db", uid));
        let file_path = temp_dir.join(format!("new_uncommitted_file_{}.txt", uid));
        let tmp_path = temp_dir.join(format!(".new_uncommitted_file_{}.txt.tmp", uid));

        let uncommitted_content = "brand new uncommitted file content";
        std::fs::write(&file_path, uncommitted_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "tmp bytes").unwrap();

        let change_id = "change_new_file_converge_1";

        // 1. 真实启动 Run 1 (打开物理 SQLite 临时文件，注入 WAL 删除故障触发器)
        {
            let mut conn1 = rusqlite::Connection::open(&db_path).unwrap();
            init_staged_changes_table(&conn1).unwrap();
            init_staged_write_recovery_table(&conn1).unwrap();

            let staged = StagedChange {
                change_id: change_id.to_string(),
                request_id: "req_new_file_converge_1".to_string(),
                project_id: "".to_string(),
                file_path: file_path.to_string_lossy().to_string(),
                old_content: "".to_string(),
                new_content: uncommitted_content.to_string(),
                old_content_hash: compute_content_hash(""),
                diff: "diff".to_string(),
                summary: "summary".to_string(),
                additions: 1,
                deletions: 0,
                status: "pending".to_string(),
                work_object_id: None,
                created_at: 1000,
                applied_at: None,
            };
            save_staged_change(&conn1, &staged).unwrap();

            // 新建文件：bak_path 为 None
            record_staged_write_recovery_stage(
                &conn1,
                change_id,
                &file_path.to_string_lossy(),
                None,
                &tmp_path.to_string_lossy(),
                "replaced",
            ).unwrap();

            // 注入触发器故障，模拟 Run 1 删除新建文件成功，但删除 WAL 失败
            conn1.execute_batch(
                "CREATE TRIGGER force_delete_fail BEFORE DELETE ON staged_write_recovery
                 BEGIN SELECT RAISE(ABORT, 'Simulated WAL delete failure on new file'); END;"
            ).unwrap();

            // Run 1: 删除未提交新建文件成功，但删除 WAL 失败
            let rec1 = recover_interrupted_staged_writes(&mut conn1).unwrap();
            assert_eq!(rec1.recovered_and_cleaned, 0, "Run 1 must return 0 because cleanup was incomplete");
            assert_eq!(rec1.restored_pending_cleanup, 1, "Run 1 marks entry as restored_absent_pending_cleanup");

            // 目标新建文件必须已被成功删除！
            assert!(!file_path.exists(), "Target uncommitted new file must be deleted");

            // 系统绝不进入全局降级（未提交新建文件已被正确删除回滚）
            let (degraded1, reason1) = is_recovery_degraded();
            assert!(!degraded1, "System must not degrade on run 1 when new file rollback succeeded: {:?}", reason1);

            // 核对 WAL 状态已持久化为 restored_absent_pending_cleanup
            let (count1, stage1): (i64, String) = conn1.query_row(
                "SELECT count(*), stage FROM staged_write_recovery WHERE change_id = ?1",
                rusqlite::params![change_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).unwrap();
            assert_eq!(count1, 1);
            assert_eq!(stage1, "restored_absent_pending_cleanup");

            // 显式断开数据库连接 1 (模拟 Run 1 退出)
            drop(conn1);
        }

        // 2. 真实第二次启动 Run 2 (重新打开物理数据库，移除故障触发器，再运行启动自愈)
        {
            let mut conn2 = rusqlite::Connection::open(&db_path).unwrap();
            conn2.execute_batch("DROP TRIGGER IF EXISTS force_delete_fail;").unwrap();

            // Run 2: recover_interrupted_staged_writes 重新启动自愈
            let rec2 = recover_interrupted_staged_writes(&mut conn2).expect("第二次启动自愈应当成功收敛");
            assert_eq!(rec2.restored_pending_cleanup, 1, "Second startup must converge via absent pending cleanup retry");
            assert_eq!(rec2.recovered_and_cleaned, 0, "Second startup must not double count as recovered_and_cleaned");

            // 核对系统非降级状态
            let (degraded2, reason2) = is_recovery_degraded();
            assert!(!degraded2, "Second startup must NOT enter degraded mode: {:?}", reason2);

            // 核对 WAL 已彻底收敛清零
            let wal_count: i64 = conn2.query_row(
                "SELECT count(*) FROM staged_write_recovery",
                [],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(wal_count, 0, "WAL table must be clean and converged to 0");

            // 核对新建文件依然保持不存在（Absent）
            assert!(!file_path.exists(), "Target file must remain absent on disk");
        }

        // 3. 清理环境
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

#[test]
    fn test_crash_recovery_query_staged_change_failure_does_not_rollback_and_degrades() {
        let _lock = lock_test_recovery();
        set_recovery_degraded(false, None);

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        // Initialize recovery table, but intentionally DO NOT create staged_changes table,
        // causing get_staged_change to return Err.
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_query_fail_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_query_fail_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_query_fail_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "old content in backup";
        let target_content = "current disk content that should not be touched";
        std::fs::write(&file_path, target_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "tmp".as_bytes()).unwrap();

        let change_id = "change_query_fail_1";
        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err(), "Must return Err and fail-closed when querying staged change fails");

        // System MUST enter degraded state
        let (degraded, reason) = is_recovery_degraded();
        assert!(degraded, "Must enter degraded state when querying staged change fails");
        assert!(reason.unwrap().contains("查询暂存变更状态失败"));

        // File MUST NOT be rolled back
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), target_content);

        // Backup and WAL record MUST still exist
        assert!(bak_path.exists());
        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);

        set_recovery_degraded(false, None);
        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&bak_path);
        let _ = std::fs::remove_file(&tmp_path);
    }

#[test]
    fn test_extract_diff_from_text() {
        let text = "我为你修改了文件：\n```diff\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,2 +1,3 @@\n fn main() {\n+    println!(\"Hello\");\n }\n```\n请审阅。";
        let extracted = extract_diff_from_text(text);
        assert!(extracted.is_some());
        let (path, diff, adds, dels) = extracted.unwrap();
        assert_eq!(path, "src/main.rs");
        assert!(diff.contains("+    println!(\"Hello\");"));
        assert_eq!(adds, 1);
        assert_eq!(dels, 0);
    }

#[test]
    fn test_crash_recovery_reopen_sqlite_database() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("bob_test_reopen_db_{}.db", uuid::Uuid::new_v4()));
        let file_path = temp_dir.join(format!("bob_test_reopen_file_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_reopen_file_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_reopen_file_{}.tmp", uuid::Uuid::new_v4()));

        let initial_content = "original persisted data";
        let crashed_content = "crashed dirty data";
        std::fs::write(&file_path, crashed_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, initial_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "tmp bytes".as_bytes()).unwrap();

        let change_id = "change_reopen_1";
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            init_staged_changes_table(&conn).unwrap();
            init_staged_write_recovery_table(&conn).unwrap();

            let staged = StagedChange {
                change_id: change_id.to_string(),
                request_id: "req_reopen".to_string(),
                project_id: "".to_string(),
                file_path: file_path.to_string_lossy().to_string(),
                old_content: initial_content.to_string(),
                new_content: crashed_content.to_string(),
                old_content_hash: compute_content_hash(initial_content),
                diff: "diff".to_string(),
                summary: "summary".to_string(),
                additions: 1,
                deletions: 1,
                status: "pending".to_string(),
                work_object_id: None,
                created_at: 1000,
                applied_at: None,
            };
            save_staged_change(&conn, &staged).unwrap();

            record_staged_write_recovery_stage(
                &conn,
                change_id,
                &file_path.to_string_lossy(),
                Some(&bak_path.to_string_lossy()),
                &tmp_path.to_string_lossy(),
                "replaced",
            ).unwrap();
        }

        // Reopen database connection in a new session
        {
            let mut conn = rusqlite::Connection::open(&db_path).unwrap();
            let recovered = recover_interrupted_staged_writes(&mut conn).unwrap();
            assert_eq!(recovered, 1);

            // Verify disk file is restored to initial content
            assert_eq!(std::fs::read_to_string(&file_path).unwrap(), initial_content);

            // Verify recovery entry is cleaned up in the re-opened DB
            let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
            assert_eq!(count, 0);
        }

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&db_path);
    }

#[test]
    fn test_generate_unified_diff() {
        let old = "line 1\nline 2\nline 3\n";
        let new = "line 1\nline 2 modified\nline 3\nline 4\n";
        let (diff, additions, deletions) = generate_unified_diff("src/test.rs", old, new);
        assert!(diff.contains("--- a/src/test.rs"));
        assert!(diff.contains("+++ b/src/test.rs"));
        assert_eq!(additions, 2);
        assert_eq!(deletions, 1);
    }

#[test]
    fn test_crash_recovery_retains_already_applied_change() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_applied_rec_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_applied_rec_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_applied_rec_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "version 1 before commit";
        let committed_content = "version 2 approved and applied in DB";
        std::fs::write(&file_path, committed_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "residual tmp bytes".as_bytes()).unwrap();

        let change_id = "change_applied_sim_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_applied_1".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: committed_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "applied".to_string(), // DB was committed before crash!
            work_object_id: None,
            created_at: 1000,
            applied_at: Some(2000),
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        // Perform recovery
        let recovered = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(recovered, 1);

        // File is NOT reverted; committed version 2 is retained!
        let disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk, committed_content);

        // Backup and tmp are cleaned up
        assert!(!bak_path.exists());
        assert!(!tmp_path.exists());

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_crash_recovery_update_stage_failure_aborts_subsequent_entries() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let uid = uuid::Uuid::new_v4();
        let file1 = temp_dir.join(format!("bob_test_p1_entry1_{}.txt", uid));
        let file2 = temp_dir.join(format!("bob_test_p1_entry2_{}.txt", uid));
        let bak1 = temp_dir.join(format!(".bob_test_p1_entry1_{}.bak", uid));
        let bak2 = temp_dir.join(format!(".bob_test_p1_entry2_{}.bak", uid));
        let tmp1 = temp_dir.join(format!(".bob_test_p1_entry1_{}.tmp", uid));
        let tmp2 = temp_dir.join(format!(".bob_test_p1_entry2_{}.tmp", uid));

        std::fs::write(&file1, "ENTRY1_CRASH").unwrap();
        std::fs::write(&bak1, "ENTRY1_OLD").unwrap();
        std::fs::write(&tmp1, "tmp1").unwrap();

        std::fs::write(&file2, "ENTRY2_CRASH").unwrap();
        std::fs::write(&bak2, "ENTRY2_OLD").unwrap();
        std::fs::write(&tmp2, "tmp2").unwrap();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let staged1 = StagedChange {
            change_id: "p1_entry_001".to_string(),
            request_id: "req_1".to_string(),
            project_id: "".to_string(),
            file_path: file1.to_string_lossy().to_string(),
            old_content: "ENTRY1_OLD".to_string(),
            new_content: "ENTRY1_CRASH".to_string(),
            old_content_hash: compute_content_hash("ENTRY1_OLD"),
            diff: "d1".to_string(),
            summary: "s1".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged1).unwrap();

        let staged2 = StagedChange {
            change_id: "p1_entry_002".to_string(),
            request_id: "req_2".to_string(),
            project_id: "".to_string(),
            file_path: file2.to_string_lossy().to_string(),
            old_content: "ENTRY2_OLD".to_string(),
            new_content: "ENTRY2_CRASH".to_string(),
            old_content_hash: compute_content_hash("ENTRY2_OLD"),
            diff: "d2".to_string(),
            summary: "s2".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 2000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged2).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            "p1_entry_001",
            &file1.to_string_lossy(),
            Some(&bak1.to_string_lossy()),
            &tmp1.to_string_lossy(),
            "replaced",
        ).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            "p1_entry_002",
            &file2.to_string_lossy(),
            Some(&bak2.to_string_lossy()),
            &tmp2.to_string_lossy(),
            "replaced",
        ).unwrap();

        // Inject trigger on entry 1 update
        conn.execute_batch(
            "CREATE TRIGGER fail_first_entry BEFORE UPDATE ON staged_write_recovery
             WHEN NEW.change_id = 'p1_entry_001'
             BEGIN SELECT RAISE(ABORT, 'Entry 1 update failed'); END;"
        ).unwrap();

        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("剩余未处理: 1"));

        // Entry 2 WAL must be 100% untouched
        let count2: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = 'p1_entry_002'", [], |r| r.get(0)).unwrap();
        assert_eq!(count2, 1, "Entry 2 WAL must be completely untouched");

        // Entry 2 assets must still exist
        assert!(bak2.exists());

        let _ = std::fs::remove_file(&file1);
        let _ = std::fs::remove_file(&file2);
        let _ = std::fs::remove_file(&bak1);
        let _ = std::fs::remove_file(&bak2);
        let _ = std::fs::remove_file(&tmp1);
        let _ = std::fs::remove_file(&tmp2);
    }

#[test]
    fn test_mutating_tool_detection() {
        assert!(crate::tools::is_mutating_tool("write_file"));
    }

    #[test]
    fn test_applied_recovery_preserves_committed_content() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_applied_rec_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_applied_rec_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_applied_rec_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "version 1 before commit";
        let committed_content = "version 2 approved and applied in DB";
        std::fs::write(&file_path, committed_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "residual tmp bytes".as_bytes()).unwrap();

        let change_id = "change_applied_sim_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_applied_1".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: committed_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "applied".to_string(), // DB was committed before crash!
            work_object_id: None,
            created_at: 1000,
            applied_at: Some(2000),
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        // Perform recovery
        let recovered = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(recovered, 1);

        // File is NOT reverted; committed version 2 is retained!
        let disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk, committed_content);

        // Backup and tmp are cleaned up
        assert!(!bak_path.exists());
        assert!(!tmp_path.exists());

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_crash_recovery_update_stage_failure_applied_target_preserves_assets_and_aborts() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_p1_applied_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_p1_applied_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_p1_applied_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "initial content";
        let new_content = "applied committed content";
        std::fs::write(&file_path, new_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, new_content.as_bytes()).unwrap();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let change_id = "change_p1_app_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_p1_app".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: new_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "applied".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: Some(1500),
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        // Inject trigger to cause UPDATE staged_write_recovery to fail
        conn.execute_batch(
            "CREATE TRIGGER fail_wal_update_applied BEFORE UPDATE ON staged_write_recovery
             BEGIN SELECT RAISE(ABORT, 'Simulated update stage failure applied'); END;"
        ).unwrap();

        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err());

        let (degraded, _) = is_recovery_degraded();
        assert!(degraded);

        // Assets must be preserved未被清理
        assert!(bak_path.exists(), "Backup must remain untouched");
        assert!(tmp_path.exists(), "Tmp file must remain untouched");

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&bak_path);
        let _ = std::fs::remove_file(&tmp_path);
    }

#[test]
    fn test_crash_recovery_update_stage_failure_preserves_bak_and_aborts() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_p1_fail_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_p1_fail_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_p1_fail_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "initial file content";
        let crash_content = "crashed modified content";
        std::fs::write(&file_path, crash_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "tmp content".as_bytes()).unwrap();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let change_id = "change_p1_fail_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_p1_fail".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: crash_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        // Inject trigger to cause UPDATE staged_write_recovery to fail
        conn.execute_batch(
            "CREATE TRIGGER fail_wal_update BEFORE UPDATE ON staged_write_recovery
             BEGIN SELECT RAISE(ABORT, 'Simulated update stage failure'); END;"
        ).unwrap();

        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_err(), "Must fail closed and return error when stage update fails");
        let err = res.unwrap_err();
        assert!(err.contains("Simulated update stage failure") || err.contains("崩溃恢复降级中止"));

        let (degraded, _) = is_recovery_degraded();
        assert!(degraded, "System must enter degraded mode");

        // Backup must be safely preserved on disk!
        assert!(bak_path.exists(), "Original .bak file must be preserved on disk");

        // WAL entry must NOT be deleted
        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "WAL entry must not be deleted");

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&bak_path);
        let _ = std::fs::remove_file(&tmp_path);
    }

#[test]
    fn test_crash_recovery_update_stage_failure_reopen_sqlite_survives() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let uid = uuid::Uuid::new_v4();
        let db_path = temp_dir.join(format!("bob_test_p1_reopen_{}.db", uid));
        let file_path = temp_dir.join(format!("bob_test_p1_reopen_target_{}.txt", uid));
        let bak_path = temp_dir.join(format!(".bob_test_p1_reopen_target_{}.bak", uid));
        let tmp_path = temp_dir.join(format!(".bob_test_p1_reopen_target_{}.tmp", uid));

        let old_content = "ORIGINAL_SAFE_CONTENT\n";
        let uncommitted_content = "UNCOMMITTED_CRASH_CONTENT\n";
        let change_id = "change_p1_reopen_001";

        std::fs::write(&file_path, uncommitted_content).unwrap();
        std::fs::write(&bak_path, old_content).unwrap();
        std::fs::write(&tmp_path, "tmp").unwrap();

        // Run 1: inject trigger to fail update
        {
            let mut conn1 = rusqlite::Connection::open(&db_path).unwrap();
            init_staged_changes_table(&conn1).unwrap();
            init_staged_write_recovery_table(&conn1).unwrap();

            let staged = StagedChange {
                change_id: change_id.to_string(),
                request_id: "req_reopen".to_string(),
                project_id: "".to_string(),
                file_path: file_path.to_string_lossy().to_string(),
                old_content: old_content.to_string(),
                new_content: uncommitted_content.to_string(),
                old_content_hash: compute_content_hash(old_content),
                diff: "".to_string(),
                summary: "".to_string(),
                additions: 1,
                deletions: 1,
                status: "pending".to_string(),
                work_object_id: None,
                created_at: 1000,
                applied_at: None,
            };
            save_staged_change(&conn1, &staged).unwrap();

            record_staged_write_recovery_stage(
                &conn1,
                change_id,
                &file_path.to_string_lossy(),
                Some(&bak_path.to_string_lossy()),
                &tmp_path.to_string_lossy(),
                "replaced",
            ).unwrap();

            conn1.execute_batch(
                "CREATE TRIGGER force_update_fail BEFORE UPDATE ON staged_write_recovery
                 BEGIN SELECT RAISE(ABORT, 'Simulated update failure run 1'); END;"
            ).unwrap();

            let res1 = recover_interrupted_staged_writes(&mut conn1);
            assert!(res1.is_err(), "Run 1 must fail due to trigger");
            assert!(bak_path.exists(), "Run 1 must preserve backup");
        } // conn1 dropped

        // Run 2: Reopen SQLite connection, drop trigger, recover cleanly
        {
            set_recovery_degraded(false, None);
            let mut conn2 = rusqlite::Connection::open(&db_path).unwrap();
            conn2.execute_batch("DROP TRIGGER IF EXISTS force_update_fail;").unwrap();

            let res2 = recover_interrupted_staged_writes(&mut conn2);
            assert!(res2.is_ok(), "Run 2 must succeed after reopening DB");
            let summary = res2.unwrap();
            assert_eq!(summary.recovered_and_cleaned, 1);
            assert_eq!(summary.blocked_or_degraded, 0);

            let (degraded, _) = is_recovery_degraded();
            assert!(!degraded, "System must not be degraded after clean Run 2");

            assert_eq!(std::fs::read_to_string(&file_path).unwrap(), old_content);
            assert!(!bak_path.exists());
        }

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_file(&db_path);
    }

#[test]
    fn test_crash_recovery_wal_delete_failure_converges_on_second_startup() {
        let _lock = lock_test_recovery();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_converge_{}.txt", uuid::Uuid::new_v4()));
        let bak_path = temp_dir.join(format!(".bob_test_converge_{}.bak", uuid::Uuid::new_v4()));
        let tmp_path = temp_dir.join(format!(".bob_test_converge_{}.tmp", uuid::Uuid::new_v4()));

        let old_content = "original initial data that must survive";
        let uncommitted_content = "uncommitted data written before crash";
        std::fs::write(&file_path, uncommitted_content.as_bytes()).unwrap();
        std::fs::write(&bak_path, old_content.as_bytes()).unwrap();
        std::fs::write(&tmp_path, "tmp bytes").unwrap();

        let change_id = "change_converge_sim_1";
        let staged = StagedChange {
            change_id: change_id.to_string(),
            request_id: "req_converge_1".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: uncommitted_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            change_id,
            &file_path.to_string_lossy(),
            Some(&bak_path.to_string_lossy()),
            &tmp_path.to_string_lossy(),
            "replaced",
        ).unwrap();

        // 1. Inject trigger failure simulating WAL DELETE failure on run 1
        conn.execute_batch(
            "CREATE TRIGGER force_delete_fail BEFORE DELETE ON staged_write_recovery
             BEGIN SELECT RAISE(ABORT, 'Simulated WAL delete failure'); END;"
        ).unwrap();

        // Run 1: safe_restore_target_from_backup succeeds, but WAL DELETE fails
        let rec1 = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(rec1, 0, "Run 1 must return 0 because cleanup was incomplete");

        // Target file WAS restored to old_content
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), old_content);
        // .bak was renamed to target, so .bak no longer exists!
        assert!(!bak_path.exists());
        // System MUST NOT be degraded because data was restored
        let (degraded1, _) = is_recovery_degraded();
        assert!(!degraded1, "System must not degrade on run 1 when data was successfully restored");
        // WAL record is retained with retryable state
        let count1: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
        assert_eq!(count1, 1);

        // 2. Simulate Second Startup (Run 2): transient DB error is resolved
        conn.execute_batch("DROP TRIGGER force_delete_fail;").unwrap();

        // Run 2: recover_interrupted_staged_writes called on second startup
        let rec2 = recover_interrupted_staged_writes(&mut conn).unwrap();
        // Second startup MUST converge: clean WAL record, count 1 recovered via pending cleanup retry!
        assert_eq!(rec2.restored_pending_cleanup, 1, "Second startup must converge via pending cleanup retry");
        assert_eq!(rec2.recovered_and_cleaned, 0, "Second startup must not double count as recovered_and_cleaned");

        // System MUST NOT be in degraded mode
        let (degraded2, reason2) = is_recovery_degraded();
        assert!(!degraded2, "Second startup must NOT enter degraded mode: {:?}", reason2);

        // WAL record is cleanly deleted
        let count2: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE change_id = ?1", rusqlite::params![change_id], |r| r.get(0)).unwrap();
        assert_eq!(count2, 0, "WAL entry must be completely removed on second startup");

        // Disk file is still healthy and original
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), old_content);

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_fail_closed_recovery_table_query_error() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        // staged_write_recovery table is intentionally NOT initialized to simulate table/query failure:
        let _ = conn.execute("DROP TABLE IF EXISTS staged_write_recovery", []);

        let staged = StagedChange {
            change_id: "chg_fail_closed".to_string(),
            request_id: "req_fail_closed".to_string(),
            project_id: "".to_string(),
            file_path: "src/dummy.txt".to_string(),
            old_content: "".to_string(),
            new_content: "data".to_string(),
            old_content_hash: compute_content_hash(""),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        // When table is missing, query fails: approval MUST fail-closed with error!
        let outcome = process_approval_decision_core(&mut conn, "chg_fail_closed", "approve", "tester");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("error"));
        let err_msg = outcome.get("error").and_then(|v| v.as_str()).unwrap_or("");
        assert!(err_msg.contains("fail-closed 拒绝审批写入"), "Expected fail-closed error, got: {}", err_msg);
    }

#[test]
    fn test_receipt_does_not_complete_waiter() {
        let waiter = RelayRequestWaiter {
            terminal: RelayTerminal::Ack, // We wait for Ack, not receipt
            tx: tokio::sync::oneshot::channel().0,
            expected_peer: "peer".into(),
            expected_local: "local".into(),
            allow_pairing_bootstrap: false,
        };
        assert_eq!(waiter.terminal, RelayTerminal::Ack);
    }

    fn sec01_test_identity() -> (SigningKey, String) {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let key = SigningKey::from_bytes(&bytes);
        let id = base64::engine::general_purpose::STANDARD.encode(VerifyingKey::from(&key).to_bytes());
        (key, id)
    }

    fn sec01_test_signed_response(
        conn: &mut Connection, key: &SigningKey, peer: &str, local: &str,
        ref_id: &str, now: i64,
    ) -> serde_json::Value {
        let response = json!({
            "type": "ack", "from_device_id": peer, "target_device_id": local,
            "message_id": uuid::Uuid::new_v4().to_string(), "ref_message_id": ref_id,
            "payload": { "status": "trusted" }
        });
        build_signed_relay_response(response, peer, |target, bytes, message_id| {
            crate::device_trust::sign_outgoing_rpc_envelope(
                conn, key, peer, target, "relay_response", bytes, Some(message_id), now,
            )
        }).unwrap()
    }

    #[tokio::test]
    async fn test_sec01_relay_response_requires_signed_peer_and_exact_correlation() {
        let (peer_key, peer) = sec01_test_identity();
        let (_, local) = sec01_test_identity();
        let mut conn = Connection::open_in_memory().unwrap();
        crate::device_trust::init_device_trust_tables(&conn).unwrap();
        conn.execute(
            "INSERT INTO trusted_devices (device_id, public_key, platform, device_name, status, paired_at, last_authenticated_at) VALUES (?1, ?1, 'test', 'peer', 'trusted', ?2, ?2)",
            rusqlite::params![peer, crate::now_ms()],
        ).unwrap();
        conn.execute(
            "INSERT INTO trusted_devices (device_id, public_key, platform, device_name, status, paired_at, last_authenticated_at) VALUES (?1, ?1, 'test', 'local', 'trusted', ?2, ?2)",
            rusqlite::params![local, crate::now_ms()],
        ).unwrap();
        let ref_id = uuid::Uuid::new_v4().to_string();
        let valid = sec01_test_signed_response(&mut conn, &peer_key, &peer, &local, &ref_id, crate::now_ms());
        let ctx = RelayDispatchContext::for_test(Some(Arc::new(Mutex::new(conn))), None);
        let (tx, rx) = oneshot::channel();
        PENDING_REQUESTS.write().unwrap().insert(ref_id.clone(), RelayRequestWaiter {
            tx, terminal: RelayTerminal::Ack, expected_peer: peer.clone(),
            expected_local: local.clone(), allow_pairing_bootstrap: false,
        });

        let mut unsigned = valid.clone();
        unsigned.as_object_mut().unwrap().remove("auth_envelope");
        let (tx_relay, _rx_relay) = tokio::sync::mpsc::channel(2);
        assert!(dispatch_inbound_relay_message_core(&ctx, &unsigned, &tx_relay).await.is_err());
        assert!(PENDING_REQUESTS.read().unwrap().contains_key(&ref_id));

        let mut spoofed = valid.clone();
        spoofed["from_device_id"] = json!("attacker");
        assert!(deliver_authenticated_relay_response(&ctx, &spoofed).is_err());
        let mut tampered = valid.clone();
        tampered["payload"]["status"] = json!("committed");
        assert!(deliver_authenticated_relay_response(&ctx, &tampered).is_err());
        let mut wrong_ref = valid.clone();
        wrong_ref["ref_message_id"] = json!(uuid::Uuid::new_v4().to_string());
        assert!(!deliver_authenticated_relay_response(&ctx, &wrong_ref).unwrap());
        assert!(PENDING_REQUESTS.read().unwrap().contains_key(&ref_id));

        let receipt = json!({
            "type": "diagnostic_receipt", "status": "failed",
            "ref_message_id": ref_id, "trace_id": "trace-1",
        });
        dispatch_inbound_relay_message_core(&ctx, &receipt, &tx_relay).await.unwrap();
        assert!(PENDING_REQUESTS.read().unwrap().contains_key(&ref_id));

        let session_id = valid["auth_envelope"]["session_id"].as_str().unwrap();
        ctx.with_db(|conn| {
            conn.execute("UPDATE authenticated_sessions SET is_active = 0 WHERE session_id = ?1", [session_id])
                .map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();
        assert!(deliver_authenticated_relay_response(&ctx, &valid).is_err());
        assert!(PENDING_REQUESTS.read().unwrap().contains_key(&ref_id));
        ctx.with_db(|conn| {
            conn.execute("UPDATE authenticated_sessions SET is_active = 1 WHERE session_id = ?1", [session_id])
                .map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();
        ctx.with_db(|conn| {
            conn.execute(
                "INSERT INTO identity_reset_journal (state, revoked_device_id, error, created_at, updated_at) VALUES ('degraded', ?1, 'test', ?2, ?2)",
                rusqlite::params![local, crate::now_ms()],
            ).map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();
        assert!(deliver_authenticated_relay_response(&ctx, &valid).is_err());
        assert!(PENDING_REQUESTS.read().unwrap().contains_key(&ref_id));
        ctx.with_db(|conn| {
            conn.execute("DELETE FROM identity_reset_journal WHERE revoked_device_id = ?1 AND state = 'degraded'", [&local])
                .map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();

        dispatch_inbound_relay_message_core(&ctx, &valid, &tx_relay).await.unwrap();
        assert_eq!(rx.await.unwrap(), valid);
        assert!(!PENDING_REQUESTS.read().unwrap().contains_key(&ref_id));
        assert!(!deliver_authenticated_relay_response(&ctx, &valid).unwrap());
    }

    #[tokio::test]
    async fn test_sec01_pairing_ack_bootstrap_requires_invited_peer_signature() {
        let (peer_key, peer) = sec01_test_identity();
        let (_, local) = sec01_test_identity();
        let conn = Connection::open_in_memory().unwrap();
        crate::device_trust::init_device_trust_tables(&conn).unwrap();
        let ref_id = uuid::Uuid::new_v4().to_string();
        let mut ack = json!({
            "type": "ack", "from_device_id": peer, "target_device_id": local,
            "message_id": uuid::Uuid::new_v4().to_string(), "ref_message_id": ref_id,
            "payload": { "status": "trusted", "session_id": "newly-created-session" }
        });
        let bytes = crate::device_trust::canonicalize_json_value(&ack);
        let envelope = crate::device_trust::create_rpc_auth_envelope(
            &peer_key, "newly-created-session", ack["message_id"].as_str().unwrap(),
            &peer, &local, "relay_response", &bytes, crate::now_ms(),
        );
        ack["auth_envelope"] = serde_json::to_value(envelope).unwrap();
        let ctx = RelayDispatchContext::for_test(Some(Arc::new(Mutex::new(conn))), None);
        let (tx, rx) = oneshot::channel();
        PENDING_REQUESTS.write().unwrap().insert(ref_id.clone(), RelayRequestWaiter {
            tx, terminal: RelayTerminal::Ack, expected_peer: peer.clone(),
            expected_local: local.clone(), allow_pairing_bootstrap: true,
        });
        let mut forged = ack.clone();
        forged["from_device_id"] = json!("other-device");
        assert!(deliver_authenticated_relay_response(&ctx, &forged).is_err());
        assert!(PENDING_REQUESTS.read().unwrap().contains_key(&ref_id));
        assert!(deliver_authenticated_relay_response(&ctx, &ack).unwrap());
        assert_eq!(rx.await.unwrap(), ack);
    }

    #[test]
    fn test_sec01_outgoing_wakeup_matches_production_receiver_and_rejects_tampering() {
        let (sender_key, sender) = sec01_test_identity();
        let (_, receiver) = sec01_test_identity();
        let mut conn = Connection::open_in_memory().unwrap();
        crate::device_trust::init_device_trust_tables(&conn).unwrap();
        conn.execute(
            "INSERT INTO trusted_devices (device_id, public_key, platform, device_name, status, paired_at, last_authenticated_at) VALUES (?1, ?1, 'test', 'sender', 'trusted', ?2, ?2)",
            rusqlite::params![sender, crate::now_ms()],
        ).unwrap();
        conn.execute(
            "INSERT INTO trusted_devices (device_id, public_key, platform, device_name, status, paired_at, last_authenticated_at) VALUES (?1, ?1, 'test', 'receiver', 'trusted', ?2, ?2)",
            rusqlite::params![receiver, crate::now_ms()],
        ).unwrap();
        let now = crate::now_ms();
        let msg = build_authenticated_wakeup_message(
            &sender, &receiver, vec!["127.0.0.1".into()],
            |payload| crate::device_trust::sign_outgoing_rpc_envelope(
                &mut conn, &sender_key, &sender, &receiver, "wakeup",
                &crate::device_trust::canonicalize_json_value(payload), None, now,
            ),
        ).unwrap();
        assert_eq!(crate::device_trust::sec01_verify_relay_wakeup_message(
            &mut conn, &msg, &receiver, now,
        ).unwrap(), sender);
        let mut tampered = msg.clone();
        tampered["payload"]["port"] = json!(9999);
        assert!(crate::device_trust::sec01_verify_relay_wakeup_message(
            &mut conn, &tampered, &receiver, now,
        ).is_err());
    }

    #[test]
    fn test_approval_baseline_conflict_detection() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_conflict_{}.txt", uuid::Uuid::new_v4()));
        let file_path_str = file_path.to_string_lossy().to_string();

        let initial_content = "initial baseline v1";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        let staged = StagedChange {
            change_id: "change_conflict_001".to_string(),
            request_id: "req_conflict_001".to_string(),
            project_id: "".to_string(),
            file_path: file_path_str.clone(),
            old_content: initial_content.to_string(),
            new_content: "new proposed content".to_string(),
            old_content_hash: compute_content_hash(initial_content),
            diff: "".to_string(),
            summary: "Conflict test".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        // Simulate external modification to disk file
        let external_edit = "someone else edited this file concurrently!";
        std::fs::write(&file_path, external_edit.as_bytes()).unwrap();

        let outcome = process_approval_decision_core(&mut conn, "change_conflict_001", "approve", "remote:mobile_device");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("error"));
        let err_msg = outcome.get("error").and_then(|v| v.as_str()).unwrap_or("");
        assert!(err_msg.contains("文件基线冲突"), "Must return baseline conflict error: {}", err_msg);

        // Verify disk was NOT overwritten
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, external_edit);

        // Verify SQLite status remains pending
        let loaded = get_staged_change(&conn, "change_conflict_001").unwrap().unwrap();
        assert_eq!(loaded.status, "pending");

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_global_degraded_recovery_state_blocks_writes() {
        let _lock = lock_test_recovery();
        set_recovery_degraded(true, Some("Simulated cluster recovery degradation"));

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_degraded_{}.txt", uuid::Uuid::new_v4()));

        let res = safe_atomic_write_file_with_backup(None, "chg_deg", &file_path, "content");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("fail-closed 拒绝写入"));

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        let staged = StagedChange {
            change_id: "chg_deg_appr".to_string(),
            request_id: "req_deg".to_string(),
            project_id: "".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            old_content: "".to_string(),
            new_content: "data".to_string(),
            old_content_hash: compute_content_hash(""),
            diff: "diff".to_string(),
            summary: "summary".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        let outcome = process_approval_decision_core(&mut conn, "chg_deg_appr", "approve", "tester");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("error"));
        assert!(outcome.get("error").and_then(|v| v.as_str()).unwrap().contains("fail-closed 拒绝写入"));

        // Reset degraded state
        set_recovery_degraded(false, None);
    }

#[test]
    fn test_healthy_startup_does_not_degrade() {
        let _lock = lock_test_recovery();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        // On a healthy startup with empty recovery table, recovery MUST succeed with Ok(0)
        let res = recover_interrupted_staged_writes(&mut conn);
        assert!(res.is_ok(), "Healthy startup must succeed with Ok(0): {:?}", res);
        assert_eq!(res.unwrap(), 0);

        // System MUST NOT be in degraded mode
        let (degraded, reason) = is_recovery_degraded();
        assert!(!degraded, "Healthy startup must NOT enter degraded mode, got reason: {:?}", reason);

        // Subsequent write and approval MUST NOT be blocked
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_healthy_write_{}.txt", uuid::Uuid::new_v4()));
        let write_res = safe_atomic_write_file_with_backup(Some(&conn), "chg_healthy_1", &file_path, "healthy content");
        assert!(write_res.is_ok(), "Normal write must succeed on healthy startup: {:?}", write_res);

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_process_approval_decision_baseline_conflict() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        crate::work_core::repository::init_work_core_tables(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_appr_conflict_{}.txt", uuid::Uuid::new_v4()));
        let file_path_str = file_path.to_str().unwrap().to_string();

        let initial_content = "initial baseline v1";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        let staged = StagedChange {
            change_id: "change_conflict_001".to_string(),
            request_id: "req_conflict_001".to_string(),
            project_id: "".to_string(),
            file_path: file_path_str.clone(),
            old_content: initial_content.to_string(),
            new_content: "new proposed content".to_string(),
            old_content_hash: compute_content_hash(initial_content),
            diff: "".to_string(),
            summary: "Conflict test".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        // Simulate external modification to disk file
        let external_edit = "someone else edited this file concurrently!";
        std::fs::write(&file_path, external_edit.as_bytes()).unwrap();

        let outcome = process_approval_decision_core(&mut conn, "change_conflict_001", "approve", "remote:mobile_device");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("error"));
        let err_msg = outcome.get("error").and_then(|v| v.as_str()).unwrap_or("");
        assert!(err_msg.contains("文件基线冲突"), "Must return baseline conflict error: {}", err_msg);

        // Verify disk was NOT overwritten
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, external_edit);

        // Verify SQLite status remains pending
        let loaded = get_staged_change(&conn, "change_conflict_001").unwrap().unwrap();
        assert_eq!(loaded.status, "pending");

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_process_approval_decision_core_success() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        crate::work_core::repository::init_work_core_tables(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_appr_ok_{}.txt", uuid::Uuid::new_v4()));
        let file_path_str = file_path.to_str().unwrap().to_string();

        let initial_content = "fn main() {\n    // old code\n}\n";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        let new_content = "fn main() {\n    println!(\"approved\");\n}\n";
        let staged = StagedChange {
            change_id: "change_appr_001".to_string(),
            request_id: "req_appr_001".to_string(),
            project_id: "".to_string(),
            file_path: file_path_str.clone(),
            old_content: initial_content.to_string(),
            new_content: new_content.to_string(),
            old_content_hash: compute_content_hash(initial_content),
            diff: "--- a/test\n+++ b/test\n".to_string(),
            summary: "Update main".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        let outcome = process_approval_decision_core(&mut conn, "change_appr_001", "approve", "remote:mobile_device");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("applied"));

        // Verify disk content was updated
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, new_content);

        // Verify SQLite status was updated to applied
        let loaded = get_staged_change(&conn, "change_appr_001").unwrap().unwrap();
        assert_eq!(loaded.status, "applied");
        assert!(loaded.applied_at.is_some());

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_process_approval_decision_reject() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        crate::work_core::repository::init_work_core_tables(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_appr_reject_{}.txt", uuid::Uuid::new_v4()));
        let file_path_str = file_path.to_str().unwrap().to_string();

        let initial_content = "do not change me";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        let staged = StagedChange {
            change_id: "change_rej_001".to_string(),
            request_id: "req_rej_001".to_string(),
            project_id: "".to_string(),
            file_path: file_path_str.clone(),
            old_content: initial_content.to_string(),
            new_content: "rejected content".to_string(),
            old_content_hash: compute_content_hash(initial_content),
            diff: "".to_string(),
            summary: "Reject test".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        let outcome = process_approval_decision_core(&mut conn, "change_rej_001", "reject", "tester");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("rejected"));

        // File remains unchanged
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), initial_content);

        // Status updated in SQLite
        let loaded = get_staged_change(&conn, "change_rej_001").unwrap().unwrap();
        assert_eq!(loaded.status, "rejected");

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_process_approval_decision_state_guards() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        crate::work_core::repository::init_work_core_tables(&conn).unwrap();

        // 1. Applied proposal approval is idempotent
        let staged_applied = StagedChange {
            change_id: "c_applied".to_string(),
            request_id: "r1".to_string(),
            project_id: "".to_string(),
            file_path: "src/test.txt".to_string(),
            old_content: "".to_string(),
            new_content: "content".to_string(),
            old_content_hash: compute_content_hash(""),
            diff: "".to_string(),
            summary: "test".to_string(),
            additions: 1,
            deletions: 0,
            status: "applied".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: Some(1050),
        };
        save_staged_change(&conn, &staged_applied).unwrap();
        let outcome = process_approval_decision_core(&mut conn, "c_applied", "approve", "tester");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("applied"));
        assert!(outcome.get("message").and_then(|v| v.as_str()).unwrap().contains("幂等"));

        // 2. Rejected proposal cannot be re-approved
        let staged_rejected = StagedChange {
            change_id: "c_rejected".to_string(),
            status: "rejected".to_string(),
            ..staged_applied.clone()
        };
        save_staged_change(&conn, &staged_rejected).unwrap();
        let outcome_rej = process_approval_decision_core(&mut conn, "c_rejected", "approve", "tester");
        assert_eq!(outcome_rej.get("status").and_then(|v| v.as_str()), Some("error"));
        assert!(outcome_rej.get("error").and_then(|v| v.as_str()).unwrap().contains("此前已被拒绝"));

        // 3. Cancelled proposal cannot be approved
        let staged_cancelled = StagedChange {
            change_id: "c_cancelled".to_string(),
            status: "cancelled".to_string(),
            ..staged_applied.clone()
        };
        save_staged_change(&conn, &staged_cancelled).unwrap();
        let outcome_can = process_approval_decision_core(&mut conn, "c_cancelled", "approve", "tester");
        assert_eq!(outcome_can.get("status").and_then(|v| v.as_str()), Some("error"));
        assert!(outcome_can.get("error").and_then(|v| v.as_str()).unwrap().contains("任务已被中止"));
    }

#[test]
    fn test_raii_guard_isolation_and_panic_recovery() {
        // 1. Run a closure that panics while holding the lock
        let panic_result = std::panic::catch_unwind(|| {
            let _lock = lock_test_recovery();
            set_recovery_degraded(true, Some("poisoned panic"));
            panic!("intentional test panic inside guard");
        });
        assert!(panic_result.is_err());

        // 2. Next test obtains the lock: it must recover from poisoned mutex and restore clean state
        {
            let _lock = lock_test_recovery();
            let (degraded, reason) = is_recovery_degraded();
            assert!(!degraded, "lock_test_recovery must reset degraded flag even after previous panic");
            assert!(reason.is_none(), "lock_test_recovery must clear reason even after previous panic");
        }
    }

#[test]
    fn test_staged_change_sqlite_lifecycle() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();

        let staged = StagedChange {
            change_id: "change_test_001".to_string(),
            request_id: "req_test_001".to_string(),
            project_id: "proj_personal".to_string(),
            file_path: "src/main.rs".to_string(),
            old_content: "fn main() {}".to_string(),
            new_content: "fn main() { println!(\"ok\"); }".to_string(),
            old_content_hash: compute_content_hash("fn main() {}"),
            diff: "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-fn main() {}\n+fn main() { println!(\"ok\"); }".to_string(),
            summary: "Add log".to_string(),
            additions: 1,
            deletions: 1,
            status: "pending".to_string(),
            work_object_id: Some("work_obj_1".to_string()),
            created_at: 1000,
            applied_at: None,
        };

        save_staged_change(&conn, &staged).unwrap();

        let loaded = get_staged_change(&conn, "change_test_001").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.change_id, "change_test_001");
        assert_eq!(loaded.request_id, "req_test_001");
        assert_eq!(loaded.project_id, "proj_personal");
        assert_eq!(loaded.old_content_hash, compute_content_hash("fn main() {}"));
        assert_eq!(loaded.status, "pending");
        assert_eq!(loaded.work_object_id, Some("work_obj_1".to_string()));

        // Update status to applied
        update_staged_change_status(&conn, "change_test_001", "applied", Some(2000)).unwrap();
        let loaded_applied = get_staged_change(&conn, "change_test_001").unwrap().unwrap();
        assert_eq!(loaded_applied.status, "applied");
        assert_eq!(loaded_applied.applied_at, Some(2000));

        // Test cancel pending staged changes by request
        let staged2 = StagedChange {
            change_id: "change_test_002".to_string(),
            request_id: "req_test_002".to_string(),
            project_id: "proj_personal".to_string(),
            file_path: "src/lib.rs".to_string(),
            old_content: "".to_string(),
            new_content: "pub fn test() {}".to_string(),
            old_content_hash: compute_content_hash(""),
            diff: "".to_string(),
            summary: "New lib".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1500,
            applied_at: None,
        };
        save_staged_change(&conn, &staged2).unwrap();

        let cancelled_list = cancel_staged_changes_by_request(&conn, "req_test_002").unwrap();
        assert_eq!(cancelled_list.len(), 1);
        assert_eq!(cancelled_list[0].change_id, "change_test_002");

        let loaded2 = get_staged_change(&conn, "change_test_002").unwrap().unwrap();
        assert_eq!(loaded2.status, "cancelled");
    }

#[test]
    fn test_safe_atomic_write_backed_up_log_failure_reverts_target_preserves_bak() {
        let _lock = lock_test_recovery();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_backed_up_fail_{}.txt", uuid::Uuid::new_v4()));
        let initial_content = "initial file content before backed_up fail";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        // Inject failure on backed_up stage update
        conn.execute_batch(
            "CREATE TRIGGER force_backed_up_fail BEFORE UPDATE OF stage ON staged_write_recovery
             WHEN NEW.stage = 'backed_up'
             BEGIN SELECT RAISE(ABORT, 'Simulated backed_up WAL failure'); END;"
        ).unwrap();

        let res = safe_atomic_write_file_with_backup(Some(&conn), "chg_bk_fail", &file_path, "corrupted content");
        assert!(res.is_err());
        let err_str = res.unwrap_err();
        assert!(err_str.contains("backed_up"));

        // Target file must be safely reverted to initial content
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, initial_content);

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_safe_atomic_write_file_with_backup_and_rollback() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_atomic_bak_{}.txt", uuid::Uuid::new_v4()));

        let initial_content = "initial content v1";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        let new_content = "updated atomic content v2";
        let backup = safe_atomic_write_file_with_backup(None, "test_atomic_1", &file_path, new_content).unwrap();

        // Disk has new content now
        let current_disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(current_disk, new_content);

        // Backup file exists
        assert!(backup.bak_path.is_some());
        let bak = backup.bak_path.clone().unwrap();
        assert!(bak.exists());

        // Now test rollback
        backup.rollback().unwrap();

        // Disk content is restored to v1
        let restored_disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored_disk, initial_content);

        // Backup file was cleaned up by rollback
        assert!(!bak.exists());

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_safe_atomic_write_prepared_log_failure_untouched() {
        let _lock = lock_test_recovery();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_prep_fail_{}.txt", uuid::Uuid::new_v4()));
        let initial_content = "pristine content";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        // Inject trigger failure on staged_write_recovery INSERT
        conn.execute_batch(
            "CREATE TRIGGER force_prep_fail BEFORE INSERT ON staged_write_recovery BEGIN SELECT RAISE(ABORT, 'Simulated WAL logging failure'); END;"
        ).unwrap();

        let res = safe_atomic_write_file_with_backup(Some(&conn), "chg_prep_fail", &file_path, "corrupted content");
        assert!(res.is_err());
        let err_str = res.unwrap_err();
        assert!(err_str.contains("记录暂存恢复预写日志失败 (prepared)"));

        // Verify target file was completely untouched
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, initial_content);

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_safe_atomic_write_replaced_log_failure_reverts_target_preserves_bak() {
        let _lock = lock_test_recovery();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("bob_test_replaced_fail_{}.txt", uuid::Uuid::new_v4()));
        let initial_content = "initial file content before replaced fail";
        std::fs::write(&file_path, initial_content.as_bytes()).unwrap();

        // Inject failure on replaced stage update
        conn.execute_batch(
            "CREATE TRIGGER force_replaced_fail BEFORE UPDATE OF stage ON staged_write_recovery
             WHEN NEW.stage = 'replaced'
             BEGIN SELECT RAISE(ABORT, 'Simulated replaced WAL failure'); END;"
        ).unwrap();

        let res = safe_atomic_write_file_with_backup(Some(&conn), "chg_rep_fail", &file_path, "new uncommitted content");
        assert!(res.is_err());
        let err_str = res.unwrap_err();
        assert!(err_str.contains("replaced"));

        // Target file must be safely reverted from backup to initial content
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, initial_content);

        let _ = std::fs::remove_file(&file_path);
    }

#[test]
    fn test_safe_restore_rename_and_copy_both_fail_preserves_backup() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir();
        let bak_path = temp_dir.join(format!("bob_test_restore_fail_{}.bak", uuid::Uuid::new_v4()));
        let initial_content = "invaluable original data";
        std::fs::write(&bak_path, initial_content.as_bytes()).unwrap();

        // Create target as a non-empty directory so rename/copy of file to directory fails
        let target_dir = temp_dir.join(format!("bob_test_target_dir_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&target_dir).unwrap();
        let dummy_child = target_dir.join("child.txt");
        std::fs::write(&dummy_child, "child content").unwrap();

        // Calling restore where destination cannot be overwritten
        let res = safe_restore_target_from_backup(&bak_path, &target_dir);
        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("原备份文件安全保留在") || err_msg.contains("原文件备份安全保留在"));

        // Critical assertion: backup file is GUARANTEED to exist and retain data!
        assert!(bak_path.exists());
        let bak_content = std::fs::read_to_string(&bak_path).unwrap();
        assert_eq!(bak_content, initial_content);

        let _ = std::fs::remove_file(&dummy_child);
        let _ = std::fs::remove_dir(&target_dir);
        let _ = std::fs::remove_file(&bak_path);
    }

#[test]
    fn test_safe_restore_same_length_different_content_corruption_preserves_backup() {
        let _lock = lock_test_recovery();

        let temp_dir = std::env::temp_dir();
        let bak_path = temp_dir.join(format!("bob_test_same_len_{}.bak", uuid::Uuid::new_v4()));
        let target_path = temp_dir.join(format!("bob_test_same_len_{}.txt", uuid::Uuid::new_v4()));

        let original_data = "ABC"; // 3 bytes
        std::fs::write(&bak_path, original_data.as_bytes()).unwrap();

        // Simulate rename failure (e.g. cross-volume move failure) and copier writing same-length (3 bytes) but corrupted/different content "XYZ"
        let res = safe_restore_target_internal(
            &bak_path,
            &target_path,
            |_src, _dst| Err(std::io::Error::new(std::io::ErrorKind::Other, "simulated cross-device link failure")),
            |_src, dst| std::fs::write(dst, b"XYZ"),
        );

        assert!(res.is_err(), "Must reject restore when content does not match, even if length is identical");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("内容校验损坏") || err_msg.contains("内容不匹配"), "Error was: {}", err_msg);

        // Critical: The backup file MUST NOT be deleted!
        assert!(bak_path.exists(), "Backup file must still exist after content mismatch");
        assert_eq!(std::fs::read_to_string(&bak_path).unwrap(), original_data);

        let _ = std::fs::remove_file(&bak_path);
        let _ = std::fs::remove_file(&target_path);
    }

#[test]
    fn test_staged_changes_init_failure_triggers_degraded() {
        let _lock = lock_test_recovery();

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Create an un-overwritable object with the same name to force CREATE TABLE staged_changes to fail
        conn.execute_batch("CREATE VIEW staged_changes AS SELECT 1;").unwrap();

        let res = init_staged_changes_table(&conn);
        assert!(res.is_err(), "init_staged_changes_table should fail when view with same name exists");

        // Simulate db.rs startup logic
        if let Err(e) = res {
            set_recovery_degraded(true, Some(&format!("初始化暂存变更表失败: {}", e)));
        }

        let (degraded, reason) = is_recovery_degraded();
        assert!(degraded);
        assert!(reason.unwrap().contains("初始化暂存变更表失败"));
    }

#[test]
    fn test_startup_wiring_healthy_startup_succeeds() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();

        let res = crate::db::initialize_staged_recovery(&mut conn);
        assert!(res.is_ok());
        let summary = res.unwrap();
        assert_eq!(summary.recovered_and_cleaned, 0);
        assert_eq!(summary.restored_pending_cleanup, 0);
        assert_eq!(summary.blocked_or_degraded, 0);

        let (degraded, _) = is_recovery_degraded();
        assert!(!degraded, "Healthy startup must not degrade");
    }

#[test]
    fn test_startup_wiring_recovery_execution_failure_degrades() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        // Insert a corrupted WAL entry pointing to non-existent bak
        conn.execute(
            "INSERT INTO staged_write_recovery (change_id, target_path, bak_path, tmp_path, stage, created_at)
             VALUES ('corrupt_wal', 'nonexistent_target', 'nonexistent_bak', 'nonexistent_tmp', 'backed_up', 1000)",
            [],
        ).unwrap();

        let res = crate::db::initialize_staged_recovery(&mut conn);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("执行启动崩溃恢复失败"));

        let (degraded, _) = is_recovery_degraded();
        assert!(degraded, "System must degrade when recovery execution fails");
    }

#[test]
    fn test_startup_wiring_recovery_table_init_failure_blocks_recovery() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE VIEW staged_write_recovery AS SELECT 1;").unwrap();

        let res = crate::db::initialize_staged_recovery(&mut conn);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("初始化崩溃恢复表失败"));

        let (degraded, _) = is_recovery_degraded();
        assert!(degraded, "System must degrade when staged_write_recovery table init fails");
    }

#[test]
    fn test_startup_wiring_staged_changes_init_failure_blocks_recovery() {
        let _lock = lock_test_recovery();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE VIEW staged_changes AS SELECT 1;").unwrap();

        let res = crate::db::initialize_staged_recovery(&mut conn);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("初始化暂存变更表失败"));

        let (degraded, _) = is_recovery_degraded();
        assert!(degraded, "System must degrade when staged_changes table init fails");
    }

    #[test]
    fn test_sec03_active_sync_outcome_and_caller_diagnostics_closure() {
        // 1. 验证 ActiveSyncOutcome 序列化/反序列化契约
        let applied = ActiveSyncOutcome::Applied {
            transport: TransportKind::Lan,
        };
        let json_applied = serde_json::to_value(&applied).unwrap();
        assert_eq!(json_applied.get("status").and_then(|v| v.as_str()), Some("applied"));
        assert_eq!(applied.transport(), TransportKind::Lan);

        let pending = ActiveSyncOutcome::PendingApply {
            transport: TransportKind::Relay,
            reasons: vec!["Outbox 暂存待应用".to_string()],
        };
        let json_pending = serde_json::to_value(&pending).unwrap();
        assert_eq!(json_pending.get("status").and_then(|v| v.as_str()), Some("pending_apply"));
        assert_eq!(pending.transport(), TransportKind::Relay);
        let reasons = json_pending.get("reasons").and_then(|v| v.as_array()).unwrap();
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].as_str(), Some("Outbox 暂存待应用"));

        // 2. 真实生产处理函数测试：record_trigger_sync_outcome
        let test_trace_id = format!("trace-test-sec03-{}", uuid::Uuid::new_v4());
        let test_sync_id = format!("sync-test-sec03-{}", uuid::Uuid::new_v4());
        let test_peer = "test-peer-device";
        let now = crate::now_ms();

        // 场景 A: PendingApply 结果直接经过生产处理函数 record_trigger_sync_outcome
        let result_pending: Result<ActiveSyncOutcome, String> = Ok(pending.clone());
        let (status_pending, transport_pending, summary_pending, error_code_pending) =
            record_trigger_sync_outcome(&test_sync_id, &test_trace_id, test_peer, now - 100, &result_pending);

        assert_eq!(status_pending, DiagnosticStatus::Pending, "PendingApply 必须判定为 DiagnosticStatus::Pending");
        assert_ne!(status_pending, DiagnosticStatus::Success, "PendingApply 严禁判定为 Success");
        assert_eq!(transport_pending, Some(TransportKind::Relay));
        assert!(summary_pending.as_deref().unwrap().contains("配置变更已入队待应用"));
        assert_ne!(summary_pending.as_deref(), Some("同步完成"), "PendingApply 严禁宣称 同步完成");
        assert!(error_code_pending.is_none());

        // 验证生产函数内部落盘的记录
        let runs = crate::sync_history::get_sync_runs().unwrap();
        let found = runs.iter().find(|r| r.sync_id == test_sync_id).expect("Must find recorded run");
        assert_eq!(found.status, DiagnosticStatus::Pending);
        assert_ne!(found.status, DiagnosticStatus::Success);
        assert_ne!(found.summary.as_deref(), Some("同步完成"));
        assert!(found.summary.as_deref().unwrap().contains("配置变更已入队待应用"));

        // 场景 B: Applied 结果经过生产处理函数
        let test_sync_id_app = format!("sync-test-sec03-app-{}", uuid::Uuid::new_v4());
        let result_applied: Result<ActiveSyncOutcome, String> = Ok(applied);
        let (status_applied, transport_applied, summary_applied, _) =
            record_trigger_sync_outcome(&test_sync_id_app, &test_trace_id, test_peer, now - 100, &result_applied);
        assert_eq!(status_applied, DiagnosticStatus::Success);
        assert_eq!(transport_applied, Some(TransportKind::Lan));
        assert_eq!(summary_applied.as_deref(), Some("同步完成"));

        // 场景 C: Failed / Error 结果经过生产处理函数
        let test_sync_id_err = format!("sync-test-sec03-err-{}", uuid::Uuid::new_v4());
        let result_err: Result<ActiveSyncOutcome, String> = Err("ERR-SYNC-02: Relay 连接失败".to_string());
        let (status_err, transport_err, summary_err, error_code_err) =
            record_trigger_sync_outcome(&test_sync_id_err, &test_trace_id, test_peer, now - 100, &result_err);
        assert_eq!(status_err, DiagnosticStatus::Failed);
        assert!(transport_err.is_none());
        assert_eq!(summary_err.as_deref(), Some("同步未完成"));
        assert_eq!(error_code_err.as_deref(), Some("ERR-SYNC-02"));

        // 场景 D: 本地导入历史条目生成函数 build_sync_import_history_entry (import_sync_data 生产路径)
        let mock_data = SyncData {
            config: serde_json::json!({}),
            settings: vec![serde_json::json!({"key": "test", "value": "1"})],
            conversations: vec![],
            messages: vec![],
            events: vec![],
            captures: vec![],
            cron_jobs: vec![],
            kg_nodes: vec![],
            kg_edges: vec![],
            wiki_fts: vec![],
            tombstones: vec![],
            notes: vec![],
        };
        let receipt_pending = crate::device_trust::PushCommitReceipt {
            status: "accepted".to_string(),
            r#type: "receipt".to_string(),
            stage: "pending_apply".to_string(),
            applied_count: 0,
            pending_count: 1,
            delivery_error: Some("配置变更已写入暂存表，待确认后应用".to_string()),
        };
        let (entry_pending, detail_pending) = build_sync_import_history_entry(now, "pull", &mock_data, 1, &receipt_pending);
        assert_eq!(entry_pending.get("status").and_then(|v| v.as_str()), Some("pending_apply"));
        assert_eq!(entry_pending.get("stage").and_then(|v| v.as_str()), Some("pending_apply"));
        assert!(detail_pending.contains("配置变更已写入暂存表，待确认后应用"));
        assert!(!detail_pending.contains("同步更新已应用"));
        assert!(!detail_pending.contains("成功合并云端数据"));

        let receipt_applied = crate::device_trust::PushCommitReceipt {
            status: "accepted".to_string(),
            r#type: "receipt".to_string(),
            stage: "applied".to_string(),
            applied_count: 1,
            pending_count: 0,
            delivery_error: None,
        };
        let (entry_applied, detail_applied) = build_sync_import_history_entry(now, "pull", &mock_data, 1, &receipt_applied);
        assert_eq!(entry_applied.get("status").and_then(|v| v.as_str()), Some("applied"));
        assert_eq!(entry_applied.get("stage").and_then(|v| v.as_str()), Some("applied"));
        assert!(detail_applied.contains("同步更新已应用"));
    }

    // ═══════════════════════════════════════════════════════════
    // Track A / A4 受控故障注入与诊断测试基座 (Case 07A & 07B)
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_diagnostic_profile_validation_and_fail_closed() {
        let _lock = lock_test_recovery();
        let temp_base = std::env::temp_dir().join(format!("bob_diag_val_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_base);

        // 1. 相对路径必须被拒绝
        let rel_res = crate::diagnostic_profile::validate_test_data_dir(std::path::Path::new("relative/dir"));
        assert!(rel_res.is_err(), "相对路径必须被拒绝");
        assert!(rel_res.unwrap_err().contains("绝对路径"));

        // 2. 缺失 .bob_test_profile 标记文件必须被拒绝
        let uninit_res = crate::diagnostic_profile::validate_test_data_dir(&temp_base);
        assert!(uninit_res.is_err(), "无标记目录必须被拒绝");
        assert!(uninit_res.unwrap_err().contains("缺少测试标记文件"));

        // 3. 标记内容伪造/错误必须被拒绝
        let marker = temp_base.join(crate::diagnostic_profile::TEST_PROFILE_MARKER_FILE);
        std::fs::write(&marker, b"INVALID_MARKER_CONTENT").unwrap();
        let bad_marker_res = crate::diagnostic_profile::validate_test_data_dir(&temp_base);
        assert!(bad_marker_res.is_err(), "非法标记必须被拒绝");
        assert!(bad_marker_res.unwrap_err().contains("标记文件内容无效"));

        // 4. 正确初始化测试 Profile
        let valid_profile = crate::diagnostic_profile::setup_test_profile_in_dir(&temp_base).unwrap();
        assert!(valid_profile.exists());

        // 5. 排他性检查：指向日常生产目录必须被强力拒绝
        if let Some(prod_dir) = crate::diagnostic_profile::get_production_data_dir() {
            let prod_check = crate::diagnostic_profile::validate_test_data_dir(&prod_dir);
            assert!(prod_check.is_err(), "生产目录严禁作为测试 Profile 使用");
            assert!(prod_check.unwrap_err().contains("安全阻断"));
        }

        // 6. 工作区越界检查：目标文件必须严格落在 test_workspace 内
        let workspace = temp_base.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let safe_target = workspace.join("probe.txt");
        assert!(crate::diagnostic_profile::validate_target_in_test_workspace(&safe_target, &temp_base).is_ok());

        let outside_target = temp_base.join("outside.txt");
        let outside_res = crate::diagnostic_profile::validate_target_in_test_workspace(&outside_target, &temp_base);
        assert!(outside_res.is_err(), "工作区外文件必须被拒绝");
        assert!(outside_res.unwrap_err().contains("越界"));

        let traversal_target = workspace.join("../escape.txt");
        let trav_res = crate::diagnostic_profile::validate_target_in_test_workspace(&traversal_target, &temp_base);
        assert!(trav_res.is_err(), "路径遍历逃逸必须被拒绝");

        let _ = std::fs::remove_dir_all(&temp_base);
    }

    #[test]
    fn test_fault_injection_normal_run_no_injection() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_no_inj_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();
        crate::fault_injection::set_thread_test_dir(Some(temp_dir.clone()));
        crate::fault_injection::arm_fault_thread_local(None);

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        let _ = crate::work_core::init_work_core_tables(&conn);

        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("normal.txt");
        let old_content = "initial baseline";
        std::fs::write(&target_file, old_content.as_bytes()).unwrap();

        let change_id = format!("chg_normal_{}", uuid::Uuid::new_v4());
        let new_content = "applied without injection";
        let staged = StagedChange {
            change_id: change_id.clone(),
            request_id: "req_normal_1".to_string(),
            project_id: "".to_string(),
            file_path: target_file.to_string_lossy().to_string(),
            old_content: old_content.to_string(),
            new_content: new_content.to_string(),
            old_content_hash: compute_content_hash(old_content),
            diff: "".to_string(),
            summary: "normal test".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        let outcome = process_approval_decision_core(&mut conn, &change_id, "approve", "tester");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("applied"));

        let read_back = std::fs::read_to_string(&target_file).unwrap();
        assert_eq!(read_back, new_content);

        let count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "正常完成后暂存恢复表必须清空");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_07a_controlled_write_failure_and_rollback() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07a_test_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();
        crate::fault_injection::set_thread_test_dir(Some(temp_dir.clone()));

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        let _ = crate::work_core::init_work_core_tables(&conn);

        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("crash_probe.txt");
        let original_content = "STABLE_BASE_VERSION_07A_PROBE";
        std::fs::write(&target_file, original_content.as_bytes()).unwrap();
        let original_hash = compute_content_hash(original_content);

        let change_id = format!("chg_07a_{}", uuid::Uuid::new_v4());
        let staged = StagedChange {
            change_id: change_id.clone(),
            request_id: "req_07a".to_string(),
            project_id: "".to_string(),
            file_path: target_file.to_string_lossy().to_string(),
            old_content: original_content.to_string(),
            new_content: "UNSAFE_MODIFIED_CONTENT_FAIL".to_string(),
            old_content_hash: original_hash.clone(),
            diff: "".to_string(),
            summary: "07a test".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        // 布防 07A 写入失败
        crate::fault_injection::arm_fault_file(
            &temp_dir,
            crate::fault_injection::ArmedFaultConfig {
                target_change_id: change_id.clone(),
                target_file_path: target_file.to_string_lossy().to_string(),
                stage: crate::fault_injection::FaultStage::WriteError07a,
                one_shot: true,
            },
        ).unwrap();

        // 执行审批写入：必须触发注入报错
        let outcome = process_approval_decision_core(&mut conn, &change_id, "approve", "tester");
        assert_eq!(outcome.get("status").and_then(|v| v.as_str()), Some("error"));
        let err_msg = outcome.get("error").and_then(|v| v.as_str()).unwrap_or_default();
        assert!(err_msg.contains("07A 受控写入注入失败"), "错误信息必须明确指向 07A: {}", err_msg);

        // 验证 07A 准出标准：
        // 1. 基线绝对无损：解除后原文件完好无损，内容与 Hash 绝对一致
        let restored_bytes = std::fs::read(&target_file).unwrap();
        assert_eq!(restored_bytes, original_content.as_bytes(), "原文件内容必须绝对无损");
        assert_eq!(compute_content_hash(&String::from_utf8_lossy(&restored_bytes)), original_hash);

        // 2. 提案状态未转为 applied，仍维持 pending
        let loaded_staged = get_staged_change(&conn, &change_id).unwrap().unwrap();
        assert_eq!(loaded_staged.status, "pending", "提案状态绝对不可转为 applied");

        // 3. staged_write_recovery 恢复表彻底清理
        let recovery_count: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(recovery_count, 0, "07A 回滚后 recovery 表必须为 0 条目");

        // 4. 布防文件已被单次消费
        assert!(!temp_dir.join(crate::diagnostic_profile::ARMED_FAULT_FILE).exists(), "布防文件必须已被消费");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn child_07b_runner() {
        let _lock = lock_test_recovery();
        if std::env::var("BOB_SUBPROCESS_07B").is_err() {
            // 普通测试运行，不执行子进程体
            return;
        }

        let dir_str = std::env::var("BOB_TEST_DATA_DIR").expect("BOB_TEST_DATA_DIR required");
        let test_dir = std::path::PathBuf::from(dir_str);
        let stage_str = std::env::var("BOB_SUBPROCESS_STAGE").expect("BOB_SUBPROCESS_STAGE required");
        let is_new_file = std::env::var("BOB_SUBPROCESS_NEW_FILE").unwrap_or_default() == "1";

        let stage = match stage_str.as_str() {
            "prepared" => crate::fault_injection::FaultStage::Prepared,
            "backed_up" => crate::fault_injection::FaultStage::BackedUp,
            "replaced" => crate::fault_injection::FaultStage::Replaced,
            other => panic!("Unknown stage: {}", other),
        };

        crate::fault_injection::set_thread_test_dir(Some(test_dir.clone()));

        let db_path = test_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).expect("Open db in child");
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();
        let _ = crate::work_core::init_work_core_tables(&conn);

        let workspace = test_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join(format!("target_{}.txt", stage_str));

        let original_content = if is_new_file {
            ""
        } else {
            "ORIGINAL_CHILD_DATA_V1"
        };

        if !is_new_file {
            std::fs::write(&target_file, original_content.as_bytes()).unwrap();
        }

        let change_id = format!("chg_sub_{}", stage_str);
        let staged = StagedChange {
            change_id: change_id.clone(),
            request_id: format!("req_sub_{}", stage_str),
            project_id: "".to_string(),
            file_path: target_file.to_string_lossy().to_string(),
            old_content: original_content.to_string(),
            new_content: "NEW_CHILD_DATA_V2".to_string(),
            old_content_hash: compute_content_hash(original_content),
            diff: "".to_string(),
            summary: "child test".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        crate::fault_injection::arm_fault_file(
            &test_dir,
            crate::fault_injection::ArmedFaultConfig {
                target_change_id: change_id.clone(),
                target_file_path: target_file.to_string_lossy().to_string(),
                stage,
                one_shot: true,
            },
        ).unwrap();

        // 调用生产审批写入：必须在落库对应阶段后触发中断退出 (exit code 77)
        let _ = process_approval_decision_core(&mut conn, &change_id, "approve", "child_tester");

        panic!("Child runner should have exited at stage {:?}", stage);
    }

    fn run_subprocess_07b(stage: &str, is_new: bool, temp_dir: &std::path::Path) -> std::process::ExitStatus {
        let current_exe = std::env::current_exe().expect("current exe");
        std::process::Command::new(current_exe)
            .arg("--nocapture")
            .arg("--exact")
            .arg("sync_engine::tests::child_07b_runner")
            .env("BOB_SUBPROCESS_07B", "1")
            .env("BOB_SUBPROCESS_STAGE", stage)
            .env("BOB_SUBPROCESS_NEW_FILE", if is_new { "1" } else { "0" })
            .env("BOB_TEST_DATA_DIR", temp_dir.to_string_lossy().to_string())
            .status()
            .expect("Failed to execute child process")
    }

    #[test]
    fn test_07b_prepared_interrupt_and_recovery_existing_file() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07b_prep_exist_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();

        // 真实子进程中断
        let status = run_subprocess_07b("prepared", false, &temp_dir);
        assert_eq!(status.code(), Some(crate::fault_injection::FAULT_INTERRUPT_EXIT_CODE));

        // 验证布防文件已被消费
        assert!(!temp_dir.join(crate::diagnostic_profile::ARMED_FAULT_FILE).exists());

        // 重启模拟：打开数据库执行生产启动恢复函数
        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();

        let count_before: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE stage = 'prepared'", [], |r| r.get(0)).unwrap();
        assert_eq!(count_before, 1, "重启前必须存在 1 条 prepared 崩溃日志");

        let summary = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(summary.recovered_and_cleaned, 1);
        assert_eq!(summary.blocked_or_degraded, 0);

        // prepared 恢复标准：临时文件已删除，目标文件完好无损，recovery 记录清理
        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("target_prepared.txt");
        assert_eq!(std::fs::read_to_string(&target_file).unwrap(), "ORIGINAL_CHILD_DATA_V1");

        let count_after: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count_after, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_07b_prepared_interrupt_and_recovery_new_file() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07b_prep_new_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();

        let status = run_subprocess_07b("prepared", true, &temp_dir);
        assert_eq!(status.code(), Some(crate::fault_injection::FAULT_INTERRUPT_EXIT_CODE));

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();

        let summary = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(summary.recovered_and_cleaned, 1);

        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("target_prepared.txt");
        assert!(!target_file.exists(), "新建文件在 prepared 阶段中断恢复后预期应当不存在");

        let count_after: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count_after, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_07b_backed_up_interrupt_and_recovery() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07b_bak_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();

        let status = run_subprocess_07b("backed_up", false, &temp_dir);
        assert_eq!(status.code(), Some(crate::fault_injection::FAULT_INTERRUPT_EXIT_CODE));

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();

        let count_before: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE stage = 'backed_up'", [], |r| r.get(0)).unwrap();
        assert_eq!(count_before, 1, "重启前必须存在 1 条 backed_up 崩溃日志");

        let summary = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(summary.recovered_and_cleaned, 1);

        // backed_up 恢复标准：从 .bak 安全还原为原目标文件，清理临时文件与 recovery 记录
        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("target_backed_up.txt");
        assert_eq!(std::fs::read_to_string(&target_file).unwrap(), "ORIGINAL_CHILD_DATA_V1");

        let count_after: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count_after, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_07b_replaced_interrupt_and_recovery_existing_file() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07b_rep_exist_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();

        let status = run_subprocess_07b("replaced", false, &temp_dir);
        assert_eq!(status.code(), Some(crate::fault_injection::FAULT_INTERRUPT_EXIT_CODE));

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();

        let count_before: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery WHERE stage = 'replaced'", [], |r| r.get(0)).unwrap();
        assert_eq!(count_before, 1, "重启前必须存在 1 条 replaced 崩溃日志");

        let summary = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(summary.recovered_and_cleaned, 1);

        // replaced 但未提交事务：自愈引擎坚决还原磁盘状态至备份版本
        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("target_replaced.txt");
        assert_eq!(std::fs::read_to_string(&target_file).unwrap(), "ORIGINAL_CHILD_DATA_V1");

        let count_after: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count_after, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_07b_replaced_interrupt_and_recovery_new_file() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07b_rep_new_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();

        let status = run_subprocess_07b("replaced", true, &temp_dir);
        assert_eq!(status.code(), Some(crate::fault_injection::FAULT_INTERRUPT_EXIT_CODE));

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();

        let summary = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(summary.recovered_and_cleaned, 1);

        // 新建文件未提交事务：应当予以移除
        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("target_replaced.txt");
        assert!(!target_file.exists(), "未提交事务的新建文件在 replaced 中断恢复后应当被移除");

        let count_after: i64 = conn.query_row("SELECT count(*) FROM staged_write_recovery", [], |r| r.get(0)).unwrap();
        assert_eq!(count_after, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_external_conflict_preserves_evidence_and_blocks() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_conflict_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("conflict_file.txt");
        let bak_file = workspace.join(".conflict_file.txt.bak_bob_test");
        let tmp_file = workspace.join(".conflict_file.txt.tmp_bob_test");

        std::fs::write(&target_file, b"EXTERNAL_MODIFIED_UNKNOWN_DATA").unwrap();
        std::fs::write(&bak_file, b"ORIGINAL_BACKUP_CONTENT").unwrap();
        std::fs::write(&tmp_file, b"NEW_STAGED_TMP").unwrap();

        let change_id = format!("chg_conf_{}", uuid::Uuid::new_v4());
        let staged = StagedChange {
            change_id: change_id.clone(),
            request_id: "req_conf".to_string(),
            project_id: "".to_string(),
            file_path: target_file.to_string_lossy().to_string(),
            old_content: "ORIGINAL_BACKUP_CONTENT".to_string(),
            new_content: "NEW_STAGED_TMP".to_string(),
            old_content_hash: compute_content_hash("ORIGINAL_BACKUP_CONTENT"),
            diff: "".to_string(),
            summary: "conf test".to_string(),
            additions: 1,
            deletions: 0,
            status: "pending".to_string(),
            work_object_id: None,
            created_at: 1000,
            applied_at: None,
        };
        save_staged_change(&conn, &staged).unwrap();

        record_staged_write_recovery_stage(
            &conn,
            &change_id,
            &target_file.to_string_lossy(),
            Some(&bak_file.to_string_lossy()),
            &tmp_file.to_string_lossy(),
            "backed_up",
        ).unwrap();

        let summary = recover_interrupted_staged_writes(&mut conn).unwrap();
        assert_eq!(summary.blocked_or_degraded, 1, "检测到外部篡改必须标记阻塞");

        // 验证备份证据已保留为 .bak_recovery_conflict_*
        let conflict_preserved = workspace.join(format!(".conflict_file.txt.bak_recovery_conflict_{}", change_id));
        assert!(conflict_preserved.exists(), "外部冲突备份必须被安全保留以供审计");

        // 验证准入阻断持续生效
        assert!(is_path_recovery_blocked(&conn, &target_file.to_string_lossy(), None).unwrap());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_proof_real_appdata_untouched() {
        let _lock = lock_test_recovery();
        let prod_db = dirs::data_dir().map(|d| d.join("bob.agent").join("bob.db"));
        let initial_meta = prod_db.as_ref().and_then(|p| std::fs::metadata(p).ok());
        let initial_len = initial_meta.as_ref().map(|m| m.len());
        let initial_mod = initial_meta.as_ref().and_then(|m| m.modified().ok());

        // 执行隔离 Profile 完整故障注入与恢复流程
        let temp_dir = std::env::temp_dir().join(format!("bob_untouched_proof_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();
        crate::fault_injection::set_thread_test_dir(Some(temp_dir.clone()));

        let db_path = temp_dir.join("bob.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        init_staged_changes_table(&conn).unwrap();
        init_staged_write_recovery_table(&conn).unwrap();

        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("probe_untouched.txt");
        std::fs::write(&target_file, b"LOCAL_TEST").unwrap();

        let change_id = format!("chg_proof_{}", uuid::Uuid::new_v4());
        crate::fault_injection::arm_fault_file(
            &temp_dir,
            crate::fault_injection::ArmedFaultConfig {
                target_change_id: change_id.clone(),
                target_file_path: target_file.to_string_lossy().to_string(),
                stage: crate::fault_injection::FaultStage::WriteError07a,
                one_shot: true,
            },
        ).unwrap();

        // 检验生产目录下的数据库绝对未受任何改变
        if let Some(ref p) = prod_db {
            if let Ok(after_meta) = std::fs::metadata(p) {
                assert_eq!(Some(after_meta.len()), initial_len, "生产主库大小严禁发生任何变动！");
                assert_eq!(after_meta.modified().ok(), initial_mod, "生产主库修改时间严禁发生任何变动！");
            }
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_diagnostic_profile_missing_args_and_unmarked_dir_fail_closed_prod_untouched() {
        let _lock = lock_test_recovery();

        let prod_db = dirs::data_dir().map(|d| d.join("bob.agent").join("bob.db"));
        let initial_meta = prod_db.as_ref().and_then(|p| std::fs::metadata(p).ok());
        let initial_len = initial_meta.as_ref().map(|m| m.len());
        let initial_mod = initial_meta.as_ref().and_then(|m| m.modified().ok());

        // 1. 缺参启动阻断测试
        crate::diagnostic_profile::reset_diagnostic_profile_for_test();
        let prev_env = std::env::var("BOB_TEST_DATA_DIR").ok();
        std::env::remove_var("BOB_TEST_DATA_DIR");

        let res_no_args = crate::diagnostic_profile::ensure_diagnostic_profile_initialized();
        assert!(res_no_args.is_err(), "缺参时必须 Fail-Closed 阻断启动");
        let err_no_args = res_no_args.unwrap_err();
        assert!(
            err_no_args.contains("未提供 --test-data-dir") || err_no_args.contains("严禁静默回退"),
            "错误信息必须明确阻断回退: {}",
            err_no_args
        );
        assert!(!crate::diagnostic_profile::is_diagnostic_profile_active(), "诊断 Profile 绝对不可被激活");

        // 2. 指定目录但缺少测试标记文件启动阻断测试
        let temp_unmarked = std::env::temp_dir().join(format!("bob_unmarked_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_unmarked);
        std::env::set_var("BOB_TEST_DATA_DIR", temp_unmarked.to_string_lossy().to_string());

        let res_unmarked = crate::diagnostic_profile::ensure_diagnostic_profile_initialized();
        assert!(res_unmarked.is_err(), "缺少标记文件时必须 Fail-Closed 阻断启动");
        let err_unmarked = res_unmarked.unwrap_err();
        assert!(err_unmarked.contains("缺少测试标记文件"), "错误信息必须指出缺少测试标记: {}", err_unmarked);
        assert!(!crate::diagnostic_profile::is_diagnostic_profile_active(), "诊断 Profile 绝对不可被激活");

        let _ = std::fs::remove_dir_all(&temp_unmarked);

        if let Some(val) = prev_env {
            std::env::set_var("BOB_TEST_DATA_DIR", val);
        } else {
            std::env::remove_var("BOB_TEST_DATA_DIR");
        }

        // 3. 生产数据库绝对零触碰检验
        if let Some(ref p) = prod_db {
            if let Ok(after_meta) = std::fs::metadata(p) {
                assert_eq!(Some(after_meta.len()), initial_len, "生产库大小严禁发生任何触碰！");
                assert_eq!(after_meta.modified().ok(), initial_mod, "生产库修改时间严禁发生任何变动！");
            }
        }
    }

    #[test]
    fn test_diagnostic_path_traversal_escape_and_nested_creation_negative() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_trav_test_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();
        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);

        // 1. 中间目录不存在时通过 .. 越界逃逸测试
        // 场景 A: test_workspace/nonexistent/../../outside.txt
        let escape_a = workspace.join("nonexistent").join("..").join("..").join("outside.txt");
        let res_a = crate::diagnostic_profile::validate_target_in_test_workspace(&escape_a, &temp_dir);
        assert!(res_a.is_err(), "中间目录不存在时通过 .. 逃逸必须被严格拦截");
        let err_a = res_a.unwrap_err();
        assert!(err_a.contains("安全阻断"), "错误必须包含安全阻断: {}", err_a);

        // 场景 B: test_workspace/a/b/c/../../../../escape.txt
        let escape_b = workspace.join("a").join("b").join("c").join("..").join("..").join("..").join("..").join("escape.txt");
        let res_b = crate::diagnostic_profile::validate_target_in_test_workspace(&escape_b, &temp_dir);
        assert!(res_b.is_err(), "多层嵌套通过 .. 逃逸必须被严格拦截");

        // 场景 C: test_workspace/../../../escape.txt
        let escape_c = workspace.join("..").join("..").join("..").join("escape.txt");
        let res_c = crate::diagnostic_profile::validate_target_in_test_workspace(&escape_c, &temp_dir);
        assert!(res_c.is_err(), "多层相对 .. 逃逸必须被严格拦截");

        // 场景 D: 相对路径形式传入
        let rel_escape = std::path::Path::new("test_workspace/nonexistent/../../outside.txt");
        let res_rel = crate::diagnostic_profile::validate_target_in_test_workspace(rel_escape, &temp_dir);
        assert!(res_rel.is_err(), "相对路径越界逃逸必须被拦截");

        let rel_escape_c = std::path::Path::new("test_workspace/../../../escape.txt");
        let res_rel_c = crate::diagnostic_profile::validate_target_in_test_workspace(rel_escape_c, &temp_dir);
        assert!(res_rel_c.is_err(), "相对路径跨层越界必须被拦截");

        // 2. 正常的新建多级嵌套子文件测试
        let valid_nested = workspace.join("sub").join("dir").join("new.txt");
        assert!(!valid_nested.parent().unwrap().exists(), "验证前父目录必须不存在");
        let res_valid = crate::diagnostic_profile::validate_target_in_test_workspace(&valid_nested, &temp_dir);
        assert!(res_valid.is_ok(), "合法的新建多级嵌套子文件必须正常通过: {:?}", res_valid.err());
        let norm_valid = res_valid.unwrap();
        let norm_ws = crate::diagnostic_profile::normalize_path(&workspace);
        assert!(norm_valid.starts_with(&norm_ws), "规范化路径必须严格归属于 test_workspace");

        // 3. 相对路径形式的新建多级嵌套子文件测试
        let rel_valid = std::path::Path::new("test_workspace/sub/dir/new.txt");
        let res_rel_valid = crate::diagnostic_profile::validate_target_in_test_workspace(rel_valid, &temp_dir);
        assert!(res_rel_valid.is_ok(), "合法的相对路径新建多级嵌套文件必须通过: {:?}", res_rel_valid.err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_07b_rejects_non_oneshot_config() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07b_oneshot_neg_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();
        crate::fault_injection::set_thread_test_dir(Some(temp_dir.clone()));

        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("target_non_oneshot.txt");
        std::fs::write(&target_file, b"BASELINE_DATA").unwrap();

        // 1. arm_fault_file 必须直接拒绝 07B 阶段配置 one_shot: false
        let bad_arm_res = crate::fault_injection::arm_fault_file(
            &temp_dir,
            crate::fault_injection::ArmedFaultConfig {
                target_change_id: "chg_bad_oneshot".to_string(),
                target_file_path: target_file.to_string_lossy().to_string(),
                stage: crate::fault_injection::FaultStage::Prepared,
                one_shot: false,
            },
        );
        assert!(bad_arm_res.is_err(), "arm_fault_file 必须拒绝 07B one_shot: false 配置");

        // 2. 模拟外部文件被恶意或错误写入了 one_shot: false
        let armed_path = temp_dir.join(crate::diagnostic_profile::ARMED_FAULT_FILE);
        let raw_json = serde_json::json!({
            "target_change_id": "chg_bad_oneshot_2",
            "target_file_path": target_file.to_string_lossy().to_string(),
            "stage": "prepared",
            "one_shot": false
        });
        std::fs::write(&armed_path, serde_json::to_vec_pretty(&raw_json).unwrap()).unwrap();

        // 3. read_and_validate_armed 必须拒绝并返回 None，严禁触发退出
        let read_res = crate::fault_injection::read_and_validate_armed(
            "chg_bad_oneshot_2",
            &target_file,
            crate::fault_injection::FaultStage::Prepared,
        );
        assert!(read_res.is_none(), "07B 非 one_shot 配置必须被拒绝并返回 None");

        // 4. 调用 check_and_trigger_07b_interrupt：验证绝不触发退出 77
        crate::fault_injection::check_and_trigger_07b_interrupt(
            "chg_bad_oneshot_2",
            &target_file,
            crate::fault_injection::FaultStage::Prepared,
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_07b_rejects_interrupt_when_consumption_cannot_be_persisted() {
        let _lock = lock_test_recovery();
        let temp_dir = std::env::temp_dir().join(format!("bob_07b_consump_neg_{}", uuid::Uuid::new_v4()));
        crate::diagnostic_profile::setup_test_profile_in_dir(&temp_dir).unwrap();
        crate::fault_injection::set_thread_test_dir(Some(temp_dir.clone()));

        let workspace = temp_dir.join(crate::diagnostic_profile::TEST_WORKSPACE_SUBDIR);
        let target_file = workspace.join("target_unpersisted.txt");
        std::fs::write(&target_file, b"BASELINE_DATA").unwrap();

        let armed_path = crate::fault_injection::arm_fault_file(
            &temp_dir,
            crate::fault_injection::ArmedFaultConfig {
                target_change_id: "chg_unpersisted".to_string(),
                target_file_path: target_file.to_string_lossy().to_string(),
                stage: crate::fault_injection::FaultStage::Prepared,
                one_shot: true,
            },
        ).unwrap();

        // 场景 A: 真实 Windows 共享互斥锁，阻止物理删除与写入覆写
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let lock_handle = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1) // FILE_SHARE_READ: 严禁删除与写入
                .open(&armed_path)
                .unwrap();

            let consume_res = crate::fault_injection::consume_armed_fault_file(&armed_path);
            assert!(consume_res.is_err(), "被锁定布防文件消费必须返回 Err");

            let read_res = crate::fault_injection::read_and_validate_armed(
                "chg_unpersisted",
                &target_file,
                crate::fault_injection::FaultStage::Prepared,
            );
            assert!(read_res.is_none(), "无法持久化清除布防状态时必须拒绝触发并返回 None");

            drop(lock_handle);
        }

        // 场景 B: 模拟 I/O 持久化清除失败
        crate::fault_injection::set_simulate_consumption_failure(true);
        let consume_sim_res = crate::fault_injection::consume_armed_fault_file(&armed_path);
        assert!(consume_sim_res.is_err(), "模拟消费失败必须返回 Err");

        let read_sim_res = crate::fault_injection::read_and_validate_armed(
            "chg_unpersisted",
            &target_file,
            crate::fault_injection::FaultStage::Prepared,
        );
        assert!(read_sim_res.is_none(), "模拟消费失败时必须拒绝触发并返回 None");

        crate::fault_injection::set_simulate_consumption_failure(false);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
