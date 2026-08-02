use tauri::{plugin::{Builder, TauriPlugin}, Runtime};

mod commands;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("speech")
        .invoke_handler(tauri::generate_handler![
            commands::start_listening,
            commands::stop_listening
        ])
        .build()
}
