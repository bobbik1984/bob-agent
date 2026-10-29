use bob_kernel::{
    inbox::HomeMessageContext,
    outbox::{AppendMessage, Store},
};
use rusqlite::Connection;
use serde_json::json;

fn request() -> AppendMessage {
    AppendMessage {
        request_id: "r1".into(),
        session_id: "s1".into(),
        message_id: "m1".into(),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        content: json!({"text":"hello"}),
    }
}
const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn ctx(s: &mut Store) -> HomeMessageContext {
    s.authenticate_home_session("home", 1, PIN, "s1", 1)
        .unwrap()
}
fn fixture(path: &std::path::Path) -> Store {
    let mut store = Store::open(path, 1).unwrap();
    store
        .initialize_local_identity("replica", "home", 1, 1)
        .unwrap();
    // Test-only trust fixture. Production pairing must authenticate/authorize.
    Connection::open(path).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='fixture-key-ref' WHERE id='replica'",[]).unwrap();
    store
        .grant_home_conversation("home", 1, "replica", PIN, "s1", 0, 100, true, 1)
        .unwrap();
    store
}
fn counts(path: &std::path::Path) -> (i64, i64, i64) {
    let c = Connection::open(path).unwrap();
    (
        c.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
            .unwrap(),
        c.query_row("SELECT count(*) FROM inbox_receipts", [], |r| r.get(0))
            .unwrap(),
        c.query_row("SELECT count(*) FROM domain_events", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn home_acceptance_receipt_survives_restart_without_duplicate_effects() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    let context = ctx(&mut s);
    let receipt = s.receive_message(&context, &request(), 2).unwrap();
    assert!(receipt.accepted);
    drop(s);
    let mut s = Store::open(&p, 3).unwrap();
    let lookup = s
        .lookup_message_receipt(&context, &request(), 5)
        .unwrap()
        .unwrap();
    assert_eq!(lookup.payload_sha256, receipt.payload_sha256);
    s.receive_message(&context, &request(), 4).unwrap();
    assert_eq!(counts(&p), (1, 1, 1));
    let mut changed = request();
    changed.content = json!({"text":"different"});
    assert!(s.receive_message(&context, &changed, 5).is_err());
    assert!(s.lookup_message_receipt(&context, &changed, 5).is_err());
    assert_eq!(counts(&p), (1, 1, 1));
}
#[test]
fn rejects_wrong_identity_epoch_scope_and_revoked_peer() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    let context = ctx(&mut s);
    for field in ["origin", "home", "conversation"] {
        let mut wrong = request();
        match field {
            "origin" => wrong.origin_device_id = "other".into(),
            "home" => wrong.home_device_id = "other".into(),
            _ => wrong.session_id = "other".into(),
        }
        assert!(s.receive_message(&context, &wrong, 2).is_err());
    }
    let mut r = request();
    r.home_epoch = 2;
    assert!(s.receive_message(&context, &r, 2).is_err());
    Connection::open(&p)
        .unwrap()
        .execute(
            "UPDATE devices SET trust_state='revoked' WHERE id='replica'",
            [],
        )
        .unwrap();
    assert!(s.receive_message(&context, &request(), 2).is_err());
    assert!(s.lookup_message_receipt(&context, &request(), 5).is_err());
    assert_eq!(counts(&p), (0, 0, 0));
}
#[test]
fn event_failure_rolls_back_message_and_receipt() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    let context = ctx(&mut s);
    let c = Connection::open(&p).unwrap();
    c.execute_batch("CREATE TRIGGER fixture_failure BEFORE INSERT ON domain_events BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(s.receive_message(&context, &request(), 2).is_err());
    assert_eq!(counts(&p), (0, 0, 0));
    c.execute_batch("DROP TRIGGER fixture_failure;").unwrap();
    s.receive_message(&context, &request(), 3).unwrap();
    assert_eq!(counts(&p), (1, 1, 1));
}
#[test]
fn absent_query_is_not_receipt_and_message_id_collision_does_not_record_acceptance() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    let context = ctx(&mut s);
    assert!(s
        .lookup_message_receipt(&context, &request(), 5)
        .unwrap()
        .is_none());
    s.receive_message(&context, &request(), 2).unwrap();
    let mut r = request();
    r.request_id = "r2".into();
    assert!(s.receive_message(&context, &r, 3).is_err());
    assert!(s.lookup_message_receipt(&context, &r, 5).unwrap().is_none());
    assert_eq!(counts(&p), (1, 1, 1));
}

