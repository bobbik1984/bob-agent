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
