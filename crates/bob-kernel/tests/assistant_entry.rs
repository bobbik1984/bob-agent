use bob_kernel::app::LocalApp;

#[test]
fn assistant_is_durable_does_not_select_or_merge_other_instances() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kernel.sqlite");
    let mut app = LocalApp::open(&path).unwrap();
    let previous = app.selected_conversation().unwrap();
    let id = app.assistant_conversation(100).unwrap();
    assert_eq!(app.selected_conversation().unwrap(), previous);
    assert_eq!(app.assistant_conversation(101).unwrap(), id);
    app.accept("assistant-test", "assistant-msg", &id, "remember this".into(), 102).unwrap();
    drop(app);
    let mut app = LocalApp::open(&path).unwrap();
    assert_eq!(app.assistant_conversation(103).unwrap(), id);
    assert_eq!(app.history(&id).unwrap()[0].content, "remember this");
    let mut other = LocalApp::open(&dir.path().join("other.sqlite")).unwrap();
    assert_ne!(other.assistant_conversation(104).unwrap(), id);
    assert!(other.history(&id).is_err());
}

#[test]
fn native_result_is_source_bound_atomic_idempotent_and_survives_restart() {
 let dir=tempfile::tempdir().unwrap(); let path=dir.path().join("state.sqlite");
 let mut app=LocalApp::open(&path).unwrap(); let id=app.assistant_conversation(1).unwrap();
 app.accept("r", "m", &id, "ask progress".into(), 2).unwrap();
 app.assistant_bind_dispatch("r","node","native","title",3).unwrap();
 assert!(app.assistant_bind_dispatch("r","other","native","title",4).is_err());
 assert!(app.assistant_return_result("r","node","wrong","answer",&[10],5).is_err());
 drop(app);
 let mut app=LocalApp::open(&path).unwrap();
 assert_eq!(app.assistant_links(&id).unwrap()[0].status,"unknown");
 app.assistant_return_result("r","node","native","answer",&[10],6).unwrap();
 app.assistant_return_result("r","node","native","answer",&[10],7).unwrap();
 assert!(app.assistant_return_result("r","node","native","changed",&[10],8).is_err());
 app.assistant_record_status("r","accepted",9).unwrap();
 assert_eq!(app.assistant_links(&id).unwrap()[0].status,"completed");
 assert_eq!(app.history(&id).unwrap().len(),2);
 drop(app);let app=LocalApp::open(&path).unwrap();
 assert_eq!(app.assistant_links(&id).unwrap()[0].result_sequences,vec![10]);
 assert_eq!(app.history(&id).unwrap().len(),2);
}
#[test]
fn ordinary_conversations_and_failed_links_cannot_receive_assistant_results() {
 let dir=tempfile::tempdir().unwrap();let mut app=LocalApp::open(&dir.path().join("state.sqlite")).unwrap();
 let normal=app.selected_conversation().unwrap();
 app.accept("normal","m1",&normal,"hello".into(),1).unwrap();
 assert!(app.assistant_bind_dispatch("normal","n","s","t",2).is_err());
 let id=app.assistant_conversation(3).unwrap();
 app.accept("r","m2",&id,"ask".into(),4).unwrap();
 app.assistant_bind_dispatch("r","n","s","t",5).unwrap();
 app.assistant_record_status("r","ambiguous",6).unwrap();
 assert!(app.assistant_return_result("r","n","s","not certain",&[1],7).is_err());
 assert_eq!(app.history(&id).unwrap().len(),1);
}
