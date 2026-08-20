use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::sync_protocol::{DiagnosticEvent, DiagnosticStatus, TransportKind};

pub const MAX_SYNC_RUNS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRun {
    pub sync_id: String,
    pub trace_id: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub status: DiagnosticStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<TransportKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

fn runs_path() -> PathBuf {
    crate::get_data_dir().join("sync_runs.json")
}

fn events_path() -> PathBuf {
    crate::get_data_dir().join("sync_trace_events.json")
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(&temp, data).map_err(|error| error.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    fs::rename(&temp, path).map_err(|error| error.to_string())
}

fn trim(runs: &mut Vec<SyncRun>, events: &mut Vec<DiagnosticEvent>) {
    runs.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    runs.truncate(MAX_SYNC_RUNS);
    let retained: std::collections::HashSet<_> =
        runs.iter().map(|run| run.trace_id.as_str()).collect();
    events.retain(|event| retained.contains(event.trace_id.as_str()));
}

pub fn record_run(run: SyncRun) -> Result<(), String> {
    let mut runs: Vec<SyncRun> = read_json(&runs_path());
    let mut events: Vec<DiagnosticEvent> = read_json(&events_path());
    runs.retain(|existing| existing.sync_id != run.sync_id);
    runs.push(run);
    trim(&mut runs, &mut events);
    write_json(&runs_path(), &runs)?;
    write_json(&events_path(), &events)
}

pub fn record_activity(
    status: DiagnosticStatus,
    transport: Option<TransportKind>,
    peer_device_id: Option<String>,
    summary: impl Into<String>,
    error_code: Option<String>,
) -> Result<(), String> {
    let now = crate::now_ms();
    record_run(SyncRun {
        sync_id: uuid::Uuid::new_v4().to_string(),
        trace_id: uuid::Uuid::new_v4().to_string(),
        started_at: now,
        finished_at: now,
        status,
        transport,
        peer_device_id,
        summary: Some(summary.into()),
        error_code,
    })
}

pub fn record_event(event: DiagnosticEvent) -> Result<(), String> {
    let mut events: Vec<DiagnosticEvent> = read_json(&events_path());
    let duplicate = events.iter().any(|existing| {
        existing.trace_id == event.trace_id
            && existing.message_id == event.message_id
            && existing.stage == event.stage
            && existing.sequence == event.sequence
    });
    if !duplicate {
        events.push(event);
        write_json(&events_path(), &events)?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_sync_runs() -> Result<Vec<SyncRun>, String> {
    let mut runs: Vec<SyncRun> = read_json(&runs_path());
    runs.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    runs.truncate(MAX_SYNC_RUNS);
    Ok(runs)
}

#[tauri::command]
pub fn get_sync_trace_events(trace_id: String) -> Result<Vec<DiagnosticEvent>, String> {
    let mut events: Vec<DiagnosticEvent> = read_json(&events_path());
    events.retain(|event| event.trace_id == trace_id);
    events.sort_by_key(|event| (event.timestamp, event.sequence));
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(index: i64) -> SyncRun {
        SyncRun {
            sync_id: format!("sync-{index}"),
            trace_id: format!("trace-{index}"),
            started_at: index,
            finished_at: index,
            status: DiagnosticStatus::Success,
            transport: Some(TransportKind::Relay),
            peer_device_id: None,
            summary: None,
            error_code: None,
        }
    }

    #[test]
    fn keeps_latest_fifty_runs_and_cascades_events() {
        let mut runs: Vec<_> = (0..55).map(run).collect();
        let mut events = vec![DiagnosticEvent {
            protocol_version: 2,
            trace_id: "trace-0".into(),
            message_id: "message-0".into(),
            sync_id: Some("sync-0".into()),
            from_device_id: "mobile".into(),
            target_device_id: "pc".into(),
            transport: TransportKind::Relay,
            stage: crate::sync_protocol::DiagnosticStage::MobileToRelay,
            status: DiagnosticStatus::Success,
            sequence: 1,
            timestamp: 0,
            error_code: None,
            detail: None,
        }];
        trim(&mut runs, &mut events);
        assert_eq!(runs.len(), MAX_SYNC_RUNS);
        assert_eq!(runs[0].sync_id, "sync-54");
        assert!(events.is_empty());
    }
}
