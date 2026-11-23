use bob_kernel::outbox::{AppendMessage, Error, Store};
use rusqlite::Connection;
const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn request() -> AppendMessage {
    AppendMessage {
        request_id: "r".into(),
        message_id: "m".into(),
        session_id: "s".into(),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        content: serde_json::json!({"text":"hello"}),
    }
}
fn fixture(p: &std::path::Path) -> Store {
    let mut s = Store::open(p, 0).unwrap();
    s.initialize_local_identity("replica", "home", 1, 0)
        .unwrap();
    Connection::open(p).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='fixture-key' WHERE id='replica'",[]).unwrap();
    s.grant_home_conversation("home", 1, "replica", PIN, "s", 0, 100, true, 1)
        .unwrap();
    s
}
#[test]
fn grant_is_durable_exact_and_transport_identity_resolves_origin() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("h.db");
    let mut s = fixture(&p);
    for (h, e, pin, c) in [
        ("other", 1, PIN, "s"),
        ("home", 2, PIN, "s"),
        ("home", 1, PIN, "other"),
        ("home", 1, "unverified", "s"),
    ] {
        assert!(s.authenticate_home_session(h, e, pin, c, 2).is_err());
    }
    let ctx = s.authenticate_home_session("home", 1, PIN, "s", 2).unwrap();
    assert_eq!(ctx.origin_device_id(), "replica");
    let mut forged = request();
    forged.origin_device_id = "home".into();
    assert!(s.receive_message(&ctx, &forged, 2).is_err());
    s.receive_message(&ctx, &request(), 2).unwrap();
    drop(s);
    let mut s = Store::open(&p, 3).unwrap();
    let ctx = s.authenticate_home_session("home", 1, PIN, "s", 3).unwrap();
    assert!(
        s.lookup_message_receipt(&ctx, &request(), 3)
            .unwrap()
            .unwrap()
            .accepted
    );
    assert_eq!(
        Connection::open(p)
            .unwrap()
            .query_row("SELECT count(*) FROM executions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn revoke_is_persistent_and_replay_lookup_and_cancel_cannot_bypass_it() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("h.db");
    let mut s = fixture(&p);
    let ctx = s.authenticate_home_session("home", 1, PIN, "s", 2).unwrap();
    s.receive_message(&ctx, &request(), 2).unwrap();
    assert_eq!(s.revoke_home_device("home", 1, "replica", 1, 3).unwrap(), 2);
    assert!(s.receive_message(&ctx, &request(), 4).is_err());
    assert!(s.lookup_message_receipt(&ctx, &request(), 4).is_err());
    let cancel = bob_kernel::dispatch::CancelExecution {
        request_id: "cancel".into(),
        session_id: "s".into(),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        execution_id: "unknown".into(),
        expected_version: 1,
        permission_ref: "not-a-grant".into(),
    };
    assert!(s.receive_cancel(&ctx, &cancel, 4).is_err());
    assert!(s.lookup_cancel_receipt(&ctx, &cancel, 4).is_err());
    drop(s);
    let mut s = Store::open(&p, 5).unwrap();
    assert!(s.authenticate_home_session("home", 1, PIN, "s", 5).is_err());
    assert_eq!(
        Connection::open(p)
            .unwrap()
            .query_row("SELECT count(*) FROM inbox_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn expiry_revision_and_operation_are_rechecked_for_existing_contexts() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("h.db");
    let mut s = fixture(&p);
    let old = s.authenticate_home_session("home", 1, PIN, "s", 2).unwrap();
    assert!(s
        .grant_home_conversation("home", 1, "replica", PIN, "s", 0, 100, false, 2)
        .is_err());
    s.grant_home_conversation("home", 1, "replica", PIN, "s", 1, 10, false, 2)
        .unwrap();
    assert!(s.receive_message(&old, &request(), 3).is_err());
    let ctx = s.authenticate_home_session("home", 1, PIN, "s", 3).unwrap();
    s.receive_message(&ctx, &request(), 3).unwrap();
    let cancel = bob_kernel::dispatch::CancelExecution {
        request_id: "cancel".into(),
        session_id: "s".into(),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        execution_id: "unknown".into(),
        expected_version: 1,
        permission_ref: "remote-claims-approved".into(),
    };
    assert!(s.receive_cancel(&ctx, &cancel, 4).is_err());
    assert!(s.lookup_cancel_receipt(&ctx, &cancel, 4).is_err());
    assert!(s.lookup_message_receipt(&ctx, &request(), 10).is_err());
    assert!(s.receive_message(&ctx, &request(), 10).is_err());
    assert!(s
        .authenticate_home_session("home", 1, PIN, "s", 10)
        .is_err());
}
#[test]
fn changed_pairing_or_authority_invalidates_sessions_and_old_conversation_grants() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("h.db");
    let mut s = fixture(&p);
    let ctx = s.authenticate_home_session("home", 1, PIN, "s", 2).unwrap();
    let c = Connection::open(&p).unwrap();
    c.execute(
        "UPDATE devices SET public_key_ref='new-key' WHERE id='replica'",
        [],
    )
    .unwrap();
    assert!(s.receive_message(&ctx, &request(), 3).is_err());
    c.execute(
        "UPDATE devices SET public_key_ref='fixture-key' WHERE id='replica'",
        [],
    )
    .unwrap();
    // Test-only authority transfer; a production transfer requires its own protocol.
    c.execute(
        "UPDATE home_authority SET device_id='replica' WHERE singleton=1",
        [],
    )
    .unwrap();
    assert!(s.receive_message(&ctx, &request(), 3).is_err());
    s.grant_home_conversation("replica", 1, "replica", PIN, "new", 1, 100, true, 3)
        .unwrap();
    assert!(s
        .authenticate_home_session("replica", 1, PIN, "s", 4)
        .is_err());
    assert!(s
        .authenticate_home_session("replica", 1, PIN, "new", 4)
        .is_ok());
    c.execute("UPDATE home_authority SET epoch=2 WHERE singleton=1", [])
        .unwrap();
    s.grant_home_conversation("replica", 2, "replica", PIN, "newest", 2, 100, true, 5)
        .unwrap();
    assert!(s
        .authenticate_home_session("replica", 2, PIN, "new", 6)
        .is_err());
}
#[test]
fn no_implicit_pairing_and_grant_migration_checksum_is_enforced() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("h.db");
    let mut s = Store::open(&p, 0).unwrap();
    s.initialize_local_identity("replica", "home", 1, 0)
        .unwrap();
    assert!(s
        .grant_home_conversation("home", 1, "replica", PIN, "s", 0, 100, true, 1)
        .is_err());
    drop(s);
    Connection::open(&p)
        .unwrap()
        .execute(
            "UPDATE schema_migrations SET checksum='tampered' WHERE version=3",
            [],
        )
        .unwrap();
    assert!(matches!(Store::open(&p, 2), Err(Error::SchemaMismatch)));
}
