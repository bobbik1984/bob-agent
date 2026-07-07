use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, RwLock};

struct Room {
    tx: broadcast::Sender<Message>,
    notify_tx: tokio::sync::mpsc::Sender<()>,
}

#[derive(Clone)]
struct DeviceSession {
    tx: tokio::sync::mpsc::Sender<Message>,
}

struct AppStateStruct {
    rooms: HashMap<String, Room>,
    devices: HashMap<String, DeviceSession>,
}

// 全局状态
type AppState = Arc<RwLock<AppStateStruct>>;

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(RwLock::new(AppStateStruct {
        rooms: HashMap::new(),
        devices: HashMap::new(),
    }));

    let app = Router::new()
        .route("/ws/send/{room_id}", get(ws_send_handler))
        .route("/ws/recv/{room_id}", get(ws_recv_handler))
        .route("/ws/device/{device_id}", get(ws_device_handler))
        .route("/api/proxy", post(proxy_handler))
        .with_state(state);

    let addr = "0.0.0.0:3900";
    println!("🚀 Bob Relay Server listening on ws://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// 发送端连入：创建房间并泵送数据
async fn ws_send_handler(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_sender(socket, room_id, state))
}

async fn handle_sender(mut socket: WebSocket, room_id: String, state: AppState) {
    println!("🔵 Sender connected to room: {}", room_id);

    let (tx, _rx) = broadcast::channel(1024);
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<()>(1);

    {
        let mut app_state = state.write().await;
        app_state.rooms.insert(
            room_id.clone(),
            Room {
                tx: tx.clone(),
                notify_tx,
            },
        );
    }

    let (mut sink, mut stream) = socket.split();

    // Task to wait for receiver and send READY
    let write_task = tokio::spawn(async move {
        if let Some(()) = notify_rx.recv().await {
            let _ = sink.send(Message::Text("READY".into())).await;
        }
        // Keep sink alive to prevent WebSocket from closing
        std::future::pending::<()>().await;
    });

    let room_id_clone = room_id.clone();
    let read_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Close(_) = msg {
                break;
            }
            let _ = tx.send(msg);
        }
        println!("🔴 Sender disconnected, destroying room: {}", room_id_clone);
    });

    let _ = read_task.await;
    write_task.abort();

    let mut app_state = state.write().await;
    app_state.rooms.remove(&room_id);
}

// 接收端连入：加入房间接收数据
async fn ws_recv_handler(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_receiver(socket, room_id, state))
}

async fn handle_receiver(mut socket: WebSocket, room_id: String, state: AppState) {
    println!("🟢 Receiver connected to room: {}", room_id);

    let mut rx = {
        let app_state = state.read().await;
        if let Some(room) = app_state.rooms.get(&room_id) {
            let _ = room.notify_tx.try_send(());
            room.tx.subscribe()
        } else {
            println!("⚠️ Room not found: {}", room_id);
            return;
        }
    };

    // 接收广播并转发给浏览器
    while let Ok(msg) = rx.recv().await {
        if socket.send(msg).await.is_err() {
            println!("🔴 Receiver disconnected from room: {}", room_id);
            break;
        }
    }
}

// -----------------------------------------------------------------------------
// 设备注册与信令 (T-2300 Phase 3a)
// -----------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct DeviceIncomingMessage {
    #[serde(rename = "type")]
    msg_type: String, // "query" | "notify"
    target_device_id: Option<String>,
    payload: Option<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct DeviceOutgoingMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn ws_device_handler(
    ws: WebSocketUpgrade,
    Path(device_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_device_session(socket, device_id, state))
}

