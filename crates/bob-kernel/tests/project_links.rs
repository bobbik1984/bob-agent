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


#[test]
fn scoped_and_owner_writers_share_cas_and_source_project_guard() {
    use bob_kernel::project_links::ProjectLinkAuthority;
    let d=tempfile::tempdir().unwrap(); let mut app=LocalApp::open(&d.path().join("kernel.sqlite")).unwrap();
    for id in ["p","private"] {app.create_project(id,id,1).unwrap();}
    let projects=vec!["p".to_string()];
    let sessions=vec![("node".to_string(),"dsh".to_string(),"s".to_string())];
    let scope=||ProjectLinkAuthority::Scoped{organize:true,projects:&projects,sessions:&sessions};
    app.set_project_session_link_authorized(scope(),"node","dsh","s",Some("p"),0,2).unwrap();
    // A normal owner write advances the very same version; the bot cannot overwrite it.
    app.set_project_session_link("node","dsh","s",Some("private"),1,3).unwrap();
    assert!(app.set_project_session_link_authorized(scope(),"node","dsh","s",Some("p"),1,4).is_err());
    assert!(app.set_project_session_link_authorized(scope(),"node","dsh","s",Some("p"),2,4).is_err());
    let rows=app.project_session_links().unwrap();assert_eq!(rows["links"][0]["project_id"],"private");
    assert_eq!(rows["links"][0]["version"],2);
}

#[test]
fn scoped_kernel_entry_denies_without_transport_checks() {
    use bob_kernel::project_links::ProjectLinkAuthority;
    let d=tempfile::tempdir().unwrap(); let mut app=LocalApp::open(&d.path().join("kernel.sqlite")).unwrap();
    app.create_project("p","P",1).unwrap();let projects=vec!["p".to_string()];
    let sessions=vec![("n".into(),"dsh".into(),"s".into())];
    let ro=ProjectLinkAuthority::Scoped{organize:false,projects:&projects,sessions:&sessions};
    assert!(app.set_project_session_link_authorized(ro,"n","dsh","s",Some("p"),0,2).is_err());
    let scope=||ProjectLinkAuthority::Scoped{organize:true,projects:&projects,sessions:&sessions};
    assert!(app.set_project_session_link_authorized(scope(),"n","dsh","other",Some("p"),0,2).is_err());
    assert!(app.set_project_session_link_authorized(scope(),"n","dsh","s",None,0,2).is_err());
    assert!(app.set_project_session_link_authorized(scope(),"n","dsh","s",Some("p"),i64::MAX,2).is_err());
    assert_eq!(app.project_session_links().unwrap()["links"].as_array().unwrap().len(),0);
}

#[test]
fn archived_project_tombstone_syncs_and_suppresses_stale_active_copy() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = LocalApp::open(&dir.path().join("hub.sqlite")).unwrap();
    let mut phone = LocalApp::open(&dir.path().join("phone.sqlite")).unwrap();

    hub.create_project("junk", "Junk", 10).unwrap();
    phone.create_project("junk", "Junk", 10).unwrap();

    hub.upsert_formal_project("junk", "Junk", "archived", 10, 20)
        .unwrap();
    assert!(hub.catalog_snapshot().unwrap().projects.is_empty());

    // Phone pushing its older active copy (version 1) must not resurrect the archived row on Hub.
    let phone_payload = phone.export_projects_and_links().unwrap();
    hub.merge_projects_and_links(&phone_payload, 30).unwrap();
    assert!(hub.catalog_snapshot().unwrap().projects.is_empty());

    // Hub exporting archived tombstone (version 2) archives the stale active copy on Phone.
    let hub_payload = hub.export_projects_and_links().unwrap();
    phone.merge_projects_and_links(&hub_payload, 30).unwrap();
    assert!(phone.catalog_snapshot().unwrap().projects.is_empty());
}
