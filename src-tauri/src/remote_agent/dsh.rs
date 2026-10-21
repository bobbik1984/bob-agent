//! DSH dsh-v0.2.1-rc.2 browser Remote contract, isolated from Bob-to-Bob RPC.
//! Cookie is native-memory-only. Mux stays serviced even between requests (DSH heartbeat).
use super::*;
use futures_util::{SinkExt, StreamExt};
use reqwest::{header, Client, Url};
use serde_json::{json, Value};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, protocol::WebSocketConfig, Message,
};

const LIMIT: usize = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
const PROTOCOL: &str = "REMOTE_PROTOCOL";
type Reply = oneshot::Sender<Result<Value, String>>;
enum Command {
    Snapshot {
        id: String,
        endpoint: &'static str,
        args: Value,
        reply: Reply,
    },
    Cancel(String),
    Follow {
        id: String,
        session: Option<String>,
        reply: Reply,
    },
}

pub struct Dsh {
    info: ConnectionInfo,
    base: Url,
    client: Client,
    cookie: String,
    commands: mpsc::Sender<Command>,
    alive: Arc<AtomicBool>,
    abort: tokio::task::AbortHandle,
    state: Arc<std::sync::Mutex<super::live::LiveState>>,
    action_lock: Mutex<()>,
}

fn endpoint(input: &str, token: &str) -> Result<(Url, String), String> {
    let mut base = Url::parse(input.trim()).map_err(|_| "REMOTE_INVALID_URL")?;
    let host = base.host_str().ok_or("REMOTE_INVALID_URL")?;
    let loopback = host == "localhost"
        || host == "[::1]"
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);
    if !(base.scheme() == "https" || (base.scheme() == "http" && loopback))
        || !base.username().is_empty()
        || base.password().is_some()
    {
        return Err("REMOTE_INVALID_URL".into());
    }
    // Accept the DSH printed URL, but never retain its query/fragment in ConnectionInfo.
    let embedded: Vec<_> = base
        .query_pairs()
        .filter(|(k, _)| k == "token")
        .map(|(_, v)| v.into_owned())
        .collect();
    if embedded.len() > 1 || base.fragment().is_some() {
        return Err("REMOTE_INVALID_URL".into());
    }
    let secret = if token.trim().is_empty() {
        embedded.first().cloned().unwrap_or_default()
    } else {
        token.trim().to_owned()
    };
    if secret.is_empty() || secret.len() > 8192 || secret.chars().any(char::is_control) {
        return Err("REMOTE_AUTH".into());
    }
    base.set_query(None);
    let path = base.path().to_owned();
    if let Some(prefix) = path.strip_suffix("/connect") {
        base.set_path(&format!("{prefix}/"));
    } else if !path.ends_with('/') {
        base.set_path(&format!("{path}/"));
    }
    Ok((base, secret))
}

