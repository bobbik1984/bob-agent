use std::time::Duration;
use serde_json::{json, Value};
use reqwest::Client;
use tauri::State;
use std::sync::Arc;
use crate::wechat::WechatState;

use super::accounts::{list_wechat_account_ids, load_wechat_account, WechatAccountData};

const QR_API_BASE: &str = "https://ilinkai.weixin.qq.com";
const BOT_TYPE: &str = "3";

#[tauri::command]
pub async fn wechat_get_login_qr() -> Result<Value, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/ilink/bot/get_bot_qrcode?bot_type={}", QR_API_BASE, BOT_TYPE);

    let tokens: Vec<String> = {
        let ids = list_wechat_account_ids();
        ids.into_iter()
            .filter_map(|id| load_wechat_account(&id))
            .filter_map(|acc| acc.token)
            .take(10)
            .collect()
    };

    let res = client.post(&url)
        .json(&json!({ "local_token_list": tokens }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("QR API Error: {}", res.status()));
    }

    let mut data: Value = res.json().await.map_err(|e| e.to_string())?;

    // The API's qrcode_img_content is an HTML page URL, not a direct image.
    // Generate the QR code locally from the qrcode hash value.
    if let Some(qr_hash) = data.get("qrcode").and_then(|v| v.as_str()) {
        let scan_url = format!(
            "https://liteapp.weixin.qq.com/q/7GiQu1?qrcode={}&bot_type={}",
            qr_hash, BOT_TYPE
        );
        match generate_qr_base64(&scan_url) {
            Ok(data_uri) => {
                data.as_object_mut().unwrap()
                    .insert("qrcode_img_content".to_string(), json!(data_uri));
            }
            Err(e) => {
                log::warn!("Failed to generate QR locally: {}", e);
            }
        }
    }

    Ok(data)
}

fn generate_qr_base64(content: &str) -> Result<String, String> {
    use qrcode::QrCode;
    use qrcode::render::svg;

    let code = QrCode::new(content.as_bytes()).map_err(|e| e.to_string())?;
    let svg_str = code.render::<svg::Color>()
        .min_dimensions(200, 200)
        .quiet_zone(true)
        .build();

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg_str.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{}", b64))
}

#[tauri::command]
pub async fn wechat_check_login_status(qrcode: String, state: State<'_, Arc<WechatState>>) -> Result<Value, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(35))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/ilink/bot/get_qrcode_status?qrcode={}", QR_API_BASE, urlencoding::encode(&qrcode));

    let res = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            if e.is_timeout() {
                return Ok(json!({ "status": "wait" }));
            }
            return Err(e.to_string());
        }
    };

    if !res.status().is_success() {
        return Err(format!("Status API Error: {}", res.status()));
    }

    let mut data: Value = res.json().await.map_err(|e| e.to_string())?;

    if let Some(status) = data.get("status").and_then(|v| v.as_str()) {
        if status == "confirmed" || status == "binded_redirect" {
            let account_id = data.get("ilink_bot_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let token = data.get("bot_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let baseurl = data.get("baseurl").and_then(|v| v.as_str()).unwrap_or(QR_API_BASE).to_string();
            let user_id = data.get("ilink_user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();

            if !account_id.is_empty() && !token.is_empty() {
                // Save credentials via WechatAccountData struct
                let update = WechatAccountData {
                    account_id: Some(account_id.clone()),
                    token: Some(token),
                    base_url: Some(baseurl),
                    user_id: if user_id.is_empty() { None } else { Some(user_id) },
                    saved_at: None,
                };
                super::accounts::save_wechat_account(&account_id, update);
                super::accounts::register_wechat_account_id(&account_id);
                
                // Switch active account to this one
                *state.account_id.write().unwrap() = Some(account_id.clone());
                
                // Restart monitor with new account
                super::monitor::start_monitor(state.inner().clone());

                // We want frontend to know the accountId
                data.as_object_mut().unwrap().insert("accountId".to_string(), json!(account_id));
            }
        }
    }

    Ok(data)
}
