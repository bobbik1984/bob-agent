// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// Force rebuild to update resources
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    app_lib::run();
}
