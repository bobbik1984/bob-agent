use bob_kernel::app::LocalApp;

#[test]
fn schedule_edit_preserves_identity_reopens_and_rejects_stale_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("schedule.db");
    let mut app = LocalApp::open(&db).unwrap();
    let first = app.create_schedule_item_with_kind("event",None,None,"Initial","event",1000,Some(2000),Some(500),"Initial notes",100).unwrap();
    let edited = app.edit_schedule_item("event", first.version,"Edited",3000,"New notes",200).unwrap();
    assert_eq!(edited.id, first.id);
    assert_eq!(edited.version, first.version + 1);
    assert_eq!(edited.end_at, Some(4000));
    assert_eq!(edited.reminder_at, Some(2500));
    assert_eq!(edited.status, first.status);
    assert!(app.edit_schedule_item("event", first.version,"Stale",5000,"Lost",300).is_err());
    assert!(app.edit_schedule_item("event", edited.version," ",5000,"Lost",300).is_err());
    drop(app);
    let app = LocalApp::open(&db).unwrap();
    let row = app.list_schedule_items().unwrap().into_iter().find(|x|x.id=="event").unwrap();
    assert_eq!(row.title,"Edited");
    assert_eq!(row.notes,"New notes");
    assert_eq!(row.start_at,3000);
    assert_eq!(row.version,edited.version);
}
