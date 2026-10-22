fn main() {
    // No WebView commands: key material is available only to the native Rust caller.
    tauri_plugin::Builder::new(&[]).android_path("android").build();
}
