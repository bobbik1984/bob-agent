import codecs

c = codecs.open('src-tauri/src/sync_engine.rs', 'r', 'utf-8').read()

func = """
#[tauri::command]
pub fn get_shared_intents(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let mut results = vec![];
    if let Ok(cache_dir) = app.path().cache_dir() {
        let incoming_dir = cache_dir.join("shared_incoming");
        if let Ok(entries) = std::fs::read_dir(&incoming_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                
                if file_name.ends_with(".txt") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        results.push(serde_json::json!({
                            "type": "text",
                            "filename": file_name,
                            "content": content
                        }));
                    }
                } else {
                    // Treat as image or binary file
                    results.push(serde_json::json!({
                        "type": "file",
                        "filename": file_name,
                        "path": path.to_string_lossy().into_owned()
                    }));
                }
            }
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn clear_shared_intent(app: tauri::AppHandle, filename: String) -> Result<(), String> {
    if let Ok(cache_dir) = app.path().cache_dir() {
        let incoming_dir = cache_dir.join("shared_incoming");
        let file_path = incoming_dir.join(filename);
        let _ = std::fs::remove_file(file_path);
    }
    Ok(())
}
"""

if "pub fn get_shared_intents" not in c:
    with codecs.open('src-tauri/src/sync_engine.rs', 'a', 'utf-8') as f:
        f.write(func)
    print("Appended commands to sync_engine.rs")
else:
    print("Commands already exist")
