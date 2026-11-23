use bob_kernel::outbox::*;
use serde_json::json;
use std::path::Path;

fn request() -> AppendMessage {
    AppendMessage {
        request_id: "r1".into(),
        session_id: "s1".into(),
        message_id: "m1".into(),
        origin_device_id: "replica".into(),
        home_device_id: "authority".into(),
        home_epoch: 1,
        content: json!({"text":"hello"}),
    }
}
#[test]
fn disk_acceptance_idempotency_and_restart_ambiguity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kernel.sqlite");
    let mut store = Store::open(&path, 1).unwrap();
    store
        .initialize_local_identity("replica", "authority", 1, 1)
        .unwrap();
    let request = request();
    let accepted = store.accept_message(&request, 2).unwrap();
    assert_eq!(store.accept_message(&request, 3).unwrap(), accepted);
    let mut altered = request.clone();
    altered.content = json!({"text":"other"});
    assert!(matches!(
        store.accept_message(&altered, 3),
        Err(Error::IdentityConflict)
    ));
    let attempt = store.begin_attempt("r1", 4).unwrap();
    store
        .finish_attempt("r1", attempt, AttemptOutcome::ConfirmedZeroBytes)
        .unwrap();
    assert_eq!(store.state("r1").unwrap(), "not_dispatched");
    store.begin_attempt("r1", 5).unwrap();
    drop(store);
    let mut store = Store::open(&path, 6).unwrap();
    assert_eq!(store.state("r1").unwrap(), "unknown_send");
    assert!(store.begin_attempt("r1", 7).is_err());
    let mut receipt = HomeReceipt {
        request_id: "r1".into(),
        home_device_id: "authority".into(),
        home_epoch: 2,
        payload_sha256: accepted.payload_sha256,
        accepted: true,
    };
    assert!(store.apply_receipt(&receipt, 8).is_err());
    receipt.home_epoch = 1;
    store.apply_receipt(&receipt, 9).unwrap();
    assert_eq!(store.state("r1").unwrap(), "peer_accepted");
    store.apply_receipt(&receipt, 10).unwrap();
    receipt.accepted = false;
    assert!(store.apply_receipt(&receipt, 11).is_err());
}
#[test]
fn rejects_volatile_database() {
    assert!(matches!(
        Store::open(Path::new(":memory:"), 0),
        Err(Error::PersistentPathRequired)
    ));
    assert!(matches!(
        Store::open(Path::new("file:mem?mode=memory"), 0),
        Err(Error::PersistentPathRequired)
    ));
}
#[test]
fn invalid_foreign_key_rolls_back_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("kernel.sqlite"), 1).unwrap();
    store
        .initialize_local_identity("replica", "authority", 1, 1)
        .unwrap();
    let mut bad = request();
    bad.origin_device_id = "unknown".into();
    assert!(store.accept_message(&bad, 2).is_err());
    assert!(store.state("r1").is_err());
    assert!(store.accept_message(&request(), 3).is_ok());
}

#[test]
fn uncertain_attempt_blocks_new_normal_message_and_stale_completion() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("kernel.sqlite"), 1).unwrap();
    store
        .initialize_local_identity("replica", "authority", 1, 1)
        .unwrap();
    store.accept_message(&request(), 2).unwrap();
    let attempt = store.begin_attempt("r1", 3).unwrap();
    store
        .finish_attempt("r1", attempt, AttemptOutcome::PossiblySent)
        .unwrap();
    assert!(store
        .finish_attempt("r1", attempt, AttemptOutcome::ConfirmedZeroBytes)
        .is_err());
    let mut next = request();
    next.request_id = "r2".into();
    next.message_id = "m2".into();
    store.accept_message(&next, 4).unwrap();
    assert!(store.begin_attempt("r2", 5).is_err());
    assert_eq!(store.state("r2").unwrap(), "local_saved");
}

#[test]
fn live_owner_blocks_second_store_and_releases_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kernel.sqlite");
    let mut store = Store::open(&path, 1).unwrap();
    store
        .initialize_local_identity("replica", "authority", 1, 1)
        .unwrap();
    store.accept_message(&request(), 2).unwrap();
    store.begin_attempt("r1", 3).unwrap();
    let alias = dir.path().join(".").join("kernel.sqlite");
    assert!(matches!(
        Store::open(&alias, 4),
        Err(Error::OwnershipUnavailable)
    ));
    assert_eq!(store.state("r1").unwrap(), "dispatching");
    drop(store);
    let reopened = Store::open(&path, 5).unwrap();
    assert_eq!(reopened.state("r1").unwrap(), "unknown_send");
}
#[test]
fn reused_message_identity_rolls_back_new_outbox() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("kernel.sqlite"), 1).unwrap();
    store
        .initialize_local_identity("replica", "authority", 1, 1)
        .unwrap();
    store.accept_message(&request(), 2).unwrap();
    let mut changed = request();
    changed.request_id = "r2".into();
    assert!(store.accept_message(&changed, 3).is_err());
    assert!(store.state("r2").is_err());
    assert_eq!(store.state("r1").unwrap(), "local_saved");
}
#[cfg(unix)]
#[test]
fn symlink_database_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.sqlite");
    drop(Store::open(&real, 1).unwrap());
    let link = dir.path().join("link.sqlite");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(matches!(Store::open(&link, 2), Err(Error::LinkedDatabase)));
}
