use base64::{engine::general_purpose::STANDARD, Engine as _};
use log::{debug, error};
use reqwest::{header, Client};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::*;

const ILINK_APP_ID: &str = "openclaw_win"; // Need to check if there is a specific ID we should use. Actually, wechat_bot gets this from openclaw-weixin package.json. Let's leave empty or default if none.
                                           // In wechat_bot/src/api/api.ts, ILINK_APP_ID was from package.json ilink_appid.

fn random_wechat_uin() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let uin = (now % std::u32::MAX as u128) as u32;
    STANDARD.encode(uin.to_string())
}

pub fn build_base_info() -> BaseInfo {
    BaseInfo {
        channel_version: Some("2.4.3".to_string()),
        bot_agent: Some("Bob-Agent (Rust)".to_string()),
    }
}

pub struct WechatApi {
    pub base_url: String,
    pub token: Option<String>,
}

impl WechatApi {
    pub fn new(base_url: String, token: Option<String>) -> Self {
        Self { base_url, token }
    }

    fn build_headers(&self) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "AuthorizationType",
            header::HeaderValue::from_static("ilink_bot_token"),
        );

        let uin = random_wechat_uin();
        if let Ok(val) = header::HeaderValue::from_str(&uin) {
            headers.insert("X-WECHAT-UIN", val);
        }

        // Common headers
        headers.insert("iLink-App-Id", header::HeaderValue::from_static("bot"));
        headers.insert(
            "iLink-App-ClientVersion",
            header::HeaderValue::from_static("132099"),
        );

        if let Some(token) = &self.token {
            let auth = format!("Bearer {}", token.trim());
            if let Ok(val) = header::HeaderValue::from_str(&auth) {
                headers.insert(header::AUTHORIZATION, val);
            }
        }

        headers
    }

    async fn post<T: Serialize, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &T,
        timeout: Duration,
    ) -> Result<R, String> {
        let mut url = self.base_url.clone();
        if !url.ends_with('/') {
            url.push('/');
        }
        url.push_str(endpoint);

        let headers = self.build_headers();
        let body_json = serde_json::to_string(body).unwrap_or_default();

        debug!("POST {} body length: {}", url, body_json.len());

        let res = crate::tunnel::send_request(
            reqwest::Method::POST,
            &url,
            headers,
            Some(reqwest::Body::from(body_json.into_bytes())),
            timeout,
        )
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        debug!("Response status: {} text length: {}", status, text.len());

        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status, text));
        }

        serde_json::from_str::<R>(&text).map_err(|e| format!("JSON parse error: {}", e))
    }

    pub async fn get_updates(
        &self,
        req: GetUpdatesReq,
        timeout_ms: u64,
    ) -> Result<GetUpdatesResp, String> {
        let timeout = Duration::from_millis(timeout_ms);
        match self.post("ilink/bot/getupdates", &req, timeout).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                if e.contains("timeout") || e.contains("deadline") || e.contains("Request failed") {
                    // Long polling timeout is normal, return empty resp to retry
                    debug!("getUpdates: client-side timeout, returning empty response");
                    Ok(GetUpdatesResp {
                        ret: Some(0),
                        errcode: None,
                        errmsg: None,
                        msgs: Some(vec![]),
                        sync_buf: req.sync_buf,
                        get_updates_buf: req.get_updates_buf,
                        longpolling_timeout_ms: None,
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn send_message(
        &self,
        mut req: SendMessageReq,
        timeout_ms: u64,
    ) -> Result<SendMessageResp, String> {
        let timeout = Duration::from_millis(timeout_ms);
        req.base_info = Some(build_base_info());

        let mut url = self.base_url.clone();
        if !url.ends_with('/') {
            url.push('/');
        }
        url.push_str("ilink/bot/sendmessage");

        let headers = self.build_headers();
        let body_json = serde_json::to_string(&req).unwrap_or_default();

        log::info!("[wechat-api] sendmessage POST {} body: {}", url, body_json);

        let res = crate::tunnel::send_request(
            reqwest::Method::POST,
            &url,
            headers,
            Some(reqwest::Body::from(body_json)),
            timeout,
        )
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

        let status = res.status();
        let text = res.text().await.unwrap_or_default();

        log::info!(
            "[wechat-api] sendmessage response: status={} body={}",
            status,
            text
        );

        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status, text));
        }

        serde_json::from_str::<SendMessageResp>(&text)
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    pub async fn get_config(
        &self,
        mut req: GetConfigReq,
        timeout_ms: u64,
    ) -> Result<GetConfigResp, String> {
        let timeout = Duration::from_millis(timeout_ms);
        req.base_info = Some(build_base_info());
        self.post("ilink/bot/getconfig", &req, timeout).await
    }

    pub async fn send_typing(
        &self,
        mut req: SendTypingReq,
        timeout_ms: u64,
    ) -> Result<SendTypingResp, String> {
        let timeout = Duration::from_millis(timeout_ms);
        req.base_info = Some(build_base_info());
        self.post("ilink/bot/sendtyping", &req, timeout).await
    }

    pub async fn notify_start(&self, timeout_ms: u64) -> Result<NotifyStartResp, String> {
        let timeout = Duration::from_millis(timeout_ms);
        let req = NotifyStartReq {
            base_info: Some(build_base_info()),
        };
        self.post("ilink/bot/msg/notifystart", &req, timeout).await
    }

    pub async fn notify_stop(&self, timeout_ms: u64) -> Result<NotifyStopResp, String> {
        let timeout = Duration::from_millis(timeout_ms);
        let req = NotifyStopReq {
            base_info: Some(build_base_info()),
        };
        self.post("ilink/bot/msg/notifystop", &req, timeout).await
    }

    /// 获取 CDN 上传预签名 URL (用于文件/图片/视频上传)
    pub async fn get_upload_url(
        &self,
        mut req: GetUploadUrlReq,
        timeout_ms: u64,
    ) -> Result<GetUploadUrlResp, String> {
        let timeout = Duration::from_millis(timeout_ms);
        req.base_info = Some(build_base_info());
        self.post("ilink/bot/getuploadurl", &req, timeout).await
    }
}