fn cancel() -> bob_kernel::dispatch::CancelExecution {
    bob_kernel::dispatch::CancelExecution {
        request_id: "cancel1".into(),
        session_id: "s1".into(),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        execution_id: "e1".into(),
        expected_version: 1,
        permission_ref: "fixture-stop-grant".into(),
    }
}
fn execution_fixture(p: &std::path::Path) {
    Connection::open(p).unwrap().execute_batch("PRAGMA foreign_keys=ON;
 INSERT INTO projects(id,title,status,version,created_at,updated_at) VALUES('p1','','active',1,1,1);
 INSERT INTO workspaces VALUES('w1','home','fixture-directory','fixture-policy',1);
 INSERT INTO project_workspaces VALUES('p1','w1');
 INSERT INTO tasks(id,project_id,source_message_id,title,status,constraints_json,version,created_at,updated_at) VALUES('t1','p1','m1','','running','{}',1,1,1);
 INSERT INTO executions(id,project_id,task_id,workspace_id,request_id,status,requested_json,contract_json,contract_sha256,version) VALUES('e1','p1','t1','w1','exec-r1','running','{}','{}','fixture-hash',1);
 INSERT INTO workspace_leases VALUES('w1','e1',1,1,'held',1);").unwrap();
}
#[test]
fn cancel_is_durable_request_not_stop_confirmation() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    let context = ctx(&mut s);
    s.receive_message(&context, &request(), 2).unwrap();
    execution_fixture(&p);
    assert!(s.receive_cancel(&context, &cancel(), 3).unwrap().accepted);
    assert!(s.receive_cancel(&context, &cancel(), 4).unwrap().accepted);
    drop(s);
    let mut s = Store::open(&p, 5).unwrap();
    assert!(
        s.lookup_cancel_receipt(&context, &cancel(), 5)
            .unwrap()
            .unwrap()
            .accepted
    );
    let c = Connection::open(&p).unwrap();
    let state:(String,Option<i64>,Option<i64>,i64)=c.query_row("SELECT status,stop_requested_at,stop_confirmed_at,version FROM executions WHERE id='e1'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(state, ("running".into(), Some(3), None, 2));
    let lease: String = c
        .query_row(
            "SELECT state FROM workspace_leases WHERE workspace_id='w1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(lease, "held");
    assert_eq!(counts(&p), (1, 2, 2));
}
#[test]
fn stale_cancel_rejection_is_persistent_and_does_not_change_execution() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    let context = ctx(&mut s);
    s.receive_message(&context, &request(), 2).unwrap();
    execution_fixture(&p);
    let mut bad = cancel();
    bad.expected_version = 9;
    assert!(!s.receive_cancel(&context, &bad, 3).unwrap().accepted);
    assert!(!s.receive_cancel(&context, &bad, 4).unwrap().accepted);
    let stop: Option<i64> = Connection::open(&p)
        .unwrap()
        .query_row(
            "SELECT stop_requested_at FROM executions WHERE id='e1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stop, None);
    assert_eq!(counts(&p), (1, 2, 1));
    let mut changed = bad;
    changed.expected_version = 1;
    assert!(s.receive_cancel(&context, &changed, 5).is_err());
}

#[test]
fn home_local_first_message_promotes_atomically_without_duplicate() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    Connection::open(&p).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='fixture-home-key' WHERE id='home'",[]).unwrap();
    let mut r = request();
    r.origin_device_id = "home".into();
    s.grant_home_conversation("home", 1, "home", &"b".repeat(64), "s1", 0, 100, true, 1)
        .unwrap();
    let c = s
        .authenticate_home_session("home", 1, &"b".repeat(64), "s1", 1)
        .unwrap();
    let saved = s.accept_message(&r, 2).unwrap();
    s.begin_attempt(&r.request_id, 3).unwrap();
    let receipt = s.receive_message(&c, &r, 4).unwrap();
    assert_eq!(receipt.payload_sha256, saved.payload_sha256);
    s.apply_receipt(&receipt, 5).unwrap();
    assert_eq!(s.state(&r.request_id).unwrap(), "peer_accepted");
    s.receive_message(&c, &r, 6).unwrap();
    assert_eq!(counts(&p), (1, 1, 1));
}
#[test]
fn mismatched_local_message_cannot_be_promoted() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("home.sqlite");
    let mut s = fixture(&p);
    Connection::open(&p).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='fixture-home-key' WHERE id='home'",[]).unwrap();
    let mut r = request();
    r.origin_device_id = "home".into();
    s.grant_home_conversation("home", 1, "home", &"b".repeat(64), "s1", 0, 100, true, 1)
        .unwrap();
    let c = s
        .authenticate_home_session("home", 1, &"b".repeat(64), "s1", 1)
        .unwrap();
    s.accept_message(&r, 2).unwrap();
    Connection::open(&p)
        .unwrap()
        .execute("UPDATE messages SET content_json='{}' WHERE id='m1'", [])
        .unwrap();
    assert!(s.receive_message(&c, &r, 3).is_err());
    assert_eq!(counts(&p), (1, 0, 0));
}
