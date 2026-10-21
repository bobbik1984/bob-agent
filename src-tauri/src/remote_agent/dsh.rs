//! DSH dsh-v0.2.1-rc.2 browser Remote contract; deliberately read-only Bob surface.
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
}

pub struct Dsh {
    info: ConnectionInfo,
    base: Url,
    client: Client,
    cookie: String,
    commands: mpsc::Sender<Command>,
    alive: Arc<AtomicBool>,
    abort: tokio::task::AbortHandle,
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
            .append_pair("interactions", "none");
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
        let (commands, mut receiver) = mpsc::channel::<Command>(16);
        let task = tokio::spawn(async move {
            let mut pending: HashMap<String, Reply> = HashMap::new();
            loop {
                tokio::select! {
                    command = receiver.recv() => match command {
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
                    send: false,
                    approvals: false,
                    artifacts: false,
                },
            },
            base,
            client,
            cookie,
            commands,
            alive,
            abort: task.abort_handle(),
        };
        // A socket handshake alone is not readiness. Require a valid workspace baseline.
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
        if !self.alive.load(Ordering::Acquire) {
            return Err("REMOTE_DISCONNECTED".into());
        }
        let rpc_id = uuid::Uuid::new_v4().to_string();
        let response = self.client.post(self.base.join("api/session/list").map_err(|_| PROTOCOL)?)
            .header(header::COOKIE, &self.cookie)
            .header(header::ORIGIN, self.base.origin().ascii_serialization())
            .json(&json!({"type":"client-request","rpcId":rpc_id,"method":"session/list","payload":{"args":{"_request":{}}}}))
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
        let items = value
            .pointer("/result/value/items")
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
    fn close(&self) {
        self.alive.store(false, Ordering::Release);
        self.abort.abort();
    }
}
impl Drop for Dsh {
    fn drop(&mut self) {
        self.close();
    }
}

fn project_history(session_id: &str, value: &Value) -> Result<History, String> {
    if value["type"] != "snapshot" || value["header"]["id"] != session_id {
        return Err(PROTOCOL.into());
    }
    let has_more = value["hasMore"].as_bool().ok_or(PROTOCOL)?;
    let records = value["records"].as_array().ok_or(PROTOCOL)?;
    let mut messages = Vec::new();
    let mut omitted = 0;
    for record in records {
        let event = &record["event"];
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
