use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("SSRF blocked navigation to URL: {0} ({1})")]
    SsrfBlocked(String, String),

    #[error("Blocked URL scheme: {0}")]
    BlockedUrlScheme(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Human takeover required: {reason}")]
    HumanTakeoverRequired {
        reason: String,
        takeover_card: serde_json::Value,
    },

    #[error("Target crashed: {0}")]
    TargetCrashed(String),

    #[error("CDP command failed ({code}): {message}")]
    CdpCommandFailed { code: i64, message: String },

    #[error("CDP timeout waiting for response to command {0}")]
    CdpTimeout(u64),

    #[error("CDP connection closed")]
    ConnectionClosed,

    #[error("Chromium binary not found (set BOB_CHROMIUM_BIN or install Chromium)")]
    BinaryNotFound,

    #[error("Failed to launch Chromium process: {0}")]
    LaunchFailed(String),

    #[error("Element [{0}] not found in element map")]
    ElementNotFound(u32),

    #[error("Element [{0}] is occluded or not interactable")]
    ElementOccluded(u32),

    #[error("Protected file export rejected: raw browser profile download is strictly forbidden")]
    RawProfileExportForbidden,

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    #[error("Reqwest error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Other browser error: {0}")]
    Other(String),
}

pub type BrowserResult<T> = Result<T, BrowserError>;
