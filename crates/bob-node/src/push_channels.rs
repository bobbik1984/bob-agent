//! VAPID Web Push, Official WeChat Bridge (`/v1/chat`), and Outbound Notification Adapters.
//!
//! Enforces:
//! - ECDSA P-256 VAPID keypair generation and standard RFC 8292 ES256 JWT signing.
//! - Exporting 65-byte uncompressed public key (`0x04 || X || Y`) in URL-safe base64 for iOS 16.4+ / Chrome.
//! - Validating WebPush subscription endpoints via `net_guard` (strictly public HTTPS).
//! - Official WeChat ClawBot / iLink Bridge compatible HTTP API (`/v1/health`, `/v1/conversations`, `/v1/chat`).
//! - SSE (`Accept: text/event-stream`) and JSON (`Accept: application/json`) streaming modes.
//! - Action Gate enforcement: when tools trigger `RequiresApproval`, returns a prompt awaiting confirmation,
//!   never bypassing Action Gates over WeChat/Webhook.
//! - Outbound Feishu / WeCom webhook adapter respecting Proactivity quiet hours.

use crate::{now_millis, NodeContext};
use bob_runtime::base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bob_runtime::base64::Engine;
use bob_runtime::growth::{ProactivityStore, ProactiveKind};
use bob_runtime::reqwest;
use ring::rand::SystemRandom;
use ring::signature::{
    EcdsaKeyPair, KeyPair, UnparsedPublicKey, ECDSA_P256_SHA256_FIXED,
    ECDSA_P256_SHA256_FIXED_SIGNING,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// 1. VAPID Web Push Store & Crypto
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebPushKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebPushSubscription {
    pub endpoint: String,
    pub keys: WebPushKeys,
    #[serde(default = "now_millis")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundWebhookRecord {
    pub id: String,
    pub platform: String, // "feishu" | "wecom" | "custom"
    pub url: String,
    pub created_at: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PushStoreData {
    pub vapid_pkcs8_base64: String,
    pub vapid_public_key_base64url: String,
    pub subscriptions: Vec<WebPushSubscription>,
    pub webhooks: Vec<OutboundWebhookRecord>,
}

#[derive(Debug, Clone)]
pub struct PushStore {
    file_path: PathBuf,
    pub data: PushStoreData,
}

impl PushStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("push_store.json");

        let mut data = if file_path.is_file() {
            let content = std::fs::read_to_string(&file_path).map_err(|_| "PUSH_STORE_READ_FAILED")?;
            serde_json::from_str::<PushStoreData>(&content).unwrap_or_default()
        } else {
            PushStoreData::default()
        };

        if data.vapid_pkcs8_base64.is_empty() || data.vapid_public_key_base64url.is_empty() {
            let (pkcs8_b64, pub_b64url) = generate_vapid_keypair()?;
            data.vapid_pkcs8_base64 = pkcs8_b64;
            data.vapid_public_key_base64url = pub_b64url;
            let serialized = serde_json::to_string_pretty(&data).map_err(|_| "PUSH_STORE_SERIALIZE_FAILED")?;
            let tmp_path = file_path.with_extension("json.tmp");
            let _ = std::fs::write(&tmp_path, serialized.as_bytes());
            let _ = std::fs::rename(&tmp_path, &file_path);
        }

        Ok(Self { file_path, data })
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data).map_err(|_| "PUSH_STORE_SERIALIZE_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes()).map_err(|_| "PUSH_STORE_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "PUSH_STORE_WRITE_FAILED")?;
        Ok(())
    }

    pub fn public_key(&self) -> &str {
        &self.data.vapid_public_key_base64url
    }

    pub fn add_subscription(&mut self, sub: WebPushSubscription) -> Result<(), &'static str> {
        bob_runtime::net_guard::validate_public_http_url(&sub.endpoint)?;
        if !sub.endpoint.to_ascii_lowercase().starts_with("https://") {
            return Err("PUSH_ENDPOINT_MUST_BE_HTTPS");
        }
        if let Some(pos) = self.data.subscriptions.iter().position(|s| s.endpoint == sub.endpoint) {
            self.data.subscriptions[pos] = sub;
        } else {
            self.data.subscriptions.push(sub);
        }
        self.save_atomic()
    }

    pub fn remove_subscription(&mut self, endpoint: &str) -> Result<bool, &'static str> {
        let initial_len = self.data.subscriptions.len();
        self.data.subscriptions.retain(|s| s.endpoint != endpoint);
        let removed = self.data.subscriptions.len() < initial_len;
        if removed {
            self.save_atomic()?;
        }
        Ok(removed)
    }

    pub fn list_subscriptions(&self) -> &[WebPushSubscription] {
        &self.data.subscriptions
    }

    pub fn add_webhook(&mut self, record: OutboundWebhookRecord) -> Result<(), &'static str> {
        bob_runtime::net_guard::validate_public_http_url(&record.url)?;
        if !record.url.to_ascii_lowercase().starts_with("https://") {
            return Err("WEBHOOK_URL_MUST_BE_HTTPS");
        }
        if let Some(pos) = self.data.webhooks.iter().position(|w| w.id == record.id) {
            self.data.webhooks[pos] = record;
        } else {
            self.data.webhooks.push(record);
        }
        self.save_atomic()
    }

    pub fn list_webhooks(&self) -> &[OutboundWebhookRecord] {
        &self.data.webhooks
    }

    pub fn sign_vapid_jwt(&self, endpoint: &str, now_secs: i64) -> Result<(String, String), &'static str> {
        let pkcs8_bytes = URL_SAFE_NO_PAD
            .decode(&self.data.vapid_pkcs8_base64)
            .map_err(|_| "VAPID_DECODE_FAILED")?;
        let rng = SystemRandom::new();
        let keypair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8_bytes, &rng)
            .map_err(|_| "VAPID_KEY_INVALID")?;

        let aud = extract_origin(endpoint)?;
        let header = json!({
            "alg": "ES256",
            "typ": "JWT"
        });
        let claims = json!({
            "aud": aud,
            "exp": now_secs + 12 * 3600,
            "sub": "mailto:admin@bob.local"
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
        let claims_b64 = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
        let signing_input = format!("{header_b64}.{claims_b64}");

        let signature = keypair
            .sign(&rng, signing_input.as_bytes())
            .map_err(|_| "VAPID_SIGN_FAILED")?;
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.as_ref());

        let jwt = format!("{signing_input}.{sig_b64}");
        Ok((jwt, self.data.vapid_public_key_base64url.clone()))
    }

    pub fn create_vapid_auth_header(&self, endpoint: &str, now_secs: i64) -> Result<String, &'static str> {
        let (jwt, pubkey) = self.sign_vapid_jwt(endpoint, now_secs)?;
        Ok(format!("vapid t={jwt}, k={pubkey}"))
    }

    pub fn send_outbound_notification(
        &self,
        data_dir: &Path,
        content: &str,
        urgent: bool,
        now_ms: i64,
    ) -> Result<usize, &'static str> {
        let pro = ProactivityStore::open(data_dir)?;
        let kind = if urgent {
            ProactiveKind::Urgent
        } else {
            ProactiveKind::MorningBrief
        };
        if !pro.config.can_send_proactive(kind, now_ms) {
            return Ok(0);
        }

        let mut sent = 0;
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|_| "CLIENT_BUILD_FAILED")?;

        for hook in &self.data.webhooks {
            if !hook.enabled {
                continue;
            }
            let payload = match hook.platform.as_str() {
                "feishu" => json!({
                    "msg_type": "text",
                    "content": { "text": content }
                }),
                "wecom" => json!({
                    "msg_type": "text",
                    "text": { "content": content }
                }),
                _ => json!({ "text": content }),
            };
            if let Ok(resp) = client.post(&hook.url).json(&payload).send() {
                if resp.status().is_success() {
                    sent += 1;
                }
            }
        }

        Ok(sent)
    }
}

