//! Native remote-Agent boundary. No startup work, local RPC fallback or credential persistence.
//! Adapter capabilities describe this client's implemented surface, not the host's full authority.
mod dsh;

use futures_util::future::BoxFuture;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub catalog: bool,
    pub history: bool,
    pub send: bool,
    pub approvals: bool,
    pub artifacts: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub connection_id: String,
    pub adapter: String,
    pub endpoint: String,
    pub capabilities: Capabilities,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWorkspace {
    pub workspace_id: String,
    pub title: String,
    pub session_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSession {
    pub session_id: String,
    pub title: String,
    pub running: bool,
    pub updated_at: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub workspaces: Vec<RemoteWorkspace>,
    pub sessions: Vec<RemoteSession>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMessage {
    pub id: String,
    pub role: String,
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub session_id: String,
    pub messages: Vec<HistoryMessage>,
    pub has_more: bool,
    /// Non-text records are deliberately not passed into Bob's local HTML/file renderer.
    pub omitted_records: usize,
}

pub trait RemoteAgent: Send + Sync {
    fn info(&self) -> ConnectionInfo;
    fn catalog(&self) -> BoxFuture<'_, Result<Catalog, String>>;
    fn history<'a>(&'a self, session_id: &'a str) -> BoxFuture<'a, Result<History, String>>;
    fn close(&self);
}

static CONNECTIONS: Lazy<Mutex<HashMap<String, Arc<dyn RemoteAgent>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Do not derive Debug/Serialize: input contains a user credential.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectInput {
    pub adapter: String,
    pub endpoint: String,
    pub token: String,
}

#[tauri::command]
pub async fn remote_agent_connect(input: ConnectInput) -> Result<ConnectionInfo, String> {
    if input.adapter != "dsh" {
        return Err("REMOTE_UNSUPPORTED_ADAPTER".into());
    }
    let adapter: Arc<dyn RemoteAgent> =
        Arc::new(dsh::Dsh::connect(&input.endpoint, &input.token).await?);
    let info = adapter.info();
    let mut connections = CONNECTIONS.lock().await;
    if connections.len() >= 4 {
        adapter.close();
        return Err("REMOTE_CONNECTION_LIMIT".into());
    }
    connections.insert(info.connection_id.clone(), adapter);
    Ok(info)
}

async fn get(connection_id: &str) -> Result<Arc<dyn RemoteAgent>, String> {
    CONNECTIONS
        .lock()
        .await
        .get(connection_id)
        .cloned()
        .ok_or_else(|| "REMOTE_DISCONNECTED".into())
}

#[tauri::command]
pub async fn remote_agent_catalog(connection_id: String) -> Result<Catalog, String> {
    get(&connection_id).await?.catalog().await
}

#[tauri::command]
pub async fn remote_agent_history(
    connection_id: String,
    session_id: String,
) -> Result<History, String> {
    if session_id.is_empty() || session_id.len() > 512 {
        return Err("REMOTE_PROTOCOL".into());
    }
    get(&connection_id).await?.history(&session_id).await
}

#[tauri::command]
pub async fn remote_agent_disconnect(connection_id: String) -> Result<(), String> {
    if let Some(adapter) = CONNECTIONS.lock().await.remove(&connection_id) {
        adapter.close();
    }
    Ok(())
}
