use bob_kernel::app::LocalApp;
#[test]
fn links_preserve_source_identity_and_cas_across_reopen() {
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("app.sqlite");
 let mut app=LocalApp::open(&path).unwrap();app.create_project("p","Shared",1).unwrap();
 app.set_project_session_link("device-a","dsh","same",Some("p"),0,2).unwrap();
 app.set_project_session_link("device-b","claude","same",Some("p"),0,3).unwrap();
 assert!(app.set_project_session_link("device-a","dsh","same",None,0,4).is_err());
 assert!(app.set_project_session_link("device-c","dsh","s",Some("missing"),0,4).is_err());
 drop(app);let mut app=LocalApp::open(&path).unwrap();
 assert_eq!(app.project_session_links().unwrap()["links"].as_array().unwrap().len(),2);
 let links=app.set_project_session_link("device-a","dsh","same",None,1,5).unwrap();
 assert_eq!(links["links"][0]["version"],2);assert!(links["links"][0]["project_id"].is_null());
 assert_eq!(links["links"][1]["project_id"],"p");
}