pub fn generate_vapid_keypair() -> Result<(String, String), &'static str> {
    let rng = SystemRandom::new();
    let pkcs8_doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
        .map_err(|_| "VAPID_GENERATE_FAILED")?;
    let keypair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8_doc.as_ref(), &rng)
        .map_err(|_| "VAPID_PARSE_FAILED")?;

    let pubkey_bytes = keypair.public_key().as_ref();
    if pubkey_bytes.len() != 65 || pubkey_bytes[0] != 0x04 {
        return Err("VAPID_INVALID_PUBLIC_KEY");
    }

    let pkcs8_b64 = URL_SAFE_NO_PAD.encode(pkcs8_doc.as_ref());
    let pub_b64url = URL_SAFE_NO_PAD.encode(pubkey_bytes);
    Ok((pkcs8_b64, pub_b64url))
}

pub fn verify_vapid_jwt(pubkey_b64url: &str, jwt: &str) -> Result<bool, &'static str> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return Ok(false);
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).map_err(|_| "SIG_DECODE_FAILED")?;
    let pub_bytes = URL_SAFE_NO_PAD.decode(pubkey_b64url).map_err(|_| "PUBKEY_DECODE_FAILED")?;

    let verifier = UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, &pub_bytes);
    match verifier.verify(signing_input.as_bytes(), &sig_bytes) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn extract_origin(endpoint: &str) -> Result<String, &'static str> {
    let trimmed = endpoint.trim();
    if let Some(pos) = trimmed.find("://") {
        let after_scheme = &trimmed[pos + 3..];
        let host = after_scheme.split('/').next().unwrap_or(after_scheme);
        let scheme = &trimmed[..pos];
        Ok(format!("{scheme}://{host}"))
    } else {
        Err("INVALID_ENDPOINT_URL")
    }
}

