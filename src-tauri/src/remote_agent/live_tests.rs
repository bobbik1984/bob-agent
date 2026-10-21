use super::*;
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocketUpgrade},
        Path, Query, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Clone)]
struct Fixture {
    frames: tokio::sync::broadcast::Sender<(String, Value)>,
    calls: Arc<StdMutex<Vec<Value>>>,
    fail: Arc<AtomicBool>,
}
async fn fixture() -> (String, Fixture, tokio::task::JoinHandle<()>) {
    async fn connect(Query(query): Query<HashMap<String, String>>) -> impl IntoResponse {
        assert_eq!(
            query.get("interactions").map(String::as_str),
            Some("answer")
        );
        (
            axum::http::StatusCode::FOUND,
            [
                ("location", "./api/remote.mux"),
                ("set-cookie", "dsh-auth-test=cookie; HttpOnly"),
            ],
        )
    }
    async fn socket(State(f): State<Fixture>, ws: WebSocketUpgrade) -> impl IntoResponse {
        ws.on_upgrade(move |mut socket| async move {
            let mut frames = f.frames.subscribe();
            let mut streams: HashMap<String,String> = HashMap::new();
            loop {
                tokio::select! {
                    message = socket.recv() => {
                        let Some(Ok(WsMessage::Text(text))) = message else { break; };
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        if frame["type"] == "cancel" { streams.retain(|_, id| id != frame["streamId"].as_str().unwrap()); continue; }
                        assert_eq!(frame["type"], "open");
                        let endpoint = frame["endpoint"].as_str().unwrap();
                        streams.insert(endpoint.into(), frame["streamId"].as_str().unwrap().into());
                        let value = match endpoint {
                            "$events" => json!({"type":"ready","clientId":"generation-1","host":{"home":"/private"}}),
                            "workspace/follow" => json!({"type":"baseline","value":{"items":[]}}),
                            "session/follow" => {
                                assert_eq!(frame.pointer("/payload/args/request/assistantStream"),Some(&json!(true)));
                                json!({"type":"snapshot","header":{"id":frame["payload"]["args"]["request"]["address"]["sessionId"]},"cursor":0,"hasMore":false,"records":[],"assistantStream":{"revision":0}})
                            }
                            _ => panic!("unexpected stream"),
                        };
                        socket.send(WsMessage::Text(json!({"type":"item","streamId":frame["streamId"],"value":value}).to_string().into())).await.unwrap();
                    }
                    message = frames.recv() => {
                        let Ok((endpoint,value)) = message else { break; };
                        if endpoint == "close" { break; }
                        if let Some(id) = streams.get(&endpoint) {
                            socket.send(WsMessage::Text(json!({"type":"item","streamId":id,"value":value}).to_string().into())).await.unwrap();
                        }
                    }
                }
            }
        })
    }
    async fn rpc(
        State(f): State<Fixture>,
        Path(endpoint): Path<String>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(headers["cookie"], "dsh-auth-test=cookie");
        assert_eq!(body["type"], "client-request");
        assert_eq!(body["method"], endpoint);
        f.calls.lock().unwrap().push(body.clone());
        let value = match endpoint.as_str() {
            "session/prompt" => {
                assert_eq!(body["payload"]["args"]["request"]["mode"], "queue");
                json!({"accepted":true})
            }
            "session/cancel" => json!({"accepted":true}),
            "$events/result" => {
                assert_eq!(body["payload"]["args"]["clientId"], "generation-1");
                Value::Null
            }
            _ => panic!("unexpected rpc"),
        };
        Json(
            json!({"type":"server-response","rpcId":body["rpcId"],"result":{"ok":!f.fail.load(Ordering::Acquire),"value":value}}),
        )
    }
    let (frames, _) = tokio::sync::broadcast::channel(32);
    let f = Fixture {
        frames,
        calls: Arc::new(StdMutex::new(vec![])),
        fail: Arc::new(AtomicBool::new(false)),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/connect", get(connect))
        .route("/api/remote.mux", get(socket))
        .route("/api/{*endpoint}", post(rpc))
        .with_state(f.clone());
    (
        url,
        f,
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }),
    )
}
fn action(
    watch: &str,
    kind: &str,
    id: Option<&str>,
    text: Option<&str>,
    event: Option<&str>,
    answer: Option<Value>,
) -> ActionInput {
    ActionInput {
        watch_id: watch.into(),
        kind: kind.into(),
        request_id: id.map(Into::into),
        text: text.map(Into::into),
        event_id: event.map(Into::into),
        answer,
    }
}
async fn until(dsh: &Dsh, watch: &str, predicate: impl Fn(&LiveView) -> bool) -> LiveView {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let view = dsh.live(watch).unwrap();
            if predicate(&view) {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("live projection did not reach expected state")
}
fn event(seq: u64, kind: &str, data: Value) -> Value {
    json!({"type":"event","event":{"type":kind,"seq":seq,"data":data}})
}

#[tokio::test]
async fn prompt_stream_approval_cancel_production_transport_closure() {
    let (url, f, server) = fixture().await;
    let dsh = Dsh::connect(&url, "private").await.unwrap();
    let view = dsh.watch("s1").await.unwrap();
    let w = &view.watch_id;
    assert!(view.ready);
    assert_eq!(
        dsh.action(action(w, "send", Some("r1"), Some("render"), None, None))
            .await
            .unwrap()
            .phase,
        "accepted"
    );
    assert_eq!(
        dsh.action(action(w, "send", Some("r1"), Some("render"), None, None))
            .await
            .unwrap()
            .phase,
        "accepted"
    );
    assert_eq!(f.calls.lock().unwrap().len(), 1); // duplicate never resubmits
    assert!(dsh
        .action(action(w, "send", Some("r1"), Some("different"), None, None))
        .await
        .is_err());
    f.frames
        .send((
            "session/follow".into(),
            event(1, "turn/start", json!({"turn":1})),
        ))
        .unwrap();
    f.frames.send(("session/follow".into(),event(2,"user/message",json!({"source":{"kind":"user","rpcId":"r1"},"content":[{"type":"text","text":"render"}]})))).unwrap();
    until(&dsh, w, |v| v.phase == "running").await;
    for frame in [
        json!({"type":"start","attemptId":"a1","revision":1,"turn":1,"step":0,"startedAfterSeq":2}),
        json!({"type":"chunk","attemptId":"a1","revision":2,"index":0,"time":1,"chunk":{"type":"text-delta","index":0,"text":"hello"}}),
    ] {
        f.frames
            .send((
                "session/follow".into(),
                json!({"type":"assistant-stream","frame":frame}),
            ))
            .unwrap();
    }
    until(&dsh, w, |v| v.draft == "hello").await;
    f.frames.send(("$events".into(),json!({"type":"waterfall","event":"approval/request","eventId":"e1","agentId":"s1","request":{"toolName":"shell","reason":"Run renderer"}}))).unwrap();
    until(&dsh, w, |v| v.interactions.len() == 1).await;
    assert!(dsh
        .action(action(
            "old-watch",
            "answer",
            None,
            None,
            Some("e1"),
            Some(json!("allowed-once"))
        ))
        .await
        .is_err());
    assert!(dsh
        .action(action(
            w,
            "answer",
            None,
            None,
            Some("e1"),
            Some(json!("always-allow"))
        ))
        .await
        .is_err());
    assert_eq!(
        dsh.action(action(
            w,
            "answer",
            None,
            None,
            Some("e1"),
            Some(json!("allowed-once"))
        ))
        .await
        .unwrap()
        .interactions[0]
            .status,
        "answered"
    );
    assert!(dsh
        .action(action(
            w,
            "answer",
            None,
            None,
            Some("e1"),
            Some(json!("allowed-once"))
        ))
        .await
        .is_err());
    assert_eq!(
        f.calls.lock().unwrap()[1]["payload"]["args"]["outcome"],
        json!({"kind":"result","value":"allowed-once"})
    );
    assert_eq!(
        dsh.action(action(w, "cancel", None, None, None, None))
            .await
            .unwrap()
            .phase,
        "cancel_requested"
    );
    f.frames
        .send((
            "$events".into(),
            json!({"type":"emit","event":"api-session/status","args":["s1",false]}),
        ))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(dsh.live(w).unwrap().phase, "cancel_requested");
    f.frames
        .send((
            "session/follow".into(),
            event(3, "turn/end", json!({"turn":1,"reason":{"kind":"aborted"}})),
        ))
        .unwrap();
    assert_eq!(
        until(&dsh, w, |v| v.phase == "stopped").await.phase,
        "stopped"
    );
    dsh.close();
    server.abort();
}

#[tokio::test]
async fn uncertain_send_is_not_retried_and_disconnect_invalidates_actions() {
    let (url, f, server) = fixture().await;
    let dsh = Dsh::connect(&url, "private").await.unwrap();
    let view = dsh.watch("s1").await.unwrap();
    f.fail.store(true, Ordering::Release);
    assert_eq!(
        dsh.action(action(
            &view.watch_id,
            "send",
            Some("r1"),
            Some("hi"),
            None,
            None
        ))
        .await
        .unwrap_err(),
        "REMOTE_OUTCOME_UNKNOWN"
    );
    assert!(!dsh.live(&view.watch_id).unwrap().ready);
    assert!(dsh
        .action(action(
            &view.watch_id,
            "send",
            Some("r2"),
            Some("hi"),
            None,
            None
        ))
        .await
        .is_err());
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    dsh.close();
    server.abort();
}

#[tokio::test]
async fn questions_withdrawal_scope_and_stream_loss_fail_closed() {
    let (url, f, server) = fixture().await;
    let dsh = Dsh::connect(&url, "private").await.unwrap();
    let view = dsh.watch("s1").await.unwrap();
    let w = &view.watch_id;
    f.frames.send(("$events".into(),json!({"type":"waterfall","event":"user-questions/request","eventId":"q1","agentId":"s1","request":{"questions":[{"id":"q","question":"Which?","options":[{"label":"yes"}]}]}}))).unwrap();
    until(&dsh, w, |v| v.interactions.len() == 1).await;
    assert!(dsh
        .action(action(
            w,
            "answer",
            None,
            None,
            Some("q1"),
            Some(json!({"answers":[{"id":"q","selected":["forged"]}]}))
        ))
        .await
        .is_err());
    f.frames
        .send(("$events".into(), json!({"type":"cancel","eventId":"q1"})))
        .unwrap();
    until(&dsh, w, |v| v.interactions[0].status == "expired").await;
    assert!(dsh
        .action(action(
            w,
            "answer",
            None,
            None,
            Some("q1"),
            Some(json!({"answers":[{"id":"q","selected":["yes"]}]}))
        ))
        .await
        .is_err());
    f.frames.send(("close".into(), Value::Null)).unwrap();
    assert!(!until(&dsh, w, |v| !v.ready).await.ready);
    assert!(f.calls.lock().unwrap().is_empty());
    dsh.close();
    server.abort();
}

#[test]
fn live_projection_deduplicates_durable_text_and_rejects_chunk_gaps() {
    let mut s = super::super::live::LiveState::default();
    s.client_id = Some("c".into());
    s.select("w".into(), "s".into());
    s.follow(
        &json!({"type":"snapshot","header":{"id":"s"},"cursor":0,"hasMore":false,"records":[]}),
    )
    .unwrap();
    s.follow(
        &json!({"type":"assistant-stream","frame":{"type":"start","attemptId":"a","revision":1}}),
    )
    .unwrap();
    let chunk = json!({"type":"assistant-stream","frame":{"type":"chunk","attemptId":"a","revision":2,"index":0,"chunk":{"type":"text-delta","text":"hello"}}});
    s.follow(&chunk).unwrap();
    s.follow(&chunk).unwrap();
    assert_eq!(s.view.draft, "hello");
    let durable = event(
        1,
        "assistant/message",
        json!({"message":{"content":[{"type":"text","text":"hello"}]}}),
    );
    s.follow(&durable).unwrap();
    s.follow(&durable).unwrap();
    assert_eq!(s.view.messages.len(), 1);
    assert!(s.view.draft.is_empty());
    assert!(s.follow(&json!({"type":"assistant-stream","frame":{"type":"chunk","attemptId":"a","revision":4,"index":3,"chunk":{"type":"text-delta","text":"lost"}}})).is_err());
}

#[tokio::test]
async fn rejection_and_question_answers_are_scoped_to_live_generation() {
    let (url, f, server) = fixture().await;
    let dsh = Dsh::connect(&url, "private").await.unwrap();
    let first = dsh.watch("s1").await.unwrap();
    f.frames.send(("$events".into(),json!({"type":"waterfall","event":"approval/request","eventId":"e2","agentId":"s2","request":{"toolName":"shell","reason":"Other session"}}))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(dsh.live(&first.watch_id).unwrap().interactions.is_empty());
    assert!(dsh
        .action(action(
            &first.watch_id,
            "answer",
            None,
            None,
            Some("e2"),
            Some(json!("rejected"))
        ))
        .await
        .is_err());
    let second = dsh.watch("s2").await.unwrap();
    assert!(dsh.live(&first.watch_id).is_err());
    let result = dsh
        .action(action(
            &second.watch_id,
            "answer",
            None,
            None,
            Some("e2"),
            Some(json!("rejected")),
        ))
        .await
        .unwrap();
    assert_eq!(result.interactions[0].status, "answered");
    f.frames.send(("$events".into(),json!({"type":"waterfall","event":"user-questions/request","eventId":"q2","agentId":"s2","request":{"questions":[{"id":"q","question":"Which?","options":[{"label":"yes"}]}]}}))).unwrap();
    until(&dsh, &second.watch_id, |v| v.interactions.len() == 2).await;
    let answer = json!({"answers":[{"id":"q","selected":["yes"],"custom":"details"}]});
    dsh.action(action(
        &second.watch_id,
        "answer",
        None,
        None,
        Some("q2"),
        Some(answer.clone()),
    ))
    .await
    .unwrap();
    assert_eq!(
        f.calls.lock().unwrap()[1]["payload"]["args"]["outcome"]["value"],
        answer
    );
    dsh.close();
    assert!(dsh
        .action(action(
            &second.watch_id,
            "answer",
            None,
            None,
            Some("q2"),
            Some(answer)
        ))
        .await
        .is_err());
    server.abort();
}
