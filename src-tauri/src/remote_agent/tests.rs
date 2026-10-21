use super::*;
use axum::{
    extract::{
        ws::{Message as AxumMessage, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

#[test]
fn endpoint_auth_is_ephemeral_and_public_http_rejected() {
    let (url, secret) = endpoint("https://example.test/dsh/connect?token=private", "").unwrap();
    assert_eq!(url.as_str(), "https://example.test/dsh/");
    assert_eq!(secret, "private");
    for url in [
        "http://example.test/",
        "file:///tmp/",
        "https://user:pass@example.test/",
        "https://example.test/#secret",
        "https://example.test/?token=a&token=b",
    ] {
        assert!(endpoint(url, "private").is_err());
    }
    assert!(endpoint("http://127.0.0.1:3080/", "private").is_ok());
    assert!(endpoint("https://example.test/", "").is_err());
}

#[test]
fn history_rejects_wrong_session_and_projects_only_text() {
    let value = json!({"type":"snapshot","header":{"id":"s1"},"hasMore":true,"records":[
        {"type":"event","event":{"type":"user/message","seq":1,"data":{"source":{"kind":"user"},"content":[{"type":"text","text":"hello"},{"type":"image","data":"private"}]}}},
        {"type":"event","event":{"type":"assistant/message","seq":2,"data":{"message":{"content":[{"type":"text","text":"reply"}]},"stream":["private"]}}},
        {"type":"event","event":{"type":"tool/result","seq":2,"data":{"secret":"private"}}}
    ]});
    assert!(project_history("another", &value).is_err());
    let history = project_history("s1", &value).unwrap();
    assert_eq!(history.messages.len(), 2);
    assert_eq!(history.messages[0].text, "hello");
    assert_eq!(history.messages[1].text, "reply");
    assert_eq!(history.omitted_records, 2);
    assert!(history.has_more);
    assert!(!serde_json::to_string(&history).unwrap().contains("private"));
}

async fn server(mode: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    async fn connect(State(mode): State<&'static str>) -> impl IntoResponse {
        if mode == "auth" {
            return (
                StatusCode::UNAUTHORIZED,
                [("location", ""), ("set-cookie", "")],
            );
        }
        (
            StatusCode::FOUND,
            [
                (
                    "location",
                    if mode == "redirect" {
                        "https://evil.test/api/remote.mux"
                    } else {
                        "./api/remote.mux"
                    },
                ),
                (
                    "set-cookie",
                    "dsh-auth-fixture=secret-cookie; HttpOnly; Path=/",
                ),
            ],
        )
    }
    async fn list(
        State(mode): State<&'static str>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        assert_eq!(headers["cookie"], "dsh-auth-fixture=secret-cookie");
        assert_eq!(body["type"], "client-request");
        assert_eq!(body["method"], "session/list");
        assert_eq!(body["payload"]["args"], json!({"_request":{}}));
        Json(
            json!({"type":"server-response","rpcId":if mode == "wrong-id" { json!("wrong") } else { body["rpcId"].clone() },"result":{"ok":true,"value":{"items":[{"sessionId":"s1","updatedAt":1,"running":false,"projections":{"values":{"title":"Render experiment"}}}]}}}),
        )
    }
    async fn socket(
        State(mode): State<&'static str>,
        headers: axum::http::HeaderMap,
        ws: WebSocketUpgrade,
    ) -> impl IntoResponse {
        assert_eq!(headers["cookie"], "dsh-auth-fixture=secret-cookie");
        ws.on_upgrade(move |mut socket| async move {
            while let Some(Ok(AxumMessage::Text(text))) = socket.recv().await {
                let frame: Value = serde_json::from_str(&text).unwrap();
                if frame["type"] == "cancel" { continue; }
                assert_eq!(frame["type"], "open");
                if mode == "disconnect" { return; }
                let value = if frame["endpoint"] == "workspace/follow" {
                    assert_eq!(frame["payload"]["args"], json!({}));
                    json!({"type":"baseline","value":{"items":[{"workspaceId":"w1","title":"Project","sessionIds":["s1"]}]}})
                } else {
                    assert_eq!(frame["endpoint"], "session/follow");
                    assert_eq!(frame["payload"]["args"]["request"]["address"], json!({"kind":"session","sessionId":"s1"}));
                    json!({"type":"snapshot","header":{"id":"s1"},"hasMore":false,"records":[{"type":"event","event":{"type":"user/message","seq":1,"data":{"source":{"kind":"user"},"content":[{"type":"text","text":"hello"}]}}}]})
                };
                if socket.send(AxumMessage::Text(json!({"type":"item","streamId":frame["streamId"],"value":value}).to_string().into())).await.is_err() { return; }
            }
        })
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/connect", get(connect))
        .route("/api/session/list", post(list))
        .route("/api/remote.mux", get(socket))
        .with_state(mode);
    (
        url,
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }),
    )
}

#[tokio::test]
async fn dsh_real_http_and_mux_read_chain_and_close() {
    let (url, server) = server("ok").await;
    let dsh = Dsh::connect(&url, "secret-token").await.unwrap();
    let info = serde_json::to_string(&dsh.info()).unwrap();
    assert!(!info.contains("secret"));
    assert!(!dsh.info().capabilities.send);
    let catalog = dsh.catalog().await.unwrap();
    assert_eq!(catalog.workspaces[0].session_ids, ["s1"]);
    assert_eq!(catalog.sessions[0].session_id, "s1");
    assert_eq!(catalog.sessions[0].title, "Render experiment");
    assert_eq!(dsh.history("s1").await.unwrap().messages[0].text, "hello");
    dsh.close();
    assert!(dsh.catalog().await.is_err());
    server.abort();
}

#[tokio::test]
async fn dsh_auth_redirect_disconnect_and_mismatched_rpc_fail_closed() {
    for (mode, expected) in [
        ("auth", "REMOTE_AUTH"),
        ("redirect", "REMOTE_REDIRECT"),
        ("disconnect", "REMOTE_DISCONNECTED"),
        ("wrong-id", "REMOTE_PROTOCOL"),
    ] {
        let (url, server) = server(mode).await;
        let result = match Dsh::connect(&url, "secret-token").await {
            Ok(dsh) => dsh.catalog().await.map(|_| ()),
            Err(error) => Err(error),
        };
        assert_eq!(result.unwrap_err(), expected);
        server.abort();
    }
}
