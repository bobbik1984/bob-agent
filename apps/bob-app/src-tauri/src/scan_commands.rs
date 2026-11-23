//! Scanning is an explicit action; it never implicitly accepts or connects a peer.
pub(crate) fn validate(code: String) -> Result<String, &'static str> {
    if code.is_empty() || code.len() > 8192 || code.contains('\0') { return Err("SCAN_INVALID"); }
    Ok(code)
}

#[tauri::command]
pub(crate) async fn app_scan_pairing_code<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<String, &'static str> {
    #[cfg(target_os = "android")]
    { crate::android_scanner::scan(&app).await }
    #[cfg(not(target_os = "android"))]
    { let _ = app; Err("SCAN_UNSUPPORTED") }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanner_data_is_bounded_and_not_an_authorization() {
        assert!(validate(String::new()).is_err());
        assert!(validate("x".repeat(8193)).is_err());
        assert!(validate("before\0after".into()).is_err());
        assert_eq!(validate("opaque-public-pairing-code".into()).unwrap(), "opaque-public-pairing-code");
    }
}
