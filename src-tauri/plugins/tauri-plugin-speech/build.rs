const COMMANDS: &[&str] = &["start_listening", "stop_listening"];
fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
