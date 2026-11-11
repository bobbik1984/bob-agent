use crate::error::{BrowserError, BrowserResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpCommandRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpErrorObject {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpRawMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CdpErrorObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpEvent {
    pub method: String,
    pub params: Value,
}

enum WorkerMsg {
    Send {
        id: u64,
        method: String,
        params: Option<Value>,
        deadline: Instant,
        reply_tx: mpsc::Sender<BrowserResult<Value>>,
    },
    Close,
}

#[derive(Clone)]
pub struct CdpClient {
    next_id: Arc<AtomicU64>,
    worker_tx: mpsc::Sender<WorkerMsg>,
    event_buffer: Arc<Mutex<VecDeque<CdpEvent>>>,
    target_crashed: Arc<AtomicBool>,
    is_closed: Arc<AtomicBool>,
    ws_url: String,
}

impl CdpClient {
    pub fn connect(ws_url: &str) -> BrowserResult<Self> {
        let (socket, _response) = tungstenite::connect(ws_url)
            .map_err(|e| BrowserError::WebSocket(e))?;

        // Configure read timeout so the worker loop can yield and process outgoing commands
        if let MaybeTlsStream::Plain(ref stream) = socket.get_ref() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        }

        let (worker_tx, worker_rx) = mpsc::channel::<WorkerMsg>();
        let event_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let target_crashed = Arc::new(AtomicBool::new(false));
        let is_closed = Arc::new(AtomicBool::new(false));

        let thread_events = Arc::clone(&event_buffer);
        let thread_crashed = Arc::clone(&target_crashed);
        let thread_closed = Arc::clone(&is_closed);

        std::thread::Builder::new()
            .name("bob-cdp-worker".to_string())
            .spawn(move || {
                run_cdp_worker(
                    socket,
                    worker_rx,
                    thread_events,
                    thread_crashed,
                    thread_closed,
                );
            })
            .map_err(|e| BrowserError::Other(format!("Failed to spawn CDP worker thread: {}", e)))?;

        Ok(Self {
            next_id: Arc::new(AtomicU64::new(1)),
            worker_tx,
            event_buffer,
            target_crashed,
            is_closed,
            ws_url: ws_url.to_string(),
        })
    }

    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }

    pub fn is_alive(&self) -> bool {
        !self.is_closed.load(Ordering::SeqCst) && !self.target_crashed.load(Ordering::SeqCst)
    }

    pub fn is_crashed(&self) -> bool {
        self.target_crashed.load(Ordering::SeqCst)
    }

    pub fn mark_crashed(&self) {
        self.target_crashed.store(true, Ordering::SeqCst);
    }

    pub fn send_command(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> BrowserResult<Value> {
        if self.target_crashed.load(Ordering::SeqCst) {
            return Err(BrowserError::TargetCrashed(
                "Target has crashed; cannot send CDP command".to_string(),
            ));
        }
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(BrowserError::ConnectionClosed);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (reply_tx, reply_rx) = mpsc::channel();
        let deadline = Instant::now() + timeout;

        self.worker_tx
            .send(WorkerMsg::Send {
                id,
                method: method.to_string(),
                params,
                deadline,
                reply_tx,
            })
            .map_err(|_| BrowserError::ConnectionClosed)?;

        match reply_rx.recv_timeout(timeout) {
            Ok(res) => res,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(BrowserError::CdpTimeout(id)),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if self.target_crashed.load(Ordering::SeqCst) {
                    Err(BrowserError::TargetCrashed("Target crashed during command".to_string()))
                } else {
                    Err(BrowserError::ConnectionClosed)
                }
            }
        }
    }

    pub fn drain_events(&self) -> Vec<CdpEvent> {
        let mut buf = self.event_buffer.lock().unwrap();
        buf.drain(..).collect()
    }

    pub fn wait_for_event(&self, method_prefix: &str, timeout: Duration) -> BrowserResult<CdpEvent> {
        let start = Instant::now();
        loop {
            {
                let mut buf = self.event_buffer.lock().unwrap();
                if let Some(pos) = buf.iter().position(|e| e.method.starts_with(method_prefix)) {
                    return Ok(buf.remove(pos).unwrap());
                }
            }

            if self.target_crashed.load(Ordering::SeqCst) {
                return Err(BrowserError::TargetCrashed("Target crashed while waiting for event".to_string()));
            }
            if self.is_closed.load(Ordering::SeqCst) {
                return Err(BrowserError::ConnectionClosed);
            }

            if start.elapsed() >= timeout {
                return Err(BrowserError::Other(format!(
                    "Timed out waiting for CDP event: {}",
                    method_prefix
                )));
            }

            std::thread::sleep(Duration::from_millis(15));
        }
    }

    pub fn enable_default_domains(&self) -> BrowserResult<()> {
        let timeout = Duration::from_secs(5);
        self.send_command("Page.enable", None, timeout)?;
        self.send_command("DOM.enable", None, timeout)?;
        self.send_command("Accessibility.enable", None, timeout)?;
        self.send_command("Runtime.enable", None, timeout)?;
        self.send_command("Network.enable", None, timeout)?;
        Ok(())
    }

    pub fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        let params = json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
        });
        let res = self.send_command("Runtime.evaluate", Some(params), Duration::from_secs(10))?;
        if let Some(exception) = res.get("exceptionDetails") {
            return Err(BrowserError::Other(format!(
                "JavaScript execution exception: {:?}",
                exception
            )));
        }
        let evaluated = if let Some(r) = res.get("result") {
            r.get("value").cloned().unwrap_or_else(|| r.clone())
        } else if let Some(v) = res.get("value") {
            v.clone()
        } else {
            res
        };
        Ok(evaluated)
    }

    pub fn close(&self) {
        let _ = self.worker_tx.send(WorkerMsg::Close);
        self.is_closed.store(true, Ordering::SeqCst);
    }
}

