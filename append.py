import codecs

c = codecs.open('src-tauri/src/sync_engine.rs', 'r', 'utf-8').read()

func = """
#[tauri::command]
pub fn get_sync_logs(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let path = crate::get_data_dir(&app).join("sync_history.json");
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(logs) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
            return Ok(logs);
        }
    }
    Ok(vec![])
}
"""
if "pub fn get_sync_logs" not in c:
    with codecs.open('src-tauri/src/sync_engine.rs', 'a', 'utf-8') as f:
        f.write(func)

print("Appended")
