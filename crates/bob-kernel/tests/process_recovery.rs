use bob_kernel::outbox::{AppendMessage, Store};
use serde_json::json;
use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "subprocess fixture invoked only by crash recovery test"]
fn child_dispatch_fixture() {
    let Some(root) = std::env::var_os("BOB_TEST_CRASH_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let mut store = Store::open(&root.join("test.sqlite"), 1).unwrap();
    store
        .initialize_local_identity("replica", "home", 1, 1)
        .unwrap();
    store
        .accept_message(
            &AppendMessage {
                request_id: "crash-request".into(),
                session_id: "test-session".into(),
                message_id: "test-message".into(),
                origin_device_id: "replica".into(),
                home_device_id: "home".into(),
                home_epoch: 1,
                content: json!({"text":"fixture"}),
            },
            2,
        )
        .unwrap();
    store.begin_attempt("crash-request", 3).unwrap();
    fs::write(root.join("ready"), b"committed").unwrap();
    loop {
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn abrupt_process_exit_preserves_message_and_blocks_replay() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_dispatch_fixture"])
        .env("BOB_TEST_CRASH_ROOT", root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ready = false;
    while Instant::now() < deadline {
        if root.path().join("ready").exists() {
            ready = true;
            break;
        }
        if child.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    // Always reap the test child, including timeout and early-exit paths.
    let _ = child.kill();
    child.wait().unwrap();
    assert!(
        ready,
        "child did not commit dispatch boundary before deadline"
    );
    let mut store = Store::open(&root.path().join("test.sqlite"), 4).unwrap();
    assert_eq!(store.state("crash-request").unwrap(), "unknown_send");
    assert!(store.begin_attempt("crash-request", 5).is_err());
    let db = rusqlite::Connection::open(root.path().join("test.sqlite")).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM messages WHERE request_id='crash-request'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
