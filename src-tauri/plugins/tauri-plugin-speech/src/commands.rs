use tauri::{Runtime, Window, AppHandle};

#[tauri::command]
pub async fn start_listening<R: Runtime>(app: AppHandle<R>, window: Window<R>) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Emitter;
        app.emit("speech_partial", "[Native code will intercept this]").unwrap();
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri::Emitter;
        window.emit("speech_partial", serde_json::json!({ "text": "Desktop voice not implemented yet." })).unwrap();
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_listening<R: Runtime>() -> Result<(), String> {
    Ok(())
}
