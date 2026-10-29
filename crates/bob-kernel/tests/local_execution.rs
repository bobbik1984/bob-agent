use bob_kernel::app::LocalApp;
use rusqlite::Connection;

fn fixture() -> (tempfile::TempDir, LocalApp, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    let session = app.default_session().unwrap();
    (dir, app, session)
}
fn prepare(
    app: &mut LocalApp,
    session: &str,
    key: &str,
) -> bob_kernel::local_execution::LocalExecution {
    app.accept(
        key,
        &format!("m-{key}"),
        session,
        format!("reply for {key}"),
        1,
    )
    .unwrap();
    app.prepare_local_execution(key, &format!("ephemeral/{key}"), 2)
        .unwrap()
}
fn reply() -> &'static str {
    include_str!("fixtures/codex-reply-probe.jsonl")
}

#[test]
fn real_reply_contract_persists_atomically_and_deduplicates_dispatch() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "one");
    assert!(e.can_start);
    assert_eq!(e.prompt, "reply for one");
    let again = app
        .prepare_local_execution("one", "not-another-dispatch", 3)
        .unwrap();
    assert_eq!(again.id, e.id);
    assert!(!again.can_start);
    app.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 2)
        .unwrap();
    app.mark_local_started(&e.id, 4).unwrap();
    assert!(app.mark_local_started(&e.id, 5).is_err());
    app.finish_local_execution(&e.id, reply(), Some(0), None, true, 6)
        .unwrap();
    app.finish_local_execution(&e.id, reply(), Some(0), None, true, 7)
        .unwrap();
    assert!(app
        .finish_local_execution(&e.id, "changed", Some(0), None, true, 8)
        .is_err());
    drop(app);
    let app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    let h = app.history(&s).unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].send_state, "execution_completed");
    assert_eq!(h[1].content, "BOB_REPLY_OK 2+3=5");
    assert_eq!(h[1].source, "codex_execution");
    assert_eq!(h[1].reply_to.as_deref(), Some("m-one"));
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "completed");
    assert_eq!(app.readiness().unwrap().pending_messages, 0);
    drop(app);
    let db = Connection::open(dir.path().join("app.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT status FROM tasks", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "awaiting_review"
    );
    assert_eq!(
        db.query_row("SELECT send_state FROM intent_outbox", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "needs_attention"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM inbox_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM reviews", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn cancellation_wins_over_late_success_and_requires_cleanup() {
    let (_dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "cancel");
    app.request_local_stop(&e.id, 3).unwrap();
    assert!(app.mark_local_started(&e.id, 4).is_err());
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "dispatching");
    app.finish_local_execution(&e.id, reply(), Some(0), None, true, 5)
        .unwrap();
    let run = &app.local_executions(&s).unwrap()[0];
    assert_eq!(run.status, "cancelled");
    assert_eq!(app.history(&s).unwrap().len(), 1);
    assert!(app
        .finish_local_execution(&e.id, reply(), Some(0), None, false, 6)
        .is_err());
}

#[test]
fn timeout_and_invalid_stream_cannot_generate_a_reply() {
    let (_dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "timeout");
    app.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 2)
        .unwrap();
    app.mark_local_started(&e.id, 3).unwrap();
    app.finish_local_execution(&e.id, reply(), Some(0), Some("timeout"), true, 4)
        .unwrap();
    let run = &app.local_executions(&s).unwrap()[0];
    assert_eq!(run.status, "failed");
    assert_eq!(run.error.as_deref(), Some("timeout"));
    let e = prepare(&mut app, &s, "invalid");
    app.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 2)
        .unwrap();
    app.mark_local_started(&e.id, 5).unwrap();
    app.finish_local_execution(&e.id, "not-json", Some(0), None, true, 6)
        .unwrap();
    assert_eq!(
        app.local_executions(&s).unwrap()[1].error.as_deref(),
        Some("invalid_reply")
    );
    assert_eq!(app.history(&s).unwrap().len(), 2);
}

#[test]
fn restart_keeps_unknown_and_never_reissues_old_or_new_request() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "orphan");
    app.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 2)
        .unwrap();
    app.mark_local_started(&e.id, 3).unwrap();
    drop(app);
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    let run = app.prepare_local_execution("orphan", "ignored", 5).unwrap();
    assert_eq!(run.id, e.id);
    assert!(!run.can_start);
    assert_eq!(run.status, "unknown");
    assert_eq!(run.error.as_deref(), Some("restart_unconfirmed"));
    assert!(app.mark_local_started(&e.id, 6).is_err());
    assert!(app
        .finish_local_execution(&e.id, reply(), Some(0), None, true, 7)
        .is_err());
    app.accept("new", "m-new", &s, "next request".into(), 8)
        .unwrap();
    assert!(app.prepare_local_execution("new", "fresh", 9).is_err());
    assert_eq!(app.local_executions(&s).unwrap().len(), 1);
}