struct PendingCmd {
    deadline: Instant,
    reply_tx: mpsc::Sender<BrowserResult<Value>>,
}

fn run_cdp_worker(
    mut socket: WebSocket<MaybeTlsStream<TcpStream>>,
    worker_rx: mpsc::Receiver<WorkerMsg>,
    event_buffer: Arc<Mutex<VecDeque<CdpEvent>>>,
    target_crashed: Arc<AtomicBool>,
    is_closed: Arc<AtomicBool>,
) {
    let mut pending: HashMap<u64, PendingCmd> = HashMap::new();

    loop {
        // 1. Drain outgoing commands from channel
        while let Ok(msg) = worker_rx.try_recv() {
            match msg {
                WorkerMsg::Send {
                    id,
                    method,
                    params,
                    deadline,
                    reply_tx,
                } => {
                    let req = CdpCommandRequest { id, method, params };
                    let serialized = match serde_json::to_string(&req) {
                        Ok(s) => s,
                        Err(e) => {
                            let _ = reply_tx.send(Err(BrowserError::Serialization(e)));
                            continue;
                        }
                    };

                    if let Err(e) = socket.send(Message::Text(serialized.into())) {
                        let _ = reply_tx.send(Err(BrowserError::WebSocket(e)));
                        is_closed.store(true, Ordering::SeqCst);
                        return;
                    }
                    pending.insert(id, PendingCmd { deadline, reply_tx });
                }
                WorkerMsg::Close => {
                    let _ = socket.close(None);
                    is_closed.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }

        // 2. Read from WebSocket with timeout
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Ok(raw) = serde_json::from_str::<CdpRawMessage>(&text) {
                    if let Some(id) = raw.id {
                        if let Some(cmd) = pending.remove(&id) {
                            if let Some(err) = raw.error {
                                let _ = cmd.reply_tx.send(Err(BrowserError::CdpCommandFailed {
                                    code: err.code,
                                    message: err.message,
                                }));
                            } else {
                                let _ = cmd.reply_tx.send(Ok(raw.result.unwrap_or(Value::Null)));
                            }
                        }
                    } else if let Some(method) = raw.method {
                        if method == "Inspector.targetCrashed" {
                            target_crashed.store(true, Ordering::SeqCst);
                            for (_, cmd) in pending.drain() {
                                let _ = cmd.reply_tx.send(Err(BrowserError::TargetCrashed(
                                    "Target crashed during command".to_string(),
                                )));
                            }
                        }
                        let mut buf = event_buffer.lock().unwrap();
                        buf.push_back(CdpEvent {
                            method,
                            params: raw.params.unwrap_or(Value::Null),
                        });
                    }
                }
            }
            Ok(Message::Binary(_)) => {}
            Ok(Message::Frame(_)) => {}
            Ok(Message::Ping(data)) => {
                let _ = socket.send(Message::Pong(data));
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => {
                is_closed.store(true, Ordering::SeqCst);
                for (_, cmd) in pending.drain() {
                    let _ = cmd.reply_tx.send(Err(BrowserError::ConnectionClosed));
                }
                return;
            }
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Timeout on read; normal loop tick
            }
            Err(_) => {
                is_closed.store(true, Ordering::SeqCst);
                for (_, cmd) in pending.drain() {
                    let _ = cmd.reply_tx.send(Err(BrowserError::ConnectionClosed));
                }
                return;
            }
        }

        // 3. Clean up expired pending commands
        let now = Instant::now();
        let mut expired_ids = Vec::new();
        for (&id, cmd) in pending.iter() {
            if now > cmd.deadline {
                expired_ids.push(id);
            }
        }
        for id in expired_ids {
            if let Some(cmd) = pending.remove(&id) {
                let _ = cmd.reply_tx.send(Err(BrowserError::CdpTimeout(id)));
            }
        }
    }
}