// ---------------------------------------------------------------------------
// 2. Official WeChat ClawBot / iLink Bridge HTTP Handlers
// ---------------------------------------------------------------------------

pub fn handle_wechat_health() -> (u16, Value) {
    (
        200,
        json!({
            "ok": true,
            "service": "bob-jarvis",
            "version": "1.0.1"
        }),
    )
}

pub fn handle_wechat_conversations(
    ctx: &mut NodeContext,
    method: &str,
    body: &Value,
) -> Result<(u16, Value), &'static str> {
    let at = now_millis();
    if method == "GET" {
        let cat = ctx.app.catalog_snapshot().map_err(|_| "CATALOG_READ_FAILED")?;
        let convs: Vec<Value> = cat
            .conversations
            .into_iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "title": c.title,
                    "updated_at": c.updated_at,
                    "project_id": c.project_id
                })
            })
            .collect();
        Ok((200, json!({ "conversations": convs })))
    } else if method == "POST" {
        let title = body.get("title").and_then(Value::as_str).unwrap_or("WeChat Conversation");
        let conv_id = format!("wechat-conv-{at}");
        ctx.app
            .create_conversation(&conv_id, None, title, at)
            .map_err(|_| "CONVERSATION_CREATE_FAILED")?;
        Ok((
            200,
            json!({
                "id": conv_id,
                "conversation_id": conv_id,
                "title": title
            }),
        ))
    } else {
        Err("METHOD_NOT_ALLOWED")
    }
}

