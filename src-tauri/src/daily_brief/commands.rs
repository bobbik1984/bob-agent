use tauri::State;

use super::models::{DailyBriefSnapshot, DateContext};

fn local_device_id() -> String {
    crate::read_config()
        .get("device_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("local-unpaired")
        .to_string()
}

#[tauri::command]
pub fn daily_brief_get(
    date_context: DateContext,
    db: State<'_, crate::db::DbState>,
) -> Result<DailyBriefSnapshot, String> {
    let conn = db.0.lock().map_err(|_| "ERR-BRIEF-DB-LOCK".to_string())?;
    super::service::get_snapshot(
        &conn,
        &crate::get_data_dir(),
        &date_context,
        &local_device_id(),
        false,
    )
}

#[tauri::command]
pub fn daily_brief_refresh(
    date_context: DateContext,
    db: State<'_, crate::db::DbState>,
) -> Result<DailyBriefSnapshot, String> {
    let conn = db.0.lock().map_err(|_| "ERR-BRIEF-DB-LOCK".to_string())?;
    super::service::get_snapshot(
        &conn,
        &crate::get_data_dir(),
        &date_context,
        &local_device_id(),
        true,
    )
}

#[tauri::command]
pub fn daily_brief_mark_seen(
    snapshot_id: String,
    revision: u64,
    db: State<'_, crate::db::DbState>,
) -> Result<bool, String> {
    let conn = db.0.lock().map_err(|_| "ERR-BRIEF-DB-LOCK".to_string())?;
    super::service::mark_seen(&conn, &local_device_id(), &snapshot_id, revision)
}
