use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::db::DbState;

pub const CAPTURE_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    Received,
    Extracting,
    Classifying,
    PendingEnrichment,
    Processing,
    NeedsClarification,
    RetryWait,
    PermanentlyFailed,
    Committing,
    Committed,
    Synced,
    Failed,
}

impl CaptureStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Extracting => "extracting",
            Self::Classifying => "classifying",
            Self::PendingEnrichment => "pending_enrichment",
            Self::Processing => "processing",
            Self::NeedsClarification => "needs_clarification",
            Self::RetryWait => "retry_wait",
            Self::PermanentlyFailed => "permanently_failed",
            Self::Committing => "committing",
            Self::Committed => "committed",
            Self::Synced => "synced",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInput {
    pub entry_point: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub explicit_intent: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub privacy_scope: Option<String>,
    #[serde(default)]
    pub sync_scope: Option<String>,
    #[serde(default)]
    pub source_device: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureEnvelope {
    pub capture_id: String,
    pub schema_version: i64,
    pub entry_point: String,
    pub source_device: String,
    pub content: Option<String>,
    pub source_url: Option<String>,
    pub file_path: Option<String>,
    pub explicit_intent: Option<String>,
    pub content_hash: String,
    pub idempotency_key: String,
    pub language: String,
    pub privacy_scope: String,
    pub sync_scope: String,
    pub status: CaptureStatus,
    pub error_stage: Option<String>,
    pub error_message: Option<String>,
    pub derived_refs: Vec<String>,
    pub retry_count: i64,
    pub next_retry_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn init_capture_tables(conn: &Connection) {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS capture_journal (
            capture_id TEXT PRIMARY KEY,
            schema_version INTEGER NOT NULL,
            entry_point TEXT NOT NULL,
            source_device TEXT NOT NULL DEFAULT 'unknown',
            original_content TEXT,
            source_url TEXT,
            file_path TEXT,
            explicit_intent TEXT,
            content_hash TEXT NOT NULL,
            idempotency_key TEXT NOT NULL UNIQUE,
            language TEXT NOT NULL DEFAULT 'auto',
            privacy_scope TEXT NOT NULL DEFAULT 'private',
            sync_scope TEXT NOT NULL DEFAULT 'paired_devices',
            status TEXT NOT NULL,
            error_stage TEXT,
            error_message TEXT,
            derived_refs TEXT NOT NULL DEFAULT '[]',
            retry_count INTEGER NOT NULL DEFAULT 0,
            next_retry_at INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_capture_status ON capture_journal(status);
        CREATE INDEX IF NOT EXISTS idx_capture_updated ON capture_journal(updated_at);
        CREATE INDEX IF NOT EXISTS idx_capture_hash ON capture_journal(content_hash);
        CREATE TABLE IF NOT EXISTS capture_events (
            event_id TEXT PRIMARY KEY,
            capture_id TEXT,
            source_device TEXT NOT NULL DEFAULT 'unknown',
            event_code TEXT NOT NULL,
            event_params TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'info',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_capture_events_device_time
            ON capture_events(source_device, created_at DESC);
        ",
    )
    .unwrap_or_default();
    // Existing installations receive the recovery metadata without a destructive migration.
    let _ = conn.execute(
        "ALTER TABLE capture_journal ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE capture_journal ADD COLUMN next_retry_at INTEGER",
        [],
    );
    crate::capture_router::init_capture_router_tables(conn);
}

fn record_event(
    conn: &Connection,
    capture_id: Option<&str>,
    source_device: &str,
    event_code: &str,
    event_params: Value,
    status: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO capture_events (event_id, capture_id, source_device, event_code, event_params, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            Uuid::new_v4().to_string(),
            capture_id,
            source_device,
            event_code,
            event_params.to_string(),
            status,
            crate::now_ms(),
        ],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM capture_events WHERE source_device = ?1 AND event_id NOT IN (SELECT event_id FROM capture_events WHERE source_device = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 50)",
        params![source_device],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn clean(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let normalized = v.replace("\r\n", "\n").trim().to_string();
        (!normalized.is_empty()).then_some(normalized)
    })
}

pub(crate) fn extract_source_url(content: &str) -> Option<String> {
    let https = content.find("https://");
    let http = content.find("http://");
    let start = match (https, http) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let tail = &content[start..];
    let end = tail
        .char_indices()
        .find_map(|(index, ch)| {
            (index > 0
                && (ch.is_whitespace()
                    || matches!(
                        ch,
                        ')' | ']'
                            | '>'
                            | ','
                            | '，'
                            | '。'
                            | '；'
                            | ';'
                            | '!'
                            | '！'
                            | '?'
                            | '？'
                            | '"'
                            | '\''
                    )))
            .then_some(index)
        })
        .unwrap_or(tail.len());
    let candidate = tail[..end].trim_end_matches(['.', ':']).to_string();
    (!candidate.is_empty()).then_some(candidate)
}

fn fnv1a_hex(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

fn safe_file_name(value: &str) -> String {
    let raw = value
        .split_once("__")
        .map(|(_, original)| original)
        .unwrap_or(value);
    let cleaned = raw
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = cleaned.trim_matches(['.', '_']).to_string();
    if trimmed.is_empty() {
        "shared-image".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

fn image_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

fn build_envelope(input: CaptureInput) -> Result<CaptureEnvelope, String> {
    let entry_point = input.entry_point.trim().to_string();
    if entry_point.is_empty() {
        return Err("Capture entryPoint 不能为空".to_string());
    }

    let content = clean(input.content);
    let source_url =
        clean(input.source_url).or_else(|| content.as_deref().and_then(extract_source_url));
    let file_path = clean(input.file_path);
    if content.is_none() && source_url.is_none() && file_path.is_none() {
        return Err("Capture 至少需要 content、sourceUrl 或 filePath 之一".to_string());
    }

    let content_hash = fnv1a_hex(&[
        content.as_deref().unwrap_or(""),
        source_url.as_deref().unwrap_or(""),
        file_path.as_deref().unwrap_or(""),
    ]);
    let idempotency_key = clean(input.idempotency_key)
        .unwrap_or_else(|| format!("capture:v{CAPTURE_SCHEMA_VERSION}:{content_hash}"));
    let now = crate::now_ms();

    Ok(CaptureEnvelope {
        capture_id: Uuid::new_v4().to_string(),
        schema_version: CAPTURE_SCHEMA_VERSION,
        entry_point,
        source_device: clean(input.source_device).unwrap_or_else(|| "unknown".to_string()),
        content,
        source_url,
        file_path,
        explicit_intent: clean(input.explicit_intent),
        content_hash,
        idempotency_key,
        language: clean(input.language).unwrap_or_else(|| "auto".to_string()),
        privacy_scope: clean(input.privacy_scope).unwrap_or_else(|| "private".to_string()),
        sync_scope: clean(input.sync_scope).unwrap_or_else(|| "paired_devices".to_string()),
        status: CaptureStatus::Received,
        error_stage: None,
        error_message: None,
        derived_refs: Vec::new(),
        retry_count: 0,
        next_retry_at: None,
        created_at: now,
        updated_at: now,
    })
}

#[cfg(test)]
pub(crate) fn build_envelope_for_test(input: CaptureInput) -> Result<CaptureEnvelope, String> {
    build_envelope(input)
}

fn envelope_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CaptureEnvelope> {
    let status: String = row.get(13)?;
    let derived_json: String = row.get(16)?;
    Ok(CaptureEnvelope {
        capture_id: row.get(0)?,
        schema_version: row.get(1)?,
        entry_point: row.get(2)?,
        source_device: row.get(3)?,
        content: row.get(4)?,
        source_url: row.get(5)?,
        file_path: row.get(6)?,
        explicit_intent: row.get(7)?,
        content_hash: row.get(8)?,
        idempotency_key: row.get(9)?,
        language: row.get(10)?,
        privacy_scope: row.get(11)?,
        sync_scope: row.get(12)?,
        status: match status.as_str() {
            "extracting" => CaptureStatus::Extracting,
            "classifying" => CaptureStatus::Classifying,
            "pending_enrichment" => CaptureStatus::PendingEnrichment,
            "processing" => CaptureStatus::Processing,
            "needs_clarification" => CaptureStatus::NeedsClarification,
            "retry_wait" => CaptureStatus::RetryWait,
            "permanently_failed" => CaptureStatus::PermanentlyFailed,
            "committing" => CaptureStatus::Committing,
            "committed" => CaptureStatus::Committed,
            "synced" => CaptureStatus::Synced,
            "failed" => CaptureStatus::Failed,
            _ => CaptureStatus::Received,
        },
        error_stage: row.get(14)?,
        error_message: row.get(15)?,
        derived_refs: serde_json::from_str(&derived_json).unwrap_or_default(),
        retry_count: row.get(17)?,
        next_retry_at: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

const CAPTURE_SELECT: &str = "SELECT capture_id, schema_version, entry_point, source_device, original_content, source_url, file_path, explicit_intent, content_hash, idempotency_key, language, privacy_scope, sync_scope, status, error_stage, error_message, derived_refs, retry_count, next_retry_at, created_at, updated_at FROM capture_journal";

fn insert_or_get(
    conn: &Connection,
    envelope: &CaptureEnvelope,
) -> Result<(CaptureEnvelope, bool), String> {
    if let Some(existing) = conn
        .query_row(
            &format!("{CAPTURE_SELECT} WHERE idempotency_key = ?1"),
            params![&envelope.idempotency_key],
            envelope_from_row,
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        return Ok((existing, true));
    }

    conn.execute(
        "INSERT INTO capture_journal (capture_id, schema_version, entry_point, source_device, original_content, source_url, file_path, explicit_intent, content_hash, idempotency_key, language, privacy_scope, sync_scope, status, error_stage, error_message, derived_refs, retry_count, next_retry_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL, NULL, '[]', 0, NULL, ?15, ?15)",
        params![
            &envelope.capture_id,
            envelope.schema_version,
            &envelope.entry_point,
            &envelope.source_device,
            &envelope.content,
            &envelope.source_url,
            &envelope.file_path,
            &envelope.explicit_intent,
            &envelope.content_hash,
            &envelope.idempotency_key,
            &envelope.language,
            &envelope.privacy_scope,
            &envelope.sync_scope,
            envelope.status.as_str(),
            envelope.created_at,
        ],
    )
    .map_err(|e| e.to_string())?;
    record_event(
        conn,
        Some(&envelope.capture_id),
        &envelope.source_device,
        "capture.received",
        json!({ "entryPoint": envelope.entry_point }),
        "info",
    )?;
    Ok((envelope.clone(), false))
}

pub(crate) fn record_committed_capture(
    conn: &Connection,
    input: CaptureInput,
    derived_refs: Vec<String>,
) -> Result<(CaptureEnvelope, bool), String> {
    let envelope = build_envelope(input)?;
    let (stored, duplicate) = insert_or_get(conn, &envelope)?;
    let already_complete = matches!(
        stored.status,
        CaptureStatus::Committed | CaptureStatus::Synced
    );
    update_capture(
        conn,
        &stored.capture_id,
        CaptureStatus::Committed,
        None,
        None,
        &derived_refs,
    )?;
    if !already_complete {
        record_event(
            conn,
            Some(&stored.capture_id),
            &stored.source_device,
            "capture.committed",
            json!({ "entryPoint": stored.entry_point, "destination": derived_refs.first() }),
            "success",
        )?;
    }
    let updated = conn
        .query_row(
            &format!("{CAPTURE_SELECT} WHERE capture_id = ?1"),
            params![&stored.capture_id],
            envelope_from_row,
        )
        .map_err(|e| e.to_string())?;
    Ok((updated, duplicate))
}

fn update_capture(
    conn: &Connection,
    capture_id: &str,
    status: CaptureStatus,
    error_stage: Option<&str>,
    error_message: Option<&str>,
    derived_refs: &[String],
) -> Result<(), String> {
    let refs = serde_json::to_string(derived_refs).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE capture_journal SET status = ?2, error_stage = ?3, error_message = ?4, derived_refs = ?5, updated_at = ?6 WHERE capture_id = ?1",
        params![capture_id, status.as_str(), error_stage, error_message, refs, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn mark_pending_enrichment(
    conn: &Connection,
    capture: &CaptureEnvelope,
    route: &crate::capture_router::RouteDecision,
) -> Result<(), String> {
    update_capture(
        conn,
        &capture.capture_id,
        CaptureStatus::PendingEnrichment,
        Some("enrichment"),
        None,
        &capture.derived_refs,
    )?;
    record_event(
        conn,
        Some(&capture.capture_id),
        &capture.source_device,
        "capture.pending_enrichment",
        json!({ "confidence": route.confidence, "reasonCodes": route.reason_codes }),
        "info",
    )
}

pub(crate) fn mark_needs_clarification(
    conn: &Connection,
    capture: &CaptureEnvelope,
    route: &crate::capture_router::RouteDecision,
) -> Result<(), String> {
    update_capture(
        conn,
        &capture.capture_id,
        CaptureStatus::NeedsClarification,
        Some("clarification"),
        Some("时间或意图信息不足"),
        &capture.derived_refs,
    )?;
    record_event(
        conn,
        Some(&capture.capture_id),
        &capture.source_device,
        "capture.needs_clarification",
        json!({ "reasonCodes": route.reason_codes }),
        "info",
    )
}

fn merged_derived_refs(capture: &CaptureEnvelope, extra: &[String]) -> Vec<String> {
    let mut refs = capture.derived_refs.clone();
    for value in extra {
        if !refs.contains(value) {
            refs.push(value.clone());
        }
    }
    refs
}

pub(crate) fn mark_project_assignment_pending(
    conn: &Connection,
    capture: &CaptureEnvelope,
    candidate_id: &str,
    reason_code: &str,
) -> Result<(), String> {
    let candidate_ref = format!("project_link:{candidate_id}");
    let refs = merged_derived_refs(capture, &[candidate_ref]);
    update_capture(
        conn,
        &capture.capture_id,
        CaptureStatus::NeedsClarification,
        Some("project_assignment"),
        Some(reason_code),
        &refs,
    )?;
    record_event(
        conn,
        Some(&capture.capture_id),
        &capture.source_device,
        "capture.project_assignment_pending",
        json!({ "candidateId": candidate_id, "reasonCode": reason_code }),
        "info",
    )
}

pub(crate) fn mark_project_link_dismissed(
    conn: &Connection,
    capture: &CaptureEnvelope,
    candidate_id: &str,
) -> Result<(), String> {
    let candidate_ref = format!("project_link:{candidate_id}");
    let refs = merged_derived_refs(capture, &[candidate_ref]);
    update_capture(
        conn,
        &capture.capture_id,
        CaptureStatus::Committed,
        None,
        None,
        &refs,
    )?;
    record_event(
        conn,
        Some(&capture.capture_id),
        &capture.source_device,
        "capture.project_assignment_dismissed",
        json!({ "candidateId": candidate_id }),
        "info",
    )
}

pub(crate) fn mark_enrichment_retry(
    conn: &Connection,
    capture: &CaptureEnvelope,
    message: &str,
) -> Result<(), String> {
    let transient = message.contains("Clerk 当前不可用")
        || message.contains("timeout")
        || message.contains("连接")
        || message.contains("network");
    // Network/model availability may recover much later. Keep those captures in
    // a bounded backoff loop instead of exhausting the durable offline queue.
    let next_count = if transient {
        (capture.retry_count + 1).min(4)
    } else {
        capture.retry_count + 1
    };
    let exhausted = !transient && next_count >= 5;
    let status = if exhausted {
        "permanently_failed"
    } else {
        "retry_wait"
    };
    let next_retry_at = (!exhausted).then(|| crate::now_ms() as i64 + retry_delay_ms(next_count));
    conn.execute(
        "UPDATE capture_journal SET status = ?2, error_stage = 'enrichment', error_message = ?3, retry_count = ?4, next_retry_at = ?5, updated_at = ?6 WHERE capture_id = ?1",
        params![&capture.capture_id, status, message, next_count, next_retry_at, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE capture_enrichment SET last_error = ?2, model_attempts = model_attempts + 1, updated_at = ?3 WHERE capture_id = ?1",
        params![&capture.capture_id, message, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    record_event(
        conn,
        Some(&capture.capture_id),
        &capture.source_device,
        if exhausted {
            "capture.enrichment_failed"
        } else {
            "capture.enrichment_deferred"
        },
        json!({ "retryCount": next_count, "exhausted": exhausted }),
        if exhausted { "failed" } else { "info" },
    )
}

pub(crate) fn mark_routed_committed(
    conn: &Connection,
    capture: &CaptureEnvelope,
    derived_refs: &[String],
) -> Result<(), String> {
    update_capture(
        conn,
        &capture.capture_id,
        CaptureStatus::Committed,
        None,
        None,
        derived_refs,
    )?;
    record_event(
        conn,
        Some(&capture.capture_id),
        &capture.source_device,
        "capture.action_committed",
        json!({ "destination": derived_refs.first() }),
        "success",
    )
}

pub(crate) fn mark_knowledge_committed(
    conn: &Connection,
    capture: &CaptureEnvelope,
    derived_refs: &[String],
) -> Result<(), String> {
    update_capture(
        conn,
        &capture.capture_id,
        CaptureStatus::Committed,
        None,
        None,
        derived_refs,
    )?;
    record_event(
        conn,
        Some(&capture.capture_id),
        &capture.source_device,
        "capture.knowledge_committed",
        json!({ "destination": derived_refs.first() }),
        "success",
    )
}

pub(crate) fn list_pending_enrichment(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<CaptureEnvelope>, String> {
    let now = crate::now_ms() as i64;
    let mut stmt = conn
        .prepare(&format!(
            "{CAPTURE_SELECT} WHERE retry_count < 5
             AND capture_id IN (SELECT capture_id FROM capture_enrichment WHERE stage IN ('pending_model','validated','awaiting_pipeline'))
             AND (status = 'pending_enrichment' OR (status = 'retry_wait' AND next_retry_at IS NOT NULL AND next_retry_at <= ?1))
             ORDER BY updated_at ASC LIMIT ?2"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![now, limit as i64], envelope_from_row)
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub(crate) fn get_capture(conn: &Connection, capture_id: &str) -> Result<CaptureEnvelope, String> {
    conn.query_row(
        &format!("{CAPTURE_SELECT} WHERE capture_id = ?1"),
        params![capture_id],
        envelope_from_row,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Capture 不存在".to_string())
}

fn retry_delay_ms(retry_count: i64) -> i64 {
    // 5s, 30s, 2m, 10m, then cap at 30m.
    match retry_count {
        0 | 1 => 5_000,
        2 => 30_000,
        3 => 120_000,
        4 => 600_000,
        _ => 1_800_000,
    }
}

fn mark_capture_failed(
    conn: &Connection,
    capture_id: &str,
    stage: &str,
    message: &str,
) -> Result<(), String> {
    let (retry_count, source_device): (i64, String) = conn
        .query_row(
            "SELECT retry_count, source_device FROM capture_journal WHERE capture_id = ?1",
            params![capture_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let next_count = retry_count + 1;
    let next_retry_at = crate::now_ms() as i64 + retry_delay_ms(next_count);
    conn.execute(
        "UPDATE capture_journal SET status = 'failed', error_stage = ?2, error_message = ?3, retry_count = ?4, next_retry_at = ?5, updated_at = ?6 WHERE capture_id = ?1",
        params![capture_id, stage, message, next_count, next_retry_at, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    record_event(
        conn,
        Some(capture_id),
        &source_device,
        "capture.failed",
        json!({ "stage": stage, "message": message, "retryCount": next_count }),
        "failed",
    )?;
    Ok(())
}

fn is_quick_note_entry(entry_point: &str) -> bool {
    matches!(entry_point, "quick_note" | "chat_memo" | "mobile_share")
}

fn commit_quick_note(
    conn: &Connection,
    capture: &CaptureEnvelope,
) -> Result<CaptureEnvelope, String> {
    if !is_quick_note_entry(&capture.entry_point) {
        return Err(format!(
            "Capture 入口 {} 尚不支持自动重放",
            capture.entry_point
        ));
    }
    let content = capture
        .content
        .clone()
        .ok_or_else(|| "Capture 原始文本缺失，无法恢复".to_string())?;
    update_capture(
        conn,
        &capture.capture_id,
        CaptureStatus::Committing,
        None,
        None,
        &capture.derived_refs,
    )?;
    match crate::notebook::notebook_append_daily_capture(content, capture.capture_id.clone()) {
        Ok(note_result) if note_result.get("ok").and_then(Value::as_bool) == Some(true) => {
            let refs: Vec<String> = note_result
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string)
                .into_iter()
                .collect();
            let refs_json = serde_json::to_string(&refs).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE capture_journal SET status = 'committed', error_stage = NULL, error_message = NULL, derived_refs = ?2, next_retry_at = NULL, updated_at = ?3 WHERE capture_id = ?1",
                params![&capture.capture_id, refs_json, crate::now_ms()],
            )
            .map_err(|e| e.to_string())?;
            record_event(
                conn,
                Some(&capture.capture_id),
                &capture.source_device,
                "capture.committed",
                json!({ "entryPoint": capture.entry_point, "destination": refs.first() }),
                "success",
            )?;
        }
        Ok(note_result) => {
            let message = note_result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("速记写入失败")
                .to_string();
            mark_capture_failed(conn, &capture.capture_id, "committing", &message)?;
            return Err(message);
        }
        Err(message) => {
            mark_capture_failed(conn, &capture.capture_id, "committing", &message)?;
            return Err(message);
        }
    }
    conn.query_row(
        &format!("{CAPTURE_SELECT} WHERE capture_id = ?1"),
        params![&capture.capture_id],
        envelope_from_row,
    )
    .map_err(|e| e.to_string())
}

/// Merge a capture received from another paired device. The idempotency key is
/// the cross-device identity; a newer processing state may advance the local row
/// without changing its capture_id or duplicating the source.
pub(crate) fn merge_capture_record(
    conn: &Connection,
    obj: &serde_json::Map<String, Value>,
    fallback_now: i64,
) -> Result<(), String> {
    let capture_id = obj
        .get("capture_id")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or("同步 Capture 缺少 capture_id")?;
    let idempotency_key = obj
        .get("idempotency_key")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or("同步 Capture 缺少 idempotency_key")?;

    let changed = conn.execute(
        "INSERT INTO capture_journal (capture_id, schema_version, entry_point, source_device, original_content, source_url, file_path, explicit_intent, content_hash, idempotency_key, language, privacy_scope, sync_scope, status, error_stage, error_message, derived_refs, retry_count, next_retry_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT(idempotency_key) DO UPDATE SET
            status = excluded.status,
            error_stage = excluded.error_stage,
            error_message = excluded.error_message,
            derived_refs = excluded.derived_refs,
            retry_count = excluded.retry_count,
            next_retry_at = excluded.next_retry_at,
            updated_at = excluded.updated_at
         WHERE excluded.updated_at > capture_journal.updated_at
           AND (CASE excluded.status
                WHEN 'received' THEN 0 WHEN 'extracting' THEN 1 WHEN 'classifying' THEN 2
                WHEN 'pending_enrichment' THEN 3 WHEN 'processing' THEN 4
                WHEN 'retry_wait' THEN 5 WHEN 'needs_clarification' THEN 6
                WHEN 'committing' THEN 7 WHEN 'failed' THEN 8 WHEN 'permanently_failed' THEN 8
                WHEN 'committed' THEN 9 WHEN 'synced' THEN 10 ELSE 0 END)
             >= (CASE capture_journal.status
                WHEN 'received' THEN 0 WHEN 'extracting' THEN 1 WHEN 'classifying' THEN 2
                WHEN 'pending_enrichment' THEN 3 WHEN 'processing' THEN 4
                WHEN 'retry_wait' THEN 5 WHEN 'needs_clarification' THEN 6
                WHEN 'committing' THEN 7 WHEN 'failed' THEN 8 WHEN 'permanently_failed' THEN 8
                WHEN 'committed' THEN 9 WHEN 'synced' THEN 10 ELSE 0 END)",
        params![
            capture_id,
            obj.get("schema_version")
                .and_then(Value::as_i64)
                .unwrap_or(CAPTURE_SCHEMA_VERSION),
            obj.get("entry_point")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            obj.get("source_device")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            obj.get("original_content").and_then(Value::as_str),
            obj.get("source_url").and_then(Value::as_str),
            obj.get("file_path").and_then(Value::as_str),
            obj.get("explicit_intent").and_then(Value::as_str),
            obj.get("content_hash")
                .and_then(Value::as_str)
                .unwrap_or(""),
            idempotency_key,
            obj.get("language")
                .and_then(Value::as_str)
                .unwrap_or("auto"),
            obj.get("privacy_scope")
                .and_then(Value::as_str)
                .unwrap_or("private"),
            obj.get("sync_scope")
                .and_then(Value::as_str)
                .unwrap_or("paired_devices"),
            obj.get("status")
                .and_then(Value::as_str)
                .unwrap_or("received"),
            obj.get("error_stage").and_then(Value::as_str),
            obj.get("error_message").and_then(Value::as_str),
            obj.get("derived_refs")
                .and_then(Value::as_str)
                .unwrap_or("[]"),
            obj.get("retry_count").and_then(Value::as_i64).unwrap_or(0),
            obj.get("next_retry_at").and_then(Value::as_i64),
            obj.get("created_at")
                .and_then(Value::as_i64)
                .unwrap_or(fallback_now),
            obj.get("updated_at")
                .and_then(Value::as_i64)
                .unwrap_or(fallback_now),
        ],
    )
    .map_err(|e| e.to_string())?;
    if changed > 0 {
        let source_device = obj
            .get("source_device")
            .and_then(Value::as_str)
            .unwrap_or("remote");
        let local_capture_id: String = conn
            .query_row(
                "SELECT capture_id FROM capture_journal WHERE idempotency_key = ?1",
                params![idempotency_key],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        record_event(
            conn,
            Some(&local_capture_id),
            source_device,
            "capture.synced",
            json!({ "entryPoint": obj.get("entry_point").and_then(Value::as_str).unwrap_or("unknown") }),
            "success",
        )?;
    }
    Ok(())
}

#[tauri::command]
pub fn capture_ingest(input: CaptureInput, db: State<DbState>) -> Result<Value, String> {
    let envelope = build_envelope(input)?;
    let mut conn = db.0.lock().map_err(|e| e.to_string())?;
    let (stored, duplicate) = insert_or_get(&conn, &envelope)?;
    if duplicate
        && matches!(
            stored.status,
            CaptureStatus::Committed | CaptureStatus::Synced
        )
    {
        return Ok(json!({ "ok": true, "duplicate": true, "capture": stored }));
    }
    let route = crate::capture_router::apply_local_route(&mut conn, &stored)?;
    let updated = conn
        .query_row(
            &format!("{CAPTURE_SELECT} WHERE capture_id = ?1"),
            params![&stored.capture_id],
            envelope_from_row,
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true, "duplicate": duplicate, "capture": updated, "routing": route }))
}

#[tauri::command]
pub fn capture_quick_note(
    content: String,
    entry_point: Option<String>,
    source_device: Option<String>,
    idempotency_key: Option<String>,
    db: State<DbState>,
) -> Result<Value, String> {
    let input = CaptureInput {
        entry_point: entry_point.unwrap_or_else(|| "quick_note".to_string()),
        content: Some(content.clone()),
        source_url: None,
        file_path: None,
        explicit_intent: Some("seed".to_string()),
        language: None,
        privacy_scope: None,
        sync_scope: None,
        source_device,
        // A repeated sentence can be a legitimate second note. Callers that can retry
        // (such as Android Share) provide a stable source key; direct notes get a new key.
        idempotency_key: Some(
            idempotency_key.unwrap_or_else(|| format!("quick-note:{}", Uuid::new_v4())),
        ),
    };
    let envelope = build_envelope(input)?;
    let (stored, duplicate) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        insert_or_get(&conn, &envelope)?
    };
    if duplicate
        && matches!(
            stored.status,
            CaptureStatus::Committed | CaptureStatus::Synced
        )
    {
        return Ok(json!({ "ok": true, "duplicate": true, "capture": stored }));
    }

    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let committed = commit_quick_note(&conn, &stored)?;
    Ok(json!({
        "ok": true,
        "duplicate": duplicate,
        "captureId": committed.capture_id,
        "status": "committed",
        "path": committed.derived_refs.first()
    }))
}

#[tauri::command]
pub async fn capture_retry(capture_id: String, app: AppHandle) -> Result<Value, String> {
    let db = app.state::<DbState>();
    let capture = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        get_capture(&conn, &capture_id)?
    };
    if matches!(
        capture.status,
        CaptureStatus::Committed | CaptureStatus::Synced
    ) {
        return Ok(json!({ "ok": true, "alreadyComplete": true, "capture": capture }));
    }
    if is_quick_note_entry(&capture.entry_point) {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let committed = commit_quick_note(&conn, &capture)?;
        return Ok(json!({ "ok": true, "alreadyComplete": false, "capture": committed }));
    }
    drop(db);
    let result = crate::capture_router::process_capture_by_id(&app, &capture_id).await?;
    Ok(json!({ "ok": true, "alreadyComplete": false, "result": result }))
}

#[tauri::command]
pub fn capture_diagnostics(db: State<DbState>) -> Result<Value, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    diagnostics(&conn)
}

#[tauri::command]
pub fn capture_activity_list(
    limit: Option<usize>,
    db: State<DbState>,
) -> Result<Vec<Value>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let safe_limit = limit.unwrap_or(50).clamp(1, 50) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT event_id, capture_id, source_device, event_code, event_params, status, created_at FROM capture_events ORDER BY created_at DESC, rowid DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![safe_limit], |row| {
            let params_json: String = row.get(4)?;
            Ok(json!({
                "eventId": row.get::<_, String>(0)?,
                "captureId": row.get::<_, Option<String>>(1)?,
                "sourceDevice": row.get::<_, String>(2)?,
                "eventCode": row.get::<_, String>(3)?,
                "params": serde_json::from_str::<Value>(&params_json).unwrap_or_else(|_| json!({})),
                "status": row.get::<_, String>(5)?,
                "createdAt": row.get::<_, i64>(6)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[tauri::command]
pub fn capture_mobile_image(
    filename: String,
    app: AppHandle,
    db: State<DbState>,
) -> Result<Value, String> {
    let cache_dir = app.path().cache_dir().map_err(|e| e.to_string())?;
    let incoming_dir = cache_dir.join("shared_incoming");
    let requested_name = Path::new(&filename)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "分享图片文件名无效".to_string())?;
    if requested_name != filename {
        return Err("分享图片路径越界".to_string());
    }
    let source = incoming_dir.join(requested_name);
    let canonical_incoming = incoming_dir.canonicalize().map_err(|e| e.to_string())?;
    let canonical_source = source.canonicalize().map_err(|e| e.to_string())?;
    if !canonical_source.starts_with(&canonical_incoming) || !canonical_source.is_file() {
        return Err("分享图片不在受信任缓存目录".to_string());
    }

    let content_hash = hash_file(&canonical_source)?;
    let original_name = safe_file_name(requested_name);
    let mut envelope = build_envelope(CaptureInput {
        entry_point: "mobile_image_share".to_string(),
        content: Some(original_name.clone()),
        source_url: None,
        file_path: Some("pending-managed-image".to_string()),
        explicit_intent: Some("source".to_string()),
        language: None,
        privacy_scope: None,
        // Asset bytes are not part of the current SQLite sync payload. Do not
        // advertise a metadata-only row to peers as if the image were present.
        sync_scope: Some("local_only".to_string()),
        source_device: Some("android".to_string()),
        idempotency_key: Some(format!("android-image:{requested_name}:{content_hash}")),
    })?;
    envelope.content_hash = content_hash.clone();
    let month = chrono::Local::now().format("%Y/%m").to_string();
    let managed_name = format!(
        "{}--{}--{}",
        envelope.capture_id,
        &content_hash[..8],
        original_name
    );
    let relative_path = format!("assets/captures/images/{month}/{managed_name}");
    envelope.file_path = Some(relative_path.clone());

    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let (stored, duplicate) = insert_or_get(&conn, &envelope)?;
    if duplicate
        && matches!(
            stored.status,
            CaptureStatus::Committed | CaptureStatus::Synced
        )
    {
        return Ok(json!({
            "ok": true,
            "duplicate": true,
            "captureId": stored.capture_id,
            "managedPath": stored.file_path
        }));
    }

    let final_relative = stored.file_path.clone().unwrap_or(relative_path);
    let final_path = crate::notebook::get_notes_dir().join(PathBuf::from(&final_relative));
    let parent = final_path
        .parent()
        .ok_or_else(|| "图片归档路径无效".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let part_path = final_path.with_extension(format!(
        "{}.part",
        final_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("img")
    ));
    update_capture(
        &conn,
        &stored.capture_id,
        CaptureStatus::Committing,
        None,
        None,
        &stored.derived_refs,
    )?;
    let archive_result = (|| -> Result<(), String> {
        let copied = std::fs::copy(&canonical_source, &part_path).map_err(|e| e.to_string())?;
        let expected = canonical_source
            .metadata()
            .map_err(|e| e.to_string())?
            .len();
        if copied != expected {
            return Err(format!("图片复制不完整：{copied}/{expected} bytes"));
        }
        if final_path.exists() {
            std::fs::remove_file(&part_path).map_err(|e| e.to_string())?;
        } else {
            std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(message) = archive_result {
        let _ = std::fs::remove_file(&part_path);
        mark_capture_failed(&conn, &stored.capture_id, "asset_copy", &message)?;
        return Err(message);
    }

    update_capture(
        &conn,
        &stored.capture_id,
        CaptureStatus::Committed,
        None,
        None,
        std::slice::from_ref(&final_relative),
    )?;
    let size = final_path.metadata().map_err(|e| e.to_string())?.len();
    record_event(
        &conn,
        Some(&stored.capture_id),
        "android",
        "capture.image_saved",
        json!({
            "fileName": original_name,
            "managedPath": final_relative,
            "mimeType": image_mime(&final_path),
            "size": size
        }),
        "success",
    )?;
    Ok(json!({
        "ok": true,
        "duplicate": duplicate,
        "captureId": stored.capture_id,
        "managedPath": final_relative,
        "size": size
    }))
}

fn diagnostics(conn: &Connection) -> Result<Value, String> {
    let pending_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM capture_journal WHERE status IN ('received','extracting','classifying','pending_enrichment','processing','retry_wait','needs_clarification','committing')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let failed_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM capture_journal WHERE status IN ('failed','permanently_failed')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "{CAPTURE_SELECT} WHERE status IN ('failed','permanently_failed') ORDER BY updated_at DESC LIMIT 20"
        ))
        .map_err(|e| e.to_string())?;
    let recent_failed = stmt
        .query_map([], envelope_from_row)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    Ok(json!({
        "pendingCount": pending_count,
        "failedCount": failed_count,
        "recentFailed": recent_failed
    }))
}

/// Replays only safe text-note captures. Unknown entry points remain visible in
/// diagnostics instead of being guessed into a user data destination.
pub fn recover_incomplete_captures(conn: &Connection) -> Result<Value, String> {
    let now = crate::now_ms() as i64;
    let mut stmt = conn
        .prepare(&format!(
            "{CAPTURE_SELECT} WHERE (status IN ('received','extracting','classifying','committing') OR (status = 'failed' AND next_retry_at IS NOT NULL AND next_retry_at <= ?1)) AND retry_count < 5 ORDER BY updated_at ASC LIMIT 20"
        ))
        .map_err(|e| e.to_string())?;
    let candidates = stmt
        .query_map(params![now], envelope_from_row)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    drop(stmt);

    let mut recovered = 0;
    let mut failed = 0;
    let mut deferred = 0;
    for capture in candidates {
        if !is_quick_note_entry(&capture.entry_point) {
            deferred += 1;
            continue;
        }
        match commit_quick_note(conn, &capture) {
            Ok(recovered_capture) => {
                recovered += 1;
                record_event(
                    conn,
                    Some(&recovered_capture.capture_id),
                    &recovered_capture.source_device,
                    "capture.recovered",
                    json!({ "entryPoint": recovered_capture.entry_point }),
                    "success",
                )?;
            }
            Err(_) => failed += 1,
        }
    }
    Ok(json!({ "recovered": recovered, "failed": failed, "deferred": deferred }))
}

#[tauri::command]
pub fn capture_list(
    limit: Option<usize>,
    db: State<DbState>,
) -> Result<Vec<CaptureEnvelope>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let safe_limit = limit.unwrap_or(50).clamp(1, 200) as i64;
    let mut stmt = conn
        .prepare(&format!(
            "{CAPTURE_SELECT} ORDER BY updated_at DESC LIMIT ?1"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![safe_limit], envelope_from_row)
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input(content: &str) -> CaptureInput {
        CaptureInput {
            entry_point: "quick_note".to_string(),
            content: Some(content.to_string()),
            source_url: None,
            file_path: None,
            explicit_intent: Some("seed".to_string()),
            language: None,
            privacy_scope: None,
            sync_scope: None,
            source_device: Some("test-device".to_string()),
            idempotency_key: None,
        }
    }

    #[test]
    fn envelope_requires_real_payload() {
        let mut input = sample_input("  ");
        input.content = Some("\r\n".to_string());
        assert!(build_envelope(input).is_err());
    }

    #[test]
    fn equivalent_content_has_stable_idempotency_key() {
        let first = build_envelope(sample_input("hello\r\nworld")).unwrap();
        let second = build_envelope(sample_input("hello\nworld")).unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.idempotency_key, second.idempotency_key);
    }

    #[test]
    fn duplicate_capture_returns_original_record() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let first = build_envelope(sample_input("same note")).unwrap();
        let second = build_envelope(sample_input("same note")).unwrap();
        let (_, duplicate_first) = insert_or_get(&conn, &first).unwrap();
        let (stored, duplicate_second) = insert_or_get(&conn, &second).unwrap();
        assert!(!duplicate_first);
        assert!(duplicate_second);
        assert_eq!(stored.capture_id, first.capture_id);
    }

    #[test]
    fn offline_enrichment_uses_bounded_backoff_without_losing_capture() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let envelope = build_envelope(sample_input("离线时先记下来")).unwrap();
        insert_or_get(&conn, &envelope).unwrap();
        for _ in 0..7 {
            let current = get_capture(&conn, &envelope.capture_id).unwrap();
            mark_enrichment_retry(&conn, &current, "Clerk 当前不可用").unwrap();
        }
        let stored = get_capture(&conn, &envelope.capture_id).unwrap();
        assert!(matches!(stored.status, CaptureStatus::RetryWait));
        assert_eq!(stored.retry_count, 4);
        assert_eq!(stored.content.as_deref(), Some("离线时先记下来"));
        assert!(stored.next_retry_at.is_some());
    }

    #[test]
    fn repeated_invalid_model_output_reaches_permanent_failure() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let envelope = build_envelope(sample_input("需要解析的复杂安排")).unwrap();
        insert_or_get(&conn, &envelope).unwrap();
        for _ in 0..5 {
            let current = get_capture(&conn, &envelope.capture_id).unwrap();
            mark_enrichment_retry(&conn, &current, "Clerk 路由结果不是有效 JSON").unwrap();
        }
        let stored = get_capture(&conn, &envelope.capture_id).unwrap();
        assert!(matches!(stored.status, CaptureStatus::PermanentlyFailed));
        assert!(stored.next_retry_at.is_none());
        assert_eq!(stored.content.as_deref(), Some("需要解析的复杂安排"));
    }

    #[test]
    fn newer_remote_state_advances_existing_capture_without_duplication() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let first = build_envelope(sample_input("sync me")).unwrap();
        insert_or_get(&conn, &first).unwrap();

        let remote = json!({
            "capture_id": "remote-id",
            "schema_version": 1,
            "entry_point": "mobile_share",
            "source_device": "android",
            "original_content": "sync me",
            "content_hash": first.content_hash,
            "idempotency_key": first.idempotency_key,
            "language": "auto",
            "privacy_scope": "private",
            "sync_scope": "paired_devices",
            "status": "committed",
            "derived_refs": "[\"daily/2026-08-09.md\"]",
            "created_at": first.created_at,
            "updated_at": first.updated_at + 1
        });
        merge_capture_record(&conn, remote.as_object().unwrap(), first.updated_at + 1).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM capture_journal", [], |row| row.get(0))
            .unwrap();
        let (capture_id, status): (String, String) = conn
            .query_row(
                "SELECT capture_id, status FROM capture_journal WHERE idempotency_key = ?1",
                params![first.idempotency_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(capture_id, first.capture_id);
        assert_eq!(status, "committed");
    }

    #[test]
    fn newer_remote_failure_cannot_regress_committed_capture() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let first = build_envelope(sample_input("already safe")).unwrap();
        insert_or_get(&conn, &first).unwrap();
        update_capture(
            &conn,
            &first.capture_id,
            CaptureStatus::Committed,
            None,
            None,
            &["daily/safe.md".to_string()],
        )
        .unwrap();
        let local = conn
            .query_row(
                &format!("{CAPTURE_SELECT} WHERE capture_id = ?1"),
                params![first.capture_id],
                envelope_from_row,
            )
            .unwrap();
        let remote = json!({
            "capture_id": "remote-failed",
            "schema_version": 1,
            "entry_point": "mobile_share",
            "source_device": "android",
            "original_content": "already safe",
            "content_hash": local.content_hash,
            "idempotency_key": local.idempotency_key,
            "status": "failed",
            "error_stage": "committing",
            "error_message": "late peer failure",
            "derived_refs": "[]",
            "updated_at": local.updated_at + 10
        });
        merge_capture_record(&conn, remote.as_object().unwrap(), local.updated_at + 10).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM capture_journal WHERE capture_id = ?1",
                params![local.capture_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "committed");
    }

    #[test]
    fn committed_source_keeps_derived_reference() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let mut input = sample_input("article summary");
        input.entry_point = "chat_article_save".to_string();
        input.source_url = Some("https://example.com/article".to_string());
        input.explicit_intent = Some("knowledge".to_string());
        let (stored, duplicate) = record_committed_capture(
            &conn,
            input,
            vec!["wiki/raw/article/example.md".to_string()],
        )
        .unwrap();
        assert!(!duplicate);
        assert!(matches!(stored.status, CaptureStatus::Committed));
        assert_eq!(stored.derived_refs, vec!["wiki/raw/article/example.md"]);
    }

    #[test]
    fn failure_records_retry_metadata_and_diagnostics() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let capture = build_envelope(sample_input("retry me")).unwrap();
        insert_or_get(&conn, &capture).unwrap();
        mark_capture_failed(&conn, &capture.capture_id, "committing", "disk busy").unwrap();

        let stored = conn
            .query_row(
                &format!("{CAPTURE_SELECT} WHERE capture_id = ?1"),
                params![capture.capture_id],
                envelope_from_row,
            )
            .unwrap();
        assert!(matches!(stored.status, CaptureStatus::Failed));
        assert_eq!(stored.retry_count, 1);
        assert!(stored.next_retry_at.is_some());
        let summary = diagnostics(&conn).unwrap();
        assert_eq!(summary["failedCount"], 1);
        assert_eq!(summary["pendingCount"], 0);
    }

    #[test]
    fn startup_recovery_defers_unknown_entry_points() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let mut input = sample_input("keep original");
        input.entry_point = "future_capture_type".to_string();
        let capture = build_envelope(input).unwrap();
        insert_or_get(&conn, &capture).unwrap();

        let result = recover_incomplete_captures(&conn).unwrap();
        assert_eq!(result["recovered"], 0);
        assert_eq!(result["failed"], 0);
        assert_eq!(result["deferred"], 1);
        let status: String = conn
            .query_row(
                "SELECT status FROM capture_journal WHERE capture_id = ?1",
                params![capture.capture_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "received");
    }

    #[test]
    fn retry_backoff_is_bounded() {
        assert_eq!(retry_delay_ms(1), 5_000);
        assert_eq!(retry_delay_ms(3), 120_000);
        assert_eq!(retry_delay_ms(99), 1_800_000);
    }

    #[test]
    fn activity_log_keeps_latest_fifty_per_device() {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        for index in 0..55 {
            record_event(
                &conn,
                None,
                "android",
                "capture.received",
                json!({ "index": index }),
                "info",
            )
            .unwrap();
        }
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM capture_events WHERE source_device = 'android'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 50);
    }

    #[test]
    fn managed_image_name_preserves_safe_original_name() {
        assert_eq!(
            safe_file_name("9d8a__My holiday photo (1).jpg"),
            "My_holiday_photo__1_.jpg"
        );
        assert_eq!(safe_file_name(".."), "shared-image");
    }

    #[test]
    fn source_url_is_extracted_consistently_from_text_and_markdown() {
        let url = "https://fixture.bob.local/articles/capture-foundation";
        assert_eq!(
            extract_source_url(&format!("收藏一下：{url}。")).as_deref(),
            Some(url)
        );
        assert_eq!(
            extract_source_url(&format!("[稳定文章]({url})")).as_deref(),
            Some(url)
        );
    }

    #[test]
    fn stable_article_three_entry_vertical_slice_is_explainable_and_syncable() {
        let fixture_html = include_str!("../../tests/fixtures/capture/stable_article.html");
        let expected: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/capture/stable_article_expected.json"
        ))
        .unwrap();
        let url = expected["canonicalUrl"].as_str().unwrap();
        assert!(fixture_html.contains(url));
        assert!(fixture_html.contains(expected["title"].as_str().unwrap()));

        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        let cases = [
            (
                "chat_article_save",
                fixture_html.to_string(),
                "knowledge",
                "wiki/raw/article/capture-foundation.md",
                "desktop",
            ),
            (
                "quick_note",
                format!("稍后思考 {url}"),
                "seed",
                "daily/fixture.md",
                "desktop",
            ),
            (
                "mobile_share",
                format!("分享文章：{url}"),
                "seed",
                "daily/fixture.md",
                "android",
            ),
        ];

        let mut stored = Vec::new();
        for (entry_point, content, intent, destination, device) in cases {
            let (capture, duplicate) = record_committed_capture(
                &conn,
                CaptureInput {
                    entry_point: entry_point.to_string(),
                    content: Some(content),
                    source_url: (entry_point == "chat_article_save").then(|| url.to_string()),
                    file_path: (entry_point == "chat_article_save")
                        .then(|| destination.to_string()),
                    explicit_intent: Some(intent.to_string()),
                    language: Some("zh-CN".to_string()),
                    privacy_scope: None,
                    sync_scope: None,
                    source_device: Some(device.to_string()),
                    idempotency_key: Some(format!("fixture:{entry_point}")),
                },
                vec![destination.to_string()],
            )
            .unwrap();
            assert!(!duplicate);
            assert_eq!(capture.source_url.as_deref(), Some(url));
            assert!(matches!(capture.status, CaptureStatus::Committed));
            assert_eq!(capture.explicit_intent.as_deref(), Some(intent));
            stored.push(capture);
        }

        let journal_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM capture_journal", [], |row| row.get(0))
            .unwrap();
        let event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM capture_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_count, 3);
        assert_eq!(event_count, 6);

        let mobile = stored
            .iter()
            .find(|capture| capture.entry_point == "mobile_share")
            .unwrap();
        let peer = Connection::open_in_memory().unwrap();
        init_capture_tables(&peer);
        let remote = json!({
            "capture_id": mobile.capture_id,
            "schema_version": mobile.schema_version,
            "entry_point": mobile.entry_point,
            "source_device": mobile.source_device,
            "original_content": mobile.content,
            "source_url": mobile.source_url,
            "file_path": mobile.file_path,
            "explicit_intent": mobile.explicit_intent,
            "content_hash": mobile.content_hash,
            "idempotency_key": mobile.idempotency_key,
            "language": mobile.language,
            "privacy_scope": mobile.privacy_scope,
            "sync_scope": mobile.sync_scope,
            "status": "committed",
            "derived_refs": serde_json::to_string(&mobile.derived_refs).unwrap(),
            "retry_count": mobile.retry_count,
            "next_retry_at": mobile.next_retry_at,
            "created_at": mobile.created_at,
            "updated_at": mobile.updated_at + 1
        });
        merge_capture_record(&peer, remote.as_object().unwrap(), mobile.updated_at + 1).unwrap();
        let peer_capture = peer
            .query_row(CAPTURE_SELECT, [], envelope_from_row)
            .unwrap();
        assert_eq!(peer_capture.source_url.as_deref(), Some(url));
        assert!(matches!(peer_capture.status, CaptureStatus::Committed));
    }
}