pub fn handle_wechat_chat(
    mut stream: TcpStream,
    state: &Arc<Mutex<NodeContext>>,
    body: &Value,
    accept_header: Option<&str>,
) {
    let is_sse = accept_header
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("text/event-stream");

    let message = body
        .get("message")
        .or_else(|| body.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();

    if message.is_empty() {
        if is_sse {
            let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"error\":\"MESSAGE_REQUIRED\"}\n\n";
            let _ = stream.write_all(resp.as_bytes());
        } else {
            let payload = json!({ "ok": false, "error": "MESSAGE_REQUIRED" }).to_string();
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
        }
        return;
    }

    let at = now_millis();
    let conv_id = body
        .get("conversation_id")
        .or_else(|| body.get("conversationId"))
        .or_else(|| body.get("sessionId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if let Ok(guard) = state.lock() {
                guard.app.default_session().unwrap_or_else(|_| format!("wechat-{at}"))
            } else {
                format!("wechat-{at}")
            }
        });

    let chat_args = json!({
        "sessionId": conv_id,
        "content": message
    });

    if is_sse {
        let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache, no-transform\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(sse_headers.as_bytes());
        let _ = stream.flush();

        let res = crate::jarvis::chat(state, &chat_args);
        match res {
            Ok(v) => {
                let reply_str = v.get("reply").and_then(Value::as_str).unwrap_or("");
                let approvals = v.get("approvals").cloned().unwrap_or_else(|| json!([]));
                let has_approvals = approvals.as_array().map(|a| !a.is_empty()).unwrap_or(false);

                let final_reply = if has_approvals {
                    "Action paused awaiting your confirmation in Bob"
                } else {
                    reply_str
                };

                let delta_event = json!({
                    "type": "text_delta",
                    "text": final_reply
                });
                let _ = stream.write_all(format!("data: {}\n\n", serde_json::to_string(&delta_event).unwrap()).as_bytes());

                let done_event = json!({
                    "type": "done",
                    "conversation_id": conv_id,
                    "reply": final_reply,
                    "approvals": approvals
                });
                let _ = stream.write_all(format!("data: {}\n\n", serde_json::to_string(&done_event).unwrap()).as_bytes());
            }
            Err(e) => {
                let err_event = json!({
                    "type": "error",
                    "error": e
                });
                let _ = stream.write_all(format!("data: {}\n\n", serde_json::to_string(&err_event).unwrap()).as_bytes());
            }
        }
    } else {
        let res = crate::jarvis::chat(state, &chat_args);
        let (status, payload) = match res {
            Ok(v) => {
                let reply_str = v.get("reply").and_then(Value::as_str).unwrap_or("");
                let approvals = v.get("approvals").cloned().unwrap_or_else(|| json!([]));
                let has_approvals = approvals.as_array().map(|a| !a.is_empty()).unwrap_or(false);

                let final_reply = if has_approvals {
                    "Action paused awaiting your confirmation in Bob".to_string()
                } else {
                    reply_str.to_string()
                };

                (
                    200,
                    json!({
                        "ok": true,
                        "conversation_id": conv_id,
                        "reply": final_reply,
                        "approvals": approvals
                    }),
                )
            }
            Err(e) => (400, json!({ "ok": false, "error": e })),
        };

        let json_str = payload.to_string();
        let resp = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json_str.len(),
            json_str
        );
        let _ = stream.write_all(resp.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vapid_p256_keypair_generation_and_jwt_signing() {
        let (pkcs8_b64, pubkey_b64url) = generate_vapid_keypair().expect("generate vapid keypair");
        assert!(!pkcs8_b64.is_empty());
        assert!(!pubkey_b64url.is_empty());

        let pub_bytes = URL_SAFE_NO_PAD.decode(&pubkey_b64url).expect("decode pubkey");
        assert_eq!(pub_bytes.len(), 65, "P-256 uncompressed public key must be exactly 65 bytes");
        assert_eq!(pub_bytes[0], 0x04, "P-256 uncompressed public key must start with 0x04 tag");

        let dir = tempfile::tempdir().expect("tempdir");
        let store = PushStore::open(dir.path()).expect("open push store");
        assert_eq!(store.public_key().len(), pubkey_b64url.len());

        let header = store
            .create_vapid_auth_header("https://updates.push.services.mozilla.com/wpush/v1/sub123", 1762000000)
            .expect("create vapid header");
        assert!(header.starts_with("vapid "));
        assert!(header.contains("t="));
        assert!(header.contains("k="));

        // Extract t=<jwt>
        let parts: Vec<&str> = header.strip_prefix("vapid ").unwrap().split(',').collect();
        let mut jwt = "";
        for p in parts {
            let p = p.trim();
            if let Some(token) = p.strip_prefix("t=") {
                jwt = token.trim();
            }
        }
        assert!(!jwt.is_empty());
        let valid = verify_vapid_jwt(&store.public_key(), jwt).expect("verify vapid jwt");
        assert!(valid, "Generated VAPID JWT must verify successfully against public key");

        // Tampered JWT should fail verification
        let tampered_jwt = format!("{jwt}x");
        let invalid = verify_vapid_jwt(&store.public_key(), &tampered_jwt).unwrap_or(false);
        assert!(!invalid, "Tampered JWT must not verify");
    }

    #[test]
    fn test_push_subscription_persistence_and_netguard_enforcement() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = PushStore::open(dir.path()).expect("open push store");

        // Valid public HTTPS subscription
        let valid_sub = WebPushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/valid-client-sub-1".to_string(),
            keys: WebPushKeys {
                p256dh: "BMc...".to_string(),
                auth: "auth123".to_string(),
            },
            created_at: now_millis(),
        };
        store.add_subscription(valid_sub.clone()).expect("valid subscription must succeed");
        assert_eq!(store.list_subscriptions().len(), 1);

        // SSRF / Loopback endpoint must be blocked by net_guard
        let loopback_sub = WebPushSubscription {
            endpoint: "http://127.0.0.1:8080/push".to_string(),
            keys: WebPushKeys {
                p256dh: "k".to_string(),
                auth: "a".to_string(),
            },
            created_at: now_millis(),
        };
        assert!(store.add_subscription(loopback_sub).is_err(), "Loopback push endpoint must be rejected");

        // Private IP RFC 1918 must be blocked by net_guard
        let private_sub = WebPushSubscription {
            endpoint: "https://192.168.1.55/push".to_string(),
            keys: WebPushKeys {
                p256dh: "k".to_string(),
                auth: "a".to_string(),
            },
            created_at: now_millis(),
        };
        assert!(store.add_subscription(private_sub).is_err(), "Private IP push endpoint must be rejected");

        // Remove subscription
        let removed = store.remove_subscription(&valid_sub.endpoint).expect("remove subscription");
        assert!(removed);
        assert_eq!(store.list_subscriptions().len(), 0);
    }

    #[test]
    fn test_outbound_webhook_and_quiet_hours_backoff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = PushStore::open(dir.path()).expect("open push store");

        // Add valid feishu webhook
        let hook = OutboundWebhookRecord {
            id: "wh-1".to_string(),
            platform: "feishu".to_string(),
            url: "https://open.feishu.cn/open-apis/bot/v2/hook/valid-uuid".to_string(),
            enabled: true,
            created_at: now_millis(),
        };
        store.add_webhook(hook).expect("save valid webhook");
        assert_eq!(store.list_webhooks().len(), 1);

        // SSRF webhook rejected
        let ssrf_hook = OutboundWebhookRecord {
            id: "wh-ssrf".to_string(),
            platform: "custom".to_string(),
            url: "http://127.0.0.1/hook".to_string(),
            enabled: true,
            created_at: now_millis(),
        };
        assert!(store.add_webhook(ssrf_hook).is_err(), "Loopback webhook URL must be rejected");

        // Quiet hours: set quiet hours 22:00-08:00
        let mut pro = ProactivityStore::open(dir.path()).expect("proactivity store");
        pro.set_config(Some(0.9), Some("22:00".into()), Some("08:00".into()), Some(480)).unwrap();

        // 23:30 local time (1410 minutes): 1410 * 60 * 1000 = 84,600,000 ms from midnight
        // UTC offset 480 min (8 hours): 23:30 local is 15:30 UTC
        let now_2330 = (15 * 60 + 30) * 60 * 1000;
        let sent = store.send_outbound_notification(dir.path(), "Nightly test brief", false, now_2330).expect("notify_all in quiet hours");
        assert_eq!(sent, 0, "Non-urgent notification must be suppressed during quiet hours");
    }

    #[test]
    fn test_wechat_bridge_health_endpoint() {
        let (status, val) = handle_wechat_health();
        assert_eq!(status, 200);
        assert_eq!(val["ok"], json!(true));
        assert_eq!(val["service"], json!("bob-jarvis"));
        assert_eq!(val["version"], json!("1.0.1"));
    }
}
