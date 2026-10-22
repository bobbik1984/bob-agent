//! Native remote-Agent boundary. No startup work or local RPC fallback.
//! Saved credentials stay in the independent, explicitly unlocked encrypted local vault.
//! Adapter capabilities describe this client's implemented surface, not the host's full authority.
mod dsh;
mod live;
mod store;
mod system_protection;
pub use live::LiveView;

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
    pub profile_id: Option<String>,
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

#[derive(Clone, Debug, Serialize)]
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
    fn watch<'a>(&'a self, session_id: &'a str) -> BoxFuture<'a, Result<LiveView, String>>;
    fn live(&self, watch_id: &str) -> Result<LiveView, String>;
    fn action(&self, input: ActionInput) -> BoxFuture<'_, Result<LiveView, String>>;
    fn close(&self);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionInput {
    pub watch_id: String,
    pub kind: String,
    pub request_id: Option<String>,
    pub text: Option<String>,
    pub event_id: Option<String>,
    pub answer: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn remote_agent_watch(
    connection_id: String,
    session_id: String,
) -> Result<LiveView, String> {
    if session_id.is_empty() || session_id.len() > 512 {
        return Err("REMOTE_PROTOCOL".into());
    }
    get(&connection_id).await?.watch(&session_id).await
}

#[tauri::command]
pub async fn remote_agent_live(
    connection_id: String,
    watch_id: String,
) -> Result<LiveView, String> {
    get(&connection_id).await?.live(&watch_id)
}

#[tauri::command]
pub async fn remote_agent_action(
    connection_id: String,
    input: ActionInput,
) -> Result<LiveView, String> {
    let adapter = get(&connection_id).await?;
    let profile_id = adapter.info().profile_id;
    let session = adapter.live(&input.watch_id)?.session_id;
    let guarded = input.kind == "send" || input.kind == "answer";
    if let Some(id) = profile_id.as_ref().filter(|_| guarded) {
        let mut guard = store::VAULT.lock().await;
        let vault = store::unlocked(&mut guard)?;
        if vault.credential(id)?.0.uncertain_sessions.contains(&session) { return Err("REMOTE_REVIEW_REQUIRED".into()); }
        vault.update(id, None, None, Some((session.clone(), true)))?;
    }
    let result = adapter.action(input).await;
    if let (Some(id), Ok(view)) = (profile_id.as_ref().filter(|_| guarded), &result) {
        if view.phase != "unknown" {
            let mut guard = store::VAULT.lock().await;
            store::unlocked(&mut guard)?.update(id, None, None, Some((session, false)))?;
        }
    }
    result
}

static CONNECTIONS: Lazy<Mutex<HashMap<String, Arc<dyn RemoteAgent>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Serialize vault changes and reconnect publication; a forgotten/locked profile cannot reappear.
static PROFILE_OPERATIONS: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Serialize)]
pub struct ProfileList { pub unlocked: bool, pub profiles: Vec<store::Profile> }

#[tauri::command]
pub async fn remote_agent_profiles() -> Result<ProfileList, String> {
    let guard = store::VAULT.lock().await;
    Ok(ProfileList { unlocked: guard.is_some(), profiles: guard.as_ref().map(|v| v.list()).unwrap_or_default() })
}

#[tauri::command]
pub async fn remote_agent_unlock(app: tauri::AppHandle, legacy_password: Option<String>) -> Result<ProfileList, String> {
    let _operation = PROFILE_OPERATIONS.lock().await;
    let path = crate::get_data_dir().join("remote-agent-vault.db");
    let vault = tokio::task::spawn_blocking(move || store::Vault::unlock_system(path, legacy_password.as_deref(), |envelope| system_protection::unlock(&app, envelope)))
        .await.map_err(|_| "REMOTE_STORAGE")??;
    *store::VAULT.lock().await = Some(vault);
    remote_agent_profiles().await
}

#[tauri::command]
pub async fn remote_agent_lock() -> Result<(), String> {
    let _operation = PROFILE_OPERATIONS.lock().await;
    for (_, adapter) in CONNECTIONS.lock().await.drain() { adapter.close(); }
    *store::VAULT.lock().await = None;
    Ok(())
}

async fn register(adapter: Arc<dyn RemoteAgent>) -> Result<ConnectionInfo, String> {
    let info = adapter.info();
    let mut connections = CONNECTIONS.lock().await;
    if let Some(id) = &info.profile_id {
        connections.retain(|_, old| {
            if old.info().profile_id.as_ref() == Some(id) { old.close(); false } else { true }
        });
    }
    if connections.len() >= 4 { adapter.close(); return Err("REMOTE_CONNECTION_LIMIT".into()); }
    connections.insert(info.connection_id.clone(), adapter);
    Ok(info)
}

#[tauri::command]
pub async fn remote_agent_save(input: ConnectInput, name: String) -> Result<ConnectionInfo, String> {
    let _operation = PROFILE_OPERATIONS.lock().await;
    if input.adapter != "dsh" { return Err("REMOTE_UNSUPPORTED_ADAPTER".into()); }
    { let mut guard = store::VAULT.lock().await; store::unlocked(&mut guard)?; }
    let mut adapter = dsh::Dsh::connect(&input.endpoint, &input.token).await?;
    let result = {
        let mut guard = store::VAULT.lock().await;
        store::unlocked(&mut guard)?.save(name, adapter.info().endpoint, adapter.credential())
    };
    let profile = match result { Ok(profile) => profile, Err(e) => { adapter.close(); return Err(e); } };
    adapter.set_profile(profile.id);
    register(Arc::new(adapter)).await
}

#[tauri::command]
pub async fn remote_agent_reconnect(profile_id: String) -> Result<ConnectionInfo, String> {
    let _operation = PROFILE_OPERATIONS.lock().await;
    let (profile, cookie) = {
        let mut guard = store::VAULT.lock().await; store::unlocked(&mut guard)?.credential(&profile_id)?
    };
    let mut adapter = dsh::Dsh::reconnect(&profile.endpoint, cookie).await?;
    adapter.set_profile(profile.id);
    register(Arc::new(adapter)).await
}

#[tauri::command]
pub async fn remote_agent_profile_update(profile_id: String, session_id: Option<String>, auto_reconnect: Option<bool>, reviewed_session: Option<String>) -> Result<(), String> {
    let _operation = PROFILE_OPERATIONS.lock().await;
    let mut guard = store::VAULT.lock().await;
    store::unlocked(&mut guard)?.update(&profile_id, session_id, auto_reconnect, reviewed_session.map(|s| (s,false)))
}

#[tauri::command]
pub async fn remote_agent_forget(profile_id: String) -> Result<(), String> {
    let _operation = PROFILE_OPERATIONS.lock().await;
    let mut guard = store::VAULT.lock().await;
    store::unlocked(&mut guard)?.forget(&profile_id)?;
    CONNECTIONS.lock().await.retain(|_, adapter| {
        if adapter.info().profile_id.as_ref() == Some(&profile_id) { adapter.close(); false } else { true }
    });
    Ok(())
}

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
