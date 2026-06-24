//! web_drop.rs — 三级渐进式文件投送 (Tier 2: WebRTC P2P + Tier 3: WebSocket Relay)
//!
//! 核心流程：
//!   1. 生成房间 ID 和 AES-128 密钥
//!   2. 立即返回分享 URL（不阻塞）
//!   3. 后台 spawn 任务：连接中继 → 等待 READY → 尝试 WebRTC P2P → 降级 WebSocket 中继
//!
//! 供 LLM 工具 `share_file` 和微信大文件降级使用。

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes128Gcm,
};
use base64::{engine::general_purpose, Engine as _};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

// 具体 WebSocket 类型（避免泛型地狱）
type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>
>;
type WsSink = futures_util::stream::SplitSink<WsStream, Message>;
type WsSource = futures_util::stream::SplitStream<WsStream>;

/// 为文件生成 Web Drop 分享链接（非阻塞）。
///
/// 立即返回 `Ok(url)`，后台异步完成文件传输。
/// 接收方打开链接后，中继服务器通知发送端开始流式传输。
#[tauri::command]
pub async fn start_web_drop(file_path: String) -> Result<String, String> {
    // 校验文件存在
    let path = Path::new(&file_path);
    if !path.exists() || !path.is_file() {
        return Err(format!("文件不存在或不是普通文件: {}", file_path));
    }

    let file_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown.bin".to_string());
    let file_size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();

    log::info!("[web_drop] Starting drop for file: {} ({} bytes)", file_name, file_size);

    // 1. Generate Room ID
    let room_id = {
        let mut rng = rand::thread_rng();
        let mut room_bytes = [0u8; 5];
        rng.fill_bytes(&mut room_bytes);

        let mut id = String::with_capacity(10);
        for byte in &room_bytes {
            use std::fmt::Write;
            write!(&mut id, "{:02x}", byte).unwrap();
        }
        id
    };

    // 2. Generate AES-128 Key (needed for Tier 3 fallback)
    let key = Aes128Gcm::generate_key(&mut OsRng);
    let key_b64 = general_purpose::URL_SAFE_NO_PAD.encode(key);

    // 3. Construct the share URL immediately
    let share_url = format!("https://bob.bobbik.org/transfer/?v=2#{}.{}", room_id, key_b64);
    log::info!("[web_drop] Share URL generated: {}", share_url);

    let path_buf = path.to_path_buf();
    let room_id_clone = room_id.clone();
    tokio::spawn(async move {
        let result = tokio::spawn(async move {
            run_drop_session(room_id_clone, key, path_buf, file_name, file_size).await
        }).await;

        match result {
            Ok(Err(e)) => log::error!("[web_drop] Session error: {}", e),
            Ok(Ok(())) => log::info!("[web_drop] Session completed successfully."),
            Err(join_err) => log::error!("[web_drop] Session PANICKED: {:?}", join_err),
        }
    });

    Ok(share_url)
}

/// 后台执行完整的 drop 会话：
/// Phase 1: 信令阶段 — 通过 WebSocket 交换 WebRTC SDP/ICE
/// Phase 2a: P2P 直连成功 — DataChannel 流式传输
/// Phase 2b: P2P 失败 — 降级为 WebSocket 加密中继
async fn run_drop_session(
    room_id: String,
    key: aes_gcm::aead::generic_array::GenericArray<u8, aes_gcm::aead::consts::U16>,
    path: std::path::PathBuf,
    file_name: String,
    file_size: u64,
) -> Result<(), String> {
    // ═══ Step 0: 初始化 Rustls CryptoProvider（0.23+ 强制要求）═══
    let _ = rustls::crypto::ring::default_provider().install_default();

    // ═══ Step 1: 连接中继服务器 ═══
    let ws_url = format!("wss://bob.bobbik.org/transfer/api/ws/send/{}", room_id);
    log::info!("[web_drop] Connecting to relay: {}", ws_url);

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| format!("中继服务器连接失败: {}", e))?;

    let (mut ws_write, mut ws_read) = ws_stream.split();

    // ═══ Step 2: 等待接收方连接（READY 信号），5 分钟超时 ═══
    log::info!("[web_drop] Waiting for receiver (5min timeout)...");
    let ready = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        async {
            while let Some(Ok(msg)) = ws_read.next().await {
                if let Message::Text(text) = msg {
                    if text == "READY" {
                        return true;
                    }
                }
            }
            false
        }
    ).await;

    match ready {
        Ok(true) => log::info!("[web_drop] Receiver connected, starting WebRTC signaling..."),
        Ok(false) => return Err("WebSocket connection closed before receiver connected".into()),
        Err(_) => return Err("Timed out waiting for receiver (5 min)".into()),
    }

    // ═══ Step 3: 尝试 WebRTC P2P ═══
    log::info!("[web_drop] Attempting WebRTC P2P connection...");

    let p2p_result = attempt_webrtc_p2p(
        &mut ws_write,
        &mut ws_read,
        &path,
        &file_name,
        file_size,
    ).await;

    match p2p_result {
        Ok(true) => {
            log::info!("[web_drop] ✅ Tier 2 P2P transfer completed successfully!");
            return Ok(());
        }
        Ok(false) => {
            log::info!("[web_drop] ⚠️ P2P failed, falling back to Tier 3 WebSocket relay...");
        }
        Err(e) => {
            log::warn!("[web_drop] ⚠️ P2P error: {}, falling back to Tier 3...", e);
        }
    }

    // ═══ Step 4: Tier 3 降级 — WebSocket 加密中继 ═══
    // 通知接收方切换到 fallback 模式
    let fallback_msg = serde_json::json!({"type": "fallback"});
    ws_write.send(Message::Text(fallback_msg.to_string().into()))
        .await
        .map_err(|e| format!("发送 fallback 信号失败: {}", e))?;

    log::info!("[web_drop] Sending file via WebSocket relay with AES-GCM encryption...");
    send_via_websocket_relay(&mut ws_write, &key, &path, &file_name, file_size).await
}