impl Dsh {
    pub async fn connect(url: &str, token: &str) -> Result<Self, String> {
        let (base, secret) = endpoint(url, token)?;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| "REMOTE_NETWORK")?;
        let mut connect = base.join("connect").map_err(|_| PROTOCOL)?;
        connect
            .query_pairs_mut()
            .append_pair("token", &secret)
            .append_pair("interactions", "answer");
        // Never forward raw transport errors (they may include the token URL).
        let response = client
            .get(connect.clone())
            .send()
            .await
            .map_err(|_| "REMOTE_NETWORK")?;
        if response.status() == 401 || response.status() == 403 {
            return Err("REMOTE_AUTH".into());
        }
        if response.status() != 302 {
            return Err(PROTOCOL.into());
        }
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(PROTOCOL)?;
        let mux = connect.join(location).map_err(|_| PROTOCOL)?;
        let expected = base.join("api/remote.mux").map_err(|_| PROTOCOL)?;
        if mux != expected {
            return Err("REMOTE_REDIRECT".into());
        }
        let cookies: Vec<_> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or("").to_owned())
            // A reverse proxy may also set its own cookie. Only retain the DSH authority cookie.
            .filter(|s| s.starts_with("dsh-auth-"))
            .collect();
        if cookies.len() != 1 || !cookies[0].contains('=') || cookies[0].len() > 8192 {
            return Err(PROTOCOL.into());
        }
        let cookie = cookies[0].clone();
        let mut ws_url = mux;
        ws_url
            .set_scheme(if base.scheme() == "https" {
                "wss"
            } else {
                "ws"
            })
            .map_err(|_| PROTOCOL)?;
        let mut request = ws_url
            .as_str()
            .into_client_request()
            .map_err(|_| PROTOCOL)?;
        request
            .headers_mut()
            .insert("Cookie", cookie.parse().map_err(|_| PROTOCOL)?);
        request.headers_mut().insert(
            "Origin",
            base.origin()
                .ascii_serialization()
                .parse()
                .map_err(|_| PROTOCOL)?,
        );
        let config = WebSocketConfig::default()
            .max_message_size(Some(LIMIT))
            .max_frame_size(Some(LIMIT));
        let (mut socket, _) = tokio::time::timeout(
            TIMEOUT,
            tokio_tungstenite::connect_async_with_config(request, Some(config), false),
        )
        .await
        .map_err(|_| "REMOTE_TIMEOUT")?
        .map_err(|_| "REMOTE_NETWORK")?;
        let alive = Arc::new(AtomicBool::new(true));
        let worker_alive = alive.clone();
        let state = Arc::new(std::sync::Mutex::new(super::live::LiveState::default()));
        let worker_state = state.clone();
        let (commands, mut receiver) = mpsc::channel::<Command>(16);
        let task = tokio::spawn(async move {
            let mut pending: HashMap<String, Reply> = HashMap::new();
            let mut watch_id: Option<String> = None;
            loop {
                tokio::select! {
                    command = receiver.recv() => match command {
                        Some(Command::Follow { id, session, reply }) => {
                            let (endpoint, args) = if let Some(session) = session {
                                if let Some(old) = watch_id.replace(id.clone()) {
                                    if let Some(reply) = pending.remove(&old) { let _ = reply.send(Err("REMOTE_STALE".into())); }
                                    if socket.send(Message::Text(json!({"type":"cancel","streamId":old}).to_string().into())).await.is_err() { break; }
                                }
                                if let Ok(mut state) = worker_state.lock() { state.select(id.clone(), session.clone()); } else { break; }
                                ("session/follow", json!({"request":{"address":{"kind":"session","sessionId":session},"maxMessages":30,"assistantStream":true}}))
                            } else { ("$events", json!({})) };
                            if socket.send(Message::Text(json!({"type":"open","streamId":id,"endpoint":endpoint,"payload":{"args":args}}).to_string().into())).await.is_err() { break; }
                            pending.insert(id, reply);
                        }
                        Some(Command::Snapshot { id, endpoint, args, reply }) => {
                            let frame = json!({"type":"open","streamId":id,"endpoint":endpoint,"payload":{"args":args}});
                            if socket.send(Message::Text(frame.to_string().into())).await.is_err() { break; }
                            pending.insert(id, reply);
                        }
                        Some(Command::Cancel(id)) => {
                            pending.remove(&id);
                            if socket.send(Message::Text(json!({"type":"cancel","streamId":id}).to_string().into())).await.is_err() { break; }
                        }
                        None => break,
                    },
                    frame = socket.next() => match frame {
                        Some(Ok(Message::Ping(bytes))) => {
                            if socket.send(Message::Pong(bytes)).await.is_err() { break; }
                        }
                        Some(Ok(Message::Text(text))) => {
                            let Ok(value) = serde_json::from_str::<Value>(&text) else { break; };
                            let Some(id) = value.get("streamId").and_then(Value::as_str) else { break; };
                            if id == "events" || watch_id.as_deref() == Some(id) {
                                let result = if value["type"] != "item" { Err(PROTOCOL.to_owned()) }
                                else if let Ok(mut state) = worker_state.lock() {
                                    if id == "events" { state.event_channel(&value["value"]) } else { state.follow(&value["value"]) }
                                } else { Err(PROTOCOL.to_owned()) };
                                if let Some(reply) = pending.remove(id) { let _ = reply.send(result.clone().map(|_| value["value"].clone())); }
                                if let Err(error) = result {
                                    if let Ok(mut state) = worker_state.lock() { state.fail(&error); }
                                    break;
                                }
                                continue;
                            }
                            if let Some(reply) = pending.remove(id) {
                                let result = if value["type"] == "item" {
                                    value.get("value").cloned().ok_or_else(|| PROTOCOL.to_string())
                                } else { Err(PROTOCOL.to_string()) };
                                let _ = reply.send(result);
                                if socket.send(Message::Text(json!({"type":"cancel","streamId":id}).to_string().into())).await.is_err() { break; }
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {},
                        _ => break,
                    }
                }
            }
            worker_alive.store(false, Ordering::Release);
            if let Ok(mut state) = worker_state.lock() {
                if state.view.error.is_none() {
                    state.fail("REMOTE_DISCONNECTED");
                }
            }
            for (_, reply) in pending {
                let _ = reply.send(Err("REMOTE_DISCONNECTED".into()));
            }
        });
        let adapter = Self {
            info: ConnectionInfo {
                connection_id: uuid::Uuid::new_v4().to_string(),
                adapter: "dsh".into(),
                endpoint: base.to_string(),
                capabilities: Capabilities {
                    catalog: true,
                    history: true,
                    send: true,
                    approvals: true,
                    artifacts: false,
                },
            },
            base,
            client,
            cookie,
            commands,
            alive,
            abort: task.abort_handle(),
            state,
            action_lock: Mutex::new(()),
        };
        // A socket is not an answerer. Wait for the generation-bound $events ready frame.
        let (reply, response) = oneshot::channel();
        adapter
            .commands
            .try_send(Command::Follow {
                id: "events".into(),
                session: None,
                reply,
            })
            .map_err(|_| "REMOTE_DISCONNECTED")?;
        let ready = tokio::time::timeout(TIMEOUT, response)
            .await
            .map_err(|_| "REMOTE_TIMEOUT")?
            .map_err(|_| "REMOTE_DISCONNECTED")??;
        if ready["type"] != "ready" {
            return Err(PROTOCOL.into());
        }
        adapter.workspaces().await?;
        Ok(adapter)
    }

    async fn snapshot(&self, endpoint: &'static str, args: Value) -> Result<Value, String> {
        if !self.alive.load(Ordering::Acquire) {
            return Err("REMOTE_DISCONNECTED".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Command::Snapshot {
                id: id.clone(),
                endpoint,
                args,
                reply,
            })
            .map_err(|_| "REMOTE_DISCONNECTED")?;
        match tokio::time::timeout(TIMEOUT, response).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("REMOTE_DISCONNECTED".into()),
            Err(_) => {
                // Close the connection if a cancel cannot be queued; never leak a server stream.
                if self.commands.try_send(Command::Cancel(id)).is_err() {
                    self.close();
                }
                Err("REMOTE_TIMEOUT".into())
            }
        }
    }

    async fn workspaces(&self) -> Result<Vec<RemoteWorkspace>, String> {
        let value = self.snapshot("workspace/follow", json!({})).await?;
        if value["type"] != "baseline" {
            return Err(PROTOCOL.into());
        }
        let items = value
            .pointer("/value/items")
            .and_then(Value::as_array)
            .ok_or(PROTOCOL)?;
        if items.len() > 5000 {
            return Err(PROTOCOL.into());
        }
        items
            .iter()
            .map(|item| serde_json::from_value(item.clone()).map_err(|_| PROTOCOL.to_owned()))
            .collect()
    }

    async fn list(&self) -> Result<Vec<RemoteSession>, String> {
        let value = self.rpc("session/list", json!({"_request":{}})).await?;
        let items = value
            .get("items")
            .and_then(Value::as_array)
            .ok_or(PROTOCOL)?;
        if items.len() > 5000 {
            return Err(PROTOCOL.into());
        }
        items
            .iter()
            .filter(|item| item["origin"] != "subagent")
            .map(|item| {
                let id = item["sessionId"].as_str().ok_or(PROTOCOL)?.to_owned();
                Ok(RemoteSession {
                    title: item
                        .pointer("/projections/values/title")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_owned(),
                    session_id: id,
                    running: item["running"].as_bool().ok_or(PROTOCOL)?,
                    updated_at: item["updatedAt"].as_f64().ok_or(PROTOCOL)?,
                })
            })
            .collect()
    }

    async fn rpc(&self, method: &str, args: Value) -> Result<Value, String> {
        if !self.alive.load(Ordering::Acquire) {
            return Err("REMOTE_DISCONNECTED".into());
        }
        let rpc_id = uuid::Uuid::new_v4().to_string();
        let response = self.client.post(self.base.join(&format!("api/{method}")).map_err(|_| PROTOCOL)?)
            .header(header::COOKIE, &self.cookie)
            .header(header::ORIGIN, self.base.origin().ascii_serialization())
            .json(&json!({"type":"client-request","rpcId":rpc_id,"method":method,"payload":{"args":args}}))
            .send().await.map_err(|_| "REMOTE_NETWORK")?;
        if response.status() == 401 || response.status() == 403 {
            self.close();
            return Err("REMOTE_AUTH".into());
        }
        if !response.status().is_success() {
            return Err("REMOTE_NETWORK".into());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "REMOTE_NETWORK")?;
            if bytes.len() + chunk.len() > LIMIT {
                return Err(PROTOCOL.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| PROTOCOL)?;
        if value["type"] != "server-response"
            || value["rpcId"] != rpc_id
            || value["result"]["ok"] != true
        {
            return Err(PROTOCOL.into());
        }
        Ok(value["result"]["value"].clone())
    }
}

impl RemoteAgent for Dsh {
    fn info(&self) -> ConnectionInfo {
        self.info.clone()
    }
    fn catalog(&self) -> BoxFuture<'_, Result<Catalog, String>> {
        Box::pin(async {
            Ok(Catalog {
                workspaces: self.workspaces().await?,
                sessions: self.list().await?,
            })
        })
    }
    fn history<'a>(&'a self, session_id: &'a str) -> BoxFuture<'a, Result<History, String>> {
        Box::pin(async move {
            let value = self.snapshot("session/follow", json!({"request":{"address":{"kind":"session","sessionId":session_id},"maxMessages":30}})).await?;
            project_history(session_id, &value)
        })
    }
    fn watch<'a>(&'a self, session_id: &'a str) -> BoxFuture<'a, Result<LiveView, String>> {
        Box::pin(async move {
            let _guard = self.action_lock.lock().await;
            if !self.alive.load(Ordering::Acquire) {
                return Err("REMOTE_DISCONNECTED".into());
            }
            let id = uuid::Uuid::new_v4().to_string();
            let (reply, response) = oneshot::channel();
            self.commands
                .try_send(Command::Follow {
                    id: id.clone(),
                    session: Some(session_id.into()),
                    reply,
                })
                .map_err(|_| "REMOTE_DISCONNECTED")?;
            match tokio::time::timeout(TIMEOUT, response).await {
                Ok(Ok(Ok(_))) => self.live(&id),
                _ => {
                    self.close();
                    Err("REMOTE_DISCONNECTED".into())
                }
            }
        })
    }
    fn live(&self, watch_id: &str) -> Result<LiveView, String> {
        let state = self.state.lock().map_err(|_| PROTOCOL)?;
        if state.view.watch_id != watch_id {
            return Err("REMOTE_STALE".into());
        }
        Ok(state.view())
    }
    fn action(&self, input: ActionInput) -> BoxFuture<'_, Result<LiveView, String>> {
        Box::pin(async move {
            let _guard = self.action_lock.lock().await;
            let (method, args) = {
                let mut state = self.state.lock().map_err(|_| PROTOCOL)?;
                state.check(&input.watch_id)?;
                match input.kind.as_str() {
                    "send" => {
                        let id = input.request_id.as_deref().ok_or(PROTOCOL)?;
                        let text = input.text.as_deref().ok_or(PROTOCOL)?;
                        if !state.begin_send(id, text)? {
                            return Ok(state.view());
                        }
                        (
                            "session/prompt",
                            json!({"request":{"requestId":id,"sessionId":state.view.session_id,"mode":"queue","content":[{"type":"text","text":text}]}}),
                        )
                    }
                    "cancel" => {
                        state.begin_cancel()?;
                        (
                            "session/cancel",
                            json!({"request":{"sessionId":state.view.session_id}}),
                        )
                    }
                    "answer" => (
                        "$events/result",
                        state.begin_answer(
                            input.event_id.as_deref().ok_or(PROTOCOL)?,
                            input.answer.as_ref().ok_or(PROTOCOL)?,
                        )?,
                    ),
                    _ => return Err(PROTOCOL.into()),
                }
            };
            let result = self.rpc(method, args).await;
            let mut state = self.state.lock().map_err(|_| PROTOCOL)?;
            match result {
                Ok(value) if method == "$events/result" || value["accepted"] == true => {
                    // HTTP acknowledgements cannot overwrite a durable event that arrived first.
                    if input.kind == "send" && state.view.phase == "sending" {
                        state.view.phase = "accepted".into();
                    }
                    if let Some(id) = input.event_id {
                        if let Some(item) = state.interactions.get_mut(&id) {
                            if item.status == "submitting" {
                                item.status = "answered".into();
                            }
                        }
                    }
                }
                _ => {
                    // Dispatch may have executed before transport failure. Never retry blindly.
                    let session = state.view.session_id.clone();
                    state.uncertain.insert(session);
                    state.fail("REMOTE_OUTCOME_UNKNOWN");
                    return Err("REMOTE_OUTCOME_UNKNOWN".into());
                }
            }
            Ok(state.view())
        })
    }
    fn close(&self) {
        self.alive.store(false, Ordering::Release);
        if let Ok(mut state) = self.state.lock() {
            state.fail("REMOTE_DISCONNECTED");
        }
        self.abort.abort();
    }
}
impl Drop for Dsh {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) fn project_history(session_id: &str, value: &Value) -> Result<History, String> {
    if value["type"] != "snapshot" || value["header"]["id"] != session_id {
        return Err(PROTOCOL.into());
    }
    let has_more = value["hasMore"].as_bool().ok_or(PROTOCOL)?;
    let records = value["records"].as_array().ok_or(PROTOCOL)?;
    let mut messages: Vec<HistoryMessage> = Vec::new();
    let mut omitted = 0;
    for record in records {
        let event = &record["event"];
        if let Some(op) = event.get("surfaceOp").filter(|v| v.is_object()) {
            let start = op["startSeq"].as_u64().ok_or(PROTOCOL)?;
            let end = op["endSeq"].as_u64().ok_or(PROTOCOL)?;
            if op["op"] != "replace" || start > end {
                return Err(PROTOCOL.into());
            }
            messages.retain(|m| {
                m.id.parse::<u64>()
                    .map(|id| id < start || id > end)
                    .unwrap_or(false)
            });
        }
        let role = match event["type"].as_str() {
            Some("user/message") => "user",
            Some("assistant/message") => "assistant",
            _ => {
                omitted += 1;
                continue;
            }
        };
        // Only explicit text content is projected. Tools, reasoning, attachments and arbitrary
        // JSON never become local file links/HTML, and missing content is not invented.
        let message = if role == "assistant" {
            &event["data"]["message"]
        } else {
            &event["data"]
        };
        // DSH also logs injected instructions as user/message. Do not misattribute these to the human.
        if role == "user" && message["source"]["kind"] != "user" {
            omitted += 1;
            continue;
        }
        let Some(content) = message["content"].as_array() else {
            omitted += 1;
            continue;
        };
        let mut parts = Vec::new();
        for part in content {
            if part["type"] == "text" {
                if let Some(text) = part["text"].as_str() {
                    parts.push(text);
                }
            } else {
                omitted += 1;
            }
        }
        if !parts.is_empty() {
            messages.push(HistoryMessage {
                id: event["seq"].to_string(),
                role: role.into(),
                text: parts.join("\n"),
            });
        }
    }
    Ok(History {
        session_id: session_id.into(),
        messages,
        has_more,
        omitted_records: omitted,
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "live_tests.rs"]
mod live_tests;
