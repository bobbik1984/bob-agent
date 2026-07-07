use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct DeviceIdentityState(pub Mutex<Option<SigningKey>>);

#[derive(Serialize, Deserialize)]
struct EncryptedKeyData {
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn get_keys_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("workspace_config")
        .join("device_identity.json")
}

fn derive_key(pin: &str, salt_str: &str) -> Result<[u8; 32], String> {
    let argon2 = Argon2::default();
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(pin.as_bytes(), salt_str.as_bytes(), &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

#[tauri::command]
pub fn check_device_keys_initialized(app: AppHandle) -> bool {
    get_keys_path(&app).exists()
}

#[tauri::command]
pub fn init_device_keys(
    pin: String,
    app: AppHandle,
    state: tauri::State<'_, DeviceIdentityState>,
) -> Result<(), String> {
    let path = get_keys_path(&app);
    if path.exists() {
        return Err("Keys already initialized. Use unlock_device_keys instead.".to_string());
    }

    // 1. Generate new Ed25519 keypair
    let mut csprng = rand::rngs::OsRng;
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);

    // 2. Generate Salt & Nonce
    let mut salt_bytes = [0u8; 16];
    csprng.fill_bytes(&mut salt_bytes);
    let salt = BASE64.encode(salt_bytes);
    let mut nonce_bytes = [0u8; 12];
    csprng.fill_bytes(&mut nonce_bytes);
    let nonce_str = BASE64.encode(nonce_bytes);

    // 3. Derive AES key from PIN + Salt
    let aes_key = derive_key(&pin, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&aes_key).map_err(|e| e.to_string())?;

    // 4. Encrypt the private key
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, key_bytes.as_ref())
        .map_err(|e| e.to_string())?;

    // 5. Save to disk
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let data = EncryptedKeyData {
        salt,
        nonce: nonce_str,
        ciphertext: BASE64.encode(ciphertext),
    };

    let json_str = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(path, json_str).map_err(|e| e.to_string())?;

    // 6. Keep in memory
    *state.0.lock().unwrap() = Some(signing_key);

    Ok(())
}

#[tauri::command]
pub fn unlock_device_keys(
    pin: String,
    app: AppHandle,
    state: tauri::State<'_, DeviceIdentityState>,
) -> Result<(), String> {
    let path = get_keys_path(&app);
    if !path.exists() {
        return Err("Keys not initialized.".to_string());
    }

    let json_str = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let data: EncryptedKeyData = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;

    let nonce_bytes = BASE64.decode(data.nonce).map_err(|e| e.to_string())?;
    let ciphertext = BASE64.decode(data.ciphertext).map_err(|e| e.to_string())?;

    let aes_key = derive_key(&pin, &data.salt)?;
    let cipher = Aes256Gcm::new_from_slice(&aes_key).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let key_bytes = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| "Incorrect PIN".to_string())?;

    if key_bytes.len() != 32 {
        return Err("Invalid decrypted key length".to_string());
    }

    let mut fixed_key = [0u8; 32];
    fixed_key.copy_from_slice(&key_bytes);

    let signing_key = SigningKey::from_bytes(&fixed_key);

    *state.0.lock().unwrap() = Some(signing_key);

    Ok(())
}

#[tauri::command]
pub fn reset_device_keys(
    app: AppHandle,
    state: tauri::State<'_, DeviceIdentityState>,
) -> Result<(), String> {
    // 1. Tell VPS to unregister (placeholder for now, will implement when relay is ready)
    // 2. Remove local file
    let path = get_keys_path(&app);
    if path.exists() {
        let _ = fs::remove_file(path);
    }

    // 3. Clear memory
    *state.0.lock().unwrap() = None;

    Ok(())
}

use std::net::UdpSocket;

#[derive(Serialize)]
pub struct PairingPayload {
    pub device_id: String,
    pub public_key: String,
    pub local_ips: Vec<String>,
    pub port: u16,
    pub relay: String,
}

fn get_local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

#[tauri::command]
pub fn get_pairing_payload(
    state: tauri::State<'_, DeviceIdentityState>,
) -> Result<PairingPayload, String> {
    let guard = state.0.lock().unwrap();
    let signing_key = guard.as_ref().ok_or("Keys not unlocked")?;

    let verifying_key = VerifyingKey::from(signing_key);
    let pub_key_bytes = verifying_key.to_bytes();
    let b64_pub = BASE64.encode(pub_key_bytes);

    let mut local_ips = Vec::new();
    if let Some(ip) = get_local_ip() {
        local_ips.push(ip);
    }

    let relay = option_env!("BOB_RELAY_SECRET")
        .map(|_| "wss://relay.bobbik.org")
        .unwrap_or("wss://relay.bobbik.org");

    Ok(PairingPayload {
        device_id: b64_pub.clone(),
        public_key: b64_pub,
        local_ips,
        port: 3722,
        relay: relay.to_string(),
    })
}
