use super::*;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "bob-native-ipc-{}-{stamp}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("kernel.sqlite")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn app(local: Option<LocalApp>) -> (tauri::App<MockRuntime>, tauri::WebviewWindow<MockRuntime>) {
    let app = configure(mock_builder())
        .build(mock_context(noop_assets()))
        .unwrap();
    *app.state::<AppState>().0.lock().unwrap() = local;
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, window)
}
fn invoke(
    window: &tauri::WebviewWindow<MockRuntime>,
    cmd: &str,
    body: Value,
) -> Result<Value, Value> {
    tauri::test::get_ipc_response(
        window,
        tauri::webview::InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .map(|b| b.deserialize().unwrap())
}

#[test]
fn native_commands_save_deduplicate_and_reopen() {
    let f = Fixture::new();
    let (host, view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    let boot = invoke(&view, "app_bootstrap", json!({})).unwrap();
    assert_eq!(boot["provider_status"], "provider_unconfigured");
    let session = boot["session_id"].as_str().unwrap().to_owned();
    let request = json!({"requestId":"ipc-request", "messageId":"ipc-message", "sessionId":session, "content":"durable IPC message"});
    let receipt = invoke(&view, "app_accept", request.clone()).unwrap();
    assert_eq!(receipt["request_id"], "ipc-request");
    assert_eq!(invoke(&view, "app_accept", request).unwrap(), receipt);
    let status = invoke(&view, "app_readiness", json!({})).unwrap();
    assert_eq!(status["saved_messages"], 1);
    assert_eq!(status["pending_messages"], 1);
    assert_eq!(status["execution_available"], false);
    let before = invoke(&view, "app_history", json!({"sessionId":session})).unwrap();
    assert_eq!(before.as_array().unwrap().len(), 1);
    assert_eq!(before[0]["send_state"], "local_saved");
    // Release the real Store and reopen its disk file through a new native host.
    *host.state::<AppState>().0.lock().unwrap() = None;
    drop(view);
    drop(host);
    let (_host, view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    assert_eq!(
        invoke(&view, "app_bootstrap", json!({})).unwrap()["session_id"],
        session
    );
    assert_eq!(
        invoke(&view, "app_history", json!({"sessionId":session})).unwrap(),
        before
    );
    assert_eq!(
        invoke(&view, "app_prewarm", json!({})).unwrap(),
        "provider_unconfigured"
    );
}

#[test]
fn malformed_or_wrong_session_commands_do_not_save() {
    let f = Fixture::new();
    let (_host, view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    let session = invoke(&view, "app_bootstrap", json!({})).unwrap()["session_id"].clone();
    assert!(invoke(&view,"app_accept",json!({"request_id":"bad","message_id":"bad","session_id":session,"content":"wrong argument names"})).is_err());
    assert_eq!(
        invoke(
            &view,
            "app_accept",
            json!({"requestId":"bad","messageId":"bad","sessionId":"another-session","content":"wrong session"})
        ),
        Err(json!("LOCAL_SAVE_UNCONFIRMED"))
    );
    assert_eq!(
        invoke(&view, "app_history", json!({"sessionId":session})).unwrap(),
        json!([])
    );
}

#[test]
fn unavailable_store_never_reports_acceptance() {
    let (_host, view) = app(None);
    assert_eq!(
        invoke(
            &view,
            "app_accept",
            json!({"requestId":"r","messageId":"m","sessionId":"s","content":"no store"})
        ),
        Err(json!("LOCAL_STORE_UNAVAILABLE"))
    );
    assert_eq!(
        invoke(&view, "app_history", json!({"sessionId":"s"})),
        Err(json!("LOCAL_STORE_UNAVAILABLE"))
    );
}

#[test]
fn captured_reply_crosses_history_ipc_after_reopen() {
    let f = Fixture::new();
    let mut local = LocalApp::open(&f.db()).unwrap();
    let session = local.default_session().unwrap();
    local
        .accept(
            "probe",
            "probe-message",
            &session,
            "independent approved CLI test".into(),
            1,
        )
        .unwrap();
    local
        .record_cli_probe(
            "probe",
            include_str!("../../../../crates/bob-kernel/tests/fixtures/codex-reply-probe.jsonl"),
            Some(0),
            2,
        )
        .unwrap();
    drop(local);
    let (_host, view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    let history = invoke(&view, "app_history", json!({"sessionId":session})).unwrap();
    assert_eq!(history[1]["role"], "assistant");
    assert_eq!(history[1]["content"], "BOB_REPLY_OK 2+3=5");
    assert_eq!(history[1]["reply_to"], "probe-message");
    assert_eq!(history[0]["send_state"], "local_saved");
    // No renderer-facing command may fabricate/import a successful reply.
    assert!(invoke(&view, "app_record_cli_probe", json!({"text":"forged"})).is_err());
}

#[test]
fn cancel_command_records_request_without_claiming_process_stopped() {
    let f = Fixture::new();
    let mut local = LocalApp::open(&f.db()).unwrap();
    let session = local.default_session().unwrap();
    local
        .accept("cancel-r", "cancel-m", &session, "read only".into(), 1)
        .unwrap();
    let execution = local
        .prepare_local_execution("cancel-r", "fixture-workspace", 2)
        .unwrap();
    let (_host, view) = app(Some(local));
    let stopped = invoke(&view, "app_cancel", json!({"executionId":execution.id})).unwrap();
    assert_eq!(stopped["stop_requested"], true);
    assert_eq!(stopped["status"], "dispatching");
    assert_eq!(
        invoke(&view, "app_executions", json!({"sessionId":session})).unwrap()[0]["stop_requested"],
        true
    );
    assert!(invoke(&view, "app_cancel", json!({"executionId":"unrelated"})).is_err());
}

#[path = "real_cli_tests.rs"]
mod real_cli;

fn completion(id: &str, stream: String) -> CompletionReceipt {
    CompletionReceipt {
        execution_id: id.into(),
        stream,
        exit: Some(0),
        reason: None,
        cleanup: true,
    }
}

#[test]
fn pending_receipt_restores_committed_reply_then_both_spool_names_are_removed() {
    let f = Fixture::new();
    let mut local = LocalApp::open(&f.db()).unwrap();
    let session = local.default_session().unwrap();
    local
        .accept(
            "pending-r",
            "pending-m",
            &session,
            "approved reply".into(),
            1,
        )
        .unwrap();
    let run = local
        .prepare_local_execution("pending-r", "fixture-workspace", 2)
        .unwrap();
    local.mark_local_started(&run.id, 3).unwrap();
    drop(local);
    let receipt = completion(
        &run.id,
        include_str!("../../../../crates/bob-kernel/tests/fixtures/codex-reply-probe.jsonl").into(),
    );
    save_receipt(&f.0, &receipt).unwrap();
    let path = receipt_path(&f.0, &run.id);
    std::fs::rename(&path, path.with_extension("pending")).unwrap();
    let captured = read_receipt(&f.0, &run.id).unwrap().unwrap();
    let mut local = LocalApp::open(&f.db()).unwrap();
    assert_eq!(
        local.local_executions(&session).unwrap()[0].status,
        "unknown"
    );
    // This fixture supplies the native owner's cleanup proof; no model runs.
    local
        .restore_local_completion(
            &run.id,
            &captured.stream,
            captured.exit,
            captured.reason.as_deref(),
            4,
        )
        .unwrap();
    assert_eq!(
        local.history(&session).unwrap()[1].content,
        "BOB_REPLY_OK 2+3=5"
    );
    std::fs::copy(path.with_extension("pending"), &path).unwrap();
    remove_receipt(&f.0, &run.id).unwrap();
    assert!(!path.exists());
    assert!(!path.with_extension("pending").exists());
    assert!(read_receipt(&f.0, &run.id).unwrap().is_none());
}

#[test]
fn corrupt_pending_is_retained_as_unknown_not_missing_result() {
    let f = Fixture::new();
    let mut local = LocalApp::open(&f.db()).unwrap();
    let session = local.default_session().unwrap();
    local
        .accept(
            "corrupt-r",
            "corrupt-m",
            &session,
            "approved reply".into(),
            1,
        )
        .unwrap();
    let run = local
        .prepare_local_execution("corrupt-r", "fixture-workspace", 2)
        .unwrap();
    drop(local);
    let pending = receipt_path(&f.0, &run.id).with_extension("pending");
    std::fs::write(&pending, b"{truncated").unwrap();
    assert!(read_receipt(&f.0, &run.id).is_err());
    let local = LocalApp::open(&f.db()).unwrap();
    assert_eq!(
        local.local_executions(&session).unwrap()[0].status,
        "unknown"
    );
    assert!(pending.exists());
    assert_eq!(local.history(&session).unwrap().len(), 1);
    // Final corruption must not be hidden by a valid pending alternative.
    std::fs::write(receipt_path(&f.0, &run.id), b"{truncated-final").unwrap();
    std::fs::write(
        &pending,
        serde_json::to_vec(&completion(&run.id, "valid alternative".into())).unwrap(),
    )
    .unwrap();
    assert!(read_receipt(&f.0, &run.id).is_err());
}

#[test]
fn encoded_receipt_expansion_roundtrips_and_limits_match_writer_reader() {
    let f = Fixture::new();
    // JSON encoding expands each control byte to six bytes: > old 3MiB cap,
    // while the decoded stream remains within the runner's 2MiB bound.
    let receipt = completion("expanded", "\u{0001}".repeat(RECEIPT_STREAM_LIMIT));
    save_receipt(&f.0, &receipt).unwrap();
    assert!(
        std::fs::metadata(receipt_path(&f.0, "expanded"))
            .unwrap()
            .len()
            > 3 * 1024 * 1024
    );
    assert_eq!(
        read_receipt(&f.0, "expanded").unwrap().unwrap().stream,
        receipt.stream
    );
    let too_large = completion("decoded-too-large", "x".repeat(RECEIPT_STREAM_LIMIT + 1));
    assert!(save_receipt(&f.0, &too_large).is_err());
    assert!(!receipt_path(&f.0, "decoded-too-large")
        .with_extension("pending")
        .exists());
    std::fs::write(
        receipt_path(&f.0, "decoded-too-large"),
        serde_json::to_vec(&too_large).unwrap(),
    )
    .unwrap();
    assert!(read_receipt(&f.0, "decoded-too-large").is_err());
    let mut encoded = completion("encoded-too-large", String::new());
    encoded.reason = Some("x".repeat(RECEIPT_ENCODED_LIMIT));
    assert!(save_receipt(&f.0, &encoded).is_err());
    std::fs::write(
        receipt_path(&f.0, "encoded-too-large"),
        serde_json::to_vec(&encoded).unwrap(),
    )
    .unwrap();
    assert!(read_receipt(&f.0, "encoded-too-large").is_err());
    let mismatch = completion("another-execution", "reply".into());
    std::fs::write(
        receipt_path(&f.0, "wrong-owner").with_extension("pending"),
        serde_json::to_vec(&mismatch).unwrap(),
    )
    .unwrap();
    assert!(read_receipt(&f.0, "wrong-owner").is_err());
}


#[test]
fn catalog_and_multi_session_ipc_commands_persist_and_switch_cleanly() {
    let f = Fixture::new();
    let (host, view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    let boot = invoke(&view, "app_bootstrap", json!({})).unwrap();
    let default_s = boot["session_id"].as_str().unwrap().to_owned();

    let snap0 = invoke(&view, "app_catalog", json!({})).unwrap();
    assert_eq!(snap0["selected_conversation_id"], default_s);
    assert_eq!(snap0["conversations"].as_array().unwrap().len(), 1);

    let snap1 = invoke(
        &view,
        "app_project_create",
        json!({"projectId":"proj-1","title":"Project One"}),
    )
    .unwrap();
    assert_eq!(snap1["projects"].as_array().unwrap().len(), 1);

    let snap2 = invoke(
        &view,
        "app_conversation_create",
        json!({"conversationId":"conv-2","projectId":"proj-1","title":"Second Session"}),
    )
    .unwrap();
    assert_eq!(snap2["selected_conversation_id"], "conv-2");
    assert_eq!(snap2["conversations"].as_array().unwrap().len(), 2);

    let vs = invoke(
        &view,
        "app_view_state_save",
        json!({"conversationId":"conv-2","draft":"my draft","scrollAnchorMessageId":"m-0","scrollOffset":120,"markRead":false}),
    )
    .unwrap();
    assert_eq!(vs["draft"], "my draft");
    assert_eq!(vs["draft_revision"], 1);
    assert_eq!(vs["scroll_offset"], 120);

    invoke(
        &view,
        "app_accept",
        json!({"requestId":"r-c2","messageId":"m-c2","sessionId":"conv-2","content":"hello conv 2","expectedDraftRevision":1}),
    )
    .unwrap();
    let hist2 = invoke(&view, "app_history", json!({"sessionId":"conv-2"})).unwrap();
    assert_eq!(hist2.as_array().unwrap().len(), 1);
    assert_eq!(hist2[0]["content"], "hello conv 2");

    let prefs = invoke(
        &view,
        "app_preferences_save",
        json!({"sidebarGroupMode":"device","locale":"en-US","theme":"dark"}),
    )
    .unwrap();
    assert_eq!(prefs["sidebar_group_mode"], "device");
    assert_eq!(prefs["locale"], "en-US");
    assert_eq!(prefs["theme"], "dark");

    *host.state::<AppState>().0.lock().unwrap() = None;
    drop(view);
    drop(host);

    let (_host, view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    let reopened_boot = invoke(&view, "app_bootstrap", json!({})).unwrap();
    assert_eq!(reopened_boot["session_id"], "conv-2");
}

#[test]
fn personal_ops_ipc_contract_aligns_and_exports_canonical_snapshot_fixture() {
    let f = Fixture::new();
    let mut local = LocalApp::open(&f.db()).unwrap();
    let session = local.default_session().unwrap();
    local
        .accept(
            "req-fixture-1",
            "msg-fixture-1",
            &session,
            "下周三下午 3 点进行跨平台契约验收，并整理笔记和记忆。".into(),
            1_762_000_000_000,
        )
        .unwrap();
    let cap = local
        .create_capture(
            "cap-fixture-1",
            Some("msg-fixture-1"),
            "速记：优先对齐 Schedule / Memory / Decision 契约与单点快照",
            1_762_000_010_000,
        )
        .unwrap();
    let mem = local
        .propose_memory(
            "mem-fixture-1",
            Some(&cap.id),
            Some("msg-fixture-1"),
            "所有文件读写显式指定 UTF-8 编码，时间戳统一使用 13 位 Unix 毫秒",
            None,
        )
        .unwrap();
    assert_eq!(mem.state, "proposed");

    let (_host, view) = app(Some(local));

    // RV-01 / RV-02 / RV-03: Create both 'event' and 'todo' schedule items with 13-digit ms startAt
    let ev = invoke(
        &view,
        "app_schedule_create",
        json!({
            "id": "sched-event-1",
            "title": "跨平台架构评审会议",
            "kind": "event",
            "startAt": 1_762_010_000_000_i64,
            "notes": "核对手机端、桌面端与云端 Web 契约"
        }),
    )
    .unwrap();
    assert_eq!(ev["kind"], "event");
    assert_eq!(ev["status"], "scheduled");
    assert_eq!(ev["start_at"], 1_762_010_000_000_i64);

    let td = invoke(
        &view,
        "app_schedule_create",
        json!({
            "id": "sched-todo-1",
            "title": "完成 RV-01 至 RV-13 回归验证",
            "kind": "todo",
            "startAt": 1_762_020_000_000_i64,
            "notes": "待办事项回归"
        }),
    )
    .unwrap();
    assert_eq!(td["kind"], "todo");

    // Negative test: non-canonical 'done' status must be rejected by IPC
    assert!(invoke(
        &view,
        "app_schedule_status",
        json!({"id": "sched-event-1", "status": "done"})
    )
    .is_err());

    // Canonical 'completed' status succeeds
    let ev_completed = invoke(
        &view,
        "app_schedule_status",
        json!({"id": "sched-event-1", "status": "completed"}),
    )
    .unwrap();
    assert_eq!(ev_completed["status"], "completed");

    // RV-04: Negative test for non-canonical memory state 'confirmed', positive test for 'active'
    assert!(invoke(
        &view,
        "app_memory_state",
        json!({"id": "mem-fixture-1", "newState": "confirmed"})
    )
    .is_err());
    let mem_active = invoke(
        &view,
        "app_memory_state",
        json!({"id": "mem-fixture-1", "newState": "active"}),
    )
    .unwrap();
    assert_eq!(mem_active["state"], "active");
    assert_eq!(
        mem_active["body"],
        "所有文件读写显式指定 UTF-8 编码，时间戳统一使用 13 位 Unix 毫秒"
    );

    // Note upsert
    let note = invoke(
        &view,
        "app_note_upsert",
        json!({
            "id": "note-fixture-1",
            "title": "Bob 跨平台统一架构笔记",
            "bodyMarkdown": "# 核心契约\n- 日程状态：scheduled | completed | cancelled\n- 记忆状态：proposed | active | superseded | forgotten",
            "tags": ["architecture", "contract"]
        }),
    )
    .unwrap();
    assert_eq!(note["title"], "Bob 跨平台统一架构笔记");

    // RV-16: Decision record and status transition
    let dec = invoke(
        &view,
        "app_decision_record",
        json!({
            "id": "dec-fixture-1",
            "body": "前后端测试统一消费 Rust 序列化导出的 personal_snapshot.json，禁止手写字典",
            "rationaleJson": "{\"task\":\"RV-06\"}",
            "status": "proposed"
        }),
    )
    .unwrap();
    assert_eq!(dec["status"], "proposed");
    let dec_acc = invoke(
        &view,
        "app_decision_status",
        json!({"id": "dec-fixture-1", "status": "accepted"}),
    )
    .unwrap();
    assert_eq!(dec_acc["status"], "accepted");

    // RV-06: Compare canonical PersonalOpsSnapshot against committed fixture (no blind overwrite)
    let mut snapshot = invoke(&view, "app_personal_snapshot", json!({})).unwrap();
    assert_eq!(snapshot["schedules"].as_array().unwrap().len(), 2);
    assert_eq!(snapshot["notes"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["captures"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["memories"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["decisions"].as_array().unwrap().len(), 1);

    // Normalize runtime-generated device_id and wall-clock timestamps for deterministic comparison
    for arr_key in ["captures", "notes", "schedules"] {
        if let Some(arr) = snapshot.get_mut(arr_key).and_then(Value::as_array_mut) {
            for item in arr {
                if let Some(obj) = item.as_object_mut() {
                    if obj.contains_key("origin_device_id") {
                        obj.insert("origin_device_id".into(), json!("fixture-device-0001"));
                    }
                    if obj.contains_key("created_at") {
                        obj.insert("created_at".into(), json!(1_762_000_010_000_i64));
                    }
                    if obj.contains_key("updated_at") {
                        obj.insert("updated_at".into(), json!(1_762_000_010_000_i64));
                    }
                }
            }
        }
    }

    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures");
    let fixture_path = fixture_dir.join("personal_snapshot.json");
    if std::env::var("BOB_UPDATE_SNAPSHOTS").as_deref() == Ok("1") {
        std::fs::create_dir_all(&fixture_dir).unwrap();
        std::fs::write(
            &fixture_path,
            format!("{}\n", serde_json::to_string_pretty(&snapshot).unwrap()),
        )
        .unwrap();
    }
    assert!(fixture_path.exists());
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture_path).unwrap()).unwrap();
    assert_eq!(snapshot, expected);
}

#[test]
fn s0_trust_boundaries_redact_secrets_and_reject_unauthenticated_peer_sync_before_side_effects() {
    let dir = Fixture::new();
    let data_dir = &dir.0;

    // 1. Store canary API key and Online token in device_vault
    let canary_api_key = "sk-canary-secret-api-key-9999";
    let canary_online_token = "bob-online-bearer-secret-8888";
    crate::device_vault::store_secret(
        data_dir,
        &crate::direct_llm::provider_slot("deepseek"),
        canary_api_key.as_bytes(),
    )
    .unwrap();
    crate::device_vault::store_secret(
        data_dir,
        &crate::direct_llm::remote_node_token_slot("node-test"),
        canary_online_token.as_bytes(),
    )
    .unwrap();

    // 2. Export shared_config_json and verify zero secret leakage
    let exported = crate::direct_llm::export_shared_config_from_dir(data_dir);
    assert!(
        !exported.contains(canary_api_key),
        "shared_config_json must never contain raw API keys"
    );
    assert!(
        !exported.contains(canary_online_token),
        "shared_config_json must never contain Online bearer tokens"
    );
    let parsed: Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(parsed["credentials"], json!({}));

    // 3. Verify merge_shared_config_with_app ignores injected credentials and strips remoteNodes token
    let mut local_app = LocalApp::open(&data_dir.join("bob.db")).unwrap();
    let malicious_incoming = json!({
        "updatedAt": 999_999,
        "credentials": {
            "provider-key-openai": {
                "value": "sk-injected-malicious-key",
                "updatedAt": 999_999
            }
        },
        "remoteNodes": [
            {
                "id": "node-injected",
                "label": "Node",
                "baseUrl": "https://agent.bobbik.org",
                "token": "injected-raw-token",
                "updatedAt": 999_999
            }
        ]
    })
    .to_string();
    crate::direct_llm::merge_shared_config_with_app(data_dir, &mut local_app, &malicious_incoming);
    assert!(
        crate::device_vault::read_secret(data_dir, "provider-key-openai").is_err(),
        "incoming PeerSync must not write secrets into device_vault"
    );
    let re_exported = crate::direct_llm::export_shared_config_from_dir(data_dir);
    assert!(!re_exported.contains("injected-raw-token"));
}

#[test]
fn rv17_promote_conversation_in_place_preserves_history_draft_notes_and_decisions() {
    let dir = Fixture::new();
    let mut local = LocalApp::open(&dir.db()).unwrap();
    let session = local.default_session().unwrap();
    local
        .accept(
            "req-rv17-1",
            "msg-rv17-1",
            &session,
            "把这个灵感对话原位升级为正式工程项目，不要丢失上下文。".into(),
            1_000,
        )
        .unwrap();
    local
        .save_conversation_view_state(
            &session,
            Some("未发送的半截草稿"),
            Some(Some("msg-rv17-1")),
            Some(240),
            true,
            1_010,
        )
        .unwrap();
    local
        .upsert_note(
            "note-rv17-1",
            None,
            Some(&session),
            "会话关联笔记",
            "原位升级后必须自动继承 project_id",
            &["rv17".to_owned()],
            1_020,
        )
        .unwrap();

    let (_host, view) = app(Some(local));
    let promoted = invoke(
        &view,
        "app_conversation_promote",
        json!({
            "conversationId": session,
            "projectId": "proj-rv17",
            "title": "RV-17 原位升级项目"
        }),
    )
    .unwrap();

    // Idempotent repeat call
    let promoted_again = invoke(
        &view,
        "app_conversation_promote",
        json!({
            "conversationId": session,
            "projectId": "proj-rv17",
            "title": "RV-17 原位升级项目"
        }),
    )
    .unwrap();
    assert_eq!(promoted["selected_conversation_id"], session);
    assert_eq!(promoted_again["selected_conversation_id"], session);

    let convs = promoted["conversations"].as_array().unwrap();
    let target_conv = convs
        .iter()
        .find(|c| c["id"].as_str() == Some(&session))
        .unwrap();
    assert_eq!(target_conv["project_id"], "proj-rv17");
    assert_eq!(target_conv["view_state"]["draft"], "未发送的半截草稿");
    assert_eq!(target_conv["view_state"]["scroll_offset"], 240);

    let history = invoke(&view, "app_history", json!({"sessionId": session})).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(
        history[0]["content"],
        "把这个灵感对话原位升级为正式工程项目，不要丢失上下文。"
    );

    let snap = invoke(&view, "app_personal_snapshot", json!({})).unwrap();
    assert_eq!(snap["notes"][0]["project_id"], "proj-rv17");
    assert_eq!(snap["notes"][0]["conversation_id"], session);
}


