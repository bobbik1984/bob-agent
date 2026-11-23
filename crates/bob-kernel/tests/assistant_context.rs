use bob_kernel::app::LocalApp;
use serde_json::json;

fn message(app: &mut LocalApp, cid: &str, id: &str, text: &str) {
    app.accept(id, id, cid, text.into(), 1000).unwrap();
}

#[test]
fn durable_memory_survives_30_messages_reopen_and_correct_forget() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let cid;
    let first;
    {
        let mut app = LocalApp::open(&path).unwrap();
        cid = app.default_session().unwrap();
        message(
            &mut app,
            &cid,
            "remember",
            "Remember: project delivery is PDF only; do not regenerate reports.",
        );
        first=app.assistant_memory_save(&cid,"remember",&json!({"body":"Delivery must be PDF only; no report regeneration.","topic":"delivery","scope":"global"}),1001).unwrap();
        for n in 0..30 {
            message(
                &mut app,
                &cid,
                &format!("filler-{n}"),
                "Unrelated short observation.",
            );
        }
        let packet = app.assistant_context(&cid, "delivery").unwrap();
        assert!(packet.to_string().contains("PDF"));
        assert_eq!(app.assistant_memories(&cid, "").unwrap().len(), 1);
        app.assistant_checkpoint(
            &cid,
            "remember",
            "Goal: delivery. Constraint: PDF. Pending: approval.",
            1002,
        )
        .unwrap();
    }
    let mut app = LocalApp::open(&path).unwrap();
    assert!(app
        .assistant_context(&cid, "delivery")
        .unwrap()
        .to_string()
        .contains("PDF"));
    message(
        &mut app,
        &cid,
        "correct",
        "Correction: delivery is SVG now.",
    );
    assert!(app.assistant_memory_save(&cid,"correct",&json!({"body":"SVG only","topic":"delivery","supersedes_id":first["id"],"expected_version":99}),1003).is_err());
    assert!(app
        .assistant_context(&cid, "delivery")
        .unwrap()
        .to_string()
        .contains("PDF"));
    let second=app.assistant_memory_save(&cid,"correct",&json!({"body":"SVG only","topic":"delivery","scope":"global","supersedes_id":first["id"],"expected_version":1}),1004).unwrap();
    let active = app.assistant_context(&cid, "delivery").unwrap().to_string();
    assert!(active.contains("SVG"));
    assert!(!active.contains("PDF"));
    assert!(app
        .assistant_history_search(&cid, "PDF")
        .unwrap()
        .is_empty());
    message(&mut app, &cid, "forget", "Forget the delivery format.");
    app.assistant_memory_forget(&cid, "forget", second["id"].as_str().unwrap(), 1)
        .unwrap();
    assert!(app.assistant_memories(&cid, "delivery").unwrap().is_empty());
    assert!(app
        .assistant_history_search(&cid, "SVG")
        .unwrap()
        .is_empty());
    assert!(app.history(&cid).unwrap().len() > 30); // UI history was not deleted.
    assert!(!app
        .kb_search("SVG", 10)
        .unwrap()
        .iter()
        .any(|m| m.doc_kind == "memory"));
    drop(app);
    let app = LocalApp::open(&path).unwrap();
    assert!(app.assistant_memories(&cid, "").unwrap().is_empty());
    assert!(app
        .assistant_history_search(&cid, "PDF SVG")
        .unwrap()
        .is_empty());
}

#[test]
fn scoped_memory_source_and_retrieval_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("context.sqlite")).unwrap();
    let cid = app.default_session().unwrap();
    app.import_conversation("other", None, "Other", 1000)
        .unwrap();
    message(&mut app, &cid, "r", "Remember my local private constraint.");
    let saved = app
        .assistant_memory_save(
            &cid,
            "r",
            &json!({"body":"private canary","topic":"private"}),
            1001,
        )
        .unwrap();
    assert!(app.assistant_memories("other", "").unwrap().is_empty());
    message(&mut app, "other", "other-r", "Change a memory.");
    assert!(app
        .assistant_memory_forget("other", "other-r", saved["id"].as_str().unwrap(), 1)
        .is_err());
    assert!(app
        .assistant_memory_save("other", "r", &json!({"body":"bad","topic":"bad"}), 1002)
        .is_err());
    assert!(app
        .assistant_memory_save(
            &cid,
            "r",
            &json!({"body":"bad","topic":"bad","scope":"project"}),
            1002
        )
        .is_err());
    let again = app
        .assistant_memory_save(
            &cid,
            "r",
            &json!({"body":"private canary","topic":"private"}),
            1002,
        )
        .unwrap();
    assert_eq!(saved["id"], again["id"]);
    assert_eq!(app.assistant_memories(&cid, "").unwrap().len(), 1);
}

