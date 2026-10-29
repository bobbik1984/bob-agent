//! Explicit opt-in genuine CLI test. Not a replay and not a desktop click test.
use super::*;
use std::time::Instant;

#[test]
#[ignore = "requires explicit real read-only Codex/subscription authorization"]
#[cfg(windows)]
fn genuine_codex_native_send_reply_and_reopen_no_replay() {
    assert!(
        std::env::var_os("BOB_CODEX_EXECUTABLE").is_some(),
        "configure official CLI first"
    );
    assert!(
        std::env::var_os("BOB_TEST_WORKER").is_some(),
        "build native worker first"
    );
    let f = Fixture::new();
    let (host, view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    let session = invoke(&view, "app_bootstrap", json!({})).unwrap()["session_id"].clone();
    let ready = invoke(&view, "app_readiness", json!({})).unwrap();
    assert_eq!(ready["mode"], "codex_readonly");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let request_id = format!("real-readonly-{nonce}");
    let message_id = format!("real-readonly-message-{nonce}");
    let prompt = "Do not use tools or read/write files. Reply exactly: BOB_NATIVE_REAL 11+7=18";
    let request =
        json!({"requestId":request_id,"messageId":message_id,"sessionId":session,"content":prompt});
    let accepted = invoke(&view, "app_accept", request.clone()).unwrap();
    assert_eq!(accepted["request_id"], request_id);
    let execution = invoke(&view, "app_execute", json!({"requestId":request_id})).unwrap();
    let id = execution["id"].clone();
    let duplicate = invoke(&view, "app_execute", json!({"requestId":request_id})).unwrap();
    assert_eq!(duplicate["id"], id);
    assert_eq!(duplicate["can_start"], false);
    let started = Instant::now();
    let terminal = loop {
        let records = invoke(&view, "app_executions", json!({"sessionId":session})).unwrap();
        assert_eq!(records.as_array().unwrap().len(), 1);
        let current = records[0].clone();
        if ["completed", "failed", "cancelled", "unknown"]
            .contains(&current["status"].as_str().unwrap())
        {
            break current;
        }
        assert!(
            started.elapsed() < Duration::from_secs(160),
            "native execution did not terminate in test deadline"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let history = invoke(&view, "app_history", json!({"sessionId":session})).unwrap();
    let source = history
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "assistant");
    let report = json!({"kind":"genuine_native_IPC_not_UI_click","execution_id":id,"status":terminal["status"],"error":terminal["error"],"reply":source.map(|m|m["content"].clone()),"reply_source":source.map(|m|m["source"].clone()),"duplicate_execution_id_equal":duplicate["id"]==id,"elapsed_ms":started.elapsed().as_millis()});
    println!("{}", report);
    if let Some(path) = std::env::var_os("BOB_REAL_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    assert_eq!(
        terminal["status"], "completed",
        "real native result failed: {}",
        terminal["error"]
    );
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[1]["content"], "BOB_NATIVE_REAL 11+7=18");
    assert_eq!(history[1]["source"], "codex_execution");
    assert_eq!(history[1]["reply_to"], message_id);
    // Wait for the actual coordinator to release its process slot before reopening SQLite.
    let cleanup_deadline = Instant::now();
    while !host
        .state::<RunnerState>()
        .active
        .lock()
        .unwrap()
        .is_empty()
    {
        assert!(cleanup_deadline.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(runner::recovered_tree_is_gone(&runner::job_name(
        id.as_str().unwrap()
    )));
    *host.state::<AppState>().0.lock().unwrap() = None;
    drop(view);
    drop(host);
    let (reopened, reopened_view) = app(Some(LocalApp::open(&f.db()).unwrap()));
    assert_eq!(
        invoke(&reopened_view, "app_history", json!({"sessionId":session})).unwrap(),
        history
    );
    let after = invoke(
        &reopened_view,
        "app_execute",
        json!({"requestId":request_id}),
    )
    .unwrap();
    assert_eq!(after["id"], id);
    assert_eq!(after["can_start"], false);
    assert_eq!(after["status"], "completed");
    assert!(reopened
        .state::<RunnerState>()
        .active
        .lock()
        .unwrap()
        .is_empty());
    let final_report = json!({"result":report,"reopen_reply_preserved":true,"duplicate_after_reopen_no_replay":true,"job_absent_after_completion":true});
    println!("{}", final_report);
    if let Some(path) = std::env::var_os("BOB_REAL_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&final_report).unwrap()).unwrap();
    }
}