async fn handle_device_session(socket: WebSocket, device_id: String, state: AppState) {
    println!("📱 Device registered: {}", device_id);

    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Message>(100);

    // 将设备注册到全局状态
    {
        let mut app_state = state.write().await;
        app_state.devices.insert(
            device_id.clone(),
            DeviceSession { tx: tx.clone() },
        );
    }

    // 写任务：将接收到的 MPSC 消息推给 WebSocket
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // 读任务：接收 WebSocket 消息并处理
    let state_clone = state.clone();
    let device_id_clone = device_id.clone();
    
    while let Some(Ok(msg)) = stream.next().await {
        if let Message::Text(text) = msg {
            if let Ok(incoming) = serde_json::from_str::<DeviceIncomingMessage>(&text) {
                match incoming.msg_type.as_str() {
                    "query" => {
                        if let Some(target_id) = incoming.target_device_id {
                            let is_online = {
                                let app_state = state_clone.read().await;
                                app_state.devices.contains_key(&target_id)
                            };
                            let resp = DeviceOutgoingMessage {
                                msg_type: "query_result".into(),
                                target_device_id: Some(target_id),
                                from_device_id: None,
                                online: Some(is_online),
                                payload: None,
                                error: None,
                            };
                            if let Ok(json_str) = serde_json::to_string(&resp) {
                                let _ = tx.send(Message::Text(json_str.into())).await;
                            }
                        }
                    }
                    "notify" | "ack" => {
                        let msg_type = incoming.msg_type.clone();
                        if let Some(target_id) = incoming.target_device_id {
                            let target_tx = {
                                let app_state = state_clone.read().await;
                                app_state.devices.get(&target_id).map(|d| d.tx.clone())
                            };
                            
                            if let Some(target_tx) = target_tx {
                                let forward_msg = DeviceOutgoingMessage {
                                    msg_type,
                                    target_device_id: None,
                                    from_device_id: Some(device_id_clone.clone()),
                                    online: None,
                                    payload: incoming.payload,
                                    error: None,
                                };
                                if let Ok(json_str) = serde_json::to_string(&forward_msg) {
                                    let _ = target_tx.send(Message::Text(json_str.into())).await;
                                }
                            } else {
                                // 目标设备离线
                                let err_resp = DeviceOutgoingMessage {
                                    msg_type: "error".into(),
                                    target_device_id: Some(target_id),
                                    from_device_id: None,
                                    online: None,
                                    payload: None,
                                    error: Some("Target device is offline".into()),
                                };
                                if let Ok(json_str) = serde_json::to_string(&err_resp) {
                                    let _ = tx.send(Message::Text(json_str.into())).await;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // 清理资源
    write_task.abort();
    {
        let mut app_state = state.write().await;
        app_state.devices.remove(&device_id);
    }
    println!("📴 Device unregistered: {}", device_id);
}

// 隐蔽隧道：JSON-over-HTTPS 代理引擎
async fn proxy_handler(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let target_url = match headers.get("X-Proxy-Target-Url") {
        Some(v) => v.to_str().unwrap_or(""),
        None => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                "Missing X-Proxy-Target-Url",
            )
                .into_response()
        }
    };

    let target_method_str = match headers.get("X-Proxy-Target-Method") {
        Some(v) => v.to_str().unwrap_or("GET"),
        None => "GET",
    };

    let method =
        reqwest::Method::from_bytes(target_method_str.as_bytes()).unwrap_or(reqwest::Method::GET);

    let client = reqwest::Client::new();
    let mut req = client.request(method, target_url);

    // 透传真实请求头
    for (k, v) in headers.iter() {
        let key_str = k.as_str();
        if key_str.starts_with("x-proxy-pass-") {
            let real_key = &key_str["x-proxy-pass-".len()..];
            req = req.header(real_key, v);
        }
    }

    if !body.is_empty() {
        req = req.body(body.to_vec());
    }

    match req.send().await {
        Ok(res) => {
            let mut response_builder = axum::response::Response::builder().status(res.status());

            for (k, v) in res.headers() {
                response_builder = response_builder.header(k.as_str(), v.as_bytes());
            }

            let body_bytes = res.bytes().await.unwrap_or_default();
            response_builder
                .body(axum::body::Body::from(body_bytes))
                .unwrap_or_else(|_| {
                    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "").into_response()
                })
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Proxy Error: {}", e),
        )
            .into_response(),
    }
}