#[test]
fn coverage_persists_all_questions_and_rejects_fake_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let mut app = LocalApp::open(&path).unwrap();
    let cid = app.default_session().unwrap();
    message(
        &mut app,
        &cid,
        "r",
        "Check deadline; explain failure; keep report unchanged.",
    );
    assert!(app
        .assistant_plan(&cid, "r", &[json!("invented request")], 1)
        .is_err());
    app.assistant_plan(
        &cid,
        "r",
        &[
            json!("Check deadline"),
            json!("explain failure"),
            json!("keep report unchanged"),
        ],
        2,
    )
    .unwrap();
    app.assistant_answer(
        &cid,
        "r",
        &[
            json!({"id":1,"status":"answered","answer":"Tomorrow at noon."}),
            json!({"id":2,"status":"failed","answer":"Provider rejected the old model ID."}),
        ],
        3,
    )
    .unwrap();
    assert_eq!(app.assistant_coverage("r").unwrap()[2]["status"], "pending");
    assert!(app
        .assistant_plan(&cid, "r", &[json!("Check deadline")], 4)
        .is_err());
    assert!(app
        .assistant_answer(
            &cid,
            "r",
            &[json!({"id":9,"status":"answered","answer":"Fake"})],
            4
        )
        .is_err());
    drop(app);
    let mut app = LocalApp::open(&path).unwrap();
    assert_eq!(app.assistant_coverage("r").unwrap().len(), 3);
    let done = app
        .assistant_answer(
            &cid,
            "r",
            &[json!({"id":3,"status":"answered","answer":"No report was generated."})],
            5,
        )
        .unwrap();
    assert_eq!(done["all_addressed"], true);
}

#[test]
fn long_memory_does_not_starve_short_and_chinese_relevance() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("context.sqlite")).unwrap();
    let cid = app.default_session().unwrap();
    message(&mut app, &cid, "r", "Remember delivery rules.");
    app.assistant_memory_save(
        &cid,
        "r",
        &json!({"body":"天窗项目不要重新生成报告","topic":"天窗"}),
        1,
    )
    .unwrap();
    app.assistant_memory_save(
        &cid,
        "r",
        &json!({"body":"x".repeat(7000),"topic":"large"}),
        2,
    )
    .unwrap();
    assert!(app
        .assistant_context(&cid, "天窗项目")
        .unwrap()
        .to_string()
        .contains("不要重新生成报告"));
    assert!(
        bob_kernel::assistant_context::context_score(
            "天窗的要求是什么",
            "天窗项目不要重新生成报告"
        ) > 0
    );
}

#[test]
fn forgetting_blocks_inflight_writes_and_ui_recall() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("context.sqlite")).unwrap();
    let cid = app.default_session().unwrap();
    message(&mut app, &cid, "r", "Remember a private requirement.");
    let first = app
        .assistant_memory_save(
            &cid,
            "r",
            &json!({"body":"old requirement","topic":"requirement"}),
            1,
        )
        .unwrap();
    message(
        &mut app,
        &cid,
        "correct",
        "Use the new requirement instead.",
    );
    let second=app.assistant_memory_save(&cid,"correct",&json!({"body":"new requirement","topic":"requirement","supersedes_id":first["id"],"expected_version":1}),2).unwrap();
    app.assistant_checkpoint(&cid, "correct", "Only the new requirement applies.", 3)
        .unwrap();
    app.update_memory_state(second["id"].as_str().unwrap(), "forgotten")
        .unwrap();
    assert!(app
        .assistant_checkpoint(&cid, "r", "old requirement", 4)
        .is_err());
    assert!(app
        .assistant_memory_save(
            &cid,
            "r",
            &json!({"body":"resurrected requirement","topic":"requirement"}),
            4
        )
        .is_err());
    assert!(app.assistant_context(&cid, "requirement").unwrap()["checkpoint"].is_null());
    assert!(app
        .assistant_history_search(&cid, "requirement")
        .unwrap()
        .is_empty());
    message(&mut app, &cid, "fresh", "Remember an entirely new rule.");
    assert!(app
        .assistant_memory_save(
            &cid,
            "fresh",
            &json!({"body":"fresh rule","topic":"new"}),
            5
        )
        .is_ok());
}

#[test]
fn checkpoint_advances_only_after_success_and_rejects_old_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("context.sqlite")).unwrap();
    let cid = app.default_session().unwrap();
    for i in 0..17 {
        message(&mut app, &cid, &format!("r{i}"), "A continuing goal.");
    }
    assert!(app.assistant_checkpoint_due(&cid, "r16").unwrap());
    assert!(app.assistant_checkpoint(&cid, "r16", "", 18).is_err());
    assert!(app.assistant_checkpoint_due(&cid, "r16").unwrap());
    app.assistant_checkpoint(
        &cid,
        "r16",
        "Current goal, constraints and unresolved work.",
        19,
    )
    .unwrap();
    assert!(!app.assistant_checkpoint_due(&cid, "r16").unwrap());
    assert!(app
        .assistant_checkpoint(&cid, "r1", "Stale derived goal.", 20)
        .is_err());
    assert_eq!(
        app.assistant_context(&cid, "").unwrap()["checkpoint"]["source_message_id"],
        "r16"
    );
}

#[test]
fn forgetting_does_not_remove_unrelated_conversation_recall() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("test.sqlite")).unwrap();
    let cid = app.default_session().unwrap();
    app.import_conversation("unrelated", None, "", 1).unwrap();
    message(&mut app, &cid, "r", "Remember a private color.");
    let saved = app
        .assistant_memory_save(
            &cid,
            "r",
            &json!({"body":"private blue","topic":"color"}),
            1,
        )
        .unwrap();
    message(
        &mut app,
        "unrelated",
        "unrelated-r",
        "Unrelated delivery requirement: keep the diagram.",
    );
    message(&mut app, &cid, "f", "Forget the private color.");
    app.assistant_memory_forget(&cid, "f", saved["id"].as_str().unwrap(), 1)
        .unwrap();
    assert!(app
        .assistant_history_search(&cid, "private")
        .unwrap()
        .is_empty());
    assert_eq!(
        app.assistant_history_search("unrelated", "diagram")
            .unwrap()
            .len(),
        1
    );
}
