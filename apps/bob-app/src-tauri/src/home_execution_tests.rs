//! Real Windows Job process integration from an authenticated remote message.
//! Pairing identities here are explicit test fixtures, not a live phone handshake.
#![cfg(windows)]
use crate::runner;
use bob_kernel::{
    app::LocalApp,
    outbox::{AppendMessage, Store},
    pairing::{PairingInvitation, ProvenPairingPeer},
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("bob-home-native-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn clock() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}
fn run_remote(program: &Path, prompt: &str, expected: &str, real: bool) {
    let worker = PathBuf::from(
        std::env::var_os("BOB_TEST_WORKER")
            .expect("build and configure the actual Bob worker first"),
    );
    assert!(worker.is_absolute() && worker.is_file());
    assert!(program.is_absolute() && program.is_file());
    let dir = TestDirectory::new();
    let db = dir.0.join("kernel.sqlite");
    let work = dir.0.join("empty-workspace");
    std::fs::create_dir(&work).unwrap();
    let app = LocalApp::open(&db).unwrap();
    let home = app.device_id().to_owned();
    drop(app);
    let n = clock();
    let mut store = Store::open(&db, n).unwrap();
    store
        .issue_pairing_invitation(
            &PairingInvitation {
                id: "native-test-invite".into(),
                home_device_id: home.clone(),
                home_epoch: 1,
                home_identity_sha256: PIN.into(),
                expires_at: n + 300_000,
            },
            &[5; 32],
            n,
        )
        .unwrap();
    store
        .propose_pairing_peer(
            "native-test-invite",
            &[5; 32],
            &ProvenPairingPeer {
                device_id: "test-phone".into(),
                identity_sha256: PIN.into(),
                public_key_ref: "fixture-public-key-only".into(),
            },
            n,
        )
        .unwrap();
    store
        .confirm_pairing_peer(
            "native-test-invite",
            "test-phone",
            PIN,
            "phone-session",
            n + 600_000,
            true,
            n,
        )
        .unwrap();
    let ctx = store
        .authenticate_home_session(&home, 1, PIN, "phone-session", n)
        .unwrap();
    let request = AppendMessage {
        request_id: format!("home-native-{}-{n}", std::process::id()),
        message_id: "phone-original-message".into(),
        session_id: "phone-session".into(),
        origin_device_id: "test-phone".into(),
        home_device_id: home.clone(),
        home_epoch: 1,
        content: json!({"text":prompt}),
    };
    let receipt = store.receive_message(&ctx, &request, n).unwrap();
    assert!(receipt.accepted);
    drop(store);
    let mut app = LocalApp::open(&db).unwrap();
    let workspace = work.to_str().unwrap();
    let grant = app
        .approve_remote_readonly(&request, workspace, n + 300_000, n)
        .unwrap();
    let execution = app
        .prepare_remote_readonly(&ctx, &request, workspace, &grant, n)
        .unwrap();
    assert!(execution.can_start);
    let duplicate = app
        .prepare_remote_readonly(&ctx, &request, workspace, &grant, n)
        .unwrap();
    assert_eq!(duplicate.id, execution.id);
    assert!(!duplicate.can_start);
    let job = runner::job_name(&execution.id);
    assert_eq!(execution.owner_job.as_deref(), Some(job.as_str()));
    let outcome = runner::execute(
        program,
        &work,
        execution.prompt.clone(),
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(if real { 120 } else { 20 }),
        &job,
        || {
            app.record_local_owner(&execution.id, &job, clock()).is_ok()
                && app
                    .recheck_remote_readonly(&ctx, &request, &grant, clock())
                    .is_ok()
                && app.mark_local_started(&execution.id, clock()).is_ok()
        },
    );
    app.finish_local_execution(
        &execution.id,
        &outcome.stream,
        outcome.exit,
        outcome.reason,
        outcome.cleanup,
        clock(),
    )
    .unwrap();
    assert!(outcome.cleanup, "worker tree cleanup is unconfirmed");
    assert!(runner::recovered_tree_is_gone(&job));
    assert_eq!(outcome.reason, None);
    assert_eq!(outcome.exit, Some(0));
    drop(app);
    let mut store = Store::open(&db, clock()).unwrap();
    let fresh = store
        .authenticate_home_session(&home, 1, PIN, "phone-session", clock())
        .unwrap();
    let result = store.lookup_home_result(&fresh, &request, clock()).unwrap();
    assert!(result.receipt.unwrap().accepted);
    let returned = result.execution.unwrap();
    assert_eq!(returned.execution_id, execution.id);
    assert_eq!(returned.status, "completed");
    assert_eq!(returned.reply.unwrap().text, expected);
    drop(store);
    let sql = rusqlite::Connection::open(&db).unwrap();
    let mut q = sql
        .prepare(
            "SELECT origin_device_id,conversation_id,role,acceptance FROM messages ORDER BY rowid",
        )
        .unwrap();
    let rows: Vec<(String, String, String, String)> = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "test-phone".into(),
                "phone-session".into(),
                "user".into(),
                "home".into()
            ),
            (
                home,
                "phone-session".into(),
                "assistant".into(),
                "home".into()
            )
        ]
    );
    assert_eq!(
        sql.query_row("SELECT count(*) FROM intent_outbox", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        sql.query_row("SELECT count(*) FROM executions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(q);
    drop(sql);
    let mut reopened = LocalApp::open(&db).unwrap();
    let again = reopened
        .prepare_remote_readonly(&fresh, &request, workspace, &grant, clock())
        .unwrap();
    assert_eq!(again.id, execution.id);
    assert_eq!(again.status, "completed");
    assert!(!again.can_start);
    assert_eq!(reopened.managed_executions().unwrap().len(), 1);
    let report = json!({"kind":if real {"real_codex_remote_source_native_runner"}else{"fake_cli_remote_source_real_windows_job"},"execution_id":execution.id,"source_session":"phone-session","source_origin":"test-phone","message_rows":2,"execution_rows":1,"fake_local_outbox_rows":0,"reply":expected,"cleanup_confirmed":true,"job_absent_or_empty":true,"reopen_no_replay":true,"phone_or_network_test":false});
    println!("{report}");
    if real {
        if let Some(path) = std::env::var_os("BOB_HOME_REAL_EVIDENCE") {
            std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
    }
}

#[test]
#[ignore = "requires built Bob worker and fake CLI; no model or phone"]
fn remote_home_fake_cli_native_job_reply_and_restart() {
    let program = PathBuf::from(
        std::env::var_os("BOB_TEST_FIXTURE").expect("set explicit fake CLI fixture path"),
    );
    run_remote(&program, "fixture-success", "FIXTURE_REPLY", false);
}

#[test]
#[ignore = "requires separately selected approved real Codex read-only test"]
fn remote_home_real_codex_native_job_reply_and_restart() {
    assert_eq!(
        std::env::var("BOB_HOME_REAL_TEST").as_deref(),
        Ok("1"),
        "explicit real test opt-in required"
    );
    let program = runner::configured_program()
        .expect("configure official Codex CLI, never a fixture fallback");
    run_remote(
        &program,
        "Do not use tools or read/write files. Reply exactly: BOB_HOME_NATIVE_REAL 13+8=21",
        "BOB_HOME_NATIVE_REAL 13+8=21",
        true,
    );
}
