use base64::{engine::general_purpose::STANDARD, Engine};
use bob_kernel::app::LocalApp;
#[test]
fn original_survives_reopen_and_capture_retry_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("app.sqlite");
    let mut app = LocalApp::open(&db).unwrap();
    let raw = STANDARD.encode(b"\x89PNG\r\n\x1a\nfixture");
    let media = app.put_capture_media("image/png", &raw, 1).unwrap();
    assert_eq!(media, app.put_capture_media("image/png", &raw, 2).unwrap());
    let capture = app
        .capture_with_image(
            "capture",
            None,
            "corrected note",
            Some("p"),
            Some("Project"),
            Some(&media),
            3,
        )
        .unwrap();
    assert_eq!(
        capture,
        app.capture_with_image(
            "capture",
            None,
            "corrected note",
            Some("p"),
            Some("Project"),
            Some(&media),
            4
        )
        .unwrap()
    );
    assert!(app
        .capture_with_image("capture", None, "corrected note", Some("p"), None, None, 5)
        .is_err());
    drop(app);
    let app = LocalApp::open(&db).unwrap();
    assert_eq!(
        app.capture_image_id("capture").unwrap(),
        Some(media.clone())
    );
    assert_eq!(app.capture_media(&media).unwrap().base64, raw);
    assert_eq!(app.list_captures().unwrap().len(), 1);
    assert_eq!(app.list_notes(Some("p")).unwrap().len(), 1);
}
#[test]
fn invalid_media_or_project_cannot_leave_partial_capture() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    assert!(app
        .put_capture_media("image/svg+xml", &STANDARD.encode(b"<svg/>"), 1)
        .is_err());
    assert!(app.put_capture_media("image/png", "not base64", 1).is_err());
    assert!(app
        .put_capture_media("image/png", &STANDARD.encode(b"not png"), 1)
        .is_err());
    assert!(app
        .capture_with_image(
            "missing",
            None,
            "note",
            Some("p"),
            Some("Project"),
            Some("missing"),
            2
        )
        .is_err());
    assert!(app.list_captures().unwrap().is_empty());
    assert!(app.catalog_snapshot().unwrap().projects.is_empty());
    let raw = STANDARD.encode(b"\x89PNG\r\n\x1a\nfixture");
    let media = app.put_capture_media("image/png", &raw, 3).unwrap();
    assert!(app
        .capture_with_image("bad", None, "note", Some("missing"), None, Some(&media), 4)
        .is_err());
    assert!(app.list_captures().unwrap().is_empty());
    assert!(app.capture_media(&media).is_ok());
}
