//! BOB-D-014 Step 1: unified spark inbox contract (unsorted/sorted, destination links,
//! idempotent retries, version checks, Replica → Home sync and snapshot compatibility).
use bob_kernel::{
    app::LocalApp,
    pairing::{PairingInvitation, ProvenPairingPeer},
    spark_inbox::TriageDestination,
};

const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn dest(kind: &str, ref_id: &str, label: &str) -> TriageDestination {
    TriageDestination {
        kind: kind.to_owned(),
        ref_id: Some(ref_id.to_owned()),
        label: Some(label.to_owned()),
    }
}

#[test]
fn new_sparks_are_unsorted_and_listed_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("app.db")).unwrap();
    app.create_capture("s1", None, "周四下午问一下供应商报价", 10).unwrap();
    app.create_capture("s2", None, "参考图的圆角和留白很舒服", 20).unwrap();

    let all = app.list_inbox(None).unwrap();
    assert_eq!(all.iter().map(|i| i.capture.id.as_str()).collect::<Vec<_>>(), ["s2", "s1"]);
    assert!(all.iter().all(|i| i.triage_status == "unsorted" && i.triage_version == 0));
    assert!(all.iter().all(|i| i.destination_kind.is_none()));
    assert_eq!(app.list_inbox(Some("unsorted")).unwrap().len(), 2);
    assert!(app.list_inbox(Some("sorted")).unwrap().is_empty());
    assert!(app.list_inbox(Some("archived")).is_err());
    assert!(app.capture_triage("s1").unwrap().is_none());
}

#[test]
fn sorting_keeps_the_spark_links_destination_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("app.db");
    let mut app = LocalApp::open(&db).unwrap();
    let cap = app.create_capture("s1", None, "周四下午问一下供应商报价", 10).unwrap();
    let to_calendar = dest("calendar", "sched-1", "日程 · 11月12日 15:00");

    let sorted = app
        .set_capture_triage("s1", Some(&to_calendar), Some("batch-1107"), 0, 30)
        .unwrap();
    assert_eq!(sorted.triage_status, "sorted");
    assert_eq!(sorted.destination_kind.as_deref(), Some("calendar"));
    assert_eq!(sorted.destination_ref_id.as_deref(), Some("sched-1"));
    assert_eq!(sorted.triage_batch_id.as_deref(), Some("batch-1107"));
    assert_eq!(sorted.version, 1);

    // Lost receipt: the identical retry with the old expected version returns the same state.
    assert_eq!(
        app.set_capture_triage("s1", Some(&to_calendar), Some("batch-1107"), 0, 31).unwrap(),
        sorted
    );
    // A different decision against the stale version is rejected.
    assert!(app
        .set_capture_triage("s1", Some(&dest("note", "note-1", "笔记")), None, 0, 32)
        .is_err());

    // The spark itself is never removed or rewritten.
    let captures = app.list_captures().unwrap();
    assert_eq!(captures, vec![cap.clone()]);
    let item = &app.list_inbox(Some("sorted")).unwrap()[0];
    assert_eq!(item.capture, cap);
    assert_eq!(item.destination_label.as_deref(), Some("日程 · 11月12日 15:00"));
    assert_eq!(item.triage_version, 1);
    assert!(app.list_inbox(Some("unsorted")).unwrap().is_empty());

    // Undo: back to unsorted with the current version; destination is cleared.
    let reopened = app.set_capture_triage("s1", None, None, 1, 40).unwrap();
    assert_eq!(reopened.triage_status, "unsorted");
    assert_eq!(reopened.destination_kind, None);
    assert_eq!(reopened.version, 2);

    // State survives reopening the database.
    drop(app);
    let app = LocalApp::open(&db).unwrap();
    assert_eq!(app.capture_triage("s1").unwrap(), Some(reopened));
    assert_eq!(app.list_inbox(Some("unsorted")).unwrap().len(), 1);
}

