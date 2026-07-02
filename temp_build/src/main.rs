use axum::{
    extract::{Path, State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
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

// 全局状态：映射 Room ID 到 Room
type AppState = Arc<RwLock<HashMap<String, Room>>>;

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(RwLock::new(HashMap::new()));

    let app = Router::new()
        .route("/ws/send/{room_id}", get(ws_send_handler))
        .route("/ws/recv/{room_id}", get(ws_recv_handler))
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
        let mut rooms = state.write().await;
        rooms.insert(room_id.clone(), Room { tx: tx.clone(), notify_tx });
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

    let mut rooms = state.write().await;
    rooms.remove(&room_id);
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
        let rooms = state.read().await;
        if let Some(room) = rooms.get(&room_id) {
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

// 隐蔽隧道：JSON-over-HTTPS 代理引擎
async fn proxy_handler(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let target_url = match headers.get("X-Proxy-Target-Url") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return (axum::http::StatusCode::BAD_REQUEST, "Missing X-Proxy-Target-Url").into_response(),
    };
    
    let target_method_str = match headers.get("X-Proxy-Target-Method") {
        Some(v) => v.to_str().unwrap_or("GET"),
        None => "GET",
    };

    let method = reqwest::Method::from_bytes(target_method_str.as_bytes()).unwrap_or(reqwest::Method::GET);

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
            let mut response_builder = axum::response::Response::builder()
                .status(res.status());
            
            for (k, v) in res.headers() {
                response_builder = response_builder.header(k.as_str(), v.as_bytes());
            }

            let body_bytes = res.bytes().await.unwrap_or_default();
            response_builder.body(axum::body::Body::from(body_bytes)).unwrap_or_else(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "").into_response())
        }
        Err(e) => {
            (axum::http::StatusCode::BAD_GATEWAY, format!("Proxy Error: {}", e)).into_response()
        }
    }
}
