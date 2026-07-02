use reqwest::{Client, ClientBuilder, Method, header::HeaderMap};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use crate::read_config;
use log::{debug, error};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug)]
pub struct ProxyRequestPayload {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProxyResponsePayload {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref DIRECT_CLIENT: Client = ClientBuilder::new()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();
}

/// Returns true if proxy tunnel is enabled in config
pub fn is_tunnel_enabled() -> bool {
    let config = read_config();
    if let Some(val) = config.get("proxyTunnelEnabled") {
        val.as_bool().unwrap_or(false)
    } else {
        false
    }
}

/// A custom helper to perform requests either directly or via our proxy tunnel
pub async fn send_request(
    method: Method,
    url: &str,
    headers: HeaderMap,
    body: Option<reqwest::Body>,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    if is_tunnel_enabled() {
        // 伪装成普通 HTTPS 流量发送给 VPS，利用 Header 传递真实目标信息
        let proxy_endpoint = "https://village.bobbik.org/api/proxy";
        
        let mut req = DIRECT_CLIENT.post(proxy_endpoint).timeout(timeout);
        
        // 传递目标 URL 和 方法
        req = req.header("X-Proxy-Target-Url", url);
        req = req.header("X-Proxy-Target-Method", method.as_str());

        // 传递真实的 Headers
        for (k, v) in headers.iter() {
            // Encode header names to avoid conflicts, or prefix them
            req = req.header(format!("X-Proxy-Pass-{}", k.as_str()), v.to_str().unwrap_or(""));
        }

        if let Some(b) = body {
            req = req.body(b);
        }

        let res = req.send().await.map_err(|e| format!("Proxy Tunnel Error: {}", e))?;

        if !res.status().is_success() {
            return Err(format!("Proxy Tunnel returned error status: {}", res.status()));
        }

        Ok(res)
    } else {
        let mut req = DIRECT_CLIENT.request(method, url).timeout(timeout);
        for (k, v) in headers {
            if let Some(k) = k {
                req = req.header(k, v);
            }
        }
        if let Some(b) = body {
            req = req.body(b);
        }
        
        req.send().await.map_err(|e| format!("Direct Request Error: {}", e))
    }
}