#[test]
fn triage_rejects_invalid_requests_without_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("app.db")).unwrap();
    app.create_capture("s1", None, "idea", 10).unwrap();

    assert!(app.set_capture_triage("missing", Some(&dest("note", "n", "笔记")), None, 0, 20).is_err());
    assert!(app.set_capture_triage("s1", Some(&dest("trash", "x", "删除")), None, 0, 20).is_err());
    assert!(app
        .set_capture_triage("s1", Some(&dest("note", "n", &"长".repeat(300))), None, 0, 20)
        .is_err());
    assert!(app.set_capture_triage("s1", Some(&dest("note", "  ", "笔记")), None, 0, 20).is_err());
    assert!(app.set_capture_triage("s1", Some(&dest("note", "n", "笔记")), None, 5, 20).is_err());
    // Keeping a never-triaged spark unsorted is a no-op, not a write.
    assert_eq!(app.set_capture_triage("s1", None, None, 0, 20).unwrap().version, 0);
    assert!(app.capture_triage("s1").unwrap().is_none());
    assert_eq!(app.list_inbox(Some("unsorted")).unwrap().len(), 1);
}

#[test]
fn merged_and_project_todo_destinations_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("app.db")).unwrap();
    app.create_capture("a", None, "圆角留白", 10).unwrap();
    app.create_capture("b", None, "首页 Bob 头像问候", 11).unwrap();
    app.create_capture("c", None, "APK 装好后检查手势返回", 12).unwrap();
    app.set_capture_triage("a", Some(&dest("note", "note-ui", "UI 改版想法")), Some("b1"), 0, 20)
        .unwrap();
    app.set_capture_triage("b", Some(&dest("merged", "a", "合并到 UI 改版想法")), Some("b1"), 0, 20)
        .unwrap();
    app.set_capture_triage(
        "c",
        Some(&TriageDestination { kind: "project_todo".into(), ref_id: None, label: Some("bob-agent / todo.md".into()) }),
        Some("b1"),
        0,
        20,
    )
    .unwrap();
    let kinds: Vec<_> = app
        .list_inbox(Some("sorted"))
        .unwrap()
        .into_iter()
        .map(|i| i.destination_kind.unwrap())
        .collect();
    assert_eq!(kinds, ["project_todo", "merged", "note"]);
}

#[test]
fn replica_triage_syncs_to_home_idempotently_with_version_checks() {
    let home_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let mut home = LocalApp::open(&home_dir.path().join("home.db")).unwrap();
    let mut phone = LocalApp::open(&phone_dir.path().join("phone.db")).unwrap();
    let home_id = home.device_id().to_owned();
    let phone_id = phone.device_id().to_owned();
    let phone_session = phone.default_session().unwrap();
    home.issue_pairing_invitation(
        &PairingInvitation {
            id: "inv-inbox".into(),
            home_device_id: home_id.clone(),
            home_epoch: 1,
            home_identity_sha256: PIN.into(),
            expires_at: 50_000,
        },
        &[9u8; 32],
        10,
    )
    .unwrap();
    home.propose_pairing_peer(
        "inv-inbox",
        &[9u8; 32],
        &ProvenPairingPeer {
            device_id: phone_id.clone(),
            identity_sha256: PIN.into(),
            public_key_ref: "phone-pub".into(),
        },
        20,
    )
    .unwrap();
    home.confirm_pairing_peer("inv-inbox", &phone_id, PIN, &phone_session, 60_000, true, 30)
        .unwrap();
    phone.bind_confirmed_home(&home_id, 1, PIN, 35).unwrap();

    phone.create_capture("p-spark", None, "APK 装好后检查手势返回", 100).unwrap();
    phone
        .set_capture_triage("p-spark", Some(&dest("project_todo", "ptw-1", "bob-agent / todo.md")), Some("b1"), 0, 110)
        .unwrap();
    let pending = phone.pending_personal_op_syncs().unwrap();
    assert_eq!(
        pending.iter().map(|e| e.entity_kind.as_str()).collect::<Vec<_>>(),
        ["capture", "capture_triage"]
    );

    let ctx = home.authenticate_home_session(&home_id, 1, PIN, &phone_session, 130).unwrap();
    for entry in &pending {
        assert!(home.host_receive_replica_personal_op(&ctx, entry, 140).unwrap().accepted);
        assert!(home.host_receive_replica_personal_op(&ctx, entry, 141).unwrap().accepted);
        phone.ack_replica_personal_op(&entry.sync_id, 145).unwrap();
    }
    let on_home = &home.list_inbox(Some("sorted")).unwrap()[0];
    assert_eq!(on_home.capture.id, "p-spark");
    assert_eq!(on_home.destination_kind.as_deref(), Some("project_todo"));
    assert_eq!(on_home.triage_version, 1);

    // Home moves on (e.g. sorted again from the web); a stale phone change is rejected.
    home.set_capture_triage("p-spark", Some(&dest("note", "note-9", "笔记")), None, 1, 150).unwrap();
    phone.set_capture_triage("p-spark", None, None, 1, 160).unwrap();
    let stale = phone.pending_personal_op_syncs().unwrap()[0].clone();
    assert_eq!(stale.expected_version, 1);
    assert!(home.host_receive_replica_personal_op(&ctx, &stale, 170).is_err());
    assert_eq!(home.capture_triage("p-spark").unwrap().unwrap().destination_kind.as_deref(), Some("note"));

    // A triage payload whose capture id disagrees with its sync envelope is refused.
    let mut orphan = stale.clone();
    orphan.entity_id = "ghost".into();
    assert!(home.host_receive_replica_personal_op(&ctx, &orphan, 180).is_err());
}