/// 尝试通过 WebRTC DataChannel 进行 P2P 传输。
/// 返回 Ok(true) 表示传输完成，Ok(false) 表示打洞失败需要降级。
async fn attempt_webrtc_p2p(
    ws_write: &mut WsSink,
    ws_read: &mut WsSource,
    path: &std::path::PathBuf,
    file_name: &str,
    file_size: u64,
) -> Result<bool, String> {
    // 创建 WebRTC API
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()
        .map_err(|e| format!("MediaEngine 初始化失败: {}", e))?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .build();

    // ICE 配置（使用自建的 coturn）
    let config = RTCConfiguration {
        ice_servers: vec![
            RTCIceServer {
                urls: vec!["stun:bob.bobbik.org:3478".to_string()],
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let peer_connection = Arc::new(
        api.new_peer_connection(config)
            .await
            .map_err(|e| format!("创建 PeerConnection 失败: {}", e))?
    );

    // 创建 DataChannel
    let dc_init = RTCDataChannelInit {
        ordered: Some(true),
        ..Default::default()
    };
    let data_channel = peer_connection
        .create_data_channel("file-transfer", Some(dc_init))
        .await
        .map_err(|e| format!("创建 DataChannel 失败: {}", e))?;

    // 用 channel 通知 DataChannel 打开
    let (dc_open_tx, mut dc_open_rx) = mpsc::channel::<()>(1);
    data_channel.on_open(Box::new(move || {
        log::info!("[web_drop] 🎉 DataChannel opened! P2P connection established.");
        let _ = dc_open_tx.try_send(());
        Box::pin(async {})
    }));

    // ICE candidate 收集 → 通过 WebSocket 发送给接收方
    let (ice_tx, mut ice_rx) = mpsc::channel::<String>(32);
    peer_connection.on_ice_candidate(Box::new(move |candidate| {
        let ice_tx = ice_tx.clone();
        Box::pin(async move {
            if let Some(c) = candidate {
                let json = match c.to_json() {
                    Ok(j) => j,
                    Err(_) => return,
                };
                let msg = serde_json::json!({
                    "type": "ice",
                    "candidate": json.candidate,
                    "sdpMid": json.sdp_mid,
                    "sdpMLineIndex": json.sdp_mline_index,
                });
                let _ = ice_tx.send(msg.to_string()).await;
            }
        })
    }));

    // 生成 Offer
    let offer = peer_connection.create_offer(None)
        .await
        .map_err(|e| format!("创建 Offer 失败: {}", e))?;

    peer_connection.set_local_description(offer.clone())
        .await
        .map_err(|e| format!("设置 LocalDescription 失败: {}", e))?;

    // 发送 Offer 到接收方
    let offer_msg = serde_json::json!({
        "type": "offer",
        "sdp": offer.sdp,
    });
    ws_write.send(Message::Text(offer_msg.to_string().into()))
        .await
        .map_err(|e| format!("发送 Offer 失败: {}", e))?;
    log::info!("[web_drop] Offer sent, waiting for Answer...");

    // 主循环：处理信令 + 等待 DataChannel 打开，10 秒超时
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);

    loop {
        tokio::select! {
            // 检查 DataChannel 是否打开
            _ = dc_open_rx.recv() => {
                log::info!("[web_drop] DataChannel open confirmed!");
                break;
            }
            // 发送排队的 ICE candidates
            Some(ice_json) = ice_rx.recv() => {
                let _ = ws_write.send(Message::Text(ice_json.into())).await;
            }
            // 接收来自接收方的信令
            Some(Ok(msg)) = ws_read.next() => {
                if let Message::Text(text) = msg {
                    let text_str: &str = &text;
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text_str) {
                        match json.get("type").and_then(|t| t.as_str()) {
                            Some("answer") => {
                                if let Some(sdp) = json.get("sdp").and_then(|s| s.as_str()) {
                                    let answer = RTCSessionDescription::answer(sdp.to_string())
                                        .map_err(|e| format!("解析 Answer 失败: {}", e))?;
                                    peer_connection.set_remote_description(answer)
                                        .await
                                        .map_err(|e| format!("设置 RemoteDescription 失败: {}", e))?;
                                    log::info!("[web_drop] Answer received and set.");
                                }
                            }
                            Some("ice") => {
                                let candidate = json.get("candidate")
                                    .and_then(|c| c.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let sdp_mid = json.get("sdpMid")
                                    .and_then(|m| m.as_str())
                                    .map(|s| s.to_string());
                                let sdp_mline_index = json.get("sdpMLineIndex")
                                    .and_then(|i| i.as_u64())
                                    .map(|i| i as u16);

                                let ice_candidate = RTCIceCandidateInit {
                                    candidate,
                                    sdp_mid,
                                    sdp_mline_index,
                                    ..Default::default()
                                };
                                let _ = peer_connection.add_ice_candidate(ice_candidate).await;
                            }
                            _ => {}
                        }
                    }
                }
            }
            // 超时
            _ = tokio::time::sleep_until(deadline) => {
                log::warn!("[web_drop] WebRTC P2P timeout (10s). DataChannel did not open.");
                peer_connection.close().await.ok();
                return Ok(false);
            }
        }
    }

    // ═══ DataChannel 已打开，开始 P2P 传输 ═══
    log::info!("[web_drop] Starting P2P file transfer via DataChannel...");

    // 发送文件元数据
    let meta_json = serde_json::json!({
        "type": "meta",
        "name": file_name,
        "size": file_size
    });
    data_channel.send_text(meta_json.to_string())
        .await
        .map_err(|e| format!("发送 P2P 元数据失败: {}", e))?;

    // 流式读取文件并通过 DataChannel 发送
    let mut file = File::open(&path).await.map_err(|e| format!("打开文件失败: {}", e))?;
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB chunks for DataChannel

    loop {
        let n = file.read(&mut buffer).await.map_err(|e| format!("读取文件失败: {}", e))?;
        if n == 0 {
            // EOF — 发送空消息作为结束标志
            data_channel.send(&bytes::Bytes::new())
                .await
                .map_err(|e| format!("发送 EOF 失败: {}", e))?;
            break;
        }

        let chunk = bytes::Bytes::copy_from_slice(&buffer[..n]);

        // 简单的流控：等待 bufferedAmount 降到合理水平
        while data_channel.buffered_amount().await > 1024 * 1024 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        data_channel.send(&chunk)
            .await
            .map_err(|e| format!("发送数据块失败: {}", e))?;
    }

    log::info!("[web_drop] P2P file transfer completed.");

    // 等一小会儿让最后的数据刷完
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    peer_connection.close().await.ok();

    Ok(true)
}

/// Tier 3: 通过 WebSocket 中继发送加密文件（现有逻辑）
async fn send_via_websocket_relay(
    ws_write: &mut WsSink,
    key: &aes_gcm::aead::generic_array::GenericArray<u8, aes_gcm::aead::consts::U16>,
    path: &std::path::PathBuf,
    file_name: &str,
    file_size: u64,
) -> Result<(), String> {
    // 发送文件元数据
    let cipher = Aes128Gcm::new(key);
    let meta_json = serde_json::json!({
        "type": "meta",
        "name": file_name,
        "size": file_size
    });
    ws_write.send(Message::Text(meta_json.to_string().into()))
        .await
        .map_err(|e| format!("发送元数据失败: {}", e))?;

    // 流式读取、加密、发送文件块
    let mut file = File::open(&path).await.map_err(|e| format!("打开文件失败: {}", e))?;
    let mut buffer = vec![0u8; 1024 * 1024]; // 1MB chunks

    loop {
        let n = file.read(&mut buffer).await.map_err(|e| format!("读取文件失败: {}", e))?;

        if n == 0 {
            log::info!("[web_drop] File EOF reached. Sending close.");
            let _ = ws_write.send(Message::Binary(vec![].into())).await;
            let _ = ws_write.close().await;
            break;
        }

        let chunk = &buffer[..n];

        // 加密
        let nonce = Aes128Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, chunk)
            .map_err(|e| format!("加密失败: {:?}", e))?;

        // 拼接 nonce + ciphertext
        let mut payload = Vec::with_capacity(12 + ciphertext.len());
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&ciphertext);

        ws_write.send(Message::Binary(payload.into()))
            .await
            .map_err(|e| format!("发送数据块失败: {}", e))?;

        // 短暂延迟防止洪泛
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    log::info!("[web_drop] WebSocket relay stream completed successfully.");
    Ok(())
}
