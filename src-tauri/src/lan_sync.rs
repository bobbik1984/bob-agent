use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

#[derive(Serialize, Deserialize, Debug)]
struct LanBroadcastPayload {
    magic: String, // "BOB_SYNC"
    device_id: String,
    port: u16,
    platform: String,
}

pub struct LanSyncEngine {
    device_id: String,
    running: Arc<Mutex<bool>>,
}

impl LanSyncEngine {
    pub fn new(device_id: String) -> Self {
        Self {
            device_id,
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start_broadcast(&self, port: u16) {
        let mut running = self.running.lock().unwrap();
        if *running {
            return;
        }
        *running = true;

        let running_clone = self.running.clone();
        let device_id = self.device_id.clone();

        tauri::async_runtime::spawn(async move {
            let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP socket");
            socket.set_broadcast(true).expect("Failed to set broadcast");
            
            println!("📢 LAN Sync Broadcast started on UDP port 3723...");

            let payload = LanBroadcastPayload {
                magic: "BOB_SYNC".to_string(),
                device_id,
                port,
                platform: "desktop".to_string(),
            };
            
            let json_str = serde_json::to_string(&payload).unwrap();
            let bytes = json_str.as_bytes();

            while *running_clone.lock().unwrap() {
                if let Err(e) = socket.send_to(bytes, "255.255.255.255:3723") {
                    println!("⚠️ UDP broadcast failed: {}", e);
                }
                sleep(Duration::from_secs(5)).await;
            }
            println!("🛑 LAN Sync Broadcast stopped.");
        });
    }

    pub fn stop_broadcast(&self) {
        let mut running = self.running.lock().unwrap();
        *running = false;
    }

    pub fn start_listen_broadcast<F>(&self, on_discovered: F)
    where
        F: Fn(String, String, u16) + Send + 'static,
    {
        tauri::async_runtime::spawn(async move {
            let socket = UdpSocket::bind("0.0.0.0:3723").expect("Failed to bind UDP listener");
            let mut buf = [0; 1024];
            println!("🎧 Listening for LAN Sync Broadcast on UDP port 3723...");
            loop {
                if let Ok((amt, src)) = socket.recv_from(&mut buf) {
                    if let Ok(json_str) = std::str::from_utf8(&buf[..amt]) {
                        if let Ok(payload) = serde_json::from_str::<LanBroadcastPayload>(json_str) {
                            if payload.magic == "BOB_SYNC" {
                                println!("🟢 Discovered device {} at {}:{}", payload.device_id, src.ip(), payload.port);
                                on_discovered(payload.device_id, src.ip().to_string(), payload.port);
                            }
                        }
                    }
                }
            }
        });
    }
}
