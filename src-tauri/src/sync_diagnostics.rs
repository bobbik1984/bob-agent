use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::sync_protocol::{DiagnosticEvent, DiagnosticStage, DiagnosticStatus, TransportKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LocalIdentityState {
    #[default]
    Uninitialized,
    Locked,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelayConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Registered,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PeerPresence {
    Online,
    Stale,
    Offline,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerSnapshot {
    pub device_id: String,
    pub paired: bool,
    pub presence: PeerPresence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transport: Option<TransportKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathState {
    pub stage: DiagnosticStage,
    pub status: DiagnosticStatus,
    pub sequence: u64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl PathState {
    fn pending(stage: DiagnosticStage) -> Self {
        Self {
            stage,
            status: DiagnosticStatus::Pending,
            sequence: 0,
            updated_at: 0,
            error_code: None,
            detail: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveTrace {
    pub trace_id: String,
    pub protocol_version: u16,
    pub overall_status: DiagnosticStatus,
    pub paths: HashMap<DiagnosticStage, PathState>,
    #[serde(skip)]
    seen_events: HashSet<String>,
}

impl ActiveTrace {
    pub fn new(trace_id: impl Into<String>, protocol_version: u16) -> Self {
        let paths = all_stages()
            .into_iter()
            .map(|stage| (stage, PathState::pending(stage)))
            .collect();
        Self {
            trace_id: trace_id.into(),
            protocol_version,
            overall_status: DiagnosticStatus::Pending,
            paths,
            seen_events: HashSet::new(),
        }
    }

    pub fn reduce(&mut self, event: &DiagnosticEvent) -> bool {
        if event.trace_id != self.trace_id {
            return false;
        }

        let event_key = format!(
            "{}:{:?}:{:?}:{}",
            event.message_id, event.stage, event.status, event.sequence
        );
        if !self.seen_events.insert(event_key) {
            return false;
        }

        let current = self
            .paths
            .entry(event.stage)
            .or_insert_with(|| PathState::pending(event.stage));

        if event.sequence < current.sequence
            || (current.status.is_terminal()
                && event.sequence <= current.sequence
                && current.status != event.status)
        {
            return false;
        }

        current.status = event.status;
        current.sequence = event.sequence;
        current.updated_at = event.timestamp;
        current.error_code = event.error_code.clone();
        current.detail = event.detail.clone();
        self.overall_status = derive_overall_status(&self.paths);
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectivitySnapshot {
    pub local_identity: LocalIdentityState,
    pub relay: RelayConnectionState,
    #[serde(default)]
    pub peers: Vec<PeerSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_trace: Option<ActiveTrace>,
}

static CONNECTIVITY: OnceLock<RwLock<ConnectivitySnapshot>> = OnceLock::new();

fn store() -> &'static RwLock<ConnectivitySnapshot> {
    CONNECTIVITY.get_or_init(|| RwLock::new(ConnectivitySnapshot::default()))
}

pub fn snapshot() -> ConnectivitySnapshot {
    store()
        .read()
        .map(|value| value.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_sync_connectivity_snapshot(app: tauri::AppHandle) -> ConnectivitySnapshot {
    use std::sync::Arc;
    use tauri::Manager;

    let config = crate::read_config();
    set_local_identity(if config.get("device_id").and_then(|value| value.as_str()).is_some() {
        LocalIdentityState::Ready
    } else {
        LocalIdentityState::Uninitialized
    });

    let now = crate::now_ms();
    let registry = app.state::<Arc<crate::sync_engine::DeviceRegistry>>();
    let peers = registry
        .get_all()
        .into_iter()
        .map(|device| PeerSnapshot {
            device_id: device.device_id,
            paired: true,
            presence: if now - device.last_seen <= 90_000 {
                PeerPresence::Online
            } else {
                PeerPresence::Stale
            },
            last_seen_at: Some(device.last_seen),
            last_transport: if device.ip_address == "relay" {
                Some(TransportKind::Relay)
            } else {
                Some(TransportKind::Lan)
            },
        })
        .collect();
    replace_peers(peers);
    snapshot()
}

pub fn set_local_identity(state: LocalIdentityState) {
    if let Ok(mut value) = store().write() {
        value.local_identity = state;
    }
}

pub fn set_relay_state(state: RelayConnectionState) {
    if let Ok(mut value) = store().write() {
        value.relay = state;
    }
}

pub fn replace_peers(peers: Vec<PeerSnapshot>) {
    if let Ok(mut value) = store().write() {
        value.peers = peers;
    }
}

pub fn begin_trace(trace_id: impl Into<String>, protocol_version: u16) {
    if let Ok(mut value) = store().write() {
        value.active_trace = Some(ActiveTrace::new(trace_id, protocol_version));
    }
}

pub fn apply_event(event: &DiagnosticEvent) -> bool {
    let Ok(mut value) = store().write() else {
        return false;
    };
    if value
        .active_trace
        .as_ref()
        .map(|trace| trace.trace_id.as_str())
        != Some(event.trace_id.as_str())
    {
        value.active_trace = Some(ActiveTrace::new(&event.trace_id, event.protocol_version));
    }
    value
        .active_trace
        .as_mut()
        .map(|trace| trace.reduce(event))
        .unwrap_or(false)
}

fn all_stages() -> [DiagnosticStage; 8] {
    [
        DiagnosticStage::LanDirect,
        DiagnosticStage::MobileToRelay,
        DiagnosticStage::RelayToPc,
        DiagnosticStage::PcProcessing,
        DiagnosticStage::PcToRelay,
        DiagnosticStage::RelayToMobile,
        DiagnosticStage::MobileProcessing,
        DiagnosticStage::LocalCommit,
    ]
}

fn derive_overall_status(paths: &HashMap<DiagnosticStage, PathState>) -> DiagnosticStatus {
    let statuses: Vec<_> = paths.values().map(|path| path.status).collect();
    if statuses
        .iter()
        .any(|status| *status == DiagnosticStatus::Running)
    {
        DiagnosticStatus::Running
    } else if statuses
        .iter()
        .any(|status| *status == DiagnosticStatus::Failed)
    {
        DiagnosticStatus::Failed
    } else if statuses
        .iter()
        .any(|status| *status == DiagnosticStatus::Timeout)
    {
        DiagnosticStatus::Timeout
    } else if statuses
        .iter()
        .any(|status| *status == DiagnosticStatus::Unknown)
    {
        DiagnosticStatus::Unknown
    } else if statuses
        .iter()
        .any(|status| *status == DiagnosticStatus::Success)
    {
        DiagnosticStatus::Success
    } else if statuses
        .iter()
        .all(|status| *status == DiagnosticStatus::Skipped)
    {
        DiagnosticStatus::Skipped
    } else {
        DiagnosticStatus::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(status: DiagnosticStatus, sequence: u64) -> DiagnosticEvent {
        DiagnosticEvent {
            protocol_version: 2,
            trace_id: "trace-1".into(),
            message_id: "message-1".into(),
            sync_id: Some("sync-1".into()),
            from_device_id: "mobile".into(),
            target_device_id: "pc".into(),
            transport: TransportKind::Relay,
            stage: DiagnosticStage::MobileToRelay,
            status,
            sequence,
            timestamp: sequence as i64,
            error_code: None,
            detail: None,
        }
    }

    #[test]
    fn duplicate_event_is_idempotent() {
        let mut trace = ActiveTrace::new("trace-1", 2);
        assert!(trace.reduce(&event(DiagnosticStatus::Running, 1)));
        assert!(!trace.reduce(&event(DiagnosticStatus::Running, 1)));
    }

    #[test]
    fn older_event_cannot_overwrite_terminal_state() {
        let mut trace = ActiveTrace::new("trace-1", 2);
        assert!(trace.reduce(&event(DiagnosticStatus::Success, 4)));
        assert!(!trace.reduce(&event(DiagnosticStatus::Running, 3)));
        assert_eq!(
            trace.paths[&DiagnosticStage::MobileToRelay].status,
            DiagnosticStatus::Success
        );
    }

    #[test]
    fn event_for_other_trace_is_ignored() {
        let mut trace = ActiveTrace::new("trace-2", 2);
        assert!(!trace.reduce(&event(DiagnosticStatus::Running, 1)));
    }
}