#[test]
fn unconfirmed_cleanup_holds_capacity_and_lease() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "cleanup");
    app.finish_local_execution(&e.id, "", Some(1), Some("timeout"), false, 4)
        .unwrap();
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "unknown");
    assert_eq!(
        app.local_executions(&s).unwrap()[0].error.as_deref(),
        Some("cleanup_unconfirmed")
    );
    drop(app);
    let db = Connection::open(dir.path().join("app.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT state FROM workspace_leases", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "unknown"
    );
    assert_eq!(
        db.query_row("SELECT ended_at FROM executions", [], |r| r
            .get::<_, Option<i64>>(0))
            .unwrap(),
        None
    );
}

#[test]
fn wrong_request_or_foreign_execution_has_no_side_effects() {
    let (_dir, mut app, s) = fixture();
    assert!(app
        .prepare_local_execution("absent", "ephemeral", 1)
        .is_err());
    assert!(app.local_executions("other").is_err());
    assert!(app.request_local_stop("other", 1).is_err());
    assert!(app.mark_local_started("other", 1).is_err());
    assert!(app
        .finish_local_execution("other", reply(), Some(0), None, true, 1)
        .is_err());
    assert!(app.local_executions(&s).unwrap().is_empty());
}

#[test]
fn reply_write_failure_rolls_back_terminal_evidence_and_allows_same_result_retry() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "atomic");
    app.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 2)
        .unwrap();
    app.mark_local_started(&e.id, 3).unwrap();
    let db = Connection::open(dir.path().join("app.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_reply BEFORE INSERT ON messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT,'test fault'); END;").unwrap();
    assert!(app
        .finish_local_execution(&e.id, reply(), Some(0), None, true, 4)
        .is_err());
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "running");
    assert_eq!(
        db.query_row("SELECT count(*) FROM evidence", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM native_sessions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(app.history(&s).unwrap().len(), 1);
    db.execute_batch("DROP TRIGGER fail_reply;").unwrap();
    app.finish_local_execution(&e.id, reply(), Some(0), None, true, 5)
        .unwrap();
    assert_eq!(app.history(&s).unwrap().len(), 2);
}

#[test]
fn missing_owner_cannot_start_or_be_guessed_stopped_after_restart() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "no-owner");
    // Simulate a legacy interrupted row created before owner metadata existed.
    let db = Connection::open(dir.path().join("app.sqlite")).unwrap();
    db.execute_batch(
        "DROP TRIGGER immutable_execution_contract; UPDATE executions SET resolved_json=NULL;",
    )
    .unwrap();
    assert!(app.mark_local_started(&e.id, 3).is_err());
    drop(db);
    drop(app);
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    assert!(app.reconcile_local_execution(&e.id, true, 4).is_err());
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "unknown");
}

#[test]
fn trusted_owner_reconciliation_releases_capacity_without_replay_or_reply() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "recovery");
    app.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 3)
        .unwrap();
    app.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 4)
        .unwrap();
    assert!(app.record_local_owner(&e.id, "different", 4).is_err());
    app.mark_local_started(&e.id, 5).unwrap();
    app.request_local_stop(&e.id, 6).unwrap();
    drop(app);
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    assert_eq!(
        app.local_executions(&s).unwrap()[0].owner_job.as_deref(),
        e.owner_job.as_deref()
    );
    app.reconcile_local_execution(&e.id, false, 7).unwrap();
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "unknown");
    app.reconcile_local_execution(&e.id, true, 8).unwrap();
    app.reconcile_local_execution(&e.id, true, 9).unwrap();
    let run = &app.local_executions(&s).unwrap()[0];
    assert_eq!(run.status, "failed");
    assert_eq!(run.error.as_deref(), Some("restart_interrupted"));
    assert_eq!(app.history(&s).unwrap().len(), 1);
    assert!(
        !app.prepare_local_execution("recovery", "same", 10)
            .unwrap()
            .can_start
    );
    assert!(prepare(&mut app, &s, "next").can_start);
    drop(app);
    let db = Connection::open(dir.path().join("app.sqlite")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT stop_confirmed_at FROM executions WHERE id=?1",
            [&e.id],
            |r| r.get::<_, Option<i64>>(0)
        )
        .unwrap(),
        Some(8)
    );
}

#[test]
fn durable_result_restores_after_restart_without_model_replay() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "spooled");
    assert!(e
        .owner_job
        .as_deref()
        .unwrap()
        .starts_with("Local\\BobReadOnly-"));
    app.mark_local_started(&e.id, 3).unwrap();
    drop(app);
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "unknown");
    app.restore_local_completion(&e.id, reply(), Some(0), None, 4)
        .unwrap();
    app.restore_local_completion(&e.id, reply(), Some(0), None, 5)
        .unwrap();
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "completed");
    assert_eq!(app.history(&s).unwrap().len(), 2);
    assert!(
        !app.prepare_local_execution("spooled", "ignored", 6)
            .unwrap()
            .can_start
    );
}

#[test]
fn owner_is_frozen_before_any_spawn_and_can_reconcile_prestart_crash() {
    let (dir, mut app, s) = fixture();
    let e = prepare(&mut app, &s, "prestart");
    assert!(e.owner_job.is_some());
    drop(app);
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    app.reconcile_local_execution(&e.id, true, 3).unwrap();
    assert_eq!(app.local_executions(&s).unwrap()[0].status, "failed");
    assert_eq!(app.history(&s).unwrap().len(), 1);
}