#[test]
fn snapshot_bundle_stays_backward_compatible_and_merges_newer_triage() {
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let mut a = LocalApp::open(&a_dir.path().join("a.db")).unwrap();
    let mut b = LocalApp::open(&b_dir.path().join("b.db")).unwrap();
    a.create_capture("s1", None, "以后也许做个家庭账本", 10).unwrap();

    // Without triage the bundle has the pre-0011 shape (no new key), so older peers verify it.
    let plain = a.export_peer_sync_bundle("sync-1", "{}".into()).unwrap();
    assert!(plain.capture_triage.is_empty());
    assert!(serde_json::to_value(&plain).unwrap().get("capture_triage").is_none());
    b.apply_peer_sync_bundle(&plain, 20).unwrap();
    assert_eq!(b.list_inbox(Some("unsorted")).unwrap().len(), 1);

    a.set_capture_triage("s1", Some(&dest("note", "note-1", "家庭账本")), None, 0, 30).unwrap();
    let with_triage = a.export_peer_sync_bundle("sync-2", "{}".into()).unwrap();
    assert_eq!(with_triage.capture_triage.len(), 1);
    let wire: bob_kernel::personal_ops::PeerSyncBundle =
        serde_json::from_str(&serde_json::to_string(&with_triage).unwrap()).unwrap();
    b.apply_peer_sync_bundle(&wire, 40).unwrap();
    assert_eq!(b.capture_triage("s1").unwrap().unwrap().version, 1);
    assert_eq!(b.list_inbox(Some("sorted")).unwrap()[0].destination_ref_id.as_deref(), Some("note-1"));

    // An older snapshot never rolls back newer local triage.
    b.set_capture_triage("s1", None, None, 1, 50).unwrap();
    b.apply_peer_sync_bundle(&wire, 60).unwrap();
    assert_eq!(b.capture_triage("s1").unwrap().unwrap().triage_status, "unsorted");

    // A tampered triage payload fails the bundle digest.
    let mut tampered = wire.clone();
    tampered.capture_triage[0].destination_label = Some("改过".into());
    assert!(b.apply_peer_sync_bundle(&tampered, 70).is_err());
}

