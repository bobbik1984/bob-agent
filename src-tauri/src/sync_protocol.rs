use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const SYNC_PROTOCOL_VERSION: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    Pending,
    Running,
    Success,
    Failed,
    Timeout,
    Unknown,
    Skipped,
}

impl DiagnosticStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Success | Self::Failed | Self::Timeout | Self::Skipped
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Lan,
    Relay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStage {
    LanDirect,
    MobileToRelay,
    RelayToPc,
    PcProcessing,
    PcToRelay,
    RelayToMobile,
    MobileProcessing,
    LocalCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayReceiptKind {
    RelayRegistrationAccepted,
    RelayRequestAccepted,
    RelayDeliveredToPc,
    PcRequestReceived,
    PcResponseCommitted,
    RelayResponseAccepted,
    RelayDeliveredToMobile,
    MobileResponseReceived,
    SyncImportCommitted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncTraceEnvelope {
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u16,
    pub trace_id: String,
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_id: Option<String>,
    pub from_device_id: String,
    pub target_device_id: String,
    pub transport: TransportKind,
    #[serde(default)]
    pub payload: Value,
}

impl SyncTraceEnvelope {
    pub fn new(
        from_device_id: impl Into<String>,
        target_device_id: impl Into<String>,
        transport: TransportKind,
        payload: Value,
    ) -> Self {
        Self {
            protocol_version: SYNC_PROTOCOL_VERSION,
            trace_id: Uuid::new_v4().to_string(),
            message_id: Uuid::new_v4().to_string(),
            sync_id: None,
            from_device_id: from_device_id.into(),
            target_device_id: target_device_id.into(),
            transport,
            payload,
        }
    }

    pub fn diagnostic_capability(&self) -> &'static str {
        if self.protocol_version >= SYNC_PROTOCOL_VERSION {
            "hop_receipts"
        } else {
            "legacy"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u16,
    pub trace_id: String,
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_id: Option<String>,
    pub from_device_id: String,
    pub target_device_id: String,
    pub transport: TransportKind,
    pub stage: DiagnosticStage,
    pub status: DiagnosticStatus,
    pub sequence: u64,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

fn default_protocol_version() -> u16 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_protocol_version_is_legacy() {
        let envelope: SyncTraceEnvelope = serde_json::from_value(serde_json::json!({
            "trace_id": "trace",
            "message_id": "message",
            "from_device_id": "mobile",
            "target_device_id": "pc",
            "transport": "relay",
            "payload": {}
        }))
        .expect("legacy envelope should deserialize");

        assert_eq!(envelope.protocol_version, 1);
        assert_eq!(envelope.diagnostic_capability(), "legacy");
    }
}
