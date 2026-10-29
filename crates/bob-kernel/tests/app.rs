use bob_kernel::app::LocalApp;
#[test]
fn command_facade_keeps_identity_history_and_receipt_after_restart() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("app.sqlite");
    let mut app = LocalApp::open(&p).unwrap();
    let session = app.default_session().unwrap();
    let receipt = app
        .accept("request", "message", &session, "hello".into(), 1)
        .unwrap();
    assert_eq!(
        receipt,
        app.accept("request", "message", &session, "hello".into(), 2)
            .unwrap()
    );
    assert_eq!(app.prewarm(), "provider_unconfigured");
    drop(app);
    let app = LocalApp::open(&p).unwrap();
    assert_eq!(app.default_session().unwrap(), session);
    let history = app.history(&session).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content, "hello");
    assert_eq!(history[0].send_state, "local_saved");
}
#[test]
fn invalid_commands_do_not_save_or_clear_existing_history() {
    let d = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&d.path().join("app.sqlite")).unwrap();
    let session = app.default_session().unwrap();
    assert!(app.accept("r", "m", "wrong", "text".into(), 1).is_err());
    assert!(app.history("wrong").is_err());
    assert!(app.accept("r", "m", &session, " ".into(), 1).is_err());
    assert!(app
        .accept("r", "m", &session, "x".repeat(256 * 1024 + 1), 1)
        .is_err());
    assert_eq!(app.history(&session).unwrap().len(), 0);
    app.accept("r", "m", &session, "one".into(), 1).unwrap();
    assert!(app.accept("r", "m", &session, "two".into(), 2).is_err());
    assert_eq!(app.history(&session).unwrap()[0].content, "one");
}

#[test]
fn readiness_reports_disk_counts_without_implying_execution() {
    let d = tempfile::tempdir().unwrap(); let p=d.path().join("app.sqlite");
    let mut app=LocalApp::open(&p).unwrap(); let session=app.default_session().unwrap();
    assert_eq!(app.readiness().unwrap().saved_messages,0);
    app.accept("status-r","status-m",&session,"saved only".into(),1).unwrap();
    drop(app); let app=LocalApp::open(&p).unwrap(); let status=app.readiness().unwrap();
    assert_eq!(status.saved_messages,1); assert_eq!(status.pending_messages,1);
    assert_eq!(status.mode,"local_only"); assert!(!status.execution_available);
}

#[test]
fn captured_real_cli_reply_is_atomic_idempotent_and_restored_without_fake_delivery() {
    let d=tempfile::tempdir().unwrap();let path=d.path().join("app.sqlite");
    let mut app=LocalApp::open(&path).unwrap();let session=app.default_session().unwrap();
    let events=include_str!("fixtures/codex-reply-probe.jsonl");
    assert!(app.record_cli_probe("missing",events,Some(0),2).is_err());
    app.accept("probe","source",&session,"approved independent CLI probe".into(),1).unwrap();
    assert!(app.record_cli_probe("probe",events,Some(1),2).is_err());
    assert_eq!(app.history(&session).unwrap().len(),1);
    let reply=app.record_cli_probe("probe",events,Some(0),2).unwrap();
    assert_eq!(app.record_cli_probe("probe",events,Some(0),3).unwrap(),reply);
    assert!(app.record_cli_probe("probe",&events.replace("2+3=5","changed"),Some(0),4).is_err());
    app.accept("second","source2",&session,"another probe".into(),5).unwrap();
    assert!(app.record_cli_probe("second",events,Some(0),6).is_err());
    drop(app);let app=LocalApp::open(&path).unwrap();let h=app.history(&session).unwrap();
    assert_eq!(h.len(),3);assert_eq!(h[1].role,"assistant");
    assert_eq!(h[1].content,"BOB_REPLY_OK 2+3=5");assert_eq!(h[1].reply_to.as_deref(),Some("source"));
    assert_eq!(h[0].send_state,"local_saved");assert!(!app.readiness().unwrap().execution_available);
}