/// Regression for the 0011 outbox rebuild: deleting a note on a paired replica enqueues a
/// `delete` op, which the 0007 CHECK constraint rejected.
#[test]
fn paired_replica_can_delete_a_note_and_home_applies_it() {
    let home_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let mut home = LocalApp::open(&home_dir.path().join("home.db")).unwrap();
    let mut phone = LocalApp::open(&phone_dir.path().join("phone.db")).unwrap();
    let home_id = home.device_id().to_owned();
    let phone_id = phone.device_id().to_owned();
    let phone_session = phone.default_session().unwrap();
    home.issue_pairing_invitation(
        &PairingInvitation {
            id: "inv-del".into(),
            home_device_id: home_id.clone(),
            home_epoch: 1,
            home_identity_sha256: PIN.into(),
            expires_at: 50_000,
        },
        &[7u8; 32],
        10,
    )
    .unwrap();
    home.propose_pairing_peer(
        "inv-del",
        &[7u8; 32],
        &ProvenPairingPeer { device_id: phone_id.clone(), identity_sha256: PIN.into(), public_key_ref: "phone-pub".into() },
        20,
    )
    .unwrap();
    home.confirm_pairing_peer("inv-del", &phone_id, PIN, &phone_session, 60_000, true, 30).unwrap();
    phone.bind_confirmed_home(&home_id, 1, PIN, 35).unwrap();
    let ctx = home.authenticate_home_session(&home_id, 1, PIN, &phone_session, 40).unwrap();

    phone.upsert_note("n-del", None, None, "临时笔记", "body", &[], 100).unwrap();
    for e in phone.pending_personal_op_syncs().unwrap() {
        home.host_receive_replica_personal_op(&ctx, &e, 110).unwrap();
        phone.ack_replica_personal_op(&e.sync_id, 111).unwrap();
    }
    assert_eq!(home.list_notes(None).unwrap().len(), 1);

    phone.delete_note("n-del", 120).unwrap();
    let pending = phone.pending_personal_op_syncs().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_kind, "delete");
    home.host_receive_replica_personal_op(&ctx, &pending[0], 130).unwrap();
    assert!(home.list_notes(None).unwrap().is_empty());
}

#[test]
fn daily_review_drafts_supersede_close_and_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("app.db");
    let mut app = LocalApp::open(&db).unwrap();
    assert!(app.latest_triage_draft().unwrap().is_none());

    let first = app.save_triage_draft("tb-1", r#"{"suggestions":[]}"#, Some("deepseek/v4"), 10).unwrap();
    assert_eq!(first.status, "draft");
    // Same id + same payload: idempotent. Same id + different payload: identity conflict.
    assert_eq!(app.save_triage_draft("tb-1", r#"{"suggestions":[]}"#, None, 11).unwrap(), first);
    assert!(app.save_triage_draft("tb-1", r#"{"suggestions":[1]}"#, None, 12).is_err());
    assert!(app.save_triage_draft("tb-x", "not json", None, 12).is_err());

    // Regenerating supersedes the open draft.
    app.save_triage_draft("tb-2", r#"{"suggestions":[{"a":1}]}"#, None, 20).unwrap();
    assert_eq!(app.latest_triage_draft().unwrap().unwrap().id, "tb-2");
    assert_eq!(app.triage_batch("tb-1").unwrap().unwrap().status, "dismissed");

    // Closing: only open drafts, repeat is idempotent, bad status rejected.
    assert!(app.close_triage_batch("tb-2", "draft", 30).is_err());
    assert_eq!(app.close_triage_batch("tb-2", "applied", 30).unwrap().status, "applied");
    assert_eq!(app.close_triage_batch("tb-2", "applied", 31).unwrap().status, "applied");
    assert!(app.close_triage_batch("tb-2", "dismissed", 32).is_err());
    assert!(app.close_triage_batch("tb-1", "applied", 33).is_err());
    assert!(app.latest_triage_draft().unwrap().is_none());

    app.save_triage_draft("tb-3", r#"{"suggestions":[]}"#, None, 40).unwrap();
    drop(app);
    let app = LocalApp::open(&db).unwrap();
    assert_eq!(app.latest_triage_draft().unwrap().unwrap().id, "tb-3");
}
